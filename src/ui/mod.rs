// Copyright (c) 2026 Marcos Quirino Deida
//! OpenMultiband editor: interactive 1-to-6-band crossover spectrum overview on top,
//! band & section tabs in the middle, live curve + 4 s history (or 3D spectrum terrain)
//! and big main knobs below, with IN/OUT meters on the rails.
//!
//! Follows the GoodLookin DRI performance and readability rules:
//! * Static chrome in the cached layer, live traces as short-piece curves & column fills.
//! * Moving gain/auto-gain readouts in whole dB (never `-0`), all text >= 8 pt.
//! * Every parameter (all 6 bands x 4 sections + global master limiter + crossover + globals)
//!   bound to a real SDK `Control` (drag, wheel, double-click type, host menu, accessibility).

#![allow(dead_code)]

pub mod analyser;
pub mod kit;
pub mod raster;
pub mod theme;
mod view3d;

use std::collections::VecDeque;
use std::sync::Arc;

use openplugin::core::editor::{
    AccessAction, AccessNode, AccessRange, AccessRole, DamageRect, DamageTracker, Key, Modifiers, ParamMenu, ParamMenuAction, SizeRange, TextEdit, Tooltip,
};
use openplugin::core::{Editor, EditorHost, EditorState, Frame};
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};
use openplugin::dri::{ControlSet, SceneCanvas};

use crate::crossover::{FREQ_MAX, FREQ_MIN};
use crate::shared::{BandSummary, Live, Shared};
use crate::{
    BAND_CHAIN, BAND_SECTIONS, INFO, LAYOUT, MAX_BANDS, MAX_SPLITS, P_AUTO_GAIN, P_BAND_COUNT, P_DELTA, P_INPUT, P_OUTPUT, P_XOVER_BASE, P_XOVER_SLOPE, P_ZERO_LATENCY,
};
use kit::{inside, rect, Cell, Kind, Size};
use raster::R;
use theme::{load_theme, store_theme, theme, Theme, ThemeId};

pub const WIDTH: u32 = 1080;
pub const HEIGHT: u32 = 768;

// Header controls (y = 14..38)
const BANDS_CTRL: R = (204.0, 14.0, 138.0, 24.0);
const SLOPE_CTRL: R = (394.0, 14.0, 144.0, 24.0);
const DELTA_RECT: R = (562.0, 15.0, 76.0, 22.0);
const AUTO_GAIN_RECT: R = (646.0, 15.0, 114.0, 22.0);
const ZERO_LATENCY_RECT: R = (768.0, 15.0, 118.0, 22.0);
const VIEW3D_BUTTON: R = (896.0, 15.0, 90.0, 22.0);
const THEME_BUTTON: R = (994.0, 15.0, 72.0, 22.0);

// Rails & center page
const LEFT_RAIL: R = (8.0, 56.0, 84.0, 704.0);
const RIGHT_RAIL: R = (988.0, 56.0, 84.0, 704.0);
const PAGE: R = (100.0, 56.0, 880.0, 704.0);

// Top multiband spectrum & crossover plot
const MB_PLOT: R = (140.0, 66.0, 804.0, 154.0);
const MB_LO: f32 = -72.0;
const MB_HI: f32 = 6.0;
const MB_SPEC_BINS: usize = 96;
const MB_SPEC_SIZE: usize = 4096;

// Band selector row + Link button + Master Limiter tab
const BAND_ROW_Y: f32 = 242.0;
const BAND_ROW_H: f32 = 28.0;
const BANDS_BAR_X: f32 = 112.0;
const BANDS_BAR_W: f32 = 630.0;
const LINK_BUTTON: R = (750.0, BAND_ROW_Y, 72.0, BAND_ROW_H);
const LIMITER_TAB: R = (830.0, BAND_ROW_Y, 138.0, BAND_ROW_H);

// Section tabs + selected band controls row
const SEC_ROW_Y: f32 = 278.0;
const SEC_ROW_H: f32 = 26.0;
const SEC_TAB_LABELS: [&str; BAND_SECTIONS] = ["GATE / EXP", "COMPRESSOR", "DE-EXPANDER", "DECOMPRESSOR"];
const BAND_SOLO_RECT: R = (632.0, 280.0, 56.0, 22.0);
const BAND_MUTE_RECT: R = (694.0, 280.0, 56.0, 22.0);
const BAND_BYP_RECT: R = (756.0, 280.0, 52.0, 22.0);
const BAND_GAIN_RECT: R = (856.0, 280.0, 88.0, 22.0);

// Chip + readout row
const CHIP_Y: f32 = 312.0;
const CHIP_H: f32 = 22.0;
const CHIP_W: f32 = 94.0;
const SEC_ON_RECT: R = (112.0, CHIP_Y, 54.0, CHIP_H);

// Main dynamics display (2D curve+history, or 3D terrain)
pub const DISPLAY: R = (140.0, 342.0, 804.0, 264.0);
const LO: f32 = -60.0;
const HI: f32 = 6.0;
const GR_DB: f32 = 24.0;
const GR_SHARE: f32 = 0.3;
const HIST_HZ: f32 = 30.0;
const HISTORY_S: usize = 4;
const TRACE_LEN: usize = HISTORY_S * HIST_HZ as usize;

// Main knobs row
const KNOB_Y: f32 = 626.0;
const KNOB_STEP: f32 = 136.0;

// Meter rails
const METER_TOP: f32 = 68.0;
const METER_BOTTOM: f32 = 636.0;
const M_LO: f32 = -60.0;
const M_HI: f32 = 6.0;

// Collapsible settings panels (over DISPLAY)
const PANEL_ROW: f32 = 30.0;
const PANEL_W_1COL: f32 = 410.0;
const PANEL_W_2COL: f32 = 640.0;

/// What the bottom dynamics section is editing: a band's section, or the post-sum Master Limiter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Band(usize),
    Limiter,
}

#[derive(Clone, Copy)]
enum Item {
    Knob(usize, &'static str),
    Keys(usize, &'static str, f32),
    Pill(usize, &'static str),
}

struct Page {
    main: &'static [Item],
    top: &'static [Item],
    panels: &'static [(&'static str, &'static [Item])],
}

/// Page definition for section `sec` (`0..4` = band sections, `4` = master limiter).
fn page(sec: usize) -> Page {
    use Item::*;
    match sec {
        0 => {
            use dynamics::gate::index as g;
            Page {
                main: &[Knob(g::THRESHOLD, "Threshold"), Knob(g::RATIO, "Ratio"), Knob(g::RANGE, "Range"), Knob(g::ATTACK, "Attack"), Knob(g::HOLD, "Hold"), Knob(g::RELEASE, "Release")],
                top: &[Keys(g::MODE, "Mode", 140.0)],
                panels: &[
                    ("SHAPE", &[Knob(g::KNEE, "Knee"), Knob(g::HYSTERESIS, "Hysteresis"), Knob(g::STEREO_LINK, "Link")]),
                    ("TIMING", &[Knob(g::LOOKAHEAD, "Look-Ahead"), Pill(g::AUTO_ATTACK, "Auto Attack"), Pill(g::AUTO_RELEASE, "Auto Release")]),
                    ("SIDECHAIN", &[Keys(g::DETECTOR, "Detector", 150.0), Knob(g::RMS_WINDOW, "RMS Window"), Keys(g::KEY_SOURCE, "Key", 160.0), Knob(g::KEY_HIGH_PASS, "Key HP"), Knob(g::KEY_LOW_PASS, "Key LP"), Knob(g::KEY_BOOST, "Key Boost"), Knob(g::KEY_BOOST_FREQ, "Boost Freq"), Knob(g::KEY_BOOST_Q, "Boost Q")]),
                ],
            }
        }
        1 => {
            use dynamics::comp::index as c;
            Page {
                main: &[Knob(c::THRESHOLD, "Threshold"), Knob(c::RATIO, "Ratio"), Knob(c::ATTACK, "Attack"), Knob(c::RELEASE, "Release"), Knob(c::MAKEUP, "Make-Up"), Knob(c::MIX, "Mix")],
                top: &[],
                panels: &[
                    ("SHAPE", &[Knob(c::KNEE, "Knee"), Knob(c::STEREO_LINK, "Link")]),
                    ("TIMING", &[Knob(c::LOOKAHEAD, "Look-Ahead"), Pill(c::AUTO_ATTACK, "Auto Attack"), Pill(c::AUTO_RELEASE, "Auto Release")]),
                    ("SIDECHAIN", &[Keys(c::DETECTOR, "Detector", 150.0), Knob(c::RMS_WINDOW, "RMS Window"), Keys(c::KEY_SOURCE, "Key", 160.0), Knob(c::KEY_HIGH_PASS, "Key HP"), Knob(c::KEY_LOW_PASS, "Key LP"), Knob(c::KEY_BOOST, "Key Boost"), Knob(c::KEY_BOOST_FREQ, "Boost Freq"), Knob(c::KEY_BOOST_Q, "Boost Q")]),
                ],
            }
        }
        2 => {
            use dyn_deexpand::index as d;
            Page {
                main: &[Knob(d::THRESHOLD, "Threshold"), Knob(d::RATIO, "Ratio"), Knob(d::RANGE, "Range"), Knob(d::ATTACK, "Attack"), Knob(d::RELEASE, "Release"), Knob(d::MIX, "Mix")],
                top: &[],
                panels: &[
                    ("SHAPE", &[Knob(d::KNEE, "Knee"), Knob(d::FLOOR, "Floor"), Knob(d::STEREO_LINK, "Link")]),
                    ("TIMING", &[Knob(d::HOLD, "Hold"), Knob(d::LOOKAHEAD, "Look-Ahead"), Pill(d::AUTO_ATTACK, "Auto Attack"), Pill(d::AUTO_RELEASE, "Auto Release")]),
                    ("SIDECHAIN", &[Keys(d::DETECTOR, "Detector", 150.0), Knob(d::DETECTOR_WINDOW, "RMS Window"), Keys(d::KEY_SOURCE, "Key", 160.0), Knob(d::KEY_HIGH_PASS, "Key HP"), Knob(d::KEY_LOW_PASS, "Key LP"), Knob(d::KEY_BOOST, "Key Boost"), Knob(d::KEY_BOOST_FREQ, "Boost Freq"), Knob(d::KEY_BOOST_Q, "Boost Q")]),
                ],
            }
        }
        3 => {
            use dyn_decompress::index as d;
            Page {
                main: &[Knob(d::THRESHOLD, "Threshold"), Knob(d::RATIO, "Ratio"), Knob(d::RANGE, "Range"), Knob(d::ATTACK, "Attack"), Knob(d::RELEASE, "Release"), Knob(d::MIX, "Mix")],
                top: &[],
                panels: &[
                    ("SHAPE", &[Knob(d::KNEE, "Knee"), Knob(d::TRIM, "Trim"), Knob(d::STEREO_LINK, "Link")]),
                    ("TIMING", &[Knob(d::LOOKAHEAD, "Look-Ahead"), Pill(d::AUTO_ATTACK, "Auto Attack"), Pill(d::AUTO_RELEASE, "Auto Release")]),
                    ("SIDECHAIN", &[Keys(d::DETECTOR, "Detector", 196.0), Knob(d::TRANSIENT_CREST, "Crest"), Knob(d::DETECTOR_WINDOW, "RMS Window"), Keys(d::KEY_SOURCE, "Key", 160.0), Knob(d::KEY_HIGH_PASS, "Key HP"), Knob(d::KEY_LOW_PASS, "Key LP"), Knob(d::KEY_BOOST, "Key Boost"), Knob(d::KEY_BOOST_FREQ, "Boost Freq"), Knob(d::KEY_BOOST_Q, "Boost Q")]),
                    ("CEILING", &[Pill(d::CEILING_ON, "Ceiling"), Pill(d::MATCH, "Match"), Knob(d::CEILING, "Ceiling"), Keys(d::CEILING_TYPE, "Type", 200.0), Knob(d::MATCH_TARGET, "Match Crest")]),
                ],
            }
        }
        _ => {
            use dynamics::limiter::index as l;
            Page {
                main: &[Knob(l::THRESHOLD, "Threshold"), Knob(l::CEILING, "Ceiling"), Knob(l::ATTACK, "Attack"), Knob(l::RELEASE, "Release")],
                top: &[Pill(l::CLEAN, "Clean")],
                panels: &[
                    ("SHAPE", &[Knob(l::STEREO_LINK, "Link")]),
                    ("TIMING", &[Knob(l::BASS_HOLD, "Bass Hold"), Pill(l::AUTO_ATTACK, "Auto Attack"), Pill(l::AUTO_RELEASE, "Auto Release")]),
                ],
            }
        }
    }
}

fn key_boost(sec: usize) -> Option<(usize, usize, usize)> {
    match sec {
        0 => Some((dynamics::gate::index::KEY_BOOST_FREQ, dynamics::gate::index::KEY_BOOST, dynamics::gate::index::KEY_BOOST_Q)),
        1 => Some((dynamics::comp::index::KEY_BOOST_FREQ, dynamics::comp::index::KEY_BOOST, dynamics::comp::index::KEY_BOOST_Q)),
        2 => Some((dyn_deexpand::index::KEY_BOOST_FREQ, dyn_deexpand::index::KEY_BOOST, dyn_deexpand::index::KEY_BOOST_Q)),
        3 => Some((dyn_decompress::index::KEY_BOOST_FREQ, dyn_decompress::index::KEY_BOOST, dyn_decompress::index::KEY_BOOST_Q)),
        _ => None,
    }
}

fn dimmed(sec: usize, local: usize, v: &dyn Fn(usize) -> f32) -> bool {
    if let Some((f, g, q)) = key_boost(sec) {
        if (local == f || local == q) && v(g).abs() < 1e-3 {
            return true;
        }
    }
    match sec {
        0 => {
            use dynamics::gate::index as g;
            let gate = v(g::MODE) < 0.5;
            (gate && (local == g::RATIO || local == g::KNEE)) || (local == g::RMS_WINDOW && v(g::DETECTOR) < 0.5)
        }
        1 => local == dynamics::comp::index::RMS_WINDOW && v(dynamics::comp::index::DETECTOR) < 0.5,
        2 => local == dyn_deexpand::index::DETECTOR_WINDOW && v(dyn_deexpand::index::DETECTOR) < 0.5,
        3 => {
            use dyn_decompress::index as d;
            (local == d::TRANSIENT_CREST && v(d::DETECTOR) < 1.5)
                || ((local == d::CEILING || local == d::CEILING_TYPE) && v(d::CEILING_ON) < 0.5)
                || (local == d::MATCH_TARGET && v(d::MATCH) < 0.5)
                || (local == d::RANGE && v(d::MATCH) >= 0.5)
        }
        4 => {
            use dynamics::limiter::index as l;
            let clean = v(l::CLEAN) >= 0.5;
            (local == l::BASS_HOLD && !clean) || ((local == l::RELEASE || local == l::AUTO_RELEASE) && clean)
        }
        _ => false,
    }
}

fn zero_latency_dims(sec: usize, local: usize) -> bool {
    match sec {
        0 => local == dynamics::gate::index::LOOKAHEAD,
        1 => local == dynamics::comp::index::LOOKAHEAD,
        2 => local == dyn_deexpand::index::LOOKAHEAD,
        3 => local == dyn_decompress::index::LOOKAHEAD || local == dyn_decompress::index::CEILING_TYPE,
        _ => false,
    }
}

fn lifts(sec: usize) -> bool {
    sec == 2 || sec == 3
}

fn whole_db(v: f32) -> String {
    let r = v.round();
    if r == 0.0 { "0".into() } else { format!("{r:+.0}") }
}

fn upward_ratio(v: f32) -> String {
    format!("1:{:.0}", v.round().max(1.0))
}

fn short_freq(hz: f32) -> String {
    if hz >= 1000.0 {
        let k = hz / 1000.0;
        if k >= 10.0 { format!("{k:.0}k") } else { format!("{k:.1}k") }
    } else {
        format!("{hz:.0}")
    }
}

/// Matching parameter indices on the other active bands for any per-band module control or Band Gain.
fn linked_peers(param: usize, n_bands: usize) -> Option<Vec<usize>> {
    for b in 0..n_bands {
        let bl = &LAYOUT.bands[b];
        if param == bl.gain {
            return Some((0..n_bands).filter(|&o| o != b).map(|o| LAYOUT.bands[o].gain).collect());
        }
        for si in 0..BAND_SECTIONS {
            let s = &bl.sections[si];
            if param == s.on {
                return Some((0..n_bands).filter(|&o| o != b).map(|o| LAYOUT.bands[o].sections[si].on).collect());
            }
            if param >= s.base && param < s.base + s.count {
                let local = param - s.base;
                return Some((0..n_bands).filter(|&o| o != b).map(|o| LAYOUT.bands[o].sections[si].base + local).collect());
            }
        }
    }
    None
}

/// Switches/choices match across bands; continuous controls move relatively in normalized taper space.
fn linked_value(info: &openplugin::core::ParamInfo, peer_start: f32, src_start: f32, src_new: f32) -> f32 {
    use openplugin::core::Unit;
    if matches!(info.unit, Unit::Toggle | Unit::Choice) || !info.choices.is_empty() || (peer_start - src_start).abs() < 1e-6 {
        info.clamp(src_new)
    } else {
        let d = info.to_normalized(src_new) - info.to_normalized(src_start);
        info.from_normalized((info.to_normalized(peer_start) + d).clamp(0.0, 1.0))
    }
}

fn curve_out(sec: usize, l: f64, v: &dyn Fn(usize) -> f32) -> f64 {
    let p = |i: usize| v(i) as f64;
    let mix_db = |gain_db: f64, mix_pct: f64, wet_db: f64| {
        let m = mix_pct / 100.0;
        20.0 * ((1.0 - m) + m * 10f64.powf((gain_db + wet_db) / 20.0)).max(1e-9).log10()
    };
    match sec {
        0 => {
            use dynamics::gate::index as g;
            l + dynamics::gate::target(p(g::MODE) < 0.5, l, p(g::THRESHOLD), p(g::RATIO), p(g::RANGE), p(g::KNEE))
        }
        1 => {
            use dynamics::comp::index as c;
            let g = dynamics::curve::compressor(l, p(c::THRESHOLD), p(c::RATIO), p(c::KNEE));
            l + mix_db(g, p(c::MIX), p(c::MAKEUP))
        }
        2 => {
            use dyn_deexpand::index as d;
            let g = dyn_deexpand::static_gain_db(l, p(d::THRESHOLD), p(d::RATIO), p(d::KNEE), p(d::RANGE), p(d::FLOOR));
            l + mix_db(g, p(d::MIX), 0.0)
        }
        3 => {
            use dyn_decompress::index as d;
            let g = dyn_decompress::static_gain_db(l, p(d::THRESHOLD), 1.0 / p(d::RATIO).max(1.0), p(d::KNEE), p(d::RANGE));
            let mut out = l + mix_db(g, p(d::MIX), 0.0) + p(d::TRIM);
            if p(d::CEILING_ON) >= 0.5 {
                out = out.min(p(d::CEILING));
            }
            out
        }
        _ => {
            use dynamics::limiter::index as li;
            (l + p(li::CEILING) - p(li::THRESHOLD)).min(p(li::CEILING))
        }
    }
}

fn threshold(sec: usize, v: &dyn Fn(usize) -> f32) -> f32 {
    match sec {
        0 => v(dynamics::gate::index::THRESHOLD),
        1 => v(dynamics::comp::index::THRESHOLD),
        2 => v(dyn_deexpand::index::THRESHOLD),
        3 => v(dyn_decompress::index::THRESHOLD),
        _ => v(dynamics::limiter::index::THRESHOLD),
    }
}

fn band_tab_rect(b: usize, count: usize) -> R {
    let gap = 6.0;
    let w = (BANDS_BAR_W - gap * (count.saturating_sub(1)) as f32) / count.max(1) as f32;
    (BANDS_BAR_X + b as f32 * (w + gap), BAND_ROW_Y, w, BAND_ROW_H)
}

fn sec_tab_rect(sec: usize) -> R {
    let pos = BAND_CHAIN.iter().position(|&s| s == sec).unwrap_or(sec);
    (112.0 + pos as f32 * 128.0, SEC_ROW_Y, 122.0, SEC_ROW_H)
}

fn sec_dot(sec: usize) -> (f32, f32) {
    let r = sec_tab_rect(sec);
    (r.0 + 13.0, r.1 + r.3 * 0.5)
}

fn sec_dot_hit(sec: usize) -> R {
    let (x, y) = sec_dot(sec);
    (x - 10.0, y - 10.0, 20.0, 20.0)
}

fn limiter_dot() -> (f32, f32) {
    (LIMITER_TAB.0 + 14.0, LIMITER_TAB.1 + LIMITER_TAB.3 * 0.5)
}

fn limiter_dot_hit() -> R {
    let (x, y) = limiter_dot();
    (x - 10.0, y - 10.0, 20.0, 20.0)
}

fn chips_start_x(sec: usize) -> f32 {
    match sec {
        0 => 322.0, // after ON + Gate/Expander Mode
        4 => 258.0, // after ON + Clean
        _ => 174.0,
    }
}

fn chip_rect(sec: usize, i: usize) -> R {
    (chips_start_x(sec) + i as f32 * (CHIP_W + 6.0), CHIP_Y, CHIP_W, CHIP_H)
}

fn panel_rows(items: &[Item]) -> usize {
    let mut n = 0;
    let mut prev_pill = false;
    for it in items {
        let pill = matches!(it, Item::Pill(..));
        if !(pill && prev_pill) {
            n += 1;
        }
        prev_pill = pill;
    }
    n
}

fn panel_uses_two_cols(items: &[Item]) -> bool {
    panel_rows(items) > 5
}

fn panel_rect(sec: usize, p: usize) -> R {
    let items = page(sec).panels[p].1;
    let two_col = panel_uses_two_cols(items);
    let w = if two_col { PANEL_W_2COL } else { PANEL_W_1COL };
    let rows = if two_col {
        let split = items.len() - 5; // right col always takes the 5 Key EQ controls (HP, LP, Boost, Freq, Q)
        panel_rows(&items[..split]).max(panel_rows(&items[split..]))
    } else {
        panel_rows(items)
    };
    let h = 34.0 + rows as f32 * PANEL_ROW + 8.0;
    let x = chip_rect(sec, p).0.clamp(DISPLAY.0 + 8.0, DISPLAY.0 + DISPLAY.2 - w - 8.0);
    (x, DISPLAY.1 + 8.0, w, h)
}

fn level_y(db: f32) -> f32 {
    let n = ((db - LO) / (HI - LO)).clamp(0.0, 1.0);
    DISPLAY.1 + DISPLAY.3 - n * DISPLAY.3
}

fn curve_x(db: f32) -> f32 {
    DISPLAY.0 + ((db - LO) / (HI - LO)).clamp(0.0, 1.0) * DISPLAY.3
}

fn gain_y(g: f32) -> f32 {
    DISPLAY.1 + (g.abs() / GR_DB).min(1.0) * DISPLAY.3 * GR_SHARE
}

fn mb_y(db: f32) -> f32 {
    let n = ((db - MB_LO) / (MB_HI - MB_LO)).clamp(0.0, 1.0);
    MB_PLOT.1 + MB_PLOT.3 - n * MB_PLOT.3
}

fn mb_freq_x(hz: f32) -> f32 {
    kit::freq_x(hz, MB_PLOT.0, MB_PLOT.0 + MB_PLOT.2, FREQ_MIN, FREQ_MAX)
}

fn meter_y(db: f32) -> f32 {
    let n = ((db - M_LO) / (M_HI - M_LO)).clamp(0.0, 1.0);
    METER_BOTTOM - n * (METER_BOTTOM - METER_TOP)
}

fn to_db(v: f32) -> f32 {
    if v > 1e-9 { 20.0 * v.log10() } else { -200.0 }
}

pub struct MultibandEditor {
    host: EditorHost,
    controls: ControlSet,
    cells: Vec<Cell>,
    panel_cells: usize,
    values: Vec<f32>,
    target: Target,
    /// Selected section index `0..4` when editing a band (defaults to Compressor = 1).
    sec_tab: usize,
    panel: Option<usize>,
    theme: ThemeId,
    shared: Option<Arc<Shared>>,
    /// Per-band per-section live readout + history traces.
    band_live: [[Live; BAND_SECTIONS]; MAX_BANDS],
    band_summary: [BandSummary; MAX_BANDS],
    band_trace: [[VecDeque<f32>; BAND_SECTIONS]; MAX_BANDS],
    band_level: [[VecDeque<f32>; BAND_SECTIONS]; MAX_BANDS],
    lim_live: Live,
    lim_trace: VecDeque<f32>,
    lim_level: VecDeque<f32>,
    meters: [[f32; 2]; 2],
    auto_gain_db: f32,
    mb_spec: [analyser::Spectrum; 2],
    taps: [Vec<f32>; 4],
    view3d: bool,
    terrain: view3d::Terrain,
    terrain_img: view3d::TerrainImage,
    hist_acc: f32,
    dragging: bool,
    xover_moved: bool,
    band_link: bool,
    link_drag: Option<(usize, f32, Vec<(usize, f32)>)>,
    modifiers: Modifiers,
    keys_active: bool,
    canvas: SceneCanvas,
    damage: DamageTracker,
}

pub fn make_editor(host: EditorHost) -> Option<Box<dyn Editor>> {
    Some(Box::new(MultibandEditor::new(host)))
}

impl MultibandEditor {
    pub fn new(host: EditorHost) -> Self {
        let mut e = Self {
            controls: ControlSet::new(host.clone()),
            host,
            cells: Vec::new(),
            panel_cells: 0,
            values: INFO.params.iter().map(|p| p.default).collect(),
            target: Target::Band(0),
            sec_tab: 1, // Compressor
            panel: None,
            theme: load_theme(),
            shared: None,
            band_live: [[Live::default(); BAND_SECTIONS]; MAX_BANDS],
            band_summary: [BandSummary::default(); MAX_BANDS],
            band_trace: std::array::from_fn(|_| std::array::from_fn(|_| VecDeque::from(vec![0.0; TRACE_LEN]))),
            band_level: std::array::from_fn(|_| std::array::from_fn(|_| VecDeque::from(vec![-200.0; TRACE_LEN]))),
            lim_live: Live::default(),
            lim_trace: VecDeque::from(vec![0.0; TRACE_LEN]),
            lim_level: VecDeque::from(vec![-200.0; TRACE_LEN]),
            meters: [[-200.0; 2]; 2],
            auto_gain_db: 0.0,
            mb_spec: std::array::from_fn(|_| analyser::Spectrum::new(MB_SPEC_SIZE, MB_SPEC_BINS, FREQ_MIN, FREQ_MAX)),
            taps: std::array::from_fn(|_| vec![0.0; view3d::SPEC3_SIZE]),
            view3d: false,
            terrain: view3d::Terrain::default(),
            terrain_img: view3d::TerrainImage::default(),
            hist_acc: 0.0,
            dragging: false,
            xover_moved: false,
            band_link: false,
            link_drag: None,
            modifiers: Modifiers::NONE,
            keys_active: true,
            canvas: SceneCanvas::new(WIDTH as f32, HEIGHT as f32),
            damage: DamageTracker::new(),
        };
        e.build();
        e
    }

    pub(super) fn t(&self) -> &'static Theme {
        theme(self.theme)
    }

    pub(super) fn active_bands(&self) -> usize {
        (self.values[P_BAND_COUNT].round() as usize + 1).clamp(1, MAX_BANDS)
    }

    /// Sorted active crossover frequencies (`active_bands - 1` entries).
    pub(super) fn sorted_xovers(&self) -> Vec<f32> {
        let splits = self.active_bands().saturating_sub(1);
        let mut f: Vec<f32> = (0..splits).map(|k| self.values[P_XOVER_BASE + k]).collect();
        f.sort_by(|a, b| a.total_cmp(b));
        f
    }

    /// Frequency span `[lo, hi]` Hz of active band `b`.
    fn band_bounds(&self, b: usize) -> (f32, f32) {
        let splits = self.sorted_xovers();
        let lo = if b == 0 { FREQ_MIN } else { splits.get(b - 1).copied().unwrap_or(FREQ_MIN) };
        let hi = splits.get(b).copied().unwrap_or(FREQ_MAX);
        (lo, hi.max(lo))
    }

    /// Section kind `0..4` (band section) or `4` (master limiter).
    fn active_sec_kind(&self) -> usize {
        match self.target {
            Target::Band(_) => self.sec_tab,
            Target::Limiter => 4,
        }
    }

    fn active_sec_info(&self) -> &'static crate::SectionInfo {
        match self.target {
            Target::Band(b) => &LAYOUT.bands[b.min(MAX_BANDS - 1)].sections[self.sec_tab],
            Target::Limiter => &LAYOUT.limiter,
        }
    }

    fn local(&self, local: usize) -> f32 {
        let base = self.active_sec_info().base;
        self.values.get(base + local).copied().unwrap_or(0.0)
    }

    fn section_cell(&self, cell: Cell) -> Cell {
        let sec = self.active_sec_kind();
        let base = self.active_sec_info().base;
        let local = cell.param - base;
        let v = |i: usize| self.local(i);
        let zero = self.values[P_ZERO_LATENCY] >= 0.5;
        let mut cell = cell.dim(dimmed(sec, local, &v) || (zero && zero_latency_dims(sec, local)));
        if sec == 3 && local == dyn_decompress::index::RATIO {
            cell = cell.format(upward_ratio);
        }
        cell
    }

    fn make_cells(&self) -> (Vec<Cell>, usize) {
        let sec_kind = self.active_sec_kind();
        let sec_info = self.active_sec_info();
        let pg = page(sec_kind);

        let mut cells = vec![
            Cell::knob(Size::Small, P_INPUT, "Input", 10.0, 662.0),
            Cell::knob(Size::Small, P_OUTPUT, "Output", 990.0, 662.0),
            Cell::segmented(P_BAND_COUNT, "Bands", BANDS_CTRL),
            Cell::segmented(P_XOVER_SLOPE, "Slope", SLOPE_CTRL),
            Cell::pill(P_DELTA, "Delta", DELTA_RECT),
            Cell::pill(P_AUTO_GAIN, "Auto Gain", AUTO_GAIN_RECT),
            Cell::pill(P_ZERO_LATENCY, "Zero Latency", ZERO_LATENCY_RECT),
            Cell::pill(sec_info.on, "On", SEC_ON_RECT),
        ];

        // Active crossover handles inside MB_PLOT (horizontal drag tracking 1:1 with log Hz).
        let splits = self.active_bands().saturating_sub(1);
        let mut prev_right = MB_PLOT.0;
        for k in 0..splits {
            let f = self.values[P_XOVER_BASE + k];
            let x = mb_freq_x(f).clamp(MB_PLOT.0 + 8.0, MB_PLOT.0 + MB_PLOT.2 - 8.0);
            let left = (x - 8.0).max(prev_right + 1.0).min(MB_PLOT.0 + MB_PLOT.2 - 16.0);
            prev_right = left + 16.0;
            cells.push(
                Cell::grip_h(P_XOVER_BASE + k, (left, MB_PLOT.1 + 2.0, 16.0, MB_PLOT.3 - 4.0))
                    .drag(MB_PLOT.2)
                    .tip(format!("Drag Crossover {} frequency (double-click to type Hz)", k + 1)),
            );
        }

        // Selected band controls (Solo, Mute, Bypass, Band Gain).
        if let Target::Band(b) = self.target {
            let bl = &LAYOUT.bands[b];
            cells.push(Cell::pill(bl.solo, "Solo", BAND_SOLO_RECT));
            cells.push(Cell::pill(bl.mute, "Mute", BAND_MUTE_RECT));
            cells.push(Cell::pill(bl.bypass, "Byp", BAND_BYP_RECT));
            cells.push(Cell::value(bl.gain, "Band Gain", BAND_GAIN_RECT));
        }

        // Section-specific top switch (Gate Mode or Limiter Clean).
        let mut x = 174.0;
        for it in pg.top {
            let (cell, w) = match *it {
                Item::Keys(i, label, w) => (Cell::segmented(sec_info.base + i, label, (x, CHIP_Y, w, CHIP_H)), w),
                Item::Pill(i, label) => (Cell::pill(sec_info.base + i, label, (x, CHIP_Y, 76.0, CHIP_H)), 76.0),
                Item::Knob(..) => continue,
            };
            cells.push(self.section_cell(cell));
            x += w + 8.0;
        }

        // Main big knobs centered across PAGE.
        let total = pg.main.len() as f32 * KNOB_STEP - (KNOB_STEP - Size::Big.cell().0);
        let x0 = PAGE.0 + (PAGE.2 - total) * 0.5;
        for (k, it) in pg.main.iter().enumerate() {
            if let Item::Knob(i, label) = *it {
                cells.push(self.section_cell(Cell::knob(Size::Big, sec_info.base + i, label, x0 + k as f32 * KNOB_STEP, KNOB_Y)));
            }
        }

        // Open settings panel controls (1-col or 2-col inside DISPLAY).
        let before = cells.len();
        if let Some(p) = self.panel {
            let (px, py, pw, _) = panel_rect(sec_kind, p);
            let items = pg.panels[p].1;
            let mut push_col = |col_items: &[Item], cx: f32, cw: f32| {
                let mut y = py + 34.0;
                let mut k = 0;
                while k < col_items.len() {
                    match col_items[k] {
                        Item::Knob(i, label) => {
                            cells.push(self.section_cell(Cell::new(Kind::FaderH, sec_info.base + i, label, (cx, y, cw, 24.0))));
                            k += 1;
                        }
                        Item::Keys(i, label, w) => {
                            let kw = w.min(cw - 78.0);
                            cells.push(self.section_cell(Cell::segmented(sec_info.base + i, label, (cx + 76.0, y, kw, 24.0))));
                            k += 1;
                        }
                        Item::Pill(..) => {
                            let mut px_pill = cx;
                            while let Some(Item::Pill(i, label)) = col_items.get(k) {
                                cells.push(self.section_cell(Cell::pill(sec_info.base + i, *label, (px_pill, y, 120.0, 24.0))));
                                px_pill += 128.0;
                                k += 1;
                            }
                        }
                    }
                    y += PANEL_ROW;
                }
            };
            if panel_uses_two_cols(items) {
                let split = items.len() - 5;
                let col_w = (pw - 48.0) * 0.5;
                push_col(&items[..split], px + 16.0, col_w);
                push_col(&items[split..], px + 32.0 + col_w, col_w);
            } else {
                push_col(items, px + 16.0, pw - 32.0);
            }
        }
        let n = cells.len() - before;
        (cells, n)
    }

    fn build(&mut self) {
        // Keep selected band inside active_bands.
        if let Target::Band(b) = self.target {
            if b >= self.active_bands() {
                self.target = Target::Band(self.active_bands() - 1);
            }
        }
        (self.cells, self.panel_cells) = self.make_cells();
        let focus = self.controls.focused();
        let mut set = ControlSet::new(self.host.clone());
        for c in &self.cells {
            set.add(kit::control_for(c));
        }
        set.set_modifiers(self.modifiers);
        for c in &self.cells {
            set.param_changed(c.param, self.values[c.param]);
        }
        set.set_focus(focus.filter(|&f| f < self.cells.len()));
        // Baseline initial state so the first interaction does not treat untouched controls as changed.
        let _ = set.take_changed();
        self.controls = set;
        self.damage.add_all();
    }

    fn sync(&mut self) {
        let changed = self.controls.take_changed();
        if changed.is_empty() {
            return;
        }
        let mut rebuild = false;
        let mut edits = Vec::new();
        for id in changed {
            if let Some(c) = self.controls.get(id) {
                let p = c.param;
                let v = c.value();
                let prev = self.values[p];
                if (v - prev).abs() > 0.0 {
                    if p == P_BAND_COUNT {
                        rebuild = true;
                    }
                    if (P_XOVER_BASE..P_XOVER_BASE + MAX_SPLITS).contains(&p) {
                        self.xover_moved = true;
                    }
                    self.values[p] = v;
                    edits.push((p, prev, v));
                }
            }
        }
        for (p, prev, v) in edits {
            self.propagate_link(p, prev, v);
        }
        if rebuild || (!self.dragging && self.dims_changed()) {
            self.build();
        }
        self.damage.add_all();
    }

    fn propagate_link(&mut self, p: usize, prev: f32, new_v: f32) {
        if !self.band_link {
            return;
        }
        let n_bands = self.active_bands();
        if n_bands <= 1 {
            return;
        }
        let Some(peers) = linked_peers(p, n_bands) else { return };
        let info = &INFO.params[p];
        if self.dragging {
            if self.link_drag.as_ref().is_some_and(|(src, _, _)| *src != p) {
                self.end_link_drag();
            }
            if self.link_drag.is_none() {
                let snap: Vec<(usize, f32)> = peers.iter().map(|&pp| (pp, self.values[pp])).collect();
                for &(pp, _) in &snap {
                    self.host.begin(pp);
                }
                self.link_drag = Some((p, prev, snap));
            }
            if let Some((_, src_start, snap)) = self.link_drag.clone() {
                for (pp, peer_start) in snap {
                    let pv = linked_value(info, peer_start, src_start, new_v);
                    if (pv - self.values[pp]).abs() > 0.0 {
                        self.values[pp] = pv;
                        self.controls.param_changed(pp, pv);
                        self.host.perform(pp, pv);
                    }
                }
                let _ = self.controls.take_changed();
            }
        } else {
            for pp in peers {
                let pv = linked_value(info, self.values[pp], prev, new_v);
                if (pv - self.values[pp]).abs() > 0.0 {
                    self.values[pp] = pv;
                    self.controls.param_changed(pp, pv);
                    self.host.set(pp, pv);
                }
            }
            let _ = self.controls.take_changed();
        }
    }

    fn end_link_drag(&mut self) {
        if let Some((_, _, snap)) = self.link_drag.take() {
            for (pp, _) in snap {
                self.host.end(pp);
            }
        }
    }

    fn dims_changed(&self) -> bool {
        let (fresh, _) = self.make_cells();
        fresh.len() != self.cells.len() || fresh.iter().zip(&self.cells).any(|(a, b)| a.dim != b.dim || a.param != b.param)
    }

    fn select_band(&mut self, b: usize) {
        let b = b.min(self.active_bands().saturating_sub(1));
        if self.target != Target::Band(b) {
            self.target = Target::Band(b);
            self.panel = None;
            self.build();
        }
    }

    fn select_limiter(&mut self) {
        if self.target != Target::Limiter {
            self.target = Target::Limiter;
            self.panel = None;
            self.build();
        }
    }

    fn select_sec(&mut self, sec: usize) {
        if sec < BAND_SECTIONS && (self.sec_tab != sec || self.target == Target::Limiter) {
            if self.target == Target::Limiter {
                self.target = Target::Band(0);
            }
            self.sec_tab = sec;
            self.panel = None;
            self.build();
        }
    }

    fn toggle_param(&mut self, p: usize) {
        let prev = self.values[p];
        let v = if prev >= 0.5 { 0.0 } else { 1.0 };
        self.values[p] = v;
        self.host.set(p, v);
        self.controls.param_changed(p, v);
        let _ = self.controls.take_changed();
        let was_dragging = self.dragging;
        self.dragging = false;
        self.propagate_link(p, prev, v);
        self.dragging = was_dragging;
        self.damage.add_all();
    }

    fn set_band_link(&mut self, on: bool) {
        if !on {
            self.end_link_drag();
        }
        self.band_link = on;
        self.damage.add_all();
    }

    fn set_view3d(&mut self, on: bool) {
        self.view3d = on;
        self.damage.add_all();
    }

    fn toggle_panel(&mut self, p: usize) {
        let sec = self.active_sec_kind();
        if p < page(sec).panels.len() {
            self.open_panel(if self.panel == Some(p) { None } else { Some(p) });
        }
    }

    fn open_panel(&mut self, p: Option<usize>) {
        let sec = self.active_sec_kind();
        self.panel = p.filter(|&p| p < page(sec).panels.len());
        self.build();
    }

    fn current_live(&self) -> (Live, &VecDeque<f32>, &VecDeque<f32>) {
        match self.target {
            Target::Band(b) => (self.band_live[b][self.sec_tab], &self.band_trace[b][self.sec_tab], &self.band_level[b][self.sec_tab]),
            Target::Limiter => (self.lim_live, &self.lim_trace, &self.lim_level),
        }
    }

    // ---- painting ----

    fn paint_layer(&self, s: &mut Scene) {
        let t = self.t();
        s.fill_rect(Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32), t.bg, t.bg, true);
        s.text(Point::new(16.0, 27.0), "OpenMultiband", t.text, 15.0, false);
        s.text(Point::new(162.0, 27.0), "BANDS", t.dim, 8.5, false);
        s.text(Point::new(354.0, 27.0), "SLOPE", t.dim, 8.5, false);
        kit::button(s, t, VIEW3D_BUTTON, "3D VIEW", self.view3d);
        kit::button(s, t, THEME_BUTTON, "THEME", false);

        // Rails & center page
        kit::panel(s, t, LEFT_RAIL);
        kit::panel(s, t, RIGHT_RAIL);
        kit::panel(s, t, PAGE);
        for (x, label) in [(50.0, "IN"), (1030.0, "OUT")] {
            s.text(Point::new(x, METER_BOTTOM + 12.0), label, t.dim, 9.0, true);
        }
        for rail in [22.0, 1002.0] {
            for c in 0..2 {
                let x = rail + c as f32 * 18.0;
                s.fill_round(Rect::new(x, METER_TOP, 14.0, METER_BOTTOM - METER_TOP), 3.0, t.seg_off, t.seg_off, true);
            }
            for db in [6.0f32, 0.0, -6.0, -12.0, -24.0, -36.0, -48.0, -60.0] {
                let y = meter_y(db);
                s.line(Point::new(rail + 40.0, y), Point::new(rail + 44.0, y), t.grid_strong, 1.0);
                s.text(Point::new(rail + 47.0, y), format!("{db:.0}"), t.dim, 8.0, false);
            }
        }

        // Top multiband spectrum plot grid
        kit::plot(s, t, MB_PLOT);
        let mb_bottom = MB_PLOT.1 + MB_PLOT.3;
        for f in [50.0f32, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10_000.0] {
            let x = mb_freq_x(f);
            let strong = f == 100.0 || f == 1000.0 || f == 10_000.0;
            s.line(Point::new(x, MB_PLOT.1), Point::new(x, mb_bottom), if strong { t.grid_strong } else { t.grid }, 1.0);
            s.text(Point::new(x, mb_bottom + 9.0), short_freq(f), t.dim, 8.0, true);
        }
        for db in [0.0f32, -12.0, -24.0, -36.0, -48.0, -60.0] {
            let y = mb_y(db);
            s.line(Point::new(MB_PLOT.0, y), Point::new(MB_PLOT.0 + MB_PLOT.2, y), if db == 0.0 { t.grid_strong } else { t.grid }, 1.0);
            kit::text_right(s, MB_PLOT.0 - 4.0, y, &format!("{db:.0}"), t.dim, 8.0);
        }
        kit::text_right(s, MB_PLOT.0 - 4.0, MB_PLOT.1 + 5.0, "dBFS", t.dim, 8.0);
        for g in [0.0f32, 12.0, 24.0] {
            let y = MB_PLOT.1 + (g / GR_DB) * MB_PLOT.3 * 0.45;
            let lbl = if g == 0.0 { "0".to_string() } else { format!("-{g:.0}") };
            s.text(Point::new(MB_PLOT.0 + MB_PLOT.2 + 4.0, y), lbl, t.over, 8.0, false);
        }

        // Static band tint regions, trim lines, and labels inside MB_PLOT (cached layer).
        let n_bands = self.active_bands();
        let any_solo = (0..n_bands).any(|b| self.values[LAYOUT.bands[b].solo] >= 0.5);
        for b in 0..n_bands {
            let (lo, hi) = self.band_bounds(b);
            let x0 = mb_freq_x(lo);
            let x1 = mb_freq_x(hi);
            let w = (x1 - x0).max(1.0);
            let bc = t.band(b);
            let bl = &LAYOUT.bands[b];
            let selected = self.target == Target::Band(b);
            let muted = self.values[bl.mute] >= 0.5;
            let soloed = self.values[bl.solo] >= 0.5;
            let byp = self.values[bl.bypass] >= 0.5;
            let audible = if any_solo { soloed } else { !muted };
            let alpha = if !audible {
                0.03
            } else if selected {
                0.14
            } else {
                0.06
            };
            s.fill_rect(Rect::new(x0, MB_PLOT.1 + 1.0, w, MB_PLOT.3 - 2.0), bc.with_alpha(alpha), bc.with_alpha(alpha), true);
            if selected {
                s.fill_rect(Rect::new(x0, MB_PLOT.1, w, 2.5), bc, bc, true);
            }
            let trim_db = self.values[bl.gain];
            if trim_db.abs() > 0.1 && !byp {
                let ty = (MB_PLOT.1 + MB_PLOT.3 * 0.5 - (trim_db / 24.0) * (MB_PLOT.3 * 0.35)).clamp(MB_PLOT.1 + 18.0, mb_bottom - 18.0);
                s.line(Point::new(x0 + 2.0, ty), Point::new(x1 - 2.0, ty), bc, 1.8);
            }
            if w >= 36.0 {
                let tag = format!("B{}", b + 1);
                let tw = goodlookindri_shell::text::width(&tag, 8.0);
                let rx = x0 + 5.0;
                let ry = mb_bottom - 16.0;
                s.fill_round(Rect::new(rx, ry, tw + 8.0, 12.0), 3.0, t.plot_bg.with_alpha(0.85), t.plot_bg.with_alpha(0.85), true);
                s.text(Point::new(rx + 4.0, ry + 6.0), tag, bc, 8.0, false);
            }
        }
        // Draggable crossover vertical lines & frequency badges (cached; re-hashed when any xover moves).
        let splits = n_bands.saturating_sub(1);
        let mut prev_badge_right = MB_PLOT.0;
        for k in 0..splits {
            let f = self.values[P_XOVER_BASE + k];
            let x = mb_freq_x(f).clamp(MB_PLOT.0 + 4.0, MB_PLOT.0 + MB_PLOT.2 - 4.0);
            s.line(Point::new(x, MB_PLOT.1), Point::new(x, mb_bottom), t.text.with_alpha(0.75), 1.4);
            let tag = short_freq(f);
            let tw = goodlookindri_shell::text::width(&tag, 8.0);
            let bw = (tw + 8.0).max(26.0);
            let bx = (x - bw * 0.5).clamp(MB_PLOT.0 + 2.0, MB_PLOT.0 + MB_PLOT.2 - bw - 2.0);
            let by = if bx < prev_badge_right + 2.0 { MB_PLOT.1 + 22.0 } else { MB_PLOT.1 + 5.0 };
            prev_badge_right = bx + bw;
            s.fill_round(Rect::new(bx, by, bw, 14.0), 4.0, t.panel, t.panel, true);
            s.stroke_round(Rect::new(bx, by, bw, 14.0), 4.0, t.accent, 1.0);
            s.text(Point::new(bx + bw * 0.5, by + 7.0), tag, t.text, 8.0, true);
        }

        // Band selector row + Master Limiter tab
        for b in 0..n_bands {
            let r = band_tab_rect(b, n_bands);
            let open = self.target == Target::Band(b);
            let bc = t.band(b);
            let bl = &LAYOUT.bands[b];
            let muted = self.values[bl.mute] >= 0.5;
            let soloed = self.values[bl.solo] >= 0.5;
            let byp = self.values[bl.bypass] >= 0.5;
            let fill = if open { t.panel } else { t.inset };
            s.fill_round(rect(r), 7.0, fill, fill, true);
            let linked_border = self.band_link && n_bands > 1;
            let (bcol, bw) = if open {
                (bc, 1.8)
            } else if linked_border {
                (t.accent.with_alpha(0.55), 1.3)
            } else {
                (t.border, 1.0)
            };
            s.stroke_round(rect(r), 7.0, bcol, bw);
            let dot = Scene::ellipse_at(Point::new(r.0 + 13.0, r.1 + r.3 * 0.5), 4.0);
            if byp || muted {
                s.stroke_ellipse(dot, bc, 1.2);
            } else {
                s.fill_ellipse_solid(dot, bc);
            }
            let (lo, hi) = self.band_bounds(b);
            let label = if r.2 >= 130.0 {
                if n_bands == 1 {
                    "B1  FULL BAND".to_string()
                } else {
                    format!("B{}  {}-{}", b + 1, short_freq(lo), short_freq(hi))
                }
            } else {
                format!("B{}", b + 1)
            };
            s.text(Point::new(r.0 + 24.0, r.1 + r.3 * 0.5), label, if open { t.text } else { t.dim }, 9.0, false);
            // Status badge right edge (S / M / BYP)
            let badge = if soloed {
                Some(("S", t.ok))
            } else if muted {
                Some(("M", t.over))
            } else if byp {
                Some(("B", t.dim))
            } else {
                None
            };
            if let Some((txt, col)) = badge {
                let bx = r.0 + r.2 - 19.0;
                let by = r.1 + 6.0;
                s.fill_round(Rect::new(bx, by, 14.0, 14.0), 3.0, col, col, true);
                s.text(Point::new(bx + 7.0, by + 7.0), txt, t.ink_on(col), 8.0, true);
            }
        }
        // Band Link toggle
        kit::button(s, t, LINK_BUTTON, "LINK", self.band_link);
        // Master Limiter tab
        {
            let r = LIMITER_TAB;
            let open = self.target == Target::Limiter;
            let on = self.values[LAYOUT.limiter.on] >= 0.5;
            s.fill_round(rect(r), 7.0, if open { t.panel } else { t.inset }, if open { t.panel } else { t.inset }, true);
            s.stroke_round(rect(r), 7.0, if open { t.accent } else { t.border }, if open { 1.8 } else { 1.0 });
            let (dx, dy) = limiter_dot();
            let dot = Scene::ellipse_at(Point::new(dx, dy), 4.0);
            if on { s.fill_ellipse_solid(dot, t.accent); } else { s.stroke_ellipse(dot, t.dim, 1.2); }
            s.text(Point::new(r.0 + 26.0, dy), "MASTER LIMITER", if open { t.text } else { t.dim }, 9.0, false);
        }

        // Section tabs (or Master Limiter heading)
        match self.target {
            Target::Band(b) => {
                let bl = &LAYOUT.bands[b];
                let bc = t.band(b);
                for si in 0..BAND_SECTIONS {
                    let r = sec_tab_rect(si);
                    let open = self.sec_tab == si;
                    let on = self.values[bl.sections[si].on] >= 0.5;
                    s.fill_round(rect(r), 6.0, if open { t.panel } else { t.inset }, if open { t.panel } else { t.inset }, true);
                    s.stroke_round(rect(r), 6.0, if open { bc } else { t.border }, if open { 1.5 } else { 1.0 });
                    let (dx, dy) = sec_dot(si);
                    let dot = Scene::ellipse_at(Point::new(dx, dy), 3.8);
                    if on { s.fill_ellipse_solid(dot, bc); } else { s.stroke_ellipse(dot, t.dim, 1.2); }
                    s.text(Point::new(r.0 + 23.0, dy), SEC_TAB_LABELS[si], if open { t.text } else { t.dim }, 8.5, false);
                }
                s.text(Point::new(820.0, 291.0), "GAIN", t.dim, 8.5, false);
            }
            Target::Limiter => {
                s.text(Point::new(112.0, 291.0), "MASTER LIMITER  ·  POST-SUM BROADBAND", t.text, 10.5, false);
            }
        }

        // Main dynamics display (2D static grid; 3D terrain image blits in paint_live so cached layer stays warm).
        let sec_kind = self.active_sec_kind();
        if self.view3d {
            if self.terrain_img.img.is_none() {
                self.paint_3d_layer(s);
            }
        } else {
            kit::plot(s, t, DISPLAY);
            let bottom = DISPLAY.1 + DISPLAY.3;
            let mut db = 0.0f32;
            while db >= LO - 1e-3 {
                let y = level_y(db);
                s.line(Point::new(DISPLAY.0, y), Point::new(DISPLAY.0 + DISPLAY.2, y), if db == 0.0 { t.grid_strong } else { t.grid }, 1.0);
                kit::text_right(s, DISPLAY.0 - 4.0, y, &format!("{db:.0}"), t.dim, 8.0);
                db -= 12.0;
            }
            kit::text_right(s, DISPLAY.0 - 4.0, DISPLAY.1 + 5.0, "dBFS", t.dim, 8.0);
            for k in 0..=HISTORY_S {
                let x = DISPLAY.0 + DISPLAY.2 * k as f32 / HISTORY_S as f32;
                if k > 0 && k < HISTORY_S {
                    s.line(Point::new(x, DISPLAY.1), Point::new(x, bottom), t.grid, 1.0);
                }
                let label = if k == HISTORY_S { "NOW".to_string() } else { format!("-{} s", HISTORY_S - k) };
                let y = bottom + 9.0;
                match k {
                    0 => s.text(Point::new(x, y), label, t.dim, 8.0, false),
                    k if k == HISTORY_S => kit::text_right(s, x, y, &label, t.dim, 8.0),
                    _ => s.text(Point::new(x, y), label, t.dim, 8.0, true),
                }
            }
            // Curve square plate
            let plate = t.plot_bg.with_alpha(0.85);
            s.fill_rect(Rect::new(DISPLAY.0, DISPLAY.1, DISPLAY.3, DISPLAY.3), plate, plate, true);
            s.stroke_round(Rect::new(DISPLAY.0, DISPLAY.1, DISPLAY.3, DISPLAY.3), 0.0, t.grid_strong, 1.0);
            for db in [-48.0f32, -36.0, -24.0, -12.0, 0.0] {
                let x = curve_x(db);
                s.line(Point::new(x, DISPLAY.1), Point::new(x, bottom), t.grid, 1.0);
                s.text(Point::new(x, bottom - 8.0), format!("{db:.0}"), t.dim, 8.0, true);
            }
            s.text(Point::new(DISPLAY.0 + 8.0, DISPLAY.1 + 11.0), "CURVE  IN -> OUT dBFS", t.dim, 8.0, false);
            kit::dashed(s, &[Point::new(curve_x(LO), level_y(LO)), Point::new(curve_x(HI), level_y(HI))], 4.0, 4.0, t.grid_strong, 1.0);
            let (gr_col, sign) = if lifts(sec_kind) { (t.ok, "+") } else { (t.over, "-") };
            for g in [0.0f32, 12.0, 24.0] {
                let label = if g == 0.0 { "0".to_string() } else { format!("{sign}{g:.0}") };
                s.text(Point::new(DISPLAY.0 + DISPLAY.2 + 4.0, gain_y(g)), label, gr_col, 8.0, false);
            }
            s.text(Point::new(DISPLAY.0 + DISPLAY.2 + 4.0, gain_y(GR_DB) + 12.0), if lifts(sec_kind) { "LIFT" } else { "GR" }, gr_col, 8.0, false);
            // Static threshold dashes & tag (cached; re-hashed when params change).
            let v = |i: usize| self.local(i);
            let thr = threshold(sec_kind, &v);
            let sq = DISPLAY.0 + DISPLAY.3;
            if (LO..=HI).contains(&thr) {
                kit::dashed(s, &[Point::new(sq, level_y(thr)), Point::new(DISPLAY.0 + DISPLAY.2, level_y(thr))], 4.0, 4.0, t.dim, 1.0);
                if self.panel.is_none() {
                    let tag = format!("THR {thr:.0} dB");
                    let (w, right, y) = (goodlookindri_shell::text::width(&tag, 8.0), DISPLAY.0 + DISPLAY.2 - 6.0, (level_y(thr) - 9.0).max(DISPLAY.1 + 8.0));
                    s.fill_round(Rect::new(right - w - 5.0, y - 6.5, w + 10.0, 13.0), 3.0, t.plot_bg, t.plot_bg, true);
                    kit::text_right(s, right, y, &tag, t.text, 8.0);
                }
                kit::dashed(s, &[Point::new(curve_x(thr), DISPLAY.1 + 20.0), Point::new(curve_x(thr), bottom)], 2.0, 3.0, t.dim, 1.0);
            }
        }

        // Settings chips
        for (i, (label, _)) in page(sec_kind).panels.iter().enumerate() {
            let open = self.panel == Some(i);
            let r = chip_rect(sec_kind, i);
            kit::button(s, t, r, label, open);
            let (cx, cy, d) = (r.0 + r.2 - 12.0, r.1 + r.3 * 0.5, if open { -2.5 } else { 2.5 });
            let ink = if open { t.on_accent } else { t.dim };
            kit::curve(s, vec![Point::new(cx - 3.5, cy - d), Point::new(cx, cy + d), Point::new(cx + 3.5, cy - d)], ink, 1.4);
        }

        let non_panel = self.cells.len() - self.panel_cells;
        for (cell, c) in self.cells[..non_panel].iter().zip(self.controls.iter()) {
            kit::paint_static(s, t, cell);
            let focused = self.keys_active && self.controls.focused().is_some_and(|f| self.controls.get(f).is_some_and(|fc| fc.param == cell.param));
            kit::paint_value(s, t, cell, c, c.value(), focused);
        }
    }

    fn paint_live(&self, s: &mut Scene) {
        let t = self.t();
        let n_bands = self.active_bands();
        let any_solo = (0..n_bands).any(|b| self.values[LAYOUT.bands[b].solo] >= 0.5);
        let mb_bottom = MB_PLOT.1 + MB_PLOT.3;

        // 1. Live multiband spectrum & per-band dynamic gain bars inside MB_PLOT.
        {
            let pre_pts: Vec<Point> = (0..MB_SPEC_BINS)
                .map(|k| Point::new(MB_PLOT.0 + MB_PLOT.2 * k as f32 / (MB_SPEC_BINS - 1) as f32, mb_y(self.mb_spec[0].db[k])))
                .collect();
            let post_pts: Vec<Point> = (0..MB_SPEC_BINS)
                .map(|k| Point::new(MB_PLOT.0 + MB_PLOT.2 * k as f32 / (MB_SPEC_BINS - 1) as f32, mb_y(self.mb_spec[1].db[k])))
                .collect();
            kit::area(s, &post_pts, mb_bottom, t.out_fill.with_alpha(t.out_fill_alpha));
            kit::curve(s, pre_pts, t.in_line.with_alpha(0.65), 1.0);
            kit::curve(s, post_pts, t.out_line, 1.3);

            // Live per-band dynamic gain bars hanging from top edge.
            for b in 0..n_bands {
                let (lo, hi) = self.band_bounds(b);
                let x0 = mb_freq_x(lo);
                let x1 = mb_freq_x(hi);
                let w = (x1 - x0).max(1.0);
                let bl = &LAYOUT.bands[b];
                let muted = self.values[bl.mute] >= 0.5;
                let soloed = self.values[bl.solo] >= 0.5;
                let byp = self.values[bl.bypass] >= 0.5;
                let audible = if any_solo { soloed } else { !muted };
                let gd = self.band_summary[b].gain_db;
                if audible && !byp && gd.abs() > 0.1 {
                    let bh = (gd.abs() / GR_DB).min(1.0) * MB_PLOT.3 * 0.45;
                    let col = if gd > 0.0 { t.ok } else { t.over };
                    s.fill_rect(Rect::new(x0 + 1.0, MB_PLOT.1 + 3.0, (w - 2.0).max(1.0), bh), col.with_alpha(0.32), col.with_alpha(0.32), true);
                    s.line(Point::new(x0 + 1.0, MB_PLOT.1 + 3.0 + bh), Point::new(x1 - 1.0, MB_PLOT.1 + 3.0 + bh), col, 1.4);
                }
            }
        }

        // 2. Mini activity bars under Band tabs, Master Limiter tab, and Section tabs.
        for b in 0..n_bands {
            let r = band_tab_rect(b, n_bands);
            let g = self.band_summary[b].gain_db.abs().min(GR_DB);
            if g > 0.1 {
                let w = (r.2 - 16.0) * g / GR_DB;
                let col = if self.band_summary[b].gain_db > 0.0 { t.ok } else { t.band(b) };
                s.fill_rect(Rect::new(r.0 + 8.0, r.1 + r.3 - 3.5, w, 2.0), col, col, true);
            }
        }
        {
            let r = LIMITER_TAB;
            let on = self.values[LAYOUT.limiter.on] >= 0.5;
            let g = self.lim_live.gain[0].abs().max(self.lim_live.gain[1].abs()).min(GR_DB);
            if on && g > 0.05 {
                let w = (r.2 - 16.0) * g / GR_DB;
                s.fill_rect(Rect::new(r.0 + 8.0, r.1 + r.3 - 3.5, w, 2.0), t.over, t.over, true);
            }
        }
        if let Target::Band(b) = self.target {
            let bl = &LAYOUT.bands[b];
            for si in 0..BAND_SECTIONS {
                let r = sec_tab_rect(si);
                let on = self.values[bl.sections[si].on] >= 0.5;
                let l = self.band_live[b][si];
                let g = l.gain[0].abs().max(l.gain[1].abs()).min(GR_DB);
                if on && g > 0.05 {
                    let w = (r.2 - 16.0) * g / GR_DB;
                    let col = if lifts(si) { t.ok } else { t.band(b) };
                    s.fill_rect(Rect::new(r.0 + 8.0, r.1 + r.3 - 3.5, w, 2.0), col, col, true);
                }
            }
        }

        // 3. Main dynamics display (2D curve+history, or cached 3D terrain image).
        let sec_kind = self.active_sec_kind();
        let (live, trace, level) = self.current_live();
        if self.view3d {
            self.paint_3d_terrain_cached(s);
        } else {
            let v = |i: usize| self.local(i);
            let on = self.values[self.active_sec_info().on] >= 0.5;
            let active_col = match self.target {
                Target::Band(b) => t.band(b),
                Target::Limiter => t.accent,
            };
            let ink = if on { active_col } else { t.dim };
            let bottom = DISPLAY.1 + DISPLAY.3;
            let gr_col = if lifts(sec_kind) { t.ok } else { t.over };
            let x_at = |k: usize| DISPLAY.0 + DISPLAY.2 * k as f32 / (TRACE_LEN - 1) as f32;
            let input: Vec<Point> = level.iter().enumerate().map(|(k, l)| Point::new(x_at(k), level_y(*l))).collect();
            let output: Vec<Point> = level.iter().zip(trace).enumerate().map(|(k, (l, g))| Point::new(x_at(k), level_y(l + g))).collect();
            let band: Vec<Point> = trace.iter().enumerate().map(|(k, g)| Point::new(x_at(k), gain_y(*g))).collect();
            let sq = DISPLAY.0 + DISPLAY.3;
            let first = ((sq - DISPLAY.0) / DISPLAY.2 * (TRACE_LEN - 1) as f32).floor() as usize;
            // Skip occluded history area fills while an opaque settings panel covers DISPLAY.
            if self.panel.is_none() {
                kit::between(s, &input[first..], &output[first..], t.in_fill.with_alpha(t.in_fill_alpha));
                kit::area(s, &output[first..], bottom, t.out_fill.with_alpha(t.out_fill_alpha));
                kit::area(s, &band[first..], DISPLAY.1, gr_col.with_alpha(0.25));
            }
            let faint = 0.28;
            kit::curve(s, input[..=first].to_vec(), t.in_line.with_alpha(faint), 1.0);
            kit::curve(s, output[..=first].to_vec(), t.out_line.with_alpha(faint), 1.0);
            kit::curve(s, band[..=first].to_vec(), gr_col.with_alpha(faint), 1.0);
            kit::curve(s, input[first..].to_vec(), t.in_line, 1.0);
            kit::curve(s, output[first..].to_vec(), if on { t.out_line } else { t.dim }, 1.5);
            kit::curve(s, band[first..].to_vec(), gr_col, 1.2);
            let pts: Vec<Point> = (0..=48)
                .map(|k| {
                    let l = LO + (HI - LO) * k as f32 / 48.0;
                    Point::new(curve_x(l), level_y(curve_out(sec_kind, l as f64, &v) as f32))
                })
                .collect();
            kit::curve(s, pts, ink, 2.0);
            if live.active {
                let l = live.level[0].max(live.level[1]);
                if l > LO {
                    let g = if live.gain[0].abs() >= live.gain[1].abs() { live.gain[0] } else { live.gain[1] };
                    let p = Point::new(curve_x(l), level_y(l + g));
                    s.fill_ellipse_solid(Scene::ellipse_at(p, 5.0), t.text);
                    s.stroke_ellipse(Scene::ellipse_at(p, 5.0), t.panel, 1.5);
                }
            }
        }

        // 4. Chip-row readouts (right-aligned to DISPLAY right edge).
        let mut right = DISPLAY.0 + DISPLAY.2;
        let y = CHIP_Y + 11.0;
        let now = *trace.back().unwrap_or(&0.0);
        let gr = format!("{} {} dB", if lifts(sec_kind) { "LIFT" } else { "GR" }, whole_db(now));
        kit::text_right(s, right, y, &gr, t.text, 11.0);
        right -= goodlookindri_shell::text::width(&gr, 11.0) + 12.0;
        if self.values[P_AUTO_GAIN] >= 0.5 {
            let ag = format!("AUTO GAIN {} dB", whole_db(self.auto_gain_db));
            kit::text_right(s, right, y, &ag, t.dim, 9.0);
            right -= goodlookindri_shell::text::width(&ag, 9.0) + 12.0;
        }
        if self.shared.as_ref().is_some_and(|sh| sh.key_present()) {
            kit::text_right(s, right, y, "KEY IN", t.ok, 9.0);
        }

        // 5. Open settings panel over DISPLAY (fast fill_rect body so 2x DRI stays well under 33 ms).
        if let Some(p) = self.panel {
            let r = panel_rect(sec_kind, p);
            s.fill_rect(rect(r), t.panel, t.panel, true);
            s.stroke_round(rect(r), 4.0, t.accent, 1.0);
            s.text(Point::new(r.0 + 16.0, r.1 + 17.0), page(sec_kind).panels[p].0, t.text, 9.5, false);
            for c in &self.cells[self.cells.len() - self.panel_cells..] {
                if matches!(c.kind, Kind::Segmented) {
                    s.text(Point::new(c.rect.0 - 76.0, c.rect.1 + c.rect.3 * 0.5), &c.label, t.dim, 9.0, false);
                }
                kit::paint_static(s, t, c);
            }
        }

        // 6. IN / OUT peak meters.
        for (m, x0) in [(0usize, 22.0f32), (1, 1002.0)] {
            for c in 0..2 {
                let x = x0 + c as f32 * 18.0;
                let db = self.meters[m][c];
                if db > M_LO {
                    let y = meter_y(db);
                    let col = if db > 0.0 { t.over } else { t.seg_on };
                    s.fill_round(Rect::new(x, y, 14.0, METER_BOTTOM - y), 3.0, col, col, true);
                }
            }
        }

        // 7. Control values for the open panel only (non-panel controls live in the cached layer).
        let non_panel = self.cells.len() - self.panel_cells;
        for (cell, c) in self.cells[non_panel..].iter().zip(self.controls.iter().skip(non_panel)) {
            let focused = self.keys_active && self.controls.focused().is_some_and(|f| self.controls.get(f).is_some_and(|fc| fc.param == cell.param));
            kit::paint_value(s, t, cell, c, c.value(), focused);
        }
    }
}

impl Editor for MultibandEditor {
    fn size(&self) -> (u32, u32) {
        (WIDTH, HEIGHT)
    }

    fn size_range(&self) -> SizeRange {
        SizeRange::fixed(WIDTH, HEIGHT)
    }

    fn bind(&mut self, state: Option<EditorState>) {
        self.shared = state.and_then(|s| s.downcast::<Shared>().ok());
    }

    fn param_changed(&mut self, index: usize, plain: f32) {
        let Some(v) = self.values.get_mut(index) else { return };
        if *v == plain {
            return;
        }
        *v = plain;
        self.controls.param_changed(index, plain);
        let _ = self.controls.take_changed();
        if index == P_BAND_COUNT || (!self.dragging && (self.dims_changed() || (P_XOVER_BASE..P_XOVER_BASE + MAX_SPLITS).contains(&index))) {
            self.build();
        }
        self.damage.add_all();
    }

    fn tick(&mut self, dt: f32) {
        let Some(sh) = self.shared.clone() else { return };
        self.hist_acc += dt.max(0.0);
        let pushes = ((self.hist_acc * HIST_HZ) as usize).min(4);
        self.hist_acc = (self.hist_acc - pushes as f32 / HIST_HZ).min(1.0 / HIST_HZ);

        for b in 0..MAX_BANDS {
            self.band_summary[b] = sh.take_band_summary(b);
            for si in 0..BAND_SECTIONS {
                let l = sh.take_band_section(b, si);
                self.band_live[b][si] = l;
                let g = if l.gain[0].abs() >= l.gain[1].abs() { l.gain[0] } else { l.gain[1] };
                for _ in 0..pushes {
                    self.band_trace[b][si].pop_front();
                    self.band_trace[b][si].push_back(if l.active { g } else { 0.0 });
                    self.band_level[b][si].pop_front();
                    self.band_level[b][si].push_back(if l.active { l.level[0].max(l.level[1]) } else { -200.0 });
                }
            }
        }
        let ll = sh.take_limiter();
        self.lim_live = ll;
        let lg = if ll.gain[0].abs() >= ll.gain[1].abs() { ll.gain[0] } else { ll.gain[1] };
        for _ in 0..pushes {
            self.lim_trace.pop_front();
            self.lim_trace.push_back(if ll.active { lg } else { 0.0 });
            self.lim_level.pop_front();
            self.lim_level.push_back(if ll.active { ll.level[0].max(ll.level[1]) } else { -200.0 });
        }

        self.auto_gain_db = sh.auto_gain_db();
        for (c, tap) in self.taps.iter_mut().enumerate() {
            sh.read_tap(c, tap);
        }
        let sr = sh.sample_rate();
        let [a, b, c, d] = &self.taps;
        self.mb_spec[0].update(a, Some(b), sr, dt);
        self.mb_spec[1].update(c, Some(d), sr, dt);
        self.terrain.feed(&self.taps, sr, dt);

        let (pin, pout) = sh.take_peaks();
        let fall = 24.0 * dt;
        for c in 0..2 {
            self.meters[0][c] = to_db(pin[c]).max(self.meters[0][c] - fall);
            self.meters[1][c] = to_db(pout[c]).max(self.meters[1][c] - fall);
        }
        self.damage.add_all();
    }

    fn damage(&mut self) -> Option<Vec<DamageRect>> {
        self.sync();
        self.damage.pending().map(<[DamageRect]>::to_vec)
    }

    fn render(&mut self, scale: f32) -> Frame<'_> {
        self.sync();
        if self.view3d {
            self.update_terrain_image(scale);
        }
        let target_key = match self.target {
            Target::Band(b) => b as u64,
            Target::Limiter => 100,
        };
        let n_bands = self.active_bands();
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        let mut mix = |v: u64| {
            h = (h ^ v).wrapping_mul(0x0100_0000_01b3);
        };
        mix(target_key);
        mix(self.sec_tab as u64);
        mix(self.theme as u64);
        mix(self.view3d as u64);
        mix(self.band_link as u64);
        mix(self.panel.map_or(0, |p| p as u64 + 1));
        mix(n_bands as u64);
        mix(u64::from(self.values[LAYOUT.limiter.on] >= 0.5));
        for k in 0..MAX_SPLITS {
            mix(self.values[P_XOVER_BASE + k].to_bits() as u64);
        }
        for b in 0..n_bands {
            let bl = &LAYOUT.bands[b];
            mix(self.values[bl.gain].to_bits() as u64);
            let mut bits = u64::from(self.values[bl.solo] >= 0.5)
                | (u64::from(self.values[bl.mute] >= 0.5) << 1)
                | (u64::from(self.values[bl.bypass] >= 0.5) << 2);
            for si in 0..BAND_SECTIONS {
                bits |= u64::from(self.values[bl.sections[si].on] >= 0.5) << (3 + si);
            }
            mix(bits);
        }
        let non_panel = self.cells.len() - self.panel_cells;
        let focused_param = if self.keys_active {
            self.controls.focused().and_then(|f| self.controls.get(f).map(|fc| fc.param))
        } else {
            None
        };
        for (cell, c) in self.cells[..non_panel].iter().zip(self.controls.iter()) {
            mix(cell.param as u64);
            mix(c.value().to_bits() as u64);
            let flags = u64::from(cell.dim)
                | (u64::from(c.hover()) << 1)
                | (u64::from(c.pressed()) << 2)
                | (u64::from(focused_param == Some(cell.param)) << 3);
            mix(flags);
        }
        let mut layer = Scene::new();
        self.paint_layer(&mut layer);
        let mut live = Scene::new();
        self.paint_live(&mut live);
        let damage = self.damage.pending();
        let frame = self.canvas.frame_damaged(scale, h, |dl| dl.append(layer), |dl| dl.append(live), damage);
        self.damage.clear();
        frame
    }

    fn modifiers_changed(&mut self, modifiers: Modifiers) {
        self.modifiers = modifiers;
        self.controls.set_modifiers(modifiers);
    }

    fn mouse_down(&mut self, x: f32, y: f32, clicks: u32) {
        self.dragging = true;
        if inside(THEME_BUTTON, x, y) {
            self.theme = self.theme.next();
            store_theme(self.theme);
            self.damage.add_all();
            return;
        }
        if inside(VIEW3D_BUTTON, x, y) {
            self.set_view3d(!self.view3d);
            return;
        }
        // Band selector row + Master Limiter tab
        let n_bands = self.active_bands();
        for b in 0..n_bands {
            if inside(band_tab_rect(b, n_bands), x, y) {
                self.select_band(b);
                return;
            }
        }
        if inside(LINK_BUTTON, x, y) {
            self.set_band_link(!self.band_link);
            return;
        }
        if inside(limiter_dot_hit(), x, y) {
            self.toggle_param(LAYOUT.limiter.on);
            return;
        }
        if inside(LIMITER_TAB, x, y) {
            self.select_limiter();
            return;
        }
        // Section tabs (when a band is selected)
        if let Target::Band(b) = self.target {
            for si in 0..BAND_SECTIONS {
                if inside(sec_dot_hit(si), x, y) {
                    self.toggle_param(LAYOUT.bands[b].sections[si].on);
                    return;
                }
                if inside(sec_tab_rect(si), x, y) {
                    self.select_sec(si);
                    return;
                }
            }
        }
        // Settings chips
        let sec_kind = self.active_sec_kind();
        for i in 0..page(sec_kind).panels.len() {
            if inside(chip_rect(sec_kind, i), x, y) {
                self.toggle_panel(i);
                return;
            }
        }
        // Click outside an open settings panel inside DISPLAY closes it.
        if let Some(p) = self.panel {
            if inside(DISPLAY, x, y) && !inside(panel_rect(sec_kind, p), x, y) {
                self.open_panel(None);
                return;
            }
        }
        // First let controls handle crossover grips & normal widgets.
        let hit_control = self.cells.iter().any(|c| inside(c.rect, x, y));
        if hit_control {
            self.controls.mouse_down(x, y, clicks);
            self.sync();
            return;
        }
        // Click inside MB_PLOT (away from crossover handles) selects that band.
        if inside(MB_PLOT, x, y) {
            let f = kit::x_freq(x, MB_PLOT.0, MB_PLOT.0 + MB_PLOT.2, FREQ_MIN, FREQ_MAX);
            for b in 0..n_bands {
                let (lo, hi) = self.band_bounds(b);
                if f >= lo && (f <= hi || b + 1 == n_bands) {
                    self.select_band(b);
                    return;
                }
            }
        }
    }

    fn mouse_move(&mut self, x: f32, y: f32) {
        self.controls.mouse_move(x, y);
        self.sync();
    }

    fn mouse_up(&mut self, x: f32, y: f32) {
        self.dragging = false;
        self.controls.mouse_up(x, y);
        self.sync();
        self.end_link_drag();
        // Refresh crossover handle hitboxes only after a crossover frequency moved;
        // rebuilding unconditionally would drop an active double-click text edit.
        if self.xover_moved {
            self.xover_moved = false;
            self.build();
        }
    }

    fn wheel(&mut self, x: f32, y: f32, dy: f32) {
        self.controls.wheel(x, y, dy);
        self.sync();
    }

    fn right_mouse_down(&mut self, x: f32, y: f32) -> bool {
        self.controls.context_click(x, y)
    }

    fn take_param_menu(&mut self) -> Option<ParamMenu> {
        self.controls.take_param_menu()
    }

    fn param_menu_action(&mut self, param: u32, action: ParamMenuAction) -> Option<String> {
        let r = self.controls.param_menu_action(param, action);
        self.sync();
        r
    }

    fn tooltips(&self) -> Vec<Tooltip> {
        let mut tips = self.controls.tooltips();
        let n_bands = self.active_bands();
        for b in 0..n_bands {
            tips.push(Tooltip { rect: DamageRect::from_tuple(band_tab_rect(b, n_bands)), text: format!("Select Band {}", b + 1) });
        }
        tips.push(Tooltip { rect: DamageRect::from_tuple(LINK_BUTTON), text: "Link all active bands for module controls and band gain".into() });
        tips.push(Tooltip { rect: DamageRect::from_tuple(limiter_dot_hit()), text: "Switch Master Limiter on or off".into() });
        tips.push(Tooltip { rect: DamageRect::from_tuple(LIMITER_TAB), text: "Edit post-sum Master Limiter".into() });
        let sec_kind = self.active_sec_kind();
        for (i, (label, _)) in page(sec_kind).panels.iter().enumerate() {
            tips.push(Tooltip { rect: DamageRect::from_tuple(chip_rect(sec_kind, i)), text: format!("Open or close {} settings", label.to_lowercase()) });
        }
        tips.push(Tooltip { rect: DamageRect::from_tuple(VIEW3D_BUTTON), text: "Toggle 3D spectrum terrain view".into() });
        tips
    }

    fn accessibility_nodes(&self) -> Vec<AccessNode> {
        let mut nodes = self.controls.accessibility_nodes();
        let sec_kind = self.active_sec_kind();
        for (i, (label, _)) in page(sec_kind).panels.iter().enumerate() {
            let open = self.panel == Some(i);
            nodes.push(AccessNode {
                label: format!("{} settings", label.to_lowercase()),
                value: if open { "open" } else { "closed" }.into(),
                role: AccessRole::Switch,
                on: open,
                rect: DamageRect::from_tuple(chip_rect(sec_kind, i)),
                focused: false,
                enabled: true,
            });
        }
        nodes.push(AccessNode {
            label: "3D spectrum view".into(),
            value: if self.view3d { "on" } else { "off" }.into(),
            role: AccessRole::Switch,
            on: self.view3d,
            rect: DamageRect::from_tuple(VIEW3D_BUTTON),
            focused: false,
            enabled: true,
        });
        nodes.push(AccessNode {
            label: "Link bands".into(),
            value: if self.band_link { "on" } else { "off" }.into(),
            role: AccessRole::Switch,
            on: self.band_link,
            rect: DamageRect::from_tuple(LINK_BUTTON),
            focused: false,
            enabled: true,
        });
        nodes
    }

    fn accessibility_action(&mut self, node: usize, action: AccessAction) -> bool {
        let n = self.controls.accessibility_nodes().len();
        if node >= n {
            let chips = page(self.active_sec_kind()).panels.len();
            if action == AccessAction::Press && node - n < chips {
                self.toggle_panel(node - n);
                return true;
            }
            if action == AccessAction::Press && node - n == chips {
                self.set_view3d(!self.view3d);
                return true;
            }
            if action == AccessAction::Press && node - n == chips + 1 {
                self.set_band_link(!self.band_link);
                return true;
            }
            return false;
        }
        let r = self.controls.accessibility_action(node, action);
        self.sync();
        r
    }

    fn accessibility_range(&self, node: usize) -> Option<AccessRange> {
        self.controls.accessibility_range(node)
    }

    fn accessibility_set_value(&mut self, node: usize, value: f64) -> bool {
        let r = self.controls.accessibility_set_value(node, value);
        self.sync();
        r
    }

    fn accessibility_focus(&mut self, node: usize) -> bool {
        self.controls.accessibility_focus(node)
    }

    fn wants_keys(&self) -> bool {
        self.controls.wants_keys()
    }

    fn key_down(&mut self, key: Key, modifiers: Modifiers) -> bool {
        let r = self.controls.key_down(key, modifiers);
        self.sync();
        r
    }

    fn key_up(&mut self, key: Key, modifiers: Modifiers) -> bool {
        self.controls.key_up(key, modifiers)
    }

    fn take_text_edit(&mut self) -> Option<TextEdit> {
        self.controls.take_text_edit()
    }

    fn text_edit_commit(&mut self, text: &str) -> bool {
        let r = self.controls.text_edit_commit(text);
        self.sync();
        r
    }

    fn text_edit_cancel(&mut self) {
        self.controls.text_edit_cancel();
    }

    fn focus_changed(&mut self, focused: bool) {
        self.keys_active = focused;
        if !focused {
            self.controls.release_keys();
        }
        self.damage.add_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel_states(sec: usize) -> Vec<Option<usize>> {
        std::iter::once(None).chain((0..page(sec).panels.len()).map(Some)).collect()
    }

    /// Every parameter (all 6 bands x 4 sections, master limiter, 5 crossovers, globals)
    /// is bound to a visible control on some band/tab/panel.
    #[test]
    fn knob_double_click_typing_and_drag_work() {
        let mut e = MultibandEditor::new(EditorHost::detached());
        e.select_band(2); // Band 3 Compressor
        let thresh_p = LAYOUT.bands[2].sections[1].base + dynamics::comp::index::THRESHOLD;
        let cell = e.cells.iter().find(|c| c.param == thresh_p).expect("B3 threshold cell");
        let cx = cell.rect.0 + cell.rect.2 * 0.5;
        let cy = cell.rect.1 + cell.rect.3 * 0.5;
        // Double-click opens a text edit that survives mouse_up and commits.
        e.mouse_down(cx, cy, 2);
        assert!(e.take_text_edit().is_some(), "double-click should open text edit");
        e.mouse_up(cx, cy);
        assert!(e.text_edit_commit("-36"), "text edit must survive mouse_up and commit");
        assert!((e.values[thresh_p] - (-36.0)).abs() < 0.01, "expected -36, got {}", e.values[thresh_p]);
        // Dragging down lowers the threshold.
        e.mouse_down(cx, cy, 1);
        e.mouse_move(cx, cy + 40.0);
        e.mouse_up(cx, cy + 40.0);
        assert!(e.values[thresh_p] < -38.0, "drag down should lower threshold, got {}", e.values[thresh_p]);
    }

    #[test]
    fn band_link_propagates_relative_and_discrete_across_active_bands() {
        use openplugin::core::Gesture;
        use std::sync::{Arc, Mutex};
        let log: Arc<Mutex<Vec<Gesture>>> = Arc::new(Mutex::new(Vec::new()));
        let log_cb = log.clone();
        let host = EditorHost::new(move |g| log_cb.lock().unwrap().push(g));
        let mut e = MultibandEditor::new(host);
        // Default 4 bands, Band 1 Compressor selected.
        let t = |b: usize| LAYOUT.bands[b].sections[1].base + dynamics::comp::index::THRESHOLD;
        let on = |b: usize| LAYOUT.bands[b].sections[1].on;
        let g = |b: usize| LAYOUT.bands[b].gain;

        // Give Band 2 a +6 dB offset (-12 dB vs default -18 dB) via host automation.
        e.param_changed(t(1), -12.0);
        assert_eq!(e.values[t(0)], -18.0);
        assert_eq!(e.values[t(1)], -12.0);

        // Click LINK button on.
        e.mouse_down(LINK_BUTTON.0 + 10.0, LINK_BUTTON.1 + 10.0, 1);
        e.mouse_up(LINK_BUTTON.0 + 10.0, LINK_BUTTON.1 + 10.0);
        assert!(e.band_link);

        // Host automation must NOT cross-propagate even with Link on.
        e.param_changed(t(2), -20.0);
        assert_eq!(e.values[t(0)], -18.0);
        assert_eq!(e.values[t(2)], -20.0);

        // Double-click type -24 dB on Band 1 Threshold (-6 dB relative delta).
        let cell = e.cells.iter().find(|c| c.param == t(0)).unwrap().rect;
        let (cx, cy) = (cell.0 + cell.2 * 0.5, cell.1 + cell.3 * 0.5);
        e.mouse_down(cx, cy, 2);
        assert!(e.take_text_edit().is_some());
        e.mouse_up(cx, cy);
        assert!(e.text_edit_commit("-24"));
        assert!((e.values[t(0)] - (-24.0)).abs() < 0.01);
        assert!((e.values[t(1)] - (-18.0)).abs() < 0.01, "relative offset +6 dB preserved: {}", e.values[t(1)]);
        assert!((e.values[t(2)] - (-26.0)).abs() < 0.01, "relative offset -2 dB preserved: {}", e.values[t(2)]);
        assert!((e.values[t(3)] - (-24.0)).abs() < 0.01, "equal band matches: {}", e.values[t(3)]);
        assert!((e.values[t(4)] - (-18.0)).abs() < 0.01, "inactive band 5 untouched: {}", e.values[t(4)]);

        // Drag Band 1 Threshold down: sends balanced Begin/Perform/End for active peers too.
        log.lock().unwrap().clear();
        e.mouse_down(cx, cy, 1);
        e.mouse_move(cx, cy + 40.0);
        e.mouse_up(cx, cy + 40.0);
        let events = log.lock().unwrap().clone();
        for b in 0..4 {
            assert!(events.iter().any(|ev| matches!(ev, Gesture::Begin(p) if *p as usize == t(b))), "missing Begin for band {b}");
            assert!(events.iter().any(|ev| matches!(ev, Gesture::End(p) if *p as usize == t(b))), "missing End for band {b}");
        }
        assert!(!events.iter().any(|ev| matches!(ev, Gesture::Begin(p) if *p as usize == t(4))), "inactive band 5 should not gesture");

        // Toggling section dot matches discrete On across all active bands.
        let dot = sec_dot_hit(1);
        e.mouse_down(dot.0 + 4.0, dot.1 + 4.0, 1);
        e.mouse_up(dot.0 + 4.0, dot.1 + 4.0);
        for b in 0..4 {
            assert_eq!(e.values[on(b)], 0.0, "band {b} comp should switch off");
        }
        assert_eq!(e.values[on(4)], 1.0, "inactive band 5 untouched");

        // Band Gain links; Solo stays independent.
        let gcell = e.cells.iter().find(|c| c.param == g(0)).unwrap().rect;
        e.mouse_down(gcell.0 + 10.0, gcell.1 + 10.0, 2);
        assert!(e.take_text_edit().is_some());
        e.mouse_up(gcell.0 + 10.0, gcell.1 + 10.0);
        assert!(e.text_edit_commit("3"));
        for b in 0..4 {
            assert!((e.values[g(b)] - 3.0).abs() < 0.01, "band {b} gain linked: {}", e.values[g(b)]);
        }
        let scell = e.cells.iter().find(|c| c.param == LAYOUT.bands[0].solo).unwrap().rect;
        e.mouse_down(scell.0 + 10.0, scell.1 + 10.0, 1);
        e.mouse_up(scell.0 + 10.0, scell.1 + 10.0);
        assert_eq!(e.values[LAYOUT.bands[0].solo], 1.0);
        assert_eq!(e.values[LAYOUT.bands[1].solo], 0.0, "Solo stays independent");
    }

    #[test]
    fn every_parameter_has_a_control() {
        let mut e = MultibandEditor::new(EditorHost::detached());
        // Enable all 6 bands so all 6 bands and all 5 crossover handles exist.
        e.param_changed(P_BAND_COUNT, (MAX_BANDS - 1) as f32);
        let mut seen = vec![false; INFO.params.len()];
        for b in 0..MAX_BANDS {
            e.select_band(b);
            for sec in 0..BAND_SECTIONS {
                e.select_sec(sec);
                for p in panel_states(sec) {
                    e.open_panel(p);
                    for c in &e.cells {
                        seen[c.param] = true;
                    }
                }
            }
        }
        e.select_limiter();
        for p in panel_states(4) {
            e.open_panel(p);
            for c in &e.cells {
                seen[c.param] = true;
            }
        }
        let missing: Vec<&str> = seen.iter().enumerate().filter(|(_, s)| !**s).map(|(i, _)| INFO.params[i].id).collect();
        assert!(missing.is_empty(), "no control for {missing:?}");
    }

    #[test]
    fn controls_stay_inside_window_and_do_not_overlap() {
        let mut e = MultibandEditor::new(EditorHost::detached());
        let win: R = (0.0, 0.0, WIDTH as f32, HEIGHT as f32);
        let within = |c: &R, r: R| c.0 >= r.0 && c.1 >= r.1 && c.0 + c.2 <= r.0 + r.2 && c.1 + c.3 <= r.1 + r.3;
        let ov = |a: &R, b: &R| a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3;

        for band_count in [1usize, 4, 6] {
            e.param_changed(P_BAND_COUNT, (band_count - 1) as f32);
            for sec in 0..BAND_SECTIONS {
                e.select_band(0);
                e.select_sec(sec);
                for p in panel_states(sec) {
                    e.open_panel(p);
                    let n = e.cells.len() - e.panel_cells;
                    for c in &e.cells {
                        assert!(within(&c.rect, win), "bc={band_count} sec={sec} p={p:?}: {} outside window {:?}", c.label, c.rect);
                    }
                    if let Some(p) = p {
                        let r = panel_rect(sec, p);
                        assert!(within(&r, DISPLAY), "sec={sec} panel={p}: {r:?} leaves DISPLAY");
                        for c in &e.cells[n..] {
                            assert!(within(&c.rect, r), "sec={sec} panel={p}: {} at {:?} outside {r:?}", c.label, c.rect);
                        }
                    }
                    // No pair of controls overlaps.
                    for (i, a) in e.cells.iter().enumerate() {
                        for b in &e.cells[i + 1..] {
                            assert!(!ov(&a.rect, &b.rect), "bc={band_count} sec={sec} p={p:?}: {} {:?} overlaps {} {:?}", a.label, a.rect, b.label, b.rect);
                        }
                    }
                }
            }
            e.select_limiter();
            for p in panel_states(4) {
                e.open_panel(p);
                for (i, a) in e.cells.iter().enumerate() {
                    for b in &e.cells[i + 1..] {
                        assert!(!ov(&a.rect, &b.rect), "limiter p={p:?}: {} overlaps {}", a.label, b.label);
                    }
                }
            }
        }
    }

    #[test]
    fn text_is_whole_apart_and_readable() {
        use goodlookindri_shell::text;
        use openplugin::dri::goodlookindri::scene::Cmd;
        let mut bad = Vec::new();
        for band_count in [1usize, 4, 6] {
            for target_lim in [false, true] {
                let secs: Vec<usize> = if target_lim { vec![4] } else { (0..BAND_SECTIONS).collect() };
                for sec in secs {
                    for p in panel_states(sec) {
                        for v3 in [false, true] {
                            let shared = Arc::new(Shared::default());
                            let mut e = MultibandEditor::new(EditorHost::detached());
                            e.bind(Some(shared.clone() as EditorState));
                            e.param_changed(P_BAND_COUNT, (band_count - 1) as f32);
                            e.param_changed(P_AUTO_GAIN, 1.0);
                            if target_lim {
                                e.select_limiter();
                            } else {
                                e.select_band(0);
                                e.select_sec(sec);
                            }
                            e.open_panel(p);
                            e.set_view3d(v3);
                            for _ in 0..3 {
                                for b in 0..band_count {
                                    shared.band_section(b, sec.min(3), [-0.04, -0.02], [-20.0, -21.0], true);
                                    shared.band_summary(b, -3.2, -18.0);
                                }
                                shared.limiter([-1.5, -1.2], [-2.0, -2.1], true);
                                shared.peaks([0.5, 0.4], [0.3, 0.25]);
                                shared.set_auto_gain(-12.6);
                                shared.set_key_present(true);
                                e.tick(1.0 / 30.0);
                            }
                            let mut s = Scene::new();
                            e.paint_layer(&mut s);
                            e.paint_live(&mut s);
                            let key = format!("bc={band_count}/sec={sec}/p={p:?}/3d={v3}");
                            let mut clips: Vec<(f32, f32, f32, f32)> = vec![];
                            let mut texts: Vec<(f32, f32, f32, f32, String, f32)> = vec![];
                            for c in &s.cmds {
                                match c {
                                    Cmd::ClipRound { rect, .. } => clips.push((rect.x, rect.y, rect.x + rect.w, rect.y + rect.h)),
                                    Cmd::PopClip => {
                                        clips.pop();
                                    }
                                    Cmd::Text { at, text: t, size, center, .. } => {
                                        let w = text::width(t, *size);
                                        let x0 = if *center { at.x - w * 0.5 } else { at.x };
                                        let (y0, y1) = (at.y - size * 0.36, at.y + size * 0.36);
                                        if let Some(cl) = clips.iter().fold(None::<(f32, f32, f32, f32)>, |a, c| Some(a.map_or(*c, |a| (a.0.max(c.0), a.1.max(c.1), a.2.min(c.2), a.3.min(c.3))))) {
                                            if x0 < cl.0 - 0.5 || x0 + w > cl.2 + 0.5 || y0 < cl.1 - 0.5 || y1 > cl.3 + 0.5 {
                                                bad.push(format!("CLIPPED {key}: {t:?} size {size}"));
                                            }
                                        }
                                        if *size < 8.0 {
                                            bad.push(format!("SMALL {key}: {t:?} size {size}"));
                                        }
                                        if t.contains("-0.0") || t.trim_start().starts_with("-0 ") || t == "-0" {
                                            bad.push(format!("MINUS ZERO {key}: {t:?}"));
                                        }
                                        texts.push((x0, y0, x0 + w, y1, t.clone(), *size));
                                    }
                                    _ => {}
                                }
                            }
                            for i in 0..texts.len() {
                                for j in i + 1..texts.len() {
                                    let (a, b) = (&texts[i], &texts[j]);
                                    // Skip text under an open panel overlay (the panel covers DISPLAY labels behind it).
                                    if p.is_some() {
                                        let pr = panel_rect(sec, p.unwrap());
                                        let in_pr = |r: &(f32, f32, f32, f32, String, f32)| r.0 < pr.0 + pr.2 && pr.0 < r.2 && r.1 < pr.1 + pr.3 && pr.1 < r.3;
                                        if in_pr(a) && in_pr(b) && (a.4.starts_with("CURVE") || b.4.starts_with("CURVE") || a.4.starts_with("3D") || b.4.starts_with("3D") || a.4.starts_with("QUIET") || b.4.starts_with("QUIET")) {
                                            continue;
                                        }
                                    }
                                    if a.0 < b.2 - 0.5 && b.0 < a.2 - 0.5 && a.1 < b.3 - 0.5 && b.1 < a.3 - 0.5 && a.4 != b.4 && !a.4.trim().is_empty() && !b.4.trim().is_empty() {
                                        bad.push(format!("OVERLAP {key}: {:?} ({}) and {:?} ({})", a.4, a.5, b.4, b.5));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }

    #[test]
    fn meters_read_and_fall_as_specified() {
        for hz in [30.0f32, 60.0] {
            let shared = Arc::new(Shared::default());
            let mut e = MultibandEditor::new(EditorHost::detached());
            e.bind(Some(shared.clone() as EditorState));
            shared.peaks([1.0, 0.5012], [0.5012, 1.0]);
            e.tick(1.0 / hz);
            assert!(e.meters[0][0].abs() < 0.01 && (e.meters[0][1] + 6.0).abs() < 0.01, "{:?}", e.meters);
            assert!((e.meters[1][0] + 6.0).abs() < 0.01 && e.meters[1][1].abs() < 0.01);
            let ticks = hz as usize;
            for _ in 0..ticks {
                e.tick(1.0 / hz);
            }
            assert!((e.meters[0][0] + 24.0).abs() < 0.1, "fall after 1s at {hz}Hz: {}", e.meters[0][0]);
        }
    }

    #[test]
    #[ignore]
    fn frame_cost() {
        let (w, h) = (WIDTH as f64, HEIGHT as f64);
        for (label, target_lim, sec, panel, v3) in [
            ("B1 Compressor", false, 1, None, false),
            ("B1 Sidechain", false, 1, Some(2), false),
            ("B1 Decomp 3D", false, 3, None, true),
            ("Master Limiter", true, 4, None, false),
        ] {
            let shared = Arc::new(Shared::default());
            let mut e = MultibandEditor::new(EditorHost::detached());
            e.bind(Some(shared.clone() as EditorState));
            e.param_changed(P_AUTO_GAIN, 1.0);
            if target_lim {
                e.select_limiter();
            } else {
                e.select_band(0);
                e.select_sec(sec);
            }
            e.open_panel(panel);
            e.set_view3d(v3);
            let mut seed = 11u32;
            for f in 0..TRACE_LEN {
                let g = -6.0 - 5.0 * (f as f32 * 0.4).sin();
                for b in 0..4 {
                    shared.band_section(b, 1, [g, g * 0.9], [-20.0 + g, -22.0 + g], true);
                    shared.band_summary(b, g * 0.6, -18.0);
                }
                shared.limiter([-2.0, -1.8], [-3.0, -3.2], true);
                let pre: Vec<f32> = (0..1600)
                    .map(|_| {
                        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        0.3 * ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5)
                    })
                    .collect();
                let post: Vec<f32> = pre.iter().map(|v| v * 10f32.powf(g / 20.0)).collect();
                shared.push_taps([&pre, &pre], [&post, &post]);
                e.tick(1.0 / 30.0);
            }
            let _ = e.render(2.0);
            let (mut time, frames) = (std::time::Duration::ZERO, 30);
            for f in 0..frames {
                let g = -6.0 - 5.0 * (f as f32 * 0.4).sin();
                for b in 0..4 {
                    shared.band_section(b, 1, [g, g * 0.9], [-20.0 + g, -22.0 + g], true);
                    shared.band_summary(b, g * 0.6, -18.0);
                }
                shared.peaks([0.5, 0.4], [0.3, 0.25]);
                shared.set_auto_gain(-3.0);
                e.tick(1.0 / 30.0);
                let t0 = std::time::Instant::now();
                let _ = e.render(2.0);
                time += t0.elapsed();
            }
            println!("{label:18} ({w:.0}x{h:.0} @2x): {:6.2} ms/frame", time.as_secs_f64() * 1000.0 / frames as f64);
        }
    }

    #[test]
    #[ignore]
    fn display_table() {
        let mut e = MultibandEditor::new(EditorHost::detached());
        e.param_changed(P_BAND_COUNT, (MAX_BANDS - 1) as f32);
        let mut seen = std::collections::HashSet::new();
        for b in 0..1 {
            e.select_band(b);
            for sec in 0..BAND_SECTIONS {
                e.select_sec(sec);
                for p in panel_states(sec) {
                    e.open_panel(p);
                    for c in e.cells.clone() {
                        let Some(info) = INFO.params.get(c.param) else { continue };
                        if !seen.insert(c.param) || matches!(info.unit, openplugin::core::Unit::Toggle | openplugin::core::Unit::Choice) {
                            continue;
                        }
                        let mid = info.from_normalized(0.5);
                        println!("{:28} | {:>12} | {:>12} | {:>12}", info.id, c.text(info.min), c.text(mid), c.text(info.max));
                    }
                }
            }
        }
        e.select_limiter();
        for p in panel_states(4) {
            e.open_panel(p);
            for c in e.cells.clone() {
                let Some(info) = INFO.params.get(c.param) else { continue };
                if !seen.insert(c.param) || matches!(info.unit, openplugin::core::Unit::Toggle | openplugin::core::Unit::Choice) {
                    continue;
                }
                let mid = info.from_normalized(0.5);
                println!("{:28} | {:>12} | {:>12} | {:>12}", info.id, c.text(info.min), c.text(mid), c.text(info.max));
            }
        }
    }

    /// Screenshots with realistic multiband activity into `SHOTS` (`.ppm`). Run with `--ignored`.
    #[test]
    #[ignore]
    fn screenshots() {
        let dir = std::env::var("SHOTS").unwrap_or_else(|_| format!("{}/qa/2026-11-24-initial/screenshots", env!("CARGO_MANIFEST_DIR")));
        std::fs::create_dir_all(&dir).expect("dir");
        let scale: f32 = std::env::var("SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0);
        let cases = [
            ("4band-comp-dark", ThemeId::Dark, 4usize, false, 1usize, None, false, false),
            ("4band-comp-link-light", ThemeId::Light, 4, false, 1, None, false, true),
            ("4band-sidechain-dark", ThemeId::Dark, 4, false, 1, Some(2), false, false),
            ("4band-3d-dark", ThemeId::Dark, 4, false, 1, None, true, false),
            ("6band-decomp-graphite", ThemeId::Graphite, 6, false, 3, None, false, false),
            ("4band-limiter-dark", ThemeId::Dark, 4, true, 4, None, false, false),
            ("4band-deexp-dark", ThemeId::Dark, 4, false, 2, Some(0), false, false),
            ("1band-gate-paper", ThemeId::Paper, 1, false, 0, None, false, false),
        ];
        for (name, th, bc, target_lim, sec, panel, v3, linked) in cases {
            let shared = Arc::new(Shared::default());
            let mut e = MultibandEditor::new(EditorHost::detached());
            e.bind(Some(shared.clone() as EditorState));
            e.theme = th;
            e.band_link = linked;
            e.param_changed(P_BAND_COUNT, (bc - 1) as f32);
            e.param_changed(P_AUTO_GAIN, 1.0);
            if bc >= 4 {
                e.param_changed(LAYOUT.bands[0].gain, 2.0);
                e.param_changed(LAYOUT.bands[2].gain, -2.0);
            }
            if target_lim {
                e.select_limiter();
            } else {
                e.select_band(if bc >= 2 { 1 } else { 0 });
                e.select_sec(sec);
                e.param_changed(LAYOUT.bands[if bc >= 2 { 1 } else { 0 }].sections[sec].on, 1.0);
            }
            for f in 0..TRACE_LEN + 3 {
                let t = (f % 15) as f32;
                let lvl = -14.0 - 2.3 * t;
                let g = match sec {
                    2 | 3 => (6.0 - t * 0.6).max(0.0),
                    _ => -(9.0 - t * 0.7).max(0.0),
                };
                for b in 0..bc {
                    let bg = match b % 4 {
                        0 => -(8.0 - t * 0.6).max(0.0),
                        1 => -(5.0 - t * 0.4).max(0.0),
                        2 => (4.0 - t * 0.3).max(0.0),
                        _ => -(3.0 - t * 0.3).max(0.0),
                    };
                    shared.band_section(b, sec.min(3), [g, g * 0.92], [lvl, lvl - 1.5], true);
                    shared.band_summary(b, bg, lvl);
                }
                shared.limiter([-2.5, -2.2], [-2.0, -2.4], true);
                let pk = 10f32.powf(lvl / 20.0);
                shared.peaks([pk * 1.15, pk], [pk * 0.95, pk * 0.85]);
                shared.set_auto_gain(-2.0);
                let mut seed = 9u32 + f as u32;
                let pre: Vec<f32> = (0..800)
                    .map(|i| {
                        let n = (f * 800 + i) as f32 / 48_000.0;
                        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        let noise = (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
                        let tau = std::f32::consts::TAU;
                        pk * (0.45 * (tau * 80.0 * n).sin() + 0.28 * (tau * 440.0 * n).sin() + 0.16 * (tau * 2200.0 * n).sin() + 0.12 * noise)
                    })
                    .collect();
                let post: Vec<f32> = pre.iter().map(|v| v * 10f32.powf(g / 20.0)).collect();
                shared.push_taps([&pre, &pre], [&post, &post]);
                e.tick(1.0 / 30.0);
            }
            e.open_panel(panel);
            e.set_view3d(v3);
            e.damage.add_all();
            let f = e.render(scale);
            let mut out = format!("P6\n{} {}\n255\n", f.width, f.height).into_bytes();
            for px in f.pixels {
                out.extend_from_slice(&[(px >> 16) as u8, (px >> 8) as u8, *px as u8]);
            }
            std::fs::write(format!("{dir}/{name}-{scale:.0}x.ppm"), out).expect("write");
        }
    }
}
