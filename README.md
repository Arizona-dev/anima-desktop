# Anima desktop

Desktop app for the [Anima](https://github.com/AnimaLab/Anima) memory server, on macOS, Windows
and Linux (Tauri 2).

- Ships the Anima server and runs it from the app data folder, detached: it keeps running after
  the app quits, since other tools (MCP clients, assistant hooks) use it. Shows its web UI in a
  native window that remembers its size and position.
- Closing the window keeps Anima in the menu bar (macOS) or the tray (Windows, Linux): About,
  updates, settings, open the window, logs, data folder, quit.
- First launch asks how the models run (below); nothing is downloaded before that choice.
- The app's own settings live in the web UI's **Settings → Desktop app** section: launch at
  login, bundled or external server, data folder, API key file.
- Updates itself from this repository's GitHub releases (signed), server included.
- Launch at login starts the app hidden, in the menu bar or tray. macOS uses SMAppService
  (System Settings → General → Login Items), which needs no write access to
  `~/Library/LaunchAgents`; Windows and Linux use the autostart plugin (Run key, XDG autostart).
- Two fixes to the web UI are injected: the namespace selector rendering under WebKit, and
  "About Anima" pinned to the bottom of the sidebar.

## First launch

With no `config.toml` in the data folder, the window offers three choices before the server
starts:

- **OpenAI-compatible API**: base URL, embedding model and dimension, chat model, optional
  re-ranker (OpenAI-style `/rerank` endpoint) and the API key. Writes a `config.toml` where every
  model is remote, sparse vectors off: nothing is downloaded.
- **Local models**: Anima's built-in defaults; the server downloads about 571 MB of models from
  Hugging Face on its first start. Chat and background processing then need an LLM (Ollama on
  `localhost` by default).
- **Import an existing installation**: a folder where Anima already ran (with `anima.db`). The
  server running from it, if any, is stopped so the database is copied closed; `anima.db` (with
  its WAL files), `config.toml` and `models/` are copied, and the import is recorded in
  `.migration`. The folder itself is not modified. If an old login item or script starts that
  server again later, the app stops it at launch.

## Data folder

The bundled server runs in the app data folder, which holds everything that is yours:
`anima.db`, `config.toml`, `api-key`, `logs/`, `models/` (plus `web/`, the UI copied from the
app at each new version, and `config.default.toml` for reference).

| OS | Data folder |
| --- | --- |
| macOS | `~/Library/Application Support/dev.anima.desktop` |
| Windows | `%APPDATA%\dev.anima.desktop` |
| Linux | `~/.local/share/dev.anima.desktop` |

The API key file (default: `api-key` in the data folder, owner-only) is exported to the server as
`EMBEDDING_API_KEY`, `OPENAI_API_KEY`, `PROCESSOR_API_KEY` and `RERANKER_API_KEY`; another file
can be chosen in Settings → Desktop app. The same section can point the app at an external
installation (a folder with its own `anima-server` and data) instead of the bundled server.

## Pointing assistants at the server

The bundled binary also runs Anima's client modes, so MCP clients and hooks can use it, e.g. on
macOS `~/Applications/Anima.app/Contents/MacOS/anima-server mcp` (the path stays the same across
app updates).

## Build

```bash
tools/fetch-server.sh                # anima-server + web UI for this machine, from the release
cd src-tauri
cargo tauri build --bundles app      # macOS: target/release/bundle/macos/Anima.app
```

- `src-tauri/server-version` is the Anima server release the app ships; `tools/fetch-server.sh`
  downloads it (or takes `--from <extracted release folder>`).
- The app version (`src-tauri/tauri.conf.json` and `Cargo.toml`) is the app's own: it can move
  for an app-only fix without a new server.
- With `createUpdaterArtifacts` on, a build signs its update archive: set
  `TAURI_SIGNING_PRIVATE_KEY` (the private key, or its path) and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (empty for this key).
- Behind a proxy that blocks crates.io, put your crates mirror in a local `.cargo/config.toml`
  (ignored by git).

## CI and releases

- `.github/workflows/ci.yml`: `cargo clippy` on macOS, Windows and Linux for every push to
  `main` and every pull request.
- `.github/workflows/release.yml`: on a `v*` tag matching the app version, builds the macOS
  (`.app`, `.dmg`), Windows (NSIS) and Linux (`.deb`, `.rpm`, AppImage) installers, signs the
  update archives, attaches them with `latest.json` to a draft release, and publishes it once
  every platform has succeeded. Needs the `TAURI_SIGNING_PRIVATE_KEY` repository secret.

The app checks `https://github.com/Arizona-dev/anima-desktop/releases/latest/download/latest.json`
and verifies each update with the public key in `tauri.conf.json`. To release: bump the version
in `tauri.conf.json` and `Cargo.toml` (and `server-version` for a new server), commit, then tag
`v<version>` and push the tag.

## Scripting

A second launch shows the running window; `--open <about|updates|settings|logs|folder|quit>`
runs that menu action in the running instance instead. On macOS, `--login-item on|off|status`
manages the login item and exits.

© 2026 Farid Naderi
