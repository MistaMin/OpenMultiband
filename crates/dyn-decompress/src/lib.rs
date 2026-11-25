//! `dyn-decompress`: a decompressor. It gives back dynamics that a compressor or limiter took
//! away, by upward expansion above a threshold: the louder a peak is above the threshold, the
//! more it is raised, so transients and peaks stand out from the body again.
//!
//! Definition (the house wiki has no page on this processor; this is the definition used):
//!
//! * Detector level `L` (dBFS), threshold `T`, ratio written 1:x (stored as the in:out value
//!   `1/x`, so 1:1.5 is stored as 0.667; the same convention as the upward ratios of the
//!   dual compressor), knee width `W` (dB, total), range `R` (dB, the most it may add).
//! * Gain `g(L) = min(R, (x - 1) * u)` with `u = L - T` above the knee, `0` below it, and
//!   `(x - 1) * (u + W/2)^2 / (2W)` inside it (`|u| <= W/2`). Output level above the
//!   threshold is `T + x * (L - T)`: a 1:x decompressor with the same threshold inverts the
//!   static curve of an x:1 hard-knee compressor exactly (until the range is used up).
//! * The gain is never negative. The module only turns gain down in the ceiling limiter and in
//!   the output trim.
//!
//! Signal path (per sample):
//!
//! ```text
//! detector: key (input, or external key) -> key high-pass -> key low-pass
//!           -> rectify (Peak |x|, RMS one pole on x^2, Transient: |x| against an RMS body)
//!           -> look-ahead hold (sliding max) -> attack / release follower -> stereo link (dB)
//!           -> static curve (threshold rides on the body level in Transient mode)
//!           -> range (Match: range follows the crest deficit) -> gain
//! audio:    in -> look-ahead delay -> x (1 + mix (gain - 1)) -> trim (down only)
//!           -> ceiling limiter (sample or true peak, stereo linked) -> out
//! ```
//!
//! Upward gain is not a reduction, so [`Meters::reduction_db`] reports only what the ceiling
//! limiter takes off; the upward gain is in the [`view`] slots `GAIN` (signed, dB).
#![forbid(unsafe_code)]

use dynamics::keyboost;
use olm_api::{Kind, Meters, Module, ParamDesc};

mod parts;
use parts::{one_pole, BoxAvg, Biquad, Delay, Follower, Limiter, SlideMax};
pub use parts::{OVERSAMPLE, TP_DELAY};

// ---------------------------------------------------------------------------------------------
// Constants (free choices, see the crate report)
// ---------------------------------------------------------------------------------------------

/// Ceiling limiter: moving-average length (= its look-ahead) and release.
pub const CEILING_LOOKAHEAD_MS: f64 = 1.5;
pub const CEILING_RELEASE_MS: f64 = 80.0;
/// Parameter smoothing time constant (threshold, ratio, range, crest, mix, trim).
pub const SMOOTH_MS: f64 = 5.0;
/// Match: time constants of the programme peak (decay) and mean-square estimates, and of the
/// smoothing of the resulting range.
pub const MATCH_PEAK_S: f64 = 3.0;
pub const MATCH_RMS_S: f64 = 3.0;
pub const MATCH_SMOOTH_S: f64 = 1.0;
/// Match ignores programme below this mean-square level (dBFS) and holds its estimate.
pub const MATCH_GATE_DB: f64 = -70.0;
/// OpenDynamics: the detector level is also held at its maximum over this long (half a period
/// of 25 Hz) so a steady tone gives a constant gain: no ripple with the waveform, no added
/// harmonics. It lengthens the look-ahead hold backward only; the front is unchanged.
pub const LEVEL_HOLD_MS: f64 = 20.0;
/// The key high-pass is off at the bottom of its range, the low-pass at the top.
pub const KEY_HP_OFF_HZ: f32 = 10.0;
pub const KEY_LP_OFF_HZ: f32 = 22_000.0;
/// The lowest level the read-out and the curve see, dBFS.
pub const FLOOR_DB: f64 = -200.0;

// ---------------------------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------------------------

/// Parameter indices.
pub mod index {
    pub const THRESHOLD: usize = 0;
    pub const RATIO: usize = 1;
    pub const RANGE: usize = 2;
    pub const KNEE: usize = 3;
    pub const ATTACK: usize = 4;
    pub const RELEASE: usize = 5;
    pub const DETECTOR: usize = 6;
    pub const DETECTOR_WINDOW: usize = 7;
    pub const TRANSIENT_CREST: usize = 8;
    pub const LOOKAHEAD: usize = 9;
    pub const STEREO_LINK: usize = 10;
    pub const KEY_SOURCE: usize = 11;
    pub const KEY_HIGH_PASS: usize = 12;
    pub const KEY_LOW_PASS: usize = 13;
    pub const MIX: usize = 14;
    pub const CEILING_ON: usize = 15;
    pub const CEILING: usize = 16;
    pub const CEILING_TYPE: usize = 17;
    pub const TRIM: usize = 18;
    pub const MATCH: usize = 19;
    pub const MATCH_TARGET: usize = 20;
    pub const AUTO_ATTACK: usize = 21;
    pub const AUTO_RELEASE: usize = 22;
    pub const KEY_BOOST_FREQ: usize = 23;
    pub const KEY_BOOST: usize = 24;
    pub const KEY_BOOST_Q: usize = 25;
    pub const COUNT: usize = 26;
}
use index as ix;

pub const DETECTORS: &[&str] = &["Peak", "RMS", "Transient"];
pub const KEY_SOURCES: &[&str] = &["Internal", "External"];
pub const CEILING_TYPES: &[&str] = &["Sample peak", "True peak"];

pub const DET_PEAK: usize = 0;
pub const DET_RMS: usize = 1;
pub const DET_TRANSIENT: usize = 2;

pub static PARAMS: [ParamDesc; ix::COUNT] = [
    ParamDesc::new("threshold", "Threshold", -60.0, 0.0, -18.0, Kind::Db),
    // The x of 1:x, in whole steps (1 = off, 4 = 1:4); the curve takes in:out = 1/x.
    ParamDesc::new("ratio", "Ratio", 1.0, 4.0, 2.0, Kind::Ratio),
    ParamDesc::new("range", "Range", 0.0, 24.0, 6.0, Kind::Db),
    ParamDesc::new("knee", "Knee", 0.0, 24.0, 6.0, Kind::Db),
    ParamDesc::new("attack", "Attack", 0.01, 100.0, 1.0, Kind::Ms).log(),
    ParamDesc::new("release", "Release", 1.0, 2000.0, 80.0, Kind::Ms).log(),
    ParamDesc::choice("detector", "Detector", DETECTORS, DET_PEAK),
    ParamDesc::new("detectorWindow", "RMS Window", 1.0, 500.0, 30.0, Kind::Ms).log(),
    ParamDesc::new("transientCrest", "Transient Crest", 0.0, 24.0, 6.0, Kind::Db),
    ParamDesc::new("lookahead", "Look-Ahead", 0.0, 10.0, 2.0, Kind::Ms),
    ParamDesc::new("stereoLink", "Stereo Link", 0.0, 100.0, 100.0, Kind::Percent),
    ParamDesc::choice("keySource", "Key Source", KEY_SOURCES, 0),
    ParamDesc::new("keyHighPass", "Key High-Pass", KEY_HP_OFF_HZ, 2000.0, KEY_HP_OFF_HZ, Kind::Hz).log(),
    ParamDesc::new("keyLowPass", "Key Low-Pass", 500.0, KEY_LP_OFF_HZ, KEY_LP_OFF_HZ, Kind::Hz).log(),
    ParamDesc::new("mix", "Mix", 0.0, 100.0, 100.0, Kind::Percent),
    ParamDesc::toggle("ceilingOn", "Ceiling", true),
    ParamDesc::new("ceiling", "Ceiling Level", -24.0, 0.0, -1.0, Kind::Db),
    ParamDesc::choice("ceilingType", "Ceiling Type", CEILING_TYPES, 1),
    ParamDesc::new("trim", "Output Trim", -24.0, 0.0, 0.0, Kind::Db),
    ParamDesc::toggle("match", "Match", false),
    ParamDesc::new("matchTarget", "Match Target Crest", 6.0, 24.0, 12.0, Kind::Db),
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

/// Upward gain in dB (never negative) for a detector level in dBFS.
///
/// `ratio` is the in:out value (`1/x` for 1:x, so `0.5` is 1:2), `knee_db` the total knee
/// width, `range_db` the most the curve may add.
pub fn static_gain_db(level_db: f64, threshold_db: f64, ratio: f64, knee_db: f64, range_db: f64) -> f64 {
    let s = 1.0 / ratio.clamp(1.0e-3, 1.0) - 1.0;
    curve(level_db, threshold_db, s, knee_db, range_db)
}

/// The threshold the curve uses in Transient mode: `crest_db` above the body (slow RMS) level,
/// never below the set threshold.
pub fn transient_threshold_db(threshold_db: f64, body_db: f64, crest_db: f64) -> f64 {
    threshold_db.max(body_db + crest_db)
}

/// The range Match asks for: how far the measured crest factor is below the target, limited
/// to `0 ..= range_db`.
pub fn match_range_db(crest_db: f64, target_db: f64, range_db: f64) -> f64 {
    (target_db - crest_db).clamp(0.0, range_db.max(0.0))
}

#[inline]
fn curve(level_db: f64, thr: f64, slope: f64, knee: f64, range: f64) -> f64 {
    let u = level_db - thr;
    let h = 0.5 * knee.max(0.0);
    let g = if u <= -h {
        0.0
    } else if u < h {
        slope * (u + h) * (u + h) / (4.0 * h)
    } else {
        slope * u
    };
    g.min(range).max(0.0)
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

/// One-pole parameter smoother that lands exactly on its target.
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

/// The decompressor. See the crate documentation.
pub struct Decompressor {
    v: [f32; ix::COUNT],
    sr: f64,
    ready: bool,

    // smoothed parameters
    thr: Smooth,
    slope: Smooth,
    range: Smooth,
    crest: Smooth,
    mix: Smooth,
    trim: Smooth,
    ksm: f64,

    // derived
    knee: f64,
    attack: f64,
    release: f64,
    kwin: f64,
    la: usize,
    zero: bool,
    /// Zero-latency ceiling: instant attack, exponential release (gain, coefficient).
    zl_g: f64,
    zl_rel: f64,
    la_max: usize,
    link: f64,
    ceiling: f32,
    kmatch_pk: f64,
    kmatch_ms: f64,
    kmatch_sm: f64,

    hp: [Biquad; 2],
    lp: [Biquad; 2],
    ms: [f64; 2],
    smax: [SlideMax; 2],
    /// Look-ahead ramp: the held level glides up over the look-ahead and arrives on the onset.
    ramp: [BoxAvg; 2],
    hold: usize,
    fol: [Follower; 2],
    key_boost: keyboost::KeyBoost,
    auto: dynamics::auto::AutoTime,
    dl: [Delay; 2],
    lim: Limiter,
    m_pk: f64,
    m_ms: f64,
    m_range: f64,
    m_crest: f64,

    meters: Meters,
    view: [f32; view::LEN],
}

impl Default for Decompressor {
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
            crest: Smooth::default(),
            mix: Smooth::default(),
            trim: Smooth::default(),
            ksm: 1.0,
            knee: 0.0,
            attack: 1.0,
            release: 0.0,
            kwin: 1.0,
            la: 0,
            zero: false,
            zl_g: 1.0,
            zl_rel: 1.0,
            la_max: 0,
            link: 1.0,
            ceiling: 1.0,
            kmatch_pk: 0.0,
            kmatch_ms: 1.0,
            kmatch_sm: 1.0,
            hp: [Biquad::default(); 2],
            lp: [Biquad::default(); 2],
            ms: [0.0; 2],
            smax: [SlideMax::default(), SlideMax::default()],
            ramp: [BoxAvg::default(), BoxAvg::default()],
            hold: 0,
            fol: [Follower::default(); 2],
            key_boost: keyboost::KeyBoost::default(),
            auto: dynamics::auto::AutoTime::default(),
            dl: [Delay::default(), Delay::default()],
            lim: Limiter::default(),
            m_pk: 0.0,
            m_ms: 0.0,
            m_range: 0.0,
            m_crest: 0.0,
            meters: Meters::default(),
            view: [0.0; view::LEN],
        };
        s.update();
        s.snap();
        s.clear_view();
        s
    }
}

impl Decompressor {
    /// Index of the parameter with this id.
    pub fn param_index(id: &str) -> Option<usize> {
        PARAMS.iter().position(|p| p.id == id)
    }

    /// Current (clamped) value of a parameter.
    pub fn param(&self, index: usize) -> Option<f32> {
        self.v.get(index).copied()
    }

    /// The range Match is asking for now, dB (tracked whether Match is on or not).
    pub fn match_range(&self) -> f64 {
        self.m_range
    }

    #[inline]
    fn f(&self, i: usize) -> f64 {
        self.v[i] as f64
    }

    #[inline]
    fn choice(&self, i: usize) -> usize {
        self.v[i].round().max(0.0) as usize
    }

    #[inline]
    fn on(&self, i: usize) -> bool {
        self.v[i] >= 0.5
    }

    /// Recomputes everything derived from the parameters. Never allocates.
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
        self.slope.t = self.f(ix::RATIO).clamp(1.0, 4.0) - 1.0;
        self.range.t = self.f(ix::RANGE);
        self.crest.t = self.f(ix::TRANSIENT_CREST);
        self.mix.t = (self.f(ix::MIX) / 100.0).clamp(0.0, 1.0);
        self.trim.t = db_to_lin(self.f(ix::TRIM).min(0.0));
        self.ksm = one_pole(SMOOTH_MS * 1.0e-3, sr);
        self.knee = self.f(ix::KNEE);
        self.timing();
        self.kwin = one_pole(self.f(ix::DETECTOR_WINDOW) * 1.0e-3, sr);
        self.la = if self.zero { 0 } else { ((self.f(ix::LOOKAHEAD) * 1.0e-3 * sr).round() as usize).min(self.la_max) };
        self.link = (self.f(ix::STEREO_LINK) / 100.0).clamp(0.0, 1.0);
        self.ceiling = db_to_lin(self.f(ix::CEILING)) as f32;
        self.kmatch_pk = (-1.0 / (MATCH_PEAK_S * sr)).exp();
        self.kmatch_ms = one_pole(MATCH_RMS_S, sr);
        self.kmatch_sm = one_pole(MATCH_SMOOTH_S, sr);
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
        self.crest.snap();
        self.mix.snap();
        self.trim.snap();
    }

    fn clear_view(&mut self) {
        self.view = [0.0; view::LEN];
        for c in 0..2 {
            self.view[view::DETECTOR + c] = FLOOR_DB as f32;
            self.view[view::ENVELOPE + c] = FLOOR_DB as f32;
            self.view[view::THRESHOLD + c] = self.v[ix::THRESHOLD];
        }
    }

    fn run(&mut self, left: &mut [f32], right: &mut [f32], key: Option<(&[f32], &[f32])>) {
        if !self.ready {
            return;
        }
        let n = left.len().min(right.len());
        let ext = self.choice(ix::KEY_SOURCE) == 1 && key.is_some();
        let det = self.choice(ix::DETECTOR).min(2);
        let transient = det == DET_TRANSIENT;
        let ceiling_on = self.on(ix::CEILING_ON);
        let true_peak = self.choice(ix::CEILING_TYPE) == 1;
        let match_on = self.on(ix::MATCH);
        let target = self.f(ix::MATCH_TARGET);
        let (ksm, knee, mut att, mut rel, kwin, link, la) = (self.ksm, self.knee, self.attack, self.release, self.kwin, self.link, self.la);
        let ceiling = self.ceiling;

        let mut gain_max = [f64::NEG_INFINITY; 2];
        let mut gain_now = [0.0f64; 2];
        let auto = self.on(ix::AUTO_ATTACK) || self.on(ix::AUTO_RELEASE);
        let mut det_max = [f64::NEG_INFINITY; 2];
        let mut thr_last = [self.thr.v; 2];
        let mut lim_min = 1.0f32;

        for i in 0..n {
            let thr = self.thr.step(ksm);
            let slope = self.slope.step(ksm);
            let range = self.range.step(ksm);
            let crest = self.crest.step(ksm);
            let mix = self.mix.step(ksm);
            let trim = self.trim.step(ksm) as f32;

            let x = [clean(left[i]), clean(right[i])];

            // Match: programme crest factor on the input (both channels).
            let pk_in = x[0].abs().max(x[1].abs()) as f64;
            self.m_pk = pk_in.max(self.m_pk * self.kmatch_pk);
            let p2 = 0.5 * ((x[0] as f64) * (x[0] as f64) + (x[1] as f64) * (x[1] as f64));
            self.m_ms += (p2 - self.m_ms) * self.kmatch_ms;
            if self.m_ms < 1.0e-30 {
                self.m_ms = 0.0;
            }
            if self.m_ms > db_to_lin(2.0 * MATCH_GATE_DB) {
                self.m_crest = lin_to_db(self.m_pk) - 10.0 * self.m_ms.log10();
                let want = match_range_db(self.m_crest, target, PARAMS[ix::RANGE].max as f64);
                self.m_range += (want - self.m_range) * self.kmatch_sm;
            }

            let d = match key {
                Some((kl, kr)) if ext => [
                    clean(kl.get(i).copied().unwrap_or(0.0)) as f64,
                    clean(kr.get(i).copied().unwrap_or(0.0)) as f64,
                ],
                _ => [x[0] as f64, x[1] as f64],
            };

            let mut e = [0.0f64; 2];
            let mut body = [0.0f64; 2];
            for c in 0..2 {
                let k = self.key_boost.run(c, self.lp[c].run(self.hp[c].run(d[c])));
                self.ms[c] += (k * k - self.ms[c]) * kwin;
                if self.ms[c] < 1.0e-30 {
                    self.ms[c] = 0.0;
                }
                let rms = self.ms[c].sqrt();
                let fast = if det == DET_RMS { rms } else { k.abs() };
                det_max[c] = det_max[c].max(lin_to_db(fast));
                let held = self.ramp[c].push(self.smax[c].push(fast, la + 1 + self.hold), la + 1);
                e[c] = lin_to_db(self.fol[c].step(held, att, rel, 0));
                body[c] = lin_to_db(rms);
            }
            let emax = e[0].max(e[1]);
            let bmax = body[0].max(body[1]);
            let range_eff = if match_on { range.min(self.m_range) } else { range };

            let mut g = [1.0f32; 2];
            for c in 0..2 {
                let lev = e[c] + link * (emax - e[c]);
                let t = if transient { transient_threshold_db(thr, body[c] + link * (bmax - body[c]), crest) } else { thr };
                thr_last[c] = t;
                let gd = curve(lev, t, slope, knee, range_eff);
                gain_max[c] = gain_max[c].max(gd);
                gain_now[c] = gd;
                g[c] = (1.0 + mix * (db_to_lin(gd) - 1.0)) as f32 * trim;
            }

            let yl = self.dl[0].push(x[0], la) * g[0];
            let yr = self.dl[1].push(x[1], la) * g[1];
            let (ol, or, gl) = if !self.zero {
                self.lim.process(yl, yr, ceiling, true_peak, ceiling_on)
            } else if ceiling_on {
                // No look-ahead: the gain drops on the very sample that would pass the ceiling
                // (sample peak; true peak needs the reader's delay), then releases.
                let pk = yl.abs().max(yr.abs()) as f64;
                let want = if pk > ceiling as f64 { ceiling as f64 / pk } else { 1.0 };
                self.zl_g = if want < self.zl_g { want } else { self.zl_g + (want - self.zl_g) * self.zl_rel };
                let g = self.zl_g as f32;
                ((yl * g).clamp(-ceiling, ceiling), (yr * g).clamp(-ceiling, ceiling), g)
            } else {
                (yl, yr, 1.0)
            };
            lim_min = lim_min.min(gl);
            left[i] = ol;
            right[i] = or;
            if auto && self.auto.push(gain_now[0].max(gain_now[1])) {
                self.timing();
                (att, rel) = (self.attack, self.release);
            }
        }

        let red = (-20.0 * (lim_min.max(1.0e-9) as f64).log10()).max(0.0) as f32;
        self.meters = Meters { reduction_db: red, ..Meters::default() };
        if n > 0 {
            for c in 0..2 {
                self.view[view::GAIN + c] = gain_max[c] as f32;
                self.view[view::DETECTOR + c] = det_max[c] as f32;
                self.view[view::THRESHOLD + c] = thr_last[c] as f32;
                self.view[view::ENVELOPE + c] = lin_to_db(self.fol[c].level()) as f32;
            }
            self.view[view::CEILING] = red;
            self.view[view::MATCH_CREST] = self.m_crest as f32;
            self.view[view::MATCH_RANGE] = self.m_range as f32;
            self.view[view::KEYED] = if ext { 1.0 } else { 0.0 };
        }
    }
}

impl Module for Decompressor {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        let sr = if sample_rate.is_finite() && sample_rate >= 8_000.0 { sample_rate as f64 } else { 48_000.0 };
        self.sr = sr;
        self.auto.set_rate(sr);
        self.la_max = (PARAMS[ix::LOOKAHEAD].max as f64 * 1.0e-3 * sr).ceil() as usize;
        self.hold = (LEVEL_HOLD_MS * 1.0e-3 * sr).round() as usize;
        for c in 0..2 {
            self.smax[c].alloc(self.la_max + self.hold + 4);
            self.ramp[c].alloc(self.la_max + 4);
            self.dl[c].alloc(self.la_max + 2);
        }
        let w = ((CEILING_LOOKAHEAD_MS * 1.0e-3 * sr).round() as usize).max(2);
        self.lim.alloc(w, one_pole(CEILING_RELEASE_MS * 1.0e-3, sr));
        self.zl_rel = one_pole(CEILING_RELEASE_MS * 1.0e-3, sr);
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
        self.lim.clear();
        self.zl_g = 1.0;
        self.m_pk = 0.0;
        self.m_ms = 0.0;
        self.m_range = 0.0;
        self.m_crest = 0.0;
        self.meters = Meters::default();
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

    /// Look-ahead (0 to 10 ms, rounded to samples) + the ceiling limiter (32 samples of
    /// true-peak reader + 1.5 ms - 1 sample). The limiter delay is there whether the ceiling is
    /// on or off and in both ceiling types, so switching them never moves the latency.
    /// 103 samples at 48 kHz with the defaults.
    /// In zero-latency mode: 0 (no look-ahead, and the ceiling becomes an instant clamp).
    fn latency(&self) -> usize {
        if self.zero { 0 } else { self.la + self.lim.delay() }
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

    /// `reduction_db`: what the ceiling limiter took off in the block (positive dB). Upward
    /// gain is not reported here (it is not a reduction); read `view::GAIN`.
    fn meters(&self) -> Meters {
        self.meters
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
    /// Signed gain of the curve per channel, dB, the highest in the block (positive = up;
    /// before mix, trim and ceiling).
    pub const GAIN: usize = 0;
    /// Detector level per channel before the follower (after key filters), dBFS, highest.
    pub const DETECTOR: usize = 2;
    /// Threshold the curve used per channel at the end of the block, dBFS (in Transient mode
    /// it rides on the body level).
    pub const THRESHOLD: usize = 4;
    /// Follower envelope per channel at the end of the block, dBFS.
    pub const ENVELOPE: usize = 6;
    /// Ceiling-limiter reduction, dB, positive, deepest in the block.
    pub const CEILING: usize = 8;
    /// Match: measured programme crest factor (dB) and the range it asks for (dB).
    pub const MATCH_CREST: usize = 9;
    pub const MATCH_RANGE: usize = 10;
    /// 1 when the detector ran from the external key in this block.
    pub const KEYED: usize = 11;
    pub const LEN: usize = 12;
}
