//! Typewriter: drawing (same picture as `web/src/widgets/typewriter.tsx`).
//!
//! Frozen layout: the complete text is laid out (and auto-shrunk) first;
//! the part not typed yet is simply not drawn, so no word ever jumps line.

use cg_core::wid::typewriter::{
    auto_next, elapsed, now_at, texts, timeline, Appear, Config, Erase, Mode, State, Timeline,
};
use cg_core::{LayerKind, Prop, TextAlign, VerticalAlign};
use tiny_skia::{FillRule, PathBuilder};

use super::text::paint;
use super::{rrect, Draw};
use crate::text as tx;

/// Lines of graphemes (pre-wrap): breaks at spaces, words longer than the
/// line broken between graphemes, typed line breaks kept. Same as `wrapG`.
pub(crate) fn wrap_g(
    g: &[String],
    max_w: f32,
    m: &mut dyn FnMut(&str) -> f32,
) -> Vec<(usize, usize)> {
    let nl = |s: &str| s.contains('\n');
    let sp = |s: &str| s.chars().all(char::is_whitespace);
    let width = |a: usize, b: usize, m: &mut dyn FnMut(&str) -> f32| m(g[a..b].concat().trim_end());
    let n = g.len();
    let mut lines = Vec::new();
    let (mut start, mut i) = (0, 0);
    while i < n {
        if nl(&g[i]) {
            lines.push((start, i + 1));
            start = i + 1;
            i += 1;
            continue;
        }
        if sp(&g[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < n && !sp(&g[j]) && !nl(&g[j]) {
            j += 1;
        }
        if width(start, j, m) <= max_w {
            i = j;
            continue;
        }
        if start < i {
            lines.push((start, i));
            start = i;
            if width(i, j, m) <= max_w {
                i = j;
                continue;
            }
        }
        for k in i..j {
            if k > start && width(start, k + 1, m) > max_w {
                lines.push((start, k));
                start = k;
            }
        }
        i = j;
    }
    lines.push((start, n));
    lines
}

/// Height of `lines` lines (the last without its leading), as fit_text.
fn block(lines: usize, size: f32, lh: f32) -> f32 {
    if lines == 0 {
        0.0
    } else {
        (lines - 1) as f32 * size * lh + size
    }
}

pub(crate) struct Lay {
    pub size: f32,
    pub lines: Vec<(usize, usize)>,
}

/// The frozen layout of a text: the size (auto-shrink on the complete text)
/// and its lines. Same as `layoutTw` in typewriter.tsx (which also tells the
/// Designer when the text does not fit).
#[allow(clippy::too_many_arguments)]
pub(crate) fn layout(
    c: &Config,
    g: &[String],
    size0: f32,
    w: f32,
    h: f32,
    m: &mut dyn FnMut(&str, f32) -> f32,
) -> Lay {
    let lh = c.line_height;
    let mut at = |size: f32| {
        let l = wrap_g(g, w, &mut |s| m(s, size));
        (block(l.len(), size, lh) <= h + 0.5, l)
    };
    let (ok, l) = at(size0);
    if ok || !c.auto_shrink || g.is_empty() {
        return Lay {
            size: size0,
            lines: l,
        };
    }
    let lo0 = c.min_size.min(size0).max(1.0);
    let (ok, l) = at(lo0);
    if !ok {
        return Lay {
            size: lo0,
            lines: l,
        };
    }
    let (mut lo, mut hi, mut best) = (lo0, size0, l);
    for _ in 0..14 {
        let mid = (lo + hi) / 2.0;
        match at(mid) {
            (true, l) => {
                lo = mid;
                best = l;
            }
            (false, _) => hi = mid,
        }
    }
    Lay {
        size: lo,
        lines: best,
    }
}

/// Opacity (and slide) of every grapheme at elapsed `e`, the cursor's place
/// and the phase. Same as `visible` in typewriter.tsx.
pub(crate) struct Vis {
    pub k: Vec<f32>,
    pub shown: usize,
    pub block: f32,
    /// typing | hold | erasing | done | idle.
    pub phase: &'static str,
}

/// `hold`: the pause between two texts, s (the text stays typed that long).
pub(crate) fn visible(c: &Config, tl: &Timeline, e: f64, next: bool, idle: bool, hold: f64) -> Vis {
    let n = tl.g.len();
    let mut k = vec![0.0f32; n];
    if idle {
        return Vis {
            k,
            shown: 0,
            block: 1.0,
            phase: "idle",
        };
    }
    let hold_end = tl.type_end + hold.max(0.0);
    if e < tl.type_end || !next || e < hold_end {
        let mut shown = 0;
        for (i, kv) in k.iter_mut().enumerate() {
            let dt = e - tl.t[tl.unit_of[i]];
            *kv = if tl.appear > 0.0 {
                (dt / tl.appear).clamp(0.0, 1.0) as f32
            } else if dt >= 0.0 {
                1.0
            } else {
                0.0
            };
            if *kv > 0.0 {
                shown = i + 1;
            }
        }
        let phase = if e < tl.type_end {
            "typing"
        } else if next {
            "hold"
        } else {
            "done"
        };
        return Vis {
            k,
            shown,
            block: 1.0,
            phase,
        };
    }
    let ee = e - hold_end;
    if c.erase == Erase::Block {
        k.fill(1.0);
        return Vis {
            k,
            shown: n,
            block: (1.0 - ee / tl.erase_dur.max(1e-6)).clamp(0.0, 1.0) as f32,
            phase: "erasing",
        };
    }
    let left = n.saturating_sub((ee * c.erase_rate.max(1.0)).floor() as usize);
    for kv in k.iter_mut().take(left) {
        *kv = 1.0;
    }
    Vis {
        k,
        shown: left,
        block: 1.0,
        phase: "erasing",
    }
}

pub(crate) fn draw(d: &mut Draw, st: &State) -> bool {
    let LayerKind::Typewriter {
        x: c,
        speed,
        cursor,
        font,
        ..
    } = &d.layer.kind
    else {
        return false;
    };
    let mut list = texts(d.layer, d.data);
    if font.uppercase {
        list = list.into_iter().map(|t| t.to_uppercase()).collect();
    }
    let n = list.len().max(1);
    let Some(main) = paint::main_font(d, &font.font_family, font.font_weight) else {
        return true;
    };
    let chain = paint::chain(d, main.clone(), &list.concat());
    let size0 = d.num(Prop::FontSize);
    let ls = d.num(Prop::LetterSpacing);
    let (px, py) = if c.plate { (48.0, 20.0) } else { (0.0, 0.0) };
    let (bw, bh) = ((d.w - 2.0 * px).max(10.0), (d.h - 2.0 * py).max(10.0));
    let gs = |t: &str| -> Vec<String> {
        cg_core::wid::text::graphemes(t)
            .into_iter()
            .map(String::from)
            .collect()
    };
    let lay_of = |i: usize| {
        let g = gs(list.get(i).map_or("", |s| s.as_str()));
        let l = layout(c, &g, size0, bw, bh, &mut |s, z| {
            tx::chain_width(&chain, s, z, ls)
        });
        (g, l)
    };
    let line_unit = c.unit == cg_core::wid::typewriter::Unit::Line;
    let mut tl_of = |i: usize| -> Timeline {
        let text = list.get(i).map_or("", |s| s.as_str());
        if line_unit {
            let (g, l) = lay_of(i);
            let mut line_of = vec![0; g.len()];
            for (li, (a, b)) in l.lines.iter().enumerate() {
                for x in line_of.iter_mut().take(*b).skip(*a) {
                    *x = li;
                }
            }
            timeline(c, *speed, text, i, Some(&line_of))
        } else {
            timeline(c, *speed, text, i, None)
        }
    };
    let e = elapsed(st, d.time.now_ms, d.time.age_secs);
    let idle = st.mode == Mode::Idle;
    let (idx, el) = if idle {
        (st.idx.min(n - 1), 0.0)
    } else {
        now_at(c, st, n, e, &mut tl_of)
    };
    let tl = tl_of(idx);
    // The text erases itself only when the next one comes by itself (chained by time).
    let next = auto_next(c, st, n, idx);
    let hold = cg_core::wid::typewriter::chain(c, st).hold;
    let vis = visible(c, &tl, el, next, idle, hold);
    let (g, lay) = lay_of(idx);

    // Plate.
    if c.plate {
        if let Some(p) = rrect(0.0, 0.0, d.w, d.h, 4.0) {
            d.fill(&p, Draw::hex(&c.plate_color), FillRule::Winding, None);
            if !c.plate_accent.trim().is_empty() {
                if let Some(m) = d.clip(0.0, 0.0, 10.0, d.h) {
                    d.fill(&p, Draw::hex(&c.plate_accent), FillRule::Winding, Some(&m));
                }
            }
        }
    }
    let clip = d.boxed();
    let mask = clip.as_ref();
    // The layout is always the complete text's (frozen): no word jumps while typing.
    let size = lay.size;
    let line_h = size * c.line_height;
    let nl = lay.lines.len();
    let top = py
        + match font.vertical_align {
            VerticalAlign::Top => 0.0,
            VerticalAlign::Middle => (bh - line_h * nl as f32) / 2.0,
            VerticalAlign::Bottom => bh - line_h * nl as f32,
        };
    let (asc, desc) = tx::metrics(&main, size);
    let half = (line_h - (asc - desc)) / 2.0;
    let fill = d.color(Prop::Fill);
    // Glyph paths grouped by opacity / slide.
    let mut groups: Vec<(f32, PathBuilder)> = Vec::new();
    let mut pictures = Vec::new();
    let mut cursor_at: Option<(f32, f32)> = None; // (x, baseline)
    for (li, (a, b)) in lay.lines.iter().enumerate() {
        let (a, b) = (*a, (*b).min(g.len()));
        let line: Vec<&String> = g[a..b]
            .iter()
            .filter(|x| !x.contains('\n') && !x.contains('\r'))
            .collect();
        let text: String = line.iter().map(|s| s.as_str()).collect();
        let wtrim = tx::chain_width(&chain, text.trim_end(), size, ls);
        let x0 = px
            + match font.align {
                TextAlign::Left => 0.0,
                TextAlign::Center => (bw - wtrim) / 2.0,
                TextAlign::Right => bw - wtrim,
            };
        let baseline = top + li as f32 * line_h + half + asc;
        let (glyphs, _) = tx::glyph_run(&chain, &text, size, ls, 0.0);
        // Grapheme of each glyph.
        let mut owner = Vec::new();
        for (gi, gr) in g[a..b].iter().enumerate().map(|(k, gr)| (a + k, gr)) {
            if !gr.contains('\n') && !gr.contains('\r') {
                for ch in gr.chars() {
                    if !tx::invisible(ch) {
                        owner.push(gi);
                    }
                }
            }
        }
        // Cursor after the last shown grapheme (at the start of a line after a line break).
        let after_break = vis.shown == 0 || g.get(vis.shown - 1).is_some_and(|x| x.contains('\n'));
        if after_break && (vis.shown == a || (li == 0 && vis.shown == 0)) {
            cursor_at = Some((x0, baseline));
        }
        for (gl, own) in glyphs.iter().zip(&owner) {
            if !after_break && *own + 1 == vis.shown {
                cursor_at = Some((x0 + gl.x + gl.adv, baseline));
            }
        }
        for (gl, own) in glyphs.iter().zip(&owner) {
            let shown_here = vis.k.get(*own).copied().unwrap_or(0.0);
            if shown_here <= 0.0 {
                continue;
            }
            let dy = if c.appear == Appear::Slide {
                (1.0 - shown_here) * 0.45 * size
            } else {
                0.0
            };
            let font = &chain.fonts[gl.font];
            match ab_glyph::Font::outline(font, gl.id) {
                Some(_) => {
                    let pos = groups
                        .iter()
                        .position(|(k, _)| (*k - shown_here).abs() < 1e-4);
                    let i = pos.unwrap_or_else(|| {
                        groups.push((shown_here, PathBuilder::new()));
                        groups.len() - 1
                    });
                    tx::run_outline(
                        &chain,
                        std::slice::from_ref(gl),
                        size,
                        x0,
                        baseline + dy,
                        &mut groups[i].1,
                    );
                }
                None if !gl.ch.is_whitespace() => {
                    pictures.push((*gl, x0 + gl.x, baseline + dy, shown_here))
                }
                None => {}
            }
        }
    }
    let alpha_all = vis.block;
    let paths: Vec<(f32, tiny_skia::Path)> = groups
        .into_iter()
        .filter_map(|(k, pb)| pb.finish().map(|p| (k, p)))
        .collect();
    if c.shadow {
        let refs: Vec<&tiny_skia::Path> = paths.iter().map(|(_, p)| p).collect();
        if !refs.is_empty() {
            paint::shadow(d, &refs, 0.0, Draw::hex("#00000088"), 0.0, 3.0, 10.0, mask);
        }
    }
    for (k, p) in &paths {
        let mut col = fill;
        col.0[3] *= k * alpha_all;
        d.fill(p, col, FillRule::Winding, mask);
    }
    for (gl, x, b, k) in &pictures {
        paint::picture(d, &chain, gl, *x, *b, size, 1.0, k * alpha_all, mask);
    }
    // Cursor.
    let all_done = vis.phase == "done";
    if *cursor && !(all_done && c.cursor_hide && !g.is_empty()) {
        let typing = vis.phase == "typing" || vis.phase == "erasing";
        let blink_on =
            !c.cursor_blink || typing || ((d.time.now_ms as f64 / 500.0).floor() as i64) % 2 == 0;
        if blink_on {
            let (cx, cb) = cursor_at.unwrap_or((px, top + half + asc));
            let gt = cb - asc;
            let (x, y, w, h) = match c.cursor_ch.as_str() {
                "█" => (cx, gt + 0.1 * size, 0.56 * size, 1.02 * size),
                "_" => (cx, gt + 1.0 * size, 0.56 * size, 0.11 * size),
                _ => (cx + 0.03 * size, gt + 0.08 * size, 0.09 * size, 1.08 * size),
            };
            let col = if c.cursor_color.trim().is_empty() {
                fill
            } else {
                Draw::hex(&c.cursor_color)
            };
            if let Some(p) = rrect(x, y, w, h, 0.0) {
                let mut col = col;
                col.0[3] *= alpha_all;
                d.fill(&p, col, FillRule::Winding, mask);
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::Renderer;
    use cg_core::sample;
    use serde_json::json;

    #[test]
    fn the_plate_accent_is_a_setting() {
        let project = sample::demo_project();
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let mut left = |accent: Option<&str>| {
            let mut l = json!({
                "id": "tw", "name": "T", "type": "typewriter", "text": "Bonjour", "speed": 20.0, "cursor": false,
                "fontFamily": "Inter", "fontWeight": 700, "plate": true, "plateColor": "#000000",
                "props": {"x": 0.0, "y": 0.0, "width": 600.0, "height": 200.0, "fontSize": 40.0, "fill": [1.0, 1.0, 1.0, 1.0]}
            });
            if let Some(a) = accent {
                l["plateAccent"] = json!(a);
            }
            let t: cg_core::Template = serde_json::from_value(json!({
                "id": "t", "name": "T", "duration": 100.0, "markers": [], "fields": [], "zone": "", "layers": [l]
            }))
            .unwrap();
            let f = r.render_template(&project, &t, "main", 50.0, &Default::default());
            let i = (100 * f.width as usize + 4) * 4;
            [f.data[i], f.data[i + 1], f.data[i + 2]]
        };
        assert_eq!(left(None), [255, 59, 92], "red by default");
        assert_eq!(left(Some("#00ff00")), [0, 255, 0]);
        assert_eq!(left(Some("")), [0, 0, 0], "none");
    }
}
