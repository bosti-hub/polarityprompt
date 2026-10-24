import { useEffect, useState } from "react";
import { listFonts } from "../ipc";
import type { Settings } from "../ipc/types";
import { availableFonts } from "../prompter/fonts";
import { SHORTCUTS } from "../prompter/useShortcuts";
import LanguagesSection from "./LanguagesSection";

interface Props {
  settings: Settings;
  update: (patch: Partial<Settings>) => void;
}

function Slider({
  label,
  value,
  min,
  max,
  step,
  format,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (v: number) => string;
  onChange: (v: number) => void;
}) {
  return (
    <label>
      {label} <span className="value">{format(value)}</span>
      <input type="range" min={min} max={max} step={step} value={value} onChange={(e) => onChange(Number(e.target.value))} />
    </label>
  );
}

export default function SettingsPanel({ settings, update }: Props) {
  const [fonts, setFonts] = useState<string[]>([]);
  // Width/position are a friendlier view of the two margins.
  const width = Math.max(0.2, 1 - settings.margin_left - settings.margin_right);
  const free = 1 - width;
  const position = free > 0.001 ? (settings.margin_left - settings.margin_right) / free : 0;
  const setLayout = (w: number, p: number) => {
    const f = 1 - w;
    update({ margin_left: Math.max(0, (f * (1 + p)) / 2), margin_right: Math.max(0, (f * (1 - p)) / 2) });
  };

  useEffect(() => {
    listFonts().then((list) => setFonts(availableFonts(list)));
  }, []);

  const fontOptions = fonts.includes(settings.font_family) ? fonts : [settings.font_family, ...fonts];

  return (
    <section className="panel">
      <h2>Typography</h2>
      <label>
        Font
        <select value={settings.font_family} onChange={(e) => update({ font_family: e.target.value })}>
          {fontOptions.map((f) => (
            <option key={f} value={f}>
              {f}
            </option>
          ))}
        </select>
      </label>
      <label>
        Custom font name
        <input
          type="text"
          defaultValue={settings.font_family}
          onBlur={(e) => e.target.value.trim() && update({ font_family: e.target.value.trim() })}
          onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
        />
      </label>
      <Slider label="Size" value={settings.font_size} min={24} max={160} step={2} format={(v) => `${v}px`} onChange={(v) => update({ font_size: v })} />
      <Slider label="Line height" value={settings.line_height} min={1} max={2.2} step={0.05} format={(v) => v.toFixed(2)} onChange={(v) => update({ line_height: v })} />

      <h2>Layout</h2>
      <Slider label="Text width" value={width} min={0.2} max={1} step={0.01} format={(v) => `${Math.round(v * 100)}%`} onChange={(v) => setLayout(v, position)} />
      <Slider label="Text position" value={position} min={-1} max={1} step={0.02} format={(v) => (Math.abs(v) < 0.01 ? "centre" : v < 0 ? `${Math.round(-v * 100)}% left` : `${Math.round(v * 100)}% right`)} onChange={(v) => setLayout(width, v)} />
      <label>
        Alignment
        <select value={settings.align} onChange={(e) => update({ align: e.target.value as Settings["align"] })}>
          <option value="left">Left</option>
          <option value="center">Centred</option>
        </select>
      </label>
      <Slider label="Read line" value={settings.read_line} min={0.05} max={0.95} step={0.01} format={(v) => `${Math.round(v * 100)}% from top`} onChange={(v) => update({ read_line: v })} />
      <Slider label="Dim text above the read line" value={settings.dim_above} min={0} max={1} step={0.05} format={(v) => (v === 0 ? "off" : `${Math.round(v * 100)}%`)} onChange={(v) => update({ dim_above: v })} />

      <h2>Colors</h2>
      <div className="row">
        <label className="color">
          Background
          <input type="color" value={settings.color_bg} onChange={(e) => update({ color_bg: e.target.value })} />
        </label>
        <label className="color">
          Text
          <input type="color" value={settings.color_fg} onChange={(e) => update({ color_fg: e.target.value })} />
        </label>
      </div>

      <h2>Operator window</h2>
      <label className="check">
        <input type="checkbox" checked={settings.sidebar_right} onChange={(e) => update({ sidebar_right: e.target.checked })} />
        Panels on the right
      </label>

      <h2>Behaviour</h2>
      <label>
        Countdown before scrolling starts
        <select value={settings.countdown_seconds} onChange={(e) => update({ countdown_seconds: Number(e.target.value) })}>
          <option value={0}>None</option>
          <option value={3}>3 seconds</option>
          <option value={5}>5 seconds</option>
          <option value={10}>10 seconds</option>
        </select>
      </label>
      <label>
        Stop listening after silence
        <select value={settings.auto_stop_minutes} onChange={(e) => update({ auto_stop_minutes: Number(e.target.value) })}>
          <option value={0}>Never</option>
          <option value={1}>1 minute</option>
          <option value={2}>2 minutes</option>
          <option value={5}>5 minutes</option>
          <option value={10}>10 minutes</option>
        </select>
      </label>
      <label>
        After a glide
        <select value={settings.after_glide} onChange={(e) => update({ after_glide: e.target.value as Settings["after_glide"] })}>
          <option value="cruise">Keep cruising</option>
          <option value="hold">Hold until next anchor</option>
        </select>
      </label>

      <LanguagesSection />

      <h2>Keyboard shortcuts</h2>
      <table className="shortcuts">
        <tbody>
          {SHORTCUTS.map((sc) => (
            <tr key={sc.keys}>
              <td>
                <kbd>{sc.keys}</kbd>
              </td>
              <td>{sc.what}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
