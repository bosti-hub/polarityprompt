//! `ScrollController`: turns estimates, silence, pace and user actions into
//! `ScrollIntent`s. See `docs/ARCHITECTURE.md` 2.5 for the transition table.
//!
//! Invariants: a glide always supersedes a running glide; speech never produces
//! `Snap` (CLAUDE.md rule 4): only `UserJump`/`UserReset` do.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::BlockId;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DocPos {
    pub block: BlockId,
    pub frac: f32,
}

impl DocPos {
    pub fn start_of(block: BlockId) -> Self {
        DocPos { block, frac: 0.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GlideProfile {
    Near,
    Skip,
    Far,
}

/// What the prompter does after a glide lands on an anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AfterGlide {
    #[default]
    Cruise,
    Hold,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScrollState {
    Idle,
    Cruise { speed: f32 },
    Hold,
    Paused { resume_to: Box<ScrollState> },
    Gliding { target: DocPos, profile: GlideProfile, then: AfterGlide, seq: u64 },
}

/// The only thing that crosses IPC for motion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScrollIntent {
    Cruise { speed: f32 },
    Hold,
    GlideTo { pos: DocPos, profile: GlideProfile, then: AfterGlide, seq: u64 },
    Pause,
    /// Explicit user action only.
    Snap { pos: DocPos },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ControllerInput {
    /// A position estimate resolved to a document position.
    Estimate { target: DocPos, profile: GlideProfile },
    Silence(Duration),
    SpeechResumed,
    PaceOverrun,
    Arrived { seq: u64 },
    UserPause,
    UserStart,
    UserHold,
    UserSpeed(f32),
    UserJump(DocPos),
    /// Jump and stay (bookmark for a retake): Snap, then Hold.
    UserJumpHold(DocPos),
    UserReset,
    EnteredTalkingPoints,
    LeftTalkingPoints,
}

#[derive(Debug, Clone)]
pub struct ControllerConfig {
    pub base_speed: f32,
    pub after_glide: AfterGlide,
    /// Silence longer than this turns Cruise into Hold.
    pub silence_hold: Duration,
}

impl Default for ControllerConfig {
    fn default() -> Self {
        ControllerConfig {
            base_speed: 2.0,
            after_glide: AfterGlide::Cruise,
            silence_hold: Duration::from_millis(2500),
        }
    }
}

pub struct ScrollController {
    state: ScrollState,
    cfg: ControllerConfig,
    seq: u64,
    in_talking_points: bool,
}

impl ScrollController {
    pub fn new(cfg: ControllerConfig) -> Self {
        ScrollController { state: ScrollState::Idle, cfg, seq: 0, in_talking_points: false }
    }

    pub fn state(&self) -> &ScrollState {
        &self.state
    }

    pub fn config(&self) -> &ControllerConfig {
        &self.cfg
    }

    pub fn set_config(&mut self, cfg: ControllerConfig) -> Vec<ScrollIntent> {
        let speed_changed = cfg.base_speed != self.cfg.base_speed;
        self.cfg = cfg;
        if speed_changed {
            if let ScrollState::Cruise { .. } = self.state {
                return self.go(ScrollState::Cruise { speed: self.cfg.base_speed });
            }
        }
        Vec::new()
    }

    /// The intent that makes the UI execute `state`, if any.
    fn intent_for(&self, state: &ScrollState) -> Option<ScrollIntent> {
        match state {
            ScrollState::Idle => None,
            ScrollState::Cruise { speed } => Some(ScrollIntent::Cruise { speed: *speed }),
            ScrollState::Hold => Some(ScrollIntent::Hold),
            ScrollState::Paused { .. } => Some(ScrollIntent::Pause),
            ScrollState::Gliding { target, profile, then, seq } => Some(ScrollIntent::GlideTo {
                pos: *target,
                profile: *profile,
                then: *then,
                seq: *seq,
            }),
        }
    }

    fn go(&mut self, next: ScrollState) -> Vec<ScrollIntent> {
        self.state = next;
        self.intent_for(&self.state).into_iter().collect()
    }

    fn cruise(&self) -> ScrollState {
        ScrollState::Cruise { speed: self.cfg.base_speed }
    }

    fn after_glide_state(&self, then: AfterGlide) -> ScrollState {
        match then {
            AfterGlide::Cruise if !self.in_talking_points => self.cruise(),
            _ => ScrollState::Hold,
        }
    }

    pub fn handle(&mut self, input: ControllerInput) -> Vec<ScrollIntent> {
        use ControllerInput as I;
        use ScrollState as S;

        // User actions apply in every state.
        match input {
            I::UserPause => {
                return match std::mem::replace(&mut self.state, S::Idle) {
                    S::Paused { resume_to } => {
                        let next = *resume_to;
                        self.go(next)
                    }
                    other => self.go(S::Paused { resume_to: Box::new(other) }),
                };
            }
            I::UserStart => return self.go(self.cruise()),
            I::UserHold => return self.go(S::Hold),
            I::UserSpeed(speed) => {
                self.cfg.base_speed = speed;
                return match self.state {
                    S::Cruise { .. } => self.go(S::Cruise { speed }),
                    _ => Vec::new(),
                };
            }
            I::UserJump(pos) => {
                let mut out = vec![ScrollIntent::Snap { pos }];
                out.extend(self.go(self.cruise()));
                return out;
            }
            I::UserJumpHold(pos) => {
                let mut out = vec![ScrollIntent::Snap { pos }];
                out.extend(self.go(S::Hold));
                return out;
            }
            I::UserReset => {
                self.in_talking_points = false;
                let mut out = vec![ScrollIntent::Snap { pos: DocPos::start_of(BlockId(0)) }];
                out.extend(self.go(S::Idle));
                return out;
            }
            _ => {}
        }

        // Track where the read line is; it affects what happens after a glide.
        match input {
            I::EnteredTalkingPoints => self.in_talking_points = true,
            I::LeftTalkingPoints => self.in_talking_points = false,
            _ => {}
        }

        match (&self.state, input) {
            (S::Paused { .. }, _) => Vec::new(),
            (S::Idle, I::SpeechResumed) => Vec::new(),
            (S::Idle | S::Cruise { .. } | S::Hold | S::Gliding { .. }, I::Estimate { target, profile }) => {
                self.seq += 1;
                let then = self.cfg.after_glide;
                self.go(S::Gliding { target, profile, then, seq: self.seq })
            }
            (S::Gliding { seq, then, .. }, I::Arrived { seq: arrived }) if *seq == arrived => {
                let next = self.after_glide_state(*then);
                self.go(next)
            }
            (S::Gliding { .. }, I::Arrived { .. }) => Vec::new(),
            (S::Cruise { .. }, I::Silence(d)) if d >= self.cfg.silence_hold => self.go(S::Hold),
            (S::Cruise { .. }, I::PaceOverrun) => self.go(S::Hold),
            (S::Cruise { .. }, I::EnteredTalkingPoints) => self.go(S::Hold),
            (S::Hold, I::SpeechResumed) if !self.in_talking_points => self.go(self.cruise()),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ControllerInput as I;
    use ScrollIntent as O;

    fn ctl() -> ScrollController {
        ScrollController::new(ControllerConfig::default())
    }

    fn glide(target: u32, profile: GlideProfile, seq: u64) -> O {
        O::GlideTo { pos: DocPos::start_of(BlockId(target)), profile, then: AfterGlide::Cruise, seq }
    }

    /// Table-driven: (input, expected intents) applied in order on one controller.
    fn run(c: &mut ScrollController, steps: &[(I, Vec<O>)]) {
        for (i, (input, expected)) in steps.iter().enumerate() {
            let got = c.handle(input.clone());
            assert_eq!(&got, expected, "step {i}: {input:?} in state {:?}", c.state());
        }
    }

    #[test]
    fn start_pause_resume() {
        let mut c = ctl();
        run(
            &mut c,
            &[
                (I::UserStart, vec![O::Cruise { speed: 2.0 }]),
                (I::UserPause, vec![O::Pause]),
                (I::Silence(Duration::from_secs(9)), vec![]),
                (I::UserPause, vec![O::Cruise { speed: 2.0 }]),
            ],
        );
    }

    #[test]
    fn estimate_glides_then_cruises_on_arrival() {
        let mut c = ctl();
        run(
            &mut c,
            &[
                (I::UserStart, vec![O::Cruise { speed: 2.0 }]),
                (I::Estimate { target: DocPos::start_of(BlockId(3)), profile: GlideProfile::Near }, vec![glide(3, GlideProfile::Near, 1)]),
                (I::Arrived { seq: 7 }, vec![]),
                (I::Arrived { seq: 1 }, vec![O::Cruise { speed: 2.0 }]),
            ],
        );
    }

    #[test]
    fn new_estimate_supersedes_running_glide() {
        let mut c = ctl();
        run(
            &mut c,
            &[
                (I::Estimate { target: DocPos::start_of(BlockId(1)), profile: GlideProfile::Near }, vec![glide(1, GlideProfile::Near, 1)]),
                (I::Estimate { target: DocPos::start_of(BlockId(4)), profile: GlideProfile::Skip }, vec![glide(4, GlideProfile::Skip, 2)]),
                // Arrival of the old glide is ignored.
                (I::Arrived { seq: 1 }, vec![]),
                (I::Arrived { seq: 2 }, vec![O::Cruise { speed: 2.0 }]),
            ],
        );
    }

    #[test]
    fn hold_mode_after_glide() {
        let mut c = ScrollController::new(ControllerConfig { after_glide: AfterGlide::Hold, ..Default::default() });
        let out = c.handle(I::Estimate { target: DocPos::start_of(BlockId(2)), profile: GlideProfile::Far });
        assert!(matches!(out[0], O::GlideTo { then: AfterGlide::Hold, .. }));
        assert_eq!(c.handle(I::Arrived { seq: 1 }), vec![O::Hold]);
        assert_eq!(c.handle(I::SpeechResumed), vec![O::Cruise { speed: 2.0 }]);
    }

    #[test]
    fn silence_and_pace_stop_cruise_and_speech_resumes_it() {
        let mut c = ctl();
        run(
            &mut c,
            &[
                (I::UserStart, vec![O::Cruise { speed: 2.0 }]),
                (I::Silence(Duration::from_secs(1)), vec![]),
                (I::Silence(Duration::from_millis(2500)), vec![O::Hold]),
                (I::SpeechResumed, vec![O::Cruise { speed: 2.0 }]),
                (I::PaceOverrun, vec![O::Hold]),
                (I::PaceOverrun, vec![]),
                (I::SpeechResumed, vec![O::Cruise { speed: 2.0 }]),
            ],
        );
    }

    #[test]
    fn talking_points_hold_until_next_anchor() {
        let mut c = ctl();
        run(
            &mut c,
            &[
                (I::UserStart, vec![O::Cruise { speed: 2.0 }]),
                (I::EnteredTalkingPoints, vec![O::Hold]),
                // Talking while in the ad-lib zone must not restart the scroll.
                (I::SpeechResumed, vec![]),
                (I::Estimate { target: DocPos::start_of(BlockId(5)), profile: GlideProfile::Near }, vec![glide(5, GlideProfile::Near, 1)]),
                (I::LeftTalkingPoints, vec![]),
                (I::Arrived { seq: 1 }, vec![O::Cruise { speed: 2.0 }]),
            ],
        );
    }

    #[test]
    fn arriving_inside_talking_points_holds_even_in_cruise_mode() {
        let mut c = ctl();
        c.handle(I::Estimate { target: DocPos::start_of(BlockId(2)), profile: GlideProfile::Near });
        c.handle(I::EnteredTalkingPoints);
        assert_eq!(c.handle(I::Arrived { seq: 1 }), vec![O::Hold]);
    }

    #[test]
    fn user_jump_snaps_then_cruises_and_reset_snaps_to_top() {
        let mut c = ctl();
        let pos = DocPos { block: BlockId(4), frac: 0.5 };
        assert_eq!(c.handle(I::UserJump(pos)), vec![O::Snap { pos }, O::Cruise { speed: 2.0 }]);
        assert_eq!(c.handle(I::UserReset), vec![O::Snap { pos: DocPos::start_of(BlockId(0)) }]);
        assert_eq!(c.state(), &ScrollState::Idle);
    }

    #[test]
    fn jump_hold_snaps_and_holds() {
        let mut c = ctl();
        c.handle(I::UserStart);
        let pos = DocPos::start_of(BlockId(3));
        assert_eq!(c.handle(I::UserJumpHold(pos)), vec![O::Snap { pos }, O::Hold]);
        assert_eq!(c.handle(I::SpeechResumed), vec![O::Cruise { speed: 2.0 }]);
    }

    #[test]
    fn speed_change_applies_immediately_only_while_cruising() {
        let mut c = ctl();
        assert_eq!(c.handle(I::UserSpeed(3.0)), vec![]);
        assert_eq!(c.handle(I::UserStart), vec![O::Cruise { speed: 3.0 }]);
        assert_eq!(c.handle(I::UserSpeed(1.5)), vec![O::Cruise { speed: 1.5 }]);
        assert_eq!(c.handle(I::UserHold), vec![O::Hold]);
        assert_eq!(c.handle(I::UserSpeed(2.5)), vec![]);
    }

    #[test]
    fn speech_never_produces_snap() {
        let mut c = ctl();
        let inputs = [
            I::UserStart,
            I::Estimate { target: DocPos::start_of(BlockId(9)), profile: GlideProfile::Far },
            I::Silence(Duration::from_secs(5)),
            I::SpeechResumed,
            I::PaceOverrun,
            I::Arrived { seq: 1 },
            I::EnteredTalkingPoints,
            I::LeftTalkingPoints,
        ];
        for i in inputs {
            for intent in c.handle(i) {
                assert!(!matches!(intent, O::Snap { .. }));
            }
        }
    }

    #[test]
    fn intents_serialize_for_the_ui() {
        let json = serde_json::to_string(&glide(3, GlideProfile::Skip, 4)).unwrap();
        assert_eq!(json, r#"{"type":"glide_to","pos":{"block":3,"frac":0.0},"profile":"skip","then":"cruise","seq":4}"#);
        assert_eq!(serde_json::to_string(&O::Cruise { speed: 2.0 }).unwrap(), r#"{"type":"cruise","speed":2.0}"#);
    }
}
