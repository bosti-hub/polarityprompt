# Decisions

Short ADR-style log. Newest at the bottom. When you reverse one, add a new entry that references it.

## D1 — Tauri 2 + Rust core + React/TS UI (2026-09-16)
Compared Tauri, Electron, Flutter. Tauri wins on a natural engine layer (Concat pattern), mature
Rust audio/STT crates (no node-gyp pain on Windows), small binary. Accepted risk: WKWebView vs
WebView2 rendering differences, mitigated by owning the scroll animation ourselves.

## D2 — Streaming STT via Vosk for V1, Whisper for post-take (2026-09-16)
Whisper is batch-oriented (1–3 s latency) and unsuitable for live anchor detection. Vosk is
streaming, has small nl and en models, and supports runtime grammar (small models only).
sherpa-onnx rejected for now: no strong Dutch streaming model. Whisper is used where latency
doesn't matter and accuracy does: the take journal.

## D3 — Anchors are keyword sets with a threshold, not phrases (2026-09-16)
Exact phrases break on rephrasing. Content words (names, terms, numbers) survive rephrasing.
Grammar = keywords + `[unk]`; `[unk]` absorbs improvisation so it produces no hits.

## D4 — Bullet lists are talking points (ad-lib zones) (2026-09-16)
A list block is not read verbatim; the prompter holds there and resumes at the next block's anchor.

## D5 — Controller decides intent, UI executes motion (2026-09-16)
`ScrollController` (Rust) emits `ScrollIntent`s in document coordinates. The UI animator
(critically damped spring) owns pixels and runs at display rate. No per-frame IPC; N windows
integrate the same intent independently and resync on every intent.

## D6 — Between anchors: Cruise with silence→Hold, user-switchable to Hold (2026-09-16)
Cruise keeps the classic prompter feel; silence (2.5 s) and pace overrun stop the scroll so
improvising is safe. Hold-only is available as a setting for scripts with dense anchors.

## D7 — Transcript is a broadcast bus with a speaker tag (2026-09-16)
Reserved seam for coaching-call mode: matcher, take recorder and future panels are all consumers.

## D8 — Audio: one USB mic shared with OBS; no virtual cable (2026-09-16)
CoreAudio and WASAPI shared mode allow multiple readers. `AudioSourceConfig.sources` is a list
so a second device (Rode Wireless Go II) can be added later without changing types.

## D9 — Local git operated by Claude Code + rsync backup to OneDrive (2026-09-16)
Floris does not use git manually. Git stays as the undo mechanism; no remote, no branches.
`scripts/backup.sh` copies source (excluding build artifacts and models) to OneDrive.
Repo lives outside OneDrive (`target/` and `node_modules/` would wreck sync).

## D10 — Docs and code in English, UI strings English for V1 (2026-09-16)
Code identifiers, crates and commits are English anyway. i18n (Dutch UI) can come later.

## D11 — Font picker: curated candidate list, checked in the UI (2026-09-16)
`list_fonts` returns a fixed list of common families; the UI measures each with a canvas and
keeps only the ones the system actually renders. A free-text field accepts any other family.
Reason: a true system font enumeration needs a platform crate (`font-kit` or similar) and
Floris wants dependencies added only after asking. Revisit when a real list is wanted.

## D12 — Transcript types live in core; Session wraps the speech-side logic (2026-09-16)
`PositionEstimator::on_transcript` takes a `TranscriptEvent`, so the event types must live in
`prompter-core`, not in `prompter-speech` as ARCHITECTURE 3.2 first said; the speech crate
re-exports them. A `Session` type in core combines estimator, pace model and controller so the
Tauri host stays plumbing. Controller inputs gained `UserStart`, `UserHold` and
`LeftTalkingPoints`; tests are Rust tables rather than JSON fixtures.

## D13 — Own downsampler and std channels instead of rubato / tokio / crossbeam (2026-09-16)
The speech path needs exactly one conversion (device rate → 16 kHz mono). A 49-tap windowed-sinc
low-pass plus linear interpolation (`prompter-speech::resample`, tested with tones) does that
without rubato's adapter API. The transcript bus fans out over `std::sync::mpsc`; the worker polls
instead of `select!`. Fewer dependencies, same behaviour. libvosk comes from the Vosk Python
wheels (macOS universal2 0.3.44, Windows 0.3.45) via `scripts/fetch-models.sh`.

## D14 — Matcher tuned after the first live read (2026-09-17)
Floris's first test: jumps backwards, up-and-down inside a paragraph, and a long delay.
Causes and fixes, all verified offline with `scripts/synth-reading.sh` + `replay --script`:
- Matching happens on **partial** results as they stream (revisions handled by common-prefix
  diffing), not only on finals that Vosk emits after a pause. Delay drops to ~0.3 s.
- A hit glides to the **spoken words** (`Anchor.positions`, `PositionEstimate.frac`), not to the
  top of the block, so a reader who is already inside the block is not pulled back.
- **Forward first.** The next anchor fires on `min_hits`. Skipping ahead needs three distinct
  keywords or the same anchor again in a later utterance. Going back (current or previous anchor)
  needs every keyword and a 12 s same-anchor cooldown; a retake of the block under the read line
  moves nothing. Far relocation needs three keywords and confirmation in a later utterance
  unless all keywords were heard.
- **Auto anchors use words unique to their block** (document frequency 1), up to four; a block
  with no unique word gets no anchor (short headings) and cruise carries through it. `min_hits` is
  2 whenever the anchor has two or more keywords. Dutch/English number words are stopwords.
- Words the engine does not know are removed from anchors at runtime (`Session::drop_unknown_words`);
  anchors left empty are transparent to skip counting.
- Events carry audio time (`epoch + samples fed`), so replays run faster than real time with the
  same windows and cooldowns as live capture.

## D15 — Full-vocabulary recognition; anchors matched in the transcript (2026-09-17)
Reverses the grammar half of D3. Floris's real recordings (MacBook Air mic, street noise) showed
the keyword grammar (`anchor words + [unk]`) missing true keywords (three key words of the script
never came out) and hallucinating others, while full-vocabulary decoding of the
same audio was near word-perfect. `VoskEngine` now decodes with the full small model; the matcher
finds keywords in the transcript with tolerance for inflections and compounds
(`word_matches`: "mechanismen" ~ "mechanisme", "onduidelijkheid" ~ "duidelijkheid").
`set_vocabulary` still reports words the model does not know. Grammar mode remains available
(`VoskEngine::with_grammar`, `replay --grammar`) for experiments. Result on both recordings:
every anchor in order, none backwards. The fixtures live in `examples/*.m4a`.

## D16 — TextAligner replaces keyword anchors as the position estimator (2026-09-17)
Floris: "IRL the scrolling doesn't match the voice well… pattern-detect our way into the right
spot, default forward next best guess." With full-vocabulary recognition (D15) the transcript is
near word-perfect, so the whole script becomes one word sequence and the last ~10 recognized
words are aligned against it (order-preserving weighted alignment, anchored at the latest heard
word). Function words weigh 0.3, content words 1.0–1.5 (rarer = more), accents are folded.
Forward within 40 words is free; longer forward jumps, moves back, and far moves need extra
evidence; a few words backwards is ignored as recognizer jitter. The current utterance alone must
carry the evidence; earlier words only add context. Positions are word-level (`DocPos.frac`),
emitted at most every 250 ms after ≥ 2 words of progress. `AnchorMatcher` stays as the fallback
implementation (`replay --anchors`); explicit `<!-- anchor: -->` comments and the anchor list in the
UI remain informative but no longer drive the scroll. The estimator trait now works in `DocPos`
(`set_position` / `cursor_pos`). The app opens on a start screen; the last script is reopened
only on request.

## D17 — GitHub remote for releases; unsigned installers for now (2026-09-18)
Amends D9: the repository may have a GitHub remote so that GitHub Actions can build the macOS
and Windows installers (`release.yml` on version tags). Git stays single-branch, no force pushes,
no history rewrites. Speech models and libvosk are bundled as Tauri resources / frameworks (macOS:
`Contents/Resources/models`, `Contents/Frameworks/libvosk.dylib`; Windows: next to the exe), so
users install nothing else. Builds are unsigned (macOS ad-hoc signature, no notarization; no
Windows certificate): the user guide explains the one-time Gatekeeper / SmartScreen steps.
Signing and notarization can be added when Floris has an Apple Developer account. Until then the
hardened runtime is off: with an ad-hoc signature, library validation rejects `libvosk.dylib`
("different Team IDs") and the app aborts at launch. The entitlement
`com.apple.security.cs.disable-library-validation` is already in place for the signed future.

## D18 — Windows as a development platform (2026-10-24)
Floris now also works on a Windows 11 PC. Toolchain there: VS 2022 Build Tools (C++), rustup
(stable MSVC), Node 22, Tauri CLI via cargo. Consequences:
- Commands that create windows or do slow work are `async` (`open_window`, `start_listening`):
  synchronous commands run on the main thread, where window creation deadlocks on WebView2 and
  model loading freezes the UI. The talent window is placed in physical pixels after creation
  because monitors can have different scale factors.
- `check.yml` runs on macOS and Windows, so Windows regressions show up on every push.
- `scripts/backup.ps1` is the Windows twin of `backup.sh` (robocopy mirror + zip snapshot to
  `%OneDrive%\Claude\PolarityPrompt\backup`). Amends D9.
- `.gitattributes` keeps `*.sh` at LF on every checkout (Windows git converts to CRLF otherwise).
- In development, tauri-build copies libvosk.dll, its MinGW runtime DLLs and the models from
  `tauri.windows.conf.json` next to `target\debug\polarityprompt.exe`; no extra PATH setup needed.

## D19 — Vosk model paths with non-ASCII characters on Windows (2026-10-24)
libvosk opens model files with narrow (ANSI) paths, so a model folder under a user name like
"José" failed to load (verified with `replay` and a copied model). `VoskEngine` now passes the
8.3 short path (`GetShortPathNameW`, declared directly, no new crate) when the path is not ASCII.
Volumes with 8.3 names switched off still fail; the alternative, a UTF-8 code page in the app
manifest, was not needed. Tauri's `resource_dir()` returns a verbatim path (`\\?\C:\…`), in which
libvosk's `/am/final.mdl` is not a valid separator, so the installed app could not load its
models; the `\\?\` prefix is removed before the path goes to Vosk (found by Floris's first
installer test, reproduced with `replay`, covered by a test that loads a canonicalized path).

## D20 — GPL-3.0-or-later, a fresh public repository, recordings out of git (2026-10-24)
- License: GPL-3.0-or-later, so redistributed versions stay open source (Floris: open source
  with donations). Everything bundled is permissive or GPL-compatible (see THIRD-PARTY-NOTICES.txt).
  `THIRD-PARTY-NOTICES.txt` is generated by `scripts/third-party-notices.mjs` from `cargo
  metadata` and the npm lock file (no new tool) and installed next to the app with `LICENSE.txt`;
  the Windows installer shows the license.
- Voice recordings are personal data and never public. Amends D15: the recording fixtures move
  to the git-ignored `fixtures/` folder. Because history cannot be partially public and D9/D17
  forbid rewriting it, this repository stays private as an archive and the public repository
  starts from one clean commit made by `scripts/make-public-snapshot.mjs` (leaves out the
  recordings and KICKOFF.md). The polarity script `examples/voorbeeld-script.md` (Floris's own
  content) is removed from the tree: the core tests now use a neutral fixture with the same
  structure, `crates/prompter-core/testdata/zuurdesem.md` (same 14 blocks, same anchor positions,
  the improvised-sentence regression kept with a heading word in the plural).
- Donations: GitHub Sponsors plus a Mollie payment link; no platform with monthly fees.
- Extra languages will be downloaded inside the app from this project's GitHub Releases, not
  chosen in the installer (works the same on macOS, keeps the installer small).
- Talent window: screen picker (left to right) and "Bring here" (centred over the operator
  window), both in physical pixels. Also: the default font is Segoe UI on Windows (Helvetica Neue is
macOS-only), and release builds suppress the webview's browser context menu and reload/print keys.

## D21 — Extra languages downloaded inside the app, with the system's curl and tar (2026-10-24)
German, French and Spanish join Dutch and English (`Lang::ALL`; stopword lists per language in
normalized form; everything else in core was already language-neutral). Dutch and English stay
bundled; the others are downloaded under Settings → Languages into the app data folder
(`models/vosk-<code>`), looked up before the bundled folder (`find_model_dir`).
- Download and unpack use the operating system's own `curl` and `tar` (bsdtar, which reads zip),
  called by full path from System32 or /usr/bin and without a console window. Both ship with
  Windows 10 (1803+) and macOS, so no HTTP client, TLS stack or zip crate is added. The only new
  direct dependency is `sha2` 0.10, already in the lock file through Tauri.
- Every model is pinned by size and SHA-256 in `prompter-speech::models::CATALOG`; the work
  happens in a temporary folder and only a verified, unpacked model is moved into place.
- Sources in order: this project's GitHub Release `models-1`, then alphacephei.com. Until the
  public repository has that release, downloads come from alphacephei.com.
- French elisions ("l'homme") are single words in the Vosk model and are normalized the same way
  in the script, so no special handling is needed.

## D22 — Only English in the installer; Portuguese; one language picker (2026-10-24)
Amends D21. Floris: keep the installer small and make every language except English optional.
- Bundled: English only (`models/vosk-en`). Dutch, German, French, Spanish and Portuguese are
  downloaded under Settings → Languages. New installs default to English; existing settings keep
  their language. The Windows installer shrinks by the Dutch model (about 25 MB compressed).
- Portuguese uses `vosk-model-small-pt-0.3` (Brazilian, Apache 2.0). It has the older flat model
  layout (`final.mdl` at the top); `is_model_dir` accepts both layouts.
- The repo's `models/` folder is a model source in development builds only, so an installed app
  on a developer machine shows exactly what a user would see.
- One practice script per language (same coffee text); a test checks every spoken word against
  each model that is present (`fetch-models.sh --all` fetches them all). The operator window has
  one Language picker (the open script's language, or the default when nothing is open), and the
  Practice script button and the start screen use it.

## D23 — Window placement in points on macOS (2026-10-24)
Found on Floris's Mac (Retina ×2 plus a ×1 screen): the talent window, meant to cover the second
screen, ended up 960×540 in its middle. macOS converts a physical position or size with the scale
of the screen the window is on *now*, while monitor positions are physical in the target screen's
own scale. Placement now works in the desktop's own units (`DesktopRect` in `apps/desktop`):
points on macOS (one coordinate space for all screens), physical pixels on Windows (unchanged,
see D18). On macOS the window is moved, resized, then moved again: a resize is capped to the
screen the window is on and keeps the bottom edge fixed. "Bring here" uses the same helpers.
Checked with a small `tao` probe on both screens in both directions. A titled window still stays
below the menu bar; covering that too would need a borderless or fullscreen window (not done).

## D24 — The speech engine gets keywords as spelled in the script (2026-10-24)
Keywords are accent-folded for matching (`normalize`: "genießen" → "geniesen"), and that folded
form was also what the engine was asked to check. The models only know the real spelling, so in
German, French, Spanish and Portuguese words with accents, ß, apostrophes or hyphens were
reported as unknown and dropped from the anchors. Now `Document::vocabulary()` gives the
engine every spelling of each keyword as it occurs in the text (`spoken_form`: lowercase, accents,
inner apostrophes and hyphens kept); the engine also accepts the folded form when a model only
knows that; `Session::drop_unknown_words` drops a keyword only when none of its spellings is
known. Matching itself is unchanged (both sides folded). The practice-script test now also checks
the exact word list the app sends.
