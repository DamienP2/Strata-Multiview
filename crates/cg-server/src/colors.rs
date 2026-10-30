//! Library › Colours: the palettes shared by every project, and each
//! project's own. A layer keeps the value of the colour it took from a
//! palette; when the palette changes the colour, the layers using it follow
//! (`cg_core::palette`).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use cg_core::{NamedColor, Palette};
use serde::Deserialize;

use crate::api::{bad, internal, require, visible_project, ApiResult};
use crate::auth::CurrentUser;
use crate::state::Shared;

fn file(s: &Shared) -> std::path::PathBuf {
    s.data_dir.join("colors.json")
}

/// The shared palettes (none at first).
pub async fn load(s: &Shared) -> Vec<Palette> {
    match tokio::fs::read(file(s)).await {
        Ok(b) => serde_json::from_slice(&b).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub async fn list(State(s): State<Shared>, CurrentUser(_me): CurrentUser) -> Json<Vec<Palette>> {
    Json(load(&s).await)
}

fn check(colors: &[NamedColor]) -> ApiResult<()> {
    for c in colors {
        if c.id.trim().is_empty()
            || cg_core::Color::from_hex(&c.value).is_none()
            || !c.value.trim().starts_with('#')
        {
            return Err(bad(format!("invalid colour {:?}", c.value)));
        }
    }
    Ok(())
}

/// Replaces the shared palettes; the projects using a colour whose value
/// changed follow it.
pub async fn put(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Json(palettes): Json<Vec<Palette>>,
) -> ApiResult<StatusCode> {
    require(me.can_create_projects())?;
    for p in &palettes {
        check(&p.colors)?;
    }
    let _edit = s.edits.lock().await;
    let old = load(&s).await;
    let all = |ps: &[Palette]| ps.iter().flat_map(|p| p.colors.clone()).collect::<Vec<_>>();
    let changes = cg_core::palette::changes(&all(&old), &all(&palettes));
    let bytes = serde_json::to_vec_pretty(&palettes).map_err(internal)?;
    tokio::fs::write(file(&s), bytes).await.map_err(internal)?;
    if !changes.is_empty() {
        for summary in s.summaries().await {
            let Some(p) = s.project(&summary.id).await else {
                continue;
            };
            let mut p = (*p).clone();
            if p.replace_colors(&changes) {
                s.save_project(p).await.map_err(internal)?;
            }
        }
    }
    s.changed("colors");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct ProjectColors {
    colors: Vec<NamedColor>,
}

/// Replaces the project's own palette (its layers follow the changed colours).
pub async fn put_project(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Json(body): Json<ProjectColors>,
) -> ApiResult<StatusCode> {
    require(me.can_design(&id))?;
    check(&body.colors)?;
    let _edit = s.edits.lock().await;
    let mut p = (*visible_project(&s, &me, &id).await?).clone();
    let changes = cg_core::palette::changes(&p.colors, &body.colors);
    p.replace_colors(&changes);
    p.colors = body.colors;
    s.save_project(p).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
