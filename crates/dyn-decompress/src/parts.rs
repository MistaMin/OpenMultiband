//! Building blocks: delay line, sliding maximum, level follower, key filter, true-peak reader
//! and the ceiling limiter. Everything is allocated once (`alloc`) and then runs without
//! allocation; ring sizes are powers of two and every index is masked.

/// Taps per phase of the 8x reconstruction filter.
const TP_TAPS: usize = 64;
/// Delay in samples between a sample going into the true-peak reader and coming out.
pub const TP_DELAY: usize = 32;
/// Kaiser window shape of the reconstruction kernel.
const TP_KAISER_BETA: f64 = 8.0;

/// Modified Bessel function of the first kind, order 0 (series).
fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let q = x * x / 4.0;
    for k in 1..40 {
        term *= q / (k as f64 * k as f64);
        sum += term;
    }
    sum
}
/// The true-peak reader looks at this many points per sample period.
pub const OVERSAMPLE: usize = 8;

#[inline]
pub fn one_pole(time_s: f64, sr: f64) -> f64 {
    let n = time_s * sr;
    if n <= 1.0e-3 {
        1.0
    } else {
        1.0 - (-1.0 / n).exp()
    }
}

// ---------------------------------------------------------------------------------------------

#[derive(Default)]
pub struct Delay {
    buf: Vec<f32>,
    mask: usize,
    pos: usize,
}

impl Delay {
    pub fn alloc(&mut self, len: usize) {
        let len = len.max(2).next_power_of_two();
        self.buf = vec![0.0; len];
        self.mask = len - 1;
        self.pos = 0;
    }

    pub fn clear(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
    }

    /// Stores `x` and returns the sample from `d` samples ago (`d = 0` returns `x`).
    #[inline]
    pub fn push(&mut self, x: f32, d: usize) -> f32 {
        if self.buf.is_empty() {
            return x;
        }
        let m = self.mask;
        self.buf[self.pos & m] = x;
        let r = self.buf[(self.pos + m + 1 - (d & m)) & m];
        self.pos = (self.pos + 1) & m;
        r
    }
}

// ---------------------------------------------------------------------------------------------

/// Maximum over the last `window` values (monotonic queue in a ring).
#[derive(Default)]
pub struct SlideMax {
    val: Vec<f64>,
    idx: Vec<u64>,
    mask: usize,
    head: usize,
    tail: usize,
    n: u64,
}

impl SlideMax {
    pub fn alloc(&mut self, len: usize) {
        let len = len.max(4).next_power_of_two();
        self.val = vec![0.0; len];
        self.idx = vec![0; len];
        self.mask = len - 1;
        self.clear();
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.n = 0;
    }

    #[inline]
    pub fn push(&mut self, v: f64, window: usize) -> f64 {
        if self.val.is_empty() || window <= 1 {
            return v;
        }
        let m = self.mask;
        let window = window.clamp(1, m.saturating_sub(1).max(1)) as u64;
        while self.tail != self.head {
            let b = (self.tail + m) & m;
            if self.val[b] <= v {
                self.tail = b;
            } else {
                break;
            }
        }
        self.val[self.tail] = v;
        self.idx[self.tail] = self.n;
        self.tail = (self.tail + 1) & m;
        while self.head != self.tail && self.idx[self.head] + window <= self.n {
            self.head = (self.head + 1) & m;
        }
        self.n += 1;
        if self.head == self.tail {
            self.head = (self.tail + m) & m;
        }
        self.val[self.head]
    }
}

// ---------------------------------------------------------------------------------------------

/// Attack / hold / release follower on a linear level.
///
/// * input at or above the level: `p += (v - p) * attack` (one pole, time constant = attack
///   time), and the hold restarts;
/// * input below the level for more than `hold` samples: `p = max(v, p * release)`
///   (exponential decay, time constant = release time, 8.686 dB per time constant).
#[derive(Clone, Copy, Default)]
pub struct Follower {
    p: f64,
    hold: u32,
}

impl Follower {
    #[inline]
    pub fn step(&mut self, v: f64, attack: f64, release: f64, hold: u32) -> f64 {
        if v >= self.p {
            self.p += (v - self.p) * attack;
            self.hold = hold;
        } else if self.hold > 0 {
            self.hold -= 1;
        } else {
            self.p = (self.p * release).max(v);
            if self.p < 1.0e-30 {
                self.p = 0.0;
            }
        }
        self.p
    }

    pub fn level(&self) -> f64 {
        self.p
    }
}

// ---------------------------------------------------------------------------------------------

/// Second-order Butterworth high- or low-pass (transposed direct form II, f64), or a wire.
#[derive(Clone, Copy, Default)]
pub struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    s: [f64; 2],
    on: bool,
}

impl Biquad {
    /// `high`: high-pass, else low-pass. `on = false` passes the signal unchanged.
    pub fn design(&mut self, high: bool, f: f64, sr: f64, on: bool) {
        self.on = on;
        if !on {
            return;
        }
        let f = f.clamp(1.0, 0.45 * sr);
        let w = std::f64::consts::TAU * f / sr;
        let (sn, cs) = w.sin_cos();
        let alpha = sn / std::f64::consts::SQRT_2; // Q = 1/sqrt(2)
        let a0 = 1.0 + alpha;
        let (b0, b1) = if high { ((1.0 + cs) / 2.0, -(1.0 + cs)) } else { ((1.0 - cs) / 2.0, 1.0 - cs) };
        self.b = [b0 / a0, b1 / a0, b0 / a0];
        self.a = [-2.0 * cs / a0, (1.0 - alpha) / a0];
    }

    pub fn clear(&mut self) {
        self.s = [0.0; 2];
    }

    #[inline]
    pub fn run(&mut self, x: f64) -> f64 {
        if !self.on {
            return x;
        }
        let y = self.b[0] * x + self.s[0];
        self.s[0] = self.b[1] * x - self.a[0] * y + self.s[1];
        self.s[1] = self.b[2] * x - self.a[1] * y;
        if self.s[0].abs() < 1.0e-30 {
            self.s[0] = 0.0;
        }
        if self.s[1].abs() < 1.0e-30 {
            self.s[1] = 0.0;
        }
        y
    }
}

// ---------------------------------------------------------------------------------------------

/// Reads the true peak: the largest magnitude among each sample and the seven values an 8x
/// windowed-sinc reconstruction (64 taps per phase, Kaiser window) puts between it and the
/// next sample.
pub struct TruePeak {
    h: [[f32; TP_TAPS]; OVERSAMPLE - 1],
    buf: [f32; 2 * TP_TAPS],
    pos: usize,
}

impl Default for TruePeak {
    fn default() -> Self {
        Self::new()
    }
}

impl TruePeak {
    pub fn new() -> Self {
        let mut h = [[0.0f32; TP_TAPS]; OVERSAMPLE - 1];
        for (p, row) in h.iter_mut().enumerate() {
            let frac = (p + 1) as f64 / OVERSAMPLE as f64;
            let mut sum = 0.0;
            let mut tmp = [0.0f64; TP_TAPS];
            for (j, t) in tmp.iter_mut().enumerate() {
                // slot j holds x[n - 63 + j]; the wanted instant is n - 32 + frac
                let a = (TP_DELAY - 1) as f64 - j as f64 + frac;
                let s = if a.abs() < 1.0e-12 { 1.0 } else { (std::f64::consts::PI * a).sin() / (std::f64::consts::PI * a) };
                let u = a / TP_DELAY as f64;
                let w = if u.abs() < 1.0 { bessel_i0(TP_KAISER_BETA * (1.0 - u * u).sqrt()) / bessel_i0(TP_KAISER_BETA) } else { 0.0 };
                *t = s * w;
                sum += *t;
            }
            for (o, t) in row.iter_mut().zip(tmp.iter()) {
                *o = (*t / sum) as f32;
            }
        }
        Self { h, buf: [0.0; 2 * TP_TAPS], pos: 0 }
    }

    pub fn clear(&mut self) {
        self.buf = [0.0; 2 * TP_TAPS];
        self.pos = 0;
    }

    /// Stores `x`; returns the sample from `TP_DELAY` samples ago and the largest magnitude of
    /// it and the seven reconstructed values after it (with `true_peak = false`, its magnitude).
    #[inline]
    pub fn push(&mut self, x: f32, true_peak: bool) -> (f32, f32) {
        let p = self.pos & (TP_TAPS - 1);
        self.buf[p] = x;
        self.buf[p + TP_TAPS] = x;
        self.pos = (p + 1) & (TP_TAPS - 1);
        let d = self.buf[self.pos + TP_TAPS - 1 - TP_DELAY];
        let mut pk = d.abs();
        if true_peak {
            let w = &self.buf[self.pos..self.pos + TP_TAPS];
            for row in self.h.iter() {
                let mut v = 0.0f32;
                for j in 0..TP_TAPS {
                    v += w[j] * row[j];
                }
                pk = pk.max(v.abs());
            }
        }
        (d, pk)
    }
}

// ---------------------------------------------------------------------------------------------

/// Stereo-linked look-ahead peak limiter with a hard ceiling.
///
/// Wanted gain per sample `t = min(1, ceiling / peak)`; minimum of `t` over `w + 1` samples;
/// an exponential release that never rises above that minimum; a `w`-sample moving average;
/// applied to the signal delayed by `w - 1`. Every averaged term is at most the gain the
/// delayed sample needs, so no sample (or 8x reconstructed value, in true-peak mode) exceeds
/// the ceiling. The delay is the same whether the limiter is engaged or not.
#[derive(Default)]
pub struct Limiter {
    tp: [TruePeak; 2],
    dl: [Delay; 2],
    smin: SlideMax,
    ring: Vec<f64>,
    mask: usize,
    pos: usize,
    sum: f64,
    rel: f64,
    w: usize,
    krel: f64,
    ones: u32,
}

impl Limiter {
    pub fn alloc(&mut self, w: usize, krel: f64) {
        let w = w.max(2);
        self.w = w;
        self.krel = krel;
        let len = (w + 2).next_power_of_two();
        self.dl[0].alloc(len);
        self.dl[1].alloc(len);
        self.smin.alloc(w + 4);
        self.ring = vec![1.0; len];
        self.mask = len - 1;
        self.clear();
    }

    pub fn clear(&mut self) {
        self.tp[0].clear();
        self.tp[1].clear();
        self.dl[0].clear();
        self.dl[1].clear();
        self.smin.clear();
        self.ring.iter_mut().for_each(|v| *v = 1.0);
        self.pos = 0;
        self.sum = self.w as f64;
        self.rel = 1.0;
        self.ones = u32::MAX;
    }

    /// Samples of delay through the limiter.
    pub fn delay(&self) -> usize {
        TP_DELAY + self.w.max(1) - 1
    }

    /// Returns the limited pair and the gain applied. With `on = false` the signal is only
    /// delayed (gain 1).
    #[inline]
    pub fn process(&mut self, l: f32, r: f32, ceiling: f32, true_peak: bool, on: bool) -> (f32, f32, f32) {
        if self.ring.is_empty() {
            return (l, r, 1.0);
        }
        let (dl, pa) = self.tp[0].push(l, true_peak && on);
        let (dr, pb) = self.tp[1].push(r, true_peak && on);
        let pk = pa.max(pb);
        let t = if on && pk > ceiling { (ceiling / pk) as f64 } else { 1.0 };
        let m = -self.smin.push(-t, self.w + 1);
        self.rel = m.min(self.rel + (1.0 - self.rel) * self.krel);
        let old = self.ring[(self.pos + self.mask + 1 - self.w) & self.mask];
        self.ring[self.pos & self.mask] = self.rel;
        self.pos = (self.pos + 1) & self.mask;
        self.sum += self.rel - old;
        if self.rel == 1.0 {
            self.ones = self.ones.saturating_add(1);
            if self.ones as usize >= self.w {
                // the whole average window is 1: remove rounding drift, unity is exact
                self.sum = self.w as f64;
            }
        } else {
            self.ones = 0;
        }
        let g = ((self.sum / self.w as f64) as f32).clamp(0.0, 1.0);
        let mut yl = self.dl[0].push(dl, self.w - 1) * g;
        let mut yr = self.dl[1].push(dr, self.w - 1) * g;
        if on {
            // rounding guard only: by construction the samples are already within the ceiling
            yl = yl.clamp(-ceiling, ceiling);
            yr = yr.clamp(-ceiling, ceiling);
        }
        (yl, yr, g)
    }
}

// ---------------------------------------------------------------------------------------------

/// Moving average over the last `len` values (look-ahead ramp). After a hold of the same
/// length it makes a step into a straight ramp that reaches the held value exactly `len - 1`
/// samples later: with an audio delay of `len - 1`, the ramp ends on the onset sample.
/// The running sum is recomputed once per ring turn, so it never drifts.
#[derive(Default)]
pub struct BoxAvg {
    buf: Vec<f64>,
    mask: usize,
    pos: usize,
    len: usize,
    sum: f64,
    turn: usize,
}

impl BoxAvg {
    pub fn alloc(&mut self, len: usize) {
        let cap = len.max(4).next_power_of_two();
        self.buf = vec![0.0; cap];
        self.mask = cap - 1;
        self.clear();
    }

    pub fn clear(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
        self.len = 0;
        self.sum = 0.0;
        self.turn = 0;
    }

    #[inline]
    pub fn push(&mut self, v: f64, len: usize) -> f64 {
        if self.buf.is_empty() || len <= 1 {
            if !self.buf.is_empty() {
                self.buf[self.pos] = v;
                self.pos = (self.pos + 1) & self.mask;
                self.len = 0;
            }
            return v;
        }
        let m = self.mask;
        let len = len.min(m);
        self.buf[self.pos] = v;
        self.turn += 1;
        if len != self.len || self.turn > m {
            self.len = len;
            self.turn = 0;
            self.sum = (0..len).map(|k| self.buf[(self.pos + m + 1 - k) & m]).sum();
        } else {
            self.sum += v - self.buf[(self.pos + m + 1 - len) & m];
        }
        self.pos = (self.pos + 1) & m;
        self.sum / len as f64
    }
}

