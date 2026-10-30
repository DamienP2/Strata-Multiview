//! Clock layers (the Clock and the Time widgets): drawing (same picture as `web/src/widgets/timer.tsx`).
//!
//! Layout (both renderers): one plate per clock (several for city clocks,
//! side by side, 36 px apart), each `[prefix] value [suffix]` on one line,
//! items centred vertically, 0.14 em apart. With tabular digits every digit
//! sits in a cell as wide as the widest digit. With width reservation the
//! value box is as wide as the widest value the format can produce.

use ab_glyph::FontArc;
use cg_core::wid::timer::fmt::{Seg, SegKind, ZoneSel};
use cg_core::wid::timer::{
    affixes, banner, display, eff_in, hands, widest, At, Disp, Eff, Face, State, Ticks,
};
use cg_core::{Color, LayerKind, Prop, TextAlign, VerticalAlign};
use tiny_skia::{FillRule, LineCap, PathBuilder, Stroke, Transform};

use super::{rrect, Draw};

const LH: f32 = 1.2;

/// A piece of the value: a run of text, or one digit in a fixed cell.
struct Piece {
    text: String,
    w: f32,
    cell: bool,
    alpha: f32,
}

fn with_alpha(c: Color, a: f32) -> Color {
    let [r, g, b, al] = c.0;
    Color([r, g, b, al * a])
}

/// Advance of the widest digit (tabular cell).
fn cell_w(font: &FontArc, size: f32) -> f32 {
    (0..10)
        .map(|i| Draw::text_width(font, &i.to_string(), size, 0.0))
        .fold(0.0, f32::max)
}

fn pieces(
    font: &FontArc,
    segs: &[Seg],
    sep: &str,
    size: f32,
    ls: f32,
    tabular: bool,
    sep_alpha: f32,
    upper: bool,
) -> Vec<Piece> {
    let cw = cell_w(font, size);
    let mut out: Vec<Piece> = Vec::new();
    let mut push = |text: String, cell: bool, alpha: f32| {
        if text.is_empty() {
            return;
        }
        if let Some(last) = out.last_mut() {
            if !cell && !last.cell && last.alpha == alpha {
                last.text.push_str(&text);
                return;
            }
        }
        out.push(Piece {
            text,
            w: 0.0,
            cell,
            alpha,
        });
    };
    for g in segs {
        let t = if g.k == SegKind::Sep {
            sep.to_string()
        } else {
            g.t.clone()
        };
        let t = if upper { t.to_uppercase() } else { t };
        let alpha = if g.k == SegKind::Sep { sep_alpha } else { 1.0 };
        if tabular {
            let mut run = String::new();
            for ch in t.chars() {
                if ch.is_ascii_digit() {
                    push(std::mem::take(&mut run), false, alpha);
                    push(ch.to_string(), true, alpha);
                } else {
                    run.push(ch);
                }
            }
            push(run, false, alpha);
        } else {
            push(t, false, alpha);
        }
    }
    for p in &mut out {
        p.w = if p.cell {
            cw
        } else {
            Draw::text_width(font, &p.text, size, ls)
        };
    }
    out
}

fn width(ps: &[Piece]) -> f32 {
    ps.iter().map(|p| p.w).sum()
}

/// One plate laid out at scale 1 (sizes then multiplied by k).
struct Plate {
    disp: Disp,
    label: String,
    /// Reserved (value) width, prefix and suffix widths at scale 1.
    wi: f32,
    wp: f32,
    ws: f32,
}

struct Ctx<'a> {
    font: FontArc,
    afont: Option<FontArc>,
    size: f32,
    ls: f32,
    prefix: String,
    suffix: String,
    e: Eff<'a>,
}

impl Ctx<'_> {
    fn affix_size(&self, k: f32) -> f32 {
        self.size * k * self.e.c.affix.size / 100.0
    }
    fn gap(&self, k: f32) -> f32 {
        self.size * k * 0.14
    }
    fn pad(&self, k: f32) -> (f32, f32, f32) {
        let p = &self.e.c.plate;
        if !p.on {
            return (0.0, 0.0, 0.0);
        }
        let py = (p.pad_y * k).max(self.size * k * 0.08);
        let acc = if p.accent { 12.0 } else { 0.0 };
        ((p.pad_x + acc) * k, p.pad_x * k, py)
    }
    fn content_w(&self, pl: &Plate, k: f32) -> f32 {
        let mut w = pl.wi * k;
        if pl.wp > 0.0 {
            w += pl.wp * k + self.gap(k);
        }
        if pl.ws > 0.0 {
            w += pl.ws * k + self.gap(k);
        }
        w
    }
    fn plate_w(&self, pl: &Plate, k: f32) -> f32 {
        let (l, r, _) = self.pad(k);
        l + self.content_w(pl, k) + r
    }
    fn plate_h(&self, k: f32) -> f32 {
        self.size * k * LH + 2.0 * self.pad(k).2
    }
    fn label_h(&self, pl: &Plate, k: f32) -> f32 {
        if pl.label.is_empty() {
            0.0
        } else {
            self.affix_size(k) * LH + self.size * k * 0.12
        }
    }
}

/// The clocks: one, or one per city (zone, label).
fn clocks(e: &Eff) -> Vec<(Option<ZoneSel>, String)> {
    let c = e.c;
    if !e.timed() && c.cities && !c.city_list.is_empty() {
        c.city_list
            .iter()
            .map(|ct| {
                let label = if ct.label.trim().is_empty() {
                    ct.zone.clone()
                } else {
                    ct.label.clone()
                };
                (
                    Some(ZoneSel::named(&ct.zone)),
                    if c.affix.upper {
                        label.to_uppercase()
                    } else {
                        label
                    },
                )
            })
            .collect()
    } else {
        vec![(None, String::new())]
    }
}

/// A digital clock is drawn from what it shows (`display`, its value
/// aside), the blinks of its separator and of its value, and its banner at
/// zero: the same picture for the second it shows. An analog one moves.
pub(crate) fn motion(d: &mut Draw, st: &State) -> Option<crate::cache::Motion> {
    use crate::cache::Motion;
    let LayerKind::Clock { .. } = &d.layer.kind else {
        return None;
    };
    let e = eff_in(d.layer, d.data, d.lang)?;
    let c = e.c;
    if c.face == Face::Analog {
        return Some(Motion::Moving);
    }
    let now = d.time.now_ms;
    let at = At {
        now,
        age_ms: (d.time.age_secs * 1000.0) as i64,
        fps: d.fps,
        default_tz: d.time.default_tz,
    };
    let sep_off = c.sep_blink && now.rem_euclid(1000) >= 500;
    let mut shown = Vec::new();
    for (z, _) in clocks(&e) {
        let mut disp = display(&e, st, at, z.as_ref());
        if shown.is_empty() && banner(&e, &disp, now).is_some() {
            return Some(Motion::Moving);
        }
        let blink_off = disp.blink && now.rem_euclid(1000) >= 600;
        // Not drawn: the value itself and when it reaches zero (the banner).
        disp.v = None;
        disp.zero_at = None;
        shown.push(format!("{disp:?}{blink_off}"));
    }
    Some(Motion::of(&(sep_off, shown)))
}

pub(crate) fn draw(d: &mut Draw, st: &State) -> bool {
    let LayerKind::Clock { font: fs, .. } = &d.layer.kind else {
        return false;
    };
    let Some(e) = eff_in(d.layer, d.data, d.lang) else {
        return false;
    };
    let c = e.c;
    let now = d.time.now_ms;
    let at = At {
        now,
        age_ms: (d.time.age_secs * 1000.0) as i64,
        fps: d.fps,
        default_tz: d.time.default_tz,
    };
    let size = d.num(Prop::FontSize).max(1.0);
    let Some(font) = d.font(&fs.font_family, fs.font_weight) else {
        return true;
    };
    let afont = d.font(&fs.font_family, c.affix.weight);
    let (prefix, suffix) = affixes(c, d.data);
    let ls = d.num(Prop::LetterSpacing);
    let cx = Ctx {
        font,
        afont,
        size,
        ls,
        prefix,
        suffix,
        e: e.clone(),
    };
    let sep_alpha = if c.sep_blink && now.rem_euclid(1000) >= 500 {
        0.12
    } else {
        1.0
    };
    let zones = clocks(&e);
    if c.face == Face::Analog {
        draw_analog(d, &cx, st, at, &zones, fs.align, fs.vertical_align);
        return true;
    }
    let aw = |s: &str| match &cx.afont {
        Some(f) if !s.is_empty() => Draw::text_width(f, s, cx.affix_size(1.0), 0.0),
        _ => 0.0,
    };
    let plates: Vec<Plate> = zones
        .iter()
        .map(|(z, label)| {
            let disp = display(&e, st, at, z.as_ref());
            let ps = pieces(
                &cx.font,
                &disp.segs,
                &c.sep,
                size,
                ls,
                c.tabular,
                1.0,
                fs.uppercase,
            );
            let wv = width(&ps);
            let wi = if c.reserve {
                widest(&e, &disp, &c.sep, d.fps)
                    .iter()
                    .map(|s| {
                        width(&pieces(
                            &cx.font,
                            &[Seg::lit(s)],
                            &c.sep,
                            size,
                            ls,
                            c.tabular,
                            1.0,
                            fs.uppercase,
                        ))
                    })
                    .fold(wv, f32::max)
            } else {
                wv
            };
            Plate {
                disp,
                label: label.clone(),

                wi,
                wp: aw(&cx.prefix),
                ws: aw(&cx.suffix),
            }
        })
        .collect();
    if plates.iter().all(|p| p.disp.hidden) {
        return true;
    }
    let gap_cities = 36.0;
    let n = plates.len() as f32;
    let total1: f32 =
        plates.iter().map(|p| cx.plate_w(p, 1.0)).sum::<f32>() + (n - 1.0) * gap_cities;
    let k = if c.fit && total1 > 0.0 {
        ((d.w - 8.0) / total1).clamp(0.01, 1.0)
    } else {
        1.0
    };
    let total = plates.iter().map(|p| cx.plate_w(p, k)).sum::<f32>() + (n - 1.0) * gap_cities * k;
    let group_h = plates.iter().map(|p| cx.label_h(p, k)).fold(0.0, f32::max) + cx.plate_h(k);
    let x0 = match fs.align {
        TextAlign::Left => 0.0,
        TextAlign::Center => (d.w - total) / 2.0,
        TextAlign::Right => d.w - total,
    };
    let y0 = match fs.vertical_align {
        VerticalAlign::Top => 0.0,
        VerticalAlign::Middle => (d.h - group_h) / 2.0,
        VerticalAlign::Bottom => d.h - group_h,
    };
    let lab_h = plates.iter().map(|p| cx.label_h(p, k)).fold(0.0, f32::max);
    let mut x = x0;
    for pl in &plates {
        let pw = cx.plate_w(pl, k);
        if !pl.disp.hidden {
            draw_plate(d, &cx, pl, x, y0 + lab_h, k, sep_alpha, fs.uppercase);
            if !pl.label.is_empty() {
                if let Some(f) = cx.afont.clone() {
                    let sz = cx.affix_size(k);
                    let col = affix_color(&cx, d);
                    if let Some(p) = Draw::text_path(
                        &f,
                        &pl.label,
                        sz,
                        0.0,
                        TextAlign::Center,
                        VerticalAlign::Middle,
                        (x, y0 + lab_h - cx.label_h(pl, k), pw, sz * LH),
                        LH,
                    ) {
                        d.fill(&p, col, FillRule::Winding, None);
                    }
                }
            }
        }
        x += pw + gap_cities * k;
    }
    // Banner at zero, under the plate.
    if let Some((sc, alpha)) = plates.first().and_then(|p| banner(&e, &p.disp, now)) {
        if let Some(f) = d.font("Inter", 900) {
            let bs = 64.0 * k;
            let tw = Draw::text_width(&f, &c.banner_text, bs, 0.0);
            let (bw, bh) = (tw + 80.0 * k, bs * LH + 28.0 * k);
            let (bcx, by) = (x0 + total / 2.0, y0 + group_h + 22.0 * k);
            let tf = Transform::from_translate(bcx, by + bh / 2.0)
                .pre_scale(sc, sc)
                .pre_translate(-bw / 2.0, -bh / 2.0);
            if let Some(p) = rrect(0.0, 0.0, bw, bh, 14.0 * k).and_then(|p| p.transform(tf)) {
                d.fill(
                    &p,
                    with_alpha(Draw::hex(&c.banner_color), alpha),
                    FillRule::Winding,
                    None,
                );
            }
            if let Some(p) = Draw::text_path(
                &f,
                &c.banner_text,
                bs,
                0.0,
                TextAlign::Center,
                VerticalAlign::Middle,
                (0.0, 0.0, bw, bh),
                LH,
            )
            .and_then(|p| p.transform(tf))
            {
                d.fill(
                    &p,
                    with_alpha(Color([1.0, 1.0, 1.0, 1.0]), alpha),
                    FillRule::Winding,
                    None,
                );
            }
        }
    }
    true
}

/// Analog faces, side by side (one per city), each with its label under
/// it. Same geometry as `AnalogFaces` in web/src/widgets/timer.tsx.
fn draw_analog(
    d: &mut Draw,
    cx: &Ctx,
    st: &State,
    at: At,
    zones: &[(Option<ZoneSel>, String)],
    align: TextAlign,
    valign: VerticalAlign,
) {
    let a = &cx.e.c.analog;
    let n = zones.len().max(1) as f32;
    let lab = if zones.iter().any(|(_, l)| !l.is_empty()) {
        0.16
    } else {
        0.0
    };
    let diam = (d.h / (1.0 + lab))
        .min(d.w / (n + (n - 1.0) * 0.15))
        .max(0.0);
    if diam < 1.0 {
        return;
    }
    let gap = diam * 0.15;
    let total = n * diam + (n - 1.0) * gap;
    let x0 = match align {
        TextAlign::Left => 0.0,
        TextAlign::Center => (d.w - total) / 2.0,
        TextAlign::Right => d.w - total,
    };
    let gh = diam * (1.0 + lab);
    let y0 = match valign {
        VerticalAlign::Top => 0.0,
        VerticalAlign::Middle => (d.h - gh) / 2.0,
        VerticalAlign::Bottom => d.h - gh,
    };
    let ink = if a.hands.trim().is_empty() {
        d.color(Prop::Fill)
    } else {
        Draw::hex(&a.hands)
    };
    let accent = Draw::hex(&a.accent);
    let round = |w: f32| Stroke {
        width: w,
        line_cap: LineCap::Round,
        ..Default::default()
    };
    for (i, (zone, label)) in zones.iter().enumerate() {
        let (ccx, ccy, r) = (
            x0 + i as f32 * (diam + gap) + diam / 2.0,
            y0 + diam / 2.0,
            diam / 2.0,
        );
        let seg = |pb: &mut PathBuilder, deg: f32, r0: f32, r1: f32| {
            let (sn, cs) = deg.to_radians().sin_cos();
            pb.move_to(ccx + sn * r0, ccy - cs * r0);
            pb.line_to(ccx + sn * r1, ccy - cs * r1);
        };
        if !a.face.trim().is_empty() {
            if let Some(p) = PathBuilder::from_circle(ccx, ccy, r) {
                d.fill(&p, Draw::hex(&a.face), FillRule::Winding, None);
            }
        }
        if !a.rim.trim().is_empty() {
            if let Some(p) = PathBuilder::from_circle(ccx, ccy, r - diam * 0.015) {
                d.stroke(&p, Draw::hex(&a.rim), &round(diam * 0.03), None);
            }
        }
        // Marks: (count, every, major from, minor from, major width, minor width).
        let marks = match a.ticks {
            Ticks::Hours => Some((
                12,
                3,
                0.78,
                0.84,
                (diam * 0.02).max(1.5),
                (diam * 0.02).max(1.5),
            )),
            Ticks::Minutes => Some((
                60,
                5,
                0.80,
                0.88,
                (diam * 0.022).max(1.5),
                (diam * 0.008).max(1.0),
            )),
            Ticks::None => None,
        };
        if let Some((count, every, major, minor, wmaj, wmin)) = marks {
            for big in [true, false] {
                let mut pb = PathBuilder::new();
                for k in 0..count {
                    if (k % every == 0) == big {
                        let from = if big { major } else { minor };
                        seg(&mut pb, k as f32 * 360.0 / count as f32, r * from, r * 0.92);
                    }
                }
                if let Some(p) = pb.finish() {
                    d.stroke(&p, ink, &round(if big { wmaj } else { wmin }), None);
                }
            }
        }
        if a.numbers {
            let rr = r * if a.ticks == Ticks::None { 0.8 } else { 0.64 };
            let sz = diam * 0.11;
            let font = cx.font.clone();
            for k in 1..=12 {
                let (sn, cs) = (k as f32 * 30.0).to_radians().sin_cos();
                let (nx, ny) = (ccx + sn * rr, ccy - cs * rr);
                if let Some(p) = Draw::text_path(
                    &font,
                    &k.to_string(),
                    sz,
                    0.0,
                    TextAlign::Center,
                    VerticalAlign::Middle,
                    (nx - sz, ny - sz * LH / 2.0, sz * 2.0, sz * LH),
                    LH,
                ) {
                    d.fill(&p, ink, FillRule::Winding, None);
                }
            }
        }
        if let Some([ha, ma, sa]) = hands(&cx.e, st, at, zone.as_ref()) {
            for (deg, len, w) in [(ha, 0.5, 0.06), (ma, 0.74, 0.04)] {
                let mut pb = PathBuilder::new();
                seg(&mut pb, deg as f32, -r * 0.08, r * len);
                if let Some(p) = pb.finish() {
                    d.stroke(&p, ink, &round(diam * w), None);
                }
            }
            if a.seconds {
                let mut pb = PathBuilder::new();
                seg(&mut pb, sa as f32, -r * 0.15, r * 0.86);
                if let Some(p) = pb.finish() {
                    d.stroke(&p, accent, &round((diam * 0.012).max(1.0)), None);
                }
            }
            if let Some(p) = PathBuilder::from_circle(ccx, ccy, diam * 0.035) {
                d.fill(
                    &p,
                    if a.seconds { accent } else { ink },
                    FillRule::Winding,
                    None,
                );
            }
        }
        if !label.is_empty() {
            if let Some(f) = cx.afont.clone() {
                let sz = diam * 0.11;
                if let Some(p) = Draw::text_path(
                    &f,
                    label,
                    sz,
                    0.0,
                    TextAlign::Center,
                    VerticalAlign::Middle,
                    (ccx - diam, y0 + diam + diam * 0.02, diam * 2.0, diam * lab),
                    LH,
                ) {
                    d.fill(&p, affix_color(cx, d), FillRule::Winding, None);
                }
            }
        }
    }
}

fn affix_color(cx: &Ctx, d: &Draw) -> Color {
    if cx.e.c.affix.color.trim().is_empty() {
        d.color(Prop::Fill)
    } else {
        Draw::hex(&cx.e.c.affix.color)
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_plate(
    d: &mut Draw,
    cx: &Ctx,
    pl: &Plate,
    x: f32,
    y: f32,
    k: f32,
    sep_alpha: f32,
    upper: bool,
) {
    let c = cx.e.c;
    let disp = &pl.disp;
    let (pl_l, _, _) = cx.pad(k);
    let pw = cx.plate_w(pl, k);
    let ph = cx.plate_h(k);
    let size = cx.size * k;
    let cy = y + ph / 2.0;
    // Plate (or the background of a style step).
    let bg = disp
        .bg
        .as_deref()
        .map(Draw::hex)
        .or_else(|| c.plate.on.then(|| Draw::hex(&c.plate.color)));
    if let Some(bg) = bg {
        let (bx, bw, by, bh) = if c.plate.on {
            (x, pw, y, ph)
        } else {
            let px = c.plate.pad_x * k;
            let pyy = (c.plate.pad_y * k).max(size * 0.08);
            (x - px, pw + 2.0 * px, y - pyy, ph + 2.0 * pyy)
        };
        let radius = c.plate.radius * k;
        if c.plate.shadow && c.plate.on {
            // Soft drop shadow (CSS: 0 18px 50px rgba(0,0,0,.45)), in layers.
            for i in 0..10 {
                let g = i as f32 * 3.5 * k;
                if let Some(p) = rrect(
                    bx - g,
                    by + 18.0 * k - g,
                    bw + 2.0 * g,
                    bh + 2.0 * g,
                    radius + g,
                ) {
                    d.fill(&p, Color([0.0, 0.0, 0.0, 0.035]), FillRule::Winding, None);
                }
            }
        }
        if let Some(p) = rrect(bx, by, bw, bh, radius) {
            d.fill(&p, bg, FillRule::Winding, None);
        }
        if c.plate.on && c.plate.accent {
            let aw = (size * 0.08).max(8.0);
            if let Some(m) = d.clip(bx, by, aw, bh) {
                if let Some(p) = rrect(bx, by, bw, bh, radius) {
                    d.fill(
                        &p,
                        Draw::hex(&c.plate.accent_color),
                        FillRule::Winding,
                        Some(&m),
                    );
                }
            }
        }
    }
    let text_col = disp
        .color
        .as_deref()
        .map(Draw::hex)
        .unwrap_or_else(|| d.color(Prop::Fill));
    let blink_off = disp.blink && d.time.now_ms.rem_euclid(1000) >= 600;
    let val_alpha = if blink_off { 0.15 } else { 1.0 };
    let mut ix = x + pl_l;
    // Prefix.
    let affix = |d: &mut Draw, s: &str, ix: f32, w: f32| {
        if let Some(f) = cx.afont.clone() {
            let sz = cx.affix_size(k);
            if let Some(p) = Draw::text_path(
                &f,
                s,
                sz,
                0.0,
                TextAlign::Left,
                VerticalAlign::Middle,
                (ix, cy - sz * LH / 2.0, w + 10.0, sz * LH),
                LH,
            ) {
                let col = affix_color(cx, d);
                d.fill(&p, col, FillRule::Winding, None);
            }
        }
    };
    if pl.wp > 0.0 {
        affix(d, &cx.prefix, ix, pl.wp * k);
        ix += pl.wp * k + cx.gap(k);
    }
    // Value, aligned in its reserved width.
    let ps = pieces(
        &cx.font,
        &disp.segs,
        &c.sep,
        size,
        cx.ls * k,
        c.tabular,
        sep_alpha,
        upper,
    );
    let wv = width(&ps);
    let wi = pl.wi * k;
    let off = match d.layer_align() {
        TextAlign::Left => 0.0,
        TextAlign::Center => (wi - wv) / 2.0,
        TextAlign::Right => wi - wv,
    };
    let mut vx = ix + off;
    let stroke_w = d.num(Prop::StrokeWidth);
    let stroke_c = d.color(Prop::Stroke);
    for p in &ps {
        let align = if p.cell {
            TextAlign::Center
        } else {
            TextAlign::Left
        };
        let bw = if p.cell { p.w } else { p.w + 10.0 };
        if let Some(path) = Draw::text_path(
            &cx.font,
            &p.text,
            size,
            if p.cell { 0.0 } else { cx.ls * k },
            align,
            VerticalAlign::Middle,
            (vx, cy - size * LH / 2.0, bw, size * LH),
            LH,
        ) {
            let a = p.alpha * val_alpha;
            d.fill(&path, with_alpha(text_col, a), FillRule::Winding, None);
            if stroke_w > 0.0 {
                d.stroke(
                    &path,
                    with_alpha(stroke_c, a),
                    &Stroke {
                        width: stroke_w,
                        ..Default::default()
                    },
                    None,
                );
            }
        }
        vx += p.w;
    }
    ix += wi;
    if pl.ws > 0.0 {
        ix += cx.gap(k);
        affix(d, &cx.suffix, ix, pl.ws * k);
    }
    // Split badge.
    if disp.split {
        if let Some(f) = d.font("Inter", 900) {
            let bs = (size * 0.16).max(18.0);
            let tw = Draw::text_width(&f, "SPLIT", bs, bs * 0.08 * 0.0);
            let (bw, bh) = (tw + 24.0, bs * LH + 6.0);
            let bx = x + pw - 18.0 - bw;
            let by = y - size * 0.22;
            if let Some(p) = rrect(bx, by, bw, bh, 6.0) {
                d.fill(&p, Draw::hex("#facc15"), FillRule::Winding, None);
            }
            if let Some(p) = Draw::text_path(
                &f,
                "SPLIT",
                bs,
                0.0,
                TextAlign::Center,
                VerticalAlign::Middle,
                (bx, by, bw, bh),
                LH,
            ) {
                d.fill(&p, Draw::hex("#1a1200"), FillRule::Winding, None);
            }
        }
    }
}

trait LayerAlign {
    fn layer_align(&self) -> TextAlign;
}

impl LayerAlign for Draw<'_, '_> {
    fn layer_align(&self) -> TextAlign {
        match &self.layer.kind {
            LayerKind::Clock { font, .. } => font.align,
            _ => TextAlign::Center,
        }
    }
}
