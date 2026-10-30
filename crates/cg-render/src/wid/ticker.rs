//! Ticker: drawing (same picture as `web/src/widgets/ticker.tsx`).

use ab_glyph::FontArc;
use cg_core::wid::ticker::{
    clock, expand, limit, period, queue, resolve, runs, segs, typed, Dir, EmptyMode, Item, Mode,
    Placed, SepType, State, Ticker, Trans, PAD,
};
use cg_core::{Color, FontStyle, LayerKind, Prop, TextAlign, VerticalAlign};
use tiny_skia::{
    FillRule, GradientStop, LinearGradient, Mask, Paint, PathBuilder, Point, Rect, SpreadMode,
    Transform,
};

use super::table::{rect, with_alpha};
use super::Draw;
use crate::text as tx;

/// A clip of (x, y, w, h) of the layer whose edges fade over `fade` px
/// (horizontal or vertical), inside the parents' clip. The same from frame
/// to frame while the box does not move: kept (`mask::Kept`), its gradient
/// is not painted again each frame.
pub(crate) fn zone_mask(
    d: &mut Draw,
    (x, y, w, h): (f32, f32, f32, f32),
    fade: Option<(bool, f32)>,
) -> Option<std::sync::Arc<Mask>> {
    let r = Rect::from_xywh(x, y, w.max(0.01), h.max(0.01))?;
    // Where the mask can be other than 0 (and reads the parents' clip).
    let (fw, fh) = (d.pm.width(), d.pm.height());
    let region = crate::scaled::device_bounds(r, d.tf, fw, fh)
        .and_then(|b| {
            tiny_skia::IntRect::from_ltrb(
                (b.left() - 3).max(0),
                (b.top() - 3).max(0),
                (b.right() + 3).min(fw as i32),
                (b.bottom() + 3).min(fh as i32),
            )
        })
        .unwrap_or_else(|| tiny_skia::IntRect::from_xywh(0, 0, 1, 1).expect("a pixel"));
    let key = crate::mask::Kept::key(
        d.pm.width(),
        d.pm.height(),
        &[
            x,
            y,
            w,
            h,
            fade.map_or(-1.0, |f| f.1),
            d.tf.sx,
            d.tf.ky,
            d.tf.kx,
            d.tf.sy,
            d.tf.tx,
            d.tf.ty,
        ],
        fade.is_some_and(|f| f.0),
        d.mask,
        region,
    );
    let (w0, h0, tf, parent) = (d.pm.width(), d.pm.height(), d.tf, d.mask);
    d.rd.masks
        .get_or_make(key, || zone_mask_drawn(w0, h0, r, (w, h), fade, tf, parent))
}

fn zone_mask_drawn(
    width: u32,
    height: u32,
    r: Rect,
    (w, h): (f32, f32),
    fade: Option<(bool, f32)>,
    tf: Transform,
    parent: Option<&Mask>,
) -> Option<Mask> {
    let (x, y) = (r.x(), r.y());
    let mut paint = Paint::default();
    paint.set_color_rgba8(0, 0, 0, 255);
    if let Some((horiz, fw)) = fade {
        let len = if horiz { w } else { h };
        let f = (fw / len.max(1.0)).clamp(0.0, 0.5);
        let (p0, p1) = if horiz {
            (Point::from_xy(x, y), Point::from_xy(x + w, y))
        } else {
            (Point::from_xy(x, y), Point::from_xy(x, y + h))
        };
        let c0 = tiny_skia::Color::from_rgba8(0, 0, 0, 0);
        let c1 = tiny_skia::Color::from_rgba8(0, 0, 0, 255);
        if let Some(sh) = LinearGradient::new(
            p0,
            p1,
            vec![
                GradientStop::new(0.0, c0),
                GradientStop::new(f, c1),
                GradientStop::new(1.0 - f, c1),
                GradientStop::new(1.0, c0),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        ) {
            paint.shader = sh;
        }
    }
    crate::mask::painted(width, height, r, &paint, tf, parent)
}

/// Everything an item needs to be measured and drawn.
pub(crate) struct Look {
    pub font: FontArc,
    pub chip_font: Option<FontArc>,
    pub size: f32,
    pub ls: f32,
    pub upper: bool,
    pub color: Color,
}

fn chip(size: f32) -> (f32, f32, f32) {
    (
        (size * 0.6).round(),
        (size * 0.36).round(),
        (size * 0.45).round(),
    )
}

fn up(s: &str, upper: bool) -> String {
    if upper {
        s.to_uppercase()
    } else {
        s.to_string()
    }
}

struct Ctx<'a> {
    t: &'a Ticker<'a>,
    lk: Look,
    sym_font: Option<FontArc>,
}

impl Ctx<'_> {
    fn chip_w(&self, it: &Item) -> f32 {
        let Some(cat) = self
            .t
            .c
            .cats
            .iter()
            .find(|c| !it.cat.is_empty() && c.name == it.cat)
        else {
            return 0.0;
        };
        let (cs, cp, cg) = chip(self.lk.size);
        let f = self.lk.chip_font.as_ref().unwrap_or(&self.lk.font);
        tx::text_width(f, &cat.name.to_uppercase(), cs, 0.0) + 2.0 * cp + cg
    }
    fn item_w(&self, it: &Item) -> f32 {
        tx::text_width(
            &self.lk.font,
            &up(&it.text, self.lk.upper),
            self.lk.size,
            self.lk.ls,
        ) + self.chip_w(it)
    }
    fn sep_w(&self, d: &mut Draw) -> f32 {
        let s = self.lk.size;
        match self.t.c.sep_type {
            SepType::None => 0.0,
            SepType::Image => {
                let h = (s * 1.05).round();
                match d.image_size(&self.t.c.sep_image) {
                    Some((iw, ih)) => h * iw as f32 / ih.max(1) as f32,
                    None => h * 2.4,
                }
            }
            SepType::Symbol => self
                .sym_font
                .as_ref()
                .map(|f| tx::text_width(f, &self.t.c.sep_symbol, (s * 0.7).round(), 0.0))
                .unwrap_or(0.0),
            SepType::Text => tx::text_width(
                &self.lk.font,
                &up(self.t.separator, self.lk.upper),
                s,
                self.lk.ls,
            ),
        }
    }
    fn text_color(&self, it: &Item) -> Color {
        self.t
            .c
            .cats
            .iter()
            .find(|c| !it.cat.is_empty() && c.name == it.cat && !c.text.is_empty())
            .map(|c| Draw::hex(&c.text))
            .unwrap_or(self.lk.color)
    }

    /// Draws an item from x, in the band (0, h) of the layer.
    fn draw_item(
        &self,
        d: &mut Draw,
        it: &Item,
        x: f32,
        h: f32,
        valign: VerticalAlign,
        m: Option<&Mask>,
    ) {
        let mut x = x;
        if let Some(cat) = self
            .t
            .c
            .cats
            .iter()
            .find(|c| !it.cat.is_empty() && c.name == it.cat)
        {
            let (cs, cp, _) = chip(self.lk.size);
            let cw = self.chip_w(it);
            let ch = (cs * 1.7).round();
            let cy = (h - ch) / 2.0;
            rect(
                d,
                x,
                cy,
                cw - chip(self.lk.size).2,
                ch,
                6.0,
                Draw::hex(&cat.bg),
                m,
            );
            let f = self.lk.chip_font.clone().unwrap_or(self.lk.font.clone());
            if let Some(p) = Draw::text_path(
                &f,
                &cat.name.to_uppercase(),
                cs,
                0.0,
                TextAlign::Left,
                VerticalAlign::Middle,
                (x + cp, cy, cw, ch),
                tx::LINE_HEIGHT,
            ) {
                d.fill(&p, Draw::hex(&cat.color), FillRule::Winding, m);
            }
            x += cw;
        }
        let s = up(&it.text, self.lk.upper);
        let w = tx::text_width(&self.lk.font, &s, self.lk.size, self.lk.ls) + 4.0;
        if let Some(p) = Draw::text_path(
            &self.lk.font,
            &s,
            self.lk.size,
            self.lk.ls,
            TextAlign::Left,
            valign,
            (x, 0.0, w, h),
            tx::LINE_HEIGHT,
        ) {
            d.fill(&p, self.text_color(it), FillRule::Winding, m);
        }
    }

    /// Draws the separator centred in (x, w).
    fn draw_sep(
        &self,
        d: &mut Draw,
        x: f32,
        w: f32,
        h: f32,
        valign: VerticalAlign,
        m: Option<&Mask>,
    ) {
        let c = self.t.c;
        let s = self.lk.size;
        let col = if c.sep_color.is_empty() {
            self.lk.color
        } else {
            Draw::hex(&c.sep_color)
        };
        match c.sep_type {
            SepType::None => {}
            SepType::Image => {
                let ih = (s * 1.05).round();
                if let Some((iw, hh)) = d.image_size(&c.sep_image) {
                    let k = ih / hh.max(1) as f32;
                    let iw = iw as f32 * k;
                    d.image(
                        &c.sep_image,
                        Transform::from_row(k, 0.0, 0.0, k, x + (w - iw) / 2.0, (h - ih) / 2.0),
                        1.0,
                        m,
                    );
                } else {
                    // No picture: a red "logo" plate.
                    let iw = ih * 2.4;
                    rect(
                        d,
                        x + (w - iw) / 2.0,
                        (h - ih) / 2.0,
                        iw,
                        ih,
                        ih * 0.24,
                        Draw::hex("#ff3b5c"),
                        m,
                    );
                }
            }
            SepType::Symbol => {
                if let Some(f) = &self.sym_font {
                    if let Some(p) = Draw::text_path(
                        f,
                        &c.sep_symbol,
                        (s * 0.7).round(),
                        0.0,
                        TextAlign::Center,
                        VerticalAlign::Middle,
                        (x, 0.0, w, h),
                        tx::LINE_HEIGHT,
                    ) {
                        d.fill(&p, col, FillRule::Winding, m);
                    }
                }
            }
            SepType::Text => {
                let t = up(self.t.separator, self.lk.upper);
                if let Some(p) = Draw::text_path(
                    &self.lk.font,
                    &t,
                    s,
                    self.lk.ls,
                    TextAlign::Center,
                    valign,
                    (x, 0.0, w, h),
                    tx::LINE_HEIGHT,
                ) {
                    d.fill(&p, col, FillRule::Winding, m);
                }
            }
        }
    }
}

fn ease(k: f32) -> f32 {
    let k = k.clamp(0.0, 1.0);
    if k < 0.5 {
        4.0 * k * k * k
    } else {
        1.0 - (-2.0 * k + 2.0).powi(3) / 2.0
    }
}

/// Draws the layer; false = not handled here (the generic code draws it).
pub(crate) fn draw(d: &mut Draw, st: &State) -> bool {
    let LayerKind::Ticker {
        x: c,
        text,
        separator,
        speed,
        font,
    } = &d.layer.kind
    else {
        return false;
    };
    let t = Ticker {
        c,
        text,
        separator,
        speed: *speed,
    };
    let font: FontStyle = font.clone();
    let (w, h) = (d.w, d.h);
    let size = d.num(Prop::FontSize).max(1.0);
    let Some(f) = d.font(&font.font_family, font.font_weight) else {
        return true;
    };
    let lk = Look {
        font: f,
        chip_font: d.font("Inter", 900),
        size,
        ls: d.num(Prop::LetterSpacing),
        upper: font.uppercase,
        color: d.color(Prop::Fill),
    };
    let cx = Ctx {
        t: &t,
        sym_font: d.font("Inter", 900),
        lk,
    };
    let q = expand(&queue(&t, st), d.data);
    let empty = q.is_empty();
    if empty && c.empty_mode == EmptyMode::Hide {
        return true;
    }
    let valign = if c.items.is_empty() {
        font.vertical_align
    } else {
        VerticalAlign::Middle
    };
    // Bar and title block.
    let bm = d.boxed();
    if !c.bar_bg.is_empty() {
        rect(d, 0.0, 0.0, w, h, 0.0, Draw::hex(&c.bar_bg), bm.as_ref());
    }
    let zx = if c.title.on {
        c.title.w.min(w - 40.0).max(0.0)
    } else {
        0.0
    };
    let zw = (w - zx).max(40.0);
    let crawl = c.mode == Mode::Crawl;
    let fw = if crawl {
        c.fade_w.min(zw / 3.0)
    } else {
        c.fade_w.min(h * 0.2)
    };
    let zm = zone_mask(d, (zx, 0.0, zw, h), c.fade.then_some((crawl, fw)));
    let zm = zm.as_deref();
    let now = d.time.now_ms;
    let e = clock(&t, st, now, d.time.age_secs, d.time.fps);
    if empty {
        if !c.empty_text.is_empty() {
            let col = with_alpha(cx.lk.color, 0.6);
            if let Some(p) = Draw::text_path(
                &cx.lk.font,
                &c.empty_text,
                size,
                cx.lk.ls,
                TextAlign::Left,
                valign,
                (zx + PAD, 0.0, zw, h),
                tx::LINE_HEIGHT,
            ) {
                d.fill(&p, col, FillRule::Winding, zm);
            }
        }
    } else if crawl {
        let sep_w = cx.sep_w(d);
        let gap = c.spacing + sep_w;
        let mut wf = |it: &Item| (cx.item_w(it) + gap) as f64;
        let first = if c.start_full { 0.0 } else { zw as f64 };
        let placed: Vec<Placed> =
            resolve(&runs(&t, st), d.data, &mut wf, zw as f64, first, limit(c));
        let right = c.dir == Dir::Right;
        for g in segs(&placed, e, e + zw as f64) {
            let iw = cx.item_w(&g.item);
            let rel = (g.x - e) as f32;
            let x0 = if right {
                zx + zw - rel - g.w as f32
            } else {
                zx + rel
            };
            if right {
                cx.draw_sep(d, x0, gap, h, valign, zm);
                cx.draw_item(d, &g.item, x0 + gap, h, valign, zm);
            } else {
                cx.draw_item(d, &g.item, x0, h, valign, zm);
                cx.draw_sep(d, x0 + iw, gap, h, valign, zm);
            }
        }
    } else {
        let p = period(&t, st, h);
        let placed = resolve(&runs(&t, st), d.data, &mut |_| 1.0, 0.0, 0.0, limit(c));
        let k = (e / p).floor();
        let wk = (e - k * p) as f32;
        let at = |s: f64| {
            segs(&placed, s, s + 1.0)
                .into_iter()
                .find(|g| g.x <= s + 1e-9)
                .map(|g| g.item)
        };
        let cur = at(k);
        let prev = if k >= 1.0 { at(k - 1.0) } else { None };
        let next = at(k + 1.0);
        let (tf0, op0) = (d.tf, d.opacity);
        let started = st.started || d.time.age_secs > 0.0;
        let scroll_of = |it: &Item, wt: f32| -> f32 {
            let over = cx.item_w(it) + 2.0 * PAD - zw;
            if over <= 0.0 {
                return 0.0;
            }
            let t0 = if c.mode == Mode::Flip {
                c.trans_dur as f32
            } else {
                0.0
            } + 0.7;
            let t1 = if c.mode == Mode::Flip {
                p as f32
            } else {
                c.item_dur as f32
            } - 0.7;
            -over * ((wt - t0) / (t1 - t0).max(0.1)).clamp(0.0, 1.0)
        };
        let put = |d: &mut Draw, it: &Item, dx: f32, dy: f32, op: f32, sy: f32, wt: f32| {
            if op <= 0.001 {
                return;
            }
            let sx = scroll_of(it, wt);
            d.tf = tf0
                .pre_translate(dx, dy)
                .pre_translate(0.0, h / 2.0)
                .pre_scale(1.0, sy.max(0.001))
                .pre_translate(0.0, -h / 2.0);
            d.opacity = op0 * op;
            cx.draw_item(d, it, zx + PAD + sx, h, valign, zm);
            d.tf = tf0;
            d.opacity = op0;
        };
        if c.mode == Mode::Flip && c.trans == Trans::Type {
            // Typed in: the first item too, once the ticker runs.
            if let Some(it) = &cur {
                let (text, typing) = if started {
                    typed(&it.text, wk as f64, c.trans_dur)
                } else {
                    (it.text.clone(), false)
                };
                let shown = Item {
                    text: if typing { format!("{text}|") } else { text },
                    ..it.clone()
                };
                put(d, &shown, 0.0, 0.0, 1.0, 1.0, wk);
            }
        } else if c.mode == Mode::Flip {
            let in_trans = wk < c.trans_dur as f32 && prev.is_some() && cur.is_some() && started;
            let pk = if in_trans {
                ease(wk / (c.trans_dur as f32).max(0.01))
            } else {
                1.0
            };
            if !in_trans {
                if let Some(it) = &cur {
                    put(d, it, 0.0, 0.0, 1.0, 1.0, wk);
                }
            } else {
                let (a, b) = (cur.as_ref().unwrap(), prev.as_ref().unwrap());
                match c.trans {
                    Trans::Fade => {
                        put(d, b, 0.0, 0.0, 1.0 - pk, 1.0, p as f32);
                        put(d, a, 0.0, 0.0, pk, 1.0, wk);
                    }
                    Trans::Push => {
                        put(d, b, -pk * zw, 0.0, 1.0, 1.0, p as f32);
                        put(d, a, (1.0 - pk) * zw, 0.0, 1.0, 1.0, wk);
                    }
                    Trans::Flip => {
                        let ang = pk * std::f32::consts::PI;
                        if pk < 0.5 {
                            put(d, b, 0.0, 0.0, 1.0, ang.cos().abs(), p as f32);
                        } else {
                            put(d, a, 0.0, 0.0, 1.0, ang.cos().abs(), wk);
                        }
                    }
                    Trans::Slide | Trans::Type => {
                        put(d, b, 0.0, -pk * h, 1.0, 1.0, p as f32);
                        put(d, a, 0.0, (1.0 - pk) * h, 1.0, 1.0, wk);
                    }
                }
            }
        } else {
            let hold = c.item_dur as f32;
            let mk = if wk > hold && started {
                ease((wk - hold) / (p as f32 - hold).max(0.01))
            } else {
                0.0
            };
            let upd = if c.roll_dir == Dir::Down { -1.0 } else { 1.0 };
            if let Some(it) = &cur {
                put(d, it, 0.0, -upd * mk * h, 1.0, 1.0, wk);
            }
            if let (Some(it), true) = (&next, mk > 0.0) {
                put(d, it, 0.0, upd * (1.0 - mk) * h, 1.0, 1.0, 0.0);
            }
        }
    }
    if c.title.on && zx > 0.0 {
        let mut pb = PathBuilder::new();
        let s = 22.0f32.min(zx);
        pb.move_to(0.0, 0.0);
        pb.line_to(zx, 0.0);
        pb.line_to(zx - s, h);
        pb.line_to(0.0, h);
        pb.close();
        if let Some(path) = pb.finish() {
            d.fill(
                &path,
                Draw::hex(&c.title.bg),
                FillRule::Winding,
                bm.as_ref(),
            );
        }
        let ts = (size * 0.95).round();
        if let Some(f9) = d.font("Inter", 900) {
            let (p, _) = super::table::fitted_path(
                &f9,
                &c.title.text,
                ts,
                ts * 0.06,
                TextAlign::Center,
                (0.0, 0.0, (zx - 16.0).max(1.0), h),
                Some(super::table::Fit {
                    overflow: cg_core::wid::table::Overflow::Shrink,
                    min_size: ts * 0.5,
                    min_squeeze: 70.0,
                }),
            );
            if let Some(p) = p {
                d.fill(
                    &p,
                    Draw::hex(&c.title.color),
                    FillRule::Winding,
                    bm.as_ref(),
                );
            }
        }
    }
    true
}
