// Presentation-time state that is not part of the Rust snapshot: the countdown
// (fed by prompter://countdown) and the reading timer (advanced by the view's
// once-a-second stats while the text is in motion or held).

import { useSyncExternalStore } from "react";
import { onCountdown } from "../ipc";

interface State {
  countdown: number;
  /** Seconds of reading time in this take. */
  elapsed: number;
}

let state: State = { countdown: 0, elapsed: 0 };
const listeners = new Set<() => void>();
let started = false;

function set(next: Partial<State>) {
  state = { ...state, ...next };
  for (const l of listeners) l();
}

export function startPresentationStore() {
  if (started) return;
  started = true;
  onCountdown((c) => set({ countdown: c.seconds_left }));
}

export function tickTimer(running: boolean) {
  if (running) set({ elapsed: state.elapsed + 1 });
}

export function resetTimer() {
  set({ elapsed: 0 });
}

export function usePresentation(): State {
  return useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    () => state,
  );
}
