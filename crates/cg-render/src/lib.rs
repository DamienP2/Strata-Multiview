//! Software render engine: turns resolved templates into RGBA frames.
//!
//! The renderer is CPU based (tiny-skia) so it runs headless on any server;
//! the scene evaluation is independent from the rasteriser so a GPU backend
//! (wgpu) can be added behind the same `Renderer` API.

// Drawing functions take the canvas, the box, the style and the time: many arguments by nature.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod cache;
mod mask;
pub mod pool;
mod scaled;
pub mod sequence;
mod shadow;
pub mod slate;
mod sprite;
mod strip;
mod text;
mod wid;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use cg_core::anim::{interpolate_text, resolve_layer, Data, ResolvedLayer};
use cg_core::widgets::{self, clock_text, scroll_positions, TimeCtx};
use cg_core::{
    ChannelState, Color, FontStyle, ImageFit, LayerKind, Project, Prop, Template, TextAlign, Value,
    VerticalAlign,
};
use tiny_skia::{FillRule, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, Rect, Stroke, Transform};

/// Text widths with the renderer's fonts, for the server's events (a
/// crawl's passes): the fonts load at the first width asked.
pub struct TextWidths {
    dir: PathBuf,
    fonts: Option<FontStore>,
}

impl TextWidths {
    pub fn new(assets_dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: assets_dir.into(),
            fonts: None,
        }
    }
}

impl cg_core::playout::TextWidth for TextWidths {
    fn width(
        &mut self,
        family: &str,
        weight: u16,
        text: &str,
        size: f32,
        letter_spacing: f32,
    ) -> f32 {
        let dir = self.dir.join("fonts");
        let fonts = self.fonts.get_or_insert_with(|| FontStore::new(Some(&dir)));
        match fonts
            .font(family, weight)
            .or_else(|| fonts.font("Inter", weight))
        {
            Some(f) => text::text_width(&f, text, size, letter_spacing),
            None => {
                cg_core::playout::EstimatedWidth.width(family, weight, text, size, letter_spacing)
            }
        }
    }
}

pub use cache::FrameCache;
pub use text::{fonts_changed, FontFamily, FontStore, BUNDLED_FONTS};
pub use wid::qrcode::qr_modules_ec;

/// QR modules of `content` (row-major, true = dark) and the side length,
/// shared with the HTML5 renderer so both draw the same code.
pub fn qr_modules(content: &str) -> Option<(usize, Vec<bool>)> {
    let code =
        qrcode::QrCode::with_error_correction_level(content.as_bytes(), qrcode::EcLevel::M).ok()?;
    Some((
        code.width(),
        code.to_colors()
            .into_iter()
            .map(|c| c == qrcode::Color::Dark)
            .collect(),
    ))
}

/// A picture as JPEG (previews of the live inputs), at most `max_width` wide.
pub fn jpeg(pm: &Pixmap, max_width: u32, quality: u8) -> Option<Vec<u8>> {
    let (w, h) = (pm.width(), pm.height());
    let k = (max_width as f32 / w.max(1) as f32).min(1.0);
    let (tw, th) = (
        ((w as f32 * k).round() as u32).max(1),
        ((h as f32 * k).round() as u32).max(1),
    );
    let small;
    let src = if k < 1.0 {
        let mut out = Pixmap::new(tw, th)?;
        out.draw_pixmap(
            0,
            0,
            pm.as_ref(),
            &PixmapPaint {
                quality: tiny_skia::FilterQuality::Bilinear,
                ..Default::default()
            },
            Transform::from_scale(k, k),
            None,
        );
        small = out;
        &small
    } else {
        pm
    };
    // Over black, without alpha.
    let rgb: Vec<u8> = src
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
        .encode(
            &rgb,
            src.width(),
            src.height(),
            image::ExtendedColorType::Rgb8,
        )
        .ok()?;
    Some(out)
}

/// A rendered video frame, RGBA 8-bit **premultiplied** alpha.
#[derive(Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    /// Wall-clock time the frame was produced (drives RTP timestamps).
    pub captured_at: SystemTime,
}

/// Its buffer goes back to the pool, for the next frame (`pool`).
impl Drop for Frame {
    fn drop(&mut self) {
        pool::give(std::mem::take(&mut self.data));
    }
}

impl Frame {
    pub fn blank(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0; (width * height * 4) as usize],
            captured_at: SystemTime::now(),
        }
    }

    /// Encodes as PNG (straight alpha), for previews and thumbnails.
    pub fn to_png(&self) -> Vec<u8> {
        let pm = Pixmap::from_vec(
            self.data.clone(),
            tiny_skia::IntSize::from_wh(self.width, self.height).unwrap(),
        );
        pm.and_then(|p| p.encode_png().ok()).unwrap_or_default()
    }
}

/// The pictures of the live video inputs (cg-server), for the Video widget.
pub trait LiveInputs: Send + Sync {
    /// The latest picture of an input (premultiplied RGBA), if it has one.
    fn picture(&self, id: &str) -> Option<Arc<Pixmap>>;
}

pub struct Renderer {
    fonts: FontStore,
    font_generation: u64,
    assets_dir: PathBuf,
    images: HashMap<String, Option<Pixmap>>,
    live: Option<Arc<dyn LiveInputs>>,
    /// Frames kept and layouts remembered (off: everything drawn from scratch).
    caching: bool,
    /// Bumped when the pictures are read again (`invalidate_assets`).
    assets_generation: u64,
    /// A web picture was asked for while drawing and is not there yet: the
    /// frame shows without it and is not kept.
    pending: bool,
    /// The frame being drawn, step by step (see `cache`).
    steps: Steps,
    /// A frame-sized picture for the drop shadows (cleared before each use).
    scratch: Option<Pixmap>,
    /// Pictures kept scaled to the size they are drawn at (see `scaled`).
    scaled: scaled::Scaled,
    /// Masks kept from frame to frame (faded zones, `mask::Kept`).
    masks: mask::Kept,
    /// Strips of the credits rolls (`strip`).
    strips: strip::Strips,
    /// A frame-sized picture for the graphics drawn over what moves
    /// (`cache::top_item`), kept cleared between frames.
    over: Option<Pixmap>,
    /// Layers kept drawn (`sprite`).
    sprites: sprite::Sprites,
    /// Scale of the output over the project (sprites of moving layers).
    root_scale: f32,
    /// Drawing a sprite (no sprite inside one, pictures at their best).
    in_sprite: bool,
    /// Sprites off: every layer drawn straight (to compare, to measure).
    sprites_on: bool,
}

/// Where a frame being drawn is, in its steps (see `cache`).
#[derive(Default)]
struct Steps {
    /// Planning: the steps met so far (None while drawing).
    plan: Option<Vec<cache::Op>>,
    /// Planning: the graphic on air the steps belong to.
    item: usize,
    /// Planning: inside a Lottie group the operator controls (its own clock).
    clocked: u32,
    /// Drawing: the steps of the plan (empty: the frame is drawn whole).
    ops: Vec<cache::Op>,
    /// Drawing: the step reached.
    at: usize,
    /// Drawing: the steps already in the picture.
    skip: usize,
    /// Drawing: the picture before this step is kept.
    keep: Option<(usize, Option<Pixmap>)>,
    /// Inside a step drawn as one (a drop shadow, a cross-fade).
    whole: u32,
    /// Drawing a layer in two steps (`sub_step`) whose first one is in the
    /// picture already: the widget draws only the second.
    under: bool,
    /// Drawing a layer in two steps: its second one not begun yet.
    split: bool,
    /// Inside a cross-fade (the graphic drawn twice: no sprites).
    fading: bool,
    /// Drawing the sprite of a layer drawn in two steps: its first only.
    base_only: bool,
    /// Values sliding or turning over in the layers showing them.
    anim: Option<ValueAnim>,
}

/// The layers showing values a data source changed, moving in their box
/// (`ValueChange::Slide`, `Flip`), by layer id: how and how far (eased, 0 → 1).
/// `old`: the pass drawing them with the old values (only them).
struct ValueAnim {
    layers: HashMap<String, (cg_core::ValueChange, f32)>,
    old: bool,
}

/// Where a layer showing a changing value is drawn (`h` its height), or
/// nowhere (turned edge on).
fn value_tf(
    tf: Transform,
    kind: cg_core::ValueChange,
    e: f32,
    old: bool,
    h: f32,
) -> Option<Transform> {
    match kind {
        cg_core::ValueChange::Slide => {
            Some(tf.pre_translate(0.0, if old { -e * h } else { (1.0 - e) * h }))
        }
        cg_core::ValueChange::Flip => {
            let s = if old { 1.0 - 2.0 * e } else { 2.0 * e - 1.0 };
            (s > 0.001).then(|| {
                tf.pre_translate(0.0, h / 2.0)
                    .pre_scale(1.0, s)
                    .pre_translate(0.0, -h / 2.0)
            })
        }
        _ => Some(tf),
    }
}

/// Values of the layers changed by a template's logic graph, by layer id.
type Logic = HashMap<String, std::collections::BTreeMap<Prop, Value>>;

/// A graphic on air, prepared for drawing only when it is drawn (or planned).
struct OnAir<'p> {
    layer: u32,
    item: &'p cg_core::PlayoutItem,
    /// Below 1: the graphic replaced in a « Fade » zone, fading out under the new one.
    alpha: f32,
    template: &'p Template,
    /// Laid out for the format, with the operator's settings.
    set: std::cell::OnceCell<std::borrow::Cow<'p, Template>>,
    logic: std::cell::OnceCell<Logic>,
    /// What the graphic's state is drawn from (`sprite`), hashed.
    key: std::cell::OnceCell<u64>,
    /// Its data as shown now (values counting on their way).
    data: std::borrow::Cow<'p, Data>,
}

/// A graphic on air, hashed (its template, data, widget states and clocks:
/// its timeline frame and age aside, which the layers' values and motion
/// keys carry).
fn item_key(item: &cg_core::PlayoutItem) -> u64 {
    let mut k = sprite::Key::new();
    k.add(&item.template_id).add(&item.data);
    k.add(&serde_json::to_string(&item.widgets).unwrap_or_default());
    k.add(&serde_json::to_string(&item.clocks).unwrap_or_default());
    k.finish()
}

/// `t` laid out for `format`, with the settings in `data`.
fn prepared<'p>(t: &'p Template, format: &str, data: &Data) -> std::borrow::Cow<'p, Template> {
    use std::borrow::Cow;
    match t.in_format(format) {
        Cow::Borrowed(t) => t.with_settings(data),
        Cow::Owned(laid) => {
            let set = match laid.with_settings(data) {
                Cow::Owned(t) => Some(t),
                Cow::Borrowed(_) => None,
            };
            Cow::Owned(set.unwrap_or(laid))
        }
    }
}

struct Ctx<'a> {
    template: &'a Template,
    frame: f64,
    data: &'a Data,
    time: TimeCtx<'a>,
    /// Values of the layers changed by the template's logic graph.
    logic: &'a Logic,
    /// The graphic on air (widget clocks the operator controls), and the
    /// project frame rate its ages are counted in.
    item: Option<&'a cg_core::PlayoutItem>,
    fps: f64,
    /// The project's language (texts without their own).
    lang: &'a str,
    /// The graphic on air: its index among those on air and its state
    /// hashed (`item_key`), for the sprites.
    item_index: usize,
    item_key: u64,
}

impl<'a> Ctx<'a> {
    /// The same at another frame (the layers of a Lottie group the operator controls).
    fn at_frame(&self, frame: f64) -> Ctx<'a> {
        Ctx {
            template: self.template,
            frame,
            data: self.data,
            time: self.time,
            logic: self.logic,
            item: self.item,
            fps: self.fps,
            lang: self.lang,
            item_index: self.item_index,
            item_key: self.item_key,
        }
    }

    /// Time inputs of one layer: its own clock when the operator controls it.
    fn time_for(&self, layer: &cg_core::Layer) -> TimeCtx<'_> {
        match self.item {
            Some(item) => TimeCtx {
                age_secs: item.layer_age(layer) / self.fps,
                ..self.time
            },
            None => self.time,
        }
    }
}

/// What drawing a step does (see `Renderer::enter_step`).
enum Enter {
    /// The step and its children are in the picture already.
    Done,
    /// The step is, some of its children may not be.
    Children,
    /// To draw.
    Draw,
}

/// The graphics of `frame` keyed over a live picture (the channel's
/// source): the picture scaled to fit the frame, centred on black, the
/// graphics laid over it. The frame is opaque.
pub fn over_live(frame: &Frame, live: &Pixmap) -> Frame {
    let (w, h) = (frame.width, frame.height);
    let mut pm = pool::pixmap(w, h);
    if (live.width(), live.height()) == (w, h) {
        pm.data_mut().copy_from_slice(live.data());
    } else {
        pm.data_mut()
            .copy_from_slice(fit_picture(live, w, h).data());
    }
    scaled::composite_bytes(&mut pm, &frame.data, w, h, 0, 0, 1.0, None);
    Frame {
        width: w,
        height: h,
        data: pm.take(),
        captured_at: frame.captured_at,
    }
}

/// `live` fitted into a `w` × `h` picture: scaled (bilinear) to fit,
/// centred on black. Rows are shared out over a few threads: a 720p
/// picture into 1080 takes a few milliseconds.
pub fn fit_picture(live: &Pixmap, w: u32, h: u32) -> Pixmap {
    let mut out = Pixmap::new(w.max(1), h.max(1)).expect("picture size");
    out.fill(tiny_skia::Color::BLACK);
    let (lw, lh) = (live.width() as usize, live.height() as usize);
    let k = (w as f64 / lw.max(1) as f64).min(h as f64 / lh.max(1) as f64);
    let (dw, dh) = (
        ((lw as f64 * k).round() as usize).min(w as usize),
        ((lh as f64 * k).round() as usize).min(h as usize),
    );
    if lw == 0 || lh == 0 || dw == 0 || dh == 0 {
        return out;
    }
    if (dw, dh) == (lw, lh) && (dw, dh) == (w as usize, h as usize) {
        out.data_mut().copy_from_slice(live.data());
        return out;
    }
    let (ox, oy) = ((w as usize - dw) / 2, (h as usize - dh) / 2);
    // Where each output column / row samples the source: the two pixels
    // around its centre and the weight of the second (in 1/256).
    let taps = |n_out: usize, n_in: usize| -> Vec<(usize, usize, u32)> {
        (0..n_out)
            .map(|i| {
                let x = ((i as f64 + 0.5) * n_in as f64 / n_out as f64 - 0.5).max(0.0);
                let a = (x as usize).min(n_in - 1);
                let b = (a + 1).min(n_in - 1);
                (a, b, ((x - a as f64) * 256.0) as u32)
            })
            .collect()
    };
    let (cols, rows) = (taps(dw, lw), taps(dh, lh));
    let src = live.data();
    let stride = w as usize * 4;
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .clamp(1, 4);
    let band = dh.div_ceil(threads);
    let body = &mut out.data_mut()[oy * stride..(oy + dh) * stride];
    std::thread::scope(|s| {
        for (t, part) in body.chunks_mut(band * stride).enumerate() {
            let (cols, rows) = (&cols, &rows);
            s.spawn(move || {
                // Source rows scaled across, kept while the next output row needs them.
                // (16-bit arithmetic all through: what SSE2 multiplies fast.)
                let mut cache: [(usize, Vec<u8>); 2] =
                    [(usize::MAX, vec![0; dw * 4]), (usize::MAX, vec![0; dw * 4])];
                let across = |y: usize, row: &mut Vec<u8>| {
                    let line = src[y * lw * 4..(y + 1) * lw * 4].as_chunks::<4>().0;
                    for (o, &(a, b, f)) in row.as_chunks_mut::<4>().0.iter_mut().zip(cols) {
                        let (pa, pb, f) = (line[a], line[b], f as u16);
                        for c in 0..4 {
                            o[c] = ((pa[c] as u16 * (256 - f) + pb[c] as u16 * f + 128) >> 8) as u8;
                        }
                    }
                };
                for (j, out_row) in part.chunks_mut(stride).enumerate() {
                    let (a, b, f) = rows[t * band + j];
                    // On one source row (the last one): that row alone.
                    let f = if a == b { 0 } else { f };
                    for (slot, y) in [(0, a), (1, b)]
                        .into_iter()
                        .take(if f == 0 { 1 } else { 2 })
                    {
                        if cache[slot].0 != y {
                            if let Some(other) = cache.iter().position(|(r, _)| *r == y) {
                                cache.swap(slot, other);
                            } else {
                                across(y, &mut cache[slot].1);
                                cache[slot].0 = y;
                            }
                        }
                    }
                    let (ra, rb, f) = (&cache[0].1, &cache[1].1, f as u16);
                    let g = 256 - f;
                    let dst = &mut out_row[ox * 4..(ox + dw) * 4];
                    if f == 0 {
                        dst.copy_from_slice(ra);
                        continue;
                    }
                    for ((d, &a), &b) in dst.iter_mut().zip(ra).zip(rb) {
                        *d = ((a as u16 * g + b as u16 * f + 128) >> 8) as u8;
                    }
                }
            });
        }
    });
    out
}

/// Output raster of a channel. Projects designed for another size are
/// scaled to fit (letterboxed) and their timelines are re-timed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutputFormat {
    pub width: u32,
    pub height: u32,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl Renderer {
    pub fn new(assets_dir: impl Into<PathBuf>) -> Self {
        let assets_dir = assets_dir.into();
        let fonts = FontStore::new(Some(&assets_dir.join("fonts")));
        Self {
            fonts,
            font_generation: text::font_generation(),
            assets_dir,
            images: HashMap::new(),
            live: None,
            caching: true,
            assets_generation: 0,
            pending: false,
            steps: Steps::default(),
            scratch: None,
            scaled: Default::default(),
            over: None,
            masks: Default::default(),
            strips: Default::default(),
            sprites: Default::default(),
            root_scale: 1.0,
            in_sprite: false,
            sprites_on: true,
        }
    }

    /// Caches on (the default) or off: with them off every frame is drawn
    /// whole, from scratch (to compare, and to measure).
    pub fn set_cache(&mut self, on: bool) {
        self.caching = on;
    }

    /// Sprites (layers kept drawn, `sprite`) on or off: off, every layer
    /// is drawn straight each frame (to compare, and to measure).
    pub fn set_sprites(&mut self, on: bool) {
        self.sprites_on = on;
    }

    /// Sprites made and laid again so far (measures).
    pub fn sprite_counts(&self) -> (u64, u64) {
        (self.sprites.made, self.sprites.found)
    }

    /// Where the Video widget finds the pictures of the live inputs.
    pub fn set_live(&mut self, live: Arc<dyn LiveInputs>) {
        self.live = Some(live);
    }

    /// The latest picture of a live input.
    pub fn live_picture(&self, id: &str) -> Option<Arc<Pixmap>> {
        self.live.as_ref()?.picture(id)
    }

    /// Picks up fonts added since the renderer was created.
    fn refresh_fonts(&mut self) {
        let g = text::font_generation();
        if g != self.font_generation {
            self.fonts = FontStore::new(Some(&self.assets_dir.join("fonts")));
            self.font_generation = g;
        }
    }

    /// Runs the template's logic graph; returns the layers it changed.
    fn logic_values(&mut self, t: &Template, frame: f64, data: &Data) -> Logic {
        if t.logic.is_empty() {
            return HashMap::new();
        }
        let hold = t.hold_frame();
        let mut values: cg_core::logic::Values = t
            .layers
            .iter()
            .map(|l| (l.id.clone(), resolve_layer(l, frame, data, hold).values))
            .collect();
        let fonts = &mut self.fonts;
        let mut measure = |id: &str,
                           v: &std::collections::BTreeMap<Prop, Value>,
                           metric: cg_core::logic::TextMetric|
         -> f64 {
            let Some(LayerKind::Text {
                text,
                font_family,
                font_weight,
                uppercase,
                transform,
                line_height,
                ..
            }) = t.layers.iter().find(|l| l.id == id).map(|l| &l.kind)
            else {
                return 0.0;
            };
            let mut content = interpolate_text(text, data);
            content = if *uppercase {
                content.to_uppercase()
            } else {
                transform.apply(&content)
            };
            let line_h = line_height.unwrap_or(text::LINE_HEIGHT) as f64;
            let num = |p: Prop| v.get(&p).and_then(Value::as_number).unwrap_or(0.0);
            let size = num(Prop::FontSize);
            let lines = content.split('\n').count() as f64;
            match metric {
                cg_core::logic::TextMetric::Lines => lines,
                cg_core::logic::TextMetric::Chars => content.chars().count() as f64,
                cg_core::logic::TextMetric::Height => {
                    if content.is_empty() {
                        0.0
                    } else {
                        lines * size * line_h
                    }
                }
                cg_core::logic::TextMetric::Width => match fonts.font(font_family, *font_weight) {
                    Some(f) if !content.is_empty() => {
                        text::text_width(&f, &content, size as f32, num(Prop::LetterSpacing) as f32)
                            as f64
                    }
                    _ => 0.0,
                },
            }
        };
        cg_core::logic::apply(&t.logic, &mut values, data, &mut measure);
        let targeted: std::collections::HashSet<&str> = t
            .logic
            .nodes
            .iter()
            .filter_map(|n| match &n.kind {
                cg_core::logic::NodeKind::Target { layer, .. } => Some(layer.as_str()),
                _ => None,
            })
            .collect();
        values.retain(|k, _| targeted.contains(k.as_str()));
        values
    }

    pub fn font_families(&mut self) -> Vec<FontFamily> {
        self.refresh_fonts();
        self.fonts.families()
    }

    pub fn font_file(&mut self, family: &str, weight: u16) -> Option<Vec<u8>> {
        self.refresh_fonts();
        self.fonts.file(family, weight)
    }

    /// Forgets cached images (call after an asset upload).
    pub fn invalidate_assets(&mut self) {
        self.sprites.clear();
        self.images.clear();
        self.scaled.clear();
        self.assets_generation += 1;
    }

    /// A still picture (`/assets/…`) filling the whole frame, cropped to the
    /// frame's shape around its centre: what a channel shows while no project
    /// is loaded. Transparent when the picture cannot be read.
    pub fn render_slate(&mut self, src: &str, out: OutputFormat) -> Frame {
        let (w, h) = (out.width.max(1), out.height.max(1));
        let mut pm = Pixmap::new(w, h).expect("valid size");
        if let Some(img) = self.image(src) {
            let k = (w as f32 / img.width() as f32).max(h as f32 / img.height() as f32);
            let tf = Transform::from_translate(
                (w as f32 - img.width() as f32 * k) / 2.0,
                (h as f32 - img.height() as f32 * k) / 2.0,
            )
            .pre_scale(k, k);
            let paint = PixmapPaint {
                quality: tiny_skia::FilterQuality::Bicubic,
                ..Default::default()
            };
            pm.draw_pixmap(0, 0, img.as_ref(), &paint, tf, None);
        }
        Frame {
            width: w,
            height: h,
            data: pm.take(),
            captured_at: SystemTime::now(),
        }
    }

    /// Composites every playout layer of a channel, lowest layer first.
    /// `default_tz` is the zone clocks use when they do not set one.
    pub fn render_channel(
        &mut self,
        project: &Project,
        state: &ChannelState,
        out: OutputFormat,
        default_tz: &str,
    ) -> Frame {
        self.render_channel_at(project, state, out, default_tz, now_ms() as f64)
    }

    /// The channel as shown at `shown_ms` (Unix ms): what moves by the clock
    /// (a ticker, a timer) is drawn at that instant, so frames rendered ahead
    /// or in parallel still step evenly.
    pub fn render_channel_at(
        &mut self,
        project: &Project,
        state: &ChannelState,
        out: OutputFormat,
        default_tz: &str,
        shown_ms: f64,
    ) -> Frame {
        let frame = self.channel(project, None, state, out, default_tz, shown_ms);
        Arc::try_unwrap(frame).unwrap_or_else(|f| (*f).clone())
    }

    /// The channel as shown at `shown_ms`, for the engine: when nothing can
    /// have changed since the frame `cache` kept, that frame again (the same
    /// `Arc`); else only what changed is drawn, on the kept picture of what
    /// did not (see `cache`). The same pixels as [`Self::render_channel_at`].
    pub fn render_channel_cached(
        &mut self,
        project: &Arc<Project>,
        state: &ChannelState,
        out: OutputFormat,
        default_tz: &str,
        shown_ms: f64,
        cache: &FrameCache,
    ) -> Arc<Frame> {
        self.channel(
            project,
            Some((project, cache)),
            state,
            out,
            default_tz,
            shown_ms,
        )
    }

    fn channel(
        &mut self,
        project: &Project,
        cache: Option<(&Arc<Project>, &FrameCache)>,
        state: &ChannelState,
        out: OutputFormat,
        default_tz: &str,
        shown_ms: f64,
    ) -> Arc<Frame> {
        self.refresh_fonts();
        self.scaled.frame();
        self.strips.frame();
        self.sprites.frame();
        self.pending = false;
        self.steps = Steps::default();
        // The picture drawn on: blank, or the one kept of the first steps.
        let mut pm = None;
        // The project layout (16:9, 9:16…) closest to this output's shape.
        let (format, dw, dh) = project.format_for(out.width, out.height);
        let sx = out.width as f32 / dw.max(1) as f32;
        let sy = out.height as f32 / dh.max(1) as f32;
        let scale = sx.min(sy);
        let root = Transform::from_translate(
            (out.width as f32 - dw as f32 * scale) / 2.0,
            (out.height as f32 - dh as f32 * scale) / 2.0,
        )
        .pre_scale(scale, scale);
        self.root_scale = scale;
        let fps = project.frame_rate.as_f64().max(1.0);
        let now = shown_ms.round() as i64;
        let portrait = dh > dw;
        // The graphics on air, lowest first, laid out for this format with
        // the settings the Operator was given (mask, font, logo…) when drawn.
        // A graphic replaced in a « Fade » zone comes just under the new one.
        let on_air: Vec<OnAir> = state
            .layers
            .iter()
            .flat_map(|(l, item)| {
                let going = item
                    .leaving
                    .as_deref()
                    .map(|g| (&g.item, g.opacity()))
                    .filter(|(_, a)| *a > 0.0);
                going
                    .into_iter()
                    .chain([(item, 1.0)])
                    .map(|(it, a)| (*l, it, a))
            })
            .filter_map(|(l, item, alpha)| {
                project.template(&item.template_id).map(|t| OnAir {
                    layer: l,
                    item,
                    alpha,
                    template: t,
                    set: Default::default(),
                    logic: Default::default(),
                    key: Default::default(),
                    data: cg_core::playout::shown_data(item),
                })
            })
            .collect();
        let leaving = on_air.iter().any(|a| a.alpha < 1.0);
        macro_rules! ctx {
            ($i:expr) => {{
                let a = &on_air[$i];
                let t = a.set.get_or_init(|| prepared(a.template, format, &a.data));
                let logic = match a.logic.get() {
                    Some(l) => l,
                    None => {
                        let l = self.logic_values(t, a.item.frame, &a.data);
                        a.logic.get_or_init(|| l)
                    }
                };
                Ctx {
                    template: t,
                    frame: a.item.frame,
                    data: &a.data,
                    time: TimeCtx {
                        now_ms: now,
                        age_secs: a.item.age / fps,
                        default_tz,
                        fps,
                    },
                    logic,
                    item: Some(a.item),
                    fps,
                    lang: &project.language,
                    item_index: $i,
                    item_key: *a.key.get_or_init(|| item_key(a.item)),
                }
            }};
        }
        fn fading(item: &cg_core::PlayoutItem) -> Option<(f32, &Data)> {
            item.fade
                .as_ref()
                .filter(|f| f.opacity() > 0.0)
                .map(|f| (f.opacity(), &f.data))
        }
        // Planned: what is the same as in the frames kept, and which
        // graphics are over the last one that moves (drawn apart, cache::top_item;
        // drawn whole too, so that frames are the same either way).
        let mut planned = None;
        // A graphic fading out: every frame drawn whole (the kept steps follow one graphic per layer).
        let kept_cache = cache.filter(|_| self.caching && !leaving);
        let mut top = None;
        let mut top_kept = None;
        if kept_cache.is_none() && on_air.len() > 1 {
            self.steps.plan = Some(Vec::new());
            for (i, a) in on_air.iter().enumerate() {
                self.steps.item = i;
                if fading(a.item).is_some() || a.alpha < 1.0 || !a.item.changes.is_empty() {
                    self.plan_step(cache::Motion::Moving);
                } else {
                    let ctx = ctx!(i);
                    let mut nowhere = Pixmap::new(1, 1).expect("valid size");
                    self.draw_children(&mut nowhere, &ctx, None, root, 1.0, None);
                }
            }
            let ops = self.steps.plan.take().unwrap_or_default();
            top = cache::top_item(&ops, on_air.len());
            self.steps = Steps::default();
        }
        if let Some((arc, kept)) = kept_cache {
            let mut scene = cache::Scene {
                project: arc.clone(),
                out,
                tz: default_tz.to_string(),
                fonts: self.font_generation,
                assets: self.assets_generation,
                items: on_air
                    .iter()
                    .map(|a| {
                        let mut item = a.item.clone();
                        item.age = 0.0;
                        item.held = 0.0;
                        (a.layer, item)
                    })
                    .collect(),
                ops: Vec::new(),
            };
            self.steps.plan = Some(Vec::new());
            for (i, a) in on_air.iter().enumerate() {
                self.steps.item = i;
                if fading(a.item).is_some() || !a.item.changes.is_empty() {
                    // A cross-fade, values moving: the graphic drawn whole, as one step.
                    self.plan_step(cache::Motion::Moving);
                } else if let Some(ops) = kept.still_steps(&scene, i) {
                    // The same graphic as in the frame before, nothing of it moving.
                    self.steps.plan.get_or_insert_with(Vec::new).extend(ops);
                } else {
                    let ctx = ctx!(i);
                    // Nothing is drawn while planning (only the widgets' inputs are read).
                    let mut nowhere = Pixmap::new(1, 1).expect("valid size");
                    self.draw_children(&mut nowhere, &ctx, None, root, 1.0, None);
                }
            }
            scene.ops = self.steps.plan.take().unwrap_or_default();
            match kept.plan(&scene) {
                cache::Plan::Same(frame) => {
                    self.steps = Steps::default();
                    return frame;
                }
                cache::Plan::Draw {
                    from,
                    picture,
                    keep,
                } => {
                    pm = picture.map(|p| pool::copy(&p));
                    self.steps.skip = from;
                    self.steps.keep = keep.map(|k| (k, None));
                    self.steps.ops = scene.ops.clone();
                }
            }
            top = cache::top_item(&scene.ops, on_air.len());
            top_kept = top.and_then(|t| kept.top(&scene, t));
            planned = Some((kept, scene));
        }
        let mut pm = pm.unwrap_or_else(|| pool::pixmap(out.width, out.height));
        // The graphics over what moves, drawn apart (when not kept).
        let mut over: Option<Pixmap> = None;
        // The step after the last one of each graphic.
        let mut item_end = vec![0; on_air.len()];
        for (i, op) in self.steps.ops.iter().enumerate() {
            item_end[op.item] = i + 1;
        }
        for i in 0..on_air.len() {
            // Every step of this graphic is in the picture already.
            if item_end[i] > self.steps.at && item_end[i] <= self.steps.skip {
                self.steps.at = item_end[i];
                continue;
            }
            let apart = top.is_some_and(|t| i >= t);
            if apart && top_kept.is_some() {
                // In the kept picture of the graphics over what moves.
                self.steps.at = self.steps.at.max(item_end[i]);
                continue;
            }
            let pm: &mut Pixmap = if apart {
                over.get_or_insert_with(|| {
                    self.over
                        .take()
                        .filter(|p| (p.width(), p.height()) == (out.width, out.height))
                        .unwrap_or_else(|| pool::pixmap(out.width, out.height))
                })
            } else {
                &mut pm
            };
            let (layer, item) = (on_air[i].layer, on_air[i].item);
            let ctx = &ctx!(i);
            // Only what is inside the zone shows on air.
            let zone_mask = project
                .zone_at(layer)
                .map(|z| z.area(format, portrait))
                .filter(|a| !a.is_full())
                .and_then(|a| {
                    Rect::from_xywh(
                        (a.x * dw as f64) as f32,
                        (a.y * dh as f64) as f32,
                        (a.width * dw as f64) as f32,
                        (a.height * dh as f64) as f32,
                    )
                })
                .map(|r| build_mask(pm, None, r, root));
            let alpha = on_air[i].alpha;
            if alpha < 1.0 {
                // The graphic going in a « Fade » zone: drawn whole, then laid at its opacity.
                self.enter_step(pm);
                self.steps.whole += 1;
                self.steps.fading = true;
                let mut going = Pixmap::new(pm.width(), pm.height()).expect("valid size");
                self.draw_children(&mut going, ctx, None, root, 1.0, zone_mask.as_ref());
                pm.draw_pixmap(
                    0,
                    0,
                    going.as_ref(),
                    &PixmapPaint {
                        opacity: alpha,
                        ..Default::default()
                    },
                    Transform::identity(),
                    None,
                );
                self.steps.whole -= 1;
                self.steps.fading = false;
                continue;
            }
            // Values sliding or turning over (a data source changed them): the
            // graphic with the new values, the layers showing them moved in
            // their box, then those layers with the old values.
            let turning: Vec<&cg_core::playout::Change> = item
                .changes
                .iter()
                .filter(|c| {
                    matches!(
                        c.kind,
                        cg_core::ValueChange::Slide | cg_core::ValueChange::Flip
                    )
                })
                .collect();
            if !turning.is_empty() {
                let mut layers = HashMap::new();
                for l in &ctx.template.layers {
                    if matches!(l.kind, LayerKind::Group { .. }) {
                        continue;
                    }
                    let json = cg_core::playout::layer_json(l);
                    if let Some(c) = turning.iter().find(|c| {
                        if c.layer.is_empty() {
                            cg_core::playout::layer_shows(&json, &c.key)
                        } else {
                            c.layer == l.id
                        }
                    }) {
                        layers.insert(l.id.clone(), (c.kind, c.eased() as f32));
                    }
                }
                self.enter_step(pm);
                self.steps.whole += 1;
                self.steps.fading = true;
                self.steps.anim = Some(ValueAnim {
                    layers: layers.clone(),
                    old: false,
                });
                self.draw_children(pm, ctx, None, root, 1.0, zone_mask.as_ref());
                let mut old_data = ctx.data.clone();
                for c in &turning {
                    old_data.insert(c.key.clone(), c.from.clone());
                }
                let t_old = prepared(on_air[i].template, format, &old_data);
                let logic = self.logic_values(&t_old, item.frame, &old_data);
                let old_ctx = Ctx {
                    template: &t_old,
                    data: &old_data,
                    logic: &logic,
                    ..ctx.at_frame(ctx.frame)
                };
                self.steps.anim = Some(ValueAnim { layers, old: true });
                self.draw_children(pm, &old_ctx, None, root, 1.0, zone_mask.as_ref());
                self.steps.anim = None;
                self.steps.whole -= 1;
                self.steps.fading = false;
                continue;
            }
            match fading(item) {
                None => self.draw_children(pm, ctx, None, root, 1.0, zone_mask.as_ref()),
                // A cross-fade (Update, data set row): the graphic with
                // the old values and with the new ones, each drawn
                // whole, then mixed (old × k + new × (1 − k)): what both
                // cover stays opaque, the rest fades in or out.
                Some((k, old_data)) => {
                    self.enter_step(pm);
                    self.steps.whole += 1;
                    self.steps.fading = true;
                    let (w, h) = (pm.width(), pm.height());
                    let mut new = Pixmap::new(w, h).expect("valid size");
                    self.draw_children(&mut new, ctx, None, root, 1.0, zone_mask.as_ref());
                    let t_old = prepared(on_air[i].template, format, old_data);
                    let logic = self.logic_values(&t_old, item.frame, old_data);
                    let old_ctx = Ctx {
                        template: &t_old,
                        data: old_data,
                        logic: &logic,
                        ..ctx.at_frame(ctx.frame)
                    };
                    let mut old = Pixmap::new(w, h).expect("valid size");
                    self.draw_children(&mut old, &old_ctx, None, root, 1.0, zone_mask.as_ref());
                    let mut mix = Pixmap::new(w, h).expect("valid size");
                    let paint = |opacity: f32, blend_mode| PixmapPaint {
                        opacity,
                        blend_mode,
                        ..Default::default()
                    };
                    let id = Transform::identity();
                    mix.draw_pixmap(
                        0,
                        0,
                        new.as_ref(),
                        &paint(1.0 - k, tiny_skia::BlendMode::SourceOver),
                        id,
                        None,
                    );
                    mix.draw_pixmap(
                        0,
                        0,
                        old.as_ref(),
                        &paint(k, tiny_skia::BlendMode::Plus),
                        id,
                        None,
                    );
                    pm.draw_pixmap(
                        0,
                        0,
                        mix.as_ref(),
                        &paint(1.0, tiny_skia::BlendMode::SourceOver),
                        id,
                        None,
                    );
                    self.steps.whole -= 1;
                    self.steps.fading = false;
                }
            }
        }
        // The graphics over what moves, laid over the frame.
        let lay = |pm: &mut Pixmap, tiles: &scaled::Tiles| {
            for (x, y, t) in tiles {
                scaled::composite(pm, t, *x, *y, 1.0, None);
            }
        };
        if let Some(tiles) = &top_kept {
            lay(&mut pm, tiles);
        } else if let Some(mut apart) = over {
            // In pieces (what is not transparent), `apart` left cleared.
            let tiles = scaled::tiles(&mut apart);
            lay(&mut pm, &tiles);
            if let (Some((kept, scene)), Some(t), false) = (&planned, top, self.pending) {
                kept.keep_top(scene, t, Arc::new(tiles));
            }
            self.over = Some(apart);
        }
        let keep = self.steps.keep.take();
        self.steps = Steps::default();
        let frame = Arc::new(Frame {
            width: out.width,
            height: out.height,
            data: pm.take(),
            captured_at: SystemTime::now(),
        });
        if let Some((kept, scene)) = planned {
            if let Some((at, Some(picture))) = keep {
                kept.keep_start(&scene, at, picture);
            }
            if scene.still() && !self.pending {
                kept.keep_still(&scene, frame.clone());
            }
        }
        frame
    }

    /// Renders a single template at `frame` (used for thumbnails / previews).
    /// Renders a template in one of the project formats ("main", "vertical"…).
    pub fn render_template(
        &mut self,
        project: &Project,
        template: &Template,
        format: &str,
        frame: f64,
        data: &Data,
    ) -> Frame {
        self.render_template_scaled(project, template, format, frame, data, u32::MAX)
    }

    /// The same, scaled down to at most `max_width` pixels wide (thumbnails:
    /// a small picture is drawn small, not drawn whole then shrunk).
    pub fn render_template_scaled(
        &mut self,
        project: &Project,
        template: &Template,
        format: &str,
        frame: f64,
        data: &Data,
        max_width: u32,
    ) -> Frame {
        self.refresh_fonts();
        self.scaled.frame();
        self.sprites.frame();
        let (dw, dh) = project
            .formats
            .iter()
            .find(|f| f.id == format)
            .map(|f| (f.width, f.height))
            .unwrap_or((project.width, project.height));
        let (dw, dh) = (dw.max(1), dh.max(1));
        let k = (max_width.max(1) as f64 / dw as f64).min(1.0);
        let (width, height) = if k < 1.0 {
            (
                ((dw as f64 * k).round() as u32).max(1),
                ((dh as f64 * k).round() as u32).max(1),
            )
        } else {
            (dw, dh)
        };
        let root = Transform::from_scale(width as f32 / dw as f32, height as f32 / dh as f32);
        self.root_scale = width as f32 / dw as f32;
        let template = template.in_format(format);
        let template = &*template.with_settings(data);
        let mut pm = Pixmap::new(width, height).expect("valid size");
        let time = TimeCtx {
            now_ms: now_ms(),
            age_secs: frame / project.frame_rate.as_f64().max(1.0),
            default_tz: "",
            fps: project.frame_rate.as_f64().max(1.0),
        };
        let logic = self.logic_values(template, frame, data);
        let ctx = Ctx {
            template,
            frame,
            data,
            time,
            logic: &logic,
            item: None,
            fps: project.frame_rate.as_f64().max(1.0),
            lang: &project.language,
            item_index: 0,
            item_key: {
                let mut k = sprite::Key::new();
                k.add(&template.id).add(data);
                k.finish()
            },
        };
        // As on air: only what is inside the template's zone.
        let zone_mask = project
            .zone(&template.zone)
            .map(|z| z.area(format, dh > dw))
            .filter(|a| !a.is_full())
            .and_then(|a| {
                Rect::from_xywh(
                    (a.x * dw as f64) as f32,
                    (a.y * dh as f64) as f32,
                    (a.width * dw as f64) as f32,
                    (a.height * dh as f64) as f32,
                )
            })
            .map(|r| build_mask(&pm, None, r, root));
        self.draw_children(&mut pm, &ctx, None, root, 1.0, zone_mask.as_ref());
        Frame {
            width,
            height,
            data: pm.take(),
            captured_at: SystemTime::now(),
        }
    }

    /// Planning: a step of the frame, with how it moves; its index.
    fn plan_step(&mut self, motion: cache::Motion) -> usize {
        let motion = if self.steps.clocked > 0 {
            cache::Motion::Moving
        } else {
            motion
        };
        let item = self.steps.item;
        let plan = self.steps.plan.get_or_insert_with(Vec::new);
        plan.push(cache::Op {
            motion,
            span: 1,
            item,
        });
        plan.len() - 1
    }

    /// Drawing: a step begins (a layer, before its children).
    fn enter_step(&mut self, pm: &Pixmap) -> Enter {
        let s = &mut self.steps;
        if s.whole > 0 {
            return Enter::Draw;
        }
        let at = s.at;
        if at < s.skip {
            let span = s.ops.get(at).map_or(1, |o| o.span);
            if at + span <= s.skip {
                s.at += span;
                return Enter::Done;
            }
            s.at += 1;
            return Enter::Children;
        }
        if let Some((k, slot)) = &mut s.keep {
            if *k == at && slot.is_none() && !self.pending {
                *slot = Some(pm.clone());
            }
        }
        s.at += 1;
        Enter::Draw
    }

    fn draw_children(
        &mut self,
        pm: &mut Pixmap,
        ctx: &Ctx,
        parent: Option<&str>,
        parent_tf: Transform,
        parent_opacity: f32,
        mask: Option<&Mask>,
    ) {
        let planning = self.steps.plan.is_some();
        // The layers of a Lottie group the operator controls follow its clock.
        let remapped;
        let mut clocked = false;
        let ctx = match (parent, ctx.item) {
            (Some(id), Some(item)) => {
                let group = ctx.template.layers.iter().find(|l| l.id == id);
                clocked = group.is_some_and(|g| {
                    matches!(
                        g.kind,
                        LayerKind::Group {
                            lottie: Some(_),
                            ..
                        }
                    ) && (item.clocks.contains_key(&g.id) || g.manual_start)
                });
                let f = group.map_or(ctx.frame, |g| {
                    item.lottie_frame(g, ctx.frame, ctx.template.hold_frame())
                });
                if f == ctx.frame {
                    ctx
                } else {
                    remapped = ctx.at_frame(f);
                    &remapped
                }
            }
            _ => ctx,
        };
        // Its own clock moves (the operator's age): planned as moving.
        if clocked {
            self.steps.clocked += 1;
        }
        let children: Vec<_> = ctx
            .template
            .layers
            .iter()
            .filter(|l| l.shown_at(ctx.frame) && l.parent.as_deref() == parent)
            .collect();
        for layer in children {
            let mut r = resolve_layer(layer, ctx.frame, ctx.data, ctx.template.hold_frame());
            // Styles typed by the operator too: the text without their tags.
            if let Some(runs) = cg_core::rich::layer_runs(ctx.template, layer, ctx.data) {
                r.text = Some(runs.into_iter().map(|r| r.text).collect());
            }
            if let Some(v) = ctx.logic.get(&layer.id) {
                r.values = v.clone();
            }
            // Its repeated movement, on its time on air.
            if let Some(m) = &layer.motion {
                let age = ctx.time_for(layer).age_secs;
                cg_core::anim::apply_motion(&mut r.values, m, ctx.frame, ctx.fps, age);
            }
            let opacity = parent_opacity * r.num(Prop::Opacity).clamp(0.0, 1.0) as f32;
            if opacity <= 0.0 {
                continue;
            }
            let (w, h) = (r.num(Prop::Width) as f32, r.num(Prop::Height) as f32);
            let tf = parent_tf.pre_concat(local_transform(&r, w, h));
            // A layer showing a value sliding or turning over: moved in its
            // box, cut by it; the old values' pass draws only these.
            let anim_mask;
            let (tf, mask) = match self
                .steps
                .anim
                .as_ref()
                .map(|a| (a.old, a.layers.get(&layer.id).copied()))
            {
                Some((old, Some((kind, e)))) => {
                    let Some(moved) = value_tf(tf, kind, e, old, h) else {
                        continue;
                    };
                    let Some(rect) = Rect::from_xywh(0.0, 0.0, w.max(0.01), h.max(0.01)) else {
                        continue;
                    };
                    anim_mask = build_mask(pm, mask, rect, tf);
                    (moved, Some(&anim_mask))
                }
                Some((true, None)) if !matches!(layer.kind, LayerKind::Group { .. }) => continue,
                _ => (tf, mask),
            };

            // Crop (reveal mask) in layer space, intersected with the parent mask.
            let crop = crop_rect(&r, w, h);
            let clip_children = matches!(layer.kind, LayerKind::Group { clip: true, .. });
            let own_rect = if crop.is_some() || clip_children {
                match crop.unwrap_or_else(|| Rect::from_xywh(0.0, 0.0, w.max(0.0), h.max(0.0))) {
                    Some(rect) => Some(rect),
                    None => continue, // fully cropped
                }
            } else {
                None
            };
            let is_group = matches!(layer.kind, LayerKind::Group { .. });
            let shadowed = layer.drop_shadow && !matches!(layer.kind, LayerKind::Text { .. });

            if planning {
                if shadowed {
                    // The layer and its children are one step (drawn together).
                    let outer = self.steps.plan.replace(Vec::new());
                    let (own, then) = self.layer_motion(pm, ctx, &r, tf, opacity);
                    let own = then.map_or(own, |t| own.and(t));
                    if is_group {
                        self.draw_children(pm, ctx, Some(&layer.id), tf, 1.0, None);
                    }
                    let inner = std::mem::replace(&mut self.steps.plan, outer).unwrap_or_default();
                    let m = inner.iter().fold(own, |m, o| m.and(o.motion));
                    self.plan_step(m);
                    continue;
                }
                let (m, then) = self.layer_motion(pm, ctx, &r, tf, opacity);
                let i = self.plan_step(m);
                if let Some(then) = then {
                    self.plan_step(then);
                    if let Some(plan) = self.steps.plan.as_mut() {
                        plan[i].span = 2;
                    }
                }
                if is_group {
                    self.draw_children(pm, ctx, Some(&layer.id), tf, opacity, None);
                    if let Some(plan) = self.steps.plan.as_mut() {
                        plan[i].span = plan.len() - i;
                    }
                }
                continue;
            }

            let own_drawn = match self.enter_step(pm) {
                Enter::Done => continue,
                Enter::Children => true,
                Enter::Draw => false,
            };
            // A layer drawn in two steps (planned so): its second one may be to draw.
            let split = !is_group
                && self.steps.whole == 0
                && self
                    .steps
                    .ops
                    .get(self.steps.at.wrapping_sub(1))
                    .is_some_and(|o| o.span == 2);
            // Its picture does not change with time: laid from its sprite
            // (drawn once), when nothing of it changed.
            // (its own motion, that of its second step when drawn in two)
            let sprite = (self.sprites_on
                && !own_drawn
                && !self.in_sprite
                && !self.steps.fading
                && self.steps.clocked == 0)
                .then(|| self.step_motion(pm, ctx, &r, tf, opacity, shadowed && is_group, split))
                .filter(|(m, _)| *m != cache::Motion::Moving);
            let own_mask = own_rect.map(|rect| build_mask(pm, mask, rect, tf));
            let m = own_mask.as_ref().or(mask);

            // A drop shadow (not for a Text: it has its own): the layer and its children drawn
            // alone, their shadow under them, all at the layer's opacity, cut by its crop.
            if shadowed {
                // A group's own clip cuts its children, not its shadow (CSS overflow vs filter).
                let cut = if crop.is_some() { m } else { mask };
                self.steps.whole += 1;
                let shadow = clip_children.then_some((w, h));
                let laid = sprite.is_some_and(|(m, _)| {
                    self.lay_sprite(pm, ctx, &r, tf, opacity, cut, m, Some(shadow), false)
                });
                if !laid {
                    self.draw_shadowed(pm, ctx, &r, tf, opacity, cut, shadow, true);
                }
                self.steps.whole -= 1;
                continue;
            }
            // Whole from its sprite; or, drawn in two with a second step that
            // moves (a map's pulsing markers), the first from its sprite and
            // the second over it.
            let whole = sprite.filter(|(_, m2)| m2.is_none_or(|m| m != cache::Motion::Moving));
            let base = sprite.filter(|(_, m2)| m2.is_some() && whole.is_none());
            if whole.is_some_and(|(m1, m2)| {
                let mo = m2.map_or(m1, |m| m1.and(m));
                self.lay_sprite(pm, ctx, &r, tf, opacity, m, mo, None, false)
            }) {
                if split {
                    // Both steps of the widget are laid: the second counted too.
                    self.steps.at += 1;
                    if let Some((k, slot)) = &mut self.steps.keep {
                        if *k < self.steps.at && slot.is_none() {
                            self.steps.keep = None;
                        }
                    }
                }
            } else if !own_drawn || split {
                let under = base.is_some_and(|(m1, _)| {
                    self.lay_sprite(pm, ctx, &r, tf, opacity, m, m1, None, true)
                });
                self.steps.under = own_drawn || under;
                self.steps.split = split;
                self.draw_layer(pm, ctx, &r, tf, opacity, m);
                self.steps.under = false;
                if std::mem::take(&mut self.steps.split) {
                    // The widget did not get to its second step: the steps go on
                    // counted, and no picture is kept (it would not be the one
                    // between them).
                    self.steps.at += 1;
                    if self
                        .steps
                        .keep
                        .as_ref()
                        .is_some_and(|(k, _)| *k < self.steps.at)
                    {
                        if let Some((_, slot)) = &mut self.steps.keep {
                            if slot.is_none() {
                                self.steps.keep = None;
                            }
                        }
                    }
                }
            }
            if is_group {
                // Group crop always applies to children; plain clip only when requested.
                let child_mask = if clip_children || crop.is_some() {
                    m
                } else {
                    mask
                };
                self.draw_children(pm, ctx, Some(&layer.id), tf, opacity, child_mask);
            }
        }
        if clocked {
            self.steps.clocked -= 1;
        }
    }

    /// How a layer's own picture (not its children's) depends on time; a
    /// layer drawn in two steps (`sub_step`): how each does.
    fn layer_motion(
        &mut self,
        pm: &mut Pixmap,
        ctx: &Ctx,
        r: &ResolvedLayer,
        tf: Transform,
        opacity: f32,
    ) -> (cache::Motion, Option<cache::Motion>) {
        use cache::Motion;
        // It (or a group it is in) repeats a movement: drawn again on each frame.
        if loops(ctx.template, r.layer) {
            return (Motion::Moving, None);
        }
        if cg_core::wid::widget_id(&r.layer.kind).is_some() {
            if let Some((a, b)) = self.widget(pm, ctx, r, tf, opacity, None, wid::split_motions) {
                return (a, Some(b));
            }
            let motion = self.widget(pm, ctx, r, tf, opacity, None, wid::motion);
            if let Some(m) = motion {
                return (m, None);
            }
        }
        // The generic drawing (lib.rs), when the widget code leaves it the layer.
        let scrolling = r.layer.kind.scrolling_as();
        let m = match scrolling.as_ref().unwrap_or(&r.layer.kind) {
            LayerKind::Group { .. }
            | LayerKind::Rect { .. }
            | LayerKind::Ellipse { .. }
            | LayerKind::Text { .. }
            | LayerKind::Image { .. }
            | LayerKind::QrCode { .. }
            | LayerKind::Polygon { .. }
            | LayerKind::Pen { .. }
            | LayerKind::Table { .. }
            | LayerKind::Credits { .. }
            | LayerKind::Chart { .. }
            | LayerKind::Map { .. }
            | LayerKind::Scoreboard { .. }
            | LayerKind::SocialWall { .. }
            | LayerKind::SocialBadge { .. }
            | LayerKind::Web { .. } => Motion::Still,
            // Clocks, tickers, rolls, counters… (by the time on air).
            _ => Motion::Moving,
        };
        (m, None)
    }

    /// Drawing: how the step of `r` (its own picture; with its children when
    /// `subtree`; both steps of a widget drawn in two when `split`) depends on
    /// time: from the plan when the frame has one, else asked now.
    fn step_motion(
        &mut self,
        pm: &mut Pixmap,
        ctx: &Ctx,
        r: &ResolvedLayer,
        tf: Transform,
        opacity: f32,
        subtree: bool,
        split: bool,
    ) -> (cache::Motion, Option<cache::Motion>) {
        use cache::Motion;
        if self.steps.whole == 0 && !self.steps.ops.is_empty() {
            let at = self.steps.at.wrapping_sub(1);
            return match self.steps.ops.get(at) {
                Some(op) => (
                    op.motion,
                    split.then(|| {
                        self.steps
                            .ops
                            .get(at + 1)
                            .map_or(Motion::Moving, |o| o.motion)
                    }),
                ),
                None => (Motion::Moving, None),
            };
        }
        let outer = self.steps.plan.replace(Vec::new());
        let (own, then) = self.layer_motion(pm, ctx, r, tf, opacity);
        if subtree {
            self.draw_children(pm, ctx, Some(&r.layer.id), tf, 1.0, None);
        }
        let inner = std::mem::replace(&mut self.steps.plan, outer).unwrap_or_default();
        (inner.iter().fold(own, |m, o| m.and(o.motion)), then)
    }

    /// Drawing a layer in two steps for its sprite: its first step only.
    pub(crate) fn base_only(&self) -> bool {
        self.steps.base_only
    }

    /// What the step of `r` is drawn from, hashed (`sprite`): the graphic's
    /// state, the layer's values but its placement, its text, the fonts and
    /// pictures, the sprite's transform; with its children when `subtree`.
    /// With it, the box the drawing is expected in, in layer space.
    fn sprite_key(
        &mut self,
        ctx: &Ctx,
        r: &ResolvedLayer,
        s: Transform,
        moving: bool,
        motion: cache::Motion,
        subtree: bool,
    ) -> (u64, Rect) {
        const PLACE: [Prop; 8] = [
            Prop::X,
            Prop::Y,
            Prop::AnchorX,
            Prop::AnchorY,
            Prop::Rotation,
            Prop::ScaleX,
            Prop::ScaleY,
            Prop::Opacity,
        ];
        fn value(k: &mut sprite::Key, v: &Value) {
            match v {
                Value::Number(n) => {
                    k.f64(*n);
                }
                Value::Color(c) => {
                    for x in c.0 {
                        k.f32(x);
                    }
                }
            }
        }
        fn frame_sensitive(kind: &LayerKind) -> bool {
            matches!(
                kind,
                LayerKind::Pen { .. }
                    | LayerKind::Typewriter { .. }
                    | LayerKind::Credits { .. }
                    | LayerKind::Roll { .. }
                    | LayerKind::Ticker { .. }
                    | LayerKind::Rotator { .. }
                    | LayerKind::Slideshow { .. }
                    | LayerKind::Group {
                        lottie: Some(_),
                        ..
                    }
            )
        }
        let mut k = sprite::Key::new();
        k.add(&ctx.item_key)
            .add(&ctx.template.id)
            .add(&ctx.template.revision)
            .add(&self.sprites.def(r.layer));
        for (p, v) in &r.values {
            if !PLACE.contains(p) {
                k.add(p);
                value(&mut k, v);
            }
        }
        k.add(&r.text).add(&r.image_src);
        // The pictures of a Text (their characters stay the same).
        for i in cg_core::rich::layer_images(ctx.template, r.layer, ctx.data) {
            k.add(&i.src);
        }
        match motion {
            cache::Motion::Still => k.add(&0u8),
            cache::Motion::Key(x) => k.add(&x),
            cache::Motion::Moving => k.add(&1u8),
        };
        k.f32(s.sx)
            .f32(s.kx)
            .f32(s.ky)
            .f32(s.sy)
            .f32(s.tx)
            .f32(s.ty);
        k.add(&moving)
            .add(&self.font_generation)
            .add(&self.assets_generation)
            .add(ctx.lang)
            .add(ctx.time.default_tz);
        if frame_sensitive(&r.layer.kind) {
            k.f64(ctx.frame);
        }
        let (w, h) = (r.num(Prop::Width) as f32, r.num(Prop::Height) as f32);
        let mut bounds = Rect::from_xywh(0.0, 0.0, w.max(1.0), h.max(1.0))
            .unwrap_or_else(|| Rect::from_xywh(0.0, 0.0, 1.0, 1.0).expect("a rect"));
        if subtree {
            // The children: everything of them (their placement moves inside).
            let hold = ctx.template.hold_frame();
            let mut stack = vec![(r.layer.id.as_str(), Transform::identity())];
            while let Some((parent, ptf)) = stack.pop() {
                for l in ctx
                    .template
                    .layers
                    .iter()
                    .filter(|l| l.parent.as_deref() == Some(parent))
                {
                    k.add(&self.sprites.def(l));
                    if !l.shown_at(ctx.frame) {
                        continue;
                    }
                    let mut c = resolve_layer(l, ctx.frame, ctx.data, hold);
                    if let Some(runs) = cg_core::rich::layer_runs(ctx.template, l, ctx.data) {
                        c.text = Some(runs.into_iter().map(|r| r.text).collect());
                    }
                    if let Some(v) = ctx.logic.get(&l.id) {
                        c.values = v.clone();
                    }
                    for (p, v) in &c.values {
                        k.add(p);
                        value(&mut k, v);
                    }
                    k.add(&c.text).add(&c.image_src);
                    for i in cg_core::rich::layer_images(ctx.template, l, ctx.data) {
                        k.add(&i.src);
                    }
                    if frame_sensitive(&l.kind) {
                        k.f64(ctx.frame);
                    }
                    let (cw, ch) = (c.num(Prop::Width) as f32, c.num(Prop::Height) as f32);
                    let ctf = ptf.pre_concat(local_transform(&c, cw, ch));
                    if let Some(b) = Rect::from_xywh(0.0, 0.0, cw.max(1.0), ch.max(1.0))
                        .and_then(|b| PathBuilder::from_rect(b).transform(ctf))
                        .map(|p| p.bounds())
                    {
                        bounds = Rect::from_ltrb(
                            bounds.left().min(b.left()),
                            bounds.top().min(b.top()),
                            bounds.right().max(b.right()),
                            bounds.bottom().max(b.bottom()),
                        )
                        .unwrap_or(bounds);
                    }
                    stack.push((l.id.as_str(), ctf));
                }
            }
        }
        (k.finish(), bounds)
    }

    /// Lays the step of `r` from its sprite, made now when none fits (the
    /// layer alone, or with its children under their drop shadow when
    /// `shadow` is given). False when the layer is drawn straight instead
    /// (it draws out of the sprite's margin, or is too big).
    #[allow(clippy::too_many_arguments)]
    fn lay_sprite(
        &mut self,
        pm: &mut Pixmap,
        ctx: &Ctx,
        r: &ResolvedLayer,
        tf: Transform,
        opacity: f32,
        mask: Option<&Mask>,
        motion: cache::Motion,
        shadow: Option<Option<(f32, f32)>>,
        base_only: bool,
    ) -> bool {
        let moving = geometry_moving(ctx.template, r.layer, ctx.frame);
        // The sprite's space: device pixels less the whole-pixel part of the
        // position; while the geometry moves, the output scale only (one
        // sprite for the whole move, laid through the transform).
        let s = if moving {
            let (sx, sy) = tf.get_scale();
            Transform::from_scale(sx.max(1e-3), sy.max(1e-3))
        } else {
            Transform {
                tx: tf.tx - tf.tx.floor(),
                ty: tf.ty - tf.ty.floor(),
                ..tf
            }
        };
        let (key, bounds) = self.sprite_key(ctx, r, s, moving, motion, shadow.is_some());
        let lay = |pm: &mut Pixmap, spm: &Pixmap, x: i32, y: i32| {
            // Sprite pixels → device: through the layer's transform, from the sprite's.
            let from = s.post_translate(-x as f32, -y as f32);
            let Some(back) = from.invert() else {
                return;
            };
            let c = tf.pre_concat(back);
            if c.is_translate() {
                let (ix, iy) = (c.tx.round(), c.ty.round());
                if (c.tx - ix).abs() < 1e-3 && (c.ty - iy).abs() < 1e-3 {
                    scaled::composite(pm, spm, ix as i32, iy as i32, opacity, mask);
                } else {
                    let (fx, fy) = (c.tx.floor(), c.ty.floor());
                    scaled::composite_bilinear(
                        pm,
                        spm,
                        fx as i32,
                        fy as i32,
                        c.tx - fx,
                        c.ty - fy,
                        opacity,
                        mask,
                    );
                }
            } else {
                let paint = PixmapPaint {
                    opacity,
                    quality: tiny_skia::FilterQuality::Bilinear,
                    ..PixmapPaint::default()
                };
                pm.draw_pixmap(0, 0, spm.as_ref(), &paint, c, mask);
            }
        };
        // The box the drawing is expected in, in sprite space, with a margin
        // for strokes and shadows (anything beyond: drawn straight).
        let scale = {
            let (sx, sy) = s.get_scale();
            sx.abs().max(sy.abs()).max(1e-3)
        };
        let mut pad = 8.0 + r.num(Prop::StrokeWidth).abs() as f32;
        if r.layer.drop_shadow || matches!(r.layer.kind, LayerKind::Text { .. }) {
            pad += r.num(Prop::ShadowX).abs() as f32
                + r.num(Prop::ShadowY).abs() as f32
                + r.num(Prop::ShadowBlur).abs() as f32 * 1.5
                + 2.0;
        }
        let Some(b) = PathBuilder::from_rect(bounds)
            .transform(s)
            .map(|p| p.bounds())
        else {
            return false;
        };
        let pad = pad * scale;
        let (mut x0, mut y0) = (
            (b.left() - pad).floor() as i32,
            (b.top() - pad).floor() as i32,
        );
        let (mut x1, mut y1) = (
            (b.right() + pad).ceil() as i32,
            (b.bottom() + pad).ceil() as i32,
        );
        let (fw, fh) = (pm.width() as i64, pm.height() as i64);
        let too_big = |x0: i32, y0: i32, x1: i32, y1: i32| {
            let (sw, sh) = ((x1 - x0).max(1) as i64, (y1 - y0).max(1) as i64);
            sw > 8192 || sh > 8192 || sw * sh > 2 * fw * fh
        };
        // A big layer at rest (a vignette larger than the frame): only what
        // the frame shows of it, which is where it stands.
        let mut sides = [true; 4];
        let mut key = key;
        if too_big(x0, y0, x1, y1) && !moving {
            let (ox, oy) = (tf.tx.floor() as i32, tf.ty.floor() as i32);
            let (vx0, vy0) = (-ox - 2, -oy - 2);
            let (vx1, vy1) = (vx0 + fw as i32 + 4, vy0 + fh as i32 + 4);
            if x0 < vx0 {
                x0 = vx0;
                sides[0] = false;
            }
            if y0 < vy0 {
                y0 = vy0;
                sides[1] = false;
            }
            if x1 > vx1 {
                x1 = vx1;
                sides[2] = false;
            }
            if y1 > vy1 {
                y1 = vy1;
                sides[3] = false;
            }
            let mut k = sprite::Key::new();
            k.add(&key).add(&ox).add(&oy).add(&fw).add(&fh);
            key = k.finish();
        }
        if base_only {
            let mut k = sprite::Key::new();
            k.add(&key).add(&true);
            key = k.finish();
        }
        if let Some((spm, x, y)) = self.sprites.get(ctx.item_index, &r.layer.id, key) {
            let Some(spm) = spm else {
                return false;
            };
            lay(pm, &spm, x, y);
            return true;
        }
        if too_big(x0, y0, x1, y1) || x1 <= x0 || y1 <= y0 {
            return false;
        }
        let (sw, sh) = ((x1 - x0) as u64, (y1 - y0) as u64);
        let mut spm = pool::pixmap(sw as u32, sh as u32);
        let s_in = s.post_translate(-x0 as f32, -y0 as f32);
        let pending = self.pending;
        self.in_sprite = true;
        self.steps.whole += 1;
        self.steps.base_only = base_only;
        let (under, split) = (
            std::mem::take(&mut self.steps.under),
            std::mem::take(&mut self.steps.split),
        );
        match shadow {
            Some(clip) => self.draw_shadowed(&mut spm, ctx, r, s_in, 1.0, None, clip, false),
            None => self.draw_layer(&mut spm, ctx, r, s_in, 1.0, None),
        }
        self.steps.under = under;
        self.steps.split = split;
        self.steps.whole -= 1;
        self.steps.base_only = false;
        self.in_sprite = false;
        if sprite::touches_edge(&spm, sides) {
            // Drawn beyond the margin: straight, and remembered so.
            if self.caching && self.pending == pending {
                self.sprites
                    .put(ctx.item_index, &r.layer.id, key, None, x0, y0);
            }
            return false;
        }
        lay(pm, &spm, x0, y0);
        if self.caching && self.pending == pending {
            self.sprites
                .put(ctx.item_index, &r.layer.id, key, Some(spm), x0, y0);
        }
        true
    }

    /// Drawing: the second step of a layer drawn in two (the map's markers
    /// over its base map), begun. Called by the widget between the two.
    pub(crate) fn sub_step(&mut self, pm: &Pixmap) {
        let s = &mut self.steps;
        if s.whole > 0 || s.plan.is_some() || !s.split {
            return;
        }
        s.split = false;
        if let Some((k, slot)) = &mut s.keep {
            if *k == s.at && slot.is_none() && !self.pending {
                *slot = Some(pm.clone());
            }
        }
        s.at += 1;
    }

    /// A layer with its drop shadow (`Layer::drop_shadow`), as the HTML5 renderer's CSS
    /// `drop-shadow()` on the layer's box: offset and blur in the layer's space.
    fn draw_shadowed(
        &mut self,
        pm: &mut Pixmap,
        ctx: &Ctx,
        r: &ResolvedLayer,
        tf: Transform,
        opacity: f32,
        mask: Option<&Mask>,
        clip: Option<(f32, f32)>,
        scratch: bool,
    ) {
        // A frame-sized picture, cleared (kept between shadows: no 8 MB
        // allocation each time).
        let mut content = match self.scratch.take().filter(|_| scratch) {
            Some(mut c) if (c.width(), c.height()) == (pm.width(), pm.height()) => {
                c.fill(tiny_skia::Color::TRANSPARENT);
                c
            }
            _ => match Pixmap::new(pm.width(), pm.height()) {
                Some(c) => c,
                None => return,
            },
        };
        self.draw_layer(&mut content, ctx, r, tf, 1.0, None);
        if matches!(r.layer.kind, LayerKind::Group { .. }) {
            let clip = clip
                .and_then(|(w, h)| Rect::from_xywh(0.0, 0.0, w.max(0.0), h.max(0.0)))
                .map(|rc| build_mask(&content, None, rc, tf));
            self.draw_children(&mut content, ctx, Some(&r.layer.id), tf, 1.0, clip.as_ref());
        }
        let (ox, oy) = (r.num(Prop::ShadowX) as f32, r.num(Prop::ShadowY) as f32);
        let (dx, dy) = (tf.sx * ox + tf.kx * oy, tf.ky * ox + tf.sy * oy);
        let scale = (tf.sx * tf.sy - tf.kx * tf.ky).abs().sqrt();
        let sigma = r.num(Prop::ShadowBlur).max(0.0) as f32 * scale / 2.0;
        let composed = shadow::compose(&content, r.color(Prop::ShadowColor), dx, dy, sigma);
        if self.caching && scratch {
            self.scratch = Some(content);
        }
        let Some((out, x, y)) = composed else {
            return;
        };
        let paint = PixmapPaint {
            opacity,
            ..PixmapPaint::default()
        };
        pm.draw_pixmap(x, y, out.as_ref(), &paint, Transform::identity(), mask);
    }

    /// Calls `f` (a widget's drawing or motion, src/wid) with the layer's inputs.
    fn widget<R>(
        &mut self,
        pm: &mut Pixmap,
        ctx: &Ctx,
        r: &ResolvedLayer,
        tf: Transform,
        opacity: f32,
        mask: Option<&Mask>,
        f: fn(&mut wid::Draw) -> R,
    ) -> R {
        let (w, h) = (r.num(Prop::Width) as f32, r.num(Prop::Height) as f32);
        let time = ctx.time_for(r.layer);
        let null = serde_json::Value::Null;
        let state = ctx
            .item
            .and_then(|i| i.widgets.get(&r.layer.id))
            .unwrap_or(&null);
        let moving = !self.in_sprite && geometry_moving(ctx.template, r.layer, ctx.frame);
        let mut d = wid::Draw {
            rd: self,
            pm,
            r,
            layer: r.layer,
            tf,
            mask,
            opacity,
            w,
            h,
            time,
            data: ctx.data,
            frame: ctx.frame,
            hold: ctx.template.hold_frame(),
            fps: ctx.fps,
            state,
            lang: ctx.lang,
            runs: cg_core::rich::layer_runs(ctx.template, r.layer, ctx.data),
            inline: cg_core::rich::layer_images(ctx.template, r.layer, ctx.data),
            moving,
            follow: match &r.layer.kind {
                LayerKind::Progress { x: c, .. } => {
                    let none = Default::default();
                    cg_core::wid::progress::followed(
                        &ctx.template.layers,
                        c,
                        ctx.item.map_or(&none, |i| &i.widgets),
                        ctx.data,
                        time.now_ms,
                        time.age_secs,
                        time.default_tz,
                    )
                }
                _ => None,
            },
        };
        f(&mut d)
    }

    fn draw_layer(
        &mut self,
        pm: &mut Pixmap,
        ctx: &Ctx,
        r: &ResolvedLayer,
        tf: Transform,
        opacity: f32,
        mask: Option<&Mask>,
    ) {
        let (w, h) = (r.num(Prop::Width) as f32, r.num(Prop::Height) as f32);
        let time = ctx.time_for(r.layer);
        // The widgets of the spec draw themselves (src/wid).
        if cg_core::wid::widget_id(&r.layer.kind).is_some()
            && self.widget(pm, ctx, r, tf, opacity, mask, wid::draw)
        {
            return;
        }
        let fill = paint_for(r.color(Prop::Fill), opacity);
        let stroke_w = r.num(Prop::StrokeWidth) as f32;
        let stroke = paint_for(r.color(Prop::Stroke), opacity);
        // A scrolling text is drawn as the ticker or roll it behaves like.
        let scrolling = r.layer.kind.scrolling_as();
        let kind = scrolling.as_ref().unwrap_or(&r.layer.kind);
        // Widgets stay inside their box.
        let boxed;
        let mask = if matches!(
            kind,
            LayerKind::Clock { .. }
                | LayerKind::Counter { .. }
                | LayerKind::Typewriter { .. }
                | LayerKind::Rotator { .. }
                | LayerKind::BarChart { .. }
                | LayerKind::Slideshow { .. }
                | LayerKind::Ticker { .. }
                | LayerKind::Roll { .. }
        ) {
            boxed = Rect::from_xywh(0.0, 0.0, w.max(1.0), h.max(1.0))
                .map(|b| build_mask(pm, mask, b, tf));
            boxed.as_ref().or(mask)
        } else {
            mask
        };
        match kind {
            LayerKind::Group { .. } => {
                // Groups paint a background only if a visible fill is set explicitly.
                if r.layer.props.contains_key(&Prop::Fill)
                    || r.layer.tracks.contains_key(&Prop::Fill)
                {
                    if let Some(path) = rounded_rect(w, h, r.num(Prop::CornerRadius) as f32) {
                        pm.fill_path(&path, &fill, FillRule::Winding, tf, mask);
                    }
                }
            }
            LayerKind::Rect { .. } => {
                if let Some(path) = rounded_rect(w, h, r.num(Prop::CornerRadius) as f32) {
                    pm.fill_path(&path, &fill, FillRule::Winding, tf, mask);
                    if stroke_w > 0.0 {
                        pm.stroke_path(
                            &path,
                            &stroke,
                            &Stroke {
                                width: stroke_w,
                                ..Default::default()
                            },
                            tf,
                            mask,
                        );
                    }
                }
            }
            LayerKind::Ellipse { .. } => {
                if let Some(path) = Rect::from_xywh(0.0, 0.0, w, h).and_then(PathBuilder::from_oval)
                {
                    pm.fill_path(&path, &fill, FillRule::Winding, tf, mask);
                    if stroke_w > 0.0 {
                        pm.stroke_path(
                            &path,
                            &stroke,
                            &Stroke {
                                width: stroke_w,
                                ..Default::default()
                            },
                            tf,
                            mask,
                        );
                    }
                }
            }
            LayerKind::Text {
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
                ..
            } => {
                let Some(content) = r.text.as_deref().filter(|t| !t.is_empty()) else {
                    return;
                };
                let Some(font) = self.fonts.font(font_family, *font_weight) else {
                    tracing::warn!(font_family, "no font available");
                    return;
                };
                let style = text::TextStyle {
                    size: r.num(Prop::FontSize) as f32,
                    letter_spacing: r.num(Prop::LetterSpacing) as f32,
                    align: *align,
                    vertical_align: *vertical_align,
                    box_w: w,
                    box_h: h,
                    line_height: line_height.unwrap_or(text::LINE_HEIGHT),
                    decoration: *decoration,
                };
                // Too long for its box: narrower and smaller, "…", or cut.
                let fit = text::fit_text(
                    &font, content, &style, *wrap, *overflow, *squeeze, *min_size,
                );
                let style = text::TextStyle {
                    size: fit.size,
                    box_w: w / fit.squeeze,
                    ..style
                };
                let boxed;
                let mask = if fit.clip {
                    boxed = Rect::from_xywh(0.0, 0.0, w.max(1.0), h.max(1.0))
                        .map(|r| build_mask(pm, mask, r, tf));
                    boxed.as_ref().or(mask)
                } else {
                    mask
                };
                let tf = tf.pre_scale(fit.squeeze, 1.0);
                if let Some(path) = text::layout_text(&font, &fit.text, &style) {
                    pm.fill_path(&path, &fill, FillRule::Winding, tf, mask);
                    if stroke_w > 0.0 {
                        pm.stroke_path(
                            &path,
                            &stroke,
                            &Stroke {
                                width: stroke_w,
                                ..Default::default()
                            },
                            tf,
                            mask,
                        );
                    }
                }
            }
            LayerKind::Clock {
                x: _,
                mode,
                format,
                timezone,
                target,
                font,
            } => {
                let text = clock_text(*mode, format, timezone, target, ctx.data, &time);
                let text = if font.uppercase {
                    text.to_uppercase()
                } else {
                    text
                };
                self.draw_text(
                    pm,
                    r,
                    font,
                    &text,
                    (font.align, font.vertical_align),
                    (0.0, 0.0, w, h),
                    tf,
                    opacity,
                    mask,
                );
            }
            LayerKind::Ticker {
                x: _,
                text,
                separator,
                speed,
                font,
            } => {
                let items: Vec<String> = interpolate_text(text, ctx.data)
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(String::from)
                    .collect();
                if items.is_empty() {
                    return;
                }
                let mut strip = items.join(separator) + separator;
                if font.uppercase {
                    strip = strip.to_uppercase();
                }
                let Some(f) = self.fonts.font(&font.font_family, font.font_weight) else {
                    return;
                };
                let size = r.num(Prop::FontSize) as f32;
                let cycle = text::text_width(&f, &strip, size, r.num(Prop::LetterSpacing) as f32)
                    + r.num(Prop::LetterSpacing) as f32;
                let Some(clip) =
                    Rect::from_xywh(0.0, 0.0, w, h).map(|rc| build_mask(pm, mask, rc, tf))
                else {
                    return;
                };
                let speed = widgets::frame_speed(*speed, time.fps);
                for x in scroll_positions(w as f64, cycle as f64, speed * time.age_secs, true) {
                    let place = (TextAlign::Left, font.vertical_align);
                    self.draw_text(
                        pm,
                        r,
                        font,
                        &strip,
                        place,
                        (x as f32, 0.0, cycle, h),
                        tf,
                        opacity,
                        Some(&clip),
                    );
                }
            }
            LayerKind::Roll {
                text,
                speed,
                looping,
                font,
            } => {
                let mut body = interpolate_text(text, ctx.data);
                if font.uppercase {
                    body = body.to_uppercase();
                }
                let size = r.num(Prop::FontSize) as f32;
                let block = body.split('\n').count() as f32 * size * text::LINE_HEIGHT;
                // A screen-high gap before the roll comes round again.
                let cycle = if *looping { block + h } else { block };
                let Some(clip) =
                    Rect::from_xywh(0.0, 0.0, w, h).map(|rc| build_mask(pm, mask, rc, tf))
                else {
                    return;
                };
                let tops: Vec<f32> = scroll_positions(
                    h as f64,
                    cycle as f64,
                    widgets::frame_speed(*speed, time.fps) * time.age_secs,
                    *looping,
                )
                .into_iter()
                .map(|y| y as f32)
                .collect();
                // The block drawn once in a strip, the part in the box laid each frame.
                let key = {
                    use std::hash::{Hash, Hasher};
                    let mut k = std::collections::hash_map::DefaultHasher::new();
                    format!("{body}{font:?}").hash(&mut k);
                    [Prop::FontSize, Prop::LetterSpacing, Prop::StrokeWidth]
                        .map(|p| r.num(p).to_bits())
                        .hash(&mut k);
                    [Prop::Fill, Prop::Stroke]
                        .map(|p| r.color(p).0.map(f32::to_bits))
                        .hash(&mut k);
                    k.finish()
                };
                let mut draw = |rd: &mut Renderer, chunk: &mut Pixmap, t: Transform| {
                    let place = (font.align, VerticalAlign::Top);
                    rd.draw_text(
                        chunk,
                        r,
                        font,
                        &body,
                        place,
                        (0.0, 0.0, w, block),
                        t,
                        1.0,
                        None,
                    );
                };
                let rolled = strip::roll(
                    self,
                    pm,
                    &r.layer.id,
                    key,
                    tf,
                    (w, h),
                    block,
                    &tops,
                    opacity,
                    Some(&clip),
                    &mut draw,
                );
                for y in tops.into_iter().filter(|_| !rolled) {
                    let place = (font.align, VerticalAlign::Top);
                    self.draw_text(
                        pm,
                        r,
                        font,
                        &body,
                        place,
                        (0.0, y, w, block),
                        tf,
                        opacity,
                        Some(&clip),
                    );
                }
            }
            LayerKind::Image { fit, .. } => {
                if let Some(src) = r.image_src.clone() {
                    self.draw_image(pm, &src, *fit, w, h, tf, opacity, mask);
                }
            }
            LayerKind::AnalogClock {
                timezone,
                seconds,
                accent,
            } => {
                let (ha, ma, sa) = widgets::clock_hands(timezone, &time);
                let d = w.min(h);
                let (cx, cy, rad) = (w / 2.0, h / 2.0, d / 2.0);
                if let Some(face) = PathBuilder::from_circle(cx, cy, rad) {
                    pm.fill_path(&face, &fill, FillRule::Winding, tf, mask);
                }
                let ink = paint_for(r.color(Prop::Stroke), opacity);
                let line = |pb: &mut PathBuilder, deg: f32, r0: f32, r1: f32| {
                    let (sn, cs) = deg.to_radians().sin_cos();
                    pb.move_to(cx + sn * r0, cy - cs * r0);
                    pb.line_to(cx + sn * r1, cy - cs * r1);
                };
                let stroke = |width: f32| Stroke {
                    width,
                    line_cap: tiny_skia::LineCap::Round,
                    ..Default::default()
                };
                let mut ticks = PathBuilder::new();
                for i in 0..12 {
                    line(
                        &mut ticks,
                        i as f32 * 30.0,
                        rad * if i % 3 == 0 { 0.78 } else { 0.84 },
                        rad * 0.92,
                    );
                }
                if let Some(p) = ticks.finish() {
                    pm.stroke_path(&p, &ink, &stroke((d * 0.02).max(1.5)), tf, mask);
                }
                for (deg, len, width) in [(ha, 0.5, 0.06), (ma, 0.74, 0.04)] {
                    let mut pb = PathBuilder::new();
                    line(&mut pb, deg as f32, -rad * 0.08, rad * len);
                    if let Some(p) = pb.finish() {
                        pm.stroke_path(&p, &ink, &stroke(d * width), tf, mask);
                    }
                }
                let accent = paint_for(Color::from_hex(accent).unwrap_or(Color::WHITE), opacity);
                if *seconds {
                    let mut pb = PathBuilder::new();
                    line(&mut pb, sa as f32, -rad * 0.15, rad * 0.86);
                    if let Some(p) = pb.finish() {
                        pm.stroke_path(&p, &accent, &stroke((d * 0.012).max(1.0)), tf, mask);
                    }
                }
                if let Some(dot) = PathBuilder::from_circle(cx, cy, d * 0.035) {
                    pm.fill_path(
                        &dot,
                        if *seconds { &accent } else { &ink },
                        FillRule::Winding,
                        tf,
                        mask,
                    );
                }
            }
            LayerKind::Counter {
                x: _,
                from,
                to,
                duration,
                decimals,
                prefix,
                suffix,
                grouping,
                font,
            } => {
                let text = widgets::counter_text(
                    from,
                    to,
                    *duration,
                    *decimals,
                    prefix,
                    suffix,
                    *grouping,
                    ctx.data,
                    time.age_secs,
                );
                self.draw_text(
                    pm,
                    r,
                    font,
                    &text,
                    (font.align, font.vertical_align),
                    (0.0, 0.0, w, h),
                    tf,
                    opacity,
                    mask,
                );
            }
            LayerKind::Progress {
                x: _,
                value,
                max,
                duration,
                track,
            } => {
                let ratio =
                    widgets::progress_ratio(value, max, *duration, ctx.data, time.age_secs) as f32;
                let radius = r.num(Prop::CornerRadius) as f32;
                let track_paint = paint_for(
                    Color::from_hex(track).unwrap_or(Color::TRANSPARENT),
                    opacity,
                );
                if let Some(p) = rounded_rect(w, h, radius) {
                    pm.fill_path(&p, &track_paint, FillRule::Winding, tf, mask);
                }
                if let Some(p) = rounded_rect(w * ratio, h, radius.min(w * ratio / 2.0)) {
                    pm.fill_path(&p, &fill, FillRule::Winding, tf, mask);
                }
            }
            LayerKind::Rotator {
                text,
                interval,
                fade,
                font,
            } => {
                let items = widgets::lines(text, ctx.data);
                if items.is_empty() {
                    return;
                }
                let (idx, alpha) =
                    widgets::rotator_state(items.len(), *interval, *fade, time.age_secs);
                let item = if font.uppercase {
                    items[idx].to_uppercase()
                } else {
                    items[idx].clone()
                };
                self.draw_text(
                    pm,
                    r,
                    font,
                    &item,
                    (font.align, font.vertical_align),
                    (0.0, 0.0, w, h),
                    tf,
                    opacity * alpha as f32,
                    mask,
                );
            }
            LayerKind::Typewriter {
                x: _,
                text,
                speed,
                cursor,
                font,
            } => {
                let shown =
                    widgets::typewriter_text(text, *speed, *cursor, ctx.data, time.age_secs);
                let shown = if font.uppercase {
                    shown.to_uppercase()
                } else {
                    shown
                };
                self.draw_text(
                    pm,
                    r,
                    font,
                    &shown,
                    (font.align, font.vertical_align),
                    (0.0, 0.0, w, h),
                    tf,
                    opacity,
                    mask,
                );
            }
            LayerKind::Slideshow {
                images,
                interval,
                fade,
                fit,
            } => {
                let list = widgets::lines(images, ctx.data);
                if list.is_empty() {
                    return;
                }
                let (cur, next, mix) =
                    widgets::slideshow_state(list.len(), *interval, *fade, time.age_secs);
                self.draw_image(
                    pm,
                    &list[cur],
                    *fit,
                    w,
                    h,
                    tf,
                    opacity * (1.0 - mix as f32),
                    mask,
                );
                if mix > 0.0 {
                    self.draw_image(pm, &list[next], *fit, w, h, tf, opacity * mix as f32, mask);
                }
            }
            // Drawn by src/wid (wid::draw returns true for them).
            LayerKind::Polygon { .. }
            | LayerKind::Pen { .. }
            | LayerKind::Table { .. }
            | LayerKind::Credits { .. }
            | LayerKind::Chart { .. }
            | LayerKind::Map { .. }
            | LayerKind::Scoreboard { .. }
            | LayerKind::SocialWall { .. }
            | LayerKind::SocialBadge { .. }
            | LayerKind::Web { .. } => {}
            LayerKind::QrCode {
                data, background, ..
            } => {
                let content = interpolate_text(data, ctx.data);
                let Some((n, modules)) = qr_modules(&content) else {
                    return;
                };
                let side = w.min(h);
                let cell = side / (n + 8) as f32; // 4-module quiet zone
                let (ox, oy) = ((w - side) / 2.0 + 4.0 * cell, (h - side) / 2.0 + 4.0 * cell);
                let bg = paint_for(Color::from_hex(background).unwrap_or(Color::WHITE), opacity);
                if let Some(rc) = Rect::from_xywh((w - side) / 2.0, (h - side) / 2.0, side, side) {
                    pm.fill_rect(rc, &bg, tf, mask);
                }
                let mut pb = PathBuilder::new();
                for (i, dark) in modules.iter().enumerate() {
                    if *dark {
                        let (x, y) = ((i % n) as f32, (i / n) as f32);
                        if let Some(rc) =
                            Rect::from_xywh(ox + x * cell, oy + y * cell, cell + 0.02, cell + 0.02)
                        {
                            pb.push_rect(rc);
                        }
                    }
                }
                if let Some(p) = pb.finish() {
                    pm.fill_path(&p, &fill, FillRule::Winding, tf, mask);
                }
            }
            LayerKind::BarChart {
                text,
                percent,
                duration,
                track,
                text_color,
                font,
            } => {
                let rows = widgets::chart_rows(text, ctx.data);
                if rows.is_empty() {
                    return;
                }
                let total: f64 = rows.iter().map(|(_, v)| v.max(0.0)).sum();
                let max = rows.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max);
                let grow = widgets::grow(*duration, time.age_secs) as f32;
                let row_h = h / rows.len() as f32;
                let (label_w, value_w) = (w * 0.28, w * 0.16);
                let bar_x = label_w + w * 0.02;
                let bar_w = (w - label_w - value_w - w * 0.04).max(0.0);
                let track_paint = paint_for(
                    Color::from_hex(track).unwrap_or(Color::TRANSPARENT),
                    opacity,
                );
                let text_r = ResolvedLayer {
                    values: {
                        let mut v = r.values.clone();
                        v.insert(
                            Prop::Fill,
                            Value::Color(Color::from_hex(text_color).unwrap_or(Color::WHITE)),
                        );
                        v.insert(Prop::StrokeWidth, Value::Number(0.0));
                        v
                    },
                    ..r.clone()
                };
                for (i, (label, v)) in rows.iter().enumerate() {
                    let y = i as f32 * row_h;
                    let bh = row_h * 0.56;
                    let by = y + (row_h - bh) / 2.0;
                    if let Some(p) = rounded_rect(bar_w, bh, bh / 4.0) {
                        pm.fill_path(
                            &p,
                            &track_paint,
                            FillRule::Winding,
                            tf.pre_translate(bar_x, by),
                            mask,
                        );
                    }
                    let frac = if max > 0.0 {
                        (v / max).clamp(0.0, 1.0) as f32
                    } else {
                        0.0
                    };
                    if let Some(p) = rounded_rect(bar_w * frac * grow, bh, bh / 4.0) {
                        pm.fill_path(
                            &p,
                            &fill,
                            FillRule::Winding,
                            tf.pre_translate(bar_x, by),
                            mask,
                        );
                    }
                    let label = if font.uppercase {
                        label.to_uppercase()
                    } else {
                        label.clone()
                    };
                    self.draw_text(
                        pm,
                        &text_r,
                        font,
                        &label,
                        (TextAlign::Left, VerticalAlign::Middle),
                        (0.0, y, label_w, row_h),
                        tf,
                        opacity,
                        mask,
                    );
                    let shown = if *percent {
                        let pct = if total > 0.0 { v / total * 100.0 } else { 0.0 };
                        format!("{}%", widgets::format_number(pct * grow as f64, 0, false))
                    } else {
                        widgets::format_number(
                            v * grow as f64,
                            if v.fract() == 0.0 { 0 } else { 1 },
                            true,
                        )
                    };
                    self.draw_text(
                        pm,
                        &text_r,
                        font,
                        &shown,
                        (TextAlign::Right, VerticalAlign::Middle),
                        (w - value_w, y, value_w, row_h),
                        tf,
                        opacity,
                        mask,
                    );
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_image(
        &mut self,
        pm: &mut Pixmap,
        src: &str,
        fit: ImageFit,
        w: f32,
        h: f32,
        tf: Transform,
        opacity: f32,
        mask: Option<&Mask>,
    ) {
        if opacity <= 0.0 {
            return;
        }
        // Cover: clip to the layer box.
        let cover_mask = if fit == ImageFit::Cover {
            Rect::from_xywh(0.0, 0.0, w, h).map(|rc| build_mask(pm, mask, rc, tf))
        } else {
            None
        };
        let Some(img) = self.image(src) else { return };
        let (iw, ih) = (img.width() as f32, img.height() as f32);
        let (sx, sy) = match fit {
            ImageFit::Stretch => (w / iw, h / ih),
            ImageFit::Contain => {
                let s = (w / iw).min(h / ih);
                (s, s)
            }
            ImageFit::Cover => {
                let s = (w / iw).max(h / ih);
                (s, s)
            }
        };
        let img_tf = tf
            .pre_translate((w - iw * sx) / 2.0, (h - ih * sy) / 2.0)
            .pre_scale(sx, sy);
        let paint = PixmapPaint {
            opacity,
            quality: tiny_skia::FilterQuality::Bicubic,
            ..Default::default()
        };
        pm.draw_pixmap(
            0,
            0,
            img.as_ref(),
            &paint,
            img_tf,
            cover_mask.as_ref().or(mask),
        );
    }

    /// Draws `content` with a widget's font inside the box (x, y, w, h)
    /// of the layer.
    #[allow(clippy::too_many_arguments)]
    fn draw_text(
        &mut self,
        pm: &mut Pixmap,
        r: &ResolvedLayer,
        font: &FontStyle,
        content: &str,
        (align, vertical_align): (TextAlign, VerticalAlign),
        (x, y, w, h): (f32, f32, f32, f32),
        tf: Transform,
        opacity: f32,
        mask: Option<&Mask>,
    ) {
        if content.is_empty() {
            return;
        }
        let Some(f) = self.fonts.font(&font.font_family, font.font_weight) else {
            tracing::warn!(font = font.font_family, "no font available");
            return;
        };
        let style = text::TextStyle {
            size: r.num(Prop::FontSize) as f32,
            letter_spacing: r.num(Prop::LetterSpacing) as f32,
            align,
            vertical_align,
            box_w: w,
            box_h: h,
            line_height: text::LINE_HEIGHT,
            decoration: cg_core::TextDecoration::None,
        };
        let Some(path) = text::layout_text(&f, content, &style) else {
            return;
        };
        let tf = tf.pre_translate(x, y);
        pm.fill_path(
            &path,
            &paint_for(r.color(Prop::Fill), opacity),
            FillRule::Winding,
            tf,
            mask,
        );
        let stroke_w = r.num(Prop::StrokeWidth) as f32;
        if stroke_w > 0.0 {
            let stroke = Stroke {
                width: stroke_w,
                ..Default::default()
            };
            pm.stroke_path(
                &path,
                &paint_for(r.color(Prop::Stroke), opacity),
                &stroke,
                tf,
                mask,
            );
        }
    }

    /// Draws the picture `src` through `img_tf` (picture → frame) at
    /// `opacity`, cut by `mask`, inside the frame region `clip`: kept scaled
    /// to its size on the frame (see `scaled`), bicubic, bilinear while it
    /// moves (`moving`). Its size in pixels (None: no such picture).
    pub(crate) fn draw_picture(
        &mut self,
        pm: &mut Pixmap,
        src: &str,
        img_tf: Transform,
        moving: bool,
        opacity: f32,
        mask: Option<&Mask>,
        clip: Option<tiny_skia::IntRect>,
    ) -> Option<(u32, u32)> {
        self.image(src)?;
        let img = self.images.get(src)?.as_ref()?;
        scaled_picture(
            &mut self.scaled,
            pm,
            src,
            img,
            img_tf,
            moving,
            opacity,
            mask,
            clip,
        );
        Some((img.width(), img.height()))
    }

    fn image(&mut self, src: &str) -> Option<&Pixmap> {
        if !self.images.contains_key(src) {
            let loaded = load_image(&self.assets_dir, src);
            if loaded.is_none() {
                // A web picture not fetched yet: asked for, and looked for again.
                if is_url(src) {
                    fetch_url(src, false);
                    self.pending = true;
                    return None;
                }
                tracing::warn!(src, "image not found or unreadable");
            }
            self.images.insert(src.to_string(), loaded);
        }
        self.images.get(src).and_then(Option::as_ref)
    }
}

/// Draws `img` (named `key`) through `img_tf`: kept scaled when the
/// transform only scales and moves it (see `scaled`), else drawn directly.
/// Bicubic, bilinear while it moves.
pub(crate) fn scaled_picture(
    scaled: &mut scaled::Scaled,
    pm: &mut Pixmap,
    key: &str,
    img: &Pixmap,
    img_tf: Transform,
    moving: bool,
    opacity: f32,
    mask: Option<&Mask>,
    clip: Option<tiny_skia::IntRect>,
) {
    let quality = if moving {
        tiny_skia::FilterQuality::Bilinear
    } else {
        tiny_skia::FilterQuality::Bicubic
    };
    if !scaled.draw(pm, key, img, img_tf, quality, opacity, mask, clip, !moving) {
        let paint = PixmapPaint {
            opacity,
            quality,
            ..Default::default()
        };
        pm.draw_pixmap(0, 0, img.as_ref(), &paint, img_tf, mask);
    }
}

/// A picture at a web address (`http://`, `https://`).
pub fn is_url(src: &str) -> bool {
    let s = src.trim().to_ascii_lowercase();
    s.starts_with("http://") || s.starts_with("https://")
}

/// Where a web picture is kept once fetched (`<assets>/.web/<hash>`).
pub fn url_cache_path(assets_dir: &Path, url: &str) -> std::path::PathBuf {
    // FNV-1a: the same name on every run.
    let h = url.trim().bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    });
    assets_dir.join(".web").join(format!("{h:016x}"))
}

/// Fetches a web picture into its cache file (`again`: even if it is there).
type UrlFetcher = dyn Fn(&str, bool) + Send + Sync;
static URL_FETCHER: std::sync::OnceLock<Box<UrlFetcher>> = std::sync::OnceLock::new();

/// The host (the server) fetches the web pictures the renderers ask for.
pub fn set_url_fetcher(f: Box<UrlFetcher>) {
    let _ = URL_FETCHER.set(f);
}

/// Asks the host for a web picture (nothing without a host).
pub fn fetch_url(url: &str, again: bool) {
    if let Some(f) = URL_FETCHER.get() {
        f(url.trim(), again);
    }
}

fn load_image(assets_dir: &Path, src: &str) -> Option<Pixmap> {
    if is_url(src) {
        let path = url_cache_path(assets_dir, src);
        let data = std::fs::read(&path).ok()?;
        let head = String::from_utf8_lossy(&data[..data.len().min(256)]).to_ascii_lowercase();
        if head.contains("<svg") {
            return load_svg(&path);
        }
        let img = image::load_from_memory(&data).ok()?.to_rgba8();
        return premultiplied(img);
    }
    let name = src.trim().strip_prefix("/assets/")?;
    if name.contains("..") {
        return None;
    }
    let path = assets_dir.join(name);
    if name.to_ascii_lowercase().ends_with(".svg") {
        return load_svg(&path);
    }
    premultiplied(image::open(path).ok()?.to_rgba8())
}

fn premultiplied(img: image::RgbaImage) -> Option<Pixmap> {
    let (w, h) = img.dimensions();
    let mut data = img.into_raw();
    for px in data.as_chunks_mut::<4>().0 {
        let a = px[3] as u16;
        for c in &mut px[..3] {
            *c = ((*c as u16 * a + 127) / 255) as u8;
        }
    }
    Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(w, h)?)
}

/// An SVG drawn into a picture (2048 px on its long side: sharp when scaled
/// down, as logos are).
fn load_svg(path: &Path) -> Option<Pixmap> {
    let data = std::fs::read(path).ok()?;
    let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    let k = 2048.0 / size.width().max(size.height()).max(1.0);
    let (w, h) = (
        (size.width() * k).round().max(1.0) as u32,
        (size.height() * k).round().max(1.0) as u32,
    );
    let mut pm = Pixmap::new(w, h)?;
    resvg::render(&tree, Transform::from_scale(k, k), &mut pm.as_mut());
    Some(pm)
}

/// The layer or one of its parents repeats a movement (`Layer::motion`).
fn loops(t: &Template, layer: &cg_core::Layer) -> bool {
    let mut at = Some(layer);
    for _ in 0..64 {
        let Some(l) = at else { break };
        if l.motion.is_some() {
            return true;
        }
        at = l
            .parent
            .as_deref()
            .and_then(|id| t.layers.iter().find(|x| x.id == id));
    }
    false
}

/// The layer or one of its parents moves, turns or changes size between
/// `frame` and the next (keyframes): pictures are then scaled with a
/// cheaper filter (bilinear) than at rest (bicubic).
fn geometry_moving(t: &Template, layer: &cg_core::Layer, frame: f64) -> bool {
    const GEOMETRY: [Prop; 9] = [
        Prop::X,
        Prop::Y,
        Prop::Width,
        Prop::Height,
        Prop::AnchorX,
        Prop::AnchorY,
        Prop::Rotation,
        Prop::ScaleX,
        Prop::ScaleY,
    ];
    let mut at = Some(layer);
    for _ in 0..64 {
        let Some(l) = at else { break };
        let moves = GEOMETRY.iter().any(|p| {
            l.tracks.get(p).is_some_and(|k| {
                k.len() > 1
                    && cg_core::anim::sample_track(k, frame)
                        != cg_core::anim::sample_track(k, frame + 1.0)
            })
        });
        if moves {
            return true;
        }
        at = l
            .parent
            .as_deref()
            .and_then(|id| t.layers.iter().find(|x| x.id == id));
    }
    false
}

/// Same convention as the web renderer: position is the top-left of the box
/// in parent space, rotation/scale happen around the anchor point.
fn local_transform(r: &ResolvedLayer, w: f32, h: f32) -> Transform {
    let ax = r.num(Prop::AnchorX) as f32 * w;
    let ay = r.num(Prop::AnchorY) as f32 * h;
    Transform::from_translate(r.num(Prop::X) as f32 + ax, r.num(Prop::Y) as f32 + ay)
        .pre_rotate(r.num(Prop::Rotation) as f32)
        .pre_scale(r.num(Prop::ScaleX) as f32, r.num(Prop::ScaleY) as f32)
        .pre_translate(-ax, -ay)
}

/// Returns `None` when no crop is set, `Some(None)` when fully cropped.
fn crop_rect(r: &ResolvedLayer, w: f32, h: f32) -> Option<Option<Rect>> {
    let (l, rt, t, b) = (
        r.num(Prop::CropLeft) as f32,
        r.num(Prop::CropRight) as f32,
        r.num(Prop::CropTop) as f32,
        r.num(Prop::CropBottom) as f32,
    );
    if l <= 0.0 && rt <= 0.0 && t <= 0.0 && b <= 0.0 {
        return None;
    }
    let x0 = w * l.clamp(0.0, 1.0);
    let x1 = w * (1.0 - rt.clamp(0.0, 1.0));
    let y0 = h * t.clamp(0.0, 1.0);
    let y1 = h * (1.0 - b.clamp(0.0, 1.0));
    Some(Rect::from_ltrb(x0, y0, x1, y1))
}

fn build_mask(pm: &Pixmap, parent: Option<&Mask>, rect: Rect, tf: Transform) -> Mask {
    let path = PathBuilder::from_rect(rect);
    match parent {
        Some(p) => mask::intersect(p, &path, tf),
        None => {
            let mut m = Mask::new(pm.width(), pm.height()).expect("valid size");
            m.fill_path(&path, FillRule::Winding, true, tf);
            m
        }
    }
}

fn paint_for(c: Color, opacity: f32) -> Paint<'static> {
    let [r, g, b, a] = c.0;
    let mut p = Paint::default();
    p.set_color(
        tiny_skia::Color::from_rgba(
            r.clamp(0.0, 1.0),
            g.clamp(0.0, 1.0),
            b.clamp(0.0, 1.0),
            (a * opacity).clamp(0.0, 1.0),
        )
        .unwrap_or(tiny_skia::Color::TRANSPARENT),
    );
    p.anti_alias = true;
    p
}

fn rounded_rect(w: f32, h: f32, radius: f32) -> Option<tiny_skia::Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let r = radius.clamp(0.0, w.min(h) / 2.0);
    if r <= 0.0 {
        return Some(PathBuilder::from_rect(Rect::from_xywh(0.0, 0.0, w, h)?));
    }
    // Circular arcs approximated with cubics (kappa).
    let k = r * 0.552_284_8;
    let mut pb = PathBuilder::new();
    pb.move_to(r, 0.0);
    pb.line_to(w - r, 0.0);
    pb.cubic_to(w - r + k, 0.0, w, r - k, w, r);
    pb.line_to(w, h - r);
    pb.cubic_to(w, h - r + k, w - r + k, h, w - r, h);
    pb.line_to(r, h);
    pb.cubic_to(r - k, h, 0.0, h - r + k, 0.0, h - r);
    pb.line_to(0.0, r);
    pb.cubic_to(0.0, r - k, r - k, 0.0, r, 0.0);
    pb.close();
    pb.finish()
}

#[cfg(test)]
mod tests {
    /// Layers at rest laid from their sprites are the layers drawn straight
    /// (to the rounding of laying premultiplied pixels twice), fractions of
    /// positions included; moving, they are laid bilinear, close to it.
    #[test]
    fn sprites_draw_what_straight_drawing_does() {
        use super::*;
        let project = cg_core::sample::demo_project();
        let t: Template = serde_json::from_value(serde_json::json!({
            "id": "sp", "name": "S", "duration": 100.0, "markers": [], "fields": [], "zone": "",
            "layers": [
                {"id": "a", "name": "Card", "type": "rect", "dropShadow": true,
                 "props": {"x": 100.5, "y": 80.25, "width": 500.0, "height": 240.0, "fill": [0.9, 0.3, 0.2, 1.0], "cornerRadius": 18.0, "shadowBlur": 24.0, "shadowX": 0.0, "shadowY": 10.0, "shadowColor": [0.0, 0.0, 0.0, 0.6]}},
                {"id": "b", "name": "Disc", "type": "ellipse",
                 "props": {"x": 700.0, "y": 60.0, "width": 300.0, "height": 300.0, "fill": [0.2, 0.5, 0.9, 1.0]},
                 "tracks": {"x": [{"frame": 0.0, "value": 700.0, "easing": {"type": "linear"}}, {"frame": 50.0, "value": 900.0, "easing": {"type": "linear"}}]}},
                {"id": "c", "name": "Text", "type": "text", "text": "Bonjour le monde", "fontFamily": "Inter", "fontWeight": 700,
                 "props": {"x": 120.75, "y": 400.0, "width": 800.0, "height": 120.0, "fontSize": 72.0, "fill": [1.0, 1.0, 1.0, 1.0]}},
                {"id": "g", "name": "Group", "type": "group", "dropShadow": true,
                 "props": {"x": 300.0, "y": 560.0, "width": 400.0, "height": 160.0, "shadowBlur": 12.0, "shadowY": 6.0, "shadowColor": [0.0, 0.0, 0.0, 0.5]}},
                {"id": "h", "name": "In group", "type": "rect", "parent": "g",
                 "props": {"x": 20.0, "y": 20.0, "width": 360.0, "height": 120.0, "fill": [0.3, 0.8, 0.4, 1.0]}}
            ]
        }))
        .unwrap();
        let dir = std::env::temp_dir().join("cg-render-test-assets");
        let mut with = Renderer::new(&dir);
        let mut without = Renderer::new(&dir);
        without.set_sprites(false);
        let data = Data::default();
        let diff = |frame: f64, with: &mut Renderer, without: &mut Renderer| {
            let a = with.render_template(&project, &t, "main", frame, &data);
            let b = without.render_template(&project, &t, "main", frame, &data);
            let (mut max, mut n) = (0u8, 0usize);
            for (x, y) in a.data.iter().zip(&b.data) {
                let d = x.abs_diff(*y);
                max = max.max(d);
                n += usize::from(d > 1);
            }
            (max, n)
        };
        // At rest (frame 60: the disc stopped), twice (the second from the
        // sprites kept). A few edge pixels differ by one coverage step (the
        // path's coordinates rounded in another place).
        for _ in 0..2 {
            let (max, n) = diff(60.0, &mut with, &mut without);
            assert!(
                max <= 24 && n <= 64,
                "at rest, sprites differ by {max} on {n} bytes"
            );
        }
        assert!(with.sprites.found > 0, "sprites laid again");
        // Moving (the disc slides): laid bilinear, close to the straight drawing.
        let (max, n) = diff(25.0, &mut with, &mut without);
        assert!(
            n < 20_000,
            "moving, {n} bytes differ by more than 1 (max {max})"
        );
    }

    /// Over a live picture: the graphics where they draw, the picture
    /// (scaled to fit, on black) elsewhere; opaque.
    #[test]
    fn a_source_of_another_size_is_fitted_on_black() {
        // 4:3 into 16:9: bars left and right, the colour kept inside.
        let mut live = Pixmap::new(160, 120).unwrap();
        live.fill(tiny_skia::Color::from_rgba8(200, 100, 50, 255));
        let out = fit_picture(&live, 320, 180);
        let px = |x: usize, y: usize| &out.data()[(y * 320 + x) * 4..(y * 320 + x) * 4 + 4];
        assert_eq!(px(5, 90), &[0, 0, 0, 255]);
        assert_eq!(px(314, 90), &[0, 0, 0, 255]);
        for (x, y) in [(40, 0), (160, 90), (279, 179)] {
            assert_eq!(px(x, y), &[200, 100, 50, 255], "at {x},{y}");
        }
        // A ramp stays a ramp (no step where the bands meet).
        let mut ramp = Pixmap::new(64, 64).unwrap();
        for (i, p) in ramp
            .data_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .enumerate()
        {
            let v = (i / 64 * 4) as u8;
            p.copy_from_slice(&[v, v, v, 255]);
        }
        let up = fit_picture(&ramp, 96, 96);
        let col: Vec<u8> = (0..96).map(|y| up.data()[(y * 96 + 10) * 4]).collect();
        assert!(
            col.windows(2).all(|w| w[1] >= w[0] && w[1] - w[0] <= 4),
            "{col:?}"
        );
    }

    #[test]
    fn graphics_are_keyed_over_the_live_picture() {
        use super::*;
        let mut g = Frame::blank(4, 2);
        // A half-transparent red pixel at (0, 0) (premultiplied).
        g.data[..4].copy_from_slice(&[128, 0, 0, 128]);
        let mut live = Pixmap::new(2, 1).unwrap();
        live.fill(tiny_skia::Color::from_rgba8(0, 0, 200, 255));
        let f = over_live(&g, &live);
        let px = |x: usize, y: usize| &f.data[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4];
        assert!(f.data.chunks(4).all(|p| p[3] == 255));
        // Red over blue at (0, 0), blue elsewhere (2×1 scaled to 4×2).
        assert_eq!(px(0, 0)[0], 128);
        assert!(px(0, 0)[2] > 90 && px(0, 0)[2] < 110);
        assert_eq!(px(3, 1), &[0, 0, 200, 255]);
    }

    #[test]
    fn a_web_picture_is_read_from_its_cache() {
        let dir = std::env::temp_dir().join(format!("cg-web-{}", std::process::id()));
        let url = "https://example.com/logo.png?v=2";
        assert!(is_url(url) && !is_url("/assets/a.png"));
        let path = url_cache_path(&dir, url);
        assert!(load_image(&dir, url).is_none());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::RgbaImage::from_pixel(3, 2, image::Rgba([255, 0, 0, 255]))
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        let pm = load_image(&dir, url).expect("read from the cache");
        assert_eq!((pm.width(), pm.height()), (3, 2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_slate_fills_the_frame() {
        let dir = std::env::temp_dir().join(format!("cg-slate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // 4×1: red then blue halves; in a 2×2 frame it is cropped to its middle.
        let img = image::RgbaImage::from_fn(4, 1, |x, _| {
            if x < 2 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 255, 255])
            }
        });
        img.save(dir.join("slate.png")).unwrap();
        let mut r = Renderer::new(&dir);
        let f = r.render_slate(
            "/assets/slate.png",
            OutputFormat {
                width: 2,
                height: 2,
            },
        );
        assert_eq!((f.width, f.height), (2, 2));
        assert!(
            f.data.chunks(4).all(|p| p[3] == 255),
            "covers the whole frame"
        );
        assert!(f.data[0] > 150 && f.data[6] > 150, "red left, blue right");
        // No picture: transparent.
        let f = r.render_slate(
            "/assets/missing.png",
            OutputFormat {
                width: 2,
                height: 2,
            },
        );
        assert!(f.data.iter().all(|b| *b == 0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;
    use cg_core::{sample, Command};

    const OUT: OutputFormat = OutputFormat {
        width: 1920,
        height: 1080,
    };

    #[test]
    fn only_the_zone_shows_on_air() {
        let mut project = sample::demo_project();
        // The credits (a full-frame shade) taken in a top-left zone.
        project.zones = vec![cg_core::Zone {
            id: "top-left".into(),
            name: "Top left".into(),
            layer: 50,
            color: String::new(),
            transition: cg_core::ZoneTransition::Cut,
            areas: Default::default(),
        }];
        let mut state = ChannelState::default();
        state
            .apply(
                &project,
                Command::Take {
                    zone: None,
                    layer: Some(50),
                    template_id: "credits".into(),
                    data: None,
                    rundown_id: None,
                },
            )
            .unwrap();
        for _ in 0..60 {
            state.tick(1.0);
        }
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let f = r.render_channel(&project, &state, OUT, "UTC");
        let alpha = |x: usize, y: usize| f.data[(y * 1920 + x) * 4 + 3];
        // Top-left zone: 4 %..28 % x 5 %..19 % of the frame.
        assert!(alpha(200, 100) > 0);
        assert_eq!(alpha(1500, 800), 0);
    }

    #[test]
    fn a_layer_shows_only_within_its_span() {
        let mut project = sample::demo_project();
        project.zones = vec![cg_core::Zone {
            id: "top-left".into(),
            name: "Top left".into(),
            layer: 50,
            color: String::new(),
            transition: cg_core::ZoneTransition::Cut,
            areas: Default::default(),
        }];
        // Every layer of the credits shown from frame 0 to 8 only.
        let t = project
            .templates
            .iter_mut()
            .find(|t| t.id == "credits")
            .unwrap();
        for l in &mut t.layers {
            l.span = Some(cg_core::Span { from: 0.0, to: 8.0 });
        }
        let mut state = ChannelState::default();
        state
            .apply(
                &project,
                Command::Take {
                    zone: None,
                    layer: Some(50),
                    template_id: "credits".into(),
                    data: None,
                    rundown_id: None,
                },
            )
            .unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let alpha = |f: &Frame| f.data[(100 * 1920 + 200) * 4 + 3];
        for _ in 0..5 {
            state.tick(1.0);
        }
        let f = r.render_channel(&project, &state, OUT, "UTC");
        assert!(alpha(&f) > 0, "shown at frame 5");
        // Held at its pause (frame 12): gone.
        for _ in 0..20 {
            state.tick(1.0);
        }
        let f = r.render_channel(&project, &state, OUT, "UTC");
        assert_eq!(alpha(&f), 0, "gone after frame 8");
    }

    #[test]
    fn an_update_cross_fades() {
        let project = sample::demo_project();
        let mut state = ChannelState::default();
        let data =
            |name: &str| std::collections::BTreeMap::from([("name".to_string(), name.to_string())]);
        state
            .apply(
                &project,
                Command::Take {
                    zone: Some("lower-third".into()),
                    layer: None,
                    template_id: "lower-third".into(),
                    data: Some(data("A")),
                    rundown_id: None,
                },
            )
            .unwrap();
        for _ in 0..200 {
            state.tick(1.0);
        }
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let old = r.render_channel(&project, &state, OUT, "UTC");
        let update = |fade| Command::Update {
            zone: Some("lower-third".into()),
            layer: None,
            data: data("Wolfgang Amadeus Mozart"),
            fade,
        };
        let mut cut = state.clone();
        cut.apply(&project, update(false)).unwrap();
        let new = r.render_channel(&project, &cut, OUT, "UTC");
        assert_ne!(old.data, new.data);
        // Halfway through the fade: half the old picture plus half the new one.
        state.apply(&project, update(true)).unwrap();
        let f = state
            .layers
            .values_mut()
            .next()
            .unwrap()
            .fade
            .as_mut()
            .unwrap();
        f.frame = f.length / 2.0;
        let mid = r.render_channel(&project, &state, OUT, "UTC");
        let worst = (0..mid.data.len())
            .map(|i| (mid.data[i] as i32 - (old.data[i] as i32 + new.data[i] as i32) / 2).abs())
            .max()
            .unwrap();
        assert!(worst <= 2, "off by {worst}");
    }

    #[test]
    fn a_fade_zone_draws_the_graphic_going_under_the_new_one() {
        let mut project = sample::demo_project();
        let data =
            |name: &str| std::collections::BTreeMap::from([("name".to_string(), name.to_string())]);
        let take = |name: &str| Command::Take {
            zone: Some("lower-third".into()),
            layer: None,
            template_id: "lower-third".into(),
            data: Some(data(name)),
            rundown_id: None,
        };
        let mut state = ChannelState::default();
        state.apply(&project, take("Marie Curie")).unwrap();
        for _ in 0..200 {
            state.tick(1.0);
        }
        // The same take in a Cut zone: the new graphic alone.
        let mut cut = state.clone();
        for z in &mut project.zones {
            z.transition = cg_core::ZoneTransition::Cut;
        }
        cut.apply(&project, take("Albert Einstein")).unwrap();
        for z in &mut project.zones {
            z.transition = cg_core::ZoneTransition::Fade;
        }
        state.apply(&project, take("Albert Einstein")).unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let alone = r.render_channel(&project, &cut, OUT, "UTC");
        let both = r.render_channel(&project, &state, OUT, "UTC");
        assert_ne!(alone.data, both.data, "the graphic going still shows");
        // Faded out: only the new graphic.
        let going = state
            .layers
            .values_mut()
            .next()
            .unwrap()
            .leaving
            .as_mut()
            .unwrap();
        going.frame = going.length;
        let after = r.render_channel(&project, &state, OUT, "UTC");
        assert_eq!(alone.data, after.data);
    }

    #[test]
    fn svg_pictures_are_drawn() {
        let dir = std::env::temp_dir().join("cg-render-svg-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("dot.svg"),
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20"><rect width="10" height="20" fill="#00ff00"/></svg>"##,
        )
        .unwrap();
        let pm = load_svg(&dir.join("dot.svg")).expect("drawn");
        assert_eq!((pm.width(), pm.height()), (1024, 2048));
        let p = pm.pixel(500, 1000).unwrap();
        assert_eq!((p.red(), p.green(), p.blue(), p.alpha()), (0, 255, 0, 255));
        assert!(load_image(&dir, "/assets/dot.svg").is_some());
    }

    #[test]
    fn a_timer_with_an_analog_face_draws_its_face_and_hands() {
        let project = sample::demo_project();
        let t: cg_core::Template = serde_json::from_value(serde_json::json!({
            "id": "ac", "name": "T", "duration": 50.0, "markers": [], "zone": "", "fields": [],
            "layers": [{
                "id": "c", "name": "Clock", "type": "clock", "timerMode": "countdown", "cdDuration": 11730000, "face": "analog",
                "analog": {"face": "#0000ff", "hands": "#ffffff", "accent": "#ff0000", "ticks": "hours"},
                "props": {"x": 0.0, "y": 0.0, "width": 400.0, "height": 400.0, "fill": [1.0, 1.0, 1.0, 1.0]}
            }]
        }))
        .unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let f = r.render_template(&project, &t, "main", 0.0, &Default::default());
        let px = |x: u32, y: u32| {
            let i = ((y * f.width + x) * 4) as usize;
            [f.data[i], f.data[i + 1], f.data[i + 2], f.data[i + 3]]
        };
        // A countdown not started yet shows 3:15:30 (hands away from 12): blue face,
        // nothing in the corner, the red dot of the second hand at the centre.
        assert_eq!(px(200, 60), [0, 0, 255, 255]);
        assert_eq!(px(5, 5)[3], 0);
        assert_eq!(px(200, 200), [255, 0, 0, 255]);
        // The 12 o'clock mark is white.
        assert_eq!(px(200, 26), [255, 255, 255, 255]);
    }

    #[test]
    fn a_flip_ticker_types_its_item_in() {
        let project = sample::demo_project();
        let t: cg_core::Template = serde_json::from_value(serde_json::json!({
            "id": "tk", "name": "T", "duration": 500.0, "markers": [], "zone": "", "fields": [],
            "layers": [{
                "id": "k", "name": "Ticker", "type": "ticker", "text": "", "separator": "", "speed": 100.0,
                "fontFamily": "Inter", "mode": "flip", "trans": "type", "transDur": 2.0, "itemDur": 8.0,
                "items": [{"cat": "", "text": "MMMMMMMM", "on": true}],
                "props": {"x": 0.0, "y": 0.0, "width": 1200.0, "height": 100.0, "fill": [1.0, 1.0, 1.0, 1.0], "fontSize": 60.0}
            }]
        }))
        .unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let fps = project.frame_rate.as_f64();
        // Ink along the middle line of the ticker.
        let mut ink = |frame: f64| {
            let f = r.render_template(&project, &t, "main", frame, &Default::default());
            (0..f.width)
                .filter(|&x| f.data[((50 * f.width + x) * 4 + 3) as usize] > 128)
                .count()
        };
        let quarter = ink(fps * 0.5);
        let typed = ink(fps * 3.0);
        assert!(typed > 100, "the whole item once typed ({typed} px)");
        assert!(
            quarter * 2 < typed,
            "a quarter of it half a second in ({quarter} px of {typed})"
        );
    }

    #[test]
    fn a_drop_shadow_falls_under_the_layer_and_its_children() {
        let project = sample::demo_project();
        let shadow = serde_json::json!({"shadowColor": [0.0, 0.0, 1.0, 1.0], "shadowX": 20.0, "shadowY": 30.0, "shadowBlur": 0.0});
        let props = |x: f64, y: f64, extra: &serde_json::Value| {
            let mut p = serde_json::json!({"x": x, "y": y, "width": 200.0, "height": 100.0, "fill": [1.0, 0.0, 0.0, 1.0]});
            for (k, v) in extra.as_object().unwrap() {
                p[k] = v.clone();
            }
            p
        };
        let t: cg_core::Template = serde_json::from_value(serde_json::json!({
            "id": "sh", "name": "T", "duration": 10.0, "markers": [], "zone": "", "fields": [],
            "layers": [
                {"id": "a", "name": "Rect", "type": "rect", "dropShadow": true, "props": props(100.0, 100.0, &shadow)},
                {"id": "e", "name": "Ellipse", "type": "ellipse", "dropShadow": true, "fillStyle": {"kind": "solid"}, "props": props(600.0, 100.0, &shadow)},
                // A group's shadow is its children's.
                {"id": "g", "name": "Group", "type": "group", "dropShadow": true, "props": {"x": 100.0, "y": 400.0, "width": 200.0, "height": 100.0, "shadowColor": [0.0, 0.0, 1.0, 1.0], "shadowX": 20.0, "shadowY": 30.0, "shadowBlur": 0.0}},
                {"id": "c", "name": "Child", "type": "rect", "parent": "g", "props": props(0.0, 0.0, &serde_json::json!({}))},
                // Blurred, and at half opacity.
                {"id": "b", "name": "Soft", "type": "rect", "dropShadow": true, "props": props(1100.0, 100.0, &serde_json::json!({"shadowColor": [0.0, 0.0, 1.0, 1.0], "shadowY": 30.0, "shadowBlur": 20.0, "opacity": 0.5}))},
                // A Text keeps its own shadow: no layer shadow.
                {"id": "t", "name": "Text", "type": "text", "text": "████", "fontFamily": "Inter", "dropShadow": true, "props": props(600.0, 400.0, &shadow)}
            ]
        }))
        .unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let f = r.render_template(&project, &t, "main", 0.0, &Default::default());
        let px = |x: u32, y: u32| {
            let i = ((y * f.width + x) * 4) as usize;
            [f.data[i], f.data[i + 1], f.data[i + 2], f.data[i + 3]]
        };
        // The rectangle over its shadow, the shadow 20 px right and 30 px down.
        assert_eq!(px(110, 110), [255, 0, 0, 255]);
        assert_eq!(px(310, 215), [0, 0, 255, 255]);
        assert_eq!(px(90, 90)[3], 0);
        assert_eq!(px(325, 215)[3], 0);
        // The ellipse's shadow is round too (its box corner stays empty).
        assert_eq!(px(700, 150), [255, 0, 0, 255]);
        assert_eq!(px(720, 225), [0, 0, 255, 255]);
        assert_eq!(px(816, 228)[3], 0);
        // The group's child casts it.
        assert_eq!(px(310, 515), [0, 0, 255, 255]);
        // Blurred: soft at the edge, at the layer's opacity.
        let mid = px(1200, 215);
        assert!(mid[2] > 100 && mid[3] < 140 && mid[3] > 100, "{mid:?}");
        let edge = px(1200, 245);
        assert!(edge[3] > 0 && edge[3] < mid[3], "{edge:?}");
        // No shadow for the Text.
        assert_eq!(px(810, 520)[3], 0);
    }

    #[test]
    fn an_ellipse_arc_and_inner_radius_animate() {
        let project = sample::demo_project();
        // A ring (inner 50 % from its settings) whose end goes 0° → 360°
        // over 10 frames, and whose inner radius goes to 0 at frame 20.
        let t: cg_core::Template = serde_json::from_value(serde_json::json!({
            "id": "el", "name": "T", "duration": 30.0, "markers": [], "zone": "", "fields": [],
            "layers": [{
                "id": "e", "name": "Ring", "type": "ellipse", "circle": true, "inner": 50.0,
                "fillStyle": {"kind": "solid"},
                "props": {"x": 0.0, "y": 0.0, "width": 400.0, "height": 400.0, "fill": [1.0, 0.0, 0.0, 1.0]},
                "tracks": {
                    "arcEnd": [
                        {"frame": 0.0, "value": 0.0, "easing": {"type": "linear"}},
                        {"frame": 10.0, "value": 360.0, "easing": {"type": "linear"}}
                    ],
                    "innerRadius": [
                        {"frame": 10.0, "value": 50.0, "easing": {"type": "linear"}},
                        {"frame": 20.0, "value": 0.0, "easing": {"type": "linear"}}
                    ]
                }
            }]
        }))
        .unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let at = |r: &mut Renderer, frame: f64| {
            let f = r.render_template(&project, &t, "main", frame, &Default::default());
            move |x: u32, y: u32| f.data[((y * f.width + x) * 4 + 3) as usize]
        };
        // Frame 0: no sweep, nothing drawn.
        let a = at(&mut r, 0.0);
        assert_eq!(a(350, 200), 0);
        // Frame 5: the right half of the ring (0° → 180°), not the left one.
        let a = at(&mut r, 5.0);
        assert_eq!(a(350, 200), 255);
        assert_eq!(a(50, 200), 0);
        // Inside the 50 % hole: empty.
        assert_eq!(a(230, 200), 0);
        // Frame 20: a full disc (no hole any more).
        let a = at(&mut r, 20.0);
        assert_eq!(a(50, 200), 255);
        assert_eq!(a(230, 200), 255);
    }

    #[test]
    fn a_scrolling_text_comes_in_from_the_right_with_its_looks() {
        let project = sample::demo_project();
        let t: cg_core::Template = serde_json::from_value(serde_json::json!({
            "id": "sc", "name": "T", "duration": 500.0, "markers": [], "zone": "", "fields": [],
            "layers": [{
                "id": "t", "name": "Ticker", "type": "text", "fontFamily": "Inter",
                "text": "Breaking <c=#00ff00>news</c>\nSecond line", "scroll": "horizontal", "speed": 600.0,
                "band": true, "bandColor": "#0000ff",
                "props": {"x": 0.0, "y": 0.0, "width": 1920.0, "height": 120.0, "fontSize": 60.0, "fill": [1.0, 0.0, 0.0, 1.0]}
            }]
        }))
        .unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let fps = project.frame_rate.as_f64();
        let seen = |f: &Frame| f.data.chunks(4).filter(|p| p[3] > 200).count();
        let count = |f: &Frame, want: [u8; 3]| {
            f.data
                .chunks(4)
                .filter(|p| p[3] == 255 && p[..3] == want)
                .count()
        };
        // At the take it starts beyond the right edge.
        assert_eq!(
            seen(&r.render_template(&project, &t, "main", 0.0, &Default::default())),
            0
        );
        // Two seconds later: on screen, with its band and its green part.
        let f = r.render_template(&project, &t, "main", 2.0 * fps, &Default::default());
        assert!(count(&f, [0, 0, 255]) > 1000, "band");
        assert!(count(&f, [0, 255, 0]) > 200, "rich text");
        assert!(count(&f, [255, 0, 0]) > 200, "text");
    }

    #[test]
    fn styled_parts_of_a_text_have_their_colour() {
        let project = sample::demo_project();
        let t: cg_core::Template = serde_json::from_value(serde_json::json!({
            "id": "rt", "name": "T", "duration": 10.0, "markers": [], "zone": "",
            "fields": [{"key": "v", "label": "V", "kind": "text", "default": ""}],
            "layers": [{
                "id": "t", "name": "Text", "type": "text", "fontFamily": "Inter",
                "text": "MMMM <c=#00ff00>MMMM</c> {{v}}",
                "props": {"x": 0.0, "y": 0.0, "width": 1600.0, "height": 200.0, "fontSize": 120.0, "fill": [1.0, 0.0, 0.0, 1.0]}
            }]
        }))
        .unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let count = |f: &Frame, want: [u8; 3]| {
            f.data
                .chunks(4)
                .filter(|p| p[3] == 255 && p[..3] == want)
                .count()
        };
        // The operator's value is styled too (a typed field).
        let data: Data = [("v".to_string(), "<c=#0000ff>MMMM</c>".to_string())].into();
        let f = r.render_template(&project, &t, "main", 5.0, &data);
        assert!(count(&f, [255, 0, 0]) > 500, "the text's own colour");
        assert!(count(&f, [0, 255, 0]) > 500, "the green part");
        assert!(count(&f, [0, 0, 255]) > 500, "the operator's blue part");
    }

    #[test]
    fn table_rows_take_their_colour_from_the_data() {
        let project = sample::demo_project();
        let t: cg_core::Template = serde_json::from_value(serde_json::json!({
            "id": "tt", "name": "T", "duration": 10.0, "markers": [], "fields": [], "zone": "",
            "layers": [{
                "id": "tb", "name": "Table", "type": "table",
                "grid": [["team", "color"], ["A", "#00ff00"], ["B", "#0000ff"]],
                "cols": [{"src": "team", "label": "Team", "pct": 100.0}],
                "title": {"on": false, "text": ""}, "headerRow": false, "showRank": false,
                "cascade": {"on": false}, "rowH": 100.0, "gap": 0.0,
                "rowFrom": "color", "rowPaint": "bg",
                "props": {"x": 0.0, "y": 0.0, "width": 800.0, "height": 400.0}
            }]
        }))
        .unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let f = r.render_template(&project, &t, "main", 5.0, &Default::default());
        let w = f.width as usize;
        let px = |x: usize, y: usize| &f.data[(y * w + x) * 4..(y * w + x) * 4 + 3];
        // Far right of each row (no text there): the row's own colour.
        assert_eq!(px(780, 50), [0, 255, 0], "first row green");
        assert_eq!(px(780, 150), [0, 0, 255], "second row blue");
    }

    #[test]
    fn widgets_render_and_scale() {
        let project = sample::demo_project();
        let mut state = ChannelState::default();
        for (layer, id) in [
            (30, "clock"),
            (31, "analog-clock"),
            (40, "ticker"),
            (50, "credits"),
            (60, "poll"),
            (70, "qr-code"),
            (80, "countdown"),
        ] {
            state
                .apply(
                    &project,
                    Command::Take {
                        zone: None,
                        layer: Some(layer),
                        template_id: id.into(),
                        data: None,
                        rundown_id: None,
                    },
                )
                .unwrap();
        }
        for _ in 0..200 {
            state.tick(1.0);
        }
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        // Half-size channel: the project is scaled down.
        let f = r.render_channel(
            &project,
            &state,
            OutputFormat {
                width: 960,
                height: 540,
            },
            "UTC",
        );
        assert_eq!((f.width, f.height), (960, 540));
        if let Ok(path) = std::env::var("CG_DUMP_WIDGETS") {
            std::fs::write(path, f.to_png()).unwrap();
        }
        let alpha = |x: usize, y: usize| f.data[(y * 960 + x) * 4 + 3];
        // Ticker bar along the bottom (y 990..1050 -> 495..525), under the credits shade.
        assert!(alpha(600, 510) > 240);
        // Clock plate top right (1640..1840 x 60..124 -> 820..920 x 30..62).
        assert!(alpha(825, 33) > 240);
    }

    #[test]
    fn renders_demo_lower_third() {
        let project = sample::demo_project();
        let mut state = ChannelState::default();
        state
            .apply(
                &project,
                Command::Take {
                    zone: None,
                    layer: Some(1),
                    template_id: "lower-third".into(),
                    data: None,
                    rundown_id: None,
                },
            )
            .unwrap();
        for _ in 0..40 {
            state.tick(1.0);
        }
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let f = r.render_channel(&project, &state, OUT, "UTC");
        assert_eq!(f.data.len(), 1920 * 1080 * 4);
        // The red accent bar at (160..174, 820..970) must be opaque.
        let px = |x: usize, y: usize| &f.data[(y * 1920 + x) * 4..(y * 1920 + x) * 4 + 4];
        assert_eq!(px(165, 900)[3], 255);
        assert!(px(165, 900)[0] > 200);
        // Outside the graphic stays transparent.
        assert_eq!(px(10, 10)[3], 0);
        if let Ok(path) = std::env::var("CG_DUMP_PNG") {
            std::fs::write(path, f.to_png()).unwrap();
        }
    }

    #[test]
    fn a_paused_lottie_freezes_while_the_template_plays_on() {
        let mut project = sample::demo_project();
        // A Lottie group whose square moves 0 → 1000 px over the 100 frames of the In.
        let t: cg_core::Template = serde_json::from_value(serde_json::json!({
            "id": "lot", "name": "T", "duration": 100.0, "markers": [], "zone": "", "fields": [],
            "layers": [
                {"id": "g", "name": "Anim", "type": "group", "lottie": "/assets/a.json",
                 "props": {"x": 0.0, "y": 0.0, "width": 1920.0, "height": 1080.0}},
                {"id": "r", "name": "Square", "type": "rect", "parent": "g",
                 "props": {"y": 0.0, "width": 100.0, "height": 100.0, "fill": [1.0, 0.0, 0.0, 1.0]},
                 "tracks": {"x": [
                     {"frame": 0.0, "value": 0.0, "easing": {"type": "linear"}},
                     {"frame": 100.0, "value": 1000.0, "easing": {"type": "linear"}}
                 ]}}
            ]
        }))
        .unwrap();
        project.templates.push(t);
        // Taken in the full-screen zone.
        let mut state = ChannelState::default();
        let take = Command::Take {
            zone: None,
            layer: Some(50),
            template_id: "lot".into(),
            data: None,
            rundown_id: None,
        };
        state.apply(&project, take).unwrap();
        for _ in 0..50 {
            state.tick(1.0);
        }
        let pause = Command::Widget {
            zone: None,
            layer: Some(50),
            target: "Anim".into(),
            control: cg_core::WidgetControl::Pause,
        };
        state.apply(&project, pause).unwrap();
        for _ in 0..30 {
            state.tick(1.0);
        }
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let f = r.render_channel(&project, &state, OUT, "UTC");
        let alpha = |x: usize, y: usize| f.data[(y * 1920 + x) * 4 + 3];
        // Frozen at frame 50 (x = 500) while the template is at frame 80.
        assert_eq!(alpha(550, 50), 255);
        assert_eq!(alpha(850, 50), 0);
    }
}
