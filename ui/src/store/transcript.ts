// Rolling transcript for the operator panel. A bus consumer like any other.

import { useSyncExternalStore } from "react";
import { onTranscript } from "../ipc";
import type { MatchDto, TranscriptDto } from "../ipc/types";

export interface TranscriptLine {
  id: number;
  kind: "final" | "silence" | "match" | "status";
  text: string;
  t_ms: number;
}

export interface TranscriptState {
  partial: string;
  lines: TranscriptLine[];
}

const MAX_LINES = 60;
let state: TranscriptState = { partial: "", lines: [] };
let nextId = 1;
const listeners = new Set<() => void>();
let started = false;

function push(line: Omit<TranscriptLine, "id">) {
  const lines = [...state.lines, { ...line, id: nextId++ }].slice(-MAX_LINES);
  state = { ...state, lines };
  notify();
}

function notify() {
  for (const l of listeners) l();
}

export function pushMatch(m: MatchDto) {
  const kind =
    m.kind.kind === "advance" ? (m.kind.skipped > 0 ? `skip ${m.kind.skipped}` : "advance") : m.kind.kind === "retake" ? "retake" : `relocate ${m.kind.distance}`;
  push({ kind: "match", text: `→ block ${m.block} (${kind}, ${(m.confidence * 100).toFixed(0)}%)`, t_ms: m.t_ms });
}

export function startTranscriptStore() {
  if (started) return;
  started = true;
  onTranscript((t: TranscriptDto) => {
    switch (t.kind) {
      case "partial":
        state = { ...state, partial: t.text };
        notify();
        break;
      case "final":
        state = { ...state, partial: "" };
        push({ kind: "final", text: t.text, t_ms: t.t_ms });
        break;
      case "silence":
        // Only the first silence after speech is interesting; later ones are noise.
        if (state.lines[state.lines.length - 1]?.kind !== "silence") {
          push({ kind: "silence", text: `… silence`, t_ms: 0 });
        }
        break;
      case "status":
        if (t.kind === "status" && (t as { kind: string; words?: string[] }).words) {
          const words = (t as unknown as { words: string[] }).words;
          if (words.length) push({ kind: "status", text: `unknown to the model: ${words.join(", ")}`, t_ms: 0 });
        }
        break;
    }
  });
}

export function useTranscript(): TranscriptState {
  return useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    () => state,
  );
}
