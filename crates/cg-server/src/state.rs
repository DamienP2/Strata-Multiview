use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use cg_core::{ChannelState, Command, Project, ReadMode};
use cg_render::slate::SlateStyle;
use cg_render::Renderer;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, watch, Notify, RwLock};

use crate::auth::Auth;
use crate::config::{slate_style, ChannelConfig, ServerConfig};
use crate::library::{Library, ProjectSummary};

/// Messages pushed to WebSocket clients (filtered per user by the session).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    /// A project was created or saved.
    Project {
        project: Arc<Project>,
    },
    ProjectDeleted {
        id: String,
    },
    /// A project saved with only one of its templates (or drafts) changed
    /// or added: that template, not the whole project.
    #[serde(rename_all = "camelCase")]
    ProjectTemplate {
        project_id: String,
        draft: bool,
        template: Box<cg_core::Template>,
    },
    /// Channel list or loaded projects changed.
    Channels {
        channels: Vec<ChannelInfo>,
    },
    /// Playout state of one channel.
    #[serde(rename_all = "camelCase")]
    Channel {
        channel_id: String,
        state: ChannelState,
        server_time: i64,
    },
    /// Who has the Operator open, on which channel.
    Presence {
        present: Vec<crate::antenna::Present>,
    },
    /// The zones held by operators on a channel changed. `takenFrom` is the
    /// user who lost a zone to another.
    #[serde(rename_all = "camelCase")]
    ZoneLocks {
        channel_id: String,
        locks: Vec<crate::antenna::ZoneLock>,
        taken_from: Option<String>,
    },
    /// A template's edit lock changed (taken, renewed after a take over,
    /// released). `takenFrom` is the designer tab that lost it.
    #[serde(rename_all = "camelCase")]
    Lock {
        project_id: String,
        template_id: String,
        lock: Option<crate::locks::Lock>,
        taken_from: Option<String>,
    },
    /// A data source was checked or changed (the value itself is fetched
    /// with GET /api/data/sources/{id}/value).
    Data {
        source: String,
        status: crate::datasources::Status,
    },
    /// A list other clients show changed and should be fetched again:
    /// `media` (shared media), `media:<project>`, `sources`, `config`.
    Changed {
        what: String,
    },
    /// Folders of the project list changed.
    Folders {
        folders: crate::media::Folders,
    },
    /// A new entry of the as-run log.
    AsRun {
        entry: Box<crate::asrun::Entry>,
    },
    /// What is programmed on a channel changed (`crate::schedule`).
    #[serde(rename_all = "camelCase")]
    Schedule {
        channel_id: String,
        items: Vec<crate::schedule::View>,
    },
    /// A project's social desk changed (`crate::social::moderation`).
    #[serde(rename_all = "camelCase")]
    Social {
        project_id: String,
        change: Box<crate::social::moderation::Change>,
    },
    Error {
        message: String,
    },
}

/// An event as broadcast: written as JSON once, for every client that
/// gets it as it is (not once per client).
#[derive(Debug)]
pub struct Sent {
    pub event: Event,
    json: std::sync::OnceLock<Option<axum::extract::ws::Utf8Bytes>>,
}

impl Sent {
    pub fn json(&self) -> Option<axum::extract::ws::Utf8Bytes> {
        self.json
            .get_or_init(|| serde_json::to_string(&self.event).ok().map(Into::into))
            .clone()
    }
}

/// The events of the server, to the WebSocket clients and the control API.
pub struct Events(broadcast::Sender<Arc<Sent>>);

impl Events {
    pub fn new(capacity: usize) -> Self {
        Self(broadcast::channel(capacity).0)
    }

    pub fn send(&self, event: Event) -> Result<usize, broadcast::error::SendError<Arc<Sent>>> {
        self.0.send(Arc::new(Sent {
            event,
            json: Default::default(),
        }))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Sent>> {
        self.0.subscribe()
    }
}

/// The project a channel shows, for its clock: read without waiting on
/// the locks a save or a command holds (an `Arc` swapped under a lock
/// held only for that).
#[derive(Default)]
pub struct Loaded(std::sync::RwLock<Option<Arc<Project>>>);

impl Loaded {
    pub fn new(p: Option<Arc<Project>>) -> Self {
        Self(std::sync::RwLock::new(p))
    }

    pub fn get(&self) -> Option<Arc<Project>> {
        self.0.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set(&self, p: Option<Arc<Project>>) {
        *self.0.write().unwrap_or_else(|e| e.into_inner()) = p;
    }
}

/// Messages accepted from WebSocket clients.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientMessage {
    #[serde(rename_all = "camelCase")]
    Command {
        channel_id: String,
        command: Command,
    },
    /// The Operator of this channel is open here (`None`: closed).
    #[serde(rename_all = "camelCase")]
    Presence { channel_id: Option<String> },
}

/// A channel as shown to clients.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub frame_rate: cg_core::FrameRate,
    /// Two fields a frame (the rate counts fields).
    pub interlaced: bool,
    pub project_id: Option<String>,
    /// What the channel goes out to: "SDI · name", "NDI · name"…
    pub outputs: Vec<String>,
    /// Extra pictures of the channel (9:16…), each with its HTML5 page
    /// /output/<channel>/<rendition>.
    pub renditions: Vec<RenditionInfo>,
    /// Still picture shown while no project is loaded ("" = none).
    pub slate: String,
    /// The design shown while no project is loaded.
    pub slate_style: SlateStyle,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenditionInfo {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub slate: String,
    pub slate_style: SlateStyle,
}

pub struct ChannelRt {
    pub config: ChannelConfig,
    pub project_id: RwLock<Option<String>>,
    pub state: RwLock<ChannelState>,
    /// The project loaded (`project_id`), for the clock (set with it).
    pub loaded: Loaded,
    /// The state the clock published last (the same one is not sent again).
    pub published: Mutex<Option<ChannelState>>,
    /// Changed since it was last saved to disk.
    pub dirty: std::sync::atomic::AtomicBool,
    /// Zones held by operators, actions that can be undone.
    pub ops: crate::antenna::ChannelOps,
}

/// What a channel has on air, kept on disk so a restart (or a crash) puts
/// it back: graphics, their data, running timers (timestamps).
#[derive(Serialize, Deserialize)]
struct OnAir {
    project_id: Option<String>,
    state: ChannelState,
}

fn on_air_path(data_dir: &std::path::Path, channel: &str) -> PathBuf {
    let safe: String = channel
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    data_dir.join("on-air").join(format!("{safe}.json"))
}

/// What the channel had on air with this project, when the engine stopped.
pub fn load_on_air(
    data_dir: &std::path::Path,
    channel: &str,
    project: Option<&str>,
) -> ChannelState {
    std::fs::read(on_air_path(data_dir, channel))
        .ok()
        .and_then(|b| serde_json::from_slice::<OnAir>(&b).ok())
        .filter(|o| o.project_id.as_deref() == project)
        .map(|o| o.state)
        .unwrap_or_default()
}

/// Saves what the channel has on air (written aside, then renamed).
pub fn save_on_air(
    data_dir: &std::path::Path,
    channel: &str,
    project: Option<String>,
    state: ChannelState,
) {
    let path = on_air_path(data_dir, channel);
    let res = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(path.parent().expect("in a folder"))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec(&OnAir {
                project_id: project,
                state,
            })?,
        )?;
        std::fs::rename(tmp, &path)
    })();
    if let Err(e) = res {
        tracing::warn!("cannot save what {channel} has on air: {e}");
    }
}

impl ChannelRt {
    pub async fn info(&self) -> ChannelInfo {
        let c = &self.config;
        let outputs = c
            .pictures()
            .into_iter()
            .flat_map(|(_, _, _, o)| {
                o.st2110
                    .iter()
                    .filter(|o| o.enabled)
                    .map(|o| format!("ST 2110 · {}", o.name))
                    .chain(
                        o.sdi
                            .iter()
                            .filter(|o| o.enabled)
                            .map(|o| format!("SDI · {}", o.name)),
                    )
                    .chain(
                        o.ndi
                            .iter()
                            .filter(|o| o.enabled)
                            .map(|o| format!("NDI · {}", o.name)),
                    )
                    .chain(o.streams.iter().filter(|o| o.enabled).map(|o| {
                        format!(
                            "{} · {}",
                            format!("{:?}", o.protocol).to_uppercase(),
                            o.name
                        )
                    }))
                    .chain(o.html5.iter().map(|o| format!("HTML5 · {}", o.name)))
            })
            .collect();
        ChannelInfo {
            id: c.id.clone(),
            name: c.name.clone(),
            width: c.width,
            height: c.height,
            frame_rate: c.frame_rate,
            interlaced: c.interlaced,
            project_id: self.project_id.read().await.clone(),
            outputs,
            renditions: c
                .renditions
                .iter()
                .map(|r| RenditionInfo {
                    id: r.id.clone(),
                    name: r.name.clone(),
                    width: r.width,
                    height: r.height,
                    slate: r.slate.clone(),
                    slate_style: slate_style(r.slate_style, &r.slate),
                })
                .collect(),
            slate: c.slate.clone(),
            slate_style: slate_style(c.slate_style, &c.slate),
        }
    }
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub library: RwLock<Library>,
    pub channels: BTreeMap<String, Arc<ChannelRt>>,
    pub events: Events,
    pub renderer: Mutex<Renderer>,
    /// Thumbnails made last (PNG), see `crate::thumbs`.
    pub thumbs: crate::thumbs::Thumbs,
    /// Held while a project is written to disk: saves one after the other,
    /// without holding the library (the playout clocks read it).
    pub saving: tokio::sync::Mutex<()>,
    /// Users and sessions outlive engine restarts.
    pub auth: Arc<Auth>,
    /// Running orders received from the newsroom system (MOS).
    pub mos: Arc<crate::mos::Store>,
    /// Data connectors and their latest values.
    pub data: Arc<crate::datasources::Hub>,
    /// Social network connections and their messages.
    pub social: Arc<crate::social::Hub>,
    /// What each project does with them: moderation, hashtags, on air.
    pub moderation: Arc<crate::social::moderation::Desk>,
    /// Who edits which template in the designer.
    pub locks: Arc<crate::locks::Locks>,
    /// Held from reading a project to saving it, so two edits at the same
    /// time (an autosave and a rundown change…) never undo each other.
    pub edits: tokio::sync::Mutex<()>,
    /// Whether each video output runs, by channel and output id.
    pub output_status: Arc<OutputStatus>,
    /// Live video inputs (kept across engine restarts).
    pub inputs: Arc<crate::inputs::Inputs>,
    /// Load of the machine, the channels, outputs and inputs (kept across engine restarts).
    pub perf: Arc<crate::perf::Perf>,
    /// Settings this engine cycle was started with.
    pub config: ServerConfig,
    /// Address actually bound by this cycle.
    pub bind: SocketAddr,
    pub started_at: SystemTime,
    /// Ask the server loop to rebuild the engine with fresh settings.
    pub restart: Notify,
    /// Flips to true when this cycle ends, so long-lived WebSockets close.
    pub closing: watch::Sender<bool>,
    /// Who has the Operator open, on which channel.
    pub presence: crate::antenna::Presence,
    /// Takes and outs programmed in advance.
    pub schedule: crate::schedule::Schedule,
}

pub type Shared = Arc<AppState>;

/// State of one video output (SDI, ST 2110, NDI).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputState {
    pub ok: bool,
    pub message: String,
    /// The full error, when it says more than `message`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

/// Video output states, by channel id then output id.
#[derive(Default)]
pub struct OutputStatus {
    map: std::sync::Mutex<BTreeMap<String, BTreeMap<String, OutputState>>>,
}

impl OutputStatus {
    pub fn set(&self, channel: &str, output: &str, ok: bool, message: impl Into<String>) {
        self.set_detailed(channel, output, ok, message, String::new());
    }

    pub fn set_detailed(
        &self,
        channel: &str,
        output: &str,
        ok: bool,
        message: impl Into<String>,
        detail: String,
    ) {
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(channel.into()).or_default().insert(
            output.into(),
            OutputState {
                ok,
                message: message.into(),
                detail,
            },
        );
    }

    /// Forgets a channel's outputs (the engine restarts them).
    pub fn reset(&self, channel: &str) {
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(channel);
    }

    pub fn snapshot(&self) -> BTreeMap<String, BTreeMap<String, OutputState>> {
        self.map.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl AppState {
    /// Tells the clients that a list changed (see [`Event::Changed`]).
    pub fn changed(&self, what: impl Into<String>) {
        let _ = self.events.send(Event::Changed { what: what.into() });
    }

    /// Where the HLS stream outputs write (served at `/hls/…`).
    pub fn hls_dir(&self) -> PathBuf {
        self.data_dir.join("hls")
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.data_dir.join("assets")
    }

    pub fn channel(&self, id: &str) -> Option<&Arc<ChannelRt>> {
        self.channels.get(id)
    }

    pub async fn project(&self, id: &str) -> Option<Arc<Project>> {
        self.library.read().await.get(id)
    }

    pub async fn summaries(&self) -> Vec<ProjectSummary> {
        let lib = self.library.read().await;
        lib.list().map(|p| lib.summary(p)).collect()
    }

    pub async fn channel_infos(&self) -> Vec<ChannelInfo> {
        let mut out = Vec::new();
        for c in self.channels.values() {
            out.push(c.info().await);
        }
        out
    }

    pub async fn save_project(&self, mut project: Project) -> anyhow::Result<Arc<Project>> {
        project.ensure_zones();
        // Written (compact) off the library lock, saves in turn; the
        // library is locked only to swap the project in.
        let _saving = self.saving.lock().await;
        let p = Arc::new(project);
        let (dir, file) = (self.library.read().await.dir().to_path_buf(), p.clone());
        tokio::task::spawn_blocking(move || Library::write_file(&dir, &file)).await??;
        let old = self.library.write().await.insert(p.clone());
        for ch in self.channels.values() {
            let pid = ch.project_id.read().await;
            if pid.as_deref() == Some(p.id.as_str()) {
                ch.loaded.set(Some(p.clone()));
            }
        }
        let event = match old.as_deref().and_then(|old| template_change(old, &p)) {
            Some((draft, template)) => Event::ProjectTemplate {
                project_id: p.id.clone(),
                draft,
                template: Box::new(template),
            },
            None => Event::Project { project: p.clone() },
        };
        let _ = self.events.send(event);
        Ok(p)
    }

    pub async fn delete_project(&self, id: &str) -> anyhow::Result<bool> {
        let deleted = {
            let _saving = self.saving.lock().await;
            self.library.write().await.delete(id)?
        };
        if deleted {
            // Unload it from every channel showing it.
            for ch in self.channels.values() {
                let mut pid = ch.project_id.write().await;
                if pid.as_deref() == Some(id) {
                    *pid = None;
                    ch.loaded.set(None);
                    ch.state.write().await.layers.clear();
                }
            }
            self.persist_loaded_projects().await;
            let _ = self.events.send(Event::ProjectDeleted { id: id.into() });
            let _ = self.events.send(Event::Channels {
                channels: self.channel_infos().await,
            });
        }
        Ok(deleted)
    }

    /// Loads a project on a channel (clears what is on air).
    pub async fn load_project(
        &self,
        channel_id: &str,
        project_id: Option<String>,
    ) -> Result<(), String> {
        let ch = self.channel(channel_id).ok_or("no such channel")?;
        if let Some(pid) = &project_id {
            if self.project(pid).await.is_none() {
                return Err("no such project".into());
            }
        }
        {
            let mut pid = ch.project_id.write().await;
            let loaded = match &project_id {
                Some(id) => self.project(id).await,
                None => None,
            };
            *pid = project_id;
            ch.loaded.set(loaded);
        }
        ch.state.write().await.layers.clear();
        self.persist_loaded_projects().await;
        self.broadcast_channel(ch).await;
        let _ = self.events.send(Event::Channels {
            channels: self.channel_infos().await,
        });
        Ok(())
    }

    /// Re-keys graphics on air after zones of a project changed layer.
    pub async fn move_layers(&self, project_id: &str, moves: &[(u32, u32)]) {
        for ch in self.channels.values() {
            if ch.project_id.read().await.as_deref() != Some(project_id) {
                continue;
            }
            {
                let mut st = ch.state.write().await;
                let mut taken: Vec<(u32, cg_core::PlayoutItem)> = moves
                    .iter()
                    .filter_map(|(from, to)| st.layers.remove(from).map(|i| (*to, i)))
                    .collect();
                for (to, item) in taken.drain(..) {
                    st.layers.insert(to, item);
                }
            }
            self.broadcast_channel(ch).await;
        }
    }

    /// Remembers which project each channel shows across restarts.
    async fn persist_loaded_projects(&self) {
        let mut cfg = ServerConfig::load(&self.data_dir);
        for c in cfg.channels.iter_mut() {
            if let Some(rt) = self.channels.get(&c.id) {
                c.project_id = rt.project_id.read().await.clone();
            }
        }
        if let Err(e) = cfg.save(&self.data_dir) {
            tracing::warn!("cannot save loaded projects: {e}");
        }
    }

    /// Tells clients the state the playout clock just made (not the same
    /// one again).
    pub fn publish_channel(&self, ch: &ChannelRt, state: ChannelState) {
        {
            let mut last = ch.published.lock().unwrap_or_else(|e| e.into_inner());
            if last.as_ref() == Some(&state) {
                return;
            }
            *last = Some(state.clone());
        }
        ch.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = self.events.send(Event::Channel {
            channel_id: ch.config.id.clone(),
            state,
            server_time: now_ms(),
        });
    }

    pub async fn broadcast_channel(&self, ch: &ChannelRt) {
        ch.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
        let state = ch.state.read().await.clone();
        let _ = self.events.send(Event::Channel {
            channel_id: ch.config.id.clone(),
            state,
            server_time: now_ms(),
        });
    }

    /// Applies a command on air for `actor`: refused on a zone another
    /// operator holds, kept for an undo, written to the as-run log.
    pub async fn command(
        &self,
        channel_id: &str,
        cmd: Command,
        actor: &crate::antenna::Actor,
    ) -> Result<ChannelState, String> {
        let ch = self.channel(channel_id).ok_or("no such channel")?;
        let pid = ch
            .project_id
            .read()
            .await
            .clone()
            .ok_or("no project loaded on this channel")?;
        let project = self
            .project(&pid)
            .await
            .ok_or("the loaded project no longer exists")?;
        let store = self.data.store();
        let zone = crate::antenna::command_zone(&project, &cmd);
        if let Some(l) = zone.as_deref().and_then(|z| ch.ops.held_by_other(z, actor)) {
            return Err(format!("{} holds this zone", l.user_name));
        }
        // Taking a graphic on a free zone gives it to the operator.
        if let (Command::Take { .. }, Some(z)) = (&cmd, zone.as_deref()) {
            if ch.ops.claim(z, actor, now_ms()) {
                let _ = self.events.send(Event::ZoneLocks {
                    channel_id: channel_id.into(),
                    locks: ch.ops.locks(),
                    taken_from: None,
                });
            }
        }
        // Everything out: but the zones others hold.
        let others: Vec<String> = project
            .zones
            .iter()
            .filter(|z| ch.ops.held_by_other(&z.id, actor).is_some())
            .map(|z| z.id.clone())
            .collect();
        if let Command::Clear {
            zone: None,
            layer: None,
        } = &cmd
        {
            if !others.is_empty() {
                let mut last = Ok(ch.state.read().await.clone());
                for z in project.zones.iter().filter(|z| !others.contains(&z.id)) {
                    let one = Command::Clear {
                        zone: Some(z.id.clone()),
                        layer: None,
                    };
                    last = Box::pin(self.command(channel_id, one, actor)).await;
                }
                return last;
            }
        }
        let action = crate::antenna::action_name(&cmd);
        let mut values = BTreeMap::new();
        let snapshot = {
            let mut st = ch.state.write().await;
            let before = st.clone();
            let label = crate::antenna::command_label(&project, &before, &cmd);
            let old = zone
                .as_deref()
                .and_then(|z| project.zone(z))
                .and_then(|z| before.layers.get(&z.layer))
                .map(|it| it.data.clone())
                .unwrap_or_default();
            if let Command::Update { data, .. }
            | Command::Take {
                data: Some(data), ..
            } = &cmd
            {
                for (k, v) in data {
                    if k.starts_with("__") || old.get(k) == Some(v) {
                        continue;
                    }
                    values.insert(k.clone(), (old.get(k).cloned(), v.clone()));
                }
            }
            let taken = match &cmd {
                Command::Take { rundown_id, .. } => rundown_id.clone(),
                _ => None,
            };
            let mut cmd = cmd;
            // Values set by hand for some seconds: until when, on the server's clock.
            if let Command::Update { data, .. }
            | Command::Take {
                data: Some(data), ..
            } = &mut cmd
            {
                for (k, v) in data.iter_mut() {
                    if let Some(secs) = k
                        .ends_with("__manual")
                        .then(|| v.strip_prefix("for:"))
                        .flatten()
                        .and_then(|n| n.trim().parse::<f64>().ok())
                        .filter(|s| s.is_finite() && *s >= 0.0)
                    {
                        // At most a day: no overflow, and never "manual for ever" by accident.
                        let ms = (secs.min(86_400.0) * 1000.0) as i64;
                        *v = format!("until:{}", now_ms().saturating_add(ms));
                    }
                }
            }
            // A widget's "reload": its graphic's sources are read again now.
            if let Command::Act {
                zone, layer, op, ..
            } = &cmd
            {
                if op == "reload" {
                    let at = layer.or_else(|| {
                        zone.as_deref()
                            .and_then(|z| project.zone(z))
                            .map(|z| z.layer)
                    });
                    if let Some(t) = at
                        .and_then(|l| st.layers.get(&l))
                        .and_then(|it| project.template(&it.template_id))
                    {
                        for src in cg_core::datapath::sources(t) {
                            self.data.refresh(src);
                        }
                    }
                }
            }
            st.apply(&project, cmd).map_err(|e| e.to_string())?;
            bind_state(&project, &mut st, &store, None, taken.as_deref(), false);
            if actor.kind != crate::antenna::ActorKind::Auto {
                ch.ops.push_undo(crate::antenna::UndoStep {
                    at: now_ms(),
                    actor: actor.clone(),
                    action: action.into(),
                    label: label.clone(),
                    zone: zone.clone().unwrap_or_default(),
                    count: if action == "clear" {
                        before.layers.len().saturating_sub(st.layers.len())
                    } else {
                        0
                    },
                    before,
                });
            }
            crate::asrun::record(
                self,
                crate::asrun::Entry {
                    at: now_ms(),
                    channel_id: channel_id.into(),
                    channel_name: ch.config.name.clone(),
                    project_id: project.id.clone(),
                    project_name: project.name.clone(),
                    action: action.into(),
                    label,
                    zone: zone.clone().unwrap_or_default(),
                    actor: actor.clone(),
                    values: std::mem::take(&mut values),
                    detail: String::new(),
                },
            );
            st.clone()
        };
        let _ = self.events.send(Event::Channel {
            channel_id: channel_id.into(),
            state: snapshot.clone(),
            server_time: now_ms(),
        });
        Ok(snapshot)
    }

    /// Puts the channel back as it was before its last action that can be
    /// undone (the last `UNDO_STEPS` are kept).
    pub async fn undo(
        &self,
        channel_id: &str,
        actor: &crate::antenna::Actor,
    ) -> Result<ChannelState, String> {
        let ch = self.channel(channel_id).ok_or("no such channel")?;
        let step = {
            let mut undo = ch.ops.undo.lock().map_err(|_| "busy".to_string())?;
            let last = undo.back().ok_or("nothing to undo")?;
            // Not an action in a zone another operator holds now.
            if let Some(l) = ch.ops.held_by_other(&last.zone, actor) {
                return Err(format!("{} holds this zone", l.user_name));
            }
            undo.pop_back().ok_or("nothing to undo")?
        };
        let project = match ch.project_id.read().await.clone() {
            Some(id) => self.project(&id).await,
            None => None,
        };
        let snapshot = {
            let mut st = ch.state.write().await;
            let mut before = step.before.clone();
            // An action in one zone: the others stay as they are now.
            let layer = project
                .as_ref()
                .and_then(|p| p.zone(&step.zone))
                .map(|z| z.layer);
            if let Some(layer) = layer {
                let mut only = st.clone();
                match before.layers.remove(&layer) {
                    Some(it) => only.layers.insert(layer, it),
                    None => only.layers.remove(&layer),
                };
                before = only;
            }
            // Values changed on air go back without replaying the graphic.
            if matches!(step.action.as_str(), "update" | "widget" | "row") {
                for (layer, it) in before.layers.iter_mut() {
                    if let Some(now) = st.layers.get(layer).filter(|n| {
                        n.template_id == it.template_id && n.rundown_id == it.rundown_id
                    }) {
                        it.frame = now.frame;
                        it.target = now.target;
                        it.phase = now.phase;
                        it.age = now.age;
                        it.held = now.held;
                        it.pause_ahead = now.pause_ahead;
                        it.clocks = now.clocks.clone();
                        it.advanced = now.advanced;
                    }
                }
            }
            *st = before;
            if let Some(p) = &project {
                bind_state(p, &mut st, &self.data.store(), None, None, false);
            }
            st.clone()
        };
        crate::asrun::record(
            self,
            crate::asrun::Entry {
                at: now_ms(),
                channel_id: channel_id.into(),
                channel_name: ch.config.name.clone(),
                project_id: project.as_ref().map(|p| p.id.clone()).unwrap_or_default(),
                project_name: project.as_ref().map(|p| p.name.clone()).unwrap_or_default(),
                action: "undo".into(),
                label: step.label.clone(),
                zone: step.zone.clone(),
                actor: actor.clone(),
                values: BTreeMap::new(),
                detail: step.action.clone(),
            },
        );
        let _ = self.events.send(Event::Channel {
            channel_id: channel_id.into(),
            state: snapshot.clone(),
            server_time: now_ms(),
        });
        Ok(snapshot)
    }

    /// Frees the zones held by people who no longer have the channel's
    /// Operator open.
    pub fn release_absent_locks(&self) {
        let present = self.presence.list();
        for (id, ch) in &self.channels {
            let here = |u: &str| {
                present
                    .iter()
                    .any(|p| p.user_id == u && &p.channel_id == id)
            };
            if ch.ops.release_absent(&here) {
                let _ = self.events.send(Event::ZoneLocks {
                    channel_id: id.clone(),
                    locks: ch.ops.locks(),
                    taken_from: None,
                });
            }
        }
    }

    /// Gives `zone` of a channel to `user` (taken from whoever held it, who
    /// is told), or frees it.
    pub fn set_zone_lock(
        &self,
        channel_id: &str,
        zone: &str,
        user: Option<&crate::auth::User>,
    ) -> Result<(), String> {
        let ch = self.channel(channel_id).ok_or("no such channel")?;
        let mut taken_from = None;
        {
            let mut locks = ch.ops.locks.lock().map_err(|_| "busy".to_string())?;
            match user {
                Some(u) => {
                    let me = crate::antenna::Actor::user(u);
                    if let Some(prev) = locks.get(zone).filter(|l| l.user_id != u.id) {
                        taken_from = Some(prev.user_id.clone());
                    }
                    locks.insert(
                        zone.into(),
                        crate::antenna::ZoneLock {
                            zone: zone.into(),
                            user_id: u.id.clone(),
                            user_name: me.name,
                            since: now_ms(),
                        },
                    );
                }
                None => {
                    locks.remove(zone);
                }
            }
        }
        let _ = self.events.send(Event::ZoneLocks {
            channel_id: channel_id.into(),
            locks: ch.ops.locks(),
            taken_from,
        });
        Ok(())
    }
}

/// Fills fields bound to data sources into the graphics on air (and the
/// ones queued); `only` limits it to templates reading that source. Items
/// read "on take" are only filled when `taken` (the rundown item just
/// taken), items read "manually" never: the operator saved their values.
pub fn bind_state(
    project: &Project,
    st: &mut ChannelState,
    store: &cg_core::datapath::Store,
    only: Option<&str>,
    taken: Option<&str>,
    live: bool,
) -> bool {
    let mut changed = false;
    for item in st.layers.values_mut() {
        let mut one = |template_id: &str, rundown_id: Option<&str>, data: &mut cg_core::Data| {
            let Some(t) = project.template(template_id) else {
                return;
            };
            // Data sets always follow their source, on the row shown.
            if only.is_none_or(|src| t.datasets.iter().any(|d| d.source == src)) {
                changed |= cg_core::datapath::bind_datasets(t, data, store);
            }
            let mode = rundown_id
                .and_then(|id| project.rundown.iter().find(|r| r.id == id))
                .map(|r| r.read)
                .unwrap_or_default();
            match mode {
                ReadMode::Live => {}
                ReadMode::Take if taken.is_some() && taken == rundown_id => {}
                _ => return,
            }
            if only.is_some_and(|src| !cg_core::datapath::sources(t).any(|s| s == src)) {
                return;
            }
            changed |= cg_core::datapath::bind(t, data, store);
        };
        let old = item.data.clone();
        one(
            &item.template_id,
            item.rundown_id.as_deref(),
            &mut item.data,
        );
        if item.data != old {
            // Widgets that play a change of their list (the social wall's cards).
            cg_core::playout::source_data_changed(project, item, &old);
            // New values of the sources shown as their layers say (fade,
            // slide, count, held until the operator updates…).
            if live {
                cg_core::playout::live_changes(project, item, &old);
            }
        }
        if let Some(q) = &mut item.next {
            one(&q.template_id, q.rundown_id.as_deref(), &mut q.data);
        }
    }
    changed
}

impl AppState {
    /// Re-applies data bindings on every channel after a source changed.
    pub async fn rebind(&self, source: Option<&str>) {
        let store = self.data.store();
        for ch in self.channels.values() {
            let Some(pid) = ch.project_id.read().await.clone() else {
                continue;
            };
            let Some(project) = self.project(&pid).await else {
                continue;
            };
            let changed = {
                let mut st = ch.state.write().await;
                bind_state(&project, &mut st, &store, source, None, true)
            };
            if changed {
                self.broadcast_channel(ch).await;
            }
        }
    }
}

/// What clients do with an `Event::ProjectTemplate`: the template replaces
/// the one with its id in the templates (or drafts), else is added last.
pub fn apply_template(p: &mut Project, draft: bool, t: cg_core::Template) {
    let list = if draft {
        &mut p.drafts
    } else {
        &mut p.templates
    };
    match list.iter_mut().find(|x| x.id == t.id) {
        Some(x) => *x = t,
        None => list.push(t),
    }
}

/// The one template (or draft) a save changed or added, when that is all
/// it changed: `old` with it applied is `new`.
fn template_change(old: &Project, new: &Project) -> Option<(bool, cg_core::Template)> {
    for draft in [false, true] {
        let (a, b) = if draft {
            (&old.drafts, &new.drafts)
        } else {
            (&old.templates, &new.templates)
        };
        if b.len() != a.len() && b.len() != a.len() + 1 {
            continue;
        }
        let Some(i) = (0..b.len()).find(|&i| a.get(i) != b.get(i)) else {
            continue;
        };
        let t = b[i].clone();
        let mut patched = old.clone();
        apply_template(&mut patched, draft, t.clone());
        return (patched == *new).then_some((draft, t));
    }
    None
}

/// Resolves once the watched flag becomes true (or its sender is gone).
pub async fn until_true(mut rx: watch::Receiver<bool>) {
    while !*rx.borrow_and_update() {
        if rx.changed().await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A save that changed one template is told as that template; clients
    /// applying it get the saved project.
    #[test]
    fn one_template_changed_or_added() {
        let old = cg_core::sample::demo_project();
        let mut new = old.clone();
        new.templates[1].name.push_str(" (2)");
        let (draft, t) = template_change(&old, &new).unwrap();
        assert!(!draft && t.id == new.templates[1].id);
        let mut applied = old.clone();
        apply_template(&mut applied, draft, t);
        assert_eq!(applied, new);
        // A new draft.
        let mut new = old.clone();
        let mut d = old.templates[0].clone();
        d.id = "new-draft".into();
        new.drafts.push(d);
        assert_eq!(template_change(&old, &new).map(|c| c.0), Some(true));
        // Two templates, or something else: the whole project.
        let mut new = old.clone();
        new.templates[0].name.push('x');
        new.templates[1].name.push('x');
        assert!(template_change(&old, &new).is_none());
        let mut new = old.clone();
        new.templates[0].name.push('x');
        new.name.push('x');
        assert!(template_change(&old, &new).is_none());
        assert!(template_change(&old, &old).is_none());
    }
}
