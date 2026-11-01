//! Drawing of the spec widgets (`strata_mv_core::wid`), one module each. A module
//! returns false while it does not draw its layer, so the generic code of
//! `lib.rs` still does.

mod broadcast;
mod ellipse;
mod fit;
mod image;
mod polygon;
mod quad;
mod scope;
mod shape;
mod source;
mod text;
mod ticker;
mod timer;

use strata_mv_core::anim::{Data, ResolvedLayer};
use strata_mv_core::widgets::TimeCtx;
use strata_mv_core::{Color, Layer, Prop, TextAlign, VerticalAlign};
use tiny_skia::{FillRule, Mask, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, Transform};

use ab_glyph::FontArc;

use crate::cache::Motion;
use crate::text as tx;
use crate::{build_mask, paint_for, Renderer};

/// Everything a widget needs to draw itself, in layer space: (0, 0) is the
/// top-left of its box, `w` × `h` its size.
pub(crate) struct Draw<'a, 'b> {
    pub rd: &'a mut Renderer,
    pub pm: &'a mut Pixmap,
    pub r: &'a ResolvedLayer<'b>,
    pub layer: &'b Layer,
    /// Layer space → picture.
    pub tf: Transform,
    /// Clip of the parents (None = none).
    pub mask: Option<&'a Mask>,
    pub opacity: f32,
    pub w: f32,
    pub h: f32,
    /// Wall clock and time on air of this layer.
    pub time: TimeCtx<'a>,
    pub data: &'a Data,
    pub fps: f64,
    /// The operator's state of this widget (`Null` = default).
    pub state: &'a serde_json::Value,
    /// The project's language.
    pub lang: &'a str,
    /// A Text's styled parts (bold, colour…), none when it has one style.
    pub runs: Option<Vec<strata_mv_core::rich::Run>>,
    /// A Text's pictures (`<img>`), by rank.
    pub inline: Vec<strata_mv_core::rich::InlineImage>,
    /// The layer (or a parent) moves or changes size now (keyframes):
    /// pictures are scaled with a cheaper filter while it does.
    pub moving: bool,
}

impl Draw<'_, '_> {
    /// Value of an animated property (fill, stroke, strokeWidth, cornerRadius, fontSize…).
    pub fn num(&self, p: Prop) -> f32 {
        self.r.num(p) as f32
    }
    pub fn color(&self, p: Prop) -> Color {
        self.r.color(p)
    }
    /// A paint of `c` with the layer's opacity.
    pub fn paint(&self, c: Color) -> Paint<'static> {
        paint_for(c, self.opacity)
    }
    /// `#rrggbb[aa]` → colour (transparent when invalid).
    pub fn hex(s: &str) -> Color {
        Color::from_hex(s).unwrap_or(Color::TRANSPARENT)
    }
    /// A clip to (x, y, w, h) of the layer, inside the parents' clip.
    pub fn clip(&self, x: f32, y: f32, w: f32, h: f32) -> Option<Mask> {
        Rect::from_xywh(x, y, w.max(0.01), h.max(0.01))
            .map(|r| build_mask(self.pm, self.mask, r, self.tf))
    }
    /// The clip of the layer's box.
    pub fn boxed(&self) -> Option<Mask> {
        self.clip(0.0, 0.0, self.w, self.h)
    }
    pub fn fill(&mut self, path: &Path, c: Color, rule: FillRule, mask: Option<&Mask>) {
        let p = self.paint(c);
        self.pm
            .fill_path(path, &p, rule, self.tf, mask.or(self.mask));
    }
    pub fn fill_paint(&mut self, path: &Path, p: &Paint, rule: FillRule, mask: Option<&Mask>) {
        self.pm
            .fill_path(path, p, rule, self.tf, mask.or(self.mask));
    }
    pub fn stroke(&mut self, path: &Path, c: Color, s: &Stroke, mask: Option<&Mask>) {
        let p = self.paint(c);
        self.pm
            .stroke_path(path, &p, s, self.tf, mask.or(self.mask));
    }
    pub fn stroke_paint(&mut self, path: &Path, p: &Paint, s: &Stroke, mask: Option<&Mask>) {
        self.pm.stroke_path(path, p, s, self.tf, mask.or(self.mask));
    }
    /// A font (falls back to Inter, then any font).
    pub fn font(&mut self, family: &str, weight: u16) -> Option<FontArc> {
        self.rd
            .fonts
            .font(family, weight)
            .or_else(|| self.rd.fonts.font("Inter", weight))
    }
    /// Advance width of one line of text.
    pub fn text_width(font: &FontArc, s: &str, size: f32, letter_spacing: f32) -> f32 {
        tx::text_width(font, s, size, letter_spacing)
    }
    /// Outline of `s` laid out in the box (x, y, w, h) of the layer.
    #[allow(clippy::too_many_arguments)]
    pub fn text_path(
        font: &FontArc,
        s: &str,
        size: f32,
        letter_spacing: f32,
        align: TextAlign,
        valign: VerticalAlign,
        (x, y, w, h): (f32, f32, f32, f32),
        line_height: f32,
    ) -> Option<Path> {
        let style = tx::TextStyle {
            size,
            letter_spacing,
            align,
            vertical_align: valign,
            box_w: w,
            box_h: h,
            line_height,
            decoration: strata_mv_core::TextDecoration::None,
        };
        tx::layout_text(font, s, &style)?.transform(Transform::from_translate(x, y))
    }
    /// Draws `s` in the box (x, y, w, h) with one colour.
    #[allow(clippy::too_many_arguments)]
    pub fn text(
        &mut self,
        family: &str,
        weight: u16,
        size: f32,
        s: &str,
        c: Color,
        align: TextAlign,
        valign: VerticalAlign,
        bx: (f32, f32, f32, f32),
        mask: Option<&Mask>,
    ) {
        let Some(f) = self.font(family, weight) else {
            return;
        };
        if let Some(p) = Self::text_path(&f, s, size, 0.0, align, valign, bx, tx::LINE_HEIGHT) {
            self.fill(&p, c, FillRule::Winding, mask);
        }
    }
    /// Draws a picture (asset path) with a transform from picture pixels to layer space.
    pub fn image(
        &mut self,
        src: &str,
        img_tf: Transform,
        opacity: f32,
        mask: Option<&Mask>,
    ) -> Option<(u32, u32)> {
        let tf = self.tf.pre_concat(img_tf);
        let m = mask.or(self.mask);
        self.rd.draw_picture(
            self.pm,
            src,
            tf,
            self.moving,
            opacity * self.opacity,
            m,
            None,
        )
    }
    /// Size of a picture, in pixels (None if missing).
    pub fn image_size(&mut self, src: &str) -> Option<(u32, u32)> {
        self.rd.image(src).map(|i| (i.width(), i.height()))
    }
}

/// A rounded rectangle path (radius capped at half the smallest side).
pub(crate) fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.max(0.0).min(w / 2.0).min(h / 2.0);
    let mut pb = PathBuilder::new();
    if r <= 0.01 {
        pb.push_rect(Rect::from_xywh(x, y, w, h)?);
        return pb.finish();
    }
    let k = 0.5523 * r;
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

/// Draws a spec widget; false = left to the generic code.
pub(crate) fn draw(d: &mut Draw) -> bool {
    let Some(id) = strata_mv_core::wid::widget_id(&d.layer.kind) else {
        return false;
    };
    let raw = d.state.clone();
    fn state<T: serde::de::DeserializeOwned + Default>(v: &serde_json::Value) -> T {
        serde_json::from_value(v.clone()).unwrap_or_default()
    }
    match id {
        "quad" => quad::draw(d, &state(&raw)),
        "polygon" => polygon::draw(d, &state(&raw)),
        "ellipse" => ellipse::draw(d, &state(&raw)),
        "text" => text::draw(d, &state(&raw)),
        "image" => image::draw(d, &state(&raw)),
        "timer" => timer::draw(d, &state(&raw)),
        "ticker" => ticker::draw(d, &state(&raw)),
        "source" => source::draw(d),
        "audioMeter" | "loudness" | "timecode" | "status" | "alarmLog" | "signalInfo" => {
            broadcast::draw(d)
        }
        "scope" => scope::draw(d),
        // No browser here: its box stays empty on these outputs.
        _ => false,
    }
}

/// How a spec widget's picture depends on time (the frames kept, `cache`):
/// None when `draw` would leave the layer to the generic code. Each module's
/// `motion` reads time exactly as its `draw` does; a widget without one
/// moves.
pub(crate) fn motion(d: &mut Draw) -> Option<Motion> {
    let id = strata_mv_core::wid::widget_id(&d.layer.kind)?;
    fn state<T: serde::de::DeserializeOwned + Default>(v: &serde_json::Value) -> T {
        serde_json::from_value(v.clone()).unwrap_or_default()
    }
    let raw = d.state;
    match id {
        // Nothing of time in their drawing.
        "quad" | "polygon" | "ellipse" => Some(Motion::Still),
        "text" => text::motion(d),
        "image" => image::motion(d, &state(raw)),
        "source" => source::motion(d),
        "audioMeter" | "loudness" | "timecode" | "status" | "alarmLog" | "scope" | "signalInfo" => {
            broadcast::motion(d)
        }
        "timer" => timer::motion(d, &state(raw)),
        // Tickers: by the time on air.
        _ => Some(Motion::Moving),
    }
}
