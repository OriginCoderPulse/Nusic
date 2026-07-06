use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::library::Track;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    Off,
    One,
    All,
}

pub struct PlayerQueue {
    tracks: Vec<Track>,
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
        if shuffle {
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
        if shuffle {
            self.shuffle_order();
        } else {
            self.order = (0..self.tracks.len()).collect();
        }
    }

    pub fn shuffle_enabled(&self) -> bool {
        self.shuffle
    }

    pub fn repeat_mode(&self) -> RepeatMode {
        self.repeat
    }

    pub fn cycle_repeat(&mut self) -> RepeatMode {
        self.repeat = match self.repeat {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        };
        self.repeat
    }

    pub fn resync(&mut self, tracks: Vec<Track>) {
        let current_path = self.current_track().map(|t| t.path.clone());
        let old_order_paths: Vec<_> = self
            .order
            .iter()
            .filter_map(|&i| self.tracks.get(i).map(|t| t.path.clone()))
            .collect();

        self.tracks = tracks;

        self.current = current_path.and_then(|path| {
            self.tracks.iter().position(|t| t.path == path)
        });

        if self.shuffle {
            let mut order = Vec::new();
            for path in old_order_paths {
                if let Some(i) = self.tracks.iter().position(|t| t.path == path) {
                    if !order.contains(&i) {
                        order.push(i);
                    }
                }
            }
            for i in 0..self.tracks.len() {
                if !order.contains(&i) {
                    order.push(i);
                }
            }
            self.order = order;
        } else {
            self.order = (0..self.tracks.len()).collect();
        }
    }

    pub fn clear_current(&mut self) {
        self.current = None;
    }

    pub fn select(&mut self, index: usize) -> Option<&Track> {
        if index < self.tracks.len() {
            self.current = Some(index);
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
            return self.first();
        }

        None
    }

    pub fn prev(&mut self) -> Option<&Track> {
        if self.order.is_empty() {
            return None;
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

    fn shuffle_order(&mut self) {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);

        let mut order: Vec<usize> = (0..self.tracks.len()).collect();
        let n = order.len();
        for i in (1..n).rev() {
            let mut hasher = DefaultHasher::new();
            seed.hash(&mut hasher);
            i.hash(&mut hasher);
            let j = (hasher.finish() as usize) % (i + 1);
            order.swap(i, j);
        }
        self.order = order;
    }
}
