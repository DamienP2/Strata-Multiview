//! REST + WebSocket API used by the web app and by automation systems
//! (Companion / Stream Deck, newsroom MOS gateways, sports data feeds...).
//! Every route checks the caller's role and project / channel scope.

use std::collections::BTreeMap;
use std::net::SocketAddr;

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use cg_core::{Command, FrameRate, Project, RundownItem, Template};
use cg_render::slate::{Slate, SlateStyle};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use crate::auth::{self, CurrentUser, MaybeUser, PublicUser, Role, User, UserInput};
use crate::config::ServerConfig;
use crate::library::{valid_id, ProjectSummary};
use crate::state::{now_ms, ChannelInfo, ClientMessage, Event, Shared};

pub fn router() -> Router<Shared> {
    Router::new()
        .merge(crate::media::router())
        .merge(crate::iconlib::router())
        .merge(crate::health::router())
        .merge(crate::apiv1::router())
        .merge(crate::social::api::router())
        .merge(crate::social::moderation::router())
        .route("/api/auth/state", get(auth_state))
        .route("/api/auth/setup", post(auth_setup))
        .route("/api/auth/login", post(auth_login))
        .route("/api/auth/logout", post(auth_logout))
        .route("/api/me/lang", put(put_my_lang))
        .route("/api/me/theme", put(put_my_theme))
        .route(
            "/api/me/shortcuts",
            get(crate::prefs::get_shortcuts).put(crate::prefs::put_shortcuts),
        )
        .route(
            "/api/me/display",
            get(crate::prefs::get_display).patch(crate::prefs::patch_display),
        )
        .route("/api/web/proxy", get(crate::webproxy::proxy))
        .route("/api/inputs", get(crate::inputs::list))
        .route("/api/inputs/ndi-sources", get(crate::inputs::ndi_sources))
        .route("/api/inputs/ndi-preview", get(crate::inputs::ndi_preview))
        .route(
            "/api/inputs/{id}/snapshot.jpg",
            get(crate::inputs::snapshot),
        )
        .route("/api/inputs/{id}/live.mjpeg", get(crate::inputs::mjpeg))
        .route("/api/users", get(list_users).post(create_user))
        .route("/api/users/{id}", put(update_user).delete(delete_user))
        .route("/api/keys", get(list_keys).post(create_key))
        .route("/api/keys/{id}", axum::routing::delete(delete_key))
        .route("/api/keys/{id}/reveal", post(reveal_key))
        .route("/api/openapi.json", get(openapi))
        .route("/api/projects", get(list_projects).post(create_project))
        .route("/api/examples", get(list_examples))
        .route("/api/examples/{id}", post(create_from_example))
        .route("/api/examples/{id}/cover.png", get(example_cover))
        .route("/api/projects/import", post(import_project))
        .route(
            "/api/import/analyze",
            post(import_analyze).layer(axum::extract::DefaultBodyLimit::max(1024 * 1024 * 1024)),
        )
        .route("/api/import/{token}/files/{name}", get(import_file))
        .route("/api/import/{token}/create", post(import_create))
        .route("/api/import/{token}/into/{id}", post(import_into))
        .route(
            "/api/projects/{id}",
            get(get_project)
                .put(put_project_meta)
                .delete(delete_project),
        )
        .route("/api/projects/{id}/export", get(export_project))
        .route("/api/projects/{id}/locks", get(project_locks))
        .route("/api/projects/{id}/rundown", put(put_rundown))
        .route("/api/projects/{id}/zones", put(put_zones))
        .route("/api/projects/{id}/playlists", put(put_playlists))
        .route("/api/projects/{id}/formats", put(put_formats))
        .route("/api/projects/{id}/guides", put(put_guides))
        .route("/api/projects/{id}/design-order", put(put_design_order))
        .route("/api/projects/{id}/publish", post(publish_project))
        .route(
            "/api/projects/{id}/versions",
            get(list_versions).post(create_version),
        )
        .route(
            "/api/projects/{id}/versions/{vid}/restore",
            post(restore_version),
        )
        .route(
            "/api/projects/{id}/templates/{tid}",
            put(put_template).delete(delete_template),
        )
        .route(
            "/api/projects/{id}/templates/{tid}/publish",
            post(publish_template),
        )
        .route(
            "/api/projects/{id}/templates/{tid}/draft",
            axum::routing::delete(discard_draft),
        )
        .route(
            "/api/projects/{id}/templates/{tid}/lock",
            post(lock_template).delete(unlock_template),
        )
        .route(
            "/api/projects/{id}/templates/{tid}/thumbnail.png",
            get(template_thumbnail),
        )
        .route(
            "/api/projects/{id}/import/after-effects",
            post(import_after_effects)
                .layer(axum::extract::DefaultBodyLimit::max(256 * 1024 * 1024)),
        )
        .route("/api/render", post(render_video))
        .route("/api/channels", get(list_channels))
        .route("/api/channels/{id}/state", get(channel_state))
        .route("/api/channels/{id}/project", put(load_project))
        .route("/api/channels/{id}/command", post(channel_command))
        .route(
            "/api/channels/{id}/undo",
            get(channel_undo_list).post(channel_undo),
        )
        .route("/api/channels/{id}/locks", get(zone_locks))
        .route(
            "/api/channels/{id}/schedule",
            get(crate::schedule::list).post(crate::schedule::create),
        )
        .route(
            "/api/channels/{id}/schedule/{sid}",
            axum::routing::patch(crate::schedule::patch).delete(crate::schedule::delete),
        )
        .route(
            "/api/channels/{id}/zones/{zone}/lock",
            put(take_zone).delete(free_zone),
        )
        .route(
            "/api/channels/{id}/items/{item}/{action}",
            post(item_command),
        )
        .route("/api/channels/{id}/preview.png", get(channel_preview))
        .route("/api/channels/{id}/slate.png", get(channel_slate))
        .route("/api/slate.png", get(slate_preview))
        .route("/api/channels/{id}/outputs/{name}/{essence}", get(get_sdp))
        .route(
            "/api/colors",
            get(crate::colors::list).put(crate::colors::put),
        )
        .route("/api/projects/{id}/colors", put(crate::colors::put_project))
        .route("/api/fonts", get(list_fonts))
        .route("/api/fonts/file/{family}/{weight}", get(font_file))
        .route(
            "/api/fonts/{name}",
            put(upload_font)
                .delete(delete_font)
                .layer(axum::extract::DefaultBodyLimit::max(64 * 1024 * 1024)),
        )
        .route("/api/assets", get(list_assets))
        .route("/api/assets/{name}", put(put_asset))
        .route("/api/health", get(health))
        .route("/api/qr", get(qr))
        .route("/api/system", get(get_system))
        .route("/api/system/metrics", get(crate::perf::metrics))
        .route("/api/system/logs", get(crate::perf::logs))
        .route("/api/system/logs/download", get(crate::perf::logs_download))
        .route("/api/asrun", get(crate::asrun::get_asrun))
        .route("/api/asrun.csv", get(crate::asrun::get_asrun_csv))
        .route("/api/system/config", put(put_system_config))
        .route("/api/system/time", get(get_system_time))
        .route("/api/system/time/sync", post(sync_system_time))
        .route("/api/outputs/status", get(outputs_status))
        .route("/api/hardware", get(get_hardware))
        .route("/api/system/restart", post(post_restart))
        .route("/api/system/tray", get(tray_info))
        .route("/api/system/restart-server", post(post_restart_server))
        .route("/api/system/stop", post(post_stop))
        .route("/api/system/network", put(put_network_roles))
        .route("/api/mos/status", get(mos_status))
        .route("/api/data/sources", get(list_sources).post(create_source))
        .route(
            "/api/data/sources/{id}",
            put(put_source).delete(delete_source),
        )
        .route(
            "/api/data/sources/{id}/value",
            get(source_value)
                .put(push_value)
                .layer(axum::extract::DefaultBodyLimit::max(
                    crate::datasources::MAX_BYTES as usize,
                )),
        )
        .route("/api/data/sources/{id}/refresh", post(refresh_source))
        .route("/api/data/test", post(test_source))
        .route("/api/data/lookup", post(data_lookup))
        .route("/api/data/pipeline", post(data_pipeline))
        .route("/api/widgets/act", post(widget_act))
        .route("/api/mos/info", get(mos_info))
        .route("/api/system/update", get(update_info))
        .route("/api/system/update/settings", put(update_settings))
        .route("/api/system/update/check", post(update_check))
        .route("/api/system/update/install", post(update_install))
        .route(
            "/api/system/update/upload",
            post(update_upload).layer(axum::extract::DefaultBodyLimit::max(512 * 1024 * 1024)),
        )
        .route("/api/system/update/rollback", post(update_rollback))
        .route("/output", get(output_redirect))
        .route("/ws", get(ws_upgrade))
}

pub(crate) type ApiResult<T> = Result<T, (StatusCode, String)>;

pub(crate) fn bad(e: impl ToString) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, e.to_string())
}
pub(crate) fn internal(e: impl ToString) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
pub(crate) fn not_found(what: &str) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, format!("no such {what}"))
}
fn forbidden() -> (StatusCode, String) {
    (
        StatusCode::FORBIDDEN,
        "you do not have the rights for this".into(),
    )
}
pub(crate) fn require(ok: bool) -> ApiResult<()> {
    if ok {
        Ok(())
    } else {
        Err(forbidden())
    }
}

// ------------------------------------------------------------------ auth ----

fn session_cookie(token: &str, max_age: i64) -> String {
    format!(
        "{}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}",
        auth::COOKIE
    )
}

fn server_timezone(s: &Shared) -> String {
    if !s.config.timezone.trim().is_empty() {
        return s.config.timezone.trim().to_string();
    }
    jiff::tz::TimeZone::system()
        .iana_name()
        .unwrap_or("UTC")
        .to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthState {
    needs_setup: bool,
    user: Option<PublicUser>,
    server_name: String,
    public_outputs: bool,
}

async fn auth_state(State(s): State<Shared>, MaybeUser(user): MaybeUser) -> Json<AuthState> {
    Json(AuthState {
        needs_setup: s.auth.needs_setup(),
        user: user.as_ref().map(PublicUser::from),
        server_name: s.config.name.clone(),
        public_outputs: s.config.public_outputs,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Credentials {
    username: String,
    password: String,
    #[serde(default)]
    display_name: String,
}

#[derive(Serialize)]
struct LoginReply {
    user: PublicUser,
    /// Also usable as `Authorization: Bearer <token>` by automation.
    token: String,
}

fn login_response(token: String, user: &User) -> Response {
    let cookie = session_cookie(&token, 14 * 24 * 3600);
    (
        [(header::SET_COOKIE, cookie)],
        Json(LoginReply {
            user: user.into(),
            token,
        }),
    )
        .into_response()
}

/// First admin account: only while no user exists, and only from the
/// server itself (or `strata-server user add` on the command line).
async fn auth_setup(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(c): Json<Credentials>,
) -> ApiResult<Response> {
    if !s.auth.needs_setup() {
        return Err((StatusCode::CONFLICT, "already set up".into()));
    }
    if !peer.ip().is_loopback() {
        return Err((StatusCode::FORBIDDEN, "the first admin must be created on the server itself (http://localhost) or with `strata-server user add`".into()));
    }
    let input = UserInput {
        username: c.username,
        display_name: c.display_name,
        role: Role::Admin,
        projects: None,
        channels: None,
        disabled: false,
        password: c.password.clone(),
    };
    let user = s.auth.create(input).map_err(bad)?;
    let (token, user) = s
        .auth
        .login(&user.username, &c.password)
        .ok_or_else(|| internal("login failed"))?;
    Ok(login_response(token, &user))
}

async fn auth_login(State(s): State<Shared>, Json(c): Json<Credentials>) -> ApiResult<Response> {
    let auth = s.auth.clone();
    // Argon2 is deliberately slow: keep it off the async workers.
    let res = tokio::task::spawn_blocking(move || auth.login(&c.username, &c.password))
        .await
        .map_err(internal)?;
    match res {
        Some((token, user)) => Ok(login_response(token, &user)),
        None => Err((
            StatusCode::UNAUTHORIZED,
            "wrong user name or password".into(),
        )),
    }
}

async fn auth_logout(State(s): State<Shared>, headers: HeaderMap) -> Response {
    let (mut parts, _) = axum::http::Request::new(()).into_parts();
    parts.headers = headers;
    if let Some(t) = auth::token_from(&parts) {
        s.auth.logout(&t);
    }
    (
        [(header::SET_COOKIE, session_cookie("", 0))],
        StatusCode::NO_CONTENT,
    )
        .into_response()
}

// ----------------------------------------------------------------- users ----

#[derive(Deserialize)]
struct LangInput {
    lang: String,
}

/// Any user keeps their own language of the interface.
async fn put_my_lang(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Json(input): Json<LangInput>,
) -> ApiResult<Json<PublicUser>> {
    let u = s.auth.set_lang(&me.id, &input.lang).map_err(bad)?;
    Ok(Json((&u).into()))
}

#[derive(Deserialize)]
struct ThemeInput {
    theme: String,
}

/// Any user keeps their own colour theme of the interface.
async fn put_my_theme(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Json(input): Json<ThemeInput>,
) -> ApiResult<Json<PublicUser>> {
    let u = s.auth.set_theme(&me.id, &input.theme).map_err(bad)?;
    Ok(Json((&u).into()))
}

async fn list_users(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Json<Vec<PublicUser>>> {
    require(me.is_admin())?;
    Ok(Json(s.auth.users().iter().map(PublicUser::from).collect()))
}

async fn create_user(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Json(input): Json<UserInput>,
) -> ApiResult<Json<PublicUser>> {
    require(me.is_admin())?;
    let auth = s.auth.clone();
    let u = tokio::task::spawn_blocking(move || auth.create(input))
        .await
        .map_err(internal)?
        .map_err(bad)?;
    Ok(Json((&u).into()))
}

async fn update_user(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Json(input): Json<UserInput>,
) -> ApiResult<Json<PublicUser>> {
    require(me.is_admin())?;
    let auth = s.auth.clone();
    let u = tokio::task::spawn_blocking(move || auth.update(&id, input))
        .await
        .map_err(internal)?
        .map_err(bad)?;
    Ok(Json((&u).into()))
}

async fn delete_user(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    require(me.is_admin())?;
    if id == me.id {
        return Err(bad("you cannot delete your own account"));
    }
    s.auth.delete(&id).map_err(bad)?;
    Ok(StatusCode::NO_CONTENT)
}

// -------------------------------------------------------------- API keys ----

async fn list_keys(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
) -> Json<Vec<auth::PublicApiKey>> {
    Json(
        s.auth
            .api_keys()
            .into_iter()
            .filter(|k| me.is_admin() || k.user_id == me.id)
            .collect(),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewKey {
    name: String,
    /// Admins can create keys for any user (e.g. a service account).
    #[serde(default)]
    user_id: Option<String>,
}

#[derive(Serialize)]
struct CreatedKey {
    key: auth::PublicApiKey,
    /// Shown once: store it in the automation system.
    secret: String,
}

async fn create_key(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Json(n): Json<NewKey>,
) -> ApiResult<Json<CreatedKey>> {
    let owner = n.user_id.unwrap_or_else(|| me.id.clone());
    require(me.is_admin() || (owner == me.id && me.role != Role::Viewer))?;
    let (secret, key) = s.auth.create_api_key(&owner, &n.name).map_err(bad)?;
    tracing::info!(by = me.username, key = key.name, "API key created");
    Ok(Json(CreatedKey { key, secret }))
}

#[derive(Deserialize)]
struct RevealKey {
    /// The admin's own password, typed again.
    password: String,
}

#[derive(Serialize)]
struct RevealedKey {
    secret: String,
}

async fn reveal_key(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Json(r): Json<RevealKey>,
) -> ApiResult<Json<RevealedKey>> {
    require(me.is_admin())?;
    let secret = s
        .auth
        .reveal_api_key(&id, &me.id, &r.password)
        .map_err(|e| {
            if e == "wrong password" {
                (StatusCode::FORBIDDEN, e)
            } else {
                bad(e)
            }
        })?;
    tracing::info!(by = me.username, key = id, "API key shown again");
    Ok(Json(RevealedKey { secret }))
}

async fn delete_key(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let key = s
        .auth
        .api_keys()
        .into_iter()
        .find(|k| k.id == id)
        .ok_or_else(|| not_found("key"))?;
    require(me.is_admin() || key.user_id == me.id)?;
    s.auth.delete_api_key(&id).map_err(bad)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The API description (OpenAPI 3.1), for code generators and the docs page.
async fn openapi() -> Response {
    let spec = include_str!("openapi.json").replace(
        "\"version\": \"0.0.0\"",
        &format!("\"version\": \"{}\"", env!("CARGO_PKG_VERSION")),
    );
    ([(header::CONTENT_TYPE, "application/json")], spec).into_response()
}

// -------------------------------------------------------------- projects ----

async fn list_projects(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
) -> Json<Vec<ProjectSummary>> {
    Json(
        s.summaries()
            .await
            .into_iter()
            .filter(|p| me.sees_project(&p.id))
            .collect(),
    )
}

pub(crate) async fn visible_project(
    s: &Shared,
    me: &User,
    id: &str,
) -> ApiResult<std::sync::Arc<Project>> {
    if !me.sees_project(id) {
        return Err(not_found("project"));
    }
    s.project(id).await.ok_or_else(|| not_found("project"))
}

async fn get_project(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Project>> {
    Ok(Json((*visible_project(&s, &me, &id).await?).clone()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewProject {
    name: String,
    #[serde(default)]
    description: String,
    width: Option<u32>,
    height: Option<u32>,
    frame_rate: Option<FrameRate>,
    /// Duplicate this project instead of starting empty.
    #[serde(default)]
    from: Option<String>,
}

fn check_format(width: u32, height: u32, rate: FrameRate) -> ApiResult<()> {
    if !(16..=7680).contains(&width)
        || !(16..=4320).contains(&height)
        || rate.num == 0
        || rate.den == 0
    {
        return Err(bad("invalid video format"));
    }
    Ok(())
}

async fn create_project(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Json(n): Json<NewProject>,
) -> ApiResult<Json<ProjectSummary>> {
    let _edit = s.edits.lock().await;
    require(me.can_create_projects())?;
    if n.name.trim().is_empty() {
        return Err(bad("a project needs a name"));
    }
    let mut p = match &n.from {
        Some(src) => {
            let mut p = (*visible_project(&s, &me, src).await?).clone();
            p.rundown.iter_mut().for_each(|r| r.id = cg_core::new_id());
            p
        }
        None => Project {
            guides: Default::default(),
            design_order: Vec::new(),
            language: "fr".into(),
            template_secs: None,
            colors: Vec::new(),
            id: String::new(),
            name: String::new(),
            description: String::new(),
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(50, 1),
            templates: Vec::new(),
            rundown: Vec::new(),
            zones: cg_core::Zone::defaults(),
            playlists: Vec::new(),
            formats: Vec::new(),
            drafts: Vec::new(),
            starter: None,
        },
    };
    p.id = cg_core::new_id();
    p.name = n.name.trim().to_string();
    p.description = n.description;
    if let Some(w) = n.width {
        p.width = w;
    }
    if let Some(h) = n.height {
        p.height = h;
    }
    if let Some(r) = n.frame_rate {
        p.frame_rate = r;
    }
    check_format(p.width, p.height, p.frame_rate)?;
    if n.from.is_none() {
        // A new project starts with one template to design, already
        // published: the operator sees it right away.
        let zone = p.zones.first().map(|z| z.id.clone()).unwrap_or_default();
        // Named in the user's language (French by default).
        let first = if me.lang.starts_with("en") {
            "Template 1"
        } else {
            "Modèle 1"
        };
        let blank = cg_core::Template::blank_secs(
            first,
            &zone,
            p.frame_rate.as_f64(),
            p.template_secs.unwrap_or(1.0),
        );
        let id = blank.id.clone();
        p.drafts.push(blank);
        p.publish(&id);
        // It goes at a publish if it is still empty (the designer made others).
        p.starter = Some(id);
    }
    if let Some(src) = &n.from {
        // The copy gets its own media, and its templates point to them.
        copy_dir(
            &crate::media::project_dir(&s, src),
            &crate::media::project_dir(&s, &p.id),
        )
        .await
        .map_err(internal)?;
        let json = serde_json::to_string(&p).map_err(internal)?.replace(
            &crate::media::project_prefix(src),
            &crate::media::project_prefix(&p.id),
        );
        p = serde_json::from_str(&json).map_err(internal)?;
    }
    let p = s.save_project(p).await.map_err(internal)?;
    Ok(Json(ProjectSummary::from(p.as_ref())))
}

/// The example projects.
async fn list_examples(
    CurrentUser(_me): CurrentUser,
) -> Json<Vec<crate::examples::ExampleSummary>> {
    Json(crate::examples::list())
}

/// A new project made from an example.
async fn create_from_example(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<ProjectSummary>> {
    let _edit = s.edits.lock().await;
    require(me.can_create_projects())?;
    let new_id = cg_core::new_id();
    let (mut p, sources) =
        crate::examples::project(&id, &new_id).ok_or_else(|| not_found("example"))?;
    p.id = new_id;
    p.rundown.iter_mut().for_each(|r| r.id = cg_core::new_id());
    // The data sources it reads (shared, or the new project's own), with
    // their sample value, unless they exist already (never replaced).
    let mut added = false;
    for ex in sources {
        let sid = ex.source.id.clone();
        if s.data.get(&sid).is_some() {
            continue;
        }
        let src = ex.source;
        s.data.put(src).map_err(internal)?;
        added = true;
        match ex.value {
            Some(v) => {
                let changed = s.data.record(&sid, Ok((v, 0)));
                crate::datasources::updated(&s, &sid, changed).await;
            }
            None => s.data.refresh(&sid),
        }
    }
    if added {
        s.changed("sources");
    }
    let p = s.save_project(p).await.map_err(internal)?;
    Ok(Json(ProjectSummary::from(p.as_ref())))
}

/// The picture of an example's card: its cover template on air.
async fn example_cover(
    State(s): State<Shared>,
    CurrentUser(_me): CurrentUser,
    Path(id): Path<String>,
    Query(q): Query<CoverQuery>,
) -> ApiResult<Response> {
    let (p, template) = crate::examples::cover(&id).ok_or_else(|| not_found("example"))?;
    // The examples are in the program: the same picture until an update.
    const CACHE: &str = "private, max-age=86400";
    let width = q.w.unwrap_or(u32::MAX).clamp(16, 7680);
    let key = crate::thumbs::Key::default()
        .part("example")
        .template(&p, &template)
        .part(&width)
        .finish();
    if let Some(bytes) = s.thumbs.get(key) {
        return Ok(png_cached(bytes, CACHE));
    }
    let st = s.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        let data = template.default_data();
        let frame = {
            let mut r = st.renderer.lock().unwrap_or_else(|e| e.into_inner());
            r.render_template_scaled(
                &p,
                &template,
                cg_core::MAIN_FORMAT,
                template.hold_frame(),
                &data,
                width,
            )
        };
        cg_render::pool::busy();
        frame.to_png()
    })
    .await
    .map_err(internal)?;
    let bytes = Bytes::from(bytes);
    s.thumbs.put(key, bytes.clone());
    Ok(png_cached(bytes, CACHE))
}

#[derive(Deserialize)]
struct CoverQuery {
    /// At most this wide (pixels).
    w: Option<u32>,
}

/// Copies the files of a directory (not recursive). A missing source is empty.
async fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    let mut rd = match tokio::fs::read_dir(from).await {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    tokio::fs::create_dir_all(to).await?;
    while let Some(e) = rd.next_entry().await? {
        if e.file_type().await?.is_file() {
            tokio::fs::copy(e.path(), to.join(e.file_name())).await?;
        }
    }
    Ok(())
}

async fn import_project(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Json(mut p): Json<Project>,
) -> ApiResult<Json<ProjectSummary>> {
    let _edit = s.edits.lock().await;
    require(me.can_create_projects())?;
    check_format(p.width, p.height, p.frame_rate)?;
    if !valid_id(&p.id) || s.project(&p.id).await.is_some() {
        p.id = cg_core::new_id();
    }
    let p = s.save_project(p).await.map_err(internal)?;
    Ok(Json(ProjectSummary::from(p.as_ref())))
}

#[derive(Deserialize)]
struct AnalyzeQuery {
    /// The file's name (its extension tells the format).
    #[serde(default)]
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Analysis {
    token: String,
    #[serde(flatten)]
    converted: crate::import::Converted,
}

/// Reads an After Effects, MOGRT, Photoshop or SVG file into templates and a
/// report of each layer, for the import modal (nothing is created yet).
async fn import_analyze(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Query(q): Query<AnalyzeQuery>,
    body: Bytes,
) -> ApiResult<Json<Analysis>> {
    require(me.can_create_projects())?;
    let st = s.clone();
    let fonts = tokio::task::spawn_blocking(move || {
        st.renderer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .font_families()
    })
    .await
    .map_err(internal)?;
    let token = cg_core::new_id();
    let ctx = crate::import::Ctx {
        prefix: crate::import::stage_prefix(&token),
        fonts: fonts.into_iter().map(|f| f.family).collect(),
        zone: cg_core::Zone::defaults()[0].id.clone(),
    };
    let mut converted =
        tokio::task::spawn_blocking(move || crate::import::analyze(&q.name, &body, &ctx))
            .await
            .map_err(internal)?
            .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e))?;
    crate::import::stage(&token, &me.username, std::mem::take(&mut converted.files));
    Ok(Json(Analysis { token, converted }))
}

/// A picture of an analysed file (the modal's preview).
async fn import_file(
    CurrentUser(me): CurrentUser,
    Path((token, name)): Path<(String, String)>,
) -> ApiResult<Response> {
    let bytes = crate::import::staged_file(&token, &me.username, &name)
        .ok_or((StatusCode::NOT_FOUND, "no such file".to_string()))?;
    let mime = match name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    };
    Ok(([(header::CONTENT_TYPE, mime)], bytes).into_response())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportCreate {
    name: String,
    width: u32,
    height: u32,
    frame_rate: FrameRate,
    /// The templates kept (layers switched off already taken out).
    templates: Vec<Template>,
}

/// Creates the project of an analysed file: its templates (published) and
/// the pictures they use.
async fn import_create(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(token): Path<String>,
    Json(c): Json<ImportCreate>,
) -> ApiResult<Json<ProjectSummary>> {
    let _edit = s.edits.lock().await;
    require(me.can_create_projects())?;
    check_format(c.width, c.height, c.frame_rate)?;
    if c.name.trim().is_empty() {
        return Err(bad("a project needs a name"));
    }
    if c.templates.is_empty() {
        return Err(bad("nothing to import"));
    }
    let files = crate::import::take_staged(&token, &me.username).ok_or((
        StatusCode::GONE,
        "this import has expired: open the file again".to_string(),
    ))?;
    let mut p = Project {
        guides: Default::default(),
        design_order: Vec::new(),
        language: "fr".into(),
        template_secs: None,
        colors: Vec::new(),
        id: cg_core::new_id(),
        name: c.name.trim().to_string(),
        description: String::new(),
        width: c.width,
        height: c.height,
        frame_rate: c.frame_rate,
        templates: Vec::new(),
        rundown: Vec::new(),
        zones: cg_core::Zone::defaults(),
        playlists: Vec::new(),
        formats: Vec::new(),
        drafts: Vec::new(),
        starter: None,
    };
    // The pictures the kept templates use go to the project's media.
    let stage = crate::import::stage_prefix(&token);
    let json = serde_json::to_string(&c.templates).map_err(internal)?;
    let dir = crate::media::project_dir(&s, &p.id);
    let mut wrote = false;
    for (name, bytes) in &files {
        if json.contains(&format!("{stage}{name}")) {
            tokio::fs::create_dir_all(&dir).await.map_err(internal)?;
            tokio::fs::write(dir.join(name), bytes)
                .await
                .map_err(internal)?;
            wrote = true;
        }
    }
    if wrote {
        s.renderer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .invalidate_assets();
        s.thumbs.clear();
    }
    let templates: Vec<Template> =
        serde_json::from_str(&json.replace(&stage, &crate::media::project_prefix(&p.id)))
            .map_err(internal)?;
    let zone = p.zones.first().map(|z| z.id.clone()).unwrap_or_default();
    for mut t in templates {
        t.id = cg_core::new_id();
        t.zone = zone.clone();
        let id = t.id.clone();
        p.drafts.push(t);
        p.publish(&id);
    }
    let p = s.save_project(p).await.map_err(internal)?;
    tracing::info!(user = me.username, project = p.name, "project imported");
    Ok(Json(ProjectSummary::from(p.as_ref())))
}

#[derive(Deserialize)]
struct ImportInto {
    /// The templates kept, laid out for the project's format.
    templates: Vec<Template>,
}

/// Adds the templates of an analysed file to a project, as drafts (the
/// designer publishes them), with the pictures they use.
async fn import_into(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path((token, id)): Path<(String, String)>,
    Json(c): Json<ImportInto>,
) -> ApiResult<Json<Vec<String>>> {
    let _edit = s.edits.lock().await;
    require(me.can_design(&id))?;
    if c.templates.is_empty() {
        return Err(bad("nothing to import"));
    }
    let mut p = (*visible_project(&s, &me, &id).await?).clone();
    let files = crate::import::take_staged(&token, &me.username).ok_or((
        StatusCode::GONE,
        "this import has expired: open the file again".to_string(),
    ))?;
    let stage = crate::import::stage_prefix(&token);
    let json = serde_json::to_string(&c.templates).map_err(internal)?;
    let dir = crate::media::project_dir(&s, &p.id);
    let mut wrote = false;
    for (name, bytes) in &files {
        if json.contains(&format!("{stage}{name}")) {
            tokio::fs::create_dir_all(&dir).await.map_err(internal)?;
            tokio::fs::write(dir.join(name), bytes)
                .await
                .map_err(internal)?;
            wrote = true;
        }
    }
    if wrote {
        s.renderer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .invalidate_assets();
        s.thumbs.clear();
        s.changed(format!("media:{}", p.id));
    }
    let templates: Vec<Template> =
        serde_json::from_str(&json.replace(&stage, &crate::media::project_prefix(&p.id)))
            .map_err(internal)?;
    // In the full-screen zone when there is one, else the first.
    let zone = p
        .zones
        .iter()
        .find(|z| z.id == "full-screen")
        .or_else(|| p.zones.first())
        .map(|z| z.id.clone())
        .unwrap_or_default();
    let mut ids = Vec::new();
    for mut t in templates {
        t.id = cg_core::new_id();
        t.zone = zone.clone();
        ids.push(t.id.clone());
        p.drafts.push(t);
    }
    s.save_project(p).await.map_err(internal)?;
    tracing::info!(
        user = me.username,
        project = id,
        n = ids.len(),
        "templates imported"
    );
    Ok(Json(ids))
}

async fn export_project(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let p = visible_project(&s, &me, &id).await?;
    let name: String = p
        .name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let disposition = format!("attachment; filename=\"{name}.strata.json\"");
    let body = serde_json::to_vec_pretty(p.as_ref()).map_err(internal)?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/json".to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        body,
    )
        .into_response())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectMeta {
    name: String,
    #[serde(default)]
    description: String,
    width: u32,
    height: u32,
    frame_rate: FrameRate,
    /// Texts' language; unchanged when absent.
    #[serde(default)]
    language: Option<String>,
    /// Length of new templates, seconds; unchanged when absent.
    #[serde(default)]
    template_secs: Option<f64>,
}

async fn put_project_meta(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Json(m): Json<ProjectMeta>,
) -> ApiResult<StatusCode> {
    let _edit = s.edits.lock().await;
    require(me.can_design(&id))?;
    check_format(m.width, m.height, m.frame_rate)?;
    let mut p = (*visible_project(&s, &me, &id).await?).clone();
    p.name = m.name.trim().to_string();
    p.description = m.description;
    p.width = m.width;
    p.height = m.height;
    p.frame_rate = m.frame_rate;
    if let Some(l) = m.language.filter(|l| !l.trim().is_empty()) {
        p.language = l.trim().to_string();
    }
    if let Some(secs) = m.template_secs.filter(|s| s.is_finite() && *s > 0.0) {
        p.template_secs = Some(secs.clamp(0.1, 600.0));
    }
    s.save_project(p).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_project(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    require(me.can_design(&id))?;
    if !s.delete_project(&id).await.map_err(internal)? {
        return Err(not_found("project"));
    }
    let _ = tokio::fs::remove_dir_all(crate::media::project_dir(&s, &id)).await;
    // Its own data connectors go with it (the shared ones stay).
    let own: Vec<String> = s
        .data
        .list()
        .into_iter()
        .filter(|x| x.source.project.as_deref() == Some(id.as_str()))
        .map(|x| x.source.id)
        .collect();
    for sid in &own {
        let _ = s.data.delete(sid);
    }
    if !own.is_empty() {
        s.changed("sources");
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, Default)]
struct PutTemplateQuery {
    /// Publish at once (automation); otherwise the template is saved as a
    /// draft that the operator does not see until it is published.
    #[serde(default)]
    publish: bool,
}

async fn put_template(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path((id, tid)): Path<(String, String)>,
    Query(q): Query<PutTemplateQuery>,
    headers: HeaderMap,
    Json(t): Json<Template>,
) -> ApiResult<StatusCode> {
    let _edit = s.edits.lock().await;
    require(me.can_design(&id))?;
    if t.id != tid {
        return Err(bad("template id mismatch"));
    }
    s.locks.check(&id, &tid, &me, client_id(&headers))?;
    let mut p = (*visible_project(&s, &me, &id).await?).clone();
    p.save_draft(t);
    if q.publish {
        p.publish(&tid);
        p.drop_empty_starter();
    }
    s.save_project(p).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The designer tab a request comes from (`X-Strata-Client`).
fn client_id(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("x-strata-client")
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty() && v.len() <= 64)
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct LockRequest {
    /// Take the lock over from whoever holds it.
    #[serde(default)]
    force: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LockReply {
    granted: bool,
    lock: crate::locks::Lock,
}

/// Takes, renews (call it every ~15 s) or takes over a template's edit lock.
async fn lock_template(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path((id, tid)): Path<(String, String)>,
    headers: HeaderMap,
    body: Option<Json<LockRequest>>,
) -> ApiResult<Json<LockReply>> {
    require(me.can_design(&id))?;
    visible_project(&s, &me, &id).await?;
    let client = client_id(&headers).ok_or_else(|| bad("missing X-Strata-Client header"))?;
    let force = body.is_some_and(|b| b.force);
    match s.locks.acquire(&id, &tid, &me, client, force) {
        crate::locks::Acquired::Held(lock) => Ok(Json(LockReply {
            granted: false,
            lock,
        })),
        crate::locks::Acquired::Granted { lock, taken_from } => {
            let fresh = lock.since >= now_ms() - 2000 || taken_from.is_some();
            if fresh {
                let _ = s.events.send(Event::Lock {
                    project_id: id.clone(),
                    template_id: tid.clone(),
                    lock: Some(lock.clone()),
                    taken_from: taken_from.map(|l| l.client),
                });
            }
            Ok(Json(LockReply {
                granted: true,
                lock,
            }))
        }
    }
}

async fn unlock_template(
    State(s): State<Shared>,
    CurrentUser(_me): CurrentUser,
    Path((id, tid)): Path<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<StatusCode> {
    let client = client_id(&headers).ok_or_else(|| bad("missing X-Strata-Client header"))?;
    if s.locks.release(&id, &tid, client) {
        let _ = s.events.send(Event::Lock {
            project_id: id,
            template_id: tid,
            lock: None,
            taken_from: None,
        });
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Current edit locks of a project, by template id.
async fn project_locks(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<std::collections::HashMap<String, crate::locks::Lock>>> {
    visible_project(&s, &me, &id).await?;
    Ok(Json(s.locks.of_project(&id)))
}

/// Makes the draft of a template the version the operator and the outputs use.
async fn publish_template(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path((id, tid)): Path<(String, String)>,
) -> ApiResult<Json<BTreeMap<&'static str, u32>>> {
    let _edit = s.edits.lock().await;
    require(me.can_design(&id))?;
    let mut p = (*visible_project(&s, &me, &id).await?).clone();
    let rev = match p.publish(&tid) {
        Some(r) => r,
        None if p.template(&tid).is_some() => p.template(&tid).map_or(0, |t| t.revision),
        None => return Err(not_found("template")),
    };
    let name = p.template(&tid).map(|t| t.name.clone()).unwrap_or_default();
    p.drop_empty_starter();
    let p = s.save_project(p).await.map_err(internal)?;
    // Every publish is a version the designer can go back to.
    let note = format!("Published “{name}” (revision {rev})");
    if let Err(e) = s.library.read().await.save_version(&p, who(&me), &note) {
        tracing::warn!("cannot keep a version: {e}");
    }
    Ok(Json(BTreeMap::from([("revision", rev)])))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Published {
    id: String,
    name: String,
    revision: u32,
}

/// Publishes every template with unpublished changes; one version is kept.
async fn publish_project(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<Published>>> {
    let _edit = s.edits.lock().await;
    require(me.can_design(&id))?;
    let mut p = (*visible_project(&s, &me, &id).await?).clone();
    let ids: Vec<String> = p.drafts.iter().map(|t| t.id.clone()).collect();
    let mut out = Vec::new();
    for tid in ids {
        let name = p.draft(&tid).map(|t| t.name.clone()).unwrap_or_default();
        if let Some(revision) = p.publish(&tid) {
            out.push(Published {
                id: tid,
                name,
                revision,
            });
        }
    }
    if out.is_empty() {
        return Ok(Json(out));
    }
    if let Some(gone) = p.drop_empty_starter() {
        out.retain(|x| x.id != gone);
    }
    let p = s.save_project(p).await.map_err(internal)?;
    let names: Vec<&str> = out.iter().map(|x| x.name.as_str()).collect();
    let note = format!(
        "Published {} template{}: {}",
        out.len(),
        if out.len() > 1 { "s" } else { "" },
        names.join(", ")
    );
    if let Err(e) = s.library.read().await.save_version(&p, who(&me), &note) {
        tracing::warn!("cannot keep a version: {e}");
    }
    Ok(Json(out))
}

fn who(u: &crate::auth::User) -> &str {
    if u.display_name.is_empty() {
        &u.username
    } else {
        &u.display_name
    }
}

async fn list_versions(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<crate::library::VersionInfo>>> {
    visible_project(&s, &me, &id).await?;
    Ok(Json(s.library.read().await.versions(&id)))
}

#[derive(Deserialize)]
struct NewVersion {
    #[serde(default)]
    note: String,
}

/// Keeps the project as it is now in its history.
async fn create_version(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Json(n): Json<NewVersion>,
) -> ApiResult<Json<crate::library::VersionInfo>> {
    require(me.can_design(&id))?;
    let p = visible_project(&s, &me, &id).await?;
    let note = if n.note.trim().is_empty() {
        "Saved by hand".to_string()
    } else {
        n.note.trim().to_string()
    };
    let v = s
        .library
        .read()
        .await
        .save_version(&p, who(&me), &note)
        .map_err(internal)?;
    Ok(Json(v))
}

/// Puts a project back as it was in a version (the current state is kept
/// as a version first, so a restore can be undone).
async fn restore_version(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path((id, vid)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let _edit = s.edits.lock().await;
    require(me.can_design(&id))?;
    let current = visible_project(&s, &me, &id).await?;
    let mut old = s
        .library
        .read()
        .await
        .version(&id, &vid)
        .ok_or_else(|| not_found("version"))?;
    s.library
        .read()
        .await
        .save_version(&current, who(&me), "Before restoring an older version")
        .map_err(internal)?;
    old.id = id;
    s.save_project(old).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Drops a template's unpublished changes (back to what is on air).
async fn discard_draft(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path((id, tid)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let _edit = s.edits.lock().await;
    require(me.can_design(&id))?;
    let mut p = (*visible_project(&s, &me, &id).await?).clone();
    if p.template(&tid).is_none() {
        return Err(bad("this template was never published: delete it instead"));
    }
    p.drafts.retain(|t| t.id != tid);
    s.save_project(p).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportQuery {
    #[serde(default)]
    name: Option<String>,
    /// Only convert (its pictures are saved): the Designer puts its layers
    /// into the open template.
    #[serde(default)]
    layers_only: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResult {
    template: Template,
    warnings: Vec<String>,
}

/// Imports an After Effects composition exported with Bodymovin /
/// LottieFiles (`.json`, `.zip`, `.lottie`) as a new template.
async fn import_after_effects(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Query(q): Query<ImportQuery>,
    body: Bytes,
) -> ApiResult<Json<ImportResult>> {
    let _edit = s.edits.lock().await;
    require(me.can_design(&id))?;
    let mut p = (*visible_project(&s, &me, &id).await?).clone();
    let opts = cg_core::lottie::Options {
        width: p.width as f64,
        height: p.height as f64,
        fps: p.frame_rate.as_f64(),
        zone: p.zones.first().map(|z| z.id.clone()).unwrap_or_default(),
    };
    let out = tokio::task::spawn_blocking(move || crate::ae_import::import(&body, &opts))
        .await
        .map_err(internal)?
        .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e))?;
    // Its images go to the project's media.
    let mut template = out.imported.template;
    if !out.files.is_empty() {
        let dir = crate::media::project_dir(&s, &id);
        tokio::fs::create_dir_all(&dir).await.map_err(internal)?;
        for (name, bytes) in &out.files {
            tokio::fs::write(dir.join(name), bytes)
                .await
                .map_err(internal)?;
        }
        s.renderer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .invalidate_assets();
        s.thumbs.clear();
        let mut json = serde_json::to_string(&template).map_err(internal)?;
        for (name, _) in &out.files {
            json = json.replace(
                &format!("\"/assets/{name}\""),
                &format!("\"{}{name}\"", crate::media::project_prefix(&id)),
            );
        }
        template = serde_json::from_str(&json).map_err(internal)?;
    }
    if let Some(name) = q.name.filter(|n| !n.trim().is_empty()) {
        template.name = name.trim().to_string();
    }
    if !q.layers_only {
        p.save_draft(template.clone());
        s.save_project(p).await.map_err(internal)?;
    }
    Ok(Json(ImportResult {
        template,
        warnings: out.imported.warnings,
    }))
}

async fn delete_template(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path((id, tid)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let _edit = s.edits.lock().await;
    require(me.can_design(&id))?;
    let mut p = (*visible_project(&s, &me, &id).await?).clone();
    if p.design_templates().all(|t| t.id == tid) {
        return Err((
            StatusCode::CONFLICT,
            "a project keeps at least one template".into(),
        ));
    }
    let before = p.templates.len() + p.drafts.len();
    p.templates.retain(|t| t.id != tid);
    p.drafts.retain(|t| t.id != tid);
    if p.templates.len() + p.drafts.len() == before {
        return Err(not_found("template"));
    }
    p.rundown.retain(|r| r.template_id != tid);
    s.save_project(p).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// A whole rundown, or what a client changed (`base` → `rundown`): only
/// that change is applied, over edits made meanwhile by others.
#[derive(Deserialize)]
#[serde(untagged)]
enum RundownPut {
    Full(Vec<RundownItem>),
    Change {
        base: Vec<RundownItem>,
        rundown: Vec<RundownItem>,
    },
}

async fn put_rundown(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Json(r): Json<RundownPut>,
) -> ApiResult<StatusCode> {
    let _edit = s.edits.lock().await;
    require(me.can_edit_rundown(&id))?;
    let mut p = (*visible_project(&s, &me, &id).await?).clone();
    p.rundown = match r {
        RundownPut::Full(r) => r,
        RundownPut::Change { base, rundown } => cg_core::merge_rundown(&p.rundown, &base, &rundown),
    };
    s.save_project(p).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Replaces the zones of a project. Zones still used by templates or the
/// rundown cannot be removed; graphics on air follow their zone's new layer.
async fn put_zones(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Json(zones): Json<Vec<cg_core::Zone>>,
) -> ApiResult<StatusCode> {
    let _edit = s.edits.lock().await;
    require(me.can_design(&id))?;
    Project::validate_zones(&zones).map_err(bad)?;
    let mut p = (*visible_project(&s, &me, &id).await?).clone();
    // Only zones removed now count: a draft already pointing to an unknown
    // zone (it plays in the default one) does not block every zone change.
    let kept = |z: &str| zones.iter().any(|x| x.id == z) || !p.zones.iter().any(|x| x.id == z);
    if let Some(t) = p.templates.iter().chain(&p.drafts).find(|t| !kept(&t.zone)) {
        return Err(bad(format!(
            "template “{}” still plays in a removed zone; move it first",
            t.name
        )));
    }
    if let Some(r) = p
        .rundown
        .iter()
        .find(|r| !r.zone.is_empty() && !kept(&r.zone))
    {
        return Err(bad(format!(
            "rundown item “{}” still plays in a removed zone; move it first",
            r.name
        )));
    }
    let moves: Vec<(u32, u32)> = p
        .zones
        .iter()
        .filter_map(|old| {
            zones
                .iter()
                .find(|n| n.id == old.id && n.layer != old.layer)
                .map(|n| (old.layer, n.layer))
        })
        .collect();
    p.zones = zones;
    s.save_project(p).await.map_err(internal)?;
    if !moves.is_empty() {
        s.move_layers(&id, &moves).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Replaces the playlists (rundown sections). Items of a removed playlist
/// move to the first one.
async fn put_playlists(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Json(lists): Json<Vec<cg_core::Playlist>>,
) -> ApiResult<StatusCode> {
    let _edit = s.edits.lock().await;
    require(me.can_edit_rundown(&id))?;
    let mut ids = std::collections::HashSet::new();
    if lists.is_empty()
        || lists
            .iter()
            .any(|l| l.name.trim().is_empty() || !valid_id(&l.id) || !ids.insert(l.id.clone()))
    {
        return Err(bad("playlists need a unique id and a name"));
    }
    let mut p = (*visible_project(&s, &me, &id).await?).clone();
    p.playlists = lists;
    s.save_project(p).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Replaces the alternative formats (9:16, 1:1…). Removing a format drops
/// its layer overrides.
async fn put_guides(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Json(g): Json<cg_core::Guides>,
) -> ApiResult<StatusCode> {
    let _edit = s.edits.lock().await;
    require(me.can_design(&id))?;
    let frac = |v: f64| v.is_finite() && (-0.5..=1.5).contains(&v);
    if !g.vertical.iter().chain(&g.horizontal).all(|v| frac(*v))
        || !(0.0..0.5).contains(&g.action_safe)
        || !(0.0..0.5).contains(&g.title_safe)
        || !g.exclusions.iter().all(|e| {
            frac(e.x)
                && frac(e.y)
                && e.width > 0.0
                && e.height > 0.0
                && e.width <= 1.0
                && e.height <= 1.0
        })
    {
        return Err(bad("guides out of the picture"));
    }
    let mut p = (*visible_project(&s, &me, &id).await?).clone();
    p.guides = g;
    s.save_project(p).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The Designer's order of the templates.
async fn put_design_order(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Json(order): Json<Vec<String>>,
) -> ApiResult<StatusCode> {
    let _edit = s.edits.lock().await;
    require(me.can_design(&id))?;
    let mut p = (*visible_project(&s, &me, &id).await?).clone();
    let known = |t: &String| p.templates.iter().chain(&p.drafts).any(|x| &x.id == t);
    let mut seen = std::collections::HashSet::new();
    p.design_order = order
        .into_iter()
        .filter(|t| known(t) && seen.insert(t.clone()))
        .collect();
    s.save_project(p).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn put_formats(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Json(formats): Json<Vec<cg_core::Format>>,
) -> ApiResult<StatusCode> {
    let _edit = s.edits.lock().await;
    require(me.can_design(&id))?;
    let mut ids = std::collections::HashSet::from([cg_core::MAIN_FORMAT.to_string()]);
    for f in &formats {
        if !valid_id(&f.id) || !ids.insert(f.id.clone()) || f.name.trim().is_empty() {
            return Err(bad("formats need a unique id and a name"));
        }
        check_format(f.width, f.height, FrameRate::new(1, 1))?;
    }
    let mut p = (*visible_project(&s, &me, &id).await?).clone();
    for t in p.templates.iter_mut().chain(p.drafts.iter_mut()) {
        for l in &mut t.layers {
            l.overrides.retain(|k, _| ids.contains(k));
        }
    }
    p.formats = formats;
    s.save_project(p).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

fn png(bytes: Vec<u8>) -> Response {
    png_cached(bytes.into(), "no-store")
}

/// A PNG with how long browsers keep it.
fn png_cached(bytes: Bytes, cache: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, cache),
        ],
        bytes,
    )
        .into_response()
}

/// A thumbnail whose address names its version (`v`) never changes:
/// browsers keep it (it is private to the signed-in user).
const VERSIONED: &str = "private, max-age=31536000, immutable";

#[derive(Deserialize)]
struct ThumbQuery {
    frame: Option<f64>,
    /// Project format id (default: main).
    format: Option<String>,
    /// Field values as JSON, over the template defaults.
    data: Option<String>,
    /// Show the designer's draft rather than the published template.
    #[serde(default)]
    draft: bool,
    /// At most this wide (pixels): drawn small for lists.
    w: Option<u32>,
    /// Version of the template in the address (cached by browsers when set).
    v: Option<String>,
}

async fn template_thumbnail(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path((id, tid)): Path<(String, String)>,
    Query(q): Query<ThumbQuery>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let p = visible_project(&s, &me, &id).await?;
    let template = if q.draft {
        p.draft(&tid)
    } else {
        p.template(&tid).or(p.draft(&tid))
    }
    .cloned()
    .ok_or_else(|| not_found("template"))?;
    // Default to the on-air point: that's what the graphic looks like on air.
    let frame = q.frame.unwrap_or_else(|| template.hold_frame());
    let mut data = template.default_data();
    if let Some(extra) = q
        .data
        .as_deref()
        .and_then(|j| serde_json::from_str::<cg_core::Data>(j).ok())
    {
        data.extend(extra);
    }
    // Data sets show their row (the first, or the one asked).
    cg_core::datapath::bind_datasets(&template, &mut data, &s.data.store());
    let width = q.w.unwrap_or(u32::MAX).clamp(16, 7680);
    let format = q.format.unwrap_or_else(|| cg_core::MAIN_FORMAT.to_string());
    // Made already (another list, another user): the same PNG.
    let key = crate::thumbs::Key::default()
        .template(&p, &template)
        .part(&format)
        .part(&frame.to_bits())
        .value(&data)
        .part(&width)
        .finish();
    // Without a version in the address: browsers ask again each time, and
    // get "not modified" while it shows the same.
    let etag = format!("\"{key:016x}\"");
    let cache = if q.v.is_some() {
        VERSIONED
    } else {
        "private, no-cache"
    };
    if q.v.is_none()
        && headers
            .get(header::IF_NONE_MATCH)
            .is_some_and(|v| v.as_bytes() == etag.as_bytes())
    {
        return Ok((StatusCode::NOT_MODIFIED, [(header::ETAG, etag)]).into_response());
    }
    let tagged = |res: Response| {
        let mut res = res;
        if let Ok(v) = header::HeaderValue::from_str(&etag) {
            res.headers_mut().insert(header::ETAG, v);
        }
        res
    };
    if let Some(bytes) = s.thumbs.get(key) {
        return Ok(tagged(png_cached(bytes, cache)));
    }
    let st = s.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        let frame = {
            let mut r = st.renderer.lock().unwrap_or_else(|e| e.into_inner());
            r.render_template_scaled(&p, &template, &format, frame, &data, width)
        };
        cg_render::pool::busy();
        frame.to_png()
    })
    .await
    .map_err(internal)?;
    let bytes = Bytes::from(bytes);
    s.thumbs.put(key, bytes.clone());
    Ok(tagged(png_cached(bytes, cache)))
}

/// Renders a graphic to a QuickTime file with alpha (post-production).
async fn render_video(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Json(req): Json<crate::render::RenderRequest>,
) -> ApiResult<Response> {
    require(me.role != Role::Viewer)?;
    let p = visible_project(&s, &me, &req.project_id).await?;
    let name: String = p
        .template(&req.template_id)
        .map(|t| t.name.clone())
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let assets = s.assets_dir();
    let tz = s.config.timezone.clone();
    let started = std::time::Instant::now();
    let mov =
        tokio::task::spawn_blocking(move || crate::render::render_clip(&p, &req, assets, &tz))
            .await
            .map_err(internal)?
            .map_err(bad)?;
    tracing::info!(
        user = me.username,
        bytes = mov.len(),
        ms = started.elapsed().as_millis() as u64,
        "clip rendered"
    );
    let disposition = format!("attachment; filename=\"{name}.mov\"");
    Ok((
        [
            (header::CONTENT_TYPE, "video/quicktime".to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        mov,
    )
        .into_response())
}

// -------------------------------------------------------------- channels ----

async fn list_channels(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
) -> Json<Vec<ChannelInfo>> {
    Json(
        s.channel_infos()
            .await
            .into_iter()
            .filter(|c| me.sees_channel(&c.id))
            .collect(),
    )
}

/// The project loaded on a channel is one the user may see (operators scoped to projects).
async fn sees_loaded(s: &Shared, me: &User, channel: &str) -> ApiResult<()> {
    let Some(ch) = s.channel(channel) else {
        return Ok(());
    };
    match ch.project_id.read().await.as_deref() {
        Some(pid) if !me.sees_project(pid) => Err(not_found("project")),
        _ => Ok(()),
    }
}

fn visible_channel(me: &User, id: &str) -> ApiResult<()> {
    if me.sees_channel(id) {
        Ok(())
    } else {
        Err(not_found("channel"))
    }
}

async fn channel_state(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    visible_channel(&me, &id)?;
    let ch = s.channel(&id).ok_or_else(|| not_found("channel"))?;
    Ok(Json(ch.state.read().await.clone()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadProject {
    project_id: Option<String>,
}

async fn load_project(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Json(l): Json<LoadProject>,
) -> ApiResult<StatusCode> {
    visible_channel(&me, &id)?;
    require(me.can_operate(&id))?;
    if let Some(pid) = &l.project_id {
        require(me.sees_project(pid))?;
    }
    s.load_project(&id, l.project_id).await.map_err(bad)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn channel_command(
    State(s): State<Shared>,
    auth::Acting(me, actor): auth::Acting,
    Path(id): Path<String>,
    Json(cmd): Json<Command>,
) -> ApiResult<impl IntoResponse> {
    visible_channel(&me, &id)?;
    require(me.can_operate(&id))?;
    sees_loaded(&s, &me, &id).await?;
    s.command(&id, cmd, &actor).await.map(Json).map_err(locked)
}

/// A command refused: on a zone another operator holds (409), or wrong.
fn locked(e: String) -> (StatusCode, String) {
    if e.ends_with("holds this zone") {
        (StatusCode::CONFLICT, e)
    } else {
        bad(e)
    }
}

/// The last actions of a channel that can be undone, latest first.
async fn channel_undo_list(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<crate::antenna::UndoStep>>> {
    visible_channel(&me, &id)?;
    let ch = s.channel(&id).ok_or_else(|| not_found("channel"))?;
    Ok(Json(ch.ops.undo_list()))
}

/// Undoes the last action of a channel.
async fn channel_undo(
    State(s): State<Shared>,
    auth::Acting(me, actor): auth::Acting,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    visible_channel(&me, &id)?;
    require(me.can_operate(&id))?;
    sees_loaded(&s, &me, &id).await?;
    s.undo(&id, &actor).await.map(Json).map_err(bad)
}

/// The zones of a channel held by operators.
async fn zone_locks(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<crate::antenna::ZoneLock>>> {
    visible_channel(&me, &id)?;
    let ch = s.channel(&id).ok_or_else(|| not_found("channel"))?;
    Ok(Json(ch.ops.locks()))
}

/// Takes a zone of a channel (from whoever held it, who is told).
async fn take_zone(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path((id, zone)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    visible_channel(&me, &id)?;
    require(me.can_operate(&id))?;
    s.set_zone_lock(&id, &zone, Some(&me)).map_err(bad)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Frees a zone of a channel (its holder, or an admin).
async fn free_zone(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path((id, zone)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    visible_channel(&me, &id)?;
    require(me.can_operate(&id))?;
    let ch = s.channel(&id).ok_or_else(|| not_found("channel"))?;
    let holder = ch.ops.locks().into_iter().find(|l| l.zone == zone);
    require(holder.is_none_or(|l| l.user_id == me.id || me.role == Role::Admin))?;
    s.set_zone_lock(&id, &zone, None).map_err(bad)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, Default)]
struct ItemBody {
    #[serde(default)]
    data: Option<BTreeMap<String, String>>,
    /// `update`: the new values cross-fade in (half a second) instead of a cut.
    #[serde(default)]
    fade: bool,
}

/// Plays a rundown item of the project loaded on the channel, by id: what a
/// Stream Deck button or a newsroom system needs (`take`, `continue`,
/// `out`, `update`, `clear`).
async fn item_command(
    State(s): State<Shared>,
    auth::Acting(me, actor): auth::Acting,
    Path((id, item, action)): Path<(String, String, String)>,
    body: Option<Json<ItemBody>>,
) -> ApiResult<impl IntoResponse> {
    visible_channel(&me, &id)?;
    require(me.can_operate(&id))?;
    sees_loaded(&s, &me, &id).await?;
    let ch = s.channel(&id).ok_or_else(|| not_found("channel"))?;
    let pid = ch
        .project_id
        .read()
        .await
        .clone()
        .ok_or_else(|| bad("no project loaded on this channel"))?;
    let project = s.project(&pid).await.ok_or_else(|| not_found("project"))?;
    let it = project
        .rundown
        .iter()
        .find(|r| r.id == item || r.name.eq_ignore_ascii_case(&item))
        .ok_or_else(|| not_found("rundown item"))?;
    let zone = project.item_zone(it).map(|z| z.id.clone());
    let fade = body.as_ref().is_some_and(|b| b.0.fade);
    let data = body.and_then(|b| b.0.data);
    let cmd = match action.as_str() {
        "take" => {
            let mut merged = it.data.clone();
            merged.extend(data.unwrap_or_default());
            Command::Take {
                zone,
                layer: None,
                template_id: it.template_id.clone(),
                data: Some(merged),
                rundown_id: Some(it.id.clone()),
            }
        }
        "continue" => Command::Continue { zone, layer: None },
        "out" => Command::TakeOut { zone, layer: None },
        "update" => Command::Update {
            zone,
            layer: None,
            data: data.unwrap_or_else(|| it.data.clone()),
            fade,
        },
        "clear" => Command::Clear { zone, layer: None },
        _ => {
            return Err((
                StatusCode::NOT_FOUND,
                "actions: take, continue, out, update, clear".into(),
            ))
        }
    };
    s.command(&id, cmd, &actor).await.map(Json).map_err(locked)
}

#[derive(Deserialize)]
struct PreviewQuery {
    /// Rendition id (default: the main picture).
    rendition: Option<String>,
}

async fn channel_preview(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Query(q): Query<PreviewQuery>,
) -> ApiResult<Response> {
    visible_channel(&me, &id)?;
    sees_loaded(&s, &me, &id).await?;
    let ch = s.channel(&id).ok_or_else(|| not_found("channel"))?.clone();
    let project = match ch.project_id.read().await.clone() {
        Some(pid) => s.project(&pid).await,
        None => None,
    };
    let state = ch.state.read().await.clone();
    let wanted = q.rendition.as_deref().unwrap_or("main");
    let slate = ch
        .config
        .slate_of(wanted)
        .ok_or_else(|| not_found("rendition"))?;
    let out = cg_render::OutputFormat {
        width: slate.width,
        height: slate.height,
    };
    let tz = s.config.timezone.clone();
    let st = s.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        let mut r = st.renderer.lock().unwrap_or_else(|e| e.into_inner());
        match project {
            Some(p) => r.render_channel(&p, &state, out, &tz).to_png(),
            // What the outputs show while no project is loaded.
            None => r.render_idle(&slate, out).to_png(),
        }
    })
    .await
    .map_err(internal)?;
    Ok(png(bytes))
}

#[derive(Deserialize)]
struct SlateQuery {
    /// Rendition id (default: the main picture).
    picture: Option<String>,
    /// Width of a smaller copy.
    w: Option<u32>,
}

/// What a picture's outputs show while no project is loaded (the HTML5
/// output shows this very picture). Public like the HTML5 outputs.
async fn channel_slate(
    State(s): State<Shared>,
    MaybeUser(me): MaybeUser,
    Path(id): Path<String>,
    Query(q): Query<SlateQuery>,
) -> ApiResult<Response> {
    match &me {
        Some(me) => visible_channel(me, &id)?,
        None => require(s.config.public_outputs)?,
    }
    let ch = s.channel(&id).ok_or_else(|| not_found("channel"))?;
    let slate = ch
        .config
        .slate_of(q.picture.as_deref().unwrap_or("main"))
        .ok_or_else(|| not_found("picture"))?;
    slate_png(&s, slate, q.w).await
}

#[derive(Deserialize)]
struct SlatePreviewQuery {
    style: SlateStyle,
    #[serde(default)]
    channel: String,
    /// The still picture of the custom style (`/assets/…`).
    #[serde(default)]
    image: String,
    width: u32,
    height: u32,
    /// Frame rate (50/1 by default).
    num: Option<u32>,
    den: Option<u32>,
    /// Width of the picture (320 by default).
    w: Option<u32>,
}

/// A slate for settings not applied yet (the Video streams page's choices).
async fn slate_preview(
    State(s): State<Shared>,
    CurrentUser(_): CurrentUser,
    Query(q): Query<SlatePreviewQuery>,
) -> ApiResult<Response> {
    let slate = Slate {
        style: q.style,
        image: q.image,
        channel: q.channel,
        width: q.width,
        height: q.height,
        rate: (q.num.unwrap_or(50), q.den.unwrap_or(1)),
    };
    slate_png(&s, slate, Some(q.w.unwrap_or(320))).await
}

/// A slate as PNG, at its size or `thumb` wide (at most 640 px).
async fn slate_png(s: &Shared, mut slate: Slate, thumb: Option<u32>) -> ApiResult<Response> {
    slate.width = slate.width.clamp(16, 7680);
    slate.height = slate.height.clamp(16, 7680);
    let (w, h) = (slate.width, slate.height);
    let out = match thumb {
        Some(t) => {
            let t = t.clamp(16, 640).min(w);
            cg_render::OutputFormat {
                width: t,
                height: ((h * t + w / 2) / w).max(1),
            }
        }
        None => cg_render::OutputFormat {
            width: w,
            height: h,
        },
    };
    let st = s.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        let mut r = st.renderer.lock().unwrap_or_else(|e| e.into_inner());
        r.render_idle(&slate, out).to_png()
    })
    .await
    .map_err(internal)?;
    Ok(png(bytes))
}

async fn get_sdp(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path((id, name, essence)): Path<(String, String, String)>,
) -> ApiResult<Response> {
    visible_channel(&me, &id)?;
    let ch = s.channel(&id).ok_or_else(|| not_found("channel"))?;
    let c = &ch.config;
    // By id (or name, as before): unique across the channel's pictures.
    let (width, height, cfg) = c
        .pictures()
        .into_iter()
        .find_map(|(_, w, h, outputs)| {
            outputs
                .st2110
                .iter()
                .find(|o| o.id == name || o.name == name)
                .map(|o| (w, h, o))
        })
        .ok_or_else(|| not_found("output"))?;
    let key = match essence.as_str() {
        "fill.sdp" => false,
        "key.sdp" => true,
        _ => return Err((StatusCode::NOT_FOUND, "use fill.sdp or key.sdp".into())),
    };
    let sdp = cfg
        .sdp(width, height, (c.frame_rate.num, c.frame_rate.den), key)
        .ok_or_else(|| not_found("key stream"))?;
    Ok(([(header::CONTENT_TYPE, "application/sdp")], sdp).into_response())
}

// ----------------------------------------------------------------- fonts ----

/// Fonts available to templates (public: output pages load them too).
async fn list_fonts(State(s): State<Shared>) -> ApiResult<Json<Vec<cg_render::FontFamily>>> {
    let st = s.clone();
    let list = tokio::task::spawn_blocking(move || {
        st.renderer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .font_families()
    })
    .await
    .map_err(internal)?;
    Ok(Json(list))
}

/// The font file the renderer uses, so browsers draw with the same font.
async fn font_file(
    State(s): State<Shared>,
    Path((family, weight)): Path<(String, u16)>,
) -> ApiResult<Response> {
    let st = s.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        st.renderer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .font_file(&family, weight)
    })
    .await
    .map_err(internal)?
    .ok_or_else(|| not_found("font"))?;
    let kind = match bytes.get(..4) {
        Some(b"OTTO") => "font/otf",
        Some(b"wOFF") => "font/woff",
        Some(b"wOF2") => "font/woff2",
        _ => "font/ttf",
    };
    Ok((
        [
            (header::CONTENT_TYPE, kind),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        bytes,
    )
        .into_response())
}

/// Adds a font (.ttf / .otf / .woff / .woff2) to the server.
async fn upload_font(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(name): Path<String>,
    body: Bytes,
) -> ApiResult<Json<serde_json::Value>> {
    require(matches!(me.role, Role::Admin | Role::Designer))?;
    let lower = name.to_lowercase();
    if !valid_asset_name(&name)
        || ![".ttf", ".otf", ".woff", ".woff2", ".ttc"]
            .iter()
            .any(|e| lower.ends_with(e))
    {
        return Err(bad("upload a .ttf, .otf or .woff font"));
    }
    if ttf_magic(&body).is_none() {
        return Err(bad("this is not a font file"));
    }
    let dir = s.assets_dir().join("fonts");
    tokio::fs::create_dir_all(&dir).await.map_err(internal)?;
    tokio::fs::write(dir.join(&name), &body)
        .await
        .map_err(internal)?;
    cg_render::fonts_changed();
    s.thumbs.clear();
    Ok(Json(serde_json::json!({ "name": name })))
}

/// Removes an uploaded font file.
async fn delete_font(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(name): Path<String>,
) -> ApiResult<StatusCode> {
    require(matches!(me.role, Role::Admin | Role::Designer))?;
    if !valid_asset_name(&name) {
        return Err(bad("invalid font name"));
    }
    let path = s.assets_dir().join("fonts").join(&name);
    tokio::fs::remove_file(&path)
        .await
        .map_err(|_| not_found("font"))?;
    cg_render::fonts_changed();
    s.thumbs.clear();
    Ok(StatusCode::NO_CONTENT)
}

fn ttf_magic(b: &[u8]) -> Option<()> {
    matches!(
        b.get(..4)?,
        [0, 1, 0, 0] | b"OTTO" | b"true" | b"ttcf" | b"wOFF" | b"wOF2"
    )
    .then_some(())
}

// ---------------------------------------------------------------- assets ----

pub(crate) fn valid_asset_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() < 128
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

async fn list_assets(
    State(s): State<Shared>,
    CurrentUser(_): CurrentUser,
) -> ApiResult<Json<Vec<String>>> {
    let mut out = Vec::new();
    let mut dir = match tokio::fs::read_dir(s.assets_dir()).await {
        Ok(d) => d,
        Err(_) => return Ok(Json(out)),
    };
    while let Some(e) = dir.next_entry().await.map_err(internal)? {
        if e.file_type().await.map(|t| t.is_file()).unwrap_or(false) {
            out.push(format!("/assets/{}", e.file_name().to_string_lossy()));
        }
    }
    out.sort();
    Ok(Json(out))
}

async fn put_asset(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(name): Path<String>,
    body: Bytes,
) -> ApiResult<Json<BTreeMap<&'static str, String>>> {
    // Operators upload pictures for image fields too.
    require(me.role != Role::Viewer)?;
    if !valid_asset_name(&name) {
        return Err(bad("invalid asset name"));
    }
    tokio::fs::create_dir_all(s.assets_dir())
        .await
        .map_err(internal)?;
    tokio::fs::write(s.assets_dir().join(&name), &body)
        .await
        .map_err(internal)?;
    s.renderer
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .invalidate_assets();
    s.thumbs.clear();
    s.changed("media");
    Ok(Json(BTreeMap::from([("src", format!("/assets/{name}"))])))
}

// ---------------------------------------------------------------- system ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetInterface {
    name: String,
    ip: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemInfo {
    version: &'static str,
    os: &'static str,
    arch: &'static str,
    hostname: String,
    cpus: usize,
    data_dir: String,
    bind: String,
    uptime_secs: u64,
    timezone: String,
    /// Admins only.
    config: Option<ServerConfig>,
    interfaces: Vec<NetInterface>,
}

async fn get_system(State(s): State<Shared>, CurrentUser(me): CurrentUser) -> Json<SystemInfo> {
    let mut interfaces: Vec<NetInterface> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| i.ip().is_ipv4())
        .map(|i| NetInterface {
            ip: i.ip().to_string(),
            name: i.name,
        })
        .collect();
    interfaces.sort_by(|a, b| a.name.cmp(&b.name));
    Json(SystemInfo {
        version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        hostname: gethostname::gethostname().to_string_lossy().into_owned(),
        cpus: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1),
        data_dir: s.data_dir.display().to_string(),
        bind: s.bind.to_string(),
        uptime_secs: s.started_at.elapsed().map(|d| d.as_secs()).unwrap_or(0),
        timezone: server_timezone(&s),
        // Re-read so the loaded projects are current.
        config: me.is_admin().then(|| ServerConfig::load(&s.data_dir)),
        interfaces,
    })
}

/// Saves the settings and restarts the engine (and listener) with them.
async fn put_system_config(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Json(mut cfg): Json<ServerConfig>,
) -> ApiResult<StatusCode> {
    require(me.is_admin())?;
    cfg.normalize();
    cfg.validate().map_err(bad)?;
    // The projects on air are not settings (they change at once): kept as
    // they are now, « no project » included.
    for c in &mut cfg.channels {
        if let Some(ch) = s.channel(&c.id) {
            c.project_id = ch.project_id.read().await.clone();
        }
    }
    if let Some(bind) = cfg.bind.filter(|b| *b != s.bind) {
        // Fail early rather than restarting into an address we cannot use.
        std::net::TcpListener::bind(bind)
            .map_err(|e| bad(format!("cannot listen on {bind}: {e}")))?;
    }
    let dir = s.data_dir.clone();
    let before = ServerConfig::load(&dir);
    let time = (before.time != cfg.time).then(|| cfg.time.clone());
    // Only the time changed: the engine goes on.
    let restart = {
        let mut same = before.clone();
        same.time = cfg.time.clone();
        same != cfg
    };
    tokio::task::spawn_blocking(move || cfg.save(&dir))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    if let Some(time) = time {
        let dir = s.data_dir.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = crate::timectl::apply(&dir, &time, false) {
                tracing::warn!("cannot set the time: {e}");
            }
        });
    }
    s.changed("config");
    if restart {
        tracing::info!("settings saved, restarting the engine");
        s.restart.notify_one();
    }
    Ok(StatusCode::ACCEPTED)
}

/// Whether the time can be set here, and how its last change went (admins).
async fn get_system_time(
    State(_): State<Shared>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Json<crate::timectl::TimeState>> {
    require(me.is_admin())?;
    tokio::task::spawn_blocking(crate::timectl::state)
        .await
        .map(Json)
        .map_err(internal)
}

/// Synchronises the machine's time now (NTP).
async fn sync_system_time(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<StatusCode> {
    require(me.is_admin())?;
    let dir = s.data_dir.clone();
    let time = ServerConfig::load(&dir).time;
    tokio::task::spawn_blocking(move || crate::timectl::apply(&dir, &time, true))
        .await
        .map_err(internal)?
        .map_err(bad)?;
    Ok(StatusCode::ACCEPTED)
}

/// The video hardware this machine has, as far as it can tell (admins).
async fn get_hardware(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Json<crate::hardware::HardwareInfo>> {
    require(me.is_admin())?;
    let mut info = tokio::task::spawn_blocking(crate::hardware::detect)
        .await
        .map_err(internal)?;
    info.sync = Some(s.perf.sync.get());
    info.ndi_restart = crate::hardware::ndi_restart_needed(&s.data_dir);
    Ok(Json(info))
}

/// Whether each video output runs, by channel and output id.
async fn outputs_status(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
) -> Json<BTreeMap<String, BTreeMap<String, crate::state::OutputState>>> {
    let mut all = s.output_status.snapshot();
    all.retain(|ch, _| me.sees_channel(ch));
    Json(all)
}

/// Unauthenticated status for the desktop tray app and monitoring.
async fn health(State(s): State<Shared>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "name": s.config.name,
        "version": env!("CARGO_PKG_VERSION"),
        "bind": s.bind,
        "channels": s.channels.len(),
        "needsSetup": s.auth.needs_setup(),
    }))
}

#[derive(Deserialize)]
struct QrQuery {
    data: String,
    /// Error correction (L, M, Q, H; M by default).
    #[serde(default)]
    ec: Option<cg_core::wid::qrcode::Ecl>,
}

/// QR modules for the HTML5 renderer (public: output pages need it).
async fn qr(Query(q): Query<QrQuery>) -> ApiResult<Json<serde_json::Value>> {
    if q.data.len() > 2048 {
        return Err(bad("too long for a QR code"));
    }
    let (size, modules) = cg_render::qr_modules_ec(&q.data, q.ec.unwrap_or_default())
        .ok_or_else(|| bad("cannot encode"))?;
    let modules: String = modules.iter().map(|d| if *d { '1' } else { '0' }).collect();
    Ok(Json(
        serde_json::json!({ "size": size, "modules": modules }),
    ))
}

/// `/output` without a channel: the first channel (tray app, old links).
async fn output_redirect(State(s): State<Shared>) -> Response {
    match s.channels.keys().next() {
        Some(id) => axum::response::Redirect::temporary(&format!("/output/{id}")).into_response(),
        None => not_found("channel").into_response(),
    }
}

/// Admins, or the tray app on the server itself.
async fn post_restart(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    MaybeUser(me): MaybeUser,
) -> ApiResult<StatusCode> {
    require(peer.ip().is_loopback() || me.is_some_and(|u| u.is_admin()))?;
    s.restart.notify_one();
    Ok(StatusCode::ACCEPTED)
}

/// What the menu bar / tray app shows: status, addresses, network roles.
/// The server itself only (the tray runs on it).
async fn tray_info(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> ApiResult<Json<serde_json::Value>> {
    require(peer.ip().is_loopback())?;
    let port = s.bind.port();
    let interfaces: Vec<serde_json::Value> = tokio::task::spawn_blocking(crate::hardware::detect)
        .await
        .map_err(internal)?
        .interfaces
        .into_iter()
        .filter(|i| !i.loopback && i.ip.parse::<std::net::Ipv4Addr>().is_ok())
        .map(|i| serde_json::json!({ "name": i.name, "ip": i.ip }))
        .collect();
    // Where other machines reach the web UI.
    let urls: Vec<String> = if s.bind.ip().is_unspecified() {
        interfaces
            .iter()
            .filter_map(|i| i["ip"].as_str())
            .map(|ip| format!("http://{ip}:{port}"))
            .collect()
    } else if s.bind.ip().is_loopback() {
        Vec::new()
    } else {
        vec![format!("http://{}:{port}", s.bind.ip())]
    };
    let outputs = s.output_status.snapshot();
    let total = outputs.values().map(|o| o.len()).sum::<usize>();
    let ok = outputs
        .values()
        .flat_map(|o| o.values())
        .filter(|o| o.ok)
        .count();
    let roles = &s.config.hardware.network;
    Ok(Json(serde_json::json!({
        "name": s.config.name,
        "version": env!("CARGO_PKG_VERSION"),
        "bind": s.bind,
        "uptimeSecs": s.started_at.elapsed().map(|d| d.as_secs()).unwrap_or(0),
        "urls": urls,
        "interfaces": interfaces,
        "control": roles.control,
        "ndi": roles.ndi,
        "channels": s.channels.len(),
        "outputs": { "ok": ok, "total": total },
    })))
}

/// Restarts the whole server process (the service manager starts it again):
/// no password from the tray. Admins, or the server itself.
async fn post_restart_server(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    MaybeUser(me): MaybeUser,
) -> ApiResult<StatusCode> {
    require(peer.ip().is_loopback() || me.is_some_and(|u| u.is_admin()))?;
    tracing::info!("restart asked from the menu bar app");
    crate::update::restart_soon();
    Ok(StatusCode::ACCEPTED)
}

/// Stops the server as the service manager would (SIGTERM: a clean exit it
/// does not restart). macOS and Linux; the server itself only.
async fn post_stop(ConnectInfo(peer): ConnectInfo<SocketAddr>) -> ApiResult<StatusCode> {
    require(peer.ip().is_loopback())?;
    #[cfg(unix)]
    {
        tracing::info!("stop asked from the menu bar app");
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(300));
            // SAFETY: signalling our own process; no memory is involved.
            unsafe {
                libc::kill(libc::getpid(), libc::SIGTERM);
            }
        });
        Ok(StatusCode::ACCEPTED)
    }
    #[cfg(not(unix))]
    Err((
        StatusCode::NOT_IMPLEMENTED,
        "stop the Windows service from its tray app".into(),
    ))
}

#[derive(Deserialize)]
struct NetworkRolesInput {
    /// The interface the web UI listens on ("" = every one).
    control: Option<String>,
    /// The interface NDI uses ("" = every one).
    ndi: Option<String>,
}

/// Picks the network adapter of the web UI and of NDI, then restarts the
/// server (NDI reads its network at start). Admins, or the server itself.
async fn put_network_roles(
    State(s): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    MaybeUser(me): MaybeUser,
    Json(i): Json<NetworkRolesInput>,
) -> ApiResult<StatusCode> {
    require(peer.ip().is_loopback() || me.is_some_and(|u| u.is_admin()))?;
    let known = |n: &str| n.is_empty() || crate::hardware::interface_ip(n).is_some();
    for n in [&i.control, &i.ndi].into_iter().flatten() {
        if !known(n) {
            return Err(bad(format!(
                "no network interface `{n}` with an IPv4 address"
            )));
        }
    }
    let dir = s.data_dir.clone();
    tokio::task::spawn_blocking(move || {
        let mut cfg = ServerConfig::load(&dir);
        if let Some(c) = i.control {
            cfg.hardware.network.control = c;
        }
        if let Some(n) = i.ndi {
            cfg.hardware.network.ndi = n;
        }
        cfg.save(&dir)
    })
    .await
    .map_err(internal)?
    .map_err(internal)?;
    tracing::info!("network roles changed from the menu bar app, restarting");
    crate::update::restart_soon();
    Ok(StatusCode::ACCEPTED)
}

// ------------------------------------------------------------------ MOS ----

/// What the HTML5 MOS plugin needs to build items.
async fn mos_info(State(s): State<Shared>, CurrentUser(_): CurrentUser) -> Json<serde_json::Value> {
    Json(
        serde_json::json!({ "mosId": s.config.mos.mos_id, "enabled": s.config.mos.enabled, "projectId": s.config.mos.project_id, "schema": crate::mos::SCHEMA }),
    )
}

async fn mos_status(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Json<crate::mos::Status>> {
    require(me.is_admin())?;
    Ok(Json(crate::mos::status(&s.mos)))
}

// ------------------------------------------------------------------ data ----

use crate::datasources::{Source, SourceInfo};

#[derive(Deserialize, Default)]
struct SourcesQuery {
    /// A project's own connectors and the shared ones.
    #[serde(default)]
    project: Option<String>,
    /// Only the shared connectors.
    #[serde(default)]
    shared: bool,
}

/// Connectors the user can see: shared ones, and those of the projects they see.
async fn list_sources(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Query(q): Query<SourcesQuery>,
) -> Json<Vec<SourceInfo>> {
    Json(
        s.data
            .list()
            .into_iter()
            .filter(|x| match (&x.source.project, &q.project) {
                (None, _) => true,
                (Some(_), _) if q.shared => false,
                (Some(p), Some(want)) => p == want && me.sees_project(p),
                (Some(p), None) => me.sees_project(p),
            })
            .collect(),
    )
}

/// May `me` change this connector: a project's, its designers; a shared one, those who create projects.
fn may_edit_source(me: &crate::auth::User, project: &Option<String>) -> ApiResult<()> {
    match project {
        Some(p) => require(me.sees_project(p) && me.can_design(p)),
        None => require(me.can_create_projects()),
    }
}

fn check_source(src: &mut Source) -> ApiResult<()> {
    src.name = src.name.trim().to_string();
    if src.name.is_empty() {
        return Err(bad("a data source needs a name"));
    }
    src.interval = src.interval.clamp(2, 86_400);
    Ok(())
}

async fn create_source(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Json(mut src): Json<Source>,
) -> ApiResult<Json<SourceInfo>> {
    may_edit_source(&me, &src.project)?;
    if let Some(p) = &src.project {
        if s.project(p).await.is_none() {
            return Err(not_found("project"));
        }
    }
    check_source(&mut src)?;
    if !valid_id(&src.id) || s.data.get(&src.id).is_some() {
        src.id = cg_core::new_id();
    }
    let id = src.id.clone();
    s.data.put(src).map_err(internal)?;
    s.data.refresh(&id);
    s.changed("sources");
    let info = s
        .data
        .list()
        .into_iter()
        .find(|x| x.source.id == id)
        .ok_or_else(|| internal("lost"))?;
    Ok(Json(info))
}

async fn put_source(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    Json(mut src): Json<Source>,
) -> ApiResult<StatusCode> {
    let old = s.data.get(&id).ok_or_else(|| not_found("data source"))?;
    may_edit_source(&me, &old.project)?;
    check_source(&mut src)?;
    // It stays where it was (shared, or its project's).
    src.project = old.project;
    src.id = id.clone();
    s.data.put(src).map_err(internal)?;
    s.data.refresh(&id);
    s.changed("sources");
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_source(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let old = s.data.get(&id).ok_or_else(|| not_found("data source"))?;
    may_edit_source(&me, &old.project)?;
    if !s.data.delete(&id).map_err(internal)? {
        return Err(not_found("data source"));
    }
    s.changed("sources");
    Ok(StatusCode::NO_CONTENT)
}

/// A connector the user may see (a project's: those who see the project).
fn visible_source(s: &Shared, me: &crate::auth::User, id: &str) -> ApiResult<Source> {
    let src = s.data.get(id).ok_or_else(|| not_found("data source"))?;
    match &src.project {
        Some(p) if !me.sees_project(p) => Err(not_found("data source")),
        _ => Ok(src),
    }
}

async fn source_value(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    visible_source(&s, &me, &id)?;
    Ok(Json(s.data.value(&id).unwrap_or(serde_json::Value::Null)))
}

/// Automation sends the value of a push source (JSON, CSV or XML).
async fn push_value(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
    body: Bytes,
) -> ApiResult<StatusCode> {
    require(me.role != Role::Viewer)?;
    let src = visible_source(&s, &me, &id)?;
    let crate::datasources::Kind::Push { format } = src.kind else {
        return Err(bad(
            "this source fetches its data itself; only push sources take values",
        ));
    };
    let v = crate::datasources::parse_push(format, &body).map_err(bad)?;
    let changed = s.data.record(&id, Ok((v, body.len())));
    crate::datasources::updated(&s, &id, changed).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn refresh_source(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    visible_source(&s, &me, &id)?;
    s.data.refresh(&id);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TestResult {
    value: Option<serde_json::Value>,
    bytes: usize,
    error: Option<String>,
}

/// Fetches a source once without saving it (the "Test" button).
async fn test_source(
    CurrentUser(me): CurrentUser,
    Json(src): Json<Source>,
) -> ApiResult<Json<TestResult>> {
    require(me.can_create_projects())?;
    let res = tokio::task::spawn_blocking(move || crate::datasources::fetch(&src))
        .await
        .map_err(internal)?;
    Ok(Json(match res {
        Ok((v, n)) => TestResult {
            value: Some(v),
            bytes: n,
            error: None,
        },
        Err(e) => TestResult {
            value: None,
            bytes: 0,
            error: Some(e),
        },
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LookupQuery {
    source: String,
    path: String,
    #[serde(default)]
    data: cg_core::Data,
    #[serde(default)]
    filters: Vec<cg_core::DataFilter>,
    #[serde(default)]
    sort: Option<cg_core::DataSort>,
}

/// Resolves a binding path against a source's current value.
async fn data_lookup(
    State(s): State<Shared>,
    CurrentUser(_me): CurrentUser,
    Json(q): Json<LookupQuery>,
) -> ApiResult<Json<BTreeMap<&'static str, Option<String>>>> {
    let v = s
        .data
        .value(&q.source)
        .ok_or_else(|| not_found("value for this data source"))?;
    Ok(Json(BTreeMap::from([(
        "value",
        cg_core::datapath::lookup_filtered(
            &q.sort
                .as_ref()
                .map_or_else(|| v.clone(), |srt| cg_core::datapath::sort(&v, srt)),
            &q.path,
            &q.filters,
            &q.data,
        ),
    )])))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PipelineQuery {
    source: String,
    #[serde(default)]
    list: String,
    #[serde(default)]
    steps: Vec<cg_core::DataStep>,
    #[serde(default)]
    data: cg_core::anim::Data,
}

#[derive(Serialize)]
struct Stage {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    /// Rows before the cut to 200.
    total: usize,
}

#[derive(Serialize)]
struct PipelineResult {
    /// The source's table, then the table after each step.
    stages: Vec<Stage>,
    /// What a field fed by the last step reads.
    text: String,
}

/// Runs a table pipeline on a source's current value (Data canvas previews).
async fn data_pipeline(
    State(s): State<Shared>,
    CurrentUser(_me): CurrentUser,
    Json(q): Json<PipelineQuery>,
) -> ApiResult<Json<PipelineResult>> {
    let v = s
        .data
        .value(&q.source)
        .ok_or_else(|| not_found("value for this data source"))?;
    let stages = cg_core::datapath::pipeline(&v, &q.list, &q.steps, &q.data)
        .ok_or_else(|| not_found("list in this data source"))?;
    let text = stages.last().map(|t| t.text()).unwrap_or_default();
    Ok(Json(PipelineResult {
        text,
        stages: stages
            .into_iter()
            .map(|t| Stage {
                total: t.rows.len(),
                rows: t.rows.into_iter().take(200).collect(),
                columns: t.columns,
            })
            .collect(),
    }))
}

/// An operator action tried in the Designer: the layer's state after it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActQuery {
    layer: cg_core::Layer,
    #[serde(default)]
    state: serde_json::Value,
    op: String,
    #[serde(default)]
    args: serde_json::Value,
    #[serde(default)]
    data: cg_core::Data,
    #[serde(default)]
    age_secs: f64,
    /// Frames per second of the channel (50 when not given).
    #[serde(default)]
    fps: f64,
}

async fn widget_act(
    CurrentUser(_me): CurrentUser,
    Json(q): Json<ActQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let ctx = cg_core::wid::ActCtx {
        now_ms: cg_core::playout::now_ms(),
        age_secs: q.age_secs,
        fps: if q.fps >= 1.0 { q.fps } else { 50.0 },
        data: &q.data,
    };
    let mut st = q.state;
    cg_core::wid::act(&q.layer, &mut st, &q.op, &q.args, &ctx)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(Json(st))
}

// --------------------------------------------------------------- update ----

async fn update_info(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Json<crate::update::Info>> {
    require(me.is_admin())?;
    let dir = s.data_dir.clone();
    Ok(Json(
        tokio::task::spawn_blocking(move || crate::update::info(&dir))
            .await
            .map_err(internal)?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateSettingsInput {
    repo: String,
    /// None = keep the stored token, "" = remove it.
    #[serde(default)]
    token: Option<String>,
    #[serde(default = "yes")]
    auto_check: bool,
}

fn yes() -> bool {
    true
}

async fn update_settings(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Json(i): Json<UpdateSettingsInput>,
) -> ApiResult<StatusCode> {
    require(me.is_admin())?;
    let repo = i
        .repo
        .trim()
        .trim_start_matches("https://github.com/")
        .trim_end_matches('/')
        .to_string();
    if repo.split('/').count() != 2 || repo.split('/').any(|p| p.is_empty()) {
        return Err(bad("the repository is written owner/name"));
    }
    let mut cfg = crate::update::Settings::load(&s.data_dir);
    cfg.repo = repo;
    cfg.auto_check = i.auto_check;
    if let Some(t) = i.token {
        cfg.token = t.trim().to_string();
    }
    cfg.save(&s.data_dir).map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_check(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Json<Option<crate::update::Release>>> {
    require(me.is_admin())?;
    let dir = s.data_dir.clone();
    tokio::task::spawn_blocking(move || crate::update::check(&dir))
        .await
        .map_err(internal)?
        .map(Json)
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("{e:#}")))
}

#[derive(Deserialize)]
struct InstallQuery {
    /// A release of the list (older ones included); the latest by default.
    #[serde(default)]
    version: Option<String>,
}

/// Downloads and installs a release in the background; follow progress
/// with GET /api/system/update.
async fn update_install(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Query(q): Query<InstallQuery>,
) -> ApiResult<StatusCode> {
    require(me.is_admin())?;
    tracing::warn!(
        user = me.username,
        version = q.version.as_deref().unwrap_or("latest"),
        "installing a release"
    );
    let dir = s.data_dir.clone();
    tokio::task::spawn_blocking(move || {
        let _ = crate::update::install_release(&dir, q.version.as_deref());
    });
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadQuery {
    /// Install even if it is not newer (downgrade / reinstall).
    #[serde(default)]
    allow_older: bool,
}

/// Installs a release archive uploaded by an admin (offline servers).
async fn update_upload(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    Query(q): Query<UploadQuery>,
    body: Bytes,
) -> ApiResult<Json<serde_json::Value>> {
    require(me.is_admin())?;
    tracing::warn!(
        user = me.username,
        bytes = body.len(),
        "installing an uploaded package"
    );
    let dir = s.data_dir.clone();
    let version = tokio::task::spawn_blocking(move || {
        let r = crate::update::extract_binary(&body)
            .and_then(|b| crate::update::install(&dir, &b, q.allow_older));
        if r.is_ok() {
            crate::update::add_ffmpeg(&body);
        }
        if let Err(e) = &r {
            crate::update::fail(e);
        }
        r
    })
    .await
    .map_err(internal)?
    .map_err(|e| bad(format!("{e:#}")))?;
    crate::update::restart_soon();
    Ok(Json(serde_json::json!({ "version": version })))
}

async fn update_rollback(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
) -> ApiResult<Json<serde_json::Value>> {
    require(me.is_admin())?;
    let dir = s.data_dir.clone();
    let version = tokio::task::spawn_blocking(move || crate::update::rollback(&dir, "rolled back"))
        .await
        .map_err(internal)?
        .map_err(|e| bad(format!("{e:#}")))?;
    crate::update::restart_soon();
    Ok(Json(serde_json::json!({ "version": version })))
}

// ------------------------------------------------------------- websocket ----

#[derive(Deserialize)]
struct WsQuery {
    /// Output page mode: follow this channel only (and its project).
    channel: Option<String>,
}

/// What a socket may see.
struct Viewer {
    user: Option<User>,
    /// Output pages follow one channel.
    channel: Option<String>,
}

impl Viewer {
    fn sees_channel(&self, id: &str) -> bool {
        match (&self.user, &self.channel) {
            (_, Some(c)) => c == id,
            (Some(u), None) => u.sees_channel(id),
            (None, None) => false,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Hello {
    r#type: &'static str,
    user: Option<PublicUser>,
    server_name: String,
    server_time: i64,
    timezone: String,
    channels: Vec<ChannelInfo>,
    projects: Vec<ProjectSummary>,
}

async fn ws_upgrade(
    State(s): State<Shared>,
    MaybeUser(user): MaybeUser,
    Query(q): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    if user.is_none() && !(s.config.public_outputs && q.channel.is_some()) {
        return (StatusCode::UNAUTHORIZED, "login required").into_response();
    }
    if let Some(c) = &q.channel {
        if s.channel(c).is_none() || user.as_ref().is_some_and(|u| !u.sees_channel(c)) {
            return not_found("channel").into_response();
        }
    }
    let viewer = Viewer {
        user,
        channel: q.channel,
    };
    ws.on_upgrade(move |socket| ws_session(s, socket, viewer))
}

async fn ws_session(s: Shared, socket: WebSocket, viewer: Viewer) {
    let socket_id = s.presence.connect();
    ws_session_on(&s, socket, &viewer, socket_id).await;
    // Gone: no longer on any channel.
    if s.presence.set(socket_id, None) {
        let _ = s.events.send(Event::Presence {
            present: s.presence.list(),
        });
        release_locks_later(&s);
    }
}

/// The zones of someone who left the Operator are freed after a while (a
/// page reloaded comes back before).
fn release_locks_later(s: &Shared) {
    let s = s.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        s.release_absent_locks();
    });
}

async fn ws_session_on(s: &Shared, socket: WebSocket, viewer: &Viewer, socket_id: u64) {
    let s = s.clone();
    let (mut tx, mut rx) = socket.split();
    let mut events = s.events.subscribe();
    let closing = crate::state::until_true(s.closing.subscribe());
    tokio::pin!(closing);

    let channels: Vec<ChannelInfo> = s
        .channel_infos()
        .await
        .into_iter()
        .filter(|c| viewer.sees_channel(&c.id))
        .collect();
    let projects = match &viewer.user {
        Some(u) if viewer.channel.is_none() => s
            .summaries()
            .await
            .into_iter()
            .filter(|p| u.sees_project(&p.id))
            .collect(),
        _ => Vec::new(),
    };
    // Output pages get the full project of their channel up front.
    let mut followed: Option<String> = channels
        .first()
        .filter(|_| viewer.channel.is_some())
        .and_then(|c| c.project_id.clone());
    let hello = Hello {
        r#type: "hello",
        user: viewer.user.as_ref().map(PublicUser::from),
        server_name: s.config.name.clone(),
        server_time: now_ms(),
        timezone: server_timezone(&s),
        channels: channels.clone(),
        projects,
    };
    let Ok(json) = serde_json::to_string(&hello) else {
        return;
    };
    if tx.send(Message::Text(json.into())).await.is_err() {
        return;
    }
    if let Some(p) = match &followed {
        Some(id) => s.project(id).await,
        None => None,
    } {
        if send(&mut tx, &Event::Project { project: p }).await.is_err() {
            return;
        }
    }
    for c in &channels {
        if let Some(ch) = s.channel(&c.id) {
            let ev = Event::Channel {
                channel_id: c.id.clone(),
                state: ch.state.read().await.clone(),
                server_time: now_ms(),
            };
            if send(&mut tx, &ev).await.is_err() {
                return;
            }
        }
    }

    loop {
        tokio::select! {
            // Engine restart / shutdown: close so the client reconnects.
            _ = &mut closing => break,
            ev = events.recv() => {
                let sent = match ev {
                    Ok(ev) => ev,
                    // Slow client: events it missed (project, channels, locks) are not sent again,
                    // so it reconnects and gets everything afresh.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => break,
                    Err(_) => break,
                };
                let mut extra = None;
                let ev = &sent.event;
                let project_id = match ev {
                    Event::Project { project } => project.id.as_str(),
                    Event::ProjectTemplate { project_id, .. } => project_id.as_str(),
                    _ => "",
                };
                let visible = match ev {
                    Event::Project { .. } | Event::ProjectTemplate { .. } => match (&viewer.user, &viewer.channel) {
                        (_, Some(_)) => followed.as_deref() == Some(project_id),
                        (Some(u), None) => u.sees_project(project_id),
                        (None, None) => false,
                    },
                    Event::ProjectDeleted { id } => viewer.user.as_ref().is_some_and(|u| u.sees_project(id)),
                    Event::Channel { channel_id, .. } => viewer.sees_channel(channel_id),
                    Event::Presence { .. } => viewer.user.is_some(),
                    Event::ZoneLocks { channel_id, .. } => viewer.user.is_some() && viewer.sees_channel(channel_id),
                    Event::AsRun { entry } => viewer.user.is_some() && viewer.sees_channel(&entry.channel_id),
                    // Signed-in users (designer, operator, settings), not output pages.
                    Event::Data { .. } | Event::Folders { .. } => viewer.user.is_some(),
                    Event::Changed { what } => viewer.user.as_ref().is_some_and(|u| what.strip_prefix("media:").is_none_or(|p| u.sees_project(p))),
                    Event::Lock { project_id, .. } => viewer.user.as_ref().is_some_and(|u| u.sees_project(project_id)),
                    Event::Channels { channels } => {
                        // An output page switches project with its channel.
                        if let Some(c) = viewer.channel.as_ref().and_then(|id| channels.iter().find(|c| &c.id == id)) {
                            if c.project_id != followed {
                                followed = c.project_id.clone();
                                if let Some(id) = &followed {
                                    extra = s.project(id).await.map(|project| Event::Project { project });
                                }
                            }
                        }
                        true
                    }
                    Event::Schedule { channel_id, .. } => viewer.user.is_some() && viewer.sees_channel(channel_id),
                    Event::Social { project_id, .. } => viewer.user.as_ref().is_some_and(|u| u.sees_project(project_id)),
                    Event::Error { .. } => false,
                };
                if !visible {
                    continue;
                }
                // What this viewer may see of a list; None: all of it (the
                // JSON written once for every client is sent).
                let own = match ev {
                    Event::Channels { channels } if !channels.iter().all(|c| viewer.sees_channel(&c.id)) => Some(Event::Channels { channels: channels.iter().filter(|c| viewer.sees_channel(&c.id)).cloned().collect() }),
                    Event::Presence { present } if !present.iter().all(|p| viewer.sees_channel(&p.channel_id)) => Some(Event::Presence { present: present.iter().filter(|p| viewer.sees_channel(&p.channel_id)).cloned().collect() }),
                    Event::Schedule { channel_id, items } => {
                        let sees = |i: &crate::schedule::View| viewer.user.as_ref().is_some_and(|u| u.sees_project(&i.entry.project_id));
                        (!items.iter().all(sees)).then(|| Event::Schedule { channel_id: channel_id.clone(), items: items.iter().filter(|i| sees(i)).cloned().collect() })
                    }
                    _ => None,
                };
                if let Some(x) = extra {
                    if send(&mut tx, &x).await.is_err() { break }
                }
                let sent = match own {
                    Some(own) => send(&mut tx, &own).await,
                    None => match sent.json() {
                        Some(json) => tx.send(Message::Text(json)).await.map_err(|_| ()),
                        None => Ok(()),
                    },
                };
                if sent.is_err() { break }
            },
            msg = rx.next() => match msg {
                Some(Ok(Message::Text(text))) => {
                    let reply = match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(ClientMessage::Command { channel_id, command }) => match &viewer.user {
                            Some(u) if u.can_operate(&channel_id) => s.command(&channel_id, command, &crate::antenna::Actor::user(u)).await.err(),
                            _ => Some("you do not have the rights to operate this channel".into()),
                        },
                        Ok(ClientMessage::Presence { channel_id }) => {
                            let at = viewer.user.as_ref().zip(channel_id.filter(|c| viewer.sees_channel(c))).map(|(u, c)| crate::antenna::Present {
                                user_id: u.id.clone(),
                                user_name: crate::antenna::Actor::user(u).name,
                                channel_id: c,
                            });
                            if s.presence.set(socket_id, at) {
                                let _ = s.events.send(Event::Presence { present: s.presence.list() });
                                release_locks_later(&s);
                            }
                            None
                        }
                        Err(e) => Some(e.to_string()),
                    };
                    if let Some(message) = reply {
                        if send(&mut tx, &Event::Error { message }).await.is_err() { break }
                    }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            },
        }
    }
}

async fn send<S>(tx: &mut S, ev: &Event) -> Result<(), ()>
where
    S: futures_util::Sink<Message> + Unpin,
{
    let json = serde_json::to_string(ev).map_err(|_| ())?;
    tx.send(Message::Text(json.into())).await.map_err(|_| ())
}
