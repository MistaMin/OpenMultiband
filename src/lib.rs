// Copyright (c) 2026 Marcos Quirino Deida
//! OpenMultiband: 1-to-6-band Linkwitz-Riley multiband dynamics processor built from OpenDynamics.
//!
//! ```text
//! in -> Input Gain -> Linkwitz-Riley Crossover (1..=6 bands, 12/24/48 dB/oct)
//!    -> per band: Decompressor -> Gate/Expander -> Compressor -> De-Expander
//!                 -> Band Latency Align -> Band Gain / Bypass / Solo / Mute
//!    -> Band Sum -> Global Broadband Limiter -> Output Gain -> Delta -> Auto Gain -> out
//! ```
//!
//! Every band aligns to the longest active band's latency before summing so crossovers stay
//! phase-coherent even when bands use different look-ahead times. The dry reference passes
//! through a matched all-pass chain and the total latency delay so Delta mode nulls to silence
//! at unity across all band counts and slopes.

mod autogain;
pub mod crossover;
mod shared;
mod ui;

use std::sync::{Arc, LazyLock};

use crossover::{Crossover, Slope, DEFAULT_FREQS, FREQ_MAX, FREQ_MIN, SLOPE_LABELS};
use olm_api::{Kind, Module, ParamDesc};
use openplugin::core::EditorState;
use openplugin::prelude::*;

pub use crossover::{MAX_BANDS, MAX_SPLITS};
pub use shared::Shared;
pub use ui::make_editor;

/// Four dynamics sections per band, in their stable parameter index order.
pub const BAND_SECTIONS: usize = 4;
/// Processing and tab order within each band: Decompressor -> Gate/Expander -> Compressor -> De-Expander.
pub const BAND_CHAIN: [usize; BAND_SECTIONS] = [3, 0, 1, 2];
pub const BAND_SECTION_KEYS: [(&str, &str, &str, bool); BAND_SECTIONS] = [
    ("gate", "Gate / Expander", "Gate", false),
    ("comp", "Compressor", "Comp", true),
    ("deexp", "De-Expander", "DeExp", false),
    ("decomp", "Decompressor", "DeComp", false),
];
pub const BAND_COUNT_CHOICES: &[&str] = &["1", "2", "3", "4", "5", "6"];

// Global & crossover parameter indices.
pub const P_INPUT: usize = 0;
pub const P_OUTPUT: usize = 1;
pub const P_ZERO_LATENCY: usize = 2;
pub const P_AUTO_GAIN: usize = 3;
pub const P_DELTA: usize = 4;
pub const P_BAND_COUNT: usize = 5;
pub const P_XOVER_SLOPE: usize = 6;
pub const P_XOVER_BASE: usize = 7; // 5 crossover frequencies: 7..12

/// Parameter indices for one dynamics section.
pub struct SectionInfo {
    pub key: &'static str,
    pub label: &'static str,
    pub on: usize,
    pub base: usize,
    pub count: usize,
}

/// Parameter indices for one band.
pub struct BandInfo {
    pub index: usize,
    pub gain: usize,
    pub solo: usize,
    pub mute: usize,
    pub bypass: usize,
    pub sections: [SectionInfo; BAND_SECTIONS],
}

pub struct ParamLayout {
    pub limiter: SectionInfo,
    pub bands: [BandInfo; MAX_BANDS],
}

fn band_modules() -> [Box<dyn Module>; BAND_SECTIONS] {
    [
        Box::new(dynamics::Gate::default()),
        Box::new(dynamics::Compressor::default()),
        Box::new(dyn_deexpand::DeExpander::default()),
        Box::new(dyn_decompress::Decompressor::default()),
    ]
}

/// Decompressor ceiling starts off because the global Master Limiter sits after the band sum.
fn default_for(key: &str, d: &ParamDesc) -> f32 {
    match (key, d.id) {
        ("decomp", "ceilingOn") => 0.0,
        _ => d.default,
    }
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Every dB and ratio parameter with whole-number ends moves in whole steps (standing house rule).
fn whole_steps(d: &ParamDesc) -> u32 {
    let whole = |v: f32| (v - v.round()).abs() < 1e-6;
    match d.kind {
        Kind::Db | Kind::Ratio if d.steps == 0 && whole(d.min) && whole(d.max) => (d.max - d.min).round() as u32 + 1,
        _ => d.steps,
    }
}

fn info_for(id_prefix: &str, name_prefix: &str, group: &'static str, sec_key: &str, d: &ParamDesc) -> ParamInfo {
    let unit = match d.kind {
        Kind::Db => Unit::Db,
        Kind::Ms => Unit::Ms,
        Kind::Hz => Unit::Hz,
        Kind::Percent => Unit::Percent,
        Kind::Ratio => Unit::Ratio,
        Kind::Plain => Unit::None,
        Kind::Toggle => Unit::Toggle,
        Kind::Choice(_) => Unit::Choice,
    };
    let mut info = ParamInfo::new(
        leak(format!("{id_prefix}.{}", d.id)),
        leak(format!("{name_prefix} {}", d.name)),
        d.min,
        d.max,
        default_for(sec_key, d),
        unit,
    )
    .with_steps(whole_steps(d))
    .with_group(group);
    if d.log {
        info = info.with_taper(Taper::Log);
    }
    if let Kind::Choice(labels) = d.kind {
        info = info.with_choices(labels);
    }
    info
}

static TABLE: LazyLock<(Vec<ParamInfo>, ParamLayout)> = LazyLock::new(|| {
    let mut params = vec![
        ParamInfo::new("input", "Input Gain", -24.0, 24.0, 0.0, Unit::Db).with_steps(49).with_group("Global"),
        ParamInfo::new("output", "Output Gain", -24.0, 24.0, 0.0, Unit::Db).with_steps(49).with_group("Global"),
        ParamInfo::new("zeroLatency", "Zero Latency", 0.0, 1.0, 0.0, Unit::Toggle).with_group("Global"),
        ParamInfo::new("autoGain", "Auto Gain", 0.0, 1.0, 0.0, Unit::Toggle).with_group("Global"),
        ParamInfo::new("delta", "Delta", 0.0, 1.0, 0.0, Unit::Toggle).with_group("Global"),
        ParamInfo::new("bandCount", "Bands", 0.0, (MAX_BANDS - 1) as f32, 3.0, Unit::Choice)
            .with_choices(BAND_COUNT_CHOICES)
            .with_steps(MAX_BANDS as u32)
            .with_group("Crossover"),
        ParamInfo::new("xoverSlope", "Crossover Slope", 0.0, (SLOPE_LABELS.len() - 1) as f32, 1.0, Unit::Choice)
            .with_choices(SLOPE_LABELS)
            .with_steps(SLOPE_LABELS.len() as u32)
            .with_group("Crossover"),
    ];
    for (k, &def) in DEFAULT_FREQS.iter().enumerate() {
        let n = k + 1;
        params.push(
            ParamInfo::new(leak(format!("xover{n}")), leak(format!("Crossover {n}")), FREQ_MIN, FREQ_MAX, def, Unit::Hz)
                .with_taper(Taper::Log)
                .with_group("Crossover"),
        );
    }

    // Global Master Limiter (after band sum).
    let lim_mod = dynamics::Limiter::default();
    let lim_on = params.len();
    params.push(ParamInfo::new("limit.on", "Limiter On", 0.0, 1.0, 1.0, Unit::Toggle).with_group("Master Limiter"));
    let lim_base = params.len();
    for d in lim_mod.params() {
        params.push(info_for("limit", "Limiter", "Master Limiter", "limit", d));
    }
    let limiter = SectionInfo { key: "limit", label: "Master Limiter", on: lim_on, base: lim_base, count: lim_mod.params().len() };

    // Per-band controls & 4-section dynamics chains.
    let template_mods = band_modules();
    let mut bands_vec = Vec::with_capacity(MAX_BANDS);
    for b in 0..MAX_BANDS {
        let bn = b + 1;
        let grp = leak(format!("Band {bn}"));
        let gain = params.len();
        params.push(ParamInfo::new(leak(format!("b{bn}.gain")), leak(format!("Band {bn} Gain")), -24.0, 24.0, 0.0, Unit::Db).with_steps(49).with_group(grp));
        let solo = params.len();
        params.push(ParamInfo::new(leak(format!("b{bn}.solo")), leak(format!("Band {bn} Solo")), 0.0, 1.0, 0.0, Unit::Toggle).with_group(grp));
        let mute = params.len();
        params.push(ParamInfo::new(leak(format!("b{bn}.mute")), leak(format!("Band {bn} Mute")), 0.0, 1.0, 0.0, Unit::Toggle).with_group(grp));
        let bypass = params.len();
        params.push(ParamInfo::new(leak(format!("b{bn}.bypass")), leak(format!("Band {bn} Bypass")), 0.0, 1.0, 0.0, Unit::Toggle).with_group(grp));

        let mut secs_vec = Vec::with_capacity(BAND_SECTIONS);
        for ((key, label, short_name, def_on), m) in BAND_SECTION_KEYS.iter().zip(template_mods.iter()) {
            // A "/" in a host group path starts a subgroup: "Gate / Expander" -> "Gate & Expander".
            let sec_grp = leak(format!("Band {bn} {}", label.replace(" / ", " & ")));
            let on_idx = params.len();
            params.push(
                ParamInfo::new(leak(format!("b{bn}.{key}.on")), leak(format!("B{bn} {short_name} On")), 0.0, 1.0, f32::from(u8::from(*def_on)), Unit::Toggle)
                    .with_group(sec_grp),
            );
            let base = params.len();
            let id_prefix = format!("b{bn}.{key}");
            let name_prefix = format!("B{bn} {short_name}");
            for d in m.params() {
                params.push(info_for(&id_prefix, &name_prefix, sec_grp, key, d));
            }
            secs_vec.push(SectionInfo { key, label, on: on_idx, base, count: m.params().len() });
        }
        let sections: [SectionInfo; BAND_SECTIONS] = secs_vec.try_into().ok().expect("4 sections");
        bands_vec.push(BandInfo { index: b, gain, solo, mute, bypass, sections });
    }
    let bands: [BandInfo; MAX_BANDS] = bands_vec.try_into().ok().expect("6 bands");
    (params, ParamLayout { limiter, bands })
});

pub static LAYOUT: LazyLock<&'static ParamLayout> = LazyLock::new(|| &TABLE.1);

pub static INFO: LazyLock<PluginInfo> = LazyLock::new(|| {
    PluginInfo::new("com.opengrid.openmultiband", "OpenMultiband", "OpenGrid", env!("CARGO_PKG_VERSION"), Category::Dynamics, &TABLE.0)
        .with_url("https://github.com/MistaMin/OpenMultiband")
        .with_editor(true)
});

static LAYOUTS: &[IoConfig] = &[
    IoConfig::STEREO,
    IoConfig { input: SpeakerLayout::Stereo, output: SpeakerLayout::Stereo, sidechain: Some(SpeakerLayout::Stereo) },
    IoConfig { input: SpeakerLayout::Stereo, output: SpeakerLayout::Stereo, sidechain: Some(SpeakerLayout::Mono) },
];

/// Max latency any band's 4-section chain can add: four 10 ms look-aheads + the 1.5 ms
/// decompressor ceiling, 0.5 ms for their rounding, + the true-peak FIR's fixed samples.
fn max_latency_samples(sr: f32) -> usize {
    (42.0 * 1e-3 * sr).ceil() as usize + dyn_decompress::TP_DELAY
}
/// Reported tail: worst-case crossover (all splits at 20 Hz, 48 dB/oct, 6 bands) rings ~490 ms to -90 dBFS.
const TAIL_MS: f32 = 600.0;
const FADE_MS: f32 = 10.0;
/// Fade out and back in around topology switches (Zero Latency, band count, slope).
const DIP_MS: f32 = 5.0;
const DELTA_GLIDE_MS: f32 = 50.0;

#[derive(Default)]
struct Delay {
    buf: Vec<f32>,
    pos: usize,
}

impl Delay {
    fn alloc(&mut self, len: usize) {
        self.buf = vec![0.0; len.max(1)];
        self.pos = 0;
    }

    fn clear(&mut self) {
        self.buf.fill(0.0);
    }

    #[inline]
    fn push(&mut self, x: f32, d: usize) -> f32 {
        let n = self.buf.len();
        if n == 0 || d == 0 {
            return x;
        }
        self.buf[self.pos] = x;
        let r = self.buf[(self.pos + n - d.min(n - 1)) % n];
        self.pos = (self.pos + 1) % n;
        r
    }
}

struct Section {
    module: Box<dyn Module>,
    on: bool,
    fade: f32,
    dry: [Delay; 2],
}

impl Section {
    fn new(module: Box<dyn Module>) -> Self {
        Self { module, on: false, fade: 0.0, dry: Default::default() }
    }
}

struct BandState {
    sections: [Section; BAND_SECTIONS],
    gain_db: f32,
    gain: f32,
    gain_target: f32,
    solo: bool,
    mute: bool,
    bypass: bool,
    /// 0 = active, 1 = bypassed (10 ms crossfade).
    bypass_fade: f32,
    /// Smoothed audibility (0 = muted/non-soloed, 1 = audible).
    aud: f32,
    /// Delay to align this band's latency with the longest active band.
    align: [Delay; 2],
    /// Clean crossover band delayed by `max_band_lat` for click-free Band Bypass.
    dry_band: [Delay; 2],
}

impl BandState {
    fn new() -> Self {
        let sections: [Section; BAND_SECTIONS] = band_modules().into_iter().map(Section::new).collect::<Vec<_>>().try_into().ok().expect("4 sections");
        Self {
            sections,
            gain_db: 0.0,
            gain: 1.0,
            gain_target: 1.0,
            solo: false,
            mute: false,
            bypass: false,
            bypass_fade: 0.0,
            aud: 1.0,
            align: Default::default(),
            dry_band: Default::default(),
        }
    }

    fn latency(&self) -> usize {
        self.sections.iter().map(|s| s.module.latency()).sum()
    }
}

pub struct OpenMultiband {
    values: Vec<f32>,
    crossover: Crossover,
    bands: [BandState; MAX_BANDS],
    limiter: Section,
    in_gain: f32,
    in_gain_target: f32,
    out_gain: f32,
    out_gain_target: f32,
    smooth: f32,
    fade_step: f32,
    sample_rate: f32,
    max_block: usize,
    // Pre-allocated scratch buffers (zero allocation on audio thread).
    band_l: [Vec<f32>; MAX_BANDS],
    band_r: [Vec<f32>; MAX_BANDS],
    band_dry_l: Vec<f32>,
    band_dry_r: Vec<f32>,
    sec_dry: [Vec<f32>; 2],
    ref_raw: [Vec<f32>; 2],
    pre: [Vec<f32>; 2],
    pre_delay: [Delay; 2],
    sum: [Vec<f32>; 2],
    mono: Vec<f32>,
    view: [f32; 16],
    shared: Arc<Shared>,
    // Topology state (dips 5 ms when changed live).
    zero_want: bool,
    zero: bool,
    bands_want: usize,
    bands_active: usize,
    slope_want: Slope,
    slope_active: Slope,
    dip: f32,
    dip_step: f32,
    autogain: autogain::AutoGain,
    delta: bool,
    delta_mix: f32,
    delta_step: f32,
    key_hold: usize,
}

fn log_line(msg: &str) {
    // Cargo sets CARGO_MANIFEST_DIR for integration tests too; hosts never do.
    if cfg!(test) || std::env::var_os("CARGO_MANIFEST_DIR").is_some() {
        return;
    }
    let path = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Library/Logs/OpenMultiband/panic.log"))
    } else {
        ui::theme::pref_file("panic.log")
    };
    if let Some(path) = path {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            use std::io::Write;
            let _ = writeln!(f, "{} {msg}", env!("CARGO_PKG_VERSION"));
        }
    }
}

fn log_panics() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            log_line(&format!("{info}"));
            prev(info);
        }));
    });
}

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn lin_to_db(v: f32) -> f32 {
    if v > 1e-9 { 20.0 * v.log10() } else { -200.0 }
}

impl Default for OpenMultiband {
    fn default() -> Self {
        log_panics();
        let values: Vec<f32> = INFO.params.iter().map(|p| p.default).collect();
        let bands: [BandState; MAX_BANDS] = std::array::from_fn(|_| BandState::new());
        let mut p = Self {
            values: values.clone(),
            crossover: Crossover::default(),
            bands,
            limiter: Section::new(Box::new(dynamics::Limiter::default())),
            in_gain: 1.0,
            in_gain_target: 1.0,
            out_gain: 1.0,
            out_gain_target: 1.0,
            smooth: 1.0,
            fade_step: 1.0,
            sample_rate: 48_000.0,
            max_block: 0,
            band_l: std::array::from_fn(|_| Vec::new()),
            band_r: std::array::from_fn(|_| Vec::new()),
            band_dry_l: Vec::new(),
            band_dry_r: Vec::new(),
            sec_dry: Default::default(),
            ref_raw: Default::default(),
            pre: Default::default(),
            pre_delay: Default::default(),
            sum: Default::default(),
            mono: Vec::new(),
            view: [0.0; 16],
            shared: Arc::new(Shared::default()),
            zero_want: false,
            zero: false,
            bands_want: 4,
            bands_active: 4,
            slope_want: Slope::Db24,
            slope_active: Slope::Db24,
            dip: 1.0,
            dip_step: 1.0,
            autogain: autogain::AutoGain::new(48_000.0, false),
            delta: false,
            delta_mix: 0.0,
            delta_step: 1.0,
            key_hold: 0,
        };
        for (i, &v) in values.iter().enumerate() {
            p.set_param_index(i, v);
        }
        p.apply_topology();
        for b in &mut p.bands {
            b.bypass_fade = if b.bypass { 1.0 } else { 0.0 };
            for s in &mut b.sections {
                s.fade = if s.on { 1.0 } else { 0.0 };
            }
        }
        p.limiter.fade = if p.limiter.on { 1.0 } else { 0.0 };
        p
    }
}

impl OpenMultiband {
    /// Lands every glide and every measured state on its target, so a pass after prepare or
    /// reset starts the same each time (offline bounces must match).
    fn snap_glides(&mut self) {
        self.delta_mix = if self.delta { 1.0 } else { 0.0 };
        let active = self.bands_active;
        let any_solo = (0..active).any(|b| self.bands[b].solo);
        for b in 0..MAX_BANDS {
            self.bands[b].aud = if any_solo {
                if self.bands[b].solo { 1.0 } else { 0.0 }
            } else if self.bands[b].mute {
                0.0
            } else {
                1.0
            };
        }
        self.dip = 1.0;
        self.autogain = autogain::AutoGain::new(self.sample_rate, self.values[P_AUTO_GAIN] >= 0.5);
        self.in_gain = self.in_gain_target;
        self.out_gain = self.out_gain_target;
        self.key_hold = 0;
    }

    fn topology_pending(&self) -> bool {
        self.zero != self.zero_want || self.bands_active != self.bands_want || self.slope_active != self.slope_want
    }

    fn apply_topology(&mut self) {
        self.zero = self.zero_want;
        self.bands_active = self.bands_want;
        self.slope_active = self.slope_want;
        self.crossover.set_topology(self.bands_active, self.slope_active);
        for b in &mut self.bands {
            for s in &mut b.sections {
                s.module.set_zero_latency(self.zero);
                s.module.reset();
                s.dry[0].clear();
                s.dry[1].clear();
            }
            b.align[0].clear();
            b.align[1].clear();
            b.dry_band[0].clear();
            b.dry_band[1].clear();
        }
        self.limiter.module.set_zero_latency(self.zero);
        self.limiter.module.reset();
        self.limiter.dry[0].clear();
        self.limiter.dry[1].clear();
        self.pre_delay[0].clear();
        self.pre_delay[1].clear();
    }

    fn max_band_latency(&self) -> usize {
        (0..self.bands_active).map(|b| self.bands[b].latency()).max().unwrap_or(0)
    }

    fn total_latency(&self) -> usize {
        self.max_band_latency() + self.limiter.module.latency()
    }

    fn run(&mut self, left: &mut [f32], right: &mut [f32], key: Option<[&[f32]; 2]>) {
        let n = left.len();
        if self.dip <= 0.0 && self.topology_pending() {
            self.apply_topology();
        }
        // ST-2: non-finite input never enters crossover, dynamics or Auto Gain state.
        for v in left.iter_mut().chain(right.iter_mut()) {
            if !v.is_finite() {
                *v = 0.0;
            }
        }
        self.autogain.feed_raw(left, right);

        let mut peak_in = [0.0f32; 2];
        for i in 0..n {
            self.in_gain += (self.in_gain_target - self.in_gain) * self.smooth;
            left[i] *= self.in_gain;
            right[i] *= self.in_gain;
            peak_in[0] = peak_in[0].max(left[i].abs());
            peak_in[1] = peak_in[1].max(right[i].abs());
        }

        // Split into 1..=6 bands plus matched all-pass phase reference.
        let [ref_l, ref_r] = &mut self.ref_raw;
        self.crossover.split_block(left, right, &mut self.band_l, &mut self.band_r, &mut ref_l[..n], &mut ref_r[..n]);

        let max_band_lat = self.max_band_latency();
        let total_lat = max_band_lat + self.limiter.module.latency();
        for i in 0..n {
            self.pre[0][i] = self.pre_delay[0].push(self.ref_raw[0][i], total_lat);
            self.pre[1][i] = self.pre_delay[1].push(self.ref_raw[1][i], total_lat);
        }

        self.sum[0][..n].fill(0.0);
        self.sum[1][..n].fill(0.0);

        let active = self.bands_active;
        let any_solo = (0..active).any(|b| self.bands[b].solo);
        let step = self.fade_step;
        let smooth = self.smooth;

        for b in 0..MAX_BANDS {
            if b >= active {
                for si in 0..BAND_SECTIONS {
                    self.shared.band_section(b, si, [0.0; 2], [-200.0; 2], false);
                }
                self.shared.band_summary(b, 0.0, -200.0);
                continue;
            }
            let band_lat = self.bands[b].latency();
            let align_d = max_band_lat.saturating_sub(band_lat);
            let bl = &mut self.band_l[b][..n];
            let br = &mut self.band_r[b][..n];
            self.band_dry_l[..n].copy_from_slice(bl);
            self.band_dry_r[..n].copy_from_slice(br);

            let band = &mut self.bands[b];
            let aud_target = if any_solo {
                if band.solo { 1.0 } else { 0.0 }
            } else if band.mute {
                0.0
            } else {
                1.0
            };

            let mut net_gain_db = 0.0f32;
            let skip_chain = band.bypass && band.bypass_fade >= 1.0;
            if skip_chain {
                // Fully bypassed: pass clean crossover band through max_band_lat alignment only.
                for si in 0..BAND_SECTIONS {
                    self.shared.band_section(b, si, [0.0; 2], [-200.0; 2], false);
                }
            } else {
                if !band.bypass && band.bypass_fade >= 1.0 {
                    for s in &mut band.sections {
                        s.module.reset();
                    }
                    band.align[0].clear();
                    band.align[1].clear();
                }
                for si in BAND_CHAIN {
                    let s = &mut band.sections[si];
                    let lat = s.module.latency();
                    if !s.on && s.fade <= 0.0 {
                        for i in 0..n {
                            bl[i] = s.dry[0].push(bl[i], lat);
                            br[i] = s.dry[1].push(br[i], lat);
                        }
                        self.shared.band_section(b, si, [0.0; 2], [-200.0; 2], false);
                        continue;
                    }
                    if s.on && s.fade <= 0.0 {
                        s.module.reset();
                    }
                    self.sec_dry[0][..n].copy_from_slice(bl);
                    self.sec_dry[1][..n].copy_from_slice(br);
                    match key {
                        Some([kl, kr]) => s.module.process_keyed(bl, br, kl, kr),
                        None => s.module.process_keyed(bl, br, &self.sec_dry[0][..n], &self.sec_dry[1][..n]),
                    }
                    let target = if s.on { 1.0 } else { 0.0 };
                    for i in 0..n {
                        let dl = s.dry[0].push(self.sec_dry[0][i], lat);
                        let dr = s.dry[1].push(self.sec_dry[1][i], lat);
                        if s.fade != target {
                            s.fade = if s.fade < target { (s.fade + step).min(1.0) } else { (s.fade - step).max(0.0) };
                        }
                        if s.fade < 1.0 {
                            bl[i] = dl + (bl[i] - dl) * s.fade;
                            br[i] = dr + (br[i] - dr) * s.fade;
                        }
                    }
                    let got = s.module.view(&mut self.view);
                    let lift = s.module.lift_db();
                    let red = s.module.meters().reduction_db;
                    let (g, d) = if got >= 4 {
                        ([self.view[0], self.view[1]], [self.view[2], self.view[3]])
                    } else {
                        ([lift - red; 2], [-200.0; 2])
                    };
                    let dom = if g[0].abs() >= g[1].abs() { g[0] } else { g[1] };
                    net_gain_db += dom * s.fade;
                    self.shared.band_section(b, si, g, d, true);
                }
            }

            let byp_target = if band.bypass { 1.0 } else { 0.0 };
            let mut band_peak = 0.0f32;
            for i in 0..n {
                let dry_l = band.dry_band[0].push(self.band_dry_l[i], max_band_lat);
                let dry_r = band.dry_band[1].push(self.band_dry_r[i], max_band_lat);
                band.gain += (band.gain_target - band.gain) * smooth;
                band.aud += (aud_target - band.aud) * smooth;
                if band.bypass_fade != byp_target {
                    band.bypass_fade = if band.bypass_fade < byp_target {
                        (band.bypass_fade + step).min(1.0)
                    } else {
                        (band.bypass_fade - step).max(0.0)
                    };
                }
                let (wl, wr) = if skip_chain {
                    (dry_l, dry_r)
                } else {
                    let al = band.align[0].push(bl[i], align_d) * band.gain;
                    let ar = band.align[1].push(br[i], align_d) * band.gain;
                    if band.bypass_fade > 0.0 {
                        (al + (dry_l - al) * band.bypass_fade, ar + (dry_r - ar) * band.bypass_fade)
                    } else {
                        (al, ar)
                    }
                };
                let ol = wl * band.aud;
                let or = wr * band.aud;
                band_peak = band_peak.max(ol.abs()).max(or.abs());
                self.sum[0][i] += ol;
                self.sum[1][i] += or;
            }
            self.shared.band_summary(b, if skip_chain { 0.0 } else { net_gain_db }, lin_to_db(band_peak));
        }

        // Copy summed bands back to left/right and run the global broadband Master Limiter.
        left.copy_from_slice(&self.sum[0][..n]);
        right.copy_from_slice(&self.sum[1][..n]);

        {
            let s = &mut self.limiter;
            let lat = s.module.latency();
            if !s.on && s.fade <= 0.0 {
                for i in 0..n {
                    left[i] = s.dry[0].push(left[i], lat);
                    right[i] = s.dry[1].push(right[i], lat);
                }
                self.shared.limiter([0.0; 2], [-200.0; 2], false);
            } else {
                if s.on && s.fade <= 0.0 {
                    s.module.reset();
                }
                self.sec_dry[0][..n].copy_from_slice(left);
                self.sec_dry[1][..n].copy_from_slice(right);
                s.module.process_keyed(left, right, &self.sec_dry[0][..n], &self.sec_dry[1][..n]);
                let target = if s.on { 1.0 } else { 0.0 };
                for i in 0..n {
                    let dl = s.dry[0].push(self.sec_dry[0][i], lat);
                    let dr = s.dry[1].push(self.sec_dry[1][i], lat);
                    if s.fade != target {
                        s.fade = if s.fade < target { (s.fade + step).min(1.0) } else { (s.fade - step).max(0.0) };
                    }
                    if s.fade < 1.0 {
                        left[i] = dl + (left[i] - dl) * s.fade;
                        right[i] = dr + (right[i] - dr) * s.fade;
                    }
                }
                let got = s.module.view(&mut self.view);
                let red = s.module.meters().reduction_db;
                let (g, d) = if got >= 4 { ([self.view[0], self.view[1]], [self.view[2], self.view[3]]) } else { ([-red; 2], [-200.0; 2]) };
                self.shared.limiter(g, d, true);
            }
        }

        // Spectrum taps (aligned phase-reference input vs post-dynamics output before output gain).
        self.shared.push_taps([&self.pre[0][..n], &self.pre[1][..n]], [left, right]);

        let mut peak_out = [0.0f32; 2];
        let dip_to = if self.topology_pending() { 0.0 } else { 1.0 };
        let delta_to = if self.delta { 1.0 } else { 0.0 };

        // Apply Output Gain & Zero-Latency/Topology Dip; feed AutoGain before Delta.
        for i in 0..n {
            self.out_gain += (self.out_gain_target - self.out_gain) * smooth;
            if self.dip != dip_to {
                self.dip = if self.dip < dip_to { (self.dip + self.dip_step).min(1.0) } else { (self.dip - self.dip_step).max(0.0) };
            }
            let g = self.out_gain * self.dip;
            left[i] *= g;
            right[i] *= g;
            peak_out[0] = peak_out[0].max(left[i].abs());
            peak_out[1] = peak_out[1].max(right[i].abs());
            // Store g in sum[0] temporarily so Delta scales pre identically sample-by-sample.
            self.sum[0][i] = g;
        }
        self.shared.peaks(peak_in, peak_out);
        self.autogain.feed_out(left, right);

        if self.delta_mix != 0.0 || delta_to != 0.0 {
            for i in 0..n {
                if self.delta_mix != delta_to {
                    self.delta_mix = if self.delta_mix < delta_to {
                        (self.delta_mix + self.delta_step).min(1.0)
                    } else {
                        (self.delta_mix - self.delta_step).max(0.0)
                    };
                }
                let m = self.delta_mix * self.sum[0][i];
                left[i] -= m * self.pre[0][i];
                right[i] -= m * self.pre[1][i];
            }
        }
        self.autogain.apply(left, right);
        self.shared.set_auto_gain(self.autogain.gain_db());
        // ST-3: f64 crossover and delay paths can leave subnormals; hosts without FTZ must not see them.
        for v in left.iter_mut().chain(right.iter_mut()) {
            if v.abs() < 1e-30 {
                *v = 0.0;
            }
        }
    }
}

impl Plugin for OpenMultiband {
    fn state_is_parameters_only(&self) -> bool {
        true
    }

    fn info(&self) -> &'static PluginInfo {
        &INFO
    }

    fn prepare(&mut self, sample_rate: f32, max_block: usize, _channels: usize) {
        self.sample_rate = sample_rate.max(1.0);
        self.max_block = max_block.max(1);
        let dry_len = max_latency_samples(self.sample_rate) + 2;
        self.crossover.prepare(self.sample_rate);
        for b in &mut self.bands {
            for s in &mut b.sections {
                s.module.prepare(self.sample_rate, self.max_block);
                s.dry[0].alloc(dry_len);
                s.dry[1].alloc(dry_len);
                s.fade = if s.on { 1.0 } else { 0.0 };
            }
            b.align[0].alloc(dry_len);
            b.align[1].alloc(dry_len);
            b.dry_band[0].alloc(dry_len);
            b.dry_band[1].alloc(dry_len);
            b.gain = b.gain_target;
            b.bypass_fade = if b.bypass { 1.0 } else { 0.0 };
        }
        self.limiter.module.prepare(self.sample_rate, self.max_block);
        self.limiter.dry[0].alloc(dry_len);
        self.limiter.dry[1].alloc(dry_len);
        self.limiter.fade = if self.limiter.on { 1.0 } else { 0.0 };

        for b in 0..MAX_BANDS {
            self.band_l[b] = vec![0.0; self.max_block];
            self.band_r[b] = vec![0.0; self.max_block];
        }
        self.band_dry_l = vec![0.0; self.max_block];
        self.band_dry_r = vec![0.0; self.max_block];
        self.sec_dry = [vec![0.0; self.max_block], vec![0.0; self.max_block]];
        self.ref_raw = [vec![0.0; self.max_block], vec![0.0; self.max_block]];
        self.pre = [vec![0.0; self.max_block], vec![0.0; self.max_block]];
        self.pre_delay[0].alloc(dry_len);
        self.pre_delay[1].alloc(dry_len);
        self.sum = [vec![0.0; self.max_block], vec![0.0; self.max_block]];
        self.mono = vec![0.0; self.max_block];

        self.smooth = 1.0 - (-1.0 / (0.005 * self.sample_rate)).exp();
        self.fade_step = 1.0 / (FADE_MS * 1e-3 * self.sample_rate);
        self.dip_step = 1.0 / (DIP_MS * 1e-3 * self.sample_rate);
        self.delta_step = 1.0 / (DELTA_GLIDE_MS * 1e-3 * self.sample_rate);
        if self.topology_pending() {
            self.apply_topology();
        }
        self.snap_glides();
        self.shared.set_sample_rate(self.sample_rate);
    }

    fn reset(&mut self) {
        self.crossover.reset();
        for b in &mut self.bands {
            for s in &mut b.sections {
                s.module.reset();
                s.dry[0].clear();
                s.dry[1].clear();
                s.fade = if s.on { 1.0 } else { 0.0 };
            }
            b.align[0].clear();
            b.align[1].clear();
            b.dry_band[0].clear();
            b.dry_band[1].clear();
            b.gain = b.gain_target;
            b.bypass_fade = if b.bypass { 1.0 } else { 0.0 };
        }
        self.limiter.module.reset();
        self.limiter.dry[0].clear();
        self.limiter.dry[1].clear();
        self.limiter.fade = if self.limiter.on { 1.0 } else { 0.0 };
        self.pre_delay[0].clear();
        self.pre_delay[1].clear();
        self.snap_glides();
    }

    fn set_param(&mut self, id: &str, value: f32) -> bool {
        match INFO.params.iter().position(|p| p.id == id) {
            Some(i) => self.set_param_index(i, value),
            None => false,
        }
    }

    fn set_param_index(&mut self, index: usize, value: f32) -> bool {
        let Some(info) = INFO.params.get(index) else { return false };
        let value = info.clamp(value);
        self.values[index] = value;
        match index {
            P_INPUT => {
                self.in_gain_target = db_to_gain(value);
                return true;
            }
            P_OUTPUT => {
                self.out_gain_target = db_to_gain(value);
                return true;
            }
            P_ZERO_LATENCY => {
                self.zero_want = value >= 0.5;
                if self.max_block == 0 {
                    self.apply_topology();
                }
                return true;
            }
            P_AUTO_GAIN => {
                self.autogain.on = value >= 0.5;
                return true;
            }
            P_DELTA => {
                self.delta = value >= 0.5;
                return true;
            }
            P_BAND_COUNT => {
                self.bands_want = (value.round() as usize + 1).clamp(1, MAX_BANDS);
                if self.max_block == 0 {
                    self.apply_topology();
                }
                return true;
            }
            P_XOVER_SLOPE => {
                self.slope_want = Slope::from_param(value);
                if self.max_block == 0 {
                    self.apply_topology();
                }
                return true;
            }
            i if (P_XOVER_BASE..P_XOVER_BASE + MAX_SPLITS).contains(&i) => {
                self.crossover.set_freq(i - P_XOVER_BASE, value);
                return true;
            }
            _ => {}
        }

        let lim = &LAYOUT.limiter;
        if index == lim.on {
            self.limiter.on = value >= 0.5;
            return true;
        }
        if (lim.base..lim.base + lim.count).contains(&index) {
            self.limiter.module.set_param(index - lim.base, value);
            return true;
        }

        for (b, bl) in self.bands.iter_mut().zip(LAYOUT.bands.iter()) {
            if index == bl.gain {
                b.gain_db = value;
                b.gain_target = db_to_gain(value);
                return true;
            }
            if index == bl.solo {
                b.solo = value >= 0.5;
                return true;
            }
            if index == bl.mute {
                b.mute = value >= 0.5;
                return true;
            }
            if index == bl.bypass {
                b.bypass = value >= 0.5;
                return true;
            }
            for (s, sl) in b.sections.iter_mut().zip(bl.sections.iter()) {
                if index == sl.on {
                    s.on = value >= 0.5;
                    return true;
                }
                if (sl.base..sl.base + sl.count).contains(&index) {
                    s.module.set_param(index - sl.base, value);
                    return true;
                }
            }
        }
        false
    }

    fn param(&self, id: &str) -> Option<f32> {
        INFO.params.iter().position(|p| p.id == id).map(|i| self.values[i])
    }

    fn set_offline(&mut self, offline: bool) {
        for b in &mut self.bands {
            for s in &mut b.sections {
                s.module.set_offline(offline);
            }
        }
        self.limiter.module.set_offline(offline);
    }

    fn supported_layouts(&self) -> &'static [IoConfig] {
        LAYOUTS
    }

    fn process(&mut self, io: &mut [&mut [f32]], frames: usize) {
        self.process_buses(io, None, frames);
    }

    fn process_buses(&mut self, io: &mut [&mut [f32]], sidechain: Option<&[&[f32]]>, frames: usize) {
        if self.max_block == 0 {
            return;
        }
        let key: Option<[&[f32]; 2]> = sidechain.and_then(|side| match side {
            [kl, kr, ..] if kl.len() >= frames && kr.len() >= frames => Some([&kl[..frames], &kr[..frames]]),
            [k] if k.len() >= frames => Some([&k[..frames], &k[..frames]]),
            _ => None,
        });
        let heard = key.is_some_and(|[a, b]| a.iter().chain(b.iter()).any(|v| v.abs() > 1e-5));
        self.key_hold = if heard { self.sample_rate as usize } else { self.key_hold.saturating_sub(frames) };
        self.shared.set_key_present(self.key_hold > 0);

        let Some((first, rest)) = io.split_first_mut() else { return };
        // Take the scratch only after the empty-io return, or that return would drop it.
        let mut mono = std::mem::take(&mut self.mono);
        let mut start = 0;
        while start < frames {
            let mut n = (frames - start).min(self.max_block);
            if self.topology_pending() && self.dip > 0.0 {
                // End the chunk where the 5 ms dip hits silence so switches complete inside large blocks.
                n = n.min(((self.dip / self.dip_step).ceil() as usize).max(1));
            }
            let k = key.map(|[a, b]| [&a[start..start + n], &b[start..start + n]]);
            let l = &mut first[start..start + n];
            match rest.first_mut() {
                Some(r) => self.run(l, &mut r[start..start + n], k),
                None => {
                    let r = &mut mono[..n];
                    r.copy_from_slice(l);
                    self.run(l, r, k);
                }
            }
            start += n;
        }
        self.mono = mono;
    }

    fn latency(&self) -> usize {
        self.total_latency()
    }

    fn tail(&self) -> openplugin::core::Tail {
        openplugin::core::Tail::Finite((TAIL_MS * 1e-3 * self.sample_rate).ceil() as u32)
    }

    fn max_latency(&self) -> Option<usize> {
        Some(max_latency_samples(self.sample_rate))
    }

    fn meters(&self) -> MeterFrame {
        let mut m = MeterFrame::SILENT;
        let band_gr = (0..self.bands_active)
            .filter(|&b| !self.bands[b].bypass && !self.bands[b].mute)
            .flat_map(|b| self.bands[b].sections.iter().filter(|s| s.on).map(|s| s.module.meters().reduction_db))
            .fold(0.0f32, f32::max);
        let lim_gr = if self.limiter.on { self.limiter.module.meters().reduction_db } else { 0.0 };
        m.reduction_db = band_gr.max(lim_gr);
        m
    }

    fn editor_state(&self) -> Option<EditorState> {
        Some(self.shared.clone())
    }

    fn note_ports(&self) -> NotePorts {
        NotePorts { input: false, output: false }
    }

    fn lv2_uri(&self) -> Option<&'static str> {
        Some("https://opengrid.studio/plugins/openmultiband")
    }

    fn aax_ids(&self) -> Option<openplugin::core::AaxIds> {
        Some(openplugin::core::AaxIds { manufacturer: *b"OpGd", product: *b"OMbd", mono: *b"OMbM", stereo: *b"OMbS" })
    }
}

openplugin::targets::export_clap!(OpenMultiband, editor = make_editor);
openplugin::targets::export_lv2!(OpenMultiband, editor = make_editor);
openplugin::targets::export_shell!(OpenMultiband, editor = make_editor);

#[cfg(test)]
mod tests {
    use super::*;

    fn block(p: &mut OpenMultiband, l: &mut [f32], r: &mut [f32]) {
        let n = l.len();
        let mut io: [&mut [f32]; 2] = [l, r];
        p.process(&mut io, n);
    }

    fn disable_all_dynamics(p: &mut OpenMultiband) {
        p.set_param_index(LAYOUT.limiter.on, 0.0);
        for b in &LAYOUT.bands {
            for s in &b.sections {
                p.set_param_index(s.on, 0.0);
            }
        }
    }

    #[test]
    fn params_are_unique_and_layout_is_complete() {
        let ids: std::collections::HashSet<_> = INFO.params.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), INFO.params.len());
        // AAX truncates parameter display names to 31 chars: keep every name unique within 31 chars.
        let mut names31 = std::collections::HashSet::new();
        for p in INFO.params {
            let short: String = p.name.chars().take(31).collect();
            assert!(names31.insert(short.clone()), "duplicate 31-char parameter name: {short:?} ({})", p.id);
        }
        let last = &LAYOUT.bands[MAX_BANDS - 1].sections[BAND_SECTIONS - 1];
        assert_eq!(last.base + last.count, INFO.params.len());
    }

    #[test]
    fn dynamics_controls_move_in_whole_steps() {
        for p in INFO.params.iter().filter(|p| matches!(p.unit, Unit::Db | Unit::Ratio)) {
            assert!(p.steps > 1, "{} is not stepped", p.id);
            assert_eq!(p.default, p.default.round(), "{} default {}", p.id, p.default);
            for v in [p.min + 0.37, (p.min + p.max) * 0.5 + 0.21, p.max - 0.4] {
                let c = p.clamp(v);
                assert!((c - c.round()).abs() < 1e-4, "{}: {v} -> {c}", p.id);
            }
        }
    }

    #[test]
    fn band_chain_order_is_decomp_first() {
        let mut seen = BAND_CHAIN;
        seen.sort_unstable();
        assert_eq!(seen, [0, 1, 2, 3]);
        assert_eq!(LAYOUT.bands[0].sections[BAND_CHAIN[0]].key, "decomp");
        assert_eq!(LAYOUT.bands[0].sections[BAND_CHAIN[1]].key, "gate");
        assert_eq!(LAYOUT.bands[0].sections[BAND_CHAIN[2]].key, "comp");
        assert_eq!(LAYOUT.bands[0].sections[BAND_CHAIN[3]].key, "deexp");
    }

    #[test]
    fn single_band_all_off_is_a_wire() {
        let mut p = OpenMultiband::default();
        p.set_param("bandCount", 0.0); // 1 band
        disable_all_dynamics(&mut p);
        p.prepare(48_000.0, 256, 2);
        let n = 2048;
        let src: Vec<f32> = (0..n).map(|i| ((i as f32) * 0.1).sin() * 0.5).collect();
        let (mut l, mut r) = (src.clone(), src.clone());
        block(&mut p, &mut l, &mut r);
        let lat = p.latency();
        assert!(lat > 0 && lat < 1000, "{lat}");
        assert_eq!(&l[lat..], &src[..n - lat]);
    }

    /// With different look-ahead latencies across bands, per-band alignment keeps the band sum
    /// flat and exact against the phase-matched dry reference.
    #[test]
    fn band_latency_alignment_sums_flat() {
        let mut p = OpenMultiband::default();
        disable_all_dynamics(&mut p);
        // Set different lookahead on Band 1 comp (10 ms) vs Band 2 comp (0 ms) vs Band 3 (5 ms).
        p.set_param("b1.comp.lookahead", 10.0);
        p.set_param("b2.comp.lookahead", 0.0);
        p.set_param("b3.comp.lookahead", 5.0);
        p.set_param("b4.comp.lookahead", 2.0);
        p.set_param("delta", 1.0);
        p.prepare(48_000.0, 256, 2);
        let n = 4096;
        let src: Vec<f32> = (0..n).map(|i| (i as f32 * 0.04).sin() * 0.4 + (i as f32 * 0.31).sin() * 0.3).collect();
        let (mut l, mut r) = (src.clone(), src.clone());
        block(&mut p, &mut l, &mut r);
        let peak_err = l[1024..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak_err < 1e-5, "misaligned band sum left delta residual {peak_err}");
    }

    #[test]
    fn delta_nulls_at_unity_for_all_band_counts_and_slopes() {
        for slope in 0..3 {
            for bands_idx in 0..MAX_BANDS {
                let mut p = OpenMultiband::default();
                disable_all_dynamics(&mut p);
                p.set_param("bandCount", bands_idx as f32);
                p.set_param("xoverSlope", slope as f32);
                p.set_param("delta", 1.0);
                p.prepare(48_000.0, 256, 2);
                let n = 4096;
                let src: Vec<f32> = (0..n)
                    .map(|i| 0.3 * (i as f32 * 0.015).sin() + 0.3 * (i as f32 * 0.12).sin() + 0.2 * (i as f32 * 0.7).sin())
                    .collect();
                let (mut l, mut r) = (src.clone(), src.clone());
                block(&mut p, &mut l, &mut r);
                let peak = l[1024..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
                assert!(peak < 1e-5, "bands={} slope={slope}: unity delta residual {peak}", bands_idx + 1);
            }
        }
    }

    #[test]
    fn solo_mute_bypass_and_band_gain_act_independently() {
        let sr = 48_000.0;
        let run_sine = |freq: f32, setup: &dyn Fn(&mut OpenMultiband)| -> f32 {
            let mut p = OpenMultiband::default();
            disable_all_dynamics(&mut p);
            setup(&mut p);
            p.prepare(sr, 256, 2);
            let n = 8192;
            let mut l: Vec<f32> = (0..n).map(|i| 0.4 * (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin()).collect();
            let mut r = l.clone();
            block(&mut p, &mut l, &mut r);
            (l[n / 2..].iter().map(|v| v * v).sum::<f32>() / (n / 2) as f32).sqrt()
        };

        // Solo Band 1 (20..120 Hz): 60 Hz passes, 2 kHz is silenced.
        let low_solo = run_sine(60.0, &|p| {
            p.set_param("b1.solo", 1.0);
        });
        let hi_solo = run_sine(2000.0, &|p| {
            p.set_param("b1.solo", 1.0);
        });
        assert!(low_solo > 0.25 && hi_solo < 0.005, "solo: low={low_solo} hi={hi_solo}");

        // Mute Band 1 (20..120 Hz LR4): 60 Hz is 1 octave below 120 Hz, so Band 2 HP passes
        // 1/17 (-24.6 dB) = 0.0166 RMS while 2 kHz passes at full level.
        let low_mute = run_sine(60.0, &|p| {
            p.set_param("b1.mute", 1.0);
        });
        let hi_mute = run_sine(2000.0, &|p| {
            p.set_param("b1.mute", 1.0);
        });
        assert!(low_mute < 0.02 && hi_mute > 0.25, "mute: low={low_mute} hi={hi_mute}");

        // Band 4 (3k..20k) gain +6 dB lifts 12 kHz (2 octaves above 3 kHz) by exact +6 dB (~1.995x)
        // without changing 60 Hz; Band 3 (750..3k) gain +6 dB lifts 1500 Hz by ~1.88x (LR4 sum of
        // 15/17 in-band + 2/17 adjacent skirts).
        let base_hi = run_sine(12_000.0, &|_| {});
        let boosted_hi = run_sine(12_000.0, &|p| {
            p.set_param("b4.gain", 6.0);
        });
        let base_mid = run_sine(1500.0, &|_| {});
        let boosted_mid = run_sine(1500.0, &|p| {
            p.set_param("b3.gain", 6.0);
        });
        let boosted_low = run_sine(60.0, &|p| {
            p.set_param("b3.gain", 6.0);
        });
        let base_low = run_sine(60.0, &|_| {});
        assert!((boosted_hi / base_hi - db_to_gain(6.0)).abs() < 0.02, "hi ratio {}", boosted_hi / base_hi);
        assert!((boosted_mid / base_mid - 1.88).abs() < 0.03, "mid ratio {}", boosted_mid / base_mid);
        assert!((boosted_low / base_low - 1.0).abs() < 0.02, "low ratio {}", boosted_low / base_low);

        // Band Bypass bypasses Band 1 compressor and gain.
        let compressed = run_sine(60.0, &|p| {
            p.set_param("b1.comp.on", 1.0);
            p.set_param("b1.comp.threshold", -30.0);
            p.set_param("b1.comp.ratio", 10.0);
        });
        let bypassed = run_sine(60.0, &|p| {
            p.set_param("b1.comp.on", 1.0);
            p.set_param("b1.comp.threshold", -30.0);
            p.set_param("b1.comp.ratio", 10.0);
            p.set_param("b1.bypass", 1.0);
        });
        assert!(compressed < base_low * 0.5 && (bypassed / base_low - 1.0).abs() < 0.02, "comp={compressed} byp={bypassed} base={base_low}");
    }

    #[test]
    fn master_limiter_clamps_post_sum_peaks() {
        let mut p = OpenMultiband::default();
        disable_all_dynamics(&mut p);
        p.set_param("b1.gain", 12.0);
        p.set_param("b3.gain", 12.0);
        p.set_param("limit.on", 1.0);
        p.set_param("limit.ceiling", -3.0);
        p.set_param("limit.attack", 0.0);
        p.prepare(48_000.0, 256, 2);
        let n = 4096;
        let mut l: Vec<f32> = (0..n).map(|i| 0.4 * (i as f32 * 0.01).sin() + 0.4 * (i as f32 * 0.2).sin()).collect();
        let mut r = l.clone();
        block(&mut p, &mut l, &mut r);
        let peak = l[512..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let ceil = db_to_gain(-3.0);
        assert!(peak <= ceil + 0.01, "limiter peak {peak} exceeded ceiling {ceil}");
    }

    #[test]
    fn zero_latency_reports_and_is_zero() {
        let mut p = OpenMultiband::default();
        p.prepare(48_000.0, 256, 2);
        assert!(p.latency() > 0);
        p.set_param("zeroLatency", 1.0);
        for _ in 0..4 {
            block(&mut p, &mut vec![0.0; 256], &mut vec![0.0; 256]);
        }
        assert_eq!(p.latency(), 0);
        // Back to lookahead.
        p.set_param("zeroLatency", 0.0);
        for _ in 0..4 {
            block(&mut p, &mut vec![0.0; 256], &mut vec![0.0; 256]);
        }
        assert!(p.latency() > 0);
    }

    #[test]
    fn zero_latency_and_band_count_switches_do_not_click() {
        let mut p = OpenMultiband::default();
        p.prepare(48_000.0, 64, 2);
        let mut prev = 0.0f32;
        let mut worst = 0.0f32;
        for b in 0..300 {
            if b == 80 {
                p.set_param("zeroLatency", 1.0);
            }
            if b == 160 {
                p.set_param("bandCount", 5.0); // 6 bands
            }
            let mut l: Vec<f32> = (0..64).map(|i| ((b * 64 + i) as f32 * 0.02).sin() * 0.4).collect();
            let mut r = l.clone();
            block(&mut p, &mut l, &mut r);
            for v in l {
                worst = worst.max((v - prev).abs());
                prev = v;
            }
        }
        assert_eq!(p.latency(), 0);
        assert_eq!(p.bands_active, 6);
        assert!(worst < 0.03, "click step {worst}");
    }

    #[test]
    fn external_sidechain_triggers_key_indicator_and_band_compression() {
        let mut p = OpenMultiband::default();
        disable_all_dynamics(&mut p);
        p.set_param("b1.comp.on", 1.0);
        p.set_param("b1.comp.threshold", -30.0);
        p.set_param("b1.comp.ratio", 10.0);
        p.set_param("b1.comp.keySource", 1.0); // External
        p.prepare(48_000.0, 256, 2);
        let n = 4096;
        let quiet: Vec<f32> = (0..n).map(|i| 0.02 * (i as f32 * 0.01).sin()).collect();
        let loud_key: Vec<f32> = (0..n).map(|i| 0.8 * (i as f32 * 0.05).sin()).collect();
        let (mut l_no_key, mut r_no_key) = (quiet.clone(), quiet.clone());
        block(&mut p, &mut l_no_key, &mut r_no_key);
        let rms_no_key: f32 = (l_no_key[n / 2..].iter().map(|v| v * v).sum::<f32>() / (n / 2) as f32).sqrt();

        p.reset();
        let (mut l_key, mut r_key) = (quiet.clone(), quiet.clone());
        let mut io: [&mut [f32]; 2] = [&mut l_key, &mut r_key];
        let sc: [&[f32]; 2] = [&loud_key, &loud_key];
        p.process_buses(&mut io, Some(&sc), n);
        let rms_key: f32 = (l_key[n / 2..].iter().map(|v| v * v).sum::<f32>() / (n / 2) as f32).sqrt();
        assert!(p.shared.key_present());
        assert!(rms_key < rms_no_key * 0.5, "external key should duck band: no_key={rms_no_key} key={rms_key}");
    }

    #[test]
    fn bad_sidechain_samples_do_not_freeze_a_band() {
        let mut p = OpenMultiband::default();
        disable_all_dynamics(&mut p);
        p.set_param("b1.comp.on", 1.0);
        p.set_param("b1.comp.threshold", -30.0);
        p.set_param("b1.comp.ratio", 10.0);
        p.set_param("b1.comp.keySource", 1.0); // External
        p.set_param("b1.comp.keyHighPass", 100.0); // a key filter whose state NaN would poison
        p.prepare(48_000.0, 256, 2);
        let n = 4096;
        let quiet: Vec<f32> = (0..n).map(|i| 0.02 * (i as f32 * 0.01).sin()).collect();
        let loud_key: Vec<f32> = (0..n).map(|i| 0.8 * (i as f32 * 0.05).sin()).collect();
        let (mut l, mut r) = (quiet.clone(), quiet.clone());
        block(&mut p, &mut l, &mut r);
        let rms_no_key = (l[n / 2..].iter().map(|v| v * v).sum::<f32>() / (n / 2) as f32).sqrt();

        let mut bad_key = loud_key.clone();
        bad_key[10] = f32::NAN;
        bad_key[20] = f32::INFINITY;
        for key in [&bad_key, &loud_key, &loud_key] {
            let (mut l, mut r) = (quiet.clone(), quiet.clone());
            let mut io: [&mut [f32]; 2] = [&mut l, &mut r];
            let sc: [&[f32]; 2] = [key, key];
            p.process_buses(&mut io, Some(&sc), n);
            assert!(l.iter().chain(&r).all(|v| v.is_finite()));
            let rms = (l[n / 2..].iter().map(|v| v * v).sum::<f32>() / (n / 2) as f32).sqrt();
            assert!(rms > 1e-4, "band silenced: {rms}");
            assert!(rms < rms_no_key * 0.5, "band stopped compressing: no_key={rms_no_key} key={rms}");
        }
    }

    #[test]
    fn a_pass_after_reset_matches_a_fresh_instance_with_auto_gain() {
        let fresh = || {
            let mut p = OpenMultiband::default();
            p.set_param("autoGain", 1.0);
            p.set_param("b1.comp.makeup", 12.0);
            p.prepare(48_000.0, 512, 2);
            p
        };
        let pass = |p: &mut OpenMultiband| {
            p.reset();
            let mut l: Vec<f32> = (0..4096).map(|i| 0.0005 * (i as f32 * 0.01).sin()).collect();
            let mut r = l.clone();
            for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
                block(p, a, b);
            }
            l
        };
        let mut played = fresh();
        // Let Auto Gain settle far from unity on loud material.
        for b in 0..200 {
            let mut l: Vec<f32> = (0..512).map(|i| 0.3 * ((b * 512 + i) as f32 * 0.01).sin()).collect();
            let mut r = l.clone();
            block(&mut played, &mut l, &mut r);
        }
        assert_eq!(pass(&mut played), pass(&mut fresh()), "a bounce after reset must not depend on what played before");
    }

    #[test]
    fn mono_after_an_empty_call_works() {
        let mut p = OpenMultiband::default();
        p.prepare(48_000.0, 256, 1);
        p.process(&mut [], 0);
        let mut m = vec![0.1f32; 256];
        p.process(&mut [&mut m[..]], 256);
        assert!(m.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn spectrum_taps_line_up_at_unity() {
        let mut p = OpenMultiband::default();
        disable_all_dynamics(&mut p);
        p.prepare(48_000.0, 512, 2);
        for b in 0..8 {
            let mut l: Vec<f32> = (0..512).map(|i| ((b * 512 + i) as f32 * 0.03).sin() * 0.5).collect();
            let mut r = l.clone();
            block(&mut p, &mut l, &mut r);
        }
        let (mut pre, mut post) = (vec![0.0f32; 2048], vec![0.0f32; 2048]);
        p.shared.read_tap(0, &mut pre);
        p.shared.read_tap(2, &mut post);
        assert!(post.iter().any(|v| *v != 0.0));
        let max_err = pre.iter().zip(&post).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(max_err < 1e-5, "tap alignment error {max_err}");
    }

    /// Writes docs/PARAMETERS.md for the manual: Global, Crossover, Master Limiter, and the
    /// per-band parameter layout (identical across Bands 1 to 6, 592 host parameters total).
    #[test]
    #[ignore]
    fn dump_manual_params() {
        let total = INFO.params.len();
        let n_global = INFO.params.iter().filter(|p| p.group == "Global").count();
        let n_xover = INFO.params.iter().filter(|p| p.group == "Crossover").count();
        let n_lim = INFO.params.iter().filter(|p| p.group == "Master Limiter").count();
        let n_band1 = INFO.params.iter().filter(|p| p.group == "Band 1" || p.group.starts_with("Band 1 ")).count();
        let mut out = format!(
            "# OpenMultiband parameter reference\n\n\
             Generated from the plugin (`cargo test --release --lib dump_manual_params -- --ignored`). \
             OpenMultiband exposes **{total} host parameters**: {n_global} Global, {n_xover} Crossover, {n_lim} Master Limiter, \
             and {n_band1} per band across {MAX_BANDS} bands (4 band strip controls plus 4 dynamics sections per band). \
             Bands 2 through {MAX_BANDS} use the exact same parameter names, ranges, and defaults as Band 1 \
             (`Band 2`..`Band {MAX_BANDS}` and `B2`..`B{MAX_BANDS}`).\n",
        );
        let mut group = "";
        for p in INFO.params.iter() {
            if p.group.starts_with("Band ") && !p.group.starts_with("Band 1") {
                continue;
            }
            if p.group != group {
                group = p.group;
                let heading = if group == "Band 1" {
                    "Per-band controls (Band 1 shown; identical for Bands 2 to 6)".to_string()
                } else if let Some(rest) = group.strip_prefix("Band 1 ") {
                    format!("Per-band {rest} (Band 1 shown; identical for Bands 2 to 6)")
                } else {
                    group.to_string()
                };
                out += &format!("\n## {heading}\n\n| Parameter | Range or choices | Default |\n|---|---|---|\n");
            }
            let (range, default) = match p.unit {
                Unit::Toggle => ("Off, On".to_string(), if p.default >= 0.5 { "On" } else { "Off" }.to_string()),
                Unit::Choice => (p.choices.join(", "), p.choices.get(p.default as usize).copied().unwrap_or("").to_string()),
                _ => (format!("{} to {}", p.format(p.min), p.format(p.max)), p.format(p.default)),
            };
            out += &format!("| {} | {range} | {default} |\n", p.name);
        }
        std::fs::create_dir_all(concat!(env!("CARGO_MANIFEST_DIR"), "/docs")).expect("docs dir");
        std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/PARAMETERS.md"), out).expect("write");
    }

    /// DSP cost on 64-sample blocks (4 bands default vs 6 bands all sections active).
    #[test]
    #[ignore]
    fn dsp_profile() {
        for (name, bands, all_on) in [("4-band default (comps+lim)", 4, false), ("6-band all sections ON", 6, true)] {
            let mut p = OpenMultiband::default();
            p.set_param("bandCount", (bands - 1) as f32);
            if all_on {
                for b in &LAYOUT.bands {
                    for s in &b.sections {
                        p.set_param_index(s.on, 1.0);
                    }
                }
            }
            p.prepare(48_000.0, 64, 2);
            let mut l = vec![0.0f32; 64];
            let mut r = vec![0.0f32; 64];
            let blocks = 4000;
            let t0 = std::time::Instant::now();
            for b in 0..blocks {
                for i in 0..64 {
                    let s = ((b * 64 + i) as f32 * 0.04).sin() * 0.35;
                    l[i] = s;
                    r[i] = s * 0.9;
                }
                block(&mut p, &mut l, &mut r);
            }
            let us = t0.elapsed().as_secs_f64() * 1e6 / blocks as f64;
            let budget = 64.0 / 48_000.0 * 1e6;
            println!("{name:28}: {us:6.1} us/block ({:4.1}% of one core)", 100.0 * us / budget);
        }
    }
}
