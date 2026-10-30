//! Strata desktop companion.
//!
//! A small menu bar (macOS) / notification area (Windows) app installed next
//! to the server: it shows whether the server runs, opens the Designer,
//! Operator and Settings pages, and starts / stops the background service.
//! All the configuration itself lives in the web Settings page.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(any(target_os = "macos", windows))]
mod app;

#[cfg(any(target_os = "macos", windows))]
fn main() {
    app::run();
}

#[cfg(not(any(target_os = "macos", windows)))]
fn main() {
    eprintln!("The Strata tray app is available on macOS and Windows; on Linux use `strata-server service status` and the web UI.");
}
