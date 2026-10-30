//! Table: drawing (same picture as `web/src/widgets/table.tsx`).

use ab_glyph::FontArc;
use cg_core::wid::table::{
    cell_in, cell_text, col_text, current_page, header_h, header_text, live, merge_of, model,
    page_count, rows_text, title_h as title_height, Align, CascadeStyle, ColKind, Config, Deco,
    EmptyMode, FitMode, MRow, Model, Overflow, PageMode, RowPaint, State, Text, Then,
};
use cg_core::{Color, LayerKind, Prop, TextAlign, VerticalAlign};
use tiny_skia::{
    FillRule, GradientStop, LinearGradient, Mask, Paint, Point, SpreadMode, Transform,
};

use super::{rrect, Draw};
use crate::text as tx;

pub(crate) const FAMILY: &str = "Inter";

/// Fills a rectangle (or a rounded one).
pub(crate) fn rect(
    d: &mut Draw,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    c: Color,
    m: Option<&Mask>,
) {
    if w <= 0.0 || h <= 0.0 || c.0[3] <= 0.0 {
        return;
    }
    if let Some(p) = rrect(x, y, w, h, r) {
        d.fill(&p, c, FillRule::Winding, m);
    }
}

pub(crate) fn with_alpha(c: Color, a: f32) -> Color {
    Color([c.0[0], c.0[1], c.0[2], c.0[3] * a])
}

/// How a too-long text fits its box: shrink → squeeze → ellipsis.
#[derive(Clone, Copy)]
pub(crate) struct Fit {
    pub overflow: Overflow,
    pub min_size: f32,
    pub min_squeeze: f32,
}

/// Lays out one line of text in (x, y, w, h), fitted; returns the path and whether it was fitted.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fitted_path(
    font: &FontArc,
    s: &str,
    size: f32,
    ls: f32,
    align: TextAlign,
    (x, y, w, h): (f32, f32, f32, f32),
    fit: Option<Fit>,
) -> (Option<tiny_skia::Path>, bool) {
    let mut style = tx::TextStyle {
        size,
        letter_spacing: ls,
        align,
        vertical_align: VerticalAlign::Middle,
        box_w: w.max(1.0),
        box_h: h.max(size),
        line_height: tx::LINE_HEIGHT,
        decoration: cg_core::TextDecoration::None,
    };
    let (mut text, mut sx, mut changed) = (s.to_string(), 1.0f32, false);
    if let Some(f) = fit {
        use cg_core::TextOverflow as O;
        let r = match f.overflow {
            Overflow::Ellipsis => tx::fit_text(font, s, &style, false, O::Ellipsis, 1.0, 1.0),
            Overflow::Squeeze => {
                tx::fit_text(font, s, &style, false, O::Fit, f.min_squeeze / 100.0, 1.0)
            }
            Overflow::Shrink => tx::fit_text(
                font,
                s,
                &style,
                false,
                O::Fit,
                f.min_squeeze / 100.0,
                (f.min_size / size).min(1.0),
            ),
        };
        changed = r.size != size || r.squeeze != 1.0 || r.text != s;
        style.size = r.size;
        sx = r.squeeze;
        text = r.text;
        if r.clip && f.overflow != Overflow::Ellipsis {
            // At the minimum: cut with an ellipsis.
            let mut st2 = tx::TextStyle {
                box_w: w / sx,
                ..style
            };
            st2.size = r.size;
            text = tx::fit_text(font, &text, &st2, false, O::Ellipsis, 1.0, 1.0).text;
        }
    }
    style.box_w = (w / sx).max(1.0);
    let p = tx::layout_text(font, &text, &style)
        .and_then(|p| p.transform(Transform::from_row(sx, 0.0, 0.0, 1.0, x, y)));
    (p, changed)
}

/// A text cut to `max_w`: with "…" (`dots`) or plainly.
fn cut_to(width: &dyn Fn(&str, f32) -> f32, s: &str, size: f32, max_w: f32, dots: bool) -> String {
    if width(s, size) <= max_w + 0.5 {
        return s.to_string();
    }
    let mut ch: Vec<char> = s.chars().collect();
    loop {
        let t: String = ch.iter().collect();
        let t = if dots {
            t.trim_end().to_string() + "…"
        } else {
            t
        };
        if ch.is_empty() || width(&t, size) <= max_w + 0.5 {
            return t;
        }
        ch.pop();
    }
}

/// Lines of `s` at most `max_w` wide: between words, or anywhere.
fn wrap_cell(
    width: &dyn Fn(&str, f32) -> f32,
    s: &str,
    size: f32,
    max_w: f32,
    any: bool,
) -> Vec<String> {
    let mut out = Vec::new();
    for para in s.split('\n') {
        let mut cur = String::new();
        if any {
            for ch in para.chars() {
                let next = format!("{cur}{ch}");
                if !cur.is_empty() && width(&next, size) > max_w + 0.5 {
                    out.push(std::mem::take(&mut cur).trim_end().to_string());
                    cur = ch.to_string().trim_start().to_string();
                } else {
                    cur = next;
                }
            }
        } else {
            for word in para.split(' ') {
                let next = if cur.is_empty() {
                    word.to_string()
                } else {
                    format!("{cur} {word}")
                };
                if !cur.is_empty() && width(&next, size) > max_w + 0.5 {
                    out.push(std::mem::take(&mut cur));
                    cur = word.to_string();
                } else {
                    cur = next;
                }
            }
        }
        out.push(cur);
    }
    out
}

fn block_h(n: usize, size: f32, lh: f32) -> f32 {
    if n == 0 {
        0.0
    } else {
        (n - 1) as f32 * size * lh + size
    }
}

/// How a text fits its cell with its level's settings: (lines, size, horizontal scale).
/// Mirrors `fitCell` of web/src/widgets/table-text.tsx.
pub(crate) fn fit_cell(
    width: &dyn Fn(&str, f32) -> f32,
    t: &Text,
    s: &str,
    w: f32,
    h: f32,
) -> (String, f32, f32) {
    let size0 = t.size.max(1.0);
    let (w, h) = (w.max(1.0), h.max(1.0));
    let dots_after_fit = t.then == Then::Ellipsis;
    if !t.multi {
        let w0 = width(s, size0);
        if w0 <= w + 0.5 {
            return (s.to_string(), size0, 1.0);
        }
        return match t.fit {
            FitMode::Ellipsis => (cut_to(width, s, size0, w, true), size0, 1.0),
            FitMode::Clip => (cut_to(width, s, size0, w, false), size0, 1.0),
            FitMode::Fit => {
                let narrow = t.narrow / 100.0;
                if w / w0 >= narrow {
                    return (s.to_string(), size0, w / w0);
                }
                let min = size0 * t.small / 100.0;
                let fits = |sz: f32| width(s, sz) * narrow <= w + 0.5;
                if !fits(min) {
                    return (
                        cut_to(width, s, min, w / narrow, dots_after_fit),
                        min,
                        narrow,
                    );
                }
                let (mut lo, mut hi) = (min, size0);
                for _ in 0..14 {
                    let mid = (lo + hi) / 2.0;
                    if fits(mid) {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                let sx = (w / width(s, lo)).clamp(narrow, 1.0);
                (s.to_string(), lo, sx)
            }
        };
    }
    let max = t.max.max(1) as usize;
    let lines = |sz: f32| wrap_cell(width, s, sz, w, t.any_break);
    let fits = |sz: f32| {
        let l = lines(sz);
        l.len() <= max
            && block_h(l.len(), sz, t.lh) <= h + 0.5
            && l.iter().all(|x| width(x, sz) <= w + 0.5)
    };
    if fits(size0) {
        return (lines(size0).join("\n"), size0, 1.0);
    }
    let mut size = size0;
    if t.fit == FitMode::Fit {
        let min = size0 * t.small / 100.0;
        if fits(min) {
            let (mut lo, mut hi) = (min, size0);
            for _ in 0..14 {
                let mid = (lo + hi) / 2.0;
                if fits(mid) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            return (lines(lo).join("\n"), lo, 1.0);
        }
        size = min;
    }
    // Still too long: the lines that fit, ending with "…" or a cut.
    let dots = match t.fit {
        FitMode::Ellipsis => true,
        FitMode::Clip => false,
        FitMode::Fit => dots_after_fit,
    };
    let room = (((h - size) / (size * t.lh)).floor().max(0.0) as usize + 1)
        .min(max)
        .max(1);
    let mut l = lines(size);
    let more = l.len() > room;
    l.truncate(room);
    let n = l.len();
    for (i, line) in l.iter_mut().enumerate() {
        if i + 1 == n && more && dots {
            *line = cut_to(width, &format!("{line}…"), size, w, true);
            if !line.ends_with('…') {
                line.push('…');
            }
        } else {
            *line = cut_to(width, line, size, w, dots);
        }
    }
    (l.join("\n"), size, 1.0)
}

/// A text in (x, y, w, h) with its level's typography (font, case, spacing, lines, italic,
/// underline) and too-long rule.
pub(crate) fn text_path(
    d: &mut Draw,
    t: &Text,
    s: &str,
    align: TextAlign,
    (x, y, w, h): (f32, f32, f32, f32),
) -> Option<tiny_skia::Path> {
    let font = super::text::paint::main_font(d, &t.family, t.weight)?;
    let s = t.cased(s);
    let ls = t.spacing;
    let width = |txt: &str, size: f32| tx::text_width(&font, txt, size, ls);
    let (text, size, sx) = fit_cell(&width, t, &s, w, h);
    let style = tx::TextStyle {
        size,
        letter_spacing: ls,
        align,
        vertical_align: VerticalAlign::Middle,
        box_w: (w / sx).max(1.0),
        box_h: h.max(size),
        line_height: if t.multi { t.lh } else { tx::LINE_HEIGHT },
        decoration: match t.deco {
            Deco::None => cg_core::TextDecoration::None,
            Deco::Underline => cg_core::TextDecoration::Underline,
            Deco::Strike => cg_core::TextDecoration::LineThrough,
        },
    };
    let skew = if t.italic { 0.25 } else { 0.0 };
    tx::layout_text(&font, &text, &style).and_then(|p| {
        p.transform(Transform::from_row(
            sx,
            0.0,
            -skew,
            1.0,
            x + skew * h / 2.0,
            y,
        ))
    })
}

pub(crate) fn align_of(a: Align) -> TextAlign {
    match a {
        Align::Left => TextAlign::Left,
        Align::Center => TextAlign::Center,
        Align::Right => TextAlign::Right,
    }
}

/// Column widths for the mode (fixed px, auto from the content, % of the width).
pub(crate) fn widths(d: &mut Draw, c: &Config, m: &Model, size: f32) -> Vec<f32> {
    let n = m.cols.len();
    let avail = d.w - c.gap * (n.max(1) - 1) as f32;
    match c.width_mode {
        cg_core::wid::table::WidthMode::Fixed => m.cols.iter().map(|x| x.col.w).collect(),
        cg_core::wid::table::WidthMode::Percent => {
            let rk = if m.cols.iter().any(|x| x.rank()) {
                80.0
            } else {
                0.0
            };
            // Shares of the width: always 100 % in total, whatever the numbers typed.
            let sum: f32 = m
                .cols
                .iter()
                .filter(|x| !x.rank())
                .map(|x| x.col.pct.max(0.0))
                .sum();
            let sum = if sum > 0.0 { sum } else { 1.0 };
            m.cols
                .iter()
                .map(|x| {
                    if x.rank() {
                        80.0
                    } else {
                        x.col.pct.max(0.0) / sum * (avail - rk)
                    }
                })
                .collect()
        }
        cg_core::wid::table::WidthMode::Auto => {
            let ht = header_text(c);
            let Some(hf) = super::text::paint::main_font(d, &ht.family, ht.weight) else {
                return vec![avail / n.max(1) as f32; n];
            };
            let mut nat = Vec::with_capacity(n);
            for x in &m.cols {
                if x.col.kind == ColKind::Image {
                    nat.push(c.row_h * 0.75 + 2.0 * c.pad);
                    continue;
                }
                let t = col_text(c, x, size);
                let Some(f) = super::text::paint::main_font(d, &t.family, t.weight) else {
                    nat.push(avail / n.max(1) as f32);
                    continue;
                };
                let mut w = tx::text_width(&hf, &ht.cased(&x.col.label), ht.size, ht.spacing);
                for r in &m.rows {
                    w = w.max(tx::text_width(
                        &f,
                        &t.cased(&cell_text(x, r)),
                        t.size,
                        t.spacing,
                    ));
                }
                nat.push(w.min(600.0) + 2.0 * c.pad + 8.0);
            }
            let is_text: Vec<bool> = m
                .cols
                .iter()
                .map(|x| x.col.kind == ColKind::Text && !x.rank())
                .collect();
            let fixed: f32 = nat
                .iter()
                .zip(&is_text)
                .filter(|(_, t)| !**t)
                .map(|(w, _)| w)
                .sum();
            let text_nat: f32 = nat
                .iter()
                .zip(&is_text)
                .filter(|(_, t)| **t)
                .map(|(w, _)| w)
                .sum();
            let rest = avail - fixed;
            if text_nat <= 0.0 || rest < text_nat * 0.25 {
                let sum: f32 = nat.iter().sum::<f32>().max(1.0);
                return nat.iter().map(|w| w * avail / sum).collect();
            }
            nat.iter()
                .zip(&is_text)
                .map(|(w, t)| if *t { w * rest / text_nat } else { *w })
                .collect()
        }
    }
}

fn ease_out(k: f32) -> f32 {
    1.0 - (1.0 - k.clamp(0.0, 1.0)).powi(3)
}

struct RowLook {
    bg: Color,
    color: Color,
    hl: bool,
    top3: bool,
}

fn row_look(c: &Config, st: &State, row: &MRow) -> RowLook {
    let h = |s: &str| Draw::hex(s);
    if !row.key.is_empty() && row.key == st.hl {
        return RowLook {
            bg: h(&c.st.hl.bg),
            color: h(&c.st.hl.color),
            hl: true,
            top3: false,
        };
    }
    if c.cf.top3 && row.rank <= c.cf.top_n.max(1) as usize {
        return RowLook {
            bg: h(&c.st.top3.bg),
            color: h(&c.st.top3.color),
            hl: false,
            top3: true,
        };
    }
    let p = if row.rank % 2 == 1 {
        &c.st.odd
    } else {
        &c.st.even
    };
    RowLook {
        bg: h(&p.bg),
        color: h(&p.color),
        hl: false,
        top3: false,
    }
}

/// Still once its page is in: what reads the time is the page shown (auto
/// pages), the cascade of its rows coming in, and a looping scroll.
pub(crate) fn motion(d: &mut Draw, st: &State) -> Option<crate::cache::Motion> {
    use crate::cache::Motion;
    let LayerKind::Table { x: c } = &d.layer.kind else {
        return None;
    };
    let m = model(c, d.data);
    let pages = page_count(c, &m);
    let st = &live(c, st, d.time.now_ms, d.time.age_secs);
    let (page, at) = current_page(st, pages, d.time.now_ms);
    let since = if at > 0 {
        (d.time.now_ms - at) as f64 / 1000.0
    } else {
        d.time.age_secs
    } as f32;
    let scroll = c.page_mode == PageMode::Scroll;
    let per = c.per_page.max(1) as usize;
    if !m.rows.is_empty() {
        if scroll && m.rows.len() > per {
            return Some(Motion::Moving);
        }
        if !scroll && c.cascade.on {
            let n = m.rows.iter().skip(page * per).take(per).count();
            let arrived = (0..n).all(|i| {
                let t = (since * 1000.0 - i as f32 * c.cascade.offset as f32)
                    / (c.cascade.dur as f32).max(1.0);
                ease_out(t) >= 1.0
            });
            if !arrived {
                return Some(Motion::Moving);
            }
        }
    }
    Some(Motion::of(&page))
}

/// Draws the layer; false = not handled here (the generic code draws it).
pub(crate) fn draw(d: &mut Draw, st: &State) -> bool {
    let LayerKind::Table { x: c } = &d.layer.kind else {
        return false;
    };
    let c = c.clone();
    let m = model(&c, d.data);
    let size = d.num(Prop::FontSize).max(1.0);
    if m.rows.is_empty() && c.empty_mode == EmptyMode::Hide {
        return true;
    }
    let pages = page_count(&c, &m);
    let st = &live(&c, st, d.time.now_ms, d.time.age_secs);
    let (page, at) = current_page(st, pages, d.time.now_ms);
    let since = if at > 0 {
        (d.time.now_ms - at) as f64 / 1000.0
    } else {
        d.time.age_secs
    } as f32;
    let scroll = c.page_mode == PageMode::Scroll;
    let ws = widths(d, &c, &m, size);
    let mut xs = Vec::with_capacity(ws.len());
    let mut acc = 0.0;
    for w in &ws {
        xs.push(acc);
        acc += w + c.gap;
    }
    let (rh, gap, pad) = (c.row_h, c.gap, c.pad);
    let hh = header_h(&c);
    let show_head = c.header_row && (scroll || c.repeat_header || page == 0);
    let title_h = if c.title.on { title_height(&c) } else { 0.0 };
    let rows_t = rows_text(&c, size);
    let boxed = d.boxed();
    let bm = boxed.as_ref();
    let per = c.per_page.max(1) as usize;
    let list: Vec<&MRow> = if scroll {
        m.rows.iter().collect()
    } else {
        m.rows.iter().skip(page * per).take(per).collect()
    };
    let shown_n = if scroll { per } else { list.len() };
    let clip_h = if m.rows.is_empty() {
        rh * 2.0
    } else {
        (per.min(shown_n) as f32 * (rh + gap) - gap).max(0.0)
    };
    let mut y = 0.0;
    if c.title.on {
        y += title_h + gap;
    }
    if show_head {
        y += hh + gap;
    }
    let total_h = y + clip_h;
    // Background of the table.
    rect(
        d,
        0.0,
        0.0,
        d.w,
        total_h,
        0.0,
        Draw::hex(&c.st.table_bg),
        bm,
    );
    let h_line = c.borders.inner_h.clone();
    let v_line = c.borders.inner_v.clone();
    let lines =
        |d: &mut Draw, x: f32, y: f32, w: f32, h: f32, top: bool, left: bool, m: Option<&Mask>| {
            if top && h_line.on {
                rect(
                    d,
                    x,
                    y,
                    w,
                    h_line.width.min(h),
                    0.0,
                    Draw::hex(&h_line.color),
                    m,
                );
            }
            if left && v_line.on {
                rect(
                    d,
                    x,
                    y,
                    v_line.width.min(w),
                    h,
                    0.0,
                    Draw::hex(&v_line.color),
                    m,
                );
            }
        };
    let mut yy = 0.0;
    if c.title.on {
        if !c.title_bg.trim().is_empty() {
            rect(d, 0.0, 0.0, d.w, title_h, 0.0, Draw::hex(&c.title_bg), bm);
        } else if let Some(p) = rrect(0.0, 0.0, d.w, title_h, 0.0) {
            let grad = LinearGradient::new(
                Point::from_xy(0.0, 0.0),
                Point::from_xy(d.w, 0.0),
                vec![
                    GradientStop::new(0.0, super::text::paint::sk(Draw::hex(&c.title_from), 1.0)),
                    GradientStop::new(1.0, super::text::paint::sk(Draw::hex(&c.title_to), 1.0)),
                ],
                SpreadMode::Pad,
                Transform::identity(),
            );
            if let Some(sh) = grad {
                let mut paint = Paint {
                    shader: sh,
                    anti_alias: true,
                    ..Paint::default()
                };
                paint.shader.apply_opacity(d.opacity);
                d.fill_paint(&p, &paint, FillRule::Winding, bm);
            }
        }
        let tt = cg_core::wid::table::title_text(&c);
        let title = cg_core::wid::table::title_or(&c, d.lang);
        if let Some(p) = text_path(
            d,
            &tt,
            &title,
            align_of(c.title_align),
            (c.title_pad, 0.0, d.w - 2.0 * c.title_pad, title_h),
        ) {
            d.fill(&p, Draw::hex(&c.title_color), FillRule::Winding, bm);
        }
        yy += title_h + gap;
    }
    if show_head {
        let ht = header_text(&c);
        for (j, mc) in m.cols.iter().enumerate() {
            rect(
                d,
                xs[j],
                yy,
                ws[j],
                hh,
                c.radius,
                Draw::hex(&c.st.header.bg),
                bm,
            );
            lines(d, xs[j], yy, ws[j], hh, false, j > 0, bm);
            if let Some(p) = text_path(
                d,
                &ht,
                &mc.col.label,
                align_of(c.head_align.unwrap_or(mc.col.align)),
                (xs[j] + pad, yy, ws[j] - 2.0 * pad, hh),
            ) {
                d.fill(&p, Draw::hex(&c.st.header.color), FillRule::Winding, bm);
            }
        }
        yy += hh + gap;
    }
    let y0 = yy;
    if m.rows.is_empty() {
        let o = 0.85;
        let bg = with_alpha(Draw::hex(&c.st.odd.bg), o);
        rect(d, 0.0, y0, d.w, rh * 2.0, 0.0, bg, bm);
        if let Some(p) = text_path(
            d,
            &rows_t,
            &cg_core::wid::table::empty_or(&c, d.lang),
            TextAlign::Center,
            (pad, y0, d.w - 2.0 * pad, rh * 2.0),
        ) {
            d.fill(
                &p,
                with_alpha(Draw::hex(&c.st.odd.color), o),
                FillRule::Winding,
                bm,
            );
        }
    } else {
        // Inside the layer's box, like the web (a table taller than its box is cut).
        let region = d.clip(0.0, y0, d.w, clip_h.min((d.h - y0).max(0.0)));
        let rm = region.as_ref().or(bm);
        let step = rh + gap;
        let looped = scroll && m.rows.len() > per;
        let off = if looped {
            (c.scroll_speed as f32 * d.time.age_secs as f32).rem_euclid(m.rows.len() as f32 * step)
        } else {
            0.0
        };
        let shown: Vec<&MRow> = if looped {
            list.iter().chain(list.iter()).copied().collect()
        } else {
            list.clone()
        };
        let col_ts: Vec<Text> = m.cols.iter().map(|mc| col_text(&c, mc, size)).collect();
        let (tf0, op0) = (d.tf, d.opacity);
        for (i, row) in shown.iter().enumerate() {
            let ry = y0 + i as f32 * step - off;
            if ry > y0 + clip_h || ry + rh < y0 {
                continue;
            }
            // Cascade of the rows (pages).
            if !scroll && c.cascade.on {
                let t = (since * 1000.0 - i as f32 * c.cascade.offset as f32)
                    / (c.cascade.dur as f32).max(1.0);
                let k = ease_out(t);
                match c.cascade.style {
                    CascadeStyle::Slide => {
                        d.tf = tf0.pre_translate((1.0 - k) * -80.0, 0.0);
                        d.opacity = op0 * k;
                    }
                    CascadeStyle::Fade => d.opacity = op0 * k,
                    CascadeStyle::Flip => {
                        let a = ((1.0 - k) * 90.0).to_radians().cos().max(0.0);
                        d.tf = tf0
                            .pre_translate(0.0, ry)
                            .pre_scale(1.0, a.max(0.001))
                            .pre_translate(0.0, -ry);
                        d.opacity = if k > 0.0 { op0 } else { 0.0 };
                    }
                }
                if d.opacity <= 0.001 {
                    d.tf = tf0;
                    d.opacity = op0;
                    continue;
                }
            }
            let mut look = row_look(&c, st, row);
            // The row's own colour, from a column of the data.
            let cell_of = |name: &str| {
                if name.is_empty() {
                    return None;
                }
                m.head
                    .iter()
                    .position(|h| h == name)
                    .and_then(|i| row.r.get(i))
                    .and_then(|v| cg_core::wid::table::cell_color(v))
                    .map(Draw::hex)
            };
            let row_color = cell_of(&c.row_from);
            if let Some(rc) = row_color {
                match c.row_paint {
                    RowPaint::Bg if !look.hl => look.bg = rc,
                    RowPaint::Text if !look.hl => look.color = rc,
                    _ => {}
                }
            }
            let merge = merge_of(&c, &m, row);
            let mut skip_to = 0usize;
            for (j, mc) in m.cols.iter().enumerate() {
                if j < skip_to {
                    continue;
                }
                let mut w = ws[j];
                let mut txt = cell_text(mc, row);
                let mut align = mc.col.align;
                let mut kind = mc.col.kind;
                if let Some((mj, span, mt)) = merge {
                    if mj == j {
                        for k in 1..span {
                            w += ws[j + k] + gap;
                        }
                        skip_to = j + span;
                        if !mt.is_empty() {
                            txt = mt.to_string();
                            kind = ColKind::Text;
                        }
                        align = Align::Center;
                    }
                }
                let (mut bg, mut color) = (look.bg, look.color);
                if !look.hl && c.header_col_on && !mc.rank() && mc.col.src == c.header_col {
                    bg = Draw::hex(&c.st.hcol.bg);
                    color = Draw::hex(&c.st.hcol.color);
                }
                if kind == ColKind::Number && !mc.rank() && c.cf.neg && !look.hl {
                    let v =
                        mc.i.and_then(|i| row.r.get(i))
                            .and_then(|s| cg_core::wid::table::num(s));
                    if v.is_some_and(|v| v < 0.0) {
                        color = Draw::hex(&c.cf.neg_color);
                    }
                }
                if mc.rank() && look.top3 {
                    color = Draw::hex(&c.cf.top3_color);
                }
                if !look.hl {
                    if let Some(v) = cell_of(&mc.col.color_from) {
                        color = v;
                    }
                    if let Some(v) = cell_of(&mc.col.bg_from) {
                        bg = v;
                    }
                }
                if cell_in(&c, row) == Some(j) {
                    bg = Draw::hex(&c.cell.bg);
                    color = Draw::hex(&c.cell.color);
                }
                let x = xs[j];
                rect(d, x, ry, w, rh, c.radius, bg, rm);
                lines(d, x, ry, w, rh, i > 0 || show_head || c.title.on, j > 0, rm);
                if j == 0 && look.top3 && c.cf.top_bar {
                    rect(d, x, ry, 8.0, rh, 0.0, Draw::hex(&c.cf.top3_color), rm);
                }
                if let (0, Some(rc), RowPaint::Bar, false) = (j, row_color, c.row_paint, look.hl) {
                    rect(d, x, ry, 8.0, rh, 0.0, rc, rm);
                }
                if j == 0 && look.hl && c.cf.hl_bar {
                    rect(d, x, ry, 8.0, rh, 0.0, Draw::hex(&c.cf.hl_bar_color), rm);
                }
                if kind == ColKind::Image {
                    cell_picture(
                        d,
                        &txt,
                        &mc.col.img,
                        align,
                        (x + pad, ry, w - 2.0 * pad, rh),
                        rm,
                    );
                    continue;
                }
                // A colour swatch before the text (a colour column of the data).
                let mut tx0 = x + pad;
                if kind == ColKind::Text && !mc.col.mark.is_empty() {
                    let mark = m
                        .head
                        .iter()
                        .position(|h| *h == mc.col.mark)
                        .and_then(|i| row.r.get(i))
                        .filter(|v| !v.trim().is_empty());
                    if let Some(v) = mark {
                        let s = (rh * 0.3).round();
                        rect(
                            d,
                            tx0,
                            ry + (rh - s) / 2.0,
                            s,
                            s,
                            s * 0.2,
                            Draw::hex(v.trim()),
                            rm,
                        );
                        tx0 += s * 1.6;
                    }
                }
                let mut t = if merge.is_some_and(|g| g.0 == j) {
                    rows_t.clone()
                } else {
                    col_ts[j].clone()
                };
                let kw = if look.hl {
                    c.kw.hl
                } else if look.top3 {
                    c.kw.top3
                } else if row.rank % 2 == 1 {
                    c.kw.odd
                } else {
                    c.kw.even
                };
                if kw > 0 {
                    t.weight = kw;
                }
                if let Some(p) = text_path(
                    d,
                    &t,
                    &txt,
                    align_of(align),
                    (tx0, ry, w - pad - (tx0 - x), rh),
                ) {
                    d.fill(&p, color, FillRule::Winding, rm);
                }
            }
            d.tf = tf0;
            d.opacity = op0;
        }
    }
    if c.borders.outer.on {
        let bw = c.borders.outer.width;
        let col = Draw::hex(&c.borders.outer.color);
        rect(d, 0.0, 0.0, d.w, bw, 0.0, col, bm);
        rect(d, 0.0, total_h - bw, d.w, bw, 0.0, col, bm);
        rect(d, 0.0, bw, bw, total_h - 2.0 * bw, 0.0, col, bm);
        rect(d, d.w - bw, bw, bw, total_h - 2.0 * bw, 0.0, col, bm);
    }
    if c.indicator && pages > 1 && !scroll && !m.rows.is_empty() {
        indicator(d, page, pages, total_h + 14.0, &c.indicator_color, bm);
    }
    true
}

/// The "● ● 2 / 4" pill under the table, right aligned.
fn indicator(d: &mut Draw, page: usize, pages: usize, top: f32, color: &str, m: Option<&Mask>) {
    let Some(f) = d.font(FAMILY, 800) else {
        return;
    };
    let label = format!("{} / {}", page + 1, pages);
    let tw = tx::text_width(&f, &label, 24.0, 0.0);
    let dots: f32 = (0..pages)
        .map(|i| if i == page { 30.0 } else { 12.0 })
        .sum::<f32>()
        + 8.0 * pages as f32;
    let h = 24.0 * tx::LINE_HEIGHT + 16.0;
    let w = 16.0 + dots + 6.0 + tw + 16.0;
    let x0 = d.w - w;
    rect(
        d,
        x0,
        top,
        w,
        h,
        24.0,
        Color([11.0 / 255.0, 13.0 / 255.0, 24.0 / 255.0, 0.8]),
        m,
    );
    let mut x = x0 + 16.0;
    for i in 0..pages {
        let dw = if i == page { 30.0 } else { 12.0 };
        let col = if i == page {
            Draw::hex(color)
        } else {
            with_alpha(Color::WHITE, 0.35)
        };
        rect(d, x, top + (h - 12.0) / 2.0, dw, 12.0, 6.0, col, m);
        x += dw + 8.0;
    }
    if let Some(p) = Draw::text_path(
        &f,
        &label,
        24.0,
        0.0,
        TextAlign::Left,
        VerticalAlign::Middle,
        (x - 2.0, top, tw + 2.0, h),
        tx::LINE_HEIGHT,
    ) {
        d.fill(&p, Color::WHITE, FillRule::Winding, m);
    }
}

/// A picture in its cell (x, y, w, h), set like the Image widget: fit, size, shape, opacity,
/// a fallback when the cell is empty. Mirrors `CellPicture` of web/src/widgets/table.tsx.
fn cell_picture(
    d: &mut Draw,
    v: &str,
    p: &cg_core::wid::table::Pic,
    align: Align,
    (x, y, w, h): (f32, f32, f32, f32),
    m: Option<&Mask>,
) {
    use cg_core::wid::table::{PicFit, PicMask};
    let v = if v.trim().is_empty() {
        p.fallback.trim()
    } else {
        v.trim()
    };
    if v.is_empty() || w <= 1.0 {
        return;
    }
    let bh = (h * p.size.clamp(5.0, 100.0) / 100.0).round();
    let img = if v.starts_with('/') || v.contains('.') {
        d.image_size(v)
    } else {
        None
    };
    let mut bw = match (p.mask, p.fit, img) {
        (PicMask::Circle, _, _) => bh,
        (_, PicFit::Contain, Some((iw, ih))) => iw as f32 * bh / ih.max(1) as f32,
        (_, PicFit::Contain, None) => bh * 1.5,
        _ => w * p.width.clamp(5.0, 100.0) / 100.0,
    };
    bw = bw.min(w);
    let bx = match align {
        Align::Left => x,
        Align::Center => x + (w - bw) / 2.0,
        Align::Right => x + w - bw,
    };
    let by = y + (h - bh) / 2.0;
    let r = match p.mask {
        PicMask::None => 0.0,
        PicMask::Rounded => p.radius,
        PicMask::Circle => bh / 2.0,
    };
    // The shape, inside the clip already given.
    let shape = rrect(bx, by, bw, bh, r).and_then(|path| {
        // Inside the clip given: multiplied only where the shape is
        // (`mask::intersect`, the same values).
        if let Some(parent) = m.or(d.mask) {
            return Some(crate::mask::intersect(parent, &path, d.tf));
        }
        let mut mk = Mask::new(d.pm.width(), d.pm.height())?;
        mk.fill_path(&path, FillRule::Winding, true, d.tf);
        Some(mk)
    });
    let op0 = d.opacity;
    d.opacity *= (p.opacity / 100.0).clamp(0.0, 1.0);
    if let Some((iw, ih)) = img {
        let (iw, ih) = (iw.max(1) as f32, ih.max(1) as f32);
        let (sx, sy) = match p.fit {
            PicFit::Contain => {
                let s = (bw / iw).min(bh / ih);
                (s, s)
            }
            PicFit::Cover => {
                let s = (bw / iw).max(bh / ih);
                (s, s)
            }
            PicFit::Stretch => (bw / iw, bh / ih),
        };
        let tf = Transform::from_row(
            sx,
            0.0,
            0.0,
            sy,
            bx + (bw - iw * sx) / 2.0,
            by + (bh - ih * sy) / 2.0,
        );
        d.image(v, tf, 1.0, shape.as_ref().or(m));
    } else {
        rect(
            d,
            bx,
            by,
            bw,
            bh,
            r.max(bh * 0.1),
            Draw::hex("#334155"),
            shape.as_ref().or(m),
        );
        let code: String = v.chars().take(3).collect::<String>().to_uppercase();
        if let Some(f) = d.font(FAMILY, 700) {
            if let Some(path) = Draw::text_path(
                &f,
                &code,
                bh * 0.35,
                0.0,
                TextAlign::Center,
                VerticalAlign::Middle,
                (bx, by, bw, bh),
                tx::LINE_HEIGHT,
            ) {
                d.fill(&path, Draw::hex("#cbd5e1"), FillRule::Winding, m);
            }
        }
    }
    d.opacity = op0;
}

#[cfg(test)]
mod fit_tests {
    use super::fit_cell;
    use cg_core::wid::table::{Config, Lines, Text, Typo};

    /// 10 px wide per character at size 20 (half the size per character).
    fn width(s: &str, size: f32) -> f32 {
        s.chars().count() as f32 * size * 0.5
    }
    fn text(t: Typo) -> Text {
        let mut c = Config::default();
        c.txt.rows = t;
        cg_core::wid::table::rows_text(&c, 20.0)
    }

    #[test]
    fn pages_turn_by_themselves_from_the_take() {
        let project = cg_core::sample::demo_project();
        let t: cg_core::Template = serde_json::from_value(serde_json::json!({
            "id": "tt", "name": "T", "duration": 1000.0, "markers": [], "fields": [], "zone": "",
            "layers": [{
                "id": "tb", "name": "Table", "type": "table",
                "grid": [["team", "color"], ["A", "#00ff00"], ["B", "#0000ff"], ["C", "#ff0000"]],
                "cols": [{"src": "team", "label": "Team", "pct": 100.0}],
                "headerRow": false, "cascade": {"on": false}, "rowH": 100.0, "gap": 0.0, "perPage": 1,
                "indicator": false, "rowFrom": "color", "rowPaint": "bg", "autoPages": true, "autoSecs": 6.0,
                "props": {"x": 0.0, "y": 0.0, "width": 800.0, "height": 400.0}
            }]
        }))
        .unwrap();
        let mut r = crate::Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let fps = project.frame_rate.as_f64();
        let mut row = |secs: f64| {
            let f = r.render_template(&project, &t, "main", secs * fps, &Default::default());
            let i = (50 * f.width as usize + 780) * 4;
            [f.data[i], f.data[i + 1], f.data[i + 2]]
        };
        assert_eq!(row(1.0), [0, 255, 0], "page 1");
        assert_eq!(row(7.0), [0, 0, 255], "page 2 after 6 s");
        assert_eq!(row(13.0), [255, 0, 0], "page 3 after 12 s");
    }

    #[test]
    fn one_line_fits_narrower_then_smaller_then_ellipsis() {
        let t = text(Typo::default());
        // Fits as it is.
        assert_eq!(
            fit_cell(&width, &t, "abcd", 100.0, 40.0),
            ("abcd".into(), 20.0, 1.0)
        );
        // A bit too wide: narrower only.
        let (s, size, sx) = fit_cell(&width, &t, "abcdefghijk", 100.0, 40.0);
        assert_eq!((s.as_str(), size), ("abcdefghijk", 20.0));
        assert!((0.7..1.0).contains(&sx));
        // Much too wide: at the minimums, then "…".
        let (s, _, _) = fit_cell(&width, &t, &"x".repeat(80), 100.0, 40.0);
        assert!(s.ends_with('…'));
    }

    #[test]
    fn ellipsis_and_cut() {
        let e = text(Typo {
            fit: Some(cg_core::wid::table::FitMode::Ellipsis),
            ..Typo::default()
        });
        let (s, size, sx) = fit_cell(&width, &e, "abcdefghijkl", 100.0, 40.0);
        assert_eq!((size, sx), (20.0, 1.0));
        assert_eq!(s, "abcdefghi…");
        let c = text(Typo {
            fit: Some(cg_core::wid::table::FitMode::Clip),
            ..Typo::default()
        });
        assert_eq!(
            fit_cell(&width, &c, "abcdefghijkl", 100.0, 40.0).0,
            "abcdefghij"
        );
    }

    #[test]
    fn several_lines_wrap_then_shrink() {
        let t = text(Typo {
            lines: Some(Lines::Multi),
            max: Some(2),
            ..Typo::default()
        });
        // Two words on two lines at full size.
        let (s, size, _) = fit_cell(&width, &t, "Paris Germain", 80.0, 60.0);
        assert_eq!((s.as_str(), size), ("Paris\nGermain", 20.0));
        // Three lines needed: smaller until two fit, or "…" at the minimum.
        let (s, _, _) = fit_cell(&width, &t, "Olympique de Marseille Club", 80.0, 60.0);
        assert!(s.lines().count() <= 2);
    }
}
