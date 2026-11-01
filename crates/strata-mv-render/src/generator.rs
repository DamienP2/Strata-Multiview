//! The test signal generator's pictures (`strata_mv_core::generator`):
//! colour bars, test patterns, a still picture, a lip sync test, with an
//! ident, the time and a moving box over them. The pattern itself is drawn
//! once per setting and size; each frame copies it and draws what moves.

use std::sync::Arc;
use std::time::SystemTime;

use strata_mv_core::generator::{
    flash_frames, frames_into_second, sound, GeneratorConfig, Pattern,
};
use strata_mv_core::model::ImageFit;
use strata_mv_core::{TextAlign, VerticalAlign};
use tiny_skia::{Color as SkColor, FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

use crate::text as tx;
use crate::{Frame, OutputFormat, Renderer};

/// A pattern drawn, for which setting and size.
pub(crate) struct Drawn {
    key: String,
    pm: Pixmap,
}

fn rgb(r: u8, g: u8, b: u8) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(SkColor::from_rgba8(r, g, b, 255));
    p.anti_alias = false;
    p
}

/// A share of white: 8-bit RGB of `pct` % (75 % bars: 191).
fn level(pct: f32) -> u8 {
    (pct / 100.0 * 255.0).round().clamp(0.0, 255.0) as u8
}

fn fill(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, p: &Paint) {
    if let Some(r) = Rect::from_xywh(
        x.round(),
        y.round(),
        (x + w).round() - x.round(),
        (y + h).round() - y.round(),
    ) {
        pm.fill_rect(r, p, Transform::identity(), None);
    }
}

/// Seven bars of `pct` %: white, yellow, cyan, green, magenta, red, blue.
const BARS: [[bool; 3]; 7] = [
    [true, true, true],
    [true, true, false],
    [false, true, true],
    [false, true, false],
    [true, false, true],
    [true, false, false],
    [false, false, true],
];

fn bars(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, pct: f32, n: usize) {
    let v = level(pct);
    let bw = w / n as f32;
    let on = |c: bool| if c { v } else { 0 };
    for (i, [r, g, b]) in BARS.iter().take(n).enumerate() {
        fill(
            pm,
            x + i as f32 * bw,
            y,
            bw,
            h,
            &rgb(on(*r), on(*g), on(*b)),
        );
    }
}

impl Renderer {
    /// The generator's picture of frame `n` (at `num`/`den` frames a second),
    /// with its sound.
    #[allow(clippy::too_many_arguments)]
    pub fn render_generator(
        &mut self,
        g: &GeneratorConfig,
        out: OutputFormat,
        n: u64,
        num: u32,
        den: u32,
        now_ms: i64,
        tz: &str,
    ) -> Frame {
        let (w, h) = (out.width.max(2), out.height.max(2));
        let key = format!(
            "{:?}|{}|{}|{w}x{h}|{}",
            g.pattern, g.color, g.image, self.assets_generation
        );
        if self.generator.as_ref().is_none_or(|d| d.key != key) {
            let pm = self.draw_pattern(g, w, h);
            self.generator = Some(Drawn { key, pm });
        }
        let mut pm = self
            .generator
            .as_ref()
            .map(|d| d.pm.clone())
            .unwrap_or_else(|| Pixmap::new(w, h).expect("size"));
        let (wf, hf) = (w as f32, h as f32);
        let fps = num as f64 / den.max(1) as f64;

        if g.pattern == Pattern::Lipsync {
            let into = frames_into_second(n, num, den);
            let per = fps.round().max(1.0) as u64;
            // The sweep: one step a frame across the second, the flash at its start.
            let x0 = wf * 0.1;
            let sw = wf * 0.8;
            let y = hf * 0.72;
            let step = sw / per as f32;
            fill(
                &mut pm,
                x0 + into.min(per - 1) as f32 * step,
                y - hf * 0.05,
                step.max(2.0),
                hf * 0.1,
                &rgb(255, 210, 0),
            );
            if into < flash_frames(num, den) {
                fill(
                    &mut pm,
                    wf * 0.3,
                    hf * 0.18,
                    wf * 0.4,
                    hf * 0.4,
                    &rgb(255, 255, 255),
                );
            }
            self.gen_text(
                &mut pm,
                &format!("{:>2} / {per}", into),
                (wf * 0.3, hf * 0.8, wf * 0.4, hf * 0.08),
                hf * 0.06,
                700,
                [255, 255, 255],
                true,
            );
        }
        if g.motion {
            // A box across the bottom, there and back in four seconds.
            let s = (hf * 0.03).max(6.0);
            let period = (fps * 4.0).max(2.0);
            let p = (n as f64 % period) / period;
            let t = if p < 0.5 { p * 2.0 } else { 2.0 - p * 2.0 } as f32;
            let x = wf * 0.05 + t * (wf * 0.9 - s);
            fill(&mut pm, x, hf * 0.94 - s, s, s, &rgb(255, 255, 255));
        }
        if g.clock {
            let tc = strata_mv_core::wid::broadcast::station_timecode(now_ms, tz, fps);
            let bw = hf * 0.42;
            let bh = hf * 0.075;
            let bx = (wf - bw) / 2.0;
            fill(&mut pm, bx, hf * 0.04, bw, bh, &rgb(0, 0, 0));
            self.gen_text(
                &mut pm,
                &tc,
                (bx, hf * 0.04, bw, bh),
                bh * 0.7,
                600,
                [255, 255, 255],
                true,
            );
        }
        let ident = g.ident.trim();
        if !ident.is_empty() {
            let bh = hf * 0.09;
            let bw = (wf * 0.6).min(bh * 0.55 * ident.chars().count() as f32 + bh);
            let bx = (wf - bw) / 2.0;
            let by = hf * 0.6;
            fill(&mut pm, bx, by, bw, bh, &rgb(0, 0, 0));
            self.gen_text(
                &mut pm,
                ident,
                (bx, by, bw, bh),
                bh * 0.6,
                700,
                [255, 255, 255],
                false,
            );
        }

        let audio = sound(g, n, num, den).map(Arc::new);
        let data = pm.take();
        Frame {
            width: w,
            height: h,
            data,
            captured_at: SystemTime::now(),
            audio,
        }
    }

    /// Words centred in a box.
    #[allow(clippy::too_many_arguments)]
    fn gen_text(
        &mut self,
        pm: &mut Pixmap,
        s: &str,
        (x, y, w, h): (f32, f32, f32, f32),
        size: f32,
        weight: u16,
        [r, g, b]: [u8; 3],
        mono: bool,
    ) {
        let family = if mono { "JetBrains Mono" } else { "Inter" };
        let Some(font) = self
            .fonts
            .font(family, weight)
            .or_else(|| self.fonts.font("Inter", weight))
        else {
            return;
        };
        let style = tx::TextStyle {
            size,
            letter_spacing: 0.0,
            align: TextAlign::Center,
            vertical_align: VerticalAlign::Middle,
            box_w: w,
            box_h: h,
            line_height: 1.2,
            decoration: strata_mv_core::TextDecoration::None,
        };
        if let Some(p) = tx::layout_text(&font, s, &style)
            .and_then(|p| p.transform(Transform::from_translate(x, y)))
        {
            let mut paint = Paint::default();
            paint.set_color(SkColor::from_rgba8(r, g, b, 255));
            paint.anti_alias = true;
            pm.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }

    /// The pattern itself (what does not move).
    fn draw_pattern(&mut self, g: &GeneratorConfig, w: u32, h: u32) -> Pixmap {
        let mut pm = Pixmap::new(w, h).expect("size");
        let (wf, hf) = (w as f32, h as f32);
        pm.fill(SkColor::BLACK);
        match g.pattern {
            Pattern::SmpteHd => {
                // SMPTE RP 219: 40 % grey sides, 75 % bars over 7/12; then
                // 100 % cyan, +I and 75 % white, 100 % blue; then 100 %
                // yellow, a ramp, 100 % red; then the PLUGE row.
                let d = wf / 8.0;
                let c = d * 6.0 / 7.0;
                let (h1, h2, h3) = (hf * 7.0 / 12.0, hf / 12.0, hf / 12.0);
                let grey = level(40.0);
                fill(&mut pm, 0.0, 0.0, d, h1, &rgb(grey, grey, grey));
                fill(&mut pm, wf - d, 0.0, d, h1, &rgb(grey, grey, grey));
                bars(&mut pm, d, 0.0, c * 7.0, h1, 75.0, 7);
                let y2 = h1;
                fill(&mut pm, 0.0, y2, d, h2, &rgb(0, 255, 255));
                fill(&mut pm, d, y2, c, h2, &rgb(0, 63, 105));
                let w75 = level(75.0);
                fill(&mut pm, d + c, y2, c * 6.0, h2, &rgb(w75, w75, w75));
                fill(&mut pm, wf - d, y2, d, h2, &rgb(0, 0, 255));
                let y3 = y2 + h2;
                fill(&mut pm, 0.0, y3, d, h3, &rgb(255, 255, 0));
                let ramp = c * 7.0;
                let steps = ramp.round().max(1.0) as u32;
                for i in 0..steps {
                    let v = (i as f32 / (steps - 1).max(1) as f32 * 255.0) as u8;
                    fill(&mut pm, d + i as f32, y3, 1.0, h3, &rgb(v, v, v));
                }
                fill(&mut pm, wf - d, y3, d, h3, &rgb(255, 0, 0));
                let y4 = y3 + h3;
                let h4 = hf - y4;
                let g15 = level(15.0);
                fill(&mut pm, 0.0, y4, d, h4, &rgb(g15, g15, g15));
                fill(&mut pm, wf - d, y4, d, h4, &rgb(g15, g15, g15));
                // Black, 100 % white, black, PLUGE (−2 % shows as 0), black.
                let x = d;
                let u = c * 7.0 / 12.0;
                fill(&mut pm, x + u * 1.5, y4, u * 2.0, h4, &rgb(255, 255, 255));
                let p2 = level(2.0);
                let p4 = level(4.0);
                fill(&mut pm, x + u * 8.0, y4, u * 0.5, h4, &rgb(p2, p2, p2));
                fill(&mut pm, x + u * 9.0, y4, u * 0.5, h4, &rgb(p4, p4, p4));
            }
            Pattern::Ebu75 => {
                // EBU 100/0/75/0: white at 100 %, the colours at 75 %, black.
                let bw = wf / 8.0;
                let v = level(75.0);
                fill(&mut pm, 0.0, 0.0, bw, hf, &rgb(255, 255, 255));
                for (i, [r, g, b]) in BARS.iter().enumerate().skip(1) {
                    let on = |x: bool| if x { v } else { 0 };
                    fill(
                        &mut pm,
                        i as f32 * bw,
                        0.0,
                        bw,
                        hf,
                        &rgb(on(*r), on(*g), on(*b)),
                    );
                }
                fill(&mut pm, 7.0 * bw, 0.0, wf - 7.0 * bw, hf, &rgb(0, 0, 0));
            }
            Pattern::Bars100 => {
                let bw = wf / 8.0;
                bars(&mut pm, 0.0, 0.0, bw * 7.0, hf, 100.0, 7);
                fill(&mut pm, 7.0 * bw, 0.0, wf - 7.0 * bw, hf, &rgb(0, 0, 0));
            }
            Pattern::GreyScale => {
                // A ramp over the top half, 11 steps (0 to 100 %) under it.
                for x in 0..w {
                    let v = (x as f32 / (w - 1).max(1) as f32 * 255.0).round() as u8;
                    fill(&mut pm, x as f32, 0.0, 1.0, hf / 2.0, &rgb(v, v, v));
                }
                let sw = wf / 11.0;
                for i in 0..11 {
                    let v = level(i as f32 * 10.0);
                    fill(
                        &mut pm,
                        i as f32 * sw,
                        hf / 2.0,
                        sw + 1.0,
                        hf / 2.0,
                        &rgb(v, v, v),
                    );
                }
            }
            Pattern::Pluge => {
                // On black: −2 % (shows as black: set right, it disappears),
                // +2 %, +4 %, and white, grey references.
                let bw = wf / 10.0;
                let y = hf * 0.2;
                let bh = hf * 0.6;
                for (i, pct) in [(2, 2.0), (4, 4.0)] {
                    let v = level(pct);
                    fill(&mut pm, (3 + i) as f32 * bw, y, bw, bh, &rgb(v, v, v));
                }
                fill(
                    &mut pm,
                    wf * 0.8,
                    y,
                    bw * 1.2,
                    bh * 0.25,
                    &rgb(255, 255, 255),
                );
                let g50 = level(50.0);
                fill(
                    &mut pm,
                    wf * 0.8,
                    y + bh * 0.35,
                    bw * 1.2,
                    bh * 0.25,
                    &rgb(g50, g50, g50),
                );
                for (label, x) in [("−2 %", 4.0), ("+2 %", 5.0), ("+4 %", 7.0)] {
                    self.gen_text(
                        &mut pm,
                        label,
                        ((x - 0.5) * bw, y + bh + hf * 0.02, bw * 2.0, hf * 0.06),
                        hf * 0.035,
                        500,
                        [120, 120, 120],
                        true,
                    );
                }
            }
            Pattern::Grid => {
                // Lines every 1/16 of the width from the centre, the centre
                // cross, a circle, the safe areas (action 93 %, title 90 %).
                let white = rgb(235, 235, 235);
                let step = wf / 16.0;
                let lw = (hf / 360.0).max(1.0);
                let (cx, cy) = (wf / 2.0, hf / 2.0);
                let mut k = 0.0;
                while k <= wf / 2.0 + 1.0 {
                    for x in [cx - k, cx + k] {
                        fill(&mut pm, x - lw / 2.0, 0.0, lw, hf, &white);
                    }
                    k += step;
                }
                let mut k = 0.0;
                while k <= hf / 2.0 + 1.0 {
                    for y in [cy - k, cy + k] {
                        fill(&mut pm, 0.0, y - lw / 2.0, wf, lw, &white);
                    }
                    k += step;
                }
                fill(
                    &mut pm,
                    cx - lw * 1.5,
                    cy - hf * 0.05,
                    lw * 3.0,
                    hf * 0.1,
                    &rgb(255, 255, 255),
                );
                fill(
                    &mut pm,
                    cx - hf * 0.05,
                    cy - lw * 1.5,
                    hf * 0.1,
                    lw * 3.0,
                    &rgb(255, 255, 255),
                );
                let stroke = |pm: &mut Pixmap, path: tiny_skia::Path, c: Paint, width: f32| {
                    let mut c = c;
                    c.anti_alias = true;
                    pm.stroke_path(
                        &path,
                        &c,
                        &Stroke {
                            width,
                            ..Default::default()
                        },
                        Transform::identity(),
                        None,
                    );
                };
                if let Some(p) = PathBuilder::from_circle(cx, cy, hf * 0.45) {
                    stroke(&mut pm, p, rgb(255, 255, 255), lw * 2.0);
                }
                for (k, c) in [(0.93, rgb(255, 210, 0)), (0.90, rgb(0, 220, 120))] {
                    let (sw, sh) = (wf * k, hf * k);
                    if let Some(r) = Rect::from_xywh(cx - sw / 2.0, cy - sh / 2.0, sw, sh) {
                        stroke(&mut pm, PathBuilder::from_rect(r), c, lw * 2.0);
                    }
                }
            }
            Pattern::ZonePlate => {
                // cos(a·r²), the frequency reaching half the sampling rate
                // at the corners (no aliasing anywhere).
                let (cx, cy) = (wf / 2.0, hf / 2.0);
                let a = std::f32::consts::PI / (2.0 * cx.hypot(cy));
                let d = pm.data_mut();
                for y in 0..h {
                    for x in 0..w {
                        let (dx, dy) = (x as f32 - cx, y as f32 - cy);
                        let v = (127.5 + 127.5 * (a * (dx * dx + dy * dy)).cos()) as u8;
                        let o = ((y * w + x) * 4) as usize;
                        d[o..o + 4].copy_from_slice(&[v, v, v, 255]);
                    }
                }
            }
            Pattern::Solid => {
                let [r, gg, b, _] = strata_mv_core::Color::from_hex(&g.color)
                    .unwrap_or(strata_mv_core::Color::BLACK)
                    .0;
                pm.fill(SkColor::from_rgba(r, gg, b, 1.0).unwrap_or(SkColor::BLACK));
            }
            Pattern::Image => {
                if !g.image.trim().is_empty() {
                    let src = g.image.trim().to_string();
                    self.draw_image(
                        &mut pm,
                        &src,
                        ImageFit::Contain,
                        wf,
                        hf,
                        Transform::identity(),
                        1.0,
                        None,
                    );
                }
            }
            Pattern::Lipsync => {
                // Dark, the sweep's track and its frame marks, the words.
                pm.fill(SkColor::from_rgba8(24, 26, 32, 255));
                let x0 = wf * 0.1;
                let sw = wf * 0.8;
                let y = hf * 0.72;
                fill(&mut pm, x0, y - hf * 0.05, sw, hf * 0.1, &rgb(48, 52, 62));
                fill(
                    &mut pm,
                    wf * 0.3,
                    hf * 0.18,
                    wf * 0.4,
                    hf * 0.4,
                    &rgb(36, 38, 46),
                );
                self.gen_text(
                    &mut pm,
                    "SYNCHRO SON / IMAGE · LIP SYNC",
                    (0.0, hf * 0.04, wf, hf * 0.1),
                    hf * 0.05,
                    700,
                    [200, 204, 214],
                    false,
                );
            }
        }
        pm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(f: &Frame, x: u32, y: u32) -> [u8; 3] {
        let o = ((y * f.width + x) * 4) as usize;
        [f.data[o], f.data[o + 1], f.data[o + 2]]
    }

    #[test]
    fn every_pattern_draws_with_its_sound() {
        let mut r = Renderer::new(std::env::temp_dir());
        for pattern in [
            Pattern::SmpteHd,
            Pattern::Ebu75,
            Pattern::Bars100,
            Pattern::GreyScale,
            Pattern::Pluge,
            Pattern::Grid,
            Pattern::ZonePlate,
            Pattern::Solid,
            Pattern::Image,
            Pattern::Lipsync,
        ] {
            let g = GeneratorConfig {
                enabled: true,
                pattern,
                clock: true,
                motion: true,
                ident: "TEST".into(),
                ..Default::default()
            };
            let f = r.render_generator(
                &g,
                OutputFormat {
                    width: 320,
                    height: 180,
                },
                3,
                25,
                1,
                0,
                "UTC",
            );
            assert_eq!((f.width, f.height, f.data.len()), (320, 180, 320 * 180 * 4));
            assert_eq!(f.audio.as_ref().map(|a| a.len()), Some(1920 * 2));
        }
    }

    #[test]
    fn bars_are_at_their_levels() {
        let mut r = Renderer::new(std::env::temp_dir());
        let g = GeneratorConfig {
            pattern: Pattern::Ebu75,
            ..Default::default()
        };
        let f = r.render_generator(
            &g,
            OutputFormat {
                width: 800,
                height: 100,
            },
            0,
            25,
            1,
            0,
            "UTC",
        );
        // White 100 %, yellow 75 %, blue 75 %, black.
        assert_eq!(at(&f, 50, 50), [255, 255, 255]);
        assert_eq!(at(&f, 150, 50), [191, 191, 0]);
        assert_eq!(at(&f, 650, 50), [0, 0, 191]);
        assert_eq!(at(&f, 750, 50), [0, 0, 0]);
    }

    #[test]
    fn the_lip_sync_flash_comes_with_the_beep() {
        let mut r = Renderer::new(std::env::temp_dir());
        let g = GeneratorConfig {
            pattern: Pattern::Lipsync,
            tone: strata_mv_core::generator::Tone::Beep,
            ..Default::default()
        };
        let out = OutputFormat {
            width: 320,
            height: 180,
        };
        for n in [0u64, 1, 25, 26] {
            let f = r.render_generator(&g, out, n, 25, 1, 0, "UTC");
            let flash = at(&f, 160, 70) == [255, 255, 255];
            let beep = f.audio.as_ref().unwrap().iter().any(|v| v.abs() > 1000);
            assert_eq!(flash, n % 25 == 0, "frame {n}");
            assert_eq!(beep, flash, "frame {n}");
        }
    }
}
