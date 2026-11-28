//! Auto Gain: what you hear is matched to the loudness of the plugin's input (before the
//! input gain), so a section does not sound better only for being louder or quieter.
//!
//! As OpenLimiter's monitor stage (and Harmonic Compressor's ADR-0001): K-weighted power
//! (BS.1770) of the raw input and of the output, each averaged over about 3 s; the output
//! gain follows their ratio within +-24 dB in a 50 ms glide, and holds while either side is
//! quiet (about -60 LUFS). It changes what leaves the plugin, so it must be off when printing.
//! Off and settled, the audio is not touched.

/// The loudness compared, as an exponential average: about the short-term window (3 s).
const AVG_S: f64 = 3.0;
/// How fast the gain follows, seconds.
const GLIDE_S: f32 = 0.05;
/// In f32 a glide this slow stalls short of its target; it lands from within this.
const SNAP: f32 = 2e-4;
pub const RANGE_DB: f32 = 24.0;
/// Below this K-weighted power (about -60 LUFS) a side counts as quiet: the gain is held.
const QUIET: f64 = 1e-6;

/// BS.1770 K-weighting: high shelf, then high-pass, as biquad `[b0, b1, b2, a1, a2]`.
fn k_weighting(sr: f64) -> ([f64; 5], [f64; 5]) {
    if (sr - 48_000.0).abs() < 0.5 {
        return (
            [1.53512485958697, -2.69169618940638, 1.19839281085285, -1.69065929318241, 0.73248077421585],
            [1.0, -2.0, 1.0, -1.99004745483398, 0.99007225036621],
        );
    }
    let (f0, g, q) = (1681.974450955533_f64, 3.999843853973347_f64, 0.7071752369554196_f64);
    let k = (std::f64::consts::PI * f0 / sr).tan();
    let vh = 10f64.powf(g / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    let s1 = [(vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0, 2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0];
    let (f0, q) = (38.13547087602444_f64, 0.5003270373238773_f64);
    let k = (std::f64::consts::PI * f0 / sr).tan();
    let a0 = 1.0 + k / q + k * k;
    let s2 = [1.0, -2.0, 1.0, 2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0];
    (s1, s2)
}

#[derive(Clone, Copy, Default)]
struct Biquad {
    c: [f64; 5],
    z: [f64; 2],
}

impl Biquad {
    fn run(&mut self, x: f64) -> f64 {
        let [b0, b1, b2, a1, a2] = self.c;
        let y = b0 * x + self.z[0];
        self.z[0] = b1 * x - a1 * y + self.z[1];
        self.z[1] = b2 * x - a2 * y;
        // Flush denormals: in silence the state decays into them and x86 slows down badly.
        for z in &mut self.z {
            if z.abs() < 1e-30 {
                *z = 0.0;
            }
        }
        y
    }
}

/// K-weighted power of a stereo signal, averaged.
#[derive(Clone, Copy, Default)]
struct Power {
    filters: [[Biquad; 2]; 2],
    avg: f64,
}

impl Power {
    fn new(sr: f64) -> Self {
        let (s1, s2) = k_weighting(sr);
        let f = [Biquad { c: s1, z: [0.0; 2] }, Biquad { c: s2, z: [0.0; 2] }];
        Self { filters: [f, f], avg: 0.0 }
    }

    fn feed(&mut self, l: f32, r: f32, coef: f64) {
        let mut sum = 0.0;
        for (c, x) in [l, r].into_iter().enumerate() {
            let [a, b] = &mut self.filters[c];
            let x = if x.is_finite() { f64::from(x) } else { 0.0 }; // ST-2: NaN would freeze the gain
            let y = b.run(a.run(x));
            sum += y * y;
        }
        self.avg += (sum - self.avg) * coef;
        if self.avg < 1e-30 {
            self.avg = 0.0;
        }
    }
}

pub struct AutoGain {
    raw: Power,
    out: Power,
    avg_coef: f64,
    glide: f32,
    gain: f32,
    gain_target: f32,
    pub on: bool,
}

impl AutoGain {
    pub fn new(sample_rate: f32, on: bool) -> Self {
        let sr = f64::from(sample_rate);
        Self {
            raw: Power::new(sr),
            out: Power::new(sr),
            avg_coef: 1.0 - (-1.0 / (AVG_S * sr)).exp(),
            glide: 1.0 - (-1.0 / (GLIDE_S * sample_rate)).exp(),
            gain: 1.0,
            gain_target: 1.0,
            on,
        }
    }

    /// The gain applied now, dB (0 when off and settled).
    pub fn gain_db(&self) -> f32 {
        20.0 * self.gain.max(1e-6).log10()
    }

    /// The plugin's input before the input gain: the loudness matched.
    pub fn feed_raw(&mut self, l: &[f32], r: &[f32]) {
        for (&a, &b) in l.iter().zip(r) {
            self.raw.feed(a, b, self.avg_coef);
        }
    }

    /// Turns the output into what is heard, in place.
    #[cfg(test)]
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        self.feed_out(l, r);
        self.apply(l, r);
    }

    /// The output whose loudness is matched (with Delta on: the output before the difference).
    pub fn feed_out(&mut self, l: &[f32], r: &[f32]) {
        for (&a, &b) in l.iter().zip(r.iter()) {
            self.out.feed(a, b, self.avg_coef);
        }
    }

    /// Applies the matching gain, in place.
    pub fn apply(&mut self, l: &mut [f32], r: &mut [f32]) {
        if self.on {
            if self.raw.avg > QUIET && self.out.avg > QUIET {
                let db = (10.0 * (self.raw.avg / self.out.avg).log10()) as f32;
                self.gain_target = 10f32.powf(db.clamp(-RANGE_DB, RANGE_DB) / 20.0);
            }
        } else {
            self.gain_target = 1.0;
        }
        if self.gain == 1.0 && self.gain_target == 1.0 {
            return;
        }
        for (a, b) in l.iter_mut().zip(r.iter_mut()) {
            self.gain += (self.gain_target - self.gain) * self.glide;
            if (self.gain - self.gain_target).abs() <= SNAP * self.gain_target {
                self.gain = self.gain_target;
            }
            *a *= self.gain;
            *b *= self.gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A section that makes everything 9 dB louder: Auto Gain takes the 9 dB back.
    #[test]
    fn matches_the_input_loudness() {
        let sr = 48_000.0;
        let mut ag = AutoGain::new(sr, true);
        let n = 480;
        let up = 10f32.powf(9.0 / 20.0);
        let mut last = Vec::new();
        for b in 0..2000 {
            let x: Vec<f32> = (0..n).map(|i| 0.1 * ((b * n + i) as f32 * 0.07).sin()).collect();
            ag.feed_raw(&x, &x);
            let mut l: Vec<f32> = x.iter().map(|v| v * up).collect();
            let mut r = l.clone();
            ag.process(&mut l, &mut r);
            last = l;
        }
        assert!((ag.gain_db() + 9.0).abs() < 0.05, "{}", ag.gain_db());
        let peak = last.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((peak - 0.1).abs() < 0.002, "{peak}");
    }

    #[test]
    fn off_is_a_wire() {
        let mut ag = AutoGain::new(48_000.0, false);
        let x: Vec<f32> = (0..512).map(|i| (i as f32 * 0.1).sin()).collect();
        ag.feed_raw(&x, &x);
        let (mut l, mut r) = (x.clone(), x.clone());
        ag.process(&mut l, &mut r);
        assert_eq!(l, x);
    }
}
