//! Attack, release and look-ahead against the follower's closed form; latency against an impulse.
mod common;
use common::*;
use dyn_decompress::{static_gain_db, Decompressor};
use olm_api::Module;

// slope 1 (1:2), no knee, no range limit in the levels used: gain = L - T
const T: f32 = -56.0;
const LEVEL_LOW: f64 = -50.0;
const LEVEL_HIGH: f64 = -36.0;

fn base(extra: &[(&str, f32)]) -> Decompressor {
    let mut s = vec![("threshold", T), ("ratio", 2.0), ("knee", 0.0), ("range", 24.0), ("ceilingOn", 0.0)];
    s.extend_from_slice(extra);
    with(&s)
}

/// Envelope (linear) the output implies: gain = L - T, so L = gain + T.
fn envelope(y: &[f32], x: &[f32]) -> Vec<f64> {
    y.iter().zip(x).map(|(a, b)| lin(db(*a as f64) - db(*b as f64) + T as f64)).collect()
}

#[test]
fn attack_follows_one_pole() {
    for &tau in &[0.1f64, 1.0, 5.0, 30.0] {
        let mut m = base(&[("attack", tau as f32), ("release", 2000.0)]);
        let s = ms(200.0 + 20.0 * tau);
        let mut x = dc(LEVEL_LOW, s);
        x.extend(dc(LEVEL_HIGH, ms(10.0 * tau + 20.0)));
        let y = render(&mut m, &x);
        let (v1, v2) = (lin(LEVEL_LOW) as f32 as f64, lin(LEVEL_HIGH) as f32 as f64);
        let a = 1.0 - (-1.0 / (tau * 1.0e-3 * SRF)).exp();
        let mut worst = 0.0f64;
        for k in 0..(x.len() - s) {
            let p = v2 - (v2 - v1) * (1.0 - a).powi(k as i32 + 1);
            let want = static_gain_db(db(p), T as f64, 0.5, 0.0, 24.0);
            let got = db(y[s + k] as f64) - db(x[s + k] as f64);
            worst = worst.max((got - want).abs());
        }
        // 63 % of the linear step
        let env = envelope(&y, &x);
        let k63 = (s..x.len()).find(|&i| (env[i] - v1) / (v2 - v1) >= 1.0 - (-1.0f64).exp()).unwrap() - s;
        let t63 = (k63 + 1) as f64 / SRF * 1.0e3;
        println!("attack {tau} ms: worst error {worst:.2e} dB, 63 % at {t63:.3} ms");
        assert!(worst < 1.0e-3);
        assert!((t63 - tau).abs() <= 1.0e3 / SRF + 1.0e-9, "63 % at {t63} ms");
    }
}

#[test]
fn release_is_exponential() {
    for &tau in &[5.0f64, 80.0, 400.0] {
        let mut m = base(&[("attack", 0.01), ("release", tau as f32)]);
        let s = ms(100.0);
        let mut x = dc(LEVEL_HIGH, s);
        x.extend(dc(LEVEL_LOW, ms(4.0 * tau + dyn_decompress::LEVEL_HOLD_MS)));
        let y = render(&mut m, &x);
        let (v1, v2) = (lin(LEVEL_LOW) as f32 as f64, lin(LEVEL_HIGH) as f32 as f64);
        let r = (-1.0 / (tau * 1.0e-3 * SRF)).exp();
        let mut worst = 0.0f64;
        // The level hold keeps the high level for LEVEL_HOLD_MS after it ends, then the
        // exponential release runs.
        let h = ms(dyn_decompress::LEVEL_HOLD_MS);
        for k in 0..(x.len() - s) {
            let p = if k < h { v2 } else { (v2 * r.powi((k - h) as i32 + 1)).max(v1) };
            let want = static_gain_db(db(p), T as f64, 0.5, 0.0, 24.0);
            let got = db(y[s + k] as f64) - db(x[s + k] as f64);
            worst = worst.max((got - want).abs());
        }
        // 8.686 dB per time constant: level after tau
        let env = envelope(&y, &x);
        let drop = db(env[s - 1]) - db(env[s - 1 + h + ms(tau)]);
        println!("release {tau} ms: worst error {worst:.2e} dB, drop after one time constant {drop:.3} dB");
        assert!(worst < 1.0e-3);
        assert!((drop - 8.6859).abs() < 0.01);
    }
}

#[test]
fn lookahead_moves_the_attack_earlier_and_not_the_release() {
    let s = ms(200.0);
    let len = ms(60.0);
    let mut x = dc(LEVEL_LOW, s);
    x.extend(dc(LEVEL_HIGH, len));
    x.extend(dc(LEVEL_LOW, ms(100.0)));
    let plain = render(&mut base(&[("attack", 2.0), ("release", 20.0)]), &x);
    for &la in &[1.0f32, 5.0, 10.0] {
        let d = ms(la as f64);
        let mut m = base(&[("attack", 2.0), ("release", 20.0), ("lookahead", la)]);
        assert_eq!(m.latency(), d + Decompressor::default().latency_at(SR));
        let y = render(&mut m, &x);
        let gain = |v: &[f32], i: usize| db(v[i] as f64) - db(x[i] as f64);
        // OpenDynamics: the look-ahead ramp starts exactly d samples early and glides, so the
        // lift is never later than without look-ahead and is further along at the onset.
        assert!((gain(&y, s - d - 1) - 6.0).abs() < 1.0e-4);
        assert!(gain(&y, s - d) >= gain(&y, s - d - 1));
        for k in 0..ms(30.0) {
            let (a, b) = (gain(&y, s + k), gain(&plain, s + k));
            assert!(a >= b - 1.0e-3, "look-ahead {la} ms, k {k}: {a} vs {b}");
        }
        assert!(gain(&y, s) > gain(&plain, s) + 0.1);
        // the release starts with the audio and ramps over the look-ahead: never earlier
        let e = s + len;
        for k in 0..ms(60.0) {
            assert!(gain(&y, e + k) >= gain(&plain, e + k) - 1.0e-3);
        }
    }
}

#[test]
fn latency_equals_impulse_delay() {
    for &la in &[0.0f32, 0.5, 3.0, 10.0] {
        for &sr in &[44_100.0f32, 48_000.0, 96_000.0] {
            let mut m = Decompressor::default();
            m.prepare(sr, 256);
            set(&mut m, "ratio", 1.0);
            set(&mut m, "lookahead", la);
            m.reset();
            let lat = m.latency();
            let mut l = vec![0.0f32; 4096];
            l[100] = 0.5;
            let mut r = l.clone();
            m.process(&mut l, &mut r);
            let at = l.iter().position(|v| *v != 0.0).unwrap();
            assert_eq!(at, 100 + lat, "look-ahead {la} ms at {sr} Hz");
            assert_eq!(l[at], 0.5);
            let w = (1.5e-3 * sr as f64).round() as usize;
            assert_eq!(lat, (la as f64 * 1.0e-3 * sr as f64).round() as usize + dyn_decompress::TP_DELAY + w - 1);
        }
    }
    assert_eq!(with(&[]).latency(), 103);
}

trait LatencyAt {
    fn latency_at(self, sr: f32) -> usize;
}
impl LatencyAt for Decompressor {
    fn latency_at(mut self, sr: f32) -> usize {
        self.prepare(sr, 64);
        self.set_param(dyn_decompress::index::LOOKAHEAD, 0.0);
        self.latency()
    }
}
