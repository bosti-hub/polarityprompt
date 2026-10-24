//! `TextAligner`: the V2 `PositionEstimator` (DECISIONS D16).
//!
//! Instead of a few keywords per paragraph, the whole script is one word
//! sequence. The last few recognized words are aligned against a window of
//! that sequence, nearest-forward first; a good alignment yields a word-level
//! position (block + fraction). Function words count a little, content words
//! a lot, rare words most.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use crate::anchors::normalize;
use crate::controller::DocPos;
use crate::estimator::{word_matches, EstimatorTuning, HitKind, PositionEstimate, PositionEstimator};
use crate::stopwords::is_stopword;
use crate::{AnchorId, BlockId, BlockKind, Document, Lang, TranscriptEvent};

#[derive(Debug, Clone)]
pub struct AlignerConfig {
    /// Recognized words older than this are forgotten.
    pub window: Duration,
    /// How many recent words take part in an alignment.
    pub tail: usize,
    /// Script words ahead of the cursor that count as "near".
    pub ahead: usize,
    /// Script words behind the cursor that count as "near".
    pub back: usize,
    /// Minimum alignment score (weighted matched words) to accept a position.
    pub min_score: f32,
    /// Extra score needed for a position behind the cursor / far away / a forward
    /// jump of more than `jump_words`.
    pub back_penalty: f32,
    pub far_penalty: f32,
    pub jump_penalty: f32,
    pub jump_words: usize,
    /// Backward moves smaller than this are recognizer jitter, not a retake.
    pub back_jitter: usize,
    /// Emit an estimate only after moving this many script words forward.
    pub min_advance: usize,
    /// Never emit more often than this.
    pub min_interval: Duration,
}

impl Default for AlignerConfig {
    fn default() -> Self {
        AlignerConfig {
            window: Duration::from_secs(8),
            tail: 10,
            ahead: 250,
            back: 25,
            min_score: 1.6,
            back_penalty: 1.2,
            far_penalty: 2.0,
            jump_penalty: 1.0,
            jump_words: 40,
            back_jitter: 8,
            min_advance: 2,
            min_interval: Duration::from_millis(250),
        }
    }
}

#[derive(Debug, Clone)]
struct ScriptWord {
    norm: String,
    block: BlockId,
    frac: f32,
    weight: f32,
}

pub struct TextAligner {
    cfg: AlignerConfig,
    words: Vec<ScriptWord>,
    /// stem → script positions, for candidate generation.
    index: HashMap<String, Vec<usize>>,
    /// Block id → (first word index, word count), to map positions and count skips.
    blocks: Vec<(BlockId, usize, usize)>,
    /// (word, time, utterance id). Words of the current utterance carry full
    /// evidence; older ones only provide context.
    recent: VecDeque<(String, Instant, u64)>,
    utterance: Vec<String>,
    utterance_id: u64,
    /// Script word index of the last confirmed position.
    cursor: Option<usize>,
    last_emit: Option<(usize, Instant)>,
}

#[derive(Clone, Copy, Default)]
struct Cell {
    score: f32,
    own: f32,
}

fn stem(word: &str) -> String {
    let n = word.chars().count();
    if n >= 6 {
        word.chars().take(6).collect()
    } else {
        word.to_string()
    }
}

impl TextAligner {
    pub fn new(cfg: AlignerConfig) -> Self {
        TextAligner {
            cfg,
            words: Vec::new(),
            index: HashMap::new(),
            blocks: Vec::new(),
            recent: VecDeque::new(),
            utterance: Vec::new(),
            utterance_id: 0,
            cursor: None,
            last_emit: None,
        }
    }

    fn word_pos(&self, i: usize) -> DocPos {
        let w = &self.words[i];
        DocPos { block: w.block, frac: w.frac }
    }

    /// Script word index closest to a document position.
    fn index_of(&self, pos: DocPos) -> Option<usize> {
        let (_, start, count) = self.blocks.iter().find(|(b, _, _)| *b == pos.block).or_else(|| {
            // Block without words (rule): take the next block that has some.
            self.blocks.iter().find(|(b, _, c)| b.0 >= pos.block.0 && *c > 0)
        })?;
        if *count == 0 {
            return Some(*start);
        }
        Some(start + ((pos.frac.clamp(0.0, 0.999) * *count as f32) as usize).min(count - 1))
    }

    fn push_tokens(&mut self, text: &str, is_final: bool, now: Instant) {
        let tokens: Vec<String> = text
            .split_whitespace()
            .filter(|t| *t != "[unk]")
            .map(normalize)
            .filter(|w| !w.is_empty())
            .collect();
        let common = tokens.iter().zip(self.utterance.iter()).take_while(|(a, b)| a == b).count();
        // Words the recognizer retracted (revised partial) leave the buffer again.
        for _ in common..self.utterance.len() {
            if matches!(self.recent.back(), Some((_, _, u)) if *u == self.utterance_id) {
                self.recent.pop_back();
            }
        }
        for w in &tokens[common..] {
            self.recent.push_back((w.clone(), now, self.utterance_id));
        }
        if is_final {
            self.utterance.clear();
        } else {
            self.utterance = tokens;
        }
        while self.recent.len() > self.cfg.tail * 2 {
            self.recent.pop_front();
        }
        while let Some((_, t, _)) = self.recent.front() {
            if now.duration_since(*t) > self.cfg.window {
                self.recent.pop_front();
            } else {
                break;
            }
        }
    }

    /// Weighted, order-preserving alignment of `heard` (with per-word "current
    /// utterance" flags) against `script[start..end]`. Returns total score and the
    /// part contributed by the current utterance.
    fn align(&self, heard: &[(String, bool)], start: usize, end: usize) -> Cell {
        if start >= end || heard.is_empty() {
            return Cell::default();
        }
        let seg = &self.words[start..end];
        let n = heard.len();
        let m = seg.len();
        let mut dp = vec![vec![Cell::default(); m + 1]; n + 1];
        for i in 1..=n {
            for j in 1..=m {
                let mut best = dp[i - 1][j];
                if dp[i][j - 1].score > best.score {
                    best = dp[i][j - 1];
                }
                if word_matches(&seg[j - 1].norm, &heard[i - 1].0) {
                    let w = seg[j - 1].weight;
                    let prev = dp[i - 1][j - 1];
                    let cand = Cell { score: prev.score + w, own: prev.own + if heard[i - 1].1 { w } else { 0.0 } };
                    if cand.score > best.score {
                        best = cand;
                    }
                }
                dp[i][j] = best;
            }
        }
        dp[n][m]
    }

    fn evaluate(&mut self, now: Instant) -> Option<PositionEstimate> {
        if self.words.is_empty() || self.recent.is_empty() {
            return None;
        }
        let current = self.utterance_id;
        let heard: Vec<(String, bool)> = self
            .recent
            .iter()
            .rev()
            .take(self.cfg.tail)
            .map(|(w, _, u)| (w.clone(), *u == current))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let n = heard.len();
        let cursor = self.cursor;
        let mut best: Option<(f32, usize)> = None;

        // Anchor the alignment at one of the last three heard words: the alignment must
        // end exactly there, so stale context cannot pull the position backwards.
        for k in 0..3.min(n) {
            let sub = &heard[..n - k];
            let (last_word, last_is_current) = &sub[n - k - 1];
            let Some(positions) = self.index.get(&stem(last_word)) else { continue };
            for &p in positions {
                let w = &self.words[p];
                if !word_matches(&w.norm, last_word) {
                    continue;
                }
                let start = p.saturating_sub(sub.len() + 4);
                let ctx = self.align(&sub[..sub.len() - 1], start, p);
                let score = ctx.score + w.weight;
                let own = ctx.own + if *last_is_current { w.weight } else { 0.0 };
                if own < self.cfg.min_score {
                    continue;
                }
                let penalty = match cursor {
                    None => 0.0,
                    Some(c) if p >= c && p <= c + self.cfg.jump_words => 0.0,
                    Some(c) if p >= c && p <= c + self.cfg.ahead => self.cfg.jump_penalty,
                    Some(c) if p < c && c - p <= self.cfg.back => self.cfg.back_penalty,
                    Some(_) => self.cfg.far_penalty,
                };
                let tiebreak = match cursor {
                    Some(c) if p >= c => (p - c) as f32 * 0.001,
                    _ => 0.5,
                } + k as f32 * 0.15;
                let adjusted = score - penalty - tiebreak;
                if adjusted >= self.cfg.min_score && best.map(|(s, _)| adjusted > s).unwrap_or(true) {
                    best = Some((adjusted, p));
                }
            }
        }
        let (score, end) = best?;

        // A few words backwards is the recognizer revising itself; hold position.
        if let Some(c) = cursor {
            if end < c && c - end < self.cfg.back_jitter {
                return None;
            }
        }
        let moved_enough = match cursor {
            None => true,
            Some(c) => end >= c + self.cfg.min_advance || end < c,
        };
        let too_soon = matches!(self.last_emit, Some((_, t)) if now.duration_since(t) < self.cfg.min_interval);
        let prev = cursor;
        self.cursor = Some(end);
        if !moved_enough || too_soon {
            return None;
        }
        self.last_emit = Some((end, now));

        let block = self.words[end].block;
        let kind = match prev {
            None => HitKind::Advance { skipped: 0 },
            Some(c) if end >= c => {
                let from = self.words[c].block;
                let skipped = self.blocks.iter().filter(|(b, _, n)| b.0 > from.0 && b.0 < block.0 && *n > 0).count();
                HitKind::Advance { skipped }
            }
            Some(c) if c - end <= self.cfg.back => HitKind::Retake,
            Some(c) => HitKind::Relocate { distance: end as isize - c as isize },
        };
        let confidence = (score / (self.cfg.min_score * 2.0)).min(1.0);
        Some(PositionEstimate { anchor: AnchorId(end as u32), kind, confidence, frac: self.words[end].frac })
    }
}

impl PositionEstimator for TextAligner {
    fn load(&mut self, doc: &Document) {
        self.words.clear();
        self.index.clear();
        self.blocks.clear();
        let lang: Lang = doc.language;
        // Document frequency per stem, for the rarity weight.
        let mut df: HashMap<String, usize> = HashMap::new();
        for b in &doc.blocks {
            for w in b.plain.split_whitespace() {
                let n = normalize(w);
                if !n.is_empty() {
                    *df.entry(stem(&n)).or_insert(0) += 1;
                }
            }
        }
        for b in &doc.blocks {
            let start = self.words.len();
            if b.kind == BlockKind::Rule {
                self.blocks.push((b.id, start, 0));
                continue;
            }
            let toks: Vec<String> = b.plain.split_whitespace().map(normalize).filter(|w| !w.is_empty()).collect();
            let count = toks.len();
            for (i, n) in toks.into_iter().enumerate() {
                let st = stem(&n);
                let short = n.chars().count() < 3;
                let weight = if short {
                    0.15
                } else if is_stopword(lang, &n) || n.chars().count() < 4 {
                    0.3
                } else {
                    let f = df.get(&st).copied().unwrap_or(1) as f32;
                    1.0 + (1.0 / f).min(1.0) * 0.5
                };
                self.index.entry(st).or_default().push(self.words.len());
                self.words.push(ScriptWord { norm: n, block: b.id, frac: i as f32 / count.max(1) as f32, weight });
            }
            self.blocks.push((b.id, start, count));
        }
        self.reset();
    }

    fn on_transcript(&mut self, ev: &TranscriptEvent, now: Instant) -> Option<PositionEstimate> {
        let is_final = match ev {
            TranscriptEvent::Partial { text, .. } => {
                self.push_tokens(text, false, now);
                false
            }
            TranscriptEvent::Final { text, .. } => {
                self.push_tokens(text, true, now);
                true
            }
            _ => return None,
        };
        let out = self.evaluate(now);
        if is_final {
            // Words of this utterance become context for the next one.
            self.utterance_id += 1;
        }
        out
    }

    fn reset(&mut self) {
        self.recent.clear();
        self.utterance.clear();
        self.utterance_id += 1;
        self.cursor = None;
        self.last_emit = None;
    }

    fn cursor_pos(&self) -> Option<DocPos> {
        self.cursor.map(|i| self.word_pos(i))
    }

    fn configure(&mut self, t: &EstimatorTuning) {
        self.cfg.min_score = t.min_score.clamp(0.5, 6.0);
        self.cfg.jump_words = t.jump_words.clamp(5, 500);
        self.cfg.jump_penalty = t.jump_penalty.clamp(0.0, 5.0);
        self.cfg.back_penalty = t.back_penalty.clamp(0.0, 5.0);
        self.cfg.far_penalty = t.far_penalty.clamp(0.0, 6.0);
        self.cfg.min_interval = Duration::from_millis(t.min_interval_ms.clamp(50, 2000));
    }

    fn set_position(&mut self, pos: Option<DocPos>) {
        self.cursor = pos.and_then(|p| self.index_of(p));
        self.recent.clear();
        self.utterance.clear();
        self.utterance_id += 1;
        self.last_emit = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = include_str!("../testdata/zuurdesem.md");

    fn aligner() -> (TextAligner, Instant) {
        let doc = Document::parse("t.md", SCRIPT, Lang::Nl);
        let mut a = TextAligner::new(AlignerConfig::default());
        a.load(&doc);
        (a, Instant::now())
    }

    fn say(a: &mut TextAligner, text: &str, t: Instant) -> Option<PositionEstimate> {
        a.on_transcript(&TranscriptEvent::final_text(text, t), t)
    }

    #[test]
    fn follows_the_words_inside_a_paragraph() {
        let (mut a, t0) = aligner();
        let e = say(&mut a, "smaak ontstaat niet uit gist", t0).unwrap();
        assert_eq!(a.words[a.cursor.unwrap()].block, BlockId(1));
        assert!(e.frac < 0.2, "{e:?}");
        let e = say(&mut a, "en dat verschil is geen toeval", t0 + Duration::from_secs(4)).unwrap();
        assert_eq!(a.words[a.cursor.unwrap()].block, BlockId(1));
        assert!(e.frac > 0.5 && e.frac < 0.8, "{e:?}");
        assert!(matches!(e.kind, HitKind::Advance { skipped: 0 }));
    }

    #[test]
    fn function_words_alone_do_not_move_it() {
        let (mut a, t0) = aligner();
        say(&mut a, "smaak ontstaat niet uit gist", t0);
        assert_eq!(say(&mut a, "en dat is het", t0 + Duration::from_secs(3)), None);
        assert_eq!(say(&mut a, "[unk] [unk] eh", t0 + Duration::from_secs(4)), None);
    }

    #[test]
    fn skipping_a_paragraph_is_reported_as_a_skip() {
        let (mut a, t0) = aligner();
        say(&mut a, "smaak ontstaat niet uit gist", t0);
        // Jump to block 5 ("Het eerste onderdeel is voorbereiding. Wie de voorbereiding serieus neemt bepaalt…").
        let e = say(&mut a, "wie de voorbereiding serieus neemt bepaalt de richting", t0 + Duration::from_secs(5)).unwrap();
        assert_eq!(a.words[a.cursor.unwrap()].block, BlockId(5));
        assert!(matches!(e.kind, HitKind::Advance { skipped } if skipped >= 1), "{e:?}");
    }

    #[test]
    fn going_back_needs_more_evidence_than_going_forward() {
        let (mut a, t0) = aligner();
        say(&mut a, "wie de voorbereiding serieus neemt bepaalt de richting van het brood", t0);
        assert_eq!(a.words[a.cursor.unwrap()].block, BlockId(5));
        // Two content words of the first paragraph: with the back penalty this is not enough.
        assert_eq!(say(&mut a, "gist zuur", t0 + Duration::from_secs(3)), None);
        // A whole sentence is.
        let e = say(&mut a, "het verschil tussen twee broden is precies wat de bakker proeft", t0 + Duration::from_secs(6)).unwrap();
        assert_eq!(a.words[a.cursor.unwrap()].block, BlockId(1));
        assert!(matches!(e.kind, HitKind::Relocate { .. } | HitKind::Retake), "{e:?}");
    }

    #[test]
    fn partials_move_it_early_and_are_rate_limited() {
        let (mut a, t0) = aligner();
        let p = |a: &mut TextAligner, s: &str, ms: u64| a.on_transcript(&TranscriptEvent::partial(s, t0 + Duration::from_millis(ms)), t0 + Duration::from_millis(ms));
        assert!(p(&mut a, "smaak ontstaat", 0).is_some());
        assert!(p(&mut a, "smaak ontstaat niet uit", 100).is_none(), "within min_interval");
        assert!(p(&mut a, "smaak ontstaat niet uit gist maar uit zuur", 600).is_some());
    }

    #[test]
    fn improvised_sentence_ending_in_a_heading_word_does_not_skip_ahead() {
        let (mut a, t0) = aligner();
        a.set_position(Some(DocPos { block: BlockId(8), frac: 0.3 }));
        let text = "en dat is misschien wat verkeerd begrepen concept en kan ook wat lastig element je hebt als je dat door kneedt door draagt naar deeg weeg onderdelen";
        let e = say(&mut a, text, t0);
        if let Some(e) = e {
            let pos = a.cursor_pos().unwrap();
            // Explain: recompute candidate scores for the last words.
            let heard: Vec<(String, bool)> = a.recent.iter().rev().take(a.cfg.tail).map(|(w, _, u)| (w.clone(), *u == a.utterance_id)).collect::<Vec<_>>().into_iter().rev().collect();
            eprintln!("heard tail: {:?}", heard.iter().map(|h| h.0.as_str()).collect::<Vec<_>>());
            for k in 0..3 {
                let sub = &heard[..heard.len() - k];
                let last = &sub[sub.len() - 1].0;
                if let Some(ps) = a.index.get(&stem(last)) {
                    for &p in ps {
                        let start = p.saturating_sub(sub.len() + 4);
                        let ctx = a.align(&sub[..sub.len() - 1], start, p);
                        let seg: Vec<&str> = a.words[start..=p].iter().map(|w| w.norm.as_str()).collect();
                        eprintln!("k={k} p={p} block={} score={:.2} own={:.2} seg={:?}", a.words[p].block.0, ctx.score + a.words[p].weight, ctx.own + a.words[p].weight, seg);
                    }
                }
            }
            panic!("unexpected move to {:?} kind {:?}", pos, e.kind);
        }
    }

    #[test]
    fn revised_partials_retract_their_words() {
        let (mut a, t0) = aligner();
        a.set_position(Some(DocPos { block: BlockId(8), frac: 0.3 }));
        let p = |a: &mut TextAligner, s: &str, ms: u64| a.on_transcript(&TranscriptEvent::partial(s, t0 + Duration::from_millis(ms)), t0 + Duration::from_millis(ms));
        assert!(p(&mut a, "als je dat door draagt naar deeg deeg onderdelen", 0).is_none());
        let words: Vec<&str> = a.recent.iter().map(|(w, _, _)| w.as_str()).collect();
        assert_eq!(words.iter().filter(|w| **w == "onderdelen").count(), 1);
        // Revision: "deeg deeg onderdelen" → "deeg weeg onderdelen".
        assert!(p(&mut a, "als je dat door draagt naar deeg weeg onderdelen", 300).is_none());
        let words: Vec<&str> = a.recent.iter().map(|(w, _, _)| w.as_str()).collect();
        assert_eq!(words.iter().filter(|w| **w == "onderdelen").count(), 1, "{words:?}");
        assert_eq!(words.iter().filter(|w| **w == "deeg").count(), 1, "{words:?}");
    }

    #[test]
    fn set_position_moves_the_cursor() {
        let (mut a, t0) = aligner();
        a.set_position(Some(DocPos { block: BlockId(8), frac: 0.0 }));
        let e = say(&mut a, "rijzen is geen wachttijd en geen gok", t0).unwrap();
        assert_eq!(a.words[a.cursor.unwrap()].block, BlockId(8));
        assert!(matches!(e.kind, HitKind::Advance { skipped: 0 }), "{e:?}");
        assert_eq!(a.cursor_pos().unwrap().block, BlockId(8));
    }
}
