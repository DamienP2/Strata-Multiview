//! Pictures at a web address (Image widget): fetched once into
//! `<assets>/.web/`, where the renderers read them (cg_render::url_cache_path).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Largest picture taken (bytes).
const MAX_BYTES: u64 = 40 << 20;
/// A failed address is tried again after this.
const RETRY: Duration = Duration::from_secs(10);

static BUSY: Mutex<Option<HashSet<String>>> = Mutex::new(None);
static FAILED: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

/// The renderers ask the server for the web pictures they draw.
pub fn install(assets_dir: PathBuf) {
    cg_render::set_url_fetcher(Box::new(move |url, again| fetch(&assets_dir, url, again)));
}

/// Fetches `url` in the background unless it is there, being fetched, or
/// just failed (`again`: a reload, fetched even when it is there).
pub fn fetch(assets_dir: &Path, url: &str, again: bool) {
    let path = cg_render::url_cache_path(assets_dir, url);
    {
        let mut failed = FAILED.lock().unwrap_or_else(|e| e.into_inner());
        let failed = failed.get_or_insert_with(HashMap::new);
        if !again && failed.get(url).is_some_and(|t| t.elapsed() < RETRY) {
            return;
        }
        failed.remove(url);
    }
    if !again && path.exists() {
        return;
    }
    let mut busy = BUSY.lock().unwrap_or_else(|e| e.into_inner());
    if !busy
        .get_or_insert_with(HashSet::new)
        .insert(url.to_string())
    {
        return;
    }
    drop(busy);
    if again {
        // Not drawn from the old copy while the new one comes.
        let _ = std::fs::remove_file(&path);
    }
    let key = url.to_string();
    let url = url.to_string();
    let spawned = std::thread::Builder::new()
        .name("cg-web-image".into())
        .spawn(move || {
            let got = download(&url).and_then(|data| {
                std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))
                    .map_err(|e| e.to_string())?;
                let tmp = path.with_extension("part");
                std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
                std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
            });
            if let Err(e) = got {
                tracing::warn!(url, error = e, "web picture not fetched");
                FAILED
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get_or_insert_with(HashMap::new)
                    .insert(url.clone(), Instant::now());
            }
            if let Some(b) = BUSY.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                b.remove(&url);
            }
        });
    if spawned.is_err() {
        if let Some(b) = BUSY.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            b.remove(&key);
        }
    }
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(Duration::from_secs(15)))
        .max_redirects(5)
        .http_status_as_error(false)
        .user_agent(concat!("strata-graphics/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let mut res = agent
        .get(url)
        .call()
        .map_err(|e| format!("cannot reach the address: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("the server answered {}", res.status()));
    }
    res.body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()
        .map_err(|e| format!("cannot read the answer: {e}"))
}
