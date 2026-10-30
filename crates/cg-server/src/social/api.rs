//! `/api/social/…`: the connections (main menu › Data), their messages, and
//! the webhooks the platforms call.

use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::{webhooks, Message, Platform, SocialSource, SourceInfo};
use crate::api::{bad, internal, not_found, require, ApiResult};
use crate::auth::CurrentUser;
use crate::library::valid_id;
use crate::state::{now_ms, Shared};

pub fn router() -> Router<Shared> {
    Router::new()
        .route("/api/social/sources", get(list))
        .route(
            "/api/social/sources/{id}",
            put(put_source).delete(delete_source),
        )
        .route("/api/social/sources/{id}/messages", get(messages))
        .route("/api/social/sources/{id}/test", post(test_source))
        .route(
            "/api/social/hook/{id}",
            get(webhooks::hook_get).post(webhooks::hook_post),
        )
}

/// A connection as listed: with its status and the projects following it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listed {
    #[serde(flatten)]
    pub info: SourceInfo,
    pub projects: usize,
}

fn listed(s: &Shared) -> Vec<Listed> {
    let follow = s.social.followers();
    s.social
        .list()
        .into_iter()
        .map(|info| Listed {
            projects: follow.get(&info.source.id).copied().unwrap_or(0),
            info,
        })
        .collect()
}

/// Any signed-in user.
async fn list(State(s): State<Shared>, CurrentUser(_): CurrentUser) -> Json<Vec<Listed>> {
    Json(listed(&s))
}

/// Calls the platform makes (`hookToken`: the token of its address).
fn has_hook(p: Platform) -> bool {
    matches!(p, Platform::Tiktok | Platform::Whatsapp | Platform::Sms)
}

/// Creates or replaces a connection (those who create projects).
async fn put_source(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Json(mut src): Json<SocialSource>,
) -> ApiResult<Json<Listed>> {
    require(me.can_create_projects())?;
    if !valid_id(&id) {
        return Err(bad("invalid id"));
    }
    src.id = id.clone();
    src.name = src.name.trim().to_string();
    if src.name.is_empty() {
        return Err(bad("a connection needs a name"));
    }
    src.settings.retain(|_, v| !v.trim().is_empty());
    if has_hook(src.platform) && !src.settings.contains_key("hookToken") {
        let old = s
            .social
            .get(&id)
            .and_then(|o| o.settings.get("hookToken").cloned());
        let token = old.unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
        src.settings.insert("hookToken".into(), token);
    }
    s.social.put(src).map_err(internal)?;
    s.changed("social");
    listed(&s)
        .into_iter()
        .find(|l| l.info.source.id == id)
        .map(Json)
        .ok_or_else(|| internal("lost"))
}

async fn delete_source(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    require(me.can_create_projects())?;
    if !s.social.delete(&id).map_err(internal)? {
        return Err(not_found("connection"));
    }
    s.changed("social");
    Ok(StatusCode::NO_CONTENT)
}

/// How long a test keeps the connector running, and how long the call
/// waits for its outcome.
const TEST_WINDOW_MS: i64 = 20_000;
const TEST_WAIT: Duration = Duration::from_secs(8);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    /// The platform answered (connected, or a message came in).
    pub ok: bool,
    /// The connection as the list gives it, once the test is over.
    pub state: Listed,
    /// Its 5 latest messages, newest first.
    pub messages: Vec<Message>,
    pub took_ms: i64,
}

/// Reads the platform now, followed or not (those who create projects).
/// The connector runs for `TEST_WINDOW_MS`; the call answers as soon as it
/// is connected, fails, or brings a message in, or after `TEST_WAIT`. A
/// webhook connection (tiktok, whatsapp, sms) cannot be fetched: its state
/// and last messages received are given at once.
async fn test_source(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<TestResult>> {
    require(me.can_create_projects())?;
    let Some(src) = s.social.get(&id) else {
        return Err(not_found("connection"));
    };
    if !src.enabled {
        return Err(bad("the connection is not enabled"));
    }
    let start = now_ms();
    let mut ok = has_hook(src.platform);
    if !ok {
        s.social.test(&id, TEST_WINDOW_MS);
        let deadline = tokio::time::Instant::now() + TEST_WAIT;
        loop {
            let st = s.social.status(&id);
            let got = s.social.recent(&id).last().is_some_and(|m| m.at >= start);
            if st.error.is_some() || st.connected || st.off_air || got {
                ok = st.error.is_none() && (st.connected || got);
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
    let state = listed(&s)
        .into_iter()
        .find(|l| l.info.source.id == id)
        .ok_or_else(|| not_found("connection"))?;
    let mut messages = s.social.recent(&id);
    messages.reverse();
    messages.truncate(5);
    Ok(Json(TestResult {
        ok,
        state,
        messages,
        took_ms: now_ms() - start,
    }))
}

#[derive(Deserialize)]
struct MessagesQuery {
    limit: Option<usize>,
}

/// The last messages of a connection, newest first.
async fn messages(
    State(s): State<Shared>,
    CurrentUser(_): CurrentUser,
    Path(id): Path<String>,
    Query(q): Query<MessagesQuery>,
) -> ApiResult<Json<Vec<Message>>> {
    if s.social.get(&id).is_none() {
        return Err(not_found("connection"));
    }
    let mut list = s.social.recent(&id);
    list.reverse();
    list.truncate(q.limit.unwrap_or(100).min(super::RECENT));
    Ok(Json(list))
}
