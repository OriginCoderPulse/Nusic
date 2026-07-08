use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::time::Duration;

use rodio::Source;
use symphonia::core::audio::{AudioBufferRef, SampleBuffer, SignalSpec};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;
use symphonia::default::{get_codecs, get_probe};

const MAX_DECODE_RETRIES: usize = 3;
const DRM_SCAN_BYTES: usize = 4 * 1024 * 1024;

/// Apple Music / iTunes FairPlay DRM leaves `drms` atoms in the MP4 container.
pub fn is_fairplay_drm(data: &[u8]) -> bool {
    let scan = data.len().min(DRM_SCAN_BYTES);
    let head = &data[..scan];
    head.windows(4).any(|w| w == b"drms")
}

fn fairplay_error(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("this file");
    format!(
        "{name} is protected by Apple FairPlay DRM and cannot be played.\n\
         Apple Music downloads (.m4p) are encrypted — use Apple Music, or add MP3/FLAC/unprotected M4A."
    )
}

/// Symphonia-backed source that avoids rodio's gapless-init seek panic on m4a/m4p.
pub struct SymphoniaSource {
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    format: Box<dyn FormatReader>,
    track_id: u32,
    spec: SignalSpec,
    sample_rate: u32,
    channels: u16,
    total_duration: Option<Duration>,
    buffer: SampleBuffer<f32>,
    frame_offset: usize,
}

pub fn open_decoder(path: &Path) -> Result<SymphoniaSource, String> {
    open_decoder_at(path, Duration::ZERO)
}

pub fn open_decoder_at(path: &Path, seek_to: Duration) -> Result<SymphoniaSource, String> {
    let data = fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let drm_protected = is_fairplay_drm(&data);
    let cursor = Cursor::new(data);
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    // Gapless init seeks into MP4/M4P and rodio panics when that fails.
    let format_opts = FormatOptions {
        enable_gapless: false,
        ..Default::default()
    };
    let metadata_opts = MetadataOptions::default();

    let mut probed = get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .map_err(|e| decode_error(path, e))?;

    let track = probed
        .format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| {
            if drm_protected {
                fairplay_error(path)
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("m4p"))
            {
                fairplay_error(path)
            } else {
                format!("no supported audio track in {}", path.display())
            }
        })?;

    let track_id = track.id;
    let time_base = track.codec_params.time_base;
    let n_frames = track.codec_params.n_frames;
    let mut decoder = get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| decode_error(path, e))?;

    if !seek_to.is_zero() {
        let seek = SeekTo::Time {
            time: duration_to_time(seek_to),
            track_id: Some(track_id),
        };
        if let Err(e) = probed.format.seek(SeekMode::Accurate, seek) {
            return Err(decode_error(path, e));
        }
        decoder.reset();
    }

    let total_duration = time_base
        .zip(n_frames)
        .map(|(base, frames)| time_to_duration(base.calc_time(frames)));

    let mut decode_errors = 0usize;
    let (spec, buffer) = loop {
        let packet = match probed.format.next_packet() {
            Ok(p) => p,
            Err(Error::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(Error::IoError(_)) => {
                return Err(format!("no decodable audio in {}", path.display()));
            }
            Err(e) => return Err(decode_error(path, e)),
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let buffer = interleaved_f32(decoded, &spec);
                break (spec, buffer);
            }
            Err(Error::DecodeError(_)) => {
                decode_errors += 1;
                if decode_errors > MAX_DECODE_RETRIES {
                    return Err(format!(
                        "failed to decode {}: too many decode errors",
                        path.display()
                    ));
                }
            }
            Err(Error::ResetRequired) => {
                decoder.reset();
            }
            Err(e) => return Err(decode_error(path, e)),
        }
    };

    Ok(SymphoniaSource {
        decoder,
        format: probed.format,
        track_id,
        spec,
        sample_rate: spec.rate,
        channels: spec.channels.count() as u16,
        total_duration,
        buffer,
        frame_offset: 0,
    })
}

fn decode_error(path: &Path, err: Error) -> String {
    let detail = match &err {
        Error::SeekError(msg) => {
            format!("seek error (file may be DRM-protected or damaged): {msg:?}")
        }
        Error::Unsupported(msg) => format!("unsupported: {msg}"),
        Error::DecodeError(msg) => format!("decode error: {msg}"),
        Error::IoError(e) => format!("io error: {e}"),
        other => format!("{other}"),
    };
    format!("failed to decode {}: {detail}", path.display())
}

fn time_to_duration(time: Time) -> Duration {
    let secs = time.seconds as f64 + if time.frac > 0.0 { 1.0 / time.frac as f64 } else { 0.0 };
    Duration::from_secs_f64(secs)
}

fn duration_to_time(duration: Duration) -> Time {
    Time {
        seconds: duration.as_secs(),
        frac: 0.0,
    }
}

fn interleaved_f32(decoded: AudioBufferRef<'_>, spec: &SignalSpec) -> SampleBuffer<f32> {
    let capacity = decoded.capacity() as u64;
    let mut buffer = SampleBuffer::<f32>::new(capacity, *spec);
    buffer.copy_interleaved_ref(decoded);
    buffer
}

impl Iterator for SymphoniaSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.frame_offset >= self.buffer.len() {
            self.refill()?;
        }
        let sample = *self.buffer.samples().get(self.frame_offset)?;
        self.frame_offset += 1;
        Some(sample)
    }
}

impl SymphoniaSource {
    fn refill(&mut self) -> Option<()> {
        let mut decode_errors = 0usize;
        loop {
            let packet = self.format.next_packet().ok()?;
            if packet.track_id() != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    decoded.spec().clone_into(&mut self.spec);
                    self.buffer = interleaved_f32(decoded, &self.spec);
                    self.frame_offset = 0;
                    return Some(());
                }
                Err(Error::DecodeError(_)) => {
                    decode_errors += 1;
                    if decode_errors > MAX_DECODE_RETRIES {
                        return None;
                    }
                }
                Err(Error::ResetRequired) => {
                    self.decoder.reset();
                }
                Err(_) => return None,
            }
        }
    }
}

impl Source for SymphoniaSource {
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.buffer.len().saturating_sub(self.frame_offset))
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        self.total_duration
    }
}
