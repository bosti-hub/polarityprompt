// The one place the UI talks to Rust. In the Tauri shell this calls real commands
// and listens to real events; in a plain browser (`npm run dev` opened directly)
// it uses ./mock so the document view and animator can be tuned without Rust.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { mock } from "./mock";
import type { CountdownDto, DeviceInfo, DocPos, DownloadDto, EngineInfo, Lang, LanguageInfo, MatchDto, MonitorInfo, ScrollIntent, Settings, Snapshot, StatusDto, TranscriptDto, UserAction, WindowRole } from "./types";

export function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export function windowRole(): WindowRole {
  const role = new URLSearchParams(window.location.search).get("role");
  return role === "talent" ? "talent" : "operator";
}

// ---- commands

export async function getSnapshot(): Promise<Snapshot> {
  return isTauri() ? invoke<Snapshot>("get_snapshot") : mock.getSnapshot();
}

export async function setSettings(settings: Settings): Promise<Snapshot> {
  return isTauri() ? invoke<Snapshot>("set_settings", { settings }) : mock.setSettings(settings);
}

/** Native file picker; resolves to null when cancelled. */
export async function openDocumentDialog(): Promise<Snapshot | null> {
  return isTauri() ? invoke<Snapshot | null>("open_document_dialog") : mock.openDocumentDialog();
}

/** Reopens the previous session's script; null when there is none or it is gone. */
export async function reopenLastDocument(): Promise<Snapshot | null> {
  return isTauri() ? invoke<Snapshot | null>("reopen_last_document") : mock.reopenLastDocument();
}

/** Opens the built-in practice script in the given language. */
export async function openPracticeScript(lang: Lang): Promise<Snapshot> {
  return isTauri() ? invoke<Snapshot>("open_practice_script", { lang }) : mock.openPracticeScript(lang);
}

export async function engineInfo(): Promise<EngineInfo> {
  return isTauri() ? invoke<EngineInfo>("engine_info") : mock.engineInfo();
}

export async function setDocumentLanguage(lang: Lang): Promise<Snapshot> {
  return isTauri() ? invoke<Snapshot>("set_document_language", { lang }) : mock.setDocumentLanguage(lang);
}

export async function listFonts(): Promise<string[]> {
  return isTauri() ? invoke<string[]>("list_fonts") : mock.listFonts();
}

export async function listAudioDevices(): Promise<DeviceInfo[]> {
  return isTauri() ? invoke<DeviceInfo[]>("list_audio_devices") : mock.listAudioDevices();
}

export async function startListening(): Promise<Snapshot> {
  return isTauri() ? invoke<Snapshot>("start_listening") : mock.startListening();
}

export async function stopListening(): Promise<Snapshot> {
  return isTauri() ? invoke<Snapshot>("stop_listening") : mock.stopListening();
}

export async function setMuted(muted: boolean): Promise<Snapshot> {
  return isTauri() ? invoke<Snapshot>("set_muted", { muted }) : mock.setMuted(muted);
}

export async function scroll(action: UserAction): Promise<void> {
  return isTauri() ? invoke("scroll", { action }) : mock.scroll(action);
}

export async function animatorArrived(seq: number): Promise<void> {
  return isTauri() ? invoke("animator_arrived", { seq }) : mock.animatorArrived();
}

export async function readLinePos(pos: DocPos): Promise<void> {
  return isTauri() ? invoke("read_line_pos", { pos }) : mock.readLinePos(pos);
}

export async function readLineBlock(block: number): Promise<void> {
  return isTauri() ? invoke("read_line_block", { block }) : mock.readLineBlock();
}

export async function openWindow(role: WindowRole): Promise<void> {
  return isTauri() ? invoke("open_window", { role }) : mock.openWindow();
}

export async function listLanguages(): Promise<LanguageInfo[]> {
  return isTauri() ? invoke<LanguageInfo[]>("list_languages") : mock.listLanguages();
}

/** Starts downloading a speech model; progress arrives via onDownload. */
export async function downloadLanguage(lang: Lang): Promise<void> {
  return isTauri() ? invoke("download_language", { lang }) : mock.downloadLanguage(lang);
}

export async function cancelDownload(lang: Lang): Promise<void> {
  return isTauri() ? invoke("cancel_download", { lang }) : mock.cancelDownload(lang);
}

export async function removeLanguage(lang: Lang): Promise<void> {
  return isTauri() ? invoke("remove_language", { lang }) : mock.removeLanguage(lang);
}

export async function listMonitors(): Promise<MonitorInfo[]> {
  return isTauri() ? invoke<MonitorInfo[]>("list_monitors") : mock.listMonitors();
}

/** Covers screen `screen` (picker index) with the talent window; null centres it over this window. */
export async function placeTalentWindow(screen: number | null): Promise<void> {
  return isTauri() ? invoke("place_talent_window", { screen }) : mock.placeTalentWindow();
}

// ---- events

function subscribe<T>(event: string, cb: (payload: T) => void): () => void {
  let unlisten: (() => void) | null = null;
  let cancelled = false;
  listen<T>(event, (ev) => cb(ev.payload)).then((un) => {
    if (cancelled) un();
    else unlisten = un;
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

export function onSnapshot(cb: (s: Snapshot) => void): () => void {
  return isTauri() ? subscribe("prompter://snapshot", cb) : mock.onSnapshot(cb);
}

export function onIntent(cb: (i: ScrollIntent) => void): () => void {
  return isTauri() ? subscribe("prompter://intent", cb) : mock.onIntent(cb);
}

export function onStatus(cb: (s: StatusDto) => void): () => void {
  return isTauri() ? subscribe("prompter://status", cb) : mock.onStatus(cb);
}

export function onTranscript(cb: (t: TranscriptDto) => void): () => void {
  return isTauri() ? subscribe("prompter://transcript", cb) : mock.onTranscript(cb);
}

export function onCountdown(cb: (c: CountdownDto) => void): () => void {
  return isTauri() ? subscribe("prompter://countdown", cb) : mock.onCountdown(cb);
}

export function onDownload(cb: (d: DownloadDto) => void): () => void {
  return isTauri() ? subscribe("prompter://download", cb) : mock.onDownload(cb);
}

export function onMatch(cb: (m: MatchDto) => void): () => void {
  return isTauri() ? subscribe("prompter://match", cb) : mock.onMatch(cb);
}
