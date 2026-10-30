//! After Effects import: reads what the Bodymovin / LottieFiles exporters
//! produce (`.json`, a `.zip` with `data.json` + `images/`, or a
//! `.lottie`) and converts it with [`cg_core::lottie`].
//!
//! `.mogrt` files are recognised to explain what to do: they wrap an After
//! Effects project (`.aep`), a closed binary format.

use std::io::Read;

use base64::Engine as _;
use cg_core::lottie::{self, ImageSource, Imported, Options};

/// Largest archive member read.
const MAX_ENTRY: u64 = 200 * 1024 * 1024;

pub struct Output {
    pub imported: Imported,
    /// Asset files to write (name, bytes).
    pub files: Vec<(String, Vec<u8>)>,
}

pub fn import(body: &[u8], opts: &Options) -> std::result::Result<Output, String> {
    let trimmed = body.trim_ascii_start();
    let trimmed = trimmed.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(trimmed);
    if trimmed.starts_with(b"{") {
        let json: serde_json::Value =
            serde_json::from_slice(trimmed).map_err(|e| format!("invalid JSON: {e}"))?;
        return finish(lottie::import(&json, opts)?, |_| None);
    }
    if !body.starts_with(b"PK") {
        return Err(
            "unsupported file: export the composition with Bodymovin / LottieFiles (.json, .zip or .lottie)"
                .into(),
        );
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(body))
        .map_err(|e| format!("invalid archive: {e}"))?;
    let names: Vec<String> = zip
        .file_names()
        .filter_map(|n| n.ok().map(|n| n.into_owned()))
        .collect();
    let mut read = |name: &str| -> Option<Vec<u8>> {
        let f = zip.by_name(name).ok()?;
        let mut out = Vec::new();
        f.take(MAX_ENTRY).read_to_end(&mut out).ok()?;
        Some(out)
    };
    // The animation: the dotLottie manifest's first animation, else the
    // first JSON with layers.
    let mut found = None;
    let mut candidates: Vec<&String> = names
        .iter()
        .filter(|n| n.ends_with(".json") && !n.ends_with("manifest.json"))
        .collect();
    candidates.sort_by_key(|n| {
        (
            !n.starts_with("animations/"),
            !n.ends_with("data.json"),
            n.len(),
        )
    });
    for n in candidates {
        let Some(bytes) = read(n) else { continue };
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        if json.get("layers").is_some() && json.get("v").is_some() {
            let dir = n
                .rsplit_once('/')
                .map(|(d, _)| format!("{d}/"))
                .unwrap_or_default();
            found = Some((json, dir));
            break;
        }
    }
    let Some((json, dir)) = found else {
        if names.iter().any(|n| n.ends_with("definition.json"))
            || names.iter().any(|n| n.ends_with(".aegraphic"))
        {
            return Err(MOGRT.into());
        }
        return Err("no Lottie / Bodymovin animation in this archive".into());
    };
    let imported = lottie::import(&json, opts)?;
    finish(imported, |path| {
        let path = path.trim_start_matches("./");
        // Paths are relative to the JSON (Bodymovin) or the archive root
        // (dotLottie keeps images in images/).
        let file = path.rsplit('/').next().unwrap_or(path);
        [
            format!("{dir}{path}"),
            path.to_string(),
            format!("images/{file}"),
            format!("{dir}images/{file}"),
        ]
        .iter()
        .find_map(|p| read(p))
    })
}

pub const MOGRT: &str = "This is a Motion Graphics template (.mogrt). It wraps an After Effects project (.aep), a closed binary format that cannot be read outside After Effects. Open the composition in After Effects and export it with the Bodymovin or LottieFiles extension (JSON, \"Glyphs\" off, images included), then import that file here.";

fn finish(
    imported: Imported,
    mut file: impl FnMut(&str) -> Option<Vec<u8>>,
) -> std::result::Result<Output, String> {
    let mut files = Vec::new();
    let mut imported = imported;
    for img in &imported.images {
        let bytes = match &img.source {
            ImageSource::DataUri(uri) => uri.split_once(',').and_then(|(_, b64)| {
                base64::engine::general_purpose::STANDARD
                    .decode(b64.trim())
                    .ok()
            }),
            ImageSource::Path(p) => file(p),
        };
        match bytes {
            Some(b) => files.push((img.file.clone(), b)),
            None => imported.warnings.push(format!(
                "image {} not found: upload it and pick it in the image layer",
                match &img.source {
                    ImageSource::Path(p) => p.as_str(),
                    ImageSource::DataUri(_) => "(embedded)",
                }
            )),
        }
    }
    Ok(Output { imported, files })
}
