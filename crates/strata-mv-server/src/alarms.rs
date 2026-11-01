//! The alarms: their settings (`<data>/alarms.json`: profiles of faults the
//! inputs share, the plant's faults, the actions), their journal kept from
//! one start to the next (`<data>/alarms-journal.json`), the calls to other
//! systems when a fault is raised, ends or is acknowledged (an automation,
//! a GPO box, a messenger), and their API (the Alarms page).
//!
//! The faults themselves are looked at by `crate::signals` (`evaluate`).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use strata_mv_core::alarm::AlarmSettings;
use strata_mv_core::signal::{AlarmKind, JournalEntry, Severity};

use crate::api::{bad, require, ApiResult};
use crate::auth::{CurrentUser, Role};
use crate::signals::{AlarmEvent, Signals};
use crate::state::Shared;

const SETTINGS: &str = "alarms.json";
const JOURNAL: &str = "alarms-journal.json";

pub struct Alarms {
    dir: PathBuf,
    settings: Arc<RwLock<AlarmSettings>>,
    calls: std::sync::mpsc::Sender<AlarmEvent>,
    /// The journal's version last saved.
    saved: AtomicU64,
}

impl Alarms {
    /// Reads the settings and the journal, gives them to `signals`.
    pub fn load(dir: &Path, signals: &Signals) -> Arc<Self> {
        let mut s: AlarmSettings = std::fs::read(dir.join(SETTINGS))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        s.normalize();
        signals.set_settings(s.clone());
        if let Some(lines) = std::fs::read(dir.join(JOURNAL))
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<JournalEntry>>(&b).ok())
        {
            signals.load_journal(lines);
        }
        let settings = Arc::new(RwLock::new(s));
        let (tx, rx) = std::sync::mpsc::channel::<AlarmEvent>();
        let shared = settings.clone();
        let _ = std::thread::Builder::new()
            .name("mv-alarm-calls".into())
            .spawn(move || {
                for ev in rx {
                    let s = shared.read().unwrap_or_else(|p| p.into_inner()).clone();
                    call(&s, &ev);
                }
            });
        Arc::new(Self {
            dir: dir.to_path_buf(),
            settings,
            calls: tx,
            saved: AtomicU64::new(signals.journal_version()),
        })
    }

    pub fn settings(&self) -> AlarmSettings {
        self.settings
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// New settings: saved, followed at once.
    pub fn set_settings(&self, mut s: AlarmSettings, signals: &Signals) -> Result<(), String> {
        s.normalize();
        for w in &s.webhooks {
            let u = w.url.trim();
            if w.enabled && !(u.starts_with("http://") || u.starts_with("https://")) {
                return Err(format!(
                    "« {} » : l’adresse doit commencer par http:// ou https://",
                    w.name
                ));
            }
        }
        let tmp = self.dir.join(format!("{SETTINGS}.tmp"));
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(&s).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, self.dir.join(SETTINGS)).map_err(|e| e.to_string())?;
        signals.set_settings(s.clone());
        *self.settings.write().unwrap_or_else(|p| p.into_inner()) = s;
        Ok(())
    }

    /// Faults raised, over or acknowledged: to the systems called.
    pub fn dispatch(&self, events: Vec<AlarmEvent>) {
        for e in events {
            tracing::info!(
                "alarm {}: {} {:?} ({:?})",
                e.state,
                e.input,
                e.kind,
                e.severity
            );
            let _ = self.calls.send(e);
        }
    }

    /// Saves the journal when it changed (called now and then).
    pub fn save_journal(&self, signals: &Signals) {
        let v = signals.journal_version();
        if self.saved.swap(v, Ordering::Relaxed) == v {
            return;
        }
        let (_, lines) = signals.journal();
        let tmp = self.dir.join(format!("{JOURNAL}.tmp"));
        let ok = serde_json::to_vec(&lines)
            .ok()
            .and_then(|b| std::fs::write(&tmp, b).ok())
            .and_then(|_| std::fs::rename(&tmp, self.dir.join(JOURNAL)).ok());
        if ok.is_none() {
            tracing::warn!("cannot save the alarm journal");
        }
    }
}

/// A URL with the fault's words in it.
fn fill(url: &str, ev: &AlarmEvent) -> String {
    let enc = |s: &str| -> String {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect()
    };
    let kind = serde_json::to_value(ev.kind)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    let severity = match ev.severity {
        Severity::Critical => "critical",
        Severity::Warning => "warning",
    };
    url.replace("{input}", &enc(&ev.input))
        .replace("{inputId}", &enc(&ev.input_id))
        .replace("{kind}", &enc(&kind))
        .replace("{severity}", severity)
        .replace("{state}", ev.state)
        .replace("{text}", &enc(ev.kind.text("fr")))
}

/// Calls the systems set for a fault (5 s at most each).
fn call(s: &AlarmSettings, ev: &AlarmEvent) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .timeout_global(Some(Duration::from_secs(5)))
        .user_agent(concat!("strata-multiview/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    for w in s.webhooks.iter().filter(|w| w.enabled) {
        if ev.severity < w.min_severity || (ev.state == "ended" && !w.on_end) {
            continue;
        }
        let url = fill(w.url.trim(), ev);
        let res = if w.method.eq_ignore_ascii_case("GET") {
            agent.get(&url).call().map(|_| ())
        } else {
            let body = serde_json::json!({
                "state": ev.state,
                "inputId": ev.input_id,
                "input": ev.input,
                "kind": ev.kind,
                "severity": ev.severity,
                "text": ev.kind.text("fr"),
                "at": ev.at,
            });
            agent
                .post(&url)
                .header("content-type", "application/json")
                .send(body.to_string())
                .map(|_| ())
        };
        if let Err(e) = res {
            tracing::warn!("alarm call « {} » failed: {e}", w.name);
        }
    }
}

// ------------------------------------------------------------------ API --

pub fn router() -> Router<Shared> {
    Router::new()
        .route("/api/alarms", get(get_alarms))
        .route("/api/alarms/settings", put(put_settings))
        .route("/api/alarms/ack", post(ack))
        .route("/api/alarms/test", post(test_call))
        .route("/api/alarms/journal.csv", get(journal_csv))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AlarmsView {
    settings: AlarmSettings,
    journal: Vec<JournalEntry>,
}

async fn get_alarms(State(s): State<Shared>, CurrentUser(_me): CurrentUser) -> Json<AlarmsView> {
    Json(AlarmsView {
        settings: s.alarms.settings(),
        journal: s.inputs.signals().journal().1,
    })
}

async fn put_settings(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Json(c): Json<AlarmSettings>,
) -> ApiResult<StatusCode> {
    require(me.is_admin())?;
    s.alarms.set_settings(c, s.inputs.signals()).map_err(bad)?;
    s.changed("alarms");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AckBody {
    /// An input (or a part of the plant: "~ptp"…); none: every one.
    #[serde(default)]
    input_id: Option<String>,
    #[serde(default)]
    kind: Option<AlarmKind>,
}

/// Acknowledges faults on now (operators and up).
async fn ack(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Json(b): Json<AckBody>,
) -> ApiResult<Json<serde_json::Value>> {
    require(matches!(
        me.role,
        Role::Admin | Role::Designer | Role::Operator
    ))?;
    let who = if me.display_name.trim().is_empty() {
        me.username.clone()
    } else {
        me.display_name.clone()
    };
    let events = s.inputs.signals().ack(b.input_id.as_deref(), b.kind, &who);
    let n = events.len();
    s.alarms.dispatch(events);
    Ok(Json(serde_json::json!({ "acked": n })))
}

#[derive(Deserialize)]
struct TestBody {
    url: String,
    #[serde(default)]
    method: String,
}

/// Tries a call (the Alarms page's « Essayer »): its answer.
async fn test_call(
    State(_s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Json(b): Json<TestBody>,
) -> ApiResult<Json<serde_json::Value>> {
    require(me.is_admin())?;
    let res = tokio::task::spawn_blocking(move || {
        let ev = AlarmEvent {
            state: "raised",
            input_id: "test".into(),
            input: "TEST".into(),
            kind: AlarmKind::Freeze,
            severity: Severity::Critical,
            at: 0,
        };
        let s = AlarmSettings {
            webhooks: vec![strata_mv_core::alarm::Webhook {
                name: "test".into(),
                url: b.url,
                method: if b.method.is_empty() { "POST".into() } else { b.method },
                ..Default::default()
            }],
            ..Default::default()
        };
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(5)))
            .build()
            .into();
        let w = &s.webhooks[0];
        let url = fill(w.url.trim(), &ev);
        let r = if w.method.eq_ignore_ascii_case("GET") {
            agent.get(&url).call()
        } else {
            agent
                .post(&url)
                .header("content-type", "application/json")
                .send(r#"{"state":"raised","input":"TEST","kind":"freeze","severity":"critical","text":"TEST"}"#)
        };
        match r {
            Ok(resp) => serde_json::json!({ "ok": true, "status": resp.status().as_u16() }),
            Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
        }
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(res))
}

#[derive(Deserialize)]
struct CsvQuery {
    #[serde(default)]
    tz: Option<String>,
}

/// The journal as CSV (one line a fault), times in the server's zone.
async fn journal_csv(
    State(s): State<Shared>,
    CurrentUser(_me): CurrentUser,
    Query(q): Query<CsvQuery>,
) -> Response {
    let tz =
        q.tz.as_deref()
            .and_then(|z| jiff::tz::TimeZone::get(z).ok())
            .unwrap_or_else(jiff::tz::TimeZone::system);
    let when = |ms: i64| -> String {
        jiff::Timestamp::from_millisecond(ms)
            .map(|t| {
                t.to_zoned(tz.clone())
                    .strftime("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_default()
    };
    let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut out =
        String::from("\u{feff}Début;Fin;Durée (s);Source;Alarme;Gravité;Acquittée;Par\r\n");
    for e in s.inputs.signals().journal().1 {
        let dur = e
            .ended
            .map(|t| ((t - e.at) as f64 / 1000.0).round().to_string());
        out.push_str(&format!(
            "{};{};{};{};{};{};{};{}\r\n",
            when(e.at),
            e.ended.map(when).unwrap_or_default(),
            dur.unwrap_or_default(),
            q(&e.input),
            q(e.kind.text("fr")),
            match e.severity {
                Severity::Critical => "critique",
                Severity::Warning => "avertissement",
            },
            e.acked.map(when).unwrap_or_default(),
            q(&e.by),
        ));
    }
    (
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"journal-alarmes.csv\"",
            ),
        ],
        out,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_with_the_fault_in_it() {
        let ev = AlarmEvent {
            state: "raised",
            input_id: "cam1".into(),
            input: "CAM 1".into(),
            kind: AlarmKind::Freeze,
            severity: Severity::Critical,
            at: 0,
        };
        assert_eq!(
            fill(
                "http://gpo/set?out=1&src={input}&k={kind}&s={severity}&st={state}",
                &ev
            ),
            "http://gpo/set?out=1&src=CAM%201&k=freeze&s=critical&st=raised"
        );
    }
}
