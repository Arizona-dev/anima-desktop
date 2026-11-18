//! First launch of the bundled server with no config.toml: the user chooses how Anima runs its
//! models — an OpenAI-compatible API (nothing downloaded), local models (the server downloads
//! about 571 MB), or importing an existing Anima installation. The server is not started before
//! that choice, so nothing is ever downloaded without it.

use crate::settings::key_path;
use crate::AppState;
use serde::Deserialize;
use serde_json::json;
use std::path::Path;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

const API: &str = include_str!("../templates/config-api.toml");
/// Header of the "local models" config, which is the server's own config.default.toml: the
/// server rejects a config missing one of its tables, so a complete file is the safe choice.
const LOCAL_HEADER: &str = "# Anima — written by the desktop app at first launch (\"local models\" choice).\n\
# This is the server's config.default.toml: embeddings and re-ranking run on this computer, and\n\
# the server downloads the models (about 571 MB, from Hugging Face) into ./models on its first\n\
# start. Chat and background processing use the LLM set in [llm] and [processor].\n\n";

pub fn needed(data: &Path) -> bool {
    !data.join("config.toml").exists()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiChoice {
    base_url: String,
    embedding_model: String,
    dimension: u32,
    chat_model: String,
    /// Optional: a re-ranker behind an OpenAI-style /rerank endpoint.
    reranker_model: String,
}

/// TOML basic string.
fn q(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.trim().chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn api_config(c: &ApiChoice) -> Result<String, String> {
    let base = c.base_url.trim();
    if !(base.starts_with("https://") || base.starts_with("http://")) {
        return Err("L'URL de l'API doit commencer par https:// ou http://.".into());
    }
    if c.embedding_model.trim().is_empty() || c.chat_model.trim().is_empty() || c.dimension == 0 {
        return Err("Indiquez le modèle d'embedding, sa dimension et le modèle de chat.".into());
    }
    let reranker = if c.reranker_model.trim().is_empty() {
        "# No re-ranker: set one served behind an OpenAI-style /rerank endpoint, or use the\n# local model (downloads 571 MB). See config.default.toml.\n[reranker]\nenabled = false".to_string()
    } else {
        format!(
            "[reranker]\nenabled = true\nbackend = \"openai_compat\"\napi_base_url = {}\napi_model = {}",
            q(base),
            q(&c.reranker_model)
        )
    };
    Ok(API
        .replace("{{BASE_URL_TOML}}", &q(base))
        .replace("{{BASE_URL}}", base)
        .replace("{{EMBEDDING_MODEL}}", &q(&c.embedding_model))
        .replace("{{DIMENSION}}", &c.dimension.to_string())
        .replace("{{CHAT_MODEL}}", &q(&c.chat_model))
        .replace("{{RERANKER}}", &reranker))
}

#[tauri::command]
pub fn setup_info(state: State<AppState>) -> serde_json::Value {
    let key = key_path(&state.settings.lock().unwrap(), &state.data_dir);
    let found = std::fs::read_to_string(&key).map(|s| !s.trim().is_empty()).unwrap_or(false);
    json!({ "keyFile": key.display().to_string(), "keyFound": found })
}

/// Saves a pasted API key to the key file (owner-only on Unix).
#[tauri::command]
pub fn save_api_key(state: State<AppState>, key: String) -> Result<serde_json::Value, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("Clé vide.".into());
    }
    let path = key_path(&state.settings.lock().unwrap(), &state.data_dir);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, format!("{key}\n")).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(setup_info(state))
}

fn write_config_and_start(app: &AppHandle, text: &str) -> Result<(), String> {
    let data = app.state::<AppState>().paths().data;
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let config = data.join("config.toml");
    if config.exists() {
        return Err("config.toml existe déjà.".into());
    }
    std::fs::write(&config, text).map_err(|e| e.to_string())?;
    crate::ui::show_loading(app);
    crate::ensure_server(app.clone());
    Ok(())
}

#[tauri::command]
pub fn finish_setup_api(app: AppHandle, choice: ApiChoice) -> Result<(), String> {
    let text = api_config(&choice)?;
    write_config_and_start(&app, &text)
}

#[tauri::command]
pub fn finish_setup_local(app: AppHandle) -> Result<(), String> {
    let defaults = app.state::<AppState>().resource_dir.join("server").join("config.default.toml");
    let text = std::fs::read_to_string(&defaults).map_err(|e| format!("{} : {e}", defaults.display()))?;
    write_config_and_start(&app, &format!("{LOCAL_HEADER}{text}"))
}

/// Imports an existing installation picked by the user, then starts the server on it.
/// Returns false when the dialog was cancelled.
#[tauri::command]
pub async fn import_existing(app: AppHandle) -> Result<bool, String> {
    let Some(picked) = app.dialog().file().set_title("Dossier d'une installation d'Anima (avec anima.db)").blocking_pick_folder()
    else {
        return Ok(false);
    };
    let src = picked.into_path().map_err(|e| e.to_string())?;
    let data = app.state::<AppState>().paths().data;
    crate::ui::show_loading(&app);
    let handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::migrate::import(&src, &data, &|m| crate::ui::status(&handle, m, false))
    })
    .await
    .map_err(|e| e.to_string())?;
    if let Err(e) = result {
        crate::ui::show_setup(&app);
        return Err(e);
    }
    crate::ensure_server(app.clone());
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(reranker: &str) -> ApiChoice {
        ApiChoice {
            base_url: "https://api.example.com/v1".into(),
            embedding_model: "text-embedding-3-small".into(),
            dimension: 1536,
            chat_model: "gpt-4o-mini".into(),
            reranker_model: reranker.into(),
        }
    }

    #[test]
    fn quotes_toml_strings() {
        assert_eq!(q(r#"a"b\c"#), r#""a\"b\\c""#);
    }

    #[test]
    fn api_config_fills_every_placeholder() {
        for r in ["", "BAAI/bge-reranker-v2-m3"] {
            let text = api_config(&choice(r)).unwrap();
            assert!(!text.contains("{{"), "{text}");
            assert!(text.contains("dimension = 1536"));
            assert_eq!(text.contains("enabled = false\n\n# Sparse"), r.is_empty());
        }
        assert!(api_config(&ApiChoice { base_url: "ftp://x".into(), ..choice("") }).is_err());
    }

    /// `ANIMA_CONFIG_OUT=<file> cargo test write_sample -- --ignored` writes a sample config.
    #[test]
    #[ignore]
    fn write_sample() {
        let out = std::env::var("ANIMA_CONFIG_OUT").expect("ANIMA_CONFIG_OUT");
        std::fs::write(out, api_config(&choice("BAAI/bge-reranker-v2-m3")).unwrap()).unwrap();
    }
}
