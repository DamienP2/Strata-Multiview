//! Platform-specific default locations.

use std::path::PathBuf;

pub const SERVICE_NAME: &str = "strata-multiview";

/// Data directory for an interactive run (per user).
pub fn user_data_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "StrataMultiview", "Strata Multiview")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("data"))
}

/// Data directory used by the system service.
pub fn system_data_dir() -> PathBuf {
    if cfg!(windows) {
        program_data().join("Strata Multiview")
    } else if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Application Support/Strata Multiview")
    } else {
        PathBuf::from("/var/lib/strata-multiview")
    }
}

fn program_data() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
}
