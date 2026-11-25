//! Static curves: target gain in dB for a detector level in dBFS. Public, so the editor draws
//! exactly what the audio does.
//!
//! All of them are built on one soft knee: [`knee`] is `max(0, x)` with the corner replaced by
//! a parabola of width `w` centred on `x = 0`, so value and slope are continuous (C1) and the
//! curve meets its straight parts exactly at `x = -w/2` and `x = +w/2`.

/// `max(0, x)` with a quadratic knee of total width `w` (dB) centred on 0.
#[inline]
pub fn knee(x: f64, w: f64) -> f64 {
    let h = 0.5 * w.max(0.0);
    if x <= -h {
        0.0
    } else if x >= h {
        x
    } else {
        (x + h) * (x + h) / (2.0 * w)
    }
}

/// Downward compressor, `ratio:1` above `threshold`: gain = `(1/ratio - 1) * knee(L - T)`.
/// Above the knee the output level is exactly `T + (L - T) / ratio`.
#[inline]
pub fn compressor(level: f64, threshold: f64, ratio: f64, knee_db: f64) -> f64 {
    (1.0 / ratio.max(1.0) - 1.0) * knee(level - threshold, knee_db)
}

/// Downward expander, `1:ratio` below `threshold`, never more than `range` dB down:
/// gain = `max(-range, -(ratio - 1) * knee(T - L))`. Below the knee the output level is
/// exactly `T + (L - T) * ratio` until the range is used up.
#[inline]
pub fn expander(level: f64, threshold: f64, ratio: f64, knee_db: f64, range: f64) -> f64 {
    (-(ratio.max(1.0) - 1.0) * knee(threshold - level, knee_db)).max(-range.max(0.0))
}

/// Gate: fully open (0 dB) at or above `threshold`, `range` dB down below it. The step is
/// made smooth in time by the ballistics, not in level.
#[inline]
pub fn gate(level: f64, threshold: f64, range: f64) -> f64 {
    if level >= threshold {
        0.0
    } else {
        -range.max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knee_is_c1_and_exact_outside() {
        let w = 6.0;
        assert_eq!(knee(-3.0, w), 0.0);
        assert_eq!(knee(3.0, w), 3.0);
        assert!((knee(0.0, w) - 0.75).abs() < 1e-12); // w / 8
        let d = |x: f64| (knee(x + 1e-6, w) - knee(x - 1e-6, w)) / 2e-6;
        assert!(d(-3.0).abs() < 1e-5 && (d(3.0) - 1.0).abs() < 1e-5 && (d(0.0) - 0.5).abs() < 1e-5);
        assert_eq!(knee(-1e-9, 0.0), 0.0);
        assert_eq!(knee(2.0, 0.0), 2.0);
    }

    #[test]
    fn compressor_output_level_follows_the_ratio() {
        for ratio in [1.5, 2.0, 4.0, 10.0, 30.0] {
            for l in [-10.0, -5.0, 0.0, 6.0] {
                let out = l + compressor(l, -20.0, ratio, 6.0);
                assert!((out - (-20.0 + (l + 20.0) / ratio)).abs() < 1e-9, "{ratio} {l}");
            }
            assert_eq!(compressor(-30.0, -20.0, ratio, 6.0), 0.0);
        }
        assert_eq!(compressor(0.0, -20.0, 1.0, 0.0), 0.0);
    }

    #[test]
    fn expander_follows_the_ratio_until_the_range() {
        let out = |l: f64| l + expander(l, -40.0, 2.0, 0.0, 30.0);
        assert!((out(-50.0) - (-60.0)).abs() < 1e-9);
        assert!((out(-60.0) - (-80.0)).abs() < 1e-9);
        assert!((expander(-100.0, -40.0, 2.0, 0.0, 30.0) + 30.0).abs() < 1e-9);
        assert_eq!(expander(-30.0, -40.0, 4.0, 6.0, 30.0), 0.0);
    }
}
