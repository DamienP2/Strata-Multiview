//! After Effects import through Lottie (Bodymovin JSON).
//!
//! After Effects projects (`.aep`) and Motion Graphics templates (`.mogrt`)
//! are closed binary formats; the open route out of After Effects is the
//! Bodymovin / LottieFiles exporter, whose JSON describes compositions,
//! layers, transforms and keyframes. This module turns such a file into a
//! Strata template:
//!
//! - shape layers (rectangles, ellipses with fill / stroke), solids, texts,
//!   images, null objects and precompositions;
//! - layer transforms (position, anchor point, scale, rotation, opacity) and
//!   their keyframes with After Effects easing (bezier influence / speed);
//! - parenting and precomps become groups;
//! - in / out points become opacity holds;
//! - every text becomes an operator data field;
//! - composition markers named "pause" / "on air" / "out" become playout
//!   markers (otherwise the longest still moment is used as the on-air point).
//!
//! Not converted (listed in the warnings): free paths drawn with the pen,
//! gradients, masks, track mattes, effects, text animators, 3D, expressions.

use std::collections::{BTreeMap, HashSet};

use serde_json::Value as Json;

use crate::easing::Easing;
use crate::model::{
    new_id, Color, Field, FieldKind, ImageFit, Keyframe, Layer, LayerKind, Marker, MarkerKind,
    Prop, Template, TextAlign, Value, VerticalAlign,
};

/// Target of the conversion: the project's size and frame rate.
#[derive(Debug, Clone)]
pub struct Options {
    pub width: f64,
    pub height: f64,
    pub fps: f64,
    pub zone: String,
}

/// An image the template refers to, to be stored as `/assets/{file}`.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageAsset {
    pub file: String,
    pub source: ImageSource,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImageSource {
    /// `data:image/png;base64,…` embedded in the JSON.
    DataUri(String),
    /// Path next to the JSON (`images/img_0.png`), found in an archive.
    Path(String),
}

#[derive(Debug, Clone)]
pub struct Imported {
    pub template: Template,
    pub images: Vec<ImageAsset>,
    pub warnings: Vec<String>,
}

/// Converts a Lottie / Bodymovin animation.
pub fn import(json: &Json, opts: &Options) -> Result<Imported, String> {
    let layers = json
        .get("layers")
        .and_then(Json::as_array)
        .ok_or("not a Lottie / Bodymovin file (no layers)")?;
    let fr = num(json.get("fr")).filter(|f| *f > 0.0).unwrap_or(25.0);
    let ip = num(json.get("ip")).unwrap_or(0.0);
    let op = num(json.get("op")).unwrap_or(ip + fr * 5.0);
    let w = num(json.get("w")).unwrap_or(opts.width).max(1.0);
    let h = num(json.get("h")).unwrap_or(opts.height).max(1.0);
    let name = json
        .get("nm")
        .and_then(Json::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("After Effects import")
        .to_string();
    let fps = if opts.fps > 0.0 { opts.fps } else { fr };
    let duration = ((op - ip) * fps / fr).round().max(1.0);

    let mut cx = Converter {
        root_ip: ip,
        scale_time: fps / fr,
        duration,
        assets: json
            .get("assets")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default(),
        fonts: json
            .pointer("/fonts/list")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default(),
        out: Vec::new(),
        fields: Vec::new(),
        images: Vec::new(),
        warnings: BTreeMap::new(),
        depth: 0,
    };

    // Fit the composition into the project frame when sizes differ.
    let root = if (w - opts.width).abs() > 0.5 || (h - opts.height).abs() > 0.5 {
        let s = (opts.width / w).min(opts.height / h);
        let id = new_id();
        let mut g = layer(
            &id,
            &name,
            None,
            LayerKind::Group {
                clip: false,
                lottie: None,
            },
        );
        set(&mut g, Prop::X, ((opts.width - w * s) / 2.0).round());
        set(&mut g, Prop::Y, ((opts.height - h * s) / 2.0).round());
        set(&mut g, Prop::Width, w);
        set(&mut g, Prop::Height, h);
        set(&mut g, Prop::AnchorX, 0.0);
        set(&mut g, Prop::AnchorY, 0.0);
        set(&mut g, Prop::ScaleX, s);
        set(&mut g, Prop::ScaleY, s);
        cx.out.push(g);
        Some(id)
    } else {
        None
    };
    let time = Time {
        scale: 1.0,
        offset: 0.0,
    };
    cx.comp(layers, root.as_deref(), (0.0, 0.0), time);

    let mut template = Template {
        id: new_id(),
        name,
        duration,
        markers: Vec::new(),
        fields: cx.fields,
        layers: cx.out,
        zone: opts.zone.clone(),
        logic: Default::default(),
        triggers: Default::default(),
        dataflow: None,
        datasets: vec![],
        revision: 0,
        marker_model: 0,
    };
    template.markers = markers(json, &template, ip, fps / fr);
    let mut warnings: Vec<String> = cx
        .warnings
        .into_iter()
        .map(|(w, n)| if n > 1 { format!("{w} (×{n})") } else { w })
        .collect();
    if template.layers.is_empty() {
        warnings.push("nothing could be converted".into());
    }
    Ok(Imported {
        template,
        images: cx.images,
        warnings,
    })
}

// ------------------------------------------------------------------ time ----

/// Maps a frame of a (pre)composition to the root composition's frames.
#[derive(Clone, Copy)]
struct Time {
    scale: f64,
    offset: f64,
}

impl Time {
    fn root(&self, t: f64) -> f64 {
        t * self.scale + self.offset
    }
}

// --------------------------------------------------------- animated values --

#[derive(Debug, Clone)]
struct Key {
    t: f64,
    v: Vec<f64>,
    easing: Easing,
}

#[derive(Debug, Clone)]
enum Anim {
    Static(Vec<f64>),
    Keys(Vec<Key>),
}

impl Anim {
    fn parse(p: Option<&Json>) -> Option<Anim> {
        let p = p?;
        let k = p.get("k")?;
        let animated = p.get("a").and_then(Json::as_i64) == Some(1)
            || k.as_array()
                .and_then(|a| a.first())
                .is_some_and(|f| f.get("t").is_some());
        if !animated {
            return Some(Anim::Static(vector(k)));
        }
        let mut keys: Vec<Key> = Vec::new();
        let mut prev_end: Option<Vec<f64>> = None;
        for kf in k.as_array()? {
            let t = num(kf.get("t"))?;
            let v = kf
                .get("s")
                .map(vector)
                .or_else(|| prev_end.clone())
                .or_else(|| keys.last().map(|k| k.v.clone()))?;
            prev_end = kf.get("e").map(vector);
            let easing = if kf.get("h").and_then(Json::as_i64) == Some(1) {
                Easing::Hold
            } else {
                match (kf.get("o"), kf.get("i")) {
                    (Some(o), Some(i)) => Easing::Bezier {
                        x1: first(o.get("x")).unwrap_or(0.0),
                        y1: first(o.get("y")).unwrap_or(0.0),
                        x2: first(i.get("x")).unwrap_or(1.0),
                        y2: first(i.get("y")).unwrap_or(1.0),
                    },
                    _ => Easing::Linear,
                }
            };
            keys.push(Key { t, v, easing });
        }
        if keys.len() == 1 {
            return Some(Anim::Static(keys.remove(0).v));
        }
        (!keys.is_empty()).then_some(Anim::Keys(keys))
    }

    fn first(&self) -> &[f64] {
        match self {
            Anim::Static(v) => v,
            Anim::Keys(k) => &k[0].v,
        }
    }

    fn at(&self, i: usize) -> f64 {
        self.first().get(i).copied().unwrap_or(0.0)
    }
}

fn vector(k: &Json) -> Vec<f64> {
    match k {
        Json::Array(a) => a.iter().filter_map(Json::as_f64).collect(),
        Json::Number(n) => vec![n.as_f64().unwrap_or(0.0)],
        _ => Vec::new(),
    }
}

fn num(v: Option<&Json>) -> Option<f64> {
    v.and_then(Json::as_f64)
}

fn first(v: Option<&Json>) -> Option<f64> {
    match v? {
        Json::Array(a) => a.first().and_then(Json::as_f64),
        j => j.as_f64(),
    }
}

// ----------------------------------------------------------------- layers ---

fn layer(id: &str, name: &str, parent: Option<&str>, kind: LayerKind) -> Layer {
    Layer {
        id: id.to_string(),
        name: name.to_string(),
        parent: parent.map(str::to_string),
        kind,
        visible: true,
        locked: false,
        manual_start: false,
        start_on: None,
        start_trigger: String::new(),
        scale_lock: false,
        lock_ratio: false,
        drop_shadow: false,
        timeline: None,
        props: BTreeMap::new(),
        tracks: BTreeMap::new(),
        bindings: BTreeMap::new(),
        setting_bindings: BTreeMap::new(),
        overrides: BTreeMap::new(),
        live_changes: BTreeMap::new(),
        span: None,
        motion: None,
    }
}

fn set(l: &mut Layer, p: Prop, v: f64) {
    l.props.insert(p, Value::Number(v));
}

/// A rectangle in a layer's own coordinates.
#[derive(Debug, Clone, Copy)]
struct Box2 {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// A rectangle or ellipse found in a shape layer.
struct Primitive {
    ellipse: bool,
    center: (f64, f64),
    size: Anim,
    radius: f64,
    fill: Option<(Anim, f64)>,
    stroke: Option<(Anim, f64, f64)>,
}

impl Primitive {
    fn bounds(&self) -> Box2 {
        let (w, h) = (self.size.at(0), self.size.at(1));
        Box2 {
            x: self.center.0 - w / 2.0,
            y: self.center.1 - h / 2.0,
            w,
            h,
        }
    }
}

struct Converter {
    root_ip: f64,
    scale_time: f64,
    duration: f64,
    assets: Vec<Json>,
    fonts: Vec<Json>,
    out: Vec<Layer>,
    fields: Vec<Field>,
    images: Vec<ImageAsset>,
    warnings: BTreeMap<String, usize>,
    depth: usize,
}

impl Converter {
    fn warn(&mut self, w: impl Into<String>) {
        *self.warnings.entry(w.into()).or_default() += 1;
    }

    /// Frame of the template for a frame of the (pre)composition.
    fn frame(&self, time: Time, t: f64) -> f64 {
        ((time.root(t) - self.root_ip) * self.scale_time * 1000.0).round() / 1000.0
    }

    /// Converts the layers of one composition. Lottie lists them top first;
    /// Strata paints the first one at the bottom.
    fn comp(&mut self, layers: &[Json], parent: Option<&str>, origin: (f64, f64), time: Time) {
        let ids: HashSet<i64> = layers
            .iter()
            .filter_map(|l| l.get("ind").and_then(Json::as_i64))
            .collect();
        for l in layers.iter().rev() {
            let linked = l
                .get("parent")
                .and_then(Json::as_i64)
                .is_some_and(|p| ids.contains(&p));
            if !linked {
                self.layer(l, layers, parent, origin, time);
            }
        }
    }

    fn layer(
        &mut self,
        l: &Json,
        siblings: &[Json],
        parent: Option<&str>,
        origin: (f64, f64),
        time: Time,
    ) {
        if l.get("hd").and_then(Json::as_bool) == Some(true) {
            return;
        }
        let ty = l.get("ty").and_then(Json::as_i64).unwrap_or(-1);
        let name = l
            .get("nm")
            .and_then(Json::as_str)
            .unwrap_or("Layer")
            .to_string();
        let ind = l.get("ind").and_then(Json::as_i64);
        let children: Vec<&Json> = match ind {
            Some(ind) => siblings
                .iter()
                .filter(|c| c.get("parent").and_then(Json::as_i64) == Some(ind))
                .collect(),
            None => Vec::new(),
        };
        if l.get("masksProperties")
            .and_then(Json::as_array)
            .is_some_and(|m| !m.is_empty())
        {
            self.warn("masks are not converted");
        }
        if l.get("tt").is_some() {
            self.warn("track mattes are not converted");
        }
        if l.get("ef")
            .and_then(Json::as_array)
            .is_some_and(|e| !e.is_empty())
        {
            self.warn("effects are not converted");
        }
        if l.get("ddd").and_then(Json::as_i64) == Some(1) {
            self.warn("3D layers are flattened");
        }

        let id = new_id();
        let ks = l.get("ks");
        let mut primitives = Vec::new();
        let mut text_style = None;
        let (kind, bounds) = match ty {
            // Precomposition.
            0 => {
                let w = num(l.get("w")).unwrap_or(100.0);
                let h = num(l.get("h")).unwrap_or(100.0);
                (
                    LayerKind::Group {
                        clip: false,
                        lottie: None,
                    },
                    Box2 {
                        x: 0.0,
                        y: 0.0,
                        w,
                        h,
                    },
                )
            }
            // Solid.
            1 => {
                let w = num(l.get("sw")).unwrap_or(100.0);
                let h = num(l.get("sh")).unwrap_or(100.0);
                (
                    LayerKind::rect(),
                    Box2 {
                        x: 0.0,
                        y: 0.0,
                        w,
                        h,
                    },
                )
            }
            // Image.
            2 => {
                let (src, w, h) = self.image(l);
                (
                    LayerKind::Image {
                        x: Default::default(),
                        src,
                        fit: ImageFit::Stretch,
                    },
                    Box2 {
                        x: 0.0,
                        y: 0.0,
                        w,
                        h,
                    },
                )
            }
            // Null object: only useful as a parent.
            3 => {
                if children.is_empty() {
                    return;
                }
                (
                    LayerKind::Group {
                        clip: false,
                        lottie: None,
                    },
                    Box2 {
                        x: 0.0,
                        y: 0.0,
                        w: 100.0,
                        h: 100.0,
                    },
                )
            }
            // Shape layer.
            4 => {
                let shapes = l
                    .get("shapes")
                    .and_then(Json::as_array)
                    .cloned()
                    .unwrap_or_default();
                self.shapes(&shapes, (0.0, 0.0), (1.0, 1.0), None, None, &mut primitives);
                if primitives.is_empty() && children.is_empty() {
                    return;
                }
                let b = primitives
                    .iter()
                    .map(Primitive::bounds)
                    .reduce(|a, b| {
                        let x = a.x.min(b.x);
                        let y = a.y.min(b.y);
                        Box2 {
                            x,
                            y,
                            w: (a.x + a.w).max(b.x + b.w) - x,
                            h: (a.y + a.h).max(b.y + b.h) - y,
                        }
                    })
                    .unwrap_or(Box2 {
                        x: 0.0,
                        y: 0.0,
                        w: 100.0,
                        h: 100.0,
                    });
                if primitives.len() == 1 && children.is_empty() {
                    let p = &primitives[0];
                    (
                        if p.ellipse {
                            LayerKind::ellipse()
                        } else {
                            LayerKind::rect()
                        },
                        b,
                    )
                } else {
                    (
                        LayerKind::Group {
                            clip: false,
                            lottie: None,
                        },
                        b,
                    )
                }
            }
            // Text.
            5 => match self.text(l, &name) {
                Some((k, b, style)) => {
                    text_style = Some(style);
                    (k, b)
                }
                None => return,
            },
            6 => return, // audio
            13 => {
                self.warn("cameras are ignored");
                return;
            }
            _ => {
                self.warn(format!("layer type {ty} is not supported"));
                return;
            }
        };
        // A solid, text or image that other layers follow becomes a group
        // holding its own content, so the children move with it.
        let (kind, mut content) =
            if !children.is_empty() && !matches!(kind, LayerKind::Group { .. }) {
                let mut c = layer(&new_id(), &name, Some(&id), kind);
                set(&mut c, Prop::X, 0.0);
                set(&mut c, Prop::Y, 0.0);
                set(&mut c, Prop::Width, bounds.w.max(1.0).round());
                set(&mut c, Prop::Height, bounds.h.max(1.0).round());
                (
                    LayerKind::Group {
                        clip: false,
                        lottie: None,
                    },
                    Some(c),
                )
            } else {
                (kind, None)
            };
        let is_group = matches!(kind, LayerKind::Group { .. });
        let mut out = layer(&id, &name, parent, kind);
        set(&mut out, Prop::Width, bounds.w.max(1.0).round());
        set(&mut out, Prop::Height, bounds.h.max(1.0).round());
        self.transform(&mut out, ks, bounds, origin, time);
        self.in_out(&mut out, l, time);

        let target = content.as_mut().unwrap_or(&mut out);
        match ty {
            1 => {
                let c = l
                    .get("sc")
                    .and_then(Json::as_str)
                    .and_then(Color::from_hex)
                    .unwrap_or(Color::WHITE);
                target.props.insert(Prop::Fill, Value::Color(c));
            }
            5 => {
                if let Some(st) = text_style {
                    set(target, Prop::FontSize, st.size);
                    if st.tracking != 0.0 {
                        // After Effects tracking is in thousandths of an em.
                        set(
                            target,
                            Prop::LetterSpacing,
                            round4(st.tracking * st.size / 1000.0),
                        );
                    }
                    let c = st.color.unwrap_or_else(|| vec![1.0, 1.0, 1.0]);
                    let k = if c.iter().any(|x| *x > 1.0) {
                        255.0
                    } else {
                        1.0
                    };
                    let ch = |i: usize| (c.get(i).copied().unwrap_or(1.0) / k) as f32;
                    target
                        .props
                        .insert(Prop::Fill, Value::Color(Color([ch(0), ch(1), ch(2), 1.0])));
                }
            }
            4 if !is_group => {
                let p = primitives.remove(0);
                self.paint(&mut out, &p, time);
                self.size_tracks(&mut out, &p, ks, origin, time);
            }
            _ => {}
        }
        self.out.push(out);
        if let Some(c) = content {
            self.out.push(c);
        }

        if is_group {
            let inner = (bounds.x, bounds.y);
            // Own content first (bottom), then the precomp, then children.
            for p in primitives {
                let pid = new_id();
                let b = p.bounds();
                let kind = if p.ellipse {
                    LayerKind::ellipse()
                } else {
                    LayerKind::rect()
                };
                let mut c = layer(&pid, &name, Some(&id), kind);
                set(&mut c, Prop::X, (b.x - inner.0).round());
                set(&mut c, Prop::Y, (b.y - inner.1).round());
                set(&mut c, Prop::Width, b.w.max(1.0).round());
                set(&mut c, Prop::Height, b.h.max(1.0).round());
                self.paint(&mut c, &p, time);
                self.out.push(c);
            }
            if ty == 0 {
                self.precomp(l, &id, inner, time);
            }
            for c in children.into_iter().rev() {
                self.layer(c, siblings, Some(&id), inner, time);
            }
        }
    }

    fn precomp(&mut self, l: &Json, id: &str, origin: (f64, f64), time: Time) {
        if self.depth > 16 {
            self.warn("precompositions nested too deeply");
            return;
        }
        let Some(asset) = l
            .get("refId")
            .and_then(Json::as_str)
            .and_then(|r| {
                self.assets
                    .iter()
                    .find(|a| a.get("id").and_then(Json::as_str) == Some(r))
            })
            .cloned()
        else {
            self.warn("missing precomposition");
            return;
        };
        let Some(layers) = asset.get("layers").and_then(Json::as_array) else {
            return;
        };
        if l.get("tm").is_some() {
            self.warn("time remapping is ignored");
        }
        let st = num(l.get("st")).unwrap_or(0.0);
        let sr = num(l.get("sr")).unwrap_or(1.0);
        let inner = Time {
            scale: time.scale * sr,
            offset: time.root(st),
        };
        self.depth += 1;
        self.comp(layers, Some(id), origin, inner);
        self.depth -= 1;
    }

    /// Position, anchor, scale, rotation and opacity. `bounds` is the box in
    /// the layer's own space; `origin` the parent's box corner.
    fn transform(
        &mut self,
        out: &mut Layer,
        ks: Option<&Json>,
        b: Box2,
        origin: (f64, f64),
        time: Time,
    ) {
        let Some(ks) = ks else { return };
        let a = Anim::parse(ks.get("a")).unwrap_or(Anim::Static(vec![0.0, 0.0]));
        if matches!(a, Anim::Keys(_)) {
            self.warn("animated anchor points keep their first value");
        }
        let (ax, ay) = (a.at(0), a.at(1));
        let w = b.w.max(1.0);
        let h = b.h.max(1.0);
        set(out, Prop::AnchorX, round4((ax - b.x) / w));
        set(out, Prop::AnchorY, round4((ay - b.y) / h));
        let dx = b.x - ax - origin.0;
        let dy = b.y - ay - origin.1;

        let pos = ks.get("p");
        if pos.and_then(|p| p.get("s")).and_then(Json::as_bool) == Some(true) {
            // Separated dimensions.
            for (prop, key, d) in [(Prop::X, "x", dx), (Prop::Y, "y", dy)] {
                let anim = Anim::parse(pos.and_then(|p| p.get(key)));
                self.apply(out, prop, anim.as_ref(), time, |v| v[0] + d, 0.0);
            }
        } else {
            let p = Anim::parse(pos);
            self.apply(out, Prop::X, p.as_ref(), time, |v| v[0] + dx, 0.0);
            self.apply(
                out,
                Prop::Y,
                p.as_ref(),
                time,
                |v| v.get(1).copied().unwrap_or(0.0) + dy,
                0.0,
            );
        }
        let s = Anim::parse(ks.get("s"));
        self.apply(out, Prop::ScaleX, s.as_ref(), time, |v| v[0] / 100.0, 1.0);
        self.apply(
            out,
            Prop::ScaleY,
            s.as_ref(),
            time,
            |v| v.get(1).copied().unwrap_or(v[0]) / 100.0,
            1.0,
        );
        let r = Anim::parse(ks.get("r").or(ks.get("rz")));
        self.apply(out, Prop::Rotation, r.as_ref(), time, |v| v[0], 0.0);
        let o = Anim::parse(ks.get("o"));
        self.apply(out, Prop::Opacity, o.as_ref(), time, |v| v[0] / 100.0, 1.0);
    }

    /// Writes a static value or keyframes; `default` values are left out.
    fn apply(
        &self,
        out: &mut Layer,
        prop: Prop,
        anim: Option<&Anim>,
        time: Time,
        f: impl Fn(&[f64]) -> f64,
        default: f64,
    ) {
        let ok = |v: &[f64]| !v.is_empty();
        match anim {
            Some(Anim::Keys(keys)) if keys.iter().all(|k| ok(&k.v)) => {
                let track = keys
                    .iter()
                    .map(|k| Keyframe {
                        frame: self.frame(time, k.t),
                        value: Value::Number(round4(f(&k.v))),
                        easing: k.easing,
                    })
                    .collect();
                out.tracks.insert(prop, track);
            }
            Some(a) if ok(a.first()) => {
                let v = round4(f(a.first()));
                if (v - default).abs() > 1e-6 || matches!(prop, Prop::X | Prop::Y) {
                    out.props.insert(prop, Value::Number(v));
                }
            }
            _ => {
                if matches!(prop, Prop::X | Prop::Y) {
                    out.props
                        .insert(prop, Value::Number(round4(f(&[0.0, 0.0]))));
                }
            }
        }
    }

    /// Layer in / out points become opacity holds.
    fn in_out(&self, out: &mut Layer, l: &Json, time: Time) {
        let ip = num(l.get("ip")).map(|t| self.frame(time, t));
        let op = num(l.get("op")).map(|t| self.frame(time, t));
        let late = ip.filter(|f| *f > 0.5);
        let early = op.filter(|f| *f < self.duration - 0.5);
        if late.is_none() && early.is_none() {
            return;
        }
        let hold = |frame: f64, v: f64| Keyframe {
            frame,
            value: Value::Number(v),
            easing: Easing::Hold,
        };
        match out.tracks.get_mut(&Prop::Opacity) {
            Some(track) => {
                if let Some(ip) = late {
                    if track.first().is_some_and(|k| k.frame > ip) {
                        let v = track[0].value;
                        track.insert(
                            0,
                            Keyframe {
                                frame: ip,
                                value: v,
                                easing: Easing::Hold,
                            },
                        );
                    }
                    track.insert(0, hold(0.0, 0.0));
                }
                if let Some(op) = early {
                    track.retain(|k| k.frame < op);
                    if let Some(last) = track.last_mut() {
                        last.easing = Easing::Hold;
                    }
                    track.push(hold(op, 0.0));
                }
                track.sort_by(|a, b| a.frame.total_cmp(&b.frame));
                track.dedup_by(|b, a| a.frame == b.frame);
            }
            None => {
                let o = out
                    .props
                    .remove(&Prop::Opacity)
                    .and_then(|v| v.as_number())
                    .unwrap_or(1.0);
                let mut track = Vec::new();
                if let Some(ip) = late {
                    track.push(hold(0.0, 0.0));
                    track.push(hold(ip, o));
                } else {
                    track.push(hold(0.0, o));
                }
                if let Some(op) = early {
                    track.push(hold(op, 0.0));
                }
                out.tracks.insert(Prop::Opacity, track);
            }
        }
    }

    /// Collects rectangles and ellipses, with the fill / stroke that paints
    /// them, through shape groups.
    fn shapes(
        &mut self,
        items: &[Json],
        offset: (f64, f64),
        scale: (f64, f64),
        fill: Option<&(Anim, f64)>,
        stroke: Option<&(Anim, f64, f64)>,
        out: &mut Vec<Primitive>,
    ) {
        let visible = |i: &&Json| i.get("hd").and_then(Json::as_bool) != Some(true);
        let fill = items
            .iter()
            .filter(visible)
            .find(|i| i.get("ty").and_then(Json::as_str) == Some("fl"))
            .and_then(|i| {
                Some((
                    Anim::parse(i.get("c"))?,
                    Anim::parse(i.get("o")).map_or(100.0, |o| o.at(0)),
                ))
            })
            .or_else(|| fill.map(|(a, o)| (a.clone(), *o)));
        let stroke = items
            .iter()
            .filter(visible)
            .find(|i| i.get("ty").and_then(Json::as_str) == Some("st"))
            .and_then(|i| {
                Some((
                    Anim::parse(i.get("c"))?,
                    Anim::parse(i.get("o")).map_or(100.0, |o| o.at(0)),
                    Anim::parse(i.get("w")).map_or(1.0, |w| w.at(0)),
                ))
            })
            .or_else(|| stroke.map(|(a, o, w)| (a.clone(), *o, *w)));
        // The group transform applies to everything in the group.
        let (mut off, mut sc) = (offset, scale);
        if let Some(tr) = items
            .iter()
            .find(|i| i.get("ty").and_then(Json::as_str) == Some("tr"))
        {
            let p = Anim::parse(tr.get("p")).map_or((0.0, 0.0), |p| (p.at(0), p.at(1)));
            let a = Anim::parse(tr.get("a")).map_or((0.0, 0.0), |a| (a.at(0), a.at(1)));
            let s =
                Anim::parse(tr.get("s")).map_or((1.0, 1.0), |s| (s.at(0) / 100.0, s.at(1) / 100.0));
            if [tr.get("p"), tr.get("s"), tr.get("r"), tr.get("o")]
                .into_iter()
                .any(|p| matches!(Anim::parse(p), Some(Anim::Keys(_))))
            {
                self.warn("animated shape-group transforms keep their first value");
            }
            off = (
                offset.0 + (p.0 - a.0 * s.0) * scale.0,
                offset.1 + (p.1 - a.1 * s.1) * scale.1,
            );
            sc = (scale.0 * s.0, scale.1 * s.1);
        }
        for i in items.iter().filter(visible) {
            match i.get("ty").and_then(Json::as_str).unwrap_or("") {
                "gr" => {
                    let it = i
                        .get("it")
                        .and_then(Json::as_array)
                        .cloned()
                        .unwrap_or_default();
                    self.shapes(&it, off, sc, fill.as_ref(), stroke.as_ref(), out);
                }
                t @ ("rc" | "el") => {
                    let c = Anim::parse(i.get("p")).map_or((0.0, 0.0), |p| (p.at(0), p.at(1)));
                    let Some(size) = Anim::parse(i.get("s")) else {
                        continue;
                    };
                    let size = match size {
                        Anim::Static(v) => Anim::Static(vec![
                            v.first().unwrap_or(&0.0) * sc.0,
                            v.get(1).unwrap_or(&0.0) * sc.1,
                        ]),
                        Anim::Keys(keys) => Anim::Keys(
                            keys.into_iter()
                                .map(|k| Key {
                                    v: vec![
                                        k.v.first().unwrap_or(&0.0) * sc.0,
                                        k.v.get(1).unwrap_or(&0.0) * sc.1,
                                    ],
                                    ..k
                                })
                                .collect(),
                        ),
                    };
                    if fill.is_none() && stroke.is_none() {
                        continue;
                    }
                    out.push(Primitive {
                        ellipse: t == "el",
                        center: (off.0 + c.0 * sc.0, off.1 + c.1 * sc.1),
                        size,
                        radius: Anim::parse(i.get("r")).map_or(0.0, |r| r.at(0) * sc.0.min(sc.1)),
                        fill: fill.clone(),
                        stroke: stroke.clone(),
                    });
                }
                "sh" => {
                    self.warn("free paths (pen tool) are not converted, use rectangles / ellipses")
                }
                "gf" | "gs" => self.warn("gradients are not converted"),
                "sr" => self.warn("stars / polygons are not converted"),
                "tm" | "rd" | "rp" | "mm" | "op" | "pb" | "tw" | "zz" => {
                    self.warn("shape modifiers (trim paths, repeater…) are not converted")
                }
                _ => {}
            }
        }
    }

    fn paint(&self, out: &mut Layer, p: &Primitive, time: Time) {
        let color = |v: &[f64], o: f64| {
            let c = |i: usize| v.get(i).copied().unwrap_or(1.0);
            let k = if v.iter().take(3).any(|x| *x > 1.0) {
                255.0
            } else {
                1.0
            };
            Color([
                (c(0) / k) as f32,
                (c(1) / k) as f32,
                (c(2) / k) as f32,
                (v.get(3).copied().unwrap_or(1.0) * o / 100.0) as f32,
            ])
        };
        let mut write = |prop: Prop, anim: &Anim, o: f64| match anim {
            Anim::Static(v) => {
                out.props.insert(prop, Value::Color(color(v, o)));
            }
            Anim::Keys(keys) => {
                out.tracks.insert(
                    prop,
                    keys.iter()
                        .map(|k| Keyframe {
                            frame: self.frame(time, k.t),
                            value: Value::Color(color(&k.v, o)),
                            easing: k.easing,
                        })
                        .collect(),
                );
            }
        };
        if let Some((c, o)) = &p.fill {
            write(Prop::Fill, c, *o);
        }
        if let Some((c, o, _)) = &p.stroke {
            write(Prop::Stroke, c, *o);
        }
        if p.fill.is_none() {
            out.props
                .insert(Prop::Fill, Value::Color(Color::TRANSPARENT));
        }
        if let Some((_, _, w)) = &p.stroke {
            out.props.insert(Prop::StrokeWidth, Value::Number(*w));
        }
        if !p.ellipse && p.radius > 0.0 {
            out.props
                .insert(Prop::CornerRadius, Value::Number(p.radius.round()));
        }
    }

    /// Animated rectangle / ellipse size: width / height keyframes, growing
    /// from the centre like in After Effects (when the layer is not moving).
    fn size_tracks(
        &self,
        out: &mut Layer,
        p: &Primitive,
        ks: Option<&Json>,
        origin: (f64, f64),
        time: Time,
    ) {
        let Anim::Keys(keys) = &p.size else { return };
        let frames = |f: &dyn Fn(&[f64]) -> f64| -> Vec<Keyframe> {
            keys.iter()
                .map(|k| Keyframe {
                    frame: self.frame(time, k.t),
                    value: Value::Number(round4(f(&k.v))),
                    easing: k.easing,
                })
                .collect()
        };
        let varies = |i: usize| {
            let v = |k: &Key| k.v.get(i).copied().unwrap_or(0.0);
            keys.iter().any(|k| (v(k) - v(&keys[0])).abs() > 1e-6)
        };
        let pos_static = !matches!(
            ks.and_then(|k| Anim::parse(k.get("p"))),
            Some(Anim::Keys(_))
        ) && ks.and_then(|k| k.pointer("/p/s")).and_then(Json::as_bool)
            != Some(true);
        let ks = ks.cloned().unwrap_or(Json::Null);
        let pos = Anim::parse(ks.get("p")).unwrap_or(Anim::Static(vec![0.0, 0.0]));
        let a = Anim::parse(ks.get("a")).unwrap_or(Anim::Static(vec![0.0, 0.0]));
        let (px, py) = (
            pos.at(0) - a.at(0) - origin.0,
            pos.at(1) - a.at(1) - origin.1,
        );
        let (cx, cy) = p.center;
        if varies(0) {
            out.tracks.insert(Prop::Width, frames(&|v| v[0]));
            out.props.remove(&Prop::Width);
            if pos_static {
                out.tracks
                    .insert(Prop::X, frames(&|v| px + cx - v[0] / 2.0));
                out.props.remove(&Prop::X);
            }
        }
        if varies(1) {
            out.tracks
                .insert(Prop::Height, frames(&|v| v.get(1).copied().unwrap_or(0.0)));
            out.props.remove(&Prop::Height);
            if pos_static {
                out.tracks.insert(
                    Prop::Y,
                    frames(&|v| py + cy - v.get(1).copied().unwrap_or(0.0) / 2.0),
                );
                out.props.remove(&Prop::Y);
            }
        }
    }

    /// Text layer: kind and box (baseline of the first line at y = 0).
    fn text(&mut self, l: &Json, name: &str) -> Option<(LayerKind, Box2, TextStyle)> {
        let docs = l.pointer("/t/d/k").and_then(Json::as_array)?;
        if docs.len() > 1 {
            self.warn("text changing over time keeps its first value");
        }
        if l.pointer("/t/a")
            .and_then(Json::as_array)
            .is_some_and(|a| !a.is_empty())
        {
            self.warn("text animators are not converted");
        }
        let d = docs.first()?.get("s")?;
        let text = d
            .get("t")
            .and_then(Json::as_str)
            .unwrap_or("")
            .replace(['\r', '\u{3}'], "\n");
        let size = num(d.get("s")).unwrap_or(48.0);
        let font = d.get("f").and_then(Json::as_str).unwrap_or("");
        let (family, weight) = self.font(font);
        let align = match d.get("j").and_then(Json::as_i64) {
            Some(1) => TextAlign::Right,
            Some(2) => TextAlign::Center,
            _ => TextAlign::Left,
        };
        let lines = text.lines().count().max(1) as f64;
        let longest = text.lines().map(|s| s.chars().count()).max().unwrap_or(1) as f64;
        // Paragraph text has a box; point text is estimated.
        let (b, valign) = match (
            d.get("sz").map(vector).filter(|v| v.len() >= 2),
            d.get("ps").map(vector).filter(|v| v.len() >= 2),
        ) {
            (Some(sz), Some(ps)) => (
                Box2 {
                    x: ps[0],
                    y: ps[1],
                    w: sz[0],
                    h: sz[1],
                },
                VerticalAlign::Top,
            ),
            _ => {
                let w = (longest * size * 0.62).max(size);
                let x = match align {
                    TextAlign::Left => 0.0,
                    TextAlign::Center => -w / 2.0,
                    TextAlign::Right => -w,
                };
                (
                    Box2 {
                        x,
                        y: -size * 1.02,
                        w,
                        h: size * 1.2 * lines,
                    },
                    VerticalAlign::Top,
                )
            }
        };
        // Every text becomes a data field the operator fills in.
        let mut key: String = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>()
            .trim_matches('_')
            .to_string();
        if key.is_empty() {
            key = "text".into();
        }
        let base = key.clone();
        let mut n = 2;
        while self.fields.iter().any(|f| f.key == key) {
            key = format!("{base}_{n}");
            n += 1;
        }
        self.fields.push(Field {
            key: key.clone(),
            label: name.to_string(),
            kind: if lines > 1.0 {
                FieldKind::Multiline
            } else {
                FieldKind::Text
            },
            default: text,
            binding: None,
            min: None,
            max: None,
            options: Vec::new(),
        });
        Some((
            LayerKind::Text {
                x: Default::default(),
                wrap: false,
                transform: Default::default(),
                decoration: Default::default(),
                line_height: None,
                scroll: Default::default(),
                speed: 120.0,
                separator: "   •   ".into(),
                looping: false,
                overflow: Default::default(),
                squeeze: 0.8,
                min_size: 0.6,
                warn_below: 0.8,
                text: format!("{{{{{key}}}}}"),
                font_family: family,
                font_weight: weight,
                align,
                vertical_align: valign,
                uppercase: false,
            },
            b,
            TextStyle {
                size,
                color: d.get("fc").map(vector),
                tracking: num(d.get("tr")).unwrap_or(0.0),
            },
        ))
    }

    fn font(&self, name: &str) -> (String, u16) {
        let f = self
            .fonts
            .iter()
            .find(|f| f.get("fName").and_then(Json::as_str) == Some(name));
        let family = f
            .and_then(|f| f.get("fFamily"))
            .and_then(Json::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| name.split('-').next().unwrap_or("Inter").to_string());
        let style = f
            .and_then(|f| f.get("fStyle"))
            .and_then(Json::as_str)
            .unwrap_or(name)
            .to_lowercase()
            .replace([' ', '-', '_'], "");
        let weight = [
            ("thin", 100),
            ("hairline", 100),
            ("extralight", 200),
            ("ultralight", 200),
            ("semibold", 600),
            ("demibold", 600),
            ("extrabold", 800),
            ("ultrabold", 800),
            ("light", 300),
            ("medium", 500),
            ("bold", 700),
            ("black", 900),
            ("heavy", 900),
        ]
        .iter()
        .find(|(k, _)| style.contains(k))
        .map_or(400, |(_, w)| *w);
        let family = if family.is_empty() {
            "Inter".into()
        } else {
            family
        };
        (family, weight)
    }

    fn image(&mut self, l: &Json) -> (String, f64, f64) {
        let asset = l
            .get("refId")
            .and_then(Json::as_str)
            .and_then(|r| {
                self.assets
                    .iter()
                    .find(|a| a.get("id").and_then(Json::as_str) == Some(r))
            })
            .cloned()
            .unwrap_or(Json::Null);
        let w = num(asset.get("w")).unwrap_or(100.0);
        let h = num(asset.get("h")).unwrap_or(100.0);
        let p = asset.get("p").and_then(Json::as_str).unwrap_or("");
        let u = asset.get("u").and_then(Json::as_str).unwrap_or("");
        if p.is_empty() {
            self.warn("image without a file");
            return (String::new(), w, h);
        }
        let id = asset.get("id").and_then(Json::as_str).unwrap_or("image");
        let ext = if let Some(rest) = p.strip_prefix("data:image/") {
            rest.split([';', ','])
                .next()
                .unwrap_or("png")
                .replace("jpeg", "jpg")
        } else {
            p.rsplit('.').next().unwrap_or("png").to_lowercase()
        };
        let stem: String = id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let file = format!("ae-{}-{stem}.{ext}", &new_id()[..6]);
        let source = if p.starts_with("data:") {
            ImageSource::DataUri(p.to_string())
        } else {
            ImageSource::Path(format!("{u}{p}"))
        };
        if let Some(existing) = self.images.iter().find(|i| i.source == source) {
            return (format!("/assets/{}", existing.file), w, h);
        }
        self.images.push(ImageAsset {
            file: file.clone(),
            source,
        });
        (format!("/assets/{file}"), w, h)
    }
}

struct TextStyle {
    size: f64,
    color: Option<Vec<f64>>,
    tracking: f64,
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

// ---------------------------------------------------------------- markers ---

fn markers(json: &Json, t: &Template, ip: f64, scale: f64) -> Vec<Marker> {
    let mut out = Vec::new();
    for m in json
        .get("markers")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        let Some(tm) = num(m.get("tm")) else { continue };
        let frame = ((tm - ip) * scale).round().clamp(0.0, t.duration);
        let raw = m.get("cm").and_then(Json::as_str).unwrap_or("").to_string();
        // Bodymovin may wrap the comment in JSON: {"name": "…"}.
        let name = serde_json::from_str::<Json>(&raw)
            .ok()
            .and_then(|j| j.get("name").and_then(Json::as_str).map(str::to_string))
            .unwrap_or(raw);
        let n = name.to_lowercase();
        let kind = if n.contains("out") {
            MarkerKind::Out
        } else if ["pause", "hold", "on air", "onair", "stop", "loop"]
            .iter()
            .any(|k| n.contains(k))
        {
            MarkerKind::Pause
        } else {
            MarkerKind::Label
        };
        out.push(Marker {
            id: new_id(),
            frame,
            name: if name.is_empty() {
                "Marker".into()
            } else {
                name
            },
            kind,
            resume: Default::default(),
        });
    }
    if out.iter().any(|m| m.kind == MarkerKind::Pause) {
        return out;
    }
    // No pause marker: the longest moment without keyframes is the on-air
    // hold; what comes after it is the out animation.
    let mut frames: Vec<f64> = t
        .layers
        .iter()
        .flat_map(|l| l.tracks.values().flatten().map(|k| k.frame))
        .filter(|f| *f > 0.0 && *f < t.duration)
        .collect();
    frames.sort_by(f64::total_cmp);
    frames.dedup();
    let gap = frames
        .windows(2)
        .map(|w| (w[0], w[1]))
        .max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)));
    let (pause, out_frame) = match gap {
        Some((a, b)) if b - a > t.duration * 0.15 => (a, Some(b)),
        _ => (frames.last().copied().unwrap_or(t.duration), None),
    };
    out.push(Marker {
        id: new_id(),
        frame: pause,
        name: "On air".into(),
        kind: MarkerKind::Pause,
        resume: Default::default(),
    });
    if !out.iter().any(|m| m.kind == MarkerKind::Out) {
        out.push(Marker {
            id: new_id(),
            frame: out_frame.unwrap_or(pause),
            name: "Out".into(),
            kind: MarkerKind::Out,
            resume: Default::default(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Imported {
        let json: Json =
            serde_json::from_str(include_str!("../tests/fixtures/lower_third.lottie.json"))
                .unwrap();
        import(
            &json,
            &Options {
                width: 1920.0,
                height: 1080.0,
                fps: 50.0,
                zone: "lower".into(),
            },
        )
        .unwrap()
    }

    fn by_name<'a>(t: &'a Template, name: &str) -> &'a Layer {
        t.layers.iter().find(|l| l.name == name).unwrap()
    }

    fn track(l: &Layer, p: Prop) -> Vec<(f64, f64)> {
        l.tracks[&p]
            .iter()
            .map(|k| (k.frame, k.value.as_number().unwrap()))
            .collect()
    }

    #[test]
    fn converts_a_lower_third() {
        let imp = fixture();
        let t = &imp.template;
        assert_eq!(t.name, "AE lower third");
        assert_eq!(t.duration, 250.0); // 125 frames at 25 fps → 50 fps
        assert_eq!(t.zone, "lower");

        // Null parent → group; its children inside, the text on top.
        let ctrl = by_name(t, "Controller");
        assert!(matches!(ctrl.kind, LayerKind::Group { .. }));
        assert_eq!(
            track(ctrl, Prop::X),
            vec![
                (0.0, -200.0),
                (40.0, 160.0),
                (200.0, 160.0),
                (240.0, -700.0)
            ]
        );
        assert_eq!(ctrl.tracks[&Prop::X][1].easing, Easing::Hold);
        let plate = by_name(t, "Plate");
        let name = by_name(t, "Name");
        assert_eq!(plate.parent.as_deref(), Some(ctrl.id.as_str()));
        assert_eq!(name.parent.as_deref(), Some(ctrl.id.as_str()));
        let pos = |l: &Layer| t.layers.iter().position(|x| x.id == l.id).unwrap();
        assert!(pos(ctrl) < pos(plate) && pos(plate) < pos(name));

        // The rectangle grows from its centre.
        assert_eq!(
            track(plate, Prop::Width),
            vec![(0.0, 0.0), (30.0, 600.0), (200.0, 600.0), (230.0, 0.0)]
        );
        assert_eq!(track(plate, Prop::X)[..2], [(0.0, 300.0), (30.0, 0.0)]);
        assert_eq!(plate.props[&Prop::Y], Value::Number(-45.0));
        assert_eq!(plate.props[&Prop::CornerRadius], Value::Number(6.0));
        let Value::Color(c) = plate.props[&Prop::Fill] else {
            panic!()
        };
        assert!((c.0[3] - 0.9).abs() < 1e-6);
        assert_eq!(
            plate.tracks[&Prop::Width][0].easing,
            Easing::Bezier {
                x1: 0.33,
                y1: 0.0,
                x2: 0.67,
                y2: 1.0
            }
        );

        // Text → data field.
        let LayerKind::Text {
            text,
            font_family,
            font_weight,
            ..
        } = &name.kind
        else {
            panic!()
        };
        assert_eq!(text, "{{name}}");
        assert_eq!((font_family.as_str(), *font_weight), ("Inter", 700));
        assert_eq!(t.fields[0].key, "name");
        assert_eq!(t.fields[0].default, "Jane Doe");
        assert_eq!(name.props[&Prop::FontSize], Value::Number(60.0));

        // Solid with a late in point: hidden until frame 10.
        let accent = by_name(t, "Accent");
        let o = track(accent, Prop::Opacity);
        assert_eq!(o[0], (0.0, 0.0));
        assert_eq!(o.last().unwrap(), &(30.0, 1.0));
        assert_eq!(
            accent.props[&Prop::Fill],
            Value::Color(Color::from_hex("#ff3b5c").unwrap())
        );

        // Precomp: group around the anchor, inner time shifted by its start.
        let logo = by_name(t, "Logo");
        assert_eq!(
            (logo.props[&Prop::X], logo.props[&Prop::Y]),
            (Value::Number(1620.0), Value::Number(40.0))
        );
        let disc = by_name(t, "Disc");
        assert_eq!(disc.parent.as_deref(), Some(logo.id.as_str()));
        assert!(matches!(disc.kind, LayerKind::Ellipse { .. }));
        assert_eq!(track(disc, Prop::ScaleX), vec![(20.0, 0.0), (40.0, 1.0)]);
        assert_eq!(disc.props[&Prop::X], Value::Number(20.0));

        // Embedded image.
        let bug = by_name(t, "Bug");
        let LayerKind::Image { src, .. } = &bug.kind else {
            panic!()
        };
        assert_eq!(imp.images.len(), 1);
        assert_eq!(src, &format!("/assets/{}", imp.images[0].file));
        assert!(matches!(imp.images[0].source, ImageSource::DataUri(_)));

        // On air between the in and out animations.
        let m: Vec<_> = t.markers.iter().map(|m| (m.kind, m.frame)).collect();
        assert_eq!(m, vec![(MarkerKind::Pause, 40.0), (MarkerKind::Out, 200.0)]);
        assert!(imp.warnings.is_empty(), "{:?}", imp.warnings);
        // Round-trips through the model.
        let back: Template = serde_json::from_value(serde_json::to_value(t).unwrap()).unwrap();
        assert_eq!(&back, t);
    }

    #[test]
    fn fits_other_sizes_and_reports_unsupported() {
        let json = serde_json::json!({
            "v": "5.7.0", "fr": 30, "ip": 0, "op": 60, "w": 1080, "h": 1080, "nm": "Square",
            "layers": [{ "ty": 4, "ind": 1, "nm": "Path", "ip": 0, "op": 60, "st": 0,
                "ks": { "p": { "a": 0, "k": [0, 0] } },
                "shapes": [{ "ty": "sh" }, { "ty": "gf" }] }],
            "markers": [{ "tm": 30, "cm": "{\"name\":\"Pause\"}", "dr": 0 }]
        });
        let imp = import(
            &json,
            &Options {
                width: 1920.0,
                height: 1080.0,
                fps: 30.0,
                zone: String::new(),
            },
        )
        .unwrap();
        assert_eq!(imp.template.markers[0].kind, MarkerKind::Pause);
        assert_eq!(imp.template.markers[0].frame, 30.0);
        assert!(imp.warnings.iter().any(|w| w.contains("free paths")));
        assert!(imp.warnings.iter().any(|w| w.contains("gradients")));
        // Only the fitting group is left.
        assert_eq!(imp.template.layers.len(), 1);
        assert!(import(
            &serde_json::json!({"foo": 1}),
            &Options {
                width: 1.0,
                height: 1.0,
                fps: 25.0,
                zone: String::new()
            }
        )
        .is_err());
    }

    #[test]
    fn solid_parent_keeps_its_children() {
        let json = serde_json::json!({
            "v": "5.7.0", "fr": 25, "ip": 0, "op": 50, "w": 1920, "h": 1080,
            "layers": [
                { "ty": 4, "ind": 2, "parent": 1, "nm": "Dot", "ip": 0, "op": 50, "st": 0,
                  "ks": { "p": { "a": 0, "k": [10, 10] } },
                  "shapes": [{ "ty": "el", "p": { "a": 0, "k": [0, 0] }, "s": { "a": 0, "k": [20, 20] } },
                             { "ty": "fl", "c": { "a": 0, "k": [1, 0, 0, 1] }, "o": { "a": 0, "k": 100 } }] },
                { "ty": 1, "ind": 1, "nm": "Master", "sw": 200, "sh": 100, "sc": "#00ff00", "ip": 0, "op": 50, "st": 0,
                  "ks": { "p": { "a": 0, "k": [500, 300] } } }
            ]
        });
        let imp = import(
            &json,
            &Options {
                width: 1920.0,
                height: 1080.0,
                fps: 25.0,
                zone: String::new(),
            },
        )
        .unwrap();
        let t = &imp.template;
        let master = by_name(t, "Master");
        assert!(matches!(master.kind, LayerKind::Group { .. }));
        let own: Vec<_> = t
            .layers
            .iter()
            .filter(|l| l.parent.as_deref() == Some(master.id.as_str()))
            .collect();
        assert_eq!(own.len(), 2);
        assert!(matches!(own[0].kind, LayerKind::Rect { .. }));
        assert_eq!(
            own[0].props[&Prop::Fill],
            Value::Color(Color::from_hex("#00ff00").unwrap())
        );
        assert!(matches!(own[1].kind, LayerKind::Ellipse { .. }));
        assert_eq!(
            (own[1].props[&Prop::X], own[1].props[&Prop::Y]),
            (Value::Number(0.0), Value::Number(0.0))
        );
    }
}
