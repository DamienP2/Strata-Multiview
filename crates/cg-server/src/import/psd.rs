//! Import of Photoshop documents (`.psd`, and `.psb` large documents): see
//! [`super`].
//!
//! The file is read after Adobe's « Photoshop File Formats Specification »:
//! header, image resources (the global light angle), layer records with
//! their channels and tagged blocks, and the composite image. Each artboard
//! (else the document) becomes a still template:
//!
//! - pixel layers and smart objects → pictures, their masks applied;
//! - text layers → texts, each one a variable of the template;
//! - solid / gradient fills and rectangle / ellipse shapes → rectangles and
//!   ellipses; other outlines, pattern fills and clipped layers are
//!   rasterised;
//! - groups → groups; drop shadows, opacity and visibility are kept;
//! - adjustment layers and the other effects are left out (reported).

use std::collections::BTreeMap;
use std::io::Read;

use cg_core::wid::shape::{FillKind, FillStyle, GradientStop, StrokePosition, StrokeStyle};
use cg_core::wid::{ellipse, quad};
use cg_core::{
    new_id, Color, Field, FieldKind, ImageFit, Layer, LayerKind, MarkerKind, Prop, Template,
    TextAlign, TextDecoration, Value, VerticalAlign,
};

use super::{Comp, Converted, Ctx, Format, Ignored, LayerReport, Note, Status};

type Res<T> = Result<T, String>;

/// Largest side of a document, px.
const MAX_SIDE: u32 = 16384;
/// Most layer records in a document.
const MAX_LAYERS: usize = 5000;
/// Largest picture decoded (a layer, a mask), px.
const MAX_PIXELS: u64 = 1 << 25;
/// Deepest nesting of groups, descriptors and text engine data.
const MAX_NEST: usize = 64;
const FPS: f64 = 25.0;

const GRAY: u16 = 1;
const INDEXED: u16 = 2;
const RGB: u16 = 3;
const CMYK: u16 = 4;
const DUOTONE: u16 = 8;

/// Tagged blocks whose length takes 8 bytes in a large document (PSB).
const BIG_KEYS: [&[u8; 4]; 20] = [
    b"LMsk", b"Lr16", b"Lr32", b"Layr", b"Mt16", b"Mt32", b"Mtrn", b"Alph", b"FMsk", b"lnk2",
    b"lnk3", b"lnkE", b"FEid", b"FXid", b"PxSD", b"pths", b"extd", b"extn", b"cinf", b"artd",
];

/// Adjustment layers (their settings block).
const ADJUSTMENTS: [&[u8; 4]; 18] = [
    b"levl", b"curv", b"brit", b"blnc", b"blwh", b"hue ", b"hue2", b"selc", b"mixr", b"grdm",
    b"phfl", b"expA", b"vibA", b"nvrt", b"post", b"thrs", b"clrL", b"CgEd",
];

pub fn analyze(bytes: &[u8], ctx: &Ctx) -> Result<Converted, String> {
    let doc = parse(bytes)?;
    let nodes = tree(&doc.layers)?;
    let mut warnings = Vec::new();
    match doc.depth {
        16 => warnings.push(Note::new(
            "16-bit colours reduced to 8 bits",
            Status::Partial,
        )),
        32 => warnings.push(Note::new(
            "32-bit colours reduced to 8 bits",
            Status::Partial,
        )),
        _ => {}
    }
    match doc.mode {
        CMYK => warnings.push(Note::new(
            "CMYK colours converted approximately",
            Status::Partial,
        )),
        DUOTONE => warnings.push(Note::new("Duotone read as grayscale", Status::Partial)),
        _ => {}
    }
    let composite = composite(&doc).ok().flatten();
    if composite.is_none() {
        warnings.push(Note::new("No composite image", Status::Partial));
    }

    // One template per artboard, else one for the document.
    let canvas = [0.0, 0.0, f64::from(doc.width), f64::from(doc.height)];
    let mut comps: Vec<(String, [f64; 4], bool, Vec<&Node>)> = Vec::new();
    let mut outside: Vec<&Node> = Vec::new();
    let board = |n: &Node| {
        n.kids.as_ref()?;
        artboard(&doc.layers[n.i])
    };
    if nodes.iter().any(|n| board(n).is_some()) {
        for n in &nodes {
            if let (Some(rect), Some(kids)) = (board(n), &n.kids) {
                let raw = &doc.layers[n.i];
                comps.push((raw.name.clone(), rect, raw.hidden, kids.iter().collect()));
            }
        }
        for n in &nodes {
            if board(n).is_some() {
                continue;
            }
            let at = extent(&doc.layers, n).and_then(|[x, y, w, h]| {
                let (cx, cy) = (x + w / 2.0, y + h / 2.0);
                comps.iter().position(|(_, [ax, ay, aw, ah], _, _)| {
                    cx >= *ax && cx < ax + aw && cy >= *ay && cy < ay + ah
                })
            });
            match at {
                Some(i) => comps[i].3.push(n),
                None => outside.push(n),
            }
        }
        // Loose layers keep their place among the artboard's own.
        for c in &mut comps {
            c.3.sort_by_key(|n| n.i);
        }
    } else {
        comps.push(("Document".into(), canvas, false, nodes.iter().collect()));
    }

    let mut files = Vec::new();
    let mut out = Vec::new();
    for (ci, (name, area, hidden, list)) in comps.into_iter().enumerate() {
        let mut cx = Cx {
            doc: &doc,
            ctx,
            files: &mut files,
            fields: Vec::new(),
            ignored: Vec::new(),
            area,
        };
        let mut built = cx.list(&list, 0);
        if doc.layers.is_empty() {
            // A flat document: its picture is the only layer.
            if let Some(b) = composite.as_ref().and_then(|c| cx.background(c)) {
                built.push(b);
            }
        }
        if ci == 0 {
            for n in &outside {
                cx.ignore_tree(n, "Outside the artboards");
            }
        }
        let flat = composite
            .as_ref()
            .and_then(|c| c.crop(irect(area)))
            .and_then(|c| cx.file(&format!("{name} flat"), &c));
        let mut reports = Vec::new();
        report(&built, &mut reports);
        let mut layers = Vec::new();
        place(built, None, (area[0], area[1]), &mut layers);
        let mut template = Template::blank(&name, &ctx.zone, FPS);
        template.duration = FPS;
        for m in &mut template.markers {
            if m.kind == MarkerKind::Out {
                m.frame = 0.0;
            }
        }
        template.fields = cx.fields;
        template.layers = layers;
        let mut ignored = cx.ignored;
        ignored.reverse();
        out.push(Comp {
            id: new_id(),
            name,
            duration: 0.0,
            selected: !hidden,
            template,
            layers: reports,
            ignored,
            variables: Vec::new(),
            animated: false,
            flat,
        });
    }
    // The first template's box: the first artboard's, else the canvas.
    let a = first_area(&doc, &nodes).unwrap_or(canvas);
    let (width, height) = (a[2].round() as u32, a[3].round() as u32);
    Ok(Converted {
        format: Format::Psd,
        name: String::new(),
        width,
        height,
        fps: FPS,
        comps: out,
        warnings,
        files,
    })
}

/// The box of the first artboard.
fn first_area(doc: &Doc, nodes: &[Node]) -> Option<[f64; 4]> {
    nodes
        .iter()
        .filter(|n| n.kids.is_some())
        .find_map(|n| artboard(&doc.layers[n.i]))
}

// ------------------------------------------------------------------ reader --

fn damaged() -> String {
    "the Photoshop file is damaged or truncated".into()
}

/// Big-endian reader over a part of the file.
#[derive(Clone)]
struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, p: 0 }
    }
    fn left(&self) -> usize {
        self.b.len().saturating_sub(self.p)
    }
    fn take(&mut self, n: usize) -> Res<&'a [u8]> {
        let end = self
            .p
            .checked_add(n)
            .filter(|e| *e <= self.b.len())
            .ok_or_else(damaged)?;
        let s = &self.b[self.p..end];
        self.p = end;
        Ok(s)
    }
    fn skip(&mut self, n: usize) -> Res<()> {
        self.take(n).map(|_| ())
    }
    fn arr<const N: usize>(&mut self) -> Res<[u8; N]> {
        let mut a = [0; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
    }
    fn u8(&mut self) -> Res<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Res<u16> {
        self.arr().map(u16::from_be_bytes)
    }
    fn i16(&mut self) -> Res<i16> {
        self.arr().map(i16::from_be_bytes)
    }
    fn u32(&mut self) -> Res<u32> {
        self.arr().map(u32::from_be_bytes)
    }
    fn i32(&mut self) -> Res<i32> {
        self.arr().map(i32::from_be_bytes)
    }
    fn u64(&mut self) -> Res<u64> {
        self.arr().map(u64::from_be_bytes)
    }
    fn f64(&mut self) -> Res<f64> {
        self.arr().map(f64::from_be_bytes)
    }
    /// A length: 4 bytes, 8 when `big`.
    fn len(&mut self, big: bool) -> Res<usize> {
        let n = if big {
            self.u64()?
        } else {
            u64::from(self.u32()?)
        };
        usize::try_from(n).map_err(|_| damaged())
    }
    /// A part preceded by its length.
    fn block(&mut self, big: bool) -> Res<Rd<'a>> {
        let n = self.len(big)?;
        Ok(Rd::new(self.take(n)?))
    }
    /// Unicode string: its count of UTF-16 units, then them.
    fn ustr(&mut self) -> Res<String> {
        let n = self.u32()? as usize;
        Ok(utf16(self.take(n.checked_mul(2).ok_or_else(damaged)?)?))
    }
    /// Descriptor key or class: its length, or 0 then 4 bytes.
    fn key(&mut self) -> Res<String> {
        let n = self.u32()? as usize;
        let raw = self.take(if n == 0 { 4 } else { n })?;
        Ok(String::from_utf8_lossy(raw).trim().to_string())
    }
}

fn utf16(raw: &[u8]) -> String {
    let units: Vec<u16> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_be_bytes(*c))
        .collect();
    String::from_utf16_lossy(&units)
        .trim_end_matches('\0')
        .to_string()
}

// ---------------------------------------------------------------- document --

struct Doc<'a> {
    big: bool,
    width: u32,
    height: u32,
    depth: u16,
    mode: u16,
    channels: usize,
    palette: Vec<u8>,
    /// Global light angle (degrees) of the effects.
    angle: f64,
    /// Bottom first.
    layers: Vec<Raw<'a>>,
    /// The composite has a transparency channel.
    merged_alpha: bool,
    merged: &'a [u8],
}

/// A layer record.
struct Raw<'a> {
    name: String,
    /// Left, top, right, bottom.
    rect: [i64; 4],
    channels: Vec<(i16, &'a [u8])>,
    blend: [u8; 4],
    opacity: u8,
    clipping: bool,
    hidden: bool,
    mask: Option<Mask>,
    info: Vec<([u8; 4], &'a [u8])>,
}

/// The user (pixel) mask of a layer.
struct Mask {
    rect: [i64; 4],
    default: u8,
    disabled: bool,
}

impl<'a> Raw<'a> {
    fn info(&self, key: &[u8; 4]) -> Option<&'a [u8]> {
        self.info.iter().find(|(k, _)| k == key).map(|(_, d)| *d)
    }
    fn size(&self) -> (usize, usize) {
        let w = (self.rect[2] - self.rect[0]).max(0) as usize;
        let h = (self.rect[3] - self.rect[1]).max(0) as usize;
        (w, h)
    }
    fn channel(&self, id: i16) -> Option<&'a [u8]> {
        self.channels.iter().find(|c| c.0 == id).map(|c| c.1)
    }
    /// Section divider: 1, 2 = a group (open, closed), 3 = its end.
    fn section(&self) -> u32 {
        self.info(b"lsct")
            .or_else(|| self.info(b"lsdk"))
            .and_then(|d| Rd::new(d).u32().ok())
            .unwrap_or(0)
    }
}

fn parse(bytes: &[u8]) -> Res<Doc<'_>> {
    let mut r = Rd::new(bytes);
    if r.take(4).ok() != Some(b"8BPS".as_slice()) {
        return Err("not a Photoshop document".into());
    }
    let version = r.u16()?;
    if version != 1 && version != 2 {
        return Err(format!("unknown Photoshop file version ({version})"));
    }
    let big = version == 2;
    r.skip(6)?;
    let channels = r.u16()? as usize;
    let height = r.u32()?;
    let width = r.u32()?;
    let depth = r.u16()?;
    let mode = r.u16()?;
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(format!(
            "the document is {width} × {height} px: at most {MAX_SIDE} px a side"
        ));
    }
    if !(1..=56).contains(&channels) {
        return Err(damaged());
    }
    let unsupported = |m: &str| {
        Err(format!(
            "{m} documents are not supported: convert the document to RGB in Photoshop"
        ))
    };
    match mode {
        GRAY | INDEXED | RGB | CMYK | DUOTONE => {}
        0 => return unsupported("Bitmap"),
        7 => return unsupported("Multichannel"),
        9 => return unsupported("Lab"),
        _ => return Err(damaged()),
    }
    if ![8, 16, 32].contains(&depth) {
        return unsupported(&format!("{depth}-bit"));
    }
    let palette = r.block(false)?.b.to_vec();

    let mut res = r.block(false)?;
    let mut angle = 120.0;
    while res.left() >= 12 {
        if res.take(4)? != b"8BIM" {
            break;
        }
        let id = res.u16()?;
        let n = res.u8()? as usize;
        res.skip(n + (n + 1) % 2)?;
        let size = res.u32()? as usize;
        let data = res.take(size)?;
        let _ = res.skip(size % 2);
        if id == 1037 && data.len() >= 4 {
            angle = f64::from(i32::from_be_bytes([data[0], data[1], data[2], data[3]]));
        }
    }

    let mut lm = r.block(big)?;
    let mut layers = Vec::new();
    let mut count_negative = false;
    let mut merged_alpha = false;
    if lm.left() > 0 {
        let mut li = lm.block(big)?;
        if li.left() > 0 {
            (layers, count_negative) = layer_info(&mut li, big)?;
        }
        if lm.left() >= 4 {
            let n = lm.u32()? as usize;
            lm.skip(n.min(lm.left()))?;
        }
        for (key, data) in tagged(&mut lm, big) {
            match &key {
                b"Lr16" | b"Lr32" | b"Layr" if layers.is_empty() => {
                    (layers, count_negative) = layer_info(&mut Rd::new(data), big)?;
                }
                b"Mtrn" | b"Mt16" | b"Mt32" => merged_alpha = true,
                _ => {}
            }
        }
    }
    let colors = match mode {
        RGB => 3,
        CMYK => 4,
        _ => 1,
    };
    // An extra channel of the composite is its transparency unless the
    // layers say otherwise (a saved selection).
    merged_alpha |= channels > colors && (layers.is_empty() || count_negative);
    Ok(Doc {
        big,
        width,
        height,
        depth,
        mode,
        channels,
        palette: if mode == INDEXED { palette } else { Vec::new() },
        angle,
        layers,
        merged_alpha,
        merged: &bytes[r.p..],
    })
}

/// Layer records then their channels.
fn layer_info<'a>(r: &mut Rd<'a>, big: bool) -> Res<(Vec<Raw<'a>>, bool)> {
    let count = r.i16()?;
    let n = count.unsigned_abs() as usize;
    if n > MAX_LAYERS {
        return Err(format!("too many layers ({n}, at most {MAX_LAYERS})"));
    }
    let mut layers = Vec::with_capacity(n);
    let mut lengths = Vec::with_capacity(n);
    for _ in 0..n {
        let (raw, chans) = record(r, big)?;
        layers.push(raw);
        lengths.push(chans);
    }
    for (l, chans) in layers.iter_mut().zip(lengths) {
        for (id, len) in chans {
            l.channels.push((id, r.take(len)?));
        }
    }
    Ok((layers, count < 0))
}

type Record<'a> = (Raw<'a>, Vec<(i16, usize)>);

fn record<'a>(r: &mut Rd<'a>, big: bool) -> Res<Record<'a>> {
    let top = i64::from(r.i32()?);
    let left = i64::from(r.i32()?);
    let bottom = i64::from(r.i32()?);
    let right = i64::from(r.i32()?);
    let n = r.u16()? as usize;
    if n > 56 {
        return Err(damaged());
    }
    let mut chans = Vec::with_capacity(n);
    for _ in 0..n {
        let id = r.i16()?;
        chans.push((id, r.len(big)?));
    }
    if r.take(4)? != b"8BIM" {
        return Err(damaged());
    }
    let blend = r.arr::<4>()?;
    let opacity = r.u8()?;
    let clipping = r.u8()? != 0;
    let flags = r.u8()?;
    r.skip(1)?;
    let mut x = r.block(false)?;
    let mut m = x.block(false)?;
    let mask = if m.left() >= 18 {
        let t = i64::from(m.i32()?);
        let l = i64::from(m.i32()?);
        let b = i64::from(m.i32()?);
        let rt = i64::from(m.i32()?);
        let default = m.u8()?;
        let f = m.u8()?;
        Some(Mask {
            rect: [l, t, rt, b],
            default,
            disabled: f & 2 != 0,
        })
    } else {
        None
    };
    x.block(false)?;
    let n = x.u8()? as usize;
    let mut name: String = x.take(n)?.iter().map(|&c| char::from(c)).collect();
    let pad = (4 - (n + 1) % 4) % 4;
    x.p = (x.p + pad).min(x.b.len());
    let info = tagged(&mut x, big);
    if let Some(u) = info.iter().find(|(k, _)| k == b"luni") {
        if let Ok(s) = Rd::new(u.1).ustr() {
            name = s;
        }
    }
    let rect = if right >= left && bottom >= top {
        [left, top, right, bottom]
    } else {
        [left, top, left, top]
    };
    Ok((
        Raw {
            name,
            rect,
            channels: Vec::with_capacity(n),
            blend,
            opacity,
            clipping,
            hidden: flags & 2 != 0,
            mask,
            info,
        },
        chans,
    ))
}

/// Tagged blocks (additional layer information) till the end of `r`;
/// tolerates the padding writers differ on.
fn tagged<'a>(r: &mut Rd<'a>, big: bool) -> Vec<([u8; 4], &'a [u8])> {
    let mut out = Vec::new();
    while let Some(pad) = (0..4).find(|&k| {
        r.b.get(r.p + k..r.p + k + 4)
            .is_some_and(|s| s == b"8BIM" || s == b"8B64")
    }) {
        r.p += pad;
        let Ok(head) = r.take(8) else { break };
        let key = [head[4], head[5], head[6], head[7]];
        let long = big && BIG_KEYS.contains(&&key);
        let Ok(n) = r.len(long) else { break };
        let Ok(data) = r.take(n) else { break };
        out.push((key, data));
    }
    out
}

/// A layer of the list: a group has its children (bottom first).
struct Node {
    i: usize,
    kids: Option<Vec<Node>>,
}

/// The nesting of the layers from their section dividers.
fn tree(layers: &[Raw]) -> Res<Vec<Node>> {
    let mut stack: Vec<Vec<Node>> = vec![Vec::new()];
    for (i, l) in layers.iter().enumerate() {
        match l.section() {
            3 => {
                if stack.len() > MAX_NEST {
                    return Err("groups are nested too deeply".into());
                }
                stack.push(Vec::new());
            }
            1 | 2 if stack.len() > 1 => {
                let kids = stack.pop().unwrap_or_default();
                if let Some(top) = stack.last_mut() {
                    top.push(Node {
                        i,
                        kids: Some(kids),
                    });
                }
            }
            1 | 2 => stack[0].push(Node {
                i,
                kids: Some(Vec::new()),
            }),
            _ => {
                if let Some(top) = stack.last_mut() {
                    top.push(Node { i, kids: None });
                }
            }
        }
    }
    // Unbalanced dividers: what is left goes to the top.
    while stack.len() > 1 {
        let rest = stack.pop().unwrap_or_default();
        if let Some(top) = stack.last_mut() {
            top.extend(rest);
        }
    }
    Ok(stack.pop().unwrap_or_default())
}

/// The box of an artboard group (x, y, w, h).
fn artboard(raw: &Raw) -> Option<[f64; 4]> {
    let data = raw
        .info(b"artb")
        .or_else(|| raw.info(b"artd"))
        .or_else(|| raw.info(b"abdd"))?;
    let mut r = Rd::new(data);
    r.u32().ok()?;
    let d = descriptor(&mut r, 0).ok()?;
    let b = d.get("artboardRect")?;
    let n = |k: &str| b.get(k).and_then(D::num);
    let (l, t, rt, bt) = (n("Left")?, n("Top ")?, n("Rght")?, n("Btom")?);
    let sane = [l, t, rt, bt].iter().all(|v| v.abs() < 1e6);
    (sane && rt > l && bt > t).then_some([l, t, rt - l, bt - t])
}

/// The box of what a layer or group shows, from the records.
fn extent(layers: &[Raw], n: &Node) -> Option<[f64; 4]> {
    match &n.kids {
        Some(kids) => kids.iter().filter_map(|k| extent(layers, k)).reduce(union),
        None => {
            let l = &layers[n.i];
            let (w, h) = l.size();
            (w > 0 && h > 0).then(|| [l.rect[0] as f64, l.rect[1] as f64, w as f64, h as f64])
        }
    }
}

fn union(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    let x = a[0].min(b[0]);
    let y = a[1].min(b[1]);
    let r = (a[0] + a[2]).max(b[0] + b[2]);
    let bt = (a[1] + a[3]).max(b[1] + b[3]);
    [x, y, r - x, bt - y]
}

fn irect(a: [f64; 4]) -> [i64; 4] {
    [
        a[0].floor() as i64,
        a[1].floor() as i64,
        a[2].ceil().max(0.0) as i64,
        a[3].ceil().max(0.0) as i64,
    ]
}

// ------------------------------------------------------------------ pixels --

/// A picture in document pixels (RGBA, straight alpha).
#[derive(Clone)]
struct Px {
    x: i64,
    y: i64,
    w: usize,
    h: usize,
    rgba: Vec<u8>,
}

impl Px {
    fn alpha(&self, x: i64, y: i64) -> u8 {
        let (dx, dy) = (x - self.x, y - self.y);
        if dx < 0 || dy < 0 || dx >= self.w as i64 || dy >= self.h as i64 {
            return 0;
        }
        self.rgba[(dy as usize * self.w + dx as usize) * 4 + 3]
    }

    /// Multiplies the alpha by `f(x, y)` (document pixels, 0..255); true
    /// when something changed.
    fn mul_alpha(&mut self, f: impl Fn(i64, i64) -> u8) -> bool {
        let mut changed = false;
        for y in 0..self.h {
            for x in 0..self.w {
                let i = (y * self.w + x) * 4 + 3;
                let a = self.rgba[i];
                if a == 0 {
                    continue;
                }
                let m = f(self.x + x as i64, self.y + y as i64);
                if m < 255 {
                    self.rgba[i] = ((u32::from(a) * u32::from(m) + 127) / 255) as u8;
                    changed = true;
                }
            }
        }
        changed
    }

    /// The part in `r` (x, y, w, h), transparent outside the picture.
    fn crop(&self, r: [i64; 4]) -> Option<Px> {
        let (w, h) = (r[2].max(0) as usize, r[3].max(0) as usize);
        if w == 0 || h == 0 || (w as u64) * (h as u64) > MAX_PIXELS {
            return None;
        }
        let mut rgba = vec![0; w * h * 4];
        for y in 0..h {
            let sy = r[1] + y as i64 - self.y;
            if sy < 0 || sy >= self.h as i64 {
                continue;
            }
            let x0 = (self.x - r[0]).max(0) as usize;
            let x1 = ((self.x + self.w as i64 - r[0]).min(w as i64)).max(0) as usize;
            if x0 >= x1 {
                continue;
            }
            let sx = (r[0] + x0 as i64 - self.x) as usize;
            let src = (sy as usize * self.w + sx) * 4;
            let dst = (y * w + x0) * 4;
            let n = (x1 - x0) * 4;
            rgba[dst..dst + n].copy_from_slice(&self.rgba[src..src + n]);
        }
        Some(Px {
            x: r[0],
            y: r[1],
            w,
            h,
            rgba,
        })
    }

    /// Without its transparent borders; None when nothing shows.
    fn trim(self) -> Option<Px> {
        let (mut x0, mut y0, mut x1, mut y1) = (self.w, self.h, 0, 0);
        for y in 0..self.h {
            for x in 0..self.w {
                if self.rgba[(y * self.w + x) * 4 + 3] > 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        if (x0, y0, x1, y1) == (0, 0, self.w, self.h) {
            return Some(self);
        }
        self.crop([
            self.x + x0 as i64,
            self.y + y0 as i64,
            (x1 - x0) as i64,
            (y1 - y0) as i64,
        ])
    }

    fn rect(&self) -> [f64; 4] {
        [self.x as f64, self.y as f64, self.w as f64, self.h as f64]
    }

    fn png(&self) -> Res<Vec<u8>> {
        let img = image::RgbaImage::from_raw(self.w as u32, self.h as u32, self.rgba.clone())
            .ok_or("bad picture size")?;
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok(out.into_inner())
    }
}

/// `rows` rows of `w` samples, decoded to 8 bits. `counts` RLE row lengths
/// precede the data (all the channels' for the composite); the first
/// `gamma_rows` rows are colours (32-bit values are linear).
fn plane(
    data: &[u8],
    w: usize,
    rows: usize,
    counts: usize,
    doc: &Doc,
    gamma_rows: usize,
) -> Res<Vec<u8>> {
    if w == 0 || rows == 0 {
        return Ok(Vec::new());
    }
    let mut r = Rd::new(data);
    let compression = r.u16()?;
    let bpc = usize::from(doc.depth / 8);
    let row = w * bpc;
    let size = row.checked_mul(rows).ok_or_else(damaged)?;
    let mut raw = match compression {
        0 => r.take(size.min(r.left()))?.to_vec(),
        1 => {
            let mut lens = Vec::with_capacity(counts.min(1 << 20));
            for _ in 0..counts.max(rows) {
                lens.push(if doc.big {
                    r.u32()? as usize
                } else {
                    usize::from(r.u16()?)
                });
            }
            let mut out = Vec::with_capacity(size);
            for n in lens.into_iter().take(rows) {
                packbits(r.take(n)?, row, &mut out);
            }
            out
        }
        2 | 3 => {
            let mut out = Vec::with_capacity(size);
            flate2::read::ZlibDecoder::new(r.take(r.left())?)
                .take(size as u64)
                .read_to_end(&mut out)
                .map_err(|_| damaged())?;
            out.resize(size, 0);
            if compression == 3 {
                unpredict(&mut out, w, doc.depth);
            }
            out
        }
        _ => return Err(damaged()),
    };
    raw.resize(size, 0);
    Ok(match doc.depth {
        8 => raw,
        16 => raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| ((u32::from(u16::from_be_bytes(*c)) * 255 + 32767) / 65535) as u8)
            .collect(),
        _ => raw
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let v = f32::from_be_bytes(*c);
                let v = if v.is_finite() {
                    v.clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let v = if i / w < gamma_rows {
                    v.powf(1.0 / 2.2)
                } else {
                    v
                };
                (v * 255.0).round() as u8
            })
            .collect(),
    })
}

/// One PackBits row.
fn packbits(src: &[u8], row: usize, out: &mut Vec<u8>) {
    let end = out.len() + row;
    let mut i = 0;
    while i < src.len() && out.len() < end {
        let n = src[i] as i8;
        i += 1;
        if n >= 0 {
            let k = (n as usize + 1).min(src.len() - i);
            out.extend_from_slice(&src[i..i + k]);
            i += k;
        } else if n != -128 {
            if let Some(&v) = src.get(i) {
                out.extend(std::iter::repeat_n(v, (1 - i32::from(n)) as usize));
            }
            i += 1;
        }
    }
    out.resize(end, 0);
}

/// Undoes the « ZIP with prediction » deltas.
fn unpredict(d: &mut [u8], w: usize, depth: u16) {
    match depth {
        8 => {
            for r in d.chunks_exact_mut(w) {
                for i in 1..r.len() {
                    r[i] = r[i].wrapping_add(r[i - 1]);
                }
            }
        }
        16 => {
            for r in d.chunks_exact_mut(w * 2) {
                for i in 1..w {
                    let p = u16::from_be_bytes([r[2 * i - 2], r[2 * i - 1]]);
                    let c = u16::from_be_bytes([r[2 * i], r[2 * i + 1]]);
                    r[2 * i..2 * i + 2].copy_from_slice(&c.wrapping_add(p).to_be_bytes());
                }
            }
        }
        _ => {
            for r in d.chunks_exact_mut(w * 4) {
                for i in 1..r.len() {
                    r[i] = r[i].wrapping_add(r[i - 1]);
                }
                // Each row holds the first bytes of every value, then the second…
                let planar = r.to_vec();
                for x in 0..w {
                    for b in 0..4 {
                        r[x * 4 + b] = planar[b * w + x];
                    }
                }
            }
        }
    }
}

/// RGB of a pixel from the values of its colour channels.
fn to_rgb(doc: &Doc, v: [u8; 4]) -> [u8; 3] {
    match doc.mode {
        RGB => [v[0], v[1], v[2]],
        CMYK => {
            // Stored inverted: 255 = no ink.
            let k = u32::from(v[3]);
            [0, 1, 2].map(|i| (u32::from(v[i]) * k / 255) as u8)
        }
        INDEXED => {
            let i = usize::from(v[0]);
            [0, 256, 512].map(|o| doc.palette.get(o + i).copied().unwrap_or(0))
        }
        _ => [v[0]; 3],
    }
}

fn color_ids(doc: &Doc) -> &'static [i16] {
    match doc.mode {
        RGB => &[0, 1, 2],
        CMYK => &[0, 1, 2, 3],
        _ => &[0],
    }
}

/// The pixels of a layer (colour and transparency), masks not applied.
fn layer_px(doc: &Doc, raw: &Raw) -> Res<Option<Px>> {
    let (w, h) = raw.size();
    if w == 0 || h == 0 {
        return Ok(None);
    }
    let get = |id: i16| -> Res<Option<Vec<u8>>> {
        raw.channel(id)
            .map(|d| plane(d, w, h, h, doc, if id >= 0 { h } else { 0 }))
            .transpose()
    };
    let colors: Vec<Option<Vec<u8>>> = color_ids(doc)
        .iter()
        .map(|&id| get(id))
        .collect::<Res<_>>()?;
    if colors.iter().all(Option::is_none) && raw.channel(-1).is_none() {
        return Ok(None);
    }
    let alpha = get(-1)?;
    let mut rgba = vec![0; w * h * 4];
    for i in 0..w * h {
        let mut v = [0, 0, 0, 255];
        for (c, p) in colors.iter().enumerate() {
            v[c] = p.as_ref().map_or(0, |p| p[i]);
        }
        let [r, g, b] = to_rgb(doc, v);
        let a = alpha.as_ref().map_or(255, |a| a[i]);
        rgba[i * 4..i * 4 + 4].copy_from_slice(&[r, g, b, a]);
    }
    Ok(Some(Px {
        x: raw.rect[0],
        y: raw.rect[1],
        w,
        h,
        rgba,
    }))
}

/// The document's composite image (Image Data section).
fn composite(doc: &Doc) -> Res<Option<Px>> {
    if doc.merged.len() < 2 {
        return Ok(None);
    }
    let (w, h) = (doc.width as usize, doc.height as usize);
    let colors = color_ids(doc).len();
    if doc.channels < colors || (w as u64) * (h as u64) > MAX_PIXELS {
        return Ok(None);
    }
    let alpha = doc.merged_alpha && doc.channels > colors;
    let n = colors + usize::from(alpha);
    let all = plane(doc.merged, w, h * n, h * doc.channels, doc, h * colors)?;
    let plane_of = |c: usize| &all[c * w * h..(c + 1) * w * h];
    let mut rgba = vec![0; w * h * 4];
    for i in 0..w * h {
        let mut v = [0, 0, 0, 255];
        for (c, x) in v.iter_mut().enumerate().take(colors) {
            *x = plane_of(c)[i];
        }
        let [mut r, mut g, mut b] = to_rgb(doc, v);
        let a = if alpha { plane_of(colors)[i] } else { 255 };
        if a < 255 && a > 0 {
            // The composite is matted on white.
            let un = |c: u8| {
                ((i32::from(c) + i32::from(a) - 255) * 255 / i32::from(a)).clamp(0, 255) as u8
            };
            (r, g, b) = (un(r), un(g), un(b));
        }
        rgba[i * 4..i * 4 + 4].copy_from_slice(&[r, g, b, a]);
    }
    Ok(Some(Px {
        x: 0,
        y: 0,
        w,
        h,
        rgba,
    }))
}

// ------------------------------------------------------------- descriptors --

/// A value of an action descriptor.
#[derive(Debug, Clone, PartialEq)]
enum D {
    /// Class and items.
    Obj(String, Vec<(String, D)>),
    List(Vec<D>),
    Num(f64),
    /// Unit (`#Pxl`, `#Prc`, `#Ang`…) and value.
    Unit(String, f64),
    Text(String),
    /// Type and value.
    Enum(String, String),
    Bool(bool),
    Data(Vec<u8>),
    Other,
}

impl D {
    fn get(&self, key: &str) -> Option<&D> {
        let key = key.trim();
        match self {
            D::Obj(_, items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn class(&self) -> &str {
        match self {
            D::Obj(c, _) => c,
            _ => "",
        }
    }
    fn num(&self) -> Option<f64> {
        match self {
            D::Num(n) | D::Unit(_, n) => Some(*n).filter(|n| n.is_finite()),
            _ => None,
        }
    }
    fn bool(&self) -> Option<bool> {
        match self {
            D::Bool(b) => Some(*b),
            _ => None,
        }
    }
    fn text(&self) -> Option<&str> {
        match self {
            D::Text(s) => Some(s),
            _ => None,
        }
    }
    fn enm(&self) -> Option<&str> {
        match self {
            D::Enum(_, v) => Some(v),
            _ => None,
        }
    }
    fn list(&self) -> &[D] {
        match self {
            D::List(l) => l,
            _ => &[],
        }
    }
}

fn descriptor(r: &mut Rd, depth: usize) -> Res<D> {
    if depth > MAX_NEST {
        return Err(damaged());
    }
    r.ustr()?;
    let class = r.key()?;
    let n = r.u32()? as usize;
    let mut items = Vec::with_capacity(n.min(64));
    for _ in 0..n {
        let k = r.key()?;
        let v = dvalue(r, depth + 1)?;
        items.push((k, v));
    }
    Ok(D::Obj(class, items))
}

fn dvalue(r: &mut Rd, depth: usize) -> Res<D> {
    if depth > MAX_NEST {
        return Err(damaged());
    }
    let t = r.arr::<4>()?;
    Ok(match &t {
        b"Objc" | b"GlbO" => descriptor(r, depth + 1)?,
        b"VlLs" => {
            let n = r.u32()? as usize;
            let mut v = Vec::with_capacity(n.min(64));
            for _ in 0..n {
                v.push(dvalue(r, depth + 1)?);
            }
            D::List(v)
        }
        b"doub" => D::Num(r.f64()?),
        b"UntF" => {
            let u = r.arr::<4>()?;
            D::Unit(String::from_utf8_lossy(&u).into_owned(), r.f64()?)
        }
        b"UnFl" => {
            let u = r.arr::<4>()?;
            let n = r.u32()? as usize;
            r.skip(n.checked_mul(8).ok_or_else(damaged)?)?;
            D::Unit(String::from_utf8_lossy(&u).into_owned(), f64::NAN)
        }
        b"TEXT" => D::Text(r.ustr()?),
        b"enum" => {
            let t = r.key()?;
            D::Enum(t, r.key()?)
        }
        b"long" => D::Num(f64::from(r.i32()?)),
        b"comp" => D::Num(r.u64()? as i64 as f64),
        b"bool" => D::Bool(r.u8()? != 0),
        b"type" | b"GlbC" => {
            r.ustr()?;
            r.key()?;
            D::Other
        }
        b"alis" | b"tdta" | b"Pth " => {
            let n = r.u32()? as usize;
            D::Data(r.take(n)?.to_vec())
        }
        b"ObAr" => {
            r.u32()?;
            descriptor(r, depth + 1)?
        }
        b"obj " => {
            let n = r.u32()?;
            for _ in 0..n {
                match &r.arr::<4>()? {
                    b"prop" => {
                        r.ustr()?;
                        r.key()?;
                        r.key()?;
                    }
                    b"Clss" => {
                        r.ustr()?;
                        r.key()?;
                    }
                    b"Enmr" => {
                        r.ustr()?;
                        r.key()?;
                        r.key()?;
                        r.key()?;
                    }
                    b"rele" => {
                        r.ustr()?;
                        r.key()?;
                        r.u32()?;
                    }
                    b"Idnt" | b"indx" => {
                        r.u32()?;
                    }
                    b"name" => {
                        r.ustr()?;
                        r.key()?;
                        r.ustr()?;
                    }
                    _ => return Err(damaged()),
                }
            }
            D::Other
        }
        _ => return Err(damaged()),
    })
}

/// A descriptor after `skip` bytes (versions).
fn block_descriptor(data: &[u8], skip: usize) -> Option<D> {
    let mut r = Rd::new(data);
    r.skip(skip).ok()?;
    descriptor(&mut r, 0).ok()
}

/// A colour object (RGB, HSB, CMYK, gray or Lab).
fn dcolor(d: &D) -> Option<Color> {
    let n = |k: &str| d.get(k).and_then(D::num);
    let c = match d.class() {
        "RGBC" => match (n("Rd"), n("Grn"), n("Bl")) {
            (Some(r), Some(g), Some(b)) => [r / 255.0, g / 255.0, b / 255.0],
            _ => [n("redFloat")?, n("greenFloat")?, n("blueFloat")?],
        },
        "HSBC" => hsb(n("H")?, n("Strt")? / 100.0, n("Brgh")? / 100.0),
        "CMYC" => {
            let k = 1.0 - n("Blck")? / 100.0;
            [
                (1.0 - n("Cyn")? / 100.0) * k,
                (1.0 - n("Mgnt")? / 100.0) * k,
                (1.0 - n("Ylw")? / 100.0) * k,
            ]
        }
        "Grsc" => [1.0 - n("Gry")? / 100.0; 3],
        "LbCl" => lab(n("Lmnc")?, n("A")?, n("B")?),
        _ => return None,
    };
    let [r, g, b] = c.map(|v| v.clamp(0.0, 1.0) as f32);
    Some(Color::rgba(r, g, b, 1.0))
}

fn hsb(h: f64, s: f64, v: f64) -> [f64; 3] {
    let h = (h.rem_euclid(360.0)) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

/// CIE Lab (D50) to sRGB, approximately.
fn lab(l: f64, a: f64, b: f64) -> [f64; 3] {
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let f = |t: f64| {
        if t > 6.0 / 29.0 {
            t * t * t
        } else {
            3.0 * (6.0f64 / 29.0).powi(2) * (t - 4.0 / 29.0)
        }
    };
    let (x, y, z) = (0.9642 * f(fx), f(fy), 0.8251 * f(fz));
    let lin = [
        3.1339 * x - 1.6169 * y - 0.4906 * z,
        -0.9788 * x + 1.9161 * y + 0.0335 * z,
        0.0719 * x - 0.2290 * y + 1.4052 * z,
    ];
    lin.map(|c| {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.003_130_8 {
            12.92 * c
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    })
}

// ------------------------------------------------------------------- fills --

#[derive(Debug, Clone, Copy, PartialEq)]
enum GradKind {
    Linear,
    Radial,
    /// Angle, reflected, diamond: drawn linear.
    Other,
}

#[derive(Debug, Clone)]
struct Grad {
    kind: GradKind,
    /// Photoshop angle, degrees (0 = to the right, 90 = upwards).
    angle: f64,
    /// Position 0..1 and colour (with its transparency), in order.
    stops: Vec<(f64, Color)>,
}

#[derive(Debug, Clone)]
enum Fill {
    Solid(Color),
    Gradient(Grad),
    Pattern,
}

/// The content of a fill or shape layer.
fn fill_of(raw: &Raw) -> Option<Fill> {
    let (key, d) = if let Some(d) = raw.info(b"SoCo") {
        (*b"SoCo", block_descriptor(d, 4))
    } else if let Some(d) = raw.info(b"GdFl") {
        (*b"GdFl", block_descriptor(d, 4))
    } else if raw.info(b"PtFl").is_some() {
        return Some(Fill::Pattern);
    } else {
        let d = raw.info(b"vscg")?;
        let key = [*d.first()?, *d.get(1)?, *d.get(2)?, *d.get(3)?];
        (key, block_descriptor(d, 8))
    };
    match &key {
        b"SoCo" => Some(Fill::Solid(d?.get("Clr")?.pipe(dcolor)?)),
        b"GdFl" => gradient(&d?).map(Fill::Gradient),
        b"PtFl" => Some(Fill::Pattern),
        _ => None,
    }
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}

/// A gradient fill descriptor (`Grad`, `Type`, `Angl`, `Rvrs`).
fn gradient(d: &D) -> Option<Grad> {
    let g = d.get("Grad")?;
    let loc = |s: &D| s.get("Lctn").and_then(D::num).unwrap_or(0.0) / 4096.0;
    let opacities: Vec<(f64, f64)> = g
        .get("Trns")
        .map(D::list)
        .unwrap_or_default()
        .iter()
        .map(|t| {
            let o = t.get("Opct").and_then(D::num).unwrap_or(100.0) / 100.0;
            (loc(t), o)
        })
        .collect();
    let opacity_at = |at: f64| {
        let first = opacities.first().map_or(1.0, |o| o.1);
        let mut v = first;
        for w in opacities.windows(2) {
            let ((a, oa), (b, ob)) = (w[0], w[1]);
            if at >= a && at <= b {
                let t = if b > a { (at - a) / (b - a) } else { 0.0 };
                return oa + (ob - oa) * t;
            }
        }
        if let Some(last) = opacities.last() {
            if at > last.0 {
                v = last.1;
            }
        }
        v
    };
    let mut stops: Vec<(f64, Color)> = g
        .get("Clrs")
        .map(D::list)
        .unwrap_or_default()
        .iter()
        .filter_map(|s| {
            let mut c = s.get("Clr").and_then(dcolor)?;
            let at = loc(s).clamp(0.0, 1.0);
            c.0[3] = opacity_at(at).clamp(0.0, 1.0) as f32;
            Some((at, c))
        })
        .collect();
    if stops.is_empty() {
        return None;
    }
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    if d.get("Rvrs").and_then(D::bool) == Some(true) {
        stops = stops.into_iter().rev().map(|(a, c)| (1.0 - a, c)).collect();
    }
    let kind = match d.get("Type").and_then(D::enm) {
        Some("Rdl") => GradKind::Radial,
        Some("Lnr") | None => GradKind::Linear,
        _ => GradKind::Other,
    };
    Some(Grad {
        kind,
        angle: d.get("Angl").and_then(D::num).unwrap_or(90.0),
        stops,
    })
}

/// The model's fill for a gradient: first colour in `fill`, the others in
/// the style.
fn fill_style(g: &Grad) -> (Color, FillStyle) {
    let first = g.stops[0].1;
    let last = g.stops[g.stops.len() - 1].1;
    let style = FillStyle {
        kind: if g.kind == GradKind::Radial {
            FillKind::Radial
        } else {
            FillKind::Linear
        },
        color2: last.to_hex(),
        angle: (90.0 - g.angle).rem_euclid(360.0) as f32,
        stops: g
            .stops
            .get(1..g.stops.len() - 1)
            .unwrap_or_default()
            .iter()
            .map(|(at, c)| GradientStop {
                at: (at * 100.0) as f32,
                color: c.to_hex(),
            })
            .collect(),
        ..FillStyle::default()
    };
    (first, style)
}

/// Stroke and fill switches of a shape layer (`vstk`).
struct VStroke {
    fill: bool,
    stroke: Option<(f64, Color, StrokePosition)>,
}

fn vstroke(raw: &Raw) -> VStroke {
    let Some(d) = raw.info(b"vstk").and_then(|d| block_descriptor(d, 4)) else {
        return VStroke {
            fill: true,
            stroke: None,
        };
    };
    let fill = d.get("fillEnabled").and_then(D::bool).unwrap_or(true);
    let on = d.get("strokeEnabled").and_then(D::bool).unwrap_or(false);
    let width = d
        .get("strokeStyleLineWidth")
        .and_then(D::num)
        .unwrap_or(0.0);
    let stroke = (on && width > 0.0)
        .then(|| {
            let mut c = d
                .get("strokeStyleContent")
                .and_then(|c| c.get("Clr"))
                .and_then(dcolor)?;
            c.0[3] = (d
                .get("strokeStyleOpacity")
                .and_then(D::num)
                .unwrap_or(100.0)
                / 100.0) as f32;
            let pos = match d.get("strokeStyleLineAlignment").and_then(D::enm) {
                Some("strokeStyleAlignInside") => StrokePosition::Inside,
                Some("strokeStyleAlignOutside") => StrokePosition::Outside,
                _ => StrokePosition::Center,
            };
            Some((width, c, pos))
        })
        .flatten();
    VStroke { fill, stroke }
}

// ------------------------------------------------------------------- paths --

/// A subpath: knots of (preceding handle, anchor, leaving handle), px.
struct Sub {
    closed: bool,
    op: i16,
    knots: Vec<[(f64, f64); 3]>,
}

struct VMask {
    subs: Vec<Sub>,
    invert: bool,
    disabled: bool,
}

fn vector_mask(raw: &Raw, doc: &Doc) -> Option<VMask> {
    let data = raw.info(b"vmsk").or_else(|| raw.info(b"vsms"))?;
    let mut r = Rd::new(data);
    r.u32().ok()?;
    let flags = r.u32().ok()?;
    let (w, h) = (f64::from(doc.width), f64::from(doc.height));
    let mut subs: Vec<Sub> = Vec::new();
    while r.left() >= 26 {
        let rec = r.take(26).ok()?;
        let num = |o: usize| i32::from_be_bytes([rec[o], rec[o + 1], rec[o + 2], rec[o + 3]]);
        match u16::from_be_bytes([rec[0], rec[1]]) {
            sel @ (0 | 3) => subs.push(Sub {
                closed: sel == 0,
                op: i16::from_be_bytes([rec[4], rec[5]]),
                knots: Vec::new(),
            }),
            1 | 2 | 4 | 5 => {
                let pt = |i: usize| {
                    let y = f64::from(num(2 + i * 8)) / 16_777_216.0 * h;
                    let x = f64::from(num(6 + i * 8)) / 16_777_216.0 * w;
                    (x, y)
                };
                if let Some(s) = subs.last_mut() {
                    s.knots.push([pt(0), pt(1), pt(2)]);
                }
            }
            _ => {}
        }
    }
    subs.retain(|s| !s.knots.is_empty());
    Some(VMask {
        subs,
        invert: flags & 1 != 0,
        disabled: flags & 4 != 0,
    })
}

/// Box of the path, handles included.
fn path_box(subs: &[Sub]) -> Option<[f64; 4]> {
    let pts = subs.iter().flat_map(|s| s.knots.iter().flatten());
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(x, y) in pts {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    (x1 >= x0 && y1 >= y0).then_some([x0, y0, x1 - x0, y1 - y0])
}

#[derive(Debug, PartialEq)]
enum Prim {
    /// Corner radius.
    Rect(f64),
    Ellipse,
}

/// A rectangle (rounded or not) or an ellipse drawn as a path, with its box.
fn primitive(subs: &[Sub]) -> Option<(Prim, [f64; 4])> {
    let [s] = subs else { return None };
    if !s.closed {
        return None;
    }
    let anchors: Vec<(f64, f64)> = s.knots.iter().map(|k| k[1]).collect();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(x, y) in &anchors {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let (w, h) = (x1 - x0, y1 - y0);
    if w < 0.5 || h < 0.5 {
        return None;
    }
    let near = |a: f64, b: f64| (a - b).abs() < 0.5;
    let straight = s.knots.iter().all(|k| {
        near(k[0].0, k[1].0) && near(k[0].1, k[1].1) && near(k[2].0, k[1].0) && near(k[2].1, k[1].1)
    });
    let bb = [x0, y0, w, h];
    let corner = |(x, y): (f64, f64)| (near(x, x0) || near(x, x1)) && (near(y, y0) || near(y, y1));
    let on_edge = |(x, y): (f64, f64)| near(x, x0) || near(x, x1) || near(y, y0) || near(y, y1);
    match anchors.len() {
        4 if straight && anchors.iter().all(|&p| corner(p)) => Some((Prim::Rect(0.0), bb)),
        4 if !straight => {
            let (cx, cy) = (x0 + w / 2.0, y0 + h / 2.0);
            let mid = |(x, y): (f64, f64)| {
                (near(x, cx) && (near(y, y0) || near(y, y1)))
                    || (near(y, cy) && (near(x, x0) || near(x, x1)))
            };
            anchors
                .iter()
                .all(|&p| mid(p))
                .then_some((Prim::Ellipse, bb))
        }
        8 if anchors.iter().all(|&p| on_edge(p) && !corner(p)) => {
            // Distance of each anchor to the nearest corner along its side.
            let r: Vec<f64> = anchors
                .iter()
                .map(|&(x, y)| {
                    if near(y, y0) || near(y, y1) {
                        (x - x0).abs().min((x - x1).abs())
                    } else {
                        (y - y0).abs().min((y - y1).abs())
                    }
                })
                .collect();
            let avg = r.iter().sum::<f64>() / 8.0;
            (r.iter().all(|v| (v - avg).abs() < 1.0) && avg <= w.min(h) / 2.0 + 0.5)
                .then_some((Prim::Rect((avg * 100.0).round() / 100.0), bb))
        }
        _ => None,
    }
}

fn skia_path(subs: &[Sub], dx: f64, dy: f64) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    let p = |(x, y): (f64, f64)| ((x - dx) as f32, (y - dy) as f32);
    for s in subs {
        let k = &s.knots;
        let (x, y) = p(k[0][1]);
        pb.move_to(x, y);
        let seg = |pb: &mut tiny_skia::PathBuilder, a: &[(f64, f64); 3], b: &[(f64, f64); 3]| {
            let (c1, c2, e) = (p(a[2]), p(b[0]), p(b[1]));
            pb.cubic_to(c1.0, c1.1, c2.0, c2.1, e.0, e.1);
        };
        for w in k.windows(2) {
            seg(&mut pb, &w[0], &w[1]);
        }
        if s.closed {
            seg(&mut pb, &k[k.len() - 1], &k[0]);
            pb.close();
        }
    }
    pb.finish()
}

fn skia_color(c: Color) -> tiny_skia::Color {
    let [r, g, b, a] = c.0.map(|v| v.clamp(0.0, 1.0));
    tiny_skia::Color::from_rgba(r, g, b, a).unwrap_or(tiny_skia::Color::BLACK)
}

/// The paint of a fill over a box (pixmap pixels).
fn shader(fill: &Fill, b: [f32; 4]) -> Option<tiny_skia::Shader<'static>> {
    use tiny_skia::{LinearGradient, Point, RadialGradient, SpreadMode, Transform};
    match fill {
        Fill::Solid(c) => Some(tiny_skia::Shader::SolidColor(skia_color(*c))),
        Fill::Pattern => None,
        Fill::Gradient(g) => {
            let stops = g
                .stops
                .iter()
                .map(|(at, c)| tiny_skia::GradientStop::new(*at as f32, skia_color(*c)))
                .collect();
            let (cx, cy) = (b[0] + b[2] / 2.0, b[1] + b[3] / 2.0);
            if g.kind == GradKind::Radial {
                let r = (b[2].hypot(b[3]) / 2.0).max(1.0);
                RadialGradient::new(
                    Point::from_xy(cx, cy),
                    Point::from_xy(cx, cy),
                    r,
                    stops,
                    SpreadMode::Pad,
                    Transform::identity(),
                )
            } else {
                let a = (g.angle as f32).to_radians();
                let (dx, dy) = (a.cos(), -a.sin());
                let len = ((b[2] * dx).abs() + (b[3] * dy).abs()).max(1.0) / 2.0;
                LinearGradient::new(
                    Point::from_xy(cx - dx * len, cy - dy * len),
                    Point::from_xy(cx + dx * len, cy + dy * len),
                    stops,
                    SpreadMode::Pad,
                    Transform::identity(),
                )
            }
        }
    }
}

fn from_pixmap(pm: &tiny_skia::Pixmap, x: i64, y: i64) -> Px {
    let rgba = pm
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Px {
        x,
        y,
        w: pm.width() as usize,
        h: pm.height() as usize,
        rgba,
    }
}

/// A fill painted over `area` (x, y, w, h), inside a path when given.
fn paint(area: [i64; 4], fill: &Fill, subs: Option<&[Sub]>, vs: &VStroke) -> Option<Px> {
    let mut pm = tiny_skia::Pixmap::new(area[2] as u32, area[3] as u32)?;
    let (dx, dy) = (area[0] as f64, area[1] as f64);
    let tf = tiny_skia::Transform::identity();
    match subs {
        Some(subs) => {
            let path = skia_path(subs, dx, dy)?;
            let b = path.bounds();
            let rule = if subs.iter().any(|s| s.op == 0 || s.op == 2) {
                tiny_skia::FillRule::EvenOdd
            } else {
                tiny_skia::FillRule::Winding
            };
            if vs.fill {
                let paint = tiny_skia::Paint {
                    shader: shader(fill, [b.x(), b.y(), b.width(), b.height()])?,
                    anti_alias: true,
                    ..Default::default()
                };
                pm.fill_path(&path, &paint, rule, tf, None);
            }
            if let Some((w, c, _)) = vs.stroke {
                let mut paint = tiny_skia::Paint::default();
                paint.set_color(skia_color(c));
                paint.anti_alias = true;
                let stroke = tiny_skia::Stroke {
                    width: w as f32,
                    ..Default::default()
                };
                pm.stroke_path(&path, &paint, &stroke, tf, None);
            }
        }
        None => {
            let paint = tiny_skia::Paint {
                shader: shader(fill, [0.0, 0.0, area[2] as f32, area[3] as f32])?,
                ..Default::default()
            };
            let r = tiny_skia::Rect::from_xywh(0.0, 0.0, area[2] as f32, area[3] as f32)?;
            pm.fill_rect(r, &paint, tf, None);
        }
    }
    Some(from_pixmap(&pm, area[0], area[1]))
}

// ---------------------------------------------------------------- text ------

/// A value of the text engine data (PostScript-like).
#[derive(Debug, Clone, PartialEq)]
enum E {
    Dict(Vec<(String, E)>),
    List(Vec<E>),
    Num(f64),
    Bool(bool),
    Str(String),
    Name(String),
}

impl E {
    fn get(&self, k: &str) -> Option<&E> {
        match self {
            E::Dict(v) => v.iter().find(|(x, _)| x == k).map(|(_, v)| v),
            _ => None,
        }
    }
    fn at(&self, i: usize) -> Option<&E> {
        match self {
            E::List(v) => v.get(i),
            _ => None,
        }
    }
    fn path(&self, keys: &[&str]) -> Option<&E> {
        keys.iter().try_fold(self, |e, k| e.get(k))
    }
    fn num(&self) -> Option<f64> {
        match self {
            E::Num(n) => Some(*n).filter(|n| n.is_finite()),
            _ => None,
        }
    }
    fn bool(&self) -> Option<bool> {
        match self {
            E::Bool(b) => Some(*b),
            _ => None,
        }
    }
    fn str(&self) -> Option<&str> {
        match self {
            E::Str(s) => Some(s),
            _ => None,
        }
    }
    fn list(&self) -> &[E] {
        match self {
            E::List(v) => v,
            _ => &[],
        }
    }
}

fn engine(b: &[u8]) -> Option<E> {
    let mut p = 0;
    eval(b, &mut p, 0)
}

fn ews(b: &[u8], p: &mut usize) {
    while b
        .get(*p)
        .is_some_and(|c| c.is_ascii_whitespace() || *c == 0)
    {
        *p += 1;
    }
}

fn word(b: &[u8], p: &mut usize) -> String {
    let start = *p;
    while b
        .get(*p)
        .is_some_and(|c| !c.is_ascii_whitespace() && *c != 0 && !b"[]<>()/".contains(c))
    {
        *p += 1;
    }
    String::from_utf8_lossy(&b[start..*p]).into_owned()
}

fn eval(b: &[u8], p: &mut usize, depth: usize) -> Option<E> {
    if depth > MAX_NEST {
        return None;
    }
    ews(b, p);
    match *b.get(*p)? {
        b'<' if b.get(*p + 1) == Some(&b'<') => {
            *p += 2;
            let mut items = Vec::new();
            loop {
                ews(b, p);
                match *b.get(*p)? {
                    b'>' => {
                        *p += 2;
                        break;
                    }
                    b'/' => {
                        *p += 1;
                        let k = word(b, p);
                        let v = eval(b, p, depth + 1)?;
                        items.push((k, v));
                    }
                    _ => return None,
                }
            }
            Some(E::Dict(items))
        }
        b'[' => {
            *p += 1;
            let mut v = Vec::new();
            loop {
                ews(b, p);
                if *b.get(*p)? == b']' {
                    *p += 1;
                    break;
                }
                v.push(eval(b, p, depth + 1)?);
            }
            Some(E::List(v))
        }
        b'(' => {
            *p += 1;
            let mut raw = Vec::new();
            loop {
                let c = *b.get(*p)?;
                *p += 1;
                match c {
                    b'\\' => {
                        raw.push(*b.get(*p)?);
                        *p += 1;
                    }
                    b')' => break,
                    _ => raw.push(c),
                }
            }
            Some(E::Str(match raw.strip_prefix(&[0xfe, 0xff]) {
                Some(u) => utf16(u),
                None => raw.iter().map(|&c| char::from(c)).collect(),
            }))
        }
        b'/' => {
            *p += 1;
            Some(E::Name(word(b, p)))
        }
        _ => {
            let w = word(b, p);
            match w.as_str() {
                "" => None,
                "true" => Some(E::Bool(true)),
                "false" => Some(E::Bool(false)),
                _ => Some(w.parse().map(E::Num).unwrap_or(E::Name(w))),
            }
        }
    }
}

/// What a text layer holds.
struct TextDoc {
    text: String,
    /// xx, xy, yx, yy, tx, ty.
    t: [f64; 6],
    /// Left, top, right, bottom, before the transform.
    bounds: Option<[f64; 4]>,
    engine: Option<E>,
}

fn type_tool(data: &[u8]) -> Option<TextDoc> {
    let mut r = Rd::new(data);
    r.u16().ok()?;
    let mut t = [0.0; 6];
    for v in &mut t {
        *v = r.f64().ok().filter(|v| v.is_finite())?;
    }
    r.u16().ok()?;
    r.u32().ok()?;
    let d = descriptor(&mut r, 0).ok()?;
    let engine = match d.get("EngineData") {
        Some(D::Data(b)) => engine(b),
        _ => None,
    };
    let text = d
        .get("Txt")
        .and_then(D::text)
        .map(str::to_string)
        .or_else(|| {
            engine
                .as_ref()?
                .path(&["EngineDict", "Editor", "Text"])?
                .str()
                .map(str::to_string)
        })
        .unwrap_or_default()
        .replace(['\r', '\u{3}'], "\n")
        .trim_end_matches('\n')
        .to_string();
    let bounds = d
        .get("bounds")
        .or_else(|| d.get("boundingBox"))
        .and_then(|b| {
            let n = |k: &str| b.get(k).and_then(D::num);
            Some([n("Left")?, n("Top")?, n("Rght")?, n("Btom")?])
        });
    Some(TextDoc {
        text,
        t,
        bounds,
        engine,
    })
}

/// The look of a text (its first style run).
struct Style {
    font: String,
    size: f64,
    color: Color,
    leading: Option<f64>,
    tracking: f64,
    caps: bool,
    bold: bool,
    italic: bool,
    underline: bool,
    /// The runs differ (font, size or colour).
    mixed: bool,
    align: TextAlign,
    justify: bool,
    /// Paragraph (box) text.
    paragraph: bool,
}

fn text_style(e: Option<&E>) -> Style {
    let res = e.and_then(|e| e.get("ResourceDict").or_else(|| e.get("DocumentResources")));
    let default = res
        .and_then(|r| r.get("StyleSheetSet"))
        .and_then(|s| s.at(0))
        .and_then(|s| s.get("StyleSheetData"));
    let runs: Vec<&E> = e
        .and_then(|e| e.path(&["EngineDict", "StyleRun", "RunArray"]))
        .map(E::list)
        .unwrap_or_default()
        .iter()
        .filter_map(|r| r.path(&["StyleSheet", "StyleSheetData"]))
        .collect();
    let first = runs.first().copied();
    let val = |k: &str| {
        first
            .and_then(|d| d.get(k))
            .or_else(|| default.and_then(|d| d.get(k)))
    };
    let mixed = runs.iter().any(|r| {
        ["Font", "FontSize", "FillColor"]
            .iter()
            .any(|k| r.get(k) != first.and_then(|f| f.get(k)))
    });
    let font = val("Font")
        .and_then(E::num)
        .and_then(|i| {
            res?.get("FontSet")?
                .at(i as usize)?
                .get("Name")?
                .str()
                .map(str::to_string)
        })
        .unwrap_or_default();
    let color = val("FillColor")
        .and_then(|c| c.get("Values"))
        .map(E::list)
        .filter(|v| v.len() >= 4)
        .map(|v| {
            let c = |i: usize| v[i].num().unwrap_or(0.0).clamp(0.0, 1.0) as f32;
            Color::rgba(c(1), c(2), c(3), c(0))
        })
        .unwrap_or(Color::BLACK);
    let auto = val("AutoLeading").and_then(E::bool).unwrap_or(true);
    let flag = |k: &str| val(k).and_then(E::bool).unwrap_or(false);
    let para = e
        .and_then(|e| e.path(&["EngineDict", "ParagraphRun", "RunArray"]))
        .and_then(|r| r.at(0))
        .and_then(|r| r.path(&["ParagraphSheet", "Properties", "Justification"]))
        .or_else(|| {
            res?.get("ParagraphSheetSet")?
                .at(0)?
                .path(&["Properties", "Justification"])
        })
        .and_then(E::num)
        .unwrap_or(0.0) as i64;
    let (align, justify) = match para {
        1 => (TextAlign::Right, false),
        2 => (TextAlign::Center, false),
        3..=6 => (TextAlign::Left, true),
        _ => (TextAlign::Left, false),
    };
    let paragraph = e
        .and_then(|e| e.path(&["EngineDict", "Rendered", "Shapes", "Children"]))
        .and_then(|c| c.at(0))
        .and_then(|c| c.get("ShapeType"))
        .and_then(E::num)
        == Some(1.0);
    Style {
        font,
        size: val("FontSize").and_then(E::num).unwrap_or(12.0),
        color,
        leading: if auto {
            None
        } else {
            val("Leading").and_then(E::num).filter(|l| *l > 0.0)
        },
        tracking: val("Tracking").and_then(E::num).unwrap_or(0.0),
        caps: val("FontCaps").and_then(E::num) == Some(2.0),
        bold: flag("FauxBold"),
        italic: flag("FauxItalic"),
        underline: flag("Underline"),
        mixed,
        align,
        justify,
        paragraph,
    }
}

/// Family, weight and italic of a PostScript font name ("Montserrat-SemiBold").
fn font_parts(ps: &str) -> (String, u16, bool) {
    let (family, style) = ps.split_once('-').unwrap_or((ps, ""));
    let mut family = family.trim().to_string();
    for suffix in ["PSMT", "MT", "PS"] {
        if family.len() > suffix.len() + 2 && family.ends_with(suffix) {
            family.truncate(family.len() - suffix.len());
            break;
        }
    }
    let s = style.to_lowercase().replace([' ', '_'], "");
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
    .find(|(k, _)| s.contains(k))
    .map_or(400, |(_, w)| *w);
    (
        family,
        weight,
        s.contains("italic") || s.contains("oblique"),
    )
}

// --------------------------------------------------------------- converter --

/// What a layer is.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Group,
    Adjustment,
    Text,
    Shape,
    Fill,
    Smart,
    Pixel,
}

impl Kind {
    fn of(raw: &Raw, group: bool) -> Kind {
        let has = |k: &[u8; 4]| raw.info(k).is_some();
        if group {
            Kind::Group
        } else if ADJUSTMENTS.iter().any(|k| has(k)) {
            Kind::Adjustment
        } else if has(b"TySh") {
            Kind::Text
        } else if has(b"SoCo") || has(b"GdFl") || has(b"PtFl") || has(b"vscg") {
            if has(b"vmsk") || has(b"vsms") {
                Kind::Shape
            } else {
                Kind::Fill
            }
        } else if has(b"SoLd") || has(b"PlLd") || has(b"SoLE") {
            Kind::Smart
        } else {
            Kind::Pixel
        }
    }
    fn source(self) -> &'static str {
        match self {
            Kind::Group => "group",
            Kind::Adjustment => "adjustment",
            Kind::Text => "text",
            Kind::Shape => "shape",
            Kind::Fill => "fill",
            Kind::Smart => "smartObject",
            Kind::Pixel => "pixel",
        }
    }
}

/// English name of a blend mode other than normal.
fn blend_name(k: &[u8; 4]) -> Option<&'static str> {
    Some(match k {
        b"norm" | b"pass" => return None,
        b"diss" => "Dissolve",
        b"dark" => "Darken",
        b"mul " => "Multiply",
        b"idiv" => "Color burn",
        b"lbrn" => "Linear burn",
        b"dkCl" => "Darker color",
        b"lite" => "Lighten",
        b"scrn" => "Screen",
        b"div " => "Color dodge",
        b"lddg" => "Linear dodge",
        b"lgCl" => "Lighter color",
        b"over" => "Overlay",
        b"sLit" => "Soft light",
        b"hLit" => "Hard light",
        b"vLit" => "Vivid light",
        b"lLit" => "Linear light",
        b"pLit" => "Pin light",
        b"hMix" => "Hard mix",
        b"diff" => "Difference",
        b"smud" => "Exclusion",
        b"fsub" => "Subtract",
        b"fdiv" => "Divide",
        b"hue " => "Hue",
        b"sat " => "Saturation",
        b"colr" => "Color",
        b"lum " => "Luminosity",
        _ => return None,
    })
}

fn layer(name: &str, kind: LayerKind) -> Layer {
    Layer {
        id: new_id(),
        name: name.to_string(),
        parent: None,
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

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0 + 0.0
}

/// Lower case ASCII letters and digits, the rest as `sep`.
fn slug(s: &str, sep: char) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ÿ' => 'y',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with(sep) {
            out.push(sep);
        }
        if out.len() >= 32 {
            break;
        }
    }
    out.trim_end_matches(sep).to_string()
}

/// A converted layer, with its children for a group.
struct Built {
    layer: Layer,
    /// Box in the document: x, y, w, h.
    rect: [f64; 4],
    report: LayerReport,
    kids: Vec<Built>,
}

/// The reports, top first, groups before their children.
fn report(items: &[Built], out: &mut Vec<LayerReport>) {
    for b in items.iter().rev() {
        out.push(b.report.clone());
        report(&b.kids, out);
    }
}

/// The layers in paint order, placed in their parent's box.
fn place(items: Vec<Built>, parent: Option<&str>, origin: (f64, f64), out: &mut Vec<Layer>) {
    for mut b in items {
        b.layer.parent = parent.map(str::to_string);
        set(&mut b.layer, Prop::X, round2(b.rect[0] - origin.0));
        set(&mut b.layer, Prop::Y, round2(b.rect[1] - origin.1));
        set(&mut b.layer, Prop::Width, round2(b.rect[2].max(1.0)));
        set(&mut b.layer, Prop::Height, round2(b.rect[3].max(1.0)));
        let id = b.layer.id.clone();
        out.push(b.layer);
        place(b.kids, Some(&id), (b.rect[0], b.rect[1]), out);
    }
}

/// The conversion of one template (an artboard or the document).
struct Cx<'d, 'a> {
    doc: &'d Doc<'a>,
    ctx: &'d Ctx,
    files: &'d mut Vec<(String, Vec<u8>)>,
    fields: Vec<Field>,
    ignored: Vec<Ignored>,
    /// The template's box in the document: x, y, w, h.
    area: [f64; 4],
}

impl Cx<'_, '_> {
    fn ignore(&mut self, raw: &Raw, source: &str, reason: &str) {
        self.ignored.push(Ignored::new(&raw.name, source, reason));
    }

    fn ignore_tree(&mut self, n: &Node, reason: &str) {
        let raw = &self.doc.layers[n.i];
        let kind = Kind::of(raw, n.kids.is_some());
        self.ignore(raw, kind.source(), reason);
        for k in n.kids.iter().flatten() {
            self.ignore_tree(k, reason);
        }
    }

    /// Stages a picture; its URL.
    fn file(&mut self, name: &str, px: &Px) -> Option<String> {
        let png = px.png().ok()?;
        let mut s = slug(name, '-');
        if s.is_empty() {
            s = "layer".into();
        }
        let file = format!("psd-{s}-{}.png", self.files.len() + 1);
        self.files.push((file.clone(), png));
        Some(format!("{}{file}", self.ctx.prefix))
    }

    /// Converts siblings, bottom first, applying clipping masks.
    fn list(&mut self, nodes: &[&Node], depth: u32) -> Vec<Built> {
        let mut out = Vec::new();
        let mut base: Option<usize> = None;
        let mut cover: Option<Option<Px>> = None;
        for n in nodes {
            let raw = &self.doc.layers[n.i];
            let clipped = raw.clipping && base.is_some();
            if !raw.clipping {
                base = Some(n.i);
                cover = None;
            }
            let clip = if clipped {
                let b = base.unwrap_or(n.i);
                cover.get_or_insert_with(|| self.cover(b)).as_ref()
            } else {
                None
            };
            if let Some(b) = self.node(n, depth, clipped, clip) {
                out.push(b);
            }
        }
        out
    }

    /// The picture a clipping base shows (none for texts and groups).
    fn cover(&self, i: usize) -> Option<Px> {
        let raw = &self.doc.layers[i];
        match Kind::of(raw, raw.section() == 1 || raw.section() == 2) {
            Kind::Group | Kind::Text | Kind::Adjustment => None,
            kind => self.raster(raw, kind, &mut Vec::new()).ok().flatten(),
        }
    }

    fn node(&mut self, n: &Node, depth: u32, clipped: bool, clip: Option<&Px>) -> Option<Built> {
        let raw = &self.doc.layers[n.i];
        let kind = Kind::of(raw, n.kids.is_some());
        let mut notes = Vec::new();
        match kind {
            Kind::Adjustment => {
                self.ignore(raw, "adjustment", "Adjustment layer");
                None
            }
            Kind::Group => {
                let kids: Vec<&Node> = n.kids.iter().flatten().collect();
                let kids = self.list(&kids, depth + 1);
                let rect = kids.iter().map(|k| k.rect).reduce(union).unwrap_or([
                    self.area[0],
                    self.area[1],
                    1.0,
                    1.0,
                ]);
                if raw.mask.as_ref().is_some_and(|m| !m.disabled) && raw.channel(-2).is_some()
                    || vector_mask(raw, self.doc).is_some_and(|v| !v.disabled)
                {
                    notes.push(Note::new("Group mask", Status::Ignored));
                }
                if clipped {
                    notes.push(Note::new("Clipping mask", Status::Partial));
                }
                let l = layer(
                    &raw.name,
                    LayerKind::Group {
                        clip: false,
                        lottie: None,
                    },
                );
                Some(self.finish(raw, l, rect, kind, notes, depth, kids))
            }
            Kind::Text => {
                let (l, rect) = self.text(raw, &mut notes)?;
                if clipped {
                    notes.push(Note::new("Clipping mask", Status::Partial));
                }
                if raw.mask.as_ref().is_some_and(|m| !m.disabled) && raw.channel(-2).is_some() {
                    notes.push(Note::new("Mask", Status::Ignored));
                }
                Some(self.finish(raw, l, rect, kind, notes, depth, Vec::new()))
            }
            Kind::Fill | Kind::Shape if !clipped && !self.mask_matters(raw) => {
                if let Some((l, rect)) = self.vector(raw, kind) {
                    return Some(self.finish(raw, l, rect, kind, notes, depth, Vec::new()));
                }
                self.picture(raw, kind, notes, depth, clipped, clip)
            }
            _ => self.picture(raw, kind, notes, depth, clipped, clip),
        }
    }

    /// A rectangle or ellipse for a fill or a simple shape.
    fn vector(&self, raw: &Raw, kind: Kind) -> Option<(Layer, [f64; 4])> {
        let (fill, style) = match fill_of(raw)? {
            Fill::Solid(c) => (c, FillStyle::default()),
            Fill::Gradient(g) if g.kind != GradKind::Other => fill_style(&g),
            _ => return None,
        };
        if kind == Kind::Fill {
            let mut l = layer(
                &raw.name,
                LayerKind::Rect {
                    x: quad::Config {
                        fill_style: style,
                        ..Default::default()
                    },
                },
            );
            l.props.insert(Prop::Fill, Value::Color(fill));
            return Some((l, self.area));
        }
        let vm = vector_mask(raw, self.doc)?;
        if vm.invert || vm.disabled {
            return None;
        }
        let (prim, rect) = primitive(&vm.subs)?;
        let vs = vstroke(raw);
        let stroke_style = StrokeStyle {
            position: vs.stroke.map_or(StrokePosition::Inside, |s| s.2),
            ..Default::default()
        };
        let lk = match prim {
            Prim::Ellipse => LayerKind::Ellipse {
                x: ellipse::Config {
                    fill_style: style,
                    stroke_style,
                    ..Default::default()
                },
            },
            Prim::Rect(_) => LayerKind::Rect {
                x: quad::Config {
                    fill_style: style,
                    stroke_style,
                    ..Default::default()
                },
            },
        };
        let mut l = layer(&raw.name, lk);
        l.props.insert(
            Prop::Fill,
            Value::Color(if vs.fill { fill } else { Color::TRANSPARENT }),
        );
        if let Prim::Rect(r) = prim {
            if r > 0.0 {
                set(&mut l, Prop::CornerRadius, r);
            }
        }
        if let Some((w, c, _)) = vs.stroke {
            l.props.insert(Prop::Stroke, Value::Color(c));
            set(&mut l, Prop::StrokeWidth, round2(w));
        }
        if prim == Prim::Ellipse {
            l.lock_ratio = (rect[2] - rect[3]).abs() < 0.5;
        }
        Some((l, rect))
    }

    /// A layer drawn as a picture.
    fn picture(
        &mut self,
        raw: &Raw,
        kind: Kind,
        mut notes: Vec<Note>,
        depth: u32,
        clipped: bool,
        clip: Option<&Px>,
    ) -> Option<Built> {
        let px = match self.raster(raw, kind, &mut notes) {
            Ok(Some(px)) => px,
            Ok(None) => {
                self.ignore(raw, kind.source(), "Empty layer");
                return None;
            }
            Err(reason) => {
                self.ignore(raw, kind.source(), reason);
                return None;
            }
        };
        let mut px = px;
        if clipped {
            match clip {
                Some(base) => {
                    px.mul_alpha(|x, y| base.alpha(x, y));
                    notes.push(Note::new("Clipping mask applied", Status::Partial));
                }
                None => notes.push(Note::new("Clipping mask", Status::Partial)),
            }
        }
        let Some(px) = px.trim() else {
            self.ignore(raw, kind.source(), "Empty layer");
            return None;
        };
        let src = self.file(&raw.name, &px)?;
        let mut l = layer(
            &raw.name,
            LayerKind::Image {
                src,
                fit: ImageFit::Contain,
                x: Default::default(),
            },
        );
        l.lock_ratio = true;
        Some(self.finish(raw, l, px.rect(), kind, notes, depth, Vec::new()))
    }

    /// The picture of a layer, its masks applied (notes say so).
    fn raster(
        &self,
        raw: &Raw,
        kind: Kind,
        notes: &mut Vec<Note>,
    ) -> Result<Option<Px>, &'static str> {
        let doc = self.doc;
        let mut px = None;
        let vm = vector_mask(raw, doc);
        if matches!(kind, Kind::Fill | Kind::Shape) {
            let fill = fill_of(raw);
            let vs = vstroke(raw);
            let subs = vm
                .as_ref()
                .filter(|v| kind == Kind::Shape && !v.disabled && !v.invert)
                .map(|v| v.subs.as_slice());
            let area = match subs {
                Some(s) => {
                    let pad = vs.stroke.map_or(0.0, |s| s.0) + 2.0;
                    path_box(s).map(|b| {
                        irect([b[0] - pad, b[1] - pad, b[2] + 2.0 * pad, b[3] + 2.0 * pad])
                    })
                }
                None => Some(irect(self.area)),
            };
            if let (Some(fill), Some(area)) = (&fill, area) {
                if (area[2].max(0) as u64) * (area[3].max(0) as u64) > MAX_PIXELS {
                    return Err("Layer too large");
                }
                px = paint(area, fill, subs, &vs);
                if px.is_some() {
                    match (kind, fill) {
                        (Kind::Shape, _) => {
                            notes.push(Note::new("Vector shape rasterised", Status::Partial))
                        }
                        (_, Fill::Gradient(_)) => {
                            notes.push(Note::new("Gradient rasterised", Status::Partial))
                        }
                        _ => notes.push(Note::new("Fill rasterised", Status::Partial)),
                    }
                }
            }
        }
        if px.is_none() {
            let (w, h) = raw.size();
            if (w as u64) * (h as u64) > MAX_PIXELS {
                return Err("Layer too large");
            }
            px = layer_px(doc, raw).map_err(|_| "Unreadable pixels")?;
            if px.is_some() {
                match kind {
                    Kind::Smart => notes.push(Note::new("Smart object → picture", Status::Partial)),
                    Kind::Fill | Kind::Shape => {
                        notes.push(Note::new("Fill → picture", Status::Partial))
                    }
                    _ => {}
                }
            }
        }
        let Some(mut px) = px else { return Ok(None) };
        if self.apply_mask(raw, &mut px) {
            notes.push(Note::new("Mask applied", Status::Partial));
        }
        if let Some(vm) = vm.filter(|v| !v.disabled && kind != Kind::Shape) {
            let mut pm =
                tiny_skia::Pixmap::new(px.w as u32, px.h as u32).ok_or("Layer too large")?;
            if let Some(path) = skia_path(&vm.subs, px.x as f64, px.y as f64) {
                let mut paint = tiny_skia::Paint::default();
                paint.set_color(tiny_skia::Color::WHITE);
                paint.anti_alias = true;
                pm.fill_path(
                    &path,
                    &paint,
                    tiny_skia::FillRule::EvenOdd,
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
            let m = from_pixmap(&pm, px.x, px.y);
            px.mul_alpha(|x, y| {
                let a = m.alpha(x, y);
                if vm.invert {
                    255 - a
                } else {
                    a
                }
            });
            notes.push(Note::new("Vector mask applied", Status::Partial));
        }
        Ok(Some(px))
    }

    /// The decoded user mask: box (x, y, w, h), values and default.
    fn mask_plane(&self, raw: &Raw) -> Option<([i64; 4], Vec<u8>, u8)> {
        let m = raw.mask.as_ref().filter(|m| !m.disabled)?;
        let data = raw.channel(-2)?;
        let r = [
            m.rect[0],
            m.rect[1],
            m.rect[2] - m.rect[0],
            m.rect[3] - m.rect[1],
        ];
        let (w, h) = (r[2].max(0) as usize, r[3].max(0) as usize);
        if (w as u64) * (h as u64) > MAX_PIXELS {
            return None;
        }
        let v = plane(data, w, h, h, self.doc, 0).ok()?;
        Some((r, v, m.default))
    }

    /// Applies the user mask; true when it hid something.
    fn apply_mask(&self, raw: &Raw, px: &mut Px) -> bool {
        let Some((r, v, default)) = self.mask_plane(raw) else {
            return false;
        };
        px.mul_alpha(|x, y| {
            let (dx, dy) = (x - r[0], y - r[1]);
            if dx >= 0 && dy >= 0 && dx < r[2] && dy < r[3] {
                v[(dy * r[2] + dx) as usize]
            } else {
                default
            }
        })
    }

    /// The user mask hides part of the template's box.
    fn mask_matters(&self, raw: &Raw) -> bool {
        let Some((r, v, default)) = self.mask_plane(raw) else {
            return false;
        };
        let a = irect(self.area);
        let covers = r[0] <= a[0]
            && r[1] <= a[1]
            && r[0] + r[2] >= a[0] + a[2]
            && r[1] + r[3] >= a[1] + a[3];
        v.iter().any(|&m| m < 255) || (default < 255 && !covers)
    }

    /// A text layer, and the field it becomes.
    fn text(&mut self, raw: &Raw, notes: &mut Vec<Note>) -> Option<(Layer, [f64; 4])> {
        let Some(td) = raw.info(b"TySh").and_then(type_tool) else {
            self.ignore(raw, "text", "Unreadable text");
            return None;
        };
        let st = text_style(td.engine.as_ref());
        let t = td.t;
        let sx = t[0].hypot(t[1]).max(1e-6);
        let sy = t[2].hypot(t[3]).max(1e-6);
        let rotation = t[1].atan2(t[0]).to_degrees();
        let size = Some(round2(st.size * sy))
            .filter(|s| (0.1..=10_000.0).contains(s))
            .unwrap_or(12.0);
        let lines = td.text.lines().count().max(1) as f64;
        let lh = st.leading.map_or(1.2, |l| l / st.size.max(1e-6));
        let rect = td
            .bounds
            .filter(|[l, tp, r, b]| r > l && b > tp)
            .map(|[l, tp, r, b]| {
                let (cx, cy) = ((l + r) / 2.0, (tp + b) / 2.0);
                let (x, y) = (t[0] * cx + t[2] * cy + t[4], t[1] * cx + t[3] * cy + t[5]);
                let (mut w, h) = ((r - l) * sx, ((b - tp) * sy).max(lines * lh * size));
                let mut x = x - w / 2.0;
                if !st.paragraph {
                    // Room for a wider replacement font.
                    let extra = w * 0.25;
                    w += extra;
                    match st.align {
                        TextAlign::Left => {}
                        TextAlign::Center => x -= extra / 2.0,
                        TextAlign::Right => x -= extra,
                    }
                }
                [x, y - h / 2.0, w, h]
            })
            // A transform beyond reason: the pixels' box.
            .filter(|r| r.iter().all(|v| v.abs() < 1e6))
            .unwrap_or_else(|| {
                let (w, h) = raw.size();
                [raw.rect[0] as f64, raw.rect[1] as f64, w as f64, h as f64]
            });
        let (family, mut weight, italic) = font_parts(&st.font);
        let (font, found) = self.ctx.font(&family);
        if !found {
            notes.push(
                Note::new("Font {from} → {to}", Status::Partial)
                    .with("from", if family.is_empty() { "?" } else { &family })
                    .with("to", font.clone()),
            );
        }
        if st.bold {
            weight = weight.max(700);
        }
        if st.mixed {
            notes.push(Note::new("Text styles simplified", Status::Partial));
        }
        // The text becomes a variable of the template.
        let mut key = slug(&raw.name, '_');
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
            label: raw.name.clone(),
            kind: if td.text.contains('\n') {
                FieldKind::Multiline
            } else {
                FieldKind::Text
            },
            default: td.text.clone(),
            binding: None,
            min: None,
            max: None,
            options: Vec::new(),
        });
        notes.push(Note::new("Text → variable", Status::Ok));
        let x = cg_core::wid::text::Config {
            italic: st.italic || italic,
            justify: st.justify && st.paragraph,
            ..Default::default()
        };
        let mut l = layer(
            &raw.name,
            LayerKind::Text {
                x,
                wrap: st.paragraph,
                transform: Default::default(),
                decoration: if st.underline {
                    TextDecoration::Underline
                } else {
                    TextDecoration::None
                },
                line_height: st.leading.map(|_| lh as f32),
                scroll: Default::default(),
                speed: 120.0,
                separator: "   •   ".into(),
                looping: false,
                overflow: Default::default(),
                squeeze: 0.8,
                min_size: 0.6,
                warn_below: 0.8,
                text: format!("{{{{{key}}}}}"),
                font_family: font,
                font_weight: weight,
                align: st.align,
                vertical_align: VerticalAlign::Top,
                uppercase: st.caps,
            },
        );
        set(&mut l, Prop::FontSize, size);
        l.props.insert(Prop::Fill, Value::Color(st.color));
        if st.tracking != 0.0 {
            set(
                &mut l,
                Prop::LetterSpacing,
                round2(st.tracking * size / 1000.0),
            );
        }
        if rotation.abs() > 0.01 {
            set(&mut l, Prop::Rotation, round2(rotation));
        }
        Some((l, rect))
    }

    /// Opacity, blend mode, effects and the report of a converted layer.
    #[allow(clippy::too_many_arguments)]
    fn finish(
        &mut self,
        raw: &Raw,
        mut l: Layer,
        rect: [f64; 4],
        kind: Kind,
        mut notes: Vec<Note>,
        depth: u32,
        kids: Vec<Built>,
    ) -> Built {
        let fill = match kind {
            Kind::Group => 255,
            _ => raw
                .info(b"iOpa")
                .and_then(|d| d.first().copied())
                .unwrap_or(255),
        };
        let opacity = f64::from(raw.opacity) / 255.0 * f64::from(fill) / 255.0;
        if opacity < 0.999 {
            set(&mut l, Prop::Opacity, (opacity * 1000.0).round() / 1000.0);
        }
        if let Some(m) = blend_name(&raw.blend) {
            notes.push(Note::new("Blend mode {mode}", Status::Partial).with("mode", m));
        }
        self.effects(raw, &mut l, &mut notes);
        let status = if notes.iter().all(|n| n.status == Status::Ok) {
            Status::Ok
        } else {
            Status::Partial
        };
        Built {
            report: LayerReport {
                layer: l.id.clone(),
                name: raw.name.clone(),
                source: kind.source().into(),
                depth,
                status,
                notes,
                hidden: raw.hidden,
            },
            layer: l,
            rect,
            kids,
        }
    }

    /// Layer effects: the drop shadow (and on texts and shapes, the stroke
    /// and the colour overlay) are kept, the others reported.
    fn effects(&self, raw: &Raw, l: &mut Layer, notes: &mut Vec<Note>) {
        let d = raw
            .info(b"lfx2")
            .or_else(|| raw.info(b"lmfx"))
            .and_then(|d| block_descriptor(d, 8));
        let Some(d) = d else {
            if let Some(old) = raw.info(b"lrFX") {
                legacy_effects(old, notes);
            }
            return;
        };
        if d.get("masterFXSwitch").and_then(D::bool) == Some(false) {
            return;
        }
        let D::Obj(_, items) = &d else { return };
        let vector = matches!(
            l.kind,
            LayerKind::Text { .. } | LayerKind::Rect { .. } | LayerKind::Ellipse { .. }
        );
        let (mut shadow, mut stroke, mut overlay) = (false, false, false);
        for (key, v) in items {
            let name = match key.as_str() {
                "DrSh" | "dropShadowMulti" => "Drop shadow",
                "IrSh" | "innerShadowMulti" => "Inner shadow",
                "OrGl" => "Outer glow",
                "IrGl" => "Inner glow",
                "ebbl" => "Bevel and emboss",
                "ChFX" => "Satin",
                "SoFi" | "solidFillMulti" => "Color overlay",
                "GrFl" | "gradientFillMulti" => "Gradient overlay",
                "patternFill" => "Pattern overlay",
                "FrFX" | "frameFXMulti" => "Stroke",
                _ => continue,
            };
            let each: Vec<&D> = match v {
                D::List(list) => list.iter().collect(),
                other => vec![other],
            };
            for e in each {
                if e.get("enab").and_then(D::bool) == Some(false) {
                    continue;
                }
                let done = match name {
                    "Drop shadow" if !shadow => {
                        shadow = true;
                        self.shadow(e, l, notes)
                    }
                    "Stroke" if vector && !stroke && !l.props.contains_key(&Prop::Stroke) => {
                        stroke = true;
                        effect_stroke(e, l, notes)
                    }
                    "Color overlay" if vector && !overlay => {
                        overlay = true;
                        effect_overlay(e, l, notes)
                    }
                    _ => false,
                };
                if !done {
                    notes.push(Note::new("Effect {name}", Status::Ignored).with("name", name));
                }
            }
        }
    }

    fn shadow(&self, e: &D, l: &mut Layer, notes: &mut Vec<Note>) -> bool {
        let n = |k: &str| e.get(k).and_then(D::num);
        let mut c = e.get("Clr").and_then(dcolor).unwrap_or(Color::BLACK);
        c.0[3] = (n("Opct").unwrap_or(75.0) / 100.0).clamp(0.0, 1.0) as f32;
        let angle = if e.get("uglg").and_then(D::bool).unwrap_or(true) {
            self.doc.angle
        } else {
            n("lagl").unwrap_or(120.0)
        }
        .to_radians();
        let dist = n("Dstn").unwrap_or(5.0);
        let (x, y) = (round2(-angle.cos() * dist), round2(angle.sin() * dist));
        let blur = round2(n("blur").unwrap_or(5.0));
        let exact = n("Ckmt").unwrap_or(0.0) <= 0.0
            && n("Nose").unwrap_or(0.0) <= 0.0
            && matches!(
                e.get("Md").and_then(D::enm),
                None | Some("Mltp") | Some("Nrml")
            );
        if let LayerKind::Text { x: t, .. } = &mut l.kind {
            t.shadow = true;
            t.shadow_color = c.to_hex();
            t.shadow_x = x as f32;
            t.shadow_y = y as f32;
            t.shadow_blur = blur as f32;
        } else {
            l.drop_shadow = true;
            l.props.insert(Prop::ShadowColor, Value::Color(c));
            set(l, Prop::ShadowX, x);
            set(l, Prop::ShadowY, y);
            set(l, Prop::ShadowBlur, blur);
        }
        notes.push(Note::new(
            "Drop shadow",
            if exact { Status::Ok } else { Status::Partial },
        ));
        true
    }

    /// The flat document's picture as its only layer.
    fn background(&mut self, composite: &Px) -> Option<Built> {
        let px = composite.crop(irect(self.area))?.trim()?;
        let src = self.file("Background", &px)?;
        let mut l = layer(
            "Background",
            LayerKind::Image {
                src,
                fit: ImageFit::Contain,
                x: Default::default(),
            },
        );
        l.lock_ratio = true;
        Some(Built {
            report: LayerReport {
                layer: l.id.clone(),
                name: "Background".into(),
                source: "pixel".into(),
                depth: 0,
                status: Status::Ok,
                notes: Vec::new(),
                hidden: false,
            },
            layer: l,
            rect: px.rect(),
            kids: Vec::new(),
        })
    }
}

/// Stroke effect on a text or shape.
fn effect_stroke(e: &D, l: &mut Layer, notes: &mut Vec<Note>) -> bool {
    if e.get("PntT").and_then(D::enm).is_some_and(|p| p != "SClr") {
        return false;
    }
    let Some(mut c) = e.get("Clr").and_then(dcolor) else {
        return false;
    };
    c.0[3] = (e.get("Opct").and_then(D::num).unwrap_or(100.0) / 100.0) as f32;
    let width = e.get("Sz").and_then(D::num).unwrap_or(3.0);
    let pos = match e.get("Styl").and_then(D::enm) {
        Some("InsF") => StrokePosition::Inside,
        Some("CtrF") => StrokePosition::Center,
        _ => StrokePosition::Outside,
    };
    match &mut l.kind {
        LayerKind::Text { x, .. } => x.stroke_outside = pos == StrokePosition::Outside,
        LayerKind::Rect { x } => x.stroke_style.position = pos,
        LayerKind::Ellipse { x } => x.stroke_style.position = pos,
        _ => return false,
    }
    l.props.insert(Prop::Stroke, Value::Color(c));
    set(l, Prop::StrokeWidth, round2(width));
    notes.push(Note::new("Stroke", Status::Ok));
    true
}

/// Colour overlay on a text or shape: its fill colour.
fn effect_overlay(e: &D, l: &mut Layer, notes: &mut Vec<Note>) -> bool {
    let normal = matches!(e.get("Md").and_then(D::enm), None | Some("Nrml"));
    let opaque = e.get("Opct").and_then(D::num).unwrap_or(100.0) >= 99.9;
    let Some(c) = e.get("Clr").and_then(dcolor).filter(|_| normal && opaque) else {
        return false;
    };
    if let LayerKind::Rect { x } = &mut l.kind {
        x.fill_style.kind = FillKind::Solid;
    }
    if let LayerKind::Ellipse { x } = &mut l.kind {
        x.fill_style.kind = FillKind::Solid;
    }
    l.props.insert(Prop::Fill, Value::Color(c));
    notes.push(Note::new("Color overlay", Status::Ok));
    true
}

/// Effects of older documents (`lrFX`): reported only.
fn legacy_effects(data: &[u8], notes: &mut Vec<Note>) {
    let mut r = Rd::new(data);
    let (Ok(_), Ok(n)) = (r.u16(), r.u16()) else {
        return;
    };
    for _ in 0..n.min(16) {
        let (Ok(_), Ok(key), Ok(size)) = (r.arr::<4>(), r.arr::<4>(), r.u32()) else {
            return;
        };
        if r.skip(size as usize).is_err() {
            return;
        }
        let name = match &key {
            b"dsdw" => "Drop shadow",
            b"isdw" => "Inner shadow",
            b"oglw" => "Outer glow",
            b"iglw" => "Inner glow",
            b"bevl" => "Bevel and emboss",
            b"sofi" => "Color overlay",
            _ => continue,
        };
        notes.push(Note::new("Effect {name}", Status::Ignored).with("name", name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Ctx {
        Ctx {
            prefix: "/stage/".into(),
            fonts: vec!["Inter".into(), "Montserrat".into()],
            zone: "main".into(),
        }
    }

    fn fixture(name: &str) -> Vec<u8> {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/import/");
        std::fs::read(format!("{dir}{name}")).expect("fixture")
    }

    fn by_name<'a>(c: &'a Comp, name: &str) -> &'a Layer {
        c.template
            .layers
            .iter()
            .find(|l| l.name == name)
            .unwrap_or_else(|| panic!("no layer {name}"))
    }

    fn rep<'a>(c: &'a Comp, name: &str) -> &'a LayerReport {
        c.layers.iter().find(|r| r.name == name).unwrap()
    }

    fn num(l: &Layer, p: Prop) -> f64 {
        l.props[&p].as_number().unwrap()
    }

    fn pos(l: &Layer) -> [f64; 4] {
        [Prop::X, Prop::Y, Prop::Width, Prop::Height].map(|p| num(l, p))
    }

    /// The decoded picture an image layer (or a flat) points at.
    fn picture(out: &Converted, url: &str) -> image::RgbaImage {
        let name = url.strip_prefix("/stage/").expect("staged");
        let (_, png) = out.files.iter().find(|f| f.0 == name).expect("file");
        image::load_from_memory(png).unwrap().to_rgba8()
    }

    fn src(l: &Layer) -> &str {
        match &l.kind {
            LayerKind::Image { src, .. } => src,
            k => panic!("not an image: {k:?}"),
        }
    }

    #[test]
    fn converts_layers_groups_and_texts() {
        let out = analyze(&fixture("layers.psd"), &ctx()).unwrap();
        assert_eq!(
            (out.format, out.width, out.height, out.fps),
            (Format::Psd, 200, 120, 25.0)
        );
        assert!(out.warnings.is_empty());
        let [c] = out.comps.as_slice() else { panic!() };
        assert_eq!(c.name, "Document");
        assert!(!c.animated && c.duration == 0.0 && c.selected);
        let t = &c.template;
        assert_eq!(t.duration, 25.0);
        let [m] = t.markers.as_slice() else { panic!() };
        assert_eq!((m.kind, m.frame), (MarkerKind::Out, 0.0));

        // The list, top first, groups before their children; each layer once.
        let names: Vec<_> = c
            .layers
            .iter()
            .map(|r| (r.name.as_str(), r.depth))
            .collect();
        assert_eq!(
            names,
            [
                ("Tint", 0),
                ("Dot", 0),
                ("Hidden dot", 0),
                ("Clipped", 0),
                ("Logo", 0),
                ("Lower third", 0),
                ("Subtitle", 1),
                ("Title", 1),
                ("Plate", 1),
                ("Background", 0),
            ]
        );
        let [lv] = c.ignored.as_slice() else { panic!() };
        assert_eq!(
            (lv.name.as_str(), lv.source.as_str(), lv.reason.as_str()),
            ("Levels", "adjustment", "Adjustment layer")
        );
        // Paint order: bottom first, a group before its children.
        let order: Vec<_> = t.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            order,
            [
                "Background",
                "Lower third",
                "Plate",
                "Title",
                "Subtitle",
                "Logo",
                "Clipped",
                "Hidden dot",
                "Dot",
                "Tint"
            ]
        );

        let group = by_name(c, "Lower third");
        assert!(matches!(group.kind, LayerKind::Group { .. }));
        assert_eq!(pos(group), [15.0, 20.0, 177.5, 90.0]);
        assert_eq!(num(group, Prop::Opacity), 0.8);

        // Shape → rectangle in its group, with its drop shadow.
        let plate = by_name(c, "Plate");
        assert!(matches!(plate.kind, LayerKind::Rect { .. }));
        assert_eq!(plate.parent.as_deref(), Some(group.id.as_str()));
        assert_eq!(pos(plate), [5.0, 50.0, 160.0, 40.0]);
        assert_eq!(
            plate.props[&Prop::Fill].as_color().unwrap().to_hex(),
            "#dc143c"
        );
        assert!(plate.drop_shadow);
        assert_eq!(
            (
                num(plate, Prop::ShadowX),
                num(plate, Prop::ShadowY),
                num(plate, Prop::ShadowBlur)
            ),
            (0.0, 4.0, 6.0)
        );
        assert_eq!(
            plate.props[&Prop::ShadowColor].as_color().unwrap().to_hex(),
            "#00000080"
        );
        let r = rep(c, "Plate");
        assert_eq!((r.source.as_str(), r.status), ("shape", Status::Partial));
        let labels: Vec<_> = r
            .notes
            .iter()
            .map(|n| (n.label.as_str(), n.status))
            .collect();
        assert_eq!(
            labels,
            [
                ("Drop shadow", Status::Ok),
                ("Effect {name}", Status::Ignored)
            ]
        );
        assert_eq!(r.notes[1].params["name"], "Outer glow");

        // Texts → variables.
        assert_eq!(
            t.fields
                .iter()
                .map(|f| (f.key.as_str(), f.kind, f.default.as_str()))
                .collect::<Vec<_>>(),
            [
                ("title", FieldKind::Text, "Hello world"),
                ("subtitle", FieldKind::Multiline, "Line one\nLine two")
            ]
        );
        let title = by_name(c, "Title");
        let LayerKind::Text {
            text,
            font_family,
            font_weight,
            align,
            ..
        } = &title.kind
        else {
            panic!()
        };
        assert_eq!(
            (text.as_str(), font_family.as_str(), *font_weight, *align),
            ("{{title}}", "Montserrat", 700, TextAlign::Left)
        );
        assert_eq!(
            (num(title, Prop::FontSize), num(title, Prop::LetterSpacing)),
            (24.0, 1.2)
        );
        assert_eq!(num(title, Prop::X), 15.0);
        let r = rep(c, "Title");
        assert_eq!((r.source.as_str(), r.status), ("text", Status::Ok));
        assert_eq!(r.notes[0].label, "Text → variable");
        // Scaled by its transform; its font replaced.
        let sub = by_name(c, "Subtitle");
        let LayerKind::Text {
            font_family, align, ..
        } = &sub.kind
        else {
            panic!()
        };
        assert_eq!((font_family.as_str(), *align), ("Inter", TextAlign::Center));
        assert_eq!(num(sub, Prop::FontSize), 24.0);
        assert_eq!(
            sub.props[&Prop::Fill].as_color().unwrap().to_hex(),
            "#ffcc00"
        );
        let font = &rep(c, "Subtitle").notes[0];
        assert_eq!(
            (font.label.as_str(), font.status),
            ("Font {from} → {to}", Status::Partial)
        );
        assert_eq!(
            (font.params["from"].as_str(), font.params["to"].as_str()),
            ("Helvetica", "Inter")
        );

        // RLE pixels, the mask hiding the right half.
        let logo = by_name(c, "Logo");
        assert_eq!(pos(logo), [150.0, 10.0, 20.0, 30.0]);
        assert!(logo.lock_ratio);
        let img = picture(&out, src(logo));
        assert_eq!(img.dimensions(), (20, 30));
        assert_eq!(img.get_pixel(3, 2).0, [18, 16, 200, 255]);
        assert_eq!(rep(c, "Logo").notes[0].label, "Mask applied");
        // Clipped to the logo.
        let clipped = by_name(c, "Clipped");
        assert_eq!(pos(clipped), [150.0, 10.0, 20.0, 30.0]);
        assert_eq!(
            picture(&out, src(clipped)).get_pixel(0, 0).0,
            [255, 0, 0, 255]
        );
        assert_eq!(rep(c, "Clipped").notes[0].label, "Clipping mask applied");

        assert!(rep(c, "Hidden dot").hidden);
        assert!(!rep(c, "Logo").hidden);

        let dot = by_name(c, "Dot");
        assert!(matches!(dot.kind, LayerKind::Ellipse { .. }));
        assert_eq!(pos(dot), [85.0, 15.0, 30.0, 30.0]);

        let tint = by_name(c, "Tint");
        assert!(matches!(tint.kind, LayerKind::Rect { .. }));
        assert_eq!(pos(tint), [0.0, 0.0, 200.0, 120.0]);
        assert_eq!(num(tint, Prop::Opacity), 0.502);
        let n = &rep(c, "Tint").notes[0];
        assert_eq!(
            (n.label.as_str(), n.params["mode"].as_str()),
            ("Blend mode {mode}", "Multiply")
        );

        let flat = picture(&out, c.flat.as_deref().unwrap());
        assert_eq!(flat.dimensions(), (200, 120));
        assert_eq!(flat.get_pixel(10, 10).0, [20, 30, 80, 255]);
        // Every picture is staged under the prefix, once.
        let files: Vec<_> = out.files.iter().map(|f| f.0.as_str()).collect();
        assert_eq!(files.len(), 5);
        assert!(files
            .iter()
            .all(|f| f.starts_with("psd-") && f.ends_with(".png")));
    }

    #[test]
    fn artboards_become_templates() {
        let out = analyze(&fixture("artboards.psd"), &ctx()).unwrap();
        assert_eq!((out.width, out.height), (200, 200));
        let [a, b] = out.comps.as_slice() else {
            panic!()
        };
        assert_eq!((a.name.as_str(), b.name.as_str()), ("Board A", "Board B"));
        assert_eq!(pos(by_name(a, "Red square")), [50.0, 50.0, 100.0, 100.0]);
        let [loose] = a.ignored.as_slice() else {
            panic!()
        };
        assert_eq!(
            (loose.name.as_str(), loose.reason.as_str()),
            ("Loose", "Outside the artboards")
        );
        assert_eq!(
            picture(&out, a.flat.as_deref().unwrap()).dimensions(),
            (200, 200)
        );

        // Placed in its artboard; the flat cropped to it.
        assert_eq!(
            picture(&out, b.flat.as_deref().unwrap()).dimensions(),
            (180, 120)
        );
        let sky = by_name(b, "Sky");
        assert_eq!(pos(sky), [0.0, 0.0, 180.0, 120.0]);
        let LayerKind::Rect { x } = &sky.kind else {
            panic!()
        };
        assert_eq!(
            (
                x.fill_style.kind,
                x.fill_style.angle,
                x.fill_style.color2.as_str()
            ),
            (FillKind::Linear, 0.0, "#ffffff80")
        );
        assert_eq!(
            (
                x.fill_style.stops[0].at,
                x.fill_style.stops[0].color.as_str()
            ),
            (50.0, "#00ff00bf")
        );
        assert_eq!(
            sky.props[&Prop::Fill].as_color().unwrap().to_hex(),
            "#0000ff"
        );
        assert_eq!(rep(b, "Sky").source, "fill");

        let caption = by_name(b, "Caption");
        assert_eq!(num(caption, Prop::X), 20.0);
        let LayerKind::Text { x, .. } = &caption.kind else {
            panic!()
        };
        assert!(x.shadow && !caption.drop_shadow);
        assert_eq!(
            (x.shadow_x, x.shadow_y, x.shadow_color.as_str()),
            (3.0, 0.0, "#000000cc")
        );

        let badge = by_name(b, "Badge");
        assert!(matches!(badge.kind, LayerKind::Rect { .. }));
        assert_eq!(pos(badge), [10.0, 80.0, 60.0, 30.0]);
        assert_eq!(num(badge, Prop::CornerRadius), 8.0);

        let tri = by_name(b, "Triangle");
        let img = picture(&out, src(tri));
        assert_eq!(
            img.get_pixel(img.width() / 2, img.height() - 3).0,
            [0, 128, 0, 255]
        );
        let r = rep(b, "Triangle");
        assert_eq!(
            (r.source.as_str(), r.notes[0].label.as_str()),
            ("shape", "Vector shape rasterised")
        );

        let photo = by_name(b, "Photo");
        assert_eq!(pos(photo), [140.0, 10.0, 30.0, 20.0]);
        let r = rep(b, "Photo");
        assert_eq!(
            (r.source.as_str(), r.status, r.notes[0].label.as_str()),
            ("smartObject", Status::Partial, "Smart object → picture")
        );
        assert_eq!(b.template.fields[0].key, "caption");
    }

    #[test]
    fn reads_large_16_bit_documents() {
        let out = analyze(&fixture("gray16.psb"), &ctx()).unwrap();
        assert_eq!(out.warnings[0].label, "16-bit colours reduced to 8 bits");
        let c = &out.comps[0];
        let ramp = by_name(c, "Ramp");
        assert_eq!(pos(ramp), [8.0, 4.0, 16.0, 8.0]);
        // ZIP with prediction: 4096 × x + 1000 × y, in 8 bits.
        let img = picture(&out, src(ramp));
        assert_eq!(img.get_pixel(1, 0).0, [16, 16, 16, 255]);
        assert_eq!(img.get_pixel(2, 1).0, [36, 36, 36, 255]);
        assert_eq!(
            picture(&out, c.flat.as_deref().unwrap()).get_pixel(0, 0).0,
            [128, 128, 128, 255]
        );
    }

    #[test]
    fn flat_cmyk_document_is_one_picture() {
        let out = analyze(&fixture("cmyk.psd"), &ctx()).unwrap();
        assert_eq!(
            out.warnings[0].label,
            "CMYK colours converted approximately"
        );
        let c = &out.comps[0];
        let [bg] = c.template.layers.as_slice() else {
            panic!()
        };
        let img = picture(&out, src(bg));
        assert_eq!(img.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(7, 3).0, [0, 0, 255, 255]);
        assert_eq!(c.layers[0].source, "pixel");
    }

    #[test]
    fn bad_files_fail_cleanly() {
        assert!(analyze(b"hello", &ctx()).is_err());
        let mut big = fixture("cmyk.psd");
        big[18..22].copy_from_slice(&20000u32.to_be_bytes());
        assert!(analyze(&big, &ctx()).unwrap_err().contains("16384"));
        // Truncated or damaged anywhere: an error or a partial result, never a panic.
        for f in ["layers.psd", "artboards.psd", "gray16.psb", "cmyk.psd"] {
            let b = fixture(f);
            let step = b.len() / 120 + 1;
            for cut in (0..b.len()).step_by(step) {
                let _ = analyze(&b[..cut], &ctx());
                let mut d = b.clone();
                for i in (cut..d.len()).step_by(997).take(4) {
                    d[i] ^= 0xa5;
                }
                let _ = analyze(&d, &ctx());
            }
        }
    }
}
