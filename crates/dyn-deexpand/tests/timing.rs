//! Attack, hold, release and look-ahead against the follower's closed form; latency against an
//! impulse; onsets after a lifted passage.
mod common;
use common::*;
use dyn_deexpand::{static_gain_db, DeExpander};
use olm_api::Module;

// 2:1 below -40, no knee, no floor or range in the way: lift = (-40 - L) / 2
const T: f32 = -40.0;
const L_LOUD: f64 = -50.0;
const L_QUIET: f64 = -60.0;

fn base(extra: &[(&str, f32)]) -> DeExpander {
    let mut s = vec![("threshold", T), ("ratio", 2.0), ("knee", 0.0), ("range", 36.0), ("floor", -120.0), ("detector", 0.0), ("lookahead", 0.0)];
    s.extend_from_slice(extra);
    with(&s)
}

fn want(p: f64) -> f64 {
    static_gain_db(db(p), T as f64, 2.0, 0.0, 36.0, -120.0)
}

#[test]
fn attack_follows_one_pole() {
    for &tau in &[0.05f64, 0.5, 5.0, 30.0] {
        let mut m = base(&[("attack", tau as f32), ("hold", 0.0), ("release", 5000.0)]);
        let s = ms(200.0 + 20.0 * tau);
        let mut x = dc(L_QUIET, s);
        x.extend(dc(L_LOUD, ms(10.0 * tau + 20.0)));
        let y = render(&mut m, &x);
        let (v1, v2) = (lin(L_QUIET) as f32 as f64, lin(L_LOUD) as f32 as f64);
        let a = 1.0 - (-1.0 / (tau * 1.0e-3 * SRF)).exp();
        let mut worst = 0.0f64;
        for k in 0..(x.len() - s) {
            let p = v2 - (v2 - v1) * (1.0 - a).powi(k as i32 + 1);
            let got = db(y[s + k] as f64) - db(x[s + k] as f64);
            worst = worst.max((got - want(p)).abs());
        }
        println!("attack {tau} ms: worst error {worst:.2e} dB");
        assert!(worst < 1.0e-3);
    }
}

#[test]
fn hold_then_exponential_release() {
    for &(hold, tau) in &[(0.0f64, 50.0f64), (20.0, 150.0), (200.0, 1000.0)] {
        let mut m = base(&[("attack", 0.01), ("hold", hold as f32), ("release", tau as f32)]);
        let s = ms(100.0);
        let mut x = dc(L_LOUD, s);
        x.extend(dc(L_QUIET, ms(hold + 5.0 * tau)));
        let y = render(&mut m, &x);
        let (v1, v2) = (lin(L_QUIET) as f32 as f64, lin(L_LOUD) as f32 as f64);
        let r = (-1.0 / (tau * 1.0e-3 * SRF)).exp();
        let h = ms(hold);
        let mut worst = 0.0f64;
        for k in 0..(x.len() - s) {
            let p = if k < h { v2 } else { (v2 * r.powi((k - h) as i32 + 1)).max(v1) };
            let got = db(y[s + k] as f64) - db(x[s + k] as f64);
            worst = worst.max((got - want(p)).abs());
        }
        // the lift stays put for exactly the hold, then grows
        let g = |i: usize| db(y[i] as f64) - db(x[i] as f64);
        if h > 0 {
            assert!((g(s + h - 1) - g(s - 1)).abs() < 1.0e-5);
        }
        assert!(g(s + h) > g(s - 1) + 1.0e-5);
        // release: the level falls 8.686 dB per time constant, so the lift grows 4.343 dB
        let grow = g(s + h - 1 + ms(tau)) - g(s - 1);
        println!("hold {hold} ms, release {tau} ms: worst error {worst:.2e} dB, lift growth after one time constant {grow:.3} dB");
        assert!(worst < 1.0e-3);
        assert!((grow - 4.3429).abs() < 0.01);
    }
}

#[test]
fn hold_rides_over_short_gaps() {
    // a 10 ms gap inside a note: with a 20 ms hold the lift never moves
    let mut x = dc(L_LOUD, ms(200.0));
    x.extend(dc(L_QUIET, ms(10.0)));
    x.extend(dc(L_LOUD, ms(100.0)));
    let y = render(&mut base(&[("hold", 20.0), ("release", 50.0)]), &x);
    let g0 = db(y[ms(199.0)] as f64) - L_LOUD;
    for i in ms(199.0)..x.len() {
        assert!((db(y[i] as f64) - db(x[i] as f64) - g0).abs() < 1.0e-6, "at {i}");
    }
    let y = render(&mut base(&[("hold", 0.0), ("release", 50.0)]), &x);
    assert!(db(y[ms(209.0)] as f64) - L_QUIET > g0 + 0.5);
}

#[test]
fn lookahead_moves_the_attack_earlier_and_not_the_release() {
    let s = ms(300.0);
    let len = ms(60.0);
    let mut x = dc(L_QUIET, s);
    x.extend(dc(L_LOUD, len));
    x.extend(dc(L_QUIET, ms(100.0)));
    let common = [("attack", 2.0f32), ("hold", 5.0), ("release", 20.0)];
    let plain = render(&mut base(&common), &x);
    for &la in &[1.0f32, 5.0, 10.0] {
        let d = ms(la as f64);
        let mut c = common.to_vec();
        c.push(("lookahead", la));
        let mut m = base(&c);
        assert_eq!(m.latency(), d);
        let y = render(&mut m, &x);
        let gain = |v: &[f32], i: usize| db(v[i] as f64) - db(x[i] as f64);
        // OpenDynamics: the look-ahead ramp starts exactly d samples early, so the lift is
        // gone no later than without look-ahead and is further down at the onset.
        assert!((gain(&y, s - d - 1) - 10.0).abs() < 1.0e-4);
        assert!(gain(&y, s - d) <= 10.0);
        for k in 0..ms(30.0) {
            assert!(gain(&y, s + k) <= gain(&plain, s + k) + 1.0e-3, "look-ahead {la}, k {k}");
        }
        assert!(gain(&y, s) < gain(&plain, s) - 0.1);
        // the lift comes back no earlier than without look-ahead
        let e = s + len;
        for k in 0..ms(60.0) {
            assert!(gain(&y, e + k) <= gain(&plain, e + k) + 1.0e-3);
        }
    }
}

#[test]
fn lookahead_stops_onsets_jumping_out() {
    // a lifted quiet tail, then a loud onset: without look-ahead the onset gets the lift until
    // the attack catches up; with the defaults (0.5 ms attack, 2 ms look-ahead) it does not
    let mut x = sine(500.0, lin(-60.0), ms(500.0));
    x.extend(sine(500.0, lin(-6.0), ms(100.0)));
    let onset = ms(500.0);
    let over = |la: f32| {
        let mut m = with(&[("lookahead", la), ("detector", 0.0)]);
        let y = render(&mut m, &x);
        (onset..onset + ms(20.0)).map(|i| db(y[i] as f64) - db(x[i] as f64)).filter(|g| g.is_finite()).fold(f64::NEG_INFINITY, f64::max)
    };
    let (without, with_la) = (over(0.0), over(2.0));
    println!("largest gain on the onset: no look-ahead {without:.2} dB, 2 ms look-ahead {with_la:.4} dB");
    assert!(without > 3.0);
    assert!(with_la < 0.05);
}

#[test]
fn latency_equals_impulse_delay() {
    for &la in &[0.0f32, 0.5, 2.0, 10.0] {
        for &sr in &[44_100.0f32, 48_000.0, 96_000.0] {
            let mut m = DeExpander::default();
            m.prepare(sr, 256);
            set(&mut m, "ratio", 1.0);
            set(&mut m, "lookahead", la);
            m.reset();
            let lat = m.latency();
            assert_eq!(lat, (la as f64 * 1.0e-3 * sr as f64).round() as usize);
            let mut l = vec![0.0f32; 2048];
            l[100] = 0.5;
            let mut r = l.clone();
            m.process(&mut l, &mut r);
            let at = l.iter().position(|v| *v != 0.0).unwrap();
            assert_eq!(at, 100 + lat);
            assert_eq!(l[at], 0.5);
        }
    }
    assert_eq!(with(&[]).latency(), 96);
}
