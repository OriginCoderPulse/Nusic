use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::library::Track;
use crate::session::QueueSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepeatMode {
    Off,
    One,
    All,
}

pub struct PlayerQueue {
    tracks: Vec<Track>,
    /// Single playback order — shuffled only when shuffle is enabled.
    order: Vec<usize>,
    current: Option<usize>,
    shuffle: bool,
    repeat: RepeatMode,
}

impl PlayerQueue {
    pub fn new(tracks: Vec<Track>, shuffle: bool, repeat: RepeatMode) -> Self {
        let order: Vec<usize> = (0..tracks.len()).collect();
        let mut queue = Self {
            tracks,
            order,
            current: None,
            shuffle,
            repeat,
        };
        if queue.shuffle {
            queue.shuffle_order();
        }
        queue
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.current.and_then(|i| self.tracks.get(i))
    }

    pub fn current_queue_pos(&self) -> Option<usize> {
        self.current.and_then(|idx| {
            self.order
                .iter()
                .position(|&track_idx| track_idx == idx)
        })
    }

    pub fn set_shuffle(&mut self, shuffle: bool) {
        if self.shuffle == shuffle {
            return;
        }
        self.shuffle = shuffle;
        self.sync_order();
    }

    pub fn shuffle_enabled(&self) -> bool {
        self.shuffle
    }

    pub fn repeat_mode(&self) -> RepeatMode {
        self.repeat
    }

    pub fn set_repeat_mode(&mut self, mode: RepeatMode) {
        self.repeat = mode;
        if mode == RepeatMode::One {
            self.set_shuffle(false);
        }
    }

    pub fn cycle_repeat(&mut self) -> RepeatMode {
        self.repeat = match self.repeat {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        };
        if self.repeat == RepeatMode::One {
            self.set_shuffle(false);
        }
        self.repeat
    }

    pub fn resync(&mut self, tracks: Vec<Track>) {
        let current_path = self.current_track().map(|t| t.path.clone());

        self.tracks = tracks;

        self.current = current_path.and_then(|path| {
            self.tracks.iter().position(|t| t.path == path)
        });

        if self.shuffle {
            self.shuffle_from_current();
        } else {
            self.order = (0..self.tracks.len()).collect();
        }
        self.sanitize_after_resync();
    }

    pub fn clear_current(&mut self) {
        self.current = None;
    }

    /// Point the queue at `path` without reshuffling (used when resuming IPC loads).
    pub fn set_current_by_path(&mut self, path: &Path) {
        self.current = self.tracks.iter().position(|t| t.path == path);
    }

    pub fn snapshot(&self) -> QueueSnapshot {
        QueueSnapshot::from_paths(
            self.order
                .iter()
                .filter_map(|&i| self.tracks.get(i).map(|t| t.path.clone()))
                .collect(),
            self.current_track().map(|t| t.path.clone()),
            self.shuffle,
            self.repeat,
        )
    }

    pub fn restore_snapshot(&mut self, snap: &QueueSnapshot) {
        self.shuffle = snap.shuffle;
        self.repeat = snap.repeat;
        if let Some((order, current)) = snap.legacy_indices() {
            self.order = order.to_vec();
            self.current = current;
        } else {
            self.order = snap
                .order_paths
                .iter()
                .filter_map(|path| self.tracks.iter().position(|t| t.path == *path))
                .collect();
            self.current = snap.current_path.as_ref().and_then(|path| {
                self.tracks.iter().position(|t| t.path == *path)
            });
        }
        self.sanitize_after_resync();
    }

    fn sanitize_after_resync(&mut self) {
        let n = self.tracks.len();
        self.order.retain(|&i| i < n);
        if self.order.is_empty() && n > 0 {
            self.order = (0..n).collect();
        }
        if let Some(cur) = self.current {
            if cur >= n {
                self.current = None;
            }
        }
    }

    pub fn select(&mut self, index: usize) -> Option<&Track> {
        if index < self.tracks.len() {
            self.current = Some(index);
            if self.shuffle {
                self.shuffle_from_current();
            }
            self.tracks.get(index)
        } else {
            None
        }
    }

    pub fn first(&mut self) -> Option<&Track> {
        if self.order.is_empty() {
            return None;
        }
        let idx = self.order[0];
        self.current = Some(idx);
        self.tracks.get(idx)
    }

    pub fn next(&mut self) -> Option<&Track> {
        if self.order.is_empty() {
            return None;
        }

        if !self.shuffle {
            return self.next_sequential();
        }

        let pos = match self.current_queue_pos() {
            Some(p) => p,
            None => return self.first(),
        };

        if pos + 1 < self.order.len() {
            let idx = self.order[pos + 1];
            self.current = Some(idx);
            return self.tracks.get(idx);
        }

        if self.repeat == RepeatMode::All {
            self.begin_new_cycle();
            let idx = self.order[0];
            self.current = Some(idx);
            return self.tracks.get(idx);
        }

        None
    }

    pub fn prev(&mut self) -> Option<&Track> {
        if self.order.is_empty() {
            return None;
        }

        if !self.shuffle {
            return self.prev_sequential();
        }

        let pos = match self.current_queue_pos() {
            Some(p) => p,
            None => return self.first(),
        };

        if pos > 0 {
            let idx = self.order[pos - 1];
            self.current = Some(idx);
            return self.tracks.get(idx);
        }

        if self.repeat == RepeatMode::All {
            let idx = self.order[self.order.len() - 1];
            self.current = Some(idx);
            return self.tracks.get(idx);
        }

        self.current_track()
    }

    fn next_sequential(&mut self) -> Option<&Track> {
        let cur = self.current?;

        if cur + 1 < self.tracks.len() {
            self.current = Some(cur + 1);
            return self.tracks.get(cur + 1);
        }

        if self.repeat == RepeatMode::All {
            self.current = Some(0);
            return self.tracks.get(0);
        }

        None
    }

    fn prev_sequential(&mut self) -> Option<&Track> {
        let cur = self.current?;

        if cur > 0 {
            self.current = Some(cur - 1);
            return self.tracks.get(cur - 1);
        }

        if self.repeat == RepeatMode::All {
            let last = self.tracks.len() - 1;
            self.current = Some(last);
            return self.tracks.get(last);
        }

        self.current_track()
    }

    fn sync_order(&mut self) {
        if self.shuffle {
            self.shuffle_from_current();
        } else {
            self.order = (0..self.tracks.len()).collect();
        }
    }

    /// Shuffle with the current track first, then the rest — so next always
    /// walks through unplayed songs before stopping (when repeat is off).
    fn shuffle_from_current(&mut self) {
        let n = self.tracks.len();
        if n == 0 {
            self.order.clear();
            return;
        }
        if let Some(cur) = self.current {
            let mut rest: Vec<usize> = (0..n).filter(|&i| i != cur).collect();
            Self::shuffle_slice(&mut rest);
            self.order = std::iter::once(cur).chain(rest).collect();
        } else {
            self.shuffle_order();
        }
    }

    fn begin_new_cycle(&mut self) {
        let current = self.current;
        self.shuffle_order();
        if let Some(cur) = current {
            if self.order.len() > 1 && self.order[0] == cur {
                self.order.swap(0, 1);
            }
        }
    }

    fn shuffle_order(&mut self) {
        self.order = (0..self.tracks.len()).collect();
        Self::shuffle_slice(&mut self.order);
    }

    fn shuffle_slice(items: &mut [usize]) {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let n = items.len();
        for i in (1..n).rev() {
            let mut hasher = DefaultHasher::new();
            seed.hash(&mut hasher);
            i.hash(&mut hasher);
            let j = (hasher.finish() as usize) % (i + 1);
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::library::Track;

    #[test]
    fn snapshot_preserves_shuffle_and_repeat() {
        let tracks = vec![
            Track {
                path: PathBuf::from("/a.mp3"),
                title: "A".into(),
                artist: String::new(),
                album: String::new(),
                duration_secs: None,
                track_number: None,
            },
            Track {
                path: PathBuf::from("/b.mp3"),
                title: "B".into(),
                artist: String::new(),
                album: String::new(),
                duration_secs: None,
                track_number: None,
            },
        ];
        let mut queue = PlayerQueue::new(tracks, true, RepeatMode::All);
        queue.select(1);

        let snap = queue.snapshot();
        assert!(snap.shuffle);
        assert_eq!(snap.repeat, RepeatMode::All);

        let mut restored = PlayerQueue::new(
            vec![
                Track {
                    path: PathBuf::from("/a.mp3"),
                    title: "A".into(),
                    artist: String::new(),
                    album: String::new(),
                    duration_secs: None,
                    track_number: None,
                },
                Track {
                    path: PathBuf::from("/b.mp3"),
                    title: "B".into(),
                    artist: String::new(),
                    album: String::new(),
                    duration_secs: None,
                    track_number: None,
                },
            ],
            false,
            RepeatMode::Off,
        );
        restored.restore_snapshot(&snap);
        assert!(restored.shuffle_enabled());
        assert_eq!(restored.repeat_mode(), RepeatMode::All);
        assert_eq!(
            restored.current_track().map(|t| t.path.to_str().unwrap()),
            Some("/b.mp3")
        );
    }

    #[test]
    fn next_honors_repeat_all_in_sequential_mode() {
        let tracks = vec![
            Track {
                path: PathBuf::from("/a.mp3"),
                title: "A".into(),
                artist: String::new(),
                album: String::new(),
                duration_secs: None,
                track_number: None,
            },
            Track {
                path: PathBuf::from("/b.mp3"),
                title: "B".into(),
                artist: String::new(),
                album: String::new(),
                duration_secs: None,
                track_number: None,
            },
        ];
        let mut queue = PlayerQueue::new(tracks, false, RepeatMode::All);
        queue.select(1);

        assert_eq!(
            queue.next().map(|t| t.path.to_str().unwrap()),
            Some("/a.mp3")
        );
    }

    #[test]
    fn set_current_by_path_does_not_reshuffle() {
        let tracks = vec![
            Track {
                path: PathBuf::from("/a.mp3"),
                title: "A".into(),
                artist: String::new(),
                album: String::new(),
                duration_secs: None,
                track_number: None,
            },
            Track {
                path: PathBuf::from("/b.mp3"),
                title: "B".into(),
                artist: String::new(),
                album: String::new(),
                duration_secs: None,
                track_number: None,
            },
        ];
        let mut queue = PlayerQueue::new(tracks, true, RepeatMode::Off);
        queue.select(0);
        let order_before = queue.snapshot().order_paths;

        queue.set_current_by_path(Path::new("/b.mp3"));
        assert_eq!(queue.snapshot().order_paths, order_before);
        assert_eq!(
            queue.current_track().map(|t| t.path.to_str().unwrap()),
            Some("/b.mp3")
        );
    }
}
