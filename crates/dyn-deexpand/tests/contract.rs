//! Module contract: no allocation on the audio path, block-size independence, every parameter
//! at min / max / default finite and below +40 dBFS on noise, the parameter table.
mod common;
use common::*;
use dyn_deexpand::{DeExpander, NPARAMS, PARAMS};
use olm_api::Module;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;
static COUNT: AtomicUsize = AtomicUsize::new(0);
thread_local! { static ARMED: Cell<bool> = const { Cell::new(false) }; }

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            COUNT.fetch_add(1, Ordering::SeqCst);
        }
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        if ARMED.with(|a| a.get()) {
            COUNT.fetch_add(1, Ordering::SeqCst);
        }
        System.dealloc(p, l)
    }
}

#[global_allocator]
static A: Counting = Counting;

#[test]
fn audio_path_does_not_allocate() {
    let mut m = DeExpander::default();
    m.prepare(48_000.0, 512);
    let x = noise(512, 5, 0.7);
    let k = noise(512, 6, 0.9);
    let (mut l, mut r) = (x.clone(), x.clone());
    let mut v = [0.0f32; 32];
    ARMED.with(|a| a.set(true));
    for it in 0..4000 {
        let i = (it * 7) % NPARAMS;
        let p = PARAMS[i];
        m.set_param(i, p.min + (p.max - p.min) * ((it % 11) as f32 / 10.0));
        let n = 1 + (it * 37) % 512;
        if it % 2 == 0 {
            m.process(&mut l[..n], &mut r[..n]);
        } else {
            m.process_keyed(&mut l[..n], &mut r[..n], &k[..n], &k[..n]);
        }
        let _ = m.latency();
        let _ = m.meters();
        m.view(&mut v);
        if it % 700 == 0 {
            m.reset();
        }
        l.copy_from_slice(&x);
        r.copy_from_slice(&x);
    }
    ARMED.with(|a| a.set(false));
    assert_eq!(COUNT.load(Ordering::SeqCst), 0);
}

#[test]
fn block_size_does_not_change_the_output() {
    let n = ms(1500.0);
    let mut x = noise(n, 8, 0.2);
    for i in (0..n).step_by(ms(90.0)) {
        for j in i..(i + 200).min(n) {
            x[j] *= 4.0;
        }
    }
    let xr: Vec<f32> = x.iter().map(|v| -0.7 * v).collect();
    let k = noise(n, 9, 0.5);
    let configs: [&[(&str, f32)]; 4] = [
        &[],
        &[("detector", 0.0), ("lookahead", 10.0), ("hold", 0.0), ("stereoLink", 30.0), ("floor", -50.0)],
        &[("detector", 1.0), ("keySource", 1.0), ("keyHighPass", 200.0), ("keyLowPass", 5000.0), ("mix", 60.0), ("detectorWindow", 1.0)],
        &[("ratio", 8.0), ("range", 36.0), ("threshold", -10.0), ("attack", 0.01), ("release", 1.0), ("knee", 0.0), ("hold", 500.0)],
    ];
    for s in configs.iter() {
        let reference = render_blocks(&mut with(s), &x, &xr, Some((&k, &k)), 512);
        for &b in &[1usize, 7, 64, 333, 4096] {
            let got = render_blocks(&mut with(s), &x, &xr, Some((&k, &k)), b);
            assert!(got.0.iter().zip(&reference.0).all(|(p, q)| p.to_bits() == q.to_bits()), "{s:?} block {b}");
            assert!(got.1.iter().zip(&reference.1).all(|(p, q)| p.to_bits() == q.to_bits()), "{s:?} block {b}");
        }
    }
}

#[test]
fn every_parameter_at_min_max_default() {
    let mut m = DeExpander::default();
    m.prepare(48_000.0, 512);
    let x = noise(4096, 3, 0.5);
    let key = noise(4096, 5, 1.0);
    let mut worst = 0.0f32;
    for i in 0..NPARAMS {
        for v in [PARAMS[i].min, PARAMS[i].max, PARAMS[i].default] {
            m.set_param(i, v);
            let (mut l, mut r) = (x.clone(), x.clone());
            m.process_keyed(&mut l[..2048], &mut r[..2048], &key[..2048], &key[..2048]);
            m.process(&mut l[2048..], &mut r[2048..]);
            let pk = l.iter().chain(&r).fold(0.0f32, |a, &s| if s.is_finite() { a.max(s.abs()) } else { f32::INFINITY });
            worst = worst.max(pk);
            assert!(pk < 100.0, "{} = {v}: peak {pk}", PARAMS[i].id);
            assert_eq!(m.meters().reduction_db, 0.0);
            let mut vv = [0.0f32; 16];
            m.view(&mut vv);
            assert!(vv.iter().all(|s| s.is_finite()), "{} view", PARAMS[i].id);
            m.set_param(i, PARAMS[i].default);
        }
    }
    // all at their minimum, then all at their maximum
    for pick in [0, 1] {
        let mut m = DeExpander::default();
        m.prepare(44_100.0, 4096);
        for (i, p) in PARAMS.iter().enumerate() {
            m.set_param(i, if pick == 0 { p.min } else { p.max });
        }
        let (mut l, mut r) = (x.clone(), x.clone());
        m.process_keyed(&mut l, &mut r, &key, &key);
        assert!(l.iter().chain(&r).all(|s| s.is_finite() && s.abs() < 100.0));
    }
    println!("worst peak {worst:.3} ({:.1} dBFS)", 20.0 * worst.log10());
    // hostile input
    let mut bad = x.clone();
    bad[10] = f32::NAN;
    bad[20] = f32::INFINITY;
    bad[30] = 1.0e30;
    let (mut l, mut r) = (bad.clone(), bad.clone());
    m.process(&mut l, &mut r);
    assert!(l.iter().chain(&r).all(|s| s.is_finite()));
    // unprepared and empty blocks
    let mut u = DeExpander::default();
    let (mut l, mut r) = (x.clone(), x.clone());
    u.process(&mut l, &mut r);
    m.process(&mut [], &mut []);
    // unknown index and NaN are ignored
    m.set_param(NPARAMS + 3, 1.0);
    m.set_param(0, f32::NAN);
    assert_eq!(m.param(0), Some(PARAMS[0].default));
}

#[test]
fn parameter_table() {
    assert_eq!(PARAMS.len(), NPARAMS);
    let mut ids: Vec<&str> = PARAMS.iter().map(|p| p.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), NPARAMS);
    for p in PARAMS.iter() {
        assert!(p.default >= p.min && p.default <= p.max, "{}", p.id);
        assert!(p.id.chars().next().unwrap().is_ascii_lowercase() && !p.id.contains('_'));
        if p.log {
            assert!(p.min > 0.0, "{}", p.id);
        }
    }
    let m = DeExpander::default();
    assert_eq!(m.params().len(), NPARAMS);
}
