# PolarityPrompt

Cross-platform (macOS first, Windows second), 100% local/offline teleprompter with
voice-locked scrolling. Reads Markdown scripts, renders them large and mirrorable,
and scrolls to keyword anchors the speaker says out loud.

Owner: Floris (solo, non-programmer, works with Claude Code). Explain things in plain
words when asked; never assume git/Rust/npm knowledge on his side.

## Read first

- `docs/ARCHITECTURE.md` — layers, Rust types, state machine, IPC contract. Authoritative.
- `docs/ROADMAP.md` — phases with acceptance criteria. Work in phase order.
- `docs/DECISIONS.md` — why things are the way they are. Add an entry when you change a decision.
- `docs/SCRIPT-FORMAT.md` — Markdown conventions for scripts (anchors, talking points).
- `docs/DEVLOG.md` — plain-language session log for Floris. Add an entry at the end of every session.
- `docs/PUBLISHING.md` — checklist for the public release: license, history, signing, donations.
- Both are **private notes**: git-ignored in the public repository (kept only on Floris's
  machines and in the private archive repository). Never commit or quote them publicly.

## Stack (decided — do not re-litigate without a DECISIONS entry)

- Tauri 2 shell, Rust workspace for the core, React + TypeScript + Vite for the UI.
- Streaming STT: Vosk (small nl + small en-us models) behind a `SpeechEngine` trait.
- Batch STT (post-take): whisper.cpp behind a `Transcriber` trait (phase 1.5).
- Audio capture: `cpal`; audio thread → lock-free ring buffer (`rtrb`) → speech worker.
- Markdown parsing in Rust (`pulldown-cmark`); the UI renders the block model, never raw Markdown.

## Layout

```
crates/prompter-core/    document model, anchors, ScrollController, PositionEstimator trait, take records
crates/prompter-speech/  AudioSource + supervisor, SpeechEngine (Vosk), Transcriber (Whisper), transcript bus
apps/desktop/            Tauri host: windows, menus, file dialogs, IPC commands. Plumbing only.
ui/                      React app: renders state, animates, sends commands. Owns pixels, nothing else.
docs/                    architecture, roadmap, decisions, script format
scripts/                 backup.sh and dev helpers
models/                  Vosk models + libvosk (bundled as Tauri resources; not committed)
```

Dependency arrow (strict): `prompter-core` depends on nothing internal. `prompter-speech`
depends only on core types. `apps/desktop` depends on both. `ui/` knows only the IPC contract.

## Hard rules

1. The UI never mutates its own copy of the model. Rust state is the single source of truth;
   the UI renders `ScrollIntent`/snapshots and sends commands.
2. No per-frame IPC. The controller emits intents; the UI animator integrates motion locally.
3. The audio callback only pushes samples into the ring buffer. It never touches the recognizer,
   never allocates, never locks.
4. `ScrollIntent::Snap` is only ever produced by an explicit user action, never by speech.
5. The transcript is a broadcast bus with a `speaker` field. Any new consumer subscribes; it does
   not tap the matcher.
6. Every engine/source/estimator is a trait with at least one real implementation and one mock.
7. Keep the coaching-call seams (`AudioSource` list, `speaker` tag, `PositionEstimator`,
   `Advisor` placeholder, panel-based operator window) intact. Do not build features for them yet.

## Working agreements

- Work in small steps: implement → `cargo test` + `npm run typecheck` + `npm run lint` → commit.
- Git is operated by you. Commit after every green step with a plain-language message. Push only
  when Floris asks. Never rewrite history, never force anything, never create branches unless asked.
- Voice recordings are personal data: they live in `fixtures/` (git-ignored) and are never committed.
- Backup: `scripts/backup.sh` (macOS) or `scripts/backup.ps1` (Windows) copies the source tree
  (excluding `target/`, `node_modules/`, `models/`) to the OneDrive folder. Remind Floris to run
  it at the end of a session.
- Ask before adding a dependency. Prefer boring, well-maintained crates/packages.
- Code, comments, commit messages: English. UI strings: English for V1 (i18n later).
- When something in `docs/` turns out wrong, fix the doc in the same change and add a DECISIONS entry.
- Never write into the OneDrive folder except via `scripts/backup.sh` / `scripts/backup.ps1`.

## Platform notes

- macOS: `NSMicrophoneUsageDescription` in Info.plist is mandatory (missing = hard kill on first
  stream open). Entitlement `com.apple.security.device.audio-input` for hardened runtime.
- Windows: WebView2 is preinstalled on Win 10/11. WASAPI shared mode lets OBS and this app read the
  same USB mic simultaneously (same on CoreAudio).
- Windows: synchronous Tauri commands run on the main thread. A command that creates a window must
  be `async` (otherwise it deadlocks on WebView2), and anything slow (model loading) belongs in an
  async command / `spawn_blocking`, or every window freezes. Monitor geometry: use physical pixels;
  monitors can have different scale factors.
- Vosk needs the prebuilt `libvosk` dynamic library (macOS arm64 / win64) shipped as a resource.
  Only the *small* models support runtime grammar; that is what we want.
