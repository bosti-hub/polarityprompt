//! Audio capture: device listing and the `AudioSupervisor` that owns the cpal
//! stream, handles device loss with reconnect, and reports levels.
//!
//! CLAUDE.md rule 3: the cpal callback only pushes samples into the ring buffer
//! (plus one atomic store for the level meter). It never allocates or locks.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::{Deserialize, Serialize};

use crate::engine::EngineCommand;

/// Stable by *name*; cpal ids are not stable across sessions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: DeviceId,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChannelTag {
    #[default]
    #[serde(rename = "self")]
    Self_,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSel {
    pub device: DeviceId,
    #[serde(default)]
    pub channel: ChannelTag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SourceMode {
    #[default]
    Single,
    Mix,
}

/// Empty `sources` means "system default input".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AudioSourceConfig {
    pub sources: Vec<SourceSel>,
    #[serde(default)]
    pub mode: SourceMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AudioStatus {
    Stopped,
    Capturing { device: DeviceId, sample_rate: u32, channels: u16 },
    Reconnecting,
    NoSignal { seconds: f32 },
    PermissionDenied,
    NoDevice,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AudioEvent {
    DeviceLost(DeviceId),
    DefaultInputChanged(DeviceId),
    Level { rms_db: f32 },
    PermissionDenied,
    Status(AudioStatus),
}

pub fn list_input_devices() -> Vec<DeviceInfo> {
    let host = cpal::default_host();
    let default_name = host.default_input_device().and_then(|d| d.description().ok()).map(|d| d.name().to_string());
    let mut out = Vec::new();
    if let Ok(devices) = host.input_devices() {
        for d in devices {
            let Ok(desc) = d.description() else { continue };
            let name = desc.name().to_string();
            out.push(DeviceInfo { id: DeviceId(name.clone()), is_default: Some(&name) == default_name.as_ref(), name });
        }
    }
    out
}

enum Cmd {
    SetSource(AudioSourceConfig),
    Stop,
    Shutdown,
}

pub struct AudioSupervisor {
    tx: Sender<Cmd>,
    handle: Option<JoinHandle<()>>,
}

const RING_SECONDS: usize = 2;
const NO_SIGNAL_DB: f32 = -60.0;
const NO_SIGNAL_AFTER: Duration = Duration::from_secs(5);
const TICK: Duration = Duration::from_millis(100);
const BACKOFF_MIN: Duration = Duration::from_millis(250);
const BACKOFF_MAX: Duration = Duration::from_secs(2);

impl AudioSupervisor {
    /// `worker` receives a `Reconfigure` with a fresh ring-buffer consumer for
    /// every stream that is opened; `events` gets status/level updates.
    pub fn spawn(worker: Sender<EngineCommand>, events: Sender<AudioEvent>) -> Self {
        let (tx, rx) = channel();
        let handle = thread::Builder::new()
            .name("audio-supervisor".into())
            .spawn(move || Supervisor::new(worker, events, rx).run())
            .expect("spawn audio supervisor");
        AudioSupervisor { tx, handle: Some(handle) }
    }

    pub fn set_source(&self, cfg: AudioSourceConfig) {
        let _ = self.tx.send(Cmd::SetSource(cfg));
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }
}

impl Drop for AudioSupervisor {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Shutdown);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

struct Open {
    _stream: cpal::Stream,
    device: DeviceId,
    lost: Arc<AtomicBool>,
    level_bits: Arc<AtomicU32>,
    quiet_since: Option<Instant>,
    no_signal_reported: bool,
}

struct Supervisor {
    worker: Sender<EngineCommand>,
    events: Sender<AudioEvent>,
    rx: Receiver<Cmd>,
    wanted: Option<AudioSourceConfig>,
    open: Option<Open>,
    retry_at: Option<Instant>,
    backoff: Duration,
}

impl Supervisor {
    fn new(worker: Sender<EngineCommand>, events: Sender<AudioEvent>, rx: Receiver<Cmd>) -> Self {
        Supervisor { worker, events, rx, wanted: None, open: None, retry_at: None, backoff: BACKOFF_MIN }
    }

    fn emit(&self, ev: AudioEvent) {
        let _ = self.events.send(ev);
    }

    fn run(mut self) {
        loop {
            loop {
                match self.rx.try_recv() {
                    Ok(Cmd::SetSource(cfg)) => {
                        self.wanted = Some(cfg);
                        self.open = None;
                        self.backoff = BACKOFF_MIN;
                        self.try_open();
                    }
                    Ok(Cmd::Stop) => {
                        self.wanted = None;
                        self.open = None;
                        self.retry_at = None;
                        self.emit(AudioEvent::Status(AudioStatus::Stopped));
                    }
                    Ok(Cmd::Shutdown) | Err(TryRecvError::Disconnected) => return,
                    Err(TryRecvError::Empty) => break,
                }
            }
            self.tick();
            thread::sleep(TICK);
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        if let Some(open) = self.open.as_mut() {
            if open.lost.swap(false, Ordering::Relaxed) {
                let device = open.device.clone();
                self.open = None;
                self.emit(AudioEvent::DeviceLost(device));
                self.emit(AudioEvent::Status(AudioStatus::Reconnecting));
                self.backoff = BACKOFF_MIN;
                self.retry_at = Some(now + self.backoff);
                return;
            }
            let rms = f32::from_bits(open.level_bits.load(Ordering::Relaxed));
            let rms_db = if rms > 0.0 { 20.0 * rms.log10() } else { -120.0 };
            let _ = self.events.send(AudioEvent::Level { rms_db });
            if rms_db < NO_SIGNAL_DB {
                let since = *open.quiet_since.get_or_insert(now);
                let quiet = now.duration_since(since);
                if quiet >= NO_SIGNAL_AFTER && (!open.no_signal_reported || quiet.as_millis() % 1000 < TICK.as_millis()) {
                    open.no_signal_reported = true;
                    let _ = self.events.send(AudioEvent::Status(AudioStatus::NoSignal { seconds: quiet.as_secs_f32() }));
                }
            } else if open.quiet_since.take().is_some() && open.no_signal_reported {
                open.no_signal_reported = false;
                let status = AudioStatus::Capturing { device: open.device.clone(), sample_rate: 0, channels: 0 };
                let _ = self.events.send(AudioEvent::Status(status));
            }
        } else if let Some(at) = self.retry_at {
            if now >= at && self.wanted.is_some() && !self.try_open() {
                self.backoff = (self.backoff * 2).min(BACKOFF_MAX);
                self.retry_at = Some(Instant::now() + self.backoff);
            }
        }
    }

    /// Opens the preferred device, falling back to the system default.
    fn try_open(&mut self) -> bool {
        let Some(cfg) = self.wanted.clone() else { return false };
        let host = cpal::default_host();
        let preferred = cfg.sources.first().map(|s| s.device.clone());
        let mut candidates: Vec<cpal::Device> = Vec::new();
        if let Some(pref) = &preferred {
            if let Ok(devices) = host.input_devices() {
                candidates.extend(devices.filter(|d| d.description().map(|x| x.name() == pref.0).unwrap_or(false)));
            }
        }
        if let Some(d) = host.default_input_device() {
            candidates.push(d);
        }
        if candidates.is_empty() {
            self.emit(AudioEvent::Status(AudioStatus::NoDevice));
            self.retry_at = Some(Instant::now() + BACKOFF_MAX);
            return false;
        }
        let mut last_err = None;
        for device in candidates {
            match self.open_device(&device) {
                Ok(open) => {
                    self.retry_at = None;
                    self.emit(AudioEvent::Status(AudioStatus::Capturing {
                        device: open.0.device.clone(),
                        sample_rate: open.1,
                        channels: open.2,
                    }));
                    self.open = Some(open.0);
                    return true;
                }
                Err(e) => {
                    if e.kind() == cpal::ErrorKind::PermissionDenied {
                        self.emit(AudioEvent::PermissionDenied);
                        self.emit(AudioEvent::Status(AudioStatus::PermissionDenied));
                        self.retry_at = None;
                        return false;
                    }
                    last_err = Some(e);
                }
            }
        }
        if let Some(e) = last_err {
            eprintln!("audio: cannot open input: {e}");
        }
        self.emit(AudioEvent::Status(AudioStatus::Reconnecting));
        self.retry_at = Some(Instant::now() + self.backoff);
        false
    }

    fn open_device(&self, device: &cpal::Device) -> Result<(Open, u32, u16), cpal::Error> {
        let name = device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "unknown".into());
        let supported = device.default_input_config()?;
        let config = supported.config();
        let sample_rate = config.sample_rate;
        let channels = config.channels;

        let (producer, consumer) = rtrb::RingBuffer::<f32>::new(sample_rate as usize * channels as usize * RING_SECONDS);
        let lost = Arc::new(AtomicBool::new(false));
        let level_bits = Arc::new(AtomicU32::new(0));

        let lost_cb = lost.clone();
        let err_cb = move |e: cpal::Error| {
            eprintln!("audio stream error: {e}");
            lost_cb.store(true, Ordering::Relaxed);
        };

        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => {
                let mut producer = producer;
                let level = level_bits.clone();
                device.build_input_stream::<f32, _, _>(
                    config,
                    move |data, _| {
                        let _ = producer.push_partial_slice(data);
                        level.store(rms(data).to_bits(), Ordering::Relaxed);
                    },
                    err_cb,
                    None,
                )?
            }
            cpal::SampleFormat::I16 => {
                let mut producer = producer;
                let level = level_bits.clone();
                device.build_input_stream::<i16, _, _>(
                    config,
                    move |data, _| {
                        let mut acc = 0.0f32;
                        for &s in data {
                            let f = s as f32 / 32768.0;
                            acc += f * f;
                            let _ = producer.push(f);
                        }
                        let r = if data.is_empty() { 0.0 } else { (acc / data.len() as f32).sqrt() };
                        level.store(r.to_bits(), Ordering::Relaxed);
                    },
                    err_cb,
                    None,
                )?
            }
            other => {
                eprintln!("audio: unsupported sample format {other:?}");
                return Err(cpal::Error::new(cpal::ErrorKind::UnsupportedConfig));
            }
        };
        stream.play()?;
        let _ = self.worker.send(EngineCommand::Reconfigure { consumer, sample_rate, channels });
        Ok((
            Open {
                _stream: stream,
                device: DeviceId(name),
                lost,
                level_bits,
                quiet_since: None,
                no_signal_reported: false,
            },
            sample_rate,
            channels,
        ))
    }
}

fn rms(data: &[f32]) -> f32 {
    if data.is_empty() {
        return 0.0;
    }
    (data.iter().map(|s| s * s).sum::<f32>() / data.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_config_serializes_with_defaults() {
        let cfg: AudioSourceConfig = serde_json::from_str(r#"{"sources":[{"device":"Mic"}]}"#).unwrap();
        assert_eq!(cfg.sources[0].device, DeviceId("Mic".into()));
        assert_eq!(cfg.mode, SourceMode::Single);
        let json = serde_json::to_string(&AudioStatus::NoSignal { seconds: 5.0 }).unwrap();
        assert_eq!(json, r#"{"kind":"no_signal","seconds":5.0}"#);
    }

    #[test]
    fn rms_of_silence_is_zero() {
        assert_eq!(rms(&[0.0; 16]), 0.0);
        assert!((rms(&[0.5, -0.5]) - 0.5).abs() < 1e-6);
    }
}
