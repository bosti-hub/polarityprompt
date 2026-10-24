//! `prompter-core`: the pure logic of PolarityPrompt.
//!
//! This crate holds the document model, anchors, the scroll controller and the
//! `PositionEstimator` trait. It knows nothing about Tauri, audio or speech
//! recognition, so everything in here can be unit-tested with scripted inputs.
//!
//! See `docs/ARCHITECTURE.md` section 2 for the full design.

pub mod aligner;
pub mod anchors;
pub mod controller;
pub mod document;
pub mod estimator;
pub mod pace;
pub mod session;
mod stopwords;
pub mod transcript;

pub use aligner::{AlignerConfig, TextAligner};
pub use anchors::{Anchor, AnchorId};
pub use controller::{AfterGlide, ControllerConfig, ControllerInput, DocPos, GlideProfile, ScrollIntent, ScrollState};
pub use document::{Block, BlockId, BlockKind, Document};
pub use estimator::{AnchorMatcher, EstimatorTuning, HitKind, MatcherConfig, PositionEstimate, PositionEstimator};
pub use session::Session;
pub use transcript::{EngineStatus, Speaker, TranscriptEvent, WordTiming};

use serde::{Deserialize, Serialize};

/// Script language. Comes from the script's front-matter (`lang:`) or the
/// settings default. Drives which speech model is loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    Nl,
    En,
    De,
    Fr,
    Es,
    Pt,
}

impl Lang {
    /// Every supported language, in the order the UI lists them.
    pub const ALL: [Lang; 6] = [Lang::Nl, Lang::En, Lang::De, Lang::Fr, Lang::Es, Lang::Pt];

    /// The ISO 639-1 code used in front-matter and settings (`nl`, `en`, …).
    pub fn code(self) -> &'static str {
        match self {
            Lang::Nl => "nl",
            Lang::En => "en",
            Lang::De => "de",
            Lang::Fr => "fr",
            Lang::Es => "es",
            Lang::Pt => "pt",
        }
    }

    /// English name with the native name, for the UI.
    pub fn name(self) -> &'static str {
        match self {
            Lang::Nl => "Dutch (Nederlands)",
            Lang::En => "English",
            Lang::De => "German (Deutsch)",
            Lang::Fr => "French (Français)",
            Lang::Es => "Spanish (Español)",
            Lang::Pt => "Portuguese (Português)",
        }
    }

    /// Parse a front-matter value such as `nl` or `en`. Unknown values give `None`.
    pub fn parse(value: &str) -> Option<Lang> {
        let value = value.trim().to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| l.code() == value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_parses_known_codes_and_rejects_unknown() {
        assert_eq!(Lang::parse("nl"), Some(Lang::Nl));
        assert_eq!(Lang::parse(" EN "), Some(Lang::En));
        assert_eq!(Lang::parse("de"), Some(Lang::De));
        assert_eq!(Lang::parse("xx"), None);
    }

    #[test]
    fn lang_serializes_as_its_code() {
        for lang in Lang::ALL {
            assert_eq!(serde_json::to_string(&lang).unwrap(), format!("\"{}\"", lang.code()));
            assert_eq!(Lang::parse(lang.code()), Some(lang));
        }
    }
}
