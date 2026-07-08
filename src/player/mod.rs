mod queue;

pub use queue::{PlayerQueue, RepeatMode};

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PlaybackState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone)]
pub enum PlayerCommand {
    Load(PathBuf),
    LoadAt { path: PathBuf, position: Duration },
    Toggle,
    Stop,
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum PlayerEvent {
    Loaded { duration: Duration },
    StateChanged(PlaybackState),
    Position(Duration),
    TrackEnded,
    Error(String),
}
