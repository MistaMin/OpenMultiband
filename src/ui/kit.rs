//! The controls every page is built from, with the sizes of `design/shell.md` section 3.
//!
//! A page lists [`Cell`]s: what kind of control, which host parameter, where. The shell turns
//! each cell into an SDK control (which handles dragging, the wheel, double-click to type,
//! the host's parameter menu and accessibility) and paints it with the functions here.
//! Chrome that never moves is painted into the cached layer, the value on top of it.

#![allow(dead_code)] // the pages' toolbox: not every page uses every piece

use openplugin::core::Unit;
use openplugin::dri::goodlookindri::color::Color as C;
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};
use goodlookindri_shell::text;
use openplugin::dri::{Area, Control, ControlKind};

use super::raster::R;
use super::theme::Theme;
use crate::INFO;

/// Knob sizes: big 120 x 124, small 80 x 84, mini 56 x 60.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    Big,
    Small,
    Mini,
}

impl Size {
    pub const fn cell(self) -> (f32, f32) {
        match self {
            Size::Big => (120.0, 124.0),
            Size::Small => (80.0, 84.0),
            Size::Mini => (56.0, 60.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// A rotary knob. The rect is the whole cell (label, knob, value).
    Knob(Size),
    /// An on/off pill, h 22.
    Pill,
    /// A choice as keys side by side, h 24. Every choice of the parameter is a key.
    Segmented,
    /// A choice as a box that opens a list, h 24.
    Dropdown,
    /// A number in a box: vertical drag, double-click to type. 64 x 22 or as given.
    Value,
    /// A vertical fader (the input gains): drag along its height.
    FaderV,
    /// A horizontal slider (setup sheet): label left, value right.
    FaderH,
    /// Only a hit area: the page paints it (a grip in a graph, a handle on a curve).
    /// Vertical drag, like a fader.
    Grip,
    /// Only a hit area, horizontal drag.
    GripH,
}

/// One control on a page.
#[derive(Clone, Debug)]
pub struct Cell {
    pub kind: Kind,
    /// Index of the host parameter.
    pub param: usize,
    pub label: String,
    /// Window points. For a knob, the whole cell.
    pub rect: R,
    /// Drawn faded and does not take input: the setting does nothing right now.
    pub dim: bool,
    pub tip: Option<String>,
    /// How the value reads. `None`: the parameter's own text.
    pub format: Option<fn(f32) -> String>,
    /// For a segmented choice: the labels shown on the keys, if shorter than the choices.
    pub keys: Option<&'static [&'static str]>,
    /// A narrower range the editor allows than the parameter's (host automation and old
    /// sessions may still set the full range), e.g. a trim after a limiter that stops at 0 dB.
    pub gui: Option<(f32, f32)>,
    /// Override drag travel in points (e.g. a narrow crossover handle dragging across a wide plot).
    pub drag: Option<f32>,
}

impl Cell {
    pub fn new(kind: Kind, param: usize, label: impl Into<String>, rect: R) -> Self {
        Self { kind, param, label: label.into(), rect, dim: false, tip: None, format: None, keys: None, gui: None, drag: None }
    }

    pub fn knob(size: Size, param: usize, label: impl Into<String>, x: f32, y: f32) -> Self {
        let (w, h) = size.cell();
        Self::new(Kind::Knob(size), param, label, (x, y, w, h))
    }

    pub fn pill(param: usize, label: impl Into<String>, rect: R) -> Self {
        Self::new(Kind::Pill, param, label, rect)
    }

    pub fn segmented(param: usize, label: impl Into<String>, rect: R) -> Self {
        Self::new(Kind::Segmented, param, label, rect)
    }

    pub fn dropdown(param: usize, label: impl Into<String>, rect: R) -> Self {
        Self::new(Kind::Dropdown, param, label, rect)
    }

    pub fn value(param: usize, label: impl Into<String>, rect: R) -> Self {
        Self::new(Kind::Value, param, label, rect)
    }

    pub fn grip(param: usize, rect: R) -> Self {
        Self::new(Kind::Grip, param, "", rect)
    }

    pub fn grip_h(param: usize, rect: R) -> Self {
        Self::new(Kind::GripH, param, "", rect)
    }

    pub fn dim(mut self, dim: bool) -> Self {
        self.dim = dim;
        self
    }

    pub fn tip(mut self, tip: impl Into<String>) -> Self {
        self.tip = Some(tip.into());
        self
    }

    pub fn format(mut self, f: fn(f32) -> String) -> Self {
        self.format = Some(f);
        self
    }

    /// Keeps the editor's own changes inside `lo..=hi`.
    pub fn gui_range(mut self, lo: f32, hi: f32) -> Self {
        self.gui = Some((lo, hi));
        self
    }

    pub fn keys(mut self, keys: &'static [&'static str]) -> Self {
        self.keys = Some(keys);
        self
    }

    pub fn drag(mut self, px: f32) -> Self {
        self.drag = Some(px);
        self
    }

    /// What its drawing can change, with the soft edges around it.
    pub fn damage_rect(&self) -> R {
        let (x, y, w, h) = self.rect;
        (x - 3.0, y - 3.0, w + 6.0, h + 6.0)
    }

    /// The text of a value, at most three digits per number.
    pub fn text(&self, v: f32) -> String {
        let s = match self.format {
            Some(f) => f(v),
            None => INFO.params.get(self.param).map_or_else(String::new, |p| match p.unit {
                // Whole-step dB and ratio controls read as whole numbers, never "-0".
                Unit::Db | Unit::Ratio if p.steps > 1 && (p.max - p.min - (p.steps - 1) as f32).abs() < 1e-3 => {
                    let r = v.round();
                    let r = if r == 0.0 { 0.0 } else { r };
                    if p.unit == Unit::Db { format!("{r:.0} dB") } else { format!("{r:.0}:1") }
                }
                // No "0.x ms": under a millisecond reads in microseconds, zero as "0 ms".
                Unit::Ms if v < 1.0 => {
                    let us = (v * 1000.0).round();
                    if us < 0.5 { "0 ms".into() } else { format!("{us:.0} µs") }
                }
                _ => p.format(v),
            }),
        };
        three_digits(&s)
    }
}

/// Rewrites every number in `s` that has more than three digits to three: 1000 ms and up
/// become seconds, 1000 Hz and up kHz, anything else is rounded to three digits
/// ("3510.0 ms" -> "3.51 s", "17.74 kHz" -> "17.7 kHz", "-144.0 dB" -> "-144 dB").
pub fn three_digits(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit() {
            out.push(s[i..].chars().next().unwrap());
            i += s[i..].chars().next().unwrap().len_utf8();
            continue;
        }
        let start = i;
        while i < b.len() && (b[i].is_ascii_digit() || (b[i] == b'.' && i + 1 < b.len() && b[i + 1].is_ascii_digit())) {
            i += 1;
        }
        let token = &s[start..i];
        let count = token.bytes().filter(u8::is_ascii_digit).count();
        let Ok(mut v) = token.parse::<f64>() else {
            out.push_str(token);
            continue;
        };
        if count <= 3 {
            out.push_str(token);
            continue;
        }
        let rest = &s[i..];
        if v >= 1000.0 && rest.starts_with(" ms") {
            v /= 1000.0;
            out.push_str(&fit(v));
            out.push_str(" s");
            i += 3;
        } else if v >= 1000.0 && rest.starts_with(" Hz") {
            v /= 1000.0;
            out.push_str(&fit(v));
            out.push_str(" kHz");
            i += 3;
        } else {
            out.push_str(&fit(v));
        }
    }
    out
}

/// A non-negative number in three digits.
fn fit(v: f64) -> String {
    match v {
        v if v.round() >= 100.0 => format!("{v:.0}"),
        v if (v * 10.0).round() >= 100.0 => format!("{v:.1}"),
        v => format!("{v:.2}"),
    }
}

/// The knob's centre and radius, in window points. The value ring is drawn at
/// `r + ring + 2` (ring 4, 3, 2 pt wide), so its outer edge is `r + 1.5 ring + 2` from the
/// centre: big y+18..92 under a caption at y+0..14, small y+14..54 under its caption at
/// y+1..11, mini y+0..27 above its value (centre y+34) and caption (centre y+45), which
/// leaves 11 pt to a mini stacked below.
pub fn knob_circle(cell: &Cell, size: Size) -> (f32, f32, f32) {
    let (x, y, w, _) = cell.rect;
    match size {
        Size::Big => (x + w * 0.5, y + 59.0, 33.0),
        Size::Small => (x + w * 0.5, y + 37.0, 17.0),
        Size::Mini => (x + w * 0.5, y + 16.0, 11.0),
    }
}

/// The value box under a knob.
pub fn knob_value_box(cell: &Cell, size: Size) -> R {
    let (x, y, w, _) = cell.rect;
    match size {
        Size::Big => (x + (w - 84.0) * 0.5, y + 98.0, 84.0, 22.0),
        Size::Small => (x + (w - 64.0) * 0.5, y + 62.0, 64.0, 18.0),
        Size::Mini => (x + 2.0, y + 28.0, w - 4.0, 12.0),
    }
}

/// The SDK control for a cell.
pub fn control_for(cell: &Cell) -> Control {
    let info = &INFO.params[cell.param];
    let (x, y, w, h) = cell.rect;
    let c = match cell.kind {
        Kind::Knob(size) => {
            let (cx, cy, r) = knob_circle(cell, size);
            let reach = match size {
                Size::Big => 8.0,
                Size::Small => 6.0,
                Size::Mini => 4.0,
            };
            // The knob takes its value box too, so a double-click there types.
            let _ = (cx, cy, r);
            Control::knob(Area::rect(x, y + 12.0, w, h - 12.0), cell.param, info).drag_range(200.0 + reach - reach)
        }
        Kind::Pill => Control::toggle(Area::rect(x, y, w, h), cell.param, info),
        Kind::Segmented => Control::key_bank(Area::rect(x, y, w, h), cell.param, info, false),
        // The list is the shell's: the control only owns the box, so the host menu and
        // accessibility still see the parameter. A click is taken by the shell first.
        Kind::Dropdown => Control::key_bank(Area::rect(x, y, w, h), cell.param, info, false),
        Kind::Value | Kind::Grip | Kind::FaderV => Control::fader_v(Area::rect(x, y, w, h), cell.param, info).drag_range(if cell.kind == Kind::Value { 200.0 } else { h }),
        Kind::FaderH | Kind::GripH => {
            let range = cell.drag.unwrap_or_else(|| if cell.kind == Kind::FaderH { slider_track(cell).1 - slider_track(cell).0 } else { w });
            Control::fader_h(Area::rect(x, y, w, h), cell.param, info).drag_range(range)
        }
    };
    match &cell.tip {
        Some(t) => c.tooltip(t.clone()),
        None => c,
    }
}

/// An arc in short pieces: the toolkit measures a stroke over its whole box, so one long arc
/// costs many times its pieces (a 270 degree value ring on a big knob: 6 ms at 2x, in one).
pub fn arc(s: &mut Scene, c: Point, r: f32, from: f32, to: f32, colour: C, width: f32) {
    const STEP: f32 = 0.3;
    // Translucent pieces darken where their soft ends overlap: draw those in one (faded knobs).
    if colour.0 >> 24 < 0xff {
        s.stroke_arc(c, r, from, to, colour, width);
        return;
    }
    let n = ((to - from) / STEP).ceil().max(1.0) as usize;
    let d = (to - from) / n as f32;
    for i in 0..n {
        s.stroke_arc(c, r, from + d * i as f32, from + d * (i + 1) as f32, colour, width);
    }
}

/// The angle of a knob's pointer: a 270 degree sweep, open at the bottom.
pub fn knob_angle(normalized: f32) -> f32 {
    use std::f32::consts::PI;
    -1.25 * PI + normalized.clamp(0.0, 1.0) * 1.5 * PI
}

pub fn rect(r: R) -> Rect {
    Rect::new(r.0, r.1, r.2, r.3)
}

pub fn inside(r: R, x: f32, y: f32) -> bool {
    x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3
}

pub fn text_right(s: &mut Scene, right: f32, y: f32, label: &str, colour: C, size: f32) {
    s.text(Point::new(right - text::width(label, size), y), label, colour, size, false);
}

/// A caption: 10 pt caps, dim.
pub fn caption(s: &mut Scene, t: &Theme, x: f32, y: f32, label: &str) {
    s.text(Point::new(x, y), label, t.dim, 9.0, false);
}

/// A panel: rounded, filled, outlined.
pub fn panel(s: &mut Scene, t: &Theme, r: R) {
    s.fill_round(rect(r), 10.0, t.panel, t.panel, true);
    s.stroke_round(rect(r), 10.0, t.border, 1.0);
}

/// A plot's background, flat, with a hairline border.
pub fn plot(s: &mut Scene, t: &Theme, r: R) {
    s.fill_round(rect(r), 4.0, t.plot_bg, t.plot_bg, true);
    s.stroke_round(rect(r), 4.0, t.border, 1.0);
}

/// An inset box (value boxes, readouts).
pub fn inset(s: &mut Scene, t: &Theme, r: R, radius: f32) {
    s.fill_round(rect(r), radius, t.inset, t.inset, true);
    s.stroke_round(rect(r), radius, t.border, 1.0);
}

/// A button face with a centred label.
pub fn button(s: &mut Scene, t: &Theme, r: R, label: &str, on: bool) {
    let fill = if on { t.accent } else { t.panel };
    s.fill_round(rect(r), 6.0, fill, fill, true);
    s.stroke_round(rect(r), 6.0, if on { t.accent } else { t.border }, 1.0);
    s.text(rect(r).center(), label, if on { t.on_accent } else { t.text }, 10.0, true);
}

/// Chrome that never moves: knob faces and tracks, labels, boxes.
pub fn paint_static(s: &mut Scene, t: &Theme, cell: &Cell) {
    let (x, y, w, h) = cell.rect;
    let fade = if cell.dim { 0.4 } else { 1.0 };
    match cell.kind {
        Kind::Knob(size) => {
            let (cx, cy, r) = knob_circle(cell, size);
            let (ring, label_size, label_y) = match size {
                Size::Big => (4.0, 10.0, y + 7.0),
                Size::Small => (3.0, 9.0, y + 6.0),
                // Close under its value, well clear of a mini stacked below (EQ band cards).
                Size::Mini => (2.0, 8.5, y + 45.0),
            };
            let face = Scene::ellipse_at(Point::new(cx, cy), r);
            s.stroke_arc(Point::new(cx, cy), r + ring + 2.0, knob_angle(0.0), knob_angle(1.0), t.grid_strong.with_alpha(fade), ring);
            if size != Size::Mini {
                s.shadow_ellipse(face.translated(0.0, 2.0), 3.0, t.shadow);
            }
            s.fill_ellipse(face, t.knob_top, t.knob_bottom);
            s.stroke_ellipse(face, t.border, 1.0);
            let label = text::ellipsize(&cell.label.to_uppercase(), label_size, w - 4.0);
            s.text(Point::new(x + w * 0.5, label_y), label, t.dim.with_alpha(fade), label_size, true);
            if size != Size::Mini {
                let b = knob_value_box(cell, size);
                inset(s, t, b, 5.0);
            }
        }
        Kind::Value => inset(s, t, cell.rect, 5.0),
        Kind::Segmented | Kind::Dropdown => inset(s, t, cell.rect, 6.0),
        Kind::FaderV => {
            let cx = x + w * 0.5;
            s.fill_round(Rect::new(cx - 1.5, y, 3.0, h), 1.5, t.grid_strong, t.grid_strong, true);
        }
        Kind::FaderH => {
            let (x0, x1, yc) = slider_track(cell);
            s.text(Point::new(x, yc), &cell.label, t.dim, 9.0, false);
            s.fill_round(Rect::new(x0, yc - 1.5, x1 - x0, 3.0), 1.5, t.grid_strong, t.grid_strong, true);
        }
        Kind::Pill | Kind::Grip | Kind::GripH => {}
    }
}

/// Where a horizontal slider's track runs: `(x0, x1, y)`.
pub fn slider_track(cell: &Cell) -> (f32, f32, f32) {
    let (x, y, w, h) = cell.rect;
    // A fixed label column, so the tracks of a stack of sliders line up.
    let label_w = (text::width(&cell.label, 9.0) + 10.0).max(96.0);
    (x + label_w, x + w - 70.0, y + h * 0.5)
}

/// The value on top of the chrome. `value` is the plain value to show.
pub fn paint_value(s: &mut Scene, t: &Theme, cell: &Cell, c: &Control, value: f32, focused: bool) {
    let (x, y, w, h) = cell.rect;
    let lit = (c.hover() || c.pressed()) && !cell.dim;
    let info = c.info;
    let n = info.to_normalized(value);
    let fade = if cell.dim { 0.4 } else { 1.0 };
    let ink = if cell.dim { t.dim } else { t.text };
    match cell.kind {
        Kind::Knob(size) => {
            let (cx, cy, r) = knob_circle(cell, size);
            let ring = match size {
                Size::Big => 4.0,
                Size::Small => 3.0,
                Size::Mini => 2.0,
            };
            // A symmetric range fills from its centre, any other from its minimum.
            let origin = if info.min < 0.0 && info.max > 0.0 && (info.min + info.max).abs() < 1e-3 { 0.5 } else { 0.0 };
            if (n - origin).abs() > 0.004 {
                let colour = if cell.dim { t.dim.with_alpha(0.45) } else { t.accent };
                arc(s, Point::new(cx, cy), r + ring + 2.0, knob_angle(origin.min(n)), knob_angle(origin.max(n)), colour, ring);
            }
            if lit {
                s.stroke_ellipse(Scene::ellipse_at(Point::new(cx, cy), r), t.accent.with_alpha(0.55), 1.5);
            }
            let a = knob_angle(n);
            let (dx, dy) = (a.cos(), a.sin());
            let width = match size {
                Size::Big => 3.0,
                Size::Small => 2.4,
                Size::Mini => 1.8,
            };
            s.line(Point::new(cx + dx * r * 0.42, cy + dy * r * 0.42), Point::new(cx + dx * r * 0.86, cy + dy * r * 0.86), ink, width);
            let b = knob_value_box(cell, size);
            let size_pt = match size {
                Size::Big => 12.0,
                Size::Small => 10.5,
                Size::Mini => 9.5,
            };
            let shown = text::ellipsize(&cell.text(value), size_pt, b.2 - 4.0);
            s.text(Point::new(b.0 + b.2 * 0.5, b.1 + b.3 * 0.5), shown, ink, size_pt, true);
            if focused {
                s.stroke_ellipse(Scene::ellipse_at(Point::new(cx, cy), r + ring + 5.0), t.accent.with_alpha(0.7), 1.5);
            }
        }
        Kind::Value => {
            let shown = text::ellipsize(&cell.text(value), 11.0, w - 6.0);
            s.text(Point::new(x + w * 0.5, y + h * 0.5), shown, ink, 11.0, true);
            if !cell.dim {
                let origin: f32 = if info.min < 0.0 && info.max > 0.0 && (info.min + info.max).abs() < 1e-3 { 0.5 } else { 0.0 };
                let x0 = x + 4.0 + (w - 8.0) * origin.min(n);
                let bw = (w - 8.0) * (n - origin).abs();
                if bw > 0.5 {
                    s.fill_round(Rect::new(x0, y + h - 3.0, bw, 2.0), 1.0, t.accent, t.accent, true);
                }
            }
            if lit || focused {
                s.stroke_round(rect(cell.rect), 5.0, t.accent.with_alpha(0.7), 1.5);
            }
        }
        Kind::Pill => {
            let on = value >= (info.min + info.max) * 0.5 && !cell.dim;
            let r = rect(cell.rect);
            let radius = h * 0.5;
            if on {
                s.fill_round(r, radius, t.accent, t.accent, true);
                if lit {
                    s.stroke_round(r, radius, t.on_accent.with_alpha(0.5), 1.5);
                }
            } else {
                s.fill_round(r, radius, t.inset, t.inset, true);
                s.stroke_round(r, radius, if lit || focused { t.accent } else { t.border }, if lit || focused { 1.5 } else { 1.0 });
            }
            let colour = if on { t.on_accent } else { ink };
            let label = text::ellipsize(&cell.label.to_uppercase(), 9.5, w - 10.0);
            s.text(r.center(), label, colour.with_alpha(if on { 1.0 } else { fade }), 9.5, true);
        }
        Kind::Segmented => {
            let r = rect(cell.rect);
            if lit || focused {
                s.stroke_round(r, 6.0, t.accent.with_alpha(0.6), 1.0);
            }
            let labels = segment_labels(cell);
            let count = labels.len().max(1);
            let kw = r.w / count as f32;
            let chosen = segment_index(cell, value);
            for (i, label) in labels.iter().enumerate() {
                let key = Rect::new(r.x + kw * i as f32 + 2.0, r.y + 2.0, kw - 4.0, r.h - 4.0);
                let on = i == chosen && !cell.dim;
                if on {
                    s.fill_round(key, 4.0, t.accent, t.accent, true);
                }
                let label = text::ellipsize(label, 9.0, kw - 6.0);
                s.text(key.center(), label, if on { t.on_accent } else { t.dim }, 9.0, true);
            }
        }
        Kind::Dropdown => {
            let shown = text::ellipsize(&cell.text(value), 10.5, w - 28.0);
            s.text(Point::new(x + 10.0, y + h * 0.5), shown, ink, 10.5, false);
            let (cx, cy) = (x + w - 12.0, y + h * 0.5);
            s.stroke_poly(vec![Point::new(cx - 4.0, cy - 2.0), Point::new(cx, cy + 2.0), Point::new(cx + 4.0, cy - 2.0)], t.dim, 1.5, false);
            if lit || focused {
                s.stroke_round(rect(cell.rect), 6.0, t.accent.with_alpha(0.7), 1.5);
            }
        }
        Kind::FaderV => {
            let cx = x + w * 0.5;
            let ty = y + h * (1.0 - n);
            let origin = if info.min < 0.0 && info.max > 0.0 { info.to_normalized(0.0) } else { 0.0 };
            let oy = y + h * (1.0 - origin);
            let (a, b) = (ty.min(oy), ty.max(oy));
            if b - a > 0.5 {
                s.fill_round(Rect::new(cx - 1.5, a, 3.0, b - a), 1.5, t.accent, t.accent, true);
            }
            let thumb = Rect::new(x, ty - 6.0, w, 12.0);
            s.shadow_round(thumb.translated(0.0, 1.0), 4.0, 2.0, t.shadow);
            s.fill_round(thumb, 4.0, t.knob_top, t.knob_bottom, false);
            s.stroke_round(thumb, 4.0, if lit || focused { t.accent } else { t.border }, if lit { 1.8 } else { 1.0 });
            s.line(Point::new(x + 4.0, ty), Point::new(x + w - 4.0, ty), t.text, 1.2);
        }
        Kind::FaderH => {
            let (x0, x1, yc) = slider_track(cell);
            let origin = if info.min < 0.0 && info.max > 0.0 { info.to_normalized(0.0) } else { 0.0 };
            let (a, b) = (x0 + (x1 - x0) * origin.min(n), x0 + (x1 - x0) * origin.max(n));
            if b - a > 0.5 {
                let fill = if cell.dim { t.dim } else { t.accent };
                s.fill_round(Rect::new(a, yc - 1.5, b - a, 3.0), 1.5, fill, fill, true);
            }
            let thumb = Scene::ellipse_at(Point::new(x0 + (x1 - x0) * n, yc), 6.5);
            s.fill_ellipse(thumb, t.knob_top, t.knob_bottom);
            s.stroke_ellipse(thumb, if lit || focused { t.accent } else { t.border }, if lit { 1.8 } else { 1.0 });
            text_right(s, x + w, yc, &cell.text(value), ink, 10.5);
        }
        Kind::Grip | Kind::GripH => {}
    }
    let _ = (C(0), Unit::Db, ControlKind::Knob);
}

/// The values a segmented cell's keys stand for: every choice of a choice parameter, or
/// every step of a stepped one (e.g. 0, 6, 12, 18, 24 dB).
pub fn segment_values(cell: &Cell) -> Vec<f32> {
    let info = &INFO.params[cell.param];
    if info.steps > 1 && !matches!(info.unit, Unit::Choice | Unit::Toggle) {
        let n = info.steps as usize;
        (0..n).map(|i| info.min + (info.max - info.min) * i as f32 / (n - 1) as f32).collect()
    } else {
        let n = (info.max - info.min).round().max(0.0) as usize + 1;
        (0..n).map(|i| info.min + i as f32).collect()
    }
}

/// The labels on a segmented cell's keys.
pub fn segment_labels(cell: &Cell) -> Vec<String> {
    if let Some(k) = cell.keys {
        return k.iter().map(|s| (*s).to_string()).collect();
    }
    let info = &INFO.params[cell.param];
    if !info.choices.is_empty() {
        return info.choices.iter().map(|s| (*s).to_string()).collect();
    }
    segment_values(cell).into_iter().map(|v| cell.text(v)).collect()
}

/// The key a value lights.
pub fn segment_index(cell: &Cell, value: f32) -> usize {
    let values = segment_values(cell);
    values.iter().enumerate().min_by(|a, b| (a.1 - value).abs().total_cmp(&(b.1 - value).abs())).map_or(0, |(i, _)| i)
}

/// The value of the key under `x`, for a stepped (not a choice) parameter whose keys the
/// SDK's key bank cannot map (it takes one key per whole unit).
pub fn segment_click(cell: &Cell, x: f32) -> Option<f32> {
    let info = &INFO.params[cell.param];
    if cell.kind != Kind::Segmented || info.steps <= 1 || matches!(info.unit, Unit::Choice | Unit::Toggle) {
        return None;
    }
    let values = segment_values(cell);
    let i = (((x - cell.rect.0) / cell.rect.2 * values.len() as f32).floor().max(0.0) as usize).min(values.len() - 1);
    Some(values[i])
}

/// The choices a dropdown lists.
pub fn choices(cell: &Cell) -> Vec<String> {
    let info = &INFO.params[cell.param];
    let n = (info.max - info.min).round() as usize + 1;
    (0..n).map(|i| info.format(info.min + i as f32)).collect()
}

/// A dropdown's open list.
#[derive(Clone, Debug)]
pub struct Popover {
    pub param: usize,
    pub items: Vec<String>,
    /// Window points of the list.
    pub rect: R,
    pub row: f32,
    pub chosen: usize,
    pub hover: Option<usize>,
    /// The first row shown, for lists taller than the window.
    pub scroll: usize,
    pub visible: usize,
}

impl Popover {
    pub const ROW: f32 = 24.0;

    /// A dropdown's list of its parameter's choices.
    pub fn open(cell: &Cell, value: f32, bounds: R) -> Self {
        let info = &INFO.params[cell.param];
        let chosen = (value - info.min).round().max(0.0) as usize;
        let mut p = Self::list(choices(cell), cell.rect, chosen, cell.rect.2, bounds);
        p.param = cell.param;
        p
    }

    /// A list of `items` under (or, if there is more room, above) the box `anchor`, kept
    /// inside `bounds`. `chosen` is marked (`usize::MAX`: none).
    pub fn list(items: Vec<String>, anchor: R, chosen: usize, min_w: f32, bounds: R) -> Self {
        let width = items.iter().map(|s| text::width(s, 10.5) + 40.0).fold(min_w.max(anchor.2), f32::max).min(bounds.2 - 16.0);
        let (top, bottom) = (bounds.1, bounds.1 + bounds.3);
        let room_below = bottom - (anchor.1 + anchor.3) - 4.0;
        let room_above = anchor.1 - top - 4.0;
        let room = room_below.max(room_above);
        let visible = items.len().min(((room - 8.0) / Self::ROW).floor().max(1.0) as usize);
        let h = visible as f32 * Self::ROW + 8.0;
        let y = if h <= room_below || room_below >= room_above { anchor.1 + anchor.3 + 2.0 } else { anchor.1 - h - 2.0 };
        let x = anchor.0.min(bounds.0 + bounds.2 - width - 8.0).max(bounds.0 + 8.0);
        let scroll = if chosen == usize::MAX { 0 } else { chosen.saturating_sub(visible / 2).min(items.len().saturating_sub(visible)) };
        Self { param: usize::MAX, items, rect: (x, y, width, h), row: Self::ROW, chosen, hover: None, scroll, visible }
    }

    /// The item under a point.
    pub fn item_at(&self, x: f32, y: f32) -> Option<usize> {
        if !inside(self.rect, x, y) {
            return None;
        }
        let row = ((y - self.rect.1 - 4.0) / self.row).floor();
        (row >= 0.0 && (row as usize) < self.visible).then(|| self.scroll + row as usize).filter(|&i| i < self.items.len())
    }

    pub fn wheel(&mut self, dy: f32) {
        let max = self.items.len().saturating_sub(self.visible);
        let step = if dy > 0.0 { -1 } else { 1 };
        self.scroll = (self.scroll as isize + step).clamp(0, max as isize) as usize;
    }

    pub fn paint(&self, s: &mut Scene, t: &Theme) {
        let r = rect(self.rect);
        s.shadow_round(r.translated(0.0, 3.0), 8.0, 8.0, t.shadow);
        s.fill_round(r, 8.0, t.panel, t.panel, true);
        s.stroke_round(r, 8.0, t.border, 1.0);
        for row in 0..self.visible {
            let i = self.scroll + row;
            let Some(item) = self.items.get(i) else { break };
            let y = self.rect.1 + 4.0 + row as f32 * self.row;
            let band = Rect::new(self.rect.0 + 4.0, y, self.rect.2 - 8.0, self.row);
            if self.hover == Some(i) {
                s.fill_round(band, 5.0, t.accent.with_alpha(0.14), t.accent.with_alpha(0.14), true);
            }
            if i == self.chosen {
                let c = Point::new(self.rect.0 + 16.0, y + self.row * 0.5);
                s.fill_ellipse_solid(Scene::ellipse_at(c, 3.0), t.accent);
            }
            s.text(Point::new(self.rect.0 + 28.0, y + self.row * 0.5), text::ellipsize(item, 10.5, self.rect.2 - 36.0), t.text, 10.5, false);
        }
        if self.scroll > 0 {
            s.text(Point::new(self.rect.0 + self.rect.2 - 14.0, self.rect.1 + 10.0), "^", t.dim, 9.0, true);
        }
        if self.scroll + self.visible < self.items.len() {
            s.text(Point::new(self.rect.0 + self.rect.2 - 14.0, self.rect.1 + self.rect.3 - 10.0), "v", t.dim, 9.0, true);
        }
    }
}

// ---- pieces pages draw with ----

/// Log frequency axis: x of `f` inside `[x0, x1]` from `lo` to `hi` Hz.
pub fn freq_x(f: f32, x0: f32, x1: f32, lo: f32, hi: f32) -> f32 {
    x0 + (x1 - x0) * ((f.max(1.0) / lo).ln() / (hi / lo).ln())
}

/// The frequency at `x`, the inverse of [`freq_x`].
pub fn x_freq(x: f32, x0: f32, x1: f32, lo: f32, hi: f32) -> f32 {
    lo * (hi / lo).powf(((x - x0) / (x1 - x0)).clamp(0.0, 1.0))
}

/// A frequency grid with decade labels inside `plot`, `lo..hi` Hz, and dB lines every `step`
/// from `top` down `span` dB.
#[allow(clippy::too_many_arguments)]
pub fn freq_grid(s: &mut Scene, t: &Theme, plot: R, lo: f32, hi: f32, top: f32, span: f32, step: f32) {
    let (x0, y0, w, h) = plot;
    let x1 = x0 + w;
    let mut decade = 10f32.powf(lo.log10().floor());
    while decade <= hi {
        for m in 1..10 {
            let f = decade * m as f32;
            if f < lo || f > hi {
                continue;
            }
            let x = freq_x(f, x0, x1, lo, hi);
            let strong = m == 1;
            s.line(Point::new(x, y0), Point::new(x, y0 + h), if strong { t.grid_strong } else { t.grid }, 1.0);
            if strong || m == 2 || m == 5 {
                let label = if f >= 1000.0 { format!("{}k", f / 1000.0) } else { format!("{f}") };
                s.text(Point::new(x, y0 + h + 10.0), label, t.dim, 8.5, true);
            }
        }
        decade *= 10.0;
    }
    let mut level = top;
    while level >= top - span - 1e-3 {
        let y = y0 + (top - level) / span * h;
        s.line(Point::new(x0, y), Point::new(x1, y), if level.abs() < 1e-3 { t.grid_strong } else { t.grid }, 1.0);
        text_right(s, x0 - 4.0, y, &format!("{level:+.0}"), t.dim, 8.5);
        level -= step;
    }
}

/// A vertical gain-reduction meter (14 wide bar plus a 22 wide scale), filling down from 0
/// at the top over `range_db`. `hold` is drawn as a line. Live.
pub fn gr_meter(s: &mut Scene, t: &Theme, r: R, reduction_db: f32, hold_db: f32, range_db: f32) {
    let (x, y, _, h) = r;
    s.fill_round(Rect::new(x, y, 14.0, h), 3.0, t.seg_off, t.seg_off, true);
    let depth = (reduction_db / range_db).clamp(0.0, 1.0) * h;
    if depth > 0.5 {
        s.fill_round(Rect::new(x, y, 14.0, depth), 3.0, t.accent, t.accent, true);
    }
    let hold = (hold_db / range_db).clamp(0.0, 1.0) * h;
    if hold > 1.0 {
        s.line(Point::new(x, y + hold), Point::new(x + 14.0, y + hold), t.text, 1.5);
    }
}

/// The scale beside a [`gr_meter`]: ticks every 3 dB, labels every 6. Static.
pub fn gr_scale(s: &mut Scene, t: &Theme, r: R, range_db: f32) {
    let (x, y, _, h) = r;
    let mut d = 0.0;
    while d <= range_db + 1e-3 {
        let yy = y + d / range_db * h;
        s.line(Point::new(x + 16.0, yy), Point::new(x + 19.0, yy), t.grid_strong, 1.0);
        if (d as i32) % 6 == 0 {
            s.text(Point::new(x + 21.0, yy), format!("{d:.0}"), t.dim, 8.0, false);
        }
        d += 3.0;
    }
}

/// Segments per piece of a long polyline. The toolkit measures every segment of a shape at
/// every pixel of its box, so a 400-point curve across a plot costs 400 tests per pixel;
/// short pieces keep each box, and its cost, small.
const PIECE: usize = 8;

/// A polyline through `points`, clipped to nothing: the caller keeps it in its plot.
pub fn curve(s: &mut Scene, points: Vec<Point>, c: C, width: f32) {
    if points.len() < 2 {
        return;
    }
    for start in (0..points.len() - 1).step_by(PIECE) {
        s.stroke_poly(points[start..=(start + PIECE).min(points.len() - 1)].to_vec(), c, width, false);
    }
}

/// A filled area between a polyline and a horizontal base line at `base_y`.
pub fn area(s: &mut Scene, points: &[Point], base_y: f32, c: C) {
    columns(s, points, None, base_y, c);
}

/// A filled area between two polylines.
pub fn between(s: &mut Scene, upper: &[Point], lower: &[Point], c: C) {
    columns(s, upper, Some(lower), 0.0, c);
}

/// The y of a polyline sorted by x at `x`; `i` walks forward with x.
fn y_at(pts: &[Point], i: &mut usize, x: f32) -> f32 {
    while *i + 2 < pts.len() && pts[*i + 1].x < x {
        *i += 1;
    }
    let (a, b) = (pts[*i], pts[*i + 1]);
    if b.x <= a.x {
        return b.y;
    }
    a.y + (b.y - a.y) * ((x - a.x) / (b.x - a.x)).clamp(0.0, 1.0)
}

/// Fills between `upper` and `lower` (or the line `base_y`) one device-pixel column at a time:
/// each column is a small rectangle on whole pixels, so neighbours meet without a seam and the
/// cost is the area filled. Points not sorted by x fall back to one polygon.
fn columns(s: &mut Scene, upper: &[Point], lower: Option<&[Point]>, base_y: f32, c: C) {
    // A trace drawn newest-first runs right to left: take it the other way round.
    let forward = |p: &[Point]| -> Vec<Point> { if p.len() >= 2 && p[0].x > p[p.len() - 1].x { p.iter().rev().copied().collect() } else { p.to_vec() } };
    let (upper, lower) = (&forward(upper)[..], lower.map(forward));
    let lower = lower.as_deref();
    let sorted = |p: &[Point]| p.len() >= 2 && p.windows(2).all(|w| w[1].x >= w[0].x);
    if !sorted(upper) || lower.is_some_and(|l| !sorted(l)) {
        let mut poly = Vec::with_capacity(upper.len() + 2);
        match lower {
            Some(l) if upper.len() >= 2 && l.len() >= 2 => {
                poly.extend_from_slice(upper);
                poly.extend(l.iter().rev());
            }
            None if upper.len() >= 2 => {
                poly.push(Point::new(upper[0].x, base_y));
                poly.extend_from_slice(upper);
                poly.push(Point::new(upper[upper.len() - 1].x, base_y));
            }
            _ => return,
        }
        s.fill_poly(poly, c, c);
        return;
    }
    let k = openplugin::dri::goodlookindri::hardware::device_scale().max(1.0);
    let (mut x0, mut x1) = (upper[0].x, upper[upper.len() - 1].x);
    if let Some(l) = lower {
        (x0, x1) = (x0.max(l[0].x), x1.min(l[l.len() - 1].x));
    }
    let (mut i, mut j) = (0, 0);
    let spans: Vec<(f32, f32)> = ((x0 * k).round() as i64..(x1 * k).round() as i64)
        .map(|col| {
            let x = (col as f32 + 0.5) / k;
            let a = y_at(upper, &mut i, x);
            let b = lower.map_or(base_y, |l| y_at(l, &mut j, x));
            (a.min(b), a.max(b))
        })
        .collect();
    let first = (x0 * k).round() as i64;
    let rect = |s: &mut Scene, col: i64, n: usize, top: f32, bottom: f32| {
        if bottom - top > 1e-3 {
            s.fill_rect(Rect::new(col as f32 / k, top, n as f32 / k, bottom - top), c, c, true);
        }
    };
    // Eight columns share one rectangle for the part they all cover, snapped to whole pixel
    // rows; each column adds only its own ragged ends. The toolkit evaluates a box a pixel
    // wider on each side, so one rectangle per column would cost three times the area.
    for (n, chunk) in spans.chunks(8).enumerate() {
        let col0 = first + 8 * n as i64;
        let top = (chunk.iter().fold(f32::MIN, |m, c| m.max(c.0)) * k).ceil() / k;
        let bottom = (chunk.iter().fold(f32::MAX, |m, c| m.min(c.1)) * k).floor() / k;
        if bottom - top < 2.0 / k {
            for (m, &(t, b)) in chunk.iter().enumerate() {
                rect(s, col0 + m as i64, 1, t, b);
            }
            continue;
        }
        rect(s, col0, chunk.len(), top, bottom);
        for (m, &(t, b)) in chunk.iter().enumerate() {
            rect(s, col0 + m as i64, 1, t, top);
            rect(s, col0 + m as i64, 1, bottom, b);
        }
    }
}

/// A dashed horizontal line.
#[allow(clippy::too_many_arguments)]
pub fn dashed_h(s: &mut Scene, x0: f32, x1: f32, y: f32, on: f32, off: f32, c: C, width: f32) {
    let mut x = x0;
    while x < x1 {
        s.line(Point::new(x, y), Point::new((x + on).min(x1), y), c, width);
        x += on + off;
    }
}

/// A dashed polyline (approximate: dashes along each segment).
pub fn dashed(s: &mut Scene, points: &[Point], on: f32, off: f32, c: C, width: f32) {
    let mut carry = 0.0f32;
    let mut drawing = true;
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let len = (b.x - a.x).hypot(b.y - a.y);
        if len <= 0.0 {
            continue;
        }
        let mut pos = 0.0;
        while pos < len {
            let left = if drawing { on } else { off } - carry;
            let step = left.min(len - pos);
            if drawing {
                let p = |d: f32| Point::new(a.x + (b.x - a.x) * d / len, a.y + (b.y - a.y) * d / len);
                s.line(p(pos), p(pos + step), c, width);
            }
            pos += step;
            carry += step;
            if carry >= if drawing { on } else { off } - 1e-4 {
                carry = 0.0;
                drawing = !drawing;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goodlookindri_shell::raster::Framebuffer;

    /// Correctness gate (plugin-gui-performance): column fill matches fill_poly (mean channel
    /// difference < 0.5) and shows no seams inside a flat translucent band.
    #[test]
    fn column_fill_matches_the_polygon() {
        let (w, h, scale) = (240usize, 120usize, 2.0f32);
        let (fw, fh) = ((w as f32 * scale) as usize, (h as f32 * scale) as usize);
        let pts: Vec<Point> = (0..48).map(|i| {
            let t = i as f32 / 47.0;
            Point::new(20.0 + 200.0 * t, 40.0 + 25.0 * (t * 6.0).sin())
        }).collect();
        let base_y = 100.0f32;
        let col = C(0x802e_8bff);
        let bg = C(0xff10_1216);

        openplugin::dri::goodlookindri::hardware::set_device_scale(scale);
        let mut s_col = Scene::new();
        area(&mut s_col, &pts, base_y, col);
        let mut fb_col = Framebuffer::new(fw, fh);
        fb_col.clear(bg);
        fb_col.replay(&s_col, scale);

        let mut poly = Vec::with_capacity(pts.len() + 2);
        poly.push(Point::new(pts[0].x, base_y));
        poly.extend_from_slice(&pts);
        poly.push(Point::new(pts[pts.len() - 1].x, base_y));
        let mut s_poly = Scene::new();
        s_poly.fill_poly(poly, col, col);
        let mut fb_poly = Framebuffer::new(fw, fh);
        fb_poly.clear(bg);
        fb_poly.replay(&s_poly, scale);

        let mut sum = 0.0f64;
        for (&a, &b) in fb_col.pixels.iter().zip(&fb_poly.pixels) {
            for shift in [16, 8, 0] {
                sum += (((a >> shift) & 255) as f64 - ((b >> shift) & 255) as f64).abs();
            }
        }
        let mean = sum / (fb_col.pixels.len() * 3) as f64;
        assert!(mean < 0.5, "column fill mean channel diff {mean:.4} >= 0.5");

        // Seam check on a flat translucent band: every pixel across a row inside it must match.
        let flat: Vec<Point> = (0..32).map(|i| Point::new(20.0 + 200.0 * i as f32 / 31.0, 30.0)).collect();
        let mut s_flat = Scene::new();
        area(&mut s_flat, &flat, 90.0, col);
        let mut fb_flat = Framebuffer::new(fw, fh);
        fb_flat.clear(bg);
        fb_flat.replay(&s_flat, scale);
        let row = &fb_flat.pixels[(60 * 2) * fw..(60 * 2 + 1) * fw];
        let ref_px = row[60];
        assert!(row[50..420].iter().all(|&p| p == ref_px), "seam detected in flat translucent column fill");
    }
}
