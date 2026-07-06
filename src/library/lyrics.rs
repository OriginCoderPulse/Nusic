use std::fs;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct LrcLine {
    pub time: Duration,
    pub text: String,
}

pub fn load_for_track(audio_path: &Path) -> Option<Vec<LrcLine>> {
    let lrc_path = audio_path.with_extension("lrc");
    let content = fs::read_to_string(lrc_path).ok()?;
    parse_lrc(&content).ok()
}

fn parse_lrc(content: &str) -> Result<Vec<LrcLine>, ()> {
    let mut lines = Vec::new();

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        let mut rest = line;
        while rest.starts_with('[') {
            let Some(end) = rest.find(']') else { break };
            let tag = &rest[1..end];
            rest = &rest[end + 1..];

            if let Some(time) = parse_lrc_timestamp(tag) {
                let text = rest.trim().to_string();
                if !text.is_empty() {
                    lines.push(LrcLine { time, text });
                }
            }
        }
    }

    lines.sort_by_key(|l| l.time);
    if lines.is_empty() {
        Err(())
    } else {
        Ok(lines)
    }
}

fn parse_lrc_timestamp(tag: &str) -> Option<Duration> {
    let (time_part, frac) = match tag.split_once('.') {
        Some((tp, f)) => (tp, Some(f)),
        None => (tag, None),
    };

    let parts: Vec<&str> = time_part.split(':').collect();
    let base_secs = match parts.as_slice() {
        [min, sec] => {
            let min: u64 = min.parse().ok()?;
            let sec: u64 = sec.parse().ok()?;
            min * 60 + sec
        }
        [hour, min, sec] => {
            let hour: u64 = hour.parse().ok()?;
            let min: u64 = min.parse().ok()?;
            let sec: u64 = sec.parse().ok()?;
            hour * 3600 + min * 60 + sec
        }
        _ => return None,
    };

    Some(Duration::from_secs(base_secs) + parse_fraction(frac))
}

fn parse_fraction(frac: Option<&str>) -> Duration {
    let Some(frac) = frac else {
        return Duration::ZERO;
    };
    let digits: String = frac
        .chars()
        .take(3)
        .filter(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return Duration::ZERO;
    }
    let n: u64 = digits.parse().unwrap_or(0);
    let ms = match digits.len() {
        1 => n * 100,
        2 => n * 10,
        _ => n,
    };
    Duration::from_millis(ms)
}

pub fn segment_fraction(lines: &[LrcLine], position: Duration, active: usize) -> f32 {
    if active + 1 >= lines.len() {
        return 0.0;
    }
    let t0 = lines[active].time;
    let t1 = lines[active + 1].time;
    if t1 <= t0 {
        return 0.0;
    }
    let pos = position.saturating_sub(t0).as_secs_f64();
    let span = (t1 - t0).as_secs_f64();
    (pos / span).clamp(0.0, 1.0) as f32
}

pub fn active_index(lines: &[LrcLine], position: Duration) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }
    let mut idx = 0;
    for (i, line) in lines.iter().enumerate() {
        if line.time <= position {
            idx = i;
        } else {
            break;
        }
    }
    Some(idx)
}
