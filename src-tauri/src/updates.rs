//! App updates from GitHub Releases (tauri-plugin-updater, signed with the key whose public half
//! is in tauri.conf.json). An app update also brings its bundled Anima server.

use crate::{server, AppState};
use serde_json::json;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;

#[tauri::command]
pub async fn check_app_update(app: AppHandle) -> Result<serde_json::Value, String> {
    let update = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
    let server = server::version(&app.state::<AppState>().paths().bin);
    Ok(match update {
        Some(u) => json!({
            "available": true,
            "version": u.version,
            "current": u.current_version,
            "notes": u.body,
            "server": server,
        }),
        None => json!({ "available": false, "current": env!("CARGO_PKG_VERSION"), "server": server }),
    })
}

/// Downloads and installs the update, then restarts the app. The bundled server is stopped
/// first: its binary is replaced (Windows cannot overwrite it while it runs), and the new app
/// must start the new server version.
#[tauri::command]
pub async fn install_app_update(app: AppHandle) -> Result<(), String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Aucune mise à jour disponible.")?;
    let bytes = update.download(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    if state.settings.lock().unwrap().bundled() {
        let paths = state.paths();
        tauri::async_runtime::spawn_blocking(move || server::stop(&paths)).await.map_err(|e| e.to_string())?;
    }
    update.install(bytes).map_err(|e| e.to_string())?;
    app.restart();
}
