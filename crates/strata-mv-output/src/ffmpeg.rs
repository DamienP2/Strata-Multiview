//! Where ffmpeg is: video files, streams in and streams out go through it.
//!
//! `FFMPEG_PATH` first, then an ffmpeg shipped next to the server, then the
//! one on the PATH.

use std::path::PathBuf;
use std::process::Command;

pub fn path() -> PathBuf {
    if let Ok(p) = std::env::var("FFMPEG_PATH") {
        if !p.trim().is_empty() {
            return PathBuf::from(p.trim());
        }
    }
    let name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    // The real place of the server (not a /usr/local/bin link to it).
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| std::fs::canonicalize(&e).ok().or(Some(e)))
        .and_then(|e| e.parent().map(|d| d.to_path_buf()))
    {
        for p in [dir.join(name), dir.join("bin").join(name)] {
            if p.is_file() {
                return p;
            }
        }
    }
    PathBuf::from(name)
}

/// A new ffmpeg command, quiet and without a console window on Windows.
pub fn command() -> Command {
    let mut c = Command::new(path());
    c.args(["-hide_banner", "-nostdin", "-loglevel", "error"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW
        c.creation_flags(0x0800_0000);
    }
    c
}

/// The last lines ffmpeg wrote (its error), for the status of an output or input.
pub fn tail(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = lines[lines.len().saturating_sub(3)..].join(" · ");
    if tail.is_empty() {
        "ffmpeg stopped".into()
    } else {
        tail.chars().take(300).collect()
    }
}
