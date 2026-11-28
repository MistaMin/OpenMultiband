//! 3D spectrum terrain view for OpenMultiband, showing frequency energy over the last 3 seconds
//! with red gain reduction, green upward lift, and active crossover frequency markers along the
//! front edge. Rendered into a cached image at 10 Hz so intermediate 30 Hz frames just copy it.

use std::collections::VecDeque;

use openplugin::dri::goodlookindri::color::Color as C;
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};

use super::{analyser, kit, raster, MultibandEditor, DISPLAY, WIDTH, HEIGHT};

pub const BANDS3: usize = 72;
pub const SPEC3_SIZE: usize = 8192;
const SUB: usize = 2;
pub const F3_LO: f32 = 20.0;
pub const F3_HI: f32 = 20_000.0;
pub const ROW_HZ: f32 = 6.0;
pub const ROWS3: usize = 18;
const SECONDS3: usize = 3;
const DB3_LO: f32 = -90.0;
const DB3_HI: f32 = -12.0;
const FRONT_GAP: f32 = 22.0;
const BACK_AT: f32 = 0.22;
const FRONT_H: f32 = 0.44;
const BACK_SCALE: f32 = 0.52;
const F_LABELS: [(f32, &str); 9] = [
    (50.0, "50"),
    (100.0, "100"),
    (200.0, "200"),
    (500.0, "500"),
    (1000.0, "1k"),
    (2000.0, "2k"),
    (5000.0, "5k"),
    (10_000.0, "10k"),
    (20_000.0, "20k"),
];

#[derive(Clone, Copy)]
pub struct Well {
    pub bg: C,
    pub grid: C,
    pub text: C,
    pub ridge: C,
    heat: [(f32, u32); 5],
}

const DARK_WELL: Well = Well {
    bg: C(0xff0a_0e14),
    grid: C(0xff26_2e38),
    text: C(0xff8a_949b),
    ridge: C(0xffff_ffff),
    heat: [(0.0, 0x14_286e), (0.35, 0x1f_6fd0), (0.65, 0x2e_c8e0), (0.85, 0xb4_f0f5), (1.0, 0xfa_faf0)],
};

const LIGHT_WELL: Well = Well {
    bg: C(0xfff4_f6f9),
    grid: C(0xffcf_d6df),
    text: C(0xff5a_6470),
    ridge: C(0xff10_1820),
    heat: [(0.0, 0xd6_e4f2), (0.35, 0x8c_b8e6), (0.65, 0x2f_7fd6), (0.85, 0x1c_4fa8), (1.0, 0x0c_2460)],
};

pub fn well(t: &super::theme::Theme) -> Well {
    let c = t.plot_bg.0;
    let luma = 0.2126 * ((c >> 16) & 255) as f32 + 0.7152 * ((c >> 8) & 255) as f32 + 0.0722 * (c & 255) as f32;
    if luma > 128.0 { Well { bg: t.plot_bg, ..LIGHT_WELL } } else { DARK_WELL }
}

const GR_COLOUR: C = C(0xffe8_3a3a);
const LIFT_COLOUR: C = C(0xff2f_b36a);
const HEAT_STEPS: f32 = 12.0;

impl Well {
    fn heat(&self, v: f32) -> C {
        let v = v.clamp(0.0, 1.0);
        let i = self.heat.iter().position(|(at, _)| *at >= v).unwrap_or(self.heat.len() - 1).max(1);
        let ((a, ca), (b, cb)) = (self.heat[i - 1], self.heat[i]);
        let f = if b > a { (v - a) / (b - a) } else { 1.0 };
        C(0xff00_0000 | raster::blend(ca, C(cb), f))
    }
}

pub type Row3 = ([f32; BANDS3], [f32; BANDS3]);

pub struct Terrain {
    pub spec: [analyser::Spectrum; 2],
    pub rows: VecDeque<Row3>,
    pub written: u64,
    acc: f32,
}

impl Default for Terrain {
    fn default() -> Self {
        Self {
            spec: std::array::from_fn(|_| analyser::Spectrum::new(SPEC3_SIZE, BANDS3, F3_LO, F3_HI).instant()),
            rows: VecDeque::from(vec![([0.0; BANDS3], [0.0; BANDS3]); ROWS3]),
            written: 0,
            acc: 0.0,
        }
    }
}

fn norm(db: f32) -> f32 {
    ((db - DB3_LO) / (DB3_HI - DB3_LO)).clamp(0.0, 1.0)
}

fn mix(a: C, b: C, t: f32) -> C {
    C(0xff00_0000 | raster::blend(a.0 & 0x00ff_ffff, b, t))
}

fn row_geom(t: f32) -> (f32, f32, f32, f32) {
    let depth = 1.0 - (1.0 - t) * (1.0 - t);
    let scale = 1.0 - (1.0 - BACK_SCALE) * depth;
    let front = DISPLAY.1 + DISPLAY.3 - FRONT_GAP;
    let back = DISPLAY.1 + DISPLAY.3 * BACK_AT;
    let w = DISPLAY.2 * scale;
    (DISPLAY.0 + (DISPLAY.2 - w) * 0.5, w, front + (back - front) * depth, DISPLAY.3 * FRONT_H * scale)
}

fn x_of(f: f32) -> f32 {
    DISPLAY.0 + DISPLAY.2 * (f / F3_LO).ln() / (F3_HI / F3_LO).ln()
}

fn spline(v: &[f32; BANDS3]) -> Vec<f32> {
    let at = |k: isize| v[k.clamp(0, BANDS3 as isize - 1) as usize];
    let mut out = Vec::with_capacity((BANDS3 - 1) * SUB + 1);
    for k in 0..BANDS3 as isize - 1 {
        let (p0, p1, p2, p3) = (at(k - 1), at(k), at(k + 1), at(k + 2));
        for j in 0..SUB {
            let t = j as f32 / SUB as f32;
            let (t2, t3) = (t * t, t * t * t);
            let y = 0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3);
            out.push(y.clamp(0.0, 1.0));
        }
    }
    out.push(v[BANDS3 - 1]);
    out
}

impl Terrain {
    pub fn feed(&mut self, taps: &[Vec<f32>; 4], sr: f32, dt: f32) -> bool {
        self.acc += dt.max(0.0);
        if self.acc < 1.0 / ROW_HZ {
            return false;
        }
        self.acc = (self.acc - 1.0 / ROW_HZ).min(1.0 / ROW_HZ);
        let [a, b, c, d] = taps;
        self.spec[0].update(a, Some(b), sr, dt);
        self.spec[1].update(c, Some(d), sr, dt);
        let smooth = |db: &[f32]| -> [f32; BANDS3] {
            std::array::from_fn(|k| {
                let at = |d: isize| db[(k as isize + d).clamp(0, BANDS3 as isize - 1) as usize];
                norm((at(-2) + 4.0 * at(-1) + 6.0 * at(0) + 4.0 * at(1) + at(2)) / 16.0)
            })
        };
        let row = (smooth(&self.spec[0].db), smooth(&self.spec[1].db));
        self.rows.pop_front();
        self.rows.push_back(row);
        self.written += 1;
        true
    }
}

#[derive(Default)]
pub struct TerrainImage {
    made_for: Option<(u64, u32, u64)>,
    fb: Option<goodlookindri_shell::raster::Framebuffer>,
    pub img: Option<std::sync::Arc<openplugin::dri::goodlookindri::scene::Image>>,
}

impl MultibandEditor {
    pub(super) fn update_terrain_image(&mut self, scale: f32) {
        let mut xhash = (self.theme as u64) | ((self.panel.map_or(0, |p| p + 1) as u64) << 8) | ((self.active_bands() as u64) << 16);
        for f in self.sorted_xovers() {
            xhash = xhash.wrapping_mul(31).wrapping_add(f.to_bits() as u64);
        }
        let want = (self.terrain.written, scale.to_bits(), xhash);
        if self.terrain_img.made_for == Some(want) && self.terrain_img.img.is_some() {
            return;
        }
        let (w, h) = ((WIDTH as f32 * scale).round() as usize, (HEIGHT as f32 * scale).round() as usize);
        let (x0, y0) = ((DISPLAY.0 * scale).round() as usize, (DISPLAY.1 * scale).round() as usize);
        let (dw, dh) = ((DISPLAY.2 * scale).round() as usize, (DISPLAY.3 * scale).round() as usize);
        let mut fb = match self.terrain_img.fb.take() {
            Some(fb) if fb.width == w && fb.height == h => fb,
            _ => goodlookindri_shell::raster::Framebuffer::new(w, h),
        };
        // Clear only the display slice of the offscreen buffer (4x fewer pixels than clearing the whole window).
        let bg = self.t().panel.0 & 0x00ff_ffff;
        for row in fb.pixels[y0 * w..].chunks_mut(w).take(dh) {
            row[x0..x0 + dw].fill(bg);
        }
        openplugin::dri::goodlookindri::hardware::set_device_scale(scale);
        let mut sc = Scene::new();
        self.paint_3d_layer(&mut sc);
        self.paint_3d_terrain(&mut sc);
        fb.replay(&sc, scale);
        let mut px = Vec::with_capacity(dw * dh);
        for row in fb.pixels[y0 * w..].chunks(w).take(dh) {
            px.extend(row[x0..x0 + dw].iter().map(|p| p | 0xff00_0000));
        }
        self.terrain_img.img = Some(std::sync::Arc::new(openplugin::dri::goodlookindri::scene::Image::new(dw as u32, dh as u32, px)));
        self.terrain_img.fb = Some(fb);
        self.terrain_img.made_for = Some(want);
    }

    pub(super) fn paint_3d_terrain_cached(&self, s: &mut Scene) {
        match &self.terrain_img.img {
            Some(img) if self.terrain_img.made_for.is_some_and(|m| m.0 == self.terrain.written) => {
                s.image(img, Rect::new(DISPLAY.0, DISPLAY.1, DISPLAY.2, DISPLAY.3), 1.0);
            }
            _ => self.paint_3d_terrain(s),
        }
    }

    pub(super) fn paint_3d_layer(&self, s: &mut Scene) {
        let t = self.t();
        let wl = well(t);
        kit::plot(s, t, DISPLAY);
        s.fill_round(Rect::new(DISPLAY.0, DISPLAY.1, DISPLAY.2, DISPLAY.3), 4.0, wl.bg, wl.bg, true);
        let (fl, fw, fb, _) = row_geom(0.0);
        let (bl, bw, bb, _) = row_geom(1.0);
        let ground = wl.grid.with_alpha(0.6);
        s.fill_poly(vec![Point::new(fl, fb), Point::new(fl + fw, fb), Point::new(bl + bw, bb), Point::new(bl, bb)], ground, ground);
        for sec in 1..SECONDS3 {
            let (l, w, b, _) = row_geom(sec as f32 / SECONDS3 as f32);
            s.line(Point::new(l, b), Point::new(l + w, b), wl.grid, 1.0);
        }
        s.line(Point::new(fl, fb), Point::new(fl + fw, fb), wl.grid, 1.0);
        // Active crossover perspective lines across the ground plane.
        for f in self.sorted_xovers() {
            let u = ((f / F3_LO).ln() / (F3_HI / F3_LO).ln()).clamp(0.0, 1.0);
            let xf = fl + fw * u;
            let xb = bl + bw * u;
            s.line(Point::new(xf, fb), Point::new(xb, bb), t.accent.with_alpha(0.55), 1.0);
        }
        for (f, label) in F_LABELS {
            let x = x_of(f).min(DISPLAY.0 + DISPLAY.2 - 12.0);
            s.line(Point::new(x, fb), Point::new(x, fb + 4.0), wl.grid, 1.0);
            s.text(Point::new(x, fb + 12.0), label, wl.text, 8.0, true);
        }
        kit::text_right(s, DISPLAY.0 + DISPLAY.2 - 6.0, DISPLAY.1 + 11.0, "Hz  ·  NOW AT THE FRONT", wl.text, 8.0);
        s.text(Point::new(DISPLAY.0 + 8.0, DISPLAY.1 + 11.0), "3D SPECTRUM  COLOUR = ENERGY", wl.text, 8.0, false);
        if self.panel.is_some() {
            return;
        }
        let (kx, ky) = (DISPLAY.0 + 8.0, DISPLAY.1 + 24.0);
        for i in 0..40 {
            let c = wl.heat(i as f32 / 39.0);
            s.fill_rect(Rect::new(kx + i as f32 * 2.0, ky, 2.0, 6.0), c, c, true);
        }
        s.text(Point::new(kx + 86.0, ky + 3.0), "QUIET TO LOUD", wl.text, 8.0, false);
        s.fill_rect(Rect::new(kx, ky + 12.0, 12.0, 6.0), GR_COLOUR, GR_COLOUR, true);
        s.text(Point::new(kx + 18.0, ky + 15.0), "CUT", wl.text, 8.0, false);
        s.fill_rect(Rect::new(kx + 56.0, ky + 12.0, 12.0, 6.0), LIFT_COLOUR, LIFT_COLOUR, true);
        s.text(Point::new(kx + 74.0, ky + 15.0), "LIFT", wl.text, 8.0, false);
    }

    pub(super) fn paint_3d_terrain(&self, s: &mut Scene) {
        let t = self.t();
        let wl = well(t);
        let tr = &self.terrain;
        let n = tr.rows.len();
        let points = (BANDS3 - 1) * SUB + 1;
        let x_at = |l: f32, w: f32, i: usize| l + w * i as f32 / (points - 1) as f32;
        type Ridge = (f32, f32, f32, f32, Vec<f32>, Vec<f32>);
        let ridges: Vec<Ridge> = (0..n)
            .map(|age| {
                let row = &tr.rows[n - 1 - age];
                let (l, w, base, h) = row_geom(age as f32 / (n - 1) as f32);
                (l, w, base, h, spline(&row.0), spline(&row.1))
            })
            .collect();
        let top_of = |age: usize| -> Vec<Point> {
            let (l, w, base, h, vi, vo) = &ridges[age];
            (0..points).map(|i| Point::new(x_at(*l, *w, i), base - h * vi[i].max(vo[i]))).collect()
        };
        for age in (0..n).rev() {
            let (l, w, base, h, vi, vo) = &ridges[age];
            let (l, w, base, h) = (*l, *w, *base, *h);
            let depth = age as f32 / (n - 1) as f32;
            let shade = 1.0 - 0.6 * depth;
            let y = |v: f32| base - h * v;
            let top = top_of(age);
            let inp: Vec<Point> = (0..points).map(|i| Point::new(x_at(l, w, i), y(vi[i]))).collect();
            let out: Vec<Point> = (0..points).map(|i| Point::new(x_at(l, w, i), y(vo[i]))).collect();
            let floor: Vec<f32> = if age == 0 {
                vec![base; points]
            } else {
                let front = top_of(age - 1);
                let mut j = 0;
                top.iter()
                    .map(|p| {
                        while j + 2 < front.len() && front[j + 1].x < p.x {
                            j += 1;
                        }
                        let (a, b) = (front[j], front[j + 1]);
                        let f = if b.x > a.x { ((p.x - a.x) / (b.x - a.x)).clamp(0.0, 1.0) } else { 0.0 };
                        (a.y + (b.y - a.y) * f).min(base)
                    })
                    .collect()
            };
            let lower_of = |pts: &[Point], other: &[Point]| -> Vec<Point> {
                pts.iter().zip(other).zip(&floor).map(|((p, q), fl)| Point::new(p.x, q.y.min(*fl).max(p.y))).collect()
            };
            let body_floor: Vec<Point> = top.iter().zip(&floor).map(|(p, fl)| Point::new(p.x, fl.max(p.y))).collect();
            let step = |i: usize| (vo[i] * HEAT_STEPS).round();
            let mut a = 0;
            while a < points - 1 {
                let q = step(a);
                let mut b = a + 1;
                while b < points - 1 && step(b) == q {
                    b += 1;
                }
                kit::between(s, &top[a..=b], &body_floor[a..=b], mix(wl.bg, wl.heat(q / HEAT_STEPS), 0.45 + 0.55 * shade));
                a = b;
            }
            let cut_lower = lower_of(&inp, &out);
            if inp.iter().zip(&cut_lower).any(|(p, q)| q.y - p.y > 0.5) {
                kit::between(s, &inp, &cut_lower, GR_COLOUR);
            }
            let add_lower = lower_of(&out, &inp);
            if out.iter().zip(&add_lower).any(|(p, q)| q.y - p.y > 0.5) {
                kit::between(s, &out, &add_lower, LIFT_COLOUR);
            }
            let ridge = if age == 0 { wl.ridge } else { mix(wl.bg, wl.ridge, 0.08 + 0.2 * shade) };
            kit::curve(s, top, ridge, if age == 0 { 1.4 } else { 0.8 });
        }
        for sec in 1..SECONDS3 {
            let (_, _, b, _) = row_geom(sec as f32 / SECONDS3 as f32);
            let tag = format!("-{sec} s");
            let w = goodlookindri_shell::text::width(&tag, 8.0);
            let right = DISPLAY.0 + DISPLAY.2 - 6.0;
            s.fill_round(Rect::new(right - w - 5.0, b - 6.5, w + 10.0, 13.0), 3.0, wl.bg, wl.bg, true);
            kit::text_right(s, right, b, &tag, wl.text, 8.0);
        }
    }
}
