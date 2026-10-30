//! The tally's API: its settings and state (the Tally page), an input's
//! tally and UMD set directly (automation), and the contacts of GPI boxes
//! (`GET /api/tally/gpi/<source>/<pin>/on?key=smv_…`: a URL a GPI box calls).

use std::collections::BTreeMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::Serialize;

use super::{Override, SourceState, TallyConfig};
use crate::api::{bad, require, ApiResult};
use crate::auth::{CurrentUser, Role, User};
use crate::state::Shared;

pub fn router() -> Router<Shared> {
    Router::new()
        .route("/api/tally", get(get_tally))
        .route("/api/tally/config", put(put_config))
        .route(
            "/api/tally/inputs/{input}",
            put(put_input).delete(delete_input),
        )
        .route("/api/tally/gpi/{source}/{pin}/{state}", get(gpi).post(gpi))
}

/// Those who play the outputs set the tally (operators and up).
fn may_set(me: &User) -> bool {
    matches!(me.role, Role::Admin | Role::Designer | Role::Operator)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TallyView {
    config: TallyConfig,
    /// How each source is doing, and its displays.
    sources: BTreeMap<String, SourceState>,
    /// The tally of the inputs now.
    inputs: BTreeMap<String, strata_mv_core::signal::Tally>,
    /// Set through the API.
    overrides: BTreeMap<String, Override>,
}

async fn get_tally(State(s): State<Shared>, CurrentUser(_me): CurrentUser) -> Json<TallyView> {
    Json(TallyView {
        config: s.tally.config(),
        sources: s.tally.states(),
        inputs: s.tally.resolved(),
        overrides: s.tally.overrides(),
    })
}

async fn put_config(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Json(c): Json<TallyConfig>,
) -> ApiResult<StatusCode> {
    require(me.is_admin())?;
    s.tally.set_config(c).map_err(bad)?;
    s.changed("tally");
    Ok(StatusCode::NO_CONTENT)
}

/// Sets an input's tally and UMD (fields left out follow its routes).
async fn put_input(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(input): Path<String>,
    Json(o): Json<Override>,
) -> ApiResult<StatusCode> {
    require(may_set(&me))?;
    s.tally.set_override(&input, Some(o));
    Ok(StatusCode::NO_CONTENT)
}

/// The input's tally follows its routes again.
async fn delete_input(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(input): Path<String>,
) -> ApiResult<StatusCode> {
    require(may_set(&me))?;
    s.tally.set_override(&input, None);
    Ok(StatusCode::NO_CONTENT)
}

/// A contact of a GPI box: `on` / `off` (also `1` / `0`, `close` / `open`).
async fn gpi(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path((source, pin, state)): Path<(String, u32, String)>,
) -> ApiResult<StatusCode> {
    require(may_set(&me))?;
    let on = match state.to_ascii_lowercase().as_str() {
        "on" | "1" | "close" | "closed" => true,
        "off" | "0" | "open" => false,
        _ => return Err(bad("the state is on or off")),
    };
    s.tally.gpi(&source, pin, on).map_err(bad)?;
    Ok(StatusCode::NO_CONTENT)
}
