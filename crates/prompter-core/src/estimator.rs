//! `PositionEstimator` trait and the V1 implementation `AnchorMatcher`.
//! See `docs/ARCHITECTURE.md` 2.3.
//!
//! Tuned after the first live test (DECISIONS D14):
//! - partial results are matched as they stream in, not only finals;
//! - forward is the default; moving back (current or previous anchor) needs
//!   every keyword of that anchor and a long cooldown;
//! - a hit reports where in the block the spoken keywords sit (`frac`).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::anchors::normalize;
use crate::controller::DocPos;
use crate::{Anchor, AnchorId, Document, TranscriptEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HitKind {
    Advance { skipped: usize },
    Retake,
    Relocate { distance: isize },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PositionEstimate {
    pub anchor: AnchorId,
    #[serde(flatten)]
    pub kind: HitKind,
    pub confidence: f32,
    /// Position inside the block (0..1) of the last keyword heard.
    pub frac: f32,
}

/// User-tunable knobs shared by estimators (persisted in settings). Defaults
/// match `AlignerConfig::default()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EstimatorTuning {
    /// Evidence needed before the text moves (weighted matched words).
    pub min_score: f32,
    /// Forward moves up to this many words are free; longer ones need extra evidence.
    pub jump_words: usize,
    /// Extra evidence for a longer forward jump.
    pub jump_penalty: f32,
    /// Extra evidence for moving back.
    pub back_penalty: f32,
    /// Extra evidence for a far move (anywhere else in the script).
    pub far_penalty: f32,
    /// Minimum time between position updates, milliseconds.
    pub min_interval_ms: u64,
}

impl Default for EstimatorTuning {
    fn default() -> Self {
        EstimatorTuning {
            min_score: 1.6,
            jump_words: 40,
            jump_penalty: 1.0,
            back_penalty: 1.2,
            far_penalty: 2.0,
            min_interval_ms: 250,
        }
    }
}

pub trait PositionEstimator: Send {
    fn load(&mut self, doc: &Document);
    fn on_transcript(&mut self, ev: &TranscriptEvent, now: Instant) -> Option<PositionEstimate>;
    fn reset(&mut self);
    /// Last confirmed position, if any.
    fn cursor_pos(&self) -> Option<DocPos>;
    /// Move the cursor without a hit (user jumped or scrolled).
    fn set_position(&mut self, pos: Option<DocPos>);
    /// Apply user tuning; estimators that have no knobs ignore it.
    fn configure(&mut self, _tuning: &EstimatorTuning) {}
}

#[derive(Debug, Clone)]
pub struct MatcherConfig {
    /// Rolling window of recognized keywords.
    pub window: Duration,
    pub lookahead: usize,
    pub lookback: usize,
    /// A different anchor won't fire within this time of the last hit.
    pub cooldown: Duration,
    /// The same anchor won't fire again within this time (a retake of the block being read).
    pub same_anchor_cooldown: Duration,
    /// Kept for configuration compatibility; relocation now confirms across utterances.
    pub far_confirmations: u8,
}

impl Default for MatcherConfig {
    fn default() -> Self {
        MatcherConfig {
            window: Duration::from_secs(6),
            lookahead: 3,
            lookback: 1,
            cooldown: Duration::from_millis(700),
            same_anchor_cooldown: Duration::from_secs(12),
            far_confirmations: 2,
        }
    }
}

pub struct AnchorMatcher {
    anchors: Vec<Anchor>,
    recent: VecDeque<(String, Instant)>,
    cursor: Option<usize>,
    last_fire: Option<(usize, Instant)>,
    /// Time each anchor last fired, for the same-anchor cooldown.
    fired_at: Vec<Option<Instant>>,
    pending_far: Option<(usize, u64)>,
    /// Tokens of the current utterance as last seen in a partial result.
    utterance: Vec<String>,
    /// Counts finals; a far relocation must be confirmed in a later utterance.
    utterance_id: u64,
    cfg: MatcherConfig,
}

impl AnchorMatcher {
    pub fn new(cfg: MatcherConfig) -> Self {
        AnchorMatcher {
            anchors: Vec::new(),
            recent: VecDeque::new(),
            cursor: None,
            last_fire: None,
            fired_at: Vec::new(),
            pending_far: None,
            utterance: Vec::new(),
            utterance_id: 0,
            cfg,
        }
    }

    /// Distinct keywords of `anchor` heard recently, with the position of the latest one.
    fn hits(&self, anchor: &Anchor) -> (usize, f32) {
        let mut count = 0;
        let mut frac: f32 = 0.0;
        for (k, pos) in anchor.keywords.iter().zip(anchor.positions.iter()) {
            if self.recent.iter().any(|(w, _)| word_matches(k, w)) {
                count += 1;
                frac = frac.max(*pos);
            }
        }
        (count, frac)
    }

    fn in_cooldown(&self, ordinal: usize, now: Instant) -> bool {
        if let Some((_, t)) = self.last_fire {
            if now.duration_since(t) < self.cfg.cooldown {
                return true;
            }
        }
        matches!(self.fired_at.get(ordinal), Some(Some(t)) if now.duration_since(*t) < self.cfg.same_anchor_cooldown)
    }

    /// Anchors whose keywords are all unknown to the engine can never fire.
    fn usable(&self, ordinal: usize) -> bool {
        self.anchors.get(ordinal).map(|a| !a.keywords.is_empty()).unwrap_or(false)
    }

    fn forward_candidates(&self) -> Vec<usize> {
        let n = self.anchors.len();
        let range = match self.cursor {
            None => 0..n,
            Some(c) => c + 1..n,
        };
        let take = if self.cursor.is_none() { self.cfg.lookahead + 1 } else { self.cfg.lookahead };
        range.filter(|o| self.usable(*o)).take(take).collect()
    }

    fn backward_candidates(&self) -> Vec<usize> {
        match self.cursor {
            None => Vec::new(),
            Some(c) => (0..=self.cfg.lookback).filter_map(|k| c.checked_sub(k)).filter(|o| self.usable(*o)).collect(),
        }
    }

    /// Usable anchors strictly between `from` (exclusive) and `to` (exclusive).
    fn skipped_between(&self, from: Option<usize>, to: usize) -> usize {
        let start = from.map(|f| f + 1).unwrap_or(0);
        (start..to).filter(|o| self.usable(*o)).count()
    }

    fn fire(&mut self, ordinal: usize, kind: HitKind, frac: f32, now: Instant) -> PositionEstimate {
        let a = &self.anchors[ordinal];
        let (count, _) = self.hits(a);
        let confidence = (count as f32 / a.keywords.len().max(1) as f32).min(1.0);
        self.cursor = Some(ordinal);
        self.last_fire = Some((ordinal, now));
        if let Some(slot) = self.fired_at.get_mut(ordinal) {
            *slot = Some(now);
        }
        self.pending_far = None;
        self.recent.clear();
        PositionEstimate { anchor: a.id, kind, confidence, frac }
    }

    fn push_tokens(&mut self, text: &str, is_final: bool, now: Instant) {
        let tokens: Vec<String> = text
            .split_whitespace()
            .filter(|t| *t != "[unk]")
            .map(normalize)
            .filter(|w| !w.is_empty())
            .collect();
        // Partial results grow and sometimes revise their last words. Push whatever
        // comes after the common prefix with the previous partial; a final ends the utterance.
        let common = tokens.iter().zip(self.utterance.iter()).take_while(|(a, b)| a == b).count();
        for _ in common..self.utterance.len() {
            self.recent.pop_back();
        }
        for w in &tokens[common..] {
            self.recent.push_back((w.clone(), now));
        }
        if is_final {
            self.utterance = Vec::new();
            self.utterance_id += 1;
        } else {
            self.utterance = tokens;
        }
        while let Some((_, t)) = self.recent.front() {
            if now.duration_since(*t) > self.cfg.window {
                self.recent.pop_front();
            } else {
                break;
            }
        }
    }

    /// Weak evidence for a jump becomes acceptable when the same anchor is still
    /// the best candidate in a later utterance. Returns true once confirmed.
    fn confirm(&mut self, ord: usize) -> bool {
        match self.pending_far {
            Some((prev, first)) if prev == ord => self.utterance_id > first,
            _ => {
                self.pending_far = Some((ord, self.utterance_id));
                false
            }
        }
    }

    fn evaluate(&mut self, now: Instant) -> Option<PositionEstimate> {
        // Forward, nearest first: the normal reading flow. The very next anchor fires
        // on `min_hits`; jumping over anchors needs three keywords or a second utterance,
        // because single hallucinated words during improvisation add up otherwise.
        for ord in self.forward_candidates() {
            let a = &self.anchors[ord];
            let (count, frac) = self.hits(a);
            if count >= a.min_hits && !self.in_cooldown(ord, now) {
                let skipped = self.skipped_between(self.cursor, ord);
                let strong = count >= 3 || count == a.keywords.len() && a.keywords.len() >= 3;
                if skipped > 0 && !strong && !self.confirm(ord) {
                    return None;
                }
                return Some(self.fire(ord, HitKind::Advance { skipped }, frac, now));
            }
        }
        // Backward (retake of the current or previous block): every keyword, and not
        // while that block is still being read.
        for ord in self.backward_candidates() {
            let a = &self.anchors[ord];
            let (count, frac) = self.hits(a);
            if a.keywords.len() >= 2 && count == a.keywords.len() && !self.in_cooldown(ord, now) {
                return Some(self.fire(ord, HitKind::Retake, frac, now));
            }
        }
        // Far: outside the window. Needs strong evidence: at least three distinct
        // keywords (so two-word anchors never relocate), and either every keyword of
        // the anchor or the same anchor heard again in a later utterance.
        let near: Vec<usize> = self.forward_candidates().into_iter().chain(self.backward_candidates()).collect();
        let far_hit = (0..self.anchors.len()).filter(|o| !near.contains(o) && self.usable(*o)).find_map(|o| {
            let a = &self.anchors[o];
            let (count, frac) = self.hits(a);
            let enough = a.keywords.len() >= 3 && count >= 3 && !self.in_cooldown(o, now);
            enough.then_some((o, count == a.keywords.len(), frac))
        });
        if let Some((ord, complete, frac)) = far_hit {
            if complete || self.confirm(ord) {
                let distance = ord as isize - self.cursor.map(|c| c as isize).unwrap_or(-1);
                return Some(self.fire(ord, HitKind::Relocate { distance }, frac, now));
            }
        }
        None
    }
}

/// Tolerates inflections and compounds: "mechanisme" ~ "mechanismen",
/// "duidelijkheid" ~ "onduidelijkheid". Short words must match exactly.
pub fn word_matches(keyword: &str, heard: &str) -> bool {
    if keyword == heard {
        return true;
    }
    let (kl, hl) = (keyword.chars().count(), heard.chars().count());
    if kl < 5 || hl < 5 {
        return false;
    }
    heard.contains(keyword) || (keyword.contains(heard) && hl + 2 >= kl)
}

impl PositionEstimator for AnchorMatcher {
    fn load(&mut self, doc: &Document) {
        self.anchors = doc.anchors.clone();
        self.fired_at = vec![None; self.anchors.len()];
        self.reset();
    }

    fn reset(&mut self) {
        self.recent.clear();
        self.cursor = None;
        self.last_fire = None;
        self.fired_at.iter_mut().for_each(|f| *f = None);
        self.pending_far = None;
        self.utterance.clear();
    }

    fn cursor_pos(&self) -> Option<DocPos> {
        self.cursor.and_then(|o| self.anchors.get(o)).map(|a| DocPos::start_of(a.block))
    }

    fn set_position(&mut self, pos: Option<DocPos>) {
        self.cursor = pos.and_then(|p| self.anchors.iter().rposition(|a| a.block.0 <= p.block.0));
        self.recent.clear();
        self.pending_far = None;
        self.utterance.clear();
        self.fired_at.iter_mut().for_each(|f| *f = None);
    }

    fn on_transcript(&mut self, ev: &TranscriptEvent, now: Instant) -> Option<PositionEstimate> {
        if self.anchors.is_empty() {
            return None;
        }
        match ev {
            TranscriptEvent::Partial { text, .. } => self.push_tokens(text, false, now),
            TranscriptEvent::Final { text, .. } => self.push_tokens(text, true, now),
            _ => return None,
        }
        self.evaluate(now)
    }
}

/// Test double: replays scripted estimates in order, one per `Final` event.
pub struct ScriptedEstimator {
    pub script: VecDeque<Option<PositionEstimate>>,
    cursor: Option<DocPos>,
}

impl ScriptedEstimator {
    pub fn new(script: Vec<Option<PositionEstimate>>) -> Self {
        ScriptedEstimator { script: script.into(), cursor: None }
    }
}

impl PositionEstimator for ScriptedEstimator {
    fn load(&mut self, _doc: &Document) {}
    fn on_transcript(&mut self, ev: &TranscriptEvent, _now: Instant) -> Option<PositionEstimate> {
        match ev {
            TranscriptEvent::Final { .. } => self.script.pop_front().flatten(),
            _ => None,
        }
    }
    fn reset(&mut self) {
        self.cursor = None;
    }
    fn cursor_pos(&self) -> Option<DocPos> {
        self.cursor
    }
    fn set_position(&mut self, pos: Option<DocPos>) {
        self.cursor = pos;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BlockId, Lang};

    const SCRIPT: &str = "\
<!-- anchor: alpha bravo charlie -->
One alpha and bravo then charlie here.

<!-- anchor: delta echo foxtrot -->
Two delta and echo then foxtrot here.

<!-- anchor: golf hotel india -->
Three golf and hotel then india here.

<!-- anchor: juliet kilo lima -->
Four juliet and kilo then lima here.

<!-- anchor: mike november oscar -->
Five mike and november then oscar here.

<!-- anchor: papa quebec romeo -->
Six papa and quebec then romeo here.
";

    fn matcher() -> (AnchorMatcher, Instant) {
        let doc = Document::parse("t.md", SCRIPT, Lang::En);
        assert_eq!(doc.anchors.len(), 6);
        let mut m = AnchorMatcher::new(MatcherConfig::default());
        m.load(&doc);
        (m, Instant::now())
    }

    fn fin(m: &mut AnchorMatcher, text: &str, t: Instant) -> Option<PositionEstimate> {
        m.on_transcript(&TranscriptEvent::final_text(text, t), t)
    }

    fn part(m: &mut AnchorMatcher, text: &str, t: Instant) -> Option<PositionEstimate> {
        m.on_transcript(&TranscriptEvent::partial(text, t), t)
    }

    #[test]
    fn first_anchor_fires_on_two_of_three_keywords() {
        let (mut m, t0) = matcher();
        assert_eq!(fin(&mut m, "alpha something", t0), None);
        let e = fin(&mut m, "and charlie", t0 + Duration::from_secs(1)).unwrap();
        assert_eq!(e.anchor, AnchorId(0));
        assert_eq!(e.kind, HitKind::Advance { skipped: 0 });
        assert_eq!(m.cursor_pos().unwrap().block, BlockId(0));
        // "charlie" is word 5 of 7 → the glide lands on the spoken words, not the top.
        assert!((e.frac - 5.0 / 7.0).abs() < 0.01, "{}", e.frac);
    }

    #[test]
    fn partials_fire_before_the_final_and_are_not_double_counted() {
        let (mut m, t0) = matcher();
        assert_eq!(part(&mut m, "alpha", t0), None);
        let e = part(&mut m, "alpha bravo", t0 + Duration::from_millis(300)).unwrap();
        assert_eq!(e.anchor, AnchorId(0));
        // The final of the same utterance must not re-fire anything.
        assert_eq!(fin(&mut m, "alpha bravo", t0 + Duration::from_millis(600)), None);
        // Next utterance: partial tokens start counting from zero again.
        let e = part(&mut m, "delta echo", t0 + Duration::from_secs(3)).unwrap();
        assert_eq!(e.anchor, AnchorId(1));
    }

    #[test]
    fn a_revised_partial_still_pushes_the_new_word() {
        let (mut m, t0) = matcher();
        assert_eq!(part(&mut m, "alpha xray", t0), None);
        // The recognizer changes its mind about the second word: "xray" → "bravo".
        let e = part(&mut m, "alpha bravo", t0 + Duration::from_millis(300));
        assert!(e.is_some(), "revised word must count");
    }

    #[test]
    fn words_outside_window_are_forgotten() {
        let (mut m, t0) = matcher();
        assert_eq!(fin(&mut m, "alpha", t0), None);
        assert_eq!(fin(&mut m, "bravo", t0 + Duration::from_secs(7)), None);
    }

    #[test]
    fn unk_tokens_are_ignored() {
        let (mut m, t0) = matcher();
        assert_eq!(fin(&mut m, "[unk] [unk] alpha [unk]", t0), None);
    }

    #[test]
    fn skipping_needs_three_keywords_or_a_second_utterance() {
        let (mut m, t0) = matcher();
        fin(&mut m, "alpha bravo", t0).unwrap();
        // Two of three words of an anchor further ahead: pending.
        assert_eq!(fin(&mut m, "golf hotel", t0 + Duration::from_secs(3)), None);
        let e = fin(&mut m, "golf india", t0 + Duration::from_secs(4)).unwrap();
        assert_eq!(e.anchor, AnchorId(2));
        assert_eq!(e.kind, HitKind::Advance { skipped: 1 });

        // All three at once skips immediately.
        let e = fin(&mut m, "mike november oscar", t0 + Duration::from_secs(8)).unwrap();
        assert_eq!(e.anchor, AnchorId(4));
        assert_eq!(e.kind, HitKind::Advance { skipped: 1 });
    }

    #[test]
    fn reading_on_inside_the_current_block_does_not_jump_back() {
        let (mut m, t0) = matcher();
        fin(&mut m, "alpha bravo", t0).unwrap();
        // The rest of the block: all three keywords heard again within 12 s → still nothing.
        assert_eq!(fin(&mut m, "alpha bravo charlie", t0 + Duration::from_secs(3)), None);
        assert_eq!(fin(&mut m, "charlie alpha bravo", t0 + Duration::from_secs(6)), None);
    }

    #[test]
    fn restarting_a_block_much_later_is_a_retake_only_with_all_keywords() {
        let (mut m, t0) = matcher();
        fin(&mut m, "alpha bravo", t0).unwrap();
        fin(&mut m, "delta echo", t0 + Duration::from_secs(5)).unwrap();
        // Two of three keywords of the previous block: not enough to go back.
        assert_eq!(fin(&mut m, "alpha bravo", t0 + Duration::from_secs(20)), None);
        let e = fin(&mut m, "charlie", t0 + Duration::from_secs(21)).unwrap();
        assert_eq!(e.anchor, AnchorId(0));
        assert_eq!(e.kind, HitKind::Retake);
    }

    #[test]
    fn far_anchor_needs_all_keywords_or_a_second_utterance() {
        let (mut m, t0) = matcher();
        fin(&mut m, "alpha bravo", t0).unwrap();
        // Two of three words far away: never enough.
        assert_eq!(fin(&mut m, "papa quebec", t0 + Duration::from_secs(2)), None);
        assert_eq!(fin(&mut m, "papa quebec", t0 + Duration::from_secs(3)), None);
        // All three in one utterance: relocate at once.
        let e = fin(&mut m, "papa quebec romeo", t0 + Duration::from_secs(4)).unwrap();
        assert_eq!(e.anchor, AnchorId(5));
        assert_eq!(e.kind, HitKind::Relocate { distance: 5 });
        assert_eq!(m.cursor_pos().unwrap().block, BlockId(5));
    }

    #[test]
    fn far_relocation_confirms_across_utterances() {
        let doc = Document::parse(
            "t.md",
            "<!-- anchor: alpha bravo charlie -->\nA.\n\n<!-- anchor: delta echo foxtrot -->\nB.\n\n<!-- anchor: golf hotel india -->\nC.\n\n<!-- anchor: juliet kilo lima -->\nD.\n\n<!-- anchor: mike november oscar papa -->\nE.\n\n<!-- anchor: quebec romeo sierra -->\nF.\n",
            Lang::En,
        );
        let mut m = AnchorMatcher::new(MatcherConfig::default());
        m.load(&doc);
        let t0 = Instant::now();
        fin(&mut m, "alpha bravo", t0).unwrap();
        // Anchor 4 has four words; three of them = strong but incomplete → needs a second utterance.
        assert_eq!(fin(&mut m, "mike november oscar", t0 + Duration::from_secs(2)), None);
        let e = fin(&mut m, "mike november oscar", t0 + Duration::from_secs(3)).unwrap();
        assert_eq!(e.kind, HitKind::Relocate { distance: 4 });
    }

    #[test]
    fn forward_is_preferred_over_retake_when_both_match() {
        let (mut m, t0) = matcher();
        fin(&mut m, "alpha bravo", t0).unwrap();
        let e = fin(&mut m, "alpha bravo charlie delta echo", t0 + Duration::from_secs(3)).unwrap();
        assert_eq!(e.anchor, AnchorId(1));
    }

    #[test]
    fn set_cursor_moves_the_window() {
        let (mut m, t0) = matcher();
        m.set_position(Some(DocPos::start_of(BlockId(4))));
        let e = fin(&mut m, "papa quebec", t0).unwrap();
        assert_eq!(e.anchor, AnchorId(5));
        assert_eq!(e.kind, HitKind::Advance { skipped: 0 });
    }

    #[test]
    fn anchors_without_usable_keywords_are_transparent() {
        let (mut m, t0) = matcher();
        // Pretend the engine knows none of anchor 0's words.
        m.anchors[0].keywords.clear();
        m.anchors[0].positions.clear();
        let e = fin(&mut m, "delta echo", t0).unwrap();
        assert_eq!(e.anchor, AnchorId(1));
        assert_eq!(e.kind, HitKind::Advance { skipped: 0 });
    }

    #[test]
    fn word_forms_match_tolerantly() {
        assert!(word_matches("mechanisme", "mechanismen"));
        assert!(word_matches("duidelijkheid", "onduidelijkheid"));
        assert!(word_matches("mechanismen", "mechanisme"));
        assert!(!word_matches("neemt", "neem"));
        assert!(!word_matches("spanning", "spannend"));
        assert!(!word_matches("moment", "momentopname".chars().take(3).collect::<String>().as_str()));
    }

    #[test]
    fn hits_are_counted_from_transcript_with_punctuation_and_case() {
        let (mut m, t0) = matcher();
        let e = fin(&mut m, "Alpha, Bravo!", t0).unwrap();
        assert_eq!(e.anchor, AnchorId(0));
        assert!(e.confidence > 0.6);
    }
}
