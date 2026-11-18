// Declaring the app's commands generates an `allow-<command>` permission for each, so the
// capabilities can grant them per origin (the local pages vs. the Anima web UI).
const COMMANDS: &[&str] = &[
    "app_info",
    "last_status",
    "open_url",
    "get_settings",
    "use_external_server",
    "use_bundled_server",
    "open_data_dir",
    "choose_key_file",
    "clear_key_file",
    "set_launch_at_login",
    "setup_info",
    "save_api_key",
    "finish_setup_api",
    "finish_setup_local",
    "import_existing",
    "check_app_update",
    "install_app_update",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
