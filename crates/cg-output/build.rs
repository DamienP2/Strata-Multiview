//! Builds the DeckLink shim (`decklink/shim.cpp`) when the `decklink`
//! feature is on, against the SDK headers vendored in `decklink-sdk/`.
//!
//! The shim only needs a C++ compiler (and, on Windows, MIDL from the
//! Windows SDK to turn `DeckLinkAPI.idl` into a header): the DeckLink
//! runtime itself is loaded at run time. When the shim cannot be built the
//! build goes on without it (with a warning) and the server reports SDI as
//! not built in; set `CG_DECKLINK_REQUIRED=1` to make that an error instead
//! (release builds).

fn main() {
    println!("cargo::rustc-check-cfg=cfg(decklink_shim)");
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(feature = "decklink")]
    decklink::build();
}

#[cfg(feature = "decklink")]
mod decklink {
    use std::env;
    use std::path::{Path, PathBuf};

    pub fn build() {
        println!("cargo:rerun-if-changed=decklink");
        println!("cargo:rerun-if-changed=decklink-sdk");
        println!("cargo:rerun-if-env-changed=CG_DECKLINK_REQUIRED");
        println!("cargo:rerun-if-env-changed=MIDL");
        match try_build() {
            Ok(()) => println!("cargo:rustc-cfg=decklink_shim"),
            Err(e) => {
                let required =
                    env::var("CG_DECKLINK_REQUIRED").is_ok_and(|v| !v.is_empty() && v != "0");
                if required {
                    panic!("cannot build the DeckLink shim: {e}");
                }
                println!("cargo:warning=DeckLink SDI support left out of this build: {e}");
            }
        }
    }

    fn try_build() -> Result<(), String> {
        let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
        let sdk = root.join("decklink-sdk");
        let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
        let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
        if target_env == "musl" {
            // Static musl binaries cannot dlopen the (glibc) DeckLink runtime.
            return Err(
                "static musl builds cannot load the DeckLink runtime (build for a -gnu target)"
                    .into(),
            );
        }
        let mut b = cc::Build::new();
        b.cpp(true).file(root.join("decklink/shim.cpp"));
        if target_env == "msvc" {
            b.flag("/EHsc");
        } else {
            b.flag("-std=c++11");
        }
        match os.as_str() {
            "linux" => {
                b.include(sdk.join("linux"))
                    .file(sdk.join("linux/DeckLinkAPIDispatch.cpp"));
                b.try_compile("cg_decklink").map_err(|e| e.to_string())?;
                println!("cargo:rustc-link-lib=dl");
                println!("cargo:rustc-link-lib=pthread");
            }
            "macos" => {
                b.include(sdk.join("mac"))
                    .file(sdk.join("mac/DeckLinkAPIDispatch.cpp"));
                b.try_compile("cg_decklink").map_err(|e| e.to_string())?;
                println!("cargo:rustc-link-lib=framework=CoreFoundation");
            }
            "windows" => {
                if target_env != "msvc" {
                    return Err("the Windows build needs the MSVC toolchain (MIDL)".into());
                }
                let out = PathBuf::from(env::var("OUT_DIR").map_err(|e| e.to_string())?);
                let gen = out.join("decklink-idl");
                std::fs::create_dir_all(&gen).map_err(|e| e.to_string())?;
                midl(&sdk.join("win"), &gen)?;
                b.include(&gen).include(sdk.join("win"));
                b.try_compile("cg_decklink").map_err(|e| e.to_string())?;
                println!("cargo:rustc-link-lib=ole32");
                println!("cargo:rustc-link-lib=oleaut32");
            }
            other => return Err(format!("no DeckLink SDK for target OS `{other}`")),
        }
        Ok(())
    }

    /// Generates `DeckLinkAPI.h` and `DeckLinkAPI_i.c` from the IDL.
    fn midl(idl_dir: &Path, out: &Path) -> Result<(), String> {
        let target = env::var("TARGET").map_err(|e| e.to_string())?;
        let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
        let midl_env = match arch.as_str() {
            "x86_64" => "x64",
            "aarch64" => "arm64",
            "x86" => "win32",
            other => return Err(format!("no MIDL environment for `{other}`")),
        };
        // MIDL preprocesses with cl.exe: give it the MSVC environment.
        let cl = cc::windows_registry::find_tool(&target, "cl.exe");
        let mut cl_env: Vec<(std::ffi::OsString, std::ffi::OsString)> = Vec::new();
        if let Some(t) = &cl {
            cl_env = t
                .get_envs()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect();
        }
        let exe = find_midl(&cl_env)
            .ok_or("MIDL (midl.exe, Windows SDK) not found; set MIDL to its path")?;
        let mut cmd = std::process::Command::new(&exe);
        for (k, v) in &cl_env {
            cmd.env(k, v);
        }
        if let Some(t) = &cl {
            // Make sure cl.exe itself is found.
            if let Some(dir) = t.path().parent() {
                let path = env::var_os("PATH").unwrap_or_default();
                let mut dirs = vec![dir.to_path_buf()];
                if let Some((_, p)) = cl_env.iter().find(|(k, _)| k.eq_ignore_ascii_case("PATH")) {
                    dirs.extend(env::split_paths(p));
                }
                dirs.extend(env::split_paths(&path));
                if let Ok(joined) = env::join_paths(dirs) {
                    cmd.env("PATH", joined);
                }
            }
        }
        cmd.current_dir(idl_dir)
            .args([
                "/nologo",
                "/env",
                midl_env,
                "/h",
                "DeckLinkAPI.h",
                "/iid",
                "DeckLinkAPI_i.c",
            ])
            .args([
                "/proxy",
                "DeckLinkAPI_p.c",
                "/dlldata",
                "dlldata.c",
                "/tlb",
                "DeckLinkAPI.tlb",
            ])
            .arg("/out")
            .arg(out)
            .arg("/I")
            .arg(idl_dir)
            .arg("DeckLinkAPI.idl");
        let output = cmd
            .output()
            .map_err(|e| format!("cannot run {}: {e}", exe.display()))?;
        if !output.status.success() {
            return Err(format!(
                "MIDL failed: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    }

    fn find_midl(cl_env: &[(std::ffi::OsString, std::ffi::OsString)]) -> Option<PathBuf> {
        if let Some(p) = env::var_os("MIDL") {
            return Some(PathBuf::from(p));
        }
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some((_, p)) = cl_env.iter().find(|(k, _)| k.eq_ignore_ascii_case("PATH")) {
            dirs.extend(env::split_paths(p));
        }
        if let Some(p) = env::var_os("PATH") {
            dirs.extend(env::split_paths(&p));
        }
        if let Some(found) = dirs
            .iter()
            .map(|d| d.join("midl.exe"))
            .find(|p| p.is_file())
        {
            return Some(found);
        }
        // Windows Kits: bin\<version>\<host>\midl.exe, newest version first.
        let host = if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "x64"
        };
        let mut roots: Vec<PathBuf> = Vec::new();
        if let Some(p) = env::var_os("WindowsSdkDir") {
            roots.push(PathBuf::from(p));
        }
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(p) = env::var_os(var) {
                roots.push(Path::new(&p).join("Windows Kits").join("10"));
            }
        }
        for root in roots {
            let Ok(entries) = std::fs::read_dir(root.join("bin")) else {
                continue;
            };
            let mut versions: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("10."))
                })
                .collect();
            versions.sort_by_key(|p| version_key(p));
            for v in versions.iter().rev() {
                let exe = v.join(host).join("midl.exe");
                if exe.is_file() {
                    return Some(exe);
                }
            }
            let exe = root.join("bin").join(host).join("midl.exe");
            if exe.is_file() {
                return Some(exe);
            }
        }
        None
    }

    fn version_key(p: &Path) -> Vec<u32> {
        p.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .split('.')
            .map(|s| s.parse().unwrap_or(0))
            .collect()
    }
}
