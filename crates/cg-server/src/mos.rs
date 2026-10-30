//! MOS 2.8 device: lets a newsroom system (Saga by Fonn Group, iNews, ENPS,
//! Octopus…) drive Strata.
//!
//! - Journalists add graphics to their stories with the HTML5 MOS plugin
//!   (`/mos/plugin`): it produces MOS items whose payload names a template
//!   and its field values.
//! - The newsroom system sends its running orders on the upper port
//!   (roCreate, roReplace, roElementAction, roDelete…). Each running order
//!   becomes a playlist of the project, with one rundown item per graphic,
//!   in story order; the operator takes them from the operator page or an
//!   automation system.
//! - The lower port answers heartbeats, machine info and template lists.
//!
//! Messages are XML, UCS-2 / UTF-16 big-endian on the wire (UTF-8 accepted),
//! acknowledged on the socket they arrived on.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cg_core::{Project, RundownItem};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::config::MosConfig;
use crate::state::Shared;

/// Schema of our item payloads (also used by the HTML5 plugin).
pub const SCHEMA: &str = "https://stratagraphics.io/mos/v1";

// ------------------------------------------------------------- the model ----

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MosItem {
    pub id: String,
    pub slug: String,
    pub mos_id: String,
    pub obj_id: String,
    /// From our payload; empty for other devices' items.
    pub project: String,
    pub template: String,
    pub zone: String,
    pub data: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Story {
    pub id: String,
    pub slug: String,
    pub items: Vec<MosItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningOrder {
    pub id: String,
    pub slug: String,
    pub stories: Vec<Story>,
}

fn child<'a>(n: roxmltree::Node<'a, 'a>, name: &str) -> Option<roxmltree::Node<'a, 'a>> {
    n.children().find(|c| c.has_tag_name(name))
}

fn text(n: roxmltree::Node, name: &str) -> String {
    child(n, name)
        .and_then(|c| c.text())
        .unwrap_or("")
        .trim()
        .to_string()
}

fn parse_item(n: roxmltree::Node) -> MosItem {
    let mut item = MosItem {
        id: text(n, "itemID"),
        slug: text(n, "itemSlug"),
        mos_id: text(n, "mosID"),
        obj_id: text(n, "objID"),
        ..Default::default()
    };
    // Our payload: <strata project=… template=… zone=…><field key=…>…</field></strata>
    // (<pulsecg> in items made before the rename).
    let ours = |d: &roxmltree::Node| d.has_tag_name("strata") || d.has_tag_name("pulsecg");
    if let Some(p) = n.descendants().find(ours) {
        item.project = p.attribute("project").unwrap_or("").into();
        item.template = p.attribute("template").unwrap_or("").into();
        item.zone = p.attribute("zone").unwrap_or("").into();
        for f in p.children().filter(|c| c.has_tag_name("field")) {
            if let Some(k) = f.attribute("key") {
                item.data
                    .insert(k.into(), f.text().unwrap_or("").to_string());
            }
        }
    }
    item
}

fn parse_story(n: roxmltree::Node) -> Story {
    Story {
        id: text(n, "storyID"),
        slug: text(n, "storySlug"),
        items: n
            .children()
            .filter(|c| c.has_tag_name("item"))
            .map(parse_item)
            .collect(),
    }
}

fn parse_ro(n: roxmltree::Node) -> RunningOrder {
    RunningOrder {
        id: text(n, "roID"),
        slug: text(n, "roSlug"),
        stories: n
            .children()
            .filter(|c| c.has_tag_name("story"))
            .map(parse_story)
            .collect(),
    }
}

/// Position before `target` ("" or unknown = end).
fn index_before<T>(list: &[T], target: &str, id: impl Fn(&T) -> &str) -> usize {
    if target.is_empty() {
        return list.len();
    }
    list.iter()
        .position(|x| id(x) == target)
        .unwrap_or(list.len())
}

impl RunningOrder {
    /// roElementAction (MOS 2.8): INSERT, REPLACE, MOVE, DELETE, SWAP of
    /// stories or items.
    pub fn element_action(&mut self, n: roxmltree::Node) -> Result<(), String> {
        let op = n.attribute("operation").unwrap_or("").to_ascii_uppercase();
        let target = child(n, "element_target");
        let t_story = target.map(|t| text(t, "storyID")).unwrap_or_default();
        let t_item = target.map(|t| text(t, "itemID")).unwrap_or_default();
        let source = child(n, "element_source").ok_or("element_source missing")?;
        let src_stories: Vec<Story> = source
            .children()
            .filter(|c| c.has_tag_name("story"))
            .map(parse_story)
            .collect();
        let src_items: Vec<MosItem> = source
            .children()
            .filter(|c| c.has_tag_name("item"))
            .map(parse_item)
            .collect();
        let src_story_ids: Vec<String> = source
            .children()
            .filter(|c| c.has_tag_name("storyID"))
            .filter_map(|c| c.text())
            .map(|t| t.trim().to_string())
            .collect();
        let src_item_ids: Vec<String> = source
            .children()
            .filter(|c| c.has_tag_name("itemID"))
            .filter_map(|c| c.text())
            .map(|t| t.trim().to_string())
            .collect();
        // Item operations name their story in the target.
        let on_items = !src_items.is_empty() || !src_item_ids.is_empty();
        if on_items {
            let story = self
                .stories
                .iter_mut()
                .find(|s| s.id == t_story)
                .ok_or_else(|| format!("unknown story {t_story}"))?;
            let items = &mut story.items;
            match op.as_str() {
                "INSERT" => {
                    let at = index_before(items, &t_item, |i| &i.id);
                    items.splice(at..at, src_items);
                }
                "REPLACE" => {
                    let at = items
                        .iter()
                        .position(|i| i.id == t_item)
                        .ok_or_else(|| format!("unknown item {t_item}"))?;
                    items.splice(at..=at, src_items);
                }
                "MOVE" => {
                    let moved: Vec<MosItem> = src_item_ids
                        .iter()
                        .filter_map(|id| {
                            items
                                .iter()
                                .position(|i| &i.id == id)
                                .map(|p| items.remove(p))
                        })
                        .collect();
                    let at = index_before(items, &t_item, |i| &i.id);
                    items.splice(at..at, moved);
                }
                "DELETE" => items.retain(|i| !src_item_ids.contains(&i.id)),
                "SWAP" => {
                    let (a, b) = match src_item_ids.as_slice() {
                        [a, b] => (a, b),
                        _ => return Err("SWAP needs two items".into()),
                    };
                    let ia = items
                        .iter()
                        .position(|i| &i.id == a)
                        .ok_or("unknown item")?;
                    let ib = items
                        .iter()
                        .position(|i| &i.id == b)
                        .ok_or("unknown item")?;
                    items.swap(ia, ib);
                }
                _ => return Err(format!("unknown operation {op}")),
            }
            return Ok(());
        }
        let stories = &mut self.stories;
        match op.as_str() {
            "INSERT" => {
                let at = index_before(stories, &t_story, |s| &s.id);
                stories.splice(at..at, src_stories);
            }
            "REPLACE" => {
                let at = stories
                    .iter()
                    .position(|s| s.id == t_story)
                    .ok_or_else(|| format!("unknown story {t_story}"))?;
                stories.splice(at..=at, src_stories);
            }
            "MOVE" => {
                let moved: Vec<Story> = src_story_ids
                    .iter()
                    .filter_map(|id| {
                        stories
                            .iter()
                            .position(|s| &s.id == id)
                            .map(|p| stories.remove(p))
                    })
                    .collect();
                let at = index_before(stories, &t_story, |s| &s.id);
                stories.splice(at..at, moved);
            }
            "DELETE" => stories.retain(|s| !src_story_ids.contains(&s.id)),
            "SWAP" => {
                let (a, b) = match src_story_ids.as_slice() {
                    [a, b] => (a, b),
                    _ => return Err("SWAP needs two stories".into()),
                };
                let ia = stories
                    .iter()
                    .position(|s| &s.id == a)
                    .ok_or("unknown story")?;
                let ib = stories
                    .iter()
                    .position(|s| &s.id == b)
                    .ok_or("unknown story")?;
                stories.swap(ia, ib);
            }
            _ => return Err(format!("unknown operation {op}")),
        }
        Ok(())
    }
}

/// Playlist id of a running order. Ids that had to be cleaned get a short
/// hash of the original, so "RO.1" and "RO/1" stay two playlists.
pub fn playlist_id(ro_id: &str) -> String {
    let legacy = legacy_playlist_id(ro_id);
    if legacy == format!("mos-{ro_id}") {
        return legacy;
    }
    // FNV-1a: stable across versions and machines.
    let h = ro_id.bytes().fold(0x811c9dc5u32, |h, b| {
        (h ^ b as u32).wrapping_mul(0x01000193)
    });
    format!("{legacy}-{h:08x}")
}

/// The id older versions gave (kept to take their playlists over).
fn legacy_playlist_id(ro_id: &str) -> String {
    let safe: String = ro_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("mos-{safe}")
}

/// Writes a running order into a project: one playlist, one rundown item
/// per Strata graphic of the stories, in order. Returns the number of items.
pub fn apply_to_project(p: &mut Project, ro: &RunningOrder, mos_id: &str) -> usize {
    let pl = playlist_id(&ro.id);
    // A playlist made by an older version under the cleaned id: it is this one.
    let old = legacy_playlist_id(&ro.id);
    if old != pl && !p.playlists.iter().any(|x| x.id == pl) {
        if let Some(x) = p.playlists.iter_mut().find(|x| x.id == old) {
            x.id = pl.clone();
            for r in p.rundown.iter_mut().filter(|r| r.playlist == old) {
                r.playlist = pl.clone();
            }
        }
    }
    let name = if ro.slug.is_empty() {
        ro.id.clone()
    } else {
        ro.slug.clone()
    };
    match p.playlists.iter_mut().find(|x| x.id == pl) {
        Some(x) => x.name = name,
        None => p.playlists.push(cg_core::Playlist {
            id: pl.clone(),
            name,
            auto_next: Default::default(),
            looping: Default::default(),
        }),
    }
    // Keep the position of the running order's items in the rundown.
    let at = p
        .rundown
        .iter()
        .position(|r| r.playlist == pl)
        .unwrap_or(p.rundown.len());
    p.rundown.retain(|r| r.playlist != pl);
    let at = at.min(p.rundown.len());
    let items: Vec<RundownItem> = ro
        .stories
        .iter()
        .flat_map(|s| s.items.iter().map(move |i| (s, i)))
        .filter(|(_, i)| {
            (i.mos_id.is_empty() || i.mos_id.eq_ignore_ascii_case(mos_id))
                && p.template(&i.template).is_some()
        })
        .map(|(s, i)| RundownItem {
            read: Default::default(),
            id: format!("mos:{}:{}:{}", ro.id, s.id, i.id),
            name: if i.slug.is_empty() {
                s.slug.clone()
            } else {
                i.slug.clone()
            },
            template_id: i.template.clone(),
            zone: i.zone.clone(),
            playlist: pl.clone(),
            layer: 0,
            data: i.data.clone(),
            auto_secs: Default::default(),
            hold: Default::default(),
        })
        .collect();
    let n = items.len();
    p.rundown.splice(at..at, items);
    p.ensure_zones();
    n
}

pub fn remove_from_project(p: &mut Project, ro_id: &str) {
    let ids = [playlist_id(ro_id), legacy_playlist_id(ro_id)];
    p.rundown.retain(|r| !ids.contains(&r.playlist));
    p.playlists.retain(|x| !ids.contains(&x.id));
    p.ensure_playlists();
}

// ------------------------------------------------------------ messages ----

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn mos_time() -> String {
    jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%S")
        .to_string()
}

struct Envelope<'a> {
    cfg: &'a MosConfig,
    ncs_id: String,
    message_id: String,
}

impl Envelope<'_> {
    fn wrap(&self, body: &str) -> String {
        format!(
            "<mos><mosID>{}</mosID><ncsID>{}</ncsID><messageID>{}</messageID>{body}</mos>",
            esc(&self.cfg.mos_id),
            esc(&self.ncs_id),
            esc(&self.message_id)
        )
    }
}

fn ro_ack(ro_id: &str, status: &str) -> String {
    format!(
        "<roAck><roID>{}</roID><roStatus>{}</roStatus></roAck>",
        esc(ro_id),
        esc(status)
    )
}

fn list_mach_info() -> String {
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    let profiles: String = [
        (0, "YES"),
        (1, "YES"),
        (2, "YES"),
        (3, "NO"),
        (4, "NO"),
        (5, "NO"),
        (6, "NO"),
        (7, "NO"),
    ]
    .iter()
    .map(|(n, v)| format!("<mosProfile number=\"{n}\">{v}</mosProfile>"))
    .collect();
    format!(
        "<listMachInfo><manufacturer>Strata</manufacturer><model>Strata</model><hwRev></hwRev><swRev>{}</swRev><DOM></DOM><SN>{}</SN><ID>{}</ID><time>{}</time><mosRev>2.8.5</mosRev><supportedProfiles deviceType=\"MOS\">{profiles}</supportedProfiles></listMachInfo>",
        env!("CARGO_PKG_VERSION"),
        esc(&host),
        esc(&host),
        mos_time()
    )
}

/// Templates as MOS objects (profile 1, mosReqAll / mosReqObj).
fn mos_objects(projects: &[std::sync::Arc<Project>], mos_id: &str) -> String {
    let mut out = String::from("<mosListAll>");
    for p in projects {
        for t in &p.templates {
            out.push_str(&format!(
                "<mosObj><objID>{}/{}</objID><objSlug>{} — {}</objSlug><mosAbstract>{}</mosAbstract><objGroup>{}</objGroup><objType>STILL</objType><objTB>{}</objTB><objRev>1</objRev><objDur>{}</objDur><status>NEW</status><objAir>READY</objAir><createdBy>Strata</createdBy><created>{}</created><description>{}</description><mosExternalMetadata><mosScope>PLAYLIST</mosScope><mosSchema>{SCHEMA}</mosSchema><mosPayload><strata project=\"{}\" template=\"{}\"/></mosPayload></mosExternalMetadata></mosObj>",
                esc(&p.id),
                esc(&t.id),
                esc(&p.name),
                esc(&t.name),
                esc(&t.name),
                esc(&p.name),
                // Exact for 29.97 / 59.94: a time base of num, durations in 1/num s.
                p.frame_rate.num.max(1),
                (t.duration.max(0.0) as u64).saturating_mul(p.frame_rate.den.max(1) as u64),
                mos_time(),
                esc(&format!("Fields: {}", t.fields.iter().map(|f| f.label.as_str()).collect::<Vec<_>>().join(", "))),
                esc(&p.id),
                esc(&t.id),
            ));
        }
    }
    let _ = mos_id;
    out.push_str("</mosListAll>");
    out
}

// --------------------------------------------------------------- server ----

/// Running orders known to the device, persisted so roElementAction keeps
/// working across restarts.
pub struct Store {
    path: PathBuf,
    ros: Mutex<BTreeMap<String, (String, RunningOrder)>>,
}

impl Store {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("mos").join("running_orders.json");
        let ros = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            path,
            ros: Mutex::new(ros),
        }
    }

    fn save(&self, ros: &BTreeMap<String, (String, RunningOrder)>) {
        if let Some(d) = self.path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Err(e) = std::fs::write(&self.path, serde_json::to_vec(ros).unwrap_or_default()) {
            tracing::warn!("MOS: cannot save running orders: {e}");
        }
    }
}

/// Status shown in the settings page.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub listening: bool,
    pub error: Option<String>,
    pub connections: u32,
    pub last_message: Option<String>,
    pub last_message_at: Option<i64>,
    pub running_orders: Vec<RoSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoSummary {
    pub id: String,
    pub slug: String,
    pub project_id: String,
    pub stories: usize,
    pub items: usize,
}

static STATUS: Mutex<Option<Status>> = Mutex::new(None);

pub fn status(store: &Store) -> Status {
    let mut s = STATUS
        .lock()
        .map(|s| s.clone().unwrap_or_default())
        .unwrap_or_default();
    if let Ok(ros) = store.ros.lock() {
        s.running_orders = ros
            .values()
            .map(|(pid, ro)| RoSummary {
                id: ro.id.clone(),
                slug: ro.slug.clone(),
                project_id: pid.clone(),
                stories: ro.stories.len(),
                items: ro.stories.iter().map(|s| s.items.len()).sum(),
            })
            .collect();
    }
    s
}

fn set_status(f: impl FnOnce(&mut Status)) {
    if let Ok(mut s) = STATUS.lock() {
        f(s.get_or_insert_with(Status::default));
    }
}

/// Listens on both MOS ports until the engine cycle ends.
pub async fn serve(state: Shared) {
    let cfg = state.config.mos.clone();
    set_status(|s| *s = Status::default());
    if !cfg.enabled {
        return;
    }
    // The MOS network role: one interface, else every one.
    let role = &state.config.hardware.network.mos;
    let ip = crate::hardware::interface_ip(role).unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
    if !role.is_empty() && ip.is_unspecified() {
        tracing::warn!("MOS interface `{role}` has no IPv4 address: listening on every interface");
    }
    let mut tasks = Vec::new();
    for port in [cfg.lower_port, cfg.upper_port] {
        match TcpListener::bind((ip, port)).await {
            Ok(l) => {
                tracing::info!(port, mos_id = cfg.mos_id, "MOS device listening");
                tasks.push(tokio::spawn(accept_loop(state.clone(), l)));
            }
            Err(e) => {
                tracing::error!(port, "MOS: cannot listen: {e}");
                set_status(|s| s.error = Some(format!("cannot listen on port {port}: {e}")));
            }
        }
    }
    set_status(|s| s.listening = !tasks.is_empty());
    crate::state::until_true(state.closing.subscribe()).await;
    for t in tasks {
        t.abort();
    }
}

async fn accept_loop(state: Shared, listener: TcpListener) {
    loop {
        let Ok((sock, peer)) = listener.accept().await else {
            continue;
        };
        tracing::info!(%peer, "MOS: newsroom system connected");
        set_status(|s| s.connections += 1);
        let st = state.clone();
        tokio::spawn(async move {
            if let Err(e) = connection(st, sock).await {
                tracing::debug!(%peer, "MOS connection closed: {e}");
            }
            set_status(|s| s.connections = s.connections.saturating_sub(1));
        });
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Encoding {
    Utf16Be,
    Utf16Le,
    Utf8,
}

fn detect(bytes: &[u8]) -> Option<Encoding> {
    match bytes {
        [0xfe, 0xff, ..] | [0, b'<', ..] => Some(Encoding::Utf16Be),
        [0xff, 0xfe, ..] | [b'<', 0, ..] => Some(Encoding::Utf16Le),
        [b'<', _, ..] | [0xef, 0xbb, ..] => Some(Encoding::Utf8),
        _ if bytes.len() < 2 => None,
        _ => Some(Encoding::Utf8),
    }
}

fn encode(s: &str, enc: Encoding) -> Vec<u8> {
    match enc {
        Encoding::Utf8 => s.as_bytes().to_vec(),
        Encoding::Utf16Be => s.encode_utf16().flat_map(|u| u.to_be_bytes()).collect(),
        Encoding::Utf16Le => s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect(),
    }
}

/// Splits complete `<mos>…</mos>` messages out of `buf`.
fn take_messages(buf: &mut Vec<u8>, enc: Encoding) -> Vec<String> {
    let end = encode("</mos>", enc);
    let mut out = Vec::new();
    loop {
        // UTF-16: only look at whole code units.
        let pos = buf
            .windows(end.len())
            .enumerate()
            .find(|(i, w)| *w == end.as_slice() && (enc == Encoding::Utf8 || i % 2 == 0))
            .map(|(i, _)| i);
        let Some(i) = pos else { break };
        let chunk: Vec<u8> = buf.drain(..i + end.len()).collect();
        let text = match enc {
            Encoding::Utf8 => String::from_utf8_lossy(&chunk).into_owned(),
            Encoding::Utf16Be | Encoding::Utf16Le => {
                let units: Vec<u16> = chunk
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| {
                        if enc == Encoding::Utf16Be {
                            u16::from_be_bytes(*c)
                        } else {
                            u16::from_le_bytes(*c)
                        }
                    })
                    .collect();
                String::from_utf16_lossy(&units)
            }
        };
        let text = text.trim_start_matches('\u{feff}');
        // Anything before <mos> (whitespace, XML declaration) is dropped.
        if let Some(start) = text.find("<mos>").or_else(|| text.find("<mos ")) {
            out.push(text[start..].to_string());
        }
    }
    out
}

async fn connection(state: Shared, mut sock: TcpStream) -> std::io::Result<()> {
    let mut buf = Vec::new();
    let mut enc = None;
    let mut tmp = [0u8; 16 * 1024];
    loop {
        let n = sock.read(&mut tmp).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
        if enc.is_none() {
            enc = detect(&buf);
        }
        let Some(e) = enc else { continue };
        for msg in take_messages(&mut buf, e) {
            let reply = handle(&state, &msg).await;
            if let Some(r) = reply {
                sock.write_all(&encode(&r, e)).await?;
            }
        }
        if buf.len() > 32 * 1024 * 1024 {
            return Err(std::io::Error::other("message too large"));
        }
    }
}

/// Handles one message; returns the reply to send on the same socket.
async fn handle(state: &Shared, xml: &str) -> Option<String> {
    let cfg = &state.config.mos;
    let doc = match roxmltree::Document::parse(xml) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("MOS: invalid XML: {e}");
            return None;
        }
    };
    let root = doc.root_element();
    let env = Envelope {
        cfg,
        ncs_id: text(root, "ncsID"),
        message_id: text(root, "messageID"),
    };
    let cmd = root.children().find(|c| {
        c.is_element() && !matches!(c.tag_name().name(), "mosID" | "ncsID" | "messageID")
    })?;
    let name = cmd.tag_name().name().to_string();
    tracing::debug!(command = name, "MOS message");
    set_status(|s| {
        s.last_message = Some(name.clone());
        s.last_message_at = Some(crate::state::now_ms());
    });
    let ro_id = text(cmd, "roID");
    let body = match name.as_str() {
        "heartbeat" => format!("<heartbeat><time>{}</time></heartbeat>", mos_time()),
        "reqMachInfo" => list_mach_info(),
        "mosReqAll" | "mosReqObj" | "mosReqObjList" => {
            let projects: Vec<_> = state.library.read().await.list().cloned().collect();
            mos_objects(&projects, &cfg.mos_id)
        }
        "roCreate" | "roReplace" => {
            let ro = parse_ro(cmd);
            let id = ro.id.clone();
            ro_ack(&id, &store_ro(state, ro).await)
        }
        "roMetadataReplace" => {
            let slug = text(cmd, "roSlug");
            ro_ack(&ro_id, &update_ro(state, &ro_id, |ro| ro.slug = slug).await)
        }
        "roElementAction" => {
            let mut err = None;
            let status = update_ro(state, &ro_id, |ro| {
                if let Err(e) = ro.element_action(cmd) {
                    err = Some(e);
                }
            })
            .await;
            ro_ack(&ro_id, err.as_deref().unwrap_or(&status))
        }
        "roDelete" => ro_ack(&ro_id, &delete_ro(state, &ro_id).await),
        // Accepted, nothing to do for a graphics device.
        "roReadyToAir" | "roReq" | "roReqAll" | "roStorySend" | "roItemCue" | "roCtrl" => {
            ro_ack(&ro_id, "OK")
        }
        "mosAck" | "roAck" => return None,
        other => {
            tracing::info!(command = other, "MOS: command not supported");
            if ro_id.is_empty() {
                format!("<mosAck><objID></objID><objRev>0</objRev><status>NACK</status><statusDescription>{} not supported</statusDescription></mosAck>", esc(other))
            } else {
                ro_ack(&ro_id, &format!("{other} not supported"))
            }
        }
    };
    Some(env.wrap(&body))
}

/// Project receiving a running order: configured, else the project of its
/// first Strata item, else the first project.
async fn target_project(state: &Shared, ro: &RunningOrder) -> Option<String> {
    let lib = state.library.read().await;
    let cfg = &state.config.mos;
    if !cfg.project_id.is_empty() && lib.get(&cfg.project_id).is_some() {
        return Some(cfg.project_id.clone());
    }
    ro.stories
        .iter()
        .flat_map(|s| &s.items)
        .find(|i| !i.project.is_empty() && lib.get(&i.project).is_some())
        .map(|i| i.project.clone())
        .or_else(|| lib.first_id())
}

async fn write_ro(state: &Shared, project_id: &str, ro: &RunningOrder) -> Result<usize, String> {
    let _edit = state.edits.lock().await;
    let mut p = (*state.project(project_id).await.ok_or("no such project")?).clone();
    let n = apply_to_project(&mut p, ro, &state.config.mos.mos_id);
    state.save_project(p).await.map_err(|e| e.to_string())?;
    Ok(n)
}

async fn store_ro(state: &Shared, ro: RunningOrder) -> String {
    let Some(pid) = target_project(state, &ro).await else {
        return "no project to receive the running order".into();
    };
    match write_ro(state, &pid, &ro).await {
        Ok(n) => {
            tracing::info!(
                ro = ro.id,
                items = n,
                project = pid,
                "MOS running order received"
            );
            let mut ros = state.mos.ros.lock().unwrap_or_else(|e| e.into_inner());
            ros.insert(ro.id.clone(), (pid, ro));
            state.mos.save(&ros);
            "OK".into()
        }
        Err(e) => e,
    }
}

async fn update_ro(state: &Shared, ro_id: &str, f: impl FnOnce(&mut RunningOrder)) -> String {
    let entry = state
        .mos
        .ros
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(ro_id)
        .cloned();
    let Some((pid, mut ro)) = entry else {
        return format!("unknown running order {ro_id}");
    };
    f(&mut ro);
    match write_ro(state, &pid, &ro).await {
        Ok(_) => {
            let mut ros = state.mos.ros.lock().unwrap_or_else(|e| e.into_inner());
            ros.insert(ro.id.clone(), (pid, ro));
            state.mos.save(&ros);
            "OK".into()
        }
        Err(e) => e,
    }
}

async fn delete_ro(state: &Shared, ro_id: &str) -> String {
    let entry = state
        .mos
        .ros
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(ro_id);
    if let Some((pid, _)) = entry {
        let edit = state.edits.lock().await;
        if let Some(p) = state.project(&pid).await {
            let mut p = (*p).clone();
            remove_from_project(&mut p, ro_id);
            if let Err(e) = state.save_project(p).await {
                return e.to_string();
            }
        }
        drop(edit);
        let ros = state.mos.ros.lock().unwrap_or_else(|e| e.into_inner());
        state.mos.save(&ros);
    }
    "OK".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_orders_never_share_a_playlist() {
        assert_eq!(playlist_id("RO-1"), "mos-RO-1");
        let (a, b) = (playlist_id("RO.1"), playlist_id("RO/1"));
        assert_ne!(a, b);
        assert!(a.starts_with("mos-RO-1-"));
        assert_eq!(a, playlist_id("RO.1"), "stable");
    }

    const RO: &str = r#"<mos><mosID>strata.mos</mosID><ncsID>SAGA</ncsID><messageID>12</messageID>
<roCreate><roID>RO-1</roID><roSlug>Evening news</roSlug>
 <story><storyID>S1</storyID><storySlug>Opening</storySlug>
  <item><itemID>1</itemID><itemSlug>LT Ada</itemSlug><objID>x</objID><mosID>strata.mos</mosID>
   <mosExternalMetadata><mosScope>PLAYLIST</mosScope><mosSchema>https://stratagraphics.io/mos/v1</mosSchema>
   <mosPayload><strata project="demo" template="lower-third" zone="lower-third"><field key="name">Ada &amp; co</field><field key="title">Mathematician</field></strata></mosPayload></mosExternalMetadata></item>
  <item><itemID>2</itemID><itemSlug>Video</itemSlug><objID>v</objID><mosID>video.server</mosID></item>
 </story>
 <story><storyID>S2</storyID><storySlug>Sport</storySlug>
  <item><itemID>1</itemID><itemSlug>Score</itemSlug><objID>y</objID><mosID>strata.mos</mosID><mosExternalMetadata><mosPayload><pulsecg project="demo" template="score-bug"/></mosPayload></mosExternalMetadata></item>
 </story>
</roCreate></mos>"#;

    #[test]
    fn running_order_to_playlist() {
        let doc = roxmltree::Document::parse(RO).unwrap();
        let ro = parse_ro(
            doc.root_element()
                .children()
                .find(|c| c.has_tag_name("roCreate"))
                .unwrap(),
        );
        assert_eq!((ro.slug.as_str(), ro.stories.len()), ("Evening news", 2));
        let mut p = cg_core::sample::demo_project();
        assert_eq!(apply_to_project(&mut p, &ro, "STRATA.MOS"), 2);
        let items: Vec<_> = p
            .rundown
            .iter()
            .filter(|r| r.playlist == "mos-RO-1")
            .collect();
        assert_eq!(items[0].data["name"], "Ada & co");
        assert_eq!(items[1].template_id, "score-bug");
        assert!(p
            .playlists
            .iter()
            .any(|x| x.id == "mos-RO-1" && x.name == "Evening news"));

        // Move the sport story first, delete the lower third.
        let mut ro = ro;
        let act = |xml: &str, ro: &mut RunningOrder| {
            let d = roxmltree::Document::parse(xml).unwrap();
            ro.element_action(d.root_element()).unwrap();
        };
        act(
            r#"<roElementAction operation="MOVE"><roID>RO-1</roID><element_target><storyID>S1</storyID></element_target><element_source><storyID>S2</storyID></element_source></roElementAction>"#,
            &mut ro,
        );
        assert_eq!(ro.stories[0].id, "S2");
        act(
            r#"<roElementAction operation="DELETE"><roID>RO-1</roID><element_target><storyID>S1</storyID></element_target><element_source><itemID>1</itemID></element_source></roElementAction>"#,
            &mut ro,
        );
        act(
            r#"<roElementAction operation="INSERT"><roID>RO-1</roID><element_target><storyID></storyID></element_target><element_source><story><storyID>S3</storyID><storySlug>End</storySlug></story></element_source></roElementAction>"#,
            &mut ro,
        );
        assert_eq!(
            ro.stories.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["S2", "S1", "S3"]
        );
        assert_eq!(apply_to_project(&mut p, &ro, "strata.mos"), 1);
        remove_from_project(&mut p, "RO-1");
        assert!(!p.rundown.iter().any(|r| r.playlist == "mos-RO-1"));
    }

    #[test]
    fn frames_utf16_messages() {
        let msg = "<mos><mosID>a</mosID><heartbeat/></mos>";
        let mut buf = vec![0xfe, 0xff];
        buf.extend(encode(msg, Encoding::Utf16Be));
        buf.extend(encode(msg, Encoding::Utf16Be));
        let half = buf.len() - 7;
        let mut acc = buf[..half].to_vec();
        assert_eq!(detect(&acc), Some(Encoding::Utf16Be));
        assert_eq!(
            take_messages(&mut acc, Encoding::Utf16Be),
            vec![msg.to_string()]
        );
        acc.extend_from_slice(&buf[half..]);
        assert_eq!(
            take_messages(&mut acc, Encoding::Utf16Be),
            vec![msg.to_string()]
        );
        assert!(acc.is_empty());
        let mut u8buf = msg.as_bytes().to_vec();
        assert_eq!(
            take_messages(&mut u8buf, detect(msg.as_bytes()).unwrap()).len(),
            1
        );
    }
}
