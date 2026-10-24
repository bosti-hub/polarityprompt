// Browser-only stand-in for the Rust host. Settings live in localStorage, the
// document is a pre-parsed copy of examples/demo-nl.md (regenerate with
// `cargo run -p prompter-core --example dump_document -- examples/demo-nl.md`).
// Scroll actions map straight to intents; there is no controller in the browser.

import mockDocument from "./mock-document.json";
import { DEFAULT_TUNING, type CountdownDto, type DeviceInfo, type DocPos, type Document, type DownloadDto, type LanguageInfo, LANGUAGES, type EngineInfo, type Lang, type MatchDto, type MonitorInfo, type ScrollIntent, type Settings, type Snapshot, type StatusDto, type TranscriptDto, type UserAction } from "./types";

const KEY = "polarityprompt.mock.settings";

export const defaultSettings: Settings = {
  font_family: "Helvetica Neue",
  font_size: 64,
  line_height: 1.35,
  margin_left: 0.08,
  margin_right: 0.08,
  mirror_h: false,
  mirror_v: false,
  read_line: 0.35,
  color_bg: "#000000",
  color_fg: "#f5f2ec",
  base_speed: 2,
  after_glide: "cruise",
  default_language: "nl",
  last_document: null,
  audio: { sources: [] },
  sidebar_right: false,
  tuning: { ...DEFAULT_TUNING },
  align: "left",
  dim_above: 0.5,
  countdown_seconds: 3,
  auto_stop_minutes: 0,
};

function loadSettings(): Settings {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) return { ...defaultSettings, ...(JSON.parse(raw) as Partial<Settings>), last_document: (mockDocument as Document).path };
  } catch {
    // ignore: private mode or corrupt value
  }
  return { ...defaultSettings };
}

let settings = loadSettings();
// Browser mode starts on the start screen like the app; "Reopen" loads the example.
let document: Document | null = null;
let listening = false;
let muted = false;
let readLinePos: DocPos | null = null;
let talentOpen = false;
const snapshotListeners = new Set<(s: Snapshot) => void>();
const intentListeners = new Set<(i: ScrollIntent) => void>();
const statusListeners = new Set<(s: StatusDto) => void>();
const transcriptListeners = new Set<(t: TranscriptDto) => void>();
const matchListeners = new Set<(m: MatchDto) => void>();
const countdownListeners = new Set<(c: CountdownDto) => void>();
const downloadListeners = new Set<(d: DownloadDto) => void>();
// Languages in the browser mock: English bundled, the rest "download" in a few seconds.
const installed = new Set<Lang>(["en"]);
const mockDownloads = new Map<Lang, number>();

function status(): StatusDto {
  return {
    listening,
    muted,
    audio: listening ? { kind: "capturing", device: "Browser mock", sample_rate: 48000, channels: 1 } : { kind: "stopped" },
    engine: { kind: "ready" },
    level_db: listening ? -30 : -120,
    unknown_words: listening ? ["koffiedik"] : [],
  };
}

function snapshot(): Snapshot {
  return {
    app_version: "0.1.0-browser",
    settings,
    document,
    last_error: null,
    status: status(),
    last_intent: null,
    current_block: null,
    read_line_pos: readLinePos,
    talent_open: talentOpen,
  };
}

function broadcast() {
  const s = snapshot();
  for (const l of snapshotListeners) l(s);
}

function intent(i: ScrollIntent) {
  for (const l of intentListeners) l(i);
}

let seq = 100;
let demoTimer: number | null = null;

/** Pretend to hear the script: glide anchor by anchor every few seconds. */
function startDemo() {
  let ordinal = 0;
  demoTimer = window.setInterval(() => {
    const anchors = document?.anchors ?? [];
    if (!listening || ordinal >= anchors.length) return;
    const a = anchors[ordinal]!;
    const t: TranscriptDto = { kind: "final", text: a.keywords.join(" "), words: [], t_ms: Date.now() };
    for (const l of transcriptListeners) l(t);
    for (const l of matchListeners) l({ anchor: a.id, block: a.block, kind: { kind: "advance", skipped: 0 }, confidence: 0.7, t_ms: Date.now() });
    intent({ type: "glide_to", pos: { block: a.block, frac: 0 }, profile: "near", then: settings.after_glide, seq: ++seq });
    ordinal += 1;
  }, 4000);
}

export const mock = {
  getSnapshot: async () => snapshot(),
  setSettings: async (next: Settings) => {
    settings = { ...next, last_document: settings.last_document };
    try {
      localStorage.setItem(KEY, JSON.stringify(settings));
    } catch {
      // ignore
    }
    broadcast();
    return snapshot();
  },
  openDocumentDialog: async () => {
    document = mockDocument as Document;
    settings = { ...settings, last_document: (mockDocument as Document).path };
    broadcast();
    return snapshot();
  },
  // The browser mock has one document; it stands in for both practice scripts.
  openPracticeScript: async (lang: Lang) => {
    document = { ...(mockDocument as Document), language: lang };
    settings = { ...settings, last_document: (mockDocument as Document).path };
    broadcast();
    return snapshot();
  },
  engineInfo: async (): Promise<EngineInfo> => ({ name: "Browser mock (no recognition)", models_dir: "—", languages: ["nl", "en"] }),
  setDocumentLanguage: async (lang: Lang) => {
    if (document) document = { ...document, language: lang };
    broadcast();
    return snapshot();
  },
  reopenLastDocument: async () => {
    document = mockDocument as Document;
    broadcast();
    return snapshot();
  },
  listFonts: async () => [
    "Helvetica Neue", "Helvetica", "Arial", "Verdana", "Tahoma", "Trebuchet MS", "Gill Sans",
    "Avenir", "Avenir Next", "Futura", "Optima", "Segoe UI", "Calibri", "Inter", "Roboto",
    "Open Sans", "Lato", "Source Sans 3", "Atkinson Hyperlegible", "Lexend", "SF Pro",
    "Georgia", "Times New Roman", "Palatino", "Baskerville", "Charter", "Garamond", "Cambria",
    "Menlo", "Monaco", "Courier New", "Consolas", "JetBrains Mono",
  ],
  listAudioDevices: async (): Promise<DeviceInfo[]> => [
    { id: "Browser mock", name: "Browser mock", is_default: true },
    { id: "USB Microphone", name: "USB Microphone", is_default: false },
  ],
  startListening: async () => {
    listening = true;
    if (demoTimer === null) startDemo();
    broadcast();
    for (const l of statusListeners) l(status());
    return snapshot();
  },
  stopListening: async () => {
    listening = false;
    if (demoTimer !== null) {
      clearInterval(demoTimer);
      demoTimer = null;
    }
    intent({ type: "hold" });
    broadcast();
    for (const l of statusListeners) l(status());
    return snapshot();
  },
  setMuted: async (m: boolean) => {
    muted = m;
    broadcast();
    return snapshot();
  },
  scroll: async (action: UserAction) => {
    switch (action.type) {
      case "start": {
        const n = settings.countdown_seconds;
        for (let i = n; i >= 1; i--) {
          const left = i;
          window.setTimeout(() => countdownListeners.forEach((l) => l({ seconds_left: left })), (n - i) * 1000);
        }
        window.setTimeout(() => {
          countdownListeners.forEach((l) => l({ seconds_left: 0 }));
          intent({ type: "cruise", speed: settings.base_speed });
        }, n * 1000);
        break;
      }
      case "jump_hold":
        intent({ type: "snap", pos: action.pos });
        intent({ type: "hold" });
        break;
      case "pause":
        intent({ type: "pause" });
        break;
      case "hold":
        intent({ type: "hold" });
        break;
      case "jump":
        intent({ type: "snap", pos: action.pos });
        intent({ type: "cruise", speed: settings.base_speed });
        break;
      case "reset":
        intent({ type: "snap", pos: { block: 0, frac: 0 } });
        break;
      case "reposition":
        readLinePos = action.pos;
        break;
    }
  },
  animatorArrived: async () => {},
  readLineBlock: async () => {},
  readLinePos: async (pos: DocPos) => {
    readLinePos = pos;
  },
  openWindow: async () => {
    // No second window in the browser: open ?role=talent in another tab by hand.
    talentOpen = !talentOpen;
    broadcast();
  },
  listMonitors: async (): Promise<MonitorInfo[]> => [
    { index: 0, width: 2880, height: 1800, operator: true, talent: false },
    { index: 1, width: 1920, height: 1080, operator: false, talent: true },
  ],
  placeTalentWindow: async () => {},
  listLanguages: async (): Promise<LanguageInfo[]> =>
    LANGUAGES.map(({ lang, name }) => ({
      lang,
      name,
      installed: installed.has(lang),
      bundled: lang === "en",
      downloading: mockDownloads.has(lang),
      download_mb: 45,
    })),
  downloadLanguage: async (lang: Lang) => {
    const total = 45_000_000;
    let received = 0;
    const emit = (d: Omit<DownloadDto, "lang" | "total" | "error"> & { error?: string | null }) => {
      for (const l of downloadListeners) l({ lang, total, error: null, ...d });
    };
    const timer = window.setInterval(() => {
      received = Math.min(total, received + total / 15);
      if (received < total) return emit({ state: "downloading", received });
      window.clearInterval(timer);
      mockDownloads.delete(lang);
      installed.add(lang);
      emit({ state: "done", received });
    }, 200);
    mockDownloads.set(lang, timer);
  },
  cancelDownload: async (lang: Lang) => {
    window.clearInterval(mockDownloads.get(lang));
    mockDownloads.delete(lang);
    for (const l of downloadListeners) l({ lang, state: "cancelled", received: 0, total: 0, error: null });
  },
  removeLanguage: async (lang: Lang) => {
    installed.delete(lang);
  },
  onDownload: (cb: (d: DownloadDto) => void) => {
    downloadListeners.add(cb);
    return () => {
      downloadListeners.delete(cb);
    };
  },
  onSnapshot: (cb: (s: Snapshot) => void) => {
    snapshotListeners.add(cb);
    return () => {
      snapshotListeners.delete(cb);
    };
  },
  onIntent: (cb: (i: ScrollIntent) => void) => {
    intentListeners.add(cb);
    return () => {
      intentListeners.delete(cb);
    };
  },
  onStatus: (cb: (s: StatusDto) => void) => {
    statusListeners.add(cb);
    return () => {
      statusListeners.delete(cb);
    };
  },
  onTranscript: (cb: (t: TranscriptDto) => void) => {
    transcriptListeners.add(cb);
    return () => {
      transcriptListeners.delete(cb);
    };
  },
  onCountdown: (cb: (c: CountdownDto) => void) => {
    countdownListeners.add(cb);
    return () => {
      countdownListeners.delete(cb);
    };
  },
  onMatch: (cb: (m: MatchDto) => void) => {
    matchListeners.add(cb);
    return () => {
      matchListeners.delete(cb);
    };
  },
};
