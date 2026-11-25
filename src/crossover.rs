//! Minimum-phase all-pass-matched Linkwitz-Riley crossover for 1 to 6 stereo bands.
//!
//! Supports three Linkwitz-Riley slopes, all built from trapezoidal (TPT / Cytomic) SVFs in f64
//! so real-time frequency automation stays unconditionally stable and click-free:
//!
//! * **12 dB/oct (LR2):** two 1-pole TPT stages per LP/HP, HP polarity-inverted so
//!   `LP^2 - HP^2 == AP1` identically in the z-domain.
//! * **24 dB/oct (LR4):** two 2-pole Butterworth SVFs (`k = sqrt(2)`), `LP2^2 + HP2^2 == AP2`.
//! * **48 dB/oct (LR8):** two 4-pole Butterworth cascades (`k1 = 2 cos(pi/8)`, `k2 = 2 cos(3pi/8)`),
//!   `LP4^2 + HP4^2 == AP2(k1) * AP2(k2)`.
//!
//! For `N >= 3` bands peeled bottom-to-top at sorted frequencies `f_0 < ... < f_{N-2}`, each lower
//! band `b < k` passes through the matched all-pass `AP(f_k)` of every higher split `k > b`.
//! By induction the sum of all bands equals `AP(f_{N-2}) ... AP(f_0) * x` to machine precision
//! (0.000 dB magnitude ripple everywhere, even when crossovers sit close together).
//!
//! A lightweight phase-reference chain runs the exact same `AP(f_0) ... AP(f_{N-2})` on the dry
//! signal so Delta mode (`wet - dry_ref`) nulls cleanly at unity even with 6 bands at 48 dB/oct.
#![forbid(unsafe_code)]

use std::f64::consts::{PI, SQRT_2};

pub const MAX_BANDS: usize = 6;
pub const MAX_SPLITS: usize = MAX_BANDS - 1;
pub const FREQ_MIN: f32 = 20.0;
pub const FREQ_MAX: f32 = 20_000.0;
pub const DEFAULT_FREQS: [f32; MAX_SPLITS] = [120.0, 750.0, 3000.0, 8000.0, 14_000.0];
pub const SLOPE_LABELS: &[&str] = &["12 dB", "24 dB", "48 dB"];

/// 4th-order Butterworth SVF dampings (`1/Q = 2 cos((2m+1) pi / 8)`).
const K8_A: f64 = 1.847_759_065_022_573_5;
const K8_B: f64 = 0.765_366_864_730_179_6;
const SMOOTH_MS: f64 = 5.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slope {
    Db12 = 0,
    Db24 = 1,
    Db48 = 2,
}

impl Slope {
    pub fn from_param(v: f32) -> Self {
        match v.round() as i32 {
            0 => Self::Db12,
            2 => Self::Db48,
            _ => Self::Db24,
        }
    }
}

/// 1-pole trapezoidal filter (used for LR2).
#[derive(Clone, Copy, Default)]
struct OnePole {
    s: f64,
}

impl OnePole {
    #[inline]
    fn lp(&mut self, x: f64, big_g: f64) -> f64 {
        let v = (x - self.s) * big_g;
        let y = v + self.s;
        self.s = y + v;
        y
    }

    #[inline]
    fn hp(&mut self, x: f64, big_g: f64) -> f64 {
        x - self.lp(x, big_g)
    }

    /// First-order all-pass `LP - HP`, matching `LP^2 - HP^2`.
    #[inline]
    fn ap(&mut self, x: f64, big_g: f64) -> f64 {
        2.0 * self.lp(x, big_g) - x
    }
}

/// 2-pole trapezoidal SVF (Cytomic / Simper), used for LR4 and LR8.
#[derive(Clone, Copy, Default)]
struct Svf2 {
    ic1: f64,
    ic2: f64,
}

#[derive(Clone, Copy)]
struct SvfCoefs {
    a1: f64,
    a2: f64,
    a3: f64,
    k: f64,
}

impl SvfCoefs {
    #[inline]
    fn new(g: f64, k: f64) -> Self {
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        Self { a1, a2, a3, k }
    }
}

impl Svf2 {
    #[inline]
    fn tick(&mut self, x: f64, c: SvfCoefs) -> (f64, f64, f64) {
        let v3 = x - self.ic2;
        let v1 = c.a1 * self.ic1 + c.a2 * v3;
        let v2 = self.ic2 + c.a2 * self.ic1 + c.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1, x - c.k * v1 - v2)
    }

    #[inline]
    fn lp(&mut self, x: f64, c: SvfCoefs) -> f64 {
        self.tick(x, c).0
    }

    #[inline]
    fn hp(&mut self, x: f64, c: SvfCoefs) -> f64 {
        self.tick(x, c).2
    }

    /// Second-order all-pass `LP - k*BP + HP = x - 2*k*BP`.
    #[inline]
    fn ap(&mut self, x: f64, c: SvfCoefs) -> f64 {
        let (_, bp, _) = self.tick(x, c);
        x - 2.0 * c.k * bp
    }
}

/// One two-way Linkwitz-Riley split (one channel).
#[derive(Clone, Copy, Default)]
struct SplitCell {
    lp1: [OnePole; 2],
    hp1: [OnePole; 2],
    lp2: [Svf2; 4],
    hp2: [Svf2; 4],
}

impl SplitCell {
    fn reset(&mut self) {
        *self = Self::default();
    }

    #[inline]
    fn run(&mut self, x: f64, slope: Slope, g: f64) -> (f64, f64) {
        match slope {
            Slope::Db12 => {
                let bg = g / (1.0 + g);
                let lo = self.lp1[0].lp(x, bg);
                let lo = self.lp1[1].lp(lo, bg);
                let hi = self.hp1[0].hp(x, bg);
                let hi = -self.hp1[1].hp(hi, bg);
                (lo, hi)
            }
            Slope::Db24 => {
                let c = SvfCoefs::new(g, SQRT_2);
                let lo = self.lp2[0].lp(x, c);
                let lo = self.lp2[1].lp(lo, c);
                let hi = self.hp2[0].hp(x, c);
                let hi = self.hp2[1].hp(hi, c);
                (lo, hi)
            }
            Slope::Db48 => {
                let ca = SvfCoefs::new(g, K8_A);
                let cb = SvfCoefs::new(g, K8_B);
                let lo = self.lp2[0].lp(x, ca);
                let lo = self.lp2[1].lp(lo, cb);
                let lo = self.lp2[2].lp(lo, ca);
                let lo = self.lp2[3].lp(lo, cb);
                let hi = self.hp2[0].hp(x, ca);
                let hi = self.hp2[1].hp(hi, cb);
                let hi = self.hp2[2].hp(hi, ca);
                let hi = self.hp2[3].hp(hi, cb);
                (lo, hi)
            }
        }
    }
}

/// One matched all-pass cell (one channel) matching a [`SplitCell`] at the same `g` and `slope`.
#[derive(Clone, Copy, Default)]
struct ApCell {
    p1: OnePole,
    s2: [Svf2; 2],
}

impl ApCell {
    fn reset(&mut self) {
        *self = Self::default();
    }

    #[inline]
    fn run(&mut self, x: f64, slope: Slope, g: f64) -> f64 {
        match slope {
            Slope::Db12 => self.p1.ap(x, g / (1.0 + g)),
            Slope::Db24 => self.s2[0].ap(x, SvfCoefs::new(g, SQRT_2)),
            Slope::Db48 => {
                let ca = SvfCoefs::new(g, K8_A);
                let cb = SvfCoefs::new(g, K8_B);
                let y = self.s2[0].ap(x, ca);
                self.s2[1].ap(y, cb)
            }
        }
    }
}

/// One mono N-band crossover channel plus its matched phase-reference chain.
#[derive(Clone, Copy, Default)]
struct Channel {
    splits: [SplitCell; MAX_SPLITS],
    /// `ap[b][k]` aligns band `b` with higher split `k` (used for `b < k`).
    ap: [[ApCell; MAX_SPLITS]; MAX_BANDS],
    /// Phase reference: `AP(f_0) * ... * AP(f_{splits-1})`, matching the sum of all bands.
    ref_ap: [ApCell; MAX_SPLITS],
}

impl Channel {
    fn reset(&mut self) {
        for s in &mut self.splits {
            s.reset();
        }
        for row in &mut self.ap {
            for a in row {
                a.reset();
            }
        }
        for a in &mut self.ref_ap {
            a.reset();
        }
    }
}

/// Stereo 1-to-6-band all-pass-matched Linkwitz-Riley crossover.
pub struct Crossover {
    sr: f64,
    bands: usize,
    slope: Slope,
    /// Target and smoothed `g = tan(pi * fc / sr)` for each active split (sorted low to high).
    g: [f64; MAX_SPLITS],
    g_target: [f64; MAX_SPLITS],
    raw_freqs: [f32; MAX_SPLITS],
    smooth: f64,
    ch: [Channel; 2],
}

impl Default for Crossover {
    fn default() -> Self {
        let mut x = Self {
            sr: 48_000.0,
            bands: 4,
            slope: Slope::Db24,
            g: [0.01; MAX_SPLITS],
            g_target: [0.01; MAX_SPLITS],
            raw_freqs: DEFAULT_FREQS,
            smooth: 1.0,
            ch: [Channel::default(); 2],
        };
        x.prepare(48_000.0);
        x
    }
}

fn freq_to_g(freq_hz: f32, sr: f64) -> f64 {
    let max_f = (sr * 0.45).max(40.0);
    let f = (freq_hz as f64).clamp(FREQ_MIN as f64, max_f);
    (PI * f / sr).tan()
}

impl Crossover {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.sr = (sample_rate as f64).max(1000.0);
        self.smooth = 1.0 - (-1.0 / (SMOOTH_MS * 1e-3 * self.sr)).exp();
        self.update_targets();
        self.g = self.g_target;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.ch[0].reset();
        self.ch[1].reset();
        self.g = self.g_target;
    }

    pub fn set_topology(&mut self, bands: usize, slope: Slope) {
        let bands = bands.clamp(1, MAX_BANDS);
        if bands != self.bands || slope != self.slope {
            self.bands = bands;
            self.slope = slope;
            self.update_targets();
            self.reset();
        }
    }

    pub fn set_freq(&mut self, split: usize, freq_hz: f32) {
        if split < MAX_SPLITS {
            self.raw_freqs[split] = freq_hz;
            self.update_targets();
        }
    }

    pub fn bands(&self) -> usize {
        self.bands
    }

    pub fn slope(&self) -> Slope {
        self.slope
    }

    /// Sorted active crossover frequencies in Hz (`bands - 1` entries).
    pub fn active_freqs(&self) -> Vec<f32> {
        let n = self.bands.saturating_sub(1);
        let mut f: Vec<f32> = self.raw_freqs[..n].to_vec();
        f.sort_by(|a, b| a.total_cmp(b));
        f
    }

    fn update_targets(&mut self) {
        let n = self.bands.saturating_sub(1);
        let mut sorted = [0.0f32; MAX_SPLITS];
        sorted[..n].copy_from_slice(&self.raw_freqs[..n]);
        sorted[..n].sort_by(|a, b| a.total_cmp(b));
        for k in 0..n {
            self.g_target[k] = freq_to_g(sorted[k], self.sr);
        }
    }

    /// Splits stereo `(in_l, in_r)` into `out_l[0..bands]` and `out_r[0..bands]`, and writes the
    /// phase-matched dry reference (`AP_total * in`) into `(ref_l, ref_r)`.
    ///
    /// For 1 band, copies input directly with zero filtering.
    pub fn split_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [Vec<f32>; MAX_BANDS],
        out_r: &mut [Vec<f32>; MAX_BANDS],
        ref_l: &mut [f32],
        ref_r: &mut [f32],
    ) {
        let n = in_l.len();
        if self.bands <= 1 {
            out_l[0][..n].copy_from_slice(in_l);
            out_r[0][..n].copy_from_slice(in_r);
            ref_l[..n].copy_from_slice(in_l);
            ref_r[..n].copy_from_slice(in_r);
            return;
        }
        let splits = self.bands - 1;
        let slope = self.slope;
        let smooth = self.smooth;
        for i in 0..n {
            for k in 0..splits {
                self.g[k] += (self.g_target[k] - self.g[k]) * smooth;
            }
            for c in 0..2 {
                let x = if c == 0 { in_l[i] as f64 } else { in_r[i] as f64 };
                let ch = &mut self.ch[c];
                let mut bands = [0.0f64; MAX_BANDS];
                let mut rem = x;
                for k in 0..splits {
                    let g = self.g[k];
                    let (lo, hi) = ch.splits[k].run(rem, slope, g);
                    bands[k] = lo;
                    // Phase-align all previously peeled lower bands with split `k`.
                    for b in 0..k {
                        bands[b] = ch.ap[b][k].run(bands[b], slope, g);
                    }
                    rem = hi;
                }
                bands[splits] = rem;
                let mut r = x;
                for k in 0..splits {
                    r = ch.ref_ap[k].run(r, slope, self.g[k]);
                }
                if c == 0 {
                    for b in 0..self.bands {
                        out_l[b][i] = bands[b] as f32;
                    }
                    ref_l[i] = r as f32;
                } else {
                    for b in 0..self.bands {
                        out_r[b][i] = bands[b] as f32;
                    }
                    ref_r[i] = r as f32;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sum_matches_ref(bands: usize, slope: Slope, sr: f32) {
        let mut xo = Crossover::default();
        xo.set_topology(bands, slope);
        xo.prepare(sr);
        let n = 4096;
        // Impulse + broadband multitone + noise-like chirp.
        let mut src = vec![0.0f32; n];
        src[0] = 1.0;
        let mut seed = 42u32;
        for (i, s) in src.iter_mut().enumerate().skip(1) {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
            let t = i as f32 / sr;
            *s = 0.25 * (2.0 * std::f32::consts::PI * 95.0 * t).sin()
                + 0.25 * (2.0 * std::f32::consts::PI * 1000.0 * t).sin()
                + 0.25 * (2.0 * std::f32::consts::PI * 6500.0 * t).sin()
                + 0.2 * noise;
        }
        let mut out_l: [Vec<f32>; MAX_BANDS] = std::array::from_fn(|_| vec![0.0; n]);
        let mut out_r: [Vec<f32>; MAX_BANDS] = std::array::from_fn(|_| vec![0.0; n]);
        let mut ref_l = vec![0.0f32; n];
        let mut ref_r = vec![0.0f32; n];
        xo.split_block(&src, &src, &mut out_l, &mut out_r, &mut ref_l, &mut ref_r);

        // 1. Sample-for-sample sum matches the all-pass phase reference (exact Delta null).
        let mut max_diff = 0.0f32;
        for i in 0..n {
            let sum: f32 = (0..bands).map(|b| out_l[b][i]).sum();
            max_diff = max_diff.max((sum - ref_l[i]).abs());
        }
        assert!(max_diff < 1e-6, "bands={bands} slope={slope:?} sr={sr}: sum vs ref diff {max_diff}");

        // 2. Flat magnitude response across the whole spectrum (0.00 dB within 0.01 dB).
        for &freq in &[40.0f32, 120.0, 350.0, 750.0, 1500.0, 3000.0, 8000.0, 14_000.0] {
            if freq >= sr * 0.42 {
                continue;
            }
            let mut xo2 = Crossover::default();
            xo2.set_topology(bands, slope);
            xo2.prepare(sr);
            let m = 16_384;
            let sine: Vec<f32> = (0..m).map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin()).collect();
            let mut ol: [Vec<f32>; MAX_BANDS] = std::array::from_fn(|_| vec![0.0; m]);
            let mut or: [Vec<f32>; MAX_BANDS] = std::array::from_fn(|_| vec![0.0; m]);
            let mut rl = vec![0.0; m];
            let mut rr = vec![0.0; m];
            xo2.split_block(&sine, &sine, &mut ol, &mut or, &mut rl, &mut rr);
            // Measure over an exact integer number of cycles after settling so phase shift
            // from the all-pass response does not bias a fractional-cycle RMS window.
            let cycles = ((m / 2) as f64 * freq as f64 / sr as f64).floor().max(1.0);
            let win = ((cycles * sr as f64 / freq as f64).round() as usize).min(m / 2);
            let start = m - win;
            let rms_in = (sine[start..m].iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / win as f64).sqrt();
            let rms_sum = ((start..m)
                .map(|i| {
                    let s: f64 = (0..bands).map(|b| ol[b][i] as f64).sum();
                    s * s
                })
                .sum::<f64>()
                / win as f64)
                .sqrt();
            let err_db = (20.0 * (rms_sum / rms_in).log10()).abs();
            assert!(err_db < 0.005, "bands={bands} slope={slope:?} freq={freq}: err {err_db:.4} dB");
        }
    }

    #[test]
    fn exact_sum_and_flat_magnitude_for_all_band_counts_and_slopes() {
        for slope in [Slope::Db12, Slope::Db24, Slope::Db48] {
            for bands in 1..=MAX_BANDS {
                sum_matches_ref(bands, slope, 48_000.0);
            }
        }
        sum_matches_ref(4, Slope::Db24, 96_000.0);
    }

    #[test]
    fn bands_isolate_their_frequency_ranges() {
        let sr = 48_000.0;
        let mut xo = Crossover::default();
        xo.set_topology(4, Slope::Db24);
        xo.prepare(sr);
        let m = 8192;
        let mut ol: [Vec<f32>; MAX_BANDS] = std::array::from_fn(|_| vec![0.0; m]);
        let mut or: [Vec<f32>; MAX_BANDS] = std::array::from_fn(|_| vec![0.0; m]);
        let mut rl = vec![0.0; m];
        let mut rr = vec![0.0; m];
        for (expected_band, freq) in [(0usize, 50.0f32), (1, 300.0), (2, 1500.0), (3, 10_000.0)] {
            xo.reset();
            let sine: Vec<f32> = (0..m).map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin()).collect();
            xo.split_block(&sine, &sine, &mut ol, &mut or, &mut rl, &mut rr);
            let energies: Vec<f64> = (0..4).map(|b| ol[b][m / 2..].iter().map(|v| (*v as f64).powi(2)).sum()).collect();
            let loudest = energies.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
            assert_eq!(loudest, expected_band, "freq={freq} Hz energies={energies:?}");
            // Out-of-band rejection well below in-band energy.
            let total: f64 = energies.iter().sum();
            assert!(energies[expected_band] / total > 0.95, "freq={freq}: share {}", energies[expected_band] / total);
        }
    }

    #[test]
    fn single_band_is_bit_exact_wire() {
        let mut xo = Crossover::default();
        xo.set_topology(1, Slope::Db48);
        xo.prepare(48_000.0);
        let src: Vec<f32> = (0..512).map(|i| (i as f32 * 0.13).sin() * 0.7).collect();
        let mut ol: [Vec<f32>; MAX_BANDS] = std::array::from_fn(|_| vec![0.0; 512]);
        let mut or: [Vec<f32>; MAX_BANDS] = std::array::from_fn(|_| vec![0.0; 512]);
        let mut rl = vec![0.0; 512];
        let mut rr = vec![0.0; 512];
        xo.split_block(&src, &src, &mut ol, &mut or, &mut rl, &mut rr);
        assert_eq!(&ol[0][..], &src[..]);
        assert_eq!(&rl[..], &src[..]);
    }

    #[test]
    fn rapid_frequency_automation_stays_finite_and_matched() {
        let mut xo = Crossover::default();
        xo.set_topology(6, Slope::Db48);
        xo.prepare(48_000.0);
        let n = 64;
        let mut ol: [Vec<f32>; MAX_BANDS] = std::array::from_fn(|_| vec![0.0; n]);
        let mut or: [Vec<f32>; MAX_BANDS] = std::array::from_fn(|_| vec![0.0; n]);
        let mut rl = vec![0.0; n];
        let mut rr = vec![0.0; n];
        for step in 0..200 {
            // Hostile: swap/cross frequencies deliberately.
            xo.set_freq(0, 100.0 + (step as f32) * 80.0);
            xo.set_freq(2, 5000.0 - (step as f32) * 20.0);
            let src: Vec<f32> = (0..n).map(|i| ((step * n + i) as f32 * 0.05).sin() * 0.5).collect();
            xo.split_block(&src, &src, &mut ol, &mut or, &mut rl, &mut rr);
            for i in 0..n {
                let sum: f32 = (0..6).map(|b| ol[b][i]).sum();
                assert!(sum.is_finite() && rl[i].is_finite());
                assert!((sum - rl[i]).abs() < 1e-4, "step {step} i {i}: sum={sum} ref={}", rl[i]);
            }
        }
    }
}
