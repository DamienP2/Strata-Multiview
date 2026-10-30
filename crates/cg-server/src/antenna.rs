//! What happens on air, and who does it: the actor of each command, the
//! zones an operator holds on a channel, the last actions that can be
//! undone, and the as-run log (`crate::asrun`).
//!
//! Every command goes through `AppState::command`, which checks the zone
//! locks, keeps the state from before for an undo, applies it, and logs it.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use cg_core::{ChannelState, Command, Project};
use serde::{Deserialize, Serialize};

/// How many actions of a channel can be undone.
pub const UNDO_STEPS: usize = 10;

/// Who did something on air.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    pub kind: ActorKind,
    /// User id, API key id, MOS id, playlist id… ("" when none).
    #[serde(default)]
    pub id: String,
    /// What the as-run log shows: a person's name, "Playlist JT", "MOS · Octopus"…
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActorKind {
    /// A person in the web app.
    User,
    /// An automation system or control surface with an API key (Bitfocus
    /// Buttons, OSC, TCP…).
    Api,
    /// The newsroom system (MOS).
    Mos,
    /// The engine itself: auto next, a timed pause or automatic out, a
    /// countdown's trigger.
    Auto,
    /// A take or out programmed in advance.
    Schedule,
    /// A data source changing what is on air.
    Data,
}

impl Actor {
    pub fn user(u: &crate::auth::User) -> Self {
        Self {
            kind: ActorKind::User,
            id: u.id.clone(),
            name: if u.display_name.trim().is_empty() {
                u.username.clone()
            } else {
                u.display_name.clone()
            },
        }
    }
    pub fn auto(name: &str) -> Self {
        Self {
            kind: ActorKind::Auto,
            id: String::new(),
            name: name.into(),
        }
    }
    /// Zone locks hold people: only they are stopped by another's lock.
    fn bound_by_locks(&self) -> bool {
        self.kind == ActorKind::User
    }
}

/// A zone of a channel held by one operator: the others cannot drive it
/// until they take it (one click; the holder is told) or it is released.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneLock {
    pub zone: String,
    pub user_id: String,
    pub user_name: String,
    /// Server time (ms) it was taken.
    pub since: i64,
}

/// One action that can be undone: the state before it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoStep {
    /// Server time (ms) of the action.
    pub at: i64,
    pub actor: Actor,
    /// "take", "out", "continue", "update", "clear", "row", "widget", "trigger".
    pub action: String,
    /// The graphic it was about ("" for several).
    pub label: String,
    /// The zone it drove ("" for several).
    #[serde(default)]
    pub zone: String,
    /// Graphics it took off air (Clear all), for the undo list.
    #[serde(skip_serializing_if = "is_zero")]
    pub count: usize,
    #[serde(skip)]
    pub before: ChannelState,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// What a channel keeps about its operators.
#[derive(Default)]
pub struct ChannelOps {
    pub locks: Mutex<BTreeMap<String, ZoneLock>>,
    pub undo: Mutex<VecDeque<UndoStep>>,
}

impl ChannelOps {
    pub fn locks(&self) -> Vec<ZoneLock> {
        self.locks
            .lock()
            .map(|l| l.values().cloned().collect())
            .unwrap_or_default()
    }

    /// The lock of `zone` held by someone else than `actor`.
    pub fn held_by_other(&self, zone: &str, actor: &Actor) -> Option<ZoneLock> {
        if !actor.bound_by_locks() {
            return None;
        }
        let l = self.locks.lock().ok()?;
        l.get(zone).filter(|l| l.user_id != actor.id).cloned()
    }

    /// Gives a free `zone` to the person driving it (a take): true when it was free.
    pub fn claim(&self, zone: &str, actor: &Actor, now: i64) -> bool {
        if !actor.bound_by_locks() {
            return false;
        }
        let Ok(mut l) = self.locks.lock() else {
            return false;
        };
        if l.contains_key(zone) {
            return false;
        }
        l.insert(
            zone.into(),
            ZoneLock {
                zone: zone.into(),
                user_id: actor.id.clone(),
                user_name: actor.name.clone(),
                since: now,
            },
        );
        true
    }

    /// Frees the zones of the people `here` says left: true when one was freed.
    pub fn release_absent(&self, here: &dyn Fn(&str) -> bool) -> bool {
        let Ok(mut l) = self.locks.lock() else {
            return false;
        };
        let n = l.len();
        l.retain(|_, z| here(&z.user_id));
        l.len() != n
    }

    pub fn push_undo(&self, step: UndoStep) {
        if let Ok(mut u) = self.undo.lock() {
            u.push_back(step);
            while u.len() > UNDO_STEPS {
                u.pop_front();
            }
        }
    }

    pub fn undo_list(&self) -> Vec<UndoStep> {
        self.undo
            .lock()
            .map(|u| u.iter().rev().cloned().collect())
            .unwrap_or_default()
    }
}

/// Someone with the Operator open on a channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Present {
    pub user_id: String,
    pub user_name: String,
    pub channel_id: String,
}

/// Who is on which channel, per open socket.
#[derive(Default)]
pub struct Presence {
    next: std::sync::atomic::AtomicU64,
    open: Mutex<BTreeMap<u64, Present>>,
}

impl Presence {
    /// A number for a new socket.
    pub fn connect(&self) -> u64 {
        self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Sets (or clears) where a socket is; true when that changed.
    pub fn set(&self, socket: u64, at: Option<Present>) -> bool {
        let Ok(mut open) = self.open.lock() else {
            return false;
        };
        let changed = open.get(&socket) != at.as_ref();
        match at {
            Some(p) => open.insert(socket, p),
            None => open.remove(&socket),
        };
        changed
    }

    /// Everyone, once per user and channel.
    pub fn list(&self) -> Vec<Present> {
        let mut all: Vec<Present> = self
            .open
            .lock()
            .map(|o| o.values().cloned().collect())
            .unwrap_or_default();
        all.sort_by(|a, b| (&a.channel_id, &a.user_name).cmp(&(&b.channel_id, &b.user_name)));
        all.dedup_by(|a, b| a.user_id == b.user_id && a.channel_id == b.channel_id);
        all
    }
}

/// The verb of a command, as the log and the undo list name it.
pub fn action_name(cmd: &Command) -> &'static str {
    match cmd {
        Command::Take { .. } => "take",
        Command::Continue { .. } => "continue",
        Command::TakeOut { .. } => "out",
        Command::Update { .. } => "update",
        Command::Widget { .. } | Command::Act { .. } => "widget",
        Command::Row { .. } => "row",
        Command::Trigger { .. } => "trigger",
        Command::Clear { .. } => "clear",
    }
}

/// The zone a command drives (`None`: several, or none).
pub fn command_zone(project: &Project, cmd: &Command) -> Option<String> {
    let (zone, layer) = match cmd {
        Command::Take {
            zone,
            layer,
            template_id,
            ..
        } => {
            if zone.is_none() && layer.is_none() {
                return project
                    .template(template_id)
                    .map(|t| t.zone.clone())
                    .filter(|z| !z.is_empty());
            }
            (zone, layer)
        }
        Command::Continue { zone, layer }
        | Command::TakeOut { zone, layer }
        | Command::Clear { zone, layer }
        | Command::Update { zone, layer, .. }
        | Command::Widget { zone, layer, .. }
        | Command::Act { zone, layer, .. }
        | Command::Row { zone, layer, .. } => (zone, layer),
        Command::Trigger { .. } => return None,
    };
    if let Some(z) = zone {
        return project.zone(z).map(|z| z.id.clone()).or(Some(z.clone()));
    }
    let l = (*layer)?;
    project
        .zones
        .iter()
        .find(|z| z.layer == l)
        .map(|z| z.id.clone())
}

/// The name of the graphic a command is about, from the state before it
/// (out, continue…) or the template it takes.
pub fn command_label(project: &Project, before: &ChannelState, cmd: &Command) -> String {
    let rundown_name = |id: &Option<String>| {
        id.as_ref()
            .and_then(|id| project.rundown.iter().find(|r| &r.id == id))
            .map(|r| r.name.clone())
    };
    if let Command::Take {
        template_id,
        rundown_id,
        ..
    } = cmd
    {
        return rundown_name(rundown_id)
            .or_else(|| project.template(template_id).map(|t| t.name.clone()))
            .unwrap_or_default();
    }
    let zone = command_zone(project, cmd);
    let layer = zone
        .as_deref()
        .and_then(|z| project.zone(z))
        .map(|z| z.layer);
    layer
        .and_then(|l| before.layers.get(&l))
        .map(|it| {
            // A take waiting for the graphic going out is the one the
            // operator sees as on air.
            let (rundown_id, template_id) = match &it.next {
                Some(n) => (&n.rundown_id, &n.template_id),
                None => (&it.rundown_id, &it.template_id),
            };
            rundown_name(rundown_id)
                .or_else(|| project.template(template_id).map(|t| t.name.clone()))
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(id: &str) -> Actor {
        Actor {
            kind: ActorKind::User,
            id: id.into(),
            name: id.into(),
        }
    }

    #[test]
    fn a_zone_held_stops_only_other_people() {
        let ops = ChannelOps::default();
        ops.locks.lock().unwrap().insert(
            "lower".into(),
            ZoneLock {
                zone: "lower".into(),
                user_id: "thomas".into(),
                user_name: "Thomas".into(),
                since: 0,
            },
        );
        assert!(ops.held_by_other("lower", &person("thomas")).is_none());
        assert!(ops.held_by_other("lower", &person("julie")).is_some());
        assert!(ops.held_by_other("bug", &person("julie")).is_none());
        // A control surface or the engine is not stopped by a person's lock.
        let api = Actor {
            kind: ActorKind::Api,
            id: "k".into(),
            name: "Buttons".into(),
        };
        assert!(ops.held_by_other("lower", &api).is_none());
        assert!(ops
            .held_by_other("lower", &Actor::auto("auto next"))
            .is_none());
    }

    #[test]
    fn a_take_gives_a_free_zone_and_leaving_frees_it() {
        let ops = ChannelOps::default();
        assert!(ops.claim("lower", &person("thomas"), 1));
        // Held: a second take does not move it (taking it over is a click of its own).
        assert!(!ops.claim("lower", &person("julie"), 2));
        assert_eq!(ops.locks()[0].user_id, "thomas");
        // The engine and the control surfaces never hold a zone.
        assert!(!ops.claim("bug", &Actor::auto("auto next"), 3));
        assert!(!ops.release_absent(&|u| u == "thomas"));
        assert!(ops.release_absent(&|u| u == "julie"));
        assert!(ops.locks().is_empty());
    }

    #[test]
    fn only_the_last_ten_actions_can_be_undone() {
        let ops = ChannelOps::default();
        for i in 0..15 {
            ops.push_undo(UndoStep {
                at: i,
                actor: person("julie"),
                action: "take".into(),
                label: format!("#{i}"),
                zone: String::new(),
                count: 0,
                before: ChannelState::default(),
            });
        }
        let list = ops.undo_list();
        assert_eq!(list.len(), UNDO_STEPS);
        assert_eq!(list[0].label, "#14");
        assert_eq!(list[9].label, "#5");
    }
}
