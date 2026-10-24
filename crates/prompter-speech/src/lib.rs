//! `prompter-speech`: audio capture and streaming speech recognition.
//!
//! Three threads (see `docs/ARCHITECTURE.md` section 1):
//! - the cpal callback pushes samples into a ring buffer and nothing else;
//! - the `SpeechWorker` drains the ring, downsamples to 16 kHz mono, feeds the
//!   `SpeechEngine` and publishes `TranscriptEvent`s on the `TranscriptBus`;
//! - the `AudioSupervisor` owns the stream and handles device loss/reconnect.

pub mod audio;
pub mod bus;
pub mod engine;
pub mod models;
pub mod resample;
pub mod vosk_engine;
pub mod wav;
pub mod worker;

pub use audio::{AudioEvent, AudioSourceConfig, AudioStatus, AudioSupervisor, ChannelTag, DeviceId, DeviceInfo, SourceMode, SourceSel};
pub use bus::TranscriptBus;
pub use engine::{EngineCommand, EngineError, MockEngine, SpeechEngine, ENGINE_SAMPLE_RATE};
pub use prompter_core::{EngineStatus, Lang, Speaker, TranscriptEvent, WordTiming};
pub use vosk_engine::VoskEngine;
pub use worker::{EngineFactory, SpeechWorker};
