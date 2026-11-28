//! Pixels straight into the frame, for what moves on every tick.

use openplugin::dri::goodlookindri::color::Color as C;
use goodlookindri_shell::text::{self, GlyphCache};

pub type R = (f32, f32, f32, f32);

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Centre,
}

/// Blend of `over` at `alpha` on top of the opaque colour `under`, as `0x00RRGGBB`.
pub fn blend(under: u32, over: C, alpha: f32) -> u32 {
    let a = alpha.clamp(0.0, 1.0);
    if a <= 0.0 {
        return under;
    }
    let ch = |shift: u32| (((under >> shift) & 255) as f32 * (1.0 - a) + ((over.0 >> shift) & 255) as f32 * a + 0.5) as u32;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// Brightness weights the toolkit's shell uses to tell light text from dark.
fn luma(c: u32) -> u32 {
    2 * ((c >> 16) & 255) + 5 * ((c >> 8) & 255) + (c & 255)
}

pub fn rgb(c: C) -> u32 {
    c.0 & 0x00ff_ffff
}

/// The frame's pixels, `0x00RRGGBB`, top row first. Everything is in device pixels.
pub struct Target<'a> {
    pub px: &'a mut [u32],
    pub w: usize,
    pub h: usize,
}

impl Target<'_> {
    pub fn mix(&mut self, x: usize, y: usize, c: C, alpha: f32) {
        if x < self.w && y < self.h && alpha > 0.0 {
            let i = y * self.w + x;
            self.px[i] = blend(self.px[i], c, alpha);
        }
    }

    /// An anti-aliased vertical run in column `x` from `y0` to `y1`, kept inside rows `top..bottom`.
    #[allow(clippy::too_many_arguments)]
    pub fn vspan(&mut self, x: usize, y0: f32, y1: f32, top: usize, bottom: usize, c: C, alpha: f32) {
        let (y0, y1) = (y0.max(top as f32), y1.min(bottom.min(self.h) as f32));
        if x >= self.w || y1 <= y0 {
            return;
        }
        for row in y0.floor() as usize..y1.ceil() as usize {
            let cover = (y1.min(row as f32 + 1.0) - y0.max(row as f32)).clamp(0.0, 1.0);
            self.mix(x, row, c, alpha * cover);
        }
    }

    pub fn fill(&mut self, x0: usize, y0: usize, x1: usize, y1: usize, value: u32) {
        let (x0, x1) = (x0.min(self.w), x1.min(self.w));
        for y in y0..y1.min(self.h) {
            self.px[y * self.w + x0..y * self.w + x1].fill(value);
        }
    }

    /// A filled rounded rectangle, anti-aliased. `ring` > 0 paints only an outline that wide.
    pub fn round(&mut self, r: R, radius: f32, ring: f32, c: C, alpha: f32) {
        let (cx, cy, hx, hy) = (r.0 + r.2 * 0.5, r.1 + r.3 * 0.5, r.2 * 0.5, r.3 * 0.5);
        let radius = radius.min(hx).min(hy);
        for y in (r.1 - 1.0).max(0.0) as usize..((r.1 + r.3 + 1.0).ceil() as usize).min(self.h) {
            for x in (r.0 - 1.0).max(0.0) as usize..((r.0 + r.2 + 1.0).ceil() as usize).min(self.w) {
                let (qx, qy) = ((x as f32 + 0.5 - cx).abs() - (hx - radius), (y as f32 + 0.5 - cy).abs() - (hy - radius));
                let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - radius;
                let cover = if ring > 0.0 { 0.5 - ((d + ring * 0.5).abs() - ring * 0.5) } else { 0.5 - d };
                self.mix(x, y, c, alpha * cover.clamp(0.0, 1.0));
            }
        }
    }

    /// Text with the canvas's own font and placement rules: `y` is the middle of the line.
    #[allow(clippy::too_many_arguments)]
    pub fn text(&mut self, glyphs: &mut GlyphCache, x: f32, y: f32, label: &str, size: f32, c: C, align: Align) {
        let px = text::quantise(size);
        let font = text::face();
        let mut pens = Vec::with_capacity(label.len());
        let (mut pen, mut prev) = (0.0f32, None);
        for ch in label.chars() {
            if let Some(p) = prev {
                pen += font.horizontal_kern(p, ch, px).unwrap_or(0.0);
            }
            pens.push((ch, pen));
            pen += font.metrics(ch, px).advance_width;
            prev = Some(ch);
        }
        let x0 = match align {
            Align::Left => x,
            Align::Centre => x - pen * 0.5,
        };
        let baseline = y + px * text::MIDDLE;
        for (ch, pen) in pens {
            let g = glyphs.get(ch, px);
            let (gx, gy) = ((x0 + pen).round() as i64 + g.left as i64, (baseline - g.top as f32).round() as i64);
            for row in 0..g.height {
                for col in 0..g.width {
                    let cover = g.coverage[row * g.width + col];
                    let (tx, ty) = (gx + col as i64, gy + row as i64);
                    if cover > 0 && tx >= 0 && (tx as usize) < self.w && ty >= 0 && (ty as usize) < self.h {
                        let (x, y) = (tx as usize, ty as usize);
                        let cover = cover as f32 / 255.0;
                        // The GoodLookin DRI rule: light text on a darker ground has its edges lifted,
                        // since blending in sRGB thins them; dark text on light is left alone.
                        let cover = if luma(c.0) > luma(self.px[y * self.w + x]) { cover.powf(1.0 / 1.45) } else { cover };
                        self.mix(x, y, c, cover);
                    }
                }
            }
        }
    }
}