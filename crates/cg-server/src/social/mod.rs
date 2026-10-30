//! Social networks: connections shared by every project (the Data page of
//! the main menu) bringing in the messages of a platform (live chats,
//! comments, mentions, WhatsApp / SMS sent to the newsroom).
//!
//! A connection only brings messages in. What a project does with them
//! (moderation, rules, hashtags, what goes on air) is the project's own
//! (`moderation`), and reaches its templates as a project data source
//! `social-<project id>` (see `moderation::source_value`).

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

pub mod api;
pub mod hashtags;
mod http;
mod meta;
pub mod moderation;
pub mod rules;
pub mod run;
mod twitch;
mod webhooks;
mod x;
mod youtube;

/// Messages kept per connection, for the Data page and a project that
/// starts following it.
pub const RECENT: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Platform {
    Youtube,
    Twitch,
    Facebook,
    Instagram,
    X,
    Tiktok,
    Whatsapp,
    Sms,
}

/// A connection to one account / live / number of a platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialSource {
    /// Given by the address when saved (`PUT /api/social/sources/{id}`).
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub platform: Platform,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Platform settings (channel, video, page, query, API key, token…).
    /// Secrets are never sent back to clients (see `SocialSource::public`).
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
}

fn yes() -> bool {
    true
}

/// Settings holding a secret: written by clients, never read back.
pub const SECRET_KEYS: &[&str] = &[
    "apiKey",
    "accessToken",
    "bearer",
    "appSecret",
    "authToken",
    "verifyToken",
];

impl SocialSource {
    /// As sent to clients: secrets replaced by "••••" when set.
    pub fn public(&self) -> Self {
        let mut s = self.clone();
        for (k, v) in s.settings.iter_mut() {
            if SECRET_KEYS.contains(&k.as_str()) && !v.is_empty() {
                *v = "••••".into();
            }
        }
        s
    }
}

/// Who wrote a message.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Author {
    /// Platform id of the account (or the phone number).
    pub id: String,
    /// Display name.
    pub name: String,
    /// @handle when the platform has one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub handle: String,
    /// Picture URL.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub avatar: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub verified: bool,
    /// Where they write from, when given (a form, an SMS keyword…).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub city: String,
}

/// One message from a platform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// Unique on the server: `<source id>:<platform message id>`.
    pub id: String,
    pub source_id: String,
    pub platform: Platform,
    pub author: Author,
    pub text: String,
    /// Picture URLs (WhatsApp, Instagram, SMS MMS…), downloaded to the
    /// server's media when the platform needs a token to read them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub photos: Vec<String>,
    /// Server time it arrived (ms).
    pub at: i64,
    /// Super Chat, Bits, gift… as shown ("5 €", "100 bits").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub amount: String,
    /// Link to the message on the platform.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
}

/// State of a connection, as shown on the Data page.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub connected: bool,
    /// Messages in the last minute.
    pub per_min: u32,
    pub error: Option<String>,
    /// A warning that does not stop it ("token expires in 6 days").
    pub warning: Option<String>,
    pub last_at: Option<i64>,
    /// Connected, but the account has no live running (nothing to read).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub off_air: bool,
    /// When the access token expires (ms), when the platform says.
    pub expires_at: Option<i64>,
    /// Last call of the platform / bridge to the webhook (ms).
    pub hook_at: Option<i64>,
    /// The account read, as the platform names it ("Le Figaro TV").
    pub account: Option<String>,
    /// Enabled, but not reading its platform: no project follows it (and
    /// no test runs). See `run::wanted`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub idle: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    #[serde(flatten)]
    pub source: SocialSource,
    #[serde(flatten)]
    pub status: Status,
}

/// The connections, their last messages, and the stream of new ones.
pub struct Hub {
    dir: PathBuf,
    sources: RwLock<Vec<SocialSource>>,
    status: RwLock<HashMap<String, Status>>,
    recent: RwLock<HashMap<String, VecDeque<Message>>>,
    tx: broadcast::Sender<Message>,
    /// Woken when the connections change (connectors restart), when the
    /// connections a project follows change, and when a test starts.
    pub changed: tokio::sync::Notify,
    /// Test windows (`POST …/test`): connection id → until (ms). The
    /// connector runs meanwhile, followed or not.
    pub testing: RwLock<HashMap<String, i64>>,
}

/// The connections a project follows (`<data>/social/projects/<id>.json`).
#[derive(Deserialize)]
struct Followed {
    #[serde(default)]
    sources: Vec<String>,
}

impl Hub {
    /// Connections from `<data>/social/sources.json`.
    pub fn load(data_dir: &Path) -> Self {
        let dir = data_dir.join("social");
        let sources = std::fs::read(dir.join("sources.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            dir,
            sources: RwLock::new(sources),
            status: RwLock::default(),
            recent: RwLock::default(),
            tx: broadcast::channel(4096).0,
            changed: tokio::sync::Notify::new(),
            testing: RwLock::default(),
        }
    }

    /// How many projects follow each connection, from their files.
    pub fn followers(&self) -> HashMap<String, usize> {
        let mut out = HashMap::new();
        let Ok(rd) = std::fs::read_dir(self.dir.join("projects")) else {
            return out;
        };
        for e in rd.flatten() {
            if e.path().extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let f = std::fs::read(e.path())
                .ok()
                .and_then(|b| serde_json::from_slice::<Followed>(&b).ok());
            for id in f.map(|f| f.sources).unwrap_or_default() {
                *out.entry(id).or_default() += 1;
            }
        }
        out
    }

    /// Opens a test window: the connector runs for `ms` even when no
    /// project follows it (the supervisor starts it).
    pub fn test(&self, id: &str, ms: i64) {
        let until = crate::state::now_ms() + ms;
        self.testing
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string(), until);
        self.changed.notify_waiters();
    }

    /// The connections under test now, and when the first window ends
    /// (ms). Windows over are dropped.
    pub fn testing(&self) -> (std::collections::HashSet<String>, Option<i64>) {
        let now = crate::state::now_ms();
        let mut testing = self.testing.write().unwrap_or_else(|e| e.into_inner());
        testing.retain(|_, until| *until > now);
        let next = testing.values().min().copied();
        (testing.keys().cloned().collect(), next)
    }

    /// New messages of every connection.
    pub fn subscribe(&self) -> broadcast::Receiver<Message> {
        self.tx.subscribe()
    }

    /// A connector brings a message in (once: the same id again is dropped).
    pub fn publish(&self, msg: Message) -> bool {
        {
            let mut recent = self.recent.write().unwrap_or_else(|e| e.into_inner());
            let list = recent.entry(msg.source_id.clone()).or_default();
            if list.iter().any(|m| m.id == msg.id) {
                return false;
            }
            list.push_back(msg.clone());
            while list.len() > RECENT {
                list.pop_front();
            }
        }
        {
            let mut status = self.status.write().unwrap_or_else(|e| e.into_inner());
            let st = status.entry(msg.source_id.clone()).or_default();
            st.last_at = Some(msg.at);
        }
        let _ = self.tx.send(msg);
        true
    }

    /// The last messages of a connection, oldest first.
    pub fn recent(&self, source_id: &str) -> Vec<Message> {
        self.recent
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(source_id)
            .map(|l| l.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn set_status(&self, source_id: &str, f: impl FnOnce(&mut Status)) {
        let mut status = self.status.write().unwrap_or_else(|e| e.into_inner());
        f(status.entry(source_id.to_string()).or_default());
    }

    /// A connection's status, as last set.
    pub fn status(&self, source_id: &str) -> Status {
        self.status
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(source_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn list(&self) -> Vec<SourceInfo> {
        let now = crate::state::now_ms();
        let status = self.status.read().unwrap_or_else(|e| e.into_inner());
        let recent = self.recent.read().unwrap_or_else(|e| e.into_inner());
        self.sources
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|s| {
                let mut st = status.get(&s.id).cloned().unwrap_or_default();
                st.per_min = recent
                    .get(&s.id)
                    .map(|l| l.iter().rev().take_while(|m| now - m.at < 60_000).count() as u32)
                    .unwrap_or(0);
                SourceInfo {
                    source: s.public(),
                    status: st,
                }
            })
            .collect()
    }

    /// With its secrets (connectors only).
    pub fn get(&self, id: &str) -> Option<SocialSource> {
        self.sources
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|s| s.id == id)
            .cloned()
    }

    pub fn all(&self) -> Vec<SocialSource> {
        self.sources
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Creates or replaces a connection. A secret sent back as "••••" (or
    /// left out) keeps the one already saved.
    pub fn put(&self, mut source: SocialSource) -> anyhow::Result<()> {
        let mut list = self.sources.write().unwrap_or_else(|e| e.into_inner());
        if let Some(old) = list.iter().find(|s| s.id == source.id) {
            for k in SECRET_KEYS {
                let keep = source.settings.get(*k).is_none_or(|v| v == "••••");
                if keep {
                    match old.settings.get(*k) {
                        Some(v) => source.settings.insert((*k).into(), v.clone()),
                        None => source.settings.remove(*k),
                    };
                }
            }
        }
        match list.iter_mut().find(|s| s.id == source.id) {
            Some(s) => *s = source,
            None => list.push(source),
        }
        self.save(&list)?;
        drop(list);
        self.changed.notify_waiters();
        Ok(())
    }

    pub fn delete(&self, id: &str) -> anyhow::Result<bool> {
        let mut list = self.sources.write().unwrap_or_else(|e| e.into_inner());
        let before = list.len();
        list.retain(|s| s.id != id);
        if list.len() == before {
            return Ok(false);
        }
        self.save(&list)?;
        drop(list);
        self.status
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        self.recent
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        self.testing
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        self.changed.notify_waiters();
        Ok(true)
    }

    fn save(&self, list: &[SocialSource]) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let tmp = self.dir.join("sources.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(list)?)?;
        std::fs::rename(tmp, self.dir.join("sources.json"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(source: &str, id: &str) -> Message {
        Message {
            id: format!("{source}:{id}"),
            source_id: source.into(),
            platform: Platform::Twitch,
            author: Author {
                id: "u".into(),
                name: "kevin_bzh".into(),
                ..Default::default()
            },
            text: "Super le reportage".into(),
            photos: vec![],
            at: crate::state::now_ms(),
            amount: String::new(),
            url: String::new(),
        }
    }

    #[test]
    fn a_message_comes_in_once_and_the_last_ones_are_kept() {
        let dir = std::env::temp_dir().join(format!("social-{}", uuid::Uuid::new_v4().simple()));
        let hub = Hub::load(&dir);
        let mut rx = hub.subscribe();
        assert!(hub.publish(msg("tw", "1")));
        assert!(!hub.publish(msg("tw", "1")));
        assert_eq!(rx.try_recv().unwrap().id, "tw:1");
        assert!(rx.try_recv().is_err());
        for i in 0..(RECENT + 10) {
            hub.publish(msg("tw", &format!("m{i}")));
        }
        let recent = hub.recent("tw");
        assert_eq!(recent.len(), RECENT);
        assert_eq!(recent.last().unwrap().id, format!("tw:m{}", RECENT + 9));
    }

    #[test]
    fn secrets_are_never_read_back_and_kept_when_not_retyped() {
        let dir = std::env::temp_dir().join(format!("social-{}", uuid::Uuid::new_v4().simple()));
        let hub = Hub::load(&dir);
        let mut s = SocialSource {
            id: "yt".into(),
            name: "YouTube · JT 20H".into(),
            platform: Platform::Youtube,
            enabled: true,
            settings: BTreeMap::from([
                ("apiKey".into(), "AIza-secret".into()),
                ("video".into(), "x8Kq".into()),
            ]),
        };
        hub.put(s.clone()).unwrap();
        assert_eq!(hub.list()[0].source.settings["apiKey"], "••••");
        s.settings.insert("apiKey".into(), "••••".into());
        s.settings.insert("video".into(), "other".into());
        hub.put(s).unwrap();
        let saved = hub.get("yt").unwrap();
        assert_eq!(saved.settings["apiKey"], "AIza-secret");
        assert_eq!(saved.settings["video"], "other");
        // Read again from disk.
        assert_eq!(
            Hub::load(&dir).get("yt").unwrap().settings["apiKey"],
            "AIza-secret"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
