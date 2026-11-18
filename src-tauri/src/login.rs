//! Launch at login. macOS: SMAppService (Réglages Système → Général → Ouverture), which needs
//! no write access to ~/Library/LaunchAgents. Windows and Linux: the autostart plugin, started
//! with `--login` so the app knows to stay in the tray.

use tauri::AppHandle;

// Only passed by the autostart plugin (Windows, Linux).
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub const LOGIN_ARG: &str = "--login";

#[derive(serde::Serialize)]
pub struct Status {
    pub enabled: bool,
    /// macOS only: registered but waiting for the user's approval in System Settings.
    pub needs_approval: bool,
}

#[cfg(target_os = "macos")]
mod imp {
    use super::Status;
    use smappservice_rs::{AppService, ServiceStatus, ServiceType};
    use tauri::AppHandle;

    pub fn status(_app: &AppHandle) -> Status {
        let st = AppService::new(ServiceType::MainApp).status();
        Status { enabled: st == ServiceStatus::Enabled, needs_approval: st == ServiceStatus::RequiresApproval }
    }

    pub fn set(_app: &AppHandle, on: bool) -> Result<(), String> {
        let svc = AppService::new(ServiceType::MainApp);
        let r = if on { svc.register() } else { svc.unregister() };
        r.map_err(|e| e.to_string())?;
        if on && svc.status() == ServiceStatus::RequiresApproval {
            AppService::open_system_settings_login_items();
        }
        Ok(())
    }

    /// Login items get no argument: a launch in the first minutes after boot counts as one.
    pub fn launched_at_login() -> bool {
        let out = std::process::Command::new("sysctl").args(["-n", "kern.boottime"]).output();
        let boot = out.ok().and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).to_string();
            s.split("sec = ").nth(1)?.split(',').next()?.trim().parse::<u64>().ok()
        });
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        matches!(boot, Some(b) if now.saturating_sub(b) < 180)
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::Status;
    use tauri::AppHandle;
    use tauri_plugin_autostart::ManagerExt;

    pub fn status(app: &AppHandle) -> Status {
        Status { enabled: app.autolaunch().is_enabled().unwrap_or(false), needs_approval: false }
    }

    pub fn set(app: &AppHandle, on: bool) -> Result<(), String> {
        let m = app.autolaunch();
        let r = if on { m.enable() } else { m.disable() };
        r.map_err(|e| e.to_string())
    }

    pub fn launched_at_login() -> bool {
        std::env::args().any(|a| a == super::LOGIN_ARG)
    }
}

pub fn status(app: &AppHandle) -> Status {
    imp::status(app)
}

pub fn set(app: &AppHandle, on: bool) -> Result<(), String> {
    imp::set(app, on)
}

pub fn launched_at_login() -> bool {
    imp::launched_at_login()
}

/// `--login-item on|off|status` (macOS): manages the login item from a script and exits.
#[cfg(target_os = "macos")]
pub fn cli() -> Option<i32> {
    use smappservice_rs::{AppService, ServiceStatus, ServiceType};
    let args: Vec<String> = std::env::args().collect();
    let i = args.iter().position(|a| a == "--login-item")?;
    let svc = AppService::new(ServiceType::MainApp);
    let r = match args.get(i + 1).map(String::as_str) {
        Some("on") => svc.register(),
        Some("off") => svc.unregister(),
        _ => Ok(()),
    };
    if let Err(e) = r {
        eprintln!("error: {e}");
        return Some(1);
    }
    let st = match svc.status() {
        ServiceStatus::Enabled => "enabled",
        ServiceStatus::RequiresApproval => "requires approval (System Settings → General → Login Items)",
        ServiceStatus::NotRegistered => "not registered",
        ServiceStatus::NotFound => "not found",
    };
    println!("login item: {st}");
    Some(0)
}
