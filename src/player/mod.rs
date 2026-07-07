mod queue;

pub use queue::{PlayerQueue, RepeatMode};

use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone)]
pub enum PlayerCommand {
    Load(PathBuf),
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
