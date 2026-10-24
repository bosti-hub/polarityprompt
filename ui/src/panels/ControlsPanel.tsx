import { useEffect, useState } from "react";
import { listAudioDevices, listMonitors, openDocumentDialog, openPracticeScript, openWindow, placeTalentWindow, reopenLastDocument, scroll, setDocumentLanguage, setMuted, startListening, stopListening } from "../ipc";
import { resetTimer } from "../store/presentation";
import { LANGUAGES, type AudioStatus, type DeviceInfo, type EngineStatus, type Lang, type MonitorInfo, type Settings, type Snapshot } from "../ipc/types";
import { useLanguages } from "../store/languages";
import { useTranscript } from "../store/transcript";

// Speed slider is logarithmic: half the travel covers 0.25–1.2 lines/s where the precision matters.
const SPEED_MIN = 0.25;
const SPEED_MAX = 6;
const toSlider = (v: number) => Math.log(v / SPEED_MIN) / Math.log(SPEED_MAX / SPEED_MIN);
const fromSlider = (s: number) => Math.round((SPEED_MIN * Math.pow(SPEED_MAX / SPEED_MIN, s)) / 0.05) * 0.05;

interface Props {
  snapshot: Snapshot;
  update: (patch: Partial<Settings>) => void;
}

function audioLabel(a: AudioStatus): string {
  switch (a.kind) {
    case "stopped":
      return "not listening";
    case "capturing":
      return `${a.device} · ${a.sample_rate / 1000} kHz`;
    case "reconnecting":
      return "microphone lost, reconnecting…";
    case "no_signal":
      return `no signal for ${a.seconds.toFixed(0)} s`;
    case "permission_denied":
      return "microphone access denied";
    case "no_device":
      return "no input device found";
  }
}

function engineLabel(e: EngineStatus): string {
  switch (e.kind) {
    case "ready":
      return "engine ready";
    case "rebuilding":
      return "loading language model…";
    case "unknown_words":
      return "engine ready";
  }
}

export default function ControlsPanel({ snapshot, update }: Props) {
  const { settings, document, status } = snapshot;
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const heard = useTranscript();
  const { languages } = useLanguages();
  const installed = (lang: Lang) => languages.find((l) => l.lang === lang)?.installed ?? true;
  // One language picker: the open script's language, or the default for new scripts and the
  // practice script when nothing is open.
  const language = document?.language ?? settings.default_language;
  const chooseLanguage = (lang: Lang) => {
    if (document) void setDocumentLanguage(lang);
    else update({ default_language: lang });
  };
  const lastFinal = [...heard.lines].reverse().find((l) => l.kind === "final")?.text ?? "";
  const name = document ? document.title ?? document.path.split(/[\\/]/).pop() : "No script open";

  useEffect(() => {
    listAudioDevices().then(setDevices).catch(() => setDevices([]));
  }, [status.listening]);

  // Screens for the talent window picker; refreshed when it opens and after every move.
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const refreshMonitors = () => void listMonitors().then(setMonitors).catch(() => setMonitors([]));
  useEffect(refreshMonitors, [snapshot.talent_open]);
  const placeTalent = async (screen: number | null) => {
    setError(null);
    try {
      await placeTalentWindow(screen);
    } catch (e) {
      setError(String(e));
    }
    refreshMonitors();
  };
  const talentScreen = monitors.find((m) => m.talent)?.index;

  const toggleListening = async () => {
    setBusy(true);
    setError(null);
    try {
      if (status.listening) await stopListening();
      else await startListening();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const selected = settings.audio.sources[0]?.device ?? "";
  const level = Math.max(0, Math.min(1, (status.level_db + 60) / 60));
  const problem = status.audio.kind === "reconnecting" || status.audio.kind === "no_signal" || status.audio.kind === "permission_denied" || status.audio.kind === "no_device";

  return (
    <section className="panel">
      <h2>Script</h2>
      <p className="doc-name" title={document?.path}>
        {name}
      </p>
      <div className="row">
        <button onClick={() => void openDocumentDialog()}>Open script…</button>
        {!document && settings.last_document && <button onClick={() => void reopenLastDocument()}>Reopen last</button>}
        <button className={snapshot.talent_open ? "active" : ""} onClick={() => void openWindow("talent")}>
          {snapshot.talent_open ? "Close talent window" : "Talent window"}
        </button>
      </div>
      {snapshot.talent_open && (
        <div className="row">
          {monitors.length > 1 && (
            <select
              aria-label="Screen for the talent window"
              value={talentScreen ?? ""}
              onFocus={refreshMonitors}
              onChange={(e) => void placeTalent(Number(e.target.value))}
            >
              {talentScreen === undefined && <option value="">Screen…</option>}
              {monitors.map((m) => (
                <option key={m.index} value={m.index}>
                  Screen {m.index + 1} · {m.width}×{m.height}
                  {m.operator ? " (this window)" : ""}
                </option>
              ))}
            </select>
          )}
          <button onClick={() => void placeTalent(null)} title="Bring the talent window here, centred over this window">
            ⌖ Bring here
          </button>
        </div>
      )}
      <label>
        Language
        <select value={language} onChange={(e) => chooseLanguage(e.target.value as Lang)}>
          {LANGUAGES.map(({ lang, name }) => (
            <option key={lang} value={lang}>
              {name}
              {installed(lang) ? "" : " (download in Settings)"}
            </option>
          ))}
        </select>
      </label>
      <div className="row">
        <button onClick={() => void openPracticeScript(language)} title="A short text about coffee in the chosen language">
          Practice script
        </button>
      </div>
      {snapshot.last_error && <p className="error">{snapshot.last_error}</p>}

      <h2>Microphone</h2>
      <label>
        Input device
        <select
          value={selected}
          onChange={(e) => update({ audio: { sources: e.target.value ? [{ device: e.target.value }] : [] } })}
        >
          <option value="">System default</option>
          {devices.map((d) => (
            <option key={d.id} value={d.id}>
              {d.name}
              {d.is_default ? " (default)" : ""}
            </option>
          ))}
        </select>
      </label>
      <div className="row">
        <button className={status.listening ? "active" : ""} onClick={() => void toggleListening()} disabled={busy}>
          {status.listening ? "Stop listening" : "Start listening"}
        </button>
        {status.listening && (
          <button className={status.muted ? "active" : ""} onClick={() => void setMuted(!status.muted)} title="M">
            {status.muted ? "Unmute" : "Mute"}
          </button>
        )}
      </div>
      <div className="meter" aria-hidden="true">
        <div className="meter-fill" style={{ width: `${level * 100}%` }} />
      </div>
      <p className={`status${problem ? " warn" : ""}`}>
        {audioLabel(status.audio)} · {engineLabel(status.engine)}
      </p>
      {status.audio.kind === "permission_denied" && (
        <p className="hint">Allow the microphone in System Settings → Privacy &amp; Security → Microphone, then start listening again.</p>
      )}
      {status.unknown_words.length > 0 && (
        <p className="hint warn">Words the speech model does not know (pick other anchors): {status.unknown_words.join(", ")}</p>
      )}
      {error && <p className="error">{error}</p>}
      {status.listening && (
        <div className="heard">
          <div className="line line-final">{lastFinal || "…"}</div>
          {heard.partial && <div className="line line-partial">{heard.partial}</div>}
        </div>
      )}

      <h2>Scroll</h2>
      <div className="row">
        <button onClick={() => void scroll({ type: "start" })}>Start</button>
        <button onClick={() => void scroll({ type: "pause" })}>Pause</button>
        <button onClick={() => void scroll({ type: "hold" })}>Hold</button>
        <button
          onClick={() => {
            resetTimer();
            void scroll({ type: "reset" });
          }}
        >
          Restart
        </button>
      </div>
      <label>
        Speed <span className="value">{settings.base_speed.toFixed(2)} lines/s</span>
        <input
          type="range"
          min={0}
          max={1}
          step={0.005}
          value={toSlider(settings.base_speed)}
          onChange={(e) => update({ base_speed: fromSlider(Number(e.target.value)) })}
        />
      </label>
      <p className="hint">Mirroring applies to the talent window when it is open, otherwise to this preview.</p>
      <div className="row">
        <label className="check">
          <input type="checkbox" checked={settings.mirror_h} onChange={(e) => update({ mirror_h: e.target.checked })} />
          Mirror horizontal
        </label>
        <label className="check">
          <input type="checkbox" checked={settings.mirror_v} onChange={(e) => update({ mirror_v: e.target.checked })} />
          Mirror vertical
        </label>
      </div>
    </section>
  );
}
