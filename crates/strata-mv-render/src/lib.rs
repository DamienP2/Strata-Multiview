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
pub mod scopes;
pub mod sequence;
mod shadow;
pub mod slate;
mod sprite;
mod text;
mod wid;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use strata_mv_core::anim::{interpolate_text, resolve_layer, Data, ResolvedLayer};
use strata_mv_core::widgets::{self, clock_text, scroll_positions, TimeCtx};
use strata_mv_core::{
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

impl strata_mv_core::playout::TextWidth for TextWidths {
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
            None => strata_mv_core::playout::EstimatedWidth.width(
                family,
                weight,
                text,
                size,
                letter_spacing,
            ),
        }
    }
}

pub use cache::FrameCache;
pub use text::{fonts_changed, FontFamily, FontStore, BUNDLED_FONTS};

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

/// The pictures of the live video inputs (strata-mv-server), for the Video widget.
pub trait LiveInputs: Send + Sync {
    /// The latest picture of an input (premultiplied RGBA), if it has one.
    fn picture(&self, id: &str) -> Option<Arc<Pixmap>>;
    /// Its name, format, tally, UMD and audio levels (the Source widget).
    fn source(&self, _id: &str) -> Option<strata_mv_core::signal::SourceView> {
        None
    }
    /// PTP, NTP and the alarm journal (the Status and Alarms widgets).
    fn system(&self) -> strata_mv_core::signal::SystemView {
        Default::default()
    }
    /// The trace of a scope of an input's newest picture (made once per
    /// picture by the server, shared by every output and page).
    fn scope(&self, id: &str, r: &scopes::ScopeRequest) -> Option<Arc<Pixmap>> {
        self.picture(id)
            .and_then(|p| scopes::trace(&p, r))
            .map(Arc::new)
    }
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
    /// A frame-sized picture for the multiviews drawn over what moves
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
    /// Planning: the multiview on air the steps belong to.
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
    /// Inside a cross-fade (the multiview drawn twice: no sprites).
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
    layers: HashMap<String, (strata_mv_core::ValueChange, f32)>,
    old: bool,
}

/// Where a layer showing a changing value is drawn (`h` its height), or
/// nowhere (turned edge on).
fn value_tf(
    tf: Transform,
    kind: strata_mv_core::ValueChange,
    e: f32,
    old: bool,
    h: f32,
) -> Option<Transform> {
    match kind {
        strata_mv_core::ValueChange::Slide => {
            Some(tf.pre_translate(0.0, if old { -e * h } else { (1.0 - e) * h }))
        }
        strata_mv_core::ValueChange::Flip => {
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

/// A multiview on air, prepared for drawing only when it is drawn (or planned).
struct OnAir<'p> {
    layer: u32,
    item: &'p strata_mv_core::PlayoutItem,
    /// Below 1: the multiview replaced in a « Fade » zone, fading out under the new one.
    alpha: f32,
    template: &'p Template,
    /// Laid out for the format, with the operator's settings.
    set: std::cell::OnceCell<std::borrow::Cow<'p, Template>>,
    /// What the multiview's state is drawn from (`sprite`), hashed.
    key: std::cell::OnceCell<u64>,
    /// Its data as shown now (values counting on their way).
    data: std::borrow::Cow<'p, Data>,
}

/// A multiview on air, hashed (its template, data, widget states and clocks:
/// its timeline frame and age aside, which the layers' values and motion
/// keys carry).
fn item_key(item: &strata_mv_core::PlayoutItem) -> u64 {
    let mut k = sprite::Key::new();
    k.add(&item.template_id).add(&item.data);
    k.add(&serde_json::to_string(&item.widgets).unwrap_or_default());
    k.add(&serde_json::to_string(&item.clocks).unwrap_or_default());
    k.finish()
}

/// `t` laid out for `format`, with the settings in `data`.
/// A template as drawn: laid out in `format`, with the settings of `data`
/// and, the lowest one on air (`backdrop`: the format's size), the
/// multiview's background under it.
fn prepared<'p>(
    t: &'p Template,
    format: &str,
    data: &Data,
    backdrop: Option<(u32, u32)>,
) -> std::borrow::Cow<'p, Template> {
    use std::borrow::Cow;
    let t = laid(t, format, data);
    match backdrop {
        Some((w, h)) => Cow::Owned(t.with_backdrop(w, h)),
        None => t,
    }
}

fn laid<'p>(t: &'p Template, format: &str, data: &Data) -> std::borrow::Cow<'p, Template> {
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
    /// The multiview on air (widget clocks the operator controls), and the
    /// project frame rate its ages are counted in.
    item: Option<&'a strata_mv_core::PlayoutItem>,
    fps: f64,
    /// The project's language (texts without their own).
    lang: &'a str,
    /// The multiview on air: its index among those on air and its state
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
            item: self.item,
            fps: self.fps,
            lang: self.lang,
            item_index: self.item_index,
            item_key: self.item_key,
        }
    }

    /// Time inputs of one layer: its own clock when the operator controls it.
    fn time_for(&self, layer: &strata_mv_core::Layer) -> TimeCtx<'_> {
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

    /// What the Source widget shows of a live input besides its picture.
    pub fn live_source(&self, id: &str) -> Option<strata_mv_core::signal::SourceView> {
        self.live.as_ref()?.source(id)
    }

    /// PTP, NTP and the alarm journal.
    pub fn live_system(&self) -> strata_mv_core::signal::SystemView {
        self.live.as_ref().map(|l| l.system()).unwrap_or_default()
    }

    /// The trace of a scope of a live input.
    pub fn live_scope(&self, id: &str, r: &scopes::ScopeRequest) -> Option<Arc<Pixmap>> {
        self.live.as_ref()?.scope(id, r)
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
        // The multiviews on air, lowest first, laid out for this format with
        // the settings the Operator was given (mask, font, logo…) when drawn.
        // A multiview replaced in a « Fade » zone comes just under the new one.
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
                    key: Default::default(),
                    data: strata_mv_core::playout::shown_data(item),
                })
            })
            .collect();
        let leaving = on_air.iter().any(|a| a.alpha < 1.0);
        macro_rules! ctx {
            ($i:expr) => {{
                let a = &on_air[$i];
                let t = a.set.get_or_init(|| {
                    prepared(a.template, format, &a.data, ($i == 0).then_some((dw, dh)))
                });
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
                    item: Some(a.item),
                    fps,
                    lang: &project.language,
                    item_index: $i,
                    item_key: *a.key.get_or_init(|| item_key(a.item)),
                }
            }};
        }
        fn fading(item: &strata_mv_core::PlayoutItem) -> Option<(f32, &Data)> {
            item.fade
                .as_ref()
                .filter(|f| f.opacity() > 0.0)
                .map(|f| (f.opacity(), &f.data))
        }
        // Planned: what is the same as in the frames kept, and which
        // multiviews are over the last one that moves (drawn apart, cache::top_item;
        // drawn whole too, so that frames are the same either way).
        let mut planned = None;
        // A multiview fading out: every frame drawn whole (the kept steps follow one multiview per layer).
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
                    // A cross-fade, values moving: the multiview drawn whole, as one step.
                    self.plan_step(cache::Motion::Moving);
                } else if let Some(ops) = kept.still_steps(&scene, i) {
                    // The same multiview as in the frame before, nothing of it moving.
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
        // The multiviews over what moves, drawn apart (when not kept).
        let mut over: Option<Pixmap> = None;
        // The step after the last one of each multiview.
        let mut item_end = vec![0; on_air.len()];
        for (i, op) in self.steps.ops.iter().enumerate() {
            item_end[op.item] = i + 1;
        }
        for i in 0..on_air.len() {
            // Every step of this multiview is in the picture already.
            if item_end[i] > self.steps.at && item_end[i] <= self.steps.skip {
                self.steps.at = item_end[i];
                continue;
            }
            let apart = top.is_some_and(|t| i >= t);
            if apart && top_kept.is_some() {
                // In the kept picture of the multiviews over what moves.
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
                // The multiview going in a « Fade » zone: drawn whole, then laid at its opacity.
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
            // multiview with the new values, the layers showing them moved in
            // their box, then those layers with the old values.
            let turning: Vec<&strata_mv_core::playout::Change> = item
                .changes
                .iter()
                .filter(|c| {
                    matches!(
                        c.kind,
                        strata_mv_core::ValueChange::Slide | strata_mv_core::ValueChange::Flip
                    )
                })
                .collect();
            if !turning.is_empty() {
                let mut layers = HashMap::new();
                for l in &ctx.template.layers {
                    if matches!(l.kind, LayerKind::Group { .. }) {
                        continue;
                    }
                    let json = strata_mv_core::playout::layer_json(l);
                    if let Some(c) = turning.iter().find(|c| {
                        if c.layer.is_empty() {
                            strata_mv_core::playout::layer_shows(&json, &c.key)
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
                let t_old = prepared(
                    on_air[i].template,
                    format,
                    &old_data,
                    (i == 0).then_some((dw, dh)),
                );
                let old_ctx = Ctx {
                    template: &t_old,
                    data: &old_data,
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
                // A cross-fade (Update, data set row): the multiview with
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
                    let t_old = prepared(
                        on_air[i].template,
                        format,
                        old_data,
                        (i == 0).then_some((dw, dh)),
                    );
                    let old_ctx = Ctx {
                        template: &t_old,
                        data: old_data,
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
        // The multiviews over what moves, laid over the frame.
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

    /// A template at `frame`, full size (tests).
    #[cfg(test)]
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
        let template = template.with_settings(data).with_backdrop(dw, dh);
        let template = &template;
        let mut pm = Pixmap::new(width, height).expect("valid size");
        let time = TimeCtx {
            now_ms: now_ms(),
            age_secs: frame / project.frame_rate.as_f64().max(1.0),
            default_tz: "",
            fps: project.frame_rate.as_f64().max(1.0),
        };
        let ctx = Ctx {
            template,
            frame,
            data,
            time,
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
            if let Some(runs) = strata_mv_core::rich::layer_runs(ctx.template, layer, ctx.data) {
                r.text = Some(runs.into_iter().map(|r| r.text).collect());
            }
            // Its repeated movement, on its time on air.
            if let Some(m) = &layer.motion {
                let age = ctx.time_for(layer).age_secs;
                strata_mv_core::anim::apply_motion(&mut r.values, m, ctx.frame, ctx.fps, age);
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
        if strata_mv_core::wid::widget_id(&r.layer.kind).is_some() {
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
            | LayerKind::Polygon { .. } => Motion::Still,
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

    /// What the step of `r` is drawn from, hashed (`sprite`): the multiview's
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
                LayerKind::Ticker { .. }
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
        for i in strata_mv_core::rich::layer_images(ctx.template, r.layer, ctx.data) {
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
                    if let Some(runs) = strata_mv_core::rich::layer_runs(ctx.template, l, ctx.data)
                    {
                        c.text = Some(runs.into_iter().map(|r| r.text).collect());
                    }
                    for (p, v) in &c.values {
                        k.add(p);
                        value(&mut k, v);
                    }
                    k.add(&c.text).add(&c.image_src);
                    for i in strata_mv_core::rich::layer_images(ctx.template, l, ctx.data) {
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
            fps: ctx.fps,
            state,
            lang: ctx.lang,
            runs: strata_mv_core::rich::layer_runs(ctx.template, r.layer, ctx.data),
            inline: strata_mv_core::rich::layer_images(ctx.template, r.layer, ctx.data),
            moving,
        };
        f(&mut d)
    }

    /// A layer, inside its frame (`Layer::panel`): the fill under it, the
    /// border over it.
    fn draw_layer(
        &mut self,
        pm: &mut Pixmap,
        ctx: &Ctx,
        r: &ResolvedLayer,
        tf: Transform,
        opacity: f32,
        mask: Option<&Mask>,
    ) {
        let panel = r.layer.panel.as_ref();
        let (w, h) = (r.num(Prop::Width) as f32, r.num(Prop::Height) as f32);
        let hex = |s: &str| strata_mv_core::Color::from_hex(s).filter(|c| c.0[3] > 0.0);
        if let Some((p, c)) = panel.and_then(|p| Some((p, hex(&p.fill)?))) {
            if let Some(path) = rounded_rect(w, h, p.radius.max(0.0)) {
                pm.fill_path(&path, &paint_for(c, opacity), FillRule::Winding, tf, mask);
            }
        }
        self.draw_layer_content(pm, ctx, r, tf, opacity, mask);
        if let Some((p, c)) = panel
            .filter(|p| p.border > 0.0)
            .and_then(|p| Some((p, hex(&p.border_color)?)))
        {
            // Inside the box: its outer edge on the box's.
            let b = p.border.min(w.min(h) / 2.0);
            let inner = rounded_rect(w - b, h - b, (p.radius - b / 2.0).max(0.0));
            if let Some(path) = inner {
                let stroke = Stroke {
                    width: b,
                    ..Default::default()
                };
                pm.stroke_path(
                    &path,
                    &paint_for(c, opacity),
                    &stroke,
                    tf.pre_translate(b / 2.0, b / 2.0),
                    mask,
                );
            }
        }
    }

    fn draw_layer_content(
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
        if strata_mv_core::wid::widget_id(&r.layer.kind).is_some()
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
        let mask = if matches!(kind, LayerKind::Clock { .. } | LayerKind::Ticker { .. }) {
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
            LayerKind::Image { fit, .. } => {
                if let Some(src) = r.image_src.clone() {
                    self.draw_image(pm, &src, *fit, w, h, tf, opacity, mask);
                }
            }
            LayerKind::Polygon { .. }
            | LayerKind::Source { .. }
            | LayerKind::AudioMeter { .. }
            | LayerKind::Loudness { .. }
            | LayerKind::Timecode { .. }
            | LayerKind::Status { .. }
            | LayerKind::AlarmLog { .. }
            | LayerKind::Scope { .. } => {}
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
            decoration: strata_mv_core::TextDecoration::None,
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
fn loops(t: &Template, layer: &strata_mv_core::Layer) -> bool {
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
fn geometry_moving(t: &Template, layer: &strata_mv_core::Layer, frame: f64) -> bool {
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
                    && strata_mv_core::anim::sample_track(k, frame)
                        != strata_mv_core::anim::sample_track(k, frame + 1.0)
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
    use super::*;
    use strata_mv_core::{sample, Command};

    const OUT: OutputFormat = OutputFormat {
        width: 1920,
        height: 1080,
    };

    #[test]
    fn only_the_zone_shows_on_air() {
        let mut project = sample::demo_project();
        // The credits (a full-frame shade) taken in a top-left zone.
        project.zones = vec![strata_mv_core::Zone {
            id: "top-left".into(),
            name: "Top left".into(),
            layer: 50,
            color: String::new(),
            transition: strata_mv_core::ZoneTransition::Cut,
            areas: Default::default(),
        }];
        let mut state = ChannelState::default();
        state
            .apply(
                &project,
                Command::Take {
                    zone: None,
                    layer: Some(50),
                    template_id: "full-frame".into(),
                },
            )
            .unwrap();
        for _ in 0..60 {
            state.tick(1.0);
        }
        let mut r = Renderer::new(std::env::temp_dir().join("strata-mv-render-test-assets"));
        let f = r.render_channel(&project, &state, OUT, "UTC");
        let alpha = |x: usize, y: usize| f.data[(y * 1920 + x) * 4 + 3];
        // Top-left zone: 4 %..28 % x 5 %..19 % of the frame.
        assert!(alpha(200, 100) > 0);
        assert_eq!(alpha(1500, 800), 0);
    }

    #[test]
    fn a_layer_shows_only_within_its_span() {
        let mut project = sample::demo_project();
        project.zones = vec![strata_mv_core::Zone {
            id: "top-left".into(),
            name: "Top left".into(),
            layer: 50,
            color: String::new(),
            transition: strata_mv_core::ZoneTransition::Cut,
            areas: Default::default(),
        }];
        // Every layer of the plate shown from frame 0 to 8 only.
        let t = project
            .templates
            .iter_mut()
            .find(|t| t.id == "full-frame")
            .unwrap();
        for l in &mut t.layers {
            l.span = Some(strata_mv_core::Span { from: 0.0, to: 8.0 });
        }
        let mut state = ChannelState::default();
        state
            .apply(
                &project,
                Command::Take {
                    zone: None,
                    layer: Some(50),
                    template_id: "full-frame".into(),
                },
            )
            .unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("strata-mv-render-test-assets"));
        let px = |f: &Frame| f.data[(100 * 1920 + 200) * 4..(100 * 1920 + 200) * 4 + 4].to_vec();
        // The multiview's background (the default dark grey) where nothing is.
        let backdrop = vec![0x14, 0x14, 0x16, 255];
        for _ in 0..5 {
            state.tick(1.0);
        }
        let f = r.render_channel(&project, &state, OUT, "UTC");
        assert_ne!(px(&f), backdrop, "shown at frame 5");
        // Held at its pause (frame 12): gone.
        for _ in 0..20 {
            state.tick(1.0);
        }
        let f = r.render_channel(&project, &state, OUT, "UTC");
        assert_eq!(px(&f), backdrop, "gone after frame 8");
    }

    #[test]
    fn a_fade_zone_draws_the_graphic_going_under_the_new_one() {
        let mut project = sample::demo_project();
        let take = |_name: &str| Command::Take {
            zone: Some("lower-third".into()),
            layer: None,
            template_id: "lower-third".into(),
        };
        let mut state = ChannelState::default();
        state.apply(&project, take("Marie Curie")).unwrap();
        for _ in 0..200 {
            state.tick(1.0);
        }
        // The same take in a Cut zone: the new multiview alone.
        let mut cut = state.clone();
        for z in &mut project.zones {
            z.transition = strata_mv_core::ZoneTransition::Cut;
        }
        cut.apply(&project, take("Albert Einstein")).unwrap();
        for z in &mut project.zones {
            z.transition = strata_mv_core::ZoneTransition::Fade;
        }
        state.apply(&project, take("Albert Einstein")).unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("strata-mv-render-test-assets"));
        let alone = r.render_channel(&project, &cut, OUT, "UTC");
        let both = r.render_channel(&project, &state, OUT, "UTC");
        assert_ne!(alone.data, both.data, "the multiview going still shows");
        // Faded out: only the new multiview.
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
        let dir = std::env::temp_dir().join("strata-mv-render-svg-test");
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
    fn widgets_render_and_scale() {
        let project = sample::demo_project();
        let mut state = ChannelState::default();
        for (layer, id) in [
            (30, "clock"),
            (40, "ticker"),
            (50, "full-frame"),
            (80, "countdown"),
        ] {
            state
                .apply(
                    &project,
                    Command::Take {
                        zone: None,
                        layer: Some(layer),
                        template_id: id.into(),
                    },
                )
                .unwrap();
        }
        for _ in 0..200 {
            state.tick(1.0);
        }
        let mut r = Renderer::new(std::env::temp_dir().join("strata-mv-render-test-assets"));
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
        if let Ok(path) = std::env::var("STRATA_MV_DUMP_WIDGETS") {
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
                },
            )
            .unwrap();
        for _ in 0..40 {
            state.tick(1.0);
        }
        let mut r = Renderer::new(std::env::temp_dir().join("strata-mv-render-test-assets"));
        let f = r.render_channel(&project, &state, OUT, "UTC");
        assert_eq!(f.data.len(), 1920 * 1080 * 4);
        // The red accent bar at (160..174, 820..970) must be opaque.
        let px = |x: usize, y: usize| &f.data[(y * 1920 + x) * 4..(y * 1920 + x) * 4 + 4];
        assert_eq!(px(165, 900)[3], 255);
        assert!(px(165, 900)[0] > 200);
        // Outside the multiview: the multiview's background (the default dark grey).
        assert_eq!(px(10, 10), &[0x14, 0x14, 0x16, 255]);
        if let Ok(path) = std::env::var("STRATA_MV_DUMP_PNG") {
            std::fs::write(path, f.to_png()).unwrap();
        }
    }
}
