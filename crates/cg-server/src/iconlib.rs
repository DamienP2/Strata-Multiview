//! The picture library: icons, emojis and flags of the Iconify catalogue and
//! free illustrations (Open Doodles, Flowbite), shipped with the app
//! (web/scripts/iconlib.mjs packs them into web/dist/iconlib) and read here,
//! with no Internet. A picture used is copied into the project's media.
//!
//! Updates: once a week the server asks npm whether a newer version of each
//! Iconify set exists; System › Updates installs one when asked, into
//! `<data>/iconlib`, which then wins over the copy shipped.
//! `STRATA_NPM_REGISTRY` changes where npm is read.

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path as FsPath, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json_;

use crate::api::{bad, internal, require, visible_project, ApiResult};
use crate::auth::{CurrentUser, Role};
use crate::state::Shared;

/// Pictures per page.
const PAGE: usize = 96;
/// Sets kept read in memory (the largest weigh tens of MB).
const KEPT: usize = 4;
/// Days between two looks for updates.
const CHECK_DAYS: i64 = 7;

pub fn router() -> Router<Shared> {
    Router::new()
        .route("/api/iconlib/search", get(search))
        .route("/api/iconlib/svg", get(svg))
        .route("/api/iconlib/libraries", get(libraries))
        .route("/api/iconlib/libraries/check", post(check_now))
        .route("/api/iconlib/libraries/{id}/update", post(update_one))
        .route("/api/projects/{id}/iconlib", post(use_picture))
}

fn registry() -> String {
    std::env::var("STRATA_NPM_REGISTRY")
        .unwrap_or_else(|_| "https://registry.npmjs.org".into())
        .trim_end_matches('/')
        .to_string()
}

// ---- What is shipped, what was installed

/// A set as System › Updates and the picture window know it (manifest.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetInfo {
    pub id: String,
    /// `icons`, `emoji`, `flags` or `illustrations`.
    pub kind: String,
    pub title: String,
    pub version: String,
    /// Its npm package, when it can be updated.
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub total: usize,
    /// The colour of an illustration replaced by the one chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
}

#[derive(Deserialize)]
struct Manifest {
    sets: Vec<SetInfo>,
}

/// The sets shipped with the app.
fn shipped() -> Vec<SetInfo> {
    crate::web::bundled("iconlib/manifest.json")
        .and_then(|b| serde_json::from_slice::<Manifest>(&b).ok())
        .map(|m| m.sets)
        .unwrap_or_default()
}

/// What the updates did: installed versions, the latest seen, when.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdState {
    #[serde(default)]
    installed: BTreeMap<String, String>,
    #[serde(default)]
    latest: BTreeMap<String, String>,
    #[serde(default)]
    last_check: Option<i64>,
    #[serde(default)]
    error: Option<String>,
}

fn lib_dir(data: &FsPath) -> PathBuf {
    data.join("iconlib")
}

fn read_state(data: &FsPath) -> UpdState {
    std::fs::read(lib_dir(data).join("state.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn write_state(data: &FsPath, st: &UpdState) {
    let dir = lib_dir(data);
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(b) = serde_json::to_vec_pretty(st) {
        let tmp = dir.join("state.json.tmp");
        if std::fs::write(&tmp, b).is_ok() {
            let _ = std::fs::rename(tmp, dir.join("state.json"));
        }
    }
}

/// `a` is a newer version than `b` (dotted numbers).
fn newer(a: &str, b: &str) -> bool {
    let key = |v: &str| -> Vec<u64> {
        v.split(['.', '-', '+'])
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    key(a) > key(b)
}

/// The version of a set in use: the one installed by an update when newer
/// than the one shipped.
fn in_use(info: &SetInfo, st: &UpdState) -> (String, bool) {
    match st.installed.get(&info.id) {
        Some(v) if newer(v, &info.version) => (v.clone(), true),
        _ => (info.version.clone(), false),
    }
}

// ---- A set read

/// An icon of a set (Iconify JSON), or an alias of one.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Icon {
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
    #[serde(default)]
    left: Option<f64>,
    #[serde(default)]
    top: Option<f64>,
    #[serde(default)]
    rotate: Option<u8>,
    #[serde(default)]
    h_flip: Option<bool>,
    #[serde(default)]
    v_flip: Option<bool>,
    #[serde(default)]
    hidden: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct Packed {
    #[serde(default)]
    icons: HashMap<String, Icon>,
    #[serde(default)]
    aliases: HashMap<String, Icon>,
    #[serde(default)]
    categories: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
    #[serde(default)]
    left: Option<f64>,
    #[serde(default)]
    top: Option<f64>,
}

/// A set ready to search and draw.
struct Set {
    packed: Packed,
    /// The names shown, in order (by category, then the others).
    names: Vec<String>,
    /// Each name's category, lower case.
    category: HashMap<String, String>,
}

impl Set {
    fn from_json(bytes: &[u8]) -> Result<Set, String> {
        let packed: Packed = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if packed.icons.is_empty() {
            return Err("no icon".into());
        }
        let shown = |n: &String| packed.icons.get(n).is_some_and(|i| i.hidden != Some(true));
        let mut names = Vec::new();
        let mut category = HashMap::new();
        let mut seen = std::collections::HashSet::new();
        for (cat, list) in &packed.categories {
            for n in list.iter().filter(|n| shown(n)) {
                if seen.insert(n.clone()) {
                    names.push(n.clone());
                    category.insert(n.clone(), cat.to_lowercase());
                }
            }
        }
        let mut rest: Vec<String> = packed
            .icons
            .keys()
            .filter(|n| shown(n) && !seen.contains(*n))
            .cloned()
            .collect();
        rest.sort();
        names.extend(rest);
        Ok(Set {
            packed,
            names,
            category,
        })
    }

    /// The names matching `q` (words, any order), best first.
    fn search(&self, q: &str) -> Vec<&String> {
        let words: Vec<String> = q
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect();
        if words.is_empty() {
            return self.names.iter().collect();
        }
        let joined = words.join("-");
        let mut hits: Vec<(u8, &String)> = self
            .names
            .iter()
            .filter_map(|n| {
                let cat = self.category.get(n).map(String::as_str).unwrap_or("");
                if !words
                    .iter()
                    .all(|w| n.contains(w.as_str()) || cat.contains(w.as_str()))
                {
                    return None;
                }
                let rank = if *n == joined {
                    0
                } else if n.split('-').any(|p| p == words[0]) {
                    1
                } else if words.iter().all(|w| n.contains(w.as_str())) {
                    2
                } else {
                    3
                };
                Some((rank, n))
            })
            .collect();
        hits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.len().cmp(&b.1.len())));
        hits.into_iter().map(|(_, n)| n).collect()
    }

    /// The SVG of an icon (its aliases followed, turned and flipped as said).
    fn svg(&self, name: &str) -> Option<String> {
        let p = &self.packed;
        let (mut rotate, mut h_flip, mut v_flip) = (0u8, false, false);
        let mut sized: Option<&Icon> = None;
        let mut name = name.to_string();
        for _ in 0..8 {
            let (icon, alias) = match p.icons.get(&name) {
                Some(i) => (i, false),
                None => (p.aliases.get(&name)?, true),
            };
            rotate = (rotate + icon.rotate.unwrap_or(0)) % 4;
            h_flip ^= icon.h_flip.unwrap_or(false);
            v_flip ^= icon.v_flip.unwrap_or(false);
            if alias {
                if icon.width.is_some() || icon.height.is_some() {
                    sized.get_or_insert(icon);
                }
                name = icon.parent.clone()?;
                continue;
            }
            let at = sized.unwrap_or(icon);
            let w = at.width.or(icon.width).or(p.width).unwrap_or(16.0);
            let h = at.height.or(icon.height).or(p.height).unwrap_or(16.0);
            let l = at.left.or(icon.left).or(p.left).unwrap_or(0.0);
            let t = at.top.or(icon.top).or(p.top).unwrap_or(0.0);
            let mut body = icon.body.clone()?;
            let (cx, cy) = (l + w / 2.0, t + h / 2.0);
            if rotate != 0 || h_flip || v_flip {
                let sx = if h_flip { -1 } else { 1 };
                let sy = if v_flip { -1 } else { 1 };
                body = format!(
                    r#"<g transform="translate({cx} {cy}) rotate({}) scale({sx} {sy}) translate({} {})">{body}</g>"#,
                    rotate as u32 * 90,
                    -cx,
                    -cy
                );
            }
            // A quarter turn: the box turns too.
            let (vw, vh) = if rotate % 2 == 1 { (h, w) } else { (w, h) };
            return Some(format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{vw}" height="{vh}" viewBox="{} {} {vw} {vh}">{body}</svg>"#,
                cx - vw / 2.0,
                cy - vh / 2.0
            ));
        }
        None
    }
}

/// The bytes of a set: installed by an update when newer, else shipped.
fn set_bytes(data: &FsPath, info: &SetInfo, st: &UpdState) -> Option<Vec<u8>> {
    let gz = match in_use(info, st) {
        (_, true) => std::fs::read(lib_dir(data).join(format!("{}.json.gz", info.id))).ok(),
        _ => None,
    }
    .or_else(|| {
        crate::web::bundled(&format!("iconlib/{}.json.gz", info.id)).map(|b| b.into_owned())
    })?;
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(&gz[..])
        .read_to_end(&mut out)
        .ok()?;
    Some(out)
}

type Cache = Mutex<Vec<(String, String, Arc<Set>)>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// A set, read once (the last [`KEPT`] used are kept).
fn open_set(data: &FsPath, id: &str) -> Result<(SetInfo, Arc<Set>), String> {
    let info = shipped()
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| "unknown set".to_string())?;
    let st = read_state(data);
    let (version, _) = in_use(&info, &st);
    {
        let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = c.iter().position(|(i, v, _)| i == id && *v == version) {
            let hit = c.remove(i);
            let set = hit.2.clone();
            c.push(hit);
            return Ok((info, set));
        }
    }
    let bytes = set_bytes(data, &info, &st).ok_or_else(|| "the set is missing".to_string())?;
    let set = Arc::new(Set::from_json(&bytes)?);
    let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
    c.retain(|(i, _, _)| i != id);
    c.push((id.to_string(), version, set.clone()));
    while c.len() > KEPT {
        c.remove(0);
    }
    Ok((info, set))
}

// ---- Browsing

/// `#rrggbb` (or `#rgb`), else none.
fn valid_color(c: &str) -> bool {
    let h = c.strip_prefix('#').unwrap_or("");
    matches!(h.len(), 3 | 6) && h.chars().all(|x| x.is_ascii_hexdigit())
}

#[derive(Deserialize)]
struct SearchQuery {
    set: String,
    #[serde(default)]
    q: String,
    #[serde(default)]
    page: usize,
}

/// A picture of the library.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    name: String,
    title: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Found {
    items: Vec<Item>,
    more: bool,
}

async fn search(
    State(s): State<Shared>,
    CurrentUser(_): CurrentUser,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Json<Found>> {
    let data = s.data_dir.clone();
    let found = tokio::task::spawn_blocking(move || -> Result<Found, String> {
        let (_, set) = open_set(&data, &q.set)?;
        let hits = set.search(&q.q);
        let from = q.page * PAGE;
        Ok(Found {
            more: hits.len() > from + PAGE,
            items: hits
                .into_iter()
                .skip(from)
                .take(PAGE)
                .map(|n| Item {
                    name: n.clone(),
                    title: n.replace('-', " "),
                })
                .collect(),
        })
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;
    Ok(Json(found))
}

#[derive(Deserialize, Clone)]
struct SvgQuery {
    set: String,
    name: String,
    /// `#rrggbb`: monochrome icons drawn in it, illustrations' accent.
    #[serde(default)]
    color: String,
}

/// The SVG of a picture, in the colour asked.
fn read_svg(data: &FsPath, q: &SvgQuery) -> Result<String, String> {
    let (info, set) = open_set(data, &q.set)?;
    let svg = set
        .svg(&q.name)
        .ok_or_else(|| "unknown picture".to_string())?;
    if !valid_color(&q.color) {
        return Ok(svg);
    }
    Ok(match &info.accent {
        Some(a) => svg
            .replace(a.as_str(), &q.color)
            .replace(&a.to_uppercase(), &q.color),
        None => svg.replace("currentColor", &q.color),
    })
}

async fn svg(
    State(s): State<Shared>,
    CurrentUser(_): CurrentUser,
    Query(q): Query<SvgQuery>,
) -> ApiResult<Response> {
    let data = s.data_dir.clone();
    let svg = tokio::task::spawn_blocking(move || read_svg(&data, &q))
        .await
        .map_err(internal)?
        .map_err(bad)?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "max-age=86400"),
        ],
        svg,
    )
        .into_response())
}

#[derive(Deserialize)]
struct UseBody {
    set: String,
    name: String,
    #[serde(default)]
    color: String,
    /// What the file is called in the media.
    #[serde(default)]
    title: String,
}

/// A file name from a title: letters, digits and dashes.
fn file_stem(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let c = match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').chars().take(60).collect::<String>();
    if out.is_empty() {
        "picture".into()
    } else {
        out
    }
}

/// A picture of the library copied into the project's media: its file.
async fn use_picture(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Json(b): Json<UseBody>,
) -> ApiResult<Json<crate::media::MediaFile>> {
    visible_project(&s, &me, &id).await?;
    require(me.role != Role::Viewer)?;
    let q = SvgQuery {
        set: b.set.clone(),
        name: b.name.clone(),
        color: b.color.clone(),
    };
    let data = s.data_dir.clone();
    let svg = tokio::task::spawn_blocking(move || read_svg(&data, &q))
        .await
        .map_err(internal)?
        .map_err(bad)?;
    let title = if b.title.trim().is_empty() {
        b.name.clone()
    } else {
        b.title.clone()
    };
    let color = b.color.trim_start_matches('#');
    let stem = file_stem(&if valid_color(&b.color) {
        format!("{title}-{color}")
    } else {
        title
    });
    crate::media::put_bytes(&s, &id, &format!("{stem}.svg"), sized(&svg).into_bytes())
        .await
        .map(Json)
}

// ---- Updates (System › Updates)

/// A set as System › Updates shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Library {
    #[serde(flatten)]
    info: SetInfo,
    /// The version in use (the one shipped, or installed by an update).
    in_use: String,
    /// A newer version on npm, to install.
    update: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Libraries {
    libraries: Vec<Library>,
    last_check: Option<i64>,
    error: Option<String>,
}

fn list(data: &FsPath) -> Libraries {
    let st = read_state(data);
    Libraries {
        libraries: shipped()
            .into_iter()
            .map(|info| {
                let (in_use, _) = in_use(&info, &st);
                let update = st
                    .latest
                    .get(&info.id)
                    .filter(|v| info.package.is_some() && newer(v, &in_use))
                    .cloned();
                Library {
                    info,
                    in_use,
                    update,
                }
            })
            .collect(),
        last_check: st.last_check,
        error: st.error,
    }
}

fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(Duration::from_secs(120)))
        .max_redirects(5)
        .http_status_as_error(false)
        .user_agent(concat!("strata-graphics/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn get_bytes(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut res = agent().get(url).call().map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("{url}: {}", res.status()));
    }
    res.body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

/// npm's latest version of a package, and where its archive is.
fn latest(package: &str) -> Result<(String, String, Option<String>), String> {
    let body = get_bytes(&format!("{}/{package}/latest", registry()), 1 << 20)?;
    let v: Json_ = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    let version = v["version"].as_str().ok_or("no version")?.to_string();
    let tarball = v["dist"]["tarball"]
        .as_str()
        .ok_or("no archive")?
        .to_string();
    let integrity = v["dist"]["integrity"].as_str().map(str::to_string);
    Ok((version, tarball, integrity))
}

/// Asks npm for the latest version of every set that can be updated.
fn check(data: &FsPath) -> UpdState {
    let mut st = read_state(data);
    let mut errors = Vec::new();
    for info in shipped() {
        let Some(pkg) = &info.package else { continue };
        match latest(pkg) {
            Ok((v, _, _)) => {
                st.latest.insert(info.id.clone(), v);
            }
            Err(e) => errors.push(format!("{}: {e}", info.title)),
        }
    }
    st.last_check = Some(crate::state::now_ms());
    st.error = (!errors.is_empty()).then(|| errors.join(" · "));
    write_state(data, &st);
    st
}

/// The files of an npm archive (`package/<name>`) among `wanted`.
fn untar(tgz: &[u8], wanted: &[&str]) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut out = HashMap::new();
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(tgz));
    for e in ar.entries().map_err(|e| e.to_string())? {
        let mut e = e.map_err(|e| e.to_string())?;
        let path = e
            .path()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .to_string();
        let name = path
            .split_once('/')
            .map_or(path.as_str(), |(_, n)| n)
            .to_string();
        if wanted.contains(&name.as_str()) {
            let mut b = Vec::new();
            e.read_to_end(&mut b).map_err(|e| e.to_string())?;
            out.insert(name, b);
        }
    }
    Ok(out)
}

/// An Iconify package's archive packed as the app ships it (iconlib.mjs).
fn pack(tgz: &[u8]) -> Result<Vec<u8>, String> {
    let files = untar(tgz, &["icons.json", "info.json", "metadata.json"])?;
    let icons: Json_ = serde_json::from_slice(files.get("icons.json").ok_or("no icons.json")?)
        .map_err(|e| e.to_string())?;
    let read = |n: &str| -> Json_ {
        files
            .get(n)
            .and_then(|b| serde_json::from_slice(b).ok())
            .unwrap_or(Json_::Null)
    };
    let mut keep = serde_json::json!({
        "prefix": icons["prefix"],
        "info": read("info.json"),
        "icons": icons["icons"],
        "aliases": icons.get("aliases").cloned().unwrap_or_else(|| serde_json::json!({})),
        "categories": read("metadata.json").get("categories").cloned().unwrap_or_else(|| serde_json::json!({})),
    });
    for k in ["width", "height", "left", "top"] {
        if let Some(v) = icons.get(k) {
            keep[k] = v.clone();
        }
    }
    let json = serde_json::to_vec(&keep).map_err(|e| e.to_string())?;
    // It must read as a set before it replaces one.
    Set::from_json(&json)?;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    std::io::Write::write_all(&mut gz, &json).map_err(|e| e.to_string())?;
    gz.finish().map_err(|e| e.to_string())
}

/// The archive is the one npm says (its SHA-512).
fn verify(tgz: &[u8], integrity: Option<&str>) -> Result<(), String> {
    use base64::Engine;
    use sha2::Digest;
    let Some(want) = integrity.and_then(|i| i.strip_prefix("sha512-")) else {
        return Ok(());
    };
    let got = base64::engine::general_purpose::STANDARD.encode(sha2::Sha512::digest(tgz));
    if got == want {
        Ok(())
    } else {
        Err("the archive is not the one npm announced".into())
    }
}

/// Installs the latest version of a set into `<data>/iconlib`.
fn install(data: &FsPath, id: &str) -> Result<String, String> {
    let info = shipped()
        .into_iter()
        .find(|s| s.id == id)
        .ok_or("unknown set")?;
    let pkg = info
        .package
        .clone()
        .ok_or("this set is updated with the app")?;
    let (version, tarball, integrity) = latest(&pkg)?;
    // Only npm's own archives.
    if !tarball.starts_with(&format!("{}/", registry())) {
        return Err("unexpected archive address".into());
    }
    let tgz = get_bytes(&tarball, 200 << 20)?;
    verify(&tgz, integrity.as_deref())?;
    let gz = pack(&tgz)?;
    let dir = lib_dir(data);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{id}.json.gz.tmp"));
    std::fs::write(&tmp, gz).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join(format!("{id}.json.gz"))).map_err(|e| e.to_string())?;
    let mut st = read_state(data);
    st.installed.insert(id.to_string(), version.clone());
    st.latest.insert(id.to_string(), version.clone());
    write_state(data, &st);
    tracing::info!(set = id, version = %version, "picture library updated");
    Ok(version)
}

async fn libraries(State(s): State<Shared>, CurrentUser(_): CurrentUser) -> Json<Libraries> {
    Json(list(&s.data_dir))
}

async fn check_now(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Json<Libraries>> {
    require(me.role == Role::Admin)?;
    let data = s.data_dir.clone();
    tokio::task::spawn_blocking(move || {
        check(&data);
        list(&data)
    })
    .await
    .map(Json)
    .map_err(internal)
}

async fn update_one(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Libraries>> {
    require(me.role == Role::Admin)?;
    let data = s.data_dir.clone();
    tokio::task::spawn_blocking(move || install(&data, &id).map(|_| list(&data)))
        .await
        .map_err(internal)?
        .map(Json)
        .map_err(|e| bad(format!("the update failed: {e}")))
}

/// Looks for updates once a week (one small request per set).
pub async fn run(s: Shared) {
    loop {
        let data = s.data_dir.clone();
        let due = read_state(&data)
            .last_check
            .is_none_or(|t| crate::state::now_ms() - t > CHECK_DAYS * 86_400_000);
        if due {
            let _ = tokio::task::spawn_blocking(move || check(&data)).await;
        }
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

/// The SVG at a size the renderer draws sharply (its longest side 512 px,
/// from its viewBox): the catalogue's icons are written 1em.
fn sized(svg: &str) -> String {
    let Some(start) = svg.find("<svg") else {
        return svg.to_string();
    };
    let Some(end) = svg[start..].find('>').map(|e| start + e) else {
        return svg.to_string();
    };
    let tag = &svg[start..end];
    let attr = |name: &str| -> Option<(usize, usize, String)> {
        let at = tag.find(&format!(" {name}=\""))?;
        let from = at + name.len() + 3;
        let to = from + tag[from..].find('"')?;
        Some((at, to + 1, tag[from..to].to_string()))
    };
    let Some((_, _, vb)) = attr("viewBox") else {
        return svg.to_string();
    };
    let n: Vec<f64> = vb
        .split([' ', ','])
        .filter_map(|x| x.parse().ok())
        .collect();
    let (w, h) = match n[..] {
        [_, _, w, h] if w > 0.0 && h > 0.0 => (w, h),
        _ => return svg.to_string(),
    };
    let k = 512.0 / w.max(h);
    let mut tag = tag.to_string();
    for name in ["width", "height"] {
        let t = tag.clone();
        let at = t.find(&format!(" {name}=\""));
        if let Some(at) = at {
            let from = at + name.len() + 3;
            if let Some(len) = t[from..].find('"') {
                tag.replace_range(at..from + len + 1, "");
            }
        }
    }
    tag.push_str(&format!(
        " width=\"{}\" height=\"{}\"",
        (w * k).round(),
        (h * k).round()
    ));
    format!("{}{}{}", &svg[..start], tag, &svg[end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> Set {
        let v = serde_json::json!({
            "prefix": "x", "width": 24, "height": 24,
            "icons": {
                "sun": { "body": "<circle r='4'/>" },
                "wide": { "body": "<path/>", "width": 32 },
                "sun-cloud": { "body": "<g/>" },
                "secret": { "body": "<g/>", "hidden": true }
            },
            "aliases": { "day": { "parent": "sun" }, "night": { "parent": "sun", "hFlip": true, "rotate": 1 } },
            "categories": { "Weather": ["sun-cloud", "sun"] }
        });
        Set::from_json(&serde_json::to_vec(&v).unwrap()).unwrap()
    }

    #[test]
    fn a_set_is_searched_and_drawn_here() {
        let s = set();
        // By category first, hidden ones left out.
        assert_eq!(s.names, ["sun-cloud", "sun", "wide"]);
        assert_eq!(s.search("sun"), ["sun", "sun-cloud"]);
        assert_eq!(s.search("cloud sun"), ["sun-cloud"]);
        assert_eq!(s.search("weather").len(), 2);
        assert!(s.search("moon").is_empty());
        let sun = s.svg("sun").unwrap();
        assert!(sun.contains(r#"viewBox="0 0 24 24""#) && sun.contains("<circle r='4'/>"));
        assert!(s.svg("wide").unwrap().contains(r#"viewBox="0 0 32 24""#));
        assert_eq!(s.svg("day"), Some(sun));
        let night = s.svg("night").unwrap();
        assert!(night.contains("rotate(90) scale(-1 1)"), "{night}");
        assert!(s.svg("none").is_none());
    }

    #[test]
    fn an_update_is_packed_from_npm_s_archive() {
        let mut tgz = Vec::new();
        {
            let gz = flate2::write::GzEncoder::new(&mut tgz, flate2::Compression::fast());
            let mut b = tar::Builder::new(gz);
            let mut add = |name: &str, body: &str| {
                let mut h = tar::Header::new_gnu();
                h.set_size(body.len() as u64);
                h.set_mode(0o644);
                h.set_cksum();
                b.append_data(&mut h, format!("package/{name}"), body.as_bytes())
                    .unwrap();
            };
            add(
                "icons.json",
                r#"{"prefix":"x","icons":{"a":{"body":"<g/>"}},"width":20,"height":20}"#,
            );
            add("metadata.json", r#"{"categories":{"Letters":["a"]}}"#);
            add("package.json", r#"{"version":"9.9.9"}"#);
            b.into_inner().unwrap().finish().unwrap();
        }
        let gz = pack(&tgz).unwrap();
        let mut json = Vec::new();
        flate2::read::GzDecoder::new(&gz[..])
            .read_to_end(&mut json)
            .unwrap();
        let s = Set::from_json(&json).unwrap();
        assert_eq!(s.search("letters"), ["a"]);
        assert!(s.svg("a").unwrap().contains(r#"viewBox="0 0 20 20""#));
        // Not an icon set: refused.
        assert!(pack(b"nope").is_err());
        assert!(newer("1.2.10", "1.2.9") && !newer("1.2.9", "1.2.9"));
        assert!(verify(b"x", Some("sha512-nope")).is_err() && verify(b"x", None).is_ok());
    }

    #[test]
    fn names_and_sizes() {
        assert_eq!(file_stem("Météo · soleil!"), "meteo-soleil");
        assert!(valid_color("#22c55e") && !valid_color("red"));
        assert_eq!(
            sized(r#"<svg xmlns="x" width="1em" height="1em" viewBox="0 0 24 12"><g/></svg>"#),
            r#"<svg xmlns="x" viewBox="0 0 24 12" width="512" height="256"><g/></svg>"#
        );
    }
}
