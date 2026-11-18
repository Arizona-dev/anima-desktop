//! Importing an existing Anima installation (a folder with anima.db) into the app data folder,
//! from the first-launch page.
//!
//! The server running from that folder, if any, is stopped first, so the database is copied
//! closed, never while written. The folder itself is left as it is.

use crate::server;
use crate::settings::{exe_name, Paths};
use std::path::{Path, PathBuf};

const MARKER: &str = ".migration";
const DB_FILES: &[&str] = &["anima.db", "anima.db-wal", "anima.db-shm"];

/// "Imported from …" line for the settings, when an import happened.
pub fn summary(data: &Path) -> Option<String> {
    let text = std::fs::read_to_string(data.join(MARKER)).ok()?;
    text.lines().next().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// The folder an import came from (second line of the marker).
fn source(data: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(data.join(MARKER)).ok()?;
    text.lines().nth(1).map(str::trim).filter(|s| !s.is_empty()).map(PathBuf::from)
}

fn today() -> String {
    // YYYY-MM-DD from the Unix time, without a date crate.
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    let (z, era) = (days + 719_468, (days + 719_468).div_euclid(146_097));
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_tree(&e.path(), &dest)?;
        } else {
            std::fs::copy(e.path(), &dest)?;
        }
    }
    Ok(())
}

/// Copies anima.db (with its WAL files), config.toml and models/ from `src` into `data`.
pub fn import(src: &Path, data: &Path, status: &dyn Fn(&str)) -> Result<(), String> {
    if !src.join("anima.db").exists() {
        return Err(format!("Aucun anima.db dans {}.", src.display()));
    }
    std::fs::create_dir_all(data).map_err(|e| e.to_string())?;

    status("Arrêt du serveur de cette installation…");
    let old = Paths { bin: src.join(exe_name()), data: src.to_path_buf() };
    if !server::stop(&old) {
        return Err("Le serveur de cette installation ne s'est pas arrêté : import annulé, rien n'a été copié.".into());
    }

    status("Import de vos mémoires…");
    for f in DB_FILES {
        let from = src.join(f);
        if from.exists() {
            std::fs::copy(&from, data.join(f)).map_err(|e| format!("copie de {f} : {e}"))?;
        }
    }
    if src.join("config.toml").exists() {
        std::fs::copy(src.join("config.toml"), data.join("config.toml")).map_err(|e| format!("copie de config.toml : {e}"))?;
    }
    if src.join("models").is_dir() {
        copy_tree(&src.join("models"), &data.join("models")).map_err(|e| format!("copie des modèles : {e}"))?;
    }
    let marker = format!("Imported from {} on {}\n{}\n", src.display(), today(), src.display());
    std::fs::write(data.join(MARKER), marker).map_err(|e| e.to_string())
}

/// After an import, an old login item or script may start the imported installation's server
/// again and take the port: stop it so the app's server can serve the imported data.
pub fn stop_imported_if_running(data: &Path) {
    if let Some(src) = source(data) {
        let old = Paths { bin: src.join(exe_name()), data: src };
        if !server::find(&old).is_empty() {
            server::stop(&old);
        }
    }
}
