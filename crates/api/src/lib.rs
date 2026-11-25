//! The contract every OpenLimiter processing module implements.
//!
//! A module is one stereo processor with its own parameters. The plugin owns a chain of
//! modules, in an order the user chooses, and adds their latencies. A module knows nothing
//! about the host, the editor or the other modules.
//!
//! Rules for an implementation:
//! * `prepare` allocates everything. `process`, `set_param` and `reset` never allocate, lock,
//!   block or panic. Index with masks or checked lengths, not with values that can run off.
//! * Parameters are plain values in the unit the description names (dB, ms, Hz, ...), never 0..1.
//! * `process` works in place on two equal-length slices no longer than `max_block`.
//! * `latency` may change after `set_param`; the plugin asks again after every change.

/// What a parameter's value means and how a host should show it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Db,
    Ms,
    Hz,
    Percent,
    /// A ratio such as 4 for 4:1.
    Ratio,
    /// A plain number.
    Plain,
    /// 0 is off, 1 is on.
    Toggle,
    /// An index into the labels: 0, 1, 2, ...
    Choice(&'static [&'static str]),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamDesc {
    /// Stable within the module, lower camel case, for example `"threshold"`.
    pub id: &'static str,
    /// What the user reads, for example `"Threshold"`.
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub kind: Kind,
    /// The control moves geometrically between `min` and `max` (times, frequencies).
    pub log: bool,
    /// Discrete positions including both ends; 0 means continuous.
    pub steps: u32,
}

impl ParamDesc {
    pub const fn new(id: &'static str, name: &'static str, min: f32, max: f32, default: f32, kind: Kind) -> Self {
        Self { id, name, min, max, default, kind, log: false, steps: 0 }
    }

    pub const fn log(mut self) -> Self {
        self.log = true;
        self
    }

    pub const fn steps(mut self, steps: u32) -> Self {
        self.steps = steps;
        self
    }

    pub const fn toggle(id: &'static str, name: &'static str, on: bool) -> Self {
        Self::new(id, name, 0.0, 1.0, if on { 1.0 } else { 0.0 }, Kind::Toggle).steps(2)
    }

    pub const fn choice(id: &'static str, name: &'static str, labels: &'static [&'static str], default: usize) -> Self {
        Self::new(id, name, 0.0, (labels.len() - 1) as f32, default as f32, Kind::Choice(labels)).steps(labels.len() as u32)
    }
}

/// What a module shows on meters. All fields are for the block just processed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Meters {
    /// The deepest gain reduction, in dB, positive. 0 when the module does not reduce gain.
    pub reduction_db: f32,
    /// Per band, for multiband modules; unused bands stay 0.
    pub band_reduction_db: [f32; 8],
}

pub trait Module: Send {
    /// The parameters, in the order of the indices `set_param` takes.
    fn params(&self) -> &'static [ParamDesc];

    /// Allocates for this sample rate and block size, and clears state. Parameters set before
    /// or after are kept.
    fn prepare(&mut self, sample_rate: f32, max_block: usize);

    /// Clears histories and envelopes. Keeps parameters.
    fn reset(&mut self);

    /// Sets parameter `index` to a plain value. Out-of-range values are clamped; an unknown
    /// index is ignored.
    fn set_param(&mut self, index: usize, value: f32);

    /// Samples between input and output.
    fn latency(&self) -> usize;

    /// Processes one block in place.
    fn process(&mut self, left: &mut [f32], right: &mut [f32]);

    /// Like `process`, with a key signal for detectors. The default ignores the key.
    fn process_keyed(&mut self, left: &mut [f32], right: &mut [f32], _key_left: &[f32], _key_right: &[f32]) {
        self.process(left, right);
    }

    fn meters(&self) -> Meters {
        Meters::default()
    }

    /// The host renders offline (bounce, export) rather than in real time. A module may
    /// switch to a costlier setting then, but must keep `latency()` unchanged. Never
    /// allocates. The default ignores it.
    fn set_offline(&mut self, _offline: bool) {}

    /// Zero-latency mode: drop every look-ahead so `latency()` returns 0. The module keeps
    /// its curve; transients then meet the attack without warning. Never allocates. The
    /// default ignores it (for modules that never add latency).
    fn set_zero_latency(&mut self, _on: bool) {}

    /// The largest gain the module ADDED in the block just processed, dB, positive (upward
    /// expanders and compressors). 0 for modules that only turn gain down.
    fn lift_db(&self) -> f32 {
        0.0
    }

    /// A module-defined read-out for the editor, taken after each block: writes up to
    /// `out.len()` values and returns how many. Each module documents its slot layout
    /// (offsets as constants). Never allocates. The default writes nothing.
    fn view(&self, _out: &mut [f32]) -> usize {
        0
    }
}
