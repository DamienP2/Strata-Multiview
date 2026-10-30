//! Platform-specific default locations.

use std::path::{Path, PathBuf};

pub const SERVICE_NAME: &str = "strata-graphics";

/// Data directory for an interactive run (per user).
pub fn user_data_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "StrataGraphics", "Strata Graphics")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("data"))
}

/// Data directory used by the system service.
pub fn system_data_dir() -> PathBuf {
    if cfg!(windows) {
        program_data().join("Strata Graphics")
    } else if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Application Support/Strata Graphics")
    } else {
        PathBuf::from("/var/lib/strata-graphics")
    }
}

fn program_data() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
}

/// Where the same data lived before the product was renamed.
fn former(dir: &Path) -> Option<PathBuf> {
    if dir == system_data_dir() {
        Some(if cfg!(windows) {
            program_data().join("PulseCG")
        } else if cfg!(target_os = "macos") {
            PathBuf::from("/Library/Application Support/PulseCG")
        } else {
            PathBuf::from("/var/lib/pulse-cg")
        })
    } else if dir == user_data_dir() {
        directories::ProjectDirs::from("com", "PulseCG", "Pulse CG")
            .map(|d| d.data_dir().to_path_buf())
    } else {
        None
    }
}

/// Moves the data of an install from before the rename to its new place,
/// once: projects, users and settings carry over.
pub fn adopt_former_data(dir: &Path) {
    if dir.exists() {
        return;
    }
    let Some(old) = former(dir).filter(|d| d.is_dir()) else {
        return;
    };
    if let Some(parent) = dir.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::rename(&old, dir) {
        Ok(()) => eprintln!("data moved from {} to {}", old.display(), dir.display()),
        Err(e) => eprintln!("cannot move {} to {}: {e}", old.display(), dir.display()),
    }
}
