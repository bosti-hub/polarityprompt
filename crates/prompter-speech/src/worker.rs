//! `SpeechWorker`: its own thread; drains the ring buffer, downsamples, feeds
//! the engine and publishes on the bus. Also detects silence (no change in
//! the partial result) which the controller turns into a Hold.

use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use prompter_core::{EngineStatus, Lang, TranscriptEvent};

use crate::bus::TranscriptBus;
use crate::engine::{EngineCommand, EngineError, SpeechEngine, ENGINE_SAMPLE_RATE};
use crate::resample::Downsampler;

pub type EngineFactory = Box<dyn Fn(Lang) -> Result<Box<dyn SpeechEngine>, EngineError> + Send>;

/// Silence is reported once the partial result has not changed for this long…
const SILENCE_AFTER: Duration = Duration::from_millis(1000);
/// …and then repeated at this interval so the controller can apply its own threshold.
const SILENCE_REPEAT: Duration = Duration::from_millis(500);
const IDLE_SLEEP: Duration = Duration::from_millis(10);
const CHUNK_FRAMES: usize = 2048;

pub struct SpeechWorker {
    tx: Sender<EngineCommand>,
    handle: Option<JoinHandle<()>>,
}

impl SpeechWorker {
    /// Builds the first engine synchronously so a missing model is reported here.
    pub fn spawn(lang: Lang, factory: EngineFactory, bus: Arc<TranscriptBus>) -> Result<Self, EngineError> {
        let engine = factory(lang)?;
        let (tx, rx) = channel();
        let handle = thread::Builder::new()
            .name("speech-worker".into())
            .spawn(move || run(engine, factory, rx, bus))
            .map_err(|e| EngineError::Init(e.to_string()))?;
        Ok(SpeechWorker { tx, handle: Some(handle) })
    }

    pub fn sender(&self) -> Sender<EngineCommand> {
        self.tx.clone()
    }

    pub fn send(&self, cmd: EngineCommand) {
        let _ = self.tx.send(cmd);
    }
}

impl Drop for SpeechWorker {
    fn drop(&mut self) {
        let _ = self.tx.send(EngineCommand::Shutdown);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

struct Loop {
    engine: Box<dyn SpeechEngine>,
    factory: EngineFactory,
    bus: Arc<TranscriptBus>,
    consumer: Option<rtrb::Consumer<f32>>,
    ds: Downsampler,
    buf: Vec<f32>,
    pcm: Vec<i16>,
    vocabulary: Vec<String>,
    /// Audio-time clock: seconds of 16 kHz audio fed so far.
    audio_time: Duration,
    last_change: Duration,
    last_silence_report: Option<Duration>,
    flush_pending: bool,
    muted: bool,
    /// Wall-clock origin of the audio clock; events are stamped `epoch + audio_time`
    /// so replaying a file faster than real time keeps the matcher's timing intact.
    epoch: Instant,
}

fn run(engine: Box<dyn SpeechEngine>, factory: EngineFactory, rx: Receiver<EngineCommand>, bus: Arc<TranscriptBus>) {
    let mut l = Loop {
        engine,
        factory,
        bus,
        consumer: None,
        ds: Downsampler::new(ENGINE_SAMPLE_RATE, 1, ENGINE_SAMPLE_RATE),
        buf: vec![0.0; CHUNK_FRAMES * 2],
        pcm: Vec::with_capacity(CHUNK_FRAMES),
        vocabulary: Vec::new(),
        audio_time: Duration::ZERO,
        last_change: Duration::ZERO,
        last_silence_report: None,
        flush_pending: false,
        muted: false,
        epoch: Instant::now(),
    };
    l.bus.publish(TranscriptEvent::EngineStatus(EngineStatus::Ready));
    loop {
        // Commands first; they are rare.
        loop {
            match rx.try_recv() {
                Ok(EngineCommand::Shutdown) | Err(TryRecvError::Disconnected) => return,
                Ok(cmd) => l.command(cmd),
                Err(TryRecvError::Empty) => break,
            }
        }
        if !l.pump() {
            if l.flush_pending {
                l.flush_pending = false;
                let stamp = l.epoch + l.audio_time;
                for mut ev in l.engine.flush() {
                    if let TranscriptEvent::Final { t, .. } = &mut ev {
                        *t = stamp;
                    }
                    l.bus.publish(ev);
                }
                l.reset_clock();
            } else {
                thread::sleep(IDLE_SLEEP);
            }
        }
    }
}

impl Loop {
    fn command(&mut self, cmd: EngineCommand) {
        match cmd {
            EngineCommand::SetVocabulary { words } => {
                self.vocabulary = words.clone();
                match self.engine.set_vocabulary(&words) {
                    Ok(unknown) => {
                        self.bus.publish(TranscriptEvent::EngineStatus(EngineStatus::UnknownWords { words: unknown }));
                        self.bus.publish(TranscriptEvent::EngineStatus(EngineStatus::Ready));
                    }
                    Err(e) => eprintln!("speech: {e}"),
                }
            }
            EngineCommand::SetLanguage(lang) => {
                self.bus.publish(TranscriptEvent::EngineStatus(EngineStatus::Rebuilding));
                match (self.factory)(lang) {
                    Ok(engine) => {
                        self.engine = engine;
                        if !self.vocabulary.is_empty() {
                            let words = self.vocabulary.clone();
                            self.command(EngineCommand::SetVocabulary { words });
                        } else {
                            self.bus.publish(TranscriptEvent::EngineStatus(EngineStatus::Ready));
                        }
                    }
                    Err(e) => {
                        eprintln!("speech: language switch failed: {e}");
                        self.bus.publish(TranscriptEvent::EngineStatus(EngineStatus::Ready));
                    }
                }
                self.reset_clock();
            }
            EngineCommand::Reset => {
                self.engine.reset();
                self.ds.reset();
                self.reset_clock();
            }
            EngineCommand::Reconfigure { consumer, sample_rate, channels } => {
                self.consumer = Some(consumer);
                self.ds = Downsampler::new(sample_rate, channels, ENGINE_SAMPLE_RATE);
                self.buf = vec![0.0; CHUNK_FRAMES * channels.max(1) as usize];
                self.engine.reset();
                self.reset_clock();
            }
            EngineCommand::Flush => self.flush_pending = true,
            EngineCommand::SetMuted(m) => {
                if self.muted && !m {
                    self.engine.reset();
                    self.reset_clock();
                }
                self.muted = m;
            }
            EngineCommand::Shutdown => {}
        }
    }

    fn reset_clock(&mut self) {
        self.last_change = self.audio_time;
        self.last_silence_report = None;
    }

    /// Returns false when there was nothing to do.
    fn pump(&mut self) -> bool {
        let Some(consumer) = self.consumer.as_mut() else { return false };
        let available = consumer.slots();
        if available == 0 {
            return false;
        }
        let take = available.min(self.buf.len());
        let Ok(chunk) = consumer.read_chunk(take) else { return false };
        let (a, b) = chunk.as_slices();
        self.buf[..a.len()].copy_from_slice(a);
        self.buf[a.len()..a.len() + b.len()].copy_from_slice(b);
        chunk.commit_all();

        if self.muted {
            return true;
        }
        self.pcm.clear();
        self.ds.process(&self.buf[..take], &mut self.pcm);
        if self.pcm.is_empty() {
            return true;
        }
        self.audio_time += Duration::from_secs_f64(self.pcm.len() as f64 / ENGINE_SAMPLE_RATE as f64);

        let stamp = self.epoch + self.audio_time;
        for mut ev in self.engine.feed(&self.pcm) {
            match &mut ev {
                TranscriptEvent::Partial { t, .. } | TranscriptEvent::Final { t, .. } => *t = stamp,
                _ => {}
            }
            match &ev {
                TranscriptEvent::Partial { .. } => self.last_change = self.audio_time,
                TranscriptEvent::Final { text, .. } if !text.is_empty() => self.last_change = self.audio_time,
                _ => {}
            }
            if matches!(ev, TranscriptEvent::Partial { .. } | TranscriptEvent::Final { .. }) {
                self.last_silence_report = None;
            }
            self.bus.publish(ev);
        }

        let since = self.audio_time.saturating_sub(self.last_change);
        if since >= SILENCE_AFTER {
            let due = match self.last_silence_report {
                None => true,
                Some(t) => self.audio_time.saturating_sub(t) >= SILENCE_REPEAT,
            };
            if due {
                self.last_silence_report = Some(self.audio_time);
                self.bus.publish(TranscriptEvent::Silence { since, t: self.epoch + self.audio_time });
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::MockEngine;

    fn collect(rx: &Receiver<TranscriptEvent>, wait: Duration) -> Vec<TranscriptEvent> {
        let deadline = Instant::now() + wait;
        let mut out = Vec::new();
        while Instant::now() < deadline {
            if let Ok(ev) = rx.recv_timeout(Duration::from_millis(20)) {
                out.push(ev);
            }
        }
        out
    }

    #[test]
    fn audio_flows_from_ring_to_engine_to_bus_with_silence_detection() {
        let bus = Arc::new(TranscriptBus::new());
        let rx = bus.subscribe();
        let t0 = Instant::now();
        let factory: EngineFactory = Box::new(move |_lang| {
            Ok(Box::new(MockEngine::new(vec![
                (Duration::from_millis(500), TranscriptEvent::partial("hal", t0)),
                (Duration::from_millis(900), TranscriptEvent::final_text("hallo", t0)),
            ])) as Box<dyn SpeechEngine>)
        });
        let worker = SpeechWorker::spawn(Lang::Nl, factory, bus.clone()).unwrap();

        // 48 kHz stereo device → 4 s of audio pushed through a ring buffer.
        let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(48000 * 2 * 5);
        worker.send(EngineCommand::Reconfigure { consumer, sample_rate: 48000, channels: 2 });
        worker.send(EngineCommand::SetVocabulary { words: vec!["hallo".into()] });
        let frames = 48000 * 4;
        for _ in 0..frames {
            producer.push(0.1).unwrap();
            producer.push(0.1).unwrap();
        }

        let events = collect(&rx, Duration::from_millis(1500));
        let kinds: Vec<&str> = events
            .iter()
            .map(|e| match e {
                TranscriptEvent::Partial { .. } => "partial",
                TranscriptEvent::Final { .. } => "final",
                TranscriptEvent::Silence { .. } => "silence",
                TranscriptEvent::EngineStatus(EngineStatus::Ready) => "ready",
                TranscriptEvent::EngineStatus(EngineStatus::UnknownWords { .. }) => "unknown",
                TranscriptEvent::EngineStatus(EngineStatus::Rebuilding) => "rebuilding",
            })
            .collect();
        assert!(kinds.starts_with(&["ready", "unknown", "ready", "partial", "final"]), "{kinds:?}");
        // After the final at 0.9 s, 3.1 s of silence remain → reports at ~1.9 s, 2.4 s, … (≥ 4).
        let silences = kinds.iter().filter(|k| **k == "silence").count();
        assert!(silences >= 4, "{kinds:?}");
        if let Some(TranscriptEvent::Silence { since, .. }) = events.last() {
            assert!(*since >= Duration::from_millis(2500), "{since:?}");
        }
        drop(worker);
    }
}
