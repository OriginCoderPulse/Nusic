use std::path::PathBuf;

/// Default folder for local music files (`~/.music`).
pub fn music_dir() -> PathBuf {
    dirs::home_dir()
        .map(|home| home.join(".music"))
        .unwrap_or_else(|| PathBuf::from(".music"))
}

pub fn ensure_music_dir() -> anyhow::Result<PathBuf> {
    let dir = music_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Runtime data for daemon IPC and session handoff (`~/.cache/nusic` or equivalent).
pub fn runtime_dir() -> PathBuf {
    dirs::cache_dir()
        .map(|d| d.join("nusic"))
        .unwrap_or_else(|| PathBuf::from(".cache/nusic"))
}

pub fn ensure_runtime_dir() -> anyhow::Result<PathBuf> {
    let dir = runtime_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn daemon_pid_path() -> PathBuf {
    runtime_dir().join("daemon.pid")
}

pub fn daemon_port_path() -> PathBuf {
    runtime_dir().join("daemon.port")
}
