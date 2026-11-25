//! Static curve: the public function against the closed form, and the audio path against it on
//! settled signals. Floor and range limits.
mod common;
use common::*;
use dyn_deexpand::static_gain_db;

fn closed(l: f64, t: f64, r: f64, w: f64, range: f64, floor: f64) -> f64 {
    let s = 1.0 - 1.0 / r;
    let u = t - l;
    let c = if w > 0.0 && u.abs() <= w / 2.0 {
        s * (u + w / 2.0).powi(2) / (2.0 * w)
    } else if u > 0.0 {
        s * u
    } else {
        0.0
    };
    if l <= floor {
        return 0.0;
    }
    c.min(range).min(l - floor).max(0.0)
}

#[test]
fn function_matches_closed_form() {
    let mut worst = 0.0f64;
    for &r in &[1.0, 1.5, 2.0, 4.0, 8.0] {
        for &w in &[0.0, 3.0, 12.0, 24.0] {
            for &range in &[0.0, 6.0, 12.0, 36.0] {
                for &(t, floor) in &[(-40.0, -72.0), (-20.0, -120.0), (0.0, -60.0), (-60.0, -50.0)] {
                    let mut l = -140.0;
                    while l <= 12.0 {
                        let a = static_gain_db(l, t, r, w, range, floor);
                        let b = closed(l, t, r, w, range, floor);
                        worst = worst.max((a - b).abs());
                        l += 0.25;
                    }
                }
            }
        }
    }
    println!("worst |function - closed form| = {worst:e} dB");
    assert!(worst < 1.0e-9);
}

#[test]
fn curve_shape() {
    let (t, r, w, range, floor) = (-40.0, 2.0, 6.0, 12.0, -90.0);
    // continuous everywhere, output level never falls as input rises, lift within 0..range
    let mut prev_o = f64::NEG_INFINITY;
    let mut prev_g = 0.0;
    let mut l = -130.0;
    while l < 10.0 {
        let g = static_gain_db(l, t, r, w, range, floor);
        assert!((0.0..=range).contains(&g));
        assert!((g - prev_g).abs() < 0.011, "jump at {l}");
        assert!(l + g >= prev_o - 1.0e-12);
        prev_o = l + g;
        prev_g = g;
        l += 0.01;
    }
    // r:1 below the knee: output slope 1/r
    let o = |l: f64| l + static_gain_db(l, t, r, w, 100.0, -200.0);
    assert!(((o(-50.0) - o(-60.0)) / 10.0 - 0.5).abs() < 1.0e-12);
    // nothing at or below the floor, 1 dB per dB just above it
    assert_eq!(static_gain_db(floor, t, r, w, range, floor), 0.0);
    assert_eq!(static_gain_db(floor - 10.0, t, r, w, range, floor), 0.0);
    assert!((static_gain_db(floor + 2.0, t, r, w, range, floor) - 2.0).abs() < 1.0e-12);
    // 1:1 does nothing
    for l in [-100.0, -50.0, -40.0, 0.0] {
        assert_eq!(static_gain_db(l, t, 1.0, w, range, floor), 0.0);
    }
}

#[test]
fn inverts_a_downward_expander() {
    // 1:r expander below T, then r:1 de-expander at the same T: levels come back exactly
    for &r in &[1.5, 2.0, 4.0] {
        let t = -40.0;
        for i in 0..40 {
            let l = -70.0 + i as f64;
            let e = if l < t { t + r * (l - t) } else { l };
            let back = e + static_gain_db(e, t, r, 0.0, 200.0, -300.0);
            assert!((back - l).abs() < 1.0e-9, "r {r} level {l}: back {back}");
        }
    }
}

fn settled_gain(settings: &[(&str, f32)], level: f64) -> f64 {
    let mut m = with(settings);
    let x = dc(level, ms(300.0));
    let y = render(&mut m, &x);
    db(y[y.len() - 1] as f64) - level
}

#[test]
fn audio_path_on_dc_matches_curve() {
    let mut worst = 0.0f64;
    for &(t, r, w, range, floor) in &[(-40.0f32, 2.0f32, 0.0f32, 36.0f32, -120.0f32), (-40.0, 2.0, 6.0, 12.0, -72.0), (-20.0, 8.0, 12.0, 24.0, -60.0), (-50.0, 1.5, 3.0, 6.0, -90.0)] {
        for i in 0..20 {
            let l = -110.0 + 5.5 * i as f64;
            let s = [("threshold", t), ("ratio", r), ("knee", w), ("range", range), ("floor", floor), ("detector", 0.0)];
            let got = settled_gain(&s, l);
            let want = static_gain_db(l, t as f64, r as f64, w as f64, range as f64, floor as f64);
            worst = worst.max((got - want).abs());
            assert!((got - want).abs() < 1.0e-3, "{s:?} level {l}: {got} vs {want}");
        }
    }
    println!("worst audio-path error on DC: {worst:.2e} dB");
}

#[test]
fn rms_detector_reads_sine_rms() {
    let (t, r, w, range, floor) = (-30.0f32, 2.0f32, 0.0f32, 36.0f32, -120.0f32);
    let mut m = with(&[("threshold", t), ("ratio", r), ("knee", w), ("range", range), ("floor", floor), ("detector", 1.0), ("detectorWindow", 30.0)]);
    let x = sine(1000.0, lin(-50.0), ms(800.0));
    let y = render(&mut m, &x);
    let a = ms(600.0);
    let g = db(tone_amp(&y, 1000.0, a, a + 4800)) + 50.0;
    let want = static_gain_db(-50.0 - 3.0103, t as f64, r as f64, w as f64, range as f64, floor as f64);
    println!("RMS sine: lift {g:.4} dB, curve {want:.4} dB");
    assert!((g - want).abs() < 0.05);
}

#[test]
fn floor_leaves_noise_untouched() {
    // noise peaking at -80 dBFS under a -72 dB floor passes bit for bit; silence stays silent
    for det in [0.0f32, 1.0] {
        let x: Vec<f32> = noise(ms(500.0), 41, lin(-80.0) as f32);
        let mut m = with(&[("detector", det)]);
        let y = render(&mut m, &x);
        assert!(y.iter().zip(&x).all(|(a, b)| a.to_bits() == b.to_bits()), "detector {det}");
        let z = vec![0.0f32; 4096];
        let mut m = with(&[("detector", det)]);
        assert!(render(&mut m, &z).iter().all(|v| *v == 0.0));
    }
}

#[test]
fn range_limits_the_lift() {
    for &range in &[0.0f32, 3.0, 12.0, 36.0] {
        let mut m = with(&[("threshold", 0.0), ("ratio", 8.0), ("range", range), ("floor", -120.0), ("detector", 0.0), ("hold", 0.0)]);
        let x = noise(ms(400.0), 5, lin(-60.0) as f32);
        let y = render(&mut m, &x);
        let most = y.iter().zip(&x).filter(|(_, b)| b.abs() > 1.0e-9).map(|(a, b)| db(*a as f64) - db(*b as f64)).fold(f64::NEG_INFINITY, f64::max);
        println!("range {range} dB: largest lift {most:.4} dB");
        assert!(most <= range as f64 + 1.0e-4);
        if range > 0.0 {
            assert!(most > range as f64 - 0.01);
        }
    }
}
