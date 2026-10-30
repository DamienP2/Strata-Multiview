//! The status control surfaces show: the tally of every graphic, what each
//! zone has on air, countdowns, widgets, zone locks and health.

use cg_core::model::{MarkerKind, Resume};
use cg_core::{ChannelState, Phase, PlayoutItem, Project, Template};
use serde::Serialize;

use super::{cue_of, next_target, Cue};
use crate::auth::User;
use crate::state::{ChannelRt, Shared};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Always "status" (the feedback socket only sends these).
    #[serde(rename = "type")]
    pub kind: &'static str,
    /// Server time (ms) it was made.
    pub server_time: i64,
    /// Control surfaces connected (feedback sockets, TCP, OSC senders).
    pub surfaces: usize,
    pub control: Control,
    pub channels: Vec<ChannelStatus>,
}

/// The OSC and TCP listeners.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Control {
    pub osc: Listener,
    pub tcp: Listener,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listener {
    pub enabled: bool,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Named {
    pub id: String,
    pub name: String,
}

/// The tally of a graphic: `pgm` on air, `pvw` taken and waiting for its
/// zone to clear, `cue` what NEXT takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tally {
    Off,
    Cue,
    Pvw,
    Pgm,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemStatus {
    pub id: String,
    pub name: String,
    /// Position in the rundown (1 = first), as the API takes it.
    pub index: usize,
    pub playlist: String,
    pub zone: String,
    pub tally: Tally,
    /// Rows (or values) of its data sources breaking their rules.
    pub warnings: usize,
}

/// What a graphic on air does by itself next, the soonest.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Countdown {
    /// "resume" (a timed pause plays on), "out", "next" (auto next).
    pub kind: &'static str,
    /// Seconds left, rounded up.
    pub seconds: u32,
    /// The whole delay.
    pub total: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetStatus {
    pub id: String,
    pub name: String,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneStatus {
    pub id: String,
    pub name: String,
    pub layer: u32,
    pub on_air: bool,
    /// The graphic of the rundown on air (the template when taken without one).
    pub item: Option<Named>,
    pub template: Option<Named>,
    pub phase: Option<Phase>,
    /// Holding on a pause that waits for Continue or a trigger.
    pub paused: bool,
    /// The graphic coming in once this one is out.
    pub queued: Option<Named>,
    pub countdown: Option<Countdown>,
    /// The operator holding the zone.
    pub locked_by: Option<String>,
    /// Timed widgets of the graphic on air.
    pub widgets: Vec<WidgetStatus>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    /// Every output runs.
    pub ok: bool,
    /// Pictures rendered per second (main picture), rounded.
    pub fps: Option<f64>,
    pub outputs: usize,
    pub outputs_ok: usize,
    /// "output: message" of those that fail.
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatus {
    pub id: String,
    pub name: String,
    pub project: Option<Named>,
    /// What NEXT takes.
    pub next: Option<Named>,
    pub items: Vec<ItemStatus>,
    pub zones: Vec<ZoneStatus>,
    pub locks: Vec<crate::antenna::ZoneLock>,
    /// Actions that can be undone.
    pub undo: usize,
    pub health: Health,
}

fn named_item(p: &Project, rundown_id: Option<&str>, template_id: &str) -> Option<Named> {
    rundown_id
        .and_then(|id| p.rundown.iter().find(|r| r.id == id))
        .map(|r| Named {
            id: r.id.clone(),
            name: r.name.clone(),
        })
        .or_else(|| {
            p.template(template_id).map(|t| Named {
                id: t.id.clone(),
                name: t.name.clone(),
            })
        })
}

/// What the graphic does by itself next (as the Operator shows it): a
/// timed pause plays on, it goes out after its time, or auto next.
pub fn countdown(p: &Project, t: &Template, it: &PlayoutItem) -> Option<Countdown> {
    if it.phase != Phase::Holding {
        return None;
    }
    let fps = p.frame_rate.as_f64().max(1.0);
    let held = it.held / fps;
    let step = |kind, secs: f64| Countdown {
        kind,
        seconds: (secs.max(0.0) - held).max(0.0).ceil() as u32,
        total: secs.max(0.0),
    };
    if let Some(m) = t.pause_at(it.frame) {
        return match m.resume {
            Resume::After { seconds } => Some(step("resume", seconds)),
            _ => None,
        };
    }
    if it.frame < t.hold_frame() - 1e-6 {
        return None;
    }
    let out = t
        .markers
        .iter()
        .find(|m| m.kind == MarkerKind::Out)
        .and_then(|m| match m.resume {
            Resume::After { seconds } => Some(step("out", seconds)),
            _ => None,
        });
    let next = it
        .rundown_id
        .as_ref()
        .and_then(|id| p.rundown.iter().find(|r| &r.id == id))
        .filter(|_| !it.advanced)
        .and_then(|r| {
            let auto = p
                .playlists
                .iter()
                .any(|l| l.id == r.playlist && l.auto_next);
            r.auto_secs.filter(|_| auto).map(|s| step("next", s))
        });
    [out, next]
        .into_iter()
        .flatten()
        .min_by(|a, b| (a.total - held).total_cmp(&(b.total - held)))
}

/// Holding on a pause marker that the operator (or a trigger) resumes.
fn paused(t: &Template, it: &PlayoutItem) -> bool {
    matches!(it.phase, Phase::Holding | Phase::OutHold)
        && t.pause_at(it.frame)
            .is_some_and(|m| !matches!(m.resume, Resume::After { .. }))
}

fn widgets(t: &Template, it: &PlayoutItem) -> Vec<WidgetStatus> {
    t.layers
        .iter()
        .filter(|l| cg_core::playout::is_timed(&l.kind))
        .map(|l| WidgetStatus {
            id: l.id.clone(),
            name: l.name.clone(),
            running: match it.clocks.get(&l.id) {
                Some(c) => c.paused_at.is_none(),
                None => !l.manual_start,
            },
        })
        .collect()
}

/// What NEXT takes, the graphics with their tally, and the zones.
pub fn project_status(
    p: &Project,
    st: &ChannelState,
    mem: &Cue,
) -> (Option<Named>, Vec<ItemStatus>, Vec<ZoneStatus>) {
    let next = next_target(p, st, mem, None);
    let tally = |id: &str| {
        if st
            .layers
            .values()
            .any(|it| it.rundown_id.as_deref() == Some(id))
        {
            Tally::Pgm
        } else if st.layers.values().any(|it| {
            it.next
                .as_ref()
                .is_some_and(|q| q.rundown_id.as_deref() == Some(id))
        }) {
            Tally::Pvw
        } else if next.is_some_and(|n| n.id == id) {
            Tally::Cue
        } else {
            Tally::Off
        }
    };
    let items = p
        .rundown
        .iter()
        .enumerate()
        .map(|(i, r)| ItemStatus {
            id: r.id.clone(),
            name: r.name.clone(),
            index: i + 1,
            playlist: r.playlist.clone(),
            zone: p.item_zone(r).map(|z| z.id.clone()).unwrap_or_default(),
            tally: tally(&r.id),
            warnings: 0,
        })
        .collect();
    let zones = p
        .zones
        .iter()
        .map(|z| {
            let it = st.layers.get(&z.layer);
            let t = it.and_then(|it| p.template(&it.template_id));
            ZoneStatus {
                id: z.id.clone(),
                name: z.name.clone(),
                layer: z.layer,
                on_air: it.is_some(),
                item: it.and_then(|it| named_item(p, it.rundown_id.as_deref(), &it.template_id)),
                template: t.map(|t| Named {
                    id: t.id.clone(),
                    name: t.name.clone(),
                }),
                phase: it.map(|it| it.phase),
                paused: it.zip(t).is_some_and(|(it, t)| paused(t, it)),
                queued: it
                    .and_then(|it| it.next.as_ref())
                    .and_then(|q| named_item(p, q.rundown_id.as_deref(), &q.template_id)),
                countdown: it.zip(t).and_then(|(it, t)| countdown(p, t, it)),
                locked_by: None,
                widgets: it.zip(t).map(|(it, t)| widgets(t, it)).unwrap_or_default(),
            }
        })
        .collect();
    (
        next.map(|n| Named {
            id: n.id.clone(),
            name: n.name.clone(),
        }),
        items,
        zones,
    )
}

fn health(s: &Shared, channel: &str) -> Health {
    let outputs = s
        .output_status
        .snapshot()
        .remove(channel)
        .unwrap_or_default();
    let errors: Vec<String> = outputs
        .iter()
        .filter(|(_, o)| !o.ok)
        .map(|(id, o)| format!("{id}: {}", o.message))
        .collect();
    let fps = s
        .perf
        .sampled()
        .pictures
        .get(&format!("{channel}/main"))
        .map(|r| r.fps.round());
    Health {
        ok: errors.is_empty(),
        fps,
        outputs: outputs.len(),
        outputs_ok: outputs.len() - errors.len(),
        errors,
    }
}

/// The status of one channel, as `user` may see it (`None`: everything).
pub async fn channel_status(s: &Shared, ch: &ChannelRt, user: Option<&User>) -> ChannelStatus {
    let id = ch.config.id.clone();
    let pid = ch.project_id.read().await.clone();
    let project = match pid.filter(|p| user.is_none_or(|u| u.sees_project(p))) {
        Some(p) => s.project(&p).await,
        None => None,
    };
    let st = ch.state.read().await.clone();
    let locks = ch.ops.locks();
    let (next, mut items, mut zones) = match &project {
        Some(p) => project_status(p, &st, &cue_of(&id)),
        None => Default::default(),
    };
    if let Some(p) = &project {
        for (it, r) in items.iter_mut().zip(&p.rundown) {
            if let Some(t) = p.template(&r.template_id) {
                it.warnings = s.data.warnings(cg_core::datapath::sources(t));
            }
        }
    }
    for z in &mut zones {
        z.locked_by = locks
            .iter()
            .find(|l| l.zone == z.id)
            .map(|l| l.user_name.clone());
    }
    ChannelStatus {
        name: ch.config.name.clone(),
        project: project.map(|p| Named {
            id: p.id.clone(),
            name: p.name.clone(),
        }),
        next,
        items,
        zones,
        locks,
        undo: ch.ops.undo.lock().map(|u| u.len()).unwrap_or(0),
        health: health(s, &id),
        id,
    }
}

/// Every channel `user` sees (or only `only`).
pub async fn status(s: &Shared, user: Option<&User>, only: Option<&str>) -> Status {
    let mut channels = Vec::new();
    for ch in s.channels.values() {
        let id = &ch.config.id;
        if only.is_some_and(|o| o != id) || user.is_some_and(|u| !u.sees_channel(id)) {
            continue;
        }
        channels.push(channel_status(s, ch, user).await);
    }
    let c = &s.config.control;
    Status {
        kind: "status",
        server_time: crate::state::now_ms(),
        surfaces: super::surfaces(),
        control: Control {
            osc: Listener {
                enabled: c.osc_enabled,
                port: c.osc_port,
            },
            tcp: Listener {
                enabled: c.tcp_enabled,
                port: c.tcp_port,
            },
        },
        channels,
    }
}
