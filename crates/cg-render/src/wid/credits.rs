//! Credits: drawing (same picture as `web/src/widgets/credits.tsx`).

use cg_core::wid::credits::{
    blocks, cards, crawl_items, crawl_logo_h, entries, ls, nominal, progress, styled, travel,
    Align, Block, Body, Config, End, Layout, Mode, Start, State, Style,
};
use cg_core::wid::table::Overflow;
use cg_core::{Color, LayerKind, TextAlign};
use tiny_skia::{
    FillRule, GradientStop, LinearGradient, Mask, Paint, PathBuilder, Point, SpreadMode, Transform,
};

use super::table::{fitted_path, rect, Fit};
use super::ticker::zone_mask;
use super::{rrect, Draw};
use crate::text as tx;

/// The generated logo plate's font.
const FAMILY: &str = "Inter";

fn is_asset(s: &str) -> bool {
    s.starts_with('/') || s.contains('.')
}

/// Width of a logo at height `h`.
pub(crate) fn logo_w(d: &mut Draw, src: &str, h: f32) -> f32 {
    if is_asset(src) {
        if let Some((w, hh)) = d.image_size(src) {
            return h * w as f32 / hh.max(1) as f32;
        }
    }
    h * 3.0
}

/// A logo: the picture, else a generated plate with the name.
fn draw_logo(d: &mut Draw, src: &str, x: f32, y: f32, h: f32, m: Option<&Mask>) {
    if is_asset(src) {
        if let Some((w, hh)) = d.image_size(src) {
            let k = h / hh.max(1) as f32;
            let _ = w;
            d.image(src, Transform::from_row(k, 0.0, 0.0, k, x, y), 1.0, m);
            return;
        }
    }
    let w = h * 3.0;
    let u = h / 140.0;
    if let Some(p) = rrect(x + 4.0 * u, y + 4.0 * u, w - 8.0 * u, h - 8.0 * u, 30.0 * u) {
        if let Some(sh) = LinearGradient::new(
            Point::from_xy(x, y),
            Point::from_xy(x + w, y),
            vec![
                GradientStop::new(0.0, tiny_skia::Color::from_rgba8(0xff, 0x3b, 0x5c, 255)),
                GradientStop::new(1.0, tiny_skia::Color::from_rgba8(0x7c, 0x5c, 0xff, 255)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        ) {
            let mut paint = Paint {
                shader: sh,
                anti_alias: true,
                ..Paint::default()
            };
            paint.shader.apply_opacity(d.opacity);
            d.fill_paint(&p, &paint, FillRule::Winding, m);
        }
    }
    for (r, c) in [(38.0, Color::WHITE), (16.0, Draw::hex("#ff3b5c"))] {
        let mut pb = PathBuilder::new();
        pb.push_circle(x + 70.0 * u, y + 70.0 * u, r * u);
        if let Some(p) = pb.finish() {
            d.fill(&p, c, FillRule::Winding, m);
        }
    }
    let name: String = src.to_uppercase().chars().take(18).collect();
    let size = if name.chars().count() > 9 { 30.0 } else { 46.0 } * u;
    if let Some(f) = d.font(FAMILY, 900) {
        let tw = tx::text_width(&f, &name, size, 0.0);
        if let Some(p) = Draw::text_path(
            &f,
            &name,
            size,
            0.0,
            TextAlign::Center,
            cg_core::VerticalAlign::Middle,
            (x + 255.0 * u - tw / 2.0 - 2.0, y, tw + 4.0, h),
            tx::LINE_HEIGHT,
        ) {
            d.fill(&p, Color::WHITE, FillRule::Winding, m);
        }
    }
}

fn ta(a: Align) -> TextAlign {
    match a {
        Align::Left => TextAlign::Left,
        Align::Center => TextAlign::Center,
        Align::Right => TextAlign::Right,
    }
}

/// One line of a style in a box, vertically centred (ellipsis when too long).
#[allow(clippy::too_many_arguments)]
fn line(
    d: &mut Draw,
    family: &str,
    s: &Style,
    t: &str,
    align: TextAlign,
    bx: (f32, f32, f32, f32),
    cut: bool,
    m: Option<&Mask>,
) {
    if t.trim().is_empty() {
        return;
    }
    let Some(f) = d.font(family, s.weight) else {
        return;
    };
    let fit = cut.then_some(Fit {
        overflow: Overflow::Ellipsis,
        min_size: s.size,
        min_squeeze: 100.0,
    });
    let (p, _) = fitted_path(&f, &styled(s, t), s.size, ls(s), align, bx, fit);
    if let Some(p) = p {
        d.fill(&p, Draw::hex(&s.color), FillRule::Winding, m);
    }
}

/// Draws a block whose top is at `y0`.
fn draw_block(d: &mut Draw, c: &Config, b: &Block, y0: f32, m: Option<&Mask>) {
    let w = d.w;
    let (ss, rs, ns) = (&c.st.section, &c.st.role, &c.st.name);
    let al = ta(c.align);
    match &b.body {
        Body::Space => {}
        Body::Section(t) => line(
            d,
            &c.font,
            ss,
            t,
            al,
            (0.0, y0, w, (ss.size * 1.3).round()),
            false,
            m,
        ),
        Body::Logo(src) => {
            let lw = logo_w(d, src, c.logo_h);
            let x = match c.align {
                Align::Left => 0.0,
                Align::Center => (w - lw) / 2.0,
                Align::Right => w - lw,
            };
            draw_logo(d, src, x, y0, c.logo_h, m);
        }
        Body::Lines(row) => match c.layout {
            Layout::Two => {
                let (role, name) = &row[0];
                let split = match c.align {
                    Align::Left => 0.4,
                    Align::Right => 0.6,
                    Align::Center => 0.5,
                };
                let lw = w * split - c.gutter / 2.0;
                let ra = if c.align == Align::Left {
                    TextAlign::Left
                } else {
                    TextAlign::Right
                };
                let na = if c.align == Align::Right {
                    TextAlign::Right
                } else {
                    TextAlign::Left
                };
                line(d, &c.font, rs, role, ra, (0.0, y0, lw, b.role_h), true, m);
                line(
                    d,
                    &c.font,
                    ns,
                    name,
                    na,
                    (lw + c.gutter, y0, w - lw - c.gutter, b.role_h),
                    true,
                    m,
                );
            }
            _ => {
                let n = if c.layout == Layout::Three { 3.0 } else { 1.0 };
                let cw = if n > 1.0 {
                    (w - 2.0 * c.gutter) / 3.0
                } else {
                    w
                };
                let nh = (ns.size * 1.25).round();
                for (j, (role, name)) in row.iter().enumerate() {
                    let x = j as f32 * (cw + c.gutter);
                    if b.role_h > 0.0 {
                        line(d, &c.font, rs, role, al, (x, y0, cw, b.role_h), n > 1.0, m);
                    }
                    line(
                        d,
                        &c.font,
                        ns,
                        name,
                        al,
                        (x, y0 + b.role_h, cw, nh),
                        n > 1.0,
                        m,
                    );
                }
            }
        },
    }
}

/// Draws the layer; false = not handled here (the generic code draws it).
pub(crate) fn draw(d: &mut Draw, st: &State) -> bool {
    let LayerKind::Credits { x: c } = &d.layer.kind else {
        return false;
    };
    let c = c.clone();
    let (w, h) = (d.w, d.h);
    let es = entries(&c, d.data);
    let bm = d.boxed();
    let vert = c.mode != Mode::Crawl;
    // Panel: a band behind the line (crawl) or the whole box.
    if c.panel {
        let col = Draw::hex(&c.panel_color);
        if vert {
            rect(d, 0.0, 0.0, w, h, 6.0, col, bm.as_ref());
        } else {
            let band = h.min((ns_max(&c, h) * 1.6).round());
            rect(
                d,
                0.0,
                ((h - band) / 2.0).round(),
                w,
                band,
                6.0,
                col,
                bm.as_ref(),
            );
        }
    }
    let zm = zone_mask(d, (0.0, 0.0, w, h), c.fade.then_some((!vert, c.fade_size)));
    let zm = zm.as_deref();
    let now = d.time.now_ms;
    let age = d.time.age_secs;
    match c.mode {
        Mode::Roll => {
            let (bl, len) = blocks(&c, &es);
            let t = travel(&c, len, bl.last().map(|b| b.y + b.h / 2.0), h);
            let p = progress(&c, st, t.x, now, age) as f32 * t.x;
            let off = t.a - p;
            if !roll_strip(d, &c, &bl, len, off, zm) {
                for b in &bl {
                    let y = off + b.y;
                    if y > h || y + b.h < 0.0 {
                        continue;
                    }
                    draw_block(d, &c, b, y, zm);
                }
            }
        }
        Mode::Crawl => {
            let lw = |_: &str, lh: f32| -> f32 { lh * 3.0 };
            // Logo widths need the pictures: measure them first.
            let logos: Vec<(String, f32)> = es
                .iter()
                .filter(|e| e.kind == cg_core::wid::credits::Kind::Logo)
                .map(|e| (e.a.clone(), logo_w(d, &e.a, crawl_logo_h(&c, h))))
                .collect();
            let fonts: Vec<_> = [c.st.section.weight, c.st.role.weight, c.st.name.weight]
                .iter()
                .map(|wt| (*wt, d.font(&c.font, *wt)))
                .collect();
            let mut mw = |t: &str, s: &Style| -> f32 {
                fonts
                    .iter()
                    .find(|(wt, _)| *wt == s.weight)
                    .and_then(|(_, f)| f.as_ref())
                    .map(|f| tx::text_width(f, &styled(s, t), s.size, ls(s)))
                    .unwrap_or(0.0)
            };
            let mut lwf = |s: &str, lh: f32| {
                logos
                    .iter()
                    .find(|x| x.0 == s)
                    .map(|x| x.1)
                    .unwrap_or_else(|| lw(s, lh))
            };
            let (items, len) = crawl_items(&c, &es, h, &mut mw, &mut lwf);
            let t = travel(&c, len, items.last().map(|i| i.x + i.w / 2.0), w);
            let p = progress(&c, st, t.x, now, age) as f32 * t.x;
            let off = t.a - p;
            let lh = crawl_logo_h(&c, h);
            for it in &items {
                let x = off + it.x;
                if x > w || x + it.w < 0.0 {
                    continue;
                }
                match &it.body {
                    Body::Section(s) => line(
                        d,
                        &c.font,
                        &c.st.section,
                        s,
                        TextAlign::Left,
                        (x, 0.0, it.w + 4.0, h),
                        false,
                        zm,
                    ),
                    Body::Logo(s) => draw_logo(d, s, x, (h - lh) / 2.0, lh, zm),
                    Body::Lines(row) => {
                        let (role, name) = &row[0];
                        if !role.is_empty() {
                            line(
                                d,
                                &c.font,
                                &c.st.role,
                                role,
                                TextAlign::Left,
                                (x, 0.0, it.role_w, h),
                                false,
                                zm,
                            );
                        }
                        line(
                            d,
                            &c.font,
                            &c.st.name,
                            name,
                            TextAlign::Left,
                            (x + it.role_w, 0.0, it.w - it.role_w + 4.0, h),
                            false,
                            zm,
                        );
                    }
                    Body::Space => {}
                }
            }
        }
        Mode::Pages => {
            let (bl, _) = blocks(&c, &es);
            let cs = cards(&bl, h);
            let n = cs.len();
            if n == 0 {
                return true;
            }
            let nf = n as f32;
            let p = progress(&c, st, nf, now, age) as f32 * nf;
            let rate = st.rate.unwrap_or_else(|| nominal(&c, nf)) as f32 * nf;
            let fk = 0.45f32.min(0.6 * rate.max(0.05));
            let (tf0, op0) = (d.tf, d.opacity);
            for (i, card) in cs.iter().enumerate() {
                let fi = i as f32;
                let inn = if i == 0 {
                    if c.start == Start::On {
                        1.0
                    } else {
                        (p / fk).clamp(0.0, 1.0)
                    }
                } else {
                    ((p - fi + fk / 2.0) / fk).clamp(0.0, 1.0)
                };
                let out = if i + 1 < n {
                    ((fi + 1.0 + fk / 2.0 - p) / fk).clamp(0.0, 1.0)
                } else if c.end == End::Exit {
                    ((nf - p) / fk).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let o = inn.min(out);
                if o <= 0.001 {
                    continue;
                }
                d.opacity = op0 * o;
                d.tf = tf0.pre_translate(0.0, (1.0 - inn) * 30.0);
                let top = (h - card.h) / 2.0;
                for b in &card.blocks {
                    draw_block(d, &c, b, top + b.y, zm);
                }
                d.tf = tf0;
                d.opacity = op0;
            }
        }
    }
    true
}

/// A roll whose content top is at `off` (layer pixels), from a strip of
/// its content drawn once (`strip::roll`). False when the layer is turned or
/// skewed (drawn directly then).
fn roll_strip(
    d: &mut Draw,
    c: &Config,
    bl: &[Block],
    len: f32,
    off: f32,
    zm: Option<&Mask>,
) -> bool {
    use std::hash::{Hash, Hasher};
    let key = {
        let mut k = std::collections::hash_map::DefaultHasher::new();
        format!("{c:?}{bl:?}").hash(&mut k);
        k.finish()
    };
    let margin = crate::strip::MARGIN as f32;
    let sy = d.tf.sy;
    let Draw {
        rd,
        pm,
        r,
        layer,
        time,
        data,
        frame,
        hold,
        fps,
        state,
        lang,
        w,
        h,
        tf,
        opacity,
        ..
    } = d;
    crate::strip::roll(
        rd,
        pm,
        &layer.id,
        key,
        *tf,
        (*w, *h),
        len,
        &[off],
        *opacity,
        zm,
        &mut |rd, chunk, t| {
            // The chunk's rows, in strip rows.
            let (c0, c1) = (margin - t.ty, margin - t.ty + chunk.height() as f32);
            let mut sub = Draw {
                rd,
                pm: chunk,
                r,
                layer,
                tf: t,
                mask: None,
                opacity: 1.0,
                w: *w,
                h: *h,
                time: *time,
                data,
                frame: *frame,
                hold: *hold,
                fps: *fps,
                state,
                lang,
                runs: None,
                inline: Vec::new(),
                follow: None,
                moving: false,
            };
            for b in bl {
                let y0 = sy * b.y + margin;
                let y1 = sy * (b.y + b.h) + margin;
                if y1 + 32.0 < c0 || y0 - 32.0 > c1 {
                    continue;
                }
                draw_block(&mut sub, c, b, b.y, None);
            }
        },
    )
}

/// Tallest thing of the crawl line (panel band).
fn ns_max(c: &Config, h: f32) -> f32 {
    c.st.name
        .size
        .max(c.st.section.size)
        .max(c.logo_h.min(h * 0.7))
}
