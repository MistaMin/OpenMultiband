//! `dyn-deexpand`: a de-expander. It undoes downward expansion or gating by upward compression
//! below a threshold: quiet passages, decays and tails are brought back up towards the
//! threshold, while everything under a floor (noise) is left where it is.
//!
//! Definition (the house wiki has no page on this processor; this is the definition used):
//!
//! * Detector level `L` (dBFS), threshold `T`, ratio `r:1` (`r >= 1`, the compression ratio
//!   below the threshold), knee width `W` (dB, total), range `R` (dB, the most it may lift),
//!   floor `F` (dBFS).
//! * With `u = T - L` (how far below the threshold) and `s = 1 - 1/r`, the threshold curve is
//!   `c(u) = s * u` below the knee, `0` above it, `s * (u + W/2)^2 / (2W)` inside it
//!   (`|u| <= W/2`). Output level below the knee is `T + (L - T) / r`: an `r:1` de-expander
//!   with the same threshold inverts the static curve of a `1:r` downward expander exactly
//!   (until the range is used up).
//! * Lift `g(L) = max(0, min(c(T - L), R, L - F))`. The last term is the floor: nothing at or
//!   below `F` is lifted, and just above it the lift grows by 1 dB per dB, so the curve has no
//!   step where the floor takes over.
//! * The gain is never negative: the module never turns anything down.
//!
//! Ballistics act on the detector level: attack is how fast it follows a rise (so the lift
//! drops quickly when the music comes back), hold and release how long it waits and how fast it
//! follows a fall (so the lift grows as a note decays). Look-ahead lets the lift drop before a
//! loud onset arrives, which stops onsets after a lifted quiet passage from jumping out.
//!
//! Signal path (per sample):
//!
//! ```text
//! detector: key (input, or external key) -> key high-pass -> key low-pass
//!           -> rectify (Peak |x|, or RMS one pole on x^2) -> look-ahead hold (sliding max)
//!           -> attack / hold / release follower -> stereo link (dB) -> static curve -> lift
//! audio:    in -> look-ahead delay -> x (1 + mix (lift - 1)) -> out
//! ```
//!
//! [`Meters::reduction_db`] is always 0 (the module never reduces gain); the lift is in the
//! [`view`] slots `GAIN` (signed, dB, positive = up).
#![forbid(unsafe_code)]

use dynamics::keyboost;
use olm_api::{Kind, Meters, Module, ParamDesc};

mod parts;
use parts::{one_pole, Biquad, Delay, BoxAvg, Follower, SlideMax};

/// Parameter smoothing time constant (threshold, ratio, range, floor, mix).
pub const SMOOTH_MS: f64 = 5.0;
/// The key high-pass is off at the bottom of its range, the low-pass at the top.
pub const KEY_HP_OFF_HZ: f32 = 10.0;
pub const KEY_LP_OFF_HZ: f32 = 22_000.0;
/// The lowest level the read-out and the curve see, dBFS.
pub const FLOOR_DB: f64 = -200.0;

/// Parameter indices.
pub mod index {
    pub const THRESHOLD: usize = 0;
    pub const RATIO: usize = 1;
    pub const RANGE: usize = 2;
    pub const KNEE: usize = 3;
    pub const FLOOR: usize = 4;
    pub const ATTACK: usize = 5;
    pub const HOLD: usize = 6;
    pub const RELEASE: usize = 7;
    pub const DETECTOR: usize = 8;
    pub const DETECTOR_WINDOW: usize = 9;
    pub const LOOKAHEAD: usize = 10;
    pub const STEREO_LINK: usize = 11;
    pub const KEY_SOURCE: usize = 12;
    pub const KEY_HIGH_PASS: usize = 13;
    pub const KEY_LOW_PASS: usize = 14;
    pub const MIX: usize = 15;
    pub const AUTO_ATTACK: usize = 16;
    pub const AUTO_RELEASE: usize = 17;
    pub const KEY_BOOST_FREQ: usize = 18;
    pub const KEY_BOOST: usize = 19;
    pub const KEY_BOOST_Q: usize = 20;
    pub const COUNT: usize = 21;
}
use index as ix;

pub const DETECTORS: &[&str] = &["Peak", "RMS"];
pub const KEY_SOURCES: &[&str] = &["Internal", "External"];
pub const DET_PEAK: usize = 0;
pub const DET_RMS: usize = 1;

pub static PARAMS: [ParamDesc; ix::COUNT] = [
    ParamDesc::new("threshold", "Threshold", -80.0, 0.0, -40.0, Kind::Db),
    ParamDesc::new("ratio", "Ratio", 1.0, 8.0, 2.0, Kind::Ratio).log(),
    ParamDesc::new("range", "Range", 0.0, 36.0, 12.0, Kind::Db),
    ParamDesc::new("knee", "Knee", 0.0, 24.0, 6.0, Kind::Db),
    ParamDesc::new("floor", "Floor", -120.0, -20.0, -72.0, Kind::Db),
    ParamDesc::new("attack", "Attack", 0.01, 100.0, 0.5, Kind::Ms).log(),
    ParamDesc::new("hold", "Hold", 0.0, 500.0, 20.0, Kind::Ms),
    ParamDesc::new("release", "Release", 1.0, 5000.0, 150.0, Kind::Ms).log(),
    ParamDesc::choice("detector", "Detector", DETECTORS, DET_RMS),
    ParamDesc::new("detectorWindow", "RMS Window", 1.0, 500.0, 10.0, Kind::Ms).log(),
    ParamDesc::new("lookahead", "Look-Ahead", 0.0, 10.0, 2.0, Kind::Ms),
    ParamDesc::new("stereoLink", "Stereo Link", 0.0, 100.0, 100.0, Kind::Percent),
    ParamDesc::choice("keySource", "Key Source", KEY_SOURCES, 0),
    ParamDesc::new("keyHighPass", "Key High-Pass", KEY_HP_OFF_HZ, 2000.0, KEY_HP_OFF_HZ, Kind::Hz).log(),
    ParamDesc::new("keyLowPass", "Key Low-Pass", 500.0, KEY_LP_OFF_HZ, KEY_LP_OFF_HZ, Kind::Hz).log(),
    ParamDesc::new("mix", "Mix", 0.0, 100.0, 100.0, Kind::Percent),
    ParamDesc::toggle("autoAttack", "Auto Attack", false),
    ParamDesc::toggle("autoRelease", "Auto Release", false),
    ParamDesc::new("keyBoostFreq", "Key Boost Freq", keyboost::FREQ_MIN, keyboost::FREQ_MAX, keyboost::FREQ_DEFAULT, Kind::Hz).log(),
    ParamDesc::new("keyBoost", "Key Boost", keyboost::GAIN_MIN, keyboost::GAIN_MAX, 0.0, Kind::Db),
    ParamDesc::new("keyBoostQ", "Key Boost Q", keyboost::Q_MIN, keyboost::Q_MAX, keyboost::Q_DEFAULT, Kind::Plain).log(),
];

pub const NPARAMS: usize = ix::COUNT;

// ---------------------------------------------------------------------------------------------
// Static curve (public, for the editor)
// ---------------------------------------------------------------------------------------------

/// Lift in dB (never negative) for a detector level in dBFS. `ratio` is `r` of `r:1`.
pub fn static_gain_db(level_db: f64, threshold_db: f64, ratio: f64, knee_db: f64, range_db: f64, floor_db: f64) -> f64 {
    let s = 1.0 - 1.0 / ratio.max(1.0);
    curve(level_db, threshold_db, s, knee_db, range_db, floor_db)
}

#[inline]
fn curve(level_db: f64, thr: f64, slope: f64, knee: f64, range: f64, floor: f64) -> f64 {
    let u = thr - level_db;
    let h = 0.5 * knee.max(0.0);
    let c = if u <= -h {
        0.0
    } else if u < h {
        slope * (u + h) * (u + h) / (4.0 * h)
    } else {
        slope * u
    };
    c.min(range).min(level_db - floor).max(0.0)
}

#[inline]
fn db_to_lin(db: f64) -> f64 {
    (db * (std::f64::consts::LN_10 / 20.0)).exp()
}

#[inline]
fn lin_to_db(v: f64) -> f64 {
    if v > 1.0e-10 {
        20.0 * v.log10()
    } else {
        FLOOR_DB
    }
}

#[inline]
fn clean(x: f32) -> f32 {
    if x.is_finite() {
        x
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Default)]
struct Smooth {
    v: f64,
    t: f64,
}

impl Smooth {
    #[inline]
    fn step(&mut self, k: f64) -> f64 {
        if self.v != self.t {
            self.v += (self.t - self.v) * k;
            if (self.t - self.v).abs() < 1.0e-9 {
                self.v = self.t;
            }
        }
        self.v
    }
    fn snap(&mut self) {
        self.v = self.t;
    }
}

// ---------------------------------------------------------------------------------------------
// The module
// ---------------------------------------------------------------------------------------------

/// The de-expander. See the crate documentation.
pub struct DeExpander {
    v: [f32; ix::COUNT],
    sr: f64,
    ready: bool,

    thr: Smooth,
    slope: Smooth,
    range: Smooth,
    floor: Smooth,
    mix: Smooth,
    ksm: f64,

    knee: f64,
    attack: f64,
    release: f64,
    hold: u32,
    kwin: f64,
    la: usize,
    zero: bool,
    la_max: usize,
    link: f64,

    hp: [Biquad; 2],
    lp: [Biquad; 2],
    ms: [f64; 2],
    smax: [SlideMax; 2],
    /// Look-ahead ramp: the held level glides up over the look-ahead and arrives on the onset.
    ramp: [BoxAvg; 2],
    fol: [Follower; 2],
    key_boost: keyboost::KeyBoost,
    auto: dynamics::auto::AutoTime,
    dl: [Delay; 2],

    view: [f32; view::LEN],
}

impl Default for DeExpander {
    fn default() -> Self {
        let mut v = [0.0f32; ix::COUNT];
        for (slot, p) in v.iter_mut().zip(PARAMS.iter()) {
            *slot = p.default;
        }
        let mut s = Self {
            v,
            sr: 48_000.0,
            ready: false,
            thr: Smooth::default(),
            slope: Smooth::default(),
            range: Smooth::default(),
            floor: Smooth::default(),
            mix: Smooth::default(),
            ksm: 1.0,
            knee: 0.0,
            attack: 1.0,
            release: 0.0,
            hold: 0,
            kwin: 1.0,
            la: 0,
            zero: false,
            la_max: 0,
            link: 1.0,
            hp: [Biquad::default(); 2],
            lp: [Biquad::default(); 2],
            ms: [0.0; 2],
            smax: [SlideMax::default(), SlideMax::default()],
            ramp: [BoxAvg::default(), BoxAvg::default()],
            fol: [Follower::default(); 2],
            key_boost: keyboost::KeyBoost::default(),
            auto: dynamics::auto::AutoTime::default(),
            dl: [Delay::default(), Delay::default()],
            view: [0.0; view::LEN],
        };
        s.update();
        s.snap();
        s.clear_view();
        s
    }
}

impl DeExpander {
    /// Index of the parameter with this id.
    pub fn param_index(id: &str) -> Option<usize> {
        PARAMS.iter().position(|p| p.id == id)
    }

    /// Current (clamped) value of a parameter.
    pub fn param(&self, index: usize) -> Option<f32> {
        self.v.get(index).copied()
    }

    #[inline]
    fn f(&self, i: usize) -> f64 {
        self.v[i] as f64
    }

    fn on(&self, i: usize) -> bool {
        self.v[i] >= 0.5
    }

    #[inline]
    fn choice(&self, i: usize) -> usize {
        self.v[i].round().max(0.0) as usize
    }

    /// Attack and release from the knobs, scaled by Auto Attack / Auto Release.
    fn timing(&mut self) {
        let sr = self.sr;
        let a = if self.on(ix::AUTO_ATTACK) { self.auto.attack() } else { 1.0 };
        let r = if self.on(ix::AUTO_RELEASE) { self.auto.release() } else { 1.0 };
        self.attack = one_pole(self.f(ix::ATTACK) * a * 1.0e-3, sr);
        self.release = (-1.0 / (self.f(ix::RELEASE) * r * 1.0e-3 * sr).max(1.0e-3)).exp();
    }

    fn update(&mut self) {
        let sr = self.sr;
        self.thr.t = self.f(ix::THRESHOLD);
        self.slope.t = 1.0 - 1.0 / self.f(ix::RATIO).max(1.0);
        self.range.t = self.f(ix::RANGE);
        self.floor.t = self.f(ix::FLOOR);
        self.mix.t = (self.f(ix::MIX) / 100.0).clamp(0.0, 1.0);
        self.ksm = one_pole(SMOOTH_MS * 1.0e-3, sr);
        self.knee = self.f(ix::KNEE);
        self.timing();
        self.hold = (self.f(ix::HOLD) * 1.0e-3 * sr).round() as u32;
        self.kwin = one_pole(self.f(ix::DETECTOR_WINDOW) * 1.0e-3, sr);
        self.la = if self.zero { 0 } else { ((self.f(ix::LOOKAHEAD) * 1.0e-3 * sr).round() as usize).min(self.la_max) };
        self.link = (self.f(ix::STEREO_LINK) / 100.0).clamp(0.0, 1.0);
        let hp = self.v[ix::KEY_HIGH_PASS];
        let lp = self.v[ix::KEY_LOW_PASS];
        for c in 0..2 {
            self.hp[c].design(true, hp as f64, sr, hp > KEY_HP_OFF_HZ * 1.001);
            self.lp[c].design(false, lp as f64, sr, lp < KEY_LP_OFF_HZ * 0.999 && (lp as f64) < 0.45 * sr);
        }
        self.key_boost.set(self.v[ix::KEY_BOOST_FREQ], self.v[ix::KEY_BOOST], self.v[ix::KEY_BOOST_Q], sr);
    }

    fn snap(&mut self) {
        self.thr.snap();
        self.slope.snap();
        self.range.snap();
        self.floor.snap();
        self.mix.snap();
    }

    fn clear_view(&mut self) {
        self.view = [0.0; view::LEN];
        for c in 0..2 {
            self.view[view::DETECTOR + c] = FLOOR_DB as f32;
            self.view[view::ENVELOPE + c] = FLOOR_DB as f32;
        }
        self.view[view::THRESHOLD] = self.v[ix::THRESHOLD];
        self.view[view::FLOOR] = self.v[ix::FLOOR];
    }

    fn run(&mut self, left: &mut [f32], right: &mut [f32], key: Option<(&[f32], &[f32])>) {
        if !self.ready {
            return;
        }
        let n = left.len().min(right.len());
        let ext = self.choice(ix::KEY_SOURCE) == 1 && key.is_some();
        let rms_mode = self.choice(ix::DETECTOR) == DET_RMS;
        let (ksm, knee, mut att, mut rel, hold, kwin, link, la) = (self.ksm, self.knee, self.attack, self.release, self.hold, self.kwin, self.link, self.la);

        let mut gain_max = [f64::NEG_INFINITY; 2];
        let mut gain_now = [0.0f64; 2];
        let auto = self.on(ix::AUTO_ATTACK) || self.on(ix::AUTO_RELEASE);
        let mut det_max = [f64::NEG_INFINITY; 2];

        for i in 0..n {
            let thr = self.thr.step(ksm);
            let slope = self.slope.step(ksm);
            let range = self.range.step(ksm);
            let floor = self.floor.step(ksm);
            let mix = self.mix.step(ksm);

            let x = [clean(left[i]), clean(right[i])];
            let d = match key {
                Some((kl, kr)) if ext => [
                    clean(kl.get(i).copied().unwrap_or(0.0)) as f64,
                    clean(kr.get(i).copied().unwrap_or(0.0)) as f64,
                ],
                _ => [x[0] as f64, x[1] as f64],
            };

            let mut e = [0.0f64; 2];
            for c in 0..2 {
                let k = self.key_boost.run(c, self.lp[c].run(self.hp[c].run(d[c])));
                let lev = if rms_mode {
                    self.ms[c] += (k * k - self.ms[c]) * kwin;
                    if self.ms[c] < 1.0e-30 {
                        self.ms[c] = 0.0;
                    }
                    self.ms[c].sqrt()
                } else {
                    k.abs()
                };
                det_max[c] = det_max[c].max(lin_to_db(lev));
                let held = self.ramp[c].push(self.smax[c].push(lev, la + 1), la + 1);
                e[c] = lin_to_db(self.fol[c].step(held, att, rel, hold));
            }
            let emax = e[0].max(e[1]);
            let mut g = [1.0f32; 2];
            for c in 0..2 {
                let lev = e[c] + link * (emax - e[c]);
                let gd = curve(lev, thr, slope, knee, range, floor);
                gain_max[c] = gain_max[c].max(gd);
                gain_now[c] = gd;
                g[c] = (1.0 + mix * (db_to_lin(gd) - 1.0)) as f32;
            }
            left[i] = self.dl[0].push(x[0], la) * g[0];
            right[i] = self.dl[1].push(x[1], la) * g[1];
            if auto && self.auto.push(gain_now[0].max(gain_now[1])) {
                self.timing();
                (att, rel) = (self.attack, self.release);
            }
        }

        if n > 0 {
            for c in 0..2 {
                self.view[view::GAIN + c] = gain_max[c] as f32;
                self.view[view::DETECTOR + c] = det_max[c] as f32;
                self.view[view::ENVELOPE + c] = lin_to_db(self.fol[c].level()) as f32;
            }
            self.view[view::THRESHOLD] = self.thr.v as f32;
            self.view[view::FLOOR] = self.floor.v as f32;
            self.view[view::KEYED] = if ext { 1.0 } else { 0.0 };
        }
    }
}

impl Module for DeExpander {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        let sr = if sample_rate.is_finite() && sample_rate >= 8_000.0 { sample_rate as f64 } else { 48_000.0 };
        self.sr = sr;
        self.auto.set_rate(sr);
        self.la_max = (PARAMS[ix::LOOKAHEAD].max as f64 * 1.0e-3 * sr).ceil() as usize;
        for c in 0..2 {
            self.smax[c].alloc(self.la_max + 4);
            self.ramp[c].alloc(self.la_max + 4);
            self.dl[c].alloc(self.la_max + 2);
        }
        self.ready = true;
        self.update();
        self.reset();
    }

    fn reset(&mut self) {
        for c in 0..2 {
            self.hp[c].clear();
            self.lp[c].clear();
            self.key_boost.clear();
            self.ms[c] = 0.0;
            self.smax[c].clear();
            self.ramp[c].clear();
            self.fol[c] = Follower::default();
            self.auto.clear();
            self.dl[c].clear();
        }
        self.snap();
        self.clear_view();
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let Some(p) = PARAMS.get(index) else { return };
        if value.is_nan() {
            return;
        }
        let mut v = value.clamp(p.min, p.max);
        if p.steps > 0 {
            v = v.round();
        }
        if self.v[index] == v {
            return;
        }
        self.v[index] = v;
        self.update();
    }

    /// The look-ahead, rounded to samples (0 to 10 ms). 96 samples at 48 kHz with the defaults.
    fn latency(&self) -> usize {
        self.la
    }

    fn set_zero_latency(&mut self, on: bool) {
        self.zero = on;
        self.update();
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.run(left, right, None);
    }

    fn process_keyed(&mut self, left: &mut [f32], right: &mut [f32], key_left: &[f32], key_right: &[f32]) {
        self.run(left, right, Some((key_left, key_right)));
    }

    /// Always zero: the module only lifts. Read the lift from `view::GAIN`.
    fn meters(&self) -> Meters {
        Meters::default()
    }

    fn lift_db(&self) -> f32 {
        self.view[view::GAIN].max(self.view[view::GAIN + 1]).max(0.0)
    }

    fn view(&self, out: &mut [f32]) -> usize {
        let n = out.len().min(view::LEN);
        out[..n].copy_from_slice(&self.view[..n]);
        n
    }
}

/// Slot layout of [`Module::view`], for the block just processed. Pairs are left, right.
pub mod view {
    /// Signed gain (lift) of the curve per channel, dB, the highest in the block (positive =
    /// up; before mix).
    pub const GAIN: usize = 0;
    /// Detector level per channel before the follower (after key filters), dBFS, highest.
    pub const DETECTOR: usize = 2;
    /// Threshold and floor in use at the end of the block (smoothed), dBFS.
    pub const THRESHOLD: usize = 4;
    pub const FLOOR: usize = 5;
    /// Follower envelope per channel at the end of the block, dBFS.
    pub const ENVELOPE: usize = 6;
    /// 1 when the detector ran from the external key in this block.
    pub const KEYED: usize = 8;
    pub const LEN: usize = 9;
}
