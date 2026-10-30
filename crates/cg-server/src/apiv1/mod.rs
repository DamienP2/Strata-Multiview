//! Control API for control surfaces (Bitfocus Buttons / Companion, Stream
//! Deck, scripts): friendly names, short paths, tally feedback.
//!
//! The same actions over HTTP (`POST /api/v1/{channel}/{action}/…`), OSC
//! over UDP (`/strata/{channel}/{action}/…`, [`osc`]) and a TCP text
//! protocol (`{channel} {action} …`, [`tcp`]). A channel is named by id or
//! name, a graphic of the rundown by id, name or 1-based position, a zone
//! or a template by id or name. The status (`GET /api/v1/status`, the
//! WebSocket `/api/v1/feedback`, TCP `watch`) gives the tally of every
//! graphic, what each zone shows, countdowns, zone locks and health.
//!
//! Everything on air goes through `AppState::command` (locks, undo, as-run
//! log), as the actor of the request: the API key, or "OSC <ip>" /
//! "TCP <ip>".

mod osc;
mod status;
mod tcp;

use std::collections::{BTreeMap, HashMap};
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use cg_core::{ChannelState, Command, Data, Phase, Project, RundownItem, WidgetControl, Zone};
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::antenna::{Actor, ActorKind};
use crate::api::{bad, not_found, ApiResult};
use crate::auth::{self, CurrentUser, User};
use crate::state::{Event, Shared};

pub use status::{channel_status, Status};

/// An error and its HTTP status (TCP and OSC only show the text).
pub type Fail = (StatusCode, String);

pub fn router() -> Router<Shared> {
    Router::new()
        .route("/api/v1/status", get(all_status))
        .route("/api/v1/feedback", get(feedback))
        .route(
            "/api/v1/{channel}/{*rest}",
            get(channel_get).post(channel_post),
        )
}

/// Starts the OSC and TCP listeners enabled in the settings; returns when
/// the engine cycle ends.
pub async fn serve(s: Shared) {
    // Stopped with this task (dropping the set aborts them): their ports
    // are free for the next cycle.
    let mut listeners = tokio::task::JoinSet::new();
    listeners.spawn(osc::serve(s.clone()));
    listeners.spawn(tcp::serve(s.clone()));
    crate::state::until_true(s.closing.subscribe()).await;
}

// ------------------------------------------------------------- memory ----

/// What NEXT takes on a channel, kept in memory.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cue {
    /// The rundown item set by `/cue` (or the one after the last NEXT).
    pub cue: Option<String>,
    /// The last item taken through this API.
    pub last: Option<String>,
}

struct Hub {
    cues: Mutex<HashMap<String, Cue>>,
    /// Something only this API knows changed (a cue): the status again.
    poke: broadcast::Sender<()>,
    /// Feedback WebSockets and TCP connections open now.
    sockets: AtomicUsize,
    /// OSC senders, last heard.
    osc_peers: Mutex<HashMap<IpAddr, Instant>>,
}

static HUB: LazyLock<Hub> = LazyLock::new(|| Hub {
    cues: Default::default(),
    poke: broadcast::channel(16).0,
    sockets: AtomicUsize::new(0),
    osc_peers: Default::default(),
});

pub fn cue_of(channel: &str) -> Cue {
    HUB.cues
        .lock()
        .map(|c| c.get(channel).cloned().unwrap_or_default())
        .unwrap_or_default()
}

fn set_cue(channel: &str, f: impl FnOnce(&mut Cue)) {
    if let Ok(mut c) = HUB.cues.lock() {
        f(c.entry(channel.into()).or_default());
    }
    let _ = HUB.poke.send(());
}

/// Control surfaces connected: feedback sockets, TCP connections, and OSC
/// senders heard in the last minute.
pub fn surfaces() -> usize {
    let osc = HUB
        .osc_peers
        .lock()
        .map(|mut p| {
            p.retain(|_, at| at.elapsed() < Duration::from_secs(60));
            p.len()
        })
        .unwrap_or(0);
    HUB.sockets.load(Ordering::Relaxed) + osc
}

/// Counts a connected surface while it lives.
struct Connected;

impl Connected {
    fn new() -> Self {
        HUB.sockets.fetch_add(1, Ordering::Relaxed);
        let _ = HUB.poke.send(());
        Connected
    }
}

impl Drop for Connected {
    fn drop(&mut self) {
        HUB.sockets.fetch_sub(1, Ordering::Relaxed);
        let _ = HUB.poke.send(());
    }
}

fn heard_osc(ip: IpAddr) {
    let new = HUB
        .osc_peers
        .lock()
        .map(|mut p| p.insert(ip, Instant::now()).is_none())
        .unwrap_or(false);
    if new {
        let _ = HUB.poke.send(());
    }
}

// ---------------------------------------------------------- resolution ----

/// A channel by id, else by id or name whatever the case.
pub fn pick_channel<'a>(
    list: impl Iterator<Item = (&'a str, &'a str)> + Clone,
    key: &str,
) -> Option<&'a str> {
    let key = key.trim();
    list.clone()
        .find(|(id, _)| *id == key)
        .or_else(|| {
            list.clone().find(|(id, name)| {
                id.eq_ignore_ascii_case(key) || name.trim().eq_ignore_ascii_case(key)
            })
        })
        .map(|(id, _)| id)
}

fn channel_id(s: &Shared, key: &str, sees: impl Fn(&str) -> bool) -> Result<String, Fail> {
    let list: Vec<(&str, &str)> = s
        .channels
        .values()
        .filter(|c| sees(&c.config.id))
        .map(|c| (c.config.id.as_str(), c.config.name.as_str()))
        .collect();
    pick_channel(list.iter().copied(), key)
        .map(str::to_string)
        .ok_or_else(|| not_found("channel"))
}

/// A graphic of the rundown by id, name, or position (1 = first).
pub fn find_item<'a>(p: &'a Project, key: &str) -> Option<&'a RundownItem> {
    let key = key.trim();
    p.rundown
        .iter()
        .find(|r| r.id == key)
        .or_else(|| {
            p.rundown
                .iter()
                .find(|r| r.name.trim().eq_ignore_ascii_case(key))
        })
        .or_else(|| {
            key.parse::<usize>()
                .ok()
                .filter(|n| *n >= 1)
                .and_then(|n| p.rundown.get(n - 1))
        })
}

/// A zone by id or name (or its layer number).
pub fn find_zone<'a>(p: &'a Project, key: &str) -> Option<&'a Zone> {
    let key = key.trim();
    p.zones
        .iter()
        .find(|z| z.id == key)
        .or_else(|| {
            p.zones
                .iter()
                .find(|z| z.id.eq_ignore_ascii_case(key) || z.name.trim().eq_ignore_ascii_case(key))
        })
        .or_else(|| {
            key.parse::<u32>()
                .ok()
                .and_then(|l| p.zones.iter().find(|z| z.layer == l))
        })
}

pub fn find_template<'a>(p: &'a Project, key: &str) -> Option<&'a cg_core::Template> {
    let key = key.trim();
    p.templates.iter().find(|t| t.id == key).or_else(|| {
        p.templates
            .iter()
            .find(|t| t.name.trim().eq_ignore_ascii_case(key))
    })
}

fn find_playlist<'a>(p: &'a Project, key: &str) -> Option<&'a cg_core::Playlist> {
    let key = key.trim();
    p.playlists
        .iter()
        .find(|l| l.id == key)
        .or_else(|| {
            p.playlists
                .iter()
                .find(|l| l.name.trim().eq_ignore_ascii_case(key))
        })
        .or_else(|| {
            key.parse::<usize>()
                .ok()
                .filter(|n| *n >= 1)
                .and_then(|n| p.playlists.get(n - 1))
        })
}

fn zone_of(p: &Project, key: &str) -> Result<String, Fail> {
    find_zone(p, key)
        .map(|z| z.id.clone())
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("no zone `{key}`")))
}

// ------------------------------------------------------------- NEXT ----

fn playable(p: &Project, r: &RundownItem) -> bool {
    p.template(&r.template_id).is_some_and(|t| t.playable())
}

/// The graphics of a playlist NEXT goes through, as the Operator lists
/// them: the playlist's (and those of no known playlist), playable.
fn playlist_items<'a>(p: &'a Project, playlist: &str) -> Vec<&'a RundownItem> {
    p.rundown
        .iter()
        .filter(|r| r.playlist == playlist || !p.playlists.iter().any(|l| l.id == r.playlist))
        .filter(|r| playable(p, r))
        .collect()
}

/// The graphic after `id` in its playlist (the first again when it loops).
fn after<'a>(p: &'a Project, id: &str) -> Option<&'a RundownItem> {
    let r = p.rundown.iter().find(|r| r.id == id)?;
    let items = playlist_items(p, &r.playlist);
    let looping = p
        .playlists
        .iter()
        .find(|l| l.id == r.playlist)
        .is_some_and(|l| l.looping);
    let i = items.iter().position(|x| x.id == id)?;
    let n = items
        .get(i + 1)
        .or(if looping { items.first() } else { None })?;
    (n.id != id).then_some(*n)
}

/// On air (or queued to come in) on the channel.
fn is_on_air(st: &ChannelState, id: &str) -> bool {
    st.layers.values().any(|it| {
        it.rundown_id.as_deref() == Some(id)
            || it
                .next
                .as_ref()
                .is_some_and(|q| q.rundown_id.as_deref() == Some(id))
    })
}

/// The rundown item taken last among those on air (queued ones first).
fn latest_on_air<'a>(p: &'a Project, st: &ChannelState) -> Option<&'a RundownItem> {
    let queued = st
        .layers
        .values()
        .filter_map(|it| it.next.as_ref().and_then(|q| q.rundown_id.clone()));
    let mut on_air: Vec<(f64, String)> = st
        .layers
        .values()
        .filter(|it| !matches!(it.phase, Phase::Out | Phase::OutHold))
        .filter_map(|it| it.rundown_id.clone().map(|id| (it.age, id)))
        .collect();
    on_air.sort_by(|a, b| a.0.total_cmp(&b.0));
    queued
        .chain(on_air.into_iter().map(|(_, id)| id))
        .find_map(|id| p.rundown.iter().find(|r| r.id == id))
}

/// What NEXT takes, as the Operator's NEXT: the cued graphic (the one
/// after it when it is already on air); without a cue, the one after the
/// last taken; else the first of the playlist.
pub fn next_target<'a>(
    p: &'a Project,
    st: &ChannelState,
    mem: &Cue,
    playlist: Option<&str>,
) -> Option<&'a RundownItem> {
    let known = |id: &Option<String>| {
        id.as_deref()
            .and_then(|id| p.rundown.iter().find(|r| r.id == id))
            .filter(|r| playable(p, r))
            .filter(|r| playlist.is_none_or(|l| l == r.playlist))
    };
    if let Some(c) = known(&mem.cue) {
        return if is_on_air(st, &c.id) {
            after(p, &c.id)
        } else {
            Some(c)
        };
    }
    let last = latest_on_air(p, st)
        .filter(|r| playlist.is_none_or(|l| l == r.playlist))
        .or_else(|| known(&mem.last));
    if let Some(l) = last {
        return after(p, &l.id);
    }
    let first = playlist
        .map(str::to_string)
        .or_else(|| p.playlists.first().map(|l| l.id.clone()))
        .unwrap_or_default();
    playlist_items(p, &first).first().copied()
}

// ------------------------------------------------------------ actions ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowStep {
    Next,
    Prev,
    First,
    Last,
    At(u32),
}

/// One thing a surface asks, whatever the protocol.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Take {
        item: String,
        data: Data,
    },
    Next,
    Cue {
        item: String,
    },
    Continue {
        zone: String,
    },
    /// Takes out a zone, or every zone on air (`None`).
    Out {
        zone: Option<String>,
    },
    /// Removes at once (no out animation).
    Clear {
        zone: Option<String>,
    },
    Update {
        zone: String,
        data: Data,
    },
    Value {
        zone: String,
        key: String,
        value: String,
    },
    Undo,
    Trigger {
        name: String,
    },
    Widget {
        zone: String,
        widget: String,
        op: String,
        args: serde_json::Value,
    },
    Row {
        zone: String,
        step: RowStep,
        dataset: Option<String>,
    },
}

/// What comes with the path: a body, a query, OSC arguments.
#[derive(Debug, Clone, Default)]
pub struct Extra {
    /// Values (`take`, `update`).
    pub data: Data,
    /// `value`: the value.
    pub value: Option<String>,
    /// `update`, `value`: the new values cross-fade in.
    pub fade: bool,
    /// `take`: in another zone than the graphic's.
    pub zone: Option<String>,
    /// `next`: the playlist to go through.
    pub playlist: Option<String>,
    /// `row`: the data set (default: the first of the graphic).
    pub dataset: Option<String>,
    /// `widget`: the arguments of the widget's action.
    pub args: Option<serde_json::Value>,
    /// Arguments after the path (OSC arguments).
    pub positional: Vec<String>,
}

/// Options of an action that are not part of it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Opts {
    pub fade: bool,
    pub zone: Option<String>,
    pub playlist: Option<String>,
}

pub const ACTIONS: &str =
    "take, next, cue, continue, out, clear, update, value, undo, trigger, widget, row";

/// `key=value` words, or words by pairs (`key value`), into values.
fn pairs(words: impl IntoIterator<Item = String>) -> Data {
    let mut out = Data::new();
    let mut it = words.into_iter();
    while let Some(w) = it.next() {
        if let Some((k, v)) = w.split_once('=') {
            out.insert(k.trim().to_string(), v.to_string());
        } else if let Some(v) = it.next() {
            out.insert(w, v);
        }
    }
    out
}

/// An action from the words of its path (`take`, `2`) and what comes with it.
pub fn parse_action(segs: &[String], extra: Extra) -> Result<(Action, Opts), String> {
    let Extra {
        mut data,
        value,
        fade,
        zone,
        playlist,
        dataset,
        args,
        positional,
    } = extra;
    let verb = segs
        .first()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("say what to do: {ACTIONS}"))?;
    let mut rest = segs[1..]
        .iter()
        .cloned()
        .chain(positional)
        .collect::<std::collections::VecDeque<String>>();
    let mut need = |what: &str| {
        rest.pop_front()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| format!("{verb}: say which {what}"))
    };
    let all = |z: Option<String>| z.filter(|z| !matches!(z.trim(), "" | "*" | "all"));
    let action =
        match verb.as_str() {
            "take" | "play" => {
                let item = need("graphic")?;
                data.extend(pairs(rest));
                Action::Take { item, data }
            }
            "next" => Action::Next,
            "cue" => Action::Cue {
                item: need("graphic")?,
            },
            "continue" | "cont" => Action::Continue {
                zone: need("zone")?,
            },
            "out" | "takeout" => Action::Out {
                zone: all(rest.pop_front()),
            },
            "clear" => Action::Clear {
                zone: all(rest.pop_front()),
            },
            "update" => {
                let zone = need("zone")?;
                data.extend(pairs(rest));
                if data.is_empty() {
                    return Err("update: no values".into());
                }
                Action::Update { zone, data }
            }
            "value" | "set" => {
                let zone = need("zone")?;
                let key = need("key")?;
                let value = rest
                    .pop_front()
                    .or(value)
                    .or_else(|| data.remove(&key))
                    .or_else(|| data.remove("value"))
                    .ok_or("value: no value")?;
                Action::Value { zone, key, value }
            }
            "undo" => Action::Undo,
            "trigger" => Action::Trigger {
                name: need("trigger")?,
            },
            "widget" => {
                let zone = need("zone")?;
                let widget = need("widget")?;
                let op = rest
                    .pop_front()
                    .unwrap_or_else(|| "toggle".into())
                    .to_ascii_lowercase();
                let args = args.unwrap_or_else(|| {
                    let a = pairs(rest);
                    if a.is_empty() {
                        serde_json::Value::Null
                    } else {
                        serde_json::Value::Object(
                            a.into_iter()
                                .map(|(k, v)| {
                                    let v = serde_json::from_str::<serde_json::Value>(&v)
                                        .ok()
                                        .filter(|j| j.is_number() || j.is_boolean())
                                        .unwrap_or(serde_json::Value::String(v));
                                    (k, v)
                                })
                                .collect(),
                        )
                    }
                });
                Action::Widget {
                    zone,
                    widget,
                    op,
                    args,
                }
            }
            "row" => {
                let zone = need("zone")?;
                let step =
                    match rest
                        .pop_front()
                        .as_deref()
                        .map(|s| s.trim().to_ascii_lowercase())
                    {
                        None => RowStep::Next,
                        Some(s) => match s.as_str() {
                            "" | "next" | "+" => RowStep::Next,
                            "prev" | "previous" | "-" => RowStep::Prev,
                            "first" => RowStep::First,
                            "last" => RowStep::Last,
                            n => RowStep::At(n.parse::<u32>().ok().filter(|n| *n >= 1).ok_or(
                                "row: next, prev, first, last or a row number (1 = first)",
                            )?),
                        },
                    };
                Action::Row {
                    zone,
                    step,
                    dataset: rest.pop_front().or(dataset),
                }
            }
            other => return Err(format!("unknown action `{other}`: {ACTIONS}")),
        };
    Ok((
        action,
        Opts {
            fade,
            zone,
            playlist,
        },
    ))
}

/// A command refused: on a zone another operator holds (409), or wrong.
fn refused(e: String) -> Fail {
    if e.ends_with("holds this zone") {
        (StatusCode::CONFLICT, e)
    } else {
        bad(e)
    }
}

async fn loaded(s: &Shared, channel: &str) -> Result<std::sync::Arc<Project>, Fail> {
    let ch = s.channel(channel).ok_or_else(|| not_found("channel"))?;
    let pid = ch
        .project_id
        .read()
        .await
        .clone()
        .ok_or_else(|| bad("no project loaded on this channel"))?;
    s.project(&pid).await.ok_or_else(|| not_found("project"))
}

async fn send(s: &Shared, channel: &str, cmd: Command, actor: &Actor) -> Result<(), Fail> {
    s.command(channel, cmd, actor)
        .await
        .map(|_| ())
        .map_err(refused)
}

/// Takes a graphic of the rundown (its values, with `data` over them).
async fn take_item(
    s: &Shared,
    channel: &str,
    p: &Project,
    it: &RundownItem,
    data: Data,
    zone: Option<String>,
    actor: &Actor,
) -> Result<(), Fail> {
    let mut merged = it.data.clone();
    merged.extend(data);
    let zone = zone.or_else(|| p.item_zone(it).map(|z| z.id.clone()));
    send(
        s,
        channel,
        Command::Take {
            zone,
            layer: None,
            template_id: it.template_id.clone(),
            data: Some(merged),
            rundown_id: Some(it.id.clone()),
        },
        actor,
    )
    .await
}

/// Does what a surface asked on a channel (its id); returns what was done.
pub async fn run(
    s: &Shared,
    channel: &str,
    action: Action,
    opts: &Opts,
    actor: &Actor,
) -> Result<String, Fail> {
    let ch = s.channel(channel).ok_or_else(|| not_found("channel"))?;
    if action == Action::Undo {
        s.undo(channel, actor).await.map_err(bad)?;
        return Ok("undo".into());
    }
    let p = loaded(s, channel).await?;
    let zone_opt = |z: &Option<String>| z.as_deref().map(|z| zone_of(&p, z)).transpose();
    match action {
        Action::Take { item, data } => {
            let zone = zone_opt(&opts.zone)?;
            if let Some(it) = find_item(&p, &item) {
                take_item(s, channel, &p, it, data, zone, actor).await?;
                let id = it.id.clone();
                set_cue(channel, |c| {
                    c.last = Some(id);
                    c.cue = None;
                });
                return Ok(format!("take {}", it.name));
            }
            let t = find_template(&p, &item).ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    format!("no graphic `{item}` in the rundown or the templates"),
                )
            })?;
            send(
                s,
                channel,
                Command::Take {
                    zone,
                    layer: None,
                    template_id: t.id.clone(),
                    data: (!data.is_empty()).then_some(data),
                    rundown_id: None,
                },
                actor,
            )
            .await?;
            Ok(format!("take {}", t.name))
        }
        Action::Next => {
            let playlist = match &opts.playlist {
                Some(l) => Some(
                    find_playlist(&p, l)
                        .map(|l| l.id.clone())
                        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("no playlist `{l}`")))?,
                ),
                None => None,
            };
            let st = ch.state.read().await.clone();
            let it = next_target(&p, &st, &cue_of(channel), playlist.as_deref())
                .ok_or((StatusCode::CONFLICT, "nothing next in the playlist".into()))?;
            take_item(s, channel, &p, it, Data::new(), None, actor).await?;
            let (id, then) = (it.id.clone(), after(&p, &it.id).map(|n| n.id.clone()));
            set_cue(channel, |c| {
                c.last = Some(id);
                c.cue = then;
            });
            Ok(format!("take {}", it.name))
        }
        Action::Cue { item } => {
            let it = find_item(&p, &item)
                .ok_or_else(|| (StatusCode::NOT_FOUND, format!("no graphic `{item}`")))?;
            let id = it.id.clone();
            set_cue(channel, |c| c.cue = Some(id));
            Ok(format!("cue {}", it.name))
        }
        Action::Continue { zone } => {
            let zone = zone_of(&p, &zone)?;
            let cmd = Command::Continue {
                zone: Some(zone.clone()),
                layer: None,
            };
            send(s, channel, cmd, actor).await?;
            Ok(format!("continue {zone}"))
        }
        Action::Out { zone: Some(zone) } => {
            let zone = zone_of(&p, &zone)?;
            let cmd = Command::TakeOut {
                zone: Some(zone.clone()),
                layer: None,
            };
            send(s, channel, cmd, actor).await?;
            Ok(format!("out {zone}"))
        }
        Action::Out { zone: None } => {
            let st = ch.state.read().await.clone();
            let busy: Vec<String> = p
                .zones
                .iter()
                .filter(|z| {
                    // Going out with a take queued behind: out cancels it.
                    st.layers.get(&z.layer).is_some_and(|it| {
                        it.next.is_some() || !matches!(it.phase, Phase::Out | Phase::OutHold)
                    })
                })
                .map(|z| z.id.clone())
                .collect();
            for z in &busy {
                let cmd = Command::TakeOut {
                    zone: Some(z.clone()),
                    layer: None,
                };
                send(s, channel, cmd, actor).await?;
            }
            Ok(if busy.is_empty() {
                "nothing on air".into()
            } else {
                format!("out {}", busy.join(", "))
            })
        }
        Action::Clear { zone } => {
            let zone = zone_opt(&zone)?;
            let label = zone.clone().unwrap_or_else(|| "all".into());
            send(s, channel, Command::Clear { zone, layer: None }, actor).await?;
            Ok(format!("clear {label}"))
        }
        Action::Update { zone, data } => {
            let zone = zone_of(&p, &zone)?;
            let cmd = Command::Update {
                zone: Some(zone.clone()),
                layer: None,
                data,
                fade: opts.fade,
            };
            send(s, channel, cmd, actor).await?;
            Ok(format!("update {zone}"))
        }
        Action::Value { zone, key, value } => {
            let zone = zone_of(&p, &zone)?;
            let cmd = Command::Update {
                zone: Some(zone.clone()),
                layer: None,
                data: Data::from([(key.clone(), value)]),
                fade: opts.fade,
            };
            send(s, channel, cmd, actor).await?;
            Ok(format!("value {zone} {key}"))
        }
        Action::Undo => unreachable!("done above"),
        Action::Trigger { name } => {
            let cmd = Command::Trigger { name: name.clone() };
            send(s, channel, cmd, actor).await?;
            Ok(format!("trigger {name}"))
        }
        Action::Widget {
            zone,
            widget,
            op,
            args,
        } => {
            let zone = zone_of(&p, &zone)?;
            let control = match op.as_str() {
                "start" | "play" | "resume" => Some(WidgetControl::Start),
                "pause" | "stop" => Some(WidgetControl::Pause),
                "toggle" => Some(WidgetControl::Toggle),
                "reset" => Some(WidgetControl::Reset),
                "restart" => Some(WidgetControl::Restart),
                _ => None,
            };
            let cmd = match control {
                Some(control) => Command::Widget {
                    zone: Some(zone.clone()),
                    layer: None,
                    target: widget.clone(),
                    control,
                },
                None => Command::Act {
                    zone: Some(zone.clone()),
                    layer: None,
                    target: widget.clone(),
                    op: op.clone(),
                    args,
                },
            };
            send(s, channel, cmd, actor).await?;
            Ok(format!("widget {zone} {widget} {op}"))
        }
        Action::Row {
            zone,
            step,
            dataset,
        } => {
            let z = find_zone(&p, &zone)
                .ok_or_else(|| (StatusCode::NOT_FOUND, format!("no zone `{zone}`")))?;
            let st = ch.state.read().await.clone();
            let (key, row) = row_of(&p, &st, z, step, dataset.as_deref())?;
            let cmd = Command::Row {
                zone: Some(z.id.clone()),
                layer: None,
                dataset: key.clone(),
                row,
            };
            send(s, channel, cmd, actor).await?;
            Ok(format!("row {} {key} {row}", z.id))
        }
    }
}

/// The data set (its key) and the row a step goes to, from the row the
/// graphic on air in the zone shows.
fn row_of(
    p: &Project,
    st: &ChannelState,
    z: &Zone,
    step: RowStep,
    dataset: Option<&str>,
) -> Result<(String, u32), Fail> {
    let it = st
        .layers
        .get(&z.layer)
        .ok_or_else(|| bad(format!("nothing on air in {}", z.id)))?;
    let data = it.next.as_ref().map(|q| &q.data).unwrap_or(&it.data);
    let t = p
        .template(&it.template_id)
        .ok_or_else(|| not_found("template"))?;
    let ds = match dataset {
        Some(d) => t
            .datasets
            .iter()
            .find(|x| x.key == d || x.name.trim().eq_ignore_ascii_case(d.trim())),
        None => t.datasets.first(),
    }
    .ok_or_else(|| bad("no data set in the graphic on air"))?;
    let num = |k: String| data.get(&k).and_then(|v| v.trim().parse::<f64>().ok());
    let row = num(ds.row_key()).unwrap_or(1.0).max(1.0) as u32;
    let count = num(ds.count_key()).map(|n| n as u32);
    let looping = data.get(&ds.loop_key()).is_some_and(|l| l == "1");
    let to = match step {
        RowStep::Next => match count {
            Some(n) if row >= n && looping => 1,
            Some(n) if row >= n => return Err(bad("already on the last row")),
            _ => row + 1,
        },
        RowStep::Prev => match count {
            Some(n) if row <= 1 && looping => n.max(1),
            _ => row.saturating_sub(1).max(1),
        },
        RowStep::First => 1,
        RowStep::Last => count.unwrap_or(row).max(1),
        RowStep::At(n) => n,
    };
    Ok((ds.key.clone(), to))
}

// --------------------------------------------------------------- HTTP ----

const RESERVED: [&str; 7] = [
    "value", "fade", "zone", "playlist", "dataset", "args", "data",
];

fn text_of(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn truthy(v: &str) -> bool {
    matches!(v.trim(), "1" | "true" | "yes" | "on")
}

/// What comes with an HTTP action: the query, then a JSON body (values
/// under `data` or at the top) or a text body (the value).
pub fn extra_of(query: BTreeMap<String, String>, body: &[u8]) -> Result<Extra, String> {
    let mut x = Extra::default();
    let put = |x: &mut Extra, k: &str, v: String| match k {
        "value" => x.value = Some(v),
        "fade" => x.fade = truthy(&v),
        "zone" => x.zone = Some(v),
        "playlist" => x.playlist = Some(v),
        "dataset" => x.dataset = Some(v),
        "key" => {}
        _ => {
            x.data.insert(k.to_string(), v);
        }
    };
    for (k, v) in query {
        put(&mut x, &k, v);
    }
    let text = std::str::from_utf8(body).map_err(|_| "the body is not text")?;
    if text.trim().is_empty() {
        return Ok(x);
    }
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(serde_json::Value::Object(o)) => {
            for (k, v) in o {
                match (k.as_str(), &v) {
                    ("data", serde_json::Value::Object(d)) => {
                        for (dk, dv) in d {
                            if let Some(t) = text_of(dv) {
                                x.data.insert(dk.clone(), t);
                            }
                        }
                    }
                    ("args", _) => x.args = Some(v),
                    ("fade", serde_json::Value::Bool(b)) => x.fade = *b,
                    (k, v) if RESERVED.contains(&k) || !v.is_object() => {
                        if let Some(t) = text_of(v) {
                            put(&mut x, k, t);
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(v) => x.value = text_of(&v),
        // Plain text: the value.
        Err(_) => x.value = Some(text.trim_end_matches(['\r', '\n']).to_string()),
    }
    Ok(x)
}

fn segments(rest: &str) -> Vec<String> {
    rest.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// The loaded project of the channel must be one the user sees.
async fn check_operate(s: &Shared, me: &User, channel: &str) -> Result<(), Fail> {
    crate::api::require(me.can_operate(channel))?;
    if let Some(ch) = s.channel(channel) {
        if let Some(pid) = ch.project_id.read().await.as_deref() {
            if !me.sees_project(pid) {
                return Err(not_found("project"));
            }
        }
    }
    Ok(())
}

async fn channel_post(
    State(s): State<Shared>,
    auth::Acting(me, actor): auth::Acting,
    Path((channel, rest)): Path<(String, String)>,
    Query(q): Query<BTreeMap<String, String>>,
    body: Bytes,
) -> ApiResult<Json<serde_json::Value>> {
    let channel = channel_id(&s, &channel, |id| me.sees_channel(id))?;
    check_operate(&s, &me, &channel).await?;
    let extra = extra_of(q, &body).map_err(bad)?;
    let (action, opts) = parse_action(&segments(&rest), extra).map_err(|e| {
        if e.starts_with("unknown action") {
            (StatusCode::NOT_FOUND, e)
        } else {
            bad(e)
        }
    })?;
    let done = run(&s, &channel, action, &opts, &actor).await?;
    Ok(Json(
        serde_json::json!({ "ok": true, "channel": channel, "done": done }),
    ))
}

async fn channel_get(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path((channel, rest)): Path<(String, String)>,
) -> ApiResult<Json<status::ChannelStatus>> {
    let channel = channel_id(&s, &channel, |id| me.sees_channel(id))?;
    if segments(&rest) != ["status"] {
        return Err((
            StatusCode::METHOD_NOT_ALLOWED,
            "actions are POST; GET /api/v1/{channel}/status".into(),
        ));
    }
    let ch = s.channel(&channel).ok_or_else(|| not_found("channel"))?;
    Ok(Json(channel_status(&s, ch, Some(&me)).await))
}

async fn all_status(State(s): State<Shared>, CurrentUser(me): CurrentUser) -> Json<Status> {
    Json(status::status(&s, Some(&me), None).await)
}

#[derive(Deserialize)]
struct FeedbackQuery {
    /// One channel (id or name); all by default.
    channel: Option<String>,
}

/// The status on connect, then again each time it changes.
async fn feedback(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Query(q): Query<FeedbackQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let only = match q.channel {
        Some(c) => match channel_id(&s, &c, |id| me.sees_channel(id)) {
            Ok(id) => Some(id),
            Err(e) => return e.into_response(),
        },
        None => None,
    };
    ws.on_upgrade(move |socket| feedback_session(s, socket, me, only))
}

async fn feedback_session(s: Shared, mut socket: WebSocket, me: User, only: Option<String>) {
    let _connected = Connected::new();
    let mut w = Watcher::new(&s, Some(me), only);
    loop {
        tokio::select! {
            next = w.next() => {
                let Some(json) = next else { break };
                if socket.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_))) => {
                    // Sends the close reply.
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
                None | Some(Err(_)) => break,
                _ => {}
            },
        }
    }
}

/// Follows the status: the full status first, then each time it changes
/// (at most 10 a second; countdowns at least once a second).
pub struct Watcher {
    s: Shared,
    user: Option<User>,
    only: Option<String>,
    events: broadcast::Receiver<std::sync::Arc<crate::state::Sent>>,
    poke: broadcast::Receiver<()>,
    closing: tokio::sync::watch::Receiver<bool>,
    last: String,
    sent_at: Option<Instant>,
}

impl Watcher {
    pub fn new(s: &Shared, user: Option<User>, only: Option<String>) -> Self {
        Self {
            events: s.events.subscribe(),
            poke: HUB.poke.subscribe(),
            closing: s.closing.subscribe(),
            s: s.clone(),
            user,
            only,
            last: String::new(),
            sent_at: None,
        }
    }

    fn concerns(only: Option<&str>, e: &Event) -> bool {
        match e {
            Event::Channel { channel_id, .. } | Event::ZoneLocks { channel_id, .. } => {
                only.is_none_or(|c| c == channel_id)
            }
            Event::Channels { .. }
            | Event::Project { .. }
            | Event::ProjectTemplate { .. }
            | Event::ProjectDeleted { .. } => true,
            _ => false,
        }
    }

    /// The next status that differs from the last one (`None`: the server
    /// is closing).
    pub async fn next(&mut self) -> Option<String> {
        loop {
            if self.sent_at.is_some() {
                let woke = tokio::select! {
                    e = self.events.recv() => match e {
                        Ok(e) => Self::concerns(self.only.as_deref(), &e.event),
                        Err(broadcast::error::RecvError::Lagged(_)) => true,
                        Err(broadcast::error::RecvError::Closed) => return None,
                    },
                    _ = self.poke.recv() => true,
                    // Countdowns and health move without events.
                    _ = tokio::time::sleep(Duration::from_millis(250)) => true,
                    _ = self.closing.wait_for(|c| *c) => return None,
                };
                if !woke {
                    continue;
                }
                let since = self.sent_at.map(|t| t.elapsed()).unwrap_or_default();
                if since < Duration::from_millis(100) {
                    tokio::time::sleep(Duration::from_millis(100) - since).await;
                }
                // Everything up to now is in the status about to be made.
                while self.events.try_recv().is_ok() {}
                while self.poke.try_recv().is_ok() {}
            }
            let mut st = status::status(&self.s, self.user.as_ref(), self.only.as_deref()).await;
            st.server_time = 0;
            let same = serde_json::to_string(&st).unwrap_or_default();
            if same == self.last {
                if self.sent_at.is_none() {
                    self.sent_at = Some(Instant::now());
                }
                continue;
            }
            self.last = same;
            self.sent_at = Some(Instant::now());
            st.server_time = crate::state::now_ms();
            return serde_json::to_string(&st).ok();
        }
    }
}

// ---------------------------------------------------------- OSC / TCP ----

/// Whether a machine may drive the playout over OSC or TCP: the machine
/// itself always, else an address or network (`10.0.0.0/24`) of the list
/// (an empty list lets anyone in).
pub fn allowed(ip: IpAddr, list: &[String]) -> bool {
    let ip = ip.to_canonical();
    if list.is_empty() || ip.is_loopback() {
        return true;
    }
    list.iter().any(|entry| {
        let entry = entry.trim();
        if entry == "*" {
            return true;
        }
        let (addr, bits) = match entry.split_once('/') {
            Some((a, b)) => (a, b.trim().parse::<u32>().ok()),
            None => (entry, None),
        };
        let Ok(net) = addr.trim().parse::<IpAddr>() else {
            return false;
        };
        match (ip, net.to_canonical()) {
            (IpAddr::V4(a), IpAddr::V4(n)) => {
                let bits = bits.unwrap_or(32).min(32);
                let mask = if bits == 0 {
                    0
                } else {
                    u32::MAX << (32 - bits)
                };
                u32::from(a) & mask == u32::from(n) & mask
            }
            (IpAddr::V6(a), IpAddr::V6(n)) => {
                let bits = bits.unwrap_or(128).min(128);
                let mask = if bits == 0 {
                    0
                } else {
                    u128::MAX << (128 - bits)
                };
                u128::from(a) & mask == u128::from(n) & mask
            }
            _ => false,
        }
    })
}

/// A word of an OSC address or a TCP path, `%20` and the like decoded
/// (an OSC address has no spaces).
pub fn unescape(word: &str) -> String {
    let b = word.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        match (
            b[i],
            b.get(i + 1).copied().and_then(hex),
            b.get(i + 2).copied().and_then(hex),
        ) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (c, _, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| word.to_string())
}

/// The actor of a surface without an API key.
fn surface_actor(proto: &str, ip: IpAddr) -> Actor {
    let ip = ip.to_canonical();
    Actor {
        kind: ActorKind::Api,
        id: format!("{}:{ip}", proto.to_ascii_lowercase()),
        name: format!("{proto} {ip}"),
    }
}

/// Runs one command of a surface without login (OSC, TCP): the channel
/// then the action's words.
pub async fn run_words(
    s: &Shared,
    words: &[String],
    extra: Extra,
    actor: &Actor,
) -> Result<String, String> {
    let (channel, rest) = words
        .split_first()
        .ok_or_else(|| "say the channel, then what to do".to_string())?;
    let channel = channel_id(s, channel, |_| true).map_err(|e| e.1)?;
    let (action, opts) = parse_action(rest, extra)?;
    run(s, &channel, action, &opts, actor)
        .await
        .map_err(|e| e.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cg_core::playout::QueuedTake;
    use cg_core::PlayoutItem;

    fn words(s: &[&str]) -> Vec<String> {
        s.iter().map(|w| w.to_string()).collect()
    }

    fn demo() -> Project {
        let mut p = cg_core::sample::demo_project();
        // Known ids for the test.
        for (i, r) in p.rundown.iter_mut().enumerate() {
            r.id = format!("r{}", i + 1);
        }
        p
    }

    fn on_air(st: &mut ChannelState, p: &Project, id: &str, age: f64) {
        let r = p.rundown.iter().find(|r| r.id == id).unwrap();
        let layer = p.item_zone(r).unwrap().layer;
        st.layers.insert(
            layer,
            PlayoutItem {
                template_id: r.template_id.clone(),
                data: r.data.clone(),
                frame: 0.0,
                target: 0.0,
                phase: Phase::Holding,
                rundown_id: Some(r.id.clone()),
                age,
                next: None,
                clocks: Default::default(),
                held: 0.0,
                pause_ahead: false,
                fade: None,
                changes: Vec::new(),
                widgets: Default::default(),
                advanced: false,
                watch: Default::default(),
                leaving: None,
            },
        );
    }

    #[test]
    fn apiv1_names_resolve_by_id_name_or_position() {
        let p = demo();
        let first = &p.rundown[0];
        assert_eq!(find_item(&p, "r1").unwrap().id, "r1");
        assert_eq!(find_item(&p, &first.name.to_uppercase()).unwrap().id, "r1");
        assert_eq!(find_item(&p, "2").unwrap().id, "r2");
        assert!(find_item(&p, "0").is_none());
        assert!(find_item(&p, "999").is_none());
        assert_eq!(find_zone(&p, "lower-third").unwrap().id, "lower-third");
        assert_eq!(find_zone(&p, "LOWER THIRD").unwrap().id, "lower-third");
        assert_eq!(find_zone(&p, "10").unwrap().id, "lower-third");
        assert!(find_zone(&p, "nowhere").is_none());
        assert_eq!(find_template(&p, "score-bug").unwrap().id, "score-bug");
        let name = p.templates[0].name.to_lowercase();
        assert_eq!(find_template(&p, &name).unwrap().id, p.templates[0].id);
        let channels = [("main", "Channel 1"), ("b", "Studio B")];
        let list = channels.iter().map(|(a, b)| (*a, *b));
        assert_eq!(pick_channel(list.clone(), "main"), Some("main"));
        assert_eq!(pick_channel(list.clone(), "MAIN"), Some("main"));
        assert_eq!(pick_channel(list.clone(), "studio b"), Some("b"));
        assert_eq!(pick_channel(list, "c"), None);
    }

    #[test]
    fn apiv1_paths_become_actions() {
        let (a, _) = parse_action(&words(&["take", "2"]), Extra::default()).unwrap();
        assert_eq!(
            a,
            Action::Take {
                item: "2".into(),
                data: Data::new()
            }
        );
        // TCP words `key=value`, OSC arguments by pairs.
        let (a, _) = parse_action(
            &words(&["take", "Marie", "name=Marie Curie"]),
            Extra::default(),
        )
        .unwrap();
        assert_eq!(
            a,
            Action::Take {
                item: "Marie".into(),
                data: Data::from([("name".into(), "Marie Curie".into())])
            }
        );
        let osc = Extra {
            positional: words(&["name", "Ada", "title", "Maths"]),
            ..Default::default()
        };
        let (a, _) = parse_action(&words(&["update", "lower"]), osc).unwrap();
        assert_eq!(
            a,
            Action::Update {
                zone: "lower".into(),
                data: Data::from([
                    ("name".into(), "Ada".into()),
                    ("title".into(), "Maths".into())
                ])
            }
        );
        let (a, _) = parse_action(
            &words(&["value", "lower", "name", "Marie Curie"]),
            Extra::default(),
        )
        .unwrap();
        assert_eq!(
            a,
            Action::Value {
                zone: "lower".into(),
                key: "name".into(),
                value: "Marie Curie".into()
            }
        );
        let body = Extra {
            value: Some("X".into()),
            ..Default::default()
        };
        let (a, _) = parse_action(&words(&["value", "lower", "name"]), body).unwrap();
        assert!(matches!(a, Action::Value { value, .. } if value == "X"));
        assert!(parse_action(&words(&["value", "lower", "name"]), Extra::default()).is_err());
        assert_eq!(
            parse_action(&words(&["out"]), Extra::default()).unwrap().0,
            Action::Out { zone: None }
        );
        assert_eq!(
            parse_action(&words(&["out", "all"]), Extra::default())
                .unwrap()
                .0,
            Action::Out { zone: None }
        );
        assert_eq!(
            parse_action(&words(&["row", "lower", "prev"]), Extra::default())
                .unwrap()
                .0,
            Action::Row {
                zone: "lower".into(),
                step: RowStep::Prev,
                dataset: None
            }
        );
        assert!(matches!(
            parse_action(&words(&["row", "lower", "3"]), Extra::default())
                .unwrap()
                .0,
            Action::Row {
                step: RowStep::At(3),
                ..
            }
        ));
        assert!(parse_action(&words(&["row", "lower", "zero"]), Extra::default()).is_err());
        let (a, _) = parse_action(
            &words(&["widget", "score", "home", "add", "points=3"]),
            Extra::default(),
        )
        .unwrap();
        assert_eq!(
            a,
            Action::Widget {
                zone: "score".into(),
                widget: "home".into(),
                op: "add".into(),
                args: serde_json::json!({ "points": 3 })
            }
        );
        // A surface's press (an int argument) is not a value.
        assert_eq!(
            parse_action(
                &words(&["next"]),
                Extra {
                    positional: words(&["1"]),
                    ..Default::default()
                }
            )
            .unwrap()
            .0,
            Action::Next
        );
        assert!(parse_action(&words(&["dance"]), Extra::default())
            .unwrap_err()
            .starts_with("unknown action"));
        assert!(parse_action(&words(&["take"]), Extra::default()).is_err());
    }

    #[test]
    fn apiv1_http_body_and_query() {
        let q = BTreeMap::from([
            ("fade".to_string(), "1".to_string()),
            ("title".into(), "Dr".into()),
        ]);
        let x = extra_of(q, br#"{"data":{"name":"Ada","age":36},"zone":"lower"}"#).unwrap();
        assert!(x.fade);
        assert_eq!(x.zone.as_deref(), Some("lower"));
        assert_eq!(x.data.get("name").map(String::as_str), Some("Ada"));
        assert_eq!(x.data.get("age").map(String::as_str), Some("36"));
        assert_eq!(x.data.get("title").map(String::as_str), Some("Dr"));
        let x = extra_of(BTreeMap::new(), br#"{"name":"Ada"}"#).unwrap();
        assert_eq!(x.data.get("name").map(String::as_str), Some("Ada"));
        let x = extra_of(BTreeMap::new(), b"Marie Curie\n").unwrap();
        assert_eq!(x.value.as_deref(), Some("Marie Curie"));
        let x = extra_of(BTreeMap::new(), b"").unwrap();
        assert!(x.value.is_none() && x.data.is_empty());
    }

    #[test]
    fn apiv1_next_follows_the_playlist_like_the_operator() {
        let p = demo();
        let show: Vec<&str> = playlist_items(&p, "show")
            .iter()
            .map(|r| r.id.as_str())
            .collect();
        assert!(show.len() >= 3, "{show:?}");
        let mut st = ChannelState::default();
        let none = Cue::default();
        // Nothing yet: the first of the first playlist.
        assert_eq!(next_target(&p, &st, &none, None).unwrap().id, show[0]);
        // After the one on air.
        on_air(&mut st, &p, show[0], 10.0);
        assert_eq!(next_target(&p, &st, &none, None).unwrap().id, show[1]);
        // A cue wins; on air already, the one after it.
        let cued = Cue {
            cue: Some(show[2].into()),
            last: None,
        };
        assert_eq!(next_target(&p, &st, &cued, None).unwrap().id, show[2]);
        let cued = Cue {
            cue: Some(show[0].into()),
            last: None,
        };
        assert_eq!(next_target(&p, &st, &cued, None).unwrap().id, show[1]);
        // Gone off air: after the last taken.
        let st = ChannelState::default();
        let last = Cue {
            cue: None,
            last: Some(show[1].into()),
        };
        assert_eq!(next_target(&p, &st, &last, None).unwrap().id, show[2]);
        // The end of a playlist that does not loop.
        let end = Cue {
            cue: None,
            last: Some(show.last().unwrap().to_string()),
        };
        assert!(next_target(&p, &st, &end, None).is_none());
        let mut looping = p.clone();
        looping.playlists[0].looping = true;
        assert_eq!(next_target(&looping, &st, &end, None).unwrap().id, show[0]);
    }

    #[test]
    fn apiv1_tally_of_a_small_show() {
        let p = demo();
        let show: Vec<String> = playlist_items(&p, "show")
            .iter()
            .map(|r| r.id.clone())
            .collect();
        let mut st = ChannelState::default();
        on_air(&mut st, &p, &show[0], 10.0);
        // The next one queued behind it in its zone (pvw).
        let r1 = p.rundown.iter().find(|r| r.id == show[0]).unwrap();
        let layer = p.item_zone(r1).unwrap().layer;
        let queued = p
            .rundown
            .iter()
            .rev()
            .find(|r| !show.contains(&r.id))
            .unwrap();
        st.layers.get_mut(&layer).unwrap().next = Some(QueuedTake {
            template_id: queued.template_id.clone(),
            data: queued.data.clone(),
            rundown_id: Some(queued.id.clone()),
            target: 0.0,
        });
        let cue = Cue {
            cue: Some(show[2].clone()),
            last: None,
        };
        let (next, items, zones) = status::project_status(&p, &st, &cue);
        let tally = |id: &str| items.iter().find(|i| i.id == id).unwrap().tally;
        assert_eq!(tally(&show[0]), status::Tally::Pgm);
        assert_eq!(tally(&queued.id), status::Tally::Pvw);
        assert_eq!(next.as_ref().map(|n| n.id.as_str()), Some(show[2].as_str()));
        assert_eq!(tally(&show[2]), status::Tally::Cue);
        let others = items
            .iter()
            .filter(|i| i.tally == status::Tally::Off)
            .count();
        assert_eq!(others, items.len() - 3);
        let z = zones.iter().find(|z| z.layer == layer).unwrap();
        assert!(z.on_air);
        assert_eq!(z.item.as_ref().unwrap().id, show[0]);
        assert_eq!(z.queued.as_ref().unwrap().id, queued.id);
        assert_eq!(items[1].index, 2);
    }

    #[test]
    fn apiv1_countdown_of_an_automatic_out() {
        let mut p = demo();
        let r = p.rundown[0].clone();
        let t = p
            .templates
            .iter_mut()
            .find(|t| t.id == r.template_id)
            .unwrap();
        let out = t
            .markers
            .iter_mut()
            .find(|m| m.kind == cg_core::MarkerKind::Out)
            .unwrap();
        out.resume = cg_core::Resume::After { seconds: 5.0 };
        let hold = t.hold_frame();
        let mut st = ChannelState::default();
        on_air(&mut st, &p, &r.id, 100.0);
        let it = st.layers.values_mut().next().unwrap();
        it.frame = hold;
        // 2 s held at 50 fps: 3 s left.
        it.held = 100.0;
        let (_, _, zones) = status::project_status(&p, &st, &Cue::default());
        let c = zones.iter().find_map(|z| z.countdown.clone()).unwrap();
        assert_eq!(c.kind, "out");
        assert_eq!(c.seconds, 3);
        assert_eq!(c.total, 5.0);
    }

    #[test]
    fn apiv1_allow_list() {
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        assert!(allowed(ip("10.0.0.7"), &[]));
        let list = vec!["10.0.0.0/24".to_string(), "192.168.1.20".into()];
        assert!(allowed(ip("10.0.0.7"), &list));
        assert!(allowed(ip("::ffff:10.0.0.9"), &list));
        assert!(allowed(ip("192.168.1.20"), &list));
        assert!(!allowed(ip("192.168.1.21"), &list));
        assert!(!allowed(ip("10.0.1.7"), &list));
        assert!(allowed(ip("127.0.0.1"), &list));
        assert!(allowed(ip("::1"), &list));
    }

    #[test]
    fn apiv1_words_unescape() {
        assert_eq!(unescape("Channel%201"), "Channel 1");
        assert_eq!(unescape("Marie%20Sk%C5%82odowska"), "Marie Skłodowska");
        assert_eq!(unescape("100%"), "100%");
        assert_eq!(unescape("%zz"), "%zz");
    }
}
