// Installed speech languages and running model downloads (Settings → Languages,
// the language pickers). Refreshed from Rust after every finished download.

import { useSyncExternalStore } from "react";
import { listLanguages, onDownload } from "../ipc";
import type { DownloadDto, Lang, LanguageInfo } from "../ipc/types";

export interface LanguagesState {
  languages: LanguageInfo[];
  /** Latest progress per language while a download runs or just ended. */
  progress: Partial<Record<Lang, DownloadDto>>;
}

let state: LanguagesState = { languages: [], progress: {} };
const listeners = new Set<() => void>();
let started = false;

function notify() {
  for (const l of listeners) l();
}

export function refreshLanguages() {
  void listLanguages()
    .then((languages) => {
      state = { ...state, languages };
      notify();
    })
    .catch(() => {});
}

export function startLanguagesStore() {
  if (started) return;
  started = true;
  refreshLanguages();
  onDownload((d) => {
    state = { ...state, progress: { ...state.progress, [d.lang]: d } };
    notify();
    if (d.state === "done" || d.state === "failed" || d.state === "cancelled") refreshLanguages();
  });
}

export function useLanguages(): LanguagesState {
  return useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    () => state,
  );
}
