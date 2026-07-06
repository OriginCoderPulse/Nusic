use std::path::Path;

use walkdir::WalkDir;

use super::track::Track;

pub fn scan_path(path: &Path) -> Vec<Track> {
    let mut tracks = Vec::new();

    if path.is_file() {
        if let Ok(track) = Track::from_path(path.to_path_buf()) {
            tracks.push(track);
        }
        return tracks;
    }

    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path().to_path_buf();
        if let Ok(track) = Track::from_path(p) {
            tracks.push(track);
        }
    }

    tracks.sort_by(|a, b| {
        a.album
            .cmp(&b.album)
            .then(a.track_number.cmp(&b.track_number))
            .then(a.title.cmp(&b.title))
    });

    tracks
}
