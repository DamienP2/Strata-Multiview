//! Salvos: several actions at once (the multiview of each channel, the
//! test generator, an input's backup, acknowledging the faults), run from
//! the Salvos page, a keyboard key, an address a Stream Deck or Companion
//! calls (`GET /api/salvos/{id}/run?key=smv_…`), or at set times.
//! Kept in `<data>/salvos.json`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::{bad, not_found, require, ApiResult};
use crate::auth::{CurrentUser, Role};
use crate::state::Shared;

const FILE: &str = "salvos.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    /// Loads a multiview on a channel (none: the slate).
    #[serde(rename_all = "camelCase")]
    Multiview {
        channel: String,
        project_id: Option<String>,
    },
    /// Switches a channel's test generator on or off.
    #[serde(rename_all = "camelCase")]
    Generator { channel: String, enabled: bool },
    /// Shows an input's backup, or the input again.
    #[serde(rename_all = "camelCase")]
    Backup { input: String, backup: bool },
    /// Acknowledges every fault.
    AckAll,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Schedule {
    /// "HH:MM", in the server's time zone.
    pub time: String,
    /// 1 Monday … 7 Sunday (none: every day).
    pub days: Vec<u8>,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            time: "08:00".into(),
            days: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Salvo {
    pub id: String,
    pub name: String,
    pub color: String,
    /// A key of the Salvos page: "F1", "1", "Ctrl+Shift+A"… ("" none).
    pub key: String,
    pub actions: Vec<Action>,
    pub schedules: Vec<Schedule>,
}

pub struct Salvos {
    file: PathBuf,
    list: RwLock<Vec<Salvo>>,
}

impl Salvos {
    pub fn load(data: &Path) -> Arc<Self> {
        let file = data.join(FILE);
        let list = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Arc::new(Self {
            file,
            list: RwLock::new(list),
        })
    }

    pub fn list(&self) -> Vec<Salvo> {
        self.list.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn set(&self, mut list: Vec<Salvo>) -> Result<(), String> {
        for (i, s) in list.iter_mut().enumerate() {
            if s.id.trim().is_empty() {
                s.id = format!("salvo-{}", i + 1);
            }
            for sc in &s.schedules {
                if parse_time(&sc.time).is_none() {
                    return Err(format!(
                        "« {} » : heure invalide « {} » (HH:MM)",
                        s.name, sc.time
                    ));
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        if let Some(d) = list.iter().find(|s| !seen.insert(s.id.clone())) {
            return Err(format!("deux salvos ont l’identifiant « {} »", d.id));
        }
        let tmp = self.file.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(&list).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.file).map_err(|e| e.to_string())?;
        *self.list.write().unwrap_or_else(|p| p.into_inner()) = list;
        Ok(())
    }
}

fn parse_time(s: &str) -> Option<(u8, u8)> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m) = (h.parse::<u8>().ok()?, m.parse::<u8>().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

/// Runs a salvo's actions in order; the errors, one per failed action.
pub async fn run(s: &Shared, salvo: &Salvo, by: &str) -> Vec<String> {
    let mut errors = Vec::new();
    for a in &salvo.actions {
        let r: Result<(), String> = match a {
            Action::Multiview {
                channel,
                project_id,
            } => s.load_project(channel, project_id.clone()).await,
            Action::Generator { channel, enabled } => match s.channel(channel) {
                Some(ch) => {
                    let mut g = ch
                        .generator
                        .read()
                        .unwrap_or_else(|p| p.into_inner())
                        .as_ref()
                        .clone();
                    g.enabled = *enabled;
                    *ch.generator.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(g);
                    Ok(())
                }
                None => Err(format!("no channel {channel}")),
            },
            Action::Backup { input, backup } => s.inputs.switch(input, *backup),
            Action::AckAll => {
                let ev = s.inputs.signals().ack(None, None, by);
                s.alarms.dispatch(ev);
                Ok(())
            }
        };
        if let Err(e) = r {
            errors.push(e);
        }
    }
    let _ = s.events.send(crate::state::Event::Channels {
        channels: s.channel_infos().await,
    });
    tracing::info!(salvo = salvo.name, by, errors = errors.len(), "salvo run");
    errors
}

/// Runs the salvos at their times, while the engine cycle runs.
pub fn spawn_schedule(s: Shared) {
    let mut closing = s.closing.subscribe();
    tokio::spawn(async move {
        let mut last = String::new();
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(10)) => {}
                _ = closing.wait_for(|c| *c) => break,
            }
            let tz = jiff::tz::TimeZone::get(&s.config.timezone)
                .unwrap_or_else(|_| jiff::tz::TimeZone::system());
            let now = jiff::Timestamp::now().to_zoned(tz);
            let minute = now.strftime("%Y-%m-%d %H:%M").to_string();
            if minute == last {
                continue;
            }
            last = minute;
            let (h, m) = (now.hour() as u8, now.minute() as u8);
            let day = now.weekday().to_monday_one_offset() as u8;
            for salvo in s.salvos.list() {
                let due = salvo.schedules.iter().any(|sc| {
                    parse_time(&sc.time) == Some((h, m))
                        && (sc.days.is_empty() || sc.days.contains(&day))
                });
                if due {
                    let errors = run(&s, &salvo, "planning").await;
                    for e in errors {
                        tracing::warn!("salvo « {} » at its time: {e}", salvo.name);
                    }
                }
            }
        }
    });
}

// ------------------------------------------------------------------ API --

pub async fn get_list(State(s): State<Shared>, CurrentUser(_me): CurrentUser) -> Json<Vec<Salvo>> {
    Json(s.salvos.list())
}

pub async fn put_list(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Json(list): Json<Vec<Salvo>>,
) -> ApiResult<StatusCode> {
    require(me.is_admin())?;
    let sv = s.salvos.clone();
    tokio::task::spawn_blocking(move || sv.set(list))
        .await
        .map_err(crate::api::internal)?
        .map_err(bad)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST` or `GET /api/salvos/{id}/run` (GET: a Stream Deck, with `?key=`).
pub async fn post_run(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    UrlPath(id): UrlPath<String>,
) -> ApiResult<Json<serde_json::Value>> {
    require(me.role != Role::Viewer)?;
    let salvo = s
        .salvos
        .list()
        .into_iter()
        .find(|x| x.id == id || x.name.eq_ignore_ascii_case(&id))
        .ok_or_else(|| not_found("salvo"))?;
    for a in &salvo.actions {
        if let Action::Multiview { channel, .. } | Action::Generator { channel, .. } = a {
            require(me.can_operate(channel))?;
        }
    }
    let errors = run(&s, &salvo, &me.username).await;
    Ok(Json(
        serde_json::json!({ "ok": errors.is_empty(), "errors": errors }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn salvos_are_read_and_checked() {
        let j = r#"[{"id":"news","name":"Journal","key":"F1","actions":[{"type":"multiview","channel":"main","projectId":"abc"},{"type":"generator","channel":"main","enabled":false},{"type":"ackAll"}],"schedules":[{"time":"19:58","days":[1,2,3,4,5]}]}]"#;
        let l: Vec<Salvo> = serde_json::from_str(j).unwrap();
        assert_eq!(
            l[0].actions[0],
            Action::Multiview {
                channel: "main".into(),
                project_id: Some("abc".into())
            }
        );
        assert_eq!(l[0].actions[2], Action::AckAll);
        assert_eq!(parse_time("19:58"), Some((19, 58)));
        assert_eq!(parse_time("24:00"), None);
        let tmp = std::env::temp_dir().join(format!("mv-salvos-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let s = Salvos::load(&tmp);
        let mut bad = l.clone();
        bad[0].schedules[0].time = "7h".into();
        assert!(s.set(bad).is_err());
        let mut two = l.clone();
        two.push(Salvo {
            id: String::new(),
            name: "B".into(),
            ..Default::default()
        });
        s.set(two).unwrap();
        assert_eq!(Salvos::load(&tmp).list()[1].id, "salvo-2");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
