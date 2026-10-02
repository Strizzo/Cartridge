//! Device hardware controls: backlight brightness and audio volume.
//!
//! These APIs are synchronous. UI callers should run the fallible variants on a
//! worker; the original getters/setters retain their fallback/no-op contracts.
//! The simulator uses the same API, backed by its process-wide device profile.

#[cfg(target_os = "linux")]
use std::path::PathBuf;

#[cfg(target_os = "linux")]
fn backlight_dir() -> Result<PathBuf, String> {
    let entries = std::fs::read_dir("/sys/class/backlight")
        .map_err(|e| format!("Cannot find backlight: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.join("brightness").exists() && path.join("max_brightness").exists() {
            return Ok(path);
        }
    }
    Err("No backlight control available".into())
}

#[cfg(target_os = "linux")]
fn read_backlight(dir: &std::path::Path, name: &str) -> Result<u32, String> {
    std::fs::read_to_string(dir.join(name))
        .map_err(|e| format!("Cannot read {name}: {e}"))?
        .trim()
        .parse()
        .map_err(|_| format!("Invalid {name} value"))
}

#[cfg(any(target_os = "linux", test))]
fn brightness_percent(current: u32, max: u32) -> Result<u8, String> {
    if max == 0 {
        return Err("Invalid max_brightness: zero".into());
    }
    Ok((u64::from(current) * 100 / u64::from(max)).min(100) as u8)
}

/// Read brightness, reporting missing hardware, permissions and invalid data.
pub fn try_get_brightness_percent() -> Result<u8, String> {
    if crate::sim::is_sim() {
        return Ok(crate::sim::brightness());
    }
    #[cfg(target_os = "linux")]
    {
        let dir = backlight_dir()?;
        brightness_percent(
            read_backlight(&dir, "brightness")?,
            read_backlight(&dir, "max_brightness")?,
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(crate::sim::brightness())
    }
}

/// Get brightness as 0..100, falling back to 100 when unavailable.
pub fn get_brightness_percent() -> u8 {
    try_get_brightness_percent().unwrap_or(100)
}

/// Set brightness, reporting failures. The physical backlight retains its
/// historical minimum of one hardware unit so the display stays usable.
pub fn try_set_brightness_percent(pct: u8) -> Result<(), String> {
    let pct = pct.min(100);
    if crate::sim::is_sim() {
        crate::sim::set_brightness(pct);
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        let dir = backlight_dir()?;
        let max = read_backlight(&dir, "max_brightness")?;
        brightness_percent(0, max)?; // Reject a broken driver value, not a silent no-op.
        let target = (u64::from(pct) * u64::from(max) / 100).max(1);
        std::fs::write(dir.join("brightness"), target.to_string())
            .map_err(|e| format!("Cannot set brightness: {e}"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        crate::sim::set_brightness(pct);
        Ok(())
    }
}

/// Set brightness as 0..100. Clamped; errors are ignored for legacy callers.
pub fn set_brightness_percent(pct: u8) {
    let _ = try_set_brightness_percent(pct);
}

#[cfg(any(target_os = "linux", test))]
fn parse_volume(text: &str) -> Result<u8, String> {
    text.split('[')
        .skip(1)
        .filter_map(|part| part.split_once("%]").map(|(value, _)| value))
        .find_map(|value| value.parse::<u32>().ok())
        .map(|value| value.min(100) as u8)
        .ok_or_else(|| "Mixer returned no volume percentage".into())
}

/// Collect small command output without blocking on either a child or its
/// pipes. The caller supplies piped stdout/stderr. Errors (including timeout)
/// kill and reap the child before the persistent hardware worker takes more work.
#[cfg(any(target_os = "linux", all(test, unix)))]
fn output_with_timeout(
    mut child: std::process::Child,
    timeout: std::time::Duration,
) -> std::io::Result<std::process::Output> {
    use std::io::{self, Read};
    use std::os::fd::AsRawFd;
    use std::time::{Duration, Instant};

    fn nonblocking(pipe: &impl AsRawFd) -> io::Result<()> {
        // These descriptors are owned by this helper until collection finishes.
        let flags = unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_GETFL) };
        if flags == -1
            || unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) }
                == -1
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn drain(pipe: &mut impl Read, output: &mut Vec<u8>) -> io::Result<()> {
        // amixer output is tiny. Bound both retained data and work per poll so a
        // noisy process cannot consume unlimited memory or starve the deadline.
        const OUTPUT_LIMIT: usize = 64 * 1024;
        let mut buffer = [0; 4096];
        for _ in 0..16 {
            match pipe.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let keep = count.min(OUTPUT_LIMIT.saturating_sub(output.len()));
                    output.extend_from_slice(&buffer[..keep]);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    let result = (|| {
        let started = Instant::now();
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("stdout is not piped"))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("stderr is not piped"))?;
        nonblocking(&stdout)?;
        nonblocking(&stderr)?;
        let mut output = Vec::new();
        let mut errors = Vec::new();
        loop {
            drain(&mut stdout, &mut output)?;
            drain(&mut stderr, &mut errors)?;
            if let Some(status) = child.try_wait()? {
                // Capture final bytes written between the last drain and exit;
                // never wait for EOF from an inherited pipe in another process.
                drain(&mut stdout, &mut output)?;
                drain(&mut stderr, &mut errors)?;
                return Ok(std::process::Output {
                    status,
                    stdout: output,
                    stderr: errors,
                });
            }
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("amixer timed out after {:.1}s", timeout.as_secs_f32()),
                ));
            }
            std::thread::sleep(remaining.min(Duration::from_millis(10)));
        }
    })();
    if result.is_err() {
        // kill can race a normal exit; wait is still required to reap that child.
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

#[cfg(target_os = "linux")]
fn amixer(args: &[&str]) -> Result<std::process::Output, String> {
    let child = std::process::Command::new("amixer")
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Cannot run amixer: {e}"))?;
    let output = output_with_timeout(child, std::time::Duration::from_secs(3))
        .map_err(|e| format!("Mixer: {e}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.trim();
        return Err(if detail.is_empty() {
            format!("Mixer failed ({})", output.status)
        } else {
            format!("Mixer: {detail}")
        });
    }
    Ok(output)
}

/// Read master volume, reporting unavailable mixers and malformed responses.
pub fn try_get_volume_percent() -> Result<u8, String> {
    if crate::sim::is_sim() {
        return Ok(crate::sim::volume());
    }
    #[cfg(target_os = "linux")]
    {
        let output = amixer(&["-M", "sget", "Master"])?;
        parse_volume(&String::from_utf8_lossy(&output.stdout))
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(crate::sim::volume())
    }
}

/// Get master volume as 0..100, falling back to 50 when unavailable.
pub fn get_volume_percent() -> u8 {
    try_get_volume_percent().unwrap_or(50)
}

/// Set master volume, reporting command launch and nonzero-exit failures.
pub fn try_set_volume_percent(pct: u8) -> Result<(), String> {
    let pct = pct.min(100);
    if crate::sim::is_sim() {
        crate::sim::set_volume(pct);
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        amixer(&["-q", "-M", "sset", "Master", &format!("{pct}%")])?;
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        crate::sim::set_volume(pct);
        Ok(())
    }
}

/// Set master volume as 0..100. Clamped; errors are ignored for legacy callers.
pub fn set_volume_percent(pct: u8) {
    let _ = try_set_volume_percent(pct);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_rejects_zero_max_and_handles_large_driver_values() {
        assert!(brightness_percent(1, 0).is_err());
        assert_eq!(brightness_percent(128, 255), Ok(50));
        assert_eq!(brightness_percent(u32::MAX, u32::MAX), Ok(100));
        assert_eq!(brightness_percent(500, 100), Ok(100));
    }

    #[test]
    fn mixer_parser_requires_a_complete_percentage_and_uses_first_channel() {
        assert_eq!(
            parse_volume("Front Left: 42 [65%] [-10.00dB] [on]\nRight: [66%]"),
            Ok(65)
        );
        assert_eq!(parse_volume("[off] [150%]"), Ok(100));
        assert!(parse_volume("[50% malformed]").is_err());
        assert!(parse_volume("[on] [-10.00dB]").is_err());
    }

    #[cfg(unix)]
    fn assert_reaped(pid: u32) {
        let mut status = 0;
        // A zombie would return its PID; a still-running child would return 0.
        assert_eq!(
            unsafe { libc::waitpid(pid as libc::pid_t, &mut status, libc::WNOHANG) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
    }

    #[cfg(unix)]
    #[test]
    fn mixer_timeout_kills_reaps_and_allows_the_next_command() {
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};

        let child = Command::new("sleep")
            .arg("30")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let started = Instant::now();
        let error = output_with_timeout(child, Duration::from_millis(80)).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(error.to_string().contains("amixer timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_reaped(pid);

        // The same path remains usable and retains stdout, stderr and exit code.
        let child = Command::new("sh")
            .args([
                "-c",
                "printf '%s' '[65%]'; printf '%s' 'mixer failure' >&2; exit 7",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let output = output_with_timeout(child, Duration::from_secs(2)).unwrap();
        assert_eq!(output.status.code(), Some(7));
        assert_eq!(output.stdout, b"[65%]");
        assert_eq!(output.stderr, b"mixer failure");
        assert_reaped(pid);
    }

    #[cfg(unix)]
    #[test]
    fn mixer_timeout_also_bounds_a_noisy_process_with_full_pipes() {
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};

        // Shell builtins only: no descendants left behind by this fake mixer.
        let child = Command::new("sh")
            .args(["-c", "while :; do printf 'noise'; printf 'error' >&2; done"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let started = Instant::now();
        let error = output_with_timeout(child, Duration::from_millis(80)).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_reaped(pid);
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn legacy_and_fallible_controls_share_simulator_state() {
        let old_brightness = get_brightness_percent();
        let old_volume = get_volume_percent();
        try_set_brightness_percent(43).unwrap();
        set_volume_percent(37);
        assert_eq!(get_brightness_percent(), 43);
        assert_eq!(try_get_volume_percent(), Ok(37));
        set_brightness_percent(255);
        try_set_volume_percent(255).unwrap();
        assert_eq!(try_get_brightness_percent(), Ok(100));
        assert_eq!(get_volume_percent(), 100);
        set_brightness_percent(old_brightness);
        set_volume_percent(old_volume);
    }
}
