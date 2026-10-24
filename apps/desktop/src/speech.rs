//! Speech rig: worker + audio supervisor + the two pump threads that turn
//! transcript and audio events into state changes and window events.
//! Plumbing only; the logic lives in `prompter_core::Session`.

use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use prompter_core::{EngineStatus, HitKind, Lang, PositionEstimate, TranscriptEvent, WordTiming};
use prompter_speech::vosk_engine::find_model_dir;
use prompter_speech::{AudioEvent, AudioSourceConfig, AudioStatus, AudioSupervisor, EngineCommand, EngineFactory, SpeechEngine, SpeechWorker, TranscriptBus, VoskEngine};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{PrompterState, EVENT_INTENT, EVENT_MATCH, EVENT_STATUS, EVENT_TRANSCRIPT};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TranscriptDto {
    Partial { text: String, t_ms: u64 },
    Final { text: String, words: Vec<WordTiming>, t_ms: u64 },
    Silence { since_ms: u64 },
    Status(EngineStatus),
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchDto {
    pub anchor: u32,
    pub block: u32,
    pub kind: HitKind,
    pub confidence: f32,
    pub t_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusDto {
    pub listening: bool,
    pub muted: bool,
    pub audio: AudioStatus,
    pub engine: EngineStatus,
    pub level_db: f32,
    pub unknown_words: Vec<String>,
}

pub struct SpeechRig {
    worker: SpeechWorker,
    supervisor: AudioSupervisor,
}

/// Folder for models downloaded inside the app (Settings → Languages).
pub fn downloaded_models_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_local_data_dir().ok().map(|d| d.join("models"))
}

/// Where Vosk models are looked up, in order: downloaded, bundled with the app,
/// and the repo checkout (development builds only).
pub fn model_roots(app: &AppHandle) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = downloaded_models_dir(app).into_iter().collect();
    if let Ok(res) = app.path().resource_dir() {
        roots.push(res.join("models"));
    }
    // The repo checkout only in development builds: an installed app on a developer's
    // machine must not pick up models it did not download itself.
    if cfg!(debug_assertions) {
        roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models"));
    }
    roots
}

impl SpeechRig {
    pub fn start(app: AppHandle, roots: Vec<PathBuf>, lang: Lang, audio: AudioSourceConfig) -> Result<Self, String> {
        let factory: EngineFactory = Box::new(move |lang| {
            VoskEngine::new(&find_model_dir(&roots, lang)).map(|e| Box::new(e) as Box<dyn SpeechEngine>)
        });
        let bus = Arc::new(TranscriptBus::new());
        let transcript_rx = bus.subscribe();
        let worker = SpeechWorker::spawn(lang, factory, bus).map_err(|e| e.to_string())?;
        let (audio_tx, audio_rx) = channel();
        let supervisor = AudioSupervisor::spawn(worker.sender(), audio_tx);
        supervisor.set_source(audio);
        let started = Instant::now();

        let app_t = app.clone();
        std::thread::Builder::new()
            .name("transcript-pump".into())
            .spawn(move || {
                for ev in transcript_rx {
                    on_transcript(&app_t, ev, started);
                }
            })
            .map_err(|e| e.to_string())?;
        let app_a = app.clone();
        std::thread::Builder::new()
            .name("audio-pump".into())
            .spawn(move || {
                for ev in audio_rx {
                    on_audio(&app_a, ev);
                }
            })
            .map_err(|e| e.to_string())?;

        Ok(SpeechRig { worker, supervisor })
    }

    pub fn set_vocabulary(&self, words: Vec<String>) {
        self.worker.send(EngineCommand::SetVocabulary { words });
    }

    pub fn set_language(&self, lang: Lang) {
        self.worker.send(EngineCommand::SetLanguage(lang));
    }

    pub fn reset(&self) {
        self.worker.send(EngineCommand::Reset);
    }

    pub fn set_source(&self, cfg: AudioSourceConfig) {
        self.supervisor.set_source(cfg);
    }

    pub fn set_muted(&self, muted: bool) {
        self.worker.send(EngineCommand::SetMuted(muted));
    }
}

fn ms_since(started: Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

fn on_transcript(app: &AppHandle, ev: TranscriptEvent, started: Instant) {
    let state = app.state::<Mutex<PrompterState>>();
    let mut st = state.lock().expect("state");
    let t_ms = ms_since(started);

    let dto = match &ev {
        TranscriptEvent::Partial { text, .. } => TranscriptDto::Partial { text: text.clone(), t_ms },
        TranscriptEvent::Final { text, words, .. } => TranscriptDto::Final { text: text.clone(), words: words.clone(), t_ms },
        TranscriptEvent::Silence { since, .. } => TranscriptDto::Silence { since_ms: since.as_millis() as u64 },
        TranscriptEvent::EngineStatus(s) => TranscriptDto::Status(s.clone()),
    };
    let _ = app.emit(EVENT_TRANSCRIPT, &dto);

    if let TranscriptEvent::EngineStatus(s) = &ev {
        if let EngineStatus::UnknownWords { words } = s {
            st.unknown_words = words.clone();
        }
        st.engine_status = s.clone();
        let status = st.status();
        let _ = app.emit(EVENT_STATUS, &status);
    }

    // Auto-stop after a long silence (setting in minutes; 0 = never).
    if let TranscriptEvent::Silence { since, .. } = &ev {
        let minutes = st.settings.auto_stop_minutes;
        if st.listening && minutes > 0 && since.as_secs() >= u64::from(minutes) * 60 {
            drop(st);
            crate::stop_listening_later(app.clone());
            return;
        }
    }

    let mut intents = Vec::new();
    let mut estimate: Option<PositionEstimate> = None;
    if let Some(session) = st.session.as_mut() {
        let outcome = session.on_transcript(&ev);
        intents = outcome.intents;
        estimate = outcome.estimate;
    }
    if let Some(est) = estimate {
        let block = st.session.as_ref().and_then(|s| s.current_block());
        if let Some(block) = block {
            st.current_block = Some(block);
            let _ = app.emit(
                EVENT_MATCH,
                MatchDto { anchor: est.anchor.0, block: block.0, kind: est.kind, confidence: est.confidence, t_ms },
            );
        }
    }
    for intent in intents {
        st.last_intent = Some(intent.clone());
        let _ = app.emit(EVENT_INTENT, &intent);
    }
}

fn on_audio(app: &AppHandle, ev: AudioEvent) {
    let state = app.state::<Mutex<PrompterState>>();
    let mut st = state.lock().expect("state");
    match ev {
        AudioEvent::Level { rms_db } => {
            st.level_db = rms_db;
            // Levels arrive 10×/s; that is the status refresh rate for the meter.
            let status = st.status();
            let _ = app.emit(EVENT_STATUS, &status);
        }
        AudioEvent::Status(s) => {
            st.audio_status = s;
            let status = st.status();
            let _ = app.emit(EVENT_STATUS, &status);
        }
        AudioEvent::DeviceLost(_) => {
            // The controller must not keep cruising on a dead mic.
            if let Some(session) = st.session.as_mut() {
                for intent in session.user(prompter_core::ControllerInput::UserHold) {
                    let _ = app.emit(EVENT_INTENT, &intent);
                }
            }
        }
        AudioEvent::DefaultInputChanged(_) | AudioEvent::PermissionDenied => {}
    }
}
