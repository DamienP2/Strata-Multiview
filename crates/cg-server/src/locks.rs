//! Edit locks: one designer at a time per template.
//!
//! The designer that opens a template takes its lock and renews it while it
//! stays open (a lock not renewed for [`TTL`] is free again). Someone else
//! sees the template read only, and can take it over: the previous holder
//! is told who did and switches to read only.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use serde::Serialize;

use crate::auth::User;
use crate::state::now_ms;

/// A lock not renewed for this long is released.
pub const TTL: Duration = Duration::from_secs(45);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lock {
    pub user_id: String,
    pub user_name: String,
    /// Browser tab holding it.
    pub client: String,
    /// Unix ms.
    pub since: i64,
    #[serde(skip)]
    seen: Option<Instant>,
}

#[derive(Default)]
pub struct Locks {
    map: Mutex<HashMap<(String, String), Lock>>,
}

pub enum Acquired {
    /// The lock is this client's (`taken_from`: the client it was taken from).
    Granted {
        lock: Lock,
        taken_from: Option<Lock>,
    },
    /// Someone else holds it.
    Held(Lock),
}

impl Locks {
    fn live(map: &mut HashMap<(String, String), Lock>, key: &(String, String)) -> Option<Lock> {
        let expired = map
            .get(key)
            .is_some_and(|l| l.seen.is_none_or(|s| s.elapsed() > TTL));
        if expired {
            map.remove(key);
        }
        map.get(key).cloned()
    }

    /// Takes or renews the lock; `force` takes it over from someone else.
    pub fn acquire(
        &self,
        project: &str,
        template: &str,
        me: &User,
        client: &str,
        force: bool,
    ) -> Acquired {
        let key = (project.to_string(), template.to_string());
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        let current = Self::live(&mut map, &key);
        if let Some(l) = &current {
            if l.client != client && !force {
                return Acquired::Held(l.clone());
            }
        }
        let taken_from = current.clone().filter(|l| l.client != client);
        let since = current
            .as_ref()
            .filter(|l| l.client == client)
            .map_or(now_ms(), |l| l.since);
        let lock = Lock {
            user_id: me.id.clone(),
            user_name: if me.display_name.trim().is_empty() {
                me.username.clone()
            } else {
                me.display_name.clone()
            },
            client: client.to_string(),
            since,
            seen: Some(Instant::now()),
        };
        map.insert(key, lock.clone());
        Acquired::Granted { lock, taken_from }
    }

    /// Releases the lock if this client holds it. Returns true if released.
    pub fn release(&self, project: &str, template: &str, client: &str) -> bool {
        let key = (project.to_string(), template.to_string());
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        if Self::live(&mut map, &key).is_some_and(|l| l.client == client) {
            map.remove(&key);
            return true;
        }
        false
    }

    /// Refuses a save from a designer tab that does not hold the lock.
    /// Requests without a client id (automation, API keys) are not locked out.
    pub fn check(
        &self,
        project: &str,
        template: &str,
        _me: &User,
        client: Option<&str>,
    ) -> Result<(), (StatusCode, String)> {
        let Some(client) = client else { return Ok(()) };
        let key = (project.to_string(), template.to_string());
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        match Self::live(&mut map, &key) {
            Some(l) if l.client != client => Err((
                StatusCode::CONFLICT,
                format!("{} is editing this template", l.user_name),
            )),
            _ => Ok(()),
        }
    }

    /// Current locks of a project, by template id.
    pub fn of_project(&self, project: &str) -> HashMap<String, Lock> {
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        let keys: Vec<_> = map.keys().filter(|(p, _)| p == project).cloned().collect();
        keys.into_iter()
            .filter_map(|k| Self::live(&mut map, &k).map(|l| (k.1, l)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(id: &str) -> User {
        serde_json::from_value(serde_json::json!({
            "id": id, "username": id, "displayName": id.to_uppercase(), "role": "designer", "passwordHash": ""
        }))
        .unwrap()
    }

    #[test]
    fn take_over() {
        let l = Locks::default();
        let (a, b) = (user("lea"), user("max"));
        assert!(matches!(
            l.acquire("p", "t", &a, "tab-a", false),
            Acquired::Granted {
                taken_from: None,
                ..
            }
        ));
        assert!(l.check("p", "t", &a, Some("tab-a")).is_ok());
        assert!(
            matches!(l.acquire("p", "t", &b, "tab-b", false), Acquired::Held(ref h) if h.user_name == "LEA")
        );
        assert!(l.check("p", "t", &b, Some("tab-b")).is_err());
        assert!(l.check("p", "t", &b, None).is_ok());
        match l.acquire("p", "t", &b, "tab-b", true) {
            Acquired::Granted {
                taken_from: Some(prev),
                ..
            } => assert_eq!(prev.client, "tab-a"),
            _ => panic!("take over"),
        }
        assert!(l.check("p", "t", &a, Some("tab-a")).is_err());
        assert!(!l.release("p", "t", "tab-a"));
        assert!(l.release("p", "t", "tab-b"));
        assert!(l.of_project("p").is_empty());
    }
}
