//! User settings (settings.json in the app config folder).
//!
//! By default the app runs the Anima server it ships with, from the app data folder
//! (anima.db, config.toml, logs, models). An external installation can be used instead.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Folder of an external Anima installation (its `anima-server`, config and data).
    /// Empty: the server bundled with the app, with the app data folder.
    pub external_dir: String,
    /// File holding the API key, exported to the server as its embedding, OpenAI, processor
    /// and reranker keys. Empty: `api-key` in the data folder. Ignored when missing.
    pub key_file: String,
}

pub fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// The API key file in use: the one chosen in the settings, else `api-key` in the data folder.
pub fn key_path(settings: &Settings, data_dir: &Path) -> PathBuf {
    if settings.key_file.trim().is_empty() {
        data_dir.join("api-key")
    } else {
        expand(settings.key_file.trim())
    }
}

impl Settings {
    pub fn load(dir: &Path) -> Settings {
        std::fs::read_to_string(dir.join("settings.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("settings.json"), json).map_err(|e| e.to_string())
    }

    pub fn bundled(&self) -> bool {
        self.external_dir.trim().is_empty()
    }
}

/// Where the server runs: its binary, and the folder it runs in (its data).
#[derive(Clone, Debug)]
pub struct Paths {
    pub bin: PathBuf,
    pub data: PathBuf,
}

impl Paths {
    pub fn log_file(&self) -> PathBuf {
        self.data.join("logs").join("anima-server.log")
    }

    /// `[server] port` of the config.toml in the data folder, else the server default (3000).
    pub fn port(&self) -> u16 {
        let Ok(text) = std::fs::read_to_string(self.data.join("config.toml")) else { return 3000 };
        let mut in_server = false;
        for line in text.lines() {
            let l = line.trim();
            if l.starts_with('[') {
                in_server = l == "[server]";
            } else if in_server {
                if let Some(v) = l.strip_prefix("port").map(str::trim).and_then(|r| r.strip_prefix('=')) {
                    if let Ok(p) = v.split('#').next().unwrap_or("").trim().parse() {
                        return p;
                    }
                }
            }
        }
        3000
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.port())
    }
}

/// `~/…` → home folder.
pub fn expand(p: &str) -> PathBuf {
    match p.strip_prefix("~/").or_else(|| p.strip_prefix("~\\")) {
        Some(rest) => home().join(rest),
        None => PathBuf::from(p),
    }
}

pub fn exe_name() -> &'static str {
    if cfg!(windows) {
        "anima-server.exe"
    } else {
        "anima-server"
    }
}
