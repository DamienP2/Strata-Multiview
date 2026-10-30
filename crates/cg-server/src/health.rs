//! The system's health, for the System › Health page: a journal kept on
//! disk (`<data>/health/journal-<day>.jsonl`, 30 days), crash reports
//! (`<data>/health/crashes/`, 90 days: panics, and stops the server did not
//! make itself), the incidents of the last day grouped, and an archive of
//! all of it for the support.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::{internal, require, ApiResult};
use crate::auth::CurrentUser;
use crate::state::{now_ms, Shared};

/// Days the journal is kept.
const JOURNAL_DAYS: i64 = 30;
/// Days a crash report is kept.
const CRASH_DAYS: i64 = 90;
const DAY_MS: i64 = 86_400_000;

static DIR: OnceLock<PathBuf> = OnceLock::new();
static STARTED: OnceLock<i64> = OnceLock::new();
/// The journal file open, and its day.
static FILE: Mutex<Option<(String, fs::File)>> = Mutex::new(None);

/// One line of the journal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Line {
    /// ms since 1970.
    pub at: i64,
    /// CRASH, ERROR, WARN or INFO.
    pub level: String,
    /// air, outputs, inputs, data, render, users or server.
    pub cat: String,
    pub target: String,
    pub msg: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fields: String,
}

/// A crash: a panic, or a stop the server did not make itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Crash {
    pub at: i64,
    /// `panic` or `stop`.
    pub kind: String,
    pub version: String,
    #[serde(default)]
    pub thread: String,
    pub message: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub backtrace: String,
    /// How long the server had been running.
    #[serde(default)]
    pub uptime_secs: i64,
}

/// What part of the server a log line comes from.
pub fn category(target: &str) -> &'static str {
    let has = |s: &str| target.contains(s);
    if has("datasources") || has("datarules") || has("datapath") || has("social") {
        "data"
    } else if has("cg_output") || has("engine") || has("decklink") || has("::rt") || has("ndi") {
        "outputs"
    } else if has("inputs") {
        "inputs"
    } else if has("cg_render") || has("render") || has("thumbs") {
        "render"
    } else if has("antenna") || has("schedule") || has("asrun") || has("playout") || has("mos") {
        "air"
    } else if has("auth") {
        "users"
    } else {
        "server"
    }
}

fn day_of(ms: i64) -> String {
    jiff::Timestamp::from_millisecond(ms)
        .map(|t| {
            t.to_zoned(jiff::tz::TimeZone::system())
                .strftime("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_else(|_| "unknown".into())
}

fn write_line(line: &Line, wait: bool) {
    let Some(dir) = DIR.get() else {
        return;
    };
    // A panic while the file is held: never wait for it (same thread).
    let mut guard = if wait {
        FILE.lock().unwrap_or_else(|e| e.into_inner())
    } else {
        match FILE.try_lock() {
            Ok(g) => g,
            Err(_) => return,
        }
    };
    let day = day_of(line.at);
    if guard.as_ref().is_none_or(|(d, _)| *d != day) {
        let f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(format!("journal-{day}.jsonl")));
        *guard = f.ok().map(|f| (day, f));
    }
    if let (Some((_, f)), Ok(s)) = (guard.as_mut(), serde_json::to_string(line)) {
        let _ = writeln!(f, "{s}");
    }
}

/// Keeps a log line (INFO or more serious) in the journal: called by the
/// log ring (`logs::Ring`) for every event.
pub fn record(e: &crate::logs::Entry) {
    if matches!(e.level, "DEBUG" | "TRACE") {
        return;
    }
    write_line(
        &Line {
            at: e.at,
            level: e.level.to_string(),
            cat: category(&e.target).to_string(),
            target: e.target.clone(),
            msg: e.message.clone(),
            fields: e.fields.clone(),
        },
        true,
    );
}

fn crashes_dir() -> Option<PathBuf> {
    DIR.get().map(|d| d.join("crashes"))
}

fn save_crash(c: &Crash) {
    let Some(dir) = crashes_dir() else {
        return;
    };
    let _ = fs::create_dir_all(&dir);
    if let Ok(s) = serde_json::to_string_pretty(c) {
        let _ = fs::write(dir.join(format!("{}-{}.json", c.at, c.kind)), s);
    }
    write_line(
        &Line {
            at: c.at,
            level: "CRASH".into(),
            cat: if c.kind == "stop" {
                "server".into()
            } else {
                category(&c.location.replace('/', "::")).to_string()
            },
            target: "crash".into(),
            msg: if c.kind == "stop" {
                c.message.clone()
            } else {
                format!("panic: {} · {}", c.message, c.location)
            },
            fields: String::new(),
        },
        false,
    );
}

/// The crash reports kept, newest first.
fn crashes() -> Vec<Crash> {
    let Some(dir) = crashes_dir() else {
        return Vec::new();
    };
    let mut out: Vec<Crash> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| serde_json::from_str(&fs::read_to_string(e.path()).ok()?).ok())
        .collect();
    out.sort_by_key(|c| std::cmp::Reverse(c.at));
    out
}

fn uptime_secs() -> i64 {
    STARTED.get().map_or(0, |s| (now_ms() - s) / 1000)
}

/// The marker of a running server: removed by a stop the server makes itself.
#[derive(Serialize, Deserialize)]
struct Running {
    started: i64,
    /// Last sign of life.
    beat: i64,
}

fn marker() -> Option<PathBuf> {
    DIR.get().map(|d| d.join("running.json"))
}

fn beat() {
    if let (Some(m), Some(started)) = (marker(), STARTED.get()) {
        let r = Running {
            started: *started,
            beat: now_ms(),
        };
        if let Ok(s) = serde_json::to_string(&r) {
            let _ = fs::write(m, s);
        }
    }
}

/// Removes the files older than their time.
fn purge() {
    let Some(dir) = DIR.get() else {
        return;
    };
    let now = now_ms();
    let old_day = day_of(now - JOURNAL_DAYS * DAY_MS);
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if let Some(day) = name
            .strip_prefix("journal-")
            .and_then(|n| n.strip_suffix(".jsonl"))
        {
            if day < old_day.as_str() {
                let _ = fs::remove_file(e.path());
            }
        }
    }
    for c in crashes()
        .into_iter()
        .filter(|c| c.at < now - CRASH_DAYS * DAY_MS)
    {
        if let Some(d) = crashes_dir() {
            let _ = fs::remove_file(d.join(format!("{}-{}.json", c.at, c.kind)));
        }
    }
}

/// Starts the journal in `<data>/health`: notes a stop the last run did not
/// make itself, keeps what was logged so far, watches for panics.
pub fn start(data_dir: &std::path::Path) {
    let dir = data_dir.join("health");
    let _ = fs::create_dir_all(dir.join("crashes"));
    if DIR.set(dir).is_err() {
        return; // the engine restarting in the same process
    }
    let started = now_ms();
    let _ = STARTED.set(started);
    // The last run did not stop by itself, and no panic says why.
    if let Some(prev) = marker()
        .and_then(|m| fs::read_to_string(m).ok())
        .and_then(|s| serde_json::from_str::<Running>(&s).ok())
    {
        if !crashes().iter().any(|c| c.at >= prev.started) {
            save_crash(&Crash {
                at: prev.beat,
                kind: "stop".into(),
                version: String::new(),
                thread: String::new(),
                message: "unexpected stop (power cut, forced stop or crash)".into(),
                location: String::new(),
                backtrace: String::new(),
                uptime_secs: (prev.beat - prev.started) / 1000,
            });
        }
    }
    beat();
    // What was logged before the journal had its folder.
    for e in crate::logs::since(0, Some("info"), 5000).0 {
        record(&e);
    }
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".into());
        save_crash(&Crash {
            at: now_ms(),
            kind: "panic".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            thread: std::thread::current().name().unwrap_or("?").to_string(),
            message,
            location: info
                .location()
                .map(|l| format!("{}:{}", l.file(), l.line()))
                .unwrap_or_default(),
            backtrace: std::backtrace::Backtrace::force_capture().to_string(),
            uptime_secs: uptime_secs(),
        });
        prev(info);
    }));
    purge();
    std::thread::Builder::new()
        .name("health".into())
        .spawn(|| {
            let mut n = 0u32;
            loop {
                std::thread::sleep(Duration::from_secs(30));
                beat();
                n += 1;
                if n.is_multiple_of(720) {
                    purge();
                }
            }
        })
        .ok();
}

/// A stop the server makes itself (quit, restart, update): not a crash.
pub fn stopped() {
    if let Some(m) = marker() {
        let _ = fs::remove_file(m);
    }
}

// ----------------------------------------------------------------- reading --

/// The journal lines of the last `days`, newest first, before `before`.
fn read_journal(
    days: i64,
    before: Option<i64>,
    keep: impl Fn(&Line) -> bool,
    limit: usize,
) -> Vec<Line> {
    let Some(dir) = DIR.get() else {
        return Vec::new();
    };
    let now = now_ms();
    let from = now - days * DAY_MS;
    let mut files: Vec<(String, PathBuf)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            let day = n
                .strip_prefix("journal-")?
                .strip_suffix(".jsonl")?
                .to_string();
            Some((day, e.path()))
        })
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    let first_day = day_of(from);
    let mut out = Vec::new();
    for (day, path) in files {
        if day < first_day {
            break;
        }
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let mut lines: Vec<Line> = text
            .lines()
            .filter_map(|l| serde_json::from_str::<Line>(l).ok())
            .filter(|l| l.at >= from && before.is_none_or(|b| l.at < b) && keep(l))
            .collect();
        lines.reverse();
        out.extend(lines);
        if out.len() >= limit {
            out.truncate(limit);
            break;
        }
    }
    out
}

fn rank(level: &str) -> u8 {
    match level {
        "CRASH" => 0,
        "ERROR" => 1,
        "WARN" => 2,
        _ => 3,
    }
}

#[derive(Deserialize)]
struct JournalQuery {
    /// crash, error, warn or info: that level and more serious.
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    days: Option<i64>,
    #[serde(default)]
    cat: Option<String>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    before: Option<i64>,
    #[serde(default)]
    limit: Option<usize>,
}

async fn journal(
    CurrentUser(me): CurrentUser,
    Query(q): Query<JournalQuery>,
) -> ApiResult<Json<Value>> {
    require(me.is_admin())?;
    let max = rank(&q.level.as_deref().unwrap_or("info").to_ascii_uppercase());
    let days = q.days.unwrap_or(1).clamp(1, JOURNAL_DAYS);
    let limit = q.limit.unwrap_or(300).min(2000);
    let needle = q.q.unwrap_or_default().to_lowercase();
    let cat = q.cat.filter(|c| !c.is_empty() && c != "all");
    let lines = tokio::task::spawn_blocking(move || {
        read_journal(
            days,
            q.before,
            |l| {
                rank(&l.level) <= max
                    && cat.as_ref().is_none_or(|c| *c == l.cat)
                    && (needle.is_empty()
                        || l.msg.to_lowercase().contains(&needle)
                        || l.fields.to_lowercase().contains(&needle)
                        || l.target.contains(&needle))
            },
            limit + 1,
        )
    })
    .await
    .map_err(internal)?;
    let more = lines.len() > limit;
    Ok(Json(
        json!({ "lines": &lines[..lines.len().min(limit)], "more": more }),
    ))
}

/// The seen state: the time up to which the crashes were seen.
fn seen_at() -> i64 {
    DIR.get()
        .and_then(|d| fs::read_to_string(d.join("seen.json")).ok())
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.get("crashesSeenAt").and_then(|x| x.as_i64()))
        .unwrap_or(0)
}

/// A message without its changing parts (numbers, quoted values): lines
/// saying the same thing are one incident.
fn shape(msg: &str) -> String {
    let mut out = String::new();
    let mut digits = false;
    for c in msg.chars() {
        if c.is_ascii_digit() {
            if !digits {
                out.push('#');
            }
            digits = true;
        } else {
            digits = false;
            out.push(c);
        }
    }
    out
}

/// Lines saying the same thing: level, category, message shape.
type GroupKey = (String, String, String);
/// How many, first and last time, the last line.
type Group = (usize, i64, i64, Line);

/// Disk of the data folder: total and free bytes.
fn disk(dir: &std::path::Path) -> Option<(u64, u64)> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let dir = dir.canonicalize().ok()?;
    disks
        .list()
        .iter()
        .filter(|d| dir.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| (d.total_space(), d.available_space()))
}

/// `GET /api/system/health`: what the Health page shows first.
async fn summary(State(s): State<Shared>, CurrentUser(me): CurrentUser) -> ApiResult<Json<Value>> {
    require(me.is_admin())?;
    let data_dir = s.data_dir.clone();
    let v = tokio::task::spawn_blocking(move || {
        let now = now_ms();
        let crashes: Vec<Crash> = crashes();
        let month: Vec<&Crash> = crashes.iter().filter(|c| c.at >= now - JOURNAL_DAYS * DAY_MS).collect();
        let day = read_journal(1, None, |l| rank(&l.level) <= 2 && l.level != "CRASH" && l.at >= now - DAY_MS, 100_000);
        let errors = day.iter().filter(|l| l.level == "ERROR").count();
        let warnings = day.iter().filter(|l| l.level == "WARN").count();
        let last_hour_errors = day.iter().filter(|l| l.level == "ERROR" && l.at >= now - 3_600_000).count();
        // The day's errors and warnings, the same message once with its count.
        let mut groups: BTreeMap<GroupKey, Group> = BTreeMap::new();
        for l in &day {
            let key = (l.level.clone(), l.cat.clone(), shape(&l.msg));
            let g = groups.entry(key).or_insert((0, l.at, l.at, l.clone()));
            g.0 += 1;
            g.1 = g.1.min(l.at);
            if l.at >= g.2 {
                g.2 = l.at;
                g.3 = l.clone();
            }
        }
        let mut incidents: Vec<Value> = month
            .iter()
            .map(|c| {
                json!({
                    "kind": c.kind, "level": "CRASH", "cat": if c.kind == "stop" { "server" } else { category(&c.location.replace('/', "::")) },
                    "title": c.message, "detail": c.location, "first": c.at, "last": c.at, "count": 1,
                    "crash": c,
                })
            })
            .collect();
        let mut rest: Vec<_> = groups.into_values().collect();
        rest.sort_by(|a, b| rank(&a.3.level).cmp(&rank(&b.3.level)).then(b.2.cmp(&a.2)));
        incidents.extend(rest.into_iter().take(40).map(|(n, first, last, l)| {
            json!({ "kind": l.level.to_lowercase(), "level": l.level, "cat": l.cat, "title": l.msg, "detail": l.fields, "first": first, "last": last, "count": n })
        }));
        let seen = seen_at();
        let (total, free) = disk(&data_dir).unwrap_or((0, 0));
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "startedAt": STARTED.get(),
            "uptimeSecs": uptime_secs(),
            "crashes": month.len(),
            "lastCrash": month.first().map(|c| c.at),
            "unseenCrash": month.iter().any(|c| c.at > seen),
            "errors": errors,
            "warnings": warnings,
            "lastHourErrors": last_hour_errors,
            "disk": { "total": total, "free": free },
            "incidents": incidents,
        })
    })
    .await
    .map_err(internal)?;
    Ok(Json(v))
}

/// `POST /api/system/health/seen`: the crashes so far were seen.
async fn seen(CurrentUser(me): CurrentUser) -> ApiResult<Json<Value>> {
    require(me.is_admin())?;
    if let Some(d) = DIR.get() {
        fs::write(
            d.join("seen.json"),
            json!({ "crashesSeenAt": now_ms() }).to_string(),
        )
        .map_err(internal)?;
    }
    Ok(Json(json!({ "ok": true })))
}

/// Settings with their secrets blanked (passwords, keys, tokens).
fn redact(v: &mut Value) {
    match v {
        Value::Object(o) => {
            for (k, x) in o.iter_mut() {
                let k = k.to_ascii_lowercase();
                if [
                    "password", "secret", "token", "apikey", "api_key", "key", "pass",
                ]
                .iter()
                .any(|s| k == *s || k.ends_with(s))
                    && x.is_string()
                {
                    *x = Value::String("•••".into());
                } else {
                    redact(x);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(redact),
        _ => {}
    }
}

fn archive(data_dir: &std::path::Path) -> Result<Vec<u8>, String> {
    let e = |x: zip::result::ZipError| x.to_string();
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    sys.refresh_cpu_all();
    let info = format!(
        "Strata Graphics {}\nOS: {} {}\nCPU: {} × {}\nMemory: {} MB (free {} MB)\nUp: {} s\nArchive made: {}\n",
        env!("CARGO_PKG_VERSION"),
        sysinfo::System::name().unwrap_or_default(),
        sysinfo::System::os_version().unwrap_or_default(),
        sys.cpus().len(),
        sys.cpus().first().map(|c| c.brand().to_string()).unwrap_or_default(),
        sys.total_memory() / 1_048_576,
        sys.available_memory() / 1_048_576,
        uptime_secs(),
        jiff::Timestamp::from_millisecond(now_ms()).map(|t| t.to_string()).unwrap_or_default(),
    );
    z.start_file("system.txt", opts).map_err(e)?;
    z.write_all(info.as_bytes()).map_err(|x| x.to_string())?;
    if let Some(dir) = DIR.get() {
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let n = entry.file_name().to_string_lossy().into_owned();
            if n.starts_with("journal-") {
                if let Ok(b) = fs::read(entry.path()) {
                    z.start_file(format!("journal/{n}"), opts).map_err(e)?;
                    z.write_all(&b).map_err(|x| x.to_string())?;
                }
            }
        }
    }
    for c in crashes() {
        z.start_file(format!("crashes/{}-{}.json", c.at, c.kind), opts)
            .map_err(e)?;
        z.write_all(
            serde_json::to_string_pretty(&c)
                .unwrap_or_default()
                .as_bytes(),
        )
        .map_err(|x| x.to_string())?;
    }
    if let Some(mut v) = fs::read_to_string(data_dir.join("config.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
    {
        redact(&mut v);
        z.start_file("config.json", opts).map_err(e)?;
        z.write_all(
            serde_json::to_string_pretty(&v)
                .unwrap_or_default()
                .as_bytes(),
        )
        .map_err(|x| x.to_string())?;
    }
    if let Some(f) = crate::logs::FILE.get() {
        if let Ok(b) = fs::read(f) {
            z.start_file("strata-server.log", opts).map_err(e)?;
            z.write_all(&b).map_err(|x| x.to_string())?;
        }
    }
    Ok(z.finish().map_err(e)?.into_inner())
}

/// `GET /api/system/health/export.zip`: everything the support needs.
async fn export(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<impl IntoResponse> {
    require(me.is_admin())?;
    let dir = s.data_dir.clone();
    let bytes = tokio::task::spawn_blocking(move || archive(&dir))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    let name = format!("strata-health-{}.zip", day_of(now_ms()));
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
        ],
        bytes,
    ))
}

pub fn router() -> Router<Shared> {
    Router::new()
        .route("/api/system/health", get(summary))
        .route("/api/system/health/journal", get(journal))
        .route("/api/system/health/seen", post(seen))
        .route("/api/system/health/export.zip", get(export))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_and_an_unexpected_stop_are_kept() {
        let dir = std::env::temp_dir().join(format!("strata-health-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        // The last run left its marker: it did not stop by itself.
        fs::create_dir_all(dir.join("health")).unwrap();
        let beat_at = now_ms() - 60_000;
        fs::write(
            dir.join("health/running.json"),
            json!({ "started": beat_at - 60_000, "beat": beat_at }).to_string(),
        )
        .unwrap();
        start(&dir);
        let r = std::panic::catch_unwind(|| panic!("boom in a test"));
        assert!(r.is_err());
        let c = crashes();
        assert!(c.iter().any(|c| c.kind == "panic"
            && c.message == "boom in a test"
            && c.location.contains("health.rs")));
        assert!(c
            .iter()
            .any(|c| c.kind == "stop" && c.at == beat_at && c.uptime_secs == 60));
        let lines = read_journal(JOURNAL_DAYS, None, |l| l.level == "CRASH", 10);
        assert!(lines.iter().any(|l| l.msg.contains("boom in a test")));
        // A stop it makes itself leaves no marker.
        stopped();
        assert!(!dir.join("health/running.json").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lines_are_grouped_and_secrets_hidden() {
        assert_eq!(
            shape("NDI: 24 frames dropped in 10 s"),
            "NDI: # frames dropped in # s"
        );
        assert_eq!(category("cg_server::datasources"), "data");
        assert_eq!(category("cg_output::ndi"), "outputs");
        assert_eq!(category("cg_render::wid::table"), "render");
        let mut v = json!({ "smtp": { "password": "x", "host": "h" }, "apiKey": "k", "keyer": 2 });
        redact(&mut v);
        assert_eq!(v["smtp"]["password"], "•••");
        assert_eq!(v["smtp"]["host"], "h");
        assert_eq!(v["apiKey"], "•••");
        assert_eq!(v["keyer"], 2);
    }
}
