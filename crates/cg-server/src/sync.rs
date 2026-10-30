//! Clock synchronisation status, for the Hardware page and the Performance
//! warnings: is the system clock kept on time (NTP), and is it locked to a
//! PTP grandmaster (linuxptp's ptp4l + phc2sys)?
//!
//! The server does not discipline clocks itself: the operating system does
//! (chrony / systemd-timesyncd / w32time / timed for NTP, ptp4l and phc2sys
//! for PTP). This only reads their state, every 10 s, with their own tools.

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub ntp: NtpStatus,
    pub ptp: PtpStatus,
    /// When it was read (ms since the Unix epoch).
    pub checked_at: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NtpStatus {
    /// The tool that told (`chronyc`, `timedatectl`, `w32tm`…); none: unknown.
    pub tool: Option<String>,
    pub synced: Option<bool>,
    /// NTP is on (None: unknown).
    pub enabled: Option<bool>,
    /// Time server.
    pub source: Option<String>,
    /// System clock − NTP time, in ms.
    pub offset_ms: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtpStatus {
    /// linuxptp's `pmc` answered (ptp4l runs).
    pub running: bool,
    /// Why not, when it does not.
    pub message: Option<String>,
    pub domain: u8,
    /// `SLAVE`, `MASTER`, `LISTENING`, `FAULTY`…
    pub port_state: Option<String>,
    pub grandmaster: Option<String>,
    /// Offset from the master, in ns.
    pub offset_ns: Option<f64>,
    /// Standard deviation of the last offsets read, in ns.
    pub jitter_ns: Option<f64>,
    /// Following a grandmaster, within 10 µs.
    pub locked: bool,
    /// phc2sys runs: PTP sets the system clock (NTP must not).
    pub sets_clock: bool,
}

/// The status, read again in the background.
pub struct Watch {
    status: Mutex<SyncStatus>,
    domain: AtomicU8,
    /// The last PTP offsets read (for the jitter).
    offsets: Mutex<std::collections::VecDeque<f64>>,
}

impl Default for Watch {
    fn default() -> Self {
        Self {
            status: Mutex::default(),
            domain: AtomicU8::new(127),
            offsets: Mutex::default(),
        }
    }
}

/// Standard deviation of the samples (None under 3).
fn spread(samples: &std::collections::VecDeque<f64>) -> Option<f64> {
    if samples.len() < 3 {
        return None;
    }
    let n = samples.len() as f64;
    let mean = samples.iter().sum::<f64>() / n;
    Some((samples.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt())
}

impl Watch {
    pub fn get(&self) -> SyncStatus {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The PTP domain of the settings.
    pub fn set_domain(&self, domain: u8) {
        self.domain.store(domain, Ordering::Relaxed);
    }

    fn check(&self) {
        let domain = self.domain.load(Ordering::Relaxed);
        let mut ptp = ptp(domain);
        {
            let mut offsets = self.offsets.lock().unwrap_or_else(|e| e.into_inner());
            match ptp.offset_ns {
                Some(o) => {
                    offsets.push_back(o);
                    while offsets.len() > 12 {
                        offsets.pop_front();
                    }
                }
                None => offsets.clear(),
            }
            ptp.jitter_ns = spread(&offsets);
        }
        let status = SyncStatus {
            ntp: ntp(),
            ptp,
            checked_at: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64),
        };
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = status;
    }
}

pub fn spawn(watch: Arc<Watch>) {
    let spawned = std::thread::Builder::new()
        .name("cg-sync".into())
        .spawn(move || loop {
            watch.check();
            std::thread::sleep(Duration::from_secs(10));
        });
    if let Err(e) = spawned {
        tracing::warn!("cannot watch the clock synchronisation: {e}");
    }
}

/// Runs a tool, its output if it ends well within 3 s.
fn output(program: &str, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < Duration::from_secs(3) => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let out = child.wait_with_output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `key : value` / `key: value` lines.
fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        (k.trim() == key).then(|| v.trim())
    })
}

fn ntp() -> NtpStatus {
    match std::env::consts::OS {
        "linux" => output("chronyc", &["-n", "tracking"])
            .map(|t| chrony(&t))
            .or_else(|| {
                let t = output(
                    "timedatectl",
                    &["show", "-p", "NTPSynchronized", "-p", "NTP"],
                )?;
                let mut s = timedatectl(&t);
                s.source = output(
                    "timedatectl",
                    &["show-timesync", "-p", "ServerName", "--value"],
                )
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty());
                Some(s)
            })
            .unwrap_or_default(),
        "windows" => output("w32tm", &["/query", "/status", "/verbose"])
            .map(|t| w32tm(&t))
            .unwrap_or_default(),
        "macos" => {
            // Needs administrator rights: unknown otherwise.
            let on = output("/usr/sbin/systemsetup", &["-getusingnetworktime"]).and_then(|t| {
                if t.contains("Network Time: On") {
                    Some(true)
                } else if t.contains("Network Time: Off") {
                    Some(false)
                } else {
                    None
                }
            });
            NtpStatus {
                tool: Some("systemsetup".into()),
                synced: on,
                enabled: on,
                source: output("/usr/sbin/systemsetup", &["-getnetworktimeserver"])
                    .and_then(|t| Some(t.split_once("Network Time Server:")?.1.trim().to_string())),
                offset_ms: None,
            }
        }
        _ => NtpStatus::default(),
    }
}

fn chrony(t: &str) -> NtpStatus {
    // "System time     : 0.000012345 seconds fast of NTP time"
    let offset_ms = field(t, "System time").and_then(|v| {
        let mut w = v.split_whitespace();
        let secs: f64 = w.next()?.parse().ok()?;
        let sign = if w.nth(1) == Some("slow") { -1.0 } else { 1.0 };
        Some(sign * secs * 1000.0)
    });
    // "Reference ID    : C0A80001 (192.168.0.1)"
    let source = field(t, "Reference ID").map(|v| {
        v.split_once('(')
            .map_or(v, |(_, r)| r.trim_end_matches(')'))
            .to_string()
    });
    let leap = field(t, "Leap status").unwrap_or("");
    NtpStatus {
        tool: Some("chronyc".into()),
        synced: Some(!leap.is_empty() && leap != "Not synchronised"),
        enabled: Some(true),
        source,
        offset_ms,
    }
}

fn timedatectl(t: &str) -> NtpStatus {
    let value = |k: &str| {
        t.lines()
            .find_map(|l| l.strip_prefix(k)?.strip_prefix('='))
            .map(str::trim)
    };
    NtpStatus {
        tool: Some("timedatectl".into()),
        synced: value("NTPSynchronized").map(|v| v == "yes"),
        enabled: value("NTP").map(|v| v == "yes"),
        source: None,
        offset_ms: None,
    }
}

fn w32tm(t: &str) -> NtpStatus {
    // "Leap Indicator: 3(not synchronized)", "Phase Offset: 0.0012345s"
    let leap = field(t, "Leap Indicator").unwrap_or("");
    let offset_ms = field(t, "Phase Offset")
        .and_then(|v| v.trim_end_matches('s').parse::<f64>().ok())
        .map(|s| s * 1000.0);
    let source = field(t, "Source").map(|s| s.trim_end_matches(",0x9").to_string());
    NtpStatus {
        tool: Some("w32tm".into()),
        synced: (!leap.is_empty()).then(|| !leap.starts_with('3')),
        enabled: source.as_deref().map(|s| !s.contains("Local CMOS Clock")),
        source,
        offset_ms,
    }
}

fn ptp(domain: u8) -> PtpStatus {
    let none = |message: &str| PtpStatus {
        domain,
        message: Some(message.into()),
        ..PtpStatus::default()
    };
    if std::env::consts::OS != "linux" {
        return none("PTP status is read from linuxptp, on Linux only");
    }
    let d = domain.to_string();
    let Some(t) = output(
        "pmc",
        &[
            "-u",
            "-b",
            "0",
            "-d",
            &d,
            "GET TIME_STATUS_NP",
            "GET PORT_DATA_SET",
        ],
    ) else {
        return none("ptp4l does not answer (is linuxptp installed and running?)");
    };
    let mut s = pmc(&t);
    s.domain = domain;
    s.sets_clock = output("pgrep", &["-x", "phc2sys"]).is_some();
    s
}

/// The answers of `pmc` (one `key value` per line).
fn pmc(t: &str) -> PtpStatus {
    let value = |k: &str| {
        t.lines().find_map(|l| {
            let mut w = l.split_whitespace();
            (w.next() == Some(k)).then(|| w.next()).flatten()
        })
    };
    if value("master_offset").is_none() && value("portState").is_none() {
        return PtpStatus {
            message: Some("ptp4l does not answer (is linuxptp installed and running?)".into()),
            ..PtpStatus::default()
        };
    }
    let offset_ns = value("master_offset").and_then(|v| v.parse().ok());
    let gm_present = value("gmPresent") == Some("true");
    let port_state = value("portState").map(str::to_string);
    let locked = gm_present
        && port_state.as_deref() == Some("SLAVE")
        && offset_ns.is_some_and(|o: f64| o.abs() < 10_000.0);
    PtpStatus {
        running: true,
        message: None,
        domain: 0,
        grandmaster: value("gmIdentity").map(str::to_string),
        port_state,
        offset_ns,
        jitter_ns: None,
        locked,
        sets_clock: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_is_the_spread_of_the_offsets() {
        let mut v: std::collections::VecDeque<f64> = [10.0, -10.0].into_iter().collect();
        assert_eq!(spread(&v), None);
        v.push_back(10.0);
        v.push_back(-10.0);
        assert_eq!(spread(&v), Some(10.0));
    }

    #[test]
    fn chrony_tracking() {
        let t = "Reference ID    : C0A80001 (192.168.0.1)\n\
                 Stratum         : 3\n\
                 System time     : 0.000250000 seconds slow of NTP time\n\
                 Leap status     : Normal\n";
        let s = chrony(t);
        assert_eq!(s.synced, Some(true));
        assert_eq!(s.source.as_deref(), Some("192.168.0.1"));
        assert!((s.offset_ms.unwrap() + 0.25).abs() < 1e-9);
        let s = chrony("Reference ID    : 00000000 ()\nLeap status     : Not synchronised\n");
        assert_eq!(s.synced, Some(false));
    }

    #[test]
    fn windows_time() {
        let t = "Leap Indicator: 0(no warning)\nSource: time.windows.com,0x9\nPhase Offset: 0.0012000s\n";
        let s = w32tm(t);
        assert_eq!(s.synced, Some(true));
        assert_eq!(s.source.as_deref(), Some("time.windows.com"));
        assert!((s.offset_ms.unwrap() - 1.2).abs() < 1e-9);
        assert_eq!(
            w32tm("Leap Indicator: 3(not synchronized)\n").synced,
            Some(false)
        );
    }

    #[test]
    fn linuxptp_answers() {
        let t = "sending: GET TIME_STATUS_NP\n\
                 \t001122.fffe.334455-1 seq 0 RESPONSE MANAGEMENT TIME_STATUS_NP\n\
                 \t\tmaster_offset              -42\n\
                 \t\tingress_time               1700000000000000000\n\
                 \t\tgmPresent                  true\n\
                 \t\tgmIdentity                 ac1f6b.fffe.123456\n\
                 sending: GET PORT_DATA_SET\n\
                 \t\tportState                  SLAVE\n";
        let s = pmc(t);
        assert!(s.running && s.locked);
        assert_eq!(s.offset_ns, Some(-42.0));
        assert_eq!(s.grandmaster.as_deref(), Some("ac1f6b.fffe.123456"));
        let s = pmc(&t.replace("SLAVE", "LISTENING"));
        assert!(s.running && !s.locked);
        assert!(!pmc("sending: GET TIME_STATUS_NP\n").running);
    }
}
