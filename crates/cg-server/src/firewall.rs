//! macOS application firewall: it blocks incoming connections to a program it
//! does not know, silently for a service (no one sees the prompt). Each
//! unsigned update is a new program to it.
//!
//! `socketfilterfw --add` signs an unsigned program in place: it must never
//! touch the file of the running server (macOS kills a process whose code
//! changes under it). It is run by the installer before the service starts,
//! and by an update on the new file once swapped in — the running process
//! keeps its old image.

#[cfg(target_os = "macos")]
pub fn allow(exe: &std::path::Path) {
    use std::process::Command;
    const FW: &str = "/usr/libexec/ApplicationFirewall/socketfilterfw";
    // SAFETY: geteuid has no preconditions and cannot fail.
    if unsafe { libc::geteuid() } != 0 {
        return;
    }
    let path = exe.to_string_lossy();
    let run = |args: &[&str]| {
        Command::new(FW)
            .args(args)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    };
    let Ok(state) = run(&["--getglobalstate"]) else {
        return;
    };
    // "Firewall is disabled. (State = 0)": nothing to do.
    if state.contains("State = 0") {
        return;
    }
    let added = run(&["--add", &path]);
    let unblocked = run(&["--unblockapp", &path]);
    match (added, unblocked) {
        (Ok(_), Ok(_)) => tracing::info!(exe = %path, "allowed through the macOS firewall"),
        (a, b) => {
            tracing::warn!(exe = %path, add = ?a.err(), unblock = ?b.err(), "cannot allow the server through the macOS firewall")
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn allow(_exe: &std::path::Path) {}
