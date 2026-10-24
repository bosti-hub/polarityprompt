//! `Session`: one loaded document + estimator + pace model + controller.
//! The host feeds it transcript events and user actions and forwards the
//! resulting intents to the UI. All the logic stays in core and is testable
//! without audio.

use std::time::Instant;

use crate::controller::{ControllerConfig, ControllerInput, DocPos, GlideProfile, ScrollController, ScrollIntent, ScrollState};
use crate::estimator::{HitKind, PositionEstimate, PositionEstimator};

use crate::pace::PaceModel;
use crate::{Block, BlockId, BlockKind, Document, TranscriptEvent};

const PACE_OVERRUN_FACTOR: f32 = 1.5;

pub struct Session {
    doc: Document,
    estimator: Box<dyn PositionEstimator>,
    pace: PaceModel,
    controller: ScrollController,
    /// Flat word index and time of the last position hit.
    last_hit: Option<(usize, Instant)>,
    speaking: bool,
    silent: bool,
    read_line_block: Option<BlockId>,
    current_block: Option<BlockId>,
}

/// What happened as a result of one transcript event, for the operator panel.
#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    pub intents: Vec<ScrollIntent>,
    pub estimate: Option<PositionEstimate>,
}

impl Session {
    pub fn new(doc: Document, mut estimator: Box<dyn PositionEstimator>, cfg: ControllerConfig) -> Self {
        estimator.load(&doc);
        Session {
            doc,
            estimator,
            pace: PaceModel::default(),
            controller: ScrollController::new(cfg),
            last_hit: None,
            speaking: false,
            silent: false,
            read_line_block: None,
            current_block: None,
        }
    }

    /// Block of the last confirmed position (for the operator panel highlight).
    pub fn current_block(&self) -> Option<BlockId> {
        self.current_block
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    pub fn state(&self) -> &ScrollState {
        self.controller.state()
    }

    pub fn pace_wpm(&self) -> f32 {
        self.pace.wpm()
    }

    pub fn set_config(&mut self, cfg: ControllerConfig) -> Vec<ScrollIntent> {
        self.controller.set_config(cfg)
    }

    pub fn set_tuning(&mut self, tuning: &crate::estimator::EstimatorTuning) {
        self.estimator.configure(tuning);
    }

    /// Change the script language (front-matter override from the UI). The
    /// estimator reloads its stopword weights; position is kept.
    pub fn set_language(&mut self, lang: crate::Lang) {
        if self.doc.language == lang {
            return;
        }
        self.doc.language = lang;
        let cursor = self.estimator.cursor_pos();
        self.estimator.load(&self.doc);
        self.estimator.set_position(cursor);
    }

    fn block(&self, id: BlockId) -> Option<&Block> {
        self.doc.blocks.iter().find(|b| b.id == id)
    }

    /// Flat word index of a document position (words of readable blocks before it,
    /// plus the fraction inside its block).
    fn word_index(&self, pos: DocPos) -> usize {
        let mut idx = 0;
        for b in &self.doc.blocks {
            if b.id == pos.block {
                return idx + (pos.frac.clamp(0.0, 1.0) * b.word_count as f32) as usize;
            }
            idx += b.word_count;
        }
        idx
    }

    /// Words from a flat index to the end of its block.
    fn words_to_block_end(&self, idx: usize) -> usize {
        let mut acc = 0;
        for b in &self.doc.blocks {
            if idx < acc + b.word_count {
                return acc + b.word_count - idx;
            }
            acc += b.word_count;
        }
        0
    }

    /// Events carry their own (audio) time; the host does not need a clock.
    pub fn on_transcript(&mut self, ev: &TranscriptEvent) -> Outcome {
        let mut out = Outcome::default();
        if let TranscriptEvent::EngineStatus(status) = ev {
            if let crate::EngineStatus::UnknownWords { words } = status {
                self.drop_unknown_words(words);
            }
            return out;
        }
        let Some(now) = ev.time() else { return out };

        // Silence / speech-resumed bookkeeping.
        match ev {
            TranscriptEvent::Silence { since, .. } => {
                self.speaking = false;
                if !self.silent {
                    // Only the first crossing matters; the controller ignores repeats.
                    out.intents.extend(self.controller.handle(ControllerInput::Silence(*since)));
                    if *since >= self.controller.config().silence_hold {
                        self.silent = true;
                    }
                }
                return out;
            }
            TranscriptEvent::Partial { text, .. } | TranscriptEvent::Final { text, .. } => {
                let has_words = text.split_whitespace().any(|w| w != "[unk]");
                if has_words && (self.silent || !self.speaking) {
                    self.speaking = true;
                    self.silent = false;
                    out.intents.extend(self.controller.handle(ControllerInput::SpeechResumed));
                }
            }
            TranscriptEvent::EngineStatus(_) => return out,
        }

        // Position.
        if let Some(est) = self.estimator.on_transcript(ev, now) {
            out.estimate = Some(est);
            if let Some(pos) = self.estimator.cursor_pos() {
                // Pace: words covered since the previous hit.
                let idx = self.word_index(pos);
                if let Some((prev_idx, t)) = self.last_hit {
                    if idx > prev_idx {
                        self.pace.observe(idx - prev_idx, now.duration_since(t));
                    }
                }
                self.last_hit = Some((idx, now));
                let profile = match est.kind {
                    HitKind::Advance { skipped: 0 } | HitKind::Retake => GlideProfile::Near,
                    HitKind::Advance { .. } => GlideProfile::Skip,
                    HitKind::Relocate { .. } => GlideProfile::Far,
                };
                let target = DocPos { block: pos.block, frac: est.frac.clamp(0.0, 0.98) };
                out.intents.extend(self.controller.handle(ControllerInput::Estimate { target, profile }));
                self.current_block = Some(pos.block);
            }
        } else if let TranscriptEvent::Final { .. } = ev {
            // Speech without a hit: is the speaker off script for too long? Allow the
            // time it takes to read to the end of the current block plus a margin.
            if let (Some((idx, t)), ScrollState::Cruise { .. }) = (self.last_hit, self.controller.state()) {
                let words = self.words_to_block_end(idx).max(8);
                let expected = self.pace.expected(words).mul_f32(PACE_OVERRUN_FACTOR);
                if now.duration_since(t) > expected {
                    out.intents.extend(self.controller.handle(ControllerInput::PaceOverrun));
                }
            }
        }
        out
    }

    /// Words the engine cannot recognize are useless as keywords: remove them so
    /// `min_hits` and skip counting reflect what can actually be heard. The
    /// estimator is reloaded with the cursor preserved.
    pub fn drop_unknown_words(&mut self, unknown: &[String]) {
        if unknown.is_empty() {
            return;
        }
        // The engine reports spellings ("genießen"); a keyword ("geniesen") is lost only when
        // the engine knows none of its spellings.
        let spellings = crate::anchors::keyword_spellings(&self.doc.blocks, &self.doc.anchors);
        let lost = |k: &String| {
            unknown.contains(k) || spellings.get(k).is_some_and(|list| list.iter().all(|s| unknown.contains(s)))
        };
        let mut changed = false;
        for a in &mut self.doc.anchors {
            let before = a.keywords.len();
            let kept: Vec<(String, f32)> = a
                .keywords
                .iter()
                .cloned()
                .zip(a.positions.iter().cloned())
                .filter(|(k, _)| !lost(k))
                .collect();
            if kept.len() != before {
                changed = true;
                a.keywords = kept.iter().map(|(k, _)| k.clone()).collect();
                a.positions = kept.iter().map(|(_, p)| *p).collect();
                a.min_hits = a.min_hits.min(a.keywords.len()).max(usize::from(!a.keywords.is_empty()));
            }
        }
        if changed {
            let cursor = self.estimator.cursor_pos();
            self.estimator.load(&self.doc);
            self.estimator.set_position(cursor);
        }
    }

    /// Anchors that lost every keyword; the UI warns on these blocks.
    pub fn dead_anchor_blocks(&self) -> Vec<BlockId> {
        self.doc.anchors.iter().filter(|a| a.keywords.is_empty()).map(|a| a.block).collect()
    }

    /// UI reports which block sits under the read line whenever it changes.
    pub fn on_read_line_block(&mut self, block: BlockId) -> Vec<ScrollIntent> {
        if self.read_line_block == Some(block) {
            return Vec::new();
        }
        self.read_line_block = Some(block);
        let is_tp = self.block(block).map(|b| b.kind == BlockKind::TalkingPoints).unwrap_or(false);
        self.controller.handle(if is_tp { ControllerInput::EnteredTalkingPoints } else { ControllerInput::LeftTalkingPoints })
    }

    pub fn on_arrived(&mut self, seq: u64) -> Vec<ScrollIntent> {
        self.controller.handle(ControllerInput::Arrived { seq })
    }

    pub fn user(&mut self, input: ControllerInput) -> Vec<ScrollIntent> {
        match &input {
            ControllerInput::UserJump(pos) | ControllerInput::UserJumpHold(pos) => {
                self.estimator.set_position(Some(*pos));
                self.last_hit = None;
            }
            ControllerInput::UserReset => {
                self.estimator.reset();
                self.last_hit = None;
            }
            _ => {}
        }
        self.controller.handle(input)
    }

    /// The user scrolled by hand (wheel/trackpad). Nothing moves on its own: the
    /// matcher cursor follows so the next anchor is the one after the new position.
    pub fn reposition(&mut self, pos: DocPos) {
        self.estimator.set_position(Some(pos));
        self.last_hit = None;
        self.read_line_block = Some(pos.block);
    }

    /// The estimator's current position, if it has one.
    pub fn cursor_pos(&self) -> Option<DocPos> {
        self.estimator.cursor_pos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use crate::estimator::{AnchorMatcher, MatcherConfig};
    use crate::Lang;

    const SCRIPT: &str = include_str!("../testdata/zuurdesem.md");

    fn session() -> (Session, Instant) {
        let doc = Document::parse("t.md", SCRIPT, Lang::Nl);
        let mut s = Session::new(doc, Box::new(AnchorMatcher::new(MatcherConfig::default())), ControllerConfig::default());
        // Pretend the model does not know "zuurdesem" (the heading's only anchor word), as the engine would report.
        s.drop_unknown_words(&["zuurdesem".to_string()]);
        (s, Instant::now())
    }

    fn say(s: &mut Session, text: &str, t: Instant) -> Outcome {
        s.on_transcript(&TranscriptEvent::final_text(text, t))
    }

    #[test]
    fn reading_the_script_glides_block_by_block() {
        let (mut s, t0) = session();
        s.user(ControllerInput::UserStart);
        // Heading "Waarom zuurdesem?" is anchor 0; the explicit anchor on block 1 is anchor 1.
        let o = say(&mut s, "smaak ontstaat niet uit gist maar uit zuur", t0);
        let est = o.estimate.expect("anchor 1 should fire");
        assert_eq!(s.document().anchors[1].block, BlockId(1));
        assert_eq!(est.anchor, s.document().anchors[1].id);
        // The heading's anchor is dead (unknown word), so this is a plain advance.
        assert!(matches!(o.intents.last(), Some(ScrollIntent::GlideTo { pos, profile: GlideProfile::Near, .. }) if pos.block == BlockId(1)), "{:?}", o.intents);

        // Skip over the talking points; "onderdelen vergroten verpesten" is the next anchor (block 3).
        let o = say(&mut s, "daarom kijken we naar drie onderdelen die dat vergroten of verpesten", t0 + Duration::from_secs(8));
        assert!(matches!(o.intents.last(), Some(ScrollIntent::GlideTo { pos, profile: GlideProfile::Near, .. }) if pos.block == BlockId(3)), "{:?}", o.intents);
        // The glide lands on the spoken words: "verpesten" is the last word of that block.
        if let Some(ScrollIntent::GlideTo { pos, .. }) = o.intents.last() {
            assert!(pos.frac > 0.8, "{}", pos.frac);
        }
    }

    #[test]
    fn talking_points_block_holds_and_next_anchor_resumes() {
        let (mut s, t0) = session();
        s.user(ControllerInput::UserStart);
        assert_eq!(s.on_read_line_block(BlockId(1)), vec![]);
        assert_eq!(s.on_read_line_block(BlockId(2)), vec![ScrollIntent::Hold]);
        // Improvising inside the list: speech resumed must not restart the cruise.
        let o = say(&mut s, "[unk] [unk] praktijk [unk]", t0);
        assert!(o.intents.is_empty(), "{:?}", o.intents);
        let o = say(&mut s, "drie onderdelen die het verschil vergroten of verpesten", t0 + Duration::from_secs(2));
        assert!(matches!(o.intents.last(), Some(ScrollIntent::GlideTo { .. })));
        assert_eq!(s.on_read_line_block(BlockId(3)), vec![]);
        let seq = match o.intents.last() {
            Some(ScrollIntent::GlideTo { seq, .. }) => *seq,
            _ => unreachable!(),
        };
        assert_eq!(s.on_arrived(seq), vec![ScrollIntent::Cruise { speed: 2.0 }]);
    }

    #[test]
    fn silence_holds_and_speech_resumes() {
        let (mut s, t0) = session();
        s.user(ControllerInput::UserStart);
        let o = s.on_transcript(&TranscriptEvent::silence(Duration::from_secs(1), t0));
        assert!(o.intents.is_empty());
        let o = s.on_transcript(&TranscriptEvent::silence(Duration::from_secs(3), t0));
        assert_eq!(o.intents, vec![ScrollIntent::Hold]);
        let o = s.on_transcript(&TranscriptEvent::silence(Duration::from_secs(4), t0));
        assert!(o.intents.is_empty());
        let o = s.on_transcript(&TranscriptEvent::partial("het eerste", t0));
        assert_eq!(o.intents, vec![ScrollIntent::Cruise { speed: 2.0 }]);
    }

    #[test]
    fn improvising_for_too_long_triggers_pace_overrun() {
        let (mut s, t0) = session();
        s.user(ControllerInput::UserStart);
        let o = say(&mut s, "smaak zuur", t0);
        let seq = match o.intents.last() {
            Some(ScrollIntent::GlideTo { seq, .. }) => *seq,
            other => panic!("{other:?}"),
        };
        s.on_arrived(seq);
        // 33 words at 140 wpm ≈ 14 s; ×1.5 ≈ 21 s. Off-script chatter after 30 s → hold.
        let o = say(&mut s, "[unk] [unk] [unk]", t0 + Duration::from_secs(10));
        assert!(o.intents.is_empty(), "{:?}", o.intents);
        let o = say(&mut s, "[unk] blabla [unk]", t0 + Duration::from_secs(30));
        assert_eq!(o.intents, vec![ScrollIntent::Hold]);
    }

    #[test]
    fn user_jump_moves_the_anchor_cursor() {
        let (mut s, _t0) = session();
        let out = s.user(ControllerInput::UserJump(DocPos::start_of(BlockId(8))));
        assert!(matches!(out[0], ScrollIntent::Snap { .. }));
        let pos = s.cursor_pos().unwrap();
        assert!(pos.block.0 <= 8 && pos.block.0 >= 7, "{pos:?}");
    }

    #[test]
    fn unknown_words_are_dropped_and_dead_anchors_do_not_count_as_skipped() {
        let doc = Document::parse("t.md", SCRIPT, Lang::Nl);
        let mut s = Session::new(doc, Box::new(AnchorMatcher::new(MatcherConfig::default())), ControllerConfig::default());
        let t0 = Instant::now();
        s.user(ControllerInput::UserStart);
        let status = TranscriptEvent::EngineStatus(crate::EngineStatus::UnknownWords { words: vec!["zuurdesem".into()] });
        s.on_transcript(&status);
        assert_eq!(s.dead_anchor_blocks(), vec![BlockId(0)]);
        let o = say(&mut s, "smaak zuur", t0);
        assert!(matches!(o.estimate, Some(PositionEstimate { kind: HitKind::Advance { skipped: 0 }, .. })), "{:?}", o.estimate);
        assert!(matches!(o.intents.last(), Some(ScrollIntent::GlideTo { profile: GlideProfile::Near, .. })), "{:?}", o.intents);
    }

    #[test]
    fn unknown_spelling_drops_the_folded_keyword() {
        let doc = Document::parse("t.md", "Wir genießen frischen Kaffee.\n\nDanach trinken alle zusammen Tee.", Lang::De);
        assert!(doc.anchors[0].keywords.contains(&"geniesen".to_string()), "{:?}", doc.anchors[0].keywords);
        let mut s = Session::new(doc, Box::new(AnchorMatcher::new(MatcherConfig::default())), ControllerConfig::default());
        s.drop_unknown_words(&["genießen".into()]);
        assert!(s.doc.anchors.iter().all(|a| !a.keywords.contains(&"geniesen".to_string())));
        assert!(s.doc.anchors[0].keywords.contains(&"kaffee".to_string()));
    }

    #[test]
    fn manual_reposition_moves_the_cursor_without_intents() {
        let (mut s, t0) = session();
        s.user(ControllerInput::UserStart);
        s.reposition(DocPos { block: BlockId(8), frac: 0.3 });
        // Next anchor after block 8 is block 11 ("spreken juiste moment omzet"): fires as a plain advance.
        let o = say(&mut s, "het juiste moment spreken", t0);
        assert!(matches!(o.estimate, Some(PositionEstimate { kind: HitKind::Advance { skipped: 0 }, .. })), "{:?}", o.estimate);
        assert!(matches!(o.intents.last(), Some(ScrollIntent::GlideTo { pos, .. }) if pos.block == BlockId(11)), "{:?}", o.intents);
    }

    #[test]
    fn pace_model_learns_from_consecutive_hits() {
        let (mut s, t0) = session();
        s.user(ControllerInput::UserStart);
        say(&mut s, "smaak zuur", t0);
        // Next anchor reached after 33 words in 10 s ≈ 198 wpm.
        say(&mut s, "onderdelen vergroten", t0 + Duration::from_secs(10));
        assert!(s.pace_wpm() > 140.0, "{}", s.pace_wpm());
    }
}
