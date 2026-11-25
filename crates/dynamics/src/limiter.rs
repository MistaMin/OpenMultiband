//! Simple peak limiter, no look-ahead, no latency.
//!
//! The signal is raised by `ceiling - threshold`, then any sample over the ceiling is turned
//! down by an infinite-ratio gain computer: target gain = `min(0, ceiling - level)` in dB.
//! With Attack at 0 the gain drops on the sample that needs it, so no sample passes the
//! ceiling. A non-zero attack lets the first part of a transient through (softer, less
//! distortion on fast peaks) and the ceiling is then a target, not a guarantee. Release is a
//! one-pole on the gain in dB. The audio is only multiplied: nothing is clipped.
//!
//! Clean (default on): the reduction is held for a while before it may release, the way
//! OpenLimiter's Pure engine does it, still without look-ahead. Without a hold the gain
//! follows each half-cycle of a bass note and adds harmonics; held over at least a
//! half-cycle it stays still on a steady tone. The hold is bass-adaptive: 0.5 ms on bright
//! material (so hats still release fast) up to Bass Hold when the 20 ms peak is mostly
//! below 180 Hz. The held reduction is never less than the instant one, so Attack 0 still
//! keeps every sample under the ceiling.
//!
//! Clean also sets the release itself (the Release knob is out of use): OpenLimiter's crest
//! rule, `AUTO_RELEASE_MS * 4^((12 - crest) / 9)` with crest = 30 ms peak against 30 ms RMS,
//! clamped to 3..21 dB. Dense material (low crest) releases slowly, 400 ms, so the gain does
//! not pump; spiky material releases fast, down to 25 ms, so it does not duck.
//!
//! Auto Release with Clean off: the same crest rule around the Release knob instead of
//! 100 ms. Auto Attack: the attack scaled by how hard the limiter works (see [`crate::auto`]).

use olm_api::{Kind, Meters, Module, ParamDesc};

use crate::detector::GainFollower;
use crate::auto::AutoTime;
use crate::parts::{one_pole, BoxAvg, SlideMax};
use crate::{clean, db_to_lin, lin_to_db, view, FLOOR_DB};

pub mod index {
    pub const THRESHOLD: usize = 0;
    pub const CEILING: usize = 1;
    pub const ATTACK: usize = 2;
    pub const RELEASE: usize = 3;
    pub const STEREO_LINK: usize = 4;
    pub const CLEAN: usize = 5;
    pub const BASS_HOLD: usize = 6;
    pub const AUTO_ATTACK: usize = 7;
    pub const AUTO_RELEASE: usize = 8;
    pub const COUNT: usize = 9;
}
use index as ix;

const SMOOTH_MS: f64 = 5.0;
/// Clean: shortest hold, the bass split, the window the bass share is read over and how
/// fast that share moves (OpenLimiter Pure's values).
const MIN_HOLD_MS: f64 = 0.5;
const BASS_HZ: f64 = 180.0;
const SHARE_WINDOW_MS: f64 = 20.0;
const SHARE_SMOOTH_MS: f64 = 50.0;
pub const MAX_BASS_HOLD_MS: f32 = 20.0;
/// Clean's automatic release: the time at a 12 dB crest, and the crest window.
pub const AUTO_RELEASE_MS: f64 = 100.0;
const CREST_WINDOW_MS: f64 = 30.0;
/// The release coefficient follows the crest every this many samples (it costs an `exp`).
const CREST_EVERY: u32 = 16;

/// Clean's release time, ms, for a crest factor in dB.
pub fn auto_release_ms(crest_db: f64) -> f64 {
    AUTO_RELEASE_MS * crest_factor(crest_db)
}

/// OpenLimiter's crest rule: 4x at 3 dB, 1x at 12 dB, 0.25x at 21 dB.
pub fn crest_factor(crest_db: f64) -> f64 {
    4f64.powf((12.0 - crest_db.clamp(3.0, 21.0)) / 9.0)
}

pub static PARAMS: [ParamDesc; ix::COUNT] = [
    ParamDesc::new("threshold", "Threshold", -24.0, 0.0, 0.0, Kind::Db),
    ParamDesc::new("ceiling", "Ceiling", -24.0, 0.0, -1.0, Kind::Db),
    ParamDesc::new("attack", "Attack", 0.0, 10.0, 0.0, Kind::Ms),
    ParamDesc::new("release", "Release", 1.0, 2000.0, 100.0, Kind::Ms).log(),
    ParamDesc::new("stereoLink", "Stereo Link", 0.0, 100.0, 100.0, Kind::Percent),
    ParamDesc::toggle("clean", "Clean", true),
    ParamDesc::new("bassHold", "Bass Hold", 0.0, MAX_BASS_HOLD_MS, 14.0, Kind::Ms),
    ParamDesc::toggle("autoAttack", "Auto Attack", false),
    ParamDesc::toggle("autoRelease", "Auto Release", false),
];

pub struct Limiter {
    v: [f32; ix::COUNT],
    sr: f64,
    gain: [GainFollower; 2],
    att: f64,
    rel: f64,
    smooth: f64,
    drive: f64,
    view: [f32; view::LEN],
    reduction: f32,
    // Clean
    hold: [SlideMax; 2],
    lp: [f64; 2],
    a_lp: f64,
    lo_max: SlideMax,
    pk_max: SlideMax,
    w_share: usize,
    share: f64,
    k_share: f64,
    crest_ms: BoxAvg,
    crest_pk: SlideMax,
    w_crest: usize,
    crest_db: f64,
    auto_rel: f64,
    tick: u32,
    auto: AutoTime,
}

impl Default for Limiter {
    fn default() -> Self {
        let mut s = Self {
            v: PARAMS.map(|p| p.default),
            sr: 48_000.0,
            gain: Default::default(),
            att: 1.0,
            rel: 1.0,
            smooth: 1.0,
            drive: 0.0,
            view: [0.0; view::LEN],
            reduction: 0.0,
            hold: Default::default(),
            lp: [0.0; 2],
            a_lp: 0.0,
            lo_max: SlideMax::default(),
            pk_max: SlideMax::default(),
            w_share: 1,
            share: 0.0,
            k_share: 1.0,
            crest_ms: BoxAvg::default(),
            crest_pk: SlideMax::default(),
            w_crest: 1,
            crest_db: 12.0,
            auto_rel: 1.0,
            tick: 0,
            auto: AutoTime::default(),
        };
        s.update();
        s.drive = s.drive_target();
        s
    }
}

impl Limiter {
    fn update(&mut self) {
        // Attack 0 is a coefficient of 1: the gain takes the target on the same sample.
        self.att = self.attack_coef();
        self.rel = one_pole(self.v[ix::RELEASE] as f64 * 1e-3, self.sr);
        self.smooth = one_pole(SMOOTH_MS * 1e-3, self.sr);
        self.a_lp = 1.0 - (-std::f64::consts::TAU * BASS_HZ / self.sr).exp();
        self.k_share = one_pole(SHARE_SMOOTH_MS * 1e-3, self.sr);
        self.w_share = (SHARE_WINDOW_MS * 1e-3 * self.sr).round() as usize;
        self.w_crest = ((CREST_WINDOW_MS * 1e-3 * self.sr).round() as usize).max(1);
        self.auto_rel = self.auto_release_coef();
    }

    fn attack_coef(&self) -> f64 {
        let a = if self.v[ix::AUTO_ATTACK] >= 0.5 { self.auto.attack() } else { 1.0 };
        one_pole(self.v[ix::ATTACK] as f64 * a * 1e-3, self.sr)
    }

    /// Clean: 100 ms around the crest rule; Auto Release alone: the Release knob instead.
    fn auto_release_coef(&self) -> f64 {
        let base = if self.v[ix::CLEAN] >= 0.5 { AUTO_RELEASE_MS } else { self.v[ix::RELEASE] as f64 };
        one_pole(base * crest_factor(self.crest_db) * 1e-3, self.sr)
    }

    fn drive_target(&self) -> f64 {
        (self.v[ix::CEILING] - self.v[ix::THRESHOLD]) as f64
    }
}

impl Module for Limiter {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.sr = sample_rate.max(1.0) as f64;
        let hold_len = (MAX_BASS_HOLD_MS as f64 * 1e-3 * self.sr).ceil() as usize + 4;
        for h in &mut self.hold {
            h.alloc(hold_len);
        }
        let share_len = (SHARE_WINDOW_MS * 1e-3 * self.sr).ceil() as usize + 4;
        self.lo_max.alloc(share_len);
        self.pk_max.alloc(share_len);
        let crest_len = (CREST_WINDOW_MS * 1e-3 * self.sr).ceil() as usize + 4;
        self.crest_ms.alloc(crest_len);
        self.crest_pk.alloc(crest_len);
        self.update();
        self.reset();
    }

    fn reset(&mut self) {
        self.gain = Default::default();
        self.drive = self.drive_target();
        self.hold.iter_mut().for_each(SlideMax::clear);
        self.lo_max.clear();
        self.pk_max.clear();
        self.lp = [0.0; 2];
        self.share = 0.0;
        self.crest_ms.clear();
        self.crest_pk.clear();
        self.crest_db = 12.0;
        self.tick = 0;
        self.auto.clear();
        self.auto.set_rate(self.sr);
        self.auto_rel = self.auto_release_coef();
        self.att = self.attack_coef();
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let Some(p) = PARAMS.get(index) else { return };
        self.v[index] = if value.is_finite() { value.clamp(p.min, p.max) } else { p.default };
        self.update();
    }

    fn latency(&self) -> usize {
        0
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let ceiling = self.v[ix::CEILING] as f64;
        let link = (self.v[ix::STEREO_LINK] as f64 / 100.0).clamp(0.0, 1.0);
        let drive_t = self.drive_target();
        let clean_on = self.v[ix::CLEAN] >= 0.5;
        let auto_rel_on = clean_on || self.v[ix::AUTO_RELEASE] >= 0.5;
        let auto_att_on = self.v[ix::AUTO_ATTACK] >= 0.5;
        let bass_hold = self.v[ix::BASS_HOLD] as f64;
        let ms = 1e-3 * self.sr;
        let mut out = [0.0f64; 2];
        let mut lvl = [FLOOR_DB; 2];
        let n = left.len().min(right.len());
        for i in 0..n {
            self.drive += (drive_t - self.drive) * self.smooth;
            let pre = db_to_lin(self.drive);
            let x = [left[i] as f64 * pre, right[i] as f64 * pre];
            let l = [lin_to_db(x[0].abs()), lin_to_db(x[1].abs())];
            let top = l[0].max(l[1]);
            // Clean: hold length from the bass share of the last 20 ms of peaks.
            let mut hold = 1;
            if clean_on {
                for c in 0..2 {
                    self.lp[c] += self.a_lp * (x[c] - self.lp[c]);
                }
                let lo = self.lo_max.push(self.lp[0].abs().max(self.lp[1].abs()), self.w_share);
                let pk = self.pk_max.push(x[0].abs().max(x[1].abs()), self.w_share);
                let share = if pk > 1e-9 { (lo / pk).clamp(0.0, 1.0) } else { 0.0 };
                self.share += (share - self.share) * self.k_share;
                let h_ms = MIN_HOLD_MS.min(bass_hold) + (bass_hold - MIN_HOLD_MS).max(0.0) * self.share;
                hold = ((h_ms * ms).round() as usize).max(1);
            }
            if auto_rel_on {
                // Auto release from the crest of the last 30 ms.
                let m2 = self.crest_ms.push(0.5 * (x[0] * x[0] + x[1] * x[1]), self.w_crest);
                let p = self.crest_pk.push(x[0].abs().max(x[1].abs()), self.w_crest);
                self.tick = self.tick.wrapping_add(1);
                if self.tick % CREST_EVERY == 0 {
                    self.crest_db = if m2 > 1e-14 && p > 1e-7 { lin_to_db(p) - 10.0 * m2.log10() } else { 12.0 };
                    self.auto_rel = self.auto_release_coef();
                }
            }
            let rel = if auto_rel_on { self.auto_rel } else { self.rel };
            let mut work = 0.0f64;
            let mut y = [0.0; 2];
            for c in 0..2 {
                // Linking moves a channel's level toward the louder one, never below its own,
                // so the ceiling holds per channel at any link.
                let level = l[c] + (top - l[c]) * link;
                let mut target = (ceiling - level).min(0.0);
                if clean_on {
                    // The deepest reduction of the hold window; at most the instant one.
                    target = -self.hold[c].push(-target, hold);
                }
                let g = self.gain[c].step(target, self.att, rel);
                work = work.min(g);
                y[c] = x[c] * db_to_lin(g);
                out[c] = out[c].min(g);
                lvl[c] = lvl[c].max(l[c]);
            }
            left[i] = clean(y[0]);
            right[i] = clean(y[1]);
            if auto_att_on && self.auto.push(work) {
                self.att = self.attack_coef();
            }
        }
        self.view[view::GAIN] = out[0] as f32;
        self.view[view::GAIN + 1] = out[1] as f32;
        self.view[view::DETECTOR] = lvl[0] as f32;
        self.view[view::DETECTOR + 1] = lvl[1] as f32;
        self.reduction = (-out[0].min(out[1])) as f32;
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

    #[test]
    fn no_sample_passes_the_ceiling_with_attack_zero() {
        let mut lim = Limiter::default();
        lim.prepare(44_100.0, 8192);
        lim.set_param(ix::THRESHOLD, -12.0);
        lim.set_param(ix::CEILING, -1.0);
        lim.reset();
        let ceil = db_to_lin(-1.0) as f32 * (1.0 + 1e-6);
        // Noise-like bursts with random peaks, and a hard square.
        let mut seed = 1u32;
        let mut l: Vec<f32> = (0..44_100)
            .map(|i| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let r = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
                if (i / 2000) % 2 == 0 { r } else if i % 50 < 25 { 0.9 } else { -0.9 }
            })
            .collect();
        let mut r: Vec<f32> = l.iter().map(|v| v * 0.3).collect();
        lim.process(&mut l, &mut r);
        let peak = l.iter().chain(&r).fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak <= ceil, "{peak} over {ceil}");
        assert!(lim.meters().reduction_db > 5.0);
    }

    #[test]
    fn clean_keeps_the_ceiling_too() {
        let mut lim = Limiter::default();
        lim.prepare(48_000.0, 48_000);
        lim.set_param(ix::THRESHOLD, -12.0);
        lim.set_param(ix::CEILING, -1.0);
        lim.reset();
        let mut seed = 7u32;
        let mut l: Vec<f32> = (0..48_000)
            .map(|i| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let r = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
                0.6 * (i as f32 * 0.006).sin() + 0.4 * r
            })
            .collect();
        let mut r = l.clone();
        lim.process(&mut l, &mut r);
        let peak = l.iter().chain(&r).fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak <= db_to_lin(-1.0) as f32 * (1.0 + 1e-6), "{peak}");
    }

    /// A steady tone 5 dB over the ceiling: plain, the gain rides every half-cycle;
    /// Clean holds it still.
    #[test]
    fn clean_bass_gets_no_harmonics() {
        for f in [40.0, 50.0, 100.0, 1000.0] {
            let run = |clean: f32| {
                let sr = 48_000.0;
                let mut lim = Limiter::default();
                lim.prepare(sr as f32, 96_000);
                lim.set_param(ix::THRESHOLD, -6.0);
                lim.set_param(ix::CLEAN, clean);
                lim.reset();
                let mut l: Vec<f32> = (0..96_000).map(|i| (0.9 * (std::f64::consts::TAU * f * i as f64 / sr).sin()) as f32).collect();
                let mut r = l.clone();
                lim.process(&mut l, &mut r);
                crate::comp::thd::thd_db(&l[48_000..], f, sr)
            };
            let (plain, clean) = (run(0.0), run(1.0));
            println!("{f:6} Hz: plain THD {plain:7.1} dB, clean {clean:7.1} dB");
            assert!(clean < -120.0, "{f} Hz: {clean}");
            assert!(plain > -80.0, "{f} Hz plain {plain}: the test tone does not reach the limiter");
        }
    }

    #[test]
    fn auto_release_follows_the_crest() {
        assert!((auto_release_ms(12.0) - 100.0).abs() < 1e-9);
        assert!((auto_release_ms(0.0) - 400.0).abs() < 1e-9);
        assert!((auto_release_ms(30.0) - 25.0).abs() < 1e-9);
        // Clean ignores the Release knob.
        let run = |release: f32| {
            let mut lim = Limiter::default();
            lim.prepare(48_000.0, 48_000);
            lim.set_param(ix::THRESHOLD, -12.0);
            lim.set_param(ix::RELEASE, release);
            lim.reset();
            let mut l: Vec<f32> = (0..48_000).map(|i| if i < 24_000 { 0.8 * (i as f32 * 0.05).sin() } else { 0.01 }).collect();
            let mut r = l.clone();
            lim.process(&mut l, &mut r);
            l
        };
        assert_eq!(run(5.0), run(2000.0));
    }

    #[test]
    fn quiet_signal_only_gets_the_makeup() {
        let mut lim = Limiter::default();
        lim.prepare(48_000.0, 512);
        lim.set_param(ix::THRESHOLD, -6.0);
        lim.set_param(ix::CEILING, 0.0);
        lim.reset();
        let mut l = vec![0.1f32; 512];
        let mut r = l.clone();
        lim.process(&mut l, &mut r);
        let want = 0.1 * db_to_lin(6.0) as f32;
        assert!((l[511] - want).abs() < 1e-6, "{} vs {want}", l[511]);
        assert_eq!(lim.latency(), 0);
    }
}
