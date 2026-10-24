// Single store fed by events from Rust. Components read it with useSnapshot();
// nothing in the UI writes to it directly (CLAUDE.md hard rule 1), except the
// optimistic settings echo in OperatorWindow which Rust immediately confirms.

import { useSyncExternalStore } from "react";
import { getSnapshot, onIntent, onMatch, onSnapshot, onStatus } from "../ipc";
import type { Snapshot } from "../ipc/types";
import { publishIntent } from "./intents";
import { pushMatch } from "./transcript";

let current: Snapshot | null = null;
const listeners = new Set<() => void>();
let started = false;

function notify() {
  for (const l of listeners) l();
}

export function startSnapshotStore() {
  if (started) return;
  started = true;
  onSnapshot((s) => {
    current = s;
    notify();
  });
  onStatus((status) => {
    if (!current) return;
    current = { ...current, status };
    notify();
  });
  onMatch((m) => {
    pushMatch(m);
    if (!current) return;
    current = { ...current, current_block: m.block };
    notify();
  });
  onIntent((intent) => {
    publishIntent(intent);
    if (!current) return;
    current = { ...current, last_intent: intent };
    notify();
  });
  getSnapshot().then((s) => {
    current = s;
    notify();
  });
}

export function applySnapshot(s: Snapshot) {
  current = s;
  notify();
}

export function useSnapshot(): Snapshot | null {
  return useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    () => current,
  );
}
