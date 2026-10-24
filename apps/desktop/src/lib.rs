//! Tauri host for PolarityPrompt.
//!
//! This crate is plumbing only: it owns the windows, the settings file, the
//! speech rig and the IPC commands, and forwards everything else to
//! `prompter-core` / `prompter-speech`. See `docs/ARCHITECTURE.md` section 4.
//!
//! Rule 1 in CLAUDE.md: the state in here is the single source of truth. Every
//! mutation goes through a command and ends with an emit.

mod languages;
mod settings;
mod speech;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use prompter_core::{AlignerConfig, BlockId, ControllerConfig, ControllerInput, DocPos, Document, EngineStatus, Lang, PositionEstimator, ScrollIntent, Session, TextAligner};
use prompter_speech::vosk_engine::find_model_dir;
use prompter_speech::{AudioStatus, DeviceInfo};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;

pub use settings::Settings;
use speech::{SpeechRig, StatusDto};

pub const EVENT_SNAPSHOT: &str = "prompter://snapshot";
pub const EVENT_INTENT: &str = "prompter://intent";
pub const EVENT_STATUS: &str = "prompter://status";
pub const EVENT_TRANSCRIPT: &str = "prompter://transcript";
pub const EVENT_MATCH: &str = "prompter://match";
pub const EVENT_COUNTDOWN: &str = "prompter://countdown";

/// Everything a window needs to render. Sent on connect and after every mutation.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub app_version: String,
    pub settings: Settings,
    pub document: Option<Document>,
    pub last_error: Option<String>,
    pub status: StatusDto,
    pub last_intent: Option<ScrollIntent>,
    pub current_block: Option<BlockId>,
    /// Where the operator's read line is (reported a few times per second), so a
    /// late-joining window can start at the same place.
    pub read_line_pos: Option<DocPos>,
    pub talent_open: bool,
}

pub struct PrompterState {
    settings: Settings,
    settings_path: PathBuf,
    document: Option<Document>,
    last_error: Option<String>,
    session: Option<Session>,
    rig: Option<SpeechRig>,
    listening: bool,
    muted: bool,
    audio_status: AudioStatus,
    engine_status: EngineStatus,
    level_db: f32,
    unknown_words: Vec<String>,
    last_intent: Option<ScrollIntent>,
    current_block: Option<BlockId>,
    read_line_pos: Option<DocPos>,
    talent_open: bool,
}

impl PrompterState {
    fn status(&self) -> StatusDto {
        StatusDto {
            listening: self.listening,
            muted: self.muted,
            audio: self.audio_status.clone(),
            engine: self.engine_status.clone(),
            level_db: self.level_db,
            unknown_words: self.unknown_words.clone(),
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            settings: self.settings.clone(),
            document: self.document.clone(),
            last_error: self.last_error.clone(),
            status: self.status(),
            last_intent: self.last_intent.clone(),
            current_block: self.current_block,
            read_line_pos: self.read_line_pos,
            talent_open: self.talent_open,
        }
    }

    fn controller_config(&self) -> ControllerConfig {
        ControllerConfig {
            base_speed: self.settings.base_speed,
            after_glide: self.settings.after_glide,
            ..Default::default()
        }
    }

    fn load_document(&mut self, path: &Path) -> Result<(), String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
        let doc = Document::parse(path, &text, self.settings.default_language);
        let words = doc.vocabulary();
        if let Some(rig) = &self.rig {
            rig.set_language(doc.language);
            rig.set_vocabulary(words);
        }
        let mut aligner: Box<dyn PositionEstimator> = Box::new(TextAligner::new(AlignerConfig::default()));
        aligner.configure(&self.settings.tuning);
        self.session = Some(Session::new(doc.clone(), aligner, self.controller_config()));
        self.document = Some(doc);
        self.last_error = None;
        self.current_block = None;
        self.last_intent = None;
        self.read_line_pos = None;
        self.unknown_words.clear();
        self.settings.last_document = Some(path.to_path_buf());
        self.persist_settings();
        Ok(())
    }

    fn persist_settings(&self) {
        if let Err(e) = self.settings.save(&self.settings_path) {
            eprintln!("settings: could not save to {}: {e}", self.settings_path.display());
        }
    }
}

type Shared<'a> = State<'a, Mutex<PrompterState>>;

fn broadcast(app: &AppHandle, snapshot: &Snapshot) {
    if let Err(e) = app.emit(EVENT_SNAPSHOT, snapshot) {
        eprintln!("emit {EVENT_SNAPSHOT}: {e}");
    }
}

fn emit_intents(app: &AppHandle, st: &mut PrompterState, intents: Vec<ScrollIntent>) {
    for intent in intents {
        st.last_intent = Some(intent.clone());
        if let Err(e) = app.emit(EVENT_INTENT, &intent) {
            eprintln!("emit {EVENT_INTENT}: {e}");
        }
    }
}

// ---------------------------------------------------------------- commands

#[tauri::command]
fn get_snapshot(state: Shared<'_>) -> Snapshot {
    state.lock().expect("state").snapshot()
}

#[tauri::command]
fn open_document(app: AppHandle, state: Shared<'_>, path: PathBuf) -> Result<Snapshot, String> {
    let mut st = state.lock().expect("state");
    let result = st.load_document(&path);
    if let Err(e) = &result {
        st.last_error = Some(e.clone());
    }
    let snap = st.snapshot();
    drop(st);
    broadcast(&app, &snap);
    // A new script starts at the top.
    let _ = app.emit(EVENT_INTENT, ScrollIntent::Snap { pos: DocPos::start_of(BlockId(0)) });
    result.map(|_| snap)
}

/// Built-in practice scripts, one per language, compiled into the app (sources in `examples/`).
const PRACTICE_NL: &str = include_str!("../../../examples/demo-nl.md");
const PRACTICE_EN: &str = include_str!("../../../examples/demo-en.md");
const PRACTICE_DE: &str = include_str!("../../../examples/demo-de.md");
const PRACTICE_FR: &str = include_str!("../../../examples/demo-fr.md");
const PRACTICE_ES: &str = include_str!("../../../examples/demo-es.md");
const PRACTICE_PT: &str = include_str!("../../../examples/demo-pt.md");

/// Opens the built-in practice script. A fresh copy is written to the app's data
/// folder each time, so it behaves like any other script file (and can be reopened).
#[tauri::command]
fn open_practice_script(app: AppHandle, state: Shared<'_>, lang: Lang) -> Result<Snapshot, String> {
    let text = match lang {
        Lang::Nl => PRACTICE_NL,
        Lang::En => PRACTICE_EN,
        Lang::De => PRACTICE_DE,
        Lang::Fr => PRACTICE_FR,
        Lang::Es => PRACTICE_ES,
        Lang::Pt => PRACTICE_PT,
    };
    let name = format!("practice-coffee-{}.md", lang.code());
    let dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("practice");
    let path = dir.join(name);
    std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::write(&path, text))
        .map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    open_document(app, state, path)
}

/// Speech engine description for the Advanced tab.
#[derive(Debug, Clone, Serialize)]
pub struct EngineInfo {
    pub name: String,
    pub models_dir: String,
    pub languages: Vec<Lang>,
}

#[tauri::command]
fn engine_info(app: AppHandle) -> EngineInfo {
    let roots = speech::model_roots(&app);
    let languages = Lang::ALL.into_iter().filter(|l| find_model_dir(&roots, *l).exists()).collect();
    // The folder of the bundled models (the English one is always there).
    let models_dir = find_model_dir(&roots, Lang::En).parent().map(|p| p.display().to_string()).unwrap_or_default();
    EngineInfo { name: "Vosk (small models, streaming)".into(), models_dir, languages }
}

/// Overrides the language of the open script (front-matter `lang:` or the default).
#[tauri::command]
fn set_document_language(app: AppHandle, state: Shared<'_>, lang: Lang) -> Snapshot {
    let mut st = state.lock().expect("state");
    if let Some(doc) = st.document.as_mut() {
        doc.language = lang;
    }
    if let Some(s) = st.session.as_mut() {
        s.set_language(lang);
    }
    if let Some(rig) = &st.rig {
        rig.set_language(lang);
    }
    let snap = st.snapshot();
    drop(st);
    broadcast(&app, &snap);
    snap
}

/// Reopens the script from the previous session, if it still exists.
#[tauri::command]
fn reopen_last_document(app: AppHandle, state: Shared<'_>) -> Result<Option<Snapshot>, String> {
    let last = state.lock().expect("state").settings.last_document.clone();
    match last {
        Some(path) if path.exists() => open_document(app, state, path).map(Some),
        _ => Ok(None),
    }
}

/// Shows the native file picker, then opens the chosen script. `None` = cancelled.
#[tauri::command]
async fn open_document_dialog(app: AppHandle) -> Result<Option<Snapshot>, String> {
    let picker = app
        .dialog()
        .file()
        .set_title("Open script")
        .add_filter("Markdown", &["md", "markdown", "txt"]);
    let picked = tauri::async_runtime::spawn_blocking(move || picker.blocking_pick_file())
        .await
        .map_err(|e| e.to_string())?;
    let Some(file) = picked else { return Ok(None) };
    let path = file.into_path().map_err(|e| e.to_string())?;
    let state = app.state::<Mutex<PrompterState>>();
    open_document(app.clone(), state, path).map(Some)
}

#[tauri::command]
fn set_settings(app: AppHandle, state: Shared<'_>, settings: Settings) -> Snapshot {
    let mut st = state.lock().expect("state");
    let mut settings = settings.sanitized();
    settings.last_document = st.settings.last_document.clone();
    let audio_changed = settings.audio != st.settings.audio;
    st.settings = settings;
    st.persist_settings();
    let cfg = st.controller_config();
    let tuning = st.settings.tuning.clone();
    let intents = st
        .session
        .as_mut()
        .map(|s| {
            s.set_tuning(&tuning);
            s.set_config(cfg)
        })
        .unwrap_or_default();
    emit_intents(&app, &mut st, intents);
    if audio_changed && st.listening {
        if let Some(rig) = &st.rig {
            rig.set_source(st.settings.audio.clone());
        }
    }
    let snap = st.snapshot();
    drop(st);
    broadcast(&app, &snap);
    snap
}

/// Candidate font families. The UI keeps only the ones the system actually has (DECISIONS D11).
#[tauri::command]
fn list_fonts() -> Vec<String> {
    [
        "Helvetica Neue", "Helvetica", "Arial", "Verdana", "Tahoma", "Trebuchet MS", "Gill Sans",
        "Avenir", "Avenir Next", "Futura", "Optima", "Segoe UI", "Calibri", "Inter", "Roboto",
        "Open Sans", "Lato", "Source Sans 3", "Atkinson Hyperlegible", "Lexend", "SF Pro",
        "Georgia", "Times New Roman", "Palatino", "Baskerville", "Charter", "Garamond", "Cambria",
        "Menlo", "Monaco", "Courier New", "Consolas", "JetBrains Mono",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[tauri::command]
fn list_audio_devices() -> Vec<DeviceInfo> {
    prompter_speech::audio::list_input_devices()
}

/// Async so that loading the speech model (a second or more) happens off the main
/// thread; a synchronous command would freeze every window meanwhile (on Windows
/// the operator window shows "Not responding").
#[tauri::command]
async fn start_listening(app: AppHandle) -> Result<Snapshot, String> {
    let state = app.state::<Mutex<PrompterState>>();
    let to_start = {
        let st = state.lock().expect("state");
        st.rig.is_none().then(|| {
            let lang = st.document.as_ref().map(|d| d.language).unwrap_or(st.settings.default_language);
            (lang, st.settings.audio.clone())
        })
    };
    if let Some((lang, audio)) = to_start {
        // The state lock is not held while the model loads.
        let roots = speech::model_roots(&app);
        let installed = find_model_dir(&roots, lang).exists();
        let app_rig = app.clone();
        let started = tauri::async_runtime::spawn_blocking(move || SpeechRig::start(app_rig, roots, lang, audio))
            .await
            .map_err(|e| e.to_string())?;
        let mut st = state.lock().expect("state");
        let rig = started.map_err(|e| {
            let msg = if installed {
                format!("{e}.")
            } else {
                format!("The {} speech model is not installed. Download it under Settings → Languages.", lang.name())
            };
            st.last_error = Some(msg.clone());
            msg
        })?;
        // A second click may have finished first; then this rig is simply dropped.
        if st.rig.is_none() {
            if let Some(doc) = &st.document {
                // The script or its language may have changed while the model loaded.
                if doc.language != lang {
                    rig.set_language(doc.language);
                }
                rig.set_vocabulary(doc.vocabulary());
            }
            st.rig = Some(rig);
        }
    } else {
        let st = state.lock().expect("state");
        if let Some(rig) = &st.rig {
            rig.set_source(st.settings.audio.clone());
            rig.reset();
        }
    }
    let mut st = state.lock().expect("state");
    st.listening = true;
    st.last_error = None;
    let snap = st.snapshot();
    drop(st);
    broadcast(&app, &snap);
    Ok(snap)
}

#[tauri::command]
fn stop_listening(app: AppHandle, state: Shared<'_>) -> Snapshot {
    let mut st = state.lock().expect("state");
    // Dropping the rig closes the stream and joins the threads.
    st.rig = None;
    st.listening = false;
    st.audio_status = AudioStatus::Stopped;
    st.level_db = -120.0;
    let intents = st.session.as_mut().map(|s| s.user(ControllerInput::UserHold)).unwrap_or_default();
    emit_intents(&app, &mut st, intents);
    let snap = st.snapshot();
    drop(st);
    broadcast(&app, &snap);
    snap
}

/// User scroll actions. All of them go through the controller in core.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UserAction {
    Start,
    Pause,
    Hold,
    Jump { pos: DocPos },
    /// Jump and stay there (bookmark).
    JumpHold { pos: DocPos },
    Reset,
    /// Manual wheel/trackpad scroll: follow the user, move nothing.
    Reposition { pos: DocPos },
}

/// Countdown tick, broadcast to every window before the scroll starts.
#[derive(Debug, Clone, Serialize)]
pub struct CountdownDto {
    pub seconds_left: u8,
}

#[tauri::command]
fn scroll(app: AppHandle, state: Shared<'_>, action: UserAction) {
    let mut st = state.lock().expect("state");
    if let UserAction::Reposition { pos } = action {
        st.read_line_pos = Some(pos);
        if let Some(s) = st.session.as_mut() {
            s.reposition(pos);
        }
        return;
    }
    // Start with a countdown: tick once a second on a helper thread, then start.
    if let UserAction::Start = action {
        let seconds = st.settings.countdown_seconds;
        if seconds > 0 {
            drop(st);
            let app2 = app.clone();
            std::thread::spawn(move || {
                for left in (1..=seconds).rev() {
                    let _ = app2.emit(EVENT_COUNTDOWN, CountdownDto { seconds_left: left });
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
                let _ = app2.emit(EVENT_COUNTDOWN, CountdownDto { seconds_left: 0 });
                let state = app2.state::<Mutex<PrompterState>>();
                let mut st = state.lock().expect("state");
                let intents = st.session.as_mut().map(|s| s.user(ControllerInput::UserStart)).unwrap_or_default();
                emit_intents(&app2, &mut st, intents);
            });
            return;
        }
    }
    let input = match action {
        UserAction::Reposition { .. } => unreachable!(),
        UserAction::Start => ControllerInput::UserStart,
        UserAction::JumpHold { pos } => ControllerInput::UserJumpHold(pos),
        UserAction::Pause => ControllerInput::UserPause,
        UserAction::Hold => ControllerInput::UserHold,
        UserAction::Jump { pos } => ControllerInput::UserJump(pos),
        UserAction::Reset => ControllerInput::UserReset,
    };
    let intents = st.session.as_mut().map(|s| s.user(input)).unwrap_or_default();
    emit_intents(&app, &mut st, intents);
}

#[tauri::command]
fn set_muted(app: AppHandle, state: Shared<'_>, muted: bool) -> Snapshot {
    let mut st = state.lock().expect("state");
    st.muted = muted;
    if let Some(rig) = &st.rig {
        rig.set_muted(muted);
    }
    let snap = st.snapshot();
    drop(st);
    broadcast(&app, &snap);
    snap
}

/// Called from the speech pump when silence exceeds the auto-stop setting.
pub(crate) fn stop_listening_later(app: AppHandle) {
    std::thread::spawn(move || {
        let state = app.state::<Mutex<PrompterState>>();
        let _ = stop_listening(app.clone(), state);
    });
}

/// The UI animator finished a glide.
#[tauri::command]
fn animator_arrived(app: AppHandle, state: Shared<'_>, seq: u64) {
    let mut st = state.lock().expect("state");
    let intents = st.session.as_mut().map(|s| s.on_arrived(seq)).unwrap_or_default();
    emit_intents(&app, &mut st, intents);
}

/// The block under the read line changed (sent on change only, never per frame).
#[tauri::command]
fn read_line_block(app: AppHandle, state: Shared<'_>, block: u32) {
    let mut st = state.lock().expect("state");
    let intents = st.session.as_mut().map(|s| s.on_read_line_block(BlockId(block))).unwrap_or_default();
    emit_intents(&app, &mut st, intents);
}

/// The operator view reports its read-line position a few times per second while
/// it moves (never per frame). Not broadcast; read by late-joining windows.
#[tauri::command]
fn read_line_pos(state: Shared<'_>, pos: DocPos) {
    state.lock().expect("state").read_line_pos = Some(pos);
}

fn set_talent_open(app: &AppHandle, open: bool) {
    let state = app.state::<Mutex<PrompterState>>();
    let mut st = state.lock().expect("state");
    st.talent_open = open;
    let snap = st.snapshot();
    drop(st);
    broadcast(app, &snap);
}

/// Toggles the talent window: opens it on a second monitor when there is one,
/// closes it when it is already open.
///
/// Async on purpose: building a webview window inside a synchronous command
/// deadlocks on Windows (WebView2 needs the main thread that such a command runs on).
#[tauri::command]
async fn open_window(app: AppHandle, role: String) -> Result<(), String> {
    if role != "talent" {
        return Err(format!("unknown window role {role}"));
    }
    if let Some(w) = app.get_webview_window("talent") {
        w.close().map_err(|e| e.to_string())?;
        set_talent_open(&app, false);
        return Ok(());
    }
    // Prefer a monitor the operator window is not on.
    let operator_monitor = app.get_webview_window("operator").and_then(|w| w.current_monitor().ok().flatten());
    let target = app.available_monitors().ok().and_then(|monitors| {
        monitors.into_iter().find(|m| match &operator_monitor {
            Some(op) => m.position() != op.position(),
            None => true,
        })
    });
    let window = WebviewWindowBuilder::new(&app, "talent", WebviewUrl::App("index.html?role=talent".into()))
        .title("PolarityPrompt — Talent")
        .inner_size(1280.0, 720.0)
        .visible(target.is_none())
        .build()
        .map_err(|e| e.to_string())?;
    if let Some(m) = target {
        if let Err(e) = fill_monitor(&window, &m) {
            eprintln!("talent window: could not place on {:?}: {e}", m.name());
        }
        window.show().map_err(|e| e.to_string())?;
    }
    let app_for_event = app.clone();
    window.on_window_event(move |ev| {
        if let tauri::WindowEvent::Destroyed = ev {
            set_talent_open(&app_for_event, false);
        }
    });
    set_talent_open(&app, true);
    Ok(())
}

/// A rectangle in the desktop's own units: physical pixels on Windows, points on macOS.
///
/// Monitors can use different scaling (a 150 % laptop next to a 100 % screen on Windows,
/// a Retina ×2 next to a ×1 screen on a Mac). Windows places windows in physical pixels,
/// so those are exact there. macOS places them in points, one coordinate space for all
/// screens, and its window calls convert physical pixels with the scale of the screen the
/// window is on *now*, not the target screen: the talent window landed at half size in
/// the middle of a ×1 screen (D23).
#[derive(Debug, Clone, Copy)]
struct DesktopRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Physical pixels per desktop unit on a screen with this scale factor.
fn unit_scale(scale: f64) -> f64 {
    if cfg!(target_os = "macos") {
        scale
    } else {
        1.0
    }
}

fn monitor_rect(m: &tauri::Monitor) -> DesktopRect {
    let s = unit_scale(m.scale_factor());
    DesktopRect {
        x: m.position().x as f64 / s,
        y: m.position().y as f64 / s,
        w: m.size().width as f64 / s,
        h: m.size().height as f64 / s,
    }
}

fn window_rect(window: &tauri::WebviewWindow) -> tauri::Result<DesktopRect> {
    let s = unit_scale(window.scale_factor()?);
    let (pos, size) = (window.outer_position()?, window.outer_size()?);
    Ok(DesktopRect { x: pos.x as f64 / s, y: pos.y as f64 / s, w: size.width as f64 / s, h: size.height as f64 / s })
}

/// The order matters. macOS: move onto the target screen first (a resize is capped to the
/// screen the window is on), then resize, then move again (a resize keeps the bottom edge
/// where it is, so it shifts the top). Windows: position first, because moving onto a
/// monitor with another scale resizes the window.
fn set_window_rect(window: &tauri::WebviewWindow, r: DesktopRect) -> tauri::Result<()> {
    if cfg!(target_os = "macos") {
        let pos = tauri::LogicalPosition::new(r.x, r.y);
        window.set_position(pos)?;
        window.set_size(tauri::LogicalSize::new(r.w, r.h))?;
        window.set_position(pos)
    } else {
        window.set_position(tauri::PhysicalPosition::new(r.x.round() as i32, r.y.round() as i32))?;
        window.set_size(tauri::PhysicalSize::new(r.w.round() as u32, r.h.round() as u32))
    }
}

/// Covers a monitor with a window.
fn fill_monitor(window: &tauri::WebviewWindow, m: &tauri::Monitor) -> tauri::Result<()> {
    set_window_rect(window, monitor_rect(m))
}

/// A window of `w`×`h` points, at most 80 % of the screen `m`, centred on `over` and
/// kept inside that screen.
fn centred_rect(m: &tauri::Monitor, over: DesktopRect, w: f64, h: f64) -> DesktopRect {
    let screen = monitor_rect(m);
    // Points to desktop units: 1 on macOS, the screen's scale elsewhere.
    let per_point = m.scale_factor() / unit_scale(m.scale_factor());
    let w = (w * per_point).min(screen.w * 0.8);
    let h = (h * per_point).min(screen.h * 0.8);
    let x = (over.x + over.w / 2.0 - w / 2.0).clamp(screen.x, screen.x + screen.w - w);
    let y = (over.y + over.h / 2.0 - h / 2.0).clamp(screen.y, screen.y + screen.h - h);
    DesktopRect { x, y, w, h }
}

/// Monitors from left to right (then top to bottom), the order the screen picker shows.
fn monitors_left_to_right(app: &AppHandle) -> Vec<tauri::Monitor> {
    let mut monitors = app.available_monitors().unwrap_or_default();
    monitors.sort_by_key(|m| (m.position().x, m.position().y));
    monitors
}

/// One entry in the talent window's screen picker.
#[derive(Debug, Clone, Serialize)]
pub struct MonitorDto {
    /// Position in the left-to-right order; "Screen 1" is index 0.
    pub index: usize,
    pub width: u32,
    pub height: u32,
    /// The operator window is on this screen.
    pub operator: bool,
    /// The talent window is on this screen.
    pub talent: bool,
}

#[tauri::command]
fn list_monitors(app: AppHandle) -> Vec<MonitorDto> {
    let on = |label: &str| {
        app.get_webview_window(label)
            .and_then(|w| w.current_monitor().ok().flatten())
            .map(|m| *m.position())
    };
    let (operator, talent) = (on("operator"), on("talent"));
    monitors_left_to_right(&app)
        .iter()
        .enumerate()
        .map(|(index, m)| MonitorDto {
            index,
            width: m.size().width,
            height: m.size().height,
            operator: operator == Some(*m.position()),
            talent: talent == Some(*m.position()),
        })
        .collect()
}

/// Moves the talent window. `screen: Some(i)` covers that screen (picker order);
/// `None` brings it back as a normal window centred over the operator window, for
/// when it ended up out of sight.
#[tauri::command]
async fn place_talent_window(app: AppHandle, screen: Option<usize>) -> Result<(), String> {
    let talent = app.get_webview_window("talent").ok_or("The talent window is not open")?;
    let _ = talent.unminimize();
    let _ = talent.unmaximize();
    let _ = talent.set_fullscreen(false);
    match screen {
        Some(i) => {
            let monitors = monitors_left_to_right(&app);
            let m = monitors.get(i).ok_or(format!("There is no screen {}", i + 1))?;
            fill_monitor(&talent, m).map_err(|e| e.to_string())?;
        }
        None => {
            let operator = app.get_webview_window("operator").ok_or("The main window is missing")?;
            let m = operator
                .current_monitor()
                .ok()
                .flatten()
                .ok_or("Cannot tell which screen the main window is on")?;
            let over = window_rect(&operator).map_err(|e| e.to_string())?;
            set_window_rect(&talent, centred_rect(&m, over, 1280.0, 720.0)).map_err(|e| e.to_string())?;
        }
    }
    talent.show().map_err(|e| e.to_string())?;
    talent.set_focus().map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // The main window is the app: closing it quits, also when the talent window is
        // still open (otherwise that window and the speech threads would keep running).
        .on_window_event(|window, event| {
            if window.label() == "operator" && matches!(event, tauri::WindowEvent::Destroyed) {
                window.app_handle().exit(0);
            }
        })
        .setup(|app| {
            let settings_path = app
                .path()
                .app_config_dir()
                .map(|d| d.join("settings.json"))
                .unwrap_or_else(|_| PathBuf::from("settings.json"));
            let settings = Settings::load(&settings_path);
            let state = PrompterState {
                settings,
                settings_path,
                document: None,
                last_error: None,
                session: None,
                rig: None,
                listening: false,
                muted: false,
                audio_status: AudioStatus::Stopped,
                engine_status: EngineStatus::Ready,
                level_db: -120.0,
                unknown_words: Vec::new(),
                last_intent: None,
                current_block: None,
                read_line_pos: None,
                talent_open: false,
            };
            // The app opens on its start screen; the last script is one click away
            // (`reopen_last_document`), never loaded unasked.
            app.manage(Mutex::new(state));
            app.manage(languages::Downloads::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            open_document,
            open_document_dialog,
            reopen_last_document,
            open_practice_script,
            engine_info,
            set_document_language,
            set_settings,
            list_fonts,
            list_audio_devices,
            start_listening,
            stop_listening,
            scroll,
            set_muted,
            animator_arrived,
            read_line_block,
            read_line_pos,
            open_window,
            languages::list_languages,
            languages::download_language,
            languages::cancel_download,
            languages::remove_language,
            list_monitors,
            place_talent_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running PolarityPrompt");
}
