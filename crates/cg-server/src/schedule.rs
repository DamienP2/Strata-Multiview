//! Takes and outs programmed in advance ("prises programmées"): at a time of
//! day, in some seconds, or at a time-of-day timecode; once or every day.
//!
//! Each entry fires through `AppState::command`, like a person's take (undo,
//! as-run log), as an `ActorKind::Schedule` actor. An entry whose project is
//! not loaded on its channel when its time comes is skipped: a daily one
//! waits for the next day, a single one stays in the list, missed.
//!
//! Timecode here is the time of day in the server's time zone, counted in
//! frames of the channel (drop-frame notation, `;`, at 29.97 and 59.94).
//!
//! TODO(ltc): an external timecode (LTC, RP188 / VITC on an SDI input) is not
//! read: the DeckLink shim (crates/cg-output, its C++ part) would have to read
//! `IDeckLinkVideoInputFrame::GetTimecode` on an input and hand it to the
//! server; the entries would then compare `tc_ms_of_day` with that clock
//! instead of the server's time of day.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::Json;
use cg_core::{Command, FrameRate};
use jiff::civil::Time;
use jiff::tz::TimeZone;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

use crate::antenna::{Actor, ActorKind};
use crate::api::{bad, not_found, require, ApiResult};
use crate::auth::{CurrentUser, User};
use crate::state::{now_ms, AppState, Event, Shared};

/// An entry late by up to this (a busy server) still fires; later, it is missed.
const GRACE_MS: i64 = 2_000;
/// Longest sleep of the loop: a clock set again is noticed within it.
const MAX_SLEEP_MS: i64 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Take,
    Out,
}

/// When an entry fires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum When {
    /// A time of day "HH:MM:SS" in the server's time zone.
    At { time: String },
    /// A moment (server clock, ms); created from "in … seconds" (`secs`).
    In {
        #[serde(default)]
        at: i64,
        #[serde(default, skip_serializing)]
        secs: Option<f64>,
    },
    /// A time-of-day timecode "HH:MM:SS:FF" at the channel's frame rate.
    Timecode { tc: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Repeat {
    #[default]
    Once,
    Daily,
}

/// A take or an out programmed on a channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scheduled {
    pub id: String,
    pub channel_id: String,
    pub project_id: String,
    pub action: Action,
    /// The rundown item taken (or whose zone goes out).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    /// The zone that goes out (an out without item).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone: Option<String>,
    pub when: When,
    #[serde(default)]
    pub repeat: Repeat,
    /// Who programmed it.
    #[serde(default)]
    pub created_by: String,
    pub enabled: bool,
    /// The item's (or zone's) name when it was programmed.
    #[serde(default)]
    pub label: String,
    /// Next time it fires (server clock, ms).
    #[serde(default)]
    pub next: i64,
    /// A single entry whose time passed without firing (its project was not
    /// on the channel, the server was off).
    #[serde(default)]
    pub missed: bool,
}

/// An entry as clients see it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    #[serde(flatten)]
    pub entry: Scheduled,
    /// Its project is not the one loaded on the channel: it is skipped.
    pub waiting: bool,
}

/// Every programmed entry, kept in `<data_dir>/schedule.json`.
pub struct Schedule {
    path: PathBuf,
    items: Mutex<Vec<Scheduled>>,
    /// Wakes the loop when the list changed.
    changed: Notify,
}

impl Schedule {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("schedule.json");
        let items = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            path,
            items: Mutex::new(items),
            changed: Notify::new(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Scheduled>> {
        self.items.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn save(&self, items: &[Scheduled]) {
        let res = (|| -> std::io::Result<()> {
            let tmp = self.path.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_vec_pretty(items)?)?;
            std::fs::rename(tmp, &self.path)
        })();
        if let Err(e) = res {
            tracing::warn!("cannot save the scheduled takes: {e}");
        }
    }

    /// Changes the list, saves it and wakes the loop.
    fn edit<R>(&self, f: impl FnOnce(&mut Vec<Scheduled>) -> R) -> R {
        let mut items = self.lock();
        let r = f(&mut items);
        self.save(&items);
        drop(items);
        self.changed.notify_one();
        r
    }

    pub fn of_channel(&self, channel: &str) -> Vec<Scheduled> {
        let mut out: Vec<Scheduled> = self
            .lock()
            .iter()
            .filter(|e| e.channel_id == channel)
            .cloned()
            .collect();
        out.sort_by_key(|e| (e.missed || !e.enabled, e.next));
        out
    }
}

// ------------------------------------------------------------- time ----

/// The server's time zone (`""`: the system's).
pub fn tz_of(name: &str) -> TimeZone {
    let n = name.trim();
    if n.is_empty() {
        return TimeZone::system();
    }
    TimeZone::get(n).unwrap_or_else(|_| TimeZone::system())
}

const DAY_MS: i64 = 86_400_000;

/// "HH:MM:SS" (or "HH:MM") → ms since midnight.
pub fn parse_time(s: &str) -> Result<i64, String> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    let num = |p: &str, max: i64| {
        p.trim()
            .parse::<i64>()
            .ok()
            .filter(|v| (0..max).contains(v))
    };
    let (h, m, sec) = match parts.as_slice() {
        [h, m] => (num(h, 24), num(m, 60), Some(0)),
        [h, m, s] => (num(h, 24), num(m, 60), num(s, 60)),
        _ => (None, None, None),
    };
    match (h, m, sec) {
        (Some(h), Some(m), Some(s)) => Ok(((h * 60 + m) * 60 + s) * 1000),
        _ => Err(format!("`{s}` is not a time (HH:MM:SS)")),
    }
}

/// Nominal frames per second of a rate, and the frames dropped each minute
/// in drop-frame notation (2 at 29.97, 4 at 59.94, 0 otherwise).
pub fn tc_rate(rate: FrameRate) -> (i64, i64) {
    let fps = rate.as_f64();
    let nominal = fps.round().max(1.0) as i64;
    let fractional = (fps - nominal as f64).abs() > 1e-3;
    let drop = if fractional && nominal % 30 == 0 {
        nominal / 15
    } else {
        0
    };
    (nominal, drop)
}

/// The time-of-day timecode of `ms` since midnight at this rate:
/// "HH:MM:SS:FF", or "HH:MM:SS;FF" in drop frame.
pub fn tc_of_day(ms: i64, rate: FrameRate) -> String {
    let (n, d) = tc_rate(rate);
    let ms = ms.rem_euclid(DAY_MS);
    if d == 0 {
        let secs = ms / 1000;
        let ff = (ms % 1000) * n / 1000;
        return format!(
            "{:02}:{:02}:{:02}:{:02}",
            secs / 3600,
            secs / 60 % 60,
            secs % 60,
            ff
        );
    }
    // Frames counted at the real rate since midnight, written in drop frame.
    let per_10 = n * 600 - d * 9;
    let per_min = n * 60 - d;
    let day = per_10 * 6 * 24;
    let mut f = ((ms as i128 * rate.num as i128) / (1000 * rate.den.max(1) as i128)) as i64 % day;
    let tens = f / per_10;
    let rem = f % per_10;
    f += d * 9 * tens;
    if rem > d {
        f += d * ((rem - d) / per_min);
    }
    let ff = f % n;
    let secs = f / n;
    format!(
        "{:02}:{:02}:{:02};{:02}",
        secs / 3600 % 24,
        secs / 60 % 60,
        secs % 60,
        ff
    )
}

/// The ms since midnight a time-of-day timecode stands for at this rate
/// (the start of that frame, rounded up to the ms).
pub fn tc_ms_of_day(tc: &str, rate: FrameRate) -> Result<i64, String> {
    let (n, d) = tc_rate(rate);
    let parts: Vec<i64> = tc
        .trim()
        .split([':', ';', '.'])
        .map(|p| p.trim().parse::<i64>().map_err(|_| ()))
        .collect::<Result<_, _>>()
        .map_err(|_| format!("`{tc}` is not a timecode (HH:MM:SS:FF)"))?;
    let [h, m, s, f] = parts[..] else {
        return Err(format!("`{tc}` is not a timecode (HH:MM:SS:FF)"));
    };
    if !(0..24).contains(&h)
        || !(0..60).contains(&m)
        || !(0..60).contains(&s)
        || !(0..n).contains(&f)
    {
        return Err(format!("`{tc}` is not a timecode at {n} frames a second"));
    }
    if d == 0 {
        return Ok(((h * 60 + m) * 60 + s) * 1000 + (f * 1000 + n - 1) / n);
    }
    // Frames 0..d of each minute but every tenth do not exist in drop frame.
    let f = if s == 0 && m % 10 != 0 { f.max(d) } else { f };
    let minutes = h * 60 + m;
    let frames = n * (minutes * 60 + s) + f - d * (minutes - minutes / 10);
    let num = frames as i128 * 1000 * rate.den.max(1) as i128;
    let den = rate.num.max(1) as i128;
    Ok(((num + den - 1) / den) as i64)
}

/// ms since local midnight of the moment `ms` in `tz`.
pub fn ms_of_day(tz: &TimeZone, ms: i64) -> i64 {
    let Ok(ts) = Timestamp::from_millisecond(ms) else {
        return 0;
    };
    let t = ts.to_zoned(tz.clone()).time();
    ((t.hour() as i64 * 60 + t.minute() as i64) * 60 + t.second() as i64) * 1000
        + t.subsec_nanosecond() as i64 / 1_000_000
}

/// The first moment after `after` (ms) when the local time of day is `tod`
/// (ms since midnight) in `tz`. In a DST gap the time moves forward with
/// the clocks; in a fold the first of the two moments is taken.
pub fn next_time_of_day(tz: &TimeZone, tod: i64, after: i64) -> Option<i64> {
    let tod = tod.rem_euclid(DAY_MS);
    let time = Time::new(
        (tod / 3_600_000) as i8,
        (tod / 60_000 % 60) as i8,
        (tod / 1000 % 60) as i8,
        (tod % 1000) as i32 * 1_000_000,
    )
    .ok()?;
    let mut date = Timestamp::from_millisecond(after)
        .ok()?
        .to_zoned(tz.clone())
        .date();
    for _ in 0..3 {
        let at = date.to_datetime(time).to_zoned(tz.clone()).ok()?;
        let ms = at.timestamp().as_millisecond();
        if ms > after {
            return Some(ms);
        }
        date = date.tomorrow().ok()?;
    }
    None
}

/// When an entry fires next after `after` (ms): `None` when never again.
pub fn next_fire(
    when: &When,
    repeat: Repeat,
    tz: &TimeZone,
    rate: FrameRate,
    after: i64,
) -> Result<Option<i64>, String> {
    Ok(match when {
        When::At { time } => next_time_of_day(tz, parse_time(time)?, after),
        When::Timecode { tc } => next_time_of_day(tz, tc_ms_of_day(tc, rate)?, after),
        When::In { at, .. } if *at > after => Some(*at),
        When::In { at, .. } => match repeat {
            Repeat::Daily => next_time_of_day(tz, ms_of_day(tz, *at), after),
            Repeat::Once => None,
        },
    })
}

/// "20:00:00", "TC 20:41:30:00": what the as-run log shows of an entry.
fn when_label(e: &Scheduled, tz: &TimeZone) -> String {
    match &e.when {
        When::At { time } => time.clone(),
        When::Timecode { tc } => format!("TC {tc}"),
        When::In { at, .. } => {
            let t = ms_of_day(tz, *at) / 1000;
            format!("{:02}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60)
        }
    }
}

// ------------------------------------------------------------- loop ----

impl AppState {
    fn tz(&self) -> TimeZone {
        tz_of(&self.config.timezone)
    }

    fn rate_of(&self, channel: &str) -> FrameRate {
        self.channel(channel)
            .map(|c| c.config.frame_rate)
            .unwrap_or_default()
    }

    async fn schedule_views(&self, channel: &str) -> Vec<View> {
        let loaded = match self.channel(channel) {
            Some(c) => c.project_id.read().await.clone(),
            None => None,
        };
        self.schedule
            .of_channel(channel)
            .into_iter()
            .map(|e| View {
                waiting: loaded.as_deref() != Some(e.project_id.as_str()),
                entry: e,
            })
            .collect()
    }

    /// Tells the clients what is programmed on a channel.
    async fn broadcast_schedule(&self, channel: &str) {
        let _ = self.events.send(Event::Schedule {
            channel_id: channel.into(),
            items: self.schedule_views(channel).await,
        });
    }

    /// The command an entry sends, from the project loaded on its channel.
    async fn schedule_command(&self, e: &Scheduled) -> Result<Command, String> {
        let project = self
            .project(&e.project_id)
            .await
            .ok_or("its project no longer exists")?;
        let item = match &e.item {
            Some(id) => Some(
                project
                    .rundown
                    .iter()
                    .find(|r| &r.id == id)
                    .ok_or("its graphic is no longer in the rundown")?,
            ),
            None => None,
        };
        let zone = match (&e.zone, item) {
            (Some(z), _) => Some(z.clone()),
            (None, Some(it)) => project.item_zone(it).map(|z| z.id.clone()),
            (None, None) => None,
        };
        Ok(match (e.action, item) {
            (Action::Take, Some(it)) => Command::Take {
                zone,
                layer: None,
                template_id: it.template_id.clone(),
                data: Some(it.data.clone()),
                rundown_id: Some(it.id.clone()),
            },
            (Action::Take, None) => return Err("nothing to take".into()),
            (Action::Out, _) => Command::TakeOut {
                zone: Some(zone.ok_or("no zone to take out")?),
                layer: None,
            },
        })
    }

    /// Fires a due entry (when its project is on air), then rolls it to the
    /// next day, removes it, or marks it missed.
    async fn fire_scheduled(&self, e: Scheduled, now: i64) {
        let loaded = match self.channel(&e.channel_id) {
            Some(c) => c.project_id.read().await.as_deref() == Some(e.project_id.as_str()),
            None => false,
        };
        let late = now - e.next > GRACE_MS;
        let tz = self.tz();
        let mut fired = false;
        if loaded && !late {
            let actor = Actor {
                kind: ActorKind::Schedule,
                id: e.id.clone(),
                name: format!("⏱ {} · {}", when_label(&e, &tz), e.created_by),
            };
            match self.schedule_command(&e).await {
                Ok(cmd) => match self.command(&e.channel_id, cmd, &actor).await {
                    Ok(_) => {
                        fired = true;
                        let rate = self.rate_of(&e.channel_id);
                        let tc = tc_of_day(ms_of_day(&tz, now_ms()), rate);
                        tracing::info!("scheduled {} fired ({}) at TC {tc}", e.label, actor.name);
                    }
                    Err(err) => tracing::warn!("scheduled {}: {err}", e.label),
                },
                Err(err) => tracing::warn!("scheduled {}: {err}", e.label),
            }
        } else if late {
            tracing::warn!("scheduled {} missed (server busy or off)", e.label);
        }
        let rate = self.rate_of(&e.channel_id);
        let next = next_fire(&e.when, e.repeat, &tz, rate, now.max(e.next))
            .ok()
            .flatten();
        self.schedule.edit(|items| {
            let Some(i) = items.iter().position(|x| x.id == e.id) else {
                return;
            };
            match (e.repeat, next) {
                (Repeat::Daily, Some(n)) => items[i].next = n,
                _ if fired => {
                    items.remove(i);
                }
                _ => items[i].missed = true,
            }
        });
        self.broadcast_schedule(&e.channel_id).await;
    }
}

/// Fires the entries when their time comes (sleeps until the next one).
pub async fn run(s: Shared) {
    loop {
        let now = now_ms();
        let due: Vec<Scheduled> = s
            .schedule
            .lock()
            .iter()
            .filter(|e| e.enabled && !e.missed && e.next > 0 && e.next <= now)
            .cloned()
            .collect();
        for e in due {
            s.fire_scheduled(e, now).await;
        }
        let soonest = s
            .schedule
            .lock()
            .iter()
            .filter(|e| e.enabled && !e.missed && e.next > 0)
            .map(|e| e.next)
            .min();
        let wait = soonest
            .map(|n| n - now_ms())
            .unwrap_or(MAX_SLEEP_MS)
            .clamp(1, MAX_SLEEP_MS);
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(wait as u64)) => {}
            _ = s.schedule.changed.notified() => {}
        }
    }
}

// -------------------------------------------------------------- api ----

fn visible(me: &User, channel: &str) -> ApiResult<()> {
    if me.sees_channel(channel) {
        Ok(())
    } else {
        Err(not_found("channel"))
    }
}

/// `GET /api/channels/{id}/schedule`: what is programmed on a channel.
pub async fn list(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    UrlPath(id): UrlPath<String>,
) -> ApiResult<Json<Vec<View>>> {
    visible(&me, &id)?;
    s.channel(&id).ok_or_else(|| not_found("channel"))?;
    let all = s.schedule_views(&id).await;
    Ok(Json(
        all.into_iter()
            .filter(|v| me.sees_project(&v.entry.project_id))
            .collect(),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewEntry {
    /// Default: the project loaded on the channel.
    #[serde(default)]
    project_id: Option<String>,
    action: Action,
    #[serde(default)]
    item: Option<String>,
    #[serde(default)]
    zone: Option<String>,
    when: When,
    #[serde(default)]
    repeat: Repeat,
}

/// `POST /api/channels/{id}/schedule`: programs a take or an out.
pub async fn create(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    UrlPath(id): UrlPath<String>,
    Json(input): Json<NewEntry>,
) -> ApiResult<(StatusCode, Json<Scheduled>)> {
    visible(&me, &id)?;
    require(me.can_operate(&id))?;
    let ch = s.channel(&id).ok_or_else(|| not_found("channel"))?;
    let pid = match input.project_id {
        Some(p) => p,
        None => ch
            .project_id
            .read()
            .await
            .clone()
            .ok_or_else(|| bad("no project loaded on this channel"))?,
    };
    if !me.sees_project(&pid) {
        return Err(not_found("project"));
    }
    let project = s.project(&pid).await.ok_or_else(|| not_found("project"))?;
    let item = match &input.item {
        Some(i) => Some(
            project
                .rundown
                .iter()
                .find(|r| &r.id == i)
                .ok_or_else(|| not_found("rundown item"))?,
        ),
        None => None,
    };
    let zone = match &input.zone {
        Some(z) => Some(project.zone(z).ok_or_else(|| not_found("zone"))?),
        None => None,
    };
    let label = match (item, zone) {
        (Some(it), _) => it.name.clone(),
        (None, Some(z)) => z.name.clone(),
        (None, None) => return Err(bad("a graphic (item) or a zone is needed")),
    };
    if input.action == Action::Take && item.is_none() {
        return Err(bad("a take needs a graphic (item)"));
    }
    let now = now_ms();
    let mut when = input.when;
    if let When::In { at, secs } = &mut when {
        if let Some(secs) = secs.take().filter(|s| s.is_finite()) {
            *at = now + (secs.clamp(0.0, 7.0 * 86_400.0) * 1000.0) as i64;
        }
    }
    let next = next_fire(&when, input.repeat, &s.tz(), s.rate_of(&id), now)
        .map_err(bad)?
        .ok_or_else(|| bad("this time has passed"))?;
    let entry = Scheduled {
        id: cg_core::new_id(),
        channel_id: id.clone(),
        project_id: pid,
        action: input.action,
        item: item.map(|i| i.id.clone()),
        zone: zone.map(|z| z.id.clone()),
        when,
        repeat: input.repeat,
        created_by: Actor::user(&me).name,
        enabled: true,
        label,
        next,
        missed: false,
    };
    s.schedule.edit(|items| items.push(entry.clone()));
    s.broadcast_schedule(&id).await;
    Ok((StatusCode::CREATED, Json(entry)))
}

/// `DELETE /api/channels/{id}/schedule/{sid}`.
pub async fn delete(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    UrlPath((id, sid)): UrlPath<(String, String)>,
) -> ApiResult<StatusCode> {
    visible(&me, &id)?;
    require(me.can_operate(&id))?;
    let gone = s.schedule.edit(|items| {
        let before = items.len();
        items.retain(|e| !(e.id == sid && e.channel_id == id));
        items.len() != before
    });
    if !gone {
        return Err(not_found("scheduled take"));
    }
    s.broadcast_schedule(&id).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct Patch {
    enabled: bool,
}

/// `PATCH /api/channels/{id}/schedule/{sid}` `{enabled}`: turns an entry
/// on or off (on again: from its next time to come).
pub async fn patch(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    UrlPath((id, sid)): UrlPath<(String, String)>,
    Json(p): Json<Patch>,
) -> ApiResult<Json<Scheduled>> {
    visible(&me, &id)?;
    require(me.can_operate(&id))?;
    let (tz, rate, now) = (s.tz(), s.rate_of(&id), now_ms());
    let entry = s.schedule.edit(|items| {
        let e = items
            .iter_mut()
            .find(|e| e.id == sid && e.channel_id == id)
            .ok_or_else(|| not_found("scheduled take"))?;
        if p.enabled && (e.missed || e.next <= now) {
            e.next = next_fire(&e.when, e.repeat, &tz, rate, now)
                .map_err(bad)?
                .ok_or_else(|| bad("this time has passed"))?;
            e.missed = false;
        }
        e.enabled = p.enabled;
        Ok::<_, (StatusCode, String)>(e.clone())
    })?;
    s.broadcast_schedule(&id).await;
    Ok(Json(entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paris() -> TimeZone {
        TimeZone::get("Europe/Paris").unwrap()
    }

    /// Server ms of a civil time in Paris.
    fn at(y: i16, mo: i8, d: i8, h: i8, mi: i8, s: i8) -> i64 {
        jiff::civil::date(y, mo, d)
            .at(h, mi, s, 0)
            .to_zoned(paris())
            .unwrap()
            .timestamp()
            .as_millisecond()
    }

    const P25: FrameRate = FrameRate::new(25, 1);

    #[test]
    fn schedule_at_today_or_tomorrow() {
        let tz = paris();
        let when = When::At {
            time: "20:00:00".into(),
        };
        // Before 20:00: today.
        let now = at(2026, 10, 20, 19, 58, 12);
        assert_eq!(
            next_fire(&when, Repeat::Once, &tz, P25, now).unwrap(),
            Some(at(2026, 10, 20, 20, 0, 0))
        );
        // After (or at) 20:00: tomorrow.
        let now = at(2026, 10, 20, 20, 0, 0);
        assert_eq!(
            next_fire(&when, Repeat::Once, &tz, P25, now).unwrap(),
            Some(at(2026, 10, 21, 20, 0, 0))
        );
        assert!(parse_time("24:00:00").is_err());
        assert!(parse_time("20:61").is_err());
        assert_eq!(parse_time("20:00").unwrap(), 72_000_000);
    }

    #[test]
    fn schedule_daily_rolls_over_dst() {
        let tz = paris();
        let when = When::At {
            time: "20:00:00".into(),
        };
        // Paris goes back to winter time on 25 October 2026: that day lasts 25 h.
        let fired = at(2026, 10, 24, 20, 0, 0);
        let next = next_fire(&when, Repeat::Daily, &tz, P25, fired)
            .unwrap()
            .unwrap();
        assert_eq!(next, at(2026, 10, 25, 20, 0, 0));
        assert_eq!(next - fired, 25 * 3_600_000);
        // Spring: 02:30 does not exist on 29 March 2026, it fires at 03:30.
        let when = When::At {
            time: "02:30:00".into(),
        };
        let next = next_fire(&when, Repeat::Daily, &tz, P25, at(2026, 3, 28, 2, 30, 0))
            .unwrap()
            .unwrap();
        assert_eq!(next, at(2026, 3, 29, 3, 30, 0));
        // "In": a single one does not come back, a daily one at the same time of day.
        let when = When::In {
            at: at(2026, 10, 20, 19, 0, 5),
            secs: None,
        };
        let later = at(2026, 10, 20, 19, 0, 5);
        assert_eq!(
            next_fire(&when, Repeat::Once, &tz, P25, later).unwrap(),
            None
        );
        assert_eq!(
            next_fire(&when, Repeat::Daily, &tz, P25, later).unwrap(),
            Some(at(2026, 10, 21, 19, 0, 5))
        );
    }

    #[test]
    fn schedule_timecode_is_the_time_of_day() {
        let tz = paris();
        // 25 fps: frame 12 is 480 ms into the second.
        assert_eq!(tc_ms_of_day("20:41:30:12", P25).unwrap(), 74_490_480);
        assert_eq!(tc_of_day(74_490_480, P25), "20:41:30:12");
        assert_eq!(tc_of_day(74_490_479, P25), "20:41:30:11");
        assert!(tc_ms_of_day("20:41:30:25", P25).is_err());
        let when = When::Timecode {
            tc: "20:41:30:00".into(),
        };
        assert_eq!(
            next_fire(&when, Repeat::Once, &tz, P25, at(2026, 10, 20, 19, 0, 0)).unwrap(),
            Some(at(2026, 10, 20, 20, 41, 30))
        );
        // 50 fps: twice the frames.
        let p50 = FrameRate::new(50, 1);
        assert_eq!(tc_ms_of_day("00:00:01:25", p50).unwrap(), 1_500);
        // 29.97 drop frame: ";" and the frames 0 and 1 of most minutes skipped.
        let df = FrameRate::new(30000, 1001);
        assert_eq!(tc_rate(df), (30, 2));
        assert_eq!(tc_rate(FrameRate::new(60000, 1001)), (60, 4));
        assert_eq!(tc_rate(FrameRate::new(24000, 1001)), (24, 0));
        // One minute in: frame 1798 (59.993 s) is 00:00:59;28 ... 1800 → 00:01:00;02.
        let ms = tc_ms_of_day("00:01:00;02", df).unwrap();
        assert_eq!(ms, (1800 * 1001 + 29) / 30);
        assert_eq!(tc_of_day(ms, df), "00:01:00;02");
        assert_eq!(tc_of_day(ms - 1, df), "00:00:59;29");
        // Ten minutes: no frame dropped.
        let ms = tc_ms_of_day("00:10:00;00", df).unwrap();
        assert_eq!(tc_of_day(ms, df), "00:10:00;00");
        // Over a day the drop-frame timecode follows the clock (within a frame or so).
        let ms = tc_ms_of_day("20:00:00;00", df).unwrap();
        assert!((ms - 72_000_000).abs() < 100, "{ms}");
        for tc in ["13:27:45;17", "23:59:59;29", "09:09:00;04"] {
            assert_eq!(tc_of_day(tc_ms_of_day(tc, df).unwrap(), df), tc);
        }
    }
}
