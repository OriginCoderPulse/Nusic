use std::fs;
use std::path::{Path, PathBuf};

use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::error::{NusicError, Result};

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "opus", "m4a", "m4p", "aac", "wav", "aiff", "aif",
];

#[derive(Debug, Clone)]
pub struct Track {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: Option<f64>,
    pub track_number: Option<u32>,
}

impl Track {
    pub fn from_path(path: PathBuf) -> Result<Self> {
        if !path.is_file() {
            return Err(NusicError::TrackNotFound(path));
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        if !AUDIO_EXTENSIONS.contains(&ext.as_str()) {
            return Err(NusicError::UnsupportedFormat(ext));
        }

        let (title, artist, album, track_number, duration_secs) = read_metadata(&path);

        Ok(Self {
            title,
            artist,
            album,
            duration_secs,
            track_number,
            path,
        })
    }

    pub fn duration_display(&self) -> String {
        self.duration_secs
            .map(format_duration)
            .unwrap_or_else(|| "--:--".to_string())
    }
}

pub fn format_duration(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn read_metadata(path: &Path) -> (String, String, String, Option<u32>, Option<f64>) {
    let filename = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();

    let mut title = filename.clone();
    let mut artist = "Unknown Artist".to_string();
    let mut album = "Unknown Album".to_string();
    let mut track_number = None;
    let mut duration_secs = None;

    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return (title, artist, album, track_number, duration_secs),
    };

    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let meta_opts: MetadataOptions = Default::default();
    let fmt_opts: FormatOptions = Default::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &fmt_opts, &meta_opts)
        .ok();

    if let Some(mut probed) = probed {
        if let Some(time_base) = probed.format.default_track().and_then(|t| t.codec_params.time_base) {
            if let Some(n_frames) = probed
                .format
                .default_track()
                .and_then(|t| t.codec_params.n_frames)
            {
                duration_secs = Some(time_base.calc_time(n_frames).seconds as f64);
            }
        }

        if let Some(rev) = probed.format.metadata().current() {
            for tag in rev.tags() {
                match tag.std_key {
                    Some(symphonia::core::meta::StandardTagKey::TrackTitle) => {
                        title = tag.value.to_string();
                    }
                    Some(symphonia::core::meta::StandardTagKey::Artist) => {
                        artist = tag.value.to_string();
                    }
                    Some(symphonia::core::meta::StandardTagKey::Album) => {
                        album = tag.value.to_string();
                    }
                    Some(symphonia::core::meta::StandardTagKey::TrackNumber) => {
                        track_number = tag.value.to_string().parse().ok();
                    }
                    _ => {}
                }
            }
        }
    }

    (title, artist, album, track_number, duration_secs)
}
