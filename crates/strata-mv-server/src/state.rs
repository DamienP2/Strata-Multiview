use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use strata_mv_core::{ChannelState, Command, Project};
use strata_mv_render::slate::SlateStyle;
use strata_mv_render::Renderer;
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
        template: Box<strata_mv_core::Template>,
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
    /// A template's edit lock changed (taken, renewed after a take over,
    /// released). `takenFrom` is the designer tab that lost it.
    #[serde(rename_all = "camelCase")]
    Lock {
        project_id: String,
        template_id: String,
        lock: Option<crate::locks::Lock>,
        taken_from: Option<String>,
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
    /// The tally, UMD and audio levels of the inputs (`crate::signals`).
    Signals {
        sources: BTreeMap<String, crate::signals::SignalView>,
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

/// A channel as shown to clients.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub frame_rate: strata_mv_core::FrameRate,
    /// Two fields a frame (the rate counts fields).
    pub interlaced: bool,
    pub project_id: Option<String>,
    /// What the channel goes out to: "SDI · name", "NDI · name"…
    pub outputs: Vec<String>,
    /// Still picture shown while no project is loaded ("" = none).
    pub slate: String,
    /// The design shown while no project is loaded.
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
    /// Who edits which template in the designer.
    pub locks: Arc<crate::locks::Locks>,
    /// Held from reading a project to saving it, so two edits at the same
    /// time (an autosave and a rundown change…) never undo each other.
    pub edits: tokio::sync::Mutex<()>,
    /// Whether each video output runs, by channel and output id.
    pub output_status: Arc<OutputStatus>,
    /// Live video inputs (kept across engine restarts).
    pub inputs: Arc<crate::inputs::Inputs>,
    /// Tally and UMD from the switchers (kept across engine restarts).
    pub tally: Arc<crate::tally::Tally>,
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
        // Its first multiview, on air at once.
        if let Err(e) = self.show_multiview(channel_id, None).await {
            tracing::warn!(channel = channel_id, "cannot show the multiview: {e}");
        }
        self.broadcast_channel(ch).await;
        let _ = self.events.send(Event::Channels {
            channels: self.channel_infos().await,
        });
        Ok(())
    }

    /// Puts a multiview of the channel's project on air, alone, at once
    /// (`None`: its first one published; nothing when there is none).
    pub async fn show_multiview(
        &self,
        channel_id: &str,
        template_id: Option<String>,
    ) -> Result<(), String> {
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
        let Some(id) = template_id.or_else(|| first_multiview(&project)) else {
            return Ok(());
        };
        if project.template(&id).is_none() {
            return Err("this multiview is not published".into());
        }
        ch.state.write().await.layers.clear();
        let take = Command::Take {
            zone: None,
            layer: None,
            template_id: id,
        };
        self.command(channel_id, take).await.map(|_| ())
    }

    /// The channels showing `project_id` with nothing on air take its first
    /// multiview (its first one was just published).
    pub async fn fill_empty_channels(&self, project_id: &str) {
        for (id, ch) in &self.channels {
            if ch.project_id.read().await.as_deref() != Some(project_id)
                || !ch.state.read().await.layers.is_empty()
            {
                continue;
            }
            if let Err(e) = self.show_multiview(id, None).await {
                tracing::warn!(channel = id, "cannot show the multiview: {e}");
            }
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

    /// Applies a command on air.
    pub async fn command(&self, channel_id: &str, cmd: Command) -> Result<ChannelState, String> {
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
        let snapshot = {
            let mut st = ch.state.write().await;
            st.apply(&project, cmd).map_err(|e| e.to_string())?;
            st.clone()
        };
        let _ = self.events.send(Event::Channel {
            channel_id: channel_id.into(),
            state: snapshot.clone(),
            server_time: now_ms(),
        });
        Ok(snapshot)
    }
}

/// The published page of a multiview.
pub fn first_multiview(p: &Project) -> Option<String> {
    p.templates.first().map(|t| t.id.clone())
}

/// What clients do with an `Event::ProjectTemplate`: the template replaces
/// the one with its id in the templates (or drafts), else is added last.
pub fn apply_template(p: &mut Project, draft: bool, t: strata_mv_core::Template) {
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
fn template_change(old: &Project, new: &Project) -> Option<(bool, strata_mv_core::Template)> {
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
        let old = strata_mv_core::sample::demo_project();
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
