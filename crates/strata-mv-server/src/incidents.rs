//! Incident recording: the pictures of each input around its faults.
//!
//! Every input's picture is kept twice a second for the last 15 seconds
//! (JPEG, 480 px wide). When a fault of an input is raised, those pictures
//! and the next 20 seconds are written to `<data>/incidents/<at>-<input>/`
//! (`000.jpg`…, `incident.json`): the alarm journal shows them as a
//! thumbnail and plays them back. Faults of the same input raised together
//! share one recording. The newest 300 recordings are kept.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::{not_found, ApiResult};
use crate::auth::CurrentUser;
use crate::signals::AlarmEvent;
use crate::state::Shared;

const EVERY_MS: u64 = 500;
/// Pictures kept before a fault (15 s).
const BEFORE: usize = 30;
/// Recorded after it.
const AFTER_MS: i64 = 20_000;
const WIDTH: u32 = 480;
const KEPT: usize = 300;
/// Faults of an input this close share a recording.
const JOIN_MS: i64 = 5_000;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// What is known of a recording (`incident.json`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Incident {
    /// Its folder's name.
    pub id: String,
    pub input_id: String,
    pub input: String,
    /// When the (first) fault began, ms since 1970.
    pub at: i64,
    /// The faults raised (camelCase names).
    pub kinds: Vec<String>,
    /// The time of each picture, ms since 1970.
    pub frames: Vec<i64>,
    /// Still recording.
    pub recording: bool,
}

struct Recording {
    meta: Incident,
    dir: PathBuf,
    until: i64,
}

#[derive(Default)]
struct Inner {
    rings: HashMap<String, VecDeque<(i64, Bytes)>>,
    active: Vec<Recording>,
}

pub struct Incidents {
    dir: PathBuf,
    inner: Mutex<Inner>,
}

fn kind_name(e: &AlarmEvent) -> String {
    serde_json::to_value(e.kind)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

fn safe(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn write_meta(dir: &Path, m: &Incident) {
    if let Ok(b) = serde_json::to_vec_pretty(m) {
        let tmp = dir.join("incident.json.tmp");
        if std::fs::write(&tmp, b).is_ok() {
            let _ = std::fs::rename(tmp, dir.join("incident.json"));
        }
    }
}

impl Incidents {
    pub fn new(data: &Path) -> Arc<Self> {
        let dir = data.join("incidents");
        // Recordings cut short by a stop: closed as they are.
        if let Ok(list) = std::fs::read_dir(&dir) {
            for d in list.flatten() {
                let p = d.path().join("incident.json");
                if let Some(mut m) = std::fs::read(&p)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Incident>(&b).ok())
                {
                    if m.recording {
                        m.recording = false;
                        write_meta(&d.path(), &m);
                    }
                }
            }
        }
        Arc::new(Self {
            dir,
            inner: Mutex::new(Inner::default()),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Samples the inputs' pictures twice a second, for as long as the server runs.
    pub fn spawn(self: &Arc<Self>, inputs: Arc<crate::inputs::Inputs>) {
        let me = self.clone();
        let _ = std::thread::Builder::new()
            .name("mv-incidents".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_millis(EVERY_MS));
                me.sample(&inputs);
            });
    }

    fn sample(&self, inputs: &crate::inputs::Inputs) {
        let at = now_ms();
        let live: Vec<String> = inputs
            .status()
            .into_iter()
            .filter(|s| s.live)
            .map(|s| s.id)
            .collect();
        let mut shots = Vec::new();
        for id in &live {
            if let Some((_, b)) = inputs.jpeg(id, WIDTH, None) {
                shots.push((id.clone(), b));
            }
        }
        let mut g = self.lock();
        g.rings.retain(|id, _| live.contains(id));
        for (id, b) in shots {
            let ring = g.rings.entry(id.clone()).or_default();
            // The same picture again (a frozen input): kept, it is what happened.
            ring.push_back((at, b.clone()));
            while ring.len() > BEFORE {
                ring.pop_front();
            }
            for r in g.active.iter_mut().filter(|r| r.meta.input_id == id) {
                let n = r.meta.frames.len();
                if std::fs::write(r.dir.join(format!("{n:03}.jpg")), &b).is_ok() {
                    r.meta.frames.push(at);
                }
            }
        }
        // Recordings over: closed.
        let mut done = Vec::new();
        g.active.retain_mut(|r| {
            if at >= r.until {
                r.meta.recording = false;
                done.push((r.dir.clone(), r.meta.clone()));
                false
            } else {
                true
            }
        });
        drop(g);
        for (dir, m) in done {
            write_meta(&dir, &m);
        }
    }

    /// Faults raised: their inputs' pictures recorded.
    pub fn raised(&self, events: &[AlarmEvent]) {
        let now = now_ms();
        let mut started = false;
        for e in events
            .iter()
            .filter(|e| e.state == "raised" && !e.input_id.is_empty())
        {
            let mut g = self.lock();
            if let Some(r) = g
                .active
                .iter_mut()
                .find(|r| r.meta.input_id == e.input_id && (e.at - r.meta.at).abs() <= JOIN_MS)
            {
                let k = kind_name(e);
                if !r.meta.kinds.contains(&k) {
                    r.meta.kinds.push(k);
                }
                continue;
            }
            // Nothing to show: an input never seen, a fault of the plant.
            if g.rings.get(&e.input_id).is_none_or(|r| r.is_empty()) {
                continue;
            }
            let id = format!("{}-{}", e.at, safe(&e.input_id));
            let dir = self.dir.join(&id);
            if std::fs::create_dir_all(&dir).is_err() {
                continue;
            }
            let mut frames = Vec::new();
            for (t, b) in g.rings.get(&e.input_id).into_iter().flatten() {
                if std::fs::write(dir.join(format!("{:03}.jpg", frames.len())), b).is_ok() {
                    frames.push(*t);
                }
            }
            let meta = Incident {
                id,
                input_id: e.input_id.clone(),
                input: e.input.clone(),
                at: e.at,
                kinds: vec![kind_name(e)],
                frames,
                recording: true,
            };
            write_meta(&dir, &meta);
            g.active.push(Recording {
                meta,
                dir,
                until: now + AFTER_MS,
            });
            started = true;
        }
        if started {
            self.prune();
        }
    }

    fn prune(&self) {
        let Ok(list) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let mut dirs: Vec<(String, PathBuf)> = list
            .flatten()
            .filter(|d| d.path().is_dir())
            .map(|d| (d.file_name().to_string_lossy().to_string(), d.path()))
            .collect();
        if dirs.len() <= KEPT {
            return;
        }
        // "<ms>-<input>": oldest first.
        dirs.sort_by_key(|(n, _)| {
            n.split('-')
                .next()
                .and_then(|t| t.parse::<i64>().ok())
                .unwrap_or(0)
        });
        let extra = dirs.len() - KEPT;
        for (_, p) in dirs.into_iter().take(extra) {
            let _ = std::fs::remove_dir_all(p);
        }
    }

    /// Every recording, newest first (the active ones as they are now).
    pub fn list(&self) -> Vec<Incident> {
        let active: Vec<Incident> = self.lock().active.iter().map(|r| r.meta.clone()).collect();
        let mut out = active.clone();
        if let Ok(list) = std::fs::read_dir(&self.dir) {
            for d in list.flatten() {
                if let Some(m) = std::fs::read(d.path().join("incident.json"))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Incident>(&b).ok())
                {
                    if !active.iter().any(|a| a.id == m.id) {
                        out.push(m);
                    }
                }
            }
        }
        out.sort_by_key(|m| std::cmp::Reverse(m.at));
        out
    }

    fn picture(&self, id: &str, n: usize) -> Option<Vec<u8>> {
        if id.contains(['/', '\\']) || id.starts_with('.') {
            return None;
        }
        std::fs::read(self.dir.join(id).join(format!("{n:03}.jpg"))).ok()
    }
}

// ------------------------------------------------------------------ API --

/// `GET /api/incidents`.
pub async fn list(
    State(s): State<Shared>,
    CurrentUser(_me): CurrentUser,
) -> ApiResult<Json<Vec<Incident>>> {
    let inc = s.incidents.clone();
    Ok(Json(
        tokio::task::spawn_blocking(move || inc.list())
            .await
            .map_err(crate::api::internal)?,
    ))
}

#[derive(Deserialize)]
pub struct Frame {
    id: String,
    n: String,
}

/// `GET /api/incidents/{id}/{n}.jpg` (`thumb.jpg`: the picture when the fault began).
pub async fn picture(
    State(s): State<Shared>,
    CurrentUser(_me): CurrentUser,
    UrlPath(f): UrlPath<Frame>,
) -> ApiResult<Response> {
    let inc = s.incidents.clone();
    let res = tokio::task::spawn_blocking(move || {
        let n = match f.n.strip_suffix(".jpg") {
            Some("thumb") => {
                let m = inc.list().into_iter().find(|m| m.id == f.id)?;
                m.frames.iter().position(|t| *t >= m.at).unwrap_or(0)
            }
            Some(n) => n.parse().ok()?,
            None => return None,
        };
        inc.picture(&f.id, n)
    })
    .await
    .map_err(crate::api::internal)?;
    match res {
        Some(b) => Ok((
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "max-age=86400"),
            ],
            b,
        )
            .into_response()),
        None => Err(not_found("picture")),
    }
}

/// `DELETE /api/incidents/{id}` (admin).
pub async fn delete(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    UrlPath(id): UrlPath<String>,
) -> ApiResult<StatusCode> {
    crate::api::require(me.is_admin())?;
    if id.contains(['/', '\\']) || id.starts_with('.') || id.is_empty() {
        return Err(not_found("incident"));
    }
    std::fs::remove_dir_all(s.incidents.dir.join(&id)).map_err(|_| not_found("incident"))?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_mv_core::signal::{AlarmKind, Severity};

    fn ev(input: &str, kind: AlarmKind, at: i64) -> AlarmEvent {
        AlarmEvent {
            state: "raised",
            input_id: input.into(),
            input: input.to_uppercase(),
            kind,
            severity: Severity::Critical,
            at,
        }
    }

    #[test]
    fn a_fault_records_before_and_after() {
        let tmp = std::env::temp_dir().join(format!("mv-incidents-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let inc = Incidents::new(&tmp);
        // Pictures before the fault.
        {
            let mut g = inc.lock();
            let ring = g.rings.entry("cam1".into()).or_default();
            for i in 0..40 {
                ring.push_back((1000 + i * 500, Bytes::from_static(b"\xff\xd8jpeg")));
                while ring.len() > BEFORE {
                    ring.pop_front();
                }
            }
        }
        let at = now_ms();
        inc.raised(&[
            ev("cam1", AlarmKind::Black, at),
            ev("cam1", AlarmKind::Silence, at + 1000),
            ev("cam2", AlarmKind::Loss, at),
        ]);
        let l = inc.list();
        // cam1's two faults share a recording; cam2 had no picture: none.
        assert_eq!(l.len(), 1);
        let c1 = l.iter().find(|m| m.input_id == "cam1").unwrap();
        assert_eq!(c1.kinds, vec!["black", "silence"]);
        assert_eq!(c1.frames.len(), BEFORE);
        assert!(c1.recording);
        assert!(inc.picture(&c1.id, 0).is_some());
        assert!(inc.picture("../x", 0).is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
