use std::path::Path;

fn main() {
    // The web app (web/dist) is embedded into the binary. Make sure the folder
    // exists so the server still compiles before `npm run build`; it then serves
    // a page explaining how to build the UI.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dist = dir.join("../../web/dist");
    if !dist.exists() {
        let _ = std::fs::create_dir_all(&dist);
    }
    println!("cargo:rerun-if-changed=../../web/dist");
    gzipped(&dist);
    examples(dir);
    icon();
}

/// The Strata Graphics icon and name on the Windows executable (Task Manager, services).
fn icon() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icons/strata.ico");
        res.set("FileDescription", "Strata Graphics Server");
        res.set("ProductName", "Strata Graphics");
        if let Err(e) = res.compile() {
            println!("cargo:warning=cannot embed the icon: {e}");
        }
    }
    println!("cargo:rerun-if-changed=../../assets/icons/strata.ico");
}

/// Gzipped copies of the web app's text files (scripts, styles, pages,
/// SVG…), made once here at the best level: src/web.rs serves them to
/// browsers that take gzip, without compressing on every request.
fn gzipped(dist: &Path) {
    use std::io::Write;
    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.push(p);
            }
        }
    }
    let gz_dir = Path::new(&std::env::var("OUT_DIR").unwrap()).join("web-gz");
    let _ = std::fs::remove_dir_all(&gz_dir);
    let mut files = Vec::new();
    walk(dist, &mut files);
    files.sort();
    let mut out = String::from("&[\n");
    for f in &files {
        let Ok(rel) = f.strip_prefix(dist) else {
            continue;
        };
        let name = rel.to_string_lossy().replace('\\', "/");
        let ext = f.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(
            ext,
            "js" | "mjs" | "css" | "html" | "svg" | "json" | "txt" | "map" | "ico" | "xml"
        ) {
            continue;
        }
        let Ok(data) = std::fs::read(f) else {
            continue;
        };
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        let gz = enc.write_all(&data).and_then(|_| enc.finish());
        let Ok(gz) = gz else {
            continue;
        };
        // Not worth it for a file that hardly shrinks.
        if data.len() < 512 || gz.len() * 10 > data.len() * 9 {
            continue;
        }
        let dest = gz_dir.join(format!("{name}.gz"));
        if let Some(parent) = dest.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::write(&dest, gz).is_ok() {
            out.push_str(&format!(
                "    ({name:?}, include_bytes!({:?})),\n",
                dest.display().to_string()
            ));
        }
    }
    out.push_str("]\n");
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("web_gz.rs");
    std::fs::write(dest, out).expect("gzipped web app list written");
}

/// The example projects: every `examples/ex-*.json` is embedded, in name
/// order (src/examples.rs includes the list written here).
fn examples(dir: &Path) {
    let src = dir.join("examples");
    println!("cargo:rerun-if-changed=examples");
    let mut files: Vec<_> = std::fs::read_dir(&src)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    name.starts_with("ex-") && name.ends_with(".json")
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    let mut out = String::from("&[\n");
    for f in &files {
        println!("cargo:rerun-if-changed={}", f.display());
        out.push_str(&format!(
            "    include_str!({:?}),\n",
            f.display().to_string()
        ));
    }
    out.push_str("]\n");
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("examples.rs");
    std::fs::write(dest, out).expect("examples list written");
}
