//! Gate / downward expander.
//!
//! * Expander: `1:ratio` below the threshold with a quadratic knee, down by at most Range.
//! * Gate: open (0 dB) at or above the threshold, Range down below it.
//!
//! Hysteresis: once open, the section stays open until the level falls Hysteresis dB under
//! the threshold, so a level hovering at the threshold does not chatter. Hold keeps the gain
//! from falling for Hold ms after the level was last above the (open) threshold. Attack is the
//! time constant of the gain rising (opening), Release of it falling, both on the gain in dB.
//! Look-ahead delays the audio and lets the gain open early, so onsets keep their front.

use olm_api::{Kind, Meters, Module, ParamDesc};

use crate::curve;
use crate::keyboost;
use crate::detector::{Detector, GainFollower, DETECTORS, KEY_HP_OFF_HZ, KEY_LP_OFF_HZ, KEY_SOURCES};
use crate::auto::AutoTime;
use crate::parts::{one_pole, BoxAvg, Delay, SlideMax};
use crate::{clean, db_to_lin, view, FLOOR_DB};

pub mod index {
    pub const MODE: usize = 0;
    pub const THRESHOLD: usize = 1;
    pub const RATIO: usize = 2;
    pub const RANGE: usize = 3;
    pub const KNEE: usize = 4;
    pub const HYSTERESIS: usize = 5;
    pub const ATTACK: usize = 6;
    pub const HOLD: usize = 7;
    pub const RELEASE: usize = 8;
    pub const DETECTOR: usize = 9;
    pub const RMS_WINDOW: usize = 10;
    pub const LOOKAHEAD: usize = 11;
    pub const STEREO_LINK: usize = 12;
    pub const KEY_SOURCE: usize = 13;
    pub const KEY_HIGH_PASS: usize = 14;
    pub const KEY_LOW_PASS: usize = 15;
    pub const AUTO_ATTACK: usize = 16;
    pub const AUTO_RELEASE: usize = 17;
    pub const KEY_BOOST_FREQ: usize = 18;
    pub const KEY_BOOST: usize = 19;
    pub const KEY_BOOST_Q: usize = 20;
    pub const COUNT: usize = 21;
}
use index as ix;

pub const MODES: &[&str] = &["Gate", "Expander"];
pub const MODE_GATE: usize = 0;
pub const MAX_LOOKAHEAD_MS: f32 = 10.0;

pub static PARAMS: [ParamDesc; ix::COUNT] = [
    ParamDesc::choice("mode", "Mode", MODES, MODE_GATE),
    ParamDesc::new("threshold", "Threshold", -90.0, 0.0, -50.0, Kind::Db),
    ParamDesc::new("ratio", "Ratio", 1.0, 20.0, 2.0, Kind::Ratio).log(),
    ParamDesc::new("range", "Range", 0.0, 90.0, 40.0, Kind::Db),
    ParamDesc::new("knee", "Knee", 0.0, 24.0, 6.0, Kind::Db),
    ParamDesc::new("hysteresis", "Hysteresis", 0.0, 12.0, 3.0, Kind::Db),
    ParamDesc::new("attack", "Attack", 0.01, 100.0, 0.5, Kind::Ms).log(),
    ParamDesc::new("hold", "Hold", 0.0, 500.0, 20.0, Kind::Ms),
    ParamDesc::new("release", "Release", 5.0, 5000.0, 150.0, Kind::Ms).log(),
    ParamDesc::choice("detector", "Detector", DETECTORS, 0),
    ParamDesc::new("rmsWindow", "RMS Window", 1.0, 300.0, 10.0, Kind::Ms).log(),
    ParamDesc::new("lookahead", "Look-Ahead", 0.0, MAX_LOOKAHEAD_MS, 2.0, Kind::Ms),
    ParamDesc::new("stereoLink", "Stereo Link", 0.0, 100.0, 100.0, Kind::Percent),
    ParamDesc::choice("keySource", "Key Source", KEY_SOURCES, 0),
    ParamDesc::new("keyHighPass", "Key High-Pass", KEY_HP_OFF_HZ, 2000.0, KEY_HP_OFF_HZ, Kind::Hz).log(),
    ParamDesc::new("keyLowPass", "Key Low-Pass", 500.0, KEY_LP_OFF_HZ, KEY_LP_OFF_HZ, Kind::Hz).log(),

    ParamDesc::toggle("autoAttack", "Auto Attack", false),
    ParamDesc::toggle("autoRelease", "Auto Release", false),
    ParamDesc::new("keyBoostFreq", "Key Boost Freq", keyboost::FREQ_MIN, keyboost::FREQ_MAX, keyboost::FREQ_DEFAULT, Kind::Hz).log(),
    ParamDesc::new("keyBoost", "Key Boost", keyboost::GAIN_MIN, keyboost::GAIN_MAX, 0.0, Kind::Db),
    ParamDesc::new("keyBoostQ", "Key Boost Q", keyboost::Q_MIN, keyboost::Q_MAX, keyboost::Q_DEFAULT, Kind::Plain).log(),
];

/// The target gain (dB) the gate or expander asks for at `level`, with the open state for
/// the hysteresis. Public for the editor's curve.
pub fn target(gate: bool, level: f64, threshold: f64, ratio: f64, range: f64, knee: f64) -> f64 {
    if gate {
        curve::gate(level, threshold, range)
    } else {
        curve::expander(level, threshold, ratio, knee, range)
    }
}

pub struct Gate {
    v: [f32; ix::COUNT],
    sr: f64,
    det: Detector,
    gain: [GainFollower; 2],
    auto: AutoTime,
    /// Max of the target over the look-ahead window (opens early).
    early: [SlideMax; 2],
    /// Look-ahead ramp: the gain opens in a straight line that is fully open on the onset.
    ramp: [BoxAvg; 2],
    delay: [Delay; 2],
    open: [bool; 2],
    hold_left: [u32; 2],
    look: usize,
    zero: bool,
    hold: u32,
    att: f64,
    rel: f64,
    view: [f32; view::LEN],
    reduction: f32,
}

impl Default for Gate {
    fn default() -> Self {
        let mut s = Self {
            v: PARAMS.map(|p| p.default),
            sr: 48_000.0,
            det: Detector::default(),
            gain: Default::default(),
            auto: AutoTime::default(),
            early: Default::default(),
            ramp: Default::default(),
            delay: Default::default(),
            open: [false; 2],
            hold_left: [0; 2],
            look: 0,
            zero: false,
            hold: 0,
            att: 1.0,
            rel: 1.0,
            view: [0.0; view::LEN],
            reduction: 0.0,
        };
        s.update();
        s.reset();
        s
    }
}

impl Gate {
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
        self.hold = (v[ix::HOLD] as f64 * 1e-3 * self.sr).round() as u32;
    }

    fn run(&mut self, left: &mut [f32], right: &mut [f32], key: Option<(&[f32], &[f32])>) {
        let gate = (self.v[ix::MODE] as usize) == MODE_GATE;
        let thr = self.v[ix::THRESHOLD] as f64;
        let hyst = self.v[ix::HYSTERESIS] as f64;
        let ratio = self.v[ix::RATIO] as f64;
        let range = self.v[ix::RANGE] as f64;
        let knee = self.v[ix::KNEE] as f64;
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
            let x = [xl, xr];
            let mut y = [0.0; 2];
            for c in 0..2 {
                let l = levels[c];
                // Hysteresis: the threshold in use drops by `hyst` while open.
                if l >= thr {
                    self.open[c] = true;
                } else if l < thr - hyst {
                    self.open[c] = false;
                }
                let t_eff = if self.open[c] { thr - hyst } else { thr };
                let mut t = target(gate, l, t_eff, ratio, range, knee);
                let t_early = self.ramp[c].push(self.early[c].push(t, self.look + 1), self.look + 1);
                if l >= t_eff {
                    self.hold_left[c] = self.hold;
                }
                t = t_early;
                // Hold: the gain may rise but not fall until the hold runs out.
                if t < self.gain[c].g && self.hold_left[c] > 0 {
                    self.hold_left[c] -= 1;
                    t = self.gain[c].g;
                }
                let g = self.gain[c].step(t, self.rel, self.att);
                work = work.min(g);
                let d = self.delay[c].push(x[c] as f32, self.look) as f64;
                y[c] = d * db_to_lin(g);
                out[c] = out[c].min(g);
                lvl[c] = lvl[c].max(l);
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

impl Module for Gate {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.sr = sample_rate.max(1.0) as f64;
        let len = (MAX_LOOKAHEAD_MS as f64 * 1e-3 * self.sr).ceil() as usize + 4;
        for c in 0..2 {
            self.delay[c].alloc(len);
            self.early[c].alloc(len);
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
        // Start closed: silence before the first note stays silent.
        let closed = -(self.v[ix::RANGE] as f64);
        for c in 0..2 {
            self.gain[c].g = closed;
            self.early[c].clear();
            self.ramp[c].fill(closed);
            self.delay[c].clear();
        }
        self.open = [false; 2];
        self.hold_left = [0; 2];
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

    fn run(g: &mut Gate, x: &mut [f32]) {
        let mut r = x.to_vec();
        g.process(x, &mut r);
    }

    #[test]
    fn gate_closes_by_range_and_opens_on_signal() {
        let mut g = Gate::default();
        g.prepare(48_000.0, 48_000);
        g.set_param(ix::LOOKAHEAD, 0.0);
        let mut quiet = vec![0.001f32; 48_000]; // -60 dBFS, under -50
        run(&mut g, &mut quiet);
        assert!((quiet[47_999] / 0.001 - db_to_lin(-40.0) as f32).abs() < 1e-4);
        let mut loud = vec![0.1f32; 4800]; // -20 dBFS
        run(&mut g, &mut loud);
        assert!((loud[4799] - 0.1).abs() < 1e-5, "{}", loud[4799]);
    }

    #[test]
    fn hysteresis_keeps_it_open_just_under_threshold() {
        let mut g = Gate::default();
        g.prepare(48_000.0, 48_000);
        let mut a = vec![db_to_lin(-45.0) as f32; 9600];
        run(&mut g, &mut a);
        let mut b = vec![db_to_lin(-52.0) as f32; 48_000]; // inside the 3 dB hysteresis
        run(&mut g, &mut b);
        assert!((b[47_999] / db_to_lin(-52.0) as f32 - 1.0).abs() < 1e-4);
    }

    #[test]
    fn expander_settles_on_its_curve() {
        let mut g = Gate::default();
        g.prepare(48_000.0, 48_000);
        g.set_param(ix::MODE, 1.0);
        g.set_param(ix::KNEE, 0.0);
        g.set_param(ix::HYSTERESIS, 0.0);
        g.set_param(ix::LOOKAHEAD, 0.0);
        g.reset();
        let mut x = vec![db_to_lin(-60.0) as f32; 96_000];
        run(&mut g, &mut x);
        // 10 dB under at 1:2 -> 10 dB down.
        let got = crate::lin_to_db(x[95_999] as f64);
        assert!((got + 70.0).abs() < 0.01, "{got}");
    }

    #[test]
    fn lookahead_opens_before_the_onset() {
        let mut g = Gate::default();
        g.prepare(48_000.0, 48_000);
        g.set_param(ix::LOOKAHEAD, 2.0); // 96 samples
        // Minimum attack: the look-ahead ramp alone decides, and it ends on the onset.
        g.set_param(ix::ATTACK, 0.01);
        g.reset();
        let mut x = vec![0.0f32; 4800];
        for v in &mut x[1000..] {
            *v = 0.5;
        }
        run(&mut g, &mut x);
        // The onset leaves at 1096; the gate is open on that sample, and was closed before
        // the ramp began.
        assert!(x[1096] > 0.49, "{}", x[1096]);
        assert!(x[..1096].iter().all(|v| *v == 0.0));
    }
}
