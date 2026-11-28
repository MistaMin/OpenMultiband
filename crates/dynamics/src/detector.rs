//! The key path shared by the gate and the compressor: key filters, rectifier, dB, link.

use crate::lin_to_db;
use crate::parts::{one_pole, Biquad, SlideMax};

pub const DETECTORS: &[&str] = &["Peak", "RMS"];
pub const KEY_SOURCES: &[&str] = &["Internal", "External"];
/// The key high-pass is off at the bottom of its range, the low-pass at the top.
pub const KEY_HP_OFF_HZ: f32 = 10.0;
pub const KEY_LP_OFF_HZ: f32 = 22_000.0;
/// The rectified level is held at its maximum over this long: half a period of 25 Hz. For a
/// steady tone above 25 Hz the level, and so the gain, is then constant: the gain does not
/// ripple with the waveform, which is what makes a plain peak detector distort.
pub const LEVEL_HOLD_MS: f64 = 20.0;

#[derive(Default)]
pub struct Detector {
    hp: [Biquad; 2],
    lp: [Biquad; 2],
    boost: crate::keyboost::KeyBoost,
    ms: [f64; 2],
    hold: [SlideMax; 2],
    hold_len: usize,
    rms: bool,
    rms_coef: f64,
    link: f64,
}

impl Detector {
    /// Allocates the level hold for this rate.
    pub fn alloc(&mut self, sr: f64) {
        self.hold_len = (LEVEL_HOLD_MS * 1e-3 * sr).round().max(1.0) as usize;
        for h in &mut self.hold {
            h.alloc(self.hold_len + 4);
        }
    }

    pub fn set(&mut self, sr: f64, rms: bool, window_ms: f64, hp_hz: f32, lp_hz: f32, link_pct: f64) {
        for c in 0..2 {
            self.hp[c].design(true, hp_hz as f64, sr, hp_hz > KEY_HP_OFF_HZ * 1.001);
            self.lp[c].design(false, lp_hz as f64, sr, lp_hz < KEY_LP_OFF_HZ * 0.999);
        }
        self.rms = rms;
        self.rms_coef = one_pole(window_ms * 1e-3, sr);
        self.link = (link_pct / 100.0).clamp(0.0, 1.0);
    }

    /// The key's bell band (see [`crate::keyboost`]).
    pub fn set_boost(&mut self, sr: f64, freq: f32, gain_db: f32, q: f32) {
        self.boost.set(freq, gain_db, q, sr);
    }

    pub fn clear(&mut self) {
        for c in 0..2 {
            self.hp[c].clear();
            self.lp[c].clear();
        }
        self.boost.clear();
        self.ms = [0.0; 2];
        for h in &mut self.hold {
            h.clear();
        }
    }

    /// Levels in dBFS for one key sample pair, after the stereo link (blended in dB toward
    /// the louder channel).
    #[inline]
    pub fn levels(&mut self, kl: f64, kr: f64) -> [f64; 2] {
        let mut l = [0.0; 2];
        for (c, k) in [kl, kr].into_iter().enumerate() {
            // A NaN or Inf key sample would poison the filter state and the gain follower for good.
            let k = if k.is_finite() { k } else { 0.0 };
            let v = self.boost.run(c, self.lp[c].run(self.hp[c].run(k)));
            let a = if self.rms {
                self.ms[c] += (v * v - self.ms[c]) * self.rms_coef;
                if self.ms[c] < 1e-30 {
                    self.ms[c] = 0.0;
                }
                self.ms[c].sqrt()
            } else {
                v.abs()
            };
            l[c] = lin_to_db(self.hold[c].push(a, self.hold_len));
        }
        let top = l[0].max(l[1]);
        [l[0] + (top - l[0]) * self.link, l[1] + (top - l[1]) * self.link]
    }
}

/// One-pole on the gain in dB with separate coefficients for falling and rising gain.
#[derive(Clone, Copy, Default)]
pub struct GainFollower {
    pub g: f64,
}

impl GainFollower {
    /// `down`: coefficient while the gain falls, `up` while it rises.
    #[inline]
    pub fn step(&mut self, target: f64, down: f64, up: f64) -> f64 {
        if !target.is_finite() {
            return self.g;
        }
        let a = if target < self.g { down } else { up };
        self.g += (target - self.g) * a;
        if (self.g - target).abs() < 1e-12 {
            self.g = target;
        }
        self.g
    }
}
