//! Headless replay: feed a WAV through the worker + Vosk and print the events.
//!
//! Usage: cargo run -p prompter-speech --bin replay -- <file.wav> [nl|en] [--script <file.md>] [--anchors] [--grammar] [word ...]
//! - words restrict the grammar, like the app does with anchor keywords;
//! - `--script` loads a Markdown script, uses its anchor words as grammar, runs the
//!   full core Session (matcher + controller) and prints every intent, so the scroll
//!   behaviour can be checked without a microphone.
//!
//! POLARITYPROMPT_MODELS overrides the models directory.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use prompter_core::{AlignerConfig, AnchorMatcher, ControllerConfig, ControllerInput, Document, MatcherConfig, PositionEstimator, ScrollIntent, Session, TextAligner};
use prompter_speech::vosk_engine::model_dir;
use prompter_speech::{EngineCommand, EngineFactory, Lang, SpeechEngine, SpeechWorker, TranscriptBus, TranscriptEvent, VoskEngine};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(wav_path) = args.first() else {
        eprintln!("usage: replay <file.wav> [nl|en] [--script <file.md>] [word ...]");
        std::process::exit(2);
    };
    let mut lang = Lang::Nl;
    let mut grammar = false;
    let mut anchors = false;
    let mut script: Option<PathBuf> = None;
    let mut words: Vec<String> = Vec::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--script" => {
                script = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--grammar" => grammar = true,
            "--anchors" => anchors = true,
            a => match Lang::parse(a) {
                Some(l) if i == 1 => lang = l,
                _ => words.push(a.to_string()),
            },
        }
        i += 1;
    }
    let models = std::env::var_os("POLARITYPROMPT_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models"));

    let wav = prompter_speech::wav::read(std::path::Path::new(wav_path)).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    eprintln!("{}: {} Hz, {} ch, {:.1} s", wav_path, wav.sample_rate, wav.channels, wav.samples.len() as f32 / (wav.sample_rate * wav.channels as u32) as f32);

    let mut session = script.map(|path| {
        let text = std::fs::read_to_string(&path).expect("cannot read script");
        let doc = Document::parse(&path, &text, lang);
        lang = doc.language;
        words = doc.vocabulary();
        eprintln!("script: {} blocks, {} anchors, {} vocabulary words", doc.blocks.len(), doc.anchors.len(), words.len());
        for a in &doc.anchors {
            eprintln!("  anchor {:2} → block {:2}  {}", a.ordinal, a.block.0, a.keywords.join(" "));
        }
        let estimator: Box<dyn PositionEstimator> = if anchors {
            Box::new(AnchorMatcher::new(MatcherConfig::default()))
        } else {
            Box::new(TextAligner::new(AlignerConfig::default()))
        };
        let mut s = Session::new(doc, estimator, ControllerConfig::default());
        s.user(ControllerInput::UserStart);
        s
    });

    let bus = Arc::new(TranscriptBus::new());
    let rx = bus.subscribe();
    let factory: EngineFactory = Box::new(move |lang| {
        VoskEngine::new(&model_dir(&models, lang)).map(|e| Box::new(e.with_grammar(grammar)) as Box<dyn SpeechEngine>)
    });
    let worker = SpeechWorker::spawn(lang, factory, bus.clone()).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(wav.samples.len() + 1);
    worker.send(EngineCommand::Reconfigure { consumer, sample_rate: wav.sample_rate, channels: wav.channels });
    if !words.is_empty() {
        worker.send(EngineCommand::SetVocabulary { words });
    }
    for s in &wav.samples {
        let _ = producer.push(*s);
    }
    worker.send(EngineCommand::Flush);

    let started = Instant::now();
    let total = Duration::from_secs_f64(wav.samples.len() as f64 / (wav.sample_rate as f64 * wav.channels as f64));
    let mut epoch: Option<Instant> = None;
    let mut last = Instant::now();
    let mut glides = 0usize;
    let mut backwards = 0usize;
    let mut last_block: i64 = -1;
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(ev) => {
                last = Instant::now();
                // Audio time of the event, relative to the first one.
                let at = match ev.time() {
                    Some(t) => {
                        let e = *epoch.get_or_insert(t);
                        t.saturating_duration_since(e).as_secs_f32()
                    }
                    None => 0.0,
                };
                let outcome = session.as_mut().map(|s| s.on_transcript(&ev));
                match &ev {
                    TranscriptEvent::Partial { text, .. } => {
                        if session.is_none() || std::env::var_os("REPLAY_VERBOSE").is_some() {
                            println!("{at:6.2}  partial  {text}");
                        }
                    }
                    TranscriptEvent::Final { text, words, .. } => {
                        println!("{at:6.2}  FINAL    {text}");
                        if session.is_none() {
                            for w in words {
                                println!("           {:5.2}-{:5.2} {:.2} {}", w.start, w.end, w.conf, w.word);
                            }
                        }
                    }
                    TranscriptEvent::Silence { since, .. } => {
                        if session.is_none() || since.as_millis() % 1000 == 0 {
                            println!("{at:6.2}  silence  {:.1}s", since.as_secs_f32());
                        }
                    }
                    TranscriptEvent::EngineStatus(s) => println!("{at:6.2}  status   {s:?}"),
                }
                if let Some(o) = outcome {
                    if let Some(est) = o.estimate {
                        let pos = session.as_ref().and_then(|s| s.cursor_pos());
                        let block = pos.map(|p| p.block.0 as i64).unwrap_or(-1);
                        println!("{at:6.2}  match    block {block:2} frac {:.2} {:?} conf {:.2}", est.frac, est.kind, est.confidence);
                    }
                    for intent in o.intents {
                        println!("{at:6.2}  INTENT   {intent:?}");
                        if let ScrollIntent::GlideTo { pos, .. } = intent {
                            glides += 1;
                            if (pos.block.0 as i64) < last_block {
                                backwards += 1;
                            }
                            last_block = pos.block.0 as i64;
                        }
                    }
                }
            }
            Err(_) => {
                if last.elapsed() > Duration::from_secs(2) || started.elapsed() > total + Duration::from_secs(5) {
                    break;
                }
            }
        }
    }
    if session.is_some() {
        println!("summary: {glides} glides, {backwards} backwards");
    }
}
