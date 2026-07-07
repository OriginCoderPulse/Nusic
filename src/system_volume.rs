use std::process::Command;

/// Adjust system output volume by `delta` (e.g. 0.05 = 5%).
pub fn adjust(delta: f32) -> Result<(), String> {
    if delta.abs() < f32::EPSILON {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    return macos::adjust(delta);

    #[cfg(target_os = "linux")]
    return linux::adjust(delta);

    #[cfg(target_os = "windows")]
    return windows::adjust(delta);

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    Err("system volume control is not supported on this platform".into())
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    pub fn adjust(delta: f32) -> Result<(), String> {
        let current = get_percent()?;
        let next = (current + delta * 100.0).round().clamp(0.0, 100.0) as i32;
        set_percent(next)
    }

    fn get_percent() -> Result<f32, String> {
        let output = Command::new("osascript")
            .args(["-e", "output volume of (get volume settings)"])
            .output()
            .map_err(|e| format!("failed to read system volume: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "failed to read system volume: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<f32>()
            .map_err(|_| "failed to parse system volume".into())
    }

    fn set_percent(percent: i32) -> Result<(), String> {
        let script = format!("set volume output volume {percent}");
        let output = Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| format!("failed to set system volume: {e}"))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "failed to set system volume: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;

    pub fn adjust(delta: f32) -> Result<(), String> {
        if try_wpctl(delta)? {
            return Ok(());
        }
        if try_pactl(delta)? {
            return Ok(());
        }
        try_amixer(delta)
    }

    fn try_wpctl(delta: f32) -> Result<bool, String> {
        if Command::new("wpctl").arg("--version").output().is_err() {
            return Ok(false);
        }

        let change = format!("{:.2}{}", delta.abs(), if delta >= 0.0 { "+" } else { "-" });
        let output = Command::new("wpctl")
            .args(["set-volume", "@DEFAULT_AUDIO_SINK@", &change])
            .output()
            .map_err(|e| format!("failed to set system volume: {e}"))?;

        if output.status.success() {
            Ok(true)
        } else {
            Err(format!(
                "failed to set system volume: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }

    fn try_pactl(delta: f32) -> Result<bool, String> {
        if Command::new("pactl").arg("--version").output().is_err() {
            return Ok(false);
        }

        let change = format!("{:+.0}%", delta * 100.0);
        let output = Command::new("pactl")
            .args(["set-sink-volume", "@DEFAULT_SINK@", &change])
            .output()
            .map_err(|e| format!("failed to set system volume: {e}"))?;

        if output.status.success() {
            Ok(true)
        } else {
            Err(format!(
                "failed to set system volume: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }

    fn try_amixer(delta: f32) -> Result<(), String> {
        let change = format!("{:+.0}%", delta * 100.0);
        let output = Command::new("amixer")
            .args(["set", "Master", &change])
            .output()
            .map_err(|e| format!("failed to set system volume: {e}"))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "failed to set system volume: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;

    pub fn adjust(delta: f32) -> Result<(), String> {
        let steps = (delta.abs() * 20.0).round().max(1.0) as i32;
        let key = if delta > 0.0 { 175 } else { 174 };
        let script = format!(
            "$sh = New-Object -ComObject WScript.Shell; \
             1..{steps} | ForEach-Object {{ $sh.SendKeys([char]{key}) }}"
        );

        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .output()
            .map_err(|e| format!("failed to set system volume: {e}"))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "failed to set system volume: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }
}
