//! The key's bell band: boost (or cut) one frequency of the detector's input, after its
//! high- and low-pass, so a section can listen harder to a kick, a voice's esses or a snare.
//! RBJ peaking biquad, stereo. At 0 dB it is off and the key passes untouched.

/// Parameter ranges shared by every keyed section.
pub const FREQ_MIN: f32 = 20.0;
pub const FREQ_MAX: f32 = 20_000.0;
pub const FREQ_DEFAULT: f32 = 1_000.0;
pub const GAIN_MIN: f32 = -24.0;
pub const GAIN_MAX: f32 = 24.0;
pub const Q_MIN: f32 = 0.3;
pub const Q_MAX: f32 = 10.0;
pub const Q_DEFAULT: f32 = 1.0;

#[derive(Clone, Copy, Default)]
pub struct KeyBoost {
    on: bool,
    b: [f64; 3],
    a: [f64; 2],
    s: [[f64; 2]; 2],
}

impl KeyBoost {
    pub fn set(&mut self, freq: f32, gain_db: f32, q: f32, sr: f64) {
        self.on = gain_db.abs() > 1e-3;
        if !self.on {
            return;
        }
        let f = (freq as f64).clamp(10.0, 0.45 * sr);
        let a = 10f64.powf(gain_db as f64 / 40.0);
        let w = std::f64::consts::TAU * f / sr;
        let (sn, cs) = w.sin_cos();
        let alpha = sn / (2.0 * (q as f64).max(0.05));
        let a0 = 1.0 + alpha / a;
        self.b = [(1.0 + alpha * a) / a0, -2.0 * cs / a0, (1.0 - alpha * a) / a0];
        self.a = [-2.0 * cs / a0, (1.0 - alpha / a) / a0];
    }

    pub fn clear(&mut self) {
        self.s = [[0.0; 2]; 2];
    }

    #[inline]
    pub fn run(&mut self, c: usize, x: f64) -> f64 {
        if !self.on {
            return x;
        }
        let s = &mut self.s[c & 1];
        let y = self.b[0] * x + s[0];
        s[0] = self.b[1] * x - self.a[0] * y + s[1];
        s[1] = self.b[2] * x - self.a[1] * y;
        if s[0].abs() < 1e-30 {
            s[0] = 0.0;
        }
        if s[1].abs() < 1e-30 {
            s[1] = 0.0;
        }
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gain_at(b: &mut KeyBoost, f: f64, sr: f64) -> f64 {
        b.clear();
        let mut peak = 0.0f64;
        for i in 0..(sr as usize) {
            let y = b.run(0, (std::f64::consts::TAU * f * i as f64 / sr).sin());
            if i > sr as usize / 2 {
                peak = peak.max(y.abs());
            }
        }
        20.0 * peak.log10()
    }

    #[test]
    fn boosts_its_frequency_and_leaves_the_rest() {
        let sr = 48_000.0;
        let mut b = KeyBoost::default();
        b.set(2_000.0, 12.0, 2.0, sr);
        assert!((gain_at(&mut b, 2_000.0, sr) - 12.0).abs() < 0.1);
        assert!(gain_at(&mut b, 100.0, sr).abs() < 0.2);
        assert!(gain_at(&mut b, 15_000.0, sr).abs() < 0.5);
        b.set(2_000.0, 0.0, 2.0, sr);
        assert_eq!(b.run(0, 0.123), 0.123);
    }
}
