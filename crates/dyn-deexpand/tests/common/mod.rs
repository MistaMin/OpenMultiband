//! Shared helpers: building a module, rendering through it with the latency removed, signals.
#![allow(dead_code)]

use dyn_deexpand::DeExpander;
use olm_api::Module;

pub const SR: f32 = 48_000.0;
pub const SRF: f64 = 48_000.0;

pub fn lin(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

pub fn db(v: f64) -> f64 {
    20.0 * v.abs().max(1.0e-12).log10()
}

pub fn ms(t: f64) -> usize {
    (t * 1.0e-3 * SRF).round() as usize
}

pub fn set(m: &mut DeExpander, id: &str, v: f32) {
    let i = DeExpander::param_index(id).unwrap_or_else(|| panic!("no parameter {id}"));
    m.set_param(i, v);
}

/// A prepared module with the given settings, reset after setting them.
pub fn with(settings: &[(&str, f32)]) -> DeExpander {
    let mut m = DeExpander::default();
    m.prepare(SR, 512);
    for (id, v) in settings {
        set(&mut m, id, *v);
    }
    m.reset();
    m
}

/// Renders in blocks of `block` samples and removes the reported latency (offline render).
pub fn render_blocks(m: &mut DeExpander, l: &[f32], r: &[f32], key: Option<(&[f32], &[f32])>, block: usize) -> (Vec<f32>, Vec<f32>) {
    let lat = m.latency();
    let n = l.len();
    let total = n + lat;
    let mut a = l.to_vec();
    let mut b = r.to_vec();
    a.resize(total, 0.0);
    b.resize(total, 0.0);
    let (mut kl, mut kr) = (Vec::new(), Vec::new());
    if let Some((x, y)) = key {
        kl = x.to_vec();
        kr = y.to_vec();
        kl.resize(total, 0.0);
        kr.resize(total, 0.0);
    }
    let mut i = 0;
    while i < total {
        let e = (i + block.max(1)).min(total);
        if key.is_some() {
            m.process_keyed(&mut a[i..e], &mut b[i..e], &kl[i..e], &kr[i..e]);
        } else {
            m.process(&mut a[i..e], &mut b[i..e]);
        }
        i = e;
    }
    (a[lat..].to_vec(), b[lat..].to_vec())
}

pub fn render2(m: &mut DeExpander, l: &[f32], r: &[f32], key: Option<(&[f32], &[f32])>) -> (Vec<f32>, Vec<f32>) {
    render_blocks(m, l, r, key, 512)
}

/// Same signal on both channels; returns the left output.
pub fn render(m: &mut DeExpander, x: &[f32]) -> Vec<f32> {
    render2(m, x, x, None).0
}

pub fn dc(level_db: f64, n: usize) -> Vec<f32> {
    vec![lin(level_db) as f32; n]
}

pub fn sine(freq: f64, amp: f64, n: usize) -> Vec<f32> {
    (0..n).map(|i| (amp * (std::f64::consts::TAU * freq * i as f64 / SRF).sin()) as f32).collect()
}

/// White noise in -a..a (xorshift, reproducible).
pub fn noise(n: usize, seed: u64, a: f32) -> Vec<f32> {
    let mut s = seed | 1;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            ((s >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0) as f32 * a
        })
        .collect()
}

pub fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

pub fn peak(x: &[f32]) -> f64 {
    x.iter().fold(0.0f64, |m, v| m.max(v.abs() as f64))
}

/// Amplitude of the component at `freq` in `y[a..b]` (quadrature correlation).
pub fn tone_amp(y: &[f32], freq: f64, a: usize, b: usize) -> f64 {
    let (mut s, mut c) = (0.0, 0.0);
    for (i, v) in y.iter().enumerate().take(b).skip(a) {
        let ph = std::f64::consts::TAU * freq * i as f64 / SRF;
        s += *v as f64 * ph.sin();
        c += *v as f64 * ph.cos();
    }
    let n = (b - a) as f64;
    (2.0 * s / n).hypot(2.0 * c / n)
}
