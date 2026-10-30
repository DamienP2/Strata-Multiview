//! Counter: drawing (same picture as `web/src/widgets/counter.tsx`).
//!
//! Prefix, number, suffix on one baseline. The number is drawn as a numeric
//! roll (the value formatted at every frame), odometer reels or split-flap
//! cards; tabular figures are drawn in cells of the widest digit.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use cg_core::wid::counter::{
    color_for, display, format, group_before, group_sep, int_digits, parts, sign, AffixRaise, Anim,
    Curve, Mem, Reserve, State, Transition,
};
use cg_core::{Color, LayerKind, Prop, TextAlign, VerticalAlign};
use tiny_skia::{FillRule, Mask, PathBuilder, Transform};

use super::text::paint;
use super::{rrect, Draw};
use crate::text::{self as tx, Chain};

/// What the renderer remembers of each counter's source (by layer id).
fn mems() -> &'static Mutex<HashMap<String, Mem>> {
    static M: OnceLock<Mutex<HashMap<String, Mem>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A run of text with tabular digits (each in a cell of the widest digit).
struct Ink<'a> {
    chain: &'a Chain,
    size: f32,
    ls: f32,
    tabular: bool,
    digit_w: f32,
}

impl Ink<'_> {
    fn width(&self, s: &str) -> f32 {
        if !self.tabular {
            return tx::chain_width(self.chain, s, self.size, self.ls);
        }
        let n = s.chars().count();
        s.chars()
            .map(|c| {
                if c.is_ascii_digit() {
                    self.digit_w
                } else {
                    tx::chain_width(self.chain, &c.to_string(), self.size, 0.0)
                }
            })
            .sum::<f32>()
            + self.ls * n.saturating_sub(1) as f32
    }
    /// Outline of `s` at (x, baseline).
    fn path(&self, s: &str, x: f32, baseline: f32, pb: &mut PathBuilder) {
        if !self.tabular {
            let (g, _) = tx::glyph_run(self.chain, s, self.size, self.ls, 0.0);
            tx::run_outline(self.chain, &g, self.size, x, baseline, pb);
            return;
        }
        let mut x = x;
        for c in s.chars() {
            let cs = c.to_string();
            let w = tx::chain_width(self.chain, &cs, self.size, 0.0);
            let cell = if c.is_ascii_digit() { self.digit_w } else { w };
            let (g, _) = tx::glyph_run(self.chain, &cs, self.size, 0.0, 0.0);
            tx::run_outline(
                self.chain,
                &g,
                self.size,
                x + (cell - w) / 2.0,
                baseline,
                pb,
            );
            x += cell + self.ls;
        }
    }
}

fn widest_digit(chain: &Chain, size: f32) -> f32 {
    (0..10)
        .map(|i| tx::chain_width(chain, &i.to_string(), size, 0.0))
        .fold(0.0, f32::max)
}

/// Odometer reels: position (0..10) of every place, least significant first.
pub(crate) fn reels(a: Option<&Anim>, v: f64, decimals: usize, now: i64, curve: Curve) -> Vec<f64> {
    let k10 = 10f64.powi(decimals as i32);
    let n_of = |x: f64| (x.abs() * k10).round();
    let digit = |n: f64, i: usize| ((n / 10f64.powi(i as i32)).floor() % 10.0).max(0.0);
    let places = |x: f64| (decimals + 1).max(int_digits(x, decimals) + decimals);
    match a {
        Some(a) if a.progress(now) < 1.0 => {
            let p = places(a.from).max(places(a.to));
            let (nf, nt) = (n_of(a.from), n_of(a.to));
            let up = nt >= nf;
            let gap = (nt - nf).abs();
            let e = curve.apply(a.progress(now));
            (0..p)
                .map(|i| {
                    let start = digit(nf, i);
                    let end = digit(nt, i);
                    let mut d = if up {
                        (end - start).rem_euclid(10.0)
                    } else {
                        -((start - end).rem_euclid(10.0))
                    };
                    // Lower reels spin extra turns (capped at 2): the duration is kept, never a strobe.
                    let turns = (gap / 10f64.powi(i as i32 + 1)).floor().min(2.0);
                    d += if up { 10.0 } else { -10.0 } * turns;
                    (start + d * e).rem_euclid(10.0)
                })
                .collect()
        }
        _ => {
            let n = n_of(v);
            (0..places(v)).map(|i| digit(n, i)).collect()
        }
    }
}

/// Still once at rest on its value: drawn from the value shown, the
/// transition to it (while it runs: moving) and its colour.
pub(crate) fn motion(d: &mut Draw, st: &State) -> Option<crate::cache::Motion> {
    use crate::cache::Motion;
    let p = parts(d.layer)?;
    let LayerKind::Counter { .. } = &d.layer.kind else {
        return None;
    };
    let now = d.time.now_ms;
    // As `draw` (it also keeps the counter's memory up to date while the
    // frames kept are shown instead of drawn).
    let (v, anim) = {
        let mut m = mems().lock().unwrap_or_else(|e| e.into_inner());
        let mem = m.entry(d.layer.id.clone()).or_default();
        display(&p, st, d.data, now, d.time.age_secs, Some(mem))
    };
    if anim.is_some_and(|a| a.progress(now) < 1.0) {
        return Some(Motion::Moving);
    }
    let col = color_for(p.c, v, anim.as_ref(), now);
    let ends = anim.map(|a| (a.from.to_bits(), a.to.to_bits()));
    Some(Motion::of(&(v.to_bits(), ends, col)))
}

pub(crate) fn draw(d: &mut Draw, st: &State) -> bool {
    let Some(p) = parts(d.layer) else {
        return false;
    };
    let LayerKind::Counter { font, .. } = &d.layer.kind else {
        return false;
    };
    let c = p.c;
    let now = d.time.now_ms;
    let (v, anim) = {
        let mut m = mems().lock().unwrap_or_else(|e| e.into_inner());
        let mem = m.entry(d.layer.id.clone()).or_default();
        display(&p, st, d.data, now, d.time.age_secs, Some(mem))
    };
    let curve = if anim.is_some_and(|a| a.t0 == now - (d.time.age_secs * 1000.0) as i64) {
        Curve::Out
    } else {
        c.easing
    };
    let fill = d.color(Prop::Fill);
    let col = color_for(c, v, anim.as_ref(), now)
        .map(|h| Draw::hex(&h))
        .unwrap_or(fill);
    let size = d.num(Prop::FontSize);
    let ls = d.num(Prop::LetterSpacing);
    let Some(main) = paint::main_font(d, &font.font_family, font.font_weight) else {
        return true;
    };
    let prefix = cg_core::anim::interpolate_text(p.prefix, d.data);
    let suffix = cg_core::anim::interpolate_text(p.suffix, d.data);
    let chain = paint::chain(
        d,
        main.clone(),
        "0123456789+-−,.’\u{202f}\u{a0}kMdBKLCrTsio",
    );
    let asize = size * c.affix_size.max(1.0) / 100.0;
    let aweight = if c.affix_weight == 0 {
        font.font_weight
    } else {
        c.affix_weight
    };
    let Some(afont) = paint::main_font(d, &font.font_family, aweight) else {
        return true;
    };
    let achain = paint::chain(d, afont, &format!("{prefix}{suffix}"));
    let dw = widest_digit(&chain, size);
    let ink = Ink {
        chain: &chain,
        size,
        ls,
        tabular: c.tabular,
        digit_w: dw,
    };
    let dec = p.fmt.decimals as usize;
    let mode = if p.fmt.compact && c.transition == Transition::Odometer {
        Transition::Roll
    } else {
        c.transition
    };
    let flip = mode == Transition::Flip && anim.is_some_and(|a| a.progress(now) < 1.0);
    let text = format(
        &p.fmt,
        if mode == Transition::None {
            anim.map_or(v, |a| a.to)
        } else {
            v
        },
    );
    // Width of the number.
    let cell_w = dw + 0.12 * size;
    let flip_w = |s: &str| -> f32 {
        s.chars()
            .map(|ch| {
                if ch.is_ascii_digit() {
                    cell_w + 0.05 * size
                } else {
                    tx::chain_width(&chain, &ch.to_string(), size, 0.0)
                }
            })
            .sum()
    };
    let (flip_from, flip_to) = match (mode, anim) {
        (Transition::Flip, Some(a)) => (format(&p.fmt, a.from), format(&p.fmt, a.to)),
        _ => (text.clone(), text.clone()),
    };
    let odo = reels(anim.as_ref(), v, dec, now, curve);
    let num_w = match mode {
        Transition::Flip => {
            let l = flip_from.chars().count().max(flip_to.chars().count());
            let pad = |s: &str| format!("{}{s}", " ".repeat(l - s.chars().count()));
            if flip {
                flip_w(&pad(&flip_to).replace(' ', "0")).max(flip_w(&flip_to))
            } else {
                flip_w(&text)
            }
        }
        Transition::Odometer => odo_width(&ink, &chain, &p.fmt, &odo, v, dec, size),
        _ => ink.width(&text),
    };
    let reserve = match c.reserve {
        Reserve::Off => 0.0,
        Reserve::Chars => c.reserve_chars as f32 * dw,
        Reserve::Max => {
            let w = |x: Option<f64>| {
                x.map_or(0.0, |x| {
                    if mode == Transition::Flip {
                        flip_w(&format(&p.fmt, x))
                    } else {
                        ink.width(&format(&p.fmt, x))
                    }
                })
            };
            w(c.min).max(w(c.max))
        }
    };
    let box_w = num_w.max(reserve);
    let pre_w = if prefix.is_empty() {
        0.0
    } else {
        tx::chain_width(&achain, &prefix, asize, 0.0) + c.affix_gap
    };
    let suf_w = if suffix.is_empty() {
        0.0
    } else {
        tx::chain_width(&achain, &suffix, asize, 0.0) + c.affix_gap
    };
    let pad = if c.plate { 44.0 } else { 0.0 };
    let avail = d.w - 2.0 * pad;
    let total = pre_w + box_w + suf_w;
    let x0 = pad
        + match font.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => (avail - total) / 2.0,
            TextAlign::Right => avail - total,
        };
    let line_h = size * tx::LINE_HEIGHT;
    let top = match font.vertical_align {
        VerticalAlign::Top => 0.0,
        VerticalAlign::Middle => (d.h - line_h) / 2.0,
        VerticalAlign::Bottom => d.h - line_h,
    };
    let (asc, desc) = tx::metrics(&main, size);
    let baseline = top + (line_h - (asc - desc)) / 2.0 + asc;
    let num_x = x0
        + pre_w
        + match font.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => (box_w - num_w) / 2.0,
            TextAlign::Right => box_w - num_w,
        };

    // Plate.
    if c.plate {
        if let Some(pp) = rrect(0.0, 0.0, d.w, d.h, 6.0) {
            d.fill(&pp, Draw::hex(&c.plate_color), FillRule::Winding, None);
            if let Some(m) = d.clip(0.0, d.h - 10.0, d.w, 10.0) {
                // The freeze stays in the control room: the bar never changes on air.
                d.fill(&pp, Draw::hex("#ff3b5c"), FillRule::Winding, Some(&m));
            }
        }
    }
    let boxed = d.boxed();
    let mask = boxed.as_ref();
    // Affixes.
    let affix_base = match c.affix_raise {
        AffixRaise::Baseline => baseline,
        AffixRaise::Top => baseline - 0.727 * (size - asize),
    };
    let acol = if c.affix_color.trim().is_empty() {
        fill
    } else {
        Draw::hex(&c.affix_color)
    };
    let mut apb = PathBuilder::new();
    if !prefix.is_empty() {
        let (g, _) = tx::glyph_run(&achain, &prefix, asize, 0.0, 0.0);
        tx::run_outline(&achain, &g, asize, x0, affix_base, &mut apb);
    }
    if !suffix.is_empty() {
        let (g, _) = tx::glyph_run(&achain, &suffix, asize, 0.0, 0.0);
        tx::run_outline(
            &achain,
            &g,
            asize,
            x0 + pre_w + box_w + c.affix_gap,
            affix_base,
            &mut apb,
        );
    }
    if let Some(pa) = apb.finish() {
        d.fill(&pa, acol, FillRule::Winding, mask);
    }
    // The number.
    match mode {
        Transition::Odometer => draw_odometer(
            d, &ink, &p.fmt, &odo, v, dec, num_x, baseline, asc, col, mask,
        ),
        Transition::Flip => {
            let a = anim.filter(|a| a.progress(now) < 1.0);
            let k = a.map_or(1.0, |a| curve.apply(a.progress(now)) as f32);
            draw_flip(
                d,
                &chain,
                &flip_from,
                &flip_to,
                &text,
                k,
                a.is_some(),
                num_x,
                baseline,
                asc,
                size,
                cell_w,
                col,
                mask,
            );
        }
        _ => {
            let mut pb = PathBuilder::new();
            ink.path(&text, num_x, baseline, &mut pb);
            if let Some(pa) = pb.finish() {
                d.fill(&pa, col, FillRule::Winding, mask);
            }
        }
    }
    true
}

/// Width of the odometer: sign, reels (leading places shrink in), separators.
#[allow(clippy::too_many_arguments)]
fn odo_width(
    ink: &Ink,
    chain: &Chain,
    f: &cg_core::wid::counter::Fmt,
    reels: &[f64],
    v: f64,
    dec: usize,
    size: f32,
) -> f32 {
    let mut w = 0.0;
    let s = sign(f, v, dec);
    if !s.is_empty() {
        w += tx::chain_width(chain, s, size, 0.0);
    }
    let p_int = reels.len().saturating_sub(dec);
    for i in (0..reels.len()).rev() {
        let fr = place_frac(v, i as i32 - dec as i32);
        w += ink.digit_w * fr;
        let pi = i as i32 - dec as i32;
        if pi == 0 && dec > 0 {
            w += tx::chain_width(chain, cg_core::wid::counter::dec_sep(f), size, 0.0);
        }
        if pi > 0 && group_after(f, p_int, pi as usize) {
            w += tx::chain_width(chain, group_sep(f), size, 0.0) * fr;
        }
    }
    w
}

/// A separator follows the integer place `p` (counted from the units).
fn group_after(f: &cg_core::wid::counter::Fmt, p_int: usize, p: usize) -> bool {
    // group_before(n, i): before the digit at index i of n (from the left).
    p < p_int && group_before(f, p_int, p_int - p)
}

/// Shown fraction of an integer place (leading places shrink in with the value).
fn place_frac(v: f64, p: i32) -> f32 {
    if p <= 0 {
        1.0
    } else {
        (((v.abs().max(1e-9)).log10() - p as f64) * 3.0 + 1.0).clamp(0.0, 1.0) as f32
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_odometer(
    d: &mut Draw,
    ink: &Ink,
    f: &cg_core::wid::counter::Fmt,
    reels: &[f64],
    v: f64,
    dec: usize,
    x: f32,
    baseline: f32,
    _asc: f32,
    col: Color,
    mask: Option<&Mask>,
) {
    let size = ink.size;
    let chain = ink.chain;
    let cell_h = 1.1 * size;
    let mut x = x;
    let mut pb = PathBuilder::new();
    let s = sign(f, v, dec);
    if !s.is_empty() {
        let (g, w) = tx::glyph_run(chain, s, size, 0.0, 0.0);
        tx::run_outline(chain, &g, size, x, baseline, &mut pb);
        x += w;
    }
    let p_int = reels.len().saturating_sub(dec);
    let mut reel_paths: Vec<(f32, tiny_skia::Path, (f32, f32, f32, f32))> = Vec::new();
    for i in (0..reels.len()).rev() {
        let pi = i as i32 - dec as i32;
        let fr = place_frac(v, pi);
        let cw = ink.digit_w * fr;
        if fr > 0.001 {
            let pos = reels[i];
            let d0 = pos.floor() as i64;
            let frac = (pos - d0 as f64) as f32;
            let mut rp = PathBuilder::new();
            for (k, dy) in [(d0, -frac * cell_h), (d0 + 1, (1.0 - frac) * cell_h)] {
                let ch = ((k.rem_euclid(10)) as u8 + b'0') as char;
                let w = tx::chain_width(chain, &ch.to_string(), size, 0.0);
                let (g, _) = tx::glyph_run(chain, &ch.to_string(), size, 0.0, 0.0);
                tx::run_outline(chain, &g, size, x + (cw - w) / 2.0, baseline + dy, &mut rp);
            }
            if let Some(path) = rp.finish() {
                reel_paths.push((fr, path, (x, baseline - 0.95 * size, cw, cell_h)));
            }
        }
        x += cw;
        if pi == 0 && dec > 0 {
            let ds = cg_core::wid::counter::dec_sep(f);
            let (g, w) = tx::glyph_run(chain, ds, size, 0.0, 0.0);
            tx::run_outline(chain, &g, size, x, baseline, &mut pb);
            x += w;
        }
        if pi > 0 && group_after(f, p_int, pi as usize) {
            let gs = group_sep(f);
            let (g, w) = tx::glyph_run(chain, gs, size, 0.0, 0.0);
            if fr > 0.001 {
                tx::run_outline(chain, &g, size, x, baseline, &mut pb);
            }
            x += w * fr;
        }
    }
    if let Some(pa) = pb.finish() {
        d.fill(&pa, col, FillRule::Winding, mask);
    }
    for (fr, path, (cx, cy, cw, ch)) in reel_paths {
        let Some(clip) = d.clip(cx, cy, cw.max(0.5), ch) else {
            continue;
        };
        let clip = combine(&clip, mask);
        let mut c2 = col;
        c2.0[3] *= fr;
        d.fill(&path, c2, FillRule::Winding, Some(&clip));
    }
}

fn combine(a: &Mask, b: Option<&Mask>) -> Mask {
    let mut m = a.clone();
    if let Some(b) = b {
        for (x, y) in m.data_mut().iter_mut().zip(b.data()) {
            *x = ((*x as u16 * *y as u16) / 255) as u8;
        }
    }
    m
}

/// Split-flap cards: every digit on a card of two halves; a changing digit
/// flips its top half down (old → new) around the middle hinge.
#[allow(clippy::too_many_arguments)]
fn draw_flip(
    d: &mut Draw,
    chain: &Chain,
    from: &str,
    to: &str,
    text: &str,
    k: f32,
    active: bool,
    x: f32,
    baseline: f32,
    asc: f32,
    size: f32,
    cell_w: f32,
    col: Color,
    mask: Option<&Mask>,
) {
    let (a, b): (Vec<char>, Vec<char>) = if active {
        let l = from.chars().count().max(to.chars().count());
        let pad = |s: &str| -> Vec<char> {
            std::iter::repeat_n(' ', l - s.chars().count())
                .chain(s.chars())
                .collect()
        };
        (pad(from), pad(to))
    } else {
        (text.chars().collect(), text.chars().collect())
    };
    let card_h = 1.15 * size;
    let card_top = baseline - asc - 0.05 * size;
    let hinge = card_top + card_h / 2.0;
    let gap = 0.012 * size;
    let r = 0.07 * size;
    let mut x = x;
    let glyph = |ch: char, cx: f32| -> Option<tiny_skia::Path> {
        if ch == ' ' {
            return None;
        }
        let s = ch.to_string();
        let w = tx::chain_width(chain, &s, size, 0.0);
        let (g, _) = tx::glyph_run(chain, &s, size, 0.0, 0.0);
        let mut pb = PathBuilder::new();
        tx::run_outline(chain, &g, size, cx + (cell_w - w) / 2.0, baseline, &mut pb);
        pb.finish()
    };
    for (o, n) in a.iter().zip(&b) {
        let digitish = n.is_ascii_digit() || o.is_ascii_digit();
        if !digitish {
            if *n != ' ' {
                let s = n.to_string();
                let (g, w) = tx::glyph_run(chain, &s, size, 0.0, 0.0);
                let mut pb = PathBuilder::new();
                tx::run_outline(chain, &g, size, x, baseline, &mut pb);
                if let Some(pa) = pb.finish() {
                    d.fill(&pa, col, FillRule::Winding, mask);
                }
                x += w;
            }
            continue;
        }
        if *n == ' ' && (!active || k >= 1.0) {
            continue;
        }
        let cx = x + 0.025 * size;
        x += cell_w + 0.05 * size;
        let half = |d: &mut Draw, ch: char, top_half: bool, sy: f32, shade: f32| {
            // One half card (scaled about the hinge by sy), its half of the glyph, shaded.
            let (hy, hh) = if top_half {
                (card_top, card_h / 2.0 - gap)
            } else {
                (hinge + gap, card_h / 2.0 - gap)
            };
            let tf = Transform::from_row(1.0, 0.0, 0.0, sy, 0.0, hinge * (1.0 - sy));
            let Some(clip) = d.clip(cx, hy.min(hinge), cell_w, hh) else {
                return;
            };
            let card = rrect(cx, hy, cell_w, hh, r).and_then(|p| p.transform(tf));
            let ink = glyph(ch, cx).and_then(|p| p.transform(tf));
            let clip_rect = combine(&clip, mask);
            if let Some(cp) = card {
                let base = if top_half {
                    Draw::hex("#2c2170")
                } else {
                    Draw::hex("#241a5c")
                };
                d.fill(&cp, base, FillRule::Winding, mask);
            }
            if let Some(ip) = ink {
                d.fill(&ip, col, FillRule::Winding, Some(&clip_rect));
            }
            if shade > 0.001 {
                if let Some(cp) = rrect(cx, hy, cell_w, hh, r).and_then(|p| p.transform(tf)) {
                    d.fill(&cp, Color([0.0, 0.0, 0.0, shade]), FillRule::Winding, mask);
                }
            }
        };
        if !active || o == n || k >= 1.0 {
            half(d, *n, true, 1.0, 0.0);
            half(d, *n, false, 1.0, 0.0);
            continue;
        }
        half(d, *n, true, 1.0, 0.0);
        half(d, *o, false, 1.0, 0.0);
        if k < 0.5 {
            let sy = (k * std::f32::consts::PI).cos();
            half(d, *o, true, sy, k);
        } else {
            let sy = ((1.0 - k) * std::f32::consts::PI).cos().abs();
            half(d, *n, false, sy, 0.5 - k / 2.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{OutputFormat, Renderer};
    use cg_core::{sample, ChannelState, Command};
    use serde_json::json;

    #[test]
    fn a_frozen_counter_looks_the_same_on_air() {
        let mut project = sample::demo_project();
        let t: cg_core::Template = serde_json::from_value(json!({
            "id": "ct", "name": "C", "duration": 10.0, "markers": [], "fields": [], "zone": "",
            "layers": [{
                "id": "c", "name": "Score", "type": "counter", "from": "0", "to": "5", "duration": 0.0,
                "fontFamily": "Inter", "fontWeight": 700, "plate": true, "plateColor": "#000000",
                "props": {"x": 0.0, "y": 0.0, "width": 400.0, "height": 200.0, "fontSize": 60.0, "fill": [1.0, 1.0, 1.0, 1.0]}
            }]
        }))
        .unwrap();
        project.templates.push(t);
        let mut state = ChannelState::default();
        let take = Command::Take {
            zone: None,
            layer: Some(90),
            template_id: "ct".into(),
            data: None,
            rundown_id: None,
        };
        state.apply(&project, take).unwrap();
        for _ in 0..30 {
            state.tick(1.0);
        }
        let out = OutputFormat {
            width: 1920,
            height: 1080,
        };
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let bar = |r: &mut Renderer, st: &ChannelState| {
            let f = r.render_channel(&project, st, out, "UTC");
            let i = (195 * 1920 + 200) * 4;
            [f.data[i], f.data[i + 1], f.data[i + 2]]
        };
        assert_eq!(bar(&mut r, &state), [255, 59, 92]);
        let freeze = Command::Act {
            zone: None,
            layer: Some(90),
            target: "Score".into(),
            op: "freeze".into(),
            args: json!({}),
        };
        state.apply(&project, freeze).unwrap();
        // The freeze stays in the control room: the bar does not change.
        assert_eq!(bar(&mut r, &state), [255, 59, 92]);
    }
}
