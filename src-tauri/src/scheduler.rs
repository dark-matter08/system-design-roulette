//! Daily auto-trigger scheduling, one backend per OS.
//!
//! All three backends register the installed binary to launch with `--triggered`
//! at the configured hour/minute, and self-heal if the registration points at a
//! stale path. The public API is platform-agnostic:
//!
//! - macOS  → launchd LaunchAgent (`~/Library/LaunchAgents/<LABEL>.plist`)
//! - Linux  → systemd user timer (`~/.config/systemd/user/<UNIT>.{service,timer}`)
//! - Windows→ Task Scheduler entry (`schtasks`, task name = PRODUCT)

pub const LABEL: &str = "com.darkmatter.system-design-roulette";
pub const PRODUCT: &str = "System Design Roulette";

/// Absolute path to the binary the scheduler should launch. Falls back to a
/// sensible per-OS default if `current_exe()` is somehow unavailable.
fn current_exe_path() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| imp::default_exe_path())
}

// ── Public, platform-agnostic API ──────────────────────────────────────────

/// Write/refresh the OS schedule and (re)activate it.
pub fn install(hour: u32, minute: u32) -> Result<(), String> {
    imp::install(hour, minute)
}

/// Remove the OS schedule. Best-effort; missing entries are not an error.
pub fn uninstall() -> Result<(), String> {
    imp::uninstall()
}

/// Whether a schedule is currently registered.
pub fn is_installed() -> bool {
    imp::is_installed()
}

/// Self-heal: if the registered schedule points at a different binary than the
/// one running (e.g. setup ran from a dev build, then the user installed the
/// app), rewrite it for the current executable. Callers skip this while paused.
pub fn ensure_current(hour: u32, minute: u32) {
    imp::ensure_current(hour, minute)
}

// ── macOS: launchd ─────────────────────────────────────────────────────────
#[cfg(target_os = "macos")]
mod imp {
    use super::{current_exe_path, LABEL};
    use std::path::PathBuf;
    use std::process::Command;

    pub fn default_exe_path() -> String {
        "/Applications/system-design-roulette.app/Contents/MacOS/system-design-roulette".into()
    }

    fn plist_path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join("Library/LaunchAgents").join(format!("{LABEL}.plist")))
    }

    pub fn plist_contents(hour: u32, minute: u32) -> String {
        let exe = current_exe_path();
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
    <string>--triggered</string>
  </array>
  <key>StartCalendarInterval</key>
  <dict>
    <key>Hour</key><integer>{hour}</integer>
    <key>Minute</key><integer>{minute}</integer>
  </dict>
  <key>RunAtLoad</key><true/>
  <key>ProcessType</key><string>Interactive</string>
  <key>StandardOutPath</key><string>/tmp/sdroulette.launchd.log</string>
  <key>StandardErrorPath</key><string>/tmp/sdroulette.launchd.log</string>
</dict>
</plist>
"#
        )
    }

    pub fn install(hour: u32, minute: u32) -> Result<(), String> {
        let path = plist_path().ok_or("no HOME")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, plist_contents(hour, minute)).map_err(|e| e.to_string())?;
        let uid = get_uid();
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("gui/{uid}/{LABEL}")])
            .output();
        let out = Command::new("launchctl")
            .args(["bootstrap", &format!("gui/{uid}"), &path.to_string_lossy()])
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            // "already bootstrapped" style errors are fine after bootout race
            if !err.contains("Bootstrap failed: 5") {
                return Err(format!("launchctl bootstrap failed: {err}"));
            }
        }
        Ok(())
    }

    pub fn uninstall() -> Result<(), String> {
        let uid = get_uid();
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("gui/{uid}/{LABEL}")])
            .output();
        if let Some(path) = plist_path() {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    pub fn is_installed() -> bool {
        plist_path().map(|p| p.exists()).unwrap_or(false)
    }

    pub fn ensure_current(hour: u32, minute: u32) {
        let Some(path) = plist_path() else { return };
        let exe = current_exe_path();
        let needs_install = match std::fs::read_to_string(&path) {
            Ok(existing) => !existing.contains(&exe),
            Err(_) => true,
        };
        if needs_install {
            log::info!("launchd plist missing/stale; reinstalling for {exe}");
            if let Err(e) = install(hour, minute) {
                log::warn!("launchd self-heal failed: {e}");
            }
        }
    }

    fn get_uid() -> u32 {
        // SAFETY: getuid is always safe to call. Declared here (not at module
        // scope) so the symbol is only linked on macOS, where it exists.
        unsafe { libc_getuid() }
    }

    extern "C" {
        #[link_name = "getuid"]
        fn libc_getuid() -> u32;
    }
}

// ── Linux: systemd user timer ──────────────────────────────────────────────
#[cfg(target_os = "linux")]
mod imp {
    use super::current_exe_path;
    use std::path::PathBuf;
    use std::process::Command;

    const UNIT: &str = "system-design-roulette";

    pub fn default_exe_path() -> String {
        "/usr/bin/system-design-roulette".into()
    }

    fn unit_dir() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("systemd/user"))
    }

    fn service_path() -> Option<PathBuf> {
        Some(unit_dir()?.join(format!("{UNIT}.service")))
    }

    fn timer_path() -> Option<PathBuf> {
        Some(unit_dir()?.join(format!("{UNIT}.timer")))
    }

    fn service_contents() -> String {
        let exe = current_exe_path();
        format!(
            "[Unit]\n\
             Description=System Design Roulette daily session\n\n\
             [Service]\n\
             Type=simple\n\
             ExecStart={exe} --triggered\n"
        )
    }

    fn timer_contents(hour: u32, minute: u32) -> String {
        format!(
            "[Unit]\n\
             Description=Daily System Design Roulette trigger\n\n\
             [Timer]\n\
             OnCalendar=*-*-* {hour:02}:{minute:02}:00\n\
             Persistent=true\n\n\
             [Install]\n\
             WantedBy=timers.target\n"
        )
    }

    fn systemctl(args: &[&str]) -> Result<(), String> {
        let out = Command::new("systemctl")
            .arg("--user")
            .args(args)
            .output()
            .map_err(|e| format!("systemctl not available: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    pub fn install(hour: u32, minute: u32) -> Result<(), String> {
        let dir = unit_dir().ok_or("no HOME/XDG_CONFIG_HOME")?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(service_path().unwrap(), service_contents()).map_err(|e| e.to_string())?;
        std::fs::write(timer_path().unwrap(), timer_contents(hour, minute))
            .map_err(|e| e.to_string())?;
        let _ = systemctl(&["daemon-reload"]);
        systemctl(&["enable", "--now", &format!("{UNIT}.timer")])
    }

    pub fn uninstall() -> Result<(), String> {
        let _ = systemctl(&["disable", "--now", &format!("{UNIT}.timer")]);
        if let Some(p) = timer_path() {
            let _ = std::fs::remove_file(p);
        }
        if let Some(p) = service_path() {
            let _ = std::fs::remove_file(p);
        }
        let _ = systemctl(&["daemon-reload"]);
        Ok(())
    }

    pub fn is_installed() -> bool {
        timer_path().map(|p| p.exists()).unwrap_or(false)
    }

    pub fn ensure_current(hour: u32, minute: u32) {
        let Some(svc) = service_path() else { return };
        let exe = current_exe_path();
        let needs_install = match std::fs::read_to_string(&svc) {
            Ok(existing) => !existing.contains(&exe),
            Err(_) => true,
        };
        if needs_install {
            log::info!("systemd unit missing/stale; reinstalling for {exe}");
            if let Err(e) = install(hour, minute) {
                log::warn!("systemd self-heal failed: {e}");
            }
        }
    }
}

// ── Windows: Task Scheduler (schtasks) ─────────────────────────────────────
#[cfg(target_os = "windows")]
mod imp {
    use super::{current_exe_path, PRODUCT};
    use std::process::Command;

    pub fn default_exe_path() -> String {
        "system-design-roulette.exe".into()
    }

    pub fn install(hour: u32, minute: u32) -> Result<(), String> {
        let exe = current_exe_path();
        // /F overwrites an existing task, making install idempotent.
        let out = Command::new("schtasks")
            .args([
                "/Create",
                "/TN",
                PRODUCT,
                "/TR",
                &format!("\"{exe}\" --triggered"),
                "/SC",
                "DAILY",
                "/ST",
                &format!("{hour:02}:{minute:02}"),
                "/F",
            ])
            .output()
            .map_err(|e| format!("schtasks not available: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    pub fn uninstall() -> Result<(), String> {
        let _ = Command::new("schtasks")
            .args(["/Delete", "/TN", PRODUCT, "/F"])
            .output();
        Ok(())
    }

    pub fn is_installed() -> bool {
        Command::new("schtasks")
            .args(["/Query", "/TN", PRODUCT])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    pub fn ensure_current(hour: u32, minute: u32) {
        let exe = current_exe_path();
        // Read the task definition; reinstall if it doesn't reference this exe.
        let needs_install = match Command::new("schtasks")
            .args(["/Query", "/TN", PRODUCT, "/XML"])
            .output()
        {
            Ok(o) if o.status.success() => !String::from_utf8_lossy(&o.stdout).contains(&exe),
            _ => true,
        };
        if needs_install {
            log::info!("scheduled task missing/stale; reinstalling for {exe}");
            if let Err(e) = install(hour, minute) {
                log::warn!("schtasks self-heal failed: {e}");
            }
        }
    }
}

// ── Other targets: no-op (compiles, returns a clear error) ─────────────────
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
mod imp {
    pub fn default_exe_path() -> String {
        "system-design-roulette".into()
    }
    pub fn install(_hour: u32, _minute: u32) -> Result<(), String> {
        Err("scheduling is not supported on this platform".into())
    }
    pub fn uninstall() -> Result<(), String> {
        Ok(())
    }
    pub fn is_installed() -> bool {
        false
    }
    pub fn ensure_current(_hour: u32, _minute: u32) {}
}
