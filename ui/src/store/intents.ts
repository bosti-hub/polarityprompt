// Intent bus. Phase 0: the dev panel publishes ScrollIntents here; the document
// view's animator consumes them. Phase 1 adds a second publisher: the
// `prompter://intent` event from Rust. Consumers never see who sent what.

import type { ScrollIntent } from "../ipc/types";

type Listener = (intent: ScrollIntent) => void;
const listeners = new Set<Listener>();
let seq = 0;

export function nextSeq(): number {
  seq += 1;
  return seq;
}

export function publishIntent(intent: ScrollIntent) {
  for (const l of listeners) l(intent);
}

export function subscribeIntents(l: Listener): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}
