//! Building blocks: delay line, sliding maximum, level follower and key filter. Everything
//! is allocated once (`alloc`) and then runs without allocation; ring sizes are powers of two
//! and every index is masked.

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

    /// Fills the history with `v` (a reset to a known state).
    pub fn fill(&mut self, v: f64) {
        self.buf.iter_mut().for_each(|b| *b = v);
        self.len = 0;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Hold then average: the ramp reaches the held step exactly on the delayed onset.
    #[test]
    fn hold_and_ramp_land_on_the_onset() {
        let w = 49;
        let (mut h, mut b, mut d) = (SlideMax::default(), BoxAvg::default(), Delay::default());
        h.alloc(64);
        b.alloc(64);
        d.alloc(64);
        let mut out = Vec::new();
        for i in 0..300 {
            let x = if i >= 100 { 1.0 } else { 0.0 };
            let level = b.push(h.push(x, w), w);
            let audio = d.push(x as f32, w - 1);
            out.push((level, audio));
        }
        // Every delayed sample sees a level at least as high as itself: never late.
        assert!(out.iter().all(|(l, a)| *l >= *a as f64 - 1e-12));
        // The ramp starts exactly w-1 samples before the onset and is straight.
        let onset = 100 + w - 1;
        assert!((out[onset].0 - 1.0).abs() < 1e-12 && out[100 - 1].0 == 0.0);
        assert!((out[100 + (w - 1) / 2].0 - 0.5 - 0.5 / w as f64).abs() < 1e-9);
    }
}
