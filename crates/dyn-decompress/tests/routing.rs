//! External sidechain, key filters and stereo link.
mod common;
use common::*;

const S: [(&str, f32); 5] = [("threshold", -40.0), ("ratio", 2.0), ("knee", 0.0), ("range", 24.0), ("ceilingOn", 0.0)];

fn with_s(extra: &[(&str, f32)]) -> dyn_decompress::Decompressor {
    let mut s = S.to_vec();
    s.extend_from_slice(extra);
    with(&s)
}

fn last_gain(y: &[f32], x: &[f32]) -> f64 {
    db(y[y.len() - 1] as f64) - db(x[x.len() - 1] as f64)
}

#[test]
fn external_key_drives_the_detector() {
    let x = dc(-50.0, ms(200.0));
    let k = dc(-20.0, ms(200.0));
    let mut m = with_s(&[("keySource", 1.0)]);
    let (y, _) = render2(&mut m, &x, &x, Some((&k, &k)));
    let g = last_gain(&y, &x);
    assert!((g - 20.0).abs() < 1.0e-3, "keyed gain {g}");
    // internal: the quiet input is below threshold
    let mut m = with_s(&[]);
    let (y, _) = render2(&mut m, &x, &x, Some((&k, &k)));
    assert!(last_gain(&y, &x).abs() < 1.0e-6);
}

#[test]
fn key_routing_rules() {
    let x = noise(ms(300.0), 21, 0.3);
    let k = noise(ms(300.0), 22, 0.9);
    // Internal ignores the key bit for bit
    let (a, _) = render2(&mut with_s(&[]), &x, &x, Some((&k, &k)));
    let (b, _) = render2(&mut with_s(&[]), &x, &x, None);
    assert!(a.iter().zip(&b).all(|(p, q)| p.to_bits() == q.to_bits()));
    // External without a key (plain process) uses the input
    let (c, _) = render2(&mut with_s(&[("keySource", 1.0)]), &x, &x, None);
    assert!(c.iter().zip(&b).all(|(p, q)| p.to_bits() == q.to_bits()));
    // the key never reaches the audio
    let z = vec![0.0f32; x.len()];
    let (d, e) = render2(&mut with_s(&[("keySource", 1.0)]), &z, &z, Some((&k, &k)));
    assert!(d.iter().chain(&e).all(|v| *v == 0.0));
    // a different key changes the output
    let (f, _) = render2(&mut with_s(&[("keySource", 1.0)]), &x, &x, Some((&k, &k)));
    assert!(f.iter().zip(&b).any(|(p, q)| p != q));
}

#[test]
fn key_filters() {
    let x = dc(-50.0, ms(300.0));
    // 50 Hz key at -20: full gain unfiltered, nothing through a 1 kHz high-pass (-52 dB)
    let k50 = sine(50.0, lin(-20.0), ms(300.0));
    let k10k = sine(10_000.0, lin(-20.0), ms(300.0));
    let run = |extra: &[(&str, f32)], k: &[f32]| {
        let mut s = vec![("keySource", 1.0), ("release", 200.0)];
        s.extend_from_slice(extra);
        let mut m = with_s(&s);
        let (y, _) = render2(&mut m, &x, &x, Some((k, k)));
        // 5 ms before the end: the key stops dead at the end, and that step is a click
        let i = y.len() - ms(5.0);
        db(y[i] as f64) - db(x[i] as f64)
    };
    let open_lo = run(&[], &k50);
    let hp = run(&[("keyHighPass", 1000.0)], &k50);
    let open_hi = run(&[], &k10k);
    let lp = run(&[("keyLowPass", 1000.0)], &k10k);
    // the filters leave the other end alone
    let hp_hi = run(&[("keyHighPass", 1000.0)], &k10k);
    let lp_lo = run(&[("keyLowPass", 1000.0)], &k50);
    println!("50 Hz key: open {open_lo:.2}, HP 1k {hp:.2}, LP 1k {lp_lo:.2}; 10 kHz key: open {open_hi:.2}, LP 1k {lp:.2}, HP 1k {hp_hi:.2}");
    assert!(open_lo > 18.0 && open_hi > 18.0);
    assert!(hp < 1.0e-6 && lp < 1.0e-6);
    assert!((hp_hi - open_hi).abs() < 0.2 && (lp_lo - open_lo).abs() < 0.2);
}

#[test]
fn stereo_link() {
    let l = dc(-20.0, ms(200.0));
    let r = dc(-50.0, ms(200.0));
    for &(link, gl, gr) in &[(100.0f32, 20.0, 20.0), (0.0, 20.0, 0.0), (50.0, 20.0, 5.0)] {
        let mut m = with_s(&[("stereoLink", link)]);
        let (yl, yr) = render2(&mut m, &l, &r, None);
        let (a, b) = (last_gain(&yl, &l), last_gain(&yr, &r));
        assert!((a - gl).abs() < 1.0e-3 && (b - gr).abs() < 1.0e-3, "link {link}: {a} {b}");
        // swapped channels give swapped gains
        let mut m = with_s(&[("stereoLink", link)]);
        let (zl, zr) = render2(&mut m, &r, &l, None);
        assert!((last_gain(&zr, &l) - a).abs() < 1.0e-9 && (last_gain(&zl, &r) - b).abs() < 1.0e-9);
    }
}
