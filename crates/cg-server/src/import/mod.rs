//! Project import: After Effects (`.aep`, or the `.zip` of « Collect
//! Files »), Motion Graphics templates (`.mogrt`), Photoshop (`.psd`) and
//! SVG. A file is first analysed into an [`Analysis`]: each composition /
//! artboard / document converted to a [`Template`], with a report of what
//! each layer became, its animations, and what was left out. The web app
//! shows it (the import modal), the person picks what to keep, then the
//! project is created from the templates it sends back
//! (`POST /api/import/{token}/create`).
//!
//! Asset files (pictures from the document) wait in a staging area under a
//! token until the project is created; the templates point at them through
//! [`Ctx::prefix`] and are re-pointed at the project's media on creation.

pub mod aep;
pub mod mogrt;
pub mod psd;
pub mod svg;

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use cg_core::{FieldKind, Template};
use serde::Serialize;

/// What the converters know of the server.
pub struct Ctx {
    /// URL prefix of the staged asset files (`<prefix><file name>`).
    pub prefix: String,
    /// Font families the renderers have (for « font X → Y » notes).
    pub fonts: Vec<String>,
    /// Zone of the templates (the new project's first zone).
    pub zone: String,
}

impl Ctx {
    /// The family to use for a font of the document: itself when the server
    /// has it (case-insensitive, ignoring a style suffix like "-Bold"), else
    /// "Inter". The second value is false when replaced.
    pub fn font(&self, family: &str) -> (String, bool) {
        let want = family.trim();
        let base = want
            .split(['-', ','])
            .next()
            .unwrap_or(want)
            .trim()
            .to_lowercase();
        for f in &self.fonts {
            let l = f.to_lowercase();
            if l == want.to_lowercase() || l == base || l.replace(' ', "") == base.replace(' ', "")
            {
                return (f.clone(), true);
            }
        }
        ("Inter".into(), want.is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Format {
    Aep,
    Mogrt,
    Psd,
    Svg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// As in the document.
    Ok,
    /// Simplified (an animation curve, a replaced font, a rasterised object…).
    Partial,
    /// Left out (shown struck through).
    Ignored,
}

/// One line of what happened to a layer: an animated property, an effect,
/// the text becoming editable, a font replaced… `label` is English, with
/// `{name}` placeholders filled from `params` (the web app translates it).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub label: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
    pub status: Status,
    /// An animation (keyframes), drawn in the modal's timeline.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub anim: bool,
}

impl Note {
    pub fn new(label: &str, status: Status) -> Self {
        Self {
            label: label.into(),
            params: BTreeMap::new(),
            status,
            anim: false,
        }
    }
    /// An animated property ("Position", "Opacity"…).
    pub fn anim(label: &str, status: Status) -> Self {
        Self {
            anim: true,
            ..Self::new(label, status)
        }
    }
    pub fn with(mut self, key: &str, value: impl Into<String>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }
}

/// A layer of the document that became a layer of the template.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerReport {
    /// Id of the layer in the template.
    pub layer: String,
    /// Its name in the document.
    pub name: String,
    /// What it was: "text", "solid", "shape", "image", "group", "smartObject",
    /// "precomp", "path", "rect", "ellipse"…
    pub source: String,
    /// Nesting in the list (children of a group: 1…).
    pub depth: u32,
    pub status: Status,
    pub notes: Vec<Note>,
    /// Hidden in the document: it starts switched off in the modal.
    pub hidden: bool,
}

/// Something of the document not imported at all. `reason` as [`Note::label`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ignored {
    pub name: String,
    /// "effect", "audio", "adjustment", "camera", "light", "script", "css"…
    pub source: String,
    pub reason: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
}

impl Ignored {
    pub fn new(name: &str, source: &str, reason: &str) -> Self {
        Self {
            name: name.into(),
            source: source.into(),
            reason: reason.into(),
            params: BTreeMap::new(),
        }
    }
}

/// A control of the document that became a variable of the template
/// (Essential Graphics of a .mogrt).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableReport {
    /// Its name in the document.
    pub name: String,
    /// The template field it became.
    pub key: String,
    pub kind: FieldKind,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<Note>,
}

/// A composition / artboard / document: one template.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comp {
    pub id: String,
    pub name: String,
    /// Seconds (0 = still).
    pub duration: f64,
    /// Selected by default (pre-compositions used by others start off).
    pub selected: bool,
    pub template: Template,
    /// The layers, in the order of the list (top first), groups before
    /// their children.
    pub layers: Vec<LayerReport>,
    pub ignored: Vec<Ignored>,
    pub variables: Vec<VariableReport>,
    /// Has animations of its own (else the modal offers an in / out preset).
    pub animated: bool,
    /// The whole composition as one picture, when the file has it (the
    /// SVG itself, a PSD's composite): the list's thumbnail, and « import
    /// as one picture ».
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flat: Option<String>,
}

/// What a file holds, converted.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Converted {
    pub format: Format,
    /// Suggested project name (the file's).
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Frames per second of the document (the project's).
    pub fps: f64,
    pub comps: Vec<Comp>,
    /// About the whole file (missing footage, unknown version…).
    pub warnings: Vec<Note>,
    #[serde(skip)]
    pub files: Vec<(String, Vec<u8>)>,
}

/// The format of a file, from its name, else its first bytes.
pub fn detect(file_name: &str, bytes: &[u8]) -> Option<Format> {
    let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "aep" => return Some(Format::Aep),
        "mogrt" => return Some(Format::Mogrt),
        "psd" | "psb" => return Some(Format::Psd),
        "svg" => return Some(Format::Svg),
        _ => {}
    }
    if bytes.starts_with(b"8BPS") {
        return Some(Format::Psd);
    }
    if bytes.starts_with(b"RIFX") {
        return Some(Format::Aep);
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(2048)]);
    if head.contains("<svg") {
        return Some(Format::Svg);
    }
    if bytes.starts_with(b"PK") {
        // A .mogrt keeps definition.json; « Collect Files » an .aep.
        let names = zip_names(bytes);
        if names.iter().any(|n| n.ends_with("definition.json")) {
            return Some(Format::Mogrt);
        }
        if names.iter().any(|n| n.to_lowercase().ends_with(".aep")) {
            return Some(Format::Aep);
        }
    }
    None
}

pub fn zip_names(bytes: &[u8]) -> Vec<String> {
    zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map(|z| {
            z.file_names()
                .filter_map(|n| n.ok().map(|n| n.into_owned()))
                .collect()
        })
        .unwrap_or_default()
}

/// Analyses a file.
pub fn analyze(file_name: &str, bytes: &[u8], ctx: &Ctx) -> Result<Converted, String> {
    let format = detect(file_name, bytes).ok_or(
        "unsupported file: After Effects (.aep, or the .zip of Collect Files), .mogrt, .psd or .svg",
    )?;
    let mut out = match format {
        Format::Aep => aep::analyze(bytes, ctx),
        Format::Mogrt => mogrt::analyze(bytes, ctx),
        Format::Psd => psd::analyze(bytes, ctx),
        Format::Svg => svg::analyze(bytes, ctx),
    }?;
    if out.name.trim().is_empty() {
        out.name = file_name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(file_name)
            .rsplit_once('.')
            .map(|(n, _)| n)
            .unwrap_or(file_name)
            .replace('_', " ");
    }
    Ok(out)
}

// -------------------------------------------------------------- staging ----

/// Staged files of the analyses not yet turned into a project.
struct Staged {
    at: Instant,
    user: String,
    files: HashMap<String, Vec<u8>>,
}

static STAGE: Mutex<Option<HashMap<String, Staged>>> = Mutex::new(None);
const KEEP: Duration = Duration::from_secs(2 * 3600);

/// Keeps the files of an analysis under `token` (made before the
/// analysis: its [`stage_prefix`] is in the templates).
pub fn stage(token: &str, user: &str, files: Vec<(String, Vec<u8>)>) {
    let mut g = STAGE.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    m.retain(|_, s| s.at.elapsed() < KEEP);
    m.insert(
        token.into(),
        Staged {
            at: Instant::now(),
            user: user.into(),
            files: files.into_iter().collect(),
        },
    );
}

/// A staged file (for the modal's preview), to its user only.
pub fn staged_file(token: &str, user: &str, name: &str) -> Option<Vec<u8>> {
    let g = STAGE.lock().unwrap_or_else(|e| e.into_inner());
    let s = g.as_ref()?.get(token)?;
    (s.user == user)
        .then(|| s.files.get(name).cloned())
        .flatten()
}

/// Takes the files of an analysis (on project creation).
pub fn take_staged(token: &str, user: &str) -> Option<HashMap<String, Vec<u8>>> {
    let mut g = STAGE.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.as_mut()?;
    if m.get(token)?.user != user {
        return None;
    }
    m.remove(token).map(|s| s.files)
}

/// URL prefix of an analysis' staged files.
pub fn stage_prefix(token: &str) -> String {
    format!("/api/import/{token}/files/")
}
