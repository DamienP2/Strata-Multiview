//! Text: drawing (same picture as `web/src/widgets/text.tsx`).
//!
//! The fit chain is `text::fit_lines` (the web's `fitLines`); then every
//! line is drawn with the fallback fonts (never an empty box: colour emoji
//! as pictures), its band, shadow, stroke and fill. A scrolling text is left
//! to the ticker / roll code (lib.rs).
//!
//! No text shaping: Arabic letters are drawn in their isolated forms (the
//! browser joins them); right-to-left lines are put in visual order.

#[path = "text_draw.rs"]
pub(crate) mod paint;

use cg_core::wid::text::{BreakMode, Direction, FillType, FitEnd, State};
use cg_core::widgets::scroll_positions;
use cg_core::{
    LayerKind, Prop, TextAlign, TextDecoration, TextOverflow, TextScroll, VerticalAlign,
};
use tiny_skia::{FillRule, Paint, PathBuilder, Stroke, Transform};

use super::{rrect, Draw};
use crate::text::{self as tx, FitOpts};

/// Size of a part above or below the line (exposant / indice).
const SUP: f32 = 0.62;

/// A scrolling text moves; any other does not (see `draw`: only the scroll reads the time).
pub(crate) fn motion(d: &mut Draw) -> Option<crate::cache::Motion> {
    let LayerKind::Text { scroll, .. } = &d.layer.kind else {
        return None;
    };
    Some(if scroll.is_none() {
        crate::cache::Motion::Still
    } else {
        crate::cache::Motion::Moving
    })
}

pub(crate) fn draw(d: &mut Draw, _st: &State) -> bool {
    let LayerKind::Text {
        font_family,
        font_weight,
        align,
        vertical_align,
        wrap,
        decoration,
        line_height,
        overflow,
        squeeze,
        min_size,
        scroll,
        speed,
        separator,
        looping,
        x: c,
        ..
    } = &d.layer.kind
    else {
        return false;
    };
    let Some(content) = d.r.text.clone().filter(|t| !t.trim().is_empty()) else {
        return true; // empty: nothing on air, the box keeps its size
    };
    // Scrolling: a ticker (the lines one after the other, with the
    // separator) or a roll (the lines going up), with all the text's looks.
    let scrolling = !scroll.is_none();
    let horizontal = *scroll == TextScroll::Horizontal;
    let content = if horizontal {
        content.replace('\n', separator) + separator
    } else {
        content
    };
    let Some(main) = paint::main_font(d, font_family, *font_weight) else {
        return true;
    };
    let chain = paint::chain(d, main.clone(), &content);
    let rtl = c.dir == Direction::Rtl;
    // Styled parts (not for right-to-left lines, drawn in visual order).
    let runs = d.runs.clone().filter(|_| !rtl).map(|mut r| {
        if horizontal {
            for x in &mut r {
                x.text = x.text.replace('\n', separator);
            }
            if let Some(last) = r.last_mut() {
                last.text.push_str(separator);
            }
        }
        r
    });
    let size0 = d.num(Prop::FontSize);
    // The font of each part: the text's, or its own (a variable's font,
    // weight, bold): 0 is the text's, n the n-th of `chains`.
    let mut chains: Vec<tx::Chain> = Vec::new();
    let mut keys: Vec<(String, u16)> = vec![(font_family.clone(), *font_weight)];
    let mut run_chain: Vec<usize> = Vec::new();
    for r in runs.iter().flatten() {
        let st = &r.style;
        let family = st
            .font_family
            .clone()
            .unwrap_or_else(|| font_family.clone());
        let mut weight = st.font_weight.unwrap_or(*font_weight);
        if st.bold {
            weight = (weight.max(400) + 300).min(900);
        }
        let key = (family, weight);
        let at = match keys.iter().position(|k| *k == key) {
            Some(i) => i,
            None => match paint::main_font(d, &key.0, key.1) {
                Some(f) => {
                    chains.push(paint::chain(d, f, &content));
                    keys.push(key);
                    chains.len()
                }
                None => 0,
            },
        };
        run_chain.push(at);
    }
    let ls = d.num(Prop::LetterSpacing);
    let lh = line_height.unwrap_or(tx::LINE_HEIGHT);
    let pad = if c.band { c.band_pad_x.max(0.0) } else { 0.0 };
    let avail = if c.band {
        (d.w - 2.0 * pad).max(20.0)
    } else {
        d.w
    };
    // Size factor, letter spacing and baseline shift of each part.
    let style_of = |run: usize| runs.as_ref().map(|r| &r[run].style);
    let scale_of = |run: usize| {
        style_of(run).map_or(1.0, |st| {
            let own = st
                .font_size
                .filter(|_| size0 > 0.0)
                .map_or(1.0, |fs| fs / size0);
            own * if st.sup || st.sub { SUP } else { 1.0 }
        })
    };
    let ls_of = |run: usize| style_of(run).and_then(|st| st.letter_spacing).unwrap_or(ls);
    let chain_of = |run: usize| match run_chain.get(run).copied().unwrap_or(0) {
        0 => &chain,
        n => &chains[n - 1],
    };
    // The line breaking measures each part in its own style.
    let (encoded, table) = match (&runs, scrolling) {
        (Some(r), false) => cg_core::rich::encode(r),
        _ => (content.clone(), Vec::new()),
    };
    let opts = FitOpts {
        size: size0,
        line_height: lh,
        box_w: avail,
        box_h: d.h,
        wrap: *wrap,
        overflow: *overflow,
        squeeze: *squeeze,
        min_size: *min_size,
        max_lines: c.max_lines as usize,
        hyphen: c.break_mode == BreakMode::Hyphen,
        lang: if c.lang.is_empty() { d.lang } else { &c.lang },
        ellipsis_end: c.fit_end == FitEnd::Ellipsis,
    };
    let measure = &chain;
    // Width of a line of the encoded text: each part in its own style, the
    // rest in the text's.
    let styled_width = |s: &str, z: f32| -> f32 {
        if table.is_empty() {
            return tx::chain_width(&chain, s, z, ls);
        }
        let mut total = 0.0;
        let mut parts = 0;
        let mut cur = String::new();
        let mut cur_run: Option<usize> = None;
        let mut flush = |cur: &mut String, run: Option<usize>| {
            if cur.is_empty() {
                return;
            }
            let w = match run {
                Some(i) => tx::chain_width(chain_of(i), cur, z * scale_of(i), ls_of(i)),
                None => tx::chain_width(&chain, cur, z, ls),
            };
            total += w;
            parts += 1;
            cur.clear();
        };
        for c in s.chars() {
            let (ch, run) = match cg_core::rich::decode(c, &table) {
                Some((ch, i)) => (ch, Some(i)),
                None => (c, None),
            };
            if run != cur_run {
                flush(&mut cur, cur_run);
                cur_run = run;
            }
            cur.push(ch);
        }
        flush(&mut cur, cur_run);
        total + ls * (parts.max(1) - 1) as f32
    };
    let fit = if horizontal {
        tx::FitLines {
            lines: vec![tx::FitLine {
                text: content.clone(),
                end: true,
            }],
            size: size0,
            squeeze: 1.0,
            clip: true,
            ratio: 1.0,
            overflows: false,
            stage: "fit",
            forced: false,
            hyphenated: false,
        }
    } else if scrolling {
        // A roll: every line at its size, however long it is.
        let o = FitOpts {
            box_h: f32::MAX / 4.0,
            overflow: TextOverflow::Visible,
            ..opts
        };
        let mut f = tx::fit_lines(&content, &o, &mut |s, z| tx::chain_width(measure, s, z, ls));
        f.clip = true;
        f
    } else {
        let mut f = tx::fit_lines(&encoded, &opts, &mut |s, z| styled_width(s, z));
        for l in &mut f.lines {
            l.text = cg_core::rich::decoded(&l.text, &table);
        }
        f
    };
    let (size, sx) = (fit.size, fit.squeeze);
    let parts = runs.as_ref().map(|r| {
        let lines: Vec<&str> = fit.lines.iter().map(|l| l.text.as_str()).collect();
        cg_core::rich::align(r, &lines)
    });
    let bw = avail / sx; // the box the lines are aligned in, before squeezing
    let line_h = size * lh;
    // A line with a bigger part is as much taller, the room added above it.
    let heights: Vec<f32> = (0..fit.lines.len())
        .map(|i| {
            let grow = parts.as_ref().map_or(1.0, |p| {
                p[i].iter()
                    .map(|(_, run)| scale_of(*run))
                    .fold(1.0, f32::max)
            });
            line_h * grow
        })
        .collect();
    let mut tops = Vec::with_capacity(heights.len());
    let mut block = 0.0;
    for h in &heights {
        tops.push(block);
        block += h;
    }
    let top = match vertical_align {
        VerticalAlign::Top => 0.0,
        VerticalAlign::Middle => (d.h - block) / 2.0,
        VerticalAlign::Bottom => d.h - block,
    };
    let (asc, desc) = tx::metrics(&main, size);
    let half = (line_h - (asc - desc)) / 2.0;
    // Where the text is drawn: once, or each copy of a scroll on its way.
    let travelled = speed * d.time.age_secs;
    let placements: Vec<(f32, f32)> = if horizontal {
        let cycle = tx::chain_width(measure, &content, size, ls) + ls;
        scroll_positions(d.w as f64, cycle as f64, travelled, true)
            .into_iter()
            .map(|x| (x as f32, 0.0))
            .collect()
    } else if scrolling {
        // A screen-high gap before the roll comes round again.
        let cycle = if *looping { block + d.h } else { block };
        scroll_positions(d.h as f64, cycle as f64, travelled, *looping)
            .into_iter()
            .map(|y| (0.0, y as f32 - top))
            .collect()
    } else {
        vec![(0.0, 0.0)]
    };

    let clip = if fit.clip { d.boxed() } else { None };
    let mask = clip.as_ref();
    let mut text_pb = PathBuilder::new();
    let mut deco_pb = PathBuilder::new();
    // Parts in their own colour: (colour, glyphs and lines).
    let mut colored: Vec<(String, PathBuilder)> = Vec::new();
    let mut pictures = Vec::new();
    let mut bands = Vec::new();
    for &(ox, oy) in &placements {
        let top = top + oy;
        for (i, l) in fit.lines.iter().enumerate() {
            let shown = if rtl {
                tx::visual_rtl(&l.text)
            } else {
                l.text.clone()
            };
            let spaces = shown.chars().filter(|ch| *ch == ' ').count();
            let segs = parts.as_ref().map(|p| &p[i]);
            let w = match segs {
                Some(segs) => {
                    segs.iter()
                        .map(|(t, run)| {
                            tx::chain_width(chain_of(*run), t, size * scale_of(*run), ls_of(*run))
                        })
                        .sum::<f32>()
                        + ls * segs.len().saturating_sub(1) as f32
                }
                None => tx::chain_width(&chain, &shown, size, ls),
            };
            let justified = c.justify && !l.end && spaces > 0 && w < bw;
            let extra = if justified {
                (bw - w) / spaces as f32
            } else {
                0.0
            };
            let lw = if justified { bw } else { w };
            let eff = if c.justify && !justified {
                if rtl {
                    TextAlign::Right
                } else {
                    TextAlign::Left
                }
            } else {
                *align
            };
            let x = match eff {
                _ if horizontal => ox,
                _ if justified => 0.0,
                TextAlign::Left => 0.0,
                TextAlign::Center => (bw - lw) / 2.0,
                TextAlign::Right => bw - lw,
            };
            let baseline = top + tops[i] + heights[i] - line_h + half + asc;
            let (lx, lwx) = (pad + x * sx, lw * sx);
            let thick = (size * 0.06).max(1.0);
            let bar = |pb: &mut PathBuilder, y: f32, x0: f32, w: f32| {
                if let Some(r) = tiny_skia::Rect::from_xywh(x0, y - thick / 2.0, w, thick) {
                    pb.push_rect(r);
                }
            };
            // Underline / strike through, as layout_text draws them.
            let under = baseline + size * 0.1;
            let through = baseline - size * 0.3;
            match segs {
                // One part after the other, each in its style.
                Some(segs) => {
                    let mut px = pad + x;
                    for (t, run) in segs {
                        let st = &runs.as_ref().expect("parts come from runs")[*run].style;
                        // Above or below the line (exposant / indice).
                        let shift = if st.sup {
                            -size * 0.33
                        } else if st.sub {
                            size * 0.12
                        } else {
                            0.0
                        };
                        let lp = paint::line(
                            chain_of(*run),
                            t,
                            size * scale_of(*run),
                            ls_of(*run),
                            extra,
                            px,
                            baseline + shift,
                            sx,
                            pad,
                            if c.italic || st.italic { 0.25 } else { 0.0 },
                        );
                        let pb = match &st.color {
                            Some(col) => match colored.iter().position(|(c, _)| c == col) {
                                Some(k) => &mut colored[k].1,
                                None => {
                                    colored.push((col.clone(), PathBuilder::new()));
                                    &mut colored.last_mut().expect("pushed").1
                                }
                            },
                            None => &mut text_pb,
                        };
                        if let Some(p) = &lp.path {
                            pb.push_path(p);
                        }
                        let (sx0, sw0) = (pad + (px - pad) * sx, lp.width * sx);
                        if sw0 > 0.0 {
                            if st.underline || *decoration == TextDecoration::Underline {
                                bar(pb, under, sx0, sw0);
                            }
                            if st.strike || *decoration == TextDecoration::LineThrough {
                                bar(pb, through, sx0, sw0);
                            }
                        }
                        pictures.extend(lp.pictures);
                        px += lp.width + ls;
                    }
                }
                None => {
                    let lp = paint::line(
                        &chain,
                        &shown,
                        size,
                        ls,
                        extra,
                        pad + x,
                        baseline,
                        sx,
                        pad,
                        if c.italic { 0.25 } else { 0.0 },
                    );
                    if let Some(p) = &lp.path {
                        text_pb.push_path(p);
                    }
                    pictures.extend(lp.pictures);
                    let y = match decoration {
                        TextDecoration::Underline => Some(under),
                        TextDecoration::LineThrough => Some(through),
                        TextDecoration::None => None,
                    };
                    if let (Some(y), true) = (y, lw > 0.0) {
                        bar(&mut deco_pb, y, lx, lwx);
                    }
                }
            }
            if c.band && !l.text.trim().is_empty() {
                bands.push((
                    lx - pad,
                    top + tops[i] + c.band_gap / 2.0,
                    lwx + 2.0 * pad,
                    (heights[i] - c.band_gap).max(4.0),
                ));
            }
        }
    }
    if let Some(dp) = deco_pb.finish() {
        text_pb.push_path(&dp);
    }
    // Bands behind the lines.
    let band_col = Draw::hex(&c.band_color);
    let accent = Draw::hex(&c.band_accent_color);
    for (bx, by, bwid, bh) in &bands {
        if let Some(p) = rrect(*bx, *by, *bwid, *bh, c.band_radius) {
            d.fill(&p, band_col, FillRule::Winding, mask);
            if c.band_accent {
                let ax = if rtl { bx + bwid - 8.0 } else { *bx };
                if let Some(m) = d.clip(ax, *by, 8.0, *bh).map(|m| combine(&m, mask)) {
                    d.fill(&p, accent, FillRule::Winding, Some(&m));
                }
            }
        }
    }
    let colored: Vec<(cg_core::Color, tiny_skia::Path)> = colored
        .into_iter()
        .filter_map(|(c, pb)| Some((Draw::hex(&c), pb.finish()?)))
        .collect();
    let path = text_pb.finish();
    // Shadow and strokes go around every part, whatever its colour.
    let all: Vec<&tiny_skia::Path> = path.iter().chain(colored.iter().map(|(_, p)| p)).collect();
    let sw = d.num(Prop::StrokeWidth);
    let stroke_col = d.color(Prop::Stroke);
    if c.shadow {
        // As the browser's drop shadow: under the stroke (outside or centred)
        // and the colour emoji too.
        let st = match (c.stroke_outside, sw > 0.0) {
            (_, false) => 0.0,
            (true, true) => sw * 2.0,
            (false, true) => sw,
        };
        let pics: Vec<_> = pictures
            .iter()
            .filter_map(|(g, x, baseline)| {
                paint::picture_at(&chain, g, *x, *baseline, size, sx, Transform::identity())
            })
            .collect();
        paint::shadow_with(
            d,
            &all,
            &pics,
            st,
            Draw::hex(&c.shadow_color),
            c.shadow_x,
            c.shadow_y,
            c.shadow_blur,
            mask,
        );
    }
    if path.is_none() && colored.is_empty() {
        draw_pictures(d, &chain, &pictures, size, sx, mask);
        return true;
    }
    if c.stroke_outside && sw > 0.0 {
        let s = Stroke {
            width: sw * 2.0,
            line_join: tiny_skia::LineJoin::Round,
            ..Default::default()
        };
        for p in &all {
            d.stroke(p, stroke_col, &s, mask);
        }
    }
    for (col, p) in &colored {
        d.fill(p, *col, FillRule::Winding, mask);
    }
    match (c.fill_type, &path) {
        (_, None) => {}
        (FillType::Solid, Some(path)) => d.fill(path, d.color(Prop::Fill), FillRule::Winding, mask),
        (FillType::Gradient, Some(path)) => {
            if let Some(shader) = paint::gradient(
                d.color(Prop::Fill),
                // Animatable (`fill2`; unset, `color2`).
                d.color(Prop::Fill2),
                c.grad_angle,
                d.w.max(1.0),
                d.h.max(1.0),
                d.opacity,
            ) {
                let p = Paint {
                    shader,
                    anti_alias: true,
                    ..Default::default()
                };
                d.fill_paint(path, &p, FillRule::Winding, mask);
            }
        }
    }
    if !c.stroke_outside && sw > 0.0 {
        let s = Stroke {
            width: sw,
            ..Default::default()
        };
        for p in &all {
            d.stroke(p, stroke_col, &s, mask);
        }
    }
    draw_pictures(d, &chain, &pictures, size, sx, mask);
    true
}

fn draw_pictures(
    d: &mut Draw,
    chain: &tx::Chain,
    pics: &[(tx::PGlyph, f32, f32)],
    size: f32,
    sx: f32,
    mask: Option<&tiny_skia::Mask>,
) {
    for (g, x, baseline) in pics {
        paint::picture(d, chain, g, *x, *baseline, size, sx, 1.0, mask);
    }
}

/// `a` ∩ `b` (b optional).
fn combine(a: &tiny_skia::Mask, b: Option<&tiny_skia::Mask>) -> tiny_skia::Mask {
    let mut m = a.clone();
    if let Some(b) = b {
        for (x, y) in m.data_mut().iter_mut().zip(b.data()) {
            *x = ((*x as u16 * *y as u16) / 255) as u8;
        }
    }
    m
}
