//! Fixed SSH-service controls. Call from a worker, never from the render loop.
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug)]
pub struct Status {
    pub running: bool,
    pub on_boot: bool,
    pub login: String,
    pub address: Option<Ipv4Addr>,
    pub simulated: bool,
}

static SIM_RUNNING: AtomicBool = AtomicBool::new(false);
static SIM_BOOT: AtomicBool = AtomicBool::new(false);

fn simulated() -> bool {
    cfg!(not(target_os = "linux")) || crate::sim::is_sim()
}

pub fn status() -> Result<Status, String> {
    if simulated() {
        return Ok(Status {
            running: SIM_RUNNING.load(Ordering::Relaxed),
            on_boot: SIM_BOOT.load(Ordering::Relaxed),
            login: "ark".into(),
            address: Some(Ipv4Addr::new(192, 0, 2, 36)),
            simulated: true,
        });
    }
    #[cfg(target_os = "linux")]
    {
        linux::status()
    }
    #[cfg(not(target_os = "linux"))]
    {
        unreachable!()
    }
}

/// Enable/start or disable/stop the existing SSH unit. No other remote services,
/// credentials, firewall rules or privilege policies are changed.
pub fn configure(enabled: bool) -> Result<Status, String> {
    if simulated() {
        SIM_RUNNING.store(enabled, Ordering::Relaxed);
        SIM_BOOT.store(enabled, Ordering::Relaxed);
        return status();
    }
    #[cfg(target_os = "linux")]
    {
        linux::configure(enabled)
    }
    #[cfg(not(target_os = "linux"))]
    {
        unreachable!()
    }
}

#[cfg(any(target_os = "linux", test))]
fn properties(text: &str) -> Result<(bool, bool), String> {
    let value = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name).and_then(|v| v.strip_prefix('=')))
    };
    if value("LoadState") != Some("loaded") {
        return Err("SSH service is missing or masked".into());
    }
    let running = match value("ActiveState") {
        Some("active" | "reloading") => true,
        Some("inactive" | "failed" | "activating" | "deactivating") => false,
        _ => return Err("Cannot read SSH service state".into()),
    };
    let on_boot = match value("UnitFileState") {
        Some("enabled") => true,
        Some(
            "disabled" | "enabled-runtime" | "static" | "indirect" | "alias" | "linked"
            | "linked-runtime",
        ) => false,
        _ => return Err("Cannot read SSH startup state".into()),
    };
    Ok((running, on_boot))
}

#[cfg(any(target_os = "linux", test))]
fn address(text: &str) -> Option<Ipv4Addr> {
    text.lines().find_map(|line| {
        let parts: Vec<_> = line.split_whitespace().collect();
        let index = parts.iter().position(|&part| part == "inet")?;
        let ip: Ipv4Addr = parts.get(index + 1)?.split('/').next()?.parse().ok()?;
        (!ip.is_loopback() && !ip.is_link_local() && !ip.is_unspecified()).then_some(ip)
    })
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    fn command(program: &str, args: &[&str]) -> Result<String, String> {
        let child = Command::new(program)
            .args(args)
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Cannot run {program}: {e}"))?;
        let result = crate::device::output_with_timeout(child, Duration::from_secs(8))
            .map_err(|e| format!("Remote access: {e}"))?;
        if !result.status.success() {
            let reason = String::from_utf8_lossy(&result.stderr);
            return Err(format!(
                "Remote access: {}",
                reason.trim().chars().take(400).collect::<String>()
            ));
        }
        Ok(String::from_utf8_lossy(&result.stdout).into_owned())
    }

    fn service() -> Result<(&'static str, bool, bool), String> {
        let mut error = "No SSH service is installed".into();
        for unit in ["ssh.service", "sshd.service"] {
            match command(
                "/bin/systemctl",
                &[
                    "show",
                    "--no-pager",
                    "-p",
                    "LoadState",
                    "-p",
                    "ActiveState",
                    "-p",
                    "UnitFileState",
                    "--",
                    unit,
                ],
            )
            .and_then(|text| properties(&text))
            {
                Ok((running, on_boot)) => return Ok((unit, running, on_boot)),
                Err(reason) => error = reason,
            }
        }
        Err(error)
    }

    pub(super) fn status() -> Result<Status, String> {
        let (_, running, on_boot) = service()?;
        Ok(Status {
            running,
            on_boot,
            login: command("/usr/bin/id", &["-un"])?.trim().to_string(),
            address: command("/sbin/ip", &["-4", "-o", "addr", "show", "scope", "global"])
                .ok()
                .and_then(|text| address(&text)),
            simulated: false,
        })
    }

    pub(super) fn configure(enabled: bool) -> Result<Status, String> {
        let (unit, _, _) = service()?;
        let action = if enabled { "enable" } else { "disable" };
        if unsafe { libc::geteuid() } == 0 {
            command("/bin/systemctl", &[action, "--now", "--", unit])?;
        } else {
            // -n refuses a password prompt; failures remain visible in Cartridge.
            command(
                "/usr/bin/sudo",
                &["-n", "--", "/bin/systemctl", action, "--now", "--", unit],
            )?;
        }
        let result = status()?;
        if result.running != enabled || result.on_boot != enabled {
            return Err(
                "SSH did not reach the requested state. Press Y to refresh its actual status."
                    .into(),
            );
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn active_and_boot_are_independent_and_bad_status_is_not_off() {
        assert_eq!(
            properties("LoadState=loaded\nActiveState=active\nUnitFileState=disabled\n"),
            Ok((true, false))
        );
        assert_eq!(
            properties("UnitFileState=enabled\nActiveState=inactive\nLoadState=loaded\n"),
            Ok((false, true))
        );
        assert!(
            properties("LoadState=masked\nActiveState=inactive\nUnitFileState=masked\n").is_err()
        );
        assert!(properties("LoadState=loaded\nActiveState=active\n").is_err());
    }
    #[test]
    fn runtime_enable_is_not_persistent_startup() {
        assert_eq!(
            properties("LoadState=loaded\nActiveState=active\nUnitFileState=enabled-runtime\n"),
            Ok((true, false))
        );
    }
    #[test]
    fn connection_address_ignores_loopback_link_local_and_malformed_rows() {
        assert_eq!(address("1: lo inet 127.0.0.1/8\n2: eth0 inet 169.254.1.2/16\n3: wlan0 inet 192.168.178.159/24 scope global wlan0"), Some(Ipv4Addr::new(192,168,178,159)));
        assert_eq!(address("inet bad/24\ninet 0.0.0.0/0"), None);
    }
}
