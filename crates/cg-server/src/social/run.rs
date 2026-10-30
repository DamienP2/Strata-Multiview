//! Runs the connections: one task per connection wanted (enabled, and
//! followed by a project or under test), restarted when it changes,
//! reconnecting with a growing delay when the platform fails. A connection
//! nobody follows stays idle: the platform is not polled (quota).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::task::{AbortHandle, JoinSet};

use super::{Author, Hub, Message, Platform, SocialSource, Status};
use crate::state::{now_ms, Shared};

/// The connections that read their platform: the enabled ones that at
/// least one project follows (`followers`: projects per connection id), or
/// that a test runs on (`testing`).
pub fn wanted(
    all: &[SocialSource],
    followers: &HashMap<String, usize>,
    testing: &HashSet<String>,
) -> Vec<SocialSource> {
    all.iter()
        .filter(|s| {
            s.enabled && (followers.get(&s.id).is_some_and(|n| *n > 0) || testing.contains(&s.id))
        })
        .cloned()
        .collect()
}

pub async fn run(shared: Shared) {
    supervise(shared.social.clone()).await
}

async fn supervise(hub: Arc<Hub>) {
    // Dropped with this task (engine restart): every connector stops.
    let mut set = JoinSet::new();
    let mut tasks: HashMap<String, (String, AbortHandle)> = HashMap::new();
    loop {
        let changed = hub.changed.notified();
        tokio::pin!(changed);
        changed.as_mut().enable();
        while set.try_join_next().is_some() {}
        let all = hub.all();
        let followers = hub.followers();
        let (testing, next_expiry) = hub.testing();
        let want = wanted(&all, &followers, &testing);
        tasks.retain(|id, (print, task)| {
            let keep = want.iter().any(|s| &s.id == id && &fingerprint(s) == print);
            if !keep {
                task.abort();
                if hub.get(id).is_some() {
                    hub.set_status(id, |st| {
                        *st = Status {
                            last_at: st.last_at,
                            ..Default::default()
                        }
                    });
                }
            }
            keep
        });
        for s in &all {
            let running = want.iter().any(|w| w.id == s.id);
            let idle = s.enabled && !running;
            if hub.status(&s.id).idle != idle {
                hub.set_status(&s.id, |st| st.idle = idle);
            }
        }
        for s in want {
            if !tasks.contains_key(&s.id) {
                let print = fingerprint(&s);
                let id = s.id.clone();
                let task = set.spawn(keep(hub.clone(), s));
                tasks.insert(id, (print, task));
            }
        }
        // Until something changes, or the first test window ends.
        match next_expiry {
            Some(at) => {
                let wait = Duration::from_millis((at - now_ms()).max(0) as u64);
                tokio::select! {
                    _ = changed => {}
                    _ = tokio::time::sleep(wait) => {}
                }
            }
            None => changed.await,
        }
    }
}

fn fingerprint(s: &SocialSource) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

/// Reads a connection's platform for good, reconnecting after a failure.
async fn keep(hub: Arc<Hub>, src: SocialSource) {
    let mut backoff = 2;
    loop {
        let started = Instant::now();
        let r = match src.platform {
            Platform::Twitch => super::twitch::run(&hub, &src).await,
            Platform::Youtube => super::youtube::run(&hub, &src).await,
            Platform::Facebook => super::meta::facebook(&hub, &src).await,
            Platform::Instagram => super::meta::instagram(&hub, &src).await,
            Platform::X => super::x::run(&hub, &src).await,
            Platform::Tiktok | Platform::Whatsapp | Platform::Sms => {
                super::webhooks::watch(&hub, &src).await
            }
        };
        if started.elapsed() > Duration::from_secs(120) {
            backoff = 2;
        }
        match r {
            // Asked to reconnect (Twitch RECONNECT).
            Ok(()) => tokio::time::sleep(Duration::from_secs(1)).await,
            Err(e) => {
                tracing::debug!(source = %src.id, "social: {e}");
                hub.set_status(&src.id, |st| {
                    st.connected = false;
                    st.off_air = false;
                    st.error = Some(e);
                });
                tokio::time::sleep(Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(120);
            }
        }
    }
}

/// A setting, trimmed ("" when not set).
pub(super) fn setting<'a>(src: &'a SocialSource, key: &str) -> &'a str {
    src.settings.get(key).map_or("", |v| v.trim())
}

/// Reading works (`off_air`: but no live is running).
pub(super) fn connected(hub: &Hub, src: &SocialSource, off_air: bool) {
    hub.set_status(&src.id, |st| {
        st.connected = true;
        st.off_air = off_air;
        st.error = None;
    });
}

/// A message of the connection, arrived now.
pub(super) fn message(src: &SocialSource, id: &str, author: Author, text: &str) -> Message {
    Message {
        id: format!("{}:{id}", src.id),
        source_id: src.id.clone(),
        platform: src.platform,
        author,
        text: text.trim().to_string(),
        photos: vec![],
        at: now_ms(),
        amount: String::new(),
        url: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn source(id: &str, platform: Platform, enabled: bool) -> SocialSource {
        SocialSource {
            id: id.into(),
            name: id.into(),
            platform,
            enabled,
            settings: BTreeMap::from([("channel".into(), "lefigaro_live".into())]),
        }
    }

    /// `<data>/social/projects/<project>.json` following `sources`.
    fn follow(dir: &std::path::Path, project: &str, sources: &[&str]) {
        let projects = dir.join("social").join("projects");
        std::fs::create_dir_all(&projects).unwrap();
        std::fs::write(
            projects.join(format!("{project}.json")),
            serde_json::json!({ "sources": sources, "rules": {} }).to_string(),
        )
        .unwrap();
    }

    #[test]
    fn a_connection_is_wanted_when_enabled_and_followed_or_under_test() {
        let all = [
            source("followed", Platform::Twitch, true),
            source("tested", Platform::Twitch, true),
            source("alone", Platform::Twitch, true),
            source("off", Platform::Twitch, false),
        ];
        let followers = HashMap::from([("followed".to_string(), 2), ("off".to_string(), 1)]);
        let testing = HashSet::from(["tested".to_string()]);
        let ids: Vec<String> = wanted(&all, &followers, &testing)
            .into_iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(ids, ["followed", "tested"]);
        // Enabled, but nobody follows it and no test runs: not wanted.
        assert!(wanted(&all, &HashMap::new(), &HashSet::new()).is_empty());
        // Followed, but disabled: not wanted.
        let followers = HashMap::from([("off".to_string(), 1)]);
        assert!(wanted(&all, &followers, &HashSet::new()).is_empty());
    }

    #[tokio::test]
    async fn only_followed_or_tested_connections_are_started_and_a_change_restarts_them() {
        let dir = std::env::temp_dir().join(format!("social-{}", uuid::Uuid::new_v4().simple()));
        let hub = Arc::new(Hub::load(&dir));
        let wa = |id: &str, enabled| source(id, Platform::Whatsapp, enabled);
        hub.put(wa("on", true)).unwrap();
        hub.put(wa("alone", true)).unwrap();
        hub.put(wa("off", false)).unwrap();
        follow(&dir, "jt", &["on", "off"]);
        let task = tokio::spawn(supervise(hub.clone()));
        let status = |id: &str| {
            hub.list()
                .into_iter()
                .find(|s| s.source.id == id)
                .unwrap()
                .status
        };
        tokio::time::sleep(Duration::from_millis(200)).await;
        // Followed and enabled: runs. Enabled alone: idle. Disabled: nothing.
        assert!(status("on").connected && !status("on").idle);
        assert!(!status("alone").connected && status("alone").idle);
        assert!(!status("off").connected && !status("off").idle);
        // The project follows the other one (the lead notifies `changed`).
        follow(&dir, "jt", &["alone"]);
        hub.changed.notify_waiters();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!status("on").connected && status("on").idle);
        assert!(status("alone").connected && !status("alone").idle);
        // Enabling a followed one starts it; disabling a running one stops it.
        hub.put(wa("alone", false)).unwrap();
        follow(&dir, "jt", &["alone", "off"]);
        hub.put(wa("off", true)).unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!status("alone").connected && !status("alone").idle);
        assert!(status("off").connected);
        // A test window starts an idle connection, and its end stops it.
        hub.test("on", 400);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(status("on").connected && !status("on").idle);
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(!status("on").connected && status("on").idle);
        assert!(status("off").connected);
        task.abort();
        let _ = std::fs::remove_dir_all(dir);
    }
}
