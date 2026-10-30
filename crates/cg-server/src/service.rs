//! Installs the server as a native background service:
//! systemd (Linux), launchd (macOS) or the Service Control Manager (Windows).
//!
//! `strata-server service install` registers the *current* executable, so copy
//! the binary to its final location first (the packaged install scripts do).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

pub struct InstallOptions {
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
}

fn exe() -> Result<PathBuf> {
    let p = std::env::current_exe().context("cannot locate the server executable")?;
    Ok(p.canonicalize().unwrap_or(p))
}

#[allow(dead_code)]
fn run(cmd: &str, args: &[&str]) -> Result<()> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .with_context(|| format!("cannot run `{cmd}`"))?;
    print!("{}", String::from_utf8_lossy(&out.stdout));
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!(
            "`{cmd} {}` failed ({}): {}",
            args.join(" "),
            out.status,
            err.trim()
        );
    }
    Ok(())
}

fn permission_hint(e: anyhow::Error, path: &Path) -> anyhow::Error {
    let denied = e.chain().any(|c| {
        c.downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied)
    });
    if denied {
        let how = if cfg!(windows) {
            "an elevated (Administrator) prompt"
        } else {
            "sudo"
        };
        anyhow::anyhow!(
            "permission denied writing {} — run this command with {how}",
            path.display()
        )
    } else {
        e
    }
}

// ---------------------------------------------------------------- Linux ----
#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use crate::paths::SERVICE_NAME;

    pub(super) const UNIT: &str = "/etc/systemd/system/strata-graphics.service";
    const USER: &str = "strata";
    /// The service of an install from before the rename.
    const FORMER_UNIT: &str = "/etc/systemd/system/pulse-cg.service";

    pub fn install(o: &InstallOptions) -> Result<()> {
        let exe = exe()?;
        if Path::new(FORMER_UNIT).exists() {
            let _ = run("systemctl", &["disable", "--now", "pulse-cg"]);
            let _ = std::fs::remove_file(FORMER_UNIT);
            let _ = Command::new("userdel").arg("pulse-cg").output();
        }
        // Dedicated unprivileged account owning the data directory.
        if Command::new("id")
            .arg(USER)
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            run(
                "useradd",
                &[
                    "--system",
                    "--no-create-home",
                    "--shell",
                    "/usr/sbin/nologin",
                    USER,
                ],
            )
            .context("cannot create the service account (are you root?)")?;
        }
        std::fs::create_dir_all(o.data_dir.join("time"))
            .map_err(|e| permission_hint(e.into(), &o.data_dir))?;
        run(
            "chown",
            &[
                "-R",
                &format!("{USER}:{USER}"),
                &o.data_dir.to_string_lossy(),
            ],
        )?;
        let unit = format!(
            "[Unit]\n\
             Description=Strata Graphics broadcast graphics server\n\
             After=network-online.target\n\
             Wants=network-online.target\n\n\
             [Service]\n\
             ExecStart=\"{exe}\" run --default-bind {bind} --data-dir \"{data}\"\n\
             User={USER}\n\
             Restart=always\n\
             RestartSec=2\n\
             # Low-latency playout: higher priority, allow ports < 1024.\n\
             Nice=-10\n\
             AmbientCapabilities=CAP_NET_BIND_SERVICE\n\
             LimitNOFILE=65536\n\
             NoNewPrivileges=yes\n\
             ProtectSystem=full\n\
             ReadWritePaths=\"{data}\"\n\n\
             [Install]\n\
             WantedBy=multi-user.target\n",
            exe = exe.display(),
            bind = o.bind,
            data = o.data_dir.display(),
        );
        std::fs::write(UNIT, unit).map_err(|e| permission_hint(e.into(), Path::new(UNIT)))?;
        // Settings › Time sets the NTP through a root helper.
        crate::timectl::install_helper(&o.data_dir)
            .map_err(|e| permission_hint(e.into(), Path::new(crate::timectl::HELPER)))?;
        run("systemctl", &["daemon-reload"])?;
        run("systemctl", &["enable", "--now", SERVICE_NAME])?;
        let _ = run("systemctl", &["enable", "--now", "strata-time.path"]);
        Ok(())
    }

    pub fn uninstall() -> Result<()> {
        let _ = run("systemctl", &["disable", "--now", SERVICE_NAME]);
        let _ = run("systemctl", &["disable", "--now", "strata-time.path"]);
        crate::timectl::uninstall_helper();
        match std::fs::remove_file(UNIT) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(permission_hint(e.into(), Path::new(UNIT))),
        }
        run("systemctl", &["daemon-reload"])
    }

    pub fn start() -> Result<()> {
        run("systemctl", &["start", SERVICE_NAME])
    }
    pub fn stop() -> Result<()> {
        run("systemctl", &["stop", SERVICE_NAME])
    }
    pub fn status() -> Result<()> {
        run("systemctl", &["status", "--no-pager", SERVICE_NAME])
    }
}

// ---------------------------------------------------------------- macOS ----
#[cfg(target_os = "macos")]
mod imp {
    use super::*;

    const LABEL: &str = "com.stratagraphics.server";
    const PLIST: &str = "/Library/LaunchDaemons/com.stratagraphics.server.plist";
    const LOG_DIR: &str = "/Library/Logs/Strata Graphics";
    /// The service of an install from before the rename.
    const FORMER_LABEL: &str = "com.pulsecg.server";
    const FORMER_PLIST: &str = "/Library/LaunchDaemons/com.pulsecg.server.plist";

    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    pub fn install(o: &InstallOptions) -> Result<()> {
        let exe = exe()?;
        if Path::new(FORMER_PLIST).exists() {
            let _ = Command::new("launchctl")
                .args(["bootout", &format!("system/{FORMER_LABEL}")])
                .output();
            let _ = std::fs::remove_file(FORMER_PLIST);
        }
        std::fs::create_dir_all(&o.data_dir).map_err(|e| permission_hint(e.into(), &o.data_dir))?;
        std::fs::create_dir_all(LOG_DIR)
            .map_err(|e| permission_hint(e.into(), Path::new(LOG_DIR)))?;
        let args = [
            exe.to_string_lossy().into_owned(),
            "run".into(),
            "--default-bind".into(),
            o.bind.to_string(),
            "--data-dir".into(),
            o.data_dir.to_string_lossy().into_owned(),
        ];
        let args_xml: String = args
            .iter()
            .map(|a| format!("    <string>{}</string>\n", xml_escape(a)))
            .collect();
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
{args_xml}  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
  <key>ProcessType</key><string>Interactive</string>
  <key>StandardOutPath</key><string>{LOG_DIR}/strata-server.log</string>
  <key>StandardErrorPath</key><string>{LOG_DIR}/strata-server.log</string>
</dict>
</plist>
"#
        );
        // Replace a previous installation cleanly. launchd unloads
        // asynchronously, so wait for it before loading the new definition.
        let _ = Command::new("launchctl")
            .args(["bootout", &target()])
            .output();
        wait_unloaded();
        std::fs::write(PLIST, plist).map_err(|e| permission_hint(e.into(), Path::new(PLIST)))?;
        bootstrap()
    }

    fn target() -> String {
        format!("system/{LABEL}")
    }

    fn loaded() -> bool {
        Command::new("launchctl")
            .args(["print", &target()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn wait_unloaded() {
        for _ in 0..50 {
            if !loaded() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    /// `launchctl bootstrap` fails with "5: Input/output error" while a
    /// previous instance is still unloading: retry for a few seconds.
    fn bootstrap() -> Result<()> {
        let mut last = String::new();
        for _ in 0..8 {
            let out = Command::new("launchctl")
                .args(["bootstrap", "system", PLIST])
                .output()
                .context("cannot run launchctl")?;
            if out.status.success() || loaded() {
                return Ok(());
            }
            last = String::from_utf8_lossy(&out.stderr).trim().to_string();
            std::thread::sleep(std::time::Duration::from_millis(750));
        }
        bail!("launchctl bootstrap failed: {last}")
    }

    pub fn uninstall() -> Result<()> {
        let _ = Command::new("launchctl")
            .args(["bootout", &target()])
            .output();
        wait_unloaded();
        match std::fs::remove_file(PLIST) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(permission_hint(e.into(), Path::new(PLIST))),
        }
    }

    pub fn start() -> Result<()> {
        if !Path::new(PLIST).exists() {
            bail!("the service is not installed — run `sudo strata-server service install`");
        }
        // Loading the daemon starts it (RunAtLoad); if it is already loaded
        // but stopped, kickstart it.
        if !loaded() {
            return bootstrap();
        }
        run("launchctl", &["kickstart", "-k", &target()])
    }
    pub fn stop() -> Result<()> {
        if !loaded() {
            return Ok(());
        }
        run("launchctl", &["kill", "SIGTERM", &target()])
    }
    pub fn status() -> Result<()> {
        run("launchctl", &["print", &format!("system/{LABEL}")])
    }
}

// -------------------------------------------------------------- Windows ----
#[cfg(windows)]
mod imp {
    use std::ffi::{OsStr, OsString};
    use std::time::Duration;

    use windows_service::service::{
        ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl,
        ServiceFailureActions, ServiceFailureResetPeriod, ServiceInfo, ServiceStartType,
        ServiceState, ServiceType,
    };
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    use super::*;
    use crate::paths::SERVICE_NAME;

    const DISPLAY_NAME: &str = "Strata Graphics Server";
    const FIREWALL_RULE: &str = "Strata Graphics Server";
    /// Rules an older install added under its former name.
    const LEGACY_FIREWALL_RULES: [&str; 1] = ["Pulse CG Server"];

    fn manager(access: ServiceManagerAccess) -> Result<ServiceManager> {
        ServiceManager::local_computer(None::<&str>, access).context(
            "cannot open the Service Control Manager — run from an elevated (Administrator) prompt",
        )
    }

    pub fn install(o: &InstallOptions) -> Result<()> {
        let exe = exe()?;
        std::fs::create_dir_all(&o.data_dir).map_err(|e| permission_hint(e.into(), &o.data_dir))?;
        let m = manager(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;
        let info = ServiceInfo {
            name: OsString::from(SERVICE_NAME),
            display_name: OsString::from(DISPLAY_NAME),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: exe.clone(),
            launch_arguments: [
                "run",
                "--service",
                "--default-bind",
                &o.bind.to_string(),
                "--data-dir",
                &o.data_dir.to_string_lossy(),
            ]
            .iter()
            .map(OsString::from)
            .collect(),
            dependencies: vec![],
            account_name: None, // LocalSystem
            account_password: None,
        };
        let svc = m
            .create_service(&info, ServiceAccess::CHANGE_CONFIG | ServiceAccess::START)
            .context(
                "cannot create the service (already installed? run `service uninstall` first)",
            )?;
        svc.set_description(
            "Broadcast character generator: HTML5, SDI and SMPTE ST 2110 graphics playout",
        )?;
        let restart = ServiceAction {
            action_type: ServiceActionType::Restart,
            delay: Duration::from_secs(2),
        };
        svc.update_failure_actions(ServiceFailureActions {
            reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86_400)),
            reboot_msg: None,
            command: None,
            actions: Some(vec![restart.clone(), restart.clone(), restart]),
        })?;
        // Let operators reach the web UI and outputs through Windows Firewall.
        for rule in std::iter::once(FIREWALL_RULE).chain(LEGACY_FIREWALL_RULES) {
            let _ = Command::new("netsh")
                .args([
                    "advfirewall",
                    "firewall",
                    "delete",
                    "rule",
                    &format!("name={rule}"),
                ])
                .output();
        }
        if let Err(e) = run(
            "netsh",
            &[
                "advfirewall",
                "firewall",
                "add",
                "rule",
                &format!("name={FIREWALL_RULE}"),
                "dir=in",
                "action=allow",
                &format!("program={}", exe.display()),
                "enable=yes",
                "profile=any",
            ],
        ) {
            eprintln!("warning: firewall rule not added: {e}");
        }
        svc.start(&[] as &[&OsStr])
            .context("service created but failed to start")?;
        Ok(())
    }

    pub fn uninstall() -> Result<()> {
        let m = manager(ServiceManagerAccess::CONNECT)?;
        let svc = m
            .open_service(
                SERVICE_NAME,
                ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
            )
            .context("service not installed")?;
        if svc.query_status()?.current_state != ServiceState::Stopped {
            let _ = svc.stop();
            for _ in 0..50 {
                if svc.query_status()?.current_state == ServiceState::Stopped {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        svc.delete()?;
        for rule in std::iter::once(FIREWALL_RULE).chain(LEGACY_FIREWALL_RULES) {
            let _ = Command::new("netsh")
                .args([
                    "advfirewall",
                    "firewall",
                    "delete",
                    "rule",
                    &format!("name={rule}"),
                ])
                .output();
        }
        Ok(())
    }

    pub fn start() -> Result<()> {
        let m = manager(ServiceManagerAccess::CONNECT)?;
        m.open_service(SERVICE_NAME, ServiceAccess::START)?
            .start(&[] as &[&OsStr])?;
        Ok(())
    }

    pub fn stop() -> Result<()> {
        let m = manager(ServiceManagerAccess::CONNECT)?;
        m.open_service(SERVICE_NAME, ServiceAccess::STOP)?.stop()?;
        Ok(())
    }

    pub fn status() -> Result<()> {
        let m = manager(ServiceManagerAccess::CONNECT)?;
        let st = m
            .open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS)
            .context("service not installed")?
            .query_status()?;
        println!("{SERVICE_NAME}: {:?}", st.current_state);
        Ok(())
    }
}

// ------------------------------------------------- other platforms (BSD) ----
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod imp {
    use super::*;
    fn unsupported() -> Result<()> {
        bail!("service management is not supported on this platform; run `strata-server run` under your init system")
    }
    pub fn install(_: &InstallOptions) -> Result<()> {
        unsupported()
    }
    pub fn uninstall() -> Result<()> {
        unsupported()
    }
    pub fn start() -> Result<()> {
        unsupported()
    }
    pub fn stop() -> Result<()> {
        unsupported()
    }
    pub fn status() -> Result<()> {
        unsupported()
    }
}

pub use imp::{install, start, status, stop, uninstall};

/// The data folder of the installed service (`--data-dir` of its unit).
#[cfg(target_os = "linux")]
pub fn data_dir() -> Option<PathBuf> {
    unit_data_dir(&std::fs::read_to_string(imp::UNIT).ok()?)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn unit_data_dir(unit: &str) -> Option<PathBuf> {
    let line = unit.lines().find(|l| l.starts_with("ExecStart="))?;
    let rest = line.split_once("--data-dir ")?.1.trim();
    let dir = match rest.strip_prefix('"') {
        Some(r) => r.split_once('"')?.0,
        None => rest.split_whitespace().next()?,
    };
    (!dir.is_empty()).then(|| PathBuf::from(dir))
}

#[cfg(not(target_os = "linux"))]
pub fn data_dir() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unit_tells_the_data_folder() {
        let unit = "[Service]\nExecStart=\"/opt/strata/strata-server\" run --default-bind 0.0.0.0:8080 --data-dir \"/srv/my strata\"\nUser=strata\n";
        assert_eq!(unit_data_dir(unit), Some(PathBuf::from("/srv/my strata")));
        assert_eq!(
            unit_data_dir("ExecStart=/x run --data-dir /d\n"),
            Some(PathBuf::from("/d"))
        );
        assert_eq!(unit_data_dir("ExecStart=/x run\n"), None);
    }
}

// ---------------------------------------------- Windows service runtime ----
/// Entry point when the SCM launches us (`run --service`).
#[cfg(windows)]
pub mod windows_runtime {
    use std::ffi::OsString;
    use std::sync::{Arc, OnceLock};
    use std::time::Duration;

    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
    use windows_service::{define_windows_service, service_dispatcher};

    use crate::paths::SERVICE_NAME;
    use crate::RunArgs;

    static ARGS: OnceLock<RunArgs> = OnceLock::new();

    define_windows_service!(ffi_service_main, service_main);

    pub fn run(args: RunArgs) -> anyhow::Result<()> {
        let _ = ARGS.set(args);
        service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
        Ok(())
    }

    fn service_main(_: Vec<OsString>) {
        if let Err(e) = run_service() {
            tracing::error!("service failed: {e:#}");
        }
    }

    fn run_service() -> anyhow::Result<()> {
        let args = ARGS.get().cloned().expect("args set before dispatch");
        let stop = Arc::new(tokio::sync::Notify::new());
        let stop2 = stop.clone();
        let handle = service_control_handler::register(SERVICE_NAME, move |ctrl| match ctrl {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                stop2.notify_one();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })?;
        let status = |state, accept| ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: accept,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::from_secs(5),
            process_id: None,
        };
        handle.set_service_status(status(
            ServiceState::Running,
            ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        ))?;
        let rt = tokio::runtime::Runtime::new()?;
        let result = rt.block_on(crate::serve(args, async move { stop.notified().await }));
        handle.set_service_status(status(ServiceState::Stopped, ServiceControlAccept::empty()))?;
        result
    }
}
