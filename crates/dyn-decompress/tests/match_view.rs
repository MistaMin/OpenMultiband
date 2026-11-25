//! Match mode (crest estimate and the range it sets) and the editor read-out.
mod common;
use common::*;
use dyn_decompress::{view, Decompressor};
use olm_api::Module;

fn run_view(m: &mut Decompressor, l: &[f32], r: &[f32], key: Option<(&[f32], &[f32])>) -> [f32; view::LEN] {
    let mut a = l.to_vec();
    let mut b = r.to_vec();
    for (i, (x, y)) in a.chunks_mut(512).zip(b.chunks_mut(512)).enumerate() {
        match key {
            Some((kl, kr)) => {
                let s = i * 512;
                m.process_keyed(x, y, &kl[s..s + x.len()], &kr[s..s + x.len()]);
            }
            None => m.process(x, y),
        }
    }
    let mut v = [0.0f32; view::LEN];
    assert_eq!(m.view(&mut v), view::LEN);
    v
}

#[test]
fn match_measures_crest_and_sets_range() {
    // uniform noise: crest factor 20 log10(sqrt 3) = 4.77 dB; target 12 -> 7.23 dB asked for
    let x = noise(ms(14_000.0), 31, 0.4);
    let mut m = with(&[("match", 1.0), ("matchTarget", 12.0), ("range", 24.0), ("threshold", -60.0), ("ratio", 4.0), ("ceilingOn", 0.0)]);
    let v = run_view(&mut m, &x, &x, None);
    let crest = v[view::MATCH_CREST] as f64;
    let range = v[view::MATCH_RANGE] as f64;
    println!("uniform noise: crest {crest:.3} dB, match range {range:.3} dB, block max gain {:.3} dB", v[view::GAIN]);
    assert!((crest - 4.771).abs() < 0.1);
    assert!((range - (12.0 - crest)).abs() < 0.05);
    // the curve is now capped at the match range
    assert!((v[view::GAIN] as f64 - range).abs() < 0.05);
    // the user's range is the upper limit
    let mut m = with(&[("match", 1.0), ("matchTarget", 12.0), ("range", 3.0), ("threshold", -60.0), ("ratio", 4.0), ("ceilingOn", 0.0)]);
    let v = run_view(&mut m, &x, &x, None);
    assert!((v[view::GAIN] - 3.0).abs() < 1.0e-3);

    // a dynamic programme (quiet tone with sharp clicks) needs nothing
    let n = ms(10_000.0);
    let mut y = sine(220.0, lin(-30.0), n);
    for i in (0..n).step_by(ms(400.0)) {
        y[i] = 0.7;
    }
    let mut m = with(&[("match", 1.0), ("matchTarget", 12.0)]);
    let v = run_view(&mut m, &y, &y, None);
    println!("dynamic programme: crest {:.2} dB, match range {:.3} dB", v[view::MATCH_CREST], v[view::MATCH_RANGE]);
    assert!(v[view::MATCH_CREST] > 15.0);
    assert!(v[view::MATCH_RANGE] < 0.01);
    assert!(v[view::GAIN] < 0.01);
}

#[test]
fn view_slots() {
    let mut m = with(&[("threshold", -40.0), ("ratio", 2.0), ("knee", 0.0), ("range", 24.0), ("ceilingOn", 0.0), ("stereoLink", 0.0)]);
    let l = dc(-30.0, ms(300.0));
    let r = dc(-50.0, ms(300.0));
    let v = run_view(&mut m, &l, &r, None);
    assert!((v[view::GAIN] - 10.0).abs() < 1.0e-3 && v[view::GAIN + 1].abs() < 1.0e-6);
    assert!((v[view::DETECTOR] + 30.0).abs() < 1.0e-3 && (v[view::DETECTOR + 1] + 50.0).abs() < 1.0e-3);
    assert!((v[view::ENVELOPE] + 30.0).abs() < 1.0e-3 && (v[view::ENVELOPE + 1] + 50.0).abs() < 1.0e-3);
    assert_eq!(v[view::THRESHOLD], -40.0);
    assert_eq!(v[view::KEYED], 0.0);
    // keyed
    let mut m = with(&[("keySource", 1.0)]);
    let v = run_view(&mut m, &l, &r, Some((&l, &l)));
    assert_eq!(v[view::KEYED], 1.0);
    // transient: the threshold rides on the body
    let mut m = with(&[("detector", 2.0), ("threshold", -60.0), ("transientCrest", 6.0)]);
    let v = run_view(&mut m, &l, &l, None);
    assert!((v[view::THRESHOLD] + 24.0).abs() < 1.0e-3, "{}", v[view::THRESHOLD]);
    // fresh state
    let m = with(&[]);
    let mut v = [1.0f32; view::LEN];
    m.view(&mut v);
    assert_eq!(v[view::GAIN], 0.0);
    assert!(v[view::DETECTOR] <= -120.0);
    // a short slice gets what fits
    let mut short = [0.0f32; 3];
    assert_eq!(m.view(&mut short), 3);
}
