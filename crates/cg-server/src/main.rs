mod ae_import;
mod antenna;
mod api;
mod apiv1;
mod asrun;
mod auth;
mod colors;
mod config;
mod datarules;
mod datasources;
mod decklink;
mod engine;
mod examples;
mod firewall;
mod hardware;
mod health;
mod iconlib;
mod import;
mod inputs;
mod library;
mod locks;
mod logs;
mod media;
mod mos;
mod paths;
mod perf;
mod prefs;
mod premiere;
mod render;
mod rt;
mod schedule;
mod service;
mod social;
mod state;
mod sync;
mod thumbs;
mod timectl;
mod update;
mod web;
mod webimages;
mod webproxy;

use std::collections::BTreeMap;
use std::future::Future;
use std::io::IsTerminal;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use axum::routing::get;
use clap::{Args, Parser, Subcommand};
use tokio::sync::{watch, Notify, RwLock};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use crate::state::{AppState, ChannelRt, Shared};

#[derive(Parser, Debug)]
#[command(
    name = "strata-server",
    version,
    about = "Strata — broadcast character generator server",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
    #[command(flatten)]
    run: RunArgs,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run the server in the foreground (default).
    Run(RunArgs),
    /// Manage the background service (systemd, launchd or Windows service).
    #[command(subcommand)]
    Service(ServiceCmd),
    /// Manage user accounts (works while the server runs).
    #[command(subcommand)]
    User(UserCmd),
}

#[derive(Subcommand, Debug)]
enum UserCmd {
    /// Create a user (prompts for the password).
    Add {
        username: String,
        #[arg(long, value_enum, default_value = "admin")]
        role: CliRole,
        #[arg(long, env = "CG_DATA_DIR")]
        data_dir: Option<PathBuf>,
    },
    /// List users.
    List {
        #[arg(long, env = "CG_DATA_DIR")]
        data_dir: Option<PathBuf>,
    },
    /// Change a user's password (prompts for it).
    Passwd {
        username: String,
        #[arg(long, env = "CG_DATA_DIR")]
        data_dir: Option<PathBuf>,
    },
    /// Delete a user.
    Delete {
        username: String,
        #[arg(long, env = "CG_DATA_DIR")]
        data_dir: Option<PathBuf>,
    },
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum CliRole {
    Admin,
    Designer,
    Operator,
    Viewer,
}

impl From<CliRole> for auth::Role {
    fn from(r: CliRole) -> Self {
        match r {
            CliRole::Admin => auth::Role::Admin,
            CliRole::Designer => auth::Role::Designer,
            CliRole::Operator => auth::Role::Operator,
            CliRole::Viewer => auth::Role::Viewer,
        }
    }
}

/// The data directory a CLI command acts on: explicit, else the installed
/// service's when it exists, else the per-user one.
fn cli_data_dir(explicit: Option<PathBuf>) -> PathBuf {
    explicit.or_else(service::data_dir).unwrap_or_else(|| {
        let system = paths::system_data_dir();
        if system.is_dir() {
            system
        } else {
            paths::user_data_dir()
        }
    })
}

/// Run with sudo, a command writes the users file as root: it goes to the
/// owner of the data folder (the service account), which must read it.
fn give_to_service(dir: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Ok(m) = std::fs::metadata(dir) {
            let _ = std::os::unix::fs::chown(dir.join("users.json"), Some(m.uid()), Some(m.gid()));
        }
    }
    #[cfg(not(unix))]
    let _ = dir;
}

fn prompt_password() -> anyhow::Result<String> {
    let pw = rpassword::prompt_password("Password: ")?;
    auth::validate_password(&pw).map_err(|e| anyhow::anyhow!(e))?;
    if rpassword::prompt_password("Repeat:   ")? != pw {
        anyhow::bail!("passwords do not match");
    }
    Ok(pw)
}

fn user_command(cmd: UserCmd) -> anyhow::Result<()> {
    let err = |e: String| anyhow::anyhow!(e);
    match cmd {
        UserCmd::Add {
            username,
            role,
            data_dir,
        } => {
            let dir = cli_data_dir(data_dir);
            std::fs::create_dir_all(&dir)?;
            let password = prompt_password()?;
            let a = auth::Auth::load(&dir);
            let input = auth::UserInput {
                username,
                display_name: String::new(),
                role: role.into(),
                projects: None,
                channels: None,
                disabled: false,
                password,
            };
            let u = a.create(input).map_err(err)?;
            give_to_service(&dir);
            println!("User `{}` created in {}.", u.username, dir.display());
        }
        UserCmd::List { data_dir } => {
            for u in auth::Auth::load(&cli_data_dir(data_dir)).users() {
                println!(
                    "{:<24} {:<10} {}{}",
                    u.username,
                    format!("{:?}", u.role).to_lowercase(),
                    u.display_name,
                    if u.disabled { " (disabled)" } else { "" }
                );
            }
        }
        UserCmd::Passwd { username, data_dir } => {
            let dir = cli_data_dir(data_dir);
            let a = auth::Auth::load(&dir);
            let pw = prompt_password()?;
            a.set_password(&username, &pw).map_err(err)?;
            give_to_service(&dir);
            println!("Password changed.");
        }
        UserCmd::Delete { username, data_dir } => {
            let dir = cli_data_dir(data_dir);
            let a = auth::Auth::load(&dir);
            let u = a
                .users()
                .into_iter()
                .find(|u| u.username.eq_ignore_ascii_case(&username))
                .ok_or_else(|| anyhow::anyhow!("no such user"))?;
            a.delete(&u.id).map_err(err)?;
            give_to_service(&dir);
            println!("User `{}` deleted.", u.username);
        }
    }
    Ok(())
}

#[derive(Args, Debug, Clone)]
pub struct RunArgs {
    /// Address to listen on, overriding the saved settings for this run
    /// (services use the settings page instead).
    #[arg(long, env = "CG_BIND")]
    bind: Option<SocketAddr>,
    /// Address used when the saved settings do not set one (installers).
    #[arg(long, hide = true)]
    default_bind: Option<SocketAddr>,
    /// Directory holding projects/, config.json, users.json and assets/
    /// [default: per-user application data directory].
    #[arg(long, env = "CG_DATA_DIR")]
    data_dir: Option<PathBuf>,
    /// Serve the web app from this folder instead of the embedded copy
    /// (useful while developing the UI).
    #[arg(long, env = "CG_WEB_DIR")]
    web_dir: Option<PathBuf>,
    /// Started by the Windows Service Control Manager.
    #[arg(long, hide = true)]
    service: bool,
}

#[derive(Subcommand, Debug)]
enum ServiceCmd {
    /// Register this executable as a system service and start it.
    Install {
        #[arg(long, default_value = "0.0.0.0:8080")]
        bind: SocketAddr,
        /// [default: /var/lib/strata-graphics, /Library/Application Support/Strata Graphics
        /// or %ProgramData%\Strata Graphics]
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
    /// Stop and remove the service (data is kept).
    Uninstall,
    Start,
    Stop,
    Status,
}

fn init_logging(log_file: Option<PathBuf>) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        // tiny-skia warns of every empty shape it is asked to fill (a shape
        // scaled from zero, ten times a second while it animates): not news.
        .unwrap_or_else(|_| "info,tower_http=warn,tiny_skia=error".into());
    let path = log_file.clone();
    let file = log_file.and_then(|p| {
        std::fs::create_dir_all(p.parent()?).ok()?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
            .ok()
    });
    if let (Some(_), Some(p)) = (&file, path) {
        let _ = logs::FILE.set(p);
    }
    use tracing_subscriber::prelude::*;
    let (to_file, to_stdout) = match file {
        Some(f) => (
            Some(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(Mutex::new(f)),
            ),
            None,
        ),
        None => (
            None,
            Some(tracing_subscriber::fmt::layer().with_ansi(std::io::stdout().is_terminal())),
        ),
    };
    tracing_subscriber::registry()
        .with(filter)
        .with(to_file)
        .with(to_stdout)
        // The recent log, for the Performance page.
        .with(logs::Ring)
        .init();
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Cmd::Service(cmd)) => {
            let result = match cmd {
                ServiceCmd::Install { bind, data_dir } => {
                    let data_dir = data_dir.unwrap_or_else(paths::system_data_dir);
                    paths::adopt_former_data(&data_dir);
                    service::install(&service::InstallOptions {
                        bind,
                        data_dir: data_dir.clone(),
                    })
                    .map(|()| {
                        println!("Service `{}` installed and started.", paths::SERVICE_NAME);
                        println!("  Data:  {}", data_dir.display());
                        println!("  Open:  http://localhost:{}/designer", bind.port());
                    })
                }
                ServiceCmd::Uninstall => {
                    service::uninstall().map(|()| println!("Service removed (data kept)."))
                }
                ServiceCmd::Start => service::start(),
                ServiceCmd::Stop => service::stop(),
                ServiceCmd::Status => service::status(),
            };
            if let Err(e) = result {
                eprintln!("error: {e:#}");
                std::process::exit(1);
            }
            Ok(())
        }
        Some(Cmd::User(cmd)) => {
            if let Err(e) = user_command(cmd) {
                eprintln!("error: {e:#}");
                std::process::exit(1);
            }
            Ok(())
        }
        Some(Cmd::Run(args)) => run(args),
        None => run(cli.run),
    }
}

fn run(args: RunArgs) -> anyhow::Result<()> {
    update::set_supervised(args.service);
    let default_dir = if args.service {
        paths::system_data_dir
    } else {
        paths::user_data_dir
    };
    paths::adopt_former_data(&args.data_dir.clone().unwrap_or_else(default_dir));
    if args.service {
        // Windows services have no console: log next to the data.
        let data = args.data_dir.clone().unwrap_or_else(paths::system_data_dir);
        init_logging(Some(data.join("logs").join("strata-server.log")));
        #[cfg(windows)]
        return service::windows_runtime::run(args);
        #[cfg(not(windows))]
        anyhow::bail!("--service is only meaningful on Windows");
    }
    init_logging(None);
    tokio::runtime::Runtime::new()?.block_on(serve(args, shutdown_signal()))
}

/// Ctrl-C everywhere, plus SIGTERM on Unix (systemd / launchd stop).
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    ctrl_c.await;
}

/// The NDI runtime is loaded (it reads its settings only then).
fn ndi_started() -> bool {
    cg_output::ndi::loaded_path().is_some()
}

pub const DEFAULT_BIND: &str = "0.0.0.0:8080";

/// After an update the previous process may still hold the port briefly.
async fn bind_retry(addr: SocketAddr) -> std::io::Result<tokio::net::TcpListener> {
    let mut attempt = 0;
    loop {
        match tokio::net::TcpListener::bind(addr).await {
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse && attempt < 20 => {
                attempt += 1;
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            r => return r,
        }
    }
}

pub async fn serve(
    args: RunArgs,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    let data_dir = args.data_dir.clone().unwrap_or_else(paths::user_data_dir);
    std::fs::create_dir_all(data_dir.join("assets"))?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), data_dir = %data_dir.display(), "starting Strata");
    // The journal on disk, crash reports, the last run's unexpected stop.
    health::start(&data_dir);

    let (stop_tx, stop_rx) = watch::channel(false);
    tokio::spawn(async move {
        shutdown.await;
        let _ = stop_tx.send(true);
    });
    let started_at = SystemTime::now();
    let fallback: SocketAddr = DEFAULT_BIND.parse()?;
    // A fresh update that keeps failing is rolled back here.
    update::startup_guard(&data_dir);
    update::confirm_after(data_dir.clone(), std::time::Duration::from_secs(60));
    update::spawn_auto_check(data_dir.clone());
    // Sessions survive engine restarts.
    let auth = Arc::new(auth::Auth::load(&data_dir));
    let mos_store = Arc::new(mos::Store::load(&data_dir));
    let data_hub = Arc::new(datasources::Hub::load(&data_dir));
    let social_hub = Arc::new(social::Hub::load(&data_dir));
    let moderation = Arc::new(social::moderation::Desk::load(&data_dir));
    let locks = Arc::new(locks::Locks::default());
    let output_status = Arc::new(state::OutputStatus::default());
    // Live inputs keep running across engine restarts.
    let inputs = Arc::new(inputs::Inputs::default());
    let perf = Arc::new(perf::Perf::default());
    perf::spawn_sampler(perf.clone(), inputs.clone());
    if auth.needs_setup() {
        tracing::warn!("no user yet: open http://localhost:<port>/ on this machine to create the admin account, or run `strata-server user add <name>`");
    }

    // The NDI runtime reads its settings (network roles) from here.
    if std::env::var_os("NDI_CONFIG_DIR").is_none() {
        std::env::set_var("NDI_CONFIG_DIR", hardware::ndi_config_dir(&data_dir));
    }

    // Each iteration is one engine "cycle"; settings changes end the cycle
    // and the loop rebuilds everything with the new configuration.
    loop {
        let mut config = config::ServerConfig::load(&data_dir);
        // The DeckLink cards as the driver lists them now: they take the
        // profile of their mode first, then adopted cards follow their
        // Desktop Video devices.
        let hw = config.hardware.clone();
        let (cards, _, _) = tokio::task::spawn_blocking(move || {
            let found = decklink::detect();
            if !found.2 && decklink::apply_profiles(&hw, &found.0) {
                return decklink::detect();
            }
            found
        })
        .await
        .unwrap_or_default();
        if config.hardware.link_detected(&cards) {
            config.normalize();
            if let Err(e) = config.save(&data_dir) {
                tracing::warn!("cannot save the DeckLink cards: {e}");
            }
        }
        perf.sync.set_domain(config.hardware.sync.ptp_domain);
        if hardware::write_ndi_settings(&data_dir, &config.hardware.network) && ndi_started() {
            tracing::warn!("NDI network settings changed: they apply when the server restarts");
        }
        let mut wanted = args
            .bind
            .or(config.bind)
            .or(args.default_bind)
            .unwrap_or(fallback);
        // The control network role: the interface the web UI and the API
        // are reached on (unless an address was given on the command line).
        let control = &config.hardware.network.control;
        let mut loopback = None;
        if args.bind.is_none() && wanted.ip().is_unspecified() && !control.is_empty() {
            match hardware::interface_ip(control) {
                Some(ip) => {
                    wanted.set_ip(ip.into());
                    loopback = Some(SocketAddr::from(([127, 0, 0, 1], wanted.port())));
                }
                None => tracing::warn!(
                    "control interface `{control}` has no IPv4 address: listening on every interface"
                ),
            }
        }
        let listener = match bind_retry(wanted).await {
            Ok(l) => l,
            // Keep the UI reachable so the setting can be fixed.
            Err(e) if wanted != fallback => {
                tracing::error!("cannot listen on {wanted} ({e}); falling back to {fallback}");
                tokio::net::TcpListener::bind(fallback).await?
            }
            Err(e) => return Err(e.into()),
        };
        let bind = listener.local_addr()?;
        // The machine itself always gets in.
        let local = match loopback.filter(|_| bind.ip() != fallback.ip()) {
            Some(addr) => match bind_retry(addr).await {
                Ok(l) => Some(l),
                Err(e) => {
                    tracing::warn!("cannot listen on {addr}: {e}");
                    None
                }
            },
            None => None,
        };
        // Lets the desktop app find the server without parsing settings.
        let runtime = serde_json::json!({ "bind": bind, "version": env!("CARGO_PKG_VERSION"), "pid": std::process::id() });
        if let Err(e) = std::fs::write(data_dir.join("runtime.json"), runtime.to_string()) {
            tracing::warn!("cannot write runtime.json: {e}");
        }

        let library = library::Library::load(&data_dir)?;
        let mut channels = BTreeMap::new();
        for c in &config.channels {
            // None: no project, on purpose; a deleted one unloads.
            let pid = c.project_id.clone().filter(|id| library.get(id).is_some());
            channels.insert(
                c.id.clone(),
                Arc::new(ChannelRt {
                    config: c.clone(),
                    // What it had on air before the restart.
                    state: RwLock::new(state::load_on_air(&data_dir, &c.id, pid.as_deref())),
                    loaded: state::Loaded::new(pid.as_deref().and_then(|id| library.get(id))),
                    project_id: RwLock::new(pid),
                    published: Default::default(),
                    dirty: Default::default(),
                    ops: Default::default(),
                }),
            );
        }
        let events = state::Events::new(256);
        let (closing, _) = watch::channel(false);
        let shared: Shared = Arc::new(AppState {
            data_dir: data_dir.clone(),
            library: RwLock::new(library),
            channels,
            events,
            renderer: Mutex::new(cg_render::Renderer::new(data_dir.join("assets"))),
            thumbs: Default::default(),
            saving: Default::default(),
            auth: auth.clone(),
            mos: mos_store.clone(),
            data: data_hub.clone(),
            social: social_hub.clone(),
            moderation: moderation.clone(),
            locks: locks.clone(),
            edits: Default::default(),
            output_status: output_status.clone(),
            inputs: inputs.clone(),
            perf: perf.clone(),
            config,
            bind,
            started_at,
            restart: Notify::new(),
            closing,
            presence: Default::default(),
            schedule: schedule::Schedule::load(&data_dir),
        });
        shared.inputs.apply(&shared.config.inputs);
        shared
            .renderer
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .set_live(shared.inputs.clone());
        let channels = engine::start(&shared);
        let engine = vec![
            tokio::spawn(mos::serve(shared.clone())),
            tokio::spawn(datasources::run(shared.clone())),
            tokio::spawn(social::run::run(shared.clone())),
            tokio::spawn(social::moderation::run(shared.clone())),
            // OSC and TCP control (Bitfocus Buttons…), when enabled.
            tokio::spawn(apiv1::serve(shared.clone())),
            tokio::spawn(schedule::run(shared.clone())),
            // The picture library's updates, looked for once a week.
            tokio::spawn(iconlib::run(shared.clone())),
            tokio::spawn(engine::give_back_memory()),
        ];

        let app = api::router()
            .route("/fonts/{name}", get(web::font))
            .merge(premiere::router())
            .nest_service("/assets", ServeDir::new(data_dir.join("assets")))
            // HLS stream outputs (index.m3u8 and its segments).
            .nest_service("/hls", ServeDir::new(data_dir.join("hls")));
        let app = match &args.web_dir {
            Some(dir) => {
                let _ = web::WEB_DIR.set(dir.clone());
                let index = dir.join("index.html");
                app.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(index)))
            }
            None => app.fallback(web::embedded),
        };
        let app = app
            // JSON and text gzipped for clients that take it (web.rs).
            .layer(axum::middleware::from_fn(web::compress))
            .layer(CorsLayer::permissive())
            .with_state(shared.clone());

        tracing::info!("listening on http://{bind} — designer: /designer, operator: /operator, settings: /settings, HTML5 output: /output/<channel>");
        let stop = state::until_true(stop_rx.clone());
        let cycle = shared.clone();
        let loopback = local.map(|l| {
            let mut closing = shared.closing.subscribe();
            let app = app.clone();
            tokio::spawn(async move {
                axum::serve(l, app.into_make_service_with_connect_info::<SocketAddr>())
                    .with_graceful_shutdown(async move {
                        let _ = closing.wait_for(|c| *c).await;
                    })
                    .await
            })
        });
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = stop => {}
                _ = cycle.restart.notified() => {}
            }
            let _ = cycle.closing.send(true);
        })
        .await?;
        if let Some(h) = loopback {
            let _ = h.await;
        }
        // Channels stop by themselves once `closing` is set, closing their
        // outputs (ports and NDI names the next cycle may open again).
        for h in channels {
            let abort = h.abort_handle();
            if tokio::time::timeout(std::time::Duration::from_secs(5), h)
                .await
                .is_err()
            {
                tracing::warn!("a channel took too long to stop");
                abort.abort();
            }
        }
        for h in &engine {
            h.abort();
        }
        for h in engine {
            let _ = h.await;
        }
        if *stop_rx.borrow() {
            break;
        }
        tracing::info!("restarting the engine with the new settings");
    }
    // Their ffmpeg stops with them (the load sampler still holds them).
    inputs.stop();
    tracing::info!("stopped");
    health::stopped();
    Ok(())
}
