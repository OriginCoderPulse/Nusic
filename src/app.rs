use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossbeam_channel::{Receiver, Sender};

use crate::ipc::IpcClient;
use crate::library::{scan_path, spawn_watcher};
use crate::paths::ensure_runtime_dir;
use crate::player::{PlaybackState, PlayerCommand, PlayerEvent, PlayerQueue, RepeatMode};
use crate::session::{write_session, SessionSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitAction {
    None,
    Quit,
    Detach,
}

#[derive(Debug, Clone)]
pub struct PlaybackInfo {
    pub state: PlaybackState,
    pub position: Duration,
    pub duration: Duration,
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
    pub help_mode: bool,
    pub search_query: String,
    pub load_path: PathBuf,
    pub status_message: Option<String>,
    pub exit_action: ExitAction,
    /// Shift+P: when true, q exits UI and keeps playback in background.
    pub quit_marked: bool,
    pub tick: u64,
    /// Smoothed bar heights (0..max level), updated every render frame.
    pub spectrum_bars: Vec<f32>,
    /// Random target heights; reseeded at irregular intervals while playing.
    pub spectrum_targets: Vec<f32>,
    pub spectrum_seed: u64,
    pub spectrum_reseed_acc: f32,
    pub lyrics: Option<Vec<crate::library::LrcLine>>,
    pub list_scroll: usize,
    pub list_viewport: usize,
    pending_pause: bool,
    resync_rx: Receiver<()>,
    cmd_tx: Sender<PlayerCommand>,
    evt_rx: Receiver<PlayerEvent>,
    pub ipc_client: Option<Arc<Mutex<IpcClient>>>,
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
        let spectrum_seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);

        let mut app = Self {
            queue,
            playback: PlaybackInfo::default(),
            list_selection: 0,
            filtered_indices,
            search_mode: false,
            help_mode: false,
            search_query: String::new(),
            load_path: path,
            status_message: None,
            exit_action: ExitAction::None,
            quit_marked: false,
            tick: 0,
            spectrum_bars: Vec::new(),
            spectrum_targets: Vec::new(),
            spectrum_seed,
            spectrum_reseed_acc: 0.0,
            lyrics: None,
            list_scroll: 0,
            list_viewport: 1,
            pending_pause: false,
            resync_rx,
            cmd_tx,
            evt_rx,
            ipc_client: None,
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

        for event in self.drain_player_events() {
            let _ = event;
        }
    }

    pub fn drain_player_events(&mut self) -> Vec<PlayerEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.evt_rx.try_recv() {
            self.apply_player_event(&event);
            events.push(event);
        }
        events
    }

    pub fn apply_player_event(&mut self, event: &PlayerEvent) {
        match event {
            PlayerEvent::Loaded { duration } => {
                self.playback.duration = *duration;
                self.playback.position_anchor = Instant::now();
                self.status_message = None;
                self.reload_lyrics();
                if self.pending_pause {
                    self.pending_pause = false;
                    let _ = self.cmd_tx.send(PlayerCommand::Toggle);
                }
            }
            PlayerEvent::StateChanged(state) => {
                let was_playing = self.playback.state == PlaybackState::Playing;
                if was_playing && *state == PlaybackState::Paused {
                    self.playback.position = self.playback_position();
                }
                self.playback.state = *state;
                if *state == PlaybackState::Playing {
                    self.playback.position_anchor = Instant::now();
                }
                if *state == PlaybackState::Stopped {
                    self.playback.position = Duration::ZERO;
                    self.playback.duration = Duration::ZERO;
                }
            }
            PlayerEvent::Position(pos) => {
                self.playback.position = *pos;
                self.playback.position_anchor = Instant::now();
            }
            PlayerEvent::TrackEnded => {
                self.on_track_ended();
            }
            PlayerEvent::Error(msg) => {
                self.status_message = Some(msg.clone());
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

    pub fn move_selection_half_page(&mut self, direction: i32) {
        let step = (self.list_viewport / 2).max(1) as i32;
        self.move_selection(direction * step);
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
        if enabled && self.queue.repeat_mode() == RepeatMode::One {
            self.queue.set_repeat_mode(RepeatMode::Off);
        }
        self.queue.set_shuffle(enabled);
    }

    pub fn cycle_repeat(&mut self) {
        self.queue.cycle_repeat();
    }

    pub fn adjust_volume(&mut self, delta: f32) {
        if let Err(e) = crate::system_volume::adjust(delta) {
            self.status_message = Some(e);
        }
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
        self.spectrum_seed = self.spectrum_seed.wrapping_add(1);
        self.spectrum_reseed_acc = 0.0;
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        let selected_track_path = self
            .filtered_indices
            .get(self.list_selection)
            .and_then(|&i| self.queue.tracks().get(i).map(|t| t.path.clone()));

        SessionSnapshot {
            load_path: self.load_path.clone(),
            playback_state: self.playback.state,
            position_ms: self.playback_position().as_millis() as u64,
            duration_ms: self.playback.duration.as_millis() as u64,
            current_path: self
                .queue
                .current_track()
                .map(|t| t.path.clone())
                .or_else(|| self.playback.current_path.clone()),
            queue: self.queue.snapshot(),
            selected_track_path,
            legacy_list_selection: 0,
            list_scroll: self.list_scroll,
            search_query: self.search_query.clone(),
            quit_marked: self.quit_marked,
        }
    }

    pub fn from_snapshot(
        snapshot: SessionSnapshot,
        cmd_tx: Sender<PlayerCommand>,
        evt_rx: Receiver<PlayerEvent>,
        watch_library: bool,
    ) -> Self {
        let tracks = scan_path(&snapshot.load_path);
        let mut queue =
            PlayerQueue::new(tracks, snapshot.queue.shuffle, snapshot.queue.repeat);
        queue.restore_snapshot(&snapshot.queue);

        let resync_rx = if watch_library {
            spawn_watcher(snapshot.load_path.clone())
        } else {
            let (_tx, rx) = crossbeam_channel::bounded(0);
            rx
        };
        let spectrum_seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);

        let mut app = Self {
            queue,
            playback: PlaybackInfo {
                state: snapshot.playback_state,
                position: snapshot.position(),
                duration: snapshot.duration(),
                current_path: snapshot.current_path.clone(),
                position_anchor: Instant::now(),
            },
            list_selection: 0,
            filtered_indices: Vec::new(),
            search_mode: false,
            help_mode: false,
            search_query: snapshot.search_query.clone(),
            load_path: snapshot.load_path.clone(),
            status_message: None,
            exit_action: ExitAction::None,
            quit_marked: snapshot.quit_marked,
            tick: 0,
            spectrum_bars: Vec::new(),
            spectrum_targets: Vec::new(),
            spectrum_seed,
            spectrum_reseed_acc: 0.0,
            lyrics: None,
            list_scroll: snapshot.list_scroll,
            list_viewport: 1,
            pending_pause: false,
            resync_rx,
            cmd_tx,
            evt_rx,
            ipc_client: None,
        };

        app.apply_filter();
        app.restore_list_selection(&snapshot);

        if let Some(path) = snapshot.current_path {
            app.set_now_playing(path.clone());
            app.lyrics = crate::library::load_for_track(&path);
        }

        app
    }

    fn restore_list_selection(&mut self, snapshot: &SessionSnapshot) {
        if let Some(path) = &snapshot.selected_track_path {
            if let Some(pos) = self.filtered_indices.iter().position(|&i| {
                self.queue
                    .tracks()
                    .get(i)
                    .is_some_and(|t| t.path == *path)
            }) {
                self.list_selection = pos;
                return;
            }
        } else if !self.filtered_indices.is_empty() {
            self.list_selection = snapshot
                .legacy_list_selection
                .min(self.filtered_indices.len().saturating_sub(1));
            return;
        }
        self.sync_selection_to_current();
    }

    pub fn bootstrap_playback(&mut self) {
        let Some(path) = self.playback.current_path.clone() else {
            return;
        };
        let position = self.playback.position;
        let state = self.playback.state;
        if state == PlaybackState::Stopped && position.is_zero() {
            return;
        }
        if state == PlaybackState::Paused {
            self.pending_pause = true;
        }
        let _ = self.cmd_tx.send(PlayerCommand::LoadAt { path, position });
    }

    pub fn spawn_background_daemon(&self) -> anyhow::Result<()> {
        let runtime = ensure_runtime_dir()?;
        let session_path = runtime.join(format!("handoff-{}.json", std::process::id()));
        write_session(&session_path, &self.snapshot())?;
        crate::ipc::spawn_detached_daemon(&session_path)?;
        if !crate::ipc::wait_for_daemon(Duration::from_secs(5)) {
            anyhow::bail!("background player failed to start");
        }
        Ok(())
    }

    pub fn toggle_quit_mark(&mut self) {
        self.quit_marked = !self.quit_marked;
    }

    /// Quit or detach depending on `quit_marked`. Returns true when the UI should close.
    pub fn try_quit(&mut self) -> bool {
        if !self.quit_marked {
            self.exit_action = ExitAction::Quit;
            return true;
        }

        if self.ipc_client.is_some() {
            self.exit_action = ExitAction::Detach;
            return true;
        }

        match self.spawn_background_daemon() {
            Ok(()) => {
                self.shutdown();
                self.exit_action = ExitAction::Detach;
                true
            }
            Err(e) => {
                self.status_message = Some(format!("Background play failed: {e}"));
                false
            }
        }
    }
}
