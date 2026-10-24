// Mirror of the Rust types that cross IPC. Keep in sync with
// apps/desktop/src/{lib,settings,speech}.rs and crates/prompter-core. The UI
// never mutates these; it renders them and sends commands.

export type Lang = "nl" | "en" | "de" | "fr" | "es" | "pt";

/** Every language the app knows, in UI order, with its display name (Lang::name in Rust). */
export const LANGUAGES: { lang: Lang; name: string }[] = [
  { lang: "nl", name: "Dutch (Nederlands)" },
  { lang: "en", name: "English" },
  { lang: "de", name: "German (Deutsch)" },
  { lang: "fr", name: "French (Français)" },
  { lang: "es", name: "Spanish (Español)" },
  { lang: "pt", name: "Portuguese (Português)" },
];

/** A row in Settings → Languages. */
export interface LanguageInfo {
  lang: Lang;
  name: string;
  installed: boolean;
  /** Part of the app: cannot be removed. */
  bundled: boolean;
  downloading: boolean;
  download_mb: number;
}

/** Progress of a model download (event prompter://download). */
export interface DownloadDto {
  lang: Lang;
  state: "downloading" | "verifying" | "installing" | "done" | "failed" | "cancelled";
  received: number;
  total: number;
  error: string | null;
}
export type WindowRole = "operator" | "talent";
export type AfterGlide = "cruise" | "hold";

export interface SourceSel {
  device: string;
  channel?: "self";
}

export interface AudioSourceConfig {
  sources: SourceSel[];
  mode?: "single" | "mix";
}

export interface Settings {
  font_family: string;
  font_size: number;
  line_height: number;
  margin_left: number;
  margin_right: number;
  mirror_h: boolean;
  mirror_v: boolean;
  read_line: number;
  color_bg: string;
  color_fg: string;
  base_speed: number;
  after_glide: AfterGlide;
  default_language: Lang;
  last_document: string | null;
  audio: AudioSourceConfig;
  sidebar_right: boolean;
  tuning: EstimatorTuning;
  align: "left" | "center";
  dim_above: number;
  countdown_seconds: number;
  auto_stop_minutes: number;
}

export interface EstimatorTuning {
  min_score: number;
  jump_words: number;
  jump_penalty: number;
  back_penalty: number;
  far_penalty: number;
  min_interval_ms: number;
}

export const DEFAULT_TUNING: EstimatorTuning = {
  min_score: 1.6,
  jump_words: 40,
  jump_penalty: 1.0,
  back_penalty: 1.2,
  far_penalty: 2.0,
  min_interval_ms: 250,
};

export interface EngineInfo {
  name: string;
  models_dir: string;
  languages: Lang[];
}

export type BlockKind = "heading" | "paragraph" | "talking_points" | "rule";

export interface Block {
  id: number;
  kind: BlockKind;
  level?: number;
  html: string;
  plain: string;
  word_count: number;
  bookmark?: string;
}

export interface Anchor {
  id: number;
  block: number;
  ordinal: number;
  keywords: string[];
  positions: number[];
  min_hits: number;
  explicit: boolean;
}

export interface Document {
  path: string;
  title: string | null;
  hash: number;
  language: Lang;
  blocks: Block[];
  anchors: Anchor[];
}

export type AudioStatus =
  | { kind: "stopped" }
  | { kind: "capturing"; device: string; sample_rate: number; channels: number }
  | { kind: "reconnecting" }
  | { kind: "no_signal"; seconds: number }
  | { kind: "permission_denied" }
  | { kind: "no_device" };

export type EngineStatus = { kind: "ready" } | { kind: "rebuilding" } | { kind: "unknown_words"; words: string[] };

export interface StatusDto {
  listening: boolean;
  muted: boolean;
  audio: AudioStatus;
  engine: EngineStatus;
  level_db: number;
  unknown_words: string[];
}

export interface DeviceInfo {
  id: string;
  name: string;
  is_default: boolean;
}

/** A screen for the talent window picker, numbered left to right ("Screen 1" = index 0). */
export interface MonitorInfo {
  index: number;
  width: number;
  height: number;
  /** The operator window is on this screen. */
  operator: boolean;
  /** The talent window is on this screen. */
  talent: boolean;
}

export interface WordTiming {
  word: string;
  start: number;
  end: number;
  conf: number;
}

export type TranscriptDto =
  | { kind: "partial"; text: string; t_ms: number }
  | { kind: "final"; text: string; words: WordTiming[]; t_ms: number }
  | { kind: "silence"; since_ms: number }
  | { kind: "status"; [k: string]: unknown };

export type HitKind = { kind: "advance"; skipped: number } | { kind: "retake" } | { kind: "relocate"; distance: number };

export interface MatchDto {
  anchor: number;
  block: number;
  kind: HitKind;
  confidence: number;
  t_ms: number;
}

export interface Snapshot {
  app_version: string;
  settings: Settings;
  document: Document | null;
  last_error: string | null;
  status: StatusDto;
  last_intent: ScrollIntent | null;
  current_block: number | null;
  read_line_pos: DocPos | null;
  talent_open: boolean;
}

// ---- Scroll intents (docs/ARCHITECTURE.md 2.5). Produced by Rust; the dev
// panel can also fire them directly for animator tuning.

export interface DocPos {
  block: number;
  frac: number;
}

export type GlideProfile = "near" | "skip" | "far";

export type ScrollIntent =
  | { type: "cruise"; speed: number }
  | { type: "hold" }
  | { type: "glide_to"; pos: DocPos; profile: GlideProfile; then: AfterGlide; seq: number }
  | { type: "pause" }
  | { type: "snap"; pos: DocPos };

export type UserAction =
  | { type: "start" }
  | { type: "pause" }
  | { type: "hold" }
  | { type: "jump"; pos: DocPos }
  | { type: "jump_hold"; pos: DocPos }
  | { type: "reset" }
  | { type: "reposition"; pos: DocPos };

export interface CountdownDto {
  seconds_left: number;
}
