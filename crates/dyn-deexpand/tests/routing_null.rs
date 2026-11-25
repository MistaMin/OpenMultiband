//! Nulls (mix 0, ratio 1:1, range 0), mix law, external sidechain, key filters, stereo link and
//! the editor read-out.
mod common;
use common::*;
use dyn_deexpand::{view, DeExpander};
use olm_api::Module;

const S: [(&str, f32); 6] = [("threshold", -40.0), ("ratio", 2.0), ("knee", 0.0), ("range", 36.0), ("floor", -120.0), ("detector", 0.0)];

fn with_s(extra: &[(&str, f32)]) -> DeExpander {
    let mut s = S.to_vec();
    s.extend_from_slice(extra);
    with(&s)
}

fn last_gain(y: &[f32], x: &[f32]) -> f64 {
    db(y[y.len() - 1] as f64) - db(x[x.len() - 1] as f64)
}

/// A programme with loud passages, decays and quiet gaps.
fn programme(n: usize, seed: u64) -> Vec<f32> {
    let w = noise(n, seed, 1.0);
    (0..n).map(|i| w[i] * (0.5 * (-((i % 24_000) as f64) / 3000.0).exp()) as f32).collect()
}

fn assert_null(settings: &[(&str, f32)], x: &[f32]) {
    let mut m = with(settings);
    let (l, r) = render2(&mut m, x, x, None);
    for i in 0..x.len() {
        assert_eq!(l[i].to_bits(), x[i].to_bits(), "{settings:?} at {i}");
        assert_eq!(r[i].to_bits(), x[i].to_bits());
    }
}

#[test]
fn nulls() {
    let x = programme(ms(1500.0), 3);
    for det in [0.0, 1.0] {
        assert_null(&[("mix", 0.0), ("detector", det), ("range", 36.0)], &x);
        assert_null(&[("ratio", 1.0), ("detector", det), ("lookahead", 7.0)], &x);
        assert_null(&[("range", 0.0), ("ratio", 8.0), ("detector", det)], &x);
    }
    // and the defaults do change this programme
    let y = render(&mut with(&[]), &x);
    assert!(y.iter().zip(&x).any(|(a, b)| a != b));
}

#[test]
fn mix_is_linear_parallel() {
    for &mix in &[25.0f32, 50.0, 75.0] {
        let mut m = with_s(&[("mix", mix)]);
        let y = render(&mut m, &dc(-60.0, ms(200.0)));
        let got = y[y.len() - 1] as f64 / lin(-60.0);
        let want = 1.0 + mix as f64 / 100.0 * (lin(10.0) - 1.0);
        assert!((got - want).abs() < 1.0e-4, "mix {mix}: {got} vs {want}");
    }
}

#[test]
fn external_key_drives_the_detector() {
    // quiet input, loud key: no lift; loud input, quiet key: lift
    let quiet = dc(-60.0, ms(200.0));
    let loud = dc(-20.0, ms(200.0));
    let mut m = with_s(&[("keySource", 1.0)]);
    let (y, _) = render2(&mut m, &quiet, &quiet, Some((&loud, &loud)));
    assert!(last_gain(&y, &quiet).abs() < 1.0e-6);
    let mut m = with_s(&[("keySource", 1.0)]);
    let (y, _) = render2(&mut m, &loud, &loud, Some((&quiet, &quiet)));
    assert!((last_gain(&y, &loud) - 10.0).abs() < 1.0e-3);
    // internal: the quiet input is lifted 10 dB whatever the key
    let mut m = with_s(&[]);
    let (y, _) = render2(&mut m, &quiet, &quiet, Some((&loud, &loud)));
    assert!((last_gain(&y, &quiet) - 10.0).abs() < 1.0e-3);
}

#[test]
fn key_routing_rules() {
    // release 10 ms so the lift follows the decays of the programme
    let x = programme(ms(800.0), 21);
    let k = programme(ms(800.0), 22);
    let int = [("release", 10.0f32)];
    let ext = [("release", 10.0f32), ("keySource", 1.0)];
    let (a, _) = render2(&mut with_s(&int), &x, &x, Some((&k, &k)));
    let (b, _) = render2(&mut with_s(&int), &x, &x, None);
    assert!(a.iter().zip(&b).all(|(p, q)| p.to_bits() == q.to_bits()));
    assert!(b.iter().zip(&x).any(|(p, q)| p != q));
    let (c, _) = render2(&mut with_s(&ext), &x, &x, None);
    assert!(c.iter().zip(&b).all(|(p, q)| p.to_bits() == q.to_bits()));
    let z = vec![0.0f32; x.len()];
    let (d, e) = render2(&mut with_s(&ext), &z, &z, Some((&k, &k)));
    assert!(d.iter().chain(&e).all(|v| *v == 0.0));
    let (f, _) = render2(&mut with_s(&ext), &x, &x, Some((&k, &k)));
    assert!(f.iter().zip(&b).any(|(p, q)| p != q));
}

#[test]
fn key_filters() {
    // a loud key the filters remove reads as quiet, so the input is lifted
    // (short hold and release, so the start-up transient of the filter has gone)
    let x = dc(-60.0, ms(400.0));
    let run = |extra: &[(&str, f32)], k: &[f32]| {
        let mut s = vec![("keySource", 1.0), ("hold", 20.0), ("release", 20.0)];
        s.extend_from_slice(extra);
        let mut m = with_s(&s);
        let (y, _) = render2(&mut m, &x, &x, Some((k, k)));
        // 5 ms before the end: the key stops dead at the end, and that step is a click
        let i = y.len() - ms(5.0);
        db(y[i] as f64) - db(x[i] as f64)
    };
    let k50 = sine(50.0, lin(-20.0), ms(400.0));
    let k10k = sine(10_000.0, lin(-20.0), ms(400.0));
    let (open_lo, hp, lp_lo) = (run(&[], &k50), run(&[("keyHighPass", 1000.0)], &k50), run(&[("keyLowPass", 1000.0)], &k50));
    let (open_hi, lp, hp_hi) = (run(&[], &k10k), run(&[("keyLowPass", 1000.0)], &k10k), run(&[("keyHighPass", 1000.0)], &k10k));
    println!("50 Hz key: open {open_lo:.2}, HP 1k {hp:.2}, LP 1k {lp_lo:.2}; 10 kHz key: open {open_hi:.2}, LP 1k {lp:.2}, HP 1k {hp_hi:.2}");
    assert!(open_lo.abs() < 1.0e-6 && open_hi.abs() < 1.0e-6 && lp_lo.abs() < 0.01 && hp_hi.abs() < 0.01);
    assert!(hp > 9.0 && lp > 9.0);
}

#[test]
fn stereo_link() {
    let l = dc(-50.0, ms(200.0));
    let r = dc(-70.0, ms(200.0));
    // lift = (-40 - level) / 2: own levels give 5 and 15 dB; full link reads -50 on both
    for &(link, gl, gr) in &[(100.0f32, 5.0, 5.0), (0.0, 5.0, 15.0), (50.0, 5.0, 10.0)] {
        let mut m = with_s(&[("stereoLink", link)]);
        let (yl, yr) = render2(&mut m, &l, &r, None);
        let (a, b) = (last_gain(&yl, &l), last_gain(&yr, &r));
        assert!((a - gl).abs() < 1.0e-3 && (b - gr).abs() < 1.0e-3, "link {link}: {a} {b}");
        let mut m = with_s(&[("stereoLink", link)]);
        let (zl, zr) = render2(&mut m, &r, &l, None);
        assert!((last_gain(&zr, &l) - a).abs() < 1.0e-9 && (last_gain(&zl, &r) - b).abs() < 1.0e-9);
    }
}

#[test]
fn view_slots_and_meters() {
    let mut m = with_s(&[("stereoLink", 0.0)]);
    let l = dc(-50.0, ms(300.0));
    let r = dc(-30.0, ms(300.0));
    let (mut a, mut b) = (l.clone(), r.clone());
    for (x, y) in a.chunks_mut(512).zip(b.chunks_mut(512)) {
        m.process(x, y);
        assert_eq!(m.meters().reduction_db, 0.0);
    }
    let mut v = [0.0f32; view::LEN];
    assert_eq!(m.view(&mut v), view::LEN);
    assert!((v[view::GAIN] - 5.0).abs() < 1.0e-3 && v[view::GAIN + 1].abs() < 1.0e-6);
    assert!((v[view::DETECTOR] + 50.0).abs() < 1.0e-3 && (v[view::DETECTOR + 1] + 30.0).abs() < 1.0e-3);
    assert!((v[view::ENVELOPE] + 50.0).abs() < 1.0e-3 && (v[view::ENVELOPE + 1] + 30.0).abs() < 1.0e-3);
    assert_eq!(v[view::THRESHOLD], -40.0);
    assert_eq!(v[view::FLOOR], -120.0);
    assert_eq!(v[view::KEYED], 0.0);
    let mut m = with_s(&[("keySource", 1.0)]);
    let (mut a, mut b) = (l.clone(), l.clone());
    m.process_keyed(&mut a[..512], &mut b[..512], &r[..512], &r[..512]);
    m.view(&mut v);
    assert_eq!(v[view::KEYED], 1.0);
    let m = with(&[]);
    m.view(&mut v);
    assert_eq!(v[view::GAIN], 0.0);
    assert!(v[view::DETECTOR] <= -120.0);
    let mut short = [0.0f32; 2];
    assert_eq!(m.view(&mut short), 2);
}
