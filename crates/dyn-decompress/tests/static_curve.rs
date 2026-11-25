//! Static curve: the public function against the closed form, and the audio path against the
//! public function on settled signals.
mod common;
use common::*;
use dyn_decompress::{match_range_db, static_gain_db, transient_threshold_db};

/// The closed form written out independently of the crate.
fn closed(l: f64, t: f64, ratio: f64, w: f64, r: f64) -> f64 {
    let x = 1.0 / ratio;
    let u = l - t;
    let g = if w > 0.0 && u.abs() <= w / 2.0 {
        (x - 1.0) * (u + w / 2.0).powi(2) / (2.0 * w)
    } else if u > 0.0 {
        (x - 1.0) * u
    } else {
        0.0
    };
    g.clamp(0.0, r)
}

#[test]
fn function_matches_closed_form() {
    let mut worst = 0.0f64;
    for &ratio in &[1.0, 1.0 / 1.2, 1.0 / 1.5, 0.5, 0.25] {
        for &w in &[0.0, 3.0, 6.0, 24.0] {
            for &r in &[0.0, 3.0, 12.0, 24.0] {
                for &t in &[-60.0, -30.0, -18.0, 0.0] {
                    let mut l = -90.0;
                    while l <= 12.0 {
                        let a = static_gain_db(l, t, ratio, w, r);
                        let b = closed(l, t, ratio, w, r);
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
    // continuous and slope-continuous across the knee; monotonic; never negative
    let (t, ratio, w, r) = (-20.0, 0.5, 6.0, 60.0);
    for &l in &[t - w / 2.0, t + w / 2.0] {
        let e = 1.0e-6;
        assert!((static_gain_db(l - e, t, ratio, w, r) - static_gain_db(l + e, t, ratio, w, r)).abs() < 1.0e-5);
        let s0 = (static_gain_db(l - e, t, ratio, w, r) - static_gain_db(l - 2.0 * e, t, ratio, w, r)) / e;
        let s1 = (static_gain_db(l + 2.0 * e, t, ratio, w, r) - static_gain_db(l + e, t, ratio, w, r)) / e;
        assert!((s0 - s1).abs() < 1.0e-3, "slope jump at {l}: {s0} {s1}");
    }
    let mut prev = -1.0;
    let mut l = -80.0;
    while l < 10.0 {
        let g = static_gain_db(l, t, ratio, w, 12.0);
        assert!(g >= 0.0 && g >= prev - 1.0e-12 && g <= 12.0);
        prev = g;
        l += 0.1;
    }
    // 1:2 above the knee: output level slope 2
    let o = |l: f64| l + static_gain_db(l, t, ratio, w, r);
    assert!(((o(-5.0) - o(-10.0)) / 5.0 - 2.0).abs() < 1.0e-12);
    // ratio 1:1 is nothing, everywhere
    for l in [-60.0, -20.0, 0.0, 6.0] {
        assert_eq!(static_gain_db(l, t, 1.0, w, r), 0.0);
    }
}

#[test]
fn inverts_a_hard_knee_compressor() {
    // x:1 compressor above T, then 1:x decompressor at the same T: levels come back exactly
    for &x in &[1.5, 2.0, 4.0] {
        let t = -24.0;
        for i in 0..60 {
            let l = -40.0 + i as f64;
            let c = if l > t { t + (l - t) / x } else { l };
            let back = c + static_gain_db(c, t, 1.0 / x, 0.0, 100.0);
            assert!((back - l).abs() < 1.0e-9, "x {x} level {l}: back {back}");
        }
    }
}

#[test]
fn helpers() {
    assert_eq!(transient_threshold_db(-30.0, -20.0, 6.0), -14.0);
    assert_eq!(transient_threshold_db(-10.0, -20.0, 6.0), -10.0);
    assert_eq!(match_range_db(8.0, 14.0, 24.0), 6.0);
    assert_eq!(match_range_db(8.0, 14.0, 4.0), 4.0);
    assert_eq!(match_range_db(16.0, 14.0, 24.0), 0.0);
}

/// Settled gain of the audio path on DC (|x| constant, so the follower lands on the level).
fn settled_gain(settings: &[(&str, f32)], level: f64) -> f64 {
    let mut m = with(settings);
    let x = dc(level, ms(300.0));
    let y = render(&mut m, &x);
    db(y[y.len() - 1] as f64) - level
}

#[test]
fn audio_path_on_dc_matches_curve() {
    let mut worst = 0.0f64;
    for &(t, ratio, w, r) in &[(-30.0, 0.5f64, 0.0, 24.0), (-30.0, 1.0 / 1.5, 6.0, 6.0), (-18.0, 0.25, 12.0, 24.0), (-40.0, 0.8, 3.0, 2.0)] {
        for i in 0..14 {
            let l = -60.0 + 4.0 * i as f64;
            let s = [
                ("threshold", t as f32),
                ("ratio", (1.0 / ratio) as f32),
                ("knee", w as f32),
                ("range", r as f32),
                ("ceilingOn", 0.0),
            ];
            let got = settled_gain(&s, l);
            // the parameter is stored as f32
            let want = static_gain_db(l, t, ratio as f32 as f64, w, r);
            worst = worst.max((got - want).abs());
            assert!((got - want).abs() < 1.0e-3, "T {t} ratio {ratio} knee {w} range {r} level {l}: {got} vs {want}");
        }
    }
    println!("worst audio-path error on DC: {worst:.2e} dB");
}

#[test]
fn rms_detector_reads_sine_rms() {
    // a 1 kHz sine of peak -20 dBFS reads -23.01 dBFS on the RMS detector
    let (t, ratio, w, r) = (-40.0, 0.5, 0.0, 24.0);
    let mut m = with(&[("threshold", t), ("ratio", 1.0 / ratio), ("knee", w), ("range", r), ("detector", 1.0), ("release", 500.0), ("ceilingOn", 0.0)]);
    let x = sine(1000.0, lin(-20.0), ms(600.0));
    let y = render(&mut m, &x);
    let a = ms(400.0);
    let g = db(tone_amp(&y, 1000.0, a, a + 4800)) + 20.0;
    let want = static_gain_db(-20.0 - 3.0103, t as f64, ratio as f64, w as f64, r as f64);
    println!("RMS sine: gain {g:.4} dB, curve {want:.4} dB");
    assert!((g - want).abs() < 0.05);
}

#[test]
fn transient_mode_leaves_steady_level_alone_and_lifts_onsets() {
    let s = [("threshold", -60.0), ("ratio", 4.0), ("knee", 0.0), ("range", 12.0), ("detector", 2.0), ("transientCrest", 6.0), ("ceilingOn", 0.0)];
    // steady DC: fast = body, threshold rides 6 dB above it, no gain
    let g = settled_gain(&s, -20.0);
    assert!(g.abs() < 1.0e-6, "steady programme gain {g}");
    // a step of 20 dB after a quiet stretch: the onset is lifted, then it settles back to 0
    let mut m = with(&s);
    let mut x = dc(-40.0, ms(300.0));
    x.extend(dc(-20.0, ms(300.0)));
    let y = render(&mut m, &x);
    let onset = ms(300.0) + ms(2.0);
    let g_on = db(y[onset] as f64) + 20.0;
    let g_end = db(y[y.len() - 1] as f64) + 20.0;
    println!("transient: onset gain {g_on:.2} dB, settled {g_end:.4} dB");
    assert!(g_on > 6.0 && g_on <= 12.0 + 1.0e-6);
    assert!(g_end.abs() < 1.0e-3);
}
