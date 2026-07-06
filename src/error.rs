use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum NusicError {
    TrackNotFound(PathBuf),
    UnsupportedFormat(String),
}

impl fmt::Display for NusicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TrackNotFound(p) => write!(f, "track not found: {}", p.display()),
            Self::UnsupportedFormat(ext) => write!(f, "unsupported audio format: {ext}"),
        }
    }
}

impl std::error::Error for NusicError {}

pub type Result<T> = std::result::Result<T, NusicError>;
