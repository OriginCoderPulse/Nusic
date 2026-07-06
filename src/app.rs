use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use crate::library::{scan_path, spawn_watcher};
use crate::player::{PlaybackState, PlayerCommand, PlayerEvent, PlayerQueue, RepeatMode};

const DEFAULT_VOLUME: f32 = 0.8;

#[derive(Debug, Clone)]
pub struct PlaybackInfo {
    pub state: PlaybackState,
    pub position: Duration,
    pub duration: Duration,
    pub volume: f32,
    pub current_path: Option<PathBuf>,
    /// Wall-clock anchor for interpolating position between engine updates.
    position_anchor: Instant,
}

impl Default for PlaybackInfo {
    fn default() -> Self {
        Self {
            state: PlaybackState::default(),
            position: Duration::ZERO,
            duration: Duration::ZERO,
            volume: 1.0,
            current_path: None,
            position_anchor: Instant::now(),
        }
    }
}

pub struct App {
    pub queue: PlayerQueue,
    pub playback: PlaybackInfo,
    pub list_selection: usize,
    pub filtered_indices: Vec<usize>,
    pub search_mode: bool,
    pub search_query: String,
    pub load_path: PathBuf,
    pub status_message: Option<String>,
    pub should_quit: bool,
    pub tick: u64,
    /// Smoothed bar heights (0..max level), updated every render frame.
    pub spectrum_bars: Vec<f32>,
    pub spectrum_started: Instant,
    /// Seconds since playback stopped; drives idle spectrum animation.
    pub spectrum_idle_secs: f32,
    pub lyrics: Option<Vec<crate::library::LrcLine>>,
    pub list_scroll: usize,
    resync_rx: Receiver<()>,
    cmd_tx: Sender<PlayerCommand>,
    evt_rx: Receiver<PlayerEvent>,
}

impl App {
    pub fn new(
        path: PathBuf,
        cmd_tx: Sender<PlayerCommand>,
        evt_rx: Receiver<PlayerEvent>,
    ) -> Self {
        let tracks = scan_path(&path);
        let queue = PlayerQueue::new(tracks, false, RepeatMode::Off);
        let filtered_indices: Vec<usize> = (0..queue.len()).collect();

        let resync_rx = spawn_watcher(path.clone());

        let mut app = Self {
            queue,
            playback: PlaybackInfo {
                volume: DEFAULT_VOLUME,
                ..Default::default()
            },
            list_selection: 0,
            filtered_indices,
            search_mode: false,
            search_query: String::new(),
            load_path: path,
            status_message: None,
            should_quit: false,
            tick: 0,
            spectrum_bars: Vec::new(),
            spectrum_started: Instant::now(),
            spectrum_idle_secs: 1000.0,
            lyrics: None,
            list_scroll: 0,
            resync_rx,
            cmd_tx,
            evt_rx,
        };

        if !app.queue.is_empty() {
            app.queue.first();
        }

        app
    }

    fn set_now_playing(&mut self, path: PathBuf) {
        self.playback.current_path = Some(path);
    }

    fn clear_now_playing(&mut self) {
        self.playback.current_path = None;
    }

    pub fn playback_position(&self) -> Duration {
        if self.playback.state == PlaybackState::Playing && !self.playback.duration.is_zero() {
            let pos = self.playback.position + self.playback.position_anchor.elapsed();
            return pos.min(self.playback.duration);
        }
        self.playback.position
    }

    pub fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);

        while self.resync_rx.try_recv().is_ok() {
            self.resync_library();
        }

        while let Ok(event) = self.evt_rx.try_recv() {
            match event {
                PlayerEvent::Loaded { duration } => {
                    self.playback.duration = duration;
                    self.playback.position = Duration::ZERO;
                    self.playback.position_anchor = Instant::now();
                    self.status_message = None;
                    self.reload_lyrics();
                }
                PlayerEvent::StateChanged(state) => {
                    let was_playing = self.playback.state == PlaybackState::Playing;
                    if was_playing && state == PlaybackState::Paused {
                        self.playback.position = self.playback_position();
                    }
                    self.playback.state = state;
                    if state == PlaybackState::Playing {
                        self.playback.position_anchor = Instant::now();
                    }
                    if was_playing && state != PlaybackState::Playing {
                        self.spectrum_idle_secs = 0.0;
                    }
                    if state == PlaybackState::Stopped {
                        self.playback.position = Duration::ZERO;
                        self.playback.duration = Duration::ZERO;
                    }
                }
                PlayerEvent::Position(pos) => {
                    self.playback.position = pos;
                    self.playback.position_anchor = Instant::now();
                }
                PlayerEvent::TrackEnded => {
                    self.on_track_ended();
                }
                PlayerEvent::Error(msg) => {
                    self.status_message = Some(msg);
                }
            }
        }
    }

    fn on_track_ended(&mut self) {
        if self.queue.repeat_mode() == RepeatMode::One {
            if let Some(track) = self.queue.current_track() {
                let path = track.path.clone();
                self.set_now_playing(path.clone());
                let _ = self.cmd_tx.send(PlayerCommand::Load(path));
            }
            return;
        }

        if self.next_track() {
            return;
        }

        self.playback.state = PlaybackState::Stopped;
        self.playback.position = Duration::ZERO;
    }

    pub fn shutdown(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::Shutdown);
    }

    pub fn toggle_playback(&mut self) {
        match self.playback.state {
            PlaybackState::Playing | PlaybackState::Paused => {
                let _ = self.cmd_tx.send(PlayerCommand::Toggle);
            }
            PlaybackState::Stopped => self.play_selected(),
        }
    }

    pub fn play_selected(&mut self) {
        let track_idx = match self.filtered_indices.get(self.list_selection) {
            Some(&idx) => idx,
            None => return,
        };

        if let Some(track) = self.queue.select(track_idx) {
            let path = track.path.clone();
            self.set_now_playing(path.clone());
            self.lyrics = crate::library::load_for_track(&path);
            let _ = self.cmd_tx.send(PlayerCommand::Load(path));
        }
    }

    fn reload_lyrics(&mut self) {
        if let Some(track) = self.queue.current_track() {
            self.lyrics = crate::library::load_for_track(&track.path);
        }
    }

    pub fn next_track(&mut self) -> bool {
        if let Some(track) = self.queue.next() {
            let path = track.path.clone();
            self.set_now_playing(path.clone());
            let _ = self.cmd_tx.send(PlayerCommand::Load(path));
            self.sync_selection_to_current();
            true
        } else {
            false
        }
    }

    pub fn prev_track(&mut self) {
        if self.playback.position > Duration::from_secs(3) {
            self.play_selected();
            return;
        }

        if let Some(track) = self.queue.prev() {
            let path = track.path.clone();
            self.set_now_playing(path.clone());
            let _ = self.cmd_tx.send(PlayerCommand::Load(path));
            self.sync_selection_to_current();
        }
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let len = self.filtered_indices.len();
        let next = self.list_selection as i32 + delta;
        self.list_selection = next.clamp(0, len as i32 - 1) as usize;
    }

    pub fn ensure_list_visible(&mut self, viewport: usize) {
        if viewport == 0 {
            return;
        }
        if self.list_selection < self.list_scroll {
            self.list_scroll = self.list_selection;
        }
        if self.list_selection >= self.list_scroll + viewport {
            self.list_scroll = self.list_selection + 1 - viewport;
        }
    }

    pub fn select_first(&mut self) {
        self.list_selection = 0;
    }

    pub fn select_last(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.list_selection = self.filtered_indices.len() - 1;
        }
    }

    pub fn toggle_shuffle(&mut self) {
        let enabled = !self.queue.shuffle_enabled();
        self.queue.set_shuffle(enabled);
    }

    pub fn cycle_repeat(&mut self) {
        self.queue.cycle_repeat();
    }

    pub fn adjust_volume(&mut self, delta: f32) {
        self.playback.volume = (self.playback.volume + delta).clamp(0.0, 1.0);
        let _ = self
            .cmd_tx
            .send(PlayerCommand::SetVolume(self.playback.volume));
    }

    pub fn open_music_dir(&mut self) {
        if let Err(e) = std::fs::create_dir_all(&self.load_path) {
            self.status_message = Some(format!("Failed to create music folder: {e}"));
            return;
        }

        let result = {
            #[cfg(target_os = "macos")]
            {
                std::process::Command::new("open").arg(&self.load_path).spawn()
            }
            #[cfg(target_os = "linux")]
            {
                std::process::Command::new("xdg-open")
                    .arg(&self.load_path)
                    .spawn()
            }
            #[cfg(target_os = "windows")]
            {
                std::process::Command::new("explorer").arg(&self.load_path).spawn()
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
            {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "opening folders is not supported on this platform",
                ))
            }
        };

        if let Err(e) = result {
            self.status_message = Some(format!("Failed to open music folder: {e}"));
        }
    }

    pub fn apply_filter(&mut self) {
        let query = self.search_query.to_lowercase();
        self.filtered_indices = self
            .queue
            .tracks()
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                query.is_empty()
                    || t.title.to_lowercase().contains(&query)
                    || t.artist.to_lowercase().contains(&query)
                    || t.album.to_lowercase().contains(&query)
            })
            .map(|(i, _)| i)
            .collect();

        if self.list_selection >= self.filtered_indices.len() {
            self.list_selection = self.filtered_indices.len().saturating_sub(1);
        }
    }

    fn sync_selection_to_current(&mut self) {
        if let Some(current) = self.queue.current_index() {
            if let Some(pos) = self.filtered_indices.iter().position(|&i| i == current) {
                self.list_selection = pos;
            }
        }
    }

    fn resync_library(&mut self) {
        let selected_path = self
            .filtered_indices
            .get(self.list_selection)
            .and_then(|&i| self.queue.tracks().get(i))
            .map(|t| t.path.clone());
        let current_path = self.queue.current_track().map(|t| t.path.clone());

        let tracks = scan_path(&self.load_path);
        self.queue.resync(tracks);

        if let Some(path) = current_path {
            if self.queue.tracks().iter().all(|t| t.path != path) {
                self.stop_playback();
            }
        }

        self.apply_filter();

        if let Some(path) = selected_path {
            if let Some(track_idx) = self.queue.tracks().iter().position(|t| t.path == path) {
                if let Some(pos) = self.filtered_indices.iter().position(|&i| i == track_idx) {
                    self.list_selection = pos;
                }
            } else if !self.filtered_indices.is_empty() {
                self.list_selection = self
                    .list_selection
                    .min(self.filtered_indices.len().saturating_sub(1));
            } else {
                self.list_selection = 0;
            }
        }

        self.sync_selection_to_current();
    }

    fn stop_playback(&mut self) {
        let _ = self.cmd_tx.send(PlayerCommand::Stop);
        self.queue.clear_current();
        self.clear_now_playing();
        self.playback.state = PlaybackState::Stopped;
        self.playback.position = Duration::ZERO;
        self.playback.duration = Duration::ZERO;
        self.lyrics = None;
        self.spectrum_idle_secs = 0.0;
    }
}
