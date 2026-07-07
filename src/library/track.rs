use std::fs;
use std::path::{Path, PathBuf};

use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, StandardTagKey, Tag, Value};
use symphonia::core::probe::Hint;

use crate::error::{NusicError, Result};

pub const MISSING: &str = "--";

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
            .unwrap_or_else(|| MISSING.to_string())
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
        .unwrap_or(MISSING)
        .to_string();

    let mut title = filename.clone();
    let mut artist = MISSING.to_string();
    let mut album = MISSING.to_string();
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

        let mut metadata = probed.format.metadata();
        let _ = metadata.skip_to_latest();
        if let Some(rev) = metadata.current() {
            for tag in rev.tags() {
                apply_tag(tag, &mut title, &mut artist, &mut album, &mut track_number);
            }
        }
    }

    if artist == MISSING {
        if let Some((parsed_artist, parsed_title)) = parse_filename_artist_title(&filename) {
            artist = parsed_artist;
            if title == filename {
                title = parsed_title;
            }
        }
    }

    (title, artist, album, track_number, duration_secs)
}

fn apply_tag(
    tag: &Tag,
    title: &mut String,
    artist: &mut String,
    album: &mut String,
    track_number: &mut Option<u32>,
) {
    if let Some(std_key) = tag.std_key {
        match std_key {
            StandardTagKey::TrackTitle => set_if_present(title, tag),
            StandardTagKey::Artist | StandardTagKey::AlbumArtist | StandardTagKey::Composer => {
                set_if_present(artist, tag);
            }
            StandardTagKey::Album => set_if_present(album, tag),
            StandardTagKey::TrackNumber | StandardTagKey::TrackTotal => {
                *track_number = parse_track_number(&tag_value_string(tag));
            }
            _ => {}
        }
        return;
    }

    let key = tag.key.to_ascii_lowercase();
    match key.as_str() {
        "title" | "tracktitle" | "name" | "©nam" | "tit2" => set_if_present(title, tag),
        "artist" | "albumartist" | "album artist" | "performer" | "©art" | "tpe1" | "tpe2" => {
            set_if_present(artist, tag);
        }
        "album" | "albumtitle" | "©alb" | "talb" => set_if_present(album, tag),
        "tracknumber" | "track" | "trck" => {
            *track_number = parse_track_number(&tag_value_string(tag));
        }
        _ => {}
    }
}

fn set_if_present(field: &mut String, tag: &Tag) {
    let value = tag_value_string(tag);
    if !value.trim().is_empty() {
        *field = value;
    }
}

fn tag_value_string(tag: &Tag) -> String {
    match &tag.value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn parse_track_number(raw: &str) -> Option<u32> {
    raw.split('/')
        .next()
        .unwrap_or(raw)
        .trim()
        .parse()
        .ok()
}

/// Parse common download filenames like `Artist - Title` or `A,B,C - Title`.
fn parse_filename_artist_title(stem: &str) -> Option<(String, String)> {
    for sep in [" - ", " – ", " — "] {
        if let Some((left, right)) = stem.split_once(sep) {
            let parsed_artist = left.trim();
            let parsed_title = right.trim();
            if !parsed_artist.is_empty() && !parsed_title.is_empty() {
                return Some((parsed_artist.to_string(), parsed_title.to_string()));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_netease_style_filename() {
        let (artist, title) =
            parse_filename_artist_title("张碧晨 - 光的方向 (Live)").unwrap();
        assert_eq!(artist, "张碧晨");
        assert_eq!(title, "光的方向 (Live)");

        let (artist, title) =
            parse_filename_artist_title("Ciyo,见过夏天P,乌托邦P - 拼接乌托邦").unwrap();
        assert_eq!(artist, "Ciyo,见过夏天P,乌托邦P");
        assert_eq!(title, "拼接乌托邦");
    }
}
