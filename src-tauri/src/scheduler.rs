use std::path::PathBuf;
use std::process::Command;

pub const LABEL: &str = "com.darkmatter.system-design-roulette";

fn plist_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/LaunchAgents").join(format!("{LABEL}.plist")))
}

fn current_exe_path() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "/Applications/system-design-roulette.app/Contents/MacOS/system-design-roulette".into())
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

/// Write the LaunchAgent plist and (re)load it in the gui domain.
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

/// Self-heal: if the installed plist points at a different binary than the one
/// running (e.g. setup was completed from a dev build, then the user switched
/// to the installed app), rewrite it for the current executable.
pub fn ensure_current(hour: u32, minute: u32) {
    let Some(path) = plist_path() else { return };
    let exe = current_exe_path();
    // Missing plist (manually removed) or one pointing at a stale binary:
    // reinstall. Callers skip this entirely while the schedule is paused.
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
    // SAFETY: getuid is always safe to call.
    unsafe { libc_getuid() }
}

extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid() -> u32;
}
