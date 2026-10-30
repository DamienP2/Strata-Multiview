//! The machine's time (NTP), set from Settings › Time.
//!
//! The operating system keeps the time; Strata only tells it which NTP
//! servers to use, whether to use NTP at all (not when PTP gives the time),
//! and when to synchronise at once:
//! - Linux: the service account may not change the system, so a small root
//!   helper does it (`strata-time`, installed with the service). Strata
//!   writes its request to `<data>/time/request`; a systemd path unit runs
//!   the helper, which sets chrony when it is installed, else
//!   systemd-timesyncd, and answers in `/run/strata-time/result`.
//! - macOS (the service runs as root): `systemsetup` and `sntp`.
//! - Windows (the service runs as LocalSystem): `w32tm`.

use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

const MAX_SERVERS: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeConfig {
    /// Keep the time with NTP.
    #[serde(default = "yes")]
    pub ntp: bool,
    /// NTP servers (names or addresses); none: the system's own.
    #[serde(default)]
    pub ntp_servers: Vec<String>,
}

fn yes() -> bool {
    true
}

impl Default for TimeConfig {
    fn default() -> Self {
        Self {
            ntp: true,
            ntp_servers: Vec::new(),
        }
    }
}

impl TimeConfig {
    pub fn normalize(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.ntp_servers = self
            .ntp_servers
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && seen.insert(s.to_ascii_lowercase()))
            .collect();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.ntp_servers.len() > MAX_SERVERS {
            return Err(format!("at most {MAX_SERVERS} NTP servers"));
        }
        match self.ntp_servers.iter().find(|s| !valid_server(s)) {
            Some(s) => Err(format!("invalid NTP server: {s}")),
            None => Ok(()),
        }
    }
}

/// A host name, an IPv4 or an IPv6 address: nothing a command line or a
/// configuration file could read as something else.
fn valid_server(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 253
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
}

/// How the last change went.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub ok: bool,
    pub message: String,
    /// When (ms since the Unix epoch).
    pub at: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeState {
    /// This machine lets Strata set its time.
    pub supported: bool,
    pub last: Option<Outcome>,
    /// The server's time (ms since the Unix epoch), to show its clock.
    pub now: u64,
}

/// The outcome of the last change made here (macOS, Windows).
static LAST: Mutex<Option<Outcome>> = Mutex::new(None);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn done(r: Result<String, String>) -> Result<(), String> {
    let outcome = match &r {
        Ok(m) => Outcome {
            ok: true,
            message: m.clone(),
            at: now_ms(),
        },
        Err(e) => Outcome {
            ok: false,
            message: e.clone(),
            at: now_ms(),
        },
    };
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some(outcome);
    r.map(|_| ())
}

#[cfg_attr(target_os = "linux", allow(dead_code))]
fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd
        .output()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let said = String::from_utf8_lossy(&out.stdout);
        Err(format!(
            "{program} {}: {}",
            args.join(" "),
            if err.trim().is_empty() {
                said.trim()
            } else {
                err.trim()
            }
        ))
    }
}

// ---------------------------------------------------------------- Linux ----

/// The root helper and its systemd units.
pub const HELPER: &str = "/usr/local/lib/strata/strata-time";
#[cfg(target_os = "linux")]
const RESULT: &str = "/run/strata-time/result";
#[cfg(target_os = "linux")]
const PATH_UNIT: &str = "/etc/systemd/system/strata-time.path";
#[cfg(target_os = "linux")]
const SERVICE_UNIT: &str = "/etc/systemd/system/strata-time.service";

/// The helper (run as root on Strata's request). It reads only `enabled`,
/// `server` and `action` lines, checks every server name, and writes its
/// answer where only root can write.
#[cfg(target_os = "linux")]
const HELPER_SCRIPT: &str = r##"#!/bin/sh
# Strata Graphics: sets the machine's NTP as Settings > Time asks (run as root).
# Request: one setting per line: action=apply|sync, enabled=0|1, server=<name>.
set -u
REQ=$1
OUT=/run/strata-time
mkdir -p "$OUT" && chmod 755 "$OUT"
result() {
  printf 'ok=%s\nat=%s\nmessage=%s\n' "$1" "$(date +%s)" "$2" > "$OUT/result.tmp"
  chmod 644 "$OUT/result.tmp" && mv -f "$OUT/result.tmp" "$OUT/result"
  exit 0
}
{ [ -f "$REQ" ] && [ ! -L "$REQ" ]; } || result 0 "no request"
action=apply enabled=1 servers=""
while IFS= read -r line; do
  case "$line" in
    action=sync) action=sync ;;
    enabled=0) enabled=0 ;;
    server=*)
      s=${line#server=}
      case "$s" in ''|-*|*[!A-Za-z0-9.:-]*) result 0 "invalid NTP server" ;; esac
      servers="$servers $s" ;;
  esac
done < "$REQ"
servers=${servers# }

if command -v chronyd >/dev/null 2>&1; then
  unit=chrony
  systemctl cat chrony >/dev/null 2>&1 || unit=chronyd
  if [ "$action" = sync ]; then
    chronyc burst 4/4 >/dev/null 2>&1; sleep 4
    chronyc makestep >/dev/null 2>&1 || result 0 "chronyc makestep failed"
    result 1 "synchronised (chrony)"
  fi
  conf=""
  for f in /etc/chrony/chrony.conf /etc/chrony.conf; do [ -f "$f" ] && { conf=$f; break; }; done
  [ -n "$conf" ] || result 0 "chrony.conf not found"
  # Back to the system's servers, then ours instead of them.
  sed -i -e '/^# strata-begin$/,/^# strata-end$/d' -e 's/^#strata# //' "$conf"
  if [ -n "$servers" ]; then
    sed -i -E 's/^(pool|server|peer) /#strata# &/' "$conf"
    { echo "# strata-begin"; for s in $servers; do echo "server $s iburst"; done; echo "# strata-end"; } >> "$conf"
  fi
  if [ "$enabled" = 1 ]; then
    systemctl enable "$unit" >/dev/null 2>&1
    systemctl restart "$unit" || result 0 "cannot restart $unit"
    result 1 "chrony set"
  fi
  systemctl disable --now "$unit" >/dev/null 2>&1
  result 1 "NTP off (chrony stopped)"
fi

command -v timedatectl >/dev/null 2>&1 || result 0 "neither chrony nor systemd-timesyncd found"
if [ "$action" = sync ]; then
  systemctl restart systemd-timesyncd || result 0 "cannot restart systemd-timesyncd"
  result 1 "synchronised (systemd-timesyncd)"
fi
dir=/etc/systemd/timesyncd.conf.d
if [ -n "$servers" ]; then
  mkdir -p "$dir" && printf '[Time]\nNTP=%s\n' "$servers" > "$dir/strata.conf"
else
  rm -f "$dir/strata.conf"
fi
if [ "$enabled" = 1 ]; then
  timedatectl set-ntp true || result 0 "timedatectl set-ntp true failed"
  systemctl restart systemd-timesyncd || result 0 "cannot restart systemd-timesyncd"
  result 1 "systemd-timesyncd set"
fi
timedatectl set-ntp false || result 0 "timedatectl set-ntp false failed"
result 1 "NTP off"
"##;

/// Installs the helper and its units, watching `<data>/time/request`.
#[cfg(target_os = "linux")]
pub fn install_helper(data_dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let request = data_dir.join("time").join("request");
    std::fs::create_dir_all(Path::new(HELPER).parent().unwrap_or(Path::new("/")))?;
    std::fs::write(HELPER, HELPER_SCRIPT)?;
    std::fs::set_permissions(HELPER, std::fs::Permissions::from_mode(0o755))?;
    std::fs::write(
        PATH_UNIT,
        format!(
            "[Unit]\nDescription=Strata Graphics: NTP settings asked from Settings > Time\n\n\
             [Path]\nPathChanged={}\n\n[Install]\nWantedBy=multi-user.target\n",
            request.display()
        ),
    )?;
    std::fs::write(
        SERVICE_UNIT,
        format!(
            "[Unit]\nDescription=Strata Graphics: apply the NTP settings\n\n\
             [Service]\nType=oneshot\nExecStart={HELPER} \"{}\"\n",
            request.display()
        ),
    )?;
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn uninstall_helper() {
    for f in [PATH_UNIT, SERVICE_UNIT, HELPER] {
        let _ = std::fs::remove_file(f);
    }
}

#[cfg(target_os = "linux")]
fn request(cfg: &TimeConfig, sync: bool) -> String {
    let mut r = format!(
        "action={}\nenabled={}\n",
        if sync { "sync" } else { "apply" },
        u8::from(cfg.ntp)
    );
    for s in &cfg.ntp_servers {
        r.push_str(&format!("server={s}\n"));
    }
    r
}

#[cfg(target_os = "linux")]
fn linux(data_dir: &Path, cfg: &TimeConfig, sync: bool) -> Result<(), String> {
    if !Path::new(HELPER).exists() {
        return Err("the time helper is not installed (install the service again)".into());
    }
    let dir = data_dir.join("time");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("request");
    std::fs::write(&path, request(cfg, sync)).map_err(|e| e.to_string())?;
    // Run by hand as root (no service): the helper at once. Otherwise the
    // path unit has seen the request.
    // SAFETY: geteuid has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        std::process::Command::new(HELPER)
            .arg(&path)
            .status()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn linux_last() -> Option<Outcome> {
    let t = std::fs::read_to_string(RESULT).ok()?;
    let value = |k: &str| {
        t.lines()
            .find_map(|l| l.strip_prefix(k)?.strip_prefix('='))
            .unwrap_or("")
    };
    Some(Outcome {
        ok: value("ok") == "1",
        message: value("message").to_string(),
        at: value("at").parse::<u64>().unwrap_or(0) * 1000,
    })
}

// ---------------------------------------------------------------- macOS ----

#[cfg(target_os = "macos")]
fn macos(cfg: &TimeConfig, sync: bool) -> Result<String, String> {
    const SETUP: &str = "/usr/sbin/systemsetup";
    if sync {
        let server = cfg
            .ntp_servers
            .first()
            .map_or("time.apple.com", String::as_str);
        run("/usr/bin/sntp", &["-sS", server])?;
        return Ok("synchronised".into());
    }
    if let Some(first) = cfg.ntp_servers.first() {
        run(SETUP, &["-setnetworktimeserver", first])?;
    }
    run(
        SETUP,
        &["-setusingnetworktime", if cfg.ntp { "on" } else { "off" }],
    )?;
    Ok(if cfg.ntp_servers.len() > 1 {
        "macOS uses the first server only".into()
    } else {
        "set".into()
    })
}

// -------------------------------------------------------------- Windows ----

#[cfg(windows)]
fn windows(cfg: &TimeConfig, sync: bool) -> Result<String, String> {
    if sync {
        run("w32tm", &["/resync", "/force"])?;
        return Ok("synchronised".into());
    }
    if !cfg.ntp {
        run("w32tm", &["/config", "/syncfromflags:NO", "/update"])?;
        return Ok("NTP off".into());
    }
    let _ = run("sc", &["config", "w32time", "start=", "auto"]);
    let _ = run("net", &["start", "w32time"]);
    if cfg.ntp_servers.is_empty() {
        run("w32tm", &["/config", "/syncfromflags:manual", "/update"])?;
    } else {
        let peers = cfg
            .ntp_servers
            .iter()
            .map(|s| format!("{s},0x8"))
            .collect::<Vec<_>>()
            .join(" ");
        run(
            "w32tm",
            &[
                "/config",
                &format!("/manualpeerlist:{peers}"),
                "/syncfromflags:manual",
                "/update",
            ],
        )?;
    }
    Ok("set".into())
}

// ------------------------------------------------------------------ all ----

/// Whether this machine lets Strata set its time.
pub fn supported() -> bool {
    if cfg!(target_os = "linux") {
        Path::new(HELPER).exists()
    } else {
        cfg!(any(target_os = "macos", windows))
    }
}

pub fn state() -> TimeState {
    #[cfg(target_os = "linux")]
    let last = linux_last();
    #[cfg(not(target_os = "linux"))]
    let last = LAST.lock().unwrap_or_else(|e| e.into_inner()).clone();
    TimeState {
        supported: supported(),
        last,
        now: now_ms(),
    }
}

/// Asks the system to keep its time as `cfg` says; `sync`: to synchronise
/// now instead. Blocking (runs the system's tools).
pub fn apply(data_dir: &Path, cfg: &TimeConfig, sync: bool) -> Result<(), String> {
    cfg.validate()?;
    #[cfg(target_os = "linux")]
    {
        let _ = done;
        linux(data_dir, cfg, sync)
    }
    #[cfg(target_os = "macos")]
    {
        let _ = data_dir;
        done(macos(cfg, sync))
    }
    #[cfg(windows)]
    {
        let _ = data_dir;
        done(windows(cfg, sync))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = (data_dir, sync);
        done(Err(
            "setting the time is not supported on this system".into()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servers_are_names_or_addresses_only() {
        for ok in ["pool.ntp.org", "10.0.0.1", "fe80::1", "ntp-1.example.fr"] {
            assert!(valid_server(ok), "{ok}");
        }
        for bad in ["", "-x", "a b", "a;rm", "a\nb", "$(id)", "a/b"] {
            assert!(!valid_server(bad), "{bad:?}");
        }
        let mut c = TimeConfig {
            ntp: true,
            ntp_servers: vec![" a.org ".into(), "".into(), "A.org".into(), "b".into()],
        };
        c.normalize();
        assert_eq!(c.ntp_servers, ["a.org", "b"]);
        assert!(c.validate().is_ok());
        c.ntp_servers.push("x y".into());
        assert!(c.validate().is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_request_lists_the_settings() {
        let c = TimeConfig {
            ntp: false,
            ntp_servers: vec!["a.org".into(), "10.0.0.1".into()],
        };
        assert_eq!(
            request(&c, false),
            "action=apply\nenabled=0\nserver=a.org\nserver=10.0.0.1\n"
        );
        assert!(request(&c, true).starts_with("action=sync\n"));
    }
}
