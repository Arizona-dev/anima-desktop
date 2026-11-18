// Anima — desktop app for the Anima memory server (https://github.com/AnimaLab/Anima):
// starts the local server if needed, shows its web UI in a native window, and lives in the
// tray / menu bar when the window is closed.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod login;
mod migrate;
mod server;
mod settings;
mod setup;
mod ui;
mod updates;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, RunEvent, WindowEvent};

pub struct AppState {
    pub settings: Mutex<settings::Settings>,
    pub config_dir: PathBuf,
    /// Data folder of the bundled server (anima.db, config.toml, logs, models, web).
    pub data_dir: PathBuf,
    pub resource_dir: PathBuf,
    /// The server answered since the last (re)configuration.
    pub ready: AtomicBool,
    /// Last message for the loading page: (text, is_error).
    pub status: Mutex<(String, bool)>,
    starting: AtomicBool,
}

impl AppState {
    /// The server in use: the bundled one with the app data folder, or an external install.
    pub fn paths(&self) -> settings::Paths {
        let s = self.settings.lock().unwrap();
        if s.bundled() {
            let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_default();
            settings::Paths { bin: exe_dir.join(settings::exe_name()), data: self.data_dir.clone() }
        } else {
            let dir = settings::expand(s.external_dir.trim());
            settings::Paths { bin: dir.join(settings::exe_name()), data: dir }
        }
    }
}

/// Starts the server if it does not answer, waits for it, then shows the web UI.
/// Bundled server: migration from a hand-installed Anima first, then the data folder layout.
/// An old server still shutting down holds on for up to 30 s: retry the start every 10 s.
/// `start_server` result: the user has to choose a configuration first.
const NEEDS_SETUP: &str = "needs-setup";

pub fn ensure_server(app: AppHandle) {
    let state = app.state::<AppState>();
    if state.starting.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let result = start_server(&app, &state);
        match result {
            Ok(url) => {
                state.ready.store(true, Ordering::SeqCst);
                ui::set_tray_tooltip(&app, true);
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || ui::load_web_ui(&handle, &url));
            }
            Err(e) if e == NEEDS_SETUP => ui::show_setup(&app),
            Err(e) => {
                let logs = state.paths().data.join("logs");
                ui::status(&app, &format!("{e}\nLogs : {}", logs.display()), true);
            }
        }
        state.starting.store(false, Ordering::SeqCst);
    });
}

fn start_server(app: &AppHandle, state: &AppState) -> Result<String, String> {
    let settings = state.settings.lock().unwrap().clone();
    let paths = state.paths();
    ui::status(app, "Démarrage d'Anima…", false);
    if settings.bundled() {
        migrate::stop_imported_if_running(&paths.data);
        server::prepare_data(&paths.data, &state.resource_dir, env!("CARGO_PKG_VERSION"))?;
        if setup::needed(&paths.data) {
            return Err(NEEDS_SETUP.into());
        }
        ui::status(app, "Démarrage d'Anima…", false);
    }
    let port = paths.port();
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut last_spawn: Option<Instant> = None;
    while Instant::now() < deadline {
        if server::is_up(port) {
            return Ok(paths.url());
        }
        if last_spawn.map_or(true, |t| t.elapsed() >= Duration::from_secs(10)) {
            server::spawn(&paths, &settings::key_path(&settings, &state.data_dir))?;
            last_spawn = Some(Instant::now());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err("Anima ne répond pas.".into())
}

fn main() {
    #[cfg(target_os = "macos")]
    if let Some(code) = login::cli() {
        std::process::exit(code);
    }
    let builder = tauri::Builder::default()
        // A second launch shows the running instance; `--open <about|updates|settings|…>`
        // runs that menu action instead (scripting, tests).
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            match args.iter().position(|a| a == "--open").and_then(|i| args.get(i + 1)) {
                Some(action) => ui::on_menu(app, action),
                None => ui::show_main(app),
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Main window size and position are remembered; not its visibility (hidden at login).
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(tauri_plugin_window_state::StateFlags::all() - tauri_plugin_window_state::StateFlags::VISIBLE)
                .with_denylist(&["about", "updates"])
                .build(),
        );
    #[cfg(not(target_os = "macos"))]
    let builder = builder.plugin(tauri_plugin_autostart::init(
        tauri_plugin_autostart::MacosLauncher::LaunchAgent,
        Some(vec![login::LOGIN_ARG]),
    ));

    builder
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let settings = settings::Settings::load(&config_dir);
            app.manage(AppState {
                settings: Mutex::new(settings),
                config_dir,
                data_dir: app.path().app_data_dir()?,
                resource_dir: app.path().resource_dir()?,
                ready: AtomicBool::new(false),
                status: Mutex::new(("Démarrage d'Anima…".into(), false)),
                starting: AtomicBool::new(false),
            });
            let handle = app.handle().clone();
            #[cfg(target_os = "macos")]
            app.set_menu(ui::app_menu(&handle)?)?;
            ui::create_tray(&handle)?;
            ui::create_main(&handle, !login::launched_at_login())?;
            ensure_server(handle.clone());

            // Keep the tray tooltip in step with the server.
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(5));
                let port = handle.state::<AppState>().paths().port();
                ui::set_tray_tooltip(&handle, server::is_up(port));
            });
            Ok(())
        })
        .on_menu_event(|app, ev| ui::on_menu(app, ev.id().as_ref()))
        .on_window_event(|window, ev| match ev {
            // Closing the main window hides it: the app stays in the tray / menu bar.
            WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                api.prevent_close();
                let _ = window.hide();
                ui::refresh_dock(window.app_handle());
            }
            WindowEvent::Destroyed => ui::refresh_dock(window.app_handle()),
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            ui::app_info,
            ui::last_status,
            ui::get_settings,
            ui::use_external_server,
            ui::use_bundled_server,
            ui::open_data_dir,
            ui::choose_key_file,
            ui::clear_key_file,
            ui::set_launch_at_login,
            ui::open_url,
            setup::setup_info,
            setup::save_api_key,
            setup::finish_setup_api,
            setup::finish_setup_local,
            setup::import_existing,
            updates::check_app_update,
            updates::install_app_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Anima")
        .run(|app, event| match event {
            // Only "Quitter" exits; closing windows keeps the tray icon.
            RunEvent::ExitRequested { api, code, .. } if code.is_none() => api.prevent_exit(),
            // macOS: clicking the Dock icon brings the window back.
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => ui::show_main(app),
            _ => {
                let _ = app;
            }
        });
}
