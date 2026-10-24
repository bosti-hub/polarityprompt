// Advanced: speech engine info, alignment thresholds (with defaults and reset),
// default language, and developer tools for tuning the animator without speech.

import { useEffect, useState } from "react";
import { engineInfo } from "../ipc";
import { DEFAULT_TUNING, LANGUAGES, type Document, type EngineInfo, type EstimatorTuning, type GlideProfile, type Settings } from "../ipc/types";
import type { ViewStats } from "../prompter/DocumentView";
import { nextSeq, publishIntent } from "../store/intents";

interface Props {
  document: Document | null;
  settings: Settings;
  update: (patch: Partial<Settings>) => void;
  stats: ViewStats | null;
  showOverlay: boolean;
  setShowOverlay: (v: boolean) => void;
  pixelSnap: boolean;
  setPixelSnap: (v: boolean) => void;
}

interface Knob {
  key: keyof EstimatorTuning;
  label: string;
  help: string;
  min: number;
  max: number;
  step: number;
  unit?: string;
}

const KNOBS: Knob[] = [
  { key: "min_score", label: "Evidence to move", help: "How much matched text is needed before the prompter moves. Lower = reacts sooner but may follow noise.", min: 0.8, max: 4, step: 0.1 },
  { key: "jump_words", label: "Free forward range", help: "Moving forward up to this many words needs no extra evidence.", min: 10, max: 200, step: 5, unit: "words" },
  { key: "jump_penalty", label: "Extra for long jumps", help: "Added evidence needed to jump further ahead than the free range.", min: 0, max: 4, step: 0.1 },
  { key: "back_penalty", label: "Extra to go back", help: "Added evidence needed to move backwards (a retake).", min: 0, max: 4, step: 0.1 },
  { key: "far_penalty", label: "Extra for far moves", help: "Added evidence to relocate anywhere else in the script.", min: 0, max: 5, step: 0.1 },
  { key: "min_interval_ms", label: "Update interval", help: "Shortest time between two position updates.", min: 100, max: 1000, step: 50, unit: "ms" },
];

export default function AdvancedPanel({ document, settings, update, stats, showOverlay, setShowOverlay, pixelSnap, setPixelSnap }: Props) {
  const [engine, setEngine] = useState<EngineInfo | null>(null);
  const [target, setTarget] = useState(0);
  const blockCount = document?.blocks.length ?? 0;
  const current = stats?.block ?? 0;
  const tuning = settings.tuning;
  const isDefault = KNOBS.every((k) => tuning[k.key] === DEFAULT_TUNING[k.key]);

  useEffect(() => {
    engineInfo().then(setEngine).catch(() => setEngine(null));
  }, []);

  const setKnob = (key: keyof EstimatorTuning, value: number) => update({ tuning: { ...tuning, [key]: value } });

  const glide = (block: number, profile: GlideProfile) => {
    const b = Math.max(0, Math.min(blockCount - 1, block));
    publishIntent({ type: "glide_to", pos: { block: b, frac: 0 }, profile, then: settings.after_glide, seq: nextSeq() });
  };

  return (
    <section className="panel">
      <h2>Speech engine</h2>
      <p className="hint">
        {engine ? engine.name : "…"}
        <br />
        Models: {engine?.languages.map((l) => LANGUAGES.find((x) => x.lang === l)?.name ?? l).join(", ") || "none found"}
        <br />
        <span title={engine?.models_dir}>Folder: {engine?.models_dir}</span>
      </p>
      <label>
        Default language for scripts without <code>lang:</code>
        <select value={settings.default_language} onChange={(e) => update({ default_language: e.target.value as Settings["default_language"] })}>
          {LANGUAGES.map(({ lang, name }) => (
            <option key={lang} value={lang}>
              {name}
            </option>
          ))}
        </select>
      </label>

      <h2>
        Text alignment{" "}
        {!isDefault && (
          <button className="link" onClick={() => update({ tuning: { ...DEFAULT_TUNING } })}>
            Reset to defaults
          </button>
        )}
      </h2>
      <p className="hint">How the spoken words are matched to the script. Defaults are shown in brackets and work for the test recordings.</p>
      {KNOBS.map((k) => (
        <label key={k.key} title={k.help}>
          {k.label}{" "}
          <span className="value">
            {tuning[k.key]}
            {k.unit ? ` ${k.unit}` : ""} <span className="default">[{DEFAULT_TUNING[k.key]}]</span>
          </span>
          <input type="range" min={k.min} max={k.max} step={k.step} value={tuning[k.key]} onChange={(e) => setKnob(k.key, Number(e.target.value))} />
        </label>
      ))}

      <h2>Developer tools</h2>
      <p className="hint">
        For tuning the scroll motion without speech. The glide buttons move the text as if the speech engine had found that block; they bypass the
        Rust logic.
      </p>
      <div className="row">
        <label className="check">
          <input type="checkbox" checked={showOverlay} onChange={(e) => setShowOverlay(e.target.checked)} />
          Frame-time overlay
        </label>
        <label className="check">
          <input type="checkbox" checked={pixelSnap} onChange={(e) => setPixelSnap(e.target.checked)} />
          Pixel snap
        </label>
      </div>
      <p className="hint">Overlay: frames per second and layout passes; there must be no layout pass per frame. Pixel snap rounds the text position to whole pixels (sharper, less smooth).</p>
      <label>
        Test glide to block <span className="value">{target}</span> of {blockCount}
        <input type="range" min={0} max={Math.max(0, blockCount - 1)} step={1} value={target} onChange={(e) => setTarget(Number(e.target.value))} />
      </label>
      <div className="row">
        <button onClick={() => glide(target, "near")}>Near</button>
        <button onClick={() => glide(target, "skip")}>Skip</button>
        <button onClick={() => glide(target, "far")}>Far</button>
        <button onClick={() => glide(current + 1, "near")}>Next</button>
      </div>
    </section>
  );
}
