//! A small painter (rectangles, paths, circles, text by centre line, clips,
//! group opacity) used by the scoreboard and the map, mirrored by
//! `web/src/widgets/scoreboard-paint.tsx` (same primitives as SVG) so the
//! two renderers draw the same picture from the same drawing code.

use std::hash::Hash;

use cg_core::wid::shape::Cmd;
use cg_core::{Color, TextAlign, VerticalAlign};
use tiny_skia::{
    FillRule, GradientStop, LinearGradient, Mask, Paint, Path, Point, SpreadMode, Stroke,
    StrokeDash, Transform,
};

use crate::wid::Draw;

/// `#rgb`, `#rrggbb[aa]` or `rgba(r, g, b, a)` → colour (transparent when invalid).
pub(crate) fn col(s: &str) -> Color {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb(")) {
        let v: Vec<f32> = inner
            .trim_end_matches(')')
            .split(',')
            .filter_map(|x| x.trim().parse().ok())
            .collect();
        if v.len() >= 3 {
            return Color([
                v[0] / 255.0,
                v[1] / 255.0,
                v[2] / 255.0,
                *v.get(3).unwrap_or(&1.0),
            ]);
        }
        return Color::TRANSPARENT;
    }
    let h = s.trim_start_matches('#');
    if h.len() == 3 {
        let e: String = h.chars().flat_map(|c| [c, c]).collect();
        return Draw::hex(&e);
    }
    Draw::hex(s)
}

/// Luminance 0–1 of a colour.
pub(crate) fn lum(s: &str) -> f32 {
    let c = col(s).0;
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

/// Readable ink on a background.
pub(crate) fn ink(bg: &str) -> &'static str {
    if lum(bg) > 0.62 {
        "#0b0d18"
    } else {
        "#ffffff"
    }
}

/// Mix of two colours (`k` = share of `b`).
pub(crate) fn mix(a: Color, b: Color, k: f32) -> Color {
    let (a, b) = (a.0, b.0);
    Color([
        a[0] + (b[0] - a[0]) * k,
        a[1] + (b[1] - a[1]) * k,
        a[2] + (b[2] - a[2]) * k,
        a[3] + (b[3] - a[3]) * k,
    ])
}

/// Path commands of a rounded rectangle.
pub(crate) fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Vec<Cmd> {
    let r = r.max(0.0).min(w / 2.0).min(h / 2.0);
    if r <= 0.01 {
        return vec![
            Cmd::Move(x, y),
            Cmd::Line(x + w, y),
            Cmd::Line(x + w, y + h),
            Cmd::Line(x, y + h),
            Cmd::Close,
        ];
    }
    let k = 0.5523 * r;
    vec![
        Cmd::Move(x + r, y),
        Cmd::Line(x + w - r, y),
        Cmd::Cubic(x + w - r + k, y, x + w, y + r - k, x + w, y + r),
        Cmd::Line(x + w, y + h - r),
        Cmd::Cubic(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h),
        Cmd::Line(x + r, y + h),
        Cmd::Cubic(x + r - k, y + h, x, y + h - r + k, x, y + h - r),
        Cmd::Line(x, y + r),
        Cmd::Cubic(x, y + r - k, x + r - k, y, x + r, y),
        Cmd::Close,
    ]
}

/// Path commands of an ellipse.
pub(crate) fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<Cmd> {
    let (kx, ky) = (0.5523 * rx, 0.5523 * ry);
    vec![
        Cmd::Move(cx + rx, cy),
        Cmd::Cubic(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry),
        Cmd::Cubic(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy),
        Cmd::Cubic(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry),
        Cmd::Cubic(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy),
        Cmd::Close,
    ]
}

/// A polyline (closed or not).
pub(crate) fn poly(pts: &[(f32, f32)], close: bool) -> Vec<Cmd> {
    let mut out = Vec::with_capacity(pts.len() + 1);
    for (i, &(x, y)) in pts.iter().enumerate() {
        out.push(if i == 0 {
            Cmd::Move(x, y)
        } else {
            Cmd::Line(x, y)
        });
    }
    if close && !out.is_empty() {
        out.push(Cmd::Close);
    }
    out
}

/// Commands moved by (dx, dy) and scaled by `s` around the origin.
pub(crate) fn tf(cmds: &[Cmd], dx: f32, dy: f32, s: f32) -> Vec<Cmd> {
    cmds.iter()
        .map(|c| match *c {
            Cmd::Move(x, y) => Cmd::Move(dx + x * s, dy + y * s),
            Cmd::Line(x, y) => Cmd::Line(dx + x * s, dy + y * s),
            Cmd::Cubic(a, b, c, d, e, f) => Cmd::Cubic(
                dx + a * s,
                dy + b * s,
                dx + c * s,
                dy + d * s,
                dx + e * s,
                dy + f * s,
            ),
            Cmd::Close => Cmd::Close,
        })
        .collect()
}

pub(crate) fn path(cmds: &[Cmd]) -> Option<Path> {
    crate::wid::shape::path(cmds)
}

/// `p` without its contours lying wholly outside the `w` × `h` frame
/// (through `tf`, with `margin` pixels around it). Such a contour draws
/// nothing on the frame (filled, it adds nothing to the winding outside of
/// its bounds; stroked, its outline stays within `margin` of them), so the
/// picture is the same without the work: a map projects the whole world,
/// most of it out of view when zoomed in. None: all of it may show;
/// `Some(None)`: none of it does.
pub(crate) fn in_view(
    p: &Path,
    tf: Transform,
    w: u32,
    h: u32,
    margin: f32,
) -> Option<Option<Path>> {
    use tiny_skia::PathSegment as S;
    let (fx0, fy0, fx1, fy1) = (-margin, -margin, w as f32 + margin, h as f32 + margin);
    let outside = |r: tiny_skia::Rect| -> bool {
        let mut pts = [
            Point::from_xy(r.left(), r.top()),
            Point::from_xy(r.right(), r.top()),
            Point::from_xy(r.left(), r.bottom()),
            Point::from_xy(r.right(), r.bottom()),
        ];
        tf.map_points(&mut pts);
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for q in pts {
            x0 = x0.min(q.x);
            y0 = y0.min(q.y);
            x1 = x1.max(q.x);
            y1 = y1.max(q.y);
        }
        x1 < fx0 || y1 < fy0 || x0 > fx1 || y0 > fy1
    };
    let b = p.bounds();
    // Wholly in view: as it is.
    {
        let mut pts = [
            Point::from_xy(b.left(), b.top()),
            Point::from_xy(b.right(), b.top()),
            Point::from_xy(b.left(), b.bottom()),
            Point::from_xy(b.right(), b.bottom()),
        ];
        tf.map_points(&mut pts);
        if pts
            .iter()
            .all(|q| q.x >= fx0 && q.y >= fy0 && q.x <= fx1 && q.y <= fy1)
        {
            return None;
        }
    }
    let segs: Vec<S> = p.segments().collect();
    // Contours: (first segment, end, bounds).
    let mut keep: Vec<(usize, usize)> = Vec::new();
    let mut dropped = false;
    let mut i = 0;
    while i < segs.len() {
        let start = i;
        let mut pts: Vec<Point> = Vec::new();
        loop {
            match segs[i] {
                S::MoveTo(a) | S::LineTo(a) => pts.push(a),
                S::QuadTo(a, b) => pts.extend([a, b]),
                S::CubicTo(a, b, c) => pts.extend([a, b, c]),
                S::Close => {}
            }
            i += 1;
            if i >= segs.len() || matches!(segs[i], S::MoveTo(_)) {
                break;
            }
        }
        match tiny_skia::Rect::from_points(&pts) {
            Some(r) if outside(r) => dropped = true,
            _ => keep.push((start, i)),
        }
    }
    if !dropped {
        return None;
    }
    let mut pb = tiny_skia::PathBuilder::new();
    for (a, b) in keep {
        for sg in &segs[a..b] {
            match *sg {
                S::MoveTo(p) => pb.move_to(p.x, p.y),
                S::LineTo(p) => pb.line_to(p.x, p.y),
                S::QuadTo(p1, p) => pb.quad_to(p1.x, p1.y, p.x, p.y),
                S::CubicTo(p1, p2, p) => pb.cubic_to(p1.x, p1.y, p2.x, p2.y, p.x, p.y),
                S::Close => pb.close(),
            }
        }
    }
    Some(pb.finish())
}

/// Text options.
#[derive(Clone, Copy)]
pub(crate) struct T {
    pub size: f32,
    pub weight: u16,
    /// -1 start, 0 middle, 1 end.
    pub anchor: i8,
    /// Letter spacing, px.
    pub ls: f32,
    /// Horizontal / vertical scale around (x, cy).
    pub sx: f32,
    pub sy: f32,
    /// Slant (fake italic), tan of the angle.
    pub skew: f32,
}

impl T {
    pub fn new(size: f32, weight: u16) -> Self {
        T {
            size,
            weight,
            anchor: -1,
            ls: 0.0,
            sx: 1.0,
            sy: 1.0,
            skew: 0.0,
        }
    }
    pub fn mid(mut self) -> Self {
        self.anchor = 0;
        self
    }
    pub fn end(mut self) -> Self {
        self.anchor = 1;
        self
    }
    pub fn ls(mut self, ls: f32) -> Self {
        self.ls = ls;
        self
    }
    pub fn scale(mut self, sx: f32, sy: f32) -> Self {
        self.sx = sx;
        self.sy = sy;
        self
    }
    pub fn skew(mut self, k: f32) -> Self {
        self.skew = k;
        self
    }
}

/// The painter: layer-space drawing with a clip stack and a group opacity.
pub(crate) struct Pt<'x, 'a, 'b> {
    pub d: &'x mut Draw<'a, 'b>,
    /// Measure only (layout pass).
    pub dry: bool,
    /// Painted already (the picture kept has it): nothing is painted, clips
    /// are still made.
    pub skip: bool,
    /// Recording: nothing is painted, every painting is summed up here
    /// (what it paints, where, how): the same sum, the same picture.
    pub rec: Option<std::collections::hash_map::DefaultHasher>,
    pub alpha: f32,
    /// Font family of the texts.
    pub font: String,
    mask: Option<Mask>,
    stack: Vec<Option<Mask>>,
}

impl<'x, 'a, 'b> Pt<'x, 'a, 'b> {
    pub fn new(d: &'x mut Draw<'a, 'b>, dry: bool) -> Self {
        let mask = d.mask.cloned();
        Pt {
            d,
            dry,
            skip: false,
            rec: None,
            alpha: 1.0,
            font: "Inter".into(),
            mask,
            stack: Vec::new(),
        }
    }

    /// Recording: sums up a painting (its kind, its numbers, the opacity
    /// and font then); true when recording (nothing to paint).
    fn record(&mut self, tag: &str, cmds: &[Cmd], nums: &[f32], text: &str) -> bool {
        let Some(h) = self.rec.as_mut() else {
            return false;
        };
        tag.hash(h);
        for c in cmds {
            match *c {
                Cmd::Move(x, y) => (0u8, x.to_bits(), y.to_bits()).hash(h),
                Cmd::Line(x, y) => (1u8, x.to_bits(), y.to_bits()).hash(h),
                Cmd::Cubic(a, b, c, d, e, f) => (2u8, [a, b, c, d, e, f].map(f32::to_bits)).hash(h),
                Cmd::Close => 3u8.hash(h),
            }
        }
        cmds.len().hash(h);
        for n in nums {
            n.to_bits().hash(h);
        }
        nums.len().hash(h);
        text.hash(h);
        self.alpha.to_bits().hash(h);
        self.font.hash(h);
        true
    }

    fn paint(&self, c: Color) -> Paint<'static> {
        crate::paint_for(c, self.d.opacity * self.alpha)
    }

    /// Clips what follows to the path, until `pop`.
    pub fn clip(&mut self, cmds: &[Cmd]) {
        self.stack.push(self.mask.clone());
        if self.record("clip", cmds, &[], "") || self.dry {
            return;
        }
        let Some(p) = path(cmds) else { return };
        let m = match &self.mask {
            Some(m) => crate::mask::intersect(m, &p, self.d.tf),
            None => {
                let mut m = Mask::new(self.d.pm.width(), self.d.pm.height()).expect("mask");
                m.fill_path(&p, FillRule::Winding, true, self.d.tf);
                m
            }
        };
        self.mask = Some(m);
    }

    pub fn pop(&mut self) {
        self.record("pop", &[], &[], "");
        if let Some(m) = self.stack.pop() {
            self.mask = m;
        }
    }

    pub fn fill_c(&mut self, cmds: &[Cmd], c: Color) {
        if self.record("fill", cmds, &c.0, "") || self.dry || self.skip || c.0[3] <= 0.0 {
            return;
        }
        let Some(p) = path(cmds) else { return };
        let (w, h) = (self.d.pm.width(), self.d.pm.height());
        let seen = in_view(&p, self.d.tf, w, h, 2.0);
        let p = match &seen {
            None => &p,
            Some(Some(part)) => part,
            Some(None) => return,
        };
        let paint = self.paint(c);
        self.d
            .pm
            .fill_path(p, &paint, FillRule::Winding, self.d.tf, self.mask.as_ref());
    }

    pub fn fill(&mut self, cmds: &[Cmd], c: &str) {
        self.fill_c(cmds, col(c));
    }

    /// Fills with a linear gradient from (x1, y1) to (x2, y2) (stops: offset, colour).
    pub fn fill_lin(
        &mut self,
        cmds: &[Cmd],
        p1: (f32, f32),
        p2: (f32, f32),
        stops: &[(f32, Color)],
    ) {
        let mut nums = vec![p1.0, p1.1, p2.0, p2.1];
        for (o, c) in stops {
            nums.push(*o);
            nums.extend(c.0);
        }
        if self.record("fill_lin", cmds, &nums, "") || self.dry || self.skip {
            return;
        }
        let Some(p) = path(cmds) else { return };
        let a = self.d.opacity * self.alpha;
        let st: Vec<GradientStop> = stops
            .iter()
            .map(|&(o, c)| {
                let [r, g, b, al] = c.0;
                GradientStop::new(
                    o,
                    tiny_skia::Color::from_rgba(
                        r.clamp(0.0, 1.0),
                        g.clamp(0.0, 1.0),
                        b.clamp(0.0, 1.0),
                        (al * a).clamp(0.0, 1.0),
                    )
                    .unwrap_or(tiny_skia::Color::TRANSPARENT),
                )
            })
            .collect();
        let Some(shader) = LinearGradient::new(
            Point::from_xy(p1.0, p1.1),
            Point::from_xy(p2.0, p2.1),
            st,
            SpreadMode::Pad,
            Transform::identity(),
        ) else {
            if let Some(&(_, c)) = stops.first() {
                self.fill_c(cmds, c);
            }
            return;
        };
        let paint = Paint {
            shader,
            anti_alias: true,
            ..Default::default()
        };
        self.d
            .pm
            .fill_path(&p, &paint, FillRule::Winding, self.d.tf, self.mask.as_ref());
    }

    /// Strokes the path (`dash`: on / off lengths; `round`: round caps and joins).
    pub fn stroke_c(
        &mut self,
        cmds: &[Cmd],
        c: Color,
        width: f32,
        round: bool,
        dash: Option<(f32, f32)>,
    ) {
        let (on, off) = dash.unwrap_or((-1.0, -1.0));
        let nums = [
            c.0[0],
            c.0[1],
            c.0[2],
            c.0[3],
            width,
            f32::from(u8::from(round)),
            on,
            off,
        ];
        if self.record("stroke", cmds, &nums, "")
            || self.dry
            || self.skip
            || width <= 0.0
            || c.0[3] <= 0.0
        {
            return;
        }
        let Some(p) = path(cmds) else { return };
        let mut s = Stroke {
            width,
            ..Default::default()
        };
        if round {
            s.line_cap = tiny_skia::LineCap::Round;
            s.line_join = tiny_skia::LineJoin::Round;
        }
        // Only what can show: the outline of a contour stays within its
        // width (miter joins: twice) of it.
        let tf = self.d.tf;
        let (w, h) = (self.d.pm.width(), self.d.pm.height());
        let k = (tf.sx.abs() + tf.kx.abs()).max(tf.ky.abs() + tf.sy.abs());
        let margin = width * k * 2.0 + 4.0;
        let mut p = match in_view(&p, tf, w, h, margin) {
            None => p,
            Some(Some(part)) => part,
            Some(None) => return,
        };
        if let Some((on, off)) = dash {
            // Dashed as tiny-skia does (each contour from its start), then
            // the dashes out of view dropped (the same picture: dashing is
            // the first thing `stroke_path` does).
            let Some(dash) = StrokeDash::new(vec![on, off], 0.0) else {
                return;
            };
            let res = tiny_skia::PathStroker::compute_resolution_scale(&tf);
            let Some(dashed) = p.dash(&dash, res) else {
                return;
            };
            p = match in_view(&dashed, tf, w, h, margin) {
                None => dashed,
                Some(Some(part)) => part,
                Some(None) => return,
            };
        }
        let paint = self.paint(c);
        self.d
            .pm
            .stroke_path(&p, &paint, &s, tf, self.mask.as_ref());
    }

    pub fn stroke(&mut self, cmds: &[Cmd], c: &str, width: f32, round: bool) {
        self.stroke_c(cmds, col(c), width, round, None);
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: &str) {
        if w > 0.0 && h > 0.0 {
            self.fill(&rrect(x, y, w, h, r), c);
        }
    }

    pub fn rect_c(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Color) {
        if w > 0.0 && h > 0.0 {
            self.fill_c(&rrect(x, y, w, h, r), c);
        }
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, c: &str) {
        if r > 0.0 {
            self.fill(&ellipse(cx, cy, r, r), c);
        }
    }

    /// Width of a line of text.
    pub fn tw(&mut self, s: &str, size: f32, weight: u16, ls: f32) -> f32 {
        let family = self.font.clone();
        match self.d.font(&family, weight) {
            Some(f) => Draw::text_width(&f, s, size, ls),
            None => s.chars().count() as f32 * size * 0.55,
        }
    }

    fn text_path(&mut self, s: &str, x: f32, cy: f32, o: T) -> Option<Path> {
        let family = self.font.clone();
        let f = self.d.font(&family, o.weight)?;
        let w = Draw::text_width(&f, s, o.size, o.ls);
        let x0 = x - w * (o.anchor as f32 + 1.0) / 2.0;
        let p = Draw::text_path(
            &f,
            s,
            o.size,
            o.ls,
            TextAlign::Left,
            VerticalAlign::Middle,
            (x0, cy - o.size, w.max(1.0) + 4.0, o.size * 2.0),
            crate::text::LINE_HEIGHT,
        )?;
        if o.sx == 1.0 && o.sy == 1.0 && o.skew == 0.0 {
            return Some(p);
        }
        let t = Transform::from_translate(x, cy)
            .pre_concat(Transform::from_row(
                o.sx,
                0.0,
                -o.skew * o.sx,
                o.sy,
                0.0,
                0.0,
            ))
            .pre_translate(-x, -cy);
        p.transform(t)
    }

    /// Text centred vertically on `cy`, anchored at `x`.
    pub fn text(&mut self, s: &str, x: f32, cy: f32, c: &str, o: T) {
        self.text_c(s, x, cy, col(c), o);
    }

    pub fn text_c(&mut self, s: &str, x: f32, cy: f32, c: Color, o: T) {
        let nums = [
            x,
            cy,
            c.0[0],
            c.0[1],
            c.0[2],
            c.0[3],
            o.size,
            f32::from(o.weight),
            f32::from(o.anchor),
            o.ls,
            o.sx,
            o.sy,
            o.skew,
        ];
        if self.record("text", &[], &nums, s)
            || self.dry
            || self.skip
            || s.is_empty()
            || c.0[3] <= 0.0
            || o.sy.abs() < 0.01
        {
            return;
        }
        let Some(p) = self.text_path(s, x, cy, o) else {
            return;
        };
        let paint = self.paint(c);
        self.d
            .pm
            .fill_path(&p, &paint, FillRule::Winding, self.d.tf, self.mask.as_ref());
    }

    /// Text with a halo (outline under the fill).
    #[allow(clippy::too_many_arguments)]
    pub fn text_halo(&mut self, s: &str, x: f32, cy: f32, c: Color, halo: Color, hw: f32, o: T) {
        let nums = [
            x,
            cy,
            c.0[0],
            c.0[1],
            c.0[2],
            c.0[3],
            halo.0[0],
            halo.0[1],
            halo.0[2],
            halo.0[3],
            hw,
            o.size,
            f32::from(o.weight),
            f32::from(o.anchor),
            o.ls,
            o.sx,
            o.sy,
            o.skew,
        ];
        if self.record("halo", &[], &nums, s) || self.dry || self.skip || s.is_empty() {
            return;
        }
        let Some(p) = self.text_path(s, x, cy, o) else {
            return;
        };
        let st = Stroke {
            width: hw,
            line_join: tiny_skia::LineJoin::Round,
            ..Default::default()
        };
        let hp = self.paint(halo);
        self.d
            .pm
            .stroke_path(&p, &hp, &st, self.d.tf, self.mask.as_ref());
        let paint = self.paint(c);
        self.d
            .pm
            .fill_path(&p, &paint, FillRule::Winding, self.d.tf, self.mask.as_ref());
    }

    /// A picture fitted (contain) in the box.
    pub fn image(&mut self, src: &str, x: f32, y: f32, w: f32, h: f32) -> bool {
        if self.record("image", &[], &[x, y, w, h], src) {
            // Whether it is there decides what is drawn instead.
            return self.d.image_size(src).is_some();
        }
        if self.dry {
            return true;
        }
        if self.skip {
            return self.d.image_size(src).is_some();
        }
        let Some((iw, ih)) = self.d.image_size(src) else {
            return false;
        };
        let s = (w / iw as f32).min(h / ih as f32);
        let t =
            Transform::from_translate(x + (w - iw as f32 * s) / 2.0, y + (h - ih as f32 * s) / 2.0)
                .pre_scale(s, s);
        let a = self.alpha;
        let m = self.mask.clone();
        self.d.image(src, t, a, m.as_ref()).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{LineCap, LineJoin, PathBuilder, Pixmap};

    /// Contours in view, across its edge, and far out of it (a map's world).
    fn world() -> Path {
        let mut pb = PathBuilder::new();
        for (x, y) in [
            (10.0, 10.0),
            (-30.0, 40.0),
            (400.0, 50.0),
            (-900.0, -700.0),
            (5000.0, 30.0),
        ] {
            pb.move_to(x, y);
            pb.line_to(x + 60.0, y + 5.0);
            pb.cubic_to(x + 80.0, y + 30.0, x + 40.0, y + 70.0, x + 10.0, y + 50.0);
            pb.close();
        }
        // An open polyline leaving the frame and coming back.
        pb.move_to(20.0, 90.0);
        for k in 0..40 {
            pb.line_to(
                20.0 + k as f32 * 37.0 % 700.0 - 200.0,
                90.0 + (k % 3) as f32 * 9.0,
            );
        }
        pb.finish().unwrap()
    }

    /// Dropping what is out of view draws the same pixels, filled, stroked,
    /// dashed, through a transform.
    #[test]
    fn what_is_out_of_view_draws_nothing() {
        let p = world();
        let tf = Transform::from_row(1.3, 0.1, -0.05, 1.2, 7.5, -3.25);
        let (w, h) = (240, 160);
        let mut paint = Paint::default();
        paint.set_color_rgba8(200, 30, 90, 180);
        paint.anti_alias = true;
        let part = in_view(&p, tf, w, h, 2.0).unwrap().unwrap();
        assert!(part.len() < p.len());
        let mut a = Pixmap::new(w, h).unwrap();
        let mut b = Pixmap::new(w, h).unwrap();
        a.fill_path(&p, &paint, FillRule::Winding, tf, None);
        b.fill_path(&part, &paint, FillRule::Winding, tf, None);
        assert_eq!(a.data(), b.data());
        let mut s = Stroke {
            width: 3.0,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        let margin = 3.0 * 1.4 * 2.0 + 4.0;
        let part = in_view(&p, tf, w, h, margin).unwrap().unwrap();
        let (mut a, mut b) = (Pixmap::new(w, h).unwrap(), Pixmap::new(w, h).unwrap());
        a.stroke_path(&p, &paint, &s, tf, None);
        b.stroke_path(&part, &paint, &s, tf, None);
        assert_eq!(a.data(), b.data());
        // Dashed: the dashes out of view dropped.
        let dash = StrokeDash::new(vec![7.0, 4.0], 0.0).unwrap();
        let dashed = part
            .dash(&dash, tiny_skia::PathStroker::compute_resolution_scale(&tf))
            .unwrap();
        let dashed = in_view(&dashed, tf, w, h, margin).unwrap().unwrap();
        let (mut a, mut b) = (Pixmap::new(w, h).unwrap(), Pixmap::new(w, h).unwrap());
        b.stroke_path(&dashed, &paint, &s, tf, None);
        s.dash = Some(dash);
        a.stroke_path(&p, &paint, &s, tf, None);
        assert_eq!(a.data(), b.data());
        // All of it in view: left as it is; none of it: nothing to draw.
        assert!(in_view(&p, Transform::from_scale(0.001, 0.001), w, h, 2.0).is_none());
        let far = Transform::from_translate(10_000.0, 0.0);
        assert!(matches!(in_view(&p, far, w, h, 2.0), Some(None)));
    }
}
