# Roadmap

Work strictly in phase order. A phase is done when every acceptance line holds and the work is
committed. Do not start the next phase's features early "because it's easy".

## Phase 0 — Reading experience, no speech (browser-first)

Status: **done** (accepted by Floris on 2026-09-16; font list is a curated set, see DECISIONS D11).

Goal: nail typography, margins, mirroring and scroll feel before any Rust logic exists.

Scope:
- Cargo workspace + Tauri 2 scaffold + Vite/React UI. `npm run dev` runs the UI in a plain browser
  with mocked IPC; `cargo tauri dev` runs it in the shell.
- `prompter-core::document`: Markdown → `Document` (blocks, kinds, html, plain, word_count).
  Anchors derived but unused.
- UI document view with settings: font family (system font list via `list_fonts`, mocked in
  browser), font size, line height, margins L/R, mirror H/V, read-line position, colors.
- Animator with spring + cruise + hold, driven by a dev panel firing `ScrollIntent`s.
- Settings persisted.

Accept when:
- Opens a `.md` via file dialog; headings, bold, blank lines and lists render clearly at 40–60 pt.
- Mirror H toggles instantly, text stays sharp.
- Cruise at 1–4 lines/sec is visually smooth on the MacBook screen; `GlideTo` retargets mid-flight
  without a visible jump. Frame-time overlay shows no per-frame layout.
- Settings survive restart.

## Phase 1 — Voice-locked scroll (Vosk + anchors)

Status: **built, awaiting acceptance** (2026-09-16). Core matcher/controller/session, speech crate,
host wiring and UI are in place with tests; the `replay` tool recognizes a synthesized Dutch
fragment. Not yet verified by a person: live microphone reading, mic unplug/replug, two windows in
sync. Models come from `scripts/fetch-models.sh`; bundling them as resources is Phase 2 packaging.

Scope:
- `prompter-speech`: `AudioSupervisor` (cpal), ring buffer, resampler, `SpeechWorker`, `VoskEngine`,
  `MockEngine`, `TranscriptBus`. Device picker in the UI (single source), remembered by name.
- Models bundled as resources (small nl, small en-us) + libvosk; language from front-matter or default.
- `AnchorMatcher`, `PaceModel`, `ScrollController` in core with tests.
- Operator window: transcript debug panel, anchor highlights on blocks, unknown-word warnings.
- Talent window (`open_window("talent")`) with mirror on, positioned on a chosen monitor.
- macOS Info.plist + entitlements; mic permission flow (denied → button to System Settings).

Accept when:
- Reading a 5-minute Dutch script aloud at natural pace, the read line stays within one block of
  the spoken position ≥ 90% of the time; no jumps that feel like a snap. Offline check:
  `replay --script` on the recordings in `fixtures/` (git-ignored, see CONTRIBUTING) reports
  every block in order and 0 backwards.
- Skipping a paragraph triggers a `Skip` glide; improvising for 20 s stops the scroll and it
  resumes on the next anchor.
- Same script in English with `lang: en` behaves the same.
- Unplugging the USB mic mid-take shows `Reconnecting`, then recovers on replug without restart.
- Operator + talent windows stay in sync (visual check) and a late-opened talent window starts at
  the right position.
- All core tests green; `replay` binary runs a recorded WAV headless.

## Phase 1.5 — Take journal

Scope:
- `start_take`/`stop_take`: WAV at device rate, `take.json`, anchor log.
- `WhisperTranscriber` (whisper-rs, Metal/CUDA), `large-v3-turbo`, run after the take.
- `Aligner`: transcript → per-block `BlockVerdict` + spoken text.
- Take panel: list takes per script; per take a two-column view (script block / spoken); mark a
  passage → appended to `<script>.suggestions.md` next to the script.

Accept when:
- A 3-minute take transcribes in well under its own duration on the M5.
- Rephrased and added passages are flagged correctly on a test script with known deviations.
- Suggestions file is written and never overwrites the script itself.

## Phase 2 — Smarter tracking and integrations (not scheduled)

- `SemanticAligner` (local multilingual embeddings via ONNX) as second `PositionEstimator`.
- OBS integration (obs-websocket v5): start/stop recording, fill `ExternalRef`.
- Windows packaging + CI builds for both platforms.
- Optional: Rode Wireless Go II as second source (`SourceMode::Mix`).

## Phase 3 — Coaching-call mode (design seams only, no work now)

- Second `AudioSource` (call audio via ScreenCaptureKit / WASAPI loopback), `Speaker::Other`.
- Notes panel, live transcript panel, `Advisor` panel (local llama.cpp or API).
