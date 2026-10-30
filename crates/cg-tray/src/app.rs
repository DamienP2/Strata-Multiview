use std::io::Write as _;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Loopback port used only as a single-instance lock.
const INSTANCE_LOCK: &str = "127.0.0.1:47819";
const DEFAULT_PORT: u16 = 8080;

#[derive(Debug, Clone, PartialEq, Default)]
struct Status {
    running: bool,
    name: String,
    port: u16,
    version: String,
    uptime_secs: u64,
    /// Where other machines reach the web UI.
    urls: Vec<String>,
    /// Network adapters with an IPv4 address: (name, address).
    interfaces: Vec<(String, String)>,
    /// The adapter of the web UI and of NDI ("" = every one).
    control: String,
    ndi: String,
    outputs_ok: usize,
    outputs_total: usize,
}

enum UserEvent {
    Menu(MenuEvent),
    Status(Status),
}

// -------------------------------------------------------------- language ----

/// The menu's words, in the language of the system (French or English).
struct Words {
    checking: &'static str,
    running: &'static str,
    stopped: &'static str,
    version_port: &'static str,
    since: &'static str,
    outputs: &'static str,
    no_outputs: &'static str,
    copy_address: &'static str,
    open_home: &'static str,
    network: &'static str,
    web_ui: &'static str,
    every_adapter: &'static str,
    start: &'static str,
    stop: &'static str,
    restart: &'static str,
    log: &'static str,
    uninstall: &'static str,
    quit: &'static str,
    cannot: &'static str,
    adapter_failed: &'static str,
}

const FR: Words = Words {
    checking: "Strata Graphics — vérification…",
    running: "● Strata Graphics — en marche",
    stopped: "○ Strata Graphics — arrêté",
    version_port: "Version {v} · port {p}",
    since: "en marche depuis {t}",
    outputs: "Sorties : {ok} sur {n} en marche",
    no_outputs: "Aucune sortie vidéo",
    copy_address: "Copier l’adresse  {u}",
    open_home: "Ouvrir la page web",
    network: "Carte réseau",
    web_ui: "Interface web",
    every_adapter: "Toutes les cartes",
    start: "Démarrer le serveur",
    stop: "Arrêter le serveur",
    restart: "Redémarrer le serveur",
    log: "Journal du serveur…",
    uninstall: "Désinstaller Strata Graphics…",
    quit: "Quitter",
    cannot: "Impossible de {a} le serveur Strata Graphics.",
    adapter_failed: "Impossible de changer la carte réseau (mettez le serveur à jour).",
};

const EN: Words = Words {
    checking: "Strata Graphics — checking…",
    running: "● Strata Graphics — running",
    stopped: "○ Strata Graphics — stopped",
    version_port: "Version {v} · port {p}",
    since: "running for {t}",
    outputs: "Outputs: {ok} of {n} running",
    no_outputs: "No video output",
    copy_address: "Copy address  {u}",
    open_home: "Open Web Page",
    network: "Network Adapter",
    web_ui: "Web interface",
    every_adapter: "Every adapter",
    start: "Start Server",
    stop: "Stop Server",
    restart: "Restart Server",
    log: "Server Log…",
    uninstall: "Uninstall Strata Graphics…",
    quit: "Quit",
    cannot: "Could not {a} the Strata Graphics server.",
    adapter_failed: "Could not change the network adapter (update the server).",
};

fn system_is_french() -> bool {
    #[cfg(target_os = "macos")]
    let lang = Command::new("defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .ok()
        .and_then(|o| {
            let text = String::from_utf8_lossy(&o.stdout).into_owned();
            text.split('"').nth(1).map(str::to_string)
        });
    #[cfg(windows)]
    let lang = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-UICulture).TwoLetterISOLanguageName",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    lang.or_else(|| std::env::var("LANG").ok())
        .is_some_and(|l| l.to_lowercase().starts_with("fr"))
}

fn duration_text(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs / 3600 % 24, secs / 60 % 60);
    if d > 0 {
        format!("{d} j {h} h")
    } else if h > 0 {
        format!("{h} h {m:02}")
    } else {
        format!("{m} min")
    }
}

// ------------------------------------------------------------ locations ----

fn data_dir() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
            .join("Strata Graphics")
    } else {
        PathBuf::from("/Library/Application Support/Strata Graphics")
    }
}

fn server_exe() -> PathBuf {
    if cfg!(windows) {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("strata-server.exe")))
            .unwrap_or_else(|| PathBuf::from("strata-server.exe"))
    } else {
        PathBuf::from("/usr/local/strata/strata-server")
    }
}

fn log_file() -> PathBuf {
    if cfg!(windows) {
        data_dir().join("logs").join("strata-server.log")
    } else {
        PathBuf::from("/Library/Logs/Strata Graphics/strata-server.log")
    }
}

fn port_from(json: &serde_json::Value, key: &str) -> Option<u16> {
    json.get(key)?.as_str()?.rsplit(':').next()?.parse().ok()
}

/// The server publishes its live address in `runtime.json`; the saved
/// settings are the fallback when it is not running.
fn discover_port() -> u16 {
    let read = |f: &str| {
        std::fs::read(data_dir().join(f))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
    };
    read("runtime.json")
        .and_then(|j| port_from(&j, "bind"))
        .or_else(|| read("config.json").and_then(|j| port_from(&j, "bind")))
        .unwrap_or(DEFAULT_PORT)
}

fn base_url(port: u16) -> String {
    format!("http://localhost:{port}")
}

// -------------------------------------------------------------- actions ----

fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let r = Command::new("open").arg(url).spawn();
    #[cfg(windows)]
    let r = Command::new("explorer.exe").arg(url).spawn();
    if let Err(e) = r {
        eprintln!("cannot open {url}: {e}");
    }
}

fn open_log() {
    let path = log_file();
    #[cfg(target_os = "macos")]
    let r = Command::new("open")
        .args(["-a", "Console"])
        .arg(&path)
        .spawn();
    #[cfg(windows)]
    let r = Command::new("notepad.exe").arg(&path).spawn();
    if let Err(e) = r {
        eprintln!("cannot open {}: {e}", path.display());
    }
}

fn copy_to_clipboard(text: &str) {
    #[cfg(target_os = "macos")]
    let child = Command::new("pbcopy").stdin(Stdio::piped()).spawn();
    #[cfg(windows)]
    let child = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        Command::new("clip")
            .stdin(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
    };
    if let Ok(mut c) = child {
        if let Some(mut stdin) = c.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = c.wait();
    }
}

/// Shows a native error dialog (the tray has no window of its own).
fn alert(message: &str) {
    #[cfg(target_os = "macos")]
    {
        let esc = |t: &str| t.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            "display alert \"Strata Graphics\" message \"{}\" as critical",
            esc(message)
        );
        let _ = Command::new("osascript").args(["-e", &script]).spawn();
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let ps = format!(
            "Add-Type -AssemblyName PresentationFramework; [System.Windows.MessageBox]::Show('{}', 'Strata Graphics', 'OK', 'Error')",
            message.replace('\'', "''")
        );
        let _ = Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_millis(1500)))
        .build()
        .into()
}

/// A request to the server on this machine (it trusts its own loopback).
fn ask(method: &str, port: u16, path: &str, json: Option<String>) -> bool {
    let url = format!("{}{path}", base_url(port));
    let a = agent();
    let r = match (method, json) {
        ("PUT", Some(body)) => a
            .put(&url)
            .header("Content-Type", "application/json")
            .send(body),
        _ => a.post(&url).send_empty(),
    };
    r.is_ok_and(|r| r.status().is_success())
}

/// Runs `strata-server service <action>` with administrator rights: without a
/// password when the installer allowed it (sudoers), else the system prompt.
fn service_as_admin(action: &'static str) -> Result<(), String> {
    let exe = server_exe();
    #[cfg(target_os = "macos")]
    {
        let quiet = Command::new("sudo")
            .arg("-n")
            .arg(&exe)
            .args(["service", action])
            .output();
        if quiet.is_ok_and(|o| o.status.success()) {
            return Ok(());
        }
        let cmd = format!("'{}' service {action} 2>&1", exe.display());
        let script = format!(
            "do shell script \"{}\" with administrator privileges",
            cmd.replace('\\', "\\\\").replace('"', "\\\"")
        );
        match Command::new("osascript").args(["-e", &script]).output() {
            Ok(o) if o.status.success() => Ok(()),
            // -128: the user cancelled the authentication dialog.
            Ok(o) if String::from_utf8_lossy(&o.stderr).contains("-128") => Ok(()),
            Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
            Err(e) => Err(e.to_string()),
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let ps = format!(
            "$p = Start-Process -Verb RunAs -WindowStyle Hidden -Wait -PassThru -FilePath '{}' -ArgumentList 'service','{action}'; exit $p.ExitCode",
            exe.display().to_string().replace('\'', "''")
        );
        match Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
        {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(format!(
                "strata-server service {action} exited with {s}. See {}",
                log_file().display()
            )),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[derive(Clone, Copy)]
enum Action {
    Start,
    Stop,
    Restart,
}

/// Start, stop or restart the server. Stop and restart ask the running
/// server itself (no password); start needs the service manager.
fn server_action(action: Action, port: u16, w: &'static Words, refresh: EventLoopProxy<UserEvent>) {
    std::thread::spawn(move || {
        let result = match action {
            Action::Restart => {
                if ask("POST", port, "/api/system/restart-server", None) {
                    Ok(())
                } else {
                    service_as_admin("stop").and_then(|()| service_as_admin("start"))
                }
            }
            Action::Stop => {
                if cfg!(unix) && ask("POST", port, "/api/system/stop", None) {
                    Ok(())
                } else {
                    service_as_admin("stop")
                }
            }
            Action::Start => service_as_admin("start"),
        };
        if let Err(e) = result {
            let what = match (action, std::ptr::eq(w, &FR)) {
                (Action::Start, true) => "démarrer",
                (Action::Stop, true) => "arrêter",
                (Action::Restart, true) => "redémarrer",
                (Action::Start, false) => "start",
                (Action::Stop, false) => "stop",
                (Action::Restart, false) => "restart",
            };
            alert(&format!("{}\n\n{e}", w.cannot.replace("{a}", what)));
        }
        let _ = refresh.send_event(UserEvent::Status(read_status()));
    });
}

/// Opens the platform uninstaller: our uninstaller app on macOS, the
/// "Installed apps" settings page on Windows (the MSI owns uninstall there).
fn uninstall() {
    #[cfg(target_os = "macos")]
    let r = Command::new("open")
        .arg("/Applications/Strata Graphics/Uninstall Strata Graphics.app")
        .spawn();
    #[cfg(windows)]
    let r = Command::new("explorer.exe")
        .arg("ms-settings:appsfeatures")
        .spawn();
    if let Err(e) = r {
        alert(&format!("Cannot open the uninstaller: {e}"));
    }
}

/// What the server says about itself (older servers: their health only).
fn read_status() -> Status {
    let port = discover_port();
    let get = |path: &str| {
        agent()
            .get(format!("{}{path}", base_url(port)))
            .call()
            .ok()
            .and_then(|mut r| r.body_mut().read_to_string().ok())
            .and_then(|b| serde_json::from_str::<serde_json::Value>(&b).ok())
    };
    let Some(j) = get("/api/system/tray").or_else(|| get("/api/health")) else {
        return Status {
            running: false,
            name: "Strata Graphics".into(),
            port,
            ..Status::default()
        };
    };
    let text = |k: &str| j.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    Status {
        running: true,
        name: Some(text("name"))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Strata Graphics".into()),
        port: port_from(&j, "bind").unwrap_or(port),
        version: text("version"),
        uptime_secs: j.get("uptimeSecs").and_then(|v| v.as_u64()).unwrap_or(0),
        urls: j
            .get("urls")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|u| u.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        interfaces: j
            .get("interfaces")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|i| {
                        Some((
                            i.get("name")?.as_str()?.to_string(),
                            i.get("ip")?.as_str()?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default(),
        control: text("control"),
        ndi: text("ndi"),
        outputs_ok: j
            .pointer("/outputs/ok")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
        outputs_total: j
            .pointer("/outputs/total")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
    }
}

fn poll(proxy: EventLoopProxy<UserEvent>) {
    std::thread::spawn(move || loop {
        if proxy.send_event(UserEvent::Status(read_status())).is_err() {
            return;
        }
        std::thread::sleep(Duration::from_secs(2));
    });
}

// ----------------------------------------------------------------- icon ----

fn icon() -> Icon {
    #[cfg(target_os = "macos")]
    let (rgba, size) = (
        include_bytes!("../../../assets/icons/tray-template-44.rgba").to_vec(),
        44,
    );
    #[cfg(windows)]
    let (rgba, size) = (
        include_bytes!("../../../assets/icons/tray-color-64.rgba").to_vec(),
        64,
    );
    Icon::from_rgba(rgba, size, size).expect("valid tray icon")
}

// ------------------------------------------------------------------ app ----

/// The adapters and the chosen web and NDI ones.
type NetworkView = (Vec<(String, String)>, String, String);

/// Which role an adapter choice sets.
#[derive(Clone, Copy, PartialEq)]
enum Role {
    Control,
    Ndi,
}

struct Items {
    w: &'static Words,
    status: MenuItem,
    version: MenuItem,
    outputs: MenuItem,
    address: MenuItem,
    home: MenuItem,
    network: Submenu,
    /// The adapter choices now in the submenu: item, role, adapter name.
    choices: Vec<(CheckMenuItem, Role, String)>,
    /// What the submenu shows (adapters, chosen ones), to rebuild it on change only.
    shown_network: Option<NetworkView>,
    start_stop: MenuItem,
    restart: MenuItem,
    log: MenuItem,
    uninstall: MenuItem,
    quit: MenuItem,
}

impl Items {
    fn new(w: &'static Words) -> (Menu, Self) {
        let items = Self {
            w,
            status: MenuItem::new(w.checking, false, None),
            version: MenuItem::new(format!("Version {VERSION}"), false, None),
            outputs: MenuItem::new("", false, None),
            address: MenuItem::new("", false, None),
            home: MenuItem::new(w.open_home, false, None),
            network: Submenu::new(w.network, false),
            choices: Vec::new(),
            shown_network: None,
            start_stop: MenuItem::new(w.start, true, None),
            restart: MenuItem::new(w.restart, false, None),
            log: MenuItem::new(w.log, true, None),
            uninstall: MenuItem::new(w.uninstall, true, None),
            quit: MenuItem::new(w.quit, true, None),
        };
        let menu = Menu::new();
        let _ = menu.append_items(&[
            &items.status,
            &items.version,
            &items.outputs,
            &items.address,
            &PredefinedMenuItem::separator(),
            &items.home,
            &PredefinedMenuItem::separator(),
            &items.network,
            &PredefinedMenuItem::separator(),
            &items.start_stop,
            &items.restart,
            &PredefinedMenuItem::separator(),
            &items.log,
            &items.uninstall,
            &items.quit,
        ]);
        (menu, items)
    }

    /// The adapter submenu: one list for the web interface, one for NDI.
    fn rebuild_network(&mut self, s: &Status) {
        let wanted = (s.interfaces.clone(), s.control.clone(), s.ndi.clone());
        if self.shown_network.as_ref() == Some(&wanted) {
            return;
        }
        while self.network.remove_at(0).is_some() {}
        self.choices.clear();
        for (role, title, chosen) in [
            (Role::Control, self.w.web_ui, &s.control),
            (Role::Ndi, "NDI", &s.ndi),
        ] {
            if role == Role::Ndi {
                let _ = self.network.append(&PredefinedMenuItem::separator());
            }
            let _ = self.network.append(&MenuItem::new(title, false, None));
            let every = CheckMenuItem::new(self.w.every_adapter, true, chosen.is_empty(), None);
            let _ = self.network.append(&every);
            self.choices.push((every, role, String::new()));
            for (name, ip) in &s.interfaces {
                let item = CheckMenuItem::new(format!("{name} — {ip}"), true, chosen == name, None);
                let _ = self.network.append(&item);
                self.choices.push((item, role, name.clone()));
            }
        }
        self.shown_network = Some(wanted);
    }

    fn apply(&mut self, s: &Status, tray: Option<&TrayIcon>) {
        let w = self.w;
        let label = if s.running { w.running } else { w.stopped };
        self.status.set_text(label);
        if s.running {
            let mut v = w
                .version_port
                .replace(
                    "{v}",
                    if s.version.is_empty() {
                        VERSION
                    } else {
                        &s.version
                    },
                )
                .replace("{p}", &s.port.to_string());
            if s.uptime_secs > 0 {
                v = format!(
                    "{v} · {}",
                    w.since.replace("{t}", &duration_text(s.uptime_secs))
                );
            }
            self.version.set_text(v);
            self.outputs.set_text(if s.outputs_total == 0 {
                w.no_outputs.to_string()
            } else {
                w.outputs
                    .replace("{ok}", &s.outputs_ok.to_string())
                    .replace("{n}", &s.outputs_total.to_string())
            });
            match s.urls.first() {
                Some(u) => {
                    self.address.set_text(w.copy_address.replace("{u}", u));
                    self.address.set_enabled(true);
                }
                None => {
                    self.address.set_text(base_url(s.port));
                    self.address.set_enabled(false);
                }
            }
        } else {
            self.version.set_text(format!("Version {VERSION}"));
            self.outputs.set_text("—");
            self.address.set_text("—");
            self.address.set_enabled(false);
        }
        for item in [&self.home, &self.restart] {
            item.set_enabled(s.running);
        }
        self.network
            .set_enabled(s.running && !s.interfaces.is_empty());
        if s.running {
            self.rebuild_network(s);
        }
        self.start_stop
            .set_text(if s.running { w.stop } else { w.start });
        if let Some(t) = tray {
            let _ = t.set_tooltip(Some(label));
        }
    }
}

pub fn run() {
    let background = std::env::args().any(|a| a == "--background");

    // Second launch (e.g. from the Applications folder while the menu bar
    // app already runs): just bring up the web interface.
    let Ok(_lock) = TcpListener::bind(INSTANCE_LOCK) else {
        open_url(&format!("{}/", base_url(discover_port())));
        return;
    };
    let w: &'static Words = if system_is_french() { &FR } else { &EN };

    #[allow(unused_mut)]
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        // Menu bar only: no Dock icon, no app switcher entry.
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
    }
    let proxy = event_loop.create_proxy();
    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = menu_proxy.send_event(UserEvent::Menu(e));
    }));

    let (menu, mut items) = Items::new(w);
    let mut menu = Some(menu);
    let mut tray: Option<TrayIcon> = None;
    let mut status = Status {
        port: discover_port(),
        ..Status::default()
    };
    let mut first_status = true;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            // The tray must be created once the event loop runs (macOS).
            Event::NewEvents(StartCause::Init) => {
                // macOS draws a monochrome template that follows the menu bar theme.
                #[cfg(target_os = "macos")]
                let builder = TrayIconBuilder::new().with_icon_templated(icon());
                #[cfg(windows)]
                let builder = TrayIconBuilder::new().with_icon(icon());
                let mut builder = builder.with_tooltip("Strata Graphics");
                if let Some(m) = menu.take() {
                    builder = builder.with_menu(Box::new(m));
                }
                match builder.build() {
                    Ok(t) => tray = Some(t),
                    Err(e) => {
                        eprintln!("cannot create the tray icon: {e}");
                        *control_flow = ControlFlow::Exit;
                    }
                }
                poll(proxy.clone());
            }
            Event::UserEvent(UserEvent::Status(s)) => {
                // Opened by the user (not at login): show the web interface
                // once the server answers.
                if first_status && !background && s.running {
                    open_url(&format!("{}/", base_url(s.port)));
                }
                first_status = false;
                if s != status || items.status.text() == w.checking {
                    items.apply(&s, tray.as_ref());
                    status = s;
                }
            }
            Event::UserEvent(UserEvent::Menu(e)) => {
                let url = |path: &str| format!("{}/{path}", base_url(status.port));
                let id = e.id();
                if id == items.home.id() {
                    open_url(&url(""));
                } else if id == items.address.id() {
                    if let Some(u) = status.urls.first() {
                        copy_to_clipboard(u);
                    }
                } else if id == items.start_stop.id() {
                    let a = if status.running {
                        Action::Stop
                    } else {
                        Action::Start
                    };
                    server_action(a, status.port, w, proxy.clone());
                } else if id == items.restart.id() {
                    server_action(Action::Restart, status.port, w, proxy.clone());
                } else if id == items.log.id() {
                    open_log();
                } else if id == items.uninstall.id() {
                    uninstall();
                } else if id == items.quit.id() {
                    tray.take();
                    *control_flow = ControlFlow::Exit;
                } else if let Some((_, role, name)) =
                    items.choices.iter().find(|(c, ..)| c.id() == id)
                {
                    let (role, name, port) = (*role, name.clone(), status.port);
                    // The check marks follow the server once it restarts.
                    items.shown_network = None;
                    std::thread::spawn(move || {
                        let key = if role == Role::Control {
                            "control"
                        } else {
                            "ndi"
                        };
                        let body = serde_json::json!({ key: name }).to_string();
                        if !ask("PUT", port, "/api/system/network", Some(body)) {
                            alert(w.adapter_failed);
                        }
                    });
                }
            }
            _ => {}
        }
    });
}
