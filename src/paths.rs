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
