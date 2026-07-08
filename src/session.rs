use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::player::{PlaybackState, RepeatMode};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueSnapshot {
    /// Playback order as track paths (stable across library rescans).
    #[serde(default)]
    pub order_paths: Vec<PathBuf>,
    #[serde(default)]
    pub current_path: Option<PathBuf>,
    /// Legacy session files stored queue order as track indices.
    #[serde(default, rename = "order", skip_serializing)]
    legacy_order: Vec<usize>,
    #[serde(default, rename = "current", skip_serializing_if = "Option::is_none")]
    legacy_current: Option<usize>,
    pub shuffle: bool,
    pub repeat: RepeatMode,
}

impl QueueSnapshot {
    pub(crate) fn legacy_indices(&self) -> Option<(&[usize], Option<usize>)> {
        if self.order_paths.is_empty() && !self.legacy_order.is_empty() {
            Some((&self.legacy_order, self.legacy_current))
        } else {
            None
        }
    }

    pub fn from_paths(
        order_paths: Vec<PathBuf>,
        current_path: Option<PathBuf>,
        shuffle: bool,
        repeat: RepeatMode,
    ) -> Self {
        Self {
            order_paths,
            current_path,
            legacy_order: Vec::new(),
            legacy_current: None,
            shuffle,
            repeat,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub load_path: PathBuf,
    pub playback_state: PlaybackState,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub current_path: Option<PathBuf>,
    pub queue: QueueSnapshot,
    /// Track path for the highlighted row in the filtered list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_track_path: Option<PathBuf>,
    /// Legacy: filtered-list index from older session files.
    #[serde(default, rename = "list_selection", skip_serializing)]
    pub legacy_list_selection: usize,
    #[serde(default)]
    pub list_scroll: usize,
    #[serde(default)]
    pub search_query: String,
    /// Shift+P pin: q exits UI but keeps playback in background.
    #[serde(default)]
    pub quit_marked: bool,
}

impl SessionSnapshot {
    pub fn position(&self) -> Duration {
        Duration::from_millis(self.position_ms)
    }

    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.duration_ms)
    }
}

pub fn write_session(path: &PathBuf, snapshot: &SessionSnapshot) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_vec_pretty(snapshot)?;
    std::fs::write(path, data)?;
    Ok(())
}

pub fn read_session(path: &PathBuf) -> anyhow::Result<SessionSnapshot> {
    let data = std::fs::read(path)?;
    Ok(serde_json::from_slice(&data)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_legacy_queue_snapshot() {
        let json = r#"{
            "order": [0, 2, 1],
            "current": 2,
            "shuffle": false,
            "repeat": "Off"
        }"#;
        let snap: QueueSnapshot = serde_json::from_str(json).unwrap();
        assert!(snap.order_paths.is_empty());
        assert_eq!(snap.legacy_indices().unwrap().0, &[0, 2, 1]);
        assert_eq!(snap.legacy_indices().unwrap().1, Some(2));
    }

    #[test]
    fn deserialize_legacy_session_list_selection() {
        let json = r#"{
            "load_path": "/music",
            "playback_state": "Playing",
            "position_ms": 1000,
            "duration_ms": 200000,
            "current_path": null,
            "queue": { "order": [0], "current": 0, "shuffle": false, "repeat": "Off" },
            "list_selection": 3,
            "list_scroll": 0,
            "search_query": ""
        }"#;
        let snap: SessionSnapshot = serde_json::from_str(json).unwrap();
        assert_eq!(snap.legacy_list_selection, 3);
        assert!(snap.selected_track_path.is_none());
    }
}
