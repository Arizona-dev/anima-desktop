//! The Anima server process: health check, data folder preparation, detached start, stop,
//! version.

use crate::settings::Paths;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Direct HTTP probe of 127.0.0.1 — no HTTP client, so no system or corporate proxy can
/// answer in the server's place.
pub fn is_up(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut s) = TcpStream::connect_timeout(&addr, Duration::from_millis(800)) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    if s.write_all(b"GET / HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n").is_err() {
        return false;
    }
    let mut buf = [0u8; 32];
    let n = s.read(&mut buf).unwrap_or(0);
    let head = String::from_utf8_lossy(&buf[..n]);
    head.starts_with("HTTP/1.") && head.contains(" 200")
}

fn copy_dir(from: &Path, to: &Path, overwrite: bool) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest, overwrite)?;
        } else if overwrite || !dest.exists() {
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

/// Bundled server: lays out the data folder from the app resources. The server reads its web
/// UI from `web/dist` under its working folder, so the app's copy goes there, refreshed when
/// the app version changes; shipped model files are added when missing.
pub fn prepare_data(data: &Path, resources: &Path, version: &str) -> Result<(), String> {
    let src = resources.join("server");
    std::fs::create_dir_all(data.join("logs")).map_err(|e| e.to_string())?;
    let marker = data.join("web").join(".app-version");
    if std::fs::read_to_string(&marker).ok().as_deref() != Some(version) {
        let _ = std::fs::remove_dir_all(data.join("web"));
        copy_dir(&src.join("web"), &data.join("web"), true).map_err(|e| format!("copie de l'interface web : {e}"))?;
        std::fs::write(&marker, version).map_err(|e| e.to_string())?;
    }
    if src.join("models").is_dir() {
        copy_dir(&src.join("models"), &data.join("models"), false).map_err(|e| format!("copie des modèles : {e}"))?;
    }
    let _ = std::fs::copy(src.join("config.default.toml"), data.join("config.default.toml"));
    Ok(())
}

/// Starts `anima-server [config.toml]` in the data folder, detached from the app so it keeps
/// running after the app quits (other tools, like the MCP bridge, rely on it).
pub fn spawn(paths: &Paths, key_file: &Path) -> Result<(), String> {
    if !paths.bin.exists() {
        return Err(format!("anima-server introuvable : {}", paths.bin.display()));
    }
    let logs = paths.data.join("logs");
    std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
    let open_log = |name: &str| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(logs.join(name))
            .map_err(|e| e.to_string())
    };

    let mut cmd = Command::new(&paths.bin);
    cmd.current_dir(&paths.data)
        .stdin(Stdio::null())
        .stdout(open_log("anima-server.log")?)
        .stderr(open_log("anima-server.err")?);
    if paths.data.join("config.toml").exists() {
        cmd.arg("config.toml");
    }
    if let Ok(key) = std::fs::read_to_string(key_file) {
        let key = key.trim();
        if !key.is_empty() {
            for var in ["EMBEDDING_API_KEY", "OPENAI_API_KEY", "PROCESSOR_API_KEY", "RERANKER_API_KEY"] {
                cmd.env(var, key);
            }
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| format!("lancement de {} : {e}", paths.bin.display()))?;
    // Reap it if it exits while the app is still running.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdin(Stdio::null()).stderr(Stdio::null())
}

fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// PIDs of the Anima servers running from these paths: same working folder (Unix) or same
/// executable (Windows). `anima-server mcp` / `recall` helpers (assistant integrations) are
/// left alone: they are clients, not servers.
pub fn find(paths: &Paths) -> Vec<u32> {
    let (data, bin) = (canon(&paths.data), canon(&paths.bin));
    let is_helper = |args: &str| {
        let words: Vec<&str> = args.split_whitespace().collect();
        words.iter().any(|w| matches!(*w, "mcp" | "recall" | "connect" | "--version" | "export" | "import"))
    };
    let mut pids = Vec::new();
    #[cfg(unix)]
    {
        let out = quiet(Command::new("ps").args(["-axo", "pid=,args="])).output();
        let Ok(out) = out else { return pids };
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let line = line.trim_start();
            let Some((pid, args)) = line.split_once(' ') else { continue };
            let first = args.split_whitespace().next().unwrap_or("");
            if !first.ends_with("anima-server") || is_helper(args) {
                continue;
            }
            let Ok(pid) = pid.parse::<u32>() else { continue };
            if cwd_of(pid).map(|c| canon(&c) == data).unwrap_or(false) || canon(Path::new(first)) == bin {
                pids.push(pid);
            }
        }
    }
    #[cfg(windows)]
    {
        let script = "Get-CimInstance Win32_Process -Filter \"Name='anima-server.exe'\" | ForEach-Object { \"$($_.ProcessId)`t$($_.ExecutablePath)`t$($_.CommandLine)\" }";
        let out = quiet(Command::new("powershell").args(["-NoProfile", "-Command", script])).output();
        let Ok(out) = out else { return pids };
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let mut parts = line.splitn(3, '\t');
            let (Some(pid), Some(exe), args) = (parts.next(), parts.next(), parts.next().unwrap_or("")) else { continue };
            if is_helper(args) || canon(Path::new(exe.trim())) != bin {
                continue;
            }
            if let Ok(pid) = pid.trim().parse() {
                pids.push(pid);
            }
        }
    }
    let _ = (&data, &bin);
    pids
}

#[cfg(target_os = "macos")]
fn cwd_of(pid: u32) -> Option<PathBuf> {
    let out = quiet(Command::new("lsof").args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])).output().ok()?;
    String::from_utf8_lossy(&out.stdout).lines().find_map(|l| l.strip_prefix('n').map(PathBuf::from))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn cwd_of(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

/// Stops the servers running from these paths and waits for them to exit (an Anima server
/// drains its in-flight jobs for up to 30 s). True once none is left.
pub fn stop(paths: &Paths) -> bool {
    let pids = find(paths);
    if pids.is_empty() {
        return true;
    }
    for pid in &pids {
        #[cfg(unix)]
        let _ = quiet(Command::new("kill").args(["-TERM", &pid.to_string()])).status();
        #[cfg(windows)]
        let _ = quiet(Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"])).status();
    }
    let deadline = Instant::now() + Duration::from_secs(45);
    while Instant::now() < deadline {
        if find(paths).is_empty() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}

/// Version of a server binary (`anima-server --version` → last word).
pub fn version(bin: &Path) -> Option<String> {
    let out = quiet(Command::new(bin).arg("--version")).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().next()?.split_whitespace().last().map(str::to_string)
}
