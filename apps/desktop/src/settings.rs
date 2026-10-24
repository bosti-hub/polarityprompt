//! User settings, persisted as JSON in the app config directory.
//! See `docs/ARCHITECTURE.md` section 4 ("Settings").

use std::path::{Path, PathBuf};

use prompter_core::{AfterGlide, EstimatorTuning, Lang};
use prompter_speech::AudioSourceConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub font_family: String,
    /// CSS pixels.
    pub font_size: f32,
    /// Multiplier of font size.
    pub line_height: f32,
    /// Fraction of the viewport width, 0.0–0.4.
    pub margin_left: f32,
    pub margin_right: f32,
    pub mirror_h: bool,
    pub mirror_v: bool,
    /// Read-line position as a fraction of the viewport height (0 = top).
    pub read_line: f32,
    pub color_bg: String,
    pub color_fg: String,
    /// Cruise speed in lines per second.
    pub base_speed: f32,
    pub after_glide: AfterGlide,
    pub default_language: Lang,
    /// Last opened script, reopened on start when it still exists.
    pub last_document: Option<PathBuf>,
    /// Microphone selection; empty sources = system default input.
    pub audio: AudioSourceConfig,
    /// Operator window: panels on the right instead of the left.
    pub sidebar_right: bool,
    /// Alignment thresholds (Advanced tab).
    pub tuning: EstimatorTuning,
    /// Text alignment: "left" or "center".
    pub align: String,
    /// How strongly the text above the read line is dimmed (0 = off, 1 = invisible).
    pub dim_above: f32,
    /// Countdown before the scroll starts, seconds (0 = none).
    pub countdown_seconds: u8,
    /// Stop listening after this many minutes of silence (0 = never).
    pub auto_stop_minutes: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            // Helvetica Neue is not installed on Windows; Segoe UI is its system sans.
            font_family: if cfg!(windows) { "Segoe UI" } else { "Helvetica Neue" }.to_string(),
            font_size: 64.0,
            line_height: 1.35,
            margin_left: 0.08,
            margin_right: 0.08,
            mirror_h: false,
            mirror_v: false,
            read_line: 0.35,
            color_bg: "#000000".to_string(),
            color_fg: "#f5f2ec".to_string(),
            base_speed: 2.0,
            after_glide: AfterGlide::Cruise,
            // English is the only language inside the installer (DECISIONS D22).
            default_language: Lang::En,
            last_document: None,
            audio: AudioSourceConfig::default(),
            sidebar_right: false,
            tuning: EstimatorTuning::default(),
            align: "left".to_string(),
            dim_above: 0.5,
            countdown_seconds: 3,
            auto_stop_minutes: 0,
        }
    }
}

impl Settings {
    /// Clamp values into sane ranges so a hand-edited or corrupt file cannot break the layout.
    pub fn sanitized(mut self) -> Self {
        self.font_size = self.font_size.clamp(16.0, 200.0);
        self.line_height = self.line_height.clamp(1.0, 2.5);
        self.margin_left = self.margin_left.clamp(0.0, 0.4);
        self.margin_right = self.margin_right.clamp(0.0, 0.4);
        self.read_line = self.read_line.clamp(0.05, 0.95);
        self.base_speed = self.base_speed.clamp(0.25, 8.0);
        if self.font_family.trim().is_empty() {
            self.font_family = Settings::default().font_family;
        }
        if self.align != "center" {
            self.align = "left".to_string();
        }
        self.dim_above = self.dim_above.clamp(0.0, 1.0);
        self.countdown_seconds = self.countdown_seconds.min(10);
        self.auto_stop_minutes = self.auto_stop_minutes.min(120);
        self
    }

    pub fn load(path: &Path) -> Settings {
        match std::fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str::<Settings>(&text) {
                Ok(s) => s.sanitized(),
                Err(e) => {
                    eprintln!("settings: {} is not valid, using defaults ({e})", path.display());
                    Settings::default()
                }
            },
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).expect("settings serialize");
        // Write to a temp file first so a crash mid-write never leaves a half file.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("pp-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        let s = Settings { font_size: 72.0, mirror_h: true, ..Default::default() };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_or_corrupt_file_gives_defaults() {
        assert_eq!(Settings::load(Path::new("/nonexistent/settings.json")), Settings::default());
        let dir = std::env::temp_dir().join(format!("pp-settings-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn partial_file_fills_in_defaults_and_clamps() {
        let s: Settings = serde_json::from_str(r#"{"font_size": 9999, "mirror_v": true}"#).unwrap();
        let s = s.sanitized();
        assert_eq!(s.font_size, 200.0);
        assert!(s.mirror_v);
        assert_eq!(s.font_family, Settings::default().font_family);
    }
}
