//! Transcript events: what a speech engine produces. These types live in core
//! (not in `prompter-speech`) because the `PositionEstimator` consumes them and
//! core must not depend on the speech crate. See DECISIONS D12.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Who is talking. Always `Self_` in V1; `Other` is the reserved seam for
/// coaching-call mode where a second audio source carries the other party.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Speaker {
    #[serde(rename = "self")]
    Self_,
    Other(u8),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WordTiming {
    pub word: String,
    /// Seconds from the start of the recognizer stream.
    pub start: f32,
    pub end: f32,
    pub conf: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EngineStatus {
    Ready,
    Rebuilding,
    UnknownWords { words: Vec<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum TranscriptEvent {
    Partial { text: String, t: Instant, speaker: Speaker },
    Final { text: String, t: Instant, words: Vec<WordTiming>, speaker: Speaker },
    /// No change in the partial result for this long.
    Silence { since: Duration, t: Instant },
    EngineStatus(EngineStatus),
}

impl TranscriptEvent {
    pub fn final_text(text: impl Into<String>, t: Instant) -> Self {
        TranscriptEvent::Final { text: text.into(), t, words: Vec::new(), speaker: Speaker::Self_ }
    }

    pub fn partial(text: impl Into<String>, t: Instant) -> Self {
        TranscriptEvent::Partial { text: text.into(), t, speaker: Speaker::Self_ }
    }

    pub fn silence(since: Duration, t: Instant) -> Self {
        TranscriptEvent::Silence { since, t }
    }

    /// When the event happened (audio time). `EngineStatus` has no time.
    pub fn time(&self) -> Option<Instant> {
        match self {
            TranscriptEvent::Partial { t, .. } | TranscriptEvent::Final { t, .. } | TranscriptEvent::Silence { t, .. } => Some(*t),
            TranscriptEvent::EngineStatus(_) => None,
        }
    }
}
