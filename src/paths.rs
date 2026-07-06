use std::path::PathBuf;

/// Default folder for local music files (`~/.config/nusic`).
pub fn music_dir() -> PathBuf {
    dirs::home_dir()
        .map(|home| home.join(".config").join("nusic"))
        .unwrap_or_else(|| PathBuf::from(".config/nusic"))
}

pub fn ensure_music_dir() -> anyhow::Result<PathBuf> {
    let dir = music_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
