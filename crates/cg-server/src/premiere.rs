//! The Premiere Pro panel, served from inside the binary.
//!
//! The extension installed in Premiere is only a loader: it fetches the
//! panel from here at every start, so the panel follows the server's
//! version. `extension.json` carries the whole extension for the loader to
//! update itself, `strata-premiere.zip` the same for a manual install.

use std::io::Write;

use axum::extract::Path;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use base64::Engine;
use rust_embed::RustEmbed;
use sha2::{Digest, Sha256};

#[derive(RustEmbed)]
#[folder = "../../integrations/premiere"]
#[exclude = "README.md"]
struct Panel;

/// What the loader installed in Premiere is made of.
fn is_extension(path: &str) -> bool {
    !path.starts_with("install/")
}

pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/premiere/manifest.json", get(manifest))
        .route("/premiere/extension.json", get(extension))
        .route("/premiere/strata-premiere.zip", get(download))
        .route("/premiere/{*path}", get(file))
}

/// `LOADER_VERSION` of loader.js: the loader updates itself when it is behind.
fn loader_version() -> u32 {
    Panel::get("loader.js")
        .and_then(|f| {
            let text = String::from_utf8_lossy(&f.data).into_owned();
            let rest = text.split("LOADER_VERSION = ").nth(1)?.to_string();
            rest.split(';').next()?.trim().parse().ok()
        })
        .unwrap_or(0)
}

fn ui_hash() -> String {
    let mut h = Sha256::new();
    for path in Panel::iter() {
        if let Some(f) = Panel::get(&path) {
            h.update(path.as_bytes());
            h.update(f.metadata.sha256_hash());
        }
    }
    hex(&h.finalize()[..6])
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn no_cache<T: IntoResponse>(body: T) -> Response {
    ([(header::CACHE_CONTROL, "no-cache")], body).into_response()
}

async fn manifest() -> Response {
    no_cache(Json(serde_json::json!({
        "version": format!("{}+{}", env!("CARGO_PKG_VERSION"), ui_hash()),
        "app": env!("CARGO_PKG_VERSION"),
        "loader": loader_version(),
    })))
}

async fn file(Path(path): Path<String>) -> Response {
    match Panel::get(&path).filter(|_| is_extension(&path)) {
        Some(f) => no_cache((
            [(header::CONTENT_TYPE, f.metadata.mimetype().to_string())],
            f.data,
        )),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn extension() -> Response {
    let b64 = base64::engine::general_purpose::STANDARD;
    let files: serde_json::Map<String, serde_json::Value> = Panel::iter()
        .filter(|p| is_extension(p))
        .filter_map(|p| Panel::get(&p).map(|f| (p.to_string(), b64.encode(&f.data).into())))
        .collect();
    no_cache(Json(
        serde_json::json!({ "loader": loader_version(), "files": files }),
    ))
}

/// The extension in its folder, with the install scripts, for a manual install.
fn zip_bytes(server: Option<&str>) -> zip::result::ZipResult<Vec<u8>> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    for path in Panel::iter() {
        let Some(f) = Panel::get(&path) else { continue };
        let name = match path.strip_prefix("install/") {
            Some(script) => script.to_string(),
            None => format!("io.stratagraphics.premiere/{path}"),
        };
        let opts = if name.ends_with(".sh") {
            opts.unix_permissions(0o755)
        } else {
            opts
        };
        z.start_file(name, opts)?;
        z.write_all(&f.data)?;
    }
    if let Some(server) = server {
        z.start_file("io.stratagraphics.premiere/strata.ini", opts)?;
        writeln!(z, "[strata]\nserver={server}")?;
    }
    Ok(z.finish()?.into_inner())
}

async fn download(headers: HeaderMap) -> Response {
    // The panel starts on this server.
    let server = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(|host| format!("http://{host}"));
    match zip_bytes(server.as_deref()) {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "application/zip"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"strata-premiere.zip\"",
                ),
            ],
            bytes,
        )
            .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_the_panel() {
        assert!(loader_version() >= 1);
        for f in [
            "index.html",
            "loader.js",
            "CSXS/manifest.xml",
            "ui/panel.js",
            "ui/panel.html",
            "ui/panel.css",
            "host/premiere.jsx",
        ] {
            assert!(Panel::get(f).is_some(), "{f}");
        }
        let zip = zip::ZipArchive::new(std::io::Cursor::new(
            zip_bytes(Some("http://cg:8080")).unwrap(),
        ))
        .unwrap();
        let names: Vec<String> = zip
            .file_names()
            .filter_map(|n| n.ok())
            .map(|n| n.to_string())
            .collect();
        assert!(names
            .iter()
            .any(|n| n == "io.stratagraphics.premiere/CSXS/manifest.xml"));
        assert!(names.iter().any(|n| n == "install-mac.sh"));
        assert!(names
            .iter()
            .any(|n| n == "io.stratagraphics.premiere/strata.ini"));
    }
}
