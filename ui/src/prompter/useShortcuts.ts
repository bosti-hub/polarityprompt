// Keyboard shortcuts for the operator window. One place, always active.

import { useEffect } from "react";
import { scroll, setMuted } from "../ipc";
import type { Settings, Snapshot } from "../ipc/types";
import { resetTimer } from "../store/presentation";
import { NUDGE_EVENT, type ViewStats } from "./DocumentView";

export const SPEED_PRESETS = [0.5, 0.75, 1, 1.25, 1.5, 2, 2.5, 3, 4];

export const SHORTCUTS: { keys: string; what: string }[] = [
  { keys: "Space", what: "Pause / resume" },
  { keys: "C", what: "Start scrolling (with countdown)" },
  { keys: "H", what: "Hold" },
  { keys: "R", what: "Restart from the top, reset timer" },
  { keys: "↑ ↓", what: "One line up / down" },
  { keys: ". ,", what: "Next / previous bookmark" },
  { keys: "+ −", what: "Faster / slower" },
  { keys: "1 – 9", what: "Speed presets (0.5 … 4 lines/s)" },
  { keys: "[ ]", what: "Narrower / wider text" },
  { keys: "M", what: "Mute microphone" },
  { keys: "F", what: "Frame-time overlay (developer)" },
];

interface Args {
  snapshot: Snapshot;
  update: (patch: Partial<Settings>) => void;
  stats: ViewStats | null;
  toggleOverlay: () => void;
}

export function useShortcuts({ snapshot, update, stats, toggleOverlay }: Args) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "SELECT" || t.tagName === "TEXTAREA")) return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const s = snapshot.settings;
      const doc = snapshot.document;
      const bookmarks = doc?.blocks.filter((b) => b.bookmark !== undefined).map((b) => b.id) ?? [];
      const current = stats?.block ?? 0;
      const nudge = (lines: number) => window.dispatchEvent(new CustomEvent(NUDGE_EVENT, { detail: lines }));
      const width = 1 - s.margin_left - s.margin_right;
      const setWidth = (w: number) => {
        const ww = Math.min(1, Math.max(0.2, w));
        const shift = s.margin_left - s.margin_right;
        update({ margin_left: Math.max(0, (1 - ww) / 2 + shift / 2), margin_right: Math.max(0, (1 - ww) / 2 - shift / 2) });
      };
      let handled = true;
      switch (e.key) {
        case " ":
          void scroll({ type: "pause" });
          break;
        case "c":
        case "C":
          void scroll({ type: "start" });
          break;
        case "h":
        case "H":
          void scroll({ type: "hold" });
          break;
        case "r":
        case "R":
          resetTimer();
          void scroll({ type: "reset" });
          break;
        case "ArrowUp":
          nudge(-1);
          break;
        case "ArrowDown":
          nudge(1);
          break;
        case ".": {
          const next = bookmarks.find((b) => b > current);
          if (next !== undefined) void scroll({ type: "jump_hold", pos: { block: next, frac: 0 } });
          break;
        }
        case ",": {
          const prev = [...bookmarks].reverse().find((b) => b < current);
          if (prev !== undefined) void scroll({ type: "jump_hold", pos: { block: prev, frac: 0 } });
          break;
        }
        case "+":
        case "=":
          update({ base_speed: Math.min(6, Math.round(s.base_speed * 1.15 * 20) / 20) });
          break;
        case "-":
        case "_":
          update({ base_speed: Math.max(0.25, Math.round((s.base_speed / 1.15) * 20) / 20) });
          break;
        case "[":
          setWidth(width - 0.05);
          break;
        case "]":
          setWidth(width + 0.05);
          break;
        case "m":
        case "M":
          void setMuted(!snapshot.status.muted);
          break;
        case "f":
        case "F":
          toggleOverlay();
          break;
        default:
          if (/^[1-9]$/.test(e.key)) {
            update({ base_speed: SPEED_PRESETS[Number(e.key) - 1] ?? s.base_speed });
          } else {
            handled = false;
          }
      }
      if (handled) e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [snapshot, update, stats, toggleOverlay]);
}
