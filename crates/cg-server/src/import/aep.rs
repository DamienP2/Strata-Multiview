//! Import of After Effects projects (`.aep`, or the `.zip` of « Collect
//! Files »): see [`super`].
//!
//! The `.aep` format is undocumented by Adobe: a big-endian RIFX tree of
//! chunks. The layout used here follows the public reverse-engineering
//! work of the py-aep project (MIT, its `docs/file_format.md`), checked
//! against projects saved by After Effects CC 2018 (file version 92) to
//! AE 2026 (97). In short:
//!
//! - `LIST:Fold` holds the items (`LIST:Item`: `idta` type 1 folder,
//!   4 composition, 7 footage), each followed by its name (`Utf8`);
//! - a composition has its settings in `cdta` and one `LIST:Layr` per
//!   layer (`ldta` record, name, `LIST:tdgp` property tree); its markers
//!   are on the hidden `LIST:SecL` layer, its Essential Graphics in
//!   `LIST:CIF3` (or `CIF2` / `CIFO`);
//! - a property group is a `LIST:tdgp` of (`tdmn` match name, body)
//!   pairs; a property is a `LIST:tdbs` (`tdb4` metadata, then a `cdat`
//!   static value or a `LIST:list` of keyframes, `lhd3` + `ldat`);
//! - text documents are COS (PDF object syntax) in `LIST:btdk`, paths in
//!   `LIST:om-s`, markers in `LIST:mrst`, effects in `LIST:sspc`;
//! - footage sources are `LIST:Pin` (`sspc` size, `opti` solid colour,
//!   `LIST:Als2`/`alas` JSON path).
//!
//! Converted: compositions, solids, shape layers (rectangles, ellipses,
//! paths, stars; fill and stroke; trim paths on a single path), texts
//! (content, font, size, colour, justification, tracking, leading),
//! footage pictures, nulls and parenting, precompositions, transforms and
//! their keyframes with After Effects easing, in / out points, markers,
//! the layer-style drop shadow, Essential Graphics controllers. Reported
//! but left out: effects, masks, track mattes, expressions, text
//! animators, blend modes, 3D, cameras, lights, audio, video.

use std::collections::{BTreeMap, HashMap, HashSet};

use cg_core::{
    new_id, Color, Easing, Field, FieldKind, ImageFit, Keyframe, Layer, LayerKind, Marker,
    MarkerKind, Prop, Resume, Template, TextAlign, Value, VerticalAlign,
};

use super::{Comp, Converted, Ctx, Format, Ignored, LayerReport, Note, Status, VariableReport};

/// Converts an `.aep`, or the `.zip` of « Collect Files » (the project and
/// its footage).
pub fn analyze(bytes: &[u8], ctx: &Ctx) -> Result<Converted, String> {
    if bytes.starts_with(b"RIFX") {
        return Ok(with_controls(convert(bytes, ctx, None, None)?));
    }
    if !bytes.starts_with(b"PK") {
        return Err("not an After Effects project (.aep) or a zip holding one".into());
    }
    let zip = Archive::open(bytes)?;
    let aep = zip
        .names()
        .iter()
        .filter(|n| n.to_lowercase().ends_with(".aep") && !is_junk(n))
        // The project at the top of the archive, not a backup in a subfolder.
        .min_by_key(|n| (n.matches('/').count(), n.len()))
        .cloned()
        .ok_or("no After Effects project (.aep) in this archive")?;
    let project = zip
        .read(&aep)
        .ok_or_else(|| format!("cannot read {aep} in the archive"))?;
    let lookup = |path: &str| zip.find(path);
    Ok(with_controls(convert(&project, ctx, Some(&lookup), None)?))
}

/// The compositions with their own Essential Graphics controllers as
/// variables.
fn with_controls(mut p: Project) -> Converted {
    for (comp, info) in p.converted.comps.iter_mut().zip(&p.comps) {
        apply_controls(comp, info, &info.controllers);
    }
    p.converted
}

/// Finds a footage file (by its path in the project) where the project
/// came from: the archive of « Collect Files » or of a `.mogrt`.
pub(super) type Media<'a> = &'a dyn Fn(&str) -> Option<Vec<u8>>;

/// The result of [`convert`]: what the import modal shows, plus what a
/// `.mogrt` needs to wire its controls.
pub(super) struct Project {
    pub converted: Converted,
    /// Per composition of `converted.comps` (same order).
    pub comps: Vec<CompInfo>,
}

/// What a composition's Essential Graphics refer to.
pub(super) struct CompInfo {
    /// Item id of the composition in the project.
    #[allow(dead_code)]
    pub item: u32,
    pub name: String,
    /// The Essential Graphics controllers of the composition.
    pub controllers: Vec<Controller>,
    /// After Effects layer id → the template layer(s) it became.
    pub layers: HashMap<u32, Vec<String>>,
    /// After Effects layer id of a text layer → its field key.
    pub text_keys: HashMap<u32, String>,
}

/// An Essential Graphics controller, as stored in the project.
#[derive(Debug, Clone)]
pub(super) struct Controller {
    /// Its id (a UUID), the `id` of the matching control of a `.mogrt`.
    pub id: String,
    pub name: String,
    /// 1 checkbox, 2 slider, 3 angle, 4 colour, 5 point, 6 text,
    /// 10 group, 13 dropdown, 14 media replacement.
    pub kind: u32,
    /// The current value, as stored (by kind: 0/1, number, RGBA 0..1…).
    pub value: Vec<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// Dropdown items.
    pub items: Vec<String>,
    /// A text controller's text.
    pub text: String,
    /// The layer whose property it exposes (`ldta` id), 0 = none.
    pub layer: u32,
    /// Match names from the layer to the property.
    pub path: Vec<String>,
}

/// Reads a project and converts every composition. `media` finds footage
/// files (none for a bare `.aep`).
pub(super) fn convert(
    bytes: &[u8],
    ctx: &Ctx,
    media: Option<Media>,
    main: Option<&str>,
) -> Result<Project, String> {
    let tree = riff::parse(bytes)?;
    let mut warnings = Vec::new();
    if tree.truncated {
        warnings.push(Note::new(
            "The file is truncated: part of it was read",
            Status::Partial,
        ));
    }
    let project = model::read(&tree.root)?;
    if project.comps.is_empty() {
        return Err("this After Effects project has no composition".into());
    }
    let mut out = conv::Converter::new(&project, ctx, media, main);
    let (comps, infos) = out.run();
    warnings.extend(out.warnings());
    let main = out.main();
    Ok(Project {
        converted: Converted {
            format: Format::Aep,
            name: String::new(),
            width: main.0,
            height: main.1,
            fps: main.2,
            comps,
            warnings,
            files: out.files,
        },
        comps: infos,
    })
}

/// Turns Essential Graphics controls into template fields wired to what
/// they drive (the property at the end of [`Controller::path`] on the
/// layer [`Controller::layer`]), and reports them as variables. Texts reuse
/// the field of their text layer; colours, opacity, rotation, stroke width
/// and corner radius bind the property of the template layer; a media
/// replacement becomes the picture's `{{key}}`; the others become fields
/// that drive nothing (« Not wired »).
pub(super) fn apply_controls(comp: &mut Comp, info: &CompInfo, controls: &[Controller]) {
    let not_wired = || Some(Note::new("Not wired", Status::Partial));
    for c in controls {
        let leaf = c.path.last().map(String::as_str).unwrap_or("");
        let targets = info.layers.get(&c.layer).cloned().unwrap_or_default();
        let t = &mut comp.template;
        let name = if c.name.trim().is_empty() {
            "Control".to_string()
        } else {
            c.name.clone()
        };
        let field = |kind: FieldKind, default: String, fields: &mut Vec<Field>| {
            let key = conv::unique_key(fields, &name);
            fields.push(Field {
                key: key.clone(),
                label: name.clone(),
                kind,
                default,
                binding: None,
                min: None,
                max: None,
                options: Vec::new(),
            });
            key
        };
        let report = match c.kind {
            // Groups and comments organise the panel.
            8 | 10 => continue,
            6 => {
                let existing = info
                    .text_keys
                    .get(&c.layer)
                    .and_then(|k| t.fields.iter_mut().find(|f| f.key == *k));
                match existing {
                    Some(f) => {
                        f.label = name.clone();
                        if !c.text.is_empty() {
                            f.default = c.text.replace(['\r', '\u{3}'], "\n");
                            if f.default.contains('\n') {
                                f.kind = FieldKind::Multiline;
                            }
                        }
                        VariableReport {
                            name: name.clone(),
                            key: f.key.clone(),
                            kind: f.kind,
                            status: Status::Ok,
                            note: None,
                        }
                    }
                    None => VariableReport {
                        key: field(FieldKind::Text, c.text.clone(), &mut t.fields),
                        name: name.clone(),
                        kind: FieldKind::Text,
                        status: Status::Partial,
                        note: not_wired(),
                    },
                }
            }
            2 | 3 => {
                let (prop, scale) = match leaf {
                    "ADBE Opacity" => (Some(Prop::Opacity), 0.01),
                    "ADBE Rotate Z" => (Some(Prop::Rotation), 1.0),
                    "ADBE Vector Stroke Width" => (Some(Prop::StrokeWidth), 1.0),
                    "ADBE Vector Rect Roundness" => (Some(Prop::CornerRadius), 1.0),
                    _ => (None, 1.0),
                };
                let v = c.value.first().copied().unwrap_or(0.0) * scale;
                let key = field(
                    FieldKind::Number,
                    conv::round4(v).to_string(),
                    &mut t.fields,
                );
                if let Some(f) = t.fields.last_mut() {
                    f.min = c.min.map(|m| m * scale);
                    f.max = c.max.map(|m| m * scale);
                }
                let note = match prop.and_then(|p| bind(t, &targets, p, &key)) {
                    Some(n) => n,
                    None => not_wired(),
                };
                VariableReport {
                    name: name.clone(),
                    key,
                    kind: FieldKind::Number,
                    status: if note.is_some() {
                        Status::Partial
                    } else {
                        Status::Ok
                    },
                    note,
                }
            }
            4 => {
                let v = |i: usize, d: f64| c.value.get(i).copied().unwrap_or(d) as f32;
                let hex = Color::rgba(v(0, 1.0), v(1, 1.0), v(2, 1.0), v(3, 1.0)).to_hex();
                let key = field(FieldKind::Color, hex, &mut t.fields);
                let (prop, effect) = match leaf {
                    "ADBE Vector Fill Color" => (Some(Prop::Fill), false),
                    "ADBE Vector Stroke Color" => (Some(Prop::Stroke), false),
                    // The Fill effect colours the whole layer.
                    "ADBE Fill-0002" => (Some(Prop::Fill), true),
                    _ => (None, false),
                };
                let mut note = match prop.and_then(|p| bind(t, &targets, p, &key)) {
                    Some(n) => n,
                    None => not_wired(),
                };
                if effect && note.is_none() {
                    note = Some(Note::new("Bound to the layer colour", Status::Partial));
                }
                VariableReport {
                    name: name.clone(),
                    key,
                    kind: FieldKind::Color,
                    status: if note.is_some() {
                        Status::Partial
                    } else {
                        Status::Ok
                    },
                    note,
                }
            }
            1 => {
                let on = c.value.first().copied().unwrap_or(0.0) != 0.0;
                VariableReport {
                    key: field(FieldKind::Boolean, on.to_string(), &mut t.fields),
                    name: name.clone(),
                    kind: FieldKind::Boolean,
                    status: Status::Partial,
                    note: not_wired(),
                }
            }
            13 => {
                let i = c.value.first().copied().unwrap_or(1.0).max(1.0) as usize - 1;
                let default = c
                    .items
                    .get(i)
                    .or(c.items.first())
                    .cloned()
                    .unwrap_or_default();
                let key = field(FieldKind::Text, default, &mut t.fields);
                if let Some(f) = t.fields.last_mut() {
                    f.options = c.items.iter().map(|s| (s.clone(), s.clone())).collect();
                }
                VariableReport {
                    name: name.clone(),
                    key,
                    kind: FieldKind::Text,
                    status: Status::Partial,
                    note: not_wired(),
                }
            }
            14 => {
                let image = t
                    .layers
                    .iter_mut()
                    .find(|l| targets.contains(&l.id) && matches!(l.kind, LayerKind::Image { .. }));
                match image {
                    Some(l) => {
                        let LayerKind::Image { src, .. } = &mut l.kind else {
                            continue;
                        };
                        let default = std::mem::take(src);
                        let key = conv::unique_key(&t.fields, &name);
                        *src = format!("{{{{{key}}}}}");
                        t.fields.push(Field {
                            key: key.clone(),
                            label: name.clone(),
                            kind: FieldKind::Image,
                            default,
                            binding: None,
                            min: None,
                            max: None,
                            options: Vec::new(),
                        });
                        VariableReport {
                            name: name.clone(),
                            key,
                            kind: FieldKind::Image,
                            status: Status::Ok,
                            note: None,
                        }
                    }
                    None => VariableReport {
                        key: field(FieldKind::Image, String::new(), &mut t.fields),
                        name: name.clone(),
                        kind: FieldKind::Image,
                        status: Status::Partial,
                        note: not_wired(),
                    },
                }
            }
            _ => VariableReport {
                name: name.clone(),
                key: String::new(),
                kind: FieldKind::Text,
                status: Status::Ignored,
                note: Some(Note::new("Unsupported control", Status::Ignored)),
            },
        };
        comp.variables.push(report);
    }
}

/// Binds `prop` of the template layers of `targets` that have it (the
/// first one when none has) to a field: `None` when there is no layer,
/// else the note about the binding (`None` when exact).
fn bind(t: &mut Template, targets: &[String], prop: Prop, key: &str) -> Option<Option<Note>> {
    let has = |l: &Layer| l.props.contains_key(&prop) || l.tracks.contains_key(&prop);
    let ids: Vec<String> = {
        let with: Vec<String> = t
            .layers
            .iter()
            .filter(|l| targets.contains(&l.id) && has(l))
            .map(|l| l.id.clone())
            .collect();
        if with.is_empty() {
            targets.first().cloned().into_iter().collect()
        } else {
            with
        }
    };
    if ids.is_empty() {
        return None;
    }
    let mut animated = false;
    for l in t.layers.iter_mut().filter(|l| ids.contains(&l.id)) {
        animated |= l.tracks.contains_key(&prop);
        l.bindings.insert(prop, key.to_string());
    }
    Some(if animated {
        Some(Note::new("Animated property", Status::Partial))
    } else if ids.len() > 1 {
        Some(Note::new("Bound to every part", Status::Partial))
    } else {
        None
    })
}

/// macOS resource forks and the like.
fn is_junk(name: &str) -> bool {
    name.starts_with("__MACOSX/") || name.rsplit('/').next().unwrap_or("").starts_with("._")
}

// ------------------------------------------------------------------ zip ----

/// A zip read lazily, with size caps (the files are untrusted).
pub(super) struct Archive<'a> {
    zip: std::cell::RefCell<zip::ZipArchive<std::io::Cursor<&'a [u8]>>>,
    names: Vec<String>,
}

/// Largest file taken out of an archive.
const MAX_ENTRY: u64 = 512 << 20;

impl<'a> Archive<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Self, String> {
        let zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|e| format!("unreadable zip archive: {e}"))?;
        let names = zip
            .file_names()
            .filter_map(|n| n.ok().map(|n| n.into_owned()))
            .collect();
        Ok(Self {
            zip: std::cell::RefCell::new(zip),
            names,
        })
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// The bytes of an entry (exact name).
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        use std::io::Read;
        let mut zip = self.zip.borrow_mut();
        let f = zip.by_name(name).ok()?;
        if f.size() > MAX_ENTRY {
            return None;
        }
        let mut out = Vec::with_capacity(f.size() as usize);
        f.take(MAX_ENTRY).read_to_end(&mut out).ok()?;
        Some(out)
    }

    /// A file by the path the project stores (absolute, from the machine
    /// it was saved on): the entry with the same file name, preferring the
    /// one whose folders match most of the path's.
    pub fn find(&self, path: &str) -> Option<Vec<u8>> {
        let parts: Vec<String> = path
            .split(['/', '\\'])
            .filter(|p| !p.is_empty())
            .map(str::to_lowercase)
            .collect();
        let file = parts.last()?.clone();
        let best = self
            .names
            .iter()
            .filter(|n| !n.ends_with('/') && !is_junk(n))
            .filter(|n| {
                n.rsplit('/')
                    .next()
                    .is_some_and(|f| f.to_lowercase() == file)
            })
            .max_by_key(|n| {
                let entry: Vec<String> = n.split('/').rev().map(str::to_lowercase).collect();
                entry
                    .iter()
                    .zip(parts.iter().rev())
                    .take_while(|(a, b)| a == b)
                    .count()
            })?
            .clone();
        self.read(&best)
    }
}

// ----------------------------------------------------------------- riff ----

/// The RIFX chunk tree.
mod riff {
    /// One chunk; a `LIST` has its list type in `kind` and its children
    /// in `kids` (the `btdk` text list keeps its raw body).
    #[derive(Debug)]
    pub struct Chunk<'a> {
        pub tag: [u8; 4],
        pub kind: [u8; 4],
        pub body: &'a [u8],
        pub kids: Vec<Chunk<'a>>,
    }

    pub struct Tree<'a> {
        pub root: Chunk<'a>,
        /// A chunk ran past the end of the file (the rest was dropped).
        pub truncated: bool,
    }

    /// Deepest nesting read (After Effects files go about 20 deep).
    const MAX_DEPTH: usize = 96;
    /// Most chunks read (production projects hold a few hundred thousand).
    const MAX_CHUNKS: usize = 4_000_000;

    pub fn parse(bytes: &[u8]) -> Result<Tree<'_>, String> {
        if bytes.len() < 12 || &bytes[..4] != b"RIFX" {
            return Err("not an After Effects project (.aep): no RIFX header".into());
        }
        if &bytes[8..12] != b"Egg!" {
            return Err(format!(
                "unsupported After Effects file: form type {:?} (expected Egg!; .aepx XML projects are not supported)",
                String::from_utf8_lossy(&bytes[8..12])
            ));
        }
        let len = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        let end = (8usize.saturating_add(len)).min(bytes.len());
        let mut st = State {
            count: 0,
            truncated: 8usize.saturating_add(len) > bytes.len(),
        };
        let kids = st.list(&bytes[12..end], 0)?;
        Ok(Tree {
            root: Chunk {
                tag: *b"RIFX",
                kind: *b"Egg!",
                body: &bytes[12..end],
                kids,
            },
            truncated: st.truncated,
        })
    }

    struct State {
        count: usize,
        truncated: bool,
    }

    impl State {
        fn list<'a>(&mut self, data: &'a [u8], depth: usize) -> Result<Vec<Chunk<'a>>, String> {
            if depth > MAX_DEPTH {
                return Err("the project's chunk tree is nested too deeply".into());
            }
            let mut out = Vec::new();
            let mut off = 0usize;
            while off + 8 <= data.len() {
                self.count += 1;
                if self.count > MAX_CHUNKS {
                    return Err("the project has too many chunks".into());
                }
                let tag = [data[off], data[off + 1], data[off + 2], data[off + 3]];
                let len = u32::from_be_bytes([
                    data[off + 4],
                    data[off + 5],
                    data[off + 6],
                    data[off + 7],
                ]) as usize;
                let start = off + 8;
                let end = match start.checked_add(len) {
                    Some(e) if e <= data.len() => e,
                    _ => {
                        self.truncated = true;
                        data.len()
                    }
                };
                let body = &data[start..end];
                let chunk = if &tag == b"LIST" && body.len() >= 4 {
                    let kind = [body[0], body[1], body[2], body[3]];
                    let inner = &body[4..];
                    let kids = if &kind == b"btdk" {
                        Vec::new()
                    } else {
                        self.list(inner, depth + 1)?
                    };
                    Chunk {
                        tag,
                        kind,
                        body: inner,
                        kids,
                    }
                } else {
                    Chunk {
                        tag,
                        kind: [0; 4],
                        body,
                        kids: Vec::new(),
                    }
                };
                out.push(chunk);
                off = end + (len & 1);
            }
            Ok(out)
        }
    }

    impl<'a> Chunk<'a> {
        pub fn is(&self, tag: &[u8; 4]) -> bool {
            &self.tag == tag
        }
        pub fn is_list(&self, kind: &[u8; 4]) -> bool {
            &self.tag == b"LIST" && &self.kind == kind
        }
        /// First child with this tag.
        pub fn child(&self, tag: &[u8; 4]) -> Option<&Chunk<'a>> {
            self.kids.iter().find(|c| c.is(tag))
        }
        /// First child list of this type.
        pub fn list(&self, kind: &[u8; 4]) -> Option<&Chunk<'a>> {
            self.kids.iter().find(|c| c.is_list(kind))
        }
        /// The body as UTF-8 text (lossy).
        pub fn text(&self) -> String {
            String::from_utf8_lossy(self.body)
                .trim_end_matches('\0')
                .to_string()
        }
        /// The string of a wrapper chunk (`tdsn`, `fnam`…): its `Utf8`.
        pub fn wrapped(&self) -> Option<String> {
            let mut st = State {
                count: 0,
                truncated: false,
            };
            let kids = st.list(self.body, MAX_DEPTH).ok()?;
            kids.iter().find(|c| c.is(b"Utf8")).map(|c| c.text())
        }
    }

    /// Bounds-checked big-endian reads.
    pub fn u8_at(b: &[u8], o: usize) -> u8 {
        b.get(o).copied().unwrap_or(0)
    }
    pub fn u16_at(b: &[u8], o: usize) -> u16 {
        match b.get(o..o + 2) {
            Some(s) => u16::from_be_bytes([s[0], s[1]]),
            None => 0,
        }
    }
    pub fn u32_at(b: &[u8], o: usize) -> u32 {
        match b.get(o..o + 4) {
            Some(s) => u32::from_be_bytes([s[0], s[1], s[2], s[3]]),
            None => 0,
        }
    }
    pub fn i32_at(b: &[u8], o: usize) -> i32 {
        u32_at(b, o) as i32
    }
    pub fn f32_at(b: &[u8], o: usize) -> f64 {
        let v = f32::from_bits(u32_at(b, o)) as f64;
        if v.is_finite() {
            v
        } else {
            0.0
        }
    }
    pub fn f64_at(b: &[u8], o: usize) -> f64 {
        match b.get(o..o + 8) {
            Some(s) => {
                let mut a = [0u8; 8];
                a.copy_from_slice(s);
                let v = f64::from_be_bytes(a);
                if v.is_finite() {
                    v
                } else {
                    0.0
                }
            }
            None => 0.0,
        }
    }
    /// A ratio `dividend / divisor` (0 when the divisor is 0).
    pub fn ratio(n: f64, d: f64) -> f64 {
        if d == 0.0 {
            0.0
        } else {
            n / d
        }
    }
    /// NUL-padded text of a fixed-size field.
    pub fn cstr(b: &[u8], o: usize, len: usize) -> String {
        let s = b.get(o..(o + len).min(b.len())).unwrap_or(&[]);
        let end = s.iter().position(|c| *c == 0).unwrap_or(s.len());
        String::from_utf8_lossy(&s[..end]).to_string()
    }
}

// ------------------------------------------------------------------ cos ----

/// The PDF-like object syntax of text documents (`LIST:btdk`).
mod cos {
    #[derive(Debug, Clone, PartialEq)]
    pub enum Obj {
        Dict(Vec<(String, Obj)>),
        Array(Vec<Obj>),
        Name(String),
        Num(f64),
        Bool(bool),
        Str(Vec<u8>),
    }

    impl Obj {
        pub fn get(&self, key: &str) -> Option<&Obj> {
            match self {
                Obj::Dict(d) => d.iter().find(|(k, _)| k == key).map(|(_, v)| v),
                Obj::Array(a) => key.parse::<usize>().ok().and_then(|i| a.get(i)),
                _ => None,
            }
        }
        /// Follows a path of keys / indexes: `"1/1/0"`.
        pub fn at(&self, path: &str) -> Option<&Obj> {
            path.split('/')
                .filter(|p| !p.is_empty())
                .try_fold(self, |o, k| o.get(k))
        }
        pub fn num(&self) -> Option<f64> {
            match self {
                Obj::Num(n) => Some(*n),
                _ => None,
            }
        }
        pub fn boolean(&self) -> Option<bool> {
            match self {
                Obj::Bool(b) => Some(*b),
                _ => None,
            }
        }
        pub fn array(&self) -> &[Obj] {
            match self {
                Obj::Array(a) => a,
                _ => &[],
            }
        }
        /// A string: UTF-16BE with a byte-order mark, else bytes as Latin-1.
        pub fn string(&self) -> Option<String> {
            let Obj::Str(b) = self else { return None };
            if b.starts_with(&[0xFE, 0xFF]) {
                let units: Vec<u16> = b[2..]
                    .chunks(2)
                    .map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]))
                    .collect();
                Some(String::from_utf16_lossy(&units))
            } else {
                Some(b.iter().map(|&c| c as char).collect())
            }
        }
    }

    /// Parses a body: a top-level dictionary without its delimiters.
    pub fn parse(body: &[u8]) -> Option<Obj> {
        let mut p = Parser {
            b: body,
            i: 0,
            depth: 0,
        };
        let mut out = Vec::new();
        loop {
            p.ws();
            if p.i >= p.b.len() {
                break;
            }
            let key = match p.value()? {
                Obj::Name(n) => n,
                _ => return None,
            };
            let v = p.value()?;
            out.push((key, v));
        }
        Some(Obj::Dict(out))
    }

    struct Parser<'a> {
        b: &'a [u8],
        i: usize,
        depth: usize,
    }

    impl Parser<'_> {
        fn ws(&mut self) {
            while self.i < self.b.len() && self.b[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
        }
        fn value(&mut self) -> Option<Obj> {
            self.ws();
            let c = *self.b.get(self.i)?;
            match c {
                b'<' if self.b.get(self.i + 1) == Some(&b'<') => {
                    self.i += 2;
                    self.nest()?;
                    let mut d = Vec::new();
                    loop {
                        self.ws();
                        if self.b.get(self.i..self.i + 2) == Some(b">>") {
                            self.i += 2;
                            break;
                        }
                        let key = match self.value()? {
                            Obj::Name(n) => n,
                            _ => return None,
                        };
                        let v = self.value()?;
                        d.push((key, v));
                    }
                    self.depth -= 1;
                    Some(Obj::Dict(d))
                }
                b'[' => {
                    self.i += 1;
                    self.nest()?;
                    let mut a = Vec::new();
                    loop {
                        self.ws();
                        if self.b.get(self.i) == Some(&b']') {
                            self.i += 1;
                            break;
                        }
                        a.push(self.value()?);
                    }
                    self.depth -= 1;
                    Some(Obj::Array(a))
                }
                b'/' => {
                    self.i += 1;
                    let s = self.i;
                    while self.i < self.b.len() && !is_delim(self.b[self.i]) {
                        self.i += 1;
                    }
                    Some(Obj::Name(
                        String::from_utf8_lossy(&self.b[s..self.i]).to_string(),
                    ))
                }
                b'(' => {
                    self.i += 1;
                    let mut out = Vec::new();
                    let mut level = 0;
                    loop {
                        let c = *self.b.get(self.i)?;
                        self.i += 1;
                        match c {
                            b'\\' => {
                                out.push(*self.b.get(self.i)?);
                                self.i += 1;
                            }
                            b'(' => {
                                level += 1;
                                out.push(c);
                            }
                            b')' if level == 0 => break,
                            b')' => {
                                level -= 1;
                                out.push(c);
                            }
                            _ => out.push(c),
                        }
                    }
                    Some(Obj::Str(out))
                }
                _ => {
                    let s = self.i;
                    while self.i < self.b.len() && !is_delim(self.b[self.i]) {
                        self.i += 1;
                    }
                    if s == self.i {
                        return None;
                    }
                    let t = std::str::from_utf8(&self.b[s..self.i]).ok()?;
                    match t {
                        "true" => Some(Obj::Bool(true)),
                        "false" => Some(Obj::Bool(false)),
                        _ => t.parse::<f64>().ok().map(Obj::Num),
                    }
                }
            }
        }
        fn nest(&mut self) -> Option<()> {
            self.depth += 1;
            (self.depth < 200).then_some(())
        }
    }

    fn is_delim(c: u8) -> bool {
        c.is_ascii_whitespace() || matches!(c, b'/' | b'(' | b')' | b'<' | b'>' | b'[' | b']')
    }
}

// ---------------------------------------------------------------- props ----

/// Property trees: groups, streams and their keyframes.
// What the file holds is read whole, converted or not.
#[allow(dead_code)]
mod props {
    use super::cos;
    use super::riff::{f32_at, f64_at, i32_at, u16_at, u32_at, u8_at, Chunk};

    /// `tdsn` of a property that keeps its default name.
    const DEFAULT_NAME: &str = "-_0_/-";

    #[derive(Debug, Default)]
    pub struct Group {
        /// Its own name (a shape group's « Group 1 », a mask's name…);
        /// `None` when it shows its default name.
        pub name: Option<String>,
        pub enabled: bool,
        pub kids: Vec<(String, Node)>,
    }

    #[derive(Debug)]
    pub enum Node {
        Group(Group),
        Stream(Stream),
        Path(PathProp),
        Text(TextProp),
        Markers(Vec<MarkerVal>),
        Effect {
            /// Display name (the instance's own name when renamed).
            name: String,
            params: Group,
        },
        Mask {
            mode: u32,
            inverted: bool,
            group: Group,
        },
        /// Gradient colours (`LIST:GCst`): one XML per keyframe.
        Gradient(Vec<String>),
        /// Orientation, overrides… (not converted).
        Other,
    }

    /// One keyframe.
    #[derive(Debug, Clone, Default)]
    pub struct Key {
        /// Seconds of layer time, unstretched.
        pub t: f64,
        /// Interpolation on each side: 1 linear, 2 bezier, 3 hold.
        pub in_type: u8,
        pub out_type: u8,
        /// Temporal auto-bezier.
        pub auto: bool,
        /// Spatial auto-bezier (the stored tangents are not used).
        pub spatial_auto: bool,
        pub v: Vec<f64>,
        /// Temporal ease per dimension (one for single-ease streams):
        /// speed in stored units per second, influence 0..1.
        pub in_ease: Vec<(f64, f64)>,
        pub out_ease: Vec<(f64, f64)>,
        /// Spatial tangents (offsets from the value).
        pub in_tan: Vec<f64>,
        pub out_tan: Vec<f64>,
    }

    /// A property stream: its static value or its keyframes.
    #[derive(Debug, Clone, Default)]
    pub struct Stream {
        pub name: Option<String>,
        pub enabled: bool,
        /// Position: dimensions separated (the values are in `_0`, `_1`).
        pub separated: bool,
        pub dims: usize,
        /// One ease for the whole value (spatial streams, colours).
        pub single_ease: bool,
        pub spatial: bool,
        pub color: bool,
        pub value: Vec<f64>,
        pub keys: Vec<Key>,
        pub expression: Option<String>,
    }

    impl Stream {
        pub fn animated(&self) -> bool {
            self.keys.len() > 1
        }
        /// The static value, or the first keyframe's.
        pub fn first(&self) -> &[f64] {
            match self.keys.first() {
                Some(k) => &k.v,
                None => &self.value,
            }
        }
        pub fn at(&self, i: usize) -> Option<f64> {
            self.first().get(i).copied()
        }
    }

    /// A bezier path: vertices and absolute control points.
    #[derive(Debug, Clone, Default)]
    pub struct Shape {
        pub closed: bool,
        pub verts: Vec<[f64; 2]>,
        /// Control point leaving vertex i.
        pub out_pt: Vec<[f64; 2]>,
        /// Control point entering vertex i.
        pub in_pt: Vec<[f64; 2]>,
    }

    #[derive(Debug, Clone, Default)]
    pub struct PathProp {
        pub stream: Stream,
        /// One per keyframe (one when static).
        pub shapes: Vec<Shape>,
    }

    #[derive(Debug, Clone, Default)]
    pub struct TextDoc {
        pub text: String,
        /// PostScript name of the font.
        pub font: String,
        pub size: f64,
        /// RGBA 0..1.
        pub fill: Option<[f64; 4]>,
        pub stroke: Option<([f64; 4], f64)>,
        /// Thousandths of an em.
        pub tracking: f64,
        /// Leading in pixels (None = auto).
        pub leading: Option<f64>,
        pub faux_bold: bool,
        pub faux_italic: bool,
        pub all_caps: bool,
        /// 0 left, 1 right, 2 centre, 3+ justified.
        pub justify: i64,
        /// Paragraph text: its box (x, y, w, h) in layer pixels.
        pub frame: Option<[f64; 4]>,
    }

    #[derive(Debug, Clone, Default)]
    pub struct TextProp {
        pub stream: Stream,
        pub docs: Vec<TextDoc>,
    }

    #[derive(Debug, Clone, Default)]
    pub struct MarkerVal {
        /// Seconds of layer time.
        pub t: f64,
        pub duration: f64,
        pub comment: String,
    }

    impl Group {
        pub fn get(&self, match_name: &str) -> Option<&Node> {
            self.kids
                .iter()
                .find(|(m, _)| m == match_name)
                .map(|(_, n)| n)
        }
        pub fn group(&self, match_name: &str) -> Option<&Group> {
            match self.get(match_name)? {
                Node::Group(g) => Some(g),
                _ => None,
            }
        }
        pub fn stream(&self, match_name: &str) -> Option<&Stream> {
            match self.get(match_name)? {
                Node::Stream(s) => Some(s),
                _ => None,
            }
        }
    }

    /// Reads a `LIST:tdgp`.
    pub fn group(list: &Chunk, depth: usize) -> Group {
        let mut g = Group {
            enabled: true,
            ..Default::default()
        };
        if depth > 64 {
            return g;
        }
        let kids = &list.kids;
        let mut i = 0;
        while i < kids.len() {
            let c = &kids[i];
            if c.is(b"tdsb") {
                g.enabled = u32_at(c.body, 0) & 1 != 0;
            } else if c.is(b"tdsn") {
                g.name = c.wrapped().filter(|n| n != DEFAULT_NAME && !n.is_empty());
            } else if c.is(b"tdmn") {
                let mn = super::riff::cstr(c.body, 0, 40);
                if mn == "ADBE Group End" {
                    break;
                }
                let end = kids[i + 1..]
                    .iter()
                    .position(|k| k.is(b"tdmn"))
                    .map_or(kids.len(), |p| i + 1 + p);
                let body = &kids[i + 1..end];
                g.kids.push((mn, node(body, depth)));
                i = end;
                continue;
            }
            i += 1;
        }
        g
    }

    fn node(body: &[Chunk], depth: usize) -> Node {
        let Some(first) = body.first() else {
            return Node::Other;
        };
        if first.is_list(b"tdgp") {
            return Node::Group(group(first, depth + 1));
        }
        if first.is_list(b"tdbs") {
            return Node::Stream(stream(first));
        }
        if first.is_list(b"om-s") {
            return Node::Path(path(first));
        }
        if first.is_list(b"btds") {
            return Node::Text(text(first));
        }
        if first.is_list(b"GCst") {
            let xml = first
                .list(b"GCky")
                .map(|k| {
                    k.kids
                        .iter()
                        .filter(|c| c.is(b"Utf8"))
                        .map(|c| c.text())
                        .collect()
                })
                .unwrap_or_default();
            return Node::Gradient(xml);
        }
        if first.is_list(b"mrst") {
            return Node::Markers(markers(first));
        }
        if first.is_list(b"sspc") {
            let name = first
                .child(b"fnam")
                .and_then(|c| c.wrapped())
                .unwrap_or_default();
            let params = first
                .list(b"tdgp")
                .map(|g| group(g, depth + 1))
                .unwrap_or_default();
            let name = params.name.clone().unwrap_or(name);
            return Node::Effect { name, params };
        }
        if first.is(b"mkif") {
            let group = body
                .iter()
                .find(|c| c.is_list(b"tdgp"))
                .map(|g| group(g, depth + 1))
                .unwrap_or_default();
            return Node::Mask {
                mode: u32_at(first.body, 4),
                inverted: u8_at(first.body, 0) != 0,
                group,
            };
        }
        // Overrides (`LIST:OvG2` + group), engine data before an effect…
        if let Some(g) = body.iter().find(|c| c.is_list(b"tdgp")) {
            return Node::Group(group(g, depth + 1));
        }
        if let Some(s) = body.iter().find(|c| c.is_list(b"sspc")) {
            return node(std::slice::from_ref(s), depth);
        }
        Node::Other
    }

    /// Reads a `LIST:tdbs`.
    pub fn stream(list: &Chunk) -> Stream {
        let mut s = Stream {
            enabled: true,
            ..Default::default()
        };
        let mut time_base = 0.0;
        let mut valueless = false;
        let mut has_expr = false;
        let mut seen_value = false;
        for c in &list.kids {
            if c.is(b"tdsb") {
                let f = u32_at(c.body, 0);
                s.enabled = f & 1 != 0;
                s.separated = f & 0x800 != 0;
            } else if c.is(b"tdsn") {
                s.name = c.wrapped().filter(|n| n != DEFAULT_NAME && !n.is_empty());
            } else if c.is(b"tdb4") {
                let b = c.body;
                s.dims = (u16_at(b, 2) as usize).clamp(1, 4);
                let flags = u16_at(b, 4);
                s.single_ease = flags & 2 != 0;
                s.spatial = flags & 8 != 0;
                time_base = u32_at(b, 0x0C) as f64;
                valueless = u16_at(b, 0x38) & 1 != 0;
                s.color = u8_at(b, 0x3B) & 1 != 0 && s.dims == 4;
                has_expr = u32_at(b, 0x78) != 0;
            } else if c.is(b"cdat") {
                seen_value = true;
                if !valueless {
                    s.value = (0..s.dims).map(|i| f64_at(c.body, i * 8)).collect();
                }
            } else if c.is_list(b"list") {
                seen_value = true;
                s.keys = keys(c, &s, time_base, valueless);
            } else if c.is(b"Utf8") && seen_value && has_expr && s.expression.is_none() {
                s.expression = Some(c.text());
            }
        }
        if s.color {
            s.value = argb(&s.value);
            for k in &mut s.keys {
                k.v = argb(&k.v);
            }
        }
        s
    }

    /// Stored colours are A, R, G, B in 0..255: RGBA 0..1.
    fn argb(v: &[f64]) -> Vec<f64> {
        if v.len() < 4 {
            return v.to_vec();
        }
        vec![v[1] / 255.0, v[2] / 255.0, v[3] / 255.0, v[0] / 255.0]
    }

    /// Items of a keyframe list.
    fn keys(list: &Chunk, s: &Stream, time_base: f64, valueless: bool) -> Vec<Key> {
        let Some(h) = list.child(b"lhd3") else {
            return Vec::new();
        };
        let count = u32_at(h.body, 8) as usize;
        let size = u32_at(h.body, 0x10) as usize;
        let Some(data) = list.child(b"ldat").map(|c| c.body) else {
            return Vec::new();
        };
        if size < 8 {
            return Vec::new();
        }
        let n = count.min(data.len() / size).min(100_000);
        let tb = if time_base > 0.0 { time_base } else { 1.0 };
        let d = s.dims;
        (0..n)
            .map(|i| {
                let b = &data[i * size..(i + 1) * size];
                let mut k = Key {
                    t: i32_at(b, 0) as f64 / tb,
                    in_type: u8_at(b, 4),
                    out_type: u8_at(b, 5),
                    auto: u16_at(b, 6) & 0x10 != 0,
                    ..Default::default()
                };
                if valueless {
                    return k;
                }
                if s.single_ease {
                    k.spatial_auto = s.spatial && u32_at(b, 8) & 2 != 0;
                    k.in_ease = vec![(f64_at(b, 0x18), f64_at(b, 0x20))];
                    k.out_ease = vec![(f64_at(b, 0x28), f64_at(b, 0x30))];
                    k.v = (0..d).map(|j| f64_at(b, 0x38 + j * 8)).collect();
                    if s.spatial {
                        k.in_tan = (0..d).map(|j| f64_at(b, 0x38 + (d + j) * 8)).collect();
                        k.out_tan = (0..d).map(|j| f64_at(b, 0x38 + (2 * d + j) * 8)).collect();
                    }
                } else {
                    let at = |block: usize, j: usize| f64_at(b, 8 + (block * d + j) * 8);
                    k.v = (0..d).map(|j| at(0, j)).collect();
                    k.in_ease = (0..d).map(|j| (at(1, j), at(2, j))).collect();
                    k.out_ease = (0..d).map(|j| (at(3, j), at(4, j))).collect();
                }
                k
            })
            .collect()
    }

    /// Reads a `LIST:om-s` (mask or shape path).
    fn path(list: &Chunk) -> PathProp {
        let stream = list.list(b"tdbs").map(stream).unwrap_or_default();
        let shapes = list
            .list(b"omks")
            .map(|o| {
                o.kids
                    .iter()
                    .filter(|c| c.is_list(b"shap"))
                    .take(100_000)
                    .map(shape)
                    .collect()
            })
            .unwrap_or_default();
        PathProp { stream, shapes }
    }

    fn shape(shap: &Chunk) -> Shape {
        let mut out = Shape::default();
        let (mut l, mut t, mut r, mut b) = (0.0, 0.0, 1.0, 1.0);
        let mut normalized = true;
        if let Some(h) = shap.child(b"shph") {
            let flags = u8_at(h.body, 3);
            out.closed = flags & 8 == 0;
            normalized = flags & 1 != 0;
            l = f32_at(h.body, 4);
            t = f32_at(h.body, 8);
            r = f32_at(h.body, 12);
            b = f32_at(h.body, 16);
        }
        let Some(list) = shap.list(b"list") else {
            return out;
        };
        let data = list.child(b"ldat").map(|c| c.body).unwrap_or(&[]);
        let pts: Vec<[f64; 2]> = data
            .chunks(8)
            .filter(|p| p.len() == 8)
            .take(300_000)
            .map(|p| {
                let (x, y) = (f32_at(p, 0), f32_at(p, 4));
                let (x, y) = if x.is_finite() && y.is_finite() {
                    (x, y)
                } else {
                    (0.0, 0.0)
                };
                if normalized {
                    [l + x * (r - l), t + y * (b - t)]
                } else {
                    [x, y]
                }
            })
            .collect();
        let n = pts.len() / 3;
        for i in 0..n {
            out.verts.push(pts[3 * i]);
            out.out_pt.push(pts[3 * i + 1]);
            // The control point entering vertex i ends the previous triple.
            out.in_pt.push(pts[(3 * i + 3 * n - 1) % (3 * n)]);
        }
        out
    }

    /// Reads a `LIST:btds` (Source Text).
    fn text(list: &Chunk) -> TextProp {
        let stream = list.list(b"tdbs").map(stream).unwrap_or_default();
        let docs = list
            .list(b"btdk")
            .and_then(|c| cos::parse(c.body))
            .map(|cos| text_docs(&cos))
            .unwrap_or_default();
        TextProp { stream, docs }
    }

    fn text_docs(cos: &cos::Obj) -> Vec<TextDoc> {
        let fonts: Vec<String> = cos
            .at("0/1/0")
            .map(|a| {
                a.array()
                    .iter()
                    .map(|f| f.at("0/0/0").and_then(|s| s.string()).unwrap_or_default())
                    .collect()
            })
            .unwrap_or_default();
        let frame = cos.at("0/8/0/0/0/1/0").map(|a| {
            let v: Vec<f64> = a.array().iter().filter_map(|n| n.num()).collect();
            v
        });
        let frame = frame.filter(|v| v.len() >= 8).map(|v| {
            let xs = v.iter().step_by(2);
            let ys = v.iter().skip(1).step_by(2);
            let (x0, x1) = xs.fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(*x), b.max(*x)));
            let (y0, y1) = ys.fold((f64::MAX, f64::MIN), |(a, b), y| (a.min(*y), b.max(*y)));
            [x0, y0, x1 - x0, y1 - y0]
        });
        let docs = cos.at("1/1").map(|d| d.array()).unwrap_or(&[]);
        docs.iter()
            .map(|d| {
                let text = d
                    .at("0/0")
                    .and_then(|s| s.string())
                    .unwrap_or_default()
                    .trim_end_matches('\r')
                    .replace("\r\n", "\n")
                    .replace(['\r', '\u{3}'], "\n");
                let style = d.at("0/6/0/0/0/0/6");
                let num = |k: &str| style.and_then(|s| s.get(k)).and_then(|v| v.num());
                let flag = |k: &str| {
                    style
                        .and_then(|s| s.get(k))
                        .and_then(|v| v.boolean())
                        .unwrap_or(false)
                };
                let paint = |k: &str| -> Option<[f64; 4]> {
                    let c: Vec<f64> = style?
                        .get(k)?
                        .at("0/1")?
                        .array()
                        .iter()
                        .filter_map(|n| n.num())
                        .collect();
                    (c.len() >= 4).then(|| [c[1], c[2], c[3], c[0]])
                };
                let font = num("0")
                    .and_then(|i| fonts.get(i.max(0.0) as usize))
                    .cloned()
                    .unwrap_or_default();
                let auto_leading = style
                    .and_then(|s| s.get("4"))
                    .and_then(|v| v.boolean())
                    .unwrap_or(true);
                let apply_fill = style
                    .and_then(|s| s.get("56"))
                    .and_then(|v| v.boolean())
                    .unwrap_or(true);
                let apply_stroke = flag("57");
                TextDoc {
                    text,
                    font,
                    size: num("1").unwrap_or(36.0),
                    fill: if apply_fill { paint("53") } else { None },
                    stroke: if apply_stroke {
                        paint("54").map(|c| (c, num("63").unwrap_or(1.0)))
                    } else {
                        None
                    },
                    tracking: num("8").unwrap_or(0.0),
                    leading: if auto_leading { None } else { num("5") },
                    faux_bold: flag("2"),
                    faux_italic: flag("3"),
                    // Font caps option: 0 normal, 1 small caps, 2 all caps.
                    all_caps: num("12") == Some(2.0),
                    justify: d.at("0/5/0/0/0/0/5/0").and_then(|v| v.num()).unwrap_or(0.0) as i64,
                    frame,
                }
            })
            .collect()
    }

    /// Reads a `LIST:mrst` (markers).
    fn markers(list: &Chunk) -> Vec<MarkerVal> {
        let s = list.list(b"tdbs").map(stream).unwrap_or_default();
        let vals: Vec<&Chunk> = list
            .list(b"mrky")
            .map(|m| m.kids.iter().filter(|c| c.is_list(b"Nmrd")).collect())
            .unwrap_or_default();
        s.keys
            .iter()
            .zip(vals)
            .map(|(k, m)| {
                let h = m.child(b"NmHd").map(|c| c.body).unwrap_or(&[]);
                let div = match u32_at(h, 12) {
                    0 => 600.0,
                    d => d as f64,
                };
                MarkerVal {
                    t: k.t,
                    duration: u32_at(h, 8) as f64 / div,
                    comment: m.child(b"Utf8").map(|c| c.text()).unwrap_or_default(),
                }
            })
            .collect()
    }
}

// ---------------------------------------------------------------- model ----

/// Items, compositions and layers of a project.
#[allow(dead_code)]
mod model {
    use std::collections::HashMap;

    use super::props::{self, Group, MarkerVal, Node};
    use super::riff::{cstr, f32_at, f64_at, i32_at, ratio, u16_at, u32_at, u8_at, Chunk};
    use super::Controller;

    pub struct Project {
        /// File version (major, minor) from `head`.
        pub version: (u16, u16),
        pub comps: Vec<CompItem>,
        pub footage: HashMap<u32, Footage>,
    }

    pub struct CompItem {
        pub id: u32,
        pub name: String,
        pub width: f64,
        pub height: f64,
        pub fps: f64,
        /// Seconds.
        pub duration: f64,
        /// Layers, top first.
        pub layers: Vec<LayerRec>,
        pub markers: Vec<MarkerVal>,
        pub controllers: Vec<Controller>,
    }

    #[derive(Debug, Clone)]
    pub enum Source {
        Solid([f64; 3]),
        /// A file: its path as stored.
        File(String),
        Placeholder,
        Other,
    }

    pub struct Footage {
        pub name: String,
        pub source: Source,
        pub width: f64,
        pub height: f64,
        pub video: bool,
        pub audio: bool,
        pub still: bool,
    }

    pub struct LayerRec {
        pub id: u32,
        pub name: String,
        /// Source item id (footage or composition), 0 = none.
        pub source: u32,
        /// Seconds of composition time.
        pub start: f64,
        /// In and out points, seconds of layer time (unstretched).
        pub in_point: f64,
        pub out_point: f64,
        /// Time stretch as a ratio (1 = 100 %).
        pub stretch: f64,
        pub guide: bool,
        pub adjustment: bool,
        pub three_d: bool,
        pub null: bool,
        pub video: bool,
        pub audio: bool,
        pub motion_blur: bool,
        pub collapse: bool,
        pub blend: u8,
        pub matte_type: u8,
        pub matte_layer: u32,
        /// 0 AV, 1 light, 2 camera, 3 text, 4 shape, 5 3D model, 7 mesh.
        pub kind: u8,
        pub parent: u32,
        pub props: Group,
    }

    /// Reads the project tree.
    pub fn read(root: &Chunk) -> Result<Project, String> {
        let head = root
            .child(b"head")
            .ok_or("not an After Effects project: no header (head chunk)")?;
        let version = (u16_at(head.body, 0), u16_at(head.body, 2));
        let fold = root
            .list(b"Fold")
            .ok_or("not an After Effects project: no item list (Fold)")?;
        let mut p = Project {
            version,
            comps: Vec::new(),
            footage: HashMap::new(),
        };
        items(fold, &mut p, 0);
        Ok(p)
    }

    fn items(container: &Chunk, p: &mut Project, depth: usize) {
        if depth > 64 {
            return;
        }
        for item in container.kids.iter().filter(|c| c.is_list(b"Item")) {
            let Some(idta) = item.child(b"idta") else {
                continue;
            };
            let ty = u16_at(idta.body, 0);
            let id = u32_at(idta.body, 0x10);
            let flags = u8_at(idta.body, 0x17);
            // The name is the first Utf8 after idta.
            let name = item
                .kids
                .iter()
                .skip_while(|c| !c.is(b"idta"))
                .find(|c| c.is(b"Utf8"))
                .map(|c| c.text())
                .unwrap_or_default();
            match ty {
                1 => {
                    if let Some(sub) = item.list(b"Sfdr") {
                        items(sub, p, depth + 1);
                    }
                }
                4 => {
                    if let Some(c) = comp(item, id, name) {
                        p.comps.push(c);
                    }
                }
                7 => {
                    p.footage.insert(id, footage(item, name, flags));
                }
                _ => {}
            }
        }
    }

    fn comp(item: &Chunk, id: u32, name: String) -> Option<CompItem> {
        let cdta = item.child(b"cdta")?.body;
        if cdta.len() < 0xA0 {
            return None;
        }
        let fps = u32_at(cdta, 0x9C) as f64 / 65536.0;
        let duration = ratio(u32_at(cdta, 0x2C) as f64, u32_at(cdta, 0x30) as f64);
        let layers: Vec<LayerRec> = item
            .kids
            .iter()
            .filter(|c| c.is_list(b"Layr"))
            .filter_map(layer)
            .collect();
        let markers = item
            .list(b"SecL")
            .and_then(|l| l.list(b"tdgp"))
            .map(|g| props::group(g, 0))
            .and_then(|g| match g.get("ADBE Marker") {
                Some(Node::Markers(m)) => Some(m.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let eg = item
            .list(b"CIF3")
            .or_else(|| item.list(b"CIF2"))
            .or_else(|| item.list(b"CIFO"));
        Some(CompItem {
            id,
            name,
            width: u16_at(cdta, 0x8C).max(1) as f64,
            height: u16_at(cdta, 0x8E).max(1) as f64,
            fps: if fps.is_finite() && fps >= 1.0 {
                fps
            } else {
                25.0
            },
            duration: duration.max(0.0),
            layers,
            markers,
            controllers: eg.map(controllers).unwrap_or_default(),
        })
    }

    fn layer(l: &Chunk) -> Option<LayerRec> {
        let b = l.child(b"ldta")?.body;
        let name = l.child(b"Utf8").map(|c| c.text()).unwrap_or_default();
        let name = if name.is_empty() {
            cstr(b, 0x40, 32)
        } else {
            name
        };
        let (fa, fb, fc) = (u8_at(b, 0x25), u8_at(b, 0x26), u8_at(b, 0x27));
        let stretch = ratio(i32_at(b, 0x08) as f64, u32_at(b, 0x6C) as f64);
        Some(LayerRec {
            id: u32_at(b, 0),
            name,
            source: u32_at(b, 0x28),
            start: ratio(i32_at(b, 0x0C) as f64, u32_at(b, 0x10) as f64),
            in_point: ratio(i32_at(b, 0x14) as f64, u32_at(b, 0x18) as f64),
            out_point: ratio(i32_at(b, 0x1C) as f64, u32_at(b, 0x20) as f64),
            stretch: if stretch == 0.0 { 1.0 } else { stretch },
            guide: fa & 0x02 != 0,
            adjustment: fb & 0x02 != 0,
            three_d: fb & 0x04 != 0,
            null: fb & 0x80 != 0,
            video: fc & 0x01 != 0,
            audio: fc & 0x02 != 0,
            motion_blur: fc & 0x08 != 0,
            collapse: fc & 0x80 != 0,
            blend: u8_at(b, 0x63),
            matte_type: u8_at(b, 0x6B),
            matte_layer: if b.len() >= 0xA4 { u32_at(b, 0xA0) } else { 0 },
            kind: u8_at(b, 0x83),
            parent: u32_at(b, 0x84),
            props: l
                .list(b"tdgp")
                .map(|g| props::group(g, 0))
                .unwrap_or_default(),
        })
    }

    fn footage(item: &Chunk, name: String, flags: u8) -> Footage {
        let pin = item.list(b"Pin ");
        let sspc = pin
            .and_then(|p| p.child(b"sspc"))
            .map(|c| c.body)
            .unwrap_or(&[]);
        let opti = pin
            .and_then(|p| p.child(b"opti"))
            .map(|c| c.body)
            .unwrap_or(&[]);
        let path = pin
            .and_then(|p| p.list(b"Als2"))
            .and_then(|a| a.child(b"alas"))
            .and_then(|c| serde_json::from_slice::<serde_json::Value>(c.body).ok())
            .and_then(|j| j.get("fullpath")?.as_str().map(str::to_string));
        let (source, own_name) = if opti.starts_with(b"Soli") {
            (
                Source::Solid([f32_at(opti, 0x0E), f32_at(opti, 0x12), f32_at(opti, 0x16)]),
                cstr(opti, 0x1A, 256),
            )
        } else if let Some(p) = path {
            let file = p.rsplit(['/', '\\']).next().unwrap_or(&p).to_string();
            (Source::File(p), file)
        } else if opti.starts_with(&[0, 0, 0, 0]) && opti.len() >= 0x0A {
            (Source::Placeholder, cstr(opti, 0x0A, 256))
        } else {
            (Source::Other, String::new())
        };
        Footage {
            name: if name.is_empty() { own_name } else { name },
            source,
            width: u32_at(sspc, 0x1E) as f64,
            height: u32_at(sspc, 0x22) as f64,
            video: flags & 0x20 != 0,
            audio: flags & 0x04 != 0,
            still: flags & 0x10 != 0,
        }
    }

    /// Essential Graphics controllers (`LIST:CIF3` and its older copies).
    fn controllers(cif: &Chunk) -> Vec<Controller> {
        cif.kids
            .iter()
            .filter(|c| c.is_list(b"CCtl"))
            .take(10_000)
            .map(|c| {
                let name = c
                    .list(b"CpS2")
                    .and_then(|s| s.child(b"Utf8"))
                    .map(|u| u.text())
                    .unwrap_or_default();
                let kind = c.child(b"CTyp").map(|t| u32_at(t.body, 0)).unwrap_or(0);
                // Direct Utf8 children: the id, then (text) the text value.
                let strings: Vec<String> = c
                    .kids
                    .iter()
                    .filter(|k| k.is(b"Utf8"))
                    .map(|k| k.text())
                    .collect();
                let cval = c.child(b"CVal").map(|v| v.body).unwrap_or(&[]);
                let value = match (kind, cval.len()) {
                    (1, _) => vec![u8_at(cval, 0) as f64],
                    (13, _) => vec![u32_at(cval, 0) as f64],
                    (4, 16) => (0..4).map(|i| f32_at(cval, i * 4)).collect(),
                    (_, n) if n >= 8 => (0..n / 8).map(|i| f64_at(cval, i * 8)).collect(),
                    _ => Vec::new(),
                };
                let f8 = |tag: &[u8; 4]| c.child(tag).map(|v| f64_at(v.body, 0));
                let items = c
                    .list(b"StVc")
                    .map(|s| {
                        s.kids
                            .iter()
                            .filter(|k| k.is(b"Utf8"))
                            .map(|k| k.text())
                            .collect()
                    })
                    .unwrap_or_default();
                let prp = c.list(b"CPrp");
                let path = prp
                    .and_then(|p| p.child(b"Utf8"))
                    .and_then(|u| serde_json::from_slice::<serde_json::Value>(u.body).ok())
                    .and_then(|j| {
                        let o = j.as_object()?;
                        let mut steps: Vec<(usize, String)> = o
                            .iter()
                            .filter_map(|(k, v)| {
                                Some((k.parse().ok()?, v.get("matchName")?.as_str()?.to_string()))
                            })
                            .collect();
                        steps.sort();
                        Some(steps.into_iter().map(|(_, m)| m).collect())
                    })
                    .unwrap_or_default();
                Controller {
                    id: strings.first().cloned().unwrap_or_default(),
                    name,
                    kind,
                    value: if kind == 6 { Vec::new() } else { value },
                    min: f8(b"Smin"),
                    max: f8(b"Smax"),
                    items,
                    text: if kind == 6 {
                        strings.get(1).cloned().unwrap_or_default()
                    } else {
                        String::new()
                    },
                    layer: prp
                        .and_then(|p| p.child(b"CLId"))
                        .map(|v| u32_at(v.body, 0))
                        .unwrap_or(0),
                    path,
                }
            })
            .collect()
    }
}

// ----------------------------------------------------------------- conv ----

/// From the project model to templates and their report.
mod conv {
    use super::model::{CompItem, Footage, LayerRec, Project, Source};
    use super::props::{Group, Key, Node, Shape, Stream};
    use super::*;
    use cg_core::wid::pen::{Arrow, HandleMode, PenPoint, PointType};
    use cg_core::wid::shape::StrokePosition;

    /// Default influence of a linear side, as After Effects reports it.
    const LINEAR_INFLUENCE: f64 = 0.16666666667;

    /// Maps a time of a (pre)composition to the root composition (s).
    #[derive(Clone, Copy)]
    struct Time {
        scale: f64,
        offset: f64,
    }

    impl Time {
        fn root(&self, t: f64) -> f64 {
            self.offset + t * self.scale
        }
    }

    /// A rectangle in a layer's own coordinates.
    #[derive(Debug, Clone, Copy)]
    struct Bx {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    }

    impl Bx {
        fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
            Self { x, y, w, h }
        }
        fn union(self, o: Bx) -> Bx {
            let x = self.x.min(o.x);
            let y = self.y.min(o.y);
            Bx::new(
                x,
                y,
                (self.x + self.w).max(o.x + o.w) - x,
                (self.y + self.h).max(o.y + o.h) - y,
            )
        }
    }

    /// A converted layer: the template layer, its line in the report (none
    /// for the inner layers the conversion adds), its children (top first)
    /// and its place in the After Effects stack.
    struct Out {
        layer: Layer,
        report: Option<LayerReport>,
        kids: Vec<Out>,
        order: usize,
    }

    /// Animated properties of a layer: label → status and detail.
    type Anims = BTreeMap<&'static str, (Status, Option<&'static str>)>;

    /// What a composition's conversion gathers.
    struct Cx {
        fps: f64,
        frames: f64,
        fields: Vec<Field>,
        ignored: Vec<Ignored>,
        /// Compositions being converted (the root first): cycles, depth.
        stack: Vec<u32>,
        layers: HashMap<u32, Vec<String>>,
        text_keys: HashMap<u32, String>,
        /// Layers converted, pre-compositions included (a pre-composition
        /// used many times is converted each time).
        count: usize,
        name: String,
    }

    /// Most layers converted in one composition.
    const MAX_LAYERS: usize = 5_000;

    impl Cx {
        /// Template frame of a layer-time second.
        fn frame(&self, time: Time, l: &LayerRec, t: f64) -> f64 {
            round3(time.root(l.start + t * l.stretch) * self.fps)
        }
        fn root_level(&self) -> bool {
            self.stack.len() == 1
        }
    }

    pub struct Converter<'a> {
        p: &'a Project,
        ctx: &'a Ctx,
        media: Option<Media<'a>>,
        pub files: Vec<(String, Vec<u8>)>,
        /// Footage id → its `src` ("" when missing).
        staged: HashMap<u32, String>,
        missing: Vec<String>,
        notes: Vec<Note>,
        main: usize,
    }

    /// What a layer is made of.
    enum What<'p> {
        Text,
        Shape,
        Solid([f64; 3], f64, f64),
        Image(u32, &'p Footage),
        Precomp(&'p CompItem),
        Null(f64, f64),
        Skip(&'static str, &'static str),
    }

    impl<'a> Converter<'a> {
        pub fn new(
            p: &'a Project,
            ctx: &'a Ctx,
            media: Option<Media<'a>>,
            main: Option<&str>,
        ) -> Self {
            let used = used_comps(p);
            let main = main
                .and_then(|n| p.comps.iter().position(|c| c.name == n))
                .or_else(|| {
                    p.comps
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| selected(c, &used))
                        .max_by_key(|(i, c)| (c.layers.len(), usize::MAX - i))
                        .map(|(i, _)| i)
                })
                .unwrap_or(0);
            Self {
                p,
                ctx,
                media,
                files: Vec::new(),
                staged: HashMap::new(),
                missing: Vec::new(),
                notes: Vec::new(),
                main,
            }
        }

        /// Width, height and frame rate of the main composition.
        pub fn main(&self) -> (u32, u32, f64) {
            let c = &self.p.comps[self.main];
            (c.width as u32, c.height as u32, round3(c.fps))
        }

        pub fn warnings(&self) -> Vec<Note> {
            let mut out = Vec::new();
            let (major, minor) = self.p.version;
            if !(92..=97).contains(&major) {
                out.push(
                    Note::new(
                        "After Effects file version {version} not tested",
                        Status::Partial,
                    )
                    .with("version", format!("{major}.{minor}")),
                );
            }
            for f in &self.missing {
                out.push(Note::new("Missing footage {file}", Status::Partial).with("file", f));
            }
            out.extend(self.notes.iter().cloned());
            out
        }

        pub fn run(&mut self) -> (Vec<Comp>, Vec<CompInfo>) {
            let used = used_comps(self.p);
            let mut comps = Vec::new();
            let mut infos = Vec::new();
            for c in &self.p.comps {
                let (comp, info) = self.comp(c, &used);
                comps.push(comp);
                infos.push(info);
            }
            (comps, infos)
        }

        fn comp(&mut self, c: &'a CompItem, used: &HashSet<u32>) -> (Comp, CompInfo) {
            let mut cx = Cx {
                fps: c.fps,
                frames: (c.duration * c.fps).round().max(1.0),
                fields: Vec::new(),
                ignored: Vec::new(),
                stack: vec![c.id],
                layers: HashMap::new(),
                text_keys: HashMap::new(),
                count: 0,
                name: c.name.clone(),
            };
            let (mw, mh, _) = self.main();
            let (mw, mh) = (mw as f64, mh as f64);
            // A composition of another size is fitted into the main one.
            let fit = if (c.width - mw).abs() > 0.5 || (c.height - mh).abs() > 0.5 {
                let s = (mw / c.width).min(mh / c.height);
                let mut g = mk(&new_id(), &c.name, None, group(false));
                set(&mut g, Prop::X, ((mw - c.width * s) / 2.0).round());
                set(&mut g, Prop::Y, ((mh - c.height * s) / 2.0).round());
                set(&mut g, Prop::Width, c.width);
                set(&mut g, Prop::Height, c.height);
                set(&mut g, Prop::AnchorX, 0.0);
                set(&mut g, Prop::AnchorY, 0.0);
                set(&mut g, Prop::ScaleX, round4(s));
                set(&mut g, Prop::ScaleY, round4(s));
                self.notes.push(
                    Note::new(
                        "Composition {name} scaled to fit {width}×{height}",
                        Status::Partial,
                    )
                    .with("name", &c.name)
                    .with("width", format!("{mw}"))
                    .with("height", format!("{mh}")),
                );
                Some(g)
            } else {
                None
            };
            let time = Time {
                scale: 1.0,
                offset: 0.0,
            };
            let parent = fit.as_ref().map(|g| g.id.clone());
            let mut outs = self.layers(&mut cx, c, parent.as_deref(), (0.0, 0.0), time);
            if let Some(g) = fit {
                outs = vec![Out {
                    layer: g,
                    report: None,
                    kids: outs,
                    order: 0,
                }];
            }
            let mut layers = Vec::new();
            let mut reports = Vec::new();
            for o in outs.iter().rev() {
                paint_order(o, &mut layers);
            }
            for o in outs {
                report_order(o, 0, &mut reports);
            }
            let mut template = Template::blank(&c.name, &self.ctx.zone, c.fps);
            template.duration = cx.frames;
            template.layers = layers;
            template.fields = cx.fields;
            template.markers = markers(c, &template);
            let animated = template
                .layers
                .iter()
                .any(|l| l.tracks.values().any(|t| t.len() > 1));
            let comp = Comp {
                id: template.id.clone(),
                name: c.name.clone(),
                duration: round3(c.duration),
                selected: selected(c, used),
                template,
                layers: reports,
                ignored: cx.ignored,
                variables: Vec::new(),
                animated,
                flat: None,
            };
            let info = CompInfo {
                item: c.id,
                name: c.name.clone(),
                controllers: c.controllers.clone(),
                layers: cx.layers,
                text_keys: cx.text_keys,
            };
            (comp, info)
        }

        /// The layers of a composition, top first. Layers parented to
        /// another one go inside it.
        fn layers(
            &mut self,
            cx: &mut Cx,
            comp: &'a CompItem,
            parent: Option<&str>,
            origin: (f64, f64),
            time: Time,
        ) -> Vec<Out> {
            let ids: HashSet<u32> = comp.layers.iter().map(|l| l.id).collect();
            let mattes: HashSet<u32> = comp
                .layers
                .iter()
                .enumerate()
                .filter(|(_, l)| l.matte_type != 0)
                .filter_map(|(i, l)| match l.matte_layer {
                    0 => i.checked_sub(1).map(|j| comp.layers[j].id),
                    id => Some(id),
                })
                .collect();
            let mut seen = HashSet::new();
            let mut outs = Vec::new();
            let roots: Vec<usize> = (0..comp.layers.len())
                .filter(|&i| {
                    let p = comp.layers[i].parent;
                    p == 0 || p == comp.layers[i].id || !ids.contains(&p)
                })
                .collect();
            for i in roots {
                let o = self.layer(cx, comp, i, parent, origin, time, &mut seen, &mattes, 0);
                outs.extend(o);
            }
            // Layers in a parenting loop never reached from a root.
            for i in 0..comp.layers.len() {
                if !seen.contains(&comp.layers[i].id) {
                    let o = self.layer(cx, comp, i, parent, origin, time, &mut seen, &mattes, 0);
                    outs.extend(o);
                }
            }
            outs.sort_by_key(|o| o.order);
            outs
        }

        fn what(&self, l: &LayerRec) -> What<'a> {
            if l.guide {
                return What::Skip("Guide layer", "guide");
            }
            match l.kind {
                1 => return What::Skip("Light layer", "light"),
                2 => return What::Skip("Camera", "camera"),
                3 => return What::Text,
                4 => return What::Shape,
                5 | 7 => return What::Skip("3D model", "3d"),
                0 => {}
                _ => return What::Skip("Unsupported layer", "layer"),
            }
            if l.adjustment {
                return What::Skip("Adjustment layer", "adjustment");
            }
            if let Some(c) = self.p.comps.iter().find(|c| c.id == l.source) {
                return What::Precomp(c);
            }
            let Some(f) = self.p.footage.get(&l.source) else {
                return if l.null {
                    What::Null(100.0, 100.0)
                } else {
                    What::Skip("Unsupported layer", "layer")
                };
            };
            if l.null {
                return What::Null(f.width.max(1.0), f.height.max(1.0));
            }
            match &f.source {
                Source::Solid(c) => What::Solid(*c, f.width.max(1.0), f.height.max(1.0)),
                Source::Placeholder if f.video => What::Image(l.source, f),
                Source::Placeholder => What::Skip("Audio", "audio"),
                Source::File(path) => {
                    let ext = ext_of(path);
                    if !f.video && f.audio {
                        What::Skip("Audio", "audio")
                    } else if !f.video {
                        What::Skip("Data footage", "data")
                    } else if f.still || is_picture(&ext) {
                        What::Image(l.source, f)
                    } else {
                        What::Skip("Video footage", "video")
                    }
                }
                Source::Other => What::Skip("Unsupported footage", "footage"),
            }
        }

        /// One layer (and the layers parented to it).
        #[allow(clippy::too_many_arguments)]
        fn layer(
            &mut self,
            cx: &mut Cx,
            comp: &'a CompItem,
            index: usize,
            parent: Option<&str>,
            origin: (f64, f64),
            time: Time,
            seen: &mut HashSet<u32>,
            mattes: &HashSet<u32>,
            depth: usize,
        ) -> Vec<Out> {
            let l = &comp.layers[index];
            if !seen.insert(l.id) {
                return Vec::new();
            }
            if depth > 24 {
                let name = self.layer_name(l);
                cx.ignored
                    .push(Ignored::new(&name, "layer", "Parenting too deep"));
                return Vec::new();
            }
            cx.count += 1;
            if cx.count > MAX_LAYERS {
                if cx.count == MAX_LAYERS + 1 {
                    self.notes.push(
                        Note::new(
                            "Composition {name}: too many layers, the rest was left out",
                            Status::Partial,
                        )
                        .with("name", &cx.name),
                    );
                }
                return Vec::new();
            }
            let name = self.layer_name(l);
            let children: Vec<usize> = (0..comp.layers.len())
                .filter(|&i| comp.layers[i].parent == l.id && comp.layers[i].id != l.id)
                .collect();
            let what = self.what(l);
            if let What::Skip(reason, source) = what {
                cx.ignored.push(Ignored::new(&name, source, reason));
                // Its children stay, without it.
                let mut out = Vec::new();
                for c in children {
                    for mut o in
                        self.layer(cx, comp, c, parent, origin, time, seen, mattes, depth + 1)
                    {
                        if let Some(r) = &mut o.report {
                            r.notes.push(
                                Note::new("Parent {name} not converted", Status::Partial)
                                    .with("name", &name),
                            );
                            r.status = worst(&r.notes);
                        }
                        out.push(o);
                    }
                }
                return out;
            }

            let mut notes = Vec::new();
            let mut anims = Anims::new();
            let id = new_id();
            let has_kids = !children.is_empty();
            // The layer's own content: kind, box in layer space, extra
            // children (shape parts, a precomposition's layers).
            let content_id = if has_kids { new_id() } else { id.clone() };
            let mut single = None;
            let (kind, bounds, mut inner, source, src_size) = match what {
                What::Text => match self.text(cx, l, &name, &mut notes) {
                    Some((k, b)) => (k, b, Vec::new(), "text", None),
                    None => {
                        cx.ignored.push(Ignored::new(&name, "text", "Empty text"));
                        return Vec::new();
                    }
                },
                What::Shape => {
                    let (k, b, parts, p) =
                        self.shape(cx, l, &content_id, time, &mut notes, &mut anims);
                    single = p;
                    (k, b, parts, "shape", None)
                }
                What::Solid(_, w, h) => (
                    LayerKind::rect(),
                    Bx::new(0.0, 0.0, w, h),
                    Vec::new(),
                    "solid",
                    Some((w, h)),
                ),
                What::Image(fid, f) => {
                    let src = self.image(fid, f, &mut notes);
                    let (w, h) = (f.width.max(1.0), f.height.max(1.0));
                    (
                        LayerKind::Image {
                            src,
                            fit: ImageFit::Stretch,
                            x: Default::default(),
                        },
                        Bx::new(0.0, 0.0, w, h),
                        Vec::new(),
                        "footage",
                        Some((w, h)),
                    )
                }
                What::Precomp(pc) => {
                    let inner = self.precomp(cx, l, pc, &content_id, time, &mut notes);
                    (
                        group(!l.collapse),
                        Bx::new(0.0, 0.0, pc.width, pc.height),
                        inner,
                        "precomp",
                        Some((pc.width, pc.height)),
                    )
                }
                What::Null(w, h) => (
                    group(false),
                    Bx::new(0.0, 0.0, w, h),
                    Vec::new(),
                    "null",
                    Some((w, h)),
                ),
                // Handled above.
                What::Skip(..) => return Vec::new(),
            };
            let is_null = matches!(what, What::Null(..));

            // The layer that moves: the content itself, or a group holding
            // the content and the layers parented to it.
            let mut top = mk(
                &id,
                &name,
                parent,
                if has_kids { group(false) } else { kind.clone() },
            );
            set(&mut top, Prop::Width, bounds.w.max(1.0).round());
            set(&mut top, Prop::Height, bounds.h.max(1.0).round());
            self.transform(
                cx, &mut top, l, comp, bounds, origin, time, src_size, is_null, &mut anims,
            );
            let mut content = if has_kids && !is_null {
                let mut c = mk(&content_id, &name, Some(&id), kind);
                set(&mut c, Prop::X, 0.0);
                set(&mut c, Prop::Y, 0.0);
                set(&mut c, Prop::Width, bounds.w.max(1.0).round());
                set(&mut c, Prop::Height, bounds.h.max(1.0).round());
                set(&mut c, Prop::AnchorX, 0.0);
                set(&mut c, Prop::AnchorY, 0.0);
                // Opacity is not inherited through parenting.
                if let Some(v) = top.props.remove(&Prop::Opacity) {
                    c.props.insert(Prop::Opacity, v);
                }
                if let Some(t) = top.tracks.remove(&Prop::Opacity) {
                    c.tracks.insert(Prop::Opacity, t);
                }
                Some(c)
            } else {
                None
            };
            if is_null {
                // A null's opacity changes nothing.
                top.props.remove(&Prop::Opacity);
                top.tracks.remove(&Prop::Opacity);
            }
            {
                let visual = content.as_mut().unwrap_or(&mut top);
                match what {
                    What::Solid(c, ..) => {
                        visual.props.insert(
                            Prop::Fill,
                            Value::Color(Color::rgba(c[0] as f32, c[1] as f32, c[2] as f32, 1.0)),
                        );
                    }
                    What::Text => self.text_props(l, visual),
                    _ => {}
                }
                if let Some(p) = &single {
                    self.paint(cx, visual, p, bounds, l, time, &mut anims);
                }
                if !is_null {
                    self.in_out(cx, visual, l, time);
                    if !l.video {
                        visual.visible = false;
                    }
                    self.styles(l, visual, &mut notes);
                }
            }
            // Shape parts already sit in `inner` (children of the content).
            for o in &mut inner {
                o.order = 0;
            }
            self.layer_notes(l, &mut notes, mattes);
            for (label, (status, detail)) in anims {
                let mut n = Note::anim(label, status);
                if let Some(d) = detail {
                    n = n.with("detail", d);
                }
                notes.push(n);
            }
            let report = LayerReport {
                layer: id.clone(),
                name: name.clone(),
                source: source.into(),
                depth: 0,
                status: worst(&notes),
                notes,
                hidden: !l.video,
            };
            if cx.root_level() {
                let mut ids = vec![id.clone()];
                if content.is_some() {
                    ids.push(content_id.clone());
                }
                ids.extend(
                    inner
                        .iter()
                        .filter(|o| o.report.is_none())
                        .map(|o| o.layer.id.clone()),
                );
                cx.layers.insert(l.id, ids);
            }

            // The parented layers, placed in the group's box.
            let mut kids = Vec::new();
            let inner_origin = (bounds.x, bounds.y);
            for c in children {
                kids.extend(self.layer(
                    cx,
                    comp,
                    c,
                    Some(&id),
                    inner_origin,
                    time,
                    seen,
                    mattes,
                    depth + 1,
                ));
            }
            match content {
                Some(c) => kids.push(Out {
                    layer: c,
                    report: None,
                    kids: inner,
                    order: index,
                }),
                None => kids.extend(inner),
            }
            kids.sort_by_key(|o| o.order);
            vec![Out {
                layer: top,
                report: Some(report),
                kids,
                order: index,
            }]
        }

        fn layer_name(&self, l: &LayerRec) -> String {
            if !l.name.trim().is_empty() {
                return l.name.clone();
            }
            if let Some(f) = self.p.footage.get(&l.source) {
                if !f.name.is_empty() {
                    return f.name.clone();
                }
            }
            if let Some(c) = self.p.comps.iter().find(|c| c.id == l.source) {
                return c.name.clone();
            }
            format!("Layer {}", l.id)
        }

        /// Notes about what the layer has that is not converted.
        fn layer_notes(&self, l: &LayerRec, notes: &mut Vec<Note>, mattes: &HashSet<u32>) {
            if let Some(fx) = l.props.group("ADBE Effect Parade") {
                for (_, n) in &fx.kids {
                    if let Node::Effect { name, .. } = n {
                        notes.push(Note::new("Effect {name}", Status::Ignored).with("name", name));
                    }
                }
            }
            if let Some(masks) = l.props.group("ADBE Mask Parade") {
                for (i, (_, n)) in masks.kids.iter().enumerate() {
                    if let Node::Mask { mode, group, .. } = n {
                        if *mode != 0 {
                            let name = group.name.clone().unwrap_or(format!("Mask {}", i + 1));
                            notes
                                .push(Note::new("Mask {name}", Status::Partial).with("name", name));
                        }
                    }
                }
            }
            if l.matte_type != 0 {
                notes.push(Note::new("Track matte", Status::Partial));
            }
            if mattes.contains(&l.id) {
                notes.push(Note::new("Used as track matte", Status::Partial));
            }
            if l.three_d {
                notes.push(Note::new("3D flattened", Status::Partial));
            }
            if !matches!(l.blend, 0 | 2) {
                notes.push(
                    Note::new("Blend mode {mode}", Status::Partial)
                        .with("mode", blend_name(l.blend)),
                );
            }
            if l.motion_blur {
                notes.push(Note::new("Motion blur", Status::Ignored));
            }
            if (l.stretch - 1.0).abs() > 1e-6 {
                notes.push(
                    Note::new("Time stretch {percent}", Status::Ok)
                        .with("percent", format!("{}%", round3(l.stretch * 100.0))),
                );
            }
            if l.props
                .stream("ADBE Time Remapping")
                .is_some_and(|s| s.enabled && s.animated())
            {
                notes.push(Note::new("Time remap", Status::Partial));
            }
            let mut exprs = Vec::new();
            expressions(&l.props, &mut exprs, 0);
            for p in exprs {
                notes.push(Note::new("Expression", Status::Ignored).with("property", p));
            }
        }

        /// Anchor point, position, scale, rotation and opacity. `b` is the
        /// box in the layer's space; `origin` the parent's box corner.
        #[allow(clippy::too_many_arguments)]
        fn transform(
            &self,
            cx: &Cx,
            out: &mut Layer,
            l: &LayerRec,
            comp: &CompItem,
            b: Bx,
            origin: (f64, f64),
            time: Time,
            src: Option<(f64, f64)>,
            null: bool,
            anims: &mut Anims,
        ) {
            let empty = Group::default();
            let tg = l.props.group("ADBE Transform Group").unwrap_or(&empty);
            // A layer with a source stores its anchor as a fraction of it.
            let (sx, sy) = src.unwrap_or((1.0, 1.0));
            let (ax, ay) = match tg.stream("ADBE Anchor Point") {
                Some(a) => {
                    if a.animated() {
                        anims.insert("Anchor point", (Status::Partial, Some("first value kept")));
                    }
                    (a.at(0).unwrap_or(0.0) * sx, a.at(1).unwrap_or(0.0) * sy)
                }
                None if null || src.is_none() => (0.0, 0.0),
                None => (sx / 2.0, sy / 2.0),
            };
            let w = b.w.max(1.0);
            let h = b.h.max(1.0);
            set(out, Prop::AnchorX, round4((ax - b.x) / w));
            set(out, Prop::AnchorY, round4((ay - b.y) / h));
            let dx = b.x - ax - origin.0;
            let dy = b.y - ay - origin.1;

            let pos = tg.stream("ADBE Position");
            if pos.is_some_and(|p| p.separated) {
                for (prop, mn, d) in [
                    (Prop::X, "ADBE Position_0", dx),
                    (Prop::Y, "ADBE Position_1", dy),
                ] {
                    let s = tg.stream(mn);
                    apply(
                        cx,
                        out,
                        prop,
                        s,
                        0,
                        time,
                        l,
                        |v| v + d,
                        None,
                        "Position",
                        anims,
                    );
                }
            } else {
                let centre = [comp.width / 2.0, comp.height / 2.0];
                for (prop, dim, d) in [(Prop::X, 0, dx), (Prop::Y, 1, dy)] {
                    let c = centre[dim];
                    apply(
                        cx,
                        out,
                        prop,
                        pos,
                        dim,
                        time,
                        l,
                        |v| v + d,
                        Some(c + d),
                        "Position",
                        anims,
                    );
                }
            }
            let s = tg.stream("ADBE Scale");
            apply(
                cx,
                out,
                Prop::ScaleX,
                s,
                0,
                time,
                l,
                |v| v,
                None,
                "Scale",
                anims,
            );
            apply(
                cx,
                out,
                Prop::ScaleY,
                s,
                1,
                time,
                l,
                |v| v,
                None,
                "Scale",
                anims,
            );
            let r = tg.stream("ADBE Rotate Z");
            apply(
                cx,
                out,
                Prop::Rotation,
                r,
                0,
                time,
                l,
                |v| v,
                None,
                "Rotation",
                anims,
            );
            let o = tg.stream("ADBE Opacity");
            apply(
                cx,
                out,
                Prop::Opacity,
                o,
                0,
                time,
                l,
                |v| v,
                None,
                "Opacity",
                anims,
            );
        }

        /// Layer in / out points become opacity holds.
        fn in_out(&self, cx: &Cx, out: &mut Layer, l: &LayerRec, time: Time) {
            let (a, b) = (
                cx.frame(time, l, l.in_point),
                cx.frame(time, l, l.out_point),
            );
            let (ip, op) = (a.min(b), a.max(b));
            let late = (ip > 0.5).then_some(ip);
            let early = (op < cx.frames - 0.5).then_some(op);
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
                    // Animated opacity: cut at the in / out points, the
                    // value there interpolated.
                    if let Some(ip) = late {
                        let (v, easing) = value_at(track, ip);
                        track.retain(|k| k.frame > ip);
                        let easing = if track.is_empty() {
                            Easing::Hold
                        } else {
                            easing
                        };
                        track.insert(
                            0,
                            Keyframe {
                                frame: ip,
                                value: Value::Number(v),
                                easing,
                            },
                        );
                        track.insert(0, hold(0.0, 0.0));
                    }
                    if let Some(op) = early {
                        let (v, _) = value_at(track, op);
                        let straddles = track.iter().any(|k| k.frame >= op);
                        track.retain(|k| k.frame < op);
                        if straddles && track.last().is_some_and(|k| k.frame < op - 0.002) {
                            track.push(hold(op - 0.001, v));
                        }
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

        /// Layer styles: the drop shadow is converted, the others noted.
        fn styles(&self, l: &LayerRec, out: &mut Layer, notes: &mut Vec<Note>) {
            let Some(st) = l.props.group("ADBE Layer Styles").filter(|g| g.enabled) else {
                return;
            };
            for (mn, n) in &st.kids {
                let Node::Group(g) = n else { continue };
                if !g.enabled || !mn.ends_with("/enabled") {
                    continue;
                }
                if mn == "dropShadow/enabled" {
                    let v = |k: &str, d: f64| {
                        g.stream(&format!("dropShadow/{k}"))
                            .and_then(|s| s.at(0))
                            .unwrap_or(d)
                    };
                    let global = st
                        .group("ADBE Blend Options Group")
                        .and_then(|b| b.stream("ADBE Global Angle2"))
                        .and_then(|s| s.at(0))
                        .unwrap_or(120.0);
                    let angle = if v("useGlobalLight", 0.0) != 0.0 {
                        global
                    } else {
                        v("localLightingAngle", 120.0)
                    }
                    .to_radians();
                    let dist = v("distance", 5.0);
                    let c = g
                        .stream("dropShadow/color")
                        .map(|s| s.first().to_vec())
                        .filter(|c| c.len() >= 3)
                        .unwrap_or(vec![0.0, 0.0, 0.0, 1.0]);
                    let alpha = (v("opacity", 75.0) / 100.0).clamp(0.0, 1.0);
                    out.drop_shadow = true;
                    out.props.insert(
                        Prop::ShadowColor,
                        Value::Color(Color::rgba(
                            c[0] as f32,
                            c[1] as f32,
                            c[2] as f32,
                            alpha as f32,
                        )),
                    );
                    set(out, Prop::ShadowX, round4(-dist * angle.cos()));
                    set(out, Prop::ShadowY, round4(dist * angle.sin()));
                    set(out, Prop::ShadowBlur, v("blur", 5.0));
                    notes.push(Note::new("Drop shadow", Status::Ok));
                } else {
                    let name = match mn.trim_end_matches("/enabled") {
                        "innerShadow" => "Inner Shadow",
                        "outerGlow" => "Outer Glow",
                        "innerGlow" => "Inner Glow",
                        "bevelEmboss" => "Bevel and Emboss",
                        "chromeFX" => "Satin",
                        "solidFill" => "Color Overlay",
                        "gradientFill" => "Gradient Overlay",
                        "patternFill" => "Pattern Overlay",
                        "frameFX" => "Stroke",
                        other => other,
                    };
                    notes.push(Note::new("Layer style {name}", Status::Ignored).with("name", name));
                }
            }
        }

        // ------------------------------------------------------- text --

        /// A text layer: its kind and box (baseline of the first line at y
        /// = 0 for point text), and the field it becomes.
        fn text(
            &mut self,
            cx: &mut Cx,
            l: &LayerRec,
            name: &str,
            notes: &mut Vec<Note>,
        ) -> Option<(LayerKind, Bx)> {
            let tp = l.props.group("ADBE Text Properties")?;
            let Some(Node::Text(t)) = tp.get("ADBE Text Document") else {
                return None;
            };
            let d = t.docs.first()?;
            if t.docs.len() > 1 || t.stream.keys.len() > 1 {
                notes.push(
                    Note::new("Text keyframes", Status::Partial).with("detail", "first text kept"),
                );
            }
            if let Some(an) = tp.group("ADBE Text Animators") {
                for (i, (_, n)) in an.kids.iter().enumerate() {
                    if let Node::Group(g) = n {
                        let nm = g.name.clone().unwrap_or(format!("Animator {}", i + 1));
                        notes.push(Note::new("Text animator", Status::Ignored).with("name", nm));
                    }
                }
            }
            let (family, weight, italic) = font_parts(&d.font);
            let (font, have) = self.ctx.font(&family);
            if !have {
                notes.push(
                    Note::new("Font {from} → {to}", Status::Partial)
                        .with("from", if d.font.is_empty() { &family } else { &d.font })
                        .with("to", &font),
                );
            }
            let size = if d.size > 0.0 { d.size } else { 36.0 };
            let (align, justify) = match d.justify {
                1 | 4 => (TextAlign::Right, d.justify == 4),
                2 | 5 => (TextAlign::Center, d.justify == 5),
                3 | 6 => (TextAlign::Left, true),
                _ => (TextAlign::Left, false),
            };
            let lines = d.text.lines().count().max(1) as f64;
            let longest = d.text.lines().map(|s| s.chars().count()).max().unwrap_or(1) as f64;
            let line_height = d.leading.filter(|v| *v > 0.0).map(|v| (v / size) as f32);
            let lh = line_height.map_or(1.2, |v| v as f64);
            let (b, wrap) = match d.frame {
                Some([x, y, w, h]) if w > 0.0 && h > 0.0 => (Bx::new(x, y, w, h), true),
                _ => {
                    let w = (longest * size * 0.62).max(size);
                    let x = match align {
                        TextAlign::Left => 0.0,
                        TextAlign::Center => -w / 2.0,
                        TextAlign::Right => -w,
                    };
                    (Bx::new(x, -size * 1.02, w, size * lh * lines), false)
                }
            };
            let key = unique_key(&cx.fields, name);
            cx.fields.push(Field {
                key: key.clone(),
                label: name.to_string(),
                kind: if lines > 1.0 {
                    FieldKind::Multiline
                } else {
                    FieldKind::Text
                },
                default: d.text.clone(),
                binding: None,
                min: None,
                max: None,
                options: Vec::new(),
            });
            if cx.root_level() {
                cx.text_keys.insert(l.id, key.clone());
            }
            notes.push(Note::new("Text → variable", Status::Ok));
            let x = cg_core::wid::text::Config {
                italic: italic || d.faux_italic,
                justify,
                ..Default::default()
            };
            Some((
                LayerKind::Text {
                    x,
                    wrap,
                    transform: Default::default(),
                    decoration: Default::default(),
                    line_height,
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
                    font_weight: if d.faux_bold { weight.max(700) } else { weight },
                    align,
                    vertical_align: VerticalAlign::Top,
                    uppercase: d.all_caps,
                },
                b,
            ))
        }

        /// Size, colour, tracking and stroke of a text.
        fn text_props(&self, l: &LayerRec, out: &mut Layer) {
            let Some(Node::Text(t)) = l
                .props
                .group("ADBE Text Properties")
                .and_then(|g| g.get("ADBE Text Document"))
            else {
                return;
            };
            let Some(d) = t.docs.first() else { return };
            let size = if d.size > 0.0 { d.size } else { 36.0 };
            set(out, Prop::FontSize, round4(size));
            if d.tracking != 0.0 {
                // Tracking is in thousandths of an em.
                set(out, Prop::LetterSpacing, round4(d.tracking * size / 1000.0));
            }
            out.props.insert(
                Prop::Fill,
                Value::Color(d.fill.map(rgba).unwrap_or(Color::TRANSPARENT)),
            );
            if let Some((c, w)) = d.stroke {
                out.props.insert(Prop::Stroke, Value::Color(rgba(c)));
                set(out, Prop::StrokeWidth, round4(w));
            }
        }

        // ------------------------------------------------------ image --

        /// The `src` of a footage picture: staged from the archive, or ""
        /// (the placeholder) when the file is not there.
        fn image(&mut self, fid: u32, f: &Footage, notes: &mut Vec<Note>) -> String {
            let file = match &f.source {
                Source::File(p) => p.rsplit(['/', '\\']).next().unwrap_or(p).to_string(),
                _ => f.name.clone(),
            };
            if let Some(src) = self.staged.get(&fid) {
                if src.is_empty() {
                    notes.push(
                        Note::new("Missing footage {file}", Status::Partial).with("file", &file),
                    );
                }
                return src.clone();
            }
            let ext = ext_of(&file);
            let bytes = match (&f.source, self.media) {
                (Source::File(p), Some(m)) => m(p),
                _ => None,
            };
            let src = match bytes {
                Some(b) if is_picture(&ext) => {
                    let name = self.unique_file(&file);
                    self.files.push((name.clone(), b));
                    format!("{}{name}", self.ctx.prefix)
                }
                Some(_) => {
                    notes.push(
                        Note::new("Unsupported picture format {format}", Status::Partial)
                            .with("format", &ext),
                    );
                    String::new()
                }
                None => {
                    if !self.missing.contains(&file) {
                        self.missing.push(file.clone());
                    }
                    notes.push(
                        Note::new("Missing footage {file}", Status::Partial).with("file", &file),
                    );
                    String::new()
                }
            };
            self.staged.insert(fid, src.clone());
            src
        }

        fn unique_file(&self, file: &str) -> String {
            let (stem, ext) = file.rsplit_once('.').unwrap_or((file, "png"));
            let clean = |s: &str| -> String {
                let c: String = s
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                            c
                        } else {
                            '-'
                        }
                    })
                    .collect();
                c.trim_matches('-').to_string()
            };
            let stem = match clean(stem) {
                s if s.is_empty() => "image".to_string(),
                s => s,
            };
            let ext = clean(ext).to_lowercase();
            let mut name = format!("{stem}.{ext}");
            let mut n = 2;
            while self.files.iter().any(|(f, _)| *f == name) {
                name = format!("{stem}-{n}.{ext}");
                n += 1;
            }
            name
        }

        // ---------------------------------------------------- precomp --

        fn precomp(
            &mut self,
            cx: &mut Cx,
            l: &LayerRec,
            pc: &'a CompItem,
            id: &str,
            time: Time,
            notes: &mut Vec<Note>,
        ) -> Vec<Out> {
            if cx.stack.contains(&pc.id) || cx.stack.len() > 10 {
                notes.push(Note::new(
                    "Precomposition nested too deeply",
                    Status::Partial,
                ));
                return Vec::new();
            }
            let inner = Time {
                scale: time.scale * l.stretch,
                offset: time.root(l.start),
            };
            cx.stack.push(pc.id);
            let outs = self.layers(cx, pc, Some(id), (0.0, 0.0), inner);
            cx.stack.pop();
            outs
        }

        // ------------------------------------------------------ shape --

        /// A shape layer: one rectangle / ellipse / path as the layer
        /// itself, or a group of them.
        #[allow(clippy::too_many_arguments)]
        fn shape(
            &mut self,
            cx: &Cx,
            l: &LayerRec,
            id: &str,
            time: Time,
            notes: &mut Vec<Note>,
            anims: &mut Anims,
        ) -> (LayerKind, Bx, Vec<Out>, Option<Prim>) {
            let mut prims = Vec::new();
            if let Some(root) = l.props.group("ADBE Root Vectors Group") {
                let mut w = Walk {
                    notes,
                    anims,
                    prims: &mut prims,
                };
                w.items(root, (0.0, 0.0), (1.0, 1.0), 1.0, None, None, None, 0);
            }
            if prims.is_empty() {
                return (
                    group(false),
                    Bx::new(0.0, 0.0, 100.0, 100.0),
                    Vec::new(),
                    None,
                );
            }
            let b = prims
                .iter()
                .map(Prim::bounds)
                .reduce(Bx::union)
                .unwrap_or(Bx::new(0.0, 0.0, 100.0, 100.0));
            if prims.len() == 1 {
                let p = prims.remove(0);
                return (p.kind(b), b, Vec::new(), Some(p));
            }
            let mut parts = Vec::new();
            for (i, p) in prims.iter().enumerate().rev() {
                let pb = p.bounds();
                let mut c = mk(&new_id(), &p.name, Some(id), p.kind(pb));
                set(&mut c, Prop::X, round4(pb.x - b.x));
                set(&mut c, Prop::Y, round4(pb.y - b.y));
                set(&mut c, Prop::Width, round4(pb.w.max(1.0)));
                set(&mut c, Prop::Height, round4(pb.h.max(1.0)));
                set(&mut c, Prop::AnchorX, 0.0);
                set(&mut c, Prop::AnchorY, 0.0);
                self.paint(cx, &mut c, p, pb, l, time, anims);
                parts.push(Out {
                    layer: c,
                    report: None,
                    kids: Vec::new(),
                    order: i,
                });
            }
            (group(false), b, parts, None)
        }

        /// Fill, stroke, corner radius and animated size of a part.
        #[allow(clippy::too_many_arguments)]
        fn paint(
            &self,
            cx: &Cx,
            out: &mut Layer,
            p: &Prim,
            b: Bx,
            l: &LayerRec,
            time: Time,
            anims: &mut Anims,
        ) {
            let colour = |cx: &Cx,
                          out: &mut Layer,
                          prop: Prop,
                          s: &Stream,
                          opacity: f64,
                          anims: &mut Anims| {
                if s.animated() {
                    let (track, q) = color_track(cx, s, time, l, opacity);
                    out.tracks.insert(prop, track);
                    merge(anims, "Colour", q);
                } else {
                    let c = s.first();
                    let c = [
                        c.first().copied().unwrap_or(1.0),
                        c.get(1).copied().unwrap_or(1.0),
                        c.get(2).copied().unwrap_or(1.0),
                        c.get(3).copied().unwrap_or(1.0) * opacity,
                    ];
                    out.props.insert(prop, Value::Color(rgba(c)));
                }
            };
            match &p.fill {
                Some(f) => colour(cx, out, Prop::Fill, &f.color, f.opacity, anims),
                None => {
                    out.props
                        .insert(Prop::Fill, Value::Color(Color::TRANSPARENT));
                }
            }
            if let Some(s) = &p.stroke {
                colour(cx, out, Prop::Stroke, &s.color, s.opacity, anims);
                let ws = &s.width;
                apply(
                    cx,
                    out,
                    Prop::StrokeWidth,
                    Some(ws),
                    0,
                    time,
                    l,
                    |v| v * p.scale,
                    Some(2.0 * p.scale),
                    "Stroke width",
                    anims,
                );
            }
            match &p.geo {
                Geo::Rect { round, size, .. } => {
                    let r = round.min(size.at(0).unwrap_or(0.0).abs() * p.sc.0 / 2.0);
                    if r > 0.0 {
                        set(out, Prop::CornerRadius, round4(r * p.scale));
                    }
                }
                Geo::Pen { animated: true, .. } => {
                    merge(anims, "Path", (Status::Partial, Some("first shape kept")));
                }
                _ => {}
            }
            if let (Geo::Pen { .. }, Some((start, end, offset))) = (&p.geo, &p.trim) {
                apply(
                    cx,
                    out,
                    Prop::DrawStart,
                    Some(start),
                    0,
                    time,
                    l,
                    |v| v,
                    Some(0.0),
                    "Trim paths",
                    anims,
                );
                apply(
                    cx,
                    out,
                    Prop::DrawEnd,
                    Some(end),
                    0,
                    time,
                    l,
                    |v| v,
                    Some(100.0),
                    "Trim paths",
                    anims,
                );
                if *offset != 0.0 {
                    merge(
                        anims,
                        "Trim paths",
                        (Status::Partial, Some("offset left out")),
                    );
                }
            }
            // Animated rectangle / ellipse size: width / height keyframes,
            // growing from the centre like in After Effects.
            if let Geo::Rect { size, center, .. } | Geo::Ellipse { size, center } = &p.geo {
                if size.animated() {
                    let base_x = out
                        .props
                        .get(&Prop::X)
                        .and_then(|v| v.as_number())
                        .unwrap_or(0.0);
                    let base_y = out
                        .props
                        .get(&Prop::Y)
                        .and_then(|v| v.as_number())
                        .unwrap_or(0.0);
                    let (cx0, cy0) = (center.0 - b.x + base_x, center.1 - b.y + base_y);
                    let (sx, sy) = p.sc;
                    let fixed_pos = !out.tracks.contains_key(&Prop::X);
                    apply(
                        cx,
                        out,
                        Prop::Width,
                        Some(size),
                        0,
                        time,
                        l,
                        |v| (v * sx).abs(),
                        None,
                        "Size",
                        anims,
                    );
                    apply(
                        cx,
                        out,
                        Prop::Height,
                        Some(size),
                        1,
                        time,
                        l,
                        |v| (v * sy).abs(),
                        None,
                        "Size",
                        anims,
                    );
                    if fixed_pos {
                        apply(
                            cx,
                            out,
                            Prop::X,
                            Some(size),
                            0,
                            time,
                            l,
                            |v| cx0 - (v * sx).abs() / 2.0,
                            None,
                            "Size",
                            anims,
                        );
                        apply(
                            cx,
                            out,
                            Prop::Y,
                            Some(size),
                            1,
                            time,
                            l,
                            |v| cy0 - (v * sy).abs() / 2.0,
                            None,
                            "Size",
                            anims,
                        );
                    }
                }
            }
        }
    }

    // ---------------------------------------------------------- shapes ---

    /// A fill or stroke.
    #[derive(Clone)]
    struct Paint {
        color: Stream,
        /// 0..1, the paint's and its groups' opacity.
        opacity: f64,
        /// Stroke width.
        width: Stream,
    }

    enum Geo {
        Rect {
            center: (f64, f64),
            size: Stream,
            round: f64,
        },
        Ellipse {
            center: (f64, f64),
            size: Stream,
        },
        /// In layer space.
        Pen {
            shape: Shape,
            animated: bool,
        },
    }

    /// A rectangle, ellipse or path found in a shape layer, with what
    /// paints it.
    struct Prim {
        name: String,
        geo: Geo,
        /// Scale of the enclosing groups.
        sc: (f64, f64),
        /// Uniform part of it (stroke width, corner radius).
        scale: f64,
        fill: Option<Paint>,
        stroke: Option<Paint>,
        /// Trim paths: start, end (%), offset (°).
        trim: Option<(Stream, Stream, f64)>,
    }

    impl Prim {
        fn bounds(&self) -> Bx {
            match &self.geo {
                Geo::Rect { center, size, .. } | Geo::Ellipse { center, size } => {
                    let w = (size.at(0).unwrap_or(100.0) * self.sc.0).abs();
                    let h = (size.at(1).unwrap_or(100.0) * self.sc.1).abs();
                    Bx::new(center.0 - w / 2.0, center.1 - h / 2.0, w, h)
                }
                Geo::Pen { shape, .. } => shape_bounds(shape),
            }
        }

        fn kind(&self, b: Bx) -> LayerKind {
            match &self.geo {
                Geo::Rect { .. } => {
                    let mut x = cg_core::wid::quad::Config::default();
                    x.stroke_style.position = StrokePosition::Center;
                    LayerKind::Rect { x }
                }
                Geo::Ellipse { .. } => {
                    let mut x = cg_core::wid::ellipse::Config::default();
                    x.stroke_style.position = StrokePosition::Center;
                    LayerKind::Ellipse { x }
                }
                Geo::Pen { shape, .. } => {
                    let w = b.w.max(1e-6);
                    let h = b.h.max(1e-6);
                    let n = shape.verts.len();
                    let points = (0..n)
                        .map(|i| {
                            let v = shape.verts[i];
                            let (o, ip) = (shape.out_pt[i], shape.in_pt[i]);
                            let curve = (o[0] - v[0]).abs()
                                + (o[1] - v[1]).abs()
                                + (ip[0] - v[0]).abs()
                                + (ip[1] - v[1]).abs()
                                > 1e-6;
                            PenPoint {
                                x: ((v[0] - b.x) / w) as f32,
                                y: ((v[1] - b.y) / h) as f32,
                                ix: ((ip[0] - v[0]) / w) as f32,
                                iy: ((ip[1] - v[1]) / h) as f32,
                                ox: ((o[0] - v[0]) / w) as f32,
                                oy: ((o[1] - v[1]) / h) as f32,
                                kind: if curve {
                                    PointType::Curve
                                } else {
                                    PointType::Corner
                                },
                                mode: HandleMode::Broken,
                            }
                        })
                        .collect();
                    let mut x = cg_core::wid::pen::Config {
                        points,
                        closed: shape.closed,
                        ..Default::default()
                    };
                    x.ref_w = w as f32;
                    x.ref_h = h as f32;
                    x.arrow_start = Arrow::None;
                    x.arrow_end = Arrow::None;
                    x.fill_on = self.fill.is_some() && shape.closed;
                    x.draw_in = false;
                    x.v_start = 0.0;
                    x.v_end = 100.0;
                    x.stroke_style.cap = Default::default();
                    x.stroke_style.join = Default::default();
                    LayerKind::Pen { x }
                }
            }
        }
    }

    fn shape_bounds(s: &Shape) -> Bx {
        let pts = s.verts.iter().chain(&s.in_pt).chain(&s.out_pt);
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in pts {
            x0 = x0.min(p[0]);
            y0 = y0.min(p[1]);
            x1 = x1.max(p[0]);
            y1 = y1.max(p[1]);
        }
        if x0 > x1 {
            return Bx::new(0.0, 0.0, 1.0, 1.0);
        }
        Bx::new(x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0))
    }

    /// Walks the contents of a shape layer.
    struct Walk<'w> {
        notes: &'w mut Vec<Note>,
        anims: &'w mut Anims,
        prims: &'w mut Vec<Prim>,
    }

    impl Walk<'_> {
        #[allow(clippy::too_many_arguments)]
        fn items(
            &mut self,
            items: &Group,
            off: (f64, f64),
            sc: (f64, f64),
            opacity: f64,
            fill: Option<&Paint>,
            stroke: Option<&Paint>,
            trim: Option<&(Stream, Stream, f64)>,
            depth: usize,
        ) {
            if depth > 32 {
                return;
            }
            let on = |n: &&(String, Node)| match &n.1 {
                Node::Group(g) => g.enabled,
                _ => true,
            };
            let grp = |mn: &str| {
                items
                    .kids
                    .iter()
                    .filter(on)
                    .find(|(m, _)| m == mn)
                    .and_then(|(_, n)| match n {
                        Node::Group(g) => Some(g),
                        _ => None,
                    })
            };
            let s_or = |g: &Group, mn: &str, d: Vec<f64>| -> Stream {
                g.stream(mn).cloned().unwrap_or(Stream {
                    dims: d.len(),
                    value: d,
                    enabled: true,
                    ..Default::default()
                })
            };
            let num = |g: &Group, mn: &str, d: f64| g.stream(mn).and_then(|s| s.at(0)).unwrap_or(d);
            let mut fill = grp("ADBE Vector Graphic - Fill")
                .map(|g| Paint {
                    color: s_or(g, "ADBE Vector Fill Color", vec![1.0, 0.0, 0.0, 1.0]),
                    opacity: num(g, "ADBE Vector Fill Opacity", 100.0) / 100.0 * opacity,
                    width: Stream::default(),
                })
                .or_else(|| fill.cloned());
            if let Some(g) = grp("ADBE Vector Graphic - G-Fill") {
                self.notes.push(Note::new("Gradient", Status::Partial));
                if fill.is_none() {
                    fill = Some(Paint {
                        color: gradient_colour(g),
                        opacity: num(g, "ADBE Vector Fill Opacity", 100.0) / 100.0 * opacity,
                        width: Stream::default(),
                    });
                }
            }
            let mut stroke = grp("ADBE Vector Graphic - Stroke")
                .map(|g| Paint {
                    color: s_or(g, "ADBE Vector Stroke Color", vec![1.0, 1.0, 1.0, 1.0]),
                    opacity: num(g, "ADBE Vector Stroke Opacity", 100.0) / 100.0 * opacity,
                    width: s_or(g, "ADBE Vector Stroke Width", vec![2.0]),
                })
                .or_else(|| stroke.cloned());
            if let Some(g) = grp("ADBE Vector Graphic - G-Stroke") {
                self.notes.push(Note::new("Gradient", Status::Partial));
                if stroke.is_none() {
                    stroke = Some(Paint {
                        color: gradient_colour(g),
                        opacity: num(g, "ADBE Vector Stroke Opacity", 100.0) / 100.0 * opacity,
                        width: s_or(g, "ADBE Vector Stroke Width", vec![2.0]),
                    });
                }
            }
            let trim = grp("ADBE Vector Filter - Trim")
                .map(|g| {
                    (
                        s_or(g, "ADBE Vector Trim Start", vec![0.0]),
                        s_or(g, "ADBE Vector Trim End", vec![100.0]),
                        num(g, "ADBE Vector Trim Offset", 0.0),
                    )
                })
                .or_else(|| trim.cloned());
            for (mn, n) in items.kids.iter().filter(on) {
                let Node::Group(g) = n else { continue };
                let label = g
                    .name
                    .clone()
                    .unwrap_or_else(|| mn.replace("ADBE Vector ", ""));
                match mn.as_str() {
                    "ADBE Vector Group" => {
                        let (o2, s2, op2) = self.group_transform(g, off, sc, opacity);
                        if let Some(c) = g.group("ADBE Vectors Group") {
                            self.items(
                                c,
                                o2,
                                s2,
                                op2,
                                fill.as_ref(),
                                stroke.as_ref(),
                                trim.as_ref(),
                                depth + 1,
                            );
                        }
                    }
                    "ADBE Vector Shape - Rect" | "ADBE Vector Shape - Ellipse" => {
                        let rect = mn.ends_with("Rect");
                        let (sz, ps) = if rect {
                            ("ADBE Vector Rect Size", "ADBE Vector Rect Position")
                        } else {
                            ("ADBE Vector Ellipse Size", "ADBE Vector Ellipse Position")
                        };
                        let size = s_or(g, sz, vec![100.0, 100.0]);
                        let p = s_or(g, ps, vec![0.0, 0.0]);
                        if p.animated() {
                            merge(
                                self.anims,
                                "Position",
                                (Status::Partial, Some("first value kept")),
                            );
                        }
                        let center = (
                            off.0 + p.at(0).unwrap_or(0.0) * sc.0,
                            off.1 + p.at(1).unwrap_or(0.0) * sc.1,
                        );
                        let geo = if rect {
                            Geo::Rect {
                                center,
                                size,
                                round: num(g, "ADBE Vector Rect Roundness", 0.0),
                            }
                        } else {
                            Geo::Ellipse { center, size }
                        };
                        self.push(label, geo, sc, &fill, &stroke, &trim);
                    }
                    "ADBE Vector Shape - Group" => {
                        let Some(Node::Path(path)) = g.get("ADBE Vector Shape") else {
                            continue;
                        };
                        let Some(first) = path.shapes.first() else {
                            continue;
                        };
                        let tf = |p: &[f64; 2]| [off.0 + p[0] * sc.0, off.1 + p[1] * sc.1];
                        let shape = Shape {
                            closed: first.closed,
                            verts: first.verts.iter().map(tf).collect(),
                            out_pt: first.out_pt.iter().map(tf).collect(),
                            in_pt: first.in_pt.iter().map(tf).collect(),
                        };
                        if shape.verts.len() < 2 {
                            continue;
                        }
                        let geo = Geo::Pen {
                            shape,
                            animated: path.shapes.len() > 1,
                        };
                        self.push(label, geo, sc, &fill, &stroke, &trim);
                    }
                    "ADBE Vector Shape - Star" => {
                        let shape = star(g, off, sc);
                        if num(g, "ADBE Vector Star Inner Roundess", 0.0) != 0.0
                            || num(g, "ADBE Vector Star Outer Roundess", 0.0) != 0.0
                        {
                            self.notes
                                .push(Note::new("Star roundness", Status::Partial));
                        }
                        if g.kids
                            .iter()
                            .any(|(_, n)| matches!(n, Node::Stream(s) if s.animated()))
                        {
                            merge(
                                self.anims,
                                "Path",
                                (Status::Partial, Some("first shape kept")),
                            );
                        }
                        let geo = Geo::Pen {
                            shape,
                            animated: false,
                        };
                        self.push(label, geo, sc, &fill, &stroke, &trim);
                    }
                    "ADBE Vector Filter - Repeater" => {
                        self.notes.push(Note::new("Repeater", Status::Partial));
                    }
                    "ADBE Vector Filter - Merge" => {
                        self.notes.push(Note::new("Merge paths", Status::Partial));
                    }
                    "ADBE Vector Filter - Trim" => {}
                    m if m.starts_with("ADBE Vector Filter") => {
                        self.notes.push(
                            Note::new("Shape modifier {name}", Status::Partial).with("name", label),
                        );
                    }
                    _ => {}
                }
            }
        }

        /// Offset, scale and opacity of a shape group.
        fn group_transform(
            &mut self,
            g: &Group,
            off: (f64, f64),
            sc: (f64, f64),
            opacity: f64,
        ) -> ((f64, f64), (f64, f64), f64) {
            let Some(tr) = g.group("ADBE Vector Transform Group") else {
                return (off, sc, opacity);
            };
            let v = |mn: &str, i: usize, d: f64| tr.stream(mn).and_then(|s| s.at(i)).unwrap_or(d);
            if tr
                .kids
                .iter()
                .any(|(_, n)| matches!(n, Node::Stream(s) if s.animated()))
            {
                merge(
                    self.anims,
                    "Shape group transform",
                    (Status::Partial, Some("first value kept")),
                );
            }
            if v("ADBE Vector Rotation", 0, 0.0) != 0.0 || v("ADBE Vector Skew", 0, 0.0) != 0.0 {
                self.notes
                    .push(Note::new("Shape group rotation", Status::Partial));
            }
            let (px, py) = (
                v("ADBE Vector Position", 0, 0.0),
                v("ADBE Vector Position", 1, 0.0),
            );
            let (ax, ay) = (
                v("ADBE Vector Anchor", 0, 0.0),
                v("ADBE Vector Anchor", 1, 0.0),
            );
            let (sx, sy) = (
                v("ADBE Vector Scale", 0, 100.0) / 100.0,
                v("ADBE Vector Scale", 1, 100.0) / 100.0,
            );
            (
                (off.0 + (px - ax * sx) * sc.0, off.1 + (py - ay * sy) * sc.1),
                (sc.0 * sx, sc.1 * sy),
                opacity * v("ADBE Vector Group Opacity", 0, 100.0) / 100.0,
            )
        }

        fn push(
            &mut self,
            name: String,
            geo: Geo,
            sc: (f64, f64),
            fill: &Option<Paint>,
            stroke: &Option<Paint>,
            trim: &Option<(Stream, Stream, f64)>,
        ) {
            if fill.is_none() && stroke.is_none() {
                return;
            }
            let pen = matches!(geo, Geo::Pen { .. });
            if trim.is_some() && !pen {
                self.notes.push(Note::new("Trim paths", Status::Partial));
            }
            self.prims.push(Prim {
                name,
                geo,
                sc,
                scale: (sc.0.abs() * sc.1.abs()).sqrt(),
                fill: fill.clone(),
                stroke: stroke.clone(),
                trim: if pen { trim.clone() } else { None },
            });
        }
    }

    /// The points of a star or polygon (first value of its settings).
    fn star(g: &Group, off: (f64, f64), sc: (f64, f64)) -> Shape {
        let num = |mn: &str, d: f64| g.stream(mn).and_then(|s| s.at(0)).unwrap_or(d);
        let polygon = num("ADBE Vector Star Type", 1.0) == 2.0;
        let n = num("ADBE Vector Star Points", 5.0)
            .round()
            .clamp(3.0, 100.0) as usize;
        let (px, py) = g
            .stream("ADBE Vector Star Position")
            .map(|s| (s.at(0).unwrap_or(0.0), s.at(1).unwrap_or(0.0)))
            .unwrap_or((0.0, 0.0));
        let rot = num("ADBE Vector Star Rotation", 0.0);
        let outer = num("ADBE Vector Star Outer Radius", 100.0);
        let inner = num("ADBE Vector Star Inner Radius", 50.0);
        let count = if polygon { n } else { 2 * n };
        let mut s = Shape {
            closed: true,
            ..Default::default()
        };
        for i in 0..count {
            let r = if polygon || i % 2 == 0 { outer } else { inner };
            let a = (rot - 90.0 + 360.0 * i as f64 / count as f64).to_radians();
            let p = [
                off.0 + (px + r * a.cos()) * sc.0,
                off.1 + (py + r * a.sin()) * sc.1,
            ];
            s.verts.push(p);
            s.in_pt.push(p);
            s.out_pt.push(p);
        }
        s
    }

    /// The first colour stop of a gradient, as a static colour.
    fn gradient_colour(g: &Group) -> Stream {
        let xml = match g.get("ADBE Vector Grad Colors") {
            Some(Node::Gradient(x)) => x.first().cloned(),
            _ => None,
        };
        let c = xml
            .and_then(|x| {
                let at = x.find("Stops Color")?;
                let rest = &x[at..];
                let v: Vec<f64> = rest
                    .split("<float>")
                    .skip(1)
                    .take(5)
                    .filter_map(|s| s.split('<').next()?.trim().parse().ok())
                    .collect();
                (v.len() == 5).then(|| vec![v[2], v[3], v[4], 1.0])
            })
            .unwrap_or(vec![1.0, 1.0, 1.0, 1.0]);
        Stream {
            dims: 4,
            color: true,
            value: c,
            enabled: true,
            ..Default::default()
        }
    }

    // --------------------------------------------------------- tracks ---

    /// Writes a static value or keyframes of one dimension of a stream.
    /// `default`: the value when the stream is missing (written for X / Y);
    /// a static value equal to the property's neutral value is left out.
    #[allow(clippy::too_many_arguments)]
    fn apply(
        cx: &Cx,
        out: &mut Layer,
        prop: Prop,
        s: Option<&Stream>,
        dim: usize,
        time: Time,
        l: &LayerRec,
        f: impl Fn(f64) -> f64,
        default: Option<f64>,
        label: &'static str,
        anims: &mut Anims,
    ) {
        let neutral = match prop {
            Prop::ScaleX | Prop::ScaleY | Prop::Opacity => Some(1.0),
            Prop::Rotation => Some(0.0),
            _ => None,
        };
        match s {
            Some(s) if s.animated() && s.keys.iter().all(|k| k.v.len() > dim) => {
                let (track, q) = number_track(cx, s, dim, time, l, &f);
                out.props.remove(&prop);
                out.tracks.insert(prop, track);
                merge(anims, label, q);
            }
            Some(s) if s.first().len() > dim => {
                let v = round4(f(s.first()[dim]));
                if neutral.is_none_or(|n| (v - n).abs() > 1e-6) {
                    out.props.insert(prop, Value::Number(v));
                }
            }
            _ => {
                if let Some(d) = default {
                    out.props.insert(prop, Value::Number(round4(d)));
                }
            }
        }
    }

    /// Worst of two qualities of an animated property.
    fn merge(anims: &mut Anims, label: &'static str, q: (Status, Option<&'static str>)) {
        let e = anims.entry(label).or_insert((Status::Ok, None));
        if q.0 == Status::Partial {
            *e = q;
        }
    }

    type Quality = (Status, Option<&'static str>);
    const EXACT: Quality = (Status::Ok, None);

    fn number_track(
        cx: &Cx,
        s: &Stream,
        dim: usize,
        time: Time,
        l: &LayerRec,
        f: &impl Fn(f64) -> f64,
    ) -> (Vec<Keyframe>, Quality) {
        let mut q = EXACT;
        let mut track: Vec<Keyframe> = Vec::new();
        for i in 0..s.keys.len() {
            let k = &s.keys[i];
            let easing = if i + 1 < s.keys.len() {
                let (e, eq) = ease(s, i, Some(dim));
                if eq.0 == Status::Partial {
                    q = eq;
                }
                e
            } else {
                Easing::Linear
            };
            track.push(Keyframe {
                frame: cx.frame(time, l, k.t),
                value: Value::Number(round4(f(k.v[dim]))),
                easing,
            });
        }
        if l.stretch < 0.0 {
            q = (Status::Partial, Some("time reversed"));
            track.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        }
        (track, q)
    }

    fn color_track(
        cx: &Cx,
        s: &Stream,
        time: Time,
        l: &LayerRec,
        opacity: f64,
    ) -> (Vec<Keyframe>, Quality) {
        let mut q = EXACT;
        let mut track = Vec::new();
        for i in 0..s.keys.len() {
            let k = &s.keys[i];
            let easing = if i + 1 < s.keys.len() {
                let (e, eq) = ease(s, i, None);
                if eq.0 == Status::Partial {
                    q = eq;
                }
                e
            } else {
                Easing::Linear
            };
            let c = |j: usize| k.v.get(j).copied().unwrap_or(1.0);
            track.push(Keyframe {
                frame: cx.frame(time, l, k.t),
                value: Value::Color(rgba([c(0), c(1), c(2), c(3) * opacity])),
                easing,
            });
        }
        track.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        (track, q)
    }

    /// The easing of the segment leaving key `i`. After Effects' temporal
    /// ease (speed + influence on each side) is a cubic bezier in (time,
    /// value): its control points `(dt·io, dt·io·so)` and `(dt - dt·ii,
    /// D - dt·ii·si)` normalised by the segment's duration `dt` and
    /// change `D` give the CSS-like curve exactly. `dim`: the dimension of
    /// a stream eased per dimension; None (or a single-ease stream): the
    /// whole value, `D` being the distance travelled (along a straight
    /// line: curved motion paths are linearised).
    fn ease(s: &Stream, i: usize, dim: Option<usize>) -> (Easing, Quality) {
        let (a, b) = (&s.keys[i], &s.keys[i + 1]);
        let dt = b.t - a.t;
        if dt.abs() < 1e-9 {
            return (Easing::Linear, EXACT);
        }
        if a.out_type == 3 || b.in_type == 3 {
            return (Easing::Hold, EXACT);
        }
        let single = s.single_ease || dim.is_none();
        let scale = if s.color { 255.0 } else { 1.0 };
        let delta = |p: &Key, q: &Key| -> f64 {
            if single {
                p.v.iter()
                    .zip(&q.v)
                    .map(|(x, y)| ((y - x) * scale).powi(2))
                    .sum::<f64>()
                    .sqrt()
            } else {
                let d = dim.unwrap_or(0);
                q.v.get(d).copied().unwrap_or(0.0) - p.v.get(d).copied().unwrap_or(0.0)
            }
        };
        let d = delta(a, b);
        let mut q = EXACT;
        if single && s.spatial && curved(s, i) {
            q = (Status::Partial, Some("motion path linearized"));
        }
        if a.out_type == 1 && b.in_type == 1 {
            return (Easing::Linear, q);
        }
        let side = |k: &Key, out: bool| -> (f64, f64) {
            let e = if out { &k.out_ease } else { &k.in_ease };
            let j = if single { 0 } else { dim.unwrap_or(0) };
            e.get(j).copied().unwrap_or((0.0, LINEAR_INFLUENCE))
        };
        // Auto-bezier: the slope between the neighbouring keys.
        let auto = |j: usize| -> f64 {
            let prev = &s.keys[j.saturating_sub(1)];
            let next = &s.keys[(j + 1).min(s.keys.len() - 1)];
            let span = next.t - prev.t;
            if span.abs() < 1e-9 {
                0.0
            } else {
                delta(prev, next) / span
            }
        };
        let (so, io) = if a.out_type == 1 {
            (d / dt, LINEAR_INFLUENCE)
        } else if a.auto {
            (auto(i), LINEAR_INFLUENCE)
        } else {
            side(a, true)
        };
        let (si, ii) = if b.in_type == 1 {
            (d / dt, LINEAR_INFLUENCE)
        } else if b.auto {
            (auto(i + 1), LINEAR_INFLUENCE)
        } else {
            side(b, false)
        };
        if d.abs() < 1e-9 {
            // No change: flat whatever the curve, unless the speeds make it
            // overshoot and come back.
            if so.abs() > 1e-6 || si.abs() > 1e-6 {
                q = (Status::Partial, Some("curve simplified"));
            }
            return (Easing::Linear, q);
        }
        let x1 = io.clamp(0.0, 1.0);
        let x2 = 1.0 - ii.clamp(0.0, 1.0);
        let y1 = so * io * dt / d;
        let y2 = 1.0 - si * ii * dt / d;
        if (x1 - y1).abs() < 1e-6 && (x2 - y2).abs() < 1e-6 {
            return (Easing::Linear, q);
        }
        (
            Easing::Bezier {
                x1: round4(x1),
                y1: round4(y1),
                x2: round4(x2),
                y2: round4(y2),
            },
            q,
        )
    }

    /// The motion path between key `i` and the next one bends.
    fn curved(s: &Stream, i: usize) -> bool {
        let (a, b) = (&s.keys[i], &s.keys[i + 1]);
        let nonzero = |t: &[f64]| t.iter().any(|v| v.abs() > 1e-6);
        let tangents =
            (!a.spatial_auto && nonzero(&a.out_tan)) || (!b.spatial_auto && nonzero(&b.in_tan));
        // Auto-bezier keys bend unless their neighbours are aligned.
        let bend = |j: usize| {
            let k = &s.keys[j];
            if !k.spatial_auto || j == 0 || j + 1 >= s.keys.len() {
                return false;
            }
            let (p, n) = (&s.keys[j - 1].v, &s.keys[j + 1].v);
            let g = |v: &Vec<f64>, d: usize| v.get(d).copied().unwrap_or(0.0);
            let (ux, uy) = (g(&k.v, 0) - g(p, 0), g(&k.v, 1) - g(p, 1));
            let (vx, vy) = (g(n, 0) - g(&k.v, 0), g(n, 1) - g(&k.v, 1));
            (ux * vy - uy * vx).abs() > 1e-3 * (ux.hypot(uy) * vx.hypot(vy)).max(1e-9)
        };
        tangents || bend(i) || bend(i + 1)
    }

    // -------------------------------------------------------- markers ---

    /// Composition markers. The on-air point (the one `Out` marker) is a
    /// marker named like « on air », « onair », « out », « hold » or
    /// « IN/OUT »; the other markers become pauses. Without such a marker,
    /// the on-air point is the end of the first block of keyframes (the
    /// « in » animation): the spans between keyframes where a property
    /// changes (in / out points and hold keyframes left out) are merged
    /// when they overlap or are less than half a second apart, and the
    /// first merged span ends at the on-air point; a composition without
    /// animation is on air from its first frame.
    pub(super) fn markers(c: &CompItem, t: &Template) -> Vec<Marker> {
        let mut out = Vec::new();
        let mut on_air = false;
        for m in &c.markers {
            let frame = (m.t * c.fps).round().clamp(0.0, t.duration);
            let name = m.comment.trim();
            let n: String = name
                .to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '/')
                .collect();
            let is_air = matches!(
                n.as_str(),
                "onair" | "out" | "hold" | "in/out" | "inout" | "holdout" | "takeout"
            ) || n.starts_with("onair");
            let kind = if is_air && !on_air {
                on_air = true;
                MarkerKind::Out
            } else {
                MarkerKind::Pause
            };
            out.push(Marker {
                id: new_id(),
                frame,
                name: if name.is_empty() {
                    "Marker".into()
                } else {
                    name.to_string()
                },
                kind,
                resume: Resume::Manual,
            });
        }
        if !on_air {
            // Spans where a property changes (in / out point holds and
            // hold keyframes left out), merged when they touch.
            let mut spans: Vec<(f64, f64)> = t
                .layers
                .iter()
                .flat_map(|l| l.tracks.values())
                .flat_map(|k| k.windows(2))
                .filter(|w| w[0].easing != Easing::Hold && w[0].value != w[1].value)
                .map(|w| (w[0].frame.max(0.0), w[1].frame.min(t.duration)))
                .filter(|(a, b)| b > a)
                .collect();
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            let gap = (c.fps / 2.0).max(1.0);
            let mut end = 0.0;
            if let Some(&(_, first)) = spans.first() {
                end = first;
                for &(a, b) in &spans[1..] {
                    if a > end + gap {
                        break;
                    }
                    end = end.max(b);
                }
            }
            out.push(Marker {
                id: new_id(),
                frame: end.round(),
                name: "On air".into(),
                kind: MarkerKind::Out,
                resume: Resume::Manual,
            });
        }
        out.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        out
    }

    // -------------------------------------------------------- helpers ---

    fn used_comps(p: &Project) -> HashSet<u32> {
        let ids: HashSet<u32> = p.comps.iter().map(|c| c.id).collect();
        p.comps
            .iter()
            .flat_map(|c| c.layers.iter().map(|l| l.source))
            .filter(|s| ids.contains(s))
            .collect()
    }

    /// Top-level compositions start selected; precompositions used by
    /// others, compositions named « _… » and empty ones start off.
    fn selected(c: &CompItem, used: &HashSet<u32>) -> bool {
        !used.contains(&c.id) && !c.name.starts_with('_') && !c.layers.is_empty()
    }

    fn paint_order(o: &Out, out: &mut Vec<Layer>) {
        out.push(o.layer.clone());
        for k in o.kids.iter().rev() {
            paint_order(k, out);
        }
    }

    fn report_order(o: Out, depth: u32, out: &mut Vec<LayerReport>) {
        let d = match o.report {
            Some(mut r) => {
                r.depth = depth;
                out.push(r);
                depth + 1
            }
            None => depth,
        };
        for k in o.kids {
            report_order(k, d, out);
        }
    }

    fn mk(id: &str, name: &str, parent: Option<&str>, kind: LayerKind) -> Layer {
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

    fn group(clip: bool) -> LayerKind {
        LayerKind::Group { clip, lottie: None }
    }

    fn set(l: &mut Layer, p: Prop, v: f64) {
        l.props.insert(p, Value::Number(v));
    }

    fn rgba(c: [f64; 4]) -> Color {
        Color(c.map(|v| v.clamp(0.0, 1.0) as f32))
    }

    pub(super) fn worst(notes: &[Note]) -> Status {
        if notes.iter().any(|n| n.status != Status::Ok) {
            Status::Partial
        } else {
            Status::Ok
        }
    }

    /// Properties with an expression (display names), outside effects.
    fn expressions(g: &Group, out: &mut Vec<String>, depth: usize) {
        if depth > 32 {
            return;
        }
        for (mn, n) in &g.kids {
            match n {
                Node::Stream(s) if s.expression.is_some() => {
                    out.push(s.name.clone().unwrap_or_else(|| prop_label(mn)));
                }
                Node::Group(g) if mn != "ADBE Effect Parade" => expressions(g, out, depth + 1),
                Node::Mask { group, .. } => expressions(group, out, depth + 1),
                _ => {}
            }
        }
    }

    /// A readable name for a match name.
    fn prop_label(mn: &str) -> String {
        match mn {
            "ADBE Anchor Point" => "Anchor point".into(),
            "ADBE Position" | "ADBE Position_0" | "ADBE Position_1" => "Position".into(),
            "ADBE Scale" => "Scale".into(),
            "ADBE Rotate Z" => "Rotation".into(),
            "ADBE Opacity" => "Opacity".into(),
            _ => mn
                .trim_start_matches("ADBE ")
                .trim_start_matches("Vector ")
                .to_string(),
        }
    }

    fn blend_name(v: u8) -> &'static str {
        match v {
            3 => "Dissolve",
            4 => "Add",
            5 => "Multiply",
            6 => "Screen",
            7 => "Overlay",
            8 => "Soft Light",
            9 => "Hard Light",
            10 => "Darken",
            11 => "Lighten",
            12 => "Classic Difference",
            13 => "Hue",
            14 => "Saturation",
            15 => "Color",
            16 => "Luminosity",
            17 => "Stencil Alpha",
            18 => "Stencil Luma",
            19 => "Silhouette Alpha",
            20 => "Silhouette Luma",
            21 => "Luminescent Premultiply",
            22 => "Alpha Add",
            23 => "Classic Color Dodge",
            24 => "Classic Color Burn",
            25 => "Exclusion",
            26 => "Difference",
            27 => "Color Dodge",
            28 => "Color Burn",
            29 => "Linear Dodge",
            30 => "Linear Burn",
            31 => "Linear Light",
            32 => "Vivid Light",
            33 => "Pin Light",
            34 => "Hard Mix",
            35 => "Lighter Color",
            36 => "Darker Color",
            37 => "Subtract",
            38 => "Divide",
            _ => "Other",
        }
    }

    fn ext_of(path: &str) -> String {
        let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
        match file.rsplit_once('.') {
            Some((_, e)) => e.to_lowercase(),
            None => String::new(),
        }
    }

    /// Pictures the renderers read.
    fn is_picture(ext: &str) -> bool {
        matches!(ext, "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg")
    }

    /// Family, weight and italic from a PostScript font name
    /// (`Montserrat-SemiBoldItalic`, `TimesNewRomanPSMT`).
    fn font_parts(ps: &str) -> (String, u16, bool) {
        let (base, style) = ps.split_once('-').unwrap_or((ps, ""));
        let mut family = base.trim().to_string();
        for suffix in ["PSMT", "MT", "PS"] {
            if family.len() > suffix.len() + 2 && family.ends_with(suffix) {
                family.truncate(family.len() - suffix.len());
                break;
            }
        }
        let st = style.to_lowercase().replace([' ', '_'], "");
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
        .find(|(k, _)| st.contains(k))
        .map_or(400, |(_, w)| *w);
        (
            family,
            weight,
            st.contains("italic") || st.contains("oblique"),
        )
    }

    /// A field key from a layer name, unique among `fields`.
    pub(super) fn unique_key(fields: &[Field], name: &str) -> String {
        let mut key: String = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>()
            .split('_')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("_");
        if key.is_empty() {
            key = "text".into();
        }
        let base = key.clone();
        let mut n = 2;
        while fields.iter().any(|f| f.key == key) {
            key = format!("{base}_{n}");
            n += 1;
        }
        key
    }

    /// The value of a number track at a frame, and the easing of the
    /// segment there.
    fn value_at(track: &[Keyframe], f: f64) -> (f64, Easing) {
        let num = |k: &Keyframe| k.value.as_number().unwrap_or(1.0);
        let Some(first) = track.first() else {
            return (1.0, Easing::Linear);
        };
        if f <= first.frame {
            return (num(first), Easing::Linear);
        }
        for w in track.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            if f < b.frame {
                let span = (b.frame - a.frame).max(1e-9);
                let p = a.easing.apply((f - a.frame) / span);
                return (num(a) + (num(b) - num(a)) * p, a.easing);
            }
        }
        (track.last().map_or(1.0, num), Easing::Hold)
    }

    pub(super) fn round3(v: f64) -> f64 {
        // `+ 0.0` turns -0.0 into 0.0.
        (v * 1000.0).round() / 1000.0 + 0.0
    }

    pub(super) fn round4(v: f64) -> f64 {
        (v * 10000.0).round() / 10000.0 + 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/tests/fixtures/import/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn ctx() -> Ctx {
        Ctx {
            prefix: "/staged/".into(),
            fonts: vec!["Arial".into(), "Inter".into()],
            zone: "lower".into(),
        }
    }

    fn comp<'a>(c: &'a Converted, name: &str) -> &'a Comp {
        c.comps.iter().find(|c| c.name == name).unwrap()
    }

    fn report<'a>(c: &'a Comp, name: &str) -> &'a LayerReport {
        c.layers.iter().find(|r| r.name == name).unwrap()
    }

    fn layer<'a>(c: &'a Comp, id: &str) -> &'a Layer {
        c.template.layers.iter().find(|l| l.id == id).unwrap()
    }

    fn num(l: &Layer, p: Prop) -> f64 {
        l.props[&p].as_number().unwrap()
    }

    fn labels(r: &LayerReport) -> Vec<&str> {
        r.notes.iter().map(|n| n.label.as_str()).collect()
    }

    #[test]
    fn walks_chunks() {
        let bytes = fixture("aep_complete_ae2025.aep");
        let tree = riff::parse(&bytes).unwrap();
        assert!(!tree.truncated);
        assert!(tree.root.child(b"head").is_some());
        let fold = tree.root.list(b"Fold").unwrap();
        assert!(fold.kids.iter().any(|c| c.is_list(b"Item")));
        let p = model::read(&tree.root).unwrap();
        assert_eq!(p.version.0, 96);
        assert_eq!(p.comps.len(), 6);
        let main = p.comps.iter().find(|c| c.name == "Main_Comp").unwrap();
        assert_eq!(
            (main.width, main.height, main.fps, main.duration),
            (1920.0, 1080.0, 24.0, 30.0)
        );
        assert_eq!(main.layers.len(), 27);
        // A cut file is read as far as it goes; garbage is refused.
        let cut = riff::parse(&bytes[..bytes.len() / 2]).unwrap();
        assert!(cut.truncated);
        assert!(riff::parse(b"RIFX\0\0\0\x04Egg!").is_ok());
        assert!(riff::parse(b"RIFF\0\0\0\x04WAVE").is_err());
        assert!(analyze(b"not a project", &ctx()).is_err());
    }

    #[test]
    fn reads_text_documents() {
        let cos = cos::parse(b" /0 << /1 [ 1 2.5 -.5 ] /2 (a\\)b) /3 true >> /1 /Name").unwrap();
        assert_eq!(cos.at("0/1/2").and_then(|v| v.num()), Some(-0.5));
        assert_eq!(
            cos.at("0/2").and_then(|v| v.string()).as_deref(),
            Some("a)b")
        );
        assert_eq!(cos.at("0/3").and_then(|v| v.boolean()), Some(true));
        assert!(cos::parse(b" /0 << /1 [ 1").is_none());
    }

    #[test]
    fn converts_a_composition() {
        let c = analyze(&fixture("aep_complete_ae2025.aep"), &ctx()).unwrap();
        assert_eq!(c.format, Format::Aep);
        assert_eq!((c.width, c.height, c.fps), (1920, 1080, 24.0));
        let main = comp(&c, "Main_Comp");
        assert!(main.selected && main.animated);
        assert_eq!(main.duration, 30.0);
        assert_eq!(main.template.duration, 720.0);
        // Pre-compositions used by another start off.
        assert!(!comp(&c, "Pre_Comp").selected);

        // Solid: a rectangle of its colour, the anchor at its centre.
        let red = report(main, "Solid_Red");
        assert_eq!(red.source, "solid");
        assert_eq!(labels(red), ["Blend mode {mode}"]);
        let l = layer(main, &red.layer);
        assert!(matches!(l.kind, LayerKind::Rect { .. }));
        assert_eq!(
            l.props[&Prop::Fill],
            Value::Color(Color::rgba(1.0, 0.0, 0.0, 1.0))
        );
        assert_eq!(
            (num(l, Prop::X), num(l, Prop::Width), num(l, Prop::AnchorX)),
            (0.0, 1920.0, 0.5)
        );
        assert!(report(main, "Solid_Blue").hidden);

        // Texts become variables.
        let t = report(main, "Text_Styled");
        assert_eq!(t.status, Status::Ok);
        assert_eq!(labels(t), ["Text → variable"]);
        let l = layer(main, &t.layer);
        let LayerKind::Text {
            text,
            font_family,
            align,
            ..
        } = &l.kind
        else {
            panic!("not a text")
        };
        assert_eq!(
            (text.as_str(), font_family.as_str(), *align),
            ("{{text_styled}}", "Arial", TextAlign::Center)
        );
        assert_eq!(num(l, Prop::FontSize), 72.0);
        assert_eq!(num(l, Prop::LetterSpacing), 3.6);
        assert_eq!(
            l.props[&Prop::Fill],
            Value::Color(Color::rgba(1.0, 1.0, 0.0, 1.0))
        );
        let f = main
            .template
            .fields
            .iter()
            .find(|f| f.key == "text_styled")
            .unwrap();
        assert_eq!(
            (f.label.as_str(), f.default.as_str()),
            ("Text_Styled", "Styled Text")
        );
        let simple = report(main, "Text_Simple");
        assert_eq!(simple.notes[0].label, "Font {from} → {to}");
        assert_eq!(simple.notes[0].params["from"], "TimesNewRomanPSMT");

        // Shapes parented to a null: inside its group.
        let null = report(main, "Null_Controller");
        assert_eq!(null.source, "null");
        let rect = report(main, "Shape_Rectangle");
        assert_eq!((rect.source.as_str(), rect.depth), ("shape", 1));
        let l = layer(main, &rect.layer);
        assert_eq!(l.parent.as_deref(), Some(null.layer.as_str()));
        assert!(matches!(l.kind, LayerKind::Rect { .. }));
        assert_eq!((num(l, Prop::X), num(l, Prop::Width)), (-50.0, 100.0));
        assert_eq!(
            l.props[&Prop::Fill],
            Value::Color(Color::rgba(1.0, 0.5, 0.0, 1.0))
        );
        assert_eq!(num(l, Prop::StrokeWidth), 2.0);
        let g = layer(main, &null.layer);
        assert_eq!((num(g, Prop::X), num(g, Prop::Y)), (960.0, 540.0));
        let star = report(main, "Shape_Star");
        assert!(matches!(
            layer(main, &star.layer).kind,
            LayerKind::Pen { .. }
        ));
        assert_eq!(labels(star), ["Repeater"]);

        // Pre-composition: a group of its layers.
        let pre = report(main, "Nested_Precomp");
        assert_eq!(pre.source, "precomp");
        assert_eq!(report(main, "Precomp_BG").depth, 1);

        // What is left out, and why.
        let reasons: Vec<(&str, &str)> = main
            .ignored
            .iter()
            .map(|i| (i.name.as_str(), i.reason.as_str()))
            .collect();
        for r in [
            ("Video_Footage", "Video footage"),
            ("Audio", "Audio"),
            ("Light_Spot", "Light layer"),
            ("Camera", "Camera"),
            ("Guide", "Guide layer"),
            ("Adjustment", "Adjustment layer"),
        ] {
            assert!(reasons.contains(&r), "{r:?}");
        }
        let fx = labels(report(main, "Effect_Demo"));
        assert!(fx.iter().all(|l| *l == "Effect {name}") && fx.len() == 7);
        let ex = report(main, "Expression_Demo");
        assert_eq!(ex.notes.len(), 4);
        assert_eq!(ex.notes[0].label, "Expression");
        assert_eq!(
            labels(report(main, "Masked_Layer")),
            ["Mask {name}", "Mask {name}"]
        );
        assert_eq!(
            labels(report(main, "TrackMatte_Alpha")),
            ["Used as track matte"]
        );
        assert!(labels(report(main, "Solid_3D")).contains(&"3D flattened"));
        // Every source layer is reported once: the 27 of the composition
        // and the 2 of its pre-composition.
        assert_eq!(main.layers.len() + main.ignored.len(), 29);
        // A placeholder: kept, with the missing file reported.
        assert!(c.warnings.iter().any(
            |w| w.label == "Missing footage {file}" && w.params["file"] == "Video_Placeholder"
        ));
    }

    #[test]
    fn converts_keyframes() {
        let c = analyze(&fixture("aep_complete_ae2025.aep"), &ctx()).unwrap();
        let main = comp(&c, "Main_Comp");
        let r = report(main, "Keyframe_Demo");
        assert_eq!(labels(r), ["Opacity", "Position", "Rotation", "Scale"]);
        assert!(r.notes.iter().all(|n| n.anim && n.status == Status::Ok));
        let l = layer(main, &r.layer);
        let x: Vec<(f64, f64)> = l.tracks[&Prop::X]
            .iter()
            .map(|k| (k.frame, k.value.as_number().unwrap()))
            .collect();
        // Position 100 → 960 → 1820… minus the anchor (the solid's centre).
        assert_eq!(
            x,
            [
                (0.0, -860.0),
                (24.0, 0.0),
                (48.0, 860.0),
                (72.0, 0.0),
                (96.0, -860.0)
            ]
        );
        let o = &l.tracks[&Prop::Opacity];
        assert_eq!(o[1].easing, Easing::Hold);
        assert_eq!(l.tracks[&Prop::ScaleX][1].value, Value::Number(1.5));
        // Auto-bezier at a constant speed is linear.
        assert_eq!(l.tracks[&Prop::Rotation][0].easing, Easing::Linear);

        // Temporal ease: speed 0, influence 75 % / 33 %.
        let c = analyze(&fixture("aep_ease_scale.aep"), &ctx()).unwrap();
        let t = &c.comps[0];
        let l = &t.template.layers[0];
        let s = &l.tracks[&Prop::ScaleX];
        assert_eq!(s.len(), 4);
        assert_eq!(
            s[0].easing,
            Easing::Bezier {
                x1: 0.75,
                y1: 0.0,
                x2: 0.25,
                y2: 1.0
            }
        );
        assert_eq!(s[1].value, Value::Number(1.2));
        assert!(t.animated);
    }

    #[test]
    fn converts_markers() {
        let c = analyze(&fixture("aep_complete_ae2025.aep"), &ctx()).unwrap();
        let m: Vec<(f64, &str, MarkerKind)> = comp(&c, "Main_Comp")
            .template
            .markers
            .iter()
            .map(|m| (m.frame, m.name.as_str(), m.kind))
            .collect();
        // No marker named « on air »: the in animation ends at 4 s.
        assert_eq!(
            m,
            [
                (0.0, "Start", MarkerKind::Pause),
                (96.0, "On air", MarkerKind::Out),
                (240.0, "Middle", MarkerKind::Pause),
                (600.0, "End", MarkerKind::Pause)
            ]
        );
        // A marker named like the on-air point is the one Out marker.
        let mut item = model::CompItem {
            id: 1,
            name: "c".into(),
            width: 100.0,
            height: 100.0,
            fps: 25.0,
            duration: 10.0,
            layers: Vec::new(),
            markers: Vec::new(),
            controllers: Vec::new(),
        };
        for (t, n) in [(1.0, "Build"), (2.0, "ON AIR"), (3.0, "Out")] {
            item.markers.push(props::MarkerVal {
                t,
                duration: 0.0,
                comment: n.into(),
            });
        }
        let t = Template::blank("c", "z", 25.0);
        let kinds: Vec<MarkerKind> = conv::markers(&item, &t).iter().map(|m| m.kind).collect();
        assert_eq!(
            kinds,
            [MarkerKind::Pause, MarkerKind::Out, MarkerKind::Pause]
        );
    }

    #[test]
    fn wires_essential_graphics() {
        let c = analyze(&fixture("aep_eg_controllers.aep"), &ctx()).unwrap();
        let p = comp(&c, "primary");
        let v: Vec<(&str, &str, Status)> = p
            .variables
            .iter()
            .map(|v| (v.name.as_str(), v.key.as_str(), v.status))
            .collect();
        assert_eq!(
            v,
            [
                ("Brightness", "brightness", Status::Partial),
                ("Layer Opacity", "layer_opacity", Status::Ok),
                ("Background Color", "background_color", Status::Partial)
            ]
        );
        let l = &p.template.layers[0];
        assert_eq!(l.bindings[&Prop::Opacity], "layer_opacity");
        assert_eq!(l.bindings[&Prop::Fill], "background_color");
        let f = &p.template.fields[1];
        assert_eq!((f.default.as_str(), f.max), ("1", Some(1.0)));
    }

    /// A tiny PNG.
    fn png() -> Vec<u8> {
        let mut out = Vec::new();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    pub(crate) fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut out = std::io::Cursor::new(Vec::new());
        let mut z = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, data) in files {
            z.start_file(*name, opts).unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap();
        out.into_inner()
    }

    #[test]
    fn stages_collected_footage() {
        let aep = fixture("aep_eg_media.aep");
        // Without the footage: a placeholder and a warning.
        let bare = analyze(&aep, &ctx()).unwrap();
        assert!(bare.files.is_empty());
        assert_eq!(bare.warnings[0].label, "Missing footage {file}");
        // « Collect Files »: the project and its footage folder.
        let png = png();
        let z = zip(&[
            ("Project folder/Project.aep", &aep),
            ("Project folder/(Footage)/assets/image_with_alpha.png", &png),
        ]);
        assert_eq!(super::super::detect("x.zip", &z), Some(Format::Aep));
        let c = analyze(&z, &ctx()).unwrap();
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.files.len(), 1);
        assert_eq!(c.files[0].0, "image_with_alpha.png");
        let comp = comp(&c, "image_with_alpha 2");
        let img = comp
            .template
            .layers
            .iter()
            .find_map(|l| match &l.kind {
                LayerKind::Image { src, .. } => Some(src.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(img, "/staged/image_with_alpha.png");
        assert!(analyze(&zip(&[("a.txt", b"x")]), &ctx()).is_err());
    }

    #[test]
    fn survives_damaged_files() {
        let bytes = fixture("aep_eg_text.aep");
        for cut in [12, 100, 5_000, 40_000, 90_000, bytes.len() - 1] {
            let _ = analyze(&bytes[..cut], &ctx());
        }
        let mut b = bytes.clone();
        let mut x: u32 = 7;
        for _ in 0..2_000 {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let i = 12 + (x as usize) % (b.len() - 12);
            b[i] = (x >> 16) as u8;
        }
        let _ = analyze(&b, &ctx());
    }

    /// AE CC 2018 (file version 92, 160-byte layer records). Local only:
    /// py-aep `samples/versions/ae2018/complete.aep` in the scratch folder.
    #[test]
    #[ignore]
    fn reads_ae2018() {
        let path = std::env::var("AE2018").unwrap_or_else(|_| {
            "/tmp/claude-0/-home-user-CG/917c9136-7e86-592c-8c65-d46e87105f33/scratchpad/samples/versions_ae2018_complete.aep".into()
        });
        let c = analyze(&std::fs::read(path).unwrap(), &ctx()).unwrap();
        let main = comp(&c, "Main_Comp");
        assert_eq!(report(main, "Keyframe_Demo").notes.len(), 4);
        assert!(main
            .template
            .fields
            .iter()
            .any(|f| f.default == "Styled Text"));
    }
}
