//! Auto Attack / Auto Release: program-dependent timing, as in Harmonic Compressor.
//!
//! A slow average (about 70 ms) of how much gain the section is changing sets a factor
//! `a = average / 6 dB`, clamped to 0..1. The knob times are then scaled: release from 0.4x
//! (light work, recover fast) to 2x (heavy work, recover slowly so it does not pump), attack
//! from 0.3x to 1.2x. On a steady tone the average is steady, so the times are too and the
//! gain stays as clean as with fixed times. The scales move every 16 samples.

use crate::parts::one_pole;

const AVG_MS: f64 = 70.0;
const FULL_DB: f64 = 6.0;
const EVERY: u32 = 16;

/// Attack scale at a factor `a` (0..1).
pub fn attack_scale(a: f64) -> f64 {
    0.3 + 0.9 * a.clamp(0.0, 1.0)
}

/// Release scale at a factor `a` (0..1).
pub fn release_scale(a: f64) -> f64 {
    0.4 + 1.6 * a.clamp(0.0, 1.0)
}

#[derive(Clone, Copy)]
pub struct AutoTime {
    avg: f64,
    k: f64,
    tick: u32,
}

impl Default for AutoTime {
    fn default() -> Self {
        Self { avg: 0.0, k: 1.0, tick: 0 }
    }
}

impl AutoTime {
    pub fn set_rate(&mut self, sr: f64) {
        self.k = one_pole(AVG_MS * 1e-3, sr);
    }

    pub fn clear(&mut self) {
        self.avg = 0.0;
        self.tick = 0;
    }

    /// Feeds this sample's gain change (dB, either sign). True when the scales should be read
    /// again (every 16 samples).
    #[inline]
    pub fn push(&mut self, change_db: f64) -> bool {
        self.avg += (change_db.abs() - self.avg) * self.k;
        self.tick = self.tick.wrapping_add(1);
        self.tick % EVERY == 0
    }

    pub fn factor(&self) -> f64 {
        (self.avg / FULL_DB).clamp(0.0, 1.0)
    }

    pub fn attack(&self) -> f64 {
        attack_scale(self.factor())
    }

    pub fn release(&self) -> f64 {
        release_scale(self.factor())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_span_the_harmonic_compressor_ranges() {
        assert_eq!((attack_scale(0.0), release_scale(0.0)), (0.3, 0.4));
        assert!((attack_scale(1.0) - 1.2).abs() < 1e-12 && (release_scale(1.0) - 2.0).abs() < 1e-12);
        let mut t = AutoTime::default();
        t.set_rate(48_000.0);
        for _ in 0..48_000 {
            t.push(-12.0);
        }
        assert!((t.release() - 2.0).abs() < 1e-9);
        for _ in 0..48_000 {
            t.push(-3.0);
        }
        assert!((t.factor() - 0.5).abs() < 1e-6);
    }
}
