//! Downward compressor: `ratio:1` above the threshold with a quadratic knee centred on it.
//!
//! Attack is the time constant of the gain falling, release of it rising, both on the gain in
//! dB, so a 10 ms attack takes the gain 63 % of the way to the curve in 10 ms at any level.
//! Look-ahead delays the audio and holds the deepest target over the window, so the gain is
//! already down when a transient arrives. Make-up is on the compressed signal; Mix blends it
//! with the delayed dry signal (parallel compression), sample-aligned.

use olm_api::{Kind, Meters, Module, ParamDesc};

use crate::curve;
use crate::keyboost;
use crate::detector::{Detector, GainFollower, DETECTORS, KEY_HP_OFF_HZ, KEY_LP_OFF_HZ, KEY_SOURCES};
use crate::auto::AutoTime;
use crate::parts::{one_pole, BoxAvg, Delay, SlideMax};
use crate::{clean, db_to_lin, view, FLOOR_DB};

pub mod index {
    pub const THRESHOLD: usize = 0;
    pub const RATIO: usize = 1;
    pub const KNEE: usize = 2;
    pub const ATTACK: usize = 3;
    pub const RELEASE: usize = 4;
    pub const DETECTOR: usize = 5;
    pub const RMS_WINDOW: usize = 6;
    pub const LOOKAHEAD: usize = 7;
    pub const STEREO_LINK: usize = 8;
    pub const KEY_SOURCE: usize = 9;
    pub const KEY_HIGH_PASS: usize = 10;
    pub const KEY_LOW_PASS: usize = 11;
    pub const MAKEUP: usize = 12;
    pub const MIX: usize = 13;
    pub const AUTO_ATTACK: usize = 14;
    pub const AUTO_RELEASE: usize = 15;
    pub const KEY_BOOST_FREQ: usize = 16;
    pub const KEY_BOOST: usize = 17;
    pub const KEY_BOOST_Q: usize = 18;
    pub const COUNT: usize = 19;
}
use index as ix;

pub const MAX_LOOKAHEAD_MS: f32 = 10.0;
const SMOOTH_MS: f64 = 5.0;

pub static PARAMS: [ParamDesc; ix::COUNT] = [
    ParamDesc::new("threshold", "Threshold", -60.0, 0.0, -18.0, Kind::Db),
    ParamDesc::new("ratio", "Ratio", 1.0, 30.0, 4.0, Kind::Ratio).log(),
    ParamDesc::new("knee", "Knee", 0.0, 24.0, 6.0, Kind::Db),
    ParamDesc::new("attack", "Attack", 0.01, 300.0, 10.0, Kind::Ms).log(),
    ParamDesc::new("release", "Release", 5.0, 5000.0, 120.0, Kind::Ms).log(),
    ParamDesc::choice("detector", "Detector", DETECTORS, 0),
    ParamDesc::new("rmsWindow", "RMS Window", 1.0, 300.0, 10.0, Kind::Ms).log(),
    ParamDesc::new("lookahead", "Look-Ahead", 0.0, MAX_LOOKAHEAD_MS, 2.0, Kind::Ms),
    ParamDesc::new("stereoLink", "Stereo Link", 0.0, 100.0, 100.0, Kind::Percent),
    ParamDesc::choice("keySource", "Key Source", KEY_SOURCES, 0),
    ParamDesc::new("keyHighPass", "Key High-Pass", KEY_HP_OFF_HZ, 2000.0, KEY_HP_OFF_HZ, Kind::Hz).log(),
    ParamDesc::new("keyLowPass", "Key Low-Pass", 500.0, KEY_LP_OFF_HZ, KEY_LP_OFF_HZ, Kind::Hz).log(),
    ParamDesc::new("makeup", "Make-Up", -12.0, 24.0, 0.0, Kind::Db),
    ParamDesc::new("mix", "Mix", 0.0, 100.0, 100.0, Kind::Percent),

    ParamDesc::toggle("autoAttack", "Auto Attack", false),
    ParamDesc::toggle("autoRelease", "Auto Release", false),
    ParamDesc::new("keyBoostFreq", "Key Boost Freq", keyboost::FREQ_MIN, keyboost::FREQ_MAX, keyboost::FREQ_DEFAULT, Kind::Hz).log(),
    ParamDesc::new("keyBoost", "Key Boost", keyboost::GAIN_MIN, keyboost::GAIN_MAX, 0.0, Kind::Db),
    ParamDesc::new("keyBoostQ", "Key Boost Q", keyboost::Q_MIN, keyboost::Q_MAX, keyboost::Q_DEFAULT, Kind::Plain).log(),
];

pub struct Compressor {
    v: [f32; ix::COUNT],
    sr: f64,
    det: Detector,
    gain: [GainFollower; 2],
    auto: AutoTime,
    hold: [SlideMax; 2],
    ramp: [BoxAvg; 2],
    delay: [Delay; 2],
    look: usize,
    zero: bool,
    att: f64,
    rel: f64,
    smooth: f64,
    wet: f64,
    dry: f64,
    view: [f32; view::LEN],
    reduction: f32,
}

impl Default for Compressor {
    fn default() -> Self {
        let mut s = Self {
            v: PARAMS.map(|p| p.default),
            sr: 48_000.0,
            det: Detector::default(),
            gain: Default::default(),
            auto: AutoTime::default(),
            hold: Default::default(),
            ramp: Default::default(),
            delay: Default::default(),
            look: 0,
            zero: false,
            att: 1.0,
            rel: 1.0,
            smooth: 1.0,
            wet: 1.0,
            dry: 0.0,
            view: [0.0; view::LEN],
            reduction: 0.0,
        };
        s.update();
        s
    }
}

impl Compressor {
    /// Attack and release coefficients from the knobs, scaled by Auto Attack / Auto Release.
    fn timing(&mut self) {
        let v = &self.v;
        let a = if v[ix::AUTO_ATTACK] >= 0.5 { self.auto.attack() } else { 1.0 };
        let r = if v[ix::AUTO_RELEASE] >= 0.5 { self.auto.release() } else { 1.0 };
        self.att = one_pole(v[ix::ATTACK] as f64 * a * 1e-3, self.sr);
        self.rel = one_pole(v[ix::RELEASE] as f64 * r * 1e-3, self.sr);
    }

    fn update(&mut self) {
        let v = self.v;
        self.det.set(self.sr, v[ix::DETECTOR] >= 0.5, v[ix::RMS_WINDOW] as f64, v[ix::KEY_HIGH_PASS], v[ix::KEY_LOW_PASS], v[ix::STEREO_LINK] as f64);
        self.timing();
        self.det.set_boost(self.sr, v[ix::KEY_BOOST_FREQ], v[ix::KEY_BOOST], v[ix::KEY_BOOST_Q]);
        self.look = if self.zero { 0 } else { (v[ix::LOOKAHEAD] as f64 * 1e-3 * self.sr).round() as usize };
        self.smooth = one_pole(SMOOTH_MS * 1e-3, self.sr);
    }

    fn run(&mut self, left: &mut [f32], right: &mut [f32], key: Option<(&[f32], &[f32])>) {
        let thr = self.v[ix::THRESHOLD] as f64;
        let ratio = self.v[ix::RATIO] as f64;
        let knee = self.v[ix::KNEE] as f64;
        let mix = self.v[ix::MIX] as f64 / 100.0;
        let wet_t = mix * db_to_lin(self.v[ix::MAKEUP] as f64);
        let dry_t = 1.0 - mix;
        let mut out = [0.0f64; 2];
        let mut lvl = [FLOOR_DB; 2];
        let auto = self.v[ix::AUTO_ATTACK] >= 0.5 || self.v[ix::AUTO_RELEASE] >= 0.5;
        let n = left.len().min(right.len());
        for i in 0..n {
            let mut work = 0.0f64;
            let (xl, xr) = (left[i] as f64, right[i] as f64);
            let (kl, kr) = match key {
                Some((a, b)) => (a.get(i).copied().unwrap_or(0.0) as f64, b.get(i).copied().unwrap_or(0.0) as f64),
                None => (xl, xr),
            };
            let levels = self.det.levels(kl, kr);
            self.wet += (wet_t - self.wet) * self.smooth;
            self.dry += (dry_t - self.dry) * self.smooth;
            let x = [xl, xr];
            let mut y = [0.0; 2];
            for c in 0..2 {
                let target = curve::compressor(levels[c], thr, ratio, knee);
                // Look-ahead: hold the deepest target over the window, then ramp to it in a
                // straight line that lands on the onset sample. The gain is already there when
                // the transient arrives, without having to jump.
                let w = self.look + 1;
                let target = self.ramp[c].push(-self.hold[c].push(-target, w), w);
                let g = self.gain[c].step(target, self.att, self.rel);
                work = work.min(g);
                let d = self.delay[c].push(x[c] as f32, self.look) as f64;
                y[c] = d * (self.dry + self.wet * db_to_lin(g));
                out[c] = out[c].min(g);
                lvl[c] = lvl[c].max(levels[c]);
            }
            left[i] = clean(y[0]);
            right[i] = clean(y[1]);
            if auto && self.auto.push(work) {
                self.timing();
            }
        }
        self.view[view::GAIN] = out[0] as f32;
        self.view[view::GAIN + 1] = out[1] as f32;
        self.view[view::DETECTOR] = lvl[0] as f32;
        self.view[view::DETECTOR + 1] = lvl[1] as f32;
        self.reduction = (-out[0].min(out[1])) as f32;
    }
}

impl Module for Compressor {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.sr = sample_rate.max(1.0) as f64;
        let len = (MAX_LOOKAHEAD_MS as f64 * 1e-3 * self.sr).ceil() as usize + 4;
        for c in 0..2 {
            self.delay[c].alloc(len);
            self.hold[c].alloc(len);
            self.ramp[c].alloc(len);
        }
        self.det.alloc(self.sr);
        self.auto.set_rate(self.sr);
        self.update();
        self.reset();
    }

    fn reset(&mut self) {
        self.det.clear();
        self.auto.clear();
        self.timing();
        for c in 0..2 {
            self.gain[c] = GainFollower::default();
            self.hold[c].clear();
            self.ramp[c].clear();
            self.delay[c].clear();
        }
        let mix = self.v[ix::MIX] as f64 / 100.0;
        self.wet = mix * db_to_lin(self.v[ix::MAKEUP] as f64);
        self.dry = 1.0 - mix;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let Some(p) = PARAMS.get(index) else { return };
        self.v[index] = if value.is_finite() { value.clamp(p.min, p.max) } else { p.default };
        self.update();
    }

    fn latency(&self) -> usize {
        self.look
    }

    fn set_zero_latency(&mut self, on: bool) {
        self.zero = on;
        self.update();
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.run(left, right, None);
    }

    fn process_keyed(&mut self, left: &mut [f32], right: &mut [f32], key_left: &[f32], key_right: &[f32]) {
        let external = self.v[ix::KEY_SOURCE] >= 0.5;
        self.run(left, right, external.then_some((key_left, key_right)));
    }

    fn meters(&self) -> Meters {
        Meters { reduction_db: self.reduction, ..Meters::default() }
    }

    fn view(&self, out: &mut [f32]) -> usize {
        let n = out.len().min(view::LEN);
        out[..n].copy_from_slice(&self.view[..n]);
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(db: f64, n: usize, f: f64, sr: f64) -> Vec<f32> {
        let a = db_to_lin(db);
        (0..n).map(|i| (a * (std::f64::consts::TAU * f * i as f64 / sr).sin()) as f32).collect()
    }

    fn peak_db(x: &[f32]) -> f64 {
        crate::lin_to_db(x.iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64)
    }

    /// The settled output level of a steady tone sits on the static curve (RMS detector,
    /// long window, so the gain carries no ripple), for every ratio.
    #[test]
    fn settled_level_is_on_the_curve() {
        let sr = 48_000.0;
        for ratio in [2.0f32, 4.0, 10.0] {
            let mut c = Compressor::default();
            c.prepare(sr as f32, 4096);
            for (i, v) in [(ix::THRESHOLD, -30.0), (ix::RATIO, ratio), (ix::KNEE, 0.0), (ix::DETECTOR, 1.0), (ix::RMS_WINDOW, 100.0), (ix::ATTACK, 5.0), (ix::RELEASE, 50.0)] {
                c.set_param(i, v);
            }
            let mut l = sine(-10.0, 96_000, 1000.0, sr);
            let mut r = l.clone();
            c.process(&mut l, &mut r);
            // RMS of a -10 dBFS-peak sine is -13.01 dBFS: 16.99 dB over, so gain = over * (1/r - 1).
            let over = -10.0 - 3.0103 + 30.0;
            let want = -10.0 + over * (1.0 / ratio as f64 - 1.0);
            let got = peak_db(&l[80_000..]);
            assert!((got - want).abs() < 0.05, "ratio {ratio}: {got} vs {want}");
        }
    }

    #[test]
    fn below_threshold_is_bit_transparent() {
        let mut c = Compressor::default();
        c.prepare(48_000.0, 4096);
        c.set_param(ix::KNEE, 0.0);
        c.set_param(ix::LOOKAHEAD, 0.0);
        let src = sine(-40.0, 4096, 440.0, 48_000.0);
        let (mut l, mut r) = (src.clone(), src.clone());
        c.process(&mut l, &mut r);
        assert_eq!(l, src);
    }

    /// An external key with a 2 kHz tone under the threshold: no reduction until the key's
    /// boost lifts 2 kHz over it, while the high- and low-pass stay on.
    #[test]
    fn external_key_through_boost_hp_and_lp() {
        let run = |boost: f32| {
            let mut c = Compressor::default();
            c.prepare(48_000.0, 48_000);
            for (i, v) in [(ix::THRESHOLD, -24.0), (ix::KNEE, 0.0), (ix::LOOKAHEAD, 0.0), (ix::KEY_SOURCE, 1.0), (ix::KEY_HIGH_PASS, 200.0), (ix::KEY_LOW_PASS, 8000.0), (ix::KEY_BOOST_FREQ, 2000.0), (ix::KEY_BOOST, boost), (ix::KEY_BOOST_Q, 2.0)] {
                c.set_param(i, v);
            }
            c.reset();
            let key: Vec<f32> = (0..48_000).map(|i| 0.04 * (std::f32::consts::TAU * 2000.0 * i as f32 / 48_000.0).sin()).collect();
            let (mut l, mut r) = (vec![0.1f32; 48_000], vec![0.1f32; 48_000]);
            c.process_keyed(&mut l, &mut r, &key, &key);
            c.meters().reduction_db
        };
        // 0.04 peak = -28 dBFS: under -24 dB, so nothing; +12 dB at 2 kHz puts it 8 dB over.
        assert!(run(0.0) < 0.01, "{}", run(0.0));
        let gr = run(12.0);
        assert!((gr - 6.0).abs() < 0.3, "{gr}");
    }

    #[test]
    fn lookahead_latency_and_alignment() {
        let mut c = Compressor::default();
        c.prepare(48_000.0, 4096);
        c.set_param(ix::LOOKAHEAD, 1.0);
        assert_eq!(c.latency(), 48);
        c.set_param(ix::MIX, 50.0);
        let mut l = vec![0.0f32; 256];
        l[10] = 0.001;
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        assert!((l[58] - 0.001).abs() < 1e-7 && l.iter().enumerate().all(|(i, v)| i == 58 || *v == 0.0));
    }

    /// Look-ahead with the shortest attack: the gain is on the curve at the first sample of a
    /// step, so the transient does not get through, and it got there along a ramp, not a jump.
    #[test]
    fn lookahead_lands_the_gain_on_the_onset() {
        let mut c = Compressor::default();
        c.prepare(48_000.0, 4096);
        for (i, v) in [(ix::THRESHOLD, -20.0), (ix::KNEE, 0.0), (ix::ATTACK, 0.01), (ix::LOOKAHEAD, 2.0)] {
            c.set_param(i, v);
        }
        let mut l = vec![0.0f32; 2000];
        for v in &mut l[1000..] {
            *v = 0.5;
        }
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        let onset = 1000 + c.latency();
        let want = 0.5 * db_to_lin(curve::compressor(crate::lin_to_db(0.5), -20.0, 4.0, 0.0)) as f32;
        assert!((l[onset] - want).abs() / want < 0.01, "{} vs {want}", l[onset]);
        assert!(l[onset..].iter().all(|v| *v <= want * 1.01));
    }

    #[test]
    fn attack_time_constant_is_as_labelled() {
        let sr = 48_000.0;
        let mut c = Compressor::default();
        c.prepare(sr as f32, 4096);
        c.set_param(ix::THRESHOLD, -20.0);
        c.set_param(ix::KNEE, 0.0);
        c.set_param(ix::ATTACK, 10.0);
        c.set_param(ix::LOOKAHEAD, 0.0);
        // DC step: the detector level is exact, so the gain is a pure one-pole.
        let mut l = vec![0.5f32; 4800];
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        let target = curve::compressor(crate::lin_to_db(0.5), -20.0, 4.0, 0.0);
        let g = crate::lin_to_db(l[480] as f64 / 0.5);
        assert!((g / target - (1.0 - (-1.0f64).exp())).abs() < 0.01, "{g} {target}");
    }
}

#[cfg(test)]
pub(crate) mod thd {
    use super::*;

    /// Harmonics 2..10 of `f` against the fundamental, dB, over a whole number of periods.
    pub fn thd_db(x: &[f32], f: f64, sr: f64) -> f64 {
        let bin = |h: f64| {
            let (mut re, mut im) = (0.0, 0.0);
            for (i, v) in x.iter().enumerate() {
                let w = std::f64::consts::TAU * h * f * i as f64 / sr;
                // Hann window keeps leakage from the fundamental out of the harmonic bins.
                let hann = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / x.len() as f64).cos();
                re += *v as f64 * hann * w.cos();
                im += *v as f64 * hann * w.sin();
            }
            re * re + im * im
        };
        let fund = bin(1.0);
        let harm: f64 = (2..=10).map(|h| bin(h as f64)).sum();
        10.0 * (harm / fund).log10()
    }

    pub fn measure_with(det: f32, attack: f32, release: f32, f: f64, zero: bool, auto: bool) -> (f64, f64) {
        let sr = 48_000.0;
        let mut c = Compressor::default();
        c.prepare(sr as f32, 1 << 17);
        c.set_zero_latency(zero);
        c.set_param(ix::AUTO_ATTACK, auto as u8 as f32);
        c.set_param(ix::AUTO_RELEASE, auto as u8 as f32);
        for (i, v) in [(ix::THRESHOLD, -30.0), (ix::RATIO, 4.0), (ix::DETECTOR, det), (ix::ATTACK, attack), (ix::RELEASE, release)] {
            c.set_param(i, v);
        }
        let n = 96_000;
        let mut l: Vec<f32> = (0..n).map(|i| (0.5 * (std::f64::consts::TAU * f * i as f64 / sr).sin()) as f32).collect();
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        let tail = &l[48_000..];
        (thd_db(tail, f, sr), c.meters().reduction_db as f64)
    }

    #[test]
    fn steady_tone_gets_no_harmonics() {
        for (name, det, a, r) in [("peak 10/120", 0.0, 10.0, 120.0), ("peak 1/50", 0.0, 1.0, 50.0), ("rms 10/120", 1.0, 10.0, 120.0)] {
            for f in [50.0, 1000.0] {
                for (zero, auto) in [(false, false), (true, false), (false, true)] {
                    let (thd, gr) = measure_with(det, a, r, f, zero, auto);
                    println!("{name:12} {f:6} Hz zero latency {zero} auto {auto}: GR {gr:5.1} dB, THD {thd:7.1} dB");
                    // Steady state: the gain is constant, so nothing but numerical noise is added.
                    assert!(thd < -120.0 && gr > 15.0, "{name} {f} Hz zero {zero} auto {auto}: THD {thd} dB at {gr} dB GR");
                }
            }
        }
    }
}
