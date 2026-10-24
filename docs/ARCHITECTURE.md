# Architecture

Status: authoritative design for V1 (phases 0–1.5). Later phases may extend, not contradict.
Reference model: jub0t/Concat — "everything important lives in the engine crates; the Tauri host
is plumbing; the UI renders state and sends commands."

## 1. Layers and data flow

```
                     ┌──────────────────────── apps/desktop (Tauri host) ────────────────────────┐
  mic(s) ──► AudioSource ──► rtrb ring ──► SpeechWorker ──► TranscriptBus ──┬──► PositionEstimator ──► ScrollController ──► emit ScrollIntent ──► ui/ (N windows)
             (cpal)          buffer        (Vosk)          (broadcast)      ├──► TakeRecorder (WAV + post-take Whisper)
                                                                            └──► (future) notes / advisor panels
  keyboard / operator commands ──► invoke(...) ──► ScrollController / Settings ──► emit
```

Three OS threads in the speech path:

| Thread | Owns | May do | May never do |
|---|---|---|---|
| cpal callback (real-time) | nothing | `producer.push_slice()` | allocate, lock, call the recognizer |
| SpeechWorker | `Box<dyn SpeechEngine>`, resampler | drain ring buffer, resample to 16 kHz mono, `engine.feed()`, publish events, handle `EngineCommand` | touch UI or Tauri state |
| main / Tauri | `PrompterState` (Mutex) | run estimator + controller, emit events, serve commands | block on audio |

## 2. Crate: `prompter-core`

No dependencies on Tauri, audio or STT. Fully unit-testable with scripted event sequences.

### 2.1 Document model

```rust
pub struct BlockId(pub u32);

pub enum BlockKind { Heading(u8), Paragraph, TalkingPoints, Rule }

pub struct Block {
    pub id: BlockId,
    pub kind: BlockKind,
    pub html: String,            // inline HTML for this block (from pulldown-cmark), UI renders as-is
    pub plain: String,           // plain text, for keyword extraction
    pub word_count: usize,       // for the pace model
}

pub struct Document {
    pub path: PathBuf,
    pub hash: u64,               // content hash; takes reference it
    pub language: Lang,          // Nl | En; from front-matter `lang:` or settings default
    pub blocks: Vec<Block>,
    pub anchors: Vec<Anchor>,
}
```

Parsing: `pulldown-cmark` → block tree. A bullet/numbered list becomes one `TalkingPoints` block
(see SCRIPT-FORMAT.md). HTML comments of the form `<!-- anchor: w1 w2 w3 -->` attach an explicit
anchor to the following block.

### 2.2 Anchors (keyword sets, not phrases)

```rust
pub struct AnchorId(pub u32);

pub struct Anchor {
    pub id: AnchorId,
    pub block: BlockId,
    pub ordinal: usize,          // reading order
    pub keywords: Vec<String>,   // normalized (lowercase, no punctuation), 2–4 per anchor
    pub min_hits: usize,         // default: 2 if len >= 3 else 1
    pub explicit: bool,          // authored via <!-- anchor: --> vs auto-derived
}
```

Auto-derivation per `Paragraph`/`Heading` block: content words ≥ 4 chars, not in the nl/en
stopword list, unique to the block (document frequency 1), keep up to 4 in reading order; a block
without a unique word gets no anchor (D14). Each anchor also records `positions` (fraction of the
block's words before each keyword) so a hit can glide to the spoken words.
`TalkingPoints` blocks get no anchor of their own (they are ad-lib zones); the *next* block's
anchor resumes scrolling. Keywords are accent-folded for matching ("geniesen"), but the engine
gets them as spelled in the script (`Document::vocabulary()`: "genießen", "l'arôme"), because the
models know those spellings (D24). Words rejected by the engine (`unknown_words` from
`set_vocabulary`) are reported to the UI; a keyword is dropped when none of its spellings is known.
If an anchor ends up empty, the UI shows a warning on that block.

### 2.3 PositionEstimator trait, TextAligner (V2, default) and AnchorMatcher (V1)

`TextAligner` (D16) flattens the document into words with weights (function words 0.3, content
words 1.0–1.5 by rarity), keeps the last ~10 recognized words, and for each of the last three
heard words aligns the tail against the script around every position of that word
(order-preserving DP). The best candidate wins after penalties: forward ≤ 40 words free, longer
forward jumps +1.0, back ≤ 25 words +1.2, far +2.0; the current utterance's own matched weight
must reach `min_score` (1.6). Small backward moves (< 8 words) are ignored; estimates are emitted
after ≥ 2 words of progress, at most every 250 ms, with `frac` = word position in the block.
`HitKind` follows from the move: same/next block → `Advance{0}` (Near), further → `Advance{n}`
(Skip), back → `Retake`, far back → `Relocate`.

The trait works in document positions:

```rust
pub trait PositionEstimator: Send {
    fn load(&mut self, doc: &Document);
    fn on_transcript(&mut self, ev: &TranscriptEvent, now: Instant) -> Option<PositionEstimate>;
    fn reset(&mut self);
    fn cursor_pos(&self) -> Option<DocPos>;
    fn set_position(&mut self, pos: Option<DocPos>);
}
```

`AnchorMatcher` below is the V1 keyword implementation, kept as fallback and for tests.

```rust
pub struct PositionEstimate { pub anchor: AnchorId, pub kind: HitKind, pub confidence: f32 }
pub enum HitKind { Advance { skipped: usize }, Retake, Relocate { distance: isize } }

pub trait PositionEstimator: Send {
    fn load(&mut self, doc: &Document);
    fn on_transcript(&mut self, ev: &TranscriptEvent, now: Instant) -> Option<PositionEstimate>;
    fn reset(&mut self);
}
```

`AnchorMatcher` (V1 implementation):

```rust
pub struct MatcherConfig {
    pub window: Duration,        // 6 s rolling window of recognized keywords
    pub lookahead: usize,        // 3
    pub lookback: usize,         // 1
    pub cooldown: Duration,      // 1500 ms, same anchor won't re-fire
    pub far_confirmations: u8,   // 2 consecutive consistent hits before a Relocate
}

pub struct AnchorMatcher {
    anchors: Vec<Anchor>,
    recent: VecDeque<(String, Instant)>,   // keywords heard, with time
    cursor: Option<usize>,                 // ordinal of last confirmed anchor
    last_fire: Option<Instant>,
    pending_far: Option<(usize, u8)>,
    cfg: MatcherConfig,
}
```

Algorithm on each `Partial`/`Final` event (D14): tokenize, drop `[unk]`, push the tokens that are
new since the previous partial (common-prefix diff) into `recent`, evict older than `window`, then:
1. forward candidates `cursor+1 ..= cursor+lookahead`: the first with `hits >= min_hits` fires as
   `Advance`; if it skips anchors it needs ≥ 3 keywords or confirmation in a later utterance;
2. backward candidates `cursor, cursor-1`: fire as `Retake` only with every keyword heard and
   outside the 12 s same-anchor cooldown;
3. anything else: `Relocate` with ≥ 3 keywords, immediately when complete, else confirmed in a
   later utterance.
After firing: set cursor, clear `recent`, record the time per anchor. `frac` = position of the
latest keyword heard.

Phase 2 replaces/ensembles this with `SemanticAligner` (local multilingual sentence embeddings via
ONNX, cosine similarity of the last ~2 transcript sentences vs. each block). Same trait; the
ScrollController does not know which one is active.

### 2.4 Pace model (drift guard)

```rust
pub struct PaceModel { wpm_ewma: f32 }   // seeded 140 wpm; updated from anchor-to-anchor timings
impl PaceModel {
    pub fn expected(&self, words_between: usize) -> Duration;
    pub fn observe(&mut self, words: usize, elapsed: Duration);
}
```

If `elapsed_since_last_hit > 1.5 × expected(words to next anchor)` while in `Cruise`, the
controller transitions to `Hold`. Speech is happening but not the script: the speaker is improvising.

### 2.5 ScrollController

```rust
pub struct DocPos { pub block: BlockId, pub frac: f32 }   // UI maps to pixels

pub enum GlideProfile { Near, Skip, Far }
pub enum AfterGlide { Cruise, Hold }

pub enum ScrollState {
    Idle,
    Cruise  { speed: f32 },                        // lines/sec base speed
    Hold,
    Paused  { resume_to: Box<ScrollState> },
    Gliding { target: DocPos, profile: GlideProfile, then: AfterGlide, seq: u64 },
}

pub enum ScrollIntent {                            // the only thing that crosses IPC
    Cruise { speed: f32 },
    Hold,
    GlideTo { pos: DocPos, profile: GlideProfile, then: AfterGlide, seq: u64 },
    Pause,
    Snap { pos: DocPos },                          // explicit user action only
}

pub enum ControllerInput {
    Estimate { target: BlockId, profile: GlideProfile },   // Session resolves the estimate
    Silence(Duration),                             // from engine, no partial change for N ms
    SpeechResumed,
    PaceOverrun,                                   // from PaceModel
    Arrived { seq: u64 },                          // from UI animator
    UserPause, UserStart, UserHold, UserSpeed(f32), UserJump(DocPos), UserReset,
    EnteredTalkingPoints, LeftTalkingPoints,       // block under the read line changed kind
}
```

`Session` (core) wraps document + estimator + `PaceModel` + `ScrollController`: the host feeds it
transcript events, user inputs, `Arrived` and read-line block changes and forwards the returned
intents. It maps `HitKind` to `GlideProfile`, detects pace overrun and speech-resumed, and keeps
the estimator cursor in sync with user jumps.

Transition table (`mode` = user setting `AfterGlide`, default `Cruise`):

| State | Input | Next |
|---|---|---|
| Cruise / Hold / Gliding | Estimate Advance{skipped: 0} or Retake | Gliding{Near, then: mode} |
| Cruise / Hold / Gliding | Estimate Advance{skipped ≥ 1} | Gliding{Skip, then: mode} |
| Cruise / Hold / Gliding | Estimate Relocate | Gliding{Far, then: mode} |
| Gliding | Arrived{seq == current} | `then` |
| Gliding | Arrived{seq != current} | ignored |
| Cruise | Silence > 2500 ms | Hold |
| Cruise | PaceOverrun | Hold |
| Cruise | EnteredTalkingPoints | Hold |
| Hold | SpeechResumed (and block is not TalkingPoints) | Cruise |
| any | UserPause | Paused{resume_to} |
| Paused | UserPause | resume_to |
| any | UserJump | Snap, then Cruise |
| any | UserReset | Snap to top, then Idle |
| any | UserStart / UserHold | Cruise / Hold |
| Gliding | Arrived, `then == Cruise`, read line in TalkingPoints | Hold |

Invariants: a `GlideTo` always supersedes a running glide (the UI spring retargets; no waiting,
no snapping). Speech never produces `Snap`.

### 2.6 Take records

```rust
pub struct TakeId(pub String);                     // "2026-09-16_143012"
pub struct Take {
    pub id: TakeId,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub document_path: PathBuf,
    pub document_hash: u64,
    pub language: Lang,
    pub audio_path: PathBuf,                       // WAV at device rate (typically 48 kHz)
    pub transcript_path: Option<PathBuf>,          // transcript.json with word timings (post-take)
    pub alignment_path: Option<PathBuf>,           // alignment.json: per block verdict + spoken text
    pub anchor_log: Vec<(Duration, AnchorId, HitKind)>,
    pub external_ref: Option<ExternalRef>,         // reserved: { kind: "obs", file: PathBuf }
}
pub enum BlockVerdict { AsWritten, Rephrased, Added, Skipped }
```

Stored as `<takes_dir>/<script-stem>/<take-id>/take.json` plus files. `takes_dir` default:
`~/Documents/PolarityPrompt Takes/`, configurable.

## 3. Crate: `prompter-speech`

### 3.1 Audio

```rust
pub struct DeviceId(pub String);                   // stable by *name*; cpal ids are not stable
pub struct AudioSourceConfig { pub sources: Vec<SourceSel>, pub mode: SourceMode }
pub struct SourceSel { pub device: DeviceId, pub channel: ChannelTag }   // ChannelTag::Self_ in V1
pub enum SourceMode { Single, Mix }                // V1 UI exposes Single only; type supports Mix

pub enum AudioEvent {
    DeviceLost(DeviceId),
    DefaultInputChanged(DeviceId),                 // CoreAudio property listener / IMMNotificationClient
    Level { rms_db: f32 },                         // 10×/s
    PermissionDenied,
}
pub enum AudioStatus { Capturing(DeviceId), Reconnecting, NoSignal(Duration), PermissionDenied, NoDevice }
```

`AudioSupervisor` owns the cpal stream on its own thread; each opened stream owns its
`rtrb::Producer<f32>` and the worker receives the matching consumer via `Reconfigure`. On
`DeviceLost`: drop stream → status `Reconnecting` → controller gets `UserHold` → retry preferred
device, then system default, backoff 250 ms → 2 s → on success a new stream + `Reconfigure`
(which resets the engine and the resampler). `DefaultInputChanged` and an explicit
permission-denied probe are not implemented yet (cpal reports `PermissionDenied` on open when it can). `NoSignal` (RMS below threshold for 5 s) is an indicator, never an
automatic device switch. Ring buffer: 2 s at device rate.

### 3.2 Speech engine (streaming)

`Speaker`, `WordTiming`, `TranscriptEvent` and `EngineStatus` are defined in `prompter-core::transcript`
(the estimator consumes them and core cannot depend on this crate); `prompter-speech` re-exports them.

```rust
pub enum EngineCommand { SetVocabulary { words: Vec<String> }, SetLanguage(Lang), Reset, Shutdown }

pub struct WordTiming { pub word: String, pub start: f32, pub end: f32, pub conf: f32 }
pub enum TranscriptEvent {
    Partial { text: String, t: Instant, speaker: Speaker },
    Final   { text: String, t: Instant, words: Vec<WordTiming>, speaker: Speaker },
    Silence { since: Duration },
    EngineStatus(EngineStatus),                    // Ready | Rebuilding | UnknownWords(Vec<String>)
}
pub enum Speaker { Self_, Other(u8) }              // always Self_ in V1

pub trait SpeechEngine: Send {
    fn set_vocabulary(&mut self, words: &[String]) -> Result<Vec<String>, EngineError>; // returns unknown words
    fn feed(&mut self, pcm_16k_mono: &[i16]) -> Vec<TranscriptEvent>;
    fn reset(&mut self);
}
```

`VoskEngine`: one `vosk::Model` per engine, one `Recognizer` decoding the **full vocabulary** (D15).
`set_vocabulary` only validates words with `Model::find_word` (the given spelling, then its
accent-folded form) and reports the unknown ones; grammar
mode (`words + ["[unk]"]`, rebuilt recognizer) exists behind `with_grammar(true)` for experiments. Language switch =
build a new `VoskEngine` on a helper thread, swap in the worker. `MockEngine` (tests + phase 0)
replays a scripted `Vec<(Duration, TranscriptEvent)>`.

The `SpeechWorker` loop polls the command channel, then drains the ring buffer: downmix →
windowed-sinc `Downsampler` (own code, no rubato) → `engine.feed()` → publish to `TranscriptBus`
(fan-out over `std::sync::mpsc`, one channel per subscriber). Every opened stream hands the worker a
fresh ring-buffer consumer via `EngineCommand::Reconfigure` so the cpal callback can own its
producer outright. Silence: `Silence { since }` is published after 1 s without a partial change and
repeated every 500 ms; the controller applies the 2500 ms threshold. `EngineCommand::Flush`
finalizes buffered audio once the ring is drained (end of file / take).

### 3.3 Transcriber (batch, phase 1.5)

```rust
pub trait Transcriber: Send {
    fn transcribe(&self, wav: &Path, lang: Lang, progress: &dyn Fn(f32)) -> Result<Transcript, EngineError>;
}
pub struct Transcript { pub segments: Vec<Segment>, pub words: Vec<WordTiming> }
```

`WhisperTranscriber` via `whisper-rs` (Metal on macOS, CUDA on Windows), model
`large-v3-turbo`. Runs on a blocking thread after a take ends. `Aligner` (in core) maps the
transcript onto blocks with a token-level local alignment (Smith–Waterman on normalized tokens)
and emits `BlockVerdict`s + spoken text per block.

## 4. Tauri host (`apps/desktop`)

`PrompterState` in `tauri::State<Mutex<..>>`: document, settings, `ScrollState`, last intent,
audio/engine status, current take. All mutations go through commands; every mutation emits.

Commands (invoke), as implemented:
`open_document(path)`, `open_document_dialog()`, `get_snapshot()`, `set_settings(settings)` (full
object; speed, after-glide and audio source live in settings), `scroll(action)` with
`start | pause | hold | jump{pos} | reset` (all through the controller), `animator_arrived(seq)`,
`read_line_block(block)` (on change only), `list_audio_devices()`, `start_listening()`,
`stop_listening()`, `list_fonts()`, `open_window(role)`. Phase 1.5 adds `start_take()`,
`stop_take()`, `list_takes(script)`.

Events (emit, broadcast to all windows):
`prompter://intent` (ScrollIntent), `prompter://snapshot` (full state, on open/settings/listen
change), `prompter://status` (listening + AudioStatus + EngineStatus + level, 10×/s while
listening), `prompter://transcript` (partial/final/silence/status for the operator panel),
`prompter://match` (anchor hit: anchor, block, kind, confidence), `prompter://take` (phase 1.5).

Windows: `operator` (controls + panels + preview) and `talent` (text only, mirror on).
Both load the same UI with `?role=`. A late-joining window calls `get_snapshot()`.

Settings (persisted JSON in app config dir): font family, font size, line height, margins L/R,
mirror H/V, colors, read-line position (fraction of viewport), base speed, `AfterGlide` mode,
language default, audio source config, takes dir, per-window overrides.

## 5. UI (`ui/`)

- React + TypeScript + Vite. State: one store fed by events; components render from it.
- Document view: one `<article>` = one composited layer (`will-change: transform; contain: layout paint`).
  Blocks rendered from `Block.html`. Mirror applied on the *outer* container (`scaleX(-1)` / `scaleY(-1)`).
- Animator: single `requestAnimationFrame` loop, dt-based, critically damped spring in pixels.
  `GlideProfile` → spring params (Near: stiff/fast, Skip: medium + velocity cap, Far: soft + velocity cap).
  `Cruise` → constant velocity; `Hold`/`Pause` → spring to rest. Fractional `translate3d` by default;
  a debug toggle enables device-pixel rounding. Emits `Arrived{seq}` when within 0.5 px and |v| < ε.
- Layout cache: block offsets measured once per layout change (`ResizeObserver`, font/margin change),
  never per frame. `DocPos ↔ px` mapping lives here and only here.
- Operator window = panel layout (dockable later). V1 panels: controls, preview, transcript debug,
  frame-time overlay (dev), take list.
- Dev panel (phase 0): buttons/keys that fire `ScrollIntent`s directly so the animator and
  typography can be tuned in a browser without Rust.

## 6. Reserved seams (do not build yet, do not break)

- `AudioSourceConfig.sources` is a list; `Speaker` on every transcript event.
- `PositionEstimator` trait: second implementation = `SemanticAligner`.
- `TranscriptBus` is multi-consumer.
- `Advisor` trait placeholder (LLM suggestions, local or API) as a future transcript consumer.
- `ExternalRef` on takes for OBS (obs-websocket v5) integration.
- Panel-based operator window.

## 7. Testing

- `prompter-core`: table-driven tests for `AnchorMatcher`, `ScrollController` and `Session` with
  scripted event sequences (Rust tables inside the modules; `crates/prompter-core/testdata/zuurdesem.md` as fixture).
- `prompter-speech`: `replay` binary feeds a WAV through worker + engine headless and prints events;
  used with recorded takes to tune keyword thresholds.
- `ui/`: typecheck + lint; animator unit tests on the spring math.
