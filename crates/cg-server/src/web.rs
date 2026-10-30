//! Web app and fonts served from inside the binary (single-file install).

use axum::extract::{Path, Request};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
struct WebApp;

const NOT_BUILT: &str = "<!doctype html><meta charset=utf-8><title>Strata</title>\
<body style=\"font:15px system-ui;background:#0b0c12;color:#e6e8f2;padding:40px\">\
<h2>Web app not built</h2><p>Run <code>cd web &amp;&amp; npm install &amp;&amp; npm run build</code> \
then rebuild the server, or start it with <code>--web-dir web/dist</code>.</p>";

/// A file shipped in the web app (`iconlib/…`), from the folder given by
/// `--web-dir` when there is one, else from the binary.
pub fn bundled(path: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
    if let Some(dir) = WEB_DIR.get() {
        if let Ok(b) = std::fs::read(dir.join(path)) {
            return Some(std::borrow::Cow::Owned(b));
        }
    }
    WebApp::get(path).map(|f| f.data)
}

/// The folder of `--web-dir`.
pub static WEB_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Gzipped copies of the text files of the web app (made by build.rs).
const GZIPPED: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/web_gz.rs"));

fn gzipped(path: &str) -> Option<&'static [u8]> {
    GZIPPED.iter().find(|(p, _)| *p == path).map(|(_, d)| *d)
}

/// Whether the client takes gzip (`Accept-Encoding`, not refused by `q=0`).
fn takes_gzip(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .any(|item| {
            let mut parts = item.split(';');
            let name = parts.next().unwrap_or("").trim();
            let refused = parts.any(|p| {
                p.trim()
                    .strip_prefix("q=")
                    .and_then(|q| q.trim().parse::<f32>().ok())
                    .is_some_and(|q| q <= 0.0)
            });
            (name.eq_ignore_ascii_case("gzip") || name == "*") && !refused
        })
}

/// Serves embedded files, falling back to `index.html` for client routes.
pub async fn embedded(uri: Uri, headers: HeaderMap) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = match WebApp::get(path).filter(|_| !path.is_empty()) {
        Some(_) => path,
        None => "index.html",
    };
    let Some(file) = WebApp::get(path) else {
        return (StatusCode::SERVICE_UNAVAILABLE, Html(NOT_BUILT)).into_response();
    };
    // Vite fingerprints everything under static/.
    let cache = if path.starts_with("static/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    let kind = file.metadata.mimetype().to_string();
    let mut res = match gzipped(path).filter(|_| takes_gzip(&headers)) {
        Some(gz) => (
            [
                (header::CONTENT_TYPE, kind),
                (header::CONTENT_ENCODING, "gzip".to_string()),
            ],
            gz,
        )
            .into_response(),
        None => ([(header::CONTENT_TYPE, kind)], file.data).into_response(),
    };
    let h = res.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    if gzipped(path).is_some() {
        h.insert(header::VARY, HeaderValue::from_static("accept-encoding"));
    }
    res
}

/// Types worth compressing (JSON of the API, text, fonts); pictures,
/// video and archives are compressed already.
fn compressible(kind: &str) -> bool {
    let kind = kind.split(';').next().unwrap_or("").trim();
    kind == "application/json"
        || kind.starts_with("text/")
        || kind == "application/javascript"
        || kind == "image/svg+xml"
        || kind == "application/xml"
        || kind == "font/ttf"
        || kind == "font/otf"
}

/// Gzips the answers worth it (JSON of the API, text, fonts) for clients
/// that take it. Bodies of unknown length (streams, files of the media
/// folders), the WebSocket and what is compressed already pass as they are.
pub async fn compress(req: Request, next: Next) -> Response {
    let gzip = takes_gzip(req.headers());
    let res = next.run(req).await;
    if !gzip
        || res.status() != StatusCode::OK
        || res.headers().contains_key(header::CONTENT_ENCODING)
        || !res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(compressible)
    {
        return res;
    }
    use axum::body::HttpBody;
    let size = res.body().size_hint().exact().unwrap_or(0);
    if !(1024..=64 << 20).contains(&size) {
        return res;
    }
    let (mut parts, body) = res.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, 64 << 20).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let gz = |data: &[u8]| {
        use std::io::Write;
        // Fast on big answers (a font), smaller on the usual JSON.
        let level = if data.len() > 256 << 10 { 1 } else { 6 };
        let mut enc = flate2::write::GzEncoder::new(
            Vec::with_capacity(data.len() / 4),
            flate2::Compression::new(level),
        );
        enc.write_all(data).and_then(|_| enc.finish())
    };
    let packed = if bytes.len() > 256 << 10 {
        let b = bytes.clone();
        tokio::task::spawn_blocking(move || gz(&b))
            .await
            .unwrap_or_else(|e| Err(std::io::Error::other(e)))
    } else {
        gz(&bytes)
    };
    let h = &mut parts.headers;
    h.append(header::VARY, HeaderValue::from_static("accept-encoding"));
    match packed {
        Ok(packed) if packed.len() < bytes.len() => {
            h.remove(header::CONTENT_LENGTH);
            h.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
            Response::from_parts(parts, axum::body::Body::from(packed))
        }
        _ => Response::from_parts(parts, axum::body::Body::from(bytes)),
    }
}

pub async fn font(Path(name): Path<String>) -> Response {
    match cg_render::BUNDLED_FONTS.iter().find(|(n, _)| *n == name) {
        Some((_, data)) => (
            [
                (header::CONTENT_TYPE, "font/otf"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            *data,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gzip_when_the_client_takes_it() {
        let with = |v: &str| {
            let mut h = HeaderMap::new();
            h.insert(header::ACCEPT_ENCODING, HeaderValue::from_str(v).unwrap());
            takes_gzip(&h)
        };
        assert!(with("gzip, deflate, br"));
        assert!(with("br;q=1.0, gzip;q=0.8"));
        assert!(!with("br, gzip;q=0"));
        assert!(!with("identity"));
        assert!(!takes_gzip(&HeaderMap::new()));
        assert!(compressible("application/json") && compressible("text/css; charset=utf-8"));
        assert!(!compressible("image/png") && !compressible("video/mp4"));
    }
}
