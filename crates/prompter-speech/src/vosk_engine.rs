//! `VoskEngine`: streaming recognition with runtime grammar (small models only).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use prompter_core::{Lang, Speaker, TranscriptEvent, WordTiming};
use vosk::{DecodingState, Model, Recognizer};

use crate::engine::{EngineError, SpeechEngine, ENGINE_SAMPLE_RATE};

/// Folder per language inside the models directory: `vosk-nl`, `vosk-de`, … (bundled
/// resource or downloaded), falling back to the versioned download folder `vosk-model-small-nl-*`.
pub fn model_dir(models_root: &Path, lang: Lang) -> PathBuf {
    let direct = models_root.join(format!("vosk-{}", lang.code()));
    if direct.exists() {
        return direct;
    }
    let prefix = format!("vosk-model-small-{}", lang.code());
    if let Ok(entries) = std::fs::read_dir(models_root) {
        let mut found: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with(&prefix)).unwrap_or(false))
            .collect();
        found.sort();
        if let Some(p) = found.pop() {
            return p;
        }
    }
    direct
}

/// The model folder for `lang` in the first of `roots` that has it (downloaded models
/// before bundled ones); the first root's folder when none has it, for the error message.
pub fn find_model_dir(roots: &[PathBuf], lang: Lang) -> PathBuf {
    roots
        .iter()
        .map(|r| model_dir(r, lang))
        .find(|d| d.exists())
        .unwrap_or_else(|| model_dir(roots.first().map(PathBuf::as_path).unwrap_or(Path::new("models")), lang))
}

/// The path as Vosk gets it. libvosk opens model files with narrow (ANSI) paths on
/// Windows, so a folder with non-ASCII characters (a user name like "José") cannot
/// be loaded. The 8.3 short form of such a path is plain ASCII; use it when there is one.
fn model_path_arg(dir: &Path) -> String {
    let path = strip_verbatim(&dir.to_string_lossy());
    #[cfg(windows)]
    if !path.is_ascii() {
        if let Some(short) = short_path(Path::new(&path)) {
            return short;
        }
    }
    path
}

/// `\\?\C:\…` → `C:\…` and `\\?\UNC\server\share\…` → `\\server\share\…`. Tauri returns
/// the installed resource folder in this "verbatim" form; libvosk appends `/am/final.mdl`,
/// and in a verbatim path a forward slash is not a separator, so loading fails.
fn strip_verbatim(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    match path.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest.to_string(),
        _ => path.to_string(),
    }
}

/// `GetShortPathNameW`, only when the result is ASCII. `None` when the volume has no
/// 8.3 names (they can be switched off) or the path does not exist.
#[cfg(windows)]
fn short_path(path: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetShortPathNameW(long_path: *const u16, short_path: *mut u16, len: u32) -> u32;
    }

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut buf = vec![0u16; 1024];
    // SAFETY: `wide` is NUL-terminated and `buf` is as long as the length passed.
    let n = unsafe { GetShortPathNameW(wide.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if n == 0 || n >= buf.len() {
        return None;
    }
    let short = String::from_utf16(&buf[..n]).ok()?;
    short.is_ascii().then_some(short)
}

/// A Vosk model folder: the current layout (`am/`, `conf/`) or the older flat one with
/// `final.mdl` at the top (e.g. the Portuguese small model).
pub fn is_model_dir(dir: &Path) -> bool {
    dir.join("am").join("final.mdl").is_file() || dir.join("final.mdl").is_file()
}

fn quiet_logs() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| vosk::set_log_level(vosk::LogLevel::Error));
}

pub struct VoskEngine {
    model: Arc<Mutex<Model>>,
    recognizer: Recognizer,
    last_partial: String,
    /// Restrict recognition to the vocabulary (+ `[unk]`). Off by default: with a
    /// real voice and room noise the grammar absorbs true keywords into `[unk]` and
    /// hallucinates others; full-vocabulary decoding is far more reliable (D15).
    use_grammar: bool,
}

impl VoskEngine {
    pub fn new(model_dir: &Path) -> Result<Self, EngineError> {
        quiet_logs();
        if !is_model_dir(model_dir) {
            return Err(EngineError::ModelNotFound(model_dir.to_path_buf()));
        }
        let model = Model::new(model_path_arg(model_dir)).ok_or_else(|| EngineError::Init(format!("cannot load model {}", model_dir.display())))?;
        let recognizer = Recognizer::new(&model, ENGINE_SAMPLE_RATE as f32)
            .ok_or_else(|| EngineError::Init("cannot create recognizer".into()))?;
        let mut engine = VoskEngine { model: Arc::new(Mutex::new(model)), recognizer, last_partial: String::new(), use_grammar: false };
        engine.recognizer.set_words(true);
        Ok(engine)
    }

    /// Grammar mode (keywords + `[unk]` only). Kept for experiments via the replay tool.
    pub fn with_grammar(mut self, on: bool) -> Self {
        self.use_grammar = on;
        self
    }

    /// Words the model can recognize, out of `words`.
    pub fn known_words(&self, words: &[String]) -> Vec<String> {
        let mut model = self.model.lock().expect("model");
        words.iter().filter(|w| model.find_word(w).is_some()).cloned().collect()
    }
}

impl SpeechEngine for VoskEngine {
    fn set_vocabulary(&mut self, words: &[String]) -> Result<Vec<String>, EngineError> {
        let model = self.model.lock().expect("model");
        let mut model = model;
        let mut known: Vec<String> = Vec::new();
        let mut unknown = Vec::new();
        for w in words {
            // The script's spelling first ("genießen"); the folded one as a fallback, for a
            // model that spells without accents.
            let folded = prompter_core::anchors::normalize(w);
            let spelling = if model.find_word(w).is_some() {
                Some(w.clone())
            } else if model.find_word(&folded).is_some() {
                Some(folded)
            } else {
                None
            };
            match spelling {
                Some(s) if !known.contains(&s) => known.push(s),
                Some(_) => {}
                None => unknown.push(w.clone()),
            }
        }
        if self.use_grammar {
            let mut grammar: Vec<String> = known;
            grammar.push("[unk]".to_string());
            // The bindings have no set_grammar; a fresh recognizer with a grammar is cheap on small models.
            let mut rec = Recognizer::new_with_grammar(&model, ENGINE_SAMPLE_RATE as f32, &grammar)
                .ok_or_else(|| EngineError::Vocabulary("recognizer rejected the grammar".into()))?;
            rec.set_words(true);
            self.recognizer = rec;
            self.last_partial.clear();
        }
        Ok(unknown)
    }

    fn feed(&mut self, pcm: &[i16]) -> Vec<TranscriptEvent> {
        let mut out = Vec::new();
        let now = Instant::now();
        match self.recognizer.accept_waveform(pcm) {
            Ok(DecodingState::Finalized) => {
                let result = self.recognizer.result();
                if let Some(single) = result.single() {
                    let text = single.text.to_string();
                    let words = single
                        .result
                        .iter()
                        .map(|w| WordTiming { word: w.word.to_string(), start: w.start, end: w.end, conf: w.conf })
                        .collect();
                    if !text.is_empty() {
                        out.push(TranscriptEvent::Final { text, t: now, words, speaker: Speaker::Self_ });
                    }
                }
                self.last_partial.clear();
            }
            Ok(DecodingState::Running) => {
                let partial = self.recognizer.partial_result().partial.to_string();
                if partial != self.last_partial {
                    self.last_partial = partial.clone();
                    if !partial.is_empty() {
                        out.push(TranscriptEvent::Partial { text: partial, t: now, speaker: Speaker::Self_ });
                    }
                }
            }
            Ok(DecodingState::Failed) | Err(_) => {}
        }
        out
    }

    fn flush(&mut self) -> Vec<TranscriptEvent> {
        let now = Instant::now();
        let mut out = Vec::new();
        let result = self.recognizer.final_result();
        if let Some(single) = result.single() {
            let text = single.text.to_string();
            let words = single
                .result
                .iter()
                .map(|w| WordTiming { word: w.word.to_string(), start: w.start, end: w.end, conf: w.conf })
                .collect();
            if !text.is_empty() {
                out.push(TranscriptEvent::Final { text, t: now, words, speaker: Speaker::Self_ });
            }
        }
        self.last_partial.clear();
        out
    }

    fn reset(&mut self) {
        self.recognizer.reset();
        self.last_partial.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn models_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models")
    }

    #[test]
    fn loads_dutch_model_and_filters_unknown_words() {
        let dir = model_dir(&models_root(), Lang::Nl);
        if !dir.exists() {
            eprintln!("skipping: run scripts/fetch-models.sh");
            return;
        }
        let mut e = VoskEngine::new(&dir).unwrap();
        let unknown = e
            .set_vocabulary(&["koffiebonen".into(), "brood".into(), "xqzzyplox".into()])
            .unwrap();
        assert_eq!(unknown, vec!["xqzzyplox"]);
        // Grammar mode still builds a restricted recognizer.
        let mut g = VoskEngine::new(&dir).unwrap().with_grammar(true);
        assert_eq!(g.set_vocabulary(&["brood".into()]).unwrap(), Vec::<String>::new());
        // Silence produces no words.
        let events = e.feed(&vec![0i16; 16000]);
        assert!(events.iter().all(|ev| !matches!(ev, TranscriptEvent::Final { .. })), "{events:?}");
    }

    /// The built-in practice scripts must be readable with the bundled models: every
    /// spoken word known, or the operator window shows "unknown words" on first try.
    #[test]
    fn practice_scripts_use_only_known_words() {
        let mut problems = Vec::new();
        for lang in Lang::ALL {
            let file = format!("demo-{}.md", lang.code());
            let dir = model_dir(&models_root(), lang);
            if !dir.exists() {
                eprintln!("skipping {file}: no {} model (scripts/fetch-models.sh --all)", lang.code());
                continue;
            }
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(&file);
            let doc = prompter_core::Document::parse(&path, &std::fs::read_to_string(&path).unwrap(), lang);
            // Words as the recognizer spells them: apostrophes and hyphens stay inside a word
            // ("l'eau", "ci-dessous").
            let mut words: Vec<String> = doc
                .blocks
                .iter()
                .flat_map(|b| {
                    b.plain
                        .split(|c: char| !(c.is_alphabetic() || c == '\'' || c == '-'))
                        .map(|w| w.trim_matches(|c| c == '\'' || c == '-').to_lowercase())
                        .collect::<Vec<_>>()
                })
                .filter(|w| !w.is_empty())
                .collect();
            words.sort();
            words.dedup();
            let mut engine = VoskEngine::new(&dir).unwrap();
            let known = engine.known_words(&words);
            let unknown: Vec<&String> = words.iter().filter(|w| !known.contains(w)).collect();
            if !unknown.is_empty() {
                problems.push(format!("{file}: {unknown:?}"));
            }
            // And the words exactly as the app hands them to the engine (D24).
            let unknown = engine.set_vocabulary(&doc.vocabulary()).unwrap();
            if !unknown.is_empty() {
                problems.push(format!("{file} vocabulary: {unknown:?}"));
            }
        }
        assert!(problems.is_empty(), "words unknown to the model:\n{}", problems.join("\n"));
    }

    #[test]
    fn verbatim_prefix_is_removed() {
        assert_eq!(strip_verbatim(r"\\?\C:\Users\me\models"), r"C:\Users\me\models");
        assert_eq!(strip_verbatim(r"\\?\UNC\nas\share\models"), r"\\nas\share\models");
        assert_eq!(strip_verbatim(r"\\?\Volume{1234}\models"), r"\\?\Volume{1234}\models");
        assert_eq!(strip_verbatim("/Applications/PolarityPrompt.app"), "/Applications/PolarityPrompt.app");
    }

    #[cfg(windows)]
    #[test]
    fn verbatim_model_path_loads() {
        let dir = model_dir(&models_root(), Lang::Nl);
        if !dir.exists() {
            eprintln!("skipping: run scripts/fetch-models.sh");
            return;
        }
        // canonicalize() gives the verbatim form on Windows, like Tauri's resource_dir().
        let verbatim = dir.canonicalize().unwrap();
        assert!(verbatim.to_string_lossy().starts_with(r"\\?\"), "{}", verbatim.display());
        assert!(VoskEngine::new(&verbatim).is_ok(), "{}", verbatim.display());
    }

    #[cfg(windows)]
    #[test]
    fn non_ascii_folder_gets_an_ascii_short_path() {
        let dir = std::env::temp_dir().join(format!("pp-modèl-José-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let arg = model_path_arg(&dir);
        let reachable = Path::new(&arg).is_dir();
        std::fs::remove_dir_all(&dir).ok();
        if !arg.is_ascii() {
            eprintln!("skipping: no 8.3 short names on this volume ({arg})");
            return;
        }
        assert!(reachable, "{arg} does not lead to the folder");
    }

    #[test]
    fn missing_model_is_a_clear_error() {
        let err = VoskEngine::new(Path::new("/nonexistent/model")).err().expect("must fail");
        assert!(matches!(err, EngineError::ModelNotFound(_)));
    }
}
