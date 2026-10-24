//! Pace model (drift guard). See `docs/ARCHITECTURE.md` 2.4.

use std::time::Duration;

const SEED_WPM: f32 = 140.0;
const ALPHA: f32 = 0.3;
const MIN_WPM: f32 = 60.0;
const MAX_WPM: f32 = 300.0;

#[derive(Debug, Clone)]
pub struct PaceModel {
    wpm_ewma: f32,
}

impl Default for PaceModel {
    fn default() -> Self {
        PaceModel { wpm_ewma: SEED_WPM }
    }
}

impl PaceModel {
    pub fn wpm(&self) -> f32 {
        self.wpm_ewma
    }

    /// How long reading `words_between` words should take at the current pace.
    pub fn expected(&self, words_between: usize) -> Duration {
        Duration::from_secs_f32(words_between as f32 / self.wpm_ewma * 60.0)
    }

    /// Learn from an anchor-to-anchor timing.
    pub fn observe(&mut self, words: usize, elapsed: Duration) {
        if words == 0 || elapsed.is_zero() {
            return;
        }
        let wpm = (words as f32 / elapsed.as_secs_f32() * 60.0).clamp(MIN_WPM, MAX_WPM);
        self.wpm_ewma = self.wpm_ewma * (1.0 - ALPHA) + wpm * ALPHA;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_at_140_wpm() {
        let p = PaceModel::default();
        assert_eq!(p.expected(140), Duration::from_secs(60));
    }

    #[test]
    fn learns_towards_observed_pace_and_clamps() {
        let mut p = PaceModel::default();
        p.observe(100, Duration::from_secs(30)); // 200 wpm
        assert!(p.wpm() > 140.0 && p.wpm() < 200.0);
        for _ in 0..50 {
            p.observe(1000, Duration::from_secs(1)); // absurd, clamps to 300
        }
        assert!((p.wpm() - 300.0).abs() < 1.0);
        p.observe(0, Duration::from_secs(5));
        assert!((p.wpm() - 300.0).abs() < 1.0);
    }
}
