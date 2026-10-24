//! Settings → Languages: which speech models are installed, and downloading or
//! removing the optional ones (DECISIONS D21). A download runs on its own thread
//! and reports on `EVENT_DOWNLOAD`; the work itself is `prompter_speech::models`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use prompter_core::Lang;
use prompter_speech::models::{self, Progress};
use prompter_speech::vosk_engine::find_model_dir;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{speech, PrompterState};

pub const EVENT_DOWNLOAD: &str = "prompter://download";

/// Cancel flags of the downloads that are running.
#[derive(Default)]
pub struct Downloads(Mutex<HashMap<Lang, Arc<AtomicBool>>>);

#[derive(Debug, Clone, Serialize)]
pub struct LanguageDto {
    pub lang: Lang,
    pub name: String,
    pub installed: bool,
    /// Part of the app: cannot be removed.
    pub bundled: bool,
    pub downloading: bool,
    pub download_mb: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadDto {
    pub lang: Lang,
    /// `downloading`, `verifying`, `installing`, `done`, `failed` or `cancelled`.
    pub state: &'static str,
    pub received: u64,
    pub total: u64,
    pub error: Option<String>,
}

#[tauri::command]
pub fn list_languages(app: AppHandle, downloads: State<'_, Downloads>) -> Vec<LanguageDto> {
    let roots = speech::model_roots(&app);
    let running = downloads.0.lock().expect("downloads");
    Lang::ALL
        .into_iter()
        .map(|lang| {
            let spec = models::spec(lang);
            LanguageDto {
                lang,
                name: lang.name().to_string(),
                installed: find_model_dir(&roots, lang).exists(),
                bundled: spec.bundled,
                downloading: running.contains_key(&lang),
                download_mb: (spec.zip_bytes / 1_000_000) as u32,
            }
        })
        .collect()
}

#[tauri::command]
pub fn download_language(app: AppHandle, downloads: State<'_, Downloads>, lang: Lang) -> Result<(), String> {
    let spec = models::spec(lang);
    if spec.bundled {
        return Err(format!("{} is part of the app already", lang.name()));
    }
    let root = speech::downloaded_models_dir(&app).ok_or("Cannot find the app's data folder")?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut running = downloads.0.lock().expect("downloads");
        if running.contains_key(&lang) {
            return Ok(());
        }
        running.insert(lang, cancel.clone());
    }
    std::thread::Builder::new()
        .name(format!("download-{}", lang.code()))
        .spawn(move || {
            let emit = |state: &'static str, received: u64, total: u64, error: Option<String>| {
                let _ = app.emit(EVENT_DOWNLOAD, DownloadDto { lang, state, received, total, error });
            };
            let result = std::fs::create_dir_all(&root).map_err(models::ModelError::Io).and_then(|_| {
                models::install(spec, &root, &cancel, |p| match p {
                    Progress::Downloading { received, total } => emit("downloading", received, total, None),
                    Progress::Verifying => emit("verifying", spec.zip_bytes, spec.zip_bytes, None),
                    Progress::Installing => emit("installing", spec.zip_bytes, spec.zip_bytes, None),
                })
            });
            app.state::<Downloads>().0.lock().expect("downloads").remove(&lang);
            match result {
                Ok(_) => emit("done", spec.zip_bytes, spec.zip_bytes, None),
                Err(models::ModelError::Cancelled) => emit("cancelled", 0, spec.zip_bytes, None),
                Err(e) => emit("failed", 0, spec.zip_bytes, Some(e.to_string())),
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn cancel_download(downloads: State<'_, Downloads>, lang: Lang) {
    if let Some(flag) = downloads.0.lock().expect("downloads").get(&lang) {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Removes a downloaded model, unless the microphone is using it right now.
#[tauri::command]
pub fn remove_language(app: AppHandle, lang: Lang) -> Result<(), String> {
    {
        let state = app.state::<Mutex<PrompterState>>();
        let st = state.lock().expect("state");
        let current = st.document.as_ref().map(|d| d.language).unwrap_or(st.settings.default_language);
        if st.listening && current == lang {
            return Err(format!("{} is in use. Stop listening first.", lang.name()));
        }
    }
    let root = speech::downloaded_models_dir(&app).ok_or("Cannot find the app's data folder")?;
    models::remove(lang, &root).map_err(|e| e.to_string())
}
