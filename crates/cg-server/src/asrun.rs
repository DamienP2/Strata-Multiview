//! As-run log: every action on air (who, what, when, with which values),
//! kept on disk per day for compliance and exported as CSV or PDF.
//!
//! Each entry is one JSON line of `<data_dir>/asrun/YYYY-MM-DD.jsonl` (UTC
//! day), written by one background thread so the playout never waits for
//! the disk. Files are kept for ever.

use std::collections::{BTreeMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::antenna::{Actor, ActorKind};
use crate::api::{internal, ApiResult};
use crate::auth::{CurrentUser, User};
use crate::state::{AppState, Event, Shared};

/// One action on air.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Server time, ms.
    pub at: i64,
    pub channel_id: String,
    pub channel_name: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub project_name: String,
    /// "take", "out", "continue", "update", "clear", "row", "widget",
    /// "trigger", "undo", or for the engine's own steps "auto-next",
    /// "auto-out", "auto-continue".
    pub action: String,
    /// The graphic (rundown item or template name).
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub zone: String,
    pub actor: Actor,
    /// Values sent (take) or changed (update): key → [before, after].
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, (Option<String>, String)>,
    /// Free detail ("scheduled 20:00:00", "auto next after 8 s"…).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

/// Keeps an entry (written in the background) and tells the clients.
pub fn record(s: &AppState, e: Entry) {
    tracing::debug!(
        channel = e.channel_id,
        action = e.action,
        label = e.label,
        by = e.actor.name,
        "on air"
    );
    let _ = s.events.send(Event::AsRun {
        entry: Box::new(e.clone()),
    });
    write(&s.data_dir, e);
}

// ---------- writing ----------

enum Msg {
    Write(PathBuf, Box<Entry>),
    /// Answered once everything sent before is on disk.
    Flush(mpsc::SyncSender<()>),
}

fn writer() -> &'static mpsc::Sender<Msg> {
    static TX: OnceLock<mpsc::Sender<Msg>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        if let Err(e) = std::thread::Builder::new()
            .name("asrun".into())
            .spawn(move || write_loop(rx))
        {
            tracing::error!("as-run log: cannot start its writer: {e}");
        }
        tx
    })
}

fn write_loop(rx: mpsc::Receiver<Msg>) {
    // The file of the day, kept open.
    let mut open: Option<(PathBuf, BufWriter<File>)> = None;
    let mut failed = false;
    while let Ok(first) = rx.recv() {
        let mut next = Some(first);
        while let Some(msg) = next.take() {
            match msg {
                Msg::Write(dir, e) => {
                    let path = day_file(&dir, e.at);
                    if open.as_ref().is_none_or(|(p, _)| *p != path) {
                        if let Some((_, mut w)) = open.take() {
                            let _ = w.flush();
                        }
                        let f = std::fs::create_dir_all(path.parent().unwrap_or(&dir))
                            .and_then(|_| OpenOptions::new().create(true).append(true).open(&path));
                        match f {
                            Ok(f) => {
                                open = Some((path.clone(), BufWriter::new(f)));
                                failed = false;
                            }
                            Err(err) => {
                                if !failed {
                                    tracing::error!("as-run log: cannot open {path:?}: {err}");
                                }
                                failed = true;
                            }
                        }
                    }
                    if let Some((_, w)) = open.as_mut() {
                        let line = serde_json::to_vec(&e).unwrap_or_default();
                        if let Err(err) = w.write_all(&line).and_then(|_| w.write_all(b"\n")) {
                            tracing::error!("as-run log: cannot write {path:?}: {err}");
                        }
                    }
                }
                Msg::Flush(done) => {
                    if let Some((_, w)) = open.as_mut() {
                        let _ = w.flush();
                    }
                    let _ = done.send(());
                }
            }
            next = rx.try_recv().ok();
        }
        // Nothing more waiting: to the disk.
        if let Some((_, w)) = open.as_mut() {
            let _ = w.flush();
        }
    }
}

fn write(data_dir: &Path, e: Entry) {
    let _ = writer().send(Msg::Write(data_dir.to_path_buf(), Box::new(e)));
}

/// Waits until everything recorded so far is on disk (at most 2 s).
fn flush() {
    let (tx, rx) = mpsc::sync_channel(1);
    if writer().send(Msg::Flush(tx)).is_ok() {
        let _ = rx.recv_timeout(Duration::from_secs(2));
    }
}

fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("asrun")
}

/// "2026-10-24" (UTC) of a time in ms.
fn day(at: i64) -> String {
    jiff::Timestamp::from_millisecond(at)
        .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
        .strftime("%Y-%m-%d")
        .to_string()
}

fn day_file(data_dir: &Path, at: i64) -> PathBuf {
    dir(data_dir).join(format!("{}.jsonl", day(at)))
}

// ---------- reading ----------

/// Query of `GET /api/asrun` and `/api/asrun.csv`. Lists are comma separated.
#[derive(Debug, Default, Deserialize)]
pub struct Filter {
    /// ms, included (default: 24 h before `to`).
    pub from: Option<i64>,
    /// ms, included (default: now).
    pub to: Option<i64>,
    pub channel: Option<String>,
    /// "take,out,update…" ("auto" for every step of the engine).
    pub actions: Option<String>,
    /// Actor ids.
    pub actors: Option<String>,
    /// "user,api,mos,auto,schedule,data".
    pub kinds: Option<String>,
    /// Text in the graphic, values, actor, project, channel, zone, detail.
    pub q: Option<String>,
    pub limit: Option<usize>,
}

fn list(s: &Option<String>) -> Vec<String> {
    s.as_deref()
        .unwrap_or("")
        .split(',')
        .map(|x| x.trim().to_lowercase())
        .filter(|x| !x.is_empty())
        .collect()
}

fn kind_name(k: ActorKind) -> &'static str {
    match k {
        ActorKind::User => "user",
        ActorKind::Api => "api",
        ActorKind::Mos => "mos",
        ActorKind::Auto => "auto",
        ActorKind::Schedule => "schedule",
        ActorKind::Data => "data",
    }
}

/// A `Filter` ready to test entries.
struct Matcher {
    from: i64,
    to: i64,
    channel: Option<String>,
    actions: Vec<String>,
    actors: Vec<String>,
    kinds: Vec<String>,
    q: String,
}

impl Matcher {
    fn new(f: &Filter, now: i64) -> Self {
        let to = f.to.unwrap_or(now);
        Self {
            from: f.from.unwrap_or(to - 86_400_000),
            to,
            channel: f.channel.clone().filter(|c| !c.is_empty()),
            actions: list(&f.actions),
            actors: list(&f.actors),
            kinds: list(&f.kinds),
            q: f.q.as_deref().unwrap_or("").trim().to_lowercase(),
        }
    }

    fn matches(&self, e: &Entry) -> bool {
        if e.at < self.from || e.at > self.to {
            return false;
        }
        if self.channel.as_ref().is_some_and(|c| *c != e.channel_id) {
            return false;
        }
        let action = e.action.to_lowercase();
        if !self.actions.is_empty()
            && !self
                .actions
                .iter()
                .any(|a| *a == action || (a == "auto" && action.starts_with("auto-")))
        {
            return false;
        }
        if !self.actors.is_empty() && !self.actors.contains(&e.actor.id.to_lowercase()) {
            return false;
        }
        if !self.kinds.is_empty() && !self.kinds.iter().any(|k| k == kind_name(e.actor.kind)) {
            return false;
        }
        if !self.q.is_empty() {
            let hay = [
                &e.label,
                &e.action,
                &e.zone,
                &e.channel_name,
                &e.project_name,
                &e.actor.name,
                &e.detail,
            ];
            let found = hay.iter().any(|h| h.to_lowercase().contains(&self.q))
                || e.values.iter().any(|(k, (b, a))| {
                    k.to_lowercase().contains(&self.q)
                        || a.to_lowercase().contains(&self.q)
                        || b.as_ref()
                            .is_some_and(|b| b.to_lowercase().contains(&self.q))
                });
            if !found {
                return false;
            }
        }
        true
    }
}

/// The entries matching `f` that `sees` lets through, newest first, at most
/// `limit`. Only the day files of the period are read, line by line.
pub fn read(
    data_dir: &Path,
    f: &Filter,
    now: i64,
    limit: usize,
    sees: impl Fn(&Entry) -> bool,
) -> Vec<Entry> {
    let m = Matcher::new(f, now);
    let (first, last) = (day(m.from.max(0)), day(m.to.max(0)));
    let mut days: Vec<(String, PathBuf)> = std::fs::read_dir(dir(data_dir))
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    let d = name.strip_suffix(".jsonl")?.to_string();
                    (d.len() == 10 && d >= first && d <= last).then(|| (d, e.path()))
                })
                .collect()
        })
        .unwrap_or_default();
    days.sort_by(|a, b| b.0.cmp(&a.0));
    let mut out = Vec::new();
    for (_, path) in days {
        if out.len() >= limit {
            break;
        }
        let Ok(file) = File::open(&path) else {
            continue;
        };
        // The latest matches of the day (a file is in time order).
        let room = limit - out.len();
        let mut found: VecDeque<Entry> = VecDeque::new();
        for line in BufReader::new(file).lines() {
            let Ok(line) = line else { break };
            let Ok(e) = serde_json::from_str::<Entry>(&line) else {
                continue;
            };
            if m.matches(&e) && sees(&e) {
                found.push_back(e);
                if found.len() > room {
                    found.pop_front();
                }
            }
        }
        let mut found: Vec<Entry> = found.into();
        found.sort_by_key(|e| std::cmp::Reverse(e.at));
        out.extend(found);
    }
    out
}

const LIMIT: usize = 5_000;
const MAX: usize = 200_000;

async fn query(s: &Shared, me: User, f: Filter, limit: usize) -> ApiResult<Vec<Entry>> {
    let data_dir = s.data_dir.clone();
    tokio::task::spawn_blocking(move || {
        flush();
        read(&data_dir, &f, crate::state::now_ms(), limit, |e: &Entry| {
            me.sees_channel(&e.channel_id)
        })
    })
    .await
    .map_err(internal)
}

/// `GET /api/asrun`: the entries, newest first.
pub async fn get_asrun(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Query(f): Query<Filter>,
) -> ApiResult<Json<Vec<Entry>>> {
    let limit = f.limit.unwrap_or(LIMIT).clamp(1, MAX);
    Ok(Json(query(&s, me, f, limit).await?))
}

/// `GET /api/asrun.csv`: the entries in time order, as a CSV download.
pub async fn get_asrun_csv(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Query(f): Query<Filter>,
) -> ApiResult<impl IntoResponse> {
    let limit = f.limit.unwrap_or(MAX).clamp(1, MAX);
    let fr = me.lang.starts_with("fr");
    let mut rows = query(&s, me, f, limit).await?;
    rows.reverse();
    let tz_name = s.config.timezone.trim();
    let tz = if tz_name.is_empty() {
        jiff::tz::TimeZone::system()
    } else {
        jiff::tz::TimeZone::get(tz_name).unwrap_or_else(|_| jiff::tz::TimeZone::system())
    };
    let name = format!(
        "asrun-{}.csv",
        rows.first()
            .map(|e| local_day(e.at, &tz))
            .unwrap_or_else(|| local_day(crate::state::now_ms(), &tz))
    );
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
        ],
        csv(&rows, &tz, fr),
    ))
}

// ---------- CSV ----------

fn local_day(at: i64, tz: &jiff::tz::TimeZone) -> String {
    jiff::Timestamp::from_millisecond(at)
        .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
        .to_zoned(tz.clone())
        .strftime("%Y-%m-%d")
        .to_string()
}

/// "2026-10-24 20:14:31.20" in `tz`.
fn local_time(at: i64, tz: &jiff::tz::TimeZone) -> String {
    let z = jiff::Timestamp::from_millisecond(at)
        .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
        .to_zoned(tz.clone());
    format!(
        "{}.{:02}",
        z.strftime("%Y-%m-%d %H:%M:%S"),
        at.rem_euclid(1000) / 10
    )
}

/// A CSV field, quoted when it needs to be.
fn field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r', ';']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// "score: 1 → 2; chrono: 67'".
fn values_text(e: &Entry) -> String {
    e.values
        .iter()
        .map(|(k, (before, after))| match before {
            Some(b) if b != after => format!("{k}: {b} → {after}"),
            _ => format!("{k}: {after}"),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn csv(rows: &[Entry], tz: &jiff::tz::TimeZone, fr: bool) -> String {
    let head = if fr {
        "heure,canal,projet,action,graphique,zone,source,par,valeurs,détail"
    } else {
        "time,channel,project,action,graphic,zone,source,actor,values,detail"
    };
    // A BOM so spreadsheets read the accents.
    let mut out = format!("\u{feff}{head}\r\n");
    for e in rows {
        let cols = [
            local_time(e.at, tz),
            e.channel_name.clone(),
            e.project_name.clone(),
            e.action.clone(),
            e.label.clone(),
            e.zone.clone(),
            kind_name(e.actor.kind).to_string(),
            e.actor.name.clone(),
            values_text(e),
            e.detail.clone(),
        ];
        out.push_str(&cols.iter().map(|c| field(c)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(at: i64, channel: &str, action: &str, kind: ActorKind, who: &str) -> Entry {
        Entry {
            at,
            channel_id: channel.into(),
            channel_name: channel.to_uppercase(),
            project_id: "p".into(),
            project_name: "News".into(),
            action: action.into(),
            label: format!("{action} graphic"),
            zone: "lower".into(),
            actor: Actor {
                kind,
                id: who.into(),
                name: who.to_uppercase(),
            },
            values: BTreeMap::new(),
            detail: String::new(),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "strata-asrun-{name}-{}-{}",
            std::process::id(),
            crate::state::now_ms()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn writes_and_reads_back_across_days() {
        let data = temp_dir("days");
        // 2026-10-23 23:59:59.500 UTC and the next day.
        let d1 = 1_792_799_999_500;
        let d2 = d1 + 1_000;
        let mut upd = entry(d2 + 10, "main", "update", ActorKind::Data, "opta");
        upd.values
            .insert("score".into(), (Some("1".into()), "2".into()));
        for e in [
            entry(d1 - 60_000, "main", "take", ActorKind::User, "lea"),
            entry(d1, "sport", "out", ActorKind::User, "tom"),
            entry(d2, "main", "auto-next", ActorKind::Auto, ""),
            upd,
        ] {
            write(&data, e);
        }
        flush();
        let mut files: Vec<_> = std::fs::read_dir(data.join("asrun"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        files.sort();
        assert_eq!(files, ["2026-10-23.jsonl", "2026-10-24.jsonl"]);

        let all = |f: &Filter| read(&data, f, d2 + 1_000, 100, |_| true);
        let range = |f: Filter| Filter {
            from: Some(d1 - 3_600_000),
            to: Some(d2 + 100),
            ..f
        };
        let got = all(&range(Filter::default()));
        let actions: Vec<_> = got.iter().map(|e| e.action.as_str()).collect();
        assert_eq!(actions, ["update", "auto-next", "out", "take"]);

        // Default period: the 24 h before `now`.
        assert_eq!(all(&Filter::default()).len(), 4);
        // Only the second day.
        let f = Filter {
            from: Some(d2),
            to: Some(d2 + 100),
            ..Default::default()
        };
        assert_eq!(all(&f).len(), 2);
        // Filters.
        let n = |f: Filter| all(&range(f)).len();
        assert_eq!(
            n(Filter {
                channel: Some("sport".into()),
                ..Default::default()
            }),
            1
        );
        assert_eq!(
            n(Filter {
                actions: Some("take,auto".into()),
                ..Default::default()
            }),
            2
        );
        assert_eq!(
            n(Filter {
                kinds: Some("user".into()),
                ..Default::default()
            }),
            2
        );
        assert_eq!(
            n(Filter {
                actors: Some("lea,opta".into()),
                ..Default::default()
            }),
            2
        );
        assert_eq!(
            n(Filter {
                q: Some("SCORE".into()),
                ..Default::default()
            }),
            1
        );
        // What the user sees, and the limit (the newest kept).
        assert_eq!(
            read(&data, &range(Filter::default()), d2, 100, |e| e.channel_id
                == "main")
            .len(),
            3
        );
        let latest = read(&data, &range(Filter::default()), d2, 1, |_| true);
        assert_eq!(latest.len(), 1);
        assert_eq!(latest[0].action, "update");
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn csv_quotes_and_values() {
        let mut e = entry(1_792_799_999_567, "main", "update", ActorKind::User, "lea");
        e.label = "Score, \"live\"".into();
        e.detail = "two\nlines".into();
        e.values
            .insert("score".into(), (Some("1".into()), "2".into()));
        e.values.insert("chrono".into(), (None, "67'".into()));
        let out = csv(
            &[e],
            &jiff::tz::TimeZone::get("Europe/Paris").unwrap(),
            false,
        );
        let mut lines = out.trim_start_matches('\u{feff}').split("\r\n");
        assert_eq!(
            lines.next().unwrap(),
            "time,channel,project,action,graphic,zone,source,actor,values,detail"
        );
        assert_eq!(
            lines.next().unwrap(),
            "2026-10-24 01:59:59.56,MAIN,News,update,\"Score, \"\"live\"\"\",lower,user,LEA,\"chrono: 67'; score: 1 → 2\",\"two\nlines\""
        );
        assert_eq!(field("plain"), "plain");
        assert_eq!(field("a;b"), "\"a;b\"");
    }
}
