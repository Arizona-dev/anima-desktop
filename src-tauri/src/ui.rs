//! Windows, tray icon and menus.
//!
//! - `main`: the Anima web UI served by the local server (a local loading page until it answers).
//!   Closing it hides it; the app stays in the tray / menu bar.
//! - `about`, `settings`, `updates`: small local pages (app/*.html).

use crate::{login, server, AppState};
use serde_json::json;
use std::sync::atomic::Ordering;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
#[cfg(target_os = "macos")]
use tauri::menu::Submenu;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, State, Url, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;

pub const REPO_URL: &str = "https://github.com/AnimaLab/Anima";
pub const AUTHOR: &str = "Farid Naderi";
const TRAY_ID: &str = "anima";

// Injected into the Anima web UI: style fixes and the "Desktop app" section of its Settings page.
const WEB_INJECT: &str = include_str!("inject.js");

// ---------------------------------------------------------------------------------------------
// Windows

fn is_local_or_app(url: &Url) -> bool {
    url.scheme() == "tauri"
        || matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "tauri.localhost" | "ipc.localhost"))
}

fn main_window(app: &AppHandle, visible: bool) -> tauri::Result<tauri::WebviewWindow> {
    if let Some(w) = app.get_webview_window("main") {
        return Ok(w);
    }
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("loading.html".into()))
        .title("Anima")
        .inner_size(1280.0, 860.0)
        .min_inner_size(720.0, 480.0)
        .center()
        .visible(visible)
        .initialization_script(WEB_INJECT)
        // Links that leave the local server open in the default browser.
        .on_navigation(move |url| {
            if is_local_or_app(url) {
                return true;
            }
            if matches!(url.scheme(), "http" | "https" | "mailto") {
                open_path(url.as_str());
            }
            false
        })
        .build()
}

#[cfg(target_os = "macos")]
fn set_dock(app: &AppHandle, visible: bool) {
    let _ = app.set_activation_policy(if visible {
        tauri::ActivationPolicy::Regular
    } else {
        tauri::ActivationPolicy::Accessory
    });
}
#[cfg(not(target_os = "macos"))]
fn set_dock(_app: &AppHandle, _visible: bool) {}

fn focus(app: &AppHandle, w: &tauri::WebviewWindow) {
    set_dock(app, true);
    let _ = w.show();
    let _ = w.unminimize();
    let _ = w.set_focus();
}

pub fn show_main(app: &AppHandle) {
    if let Ok(w) = main_window(app, true) {
        focus(app, &w);
    }
}

/// The app's settings live in the Anima web UI's own Settings page ("Desktop app" section).
pub fn show_settings(app: &AppHandle) {
    show_main(app);
    let state = app.state::<AppState>();
    if !state.ready.load(Ordering::SeqCst) {
        return; // still on the loading page: the web UI comes up on its own
    }
    let url = format!("{}settings#desktop", state.paths().url());
    if let (Some(w), Ok(u)) = (app.get_webview_window("main"), Url::parse(&url)) {
        let _ = w.navigate(u);
    }
}

fn local_page(page: &str) -> Option<Url> {
    // The app's own pages: tauri://localhost (macOS, Linux), http://tauri.localhost (Windows).
    let base = if cfg!(windows) { "http://tauri.localhost/" } else { "tauri://localhost/" };
    Url::parse(&format!("{base}{page}")).ok()
}

/// Back to the loading page (server restarting).
pub fn show_loading(app: &AppHandle) {
    if let (Some(w), Some(u)) = (app.get_webview_window("main"), local_page("loading.html")) {
        let _ = w.navigate(u);
    }
}

/// First launch without a config: the choice page, shown even when started at login.
pub fn show_setup(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        show_main(&handle);
        if let (Some(w), Some(u)) = (handle.get_webview_window("main"), local_page("setup.html")) {
            let _ = w.navigate(u);
        }
    });
}

/// Created hidden at login, visible otherwise.
pub fn create_main(app: &AppHandle, visible: bool) -> tauri::Result<()> {
    let w = main_window(app, visible)?;
    if visible {
        focus(app, &w);
    } else {
        set_dock(app, false);
    }
    Ok(())
}

/// Called once the server answers: leaves the loading page for the web UI.
pub fn load_web_ui(app: &AppHandle, url: &str) {
    if let (Some(w), Ok(u)) = (app.get_webview_window("main"), Url::parse(url)) {
        let on_loading = w.url().map(|cur| cur.path().ends_with("loading.html")).unwrap_or(true);
        if on_loading {
            let _ = w.navigate(u);
        }
    }
}

pub fn status(app: &AppHandle, message: &str, error: bool) {
    let state = app.state::<AppState>();
    *state.status.lock().unwrap() = (message.to_string(), error);
    let _ = app.emit_to("main", "anima://status", json!({ "message": message, "error": error }));
}

fn open_page(app: &AppHandle, label: &str, title: &str, w: f64, h: f64) {
    if let Some(win) = app.get_webview_window(label) {
        focus(app, &win);
        return;
    }
    let built = WebviewWindowBuilder::new(app, label, WebviewUrl::App(format!("{label}.html").into()))
        .title(title)
        .inner_size(w, h)
        .resizable(false)
        .minimizable(false)
        .maximizable(false)
        // macOS: the first click on an inactive window also reaches the page (buttons).
        .accept_first_mouse(true)
        .center()
        .build();
    if let Ok(win) = built {
        focus(app, &win);
    }
}

/// macOS: the Dock icon is only shown while one of the app's windows is visible.
pub fn refresh_dock(app: &AppHandle) {
    let any_visible = app.webview_windows().values().any(|w| w.is_visible().unwrap_or(false));
    set_dock(app, any_visible);
}

pub fn open_path(target: &str) {
    let (cmd, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![target])
    } else if cfg!(windows) {
        ("cmd", vec!["/C", "start", "", target])
    } else {
        ("xdg-open", vec![target])
    };
    let mut c = std::process::Command::new(cmd);
    c.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let _ = c.spawn();
}

// ---------------------------------------------------------------------------------------------
// Menus

fn item<R: Runtime>(app: &AppHandle<R>, id: &str, text: &str, accel: Option<&str>) -> tauri::Result<MenuItem<R>> {
    MenuItem::with_id(app, id, text, true, accel)
}

pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let sep = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(
        app,
        &[
            &item(app, "about", "À propos d'Anima", None)?,
            &item(app, "updates", "Rechercher les mises à jour…", None)?,
            &item(app, "settings", "Réglages…", None)?,
            &sep()?,
            &item(app, "open", "Ouvrir Anima", None)?,
            &sep()?,
            &item(app, "logs", "Afficher les logs", None)?,
            &item(app, "folder", "Ouvrir le dossier des données", None)?,
            &sep()?,
            &item(app, "quit", "Quitter Anima", None)?,
        ],
    )?;
    // macOS: monochrome "template" A, recoloured by the system like every menu-bar icon.
    // Elsewhere a black glyph would vanish on a dark taskbar: use the coloured app icon.
    let (icon, template) = if cfg!(target_os = "macos") {
        (tauri::image::Image::from_bytes(include_bytes!("../icons/tray/tray-template@2x.png"))?, true)
    } else {
        (app.default_window_icon().unwrap().clone(), false)
    };
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(template)
        .tooltip("Anima")
        .menu(&menu)
        // macOS: the menu on click, like every menu-bar item. Windows: left click opens the
        // window, right click shows the menu. (Linux trays always show the menu.)
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                if !cfg!(target_os = "macos") {
                    show_main(tray.app_handle());
                }
            }
        })
        .build(app)?;
    Ok(())
}

/// macOS application menu (Windows and Linux windows have no menu bar).
#[cfg(target_os = "macos")]
pub fn app_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let sep = || PredefinedMenuItem::separator(app);
    let anima = Submenu::with_items(
        app,
        "Anima",
        true,
        &[
            &item(app, "about", "À propos d'Anima", None)?,
            &item(app, "updates", "Rechercher les mises à jour…", None)?,
            &sep()?,
            &item(app, "settings", "Réglages…", Some("CmdOrCtrl+,"))?,
            &sep()?,
            &PredefinedMenuItem::hide(app, Some("Masquer Anima"))?,
            &PredefinedMenuItem::hide_others(app, Some("Masquer les autres"))?,
            &sep()?,
            &item(app, "quit", "Quitter Anima", Some("CmdOrCtrl+Q"))?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Édition",
        true,
        &[
            &PredefinedMenuItem::undo(app, Some("Annuler"))?,
            &PredefinedMenuItem::redo(app, Some("Rétablir"))?,
            &sep()?,
            &PredefinedMenuItem::cut(app, Some("Couper"))?,
            &PredefinedMenuItem::copy(app, Some("Copier"))?,
            &PredefinedMenuItem::paste(app, Some("Coller"))?,
            &PredefinedMenuItem::select_all(app, Some("Tout sélectionner"))?,
        ],
    )?;
    let view = Submenu::with_items(app, "Présentation", true, &[&item(app, "reload", "Recharger", Some("CmdOrCtrl+R"))?])?;
    let window = Submenu::with_items(
        app,
        "Fenêtre",
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some("Réduire"))?,
            &PredefinedMenuItem::close_window(app, Some("Fermer"))?,
        ],
    )?;
    Menu::with_items(app, &[&anima, &edit, &view, &window])
}

/// One handler for the tray menu and the macOS application menu.
pub fn on_menu(app: &AppHandle, id: &str) {
    let paths = app.state::<AppState>().paths();
    match id {
        "about" => open_page(app, "about", "À propos d'Anima", 300.0, 290.0),
        "updates" => open_page(app, "updates", "Mises à jour d'Anima", 400.0, 240.0),
        "settings" => show_settings(app),
        "open" => show_main(app),
        "logs" => open_path(&paths.log_file().display().to_string()),
        "folder" => open_path(&paths.data.display().to_string()),
        "reload" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.eval("location.reload()");
            }
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

/// Tray tooltip follows the server state.
pub fn set_tray_tooltip(app: &AppHandle, up: bool) {
    if let Some(t) = app.tray_by_id(TRAY_ID) {
        let _ = t.set_tooltip(Some(if up { "Anima" } else { "Anima — serveur arrêté" }));
    }
}

// ---------------------------------------------------------------------------------------------
// Commands for the local pages

#[tauri::command]
pub fn app_info(state: State<AppState>) -> serde_json::Value {
    let paths = state.paths();
    json!({
        "version": server::version(&paths.bin),
        "bundled": state.settings.lock().unwrap().bundled(),
        "serverDir": paths.data.display().to_string(),
        "appVersion": env!("CARGO_PKG_VERSION"),
        "repo": REPO_URL,
        "author": AUTHOR,
    })
}

#[tauri::command]
pub fn last_status(state: State<AppState>) -> serde_json::Value {
    let (message, error) = state.status.lock().unwrap().clone();
    json!({ "message": message, "error": error })
}

fn settings_json(app: &AppHandle) -> serde_json::Value {
    let state = app.state::<AppState>();
    let s = state.settings.lock().unwrap().clone();
    let paths = state.paths();
    json!({
        "bundled": s.bundled(),
        "externalDir": s.external_dir,
        "dataDir": paths.data.display().to_string(),
        "serverFound": paths.bin.exists(),
        "version": server::version(&paths.bin),
        "port": paths.port(),
        "keyFile": crate::settings::key_path(&s, &state.data_dir).display().to_string(),
        "keyDefault": s.key_file.trim().is_empty(),
        "migration": crate::migrate::summary(&state.data_dir),
        "login": login::status(app),
        "platform": std::env::consts::OS,
    })
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> serde_json::Value {
    settings_json(&app)
}

/// Applies a change and saves it. When it changes which server runs, the old one is stopped
/// and the new one started.
fn update(app: &AppHandle, f: impl FnOnce(&mut crate::settings::Settings)) -> Result<serde_json::Value, String> {
    let state = app.state::<AppState>();
    let before = state.paths();
    {
        let mut s = state.settings.lock().unwrap();
        f(&mut s);
        s.save(&state.config_dir)?;
    }
    let after = state.paths();
    if before.data != after.data || before.bin != after.bin {
        status(app, "Changement de serveur…", false);
        show_loading(app);
        server::stop(&before);
        state.ready.store(false, Ordering::SeqCst);
        crate::ensure_server(app.clone());
    }
    Ok(settings_json(app))
}

// Paths are only ever chosen in the native dialog, never sent by the page: the web UI is
// served by the local server, and must not be able to point the app at another program.

#[tauri::command]
pub async fn use_external_server(app: AppHandle) -> Result<serde_json::Value, String> {
    let picked = app.dialog().file().set_title("Dossier d'une installation d'Anima").blocking_pick_folder();
    let Some(p) = picked else { return Ok(settings_json(&app)) };
    let dir = p.into_path().map_err(|e| e.to_string())?;
    if !dir.join(crate::settings::exe_name()).exists() {
        return Err(format!("{} introuvable dans ce dossier.", crate::settings::exe_name()));
    }
    update(&app, |s| s.external_dir = dir.display().to_string())
}

#[tauri::command]
pub async fn use_bundled_server(app: AppHandle) -> Result<serde_json::Value, String> {
    update(&app, |s| s.external_dir.clear())
}

#[tauri::command]
pub fn open_data_dir(app: AppHandle) {
    open_path(&app.state::<AppState>().paths().data.display().to_string());
}

#[tauri::command]
pub async fn choose_key_file(app: AppHandle) -> Result<serde_json::Value, String> {
    match app.dialog().file().set_title("Fichier de clé API").blocking_pick_file() {
        Some(p) => {
            let p = p.into_path().map_err(|e| e.to_string())?;
            update(&app, |s| s.key_file = p.display().to_string())
        }
        None => Ok(settings_json(&app)),
    }
}

#[tauri::command]
pub fn clear_key_file(app: AppHandle) -> Result<serde_json::Value, String> {
    update(&app, |s| s.key_file.clear())
}

#[tauri::command]
pub fn set_launch_at_login(app: AppHandle, on: bool) -> Result<login::Status, String> {
    login::set(&app, on)?;
    Ok(login::status(&app))
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("only https links".into());
    }
    open_path(&url);
    Ok(())
}
