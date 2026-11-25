//! `dynamics`: the gate/expander, the compressor and the look-ahead limiter of OpenDynamics.
//!
//! Every section is a gain multiplier and nothing else: no saturation, no clipper, no
//! waveshaping. The audio is only ever `x * g`, with `g` from an exact static curve
//! ([`curve`]) smoothed by ballistics on the gain in dB, in f64.
//!
//! Topology (feed-forward, log domain, after the "smooth branching" detector in Giannoulis,
//! Massberg and Reiss, "Digital Dynamic Range Compressor Design", JAES 2012):
//!
//! ```text
//! key -> key high-pass -> key low-pass -> rectify (peak |x|, or RMS) -> dB -> stereo link
//!     -> static curve (target gain, dB) -> [look-ahead: hold the deepest target]
//!     -> attack / release one-pole on the gain in dB -> gain
//! audio -> look-ahead delay -> x * gain -> out
//! ```
//!
//! Ballistics on the gain, not on the level, keep the static curve exact in the steady
//! state and the time constants independent of level and ratio.
#![forbid(unsafe_code)]

pub mod auto;
pub mod curve;
mod detector;
pub mod gate;
pub mod keyboost;
pub mod comp;
pub mod limiter;
mod parts;

pub use comp::Compressor;
pub use gate::Gate;
pub use limiter::Limiter;

/// Read-out slots every OpenDynamics section writes (the de-expander and decompressor crates
/// use the same first four).
pub mod view {
    /// Signed gain per channel, dB (negative = down), the furthest from 0 in the block.
    pub const GAIN: usize = 0;
    /// Detector level per channel, dBFS, the highest in the block.
    pub const DETECTOR: usize = 2;
    pub const LEN: usize = 4;
}

pub(crate) const FLOOR_DB: f64 = -200.0;

#[inline]
pub(crate) fn db_to_lin(db: f64) -> f64 {
    (db * (std::f64::consts::LN_10 / 20.0)).exp()
}

#[inline]
pub(crate) fn lin_to_db(v: f64) -> f64 {
    if v > 1.0e-10 {
        20.0 * v.log10()
    } else {
        FLOOR_DB
    }
}

#[inline]
pub(crate) fn clean(x: f64) -> f32 {
    if x.is_finite() {
        x as f32
    } else {
        0.0
    }
}
