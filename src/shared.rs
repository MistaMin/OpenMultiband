//! Lock-free engine-to-editor telemetry: per-band per-section gains/levels, band summaries,
//! global master limiter, input/output peaks, auto-gain, external key presence, and aligned
//! spectrum taps.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering, Ordering::Relaxed};

use crate::{BAND_SECTIONS, MAX_BANDS};

struct F(AtomicU32);

impl Default for F {
    fn default() -> Self {
        Self(AtomicU32::new(f32::NAN.to_bits()))
    }
}

impl F {
    fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Relaxed))
    }
    fn set(&self, v: f32) {
        self.0.store(v.to_bits(), Relaxed);
    }
    fn merge(&self, v: f32, keep: impl Fn(f32, f32) -> bool) {
        let old = self.get();
        if old.is_nan() || keep(v, old) {
            self.set(v);
        }
    }
    fn take(&self, empty: f32) -> f32 {
        let v = f32::from_bits(self.0.swap(f32::NAN.to_bits(), Relaxed));
        if v.is_nan() { empty } else { v }
    }
}

#[derive(Default)]
struct SectionLive {
    gain: [F; 2],
    level: [F; 2],
    active: AtomicBool,
}

#[derive(Default)]
struct BandLive {
    sections: [SectionLive; BAND_SECTIONS],
    /// Net dynamic gain applied to the band (dB, signed) for the multiband overview bar.
    net_gain: F,
    /// Peak output level of the band (dBFS) for overview activity.
    level: F,
}

/// Samples kept per tap channel, a power of two.
pub const TAP_LEN: usize = 8192;

struct Taps {
    buf: Vec<AtomicU32>,
    head: AtomicUsize,
}

impl Default for Taps {
    fn default() -> Self {
        Self { buf: (0..4 * TAP_LEN).map(|_| AtomicU32::new(0)).collect(), head: AtomicUsize::new(0) }
    }
}

#[derive(Default)]
pub struct Shared {
    taps: Taps,
    bands: [BandLive; MAX_BANDS],
    limiter: SectionLive,
    peak_in: [F; 2],
    peak_out: [F; 2],
    key: AtomicBool,
    rate: F,
    auto_gain: F,
}

/// One dynamics section (either a band's section or the master limiter) as of the last tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct Live {
    /// Signed gain per channel, dB.
    pub gain: [f32; 2],
    /// Detector level per channel, dBFS.
    pub level: [f32; 2],
    pub active: bool,
}

/// Summary for one band on the multiband overview plot.
#[derive(Clone, Copy, Debug, Default)]
pub struct BandSummary {
    /// Net signed dynamic gain in dB.
    pub gain_db: f32,
    /// Post-band peak level in dBFS.
    pub level_db: f32,
}

impl Shared {
    pub(crate) fn band_section(&self, band: usize, sec: usize, gain: [f32; 2], level: [f32; 2], active: bool) {
        let Some(s) = self.bands.get(band).and_then(|b| b.sections.get(sec)) else { return };
        for c in 0..2 {
            s.gain[c].merge(gain[c], |v, old| v.abs() > old.abs());
            s.level[c].merge(level[c], |v, old| v > old);
        }
        s.active.store(active, Relaxed);
    }

    pub(crate) fn band_summary(&self, band: usize, net_gain_db: f32, level_db: f32) {
        let Some(b) = self.bands.get(band) else { return };
        b.net_gain.merge(net_gain_db, |v, old| v.abs() > old.abs());
        b.level.merge(level_db, |v, old| v > old);
    }

    pub(crate) fn limiter(&self, gain: [f32; 2], level: [f32; 2], active: bool) {
        for c in 0..2 {
            self.limiter.gain[c].merge(gain[c], |v, old| v.abs() > old.abs());
            self.limiter.level[c].merge(level[c], |v, old| v > old);
        }
        self.limiter.active.store(active, Relaxed);
    }

    pub(crate) fn peaks(&self, input: [f32; 2], output: [f32; 2]) {
        for c in 0..2 {
            self.peak_in[c].merge(input[c], |v, old| v > old);
            self.peak_out[c].merge(output[c], |v, old| v > old);
        }
    }

    pub(crate) fn push_taps(&self, pre: [&[f32]; 2], post: [&[f32]; 2]) {
        let head = self.taps.head.load(Relaxed);
        for (c, src) in [pre[0], pre[1], post[0], post[1]].into_iter().enumerate() {
            let base = c * TAP_LEN;
            for (j, v) in src.iter().enumerate() {
                self.taps.buf[base + ((head + j) & (TAP_LEN - 1))].store(v.to_bits(), Relaxed);
            }
        }
        self.taps.head.store(head.wrapping_add(pre[0].len()), Ordering::Release);
    }

    pub fn read_tap(&self, channel: usize, out: &mut [f32]) {
        let head = self.taps.head.load(Ordering::Acquire);
        let n = out.len().min(TAP_LEN);
        let base = (channel % 4) * TAP_LEN;
        for (j, o) in out[..n].iter_mut().enumerate() {
            let i = head.wrapping_sub(n).wrapping_add(j) & (TAP_LEN - 1);
            *o = f32::from_bits(self.taps.buf[base + i].load(Relaxed));
        }
    }

    pub(crate) fn set_key_present(&self, on: bool) {
        self.key.store(on, Relaxed);
    }

    pub(crate) fn set_auto_gain(&self, db: f32) {
        self.auto_gain.set(db);
    }

    pub fn auto_gain_db(&self) -> f32 {
        let v = self.auto_gain.get();
        if v.is_nan() { 0.0 } else { v }
    }

    pub(crate) fn set_sample_rate(&self, sr: f32) {
        self.rate.set(sr);
    }

    pub fn take_band_section(&self, band: usize, sec: usize) -> Live {
        let Some(s) = self.bands.get(band).and_then(|b| b.sections.get(sec)) else { return Live::default() };
        Live {
            gain: [s.gain[0].take(0.0), s.gain[1].take(0.0)],
            level: [s.level[0].take(-200.0), s.level[1].take(-200.0)],
            active: s.active.load(Relaxed),
        }
    }

    pub fn take_band_summary(&self, band: usize) -> BandSummary {
        let Some(b) = self.bands.get(band) else { return BandSummary::default() };
        BandSummary { gain_db: b.net_gain.take(0.0), level_db: b.level.take(-200.0) }
    }

    pub fn take_limiter(&self) -> Live {
        Live {
            gain: [self.limiter.gain[0].take(0.0), self.limiter.gain[1].take(0.0)],
            level: [self.limiter.level[0].take(-200.0), self.limiter.level[1].take(-200.0)],
            active: self.limiter.active.load(Relaxed),
        }
    }

    pub fn take_peaks(&self) -> ([f32; 2], [f32; 2]) {
        ([self.peak_in[0].take(0.0), self.peak_in[1].take(0.0)], [self.peak_out[0].take(0.0), self.peak_out[1].take(0.0)])
    }

    pub fn key_present(&self) -> bool {
        self.key.load(Relaxed)
    }

    pub fn sample_rate(&self) -> f32 {
        let r = self.rate.get();
        if r.is_nan() { 48_000.0 } else { r }
    }
}
