//! Spectrum analysis for the pages: a Hann-windowed FFT of the newest samples, read out on
//! log-spaced bands with fractional-octave smoothing and a falling decay.

#![allow(dead_code)] // the pages' toolbox: not every page uses every piece

use std::f32::consts::PI;

/// In-place radix-2 FFT of `re` + i`im` (length a power of two).
pub fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    debug_assert!(n.is_power_of_two() && im.len() == n);
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let step = -2.0 * PI / len as f32;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (s, c) = (step * k as f32).sin_cos();
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

/// A spectrum on `bands` log-spaced frequencies from `lo` to `hi` Hz, in dB (a sine at full
/// scale reads about 0 dB), smoothed over `1/smooth` octave and falling at `decay_db_s`.
pub struct Spectrum {
    pub lo: f32,
    pub hi: f32,
    /// Band levels in dB, low to high.
    pub db: Vec<f32>,
    window: Vec<f32>,
    re: Vec<f32>,
    im: Vec<f32>,
    smooth: f32,
    decay_db_s: f32,
}

pub const SPECTRUM_FLOOR: f32 = -120.0;

impl Spectrum {
    pub fn new(size: usize, bands: usize, lo: f32, hi: f32) -> Self {
        let size = size.next_power_of_two();
        let window = (0..size).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / size as f32).cos()).collect();
        Self { lo, hi, db: vec![SPECTRUM_FLOOR; bands], window, re: vec![0.0; size], im: vec![0.0; size], smooth: 6.0, decay_db_s: 40.0 }
    }

    /// No peak hold: every update reads the newest window as it is (for comparing two spectra).
    pub fn instant(mut self) -> Self {
        self.decay_db_s = f32::INFINITY;
        self
    }

    /// The frequency of band `i`.
    pub fn freq(&self, i: usize) -> f32 {
        let n = self.db.len().max(2) - 1;
        self.lo * (self.hi / self.lo).powf(i as f32 / n as f32)
    }

    /// Takes the newest samples (L and R are averaged when both given) and updates the bands.
    pub fn update(&mut self, left: &[f32], right: Option<&[f32]>, sample_rate: f32, dt: f32) {
        let size = self.re.len();
        let take = left.len().min(size);
        let off = left.len() - take;
        for i in 0..size {
            let s = if i < size - take {
                0.0
            } else {
                let j = off + i - (size - take);
                match right {
                    Some(r) => 0.5 * (left[j] + r.get(j).copied().unwrap_or(0.0)),
                    None => left[j],
                }
            };
            self.re[i] = s * self.window[i];
            self.im[i] = 0.0;
        }
        fft(&mut self.re, &mut self.im);
        // Power per bin, scaled so a full-scale sine reads 0 dB (Hann has gain 0.5).
        let norm = 16.0 / (size as f32 * size as f32);
        let half = size / 2;
        let bin_hz = sample_rate / size as f32;
        let fall = self.decay_db_s * dt.max(0.0);
        for b in 0..self.db.len() {
            let f = self.freq(b);
            let width = 2f32.powf(0.5 / self.smooth);
            let (lo, hi) = (((f / width) / bin_hz).floor() as usize, ((f * width) / bin_hz).ceil() as usize);
            let (lo, hi) = (lo.clamp(1, half - 1), hi.clamp(1, half - 1));
            let mut p = 0.0f32;
            for k in lo..=hi.max(lo) {
                p = p.max(self.re[k] * self.re[k] + self.im[k] * self.im[k]);
            }
            let level = (10.0 * (p * norm).max(1e-14).log10()).max(SPECTRUM_FLOOR);
            let old = self.db[b];
            self.db[b] = if level >= old { level } else { (old - fall).max(level) };
        }
    }

    pub fn clear(&mut self) {
        self.db.fill(SPECTRUM_FLOOR);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_scale_sine_reads_near_zero_at_its_frequency() {
        let sr = 48_000.0;
        let x: Vec<f32> = (0..4096).map(|i| (2.0 * PI * 1000.0 * i as f32 / sr).sin()).collect();
        let mut s = Spectrum::new(4096, 120, 20.0, 20_000.0);
        s.update(&x, None, sr, 0.0);
        let (i, peak) = s.db.iter().enumerate().fold((0, f32::MIN), |m, (i, &v)| if v > m.1 { (i, v) } else { m });
        assert!((s.freq(i) / 1000.0 - 1.0).abs() < 0.1, "peak at {} Hz", s.freq(i));
        assert!(peak > -2.0 && peak < 1.0, "peak {peak} dB");
        let far = s.db[s.db.len() - 5];
        assert!(far < -60.0, "far band {far}");
    }
}
