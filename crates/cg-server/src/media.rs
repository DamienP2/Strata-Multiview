//! Media libraries: files shared by every project (`/assets/<name>`) and the
//! files of one project (`/assets/projects/<id>/<name>`), plus the folders
//! the project list is sorted into.

use std::collections::BTreeMap;
use std::path::{Path as FsPath, PathBuf};

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::api::{bad, internal, not_found, require, valid_asset_name, visible_project, ApiResult};
use crate::auth::{CurrentUser, Role, User};
use crate::state::{Event, Shared};

/// Largest upload (videos).
const MAX_UPLOAD: usize = 2 * 1024 * 1024 * 1024;

pub fn router() -> Router<Shared> {
    Router::new()
        .route("/api/media", get(list_shared))
        .route(
            "/api/media/{name}",
            put(put_shared)
                .delete(delete_shared)
                .layer(DefaultBodyLimit::max(MAX_UPLOAD)),
        )
        .route("/api/projects/{id}/media", get(list_project))
        .route(
            "/api/projects/{id}/sequence/{name}",
            axum::routing::post(put_sequence).layer(DefaultBodyLimit::max(MAX_UPLOAD)),
        )
        .route(
            "/api/projects/{id}/sequence-from",
            axum::routing::post(post_sequence_from),
        )
        .route(
            "/api/projects/{id}/media/{name}",
            put(put_project)
                .delete(delete_project)
                .layer(DefaultBodyLimit::max(MAX_UPLOAD)),
        )
        .route("/api/folders", get(get_folders).put(put_folders))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaFile {
    pub name: String,
    /// URL used in templates.
    pub src: String,
    pub size: u64,
    /// Unix ms.
    pub modified: i64,
    /// image, video, audio, font or other.
    pub kind: &'static str,
}

pub fn kind_of(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "avif" | "bmp" => "image",
        "mp4" | "mov" | "webm" | "mkv" | "m4v" => "video",
        "mp3" | "wav" | "aac" | "m4a" | "ogg" | "flac" => "audio",
        "ttf" | "otf" | "woff" | "woff2" => "font",
        _ => "other",
    }
}

/// Directory of a project's media.
pub fn project_dir(s: &Shared, project: &str) -> PathBuf {
    s.assets_dir().join("projects").join(project)
}

/// URL prefix of a project's media.
pub fn project_prefix(project: &str) -> String {
    format!("/assets/projects/{project}/")
}

async fn list_dir(dir: &FsPath, prefix: &str) -> ApiResult<Vec<MediaFile>> {
    let mut out = Vec::new();
    let mut rd = match tokio::fs::read_dir(dir).await {
        Ok(d) => d,
        Err(_) => return Ok(out),
    };
    while let Some(e) = rd.next_entry().await.map_err(internal)? {
        let Ok(meta) = e.metadata().await else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue; // upload in progress
        }
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as i64);
        out.push(MediaFile {
            src: format!("{prefix}{name}"),
            kind: kind_of(&name),
            size: meta.len(),
            modified,
            name,
        });
    }
    out.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(out)
}

/// Streams the request body to `dir/name` (through a temporary file).
async fn store(
    s: &Shared,
    dir: &FsPath,
    prefix: &str,
    name: &str,
    body: Body,
) -> ApiResult<Json<MediaFile>> {
    if !valid_asset_name(name) {
        return Err(bad("invalid file name (letters, digits, . - _)"));
    }
    tokio::fs::create_dir_all(dir).await.map_err(internal)?;
    let tmp = dir.join(format!(".{name}.part"));
    let mut f = tokio::fs::File::create(&tmp).await.map_err(internal)?;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                let _ = tokio::fs::remove_file(&tmp).await;
                return Err(bad(e));
            }
        };
        f.write_all(&chunk).await.map_err(internal)?;
    }
    f.flush().await.map_err(internal)?;
    drop(f);
    // An Illustrator file: kept as SVG pictures (one per artboard), vectors and all.
    let name = if let Some(stem) = illustrator_stem(name) {
        let bytes = tokio::fs::read(&tmp).await.map_err(internal)?;
        let _ = tokio::fs::remove_file(&tmp).await;
        let svgs = tokio::task::spawn_blocking(move || illustrator_svgs(bytes))
            .await
            .map_err(internal)?
            .map_err(bad)?;
        let names = artboard_names(stem, svgs.len());
        for (n, svg) in names.iter().zip(svgs) {
            tokio::fs::write(dir.join(n), svg).await.map_err(internal)?;
        }
        names.into_iter().next().unwrap_or_default()
    } else {
        tokio::fs::rename(&tmp, dir.join(name))
            .await
            .map_err(internal)?;
        name.to_string()
    };
    let name = name.as_str();
    s.renderer
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .invalidate_assets();
    s.thumbs.clear();
    list_dir(dir, prefix)
        .await?
        .into_iter()
        .find(|m| m.name == name)
        .map(Json)
        .ok_or_else(|| internal("upload lost"))
}

/// A file written into a project's media (a picture of the library): its
/// entry, under `name`, or `name-2`… when another file has that name.
pub async fn put_bytes(
    s: &Shared,
    project: &str,
    name: &str,
    bytes: Vec<u8>,
) -> ApiResult<MediaFile> {
    let dir = project_dir(s, project);
    tokio::fs::create_dir_all(&dir).await.map_err(internal)?;
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    let mut name = name.to_string();
    for i in 2.. {
        match tokio::fs::read(dir.join(&name)).await {
            // The same picture already there: used again.
            Ok(old) if old == bytes => break,
            Ok(_) => name = format!("{stem}-{i}.{ext}"),
            Err(_) => {
                tokio::fs::write(dir.join(&name), &bytes)
                    .await
                    .map_err(internal)?;
                break;
            }
        }
    }
    if !valid_asset_name(&name) {
        return Err(bad("invalid file name"));
    }
    s.renderer
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .invalidate_assets();
    s.thumbs.clear();
    s.changed(format!("media:{project}"));
    list_dir(&dir, &project_prefix(project))
        .await?
        .into_iter()
        .find(|m| m.name == name)
        .ok_or_else(|| internal("file lost"))
}

/// The name without `.ai` of an Illustrator file.
fn illustrator_stem(name: &str) -> Option<&str> {
    let (stem, ext) = name.rsplit_once('.')?;
    (ext.eq_ignore_ascii_case("ai") && !stem.is_empty()).then_some(stem)
}

/// The SVG pictures of an Illustrator file's artboards: `logo.svg`, then `logo-2.svg`…
fn artboard_names(stem: &str, n: usize) -> Vec<String> {
    (0..n)
        .map(|i| match i {
            0 => format!("{stem}.svg"),
            i => format!("{stem}-{}.svg", i + 1),
        })
        .collect()
}

/// An Illustrator file as SVG, one picture per artboard. Illustrator saves a
/// PDF inside its files (« Create PDF Compatible File », on by default): its
/// pages are the artboards, drawn here with their paths, text and pictures.
fn illustrator_svgs(bytes: Vec<u8>) -> Result<Vec<String>, String> {
    use hayro_svg::hayro_syntax::Pdf;
    let unreadable = "this Illustrator file cannot be read: save it again with « Create PDF Compatible File » checked";
    let pdf = Pdf::new(bytes).map_err(|_| unreadable.to_string())?;
    let cache = hayro_svg::RenderCache::new();
    let settings = hayro_svg::hayro_interpret::InterpreterSettings::default();
    let svgs: Vec<String> = pdf
        .pages()
        .iter()
        .map(|page| hayro_svg::convert(page, &cache, &settings, &Default::default()))
        .collect();
    if svgs.is_empty() {
        return Err(unreadable.into());
    }
    Ok(svgs)
}

async fn remove(s: &Shared, dir: &FsPath, name: &str) -> ApiResult<StatusCode> {
    if !valid_asset_name(name) {
        return Err(bad("invalid file name"));
    }
    match tokio::fs::remove_file(dir.join(name)).await {
        Ok(()) => {
            s.renderer
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .invalidate_assets();
            s.thumbs.clear();
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(not_found("file")),
        Err(e) => Err(internal(e)),
    }
}

fn can_upload(me: &User) -> bool {
    // Operators upload pictures for image fields too.
    me.role != Role::Viewer
}

async fn list_shared(
    State(s): State<Shared>,
    CurrentUser(_): CurrentUser,
) -> ApiResult<Json<Vec<MediaFile>>> {
    list_dir(&s.assets_dir(), "/assets/").await.map(Json)
}

async fn put_shared(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(name): Path<String>,
    body: Body,
) -> ApiResult<Json<MediaFile>> {
    require(can_upload(&me))?;
    let r = store(&s, &s.assets_dir(), "/assets/", &name, body).await?;
    s.changed("media");
    Ok(r)
}

async fn delete_shared(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(name): Path<String>,
) -> ApiResult<StatusCode> {
    require(me.can_create_projects())?;
    let r = remove(&s, &s.assets_dir(), &name).await?;
    s.changed("media");
    Ok(r)
}

async fn list_project(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<MediaFile>>> {
    visible_project(&s, &me, &id).await?;
    list_dir(&project_dir(&s, &id), &project_prefix(&id))
        .await
        .map(Json)
}

async fn put_project(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path((id, name)): Path<(String, String)>,
    body: Body,
) -> ApiResult<Json<MediaFile>> {
    visible_project(&s, &me, &id).await?;
    require(can_upload(&me))?;
    let r = store(&s, &project_dir(&s, &id), &project_prefix(&id), &name, body).await?;
    s.changed(format!("media:{id}"));
    Ok(r)
}

/// An animated GIF / APNG split into PNG frames in the project's media
/// (`<name>-000.png`…), for an Image sequence: its frames and frame rate.
#[derive(Serialize)]
struct Sequence {
    frames: Vec<String>,
    fps: f32,
    /// Size of the pictures.
    width: u32,
    height: u32,
}

const VIDEO: [&str; 7] = ["mp4", "mov", "m4v", "webm", "mkv", "avi", "mxf"];
/// At most a minute of video in a sequence.
const VIDEO_SECONDS: u32 = 60;

/// A video split into PNG frames (alpha kept) at `fps`, with ffmpeg.
fn video_frames(bytes: &[u8], ext: &str, fps: f64) -> Result<Vec<Vec<u8>>, String> {
    let dir = std::env::temp_dir().join(format!("cg-video-{}", cg_core::new_id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let run = || -> Result<Vec<Vec<u8>>, String> {
        let input = dir.join(format!("in.{ext}"));
        std::fs::write(&input, bytes).map_err(|e| e.to_string())?;
        let out = cg_output::ffmpeg::command()
            .arg("-i")
            .arg(&input)
            .args([
                "-t",
                &VIDEO_SECONDS.to_string(),
                "-an",
                "-vf",
                &format!("fps={fps}"),
                "-pix_fmt",
                "rgba",
            ])
            .arg(dir.join("f-%05d.png"))
            .output()
            .map_err(|e| format!("ffmpeg is needed for videos on this server ({e})"))?;
        if !out.status.success() {
            return Err(format!(
                "this video cannot be read: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
            .filter(|n| n.starts_with("f-") && n.ends_with(".png"))
            .collect();
        names.sort();
        names
            .iter()
            .map(|n| std::fs::read(dir.join(n)).map_err(|e| e.to_string()))
            .collect()
    };
    let r = run();
    let _ = std::fs::remove_dir_all(&dir);
    match r {
        Ok(f) if f.is_empty() => Err("no picture in this video".into()),
        r => r,
    }
}

/// Names compared with their numbers as numbers (`f2` before `f10`).
fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    let chunks = |s: &str| {
        let mut out: Vec<(bool, String)> = Vec::new();
        for c in s.chars() {
            let d = c.is_ascii_digit();
            match out.last_mut() {
                Some((was, cur)) if *was == d => cur.push(c),
                _ => out.push((d, c.to_string())),
            }
        }
        out
    };
    let (x, y) = (chunks(a), chunks(b));
    for ((dx, cx), (dy, cy)) in x.iter().zip(&y) {
        let o = if *dx && *dy {
            let (tx, ty) = (cx.trim_start_matches('0'), cy.trim_start_matches('0'));
            tx.len().cmp(&ty.len()).then_with(|| tx.cmp(ty))
        } else {
            cx.to_lowercase().cmp(&cy.to_lowercase())
        };
        if o != std::cmp::Ordering::Equal {
            return o;
        }
    }
    x.len().cmp(&y.len())
}

/// A PNG sequence in a zip: its pictures in the order of their names.
fn zip_frames(bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("invalid archive: {e}"))?;
    let mut names: Vec<String> = zip
        .file_names()
        .filter_map(|n| n.ok().map(|n| n.into_owned()))
        .filter(|n| {
            let base = n.rsplit('/').next().unwrap_or(n);
            n.to_ascii_lowercase().ends_with(".png")
                && !n.starts_with("__MACOSX/")
                && !base.starts_with('.')
        })
        .collect();
    names.sort_by(|a, b| natural(a, b));
    if names.is_empty() {
        return Err("no PNG picture in this zip".into());
    }
    if names.len() > 2000 {
        return Err(format!("{} pictures: at most 2000", names.len()));
    }
    names
        .iter()
        .map(|n| {
            let f = zip.by_name(n).map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            f.take(64 * 1024 * 1024)
                .read_to_end(&mut out)
                .map_err(|e| e.to_string())?;
            Ok(out)
        })
        .collect()
}

async fn put_sequence(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path((id, name)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> ApiResult<Json<Sequence>> {
    sequence(&s, &me, &id, &name, body).await
}

#[derive(Deserialize)]
struct SequenceFrom {
    /// A video or animation already in the assets (`/assets/…`).
    src: String,
}

/// A video already in the shared assets or the project's, split into frames.
async fn post_sequence_from(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Json(q): Json<SequenceFrom>,
) -> ApiResult<Json<Sequence>> {
    let prefix = project_prefix(&id);
    let (dir, name) = if let Some(n) = q.src.strip_prefix(&prefix) {
        (project_dir(&s, &id), n.to_string())
    } else if let Some(n) = q.src.strip_prefix("/assets/").filter(|n| !n.contains('/')) {
        (s.assets_dir(), n.to_string())
    } else {
        return Err(bad("not a file of this project or of the shared assets"));
    };
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.starts_with('.') {
        return Err(bad("invalid file name"));
    }
    let body = tokio::fs::read(dir.join(&name))
        .await
        .map_err(|_| not_found("file"))?;
    sequence(&s, &me, &id, &name, body.into()).await
}

async fn sequence(
    s: &Shared,
    me: &crate::auth::User,
    id: &str,
    name: &str,
    body: axum::body::Bytes,
) -> ApiResult<Json<Sequence>> {
    let (s, id, name) = (s.clone(), id.to_string(), name.to_string());
    visible_project(&s, me, &id).await?;
    require(can_upload(me))?;
    // The frames' base name: the file's, cleaned.
    let stem: String = name
        .rsplit_once('.')
        .map_or(name.as_str(), |(a, _)| a)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(60)
        .collect();
    let stem = if stem.is_empty() {
        "sequence".to_string()
    } else {
        stem
    };
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    // A video plays at the project's rate: one picture per frame.
    let project_fps = visible_project(&s, me, &id)
        .await?
        .frame_rate
        .as_f64()
        .clamp(1.0, 120.0);
    let (pngs, fps) = tokio::task::spawn_blocking(move || {
        if VIDEO.contains(&ext.as_str()) {
            video_frames(&body, &ext, project_fps).map(|f| (f, project_fps as f32))
        } else if ext == "zip" {
            // A PNG sequence: played at the project's rate.
            zip_frames(&body).map(|f| (f, project_fps as f32))
        } else {
            cg_render::sequence::split(&body)
        }
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;
    let (width, height) = pngs
        .first()
        .and_then(|p| cg_render::sequence::size(p))
        .unwrap_or((0, 0));
    let dir = project_dir(&s, &id);
    tokio::fs::create_dir_all(&dir).await.map_err(internal)?;
    let prefix = project_prefix(&id);
    let mut frames = Vec::with_capacity(pngs.len());
    for (i, png) in pngs.iter().enumerate() {
        let file = format!("{stem}-{i:04}.png");
        tokio::fs::write(dir.join(&file), png)
            .await
            .map_err(internal)?;
        frames.push(format!("{prefix}{file}"));
    }
    s.renderer
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .invalidate_assets();
    s.thumbs.clear();
    s.changed(format!("media:{id}"));
    Ok(Json(Sequence {
        frames,
        fps,
        width,
        height,
    }))
}

async fn delete_project(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path((id, name)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    visible_project(&s, &me, &id).await?;
    require(me.can_design(&id))?;
    let r = remove(&s, &project_dir(&s, &id), &name).await?;
    s.changed(format!("media:{id}"));
    Ok(r)
}

// ---------------------------------------------------------------- folders ----

/// Folders of the project list (one level), and the folder of each project.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Folders {
    #[serde(default)]
    pub folders: Vec<String>,
    /// Project id → folder name. Projects not listed are at the top level.
    #[serde(default)]
    pub assign: BTreeMap<String, String>,
}

static FOLDERS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn folders_file(s: &Shared) -> PathBuf {
    s.data_dir.join("folders.json")
}

pub async fn load_folders(s: &Shared) -> Folders {
    tokio::fs::read(folders_file(s))
        .await
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

async fn get_folders(State(s): State<Shared>, CurrentUser(_): CurrentUser) -> Json<Folders> {
    Json(load_folders(&s).await)
}

async fn put_folders(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Json(mut f): Json<Folders>,
) -> ApiResult<Json<Folders>> {
    require(me.can_create_projects())?;
    let _guard = FOLDERS.lock().await;
    let mut names = Vec::new();
    for n in f.folders.iter().map(|n| n.trim()) {
        if n.is_empty() || n.len() > 80 {
            return Err(bad("a folder name has 1 to 80 characters"));
        }
        if !names.iter().any(|x: &String| x == n) {
            names.push(n.to_string());
        }
    }
    f.folders = names;
    // Keep the folders of projects this user cannot see.
    let old = load_folders(&s).await;
    let mut assign: BTreeMap<String, String> = old
        .assign
        .into_iter()
        .filter(|(p, _)| !me.sees_project(p))
        .collect();
    assign.extend(f.assign.into_iter().filter(|(p, _)| me.sees_project(p)));
    assign.retain(|_, folder| f.folders.contains(folder));
    f.assign = assign;
    let json = serde_json::to_vec_pretty(&f).map_err(internal)?;
    tokio::fs::write(folders_file(&s), json)
        .await
        .map_err(internal)?;
    let _ = s.events.send(Event::Folders { folders: f.clone() });
    Ok(Json(f))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        assert_eq!(kind_of("a.PNG"), "image");
        assert_eq!(kind_of("clip.mov"), "video");
        assert_eq!(kind_of("Inter.woff2"), "font");
        assert_eq!(kind_of("readme"), "other");
    }

    #[test]
    fn a_zip_of_pngs_is_a_sequence_in_name_order() {
        use std::io::Write;
        // The pictures' bytes (read as they are).
        let png = |v: u8| vec![0x89, b'P', b'N', b'G', v];
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        for (name, v) in [
            ("seq/f10.png", 10),
            ("seq/f2.png", 2),
            ("seq/.hidden.png", 99),
            ("seq/notes.txt", 0),
        ] {
            z.start_file(name, opts).unwrap();
            z.write_all(&png(v)).unwrap();
        }
        let bytes = z.finish().unwrap().into_inner();
        let frames = zip_frames(&bytes).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], png(2));
        assert_eq!(natural("f2.png", "f10.png"), std::cmp::Ordering::Less);
    }
}

#[cfg(test)]
mod illustrator_tests {
    use super::*;

    /// A PDF of one page (a red square on a 200 × 100 artboard), as Illustrator saves inside an .ai.
    fn tiny_pdf() -> Vec<u8> {
        let content = "1 0 0 rg 50 25 50 50 re f";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Contents 4 0 R >>".to_string(),
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, o) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
        for o in offsets {
            out.extend(format!("{o:010} 00000 n \n").bytes());
        }
        out.extend(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .bytes(),
        );
        out
    }

    #[test]
    fn an_illustrator_file_becomes_svg_pictures() {
        assert_eq!(illustrator_stem("logo.AI"), Some("logo"));
        assert_eq!(illustrator_stem("logo.png"), None);
        assert_eq!(
            artboard_names("logo", 3),
            ["logo.svg", "logo-2.svg", "logo-3.svg"]
        );
        let svgs = illustrator_svgs(tiny_pdf()).unwrap();
        assert_eq!(svgs.len(), 1);
        assert!(
            svgs[0].starts_with("<svg") || svgs[0].contains("<svg"),
            "{}",
            &svgs[0][..80.min(svgs[0].len())]
        );
        assert!(
            svgs[0].contains("200") && svgs[0].contains("100"),
            "the artboard's size"
        );
        // Drawn by the renderer like any SVG picture: the square is red, the rest transparent.
        let tree = resvg::usvg::Tree::from_str(&svgs[0], &Default::default()).unwrap();
        let mut pm = tiny_skia::Pixmap::new(200, 100).unwrap();
        resvg::render(&tree, tiny_skia::Transform::identity(), &mut pm.as_mut());
        let px = pm.pixel(75, 50).unwrap();
        assert_eq!((px.red(), px.green(), px.alpha()), (255, 0, 255));
        assert_eq!(pm.pixel(10, 10).unwrap().alpha(), 0);
        // An old Illustrator file (PostScript only): a clear refusal.
        assert!(illustrator_svgs(b"%!PS-Adobe-3.0 EPSF".to_vec()).is_err());
    }
}
