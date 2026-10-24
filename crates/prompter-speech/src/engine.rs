//! `SpeechEngine` trait, its commands and the mock implementation.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use prompter_core::{Lang, TranscriptEvent};

/// Every engine consumes 16 kHz mono i16.
pub const ENGINE_SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, Clone, PartialEq)]
pub enum EngineError {
    ModelNotFound(PathBuf),
    Init(String),
    Vocabulary(String),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::ModelNotFound(p) => write!(f, "speech model not found at {}", p.display()),
            EngineError::Init(m) => write!(f, "speech engine failed to start: {m}"),
            EngineError::Vocabulary(m) => write!(f, "could not set vocabulary: {m}"),
        }
    }
}

impl std::error::Error for EngineError {}

/// Messages to the `SpeechWorker`.
pub enum EngineCommand {
    SetVocabulary { words: Vec<String> },
    SetLanguage(Lang),
    Reset,
    /// Finalize whatever is buffered (end of a file or a take). Runs once the ring is drained.
    Flush,
    /// Drop incoming audio without stopping the stream.
    SetMuted(bool),
    Shutdown,
    /// A (new) audio stream is delivering samples through `consumer`.
    Reconfigure { consumer: rtrb::Consumer<f32>, sample_rate: u32, channels: u16 },
}

pub trait SpeechEngine: Send {
    /// Restrict recognition to `words` (+ `[unk]`). Returns the words the model does not know.
    fn set_vocabulary(&mut self, words: &[String]) -> Result<Vec<String>, EngineError>;
    /// Feed 16 kHz mono audio; returns whatever events it produced.
    fn feed(&mut self, pcm_16k_mono: &[i16]) -> Vec<TranscriptEvent>;
    /// Force a final result for buffered audio (end of stream).
    fn flush(&mut self) -> Vec<TranscriptEvent> {
        Vec::new()
    }
    fn reset(&mut self);
}

/// Replays a script of events keyed by audio time. Used by tests and by the
/// UI in the absence of a microphone.
pub struct MockEngine {
    script: Vec<(Duration, TranscriptEvent)>,
    next: usize,
    fed: u64,
    unknown: Vec<String>,
    pub vocabulary: Vec<String>,
    pub resets: usize,
}

impl MockEngine {
    pub fn new(mut script: Vec<(Duration, TranscriptEvent)>) -> Self {
        script.sort_by_key(|(t, _)| *t);
        MockEngine { script, next: 0, fed: 0, unknown: Vec::new(), vocabulary: Vec::new(), resets: 0 }
    }

    /// Words this mock will report as unknown to the "model".
    pub fn with_unknown_words(mut self, words: Vec<String>) -> Self {
        self.unknown = words;
        self
    }

    pub fn audio_time(&self) -> Duration {
        Duration::from_secs_f64(self.fed as f64 / ENGINE_SAMPLE_RATE as f64)
    }
}

impl SpeechEngine for MockEngine {
    fn set_vocabulary(&mut self, words: &[String]) -> Result<Vec<String>, EngineError> {
        self.vocabulary = words.to_vec();
        Ok(words.iter().filter(|w| self.unknown.contains(w)).cloned().collect())
    }

    fn feed(&mut self, pcm: &[i16]) -> Vec<TranscriptEvent> {
        self.fed += pcm.len() as u64;
        let now = self.audio_time();
        let mut out = Vec::new();
        while let Some((t, ev)) = self.script.get(self.next) {
            if *t > now {
                break;
            }
            let mut ev = ev.clone();
            // Stamp with wall-clock now so the matcher window works in tests.
            match &mut ev {
                TranscriptEvent::Partial { t, .. } | TranscriptEvent::Final { t, .. } => *t = Instant::now(),
                _ => {}
            }
            out.push(ev);
            self.next += 1;
        }
        out
    }

    fn reset(&mut self) {
        self.resets += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_emits_events_as_audio_time_passes() {
        let t0 = Instant::now();
        let mut m = MockEngine::new(vec![
            (Duration::from_secs(1), TranscriptEvent::final_text("one", t0)),
            (Duration::from_millis(500), TranscriptEvent::partial("o", t0)),
        ]);
        assert!(m.feed(&vec![0; 4000]).is_empty()); // 0.25 s
        let ev = m.feed(&vec![0; 4000]); // 0.5 s
        assert!(matches!(ev.as_slice(), [TranscriptEvent::Partial { text, .. }] if text == "o"));
        let ev = m.feed(&vec![0; 8000]); // 1.0 s
        assert!(matches!(ev.as_slice(), [TranscriptEvent::Final { text, .. }] if text == "one"));
        assert!(m.feed(&vec![0; 16000]).is_empty());
    }

    #[test]
    fn mock_reports_unknown_words() {
        let mut m = MockEngine::new(vec![]).with_unknown_words(vec!["xyzzy".into()]);
        let unknown = m.set_vocabulary(&["hello".into(), "xyzzy".into()]).unwrap();
        assert_eq!(unknown, vec!["xyzzy"]);
        assert_eq!(m.vocabulary.len(), 2);
    }
}
