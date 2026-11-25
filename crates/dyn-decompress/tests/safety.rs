//! Ceiling (never exceeded), range limit, nulls (mix 0, ratio 1:1, range 0), trim and mix law.
mod common;
use common::*;
use olm_api::Module;

/// Noise with loud short bursts: the kind of programme the decompressor lifts most.
fn bursty(n: usize, seed: u64) -> Vec<f32> {
    let w = noise(n, seed, 1.0);
    (0..n)
        .map(|i| {
            let ph = i % 4800;
            let env = if ph < 240 { 0.9 } else { 0.12 };
            w[i] * env
        })
        .collect()
}

/// Inter-sample peak of `y` by 16x windowed-sinc interpolation (64 taps each side).
fn true_peak_16x(y: &[f32]) -> f64 {
    let half = 64i64;
    let mut best = 0.0f64;
    for n in half as usize..y.len() - half as usize {
        for p in 0..16 {
            let frac = p as f64 / 16.0;
            let mut v = 0.0;
            for k in -half + 1..=half {
                let t = k as f64 - frac;
                let s = if t.abs() < 1.0e-12 { 1.0 } else { (std::f64::consts::PI * t).sin() / (std::f64::consts::PI * t) };
                let u = t / half as f64;
                let w = if u.abs() < 1.0 { 0.42 + 0.5 * (std::f64::consts::PI * u).cos() + 0.08 * (2.0 * std::f64::consts::PI * u).cos() } else { 0.0 };
                v += y[(n as i64 + k) as usize] as f64 * s * w;
            }
            best = best.max(v.abs());
        }
    }
    best
}

#[test]
fn sample_peak_ceiling_is_never_exceeded() {
    let x = bursty(ms(2000.0), 7);
    for &c in &[-24.0f32, -6.0, -1.0, -0.1, 0.0] {
        for &la in &[0.0f32, 4.0] {
            let mut m = with(&[("threshold", -40.0), ("ratio", 4.0), ("range", 24.0), ("attack", 0.01), ("ceiling", c), ("ceilingType", 0.0), ("lookahead", la)]);
            let (l, r) = render2(&mut m, &x, &x, None);
            let pk = peak(&l).max(peak(&r));
            let lim = (lin(c as f64) as f32) as f64;
            println!("ceiling {c} dB, look-ahead {la}: output peak {:.4} dBFS", db(pk));
            assert!(pk <= lim, "ceiling {c}: peak {pk} > {lim}");
        }
    }
    // without the ceiling the same settings go far over
    let mut m = with(&[("threshold", -40.0), ("ratio", 4.0), ("range", 24.0), ("attack", 0.01), ("ceilingOn", 0.0)]);
    let y = render(&mut m, &x);
    assert!(db(peak(&y)) > 10.0);
}

/// Linear-phase low-pass (windowed sinc, 255 taps) at `fc` Hz.
fn low_pass(x: &[f32], fc: f64) -> Vec<f32> {
    let half = 127i64;
    let a = 2.0 * fc / SRF;
    let h: Vec<f64> = (-half..=half)
        .map(|k| {
            let t = k as f64;
            let s = if k == 0 { a } else { (std::f64::consts::PI * a * t).sin() / (std::f64::consts::PI * t) };
            let u = t / (half + 1) as f64;
            s * (0.42 + 0.5 * (std::f64::consts::PI * u).cos() + 0.08 * (2.0 * std::f64::consts::PI * u).cos())
        })
        .collect();
    (0..x.len())
        .map(|n| {
            let mut v = 0.0;
            for (j, c) in h.iter().enumerate() {
                let i = n as i64 + j as i64 - half;
                if i >= 0 && (i as usize) < x.len() {
                    v += x[i as usize] as f64 * c;
                }
            }
            v as f32
        })
        .collect()
}

/// Sample peak and 16x true peak (dB) of the output in both ceiling types.
fn ceiling_peaks(x: &[f32], c: f32) -> (f64, f64, f64) {
    let s = [("threshold", -40.0), ("ratio", 4.0), ("range", 24.0), ("ceiling", c)];
    let mut m = with(&s);
    set(&mut m, "ceilingType", 1.0);
    m.reset();
    let (l, r) = render2(&mut m, x, x, None);
    let sp = peak(&l).max(peak(&r));
    let tp = true_peak_16x(&l).max(true_peak_16x(&r));
    let mut m = with(&s);
    set(&mut m, "ceilingType", 0.0);
    m.reset();
    let (ls, _) = render2(&mut m, x, x, None);
    (db(sp), db(tp), db(true_peak_16x(&ls)))
}

#[test]
fn true_peak_ceiling() {
    // a tone near a quarter of the rate plus full-band bursts: many inter-sample peaks
    let n = ms(500.0);
    let hf: Vec<f32> = (0..n).map(|i| (0.3 * (std::f64::consts::TAU * 11_025.0 * i as f64 / SRF + 0.7).sin()) as f32).collect();
    let b = bursty(n, 3);
    let full: Vec<f32> = hf.iter().zip(&b).map(|(a, c)| a + 0.5 * c).collect();
    let band = low_pass(&full, 20_000.0);
    let c = -1.0f32;
    let (sp, tp, tp_sample) = ceiling_peaks(&full, c);
    println!("full band: true-peak mode sample peak {sp:.3} dBFS, 16x {tp:.3} dBTP; sample mode 16x {tp_sample:.3} dBTP");
    let (bsp, btp, btp_sample) = ceiling_peaks(&band, c);
    println!("below 20 kHz: true-peak mode sample peak {bsp:.3} dBFS, 16x {btp:.3} dBTP; sample mode 16x {btp_sample:.3} dBTP");
    assert!(sp <= c as f64 + 1.0e-6 && bsp <= c as f64 + 1.0e-6);
    // programme below 20 kHz: within 0.1 dB of the ceiling on a 16x reconstruction
    assert!(btp <= c as f64 + 0.1, "below 20 kHz: {btp} dBTP");
    // full-band white bursts: the reader under-reads content near Nyquist (limit in the report)
    assert!(tp <= c as f64 + 0.25, "full band: {tp} dBTP");
    assert!(tp < tp_sample && btp < btp_sample);
}

#[test]
fn ceiling_reports_reduction() {
    let x = bursty(ms(500.0), 9);
    let mut m = with(&[("threshold", -40.0), ("ratio", 4.0), ("range", 24.0), ("ceiling", -1.0)]);
    let mut l = x.clone();
    let mut r = x.clone();
    let mut deepest = 0.0f32;
    for (a, b) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        m.process(a, b);
        deepest = deepest.max(m.meters().reduction_db);
        let mut v = [0.0f32; 16];
        m.view(&mut v);
        assert_eq!(v[dyn_decompress::view::CEILING], m.meters().reduction_db);
    }
    assert!(deepest > 3.0, "deepest {deepest}");
    // nothing to limit: no reduction is reported (upward gain is not a reduction)
    let mut m = with(&[("threshold", -40.0), ("ratio", 2.0), ("range", 6.0)]);
    let mut l = sine(1000.0, 0.1, 4096);
    let mut r = l.clone();
    m.process(&mut l, &mut r);
    assert_eq!(m.meters().reduction_db, 0.0);
    assert!(l.iter().any(|v| v.abs() > 0.15));
}

#[test]
fn range_limits_the_boost() {
    for &r in &[0.0f32, 1.0, 6.0, 12.0, 24.0] {
        let mut m = with(&[("threshold", -60.0), ("ratio", 4.0), ("range", r), ("ceilingOn", 0.0)]);
        let x = noise(ms(300.0), 4, 0.5);
        let y = render(&mut m, &x);
        let most = y.iter().zip(&x).filter(|(_, b)| b.abs() > 1.0e-3).map(|(a, b)| db(*a as f64) - db(*b as f64)).fold(f64::NEG_INFINITY, f64::max);
        println!("range {r} dB: largest gain {most:.4} dB");
        assert!(most <= r as f64 + 1.0e-4);
        if r > 0.0 {
            assert!(most > r as f64 - 0.01);
        }
    }
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
    let quiet = bursty(ms(400.0), 11).iter().map(|v| v * 0.4).collect::<Vec<_>>(); // true peaks well below -1 dBTP
    let loud = bursty(ms(400.0), 12).iter().map(|v| v * 2.0).collect::<Vec<_>>(); // peaks above 0 dBFS
    for det in [0.0, 1.0, 2.0] {
        assert_null(&[("mix", 0.0), ("ratio", 4.0), ("detector", det)], &quiet);
        assert_null(&[("ratio", 1.0), ("detector", det), ("lookahead", 3.0)], &quiet);
        assert_null(&[("range", 0.0), ("ratio", 4.0), ("detector", det)], &quiet);
        assert_null(&[("mix", 0.0), ("ratio", 4.0), ("detector", det), ("ceilingOn", 0.0)], &loud);
        assert_null(&[("ratio", 1.0), ("detector", det), ("ceilingOn", 0.0), ("match", 1.0)], &loud);
    }
}

#[test]
fn trim_is_down_only_and_mix_is_linear_parallel() {
    let x = bursty(ms(200.0), 13);
    let mut m = with(&[("ratio", 1.0), ("trim", -6.0), ("ceilingOn", 0.0)]);
    let y = render(&mut m, &x);
    let g = lin(-6.0) as f32;
    for i in 0..x.len() {
        assert!((y[i] - x[i] * g).abs() <= 1.0e-7);
    }
    // above 0 dB is clamped to 0 dB
    let m = with(&[("trim", 6.0)]);
    assert_eq!(m.param(dyn_decompress::index::TRIM), Some(0.0));
    // mix law on DC: 1 + mix (g - 1), linear
    for &mix in &[25.0f32, 50.0, 75.0] {
        let mut m = with(&[("threshold", -40.0), ("ratio", 2.0), ("knee", 0.0), ("range", 24.0), ("mix", mix), ("ceilingOn", 0.0)]);
        let y = render(&mut m, &dc(-30.0, ms(200.0)));
        let got = y[y.len() - 1] as f64 / lin(-30.0);
        let want = 1.0 + mix as f64 / 100.0 * (lin(10.0) - 1.0);
        assert!((got - want).abs() < 1.0e-4, "mix {mix}: {got} vs {want}");
        m.reset();
    }
}
