use crate::state::AppState;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager};

/// Silence the room when the lock engages: pause every scriptable media
/// player that's running, remember the system mute state, then mute.
/// Best-effort — failures are logged and ignored.
fn pause_media(state: &AppState) {
    let was_muted = std::process::Command::new("osascript")
        .args(["-e", "output muted of (get volume settings)"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true");
    *state.prev_muted.lock().unwrap() = was_muted;

    for app_name in ["Music", "Spotify", "TV", "QuickTime Player", "VLC", "IINA"] {
        let script = format!(
            "if application \"{app_name}\" is running then tell application \"{app_name}\" to pause"
        );
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    let _ = std::process::Command::new("osascript")
        .args(["-e", "set volume output muted true"])
        .spawn();
}

/// Restore the pre-lock mute state (only unmute if WE muted).
fn restore_media(state: &AppState) {
    if let Some(false) = *state.prev_muted.lock().unwrap() {
        let _ = std::process::Command::new("osascript")
            .args(["-e", "set volume output muted false"])
            .spawn();
    }
    *state.prev_muted.lock().unwrap() = None;
}

#[cfg(target_os = "macos")]
mod mac {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    // NSApplicationPresentationOptions bits
    pub const HIDE_DOCK: usize = 1 << 1;
    pub const HIDE_MENU_BAR: usize = 1 << 3;
    pub const DISABLE_PROCESS_SWITCHING: usize = 1 << 5;
    pub const DISABLE_FORCE_QUIT: usize = 1 << 6;
    pub const DISABLE_SESSION_TERMINATION: usize = 1 << 7;
    pub const DISABLE_HIDE_APPLICATION: usize = 1 << 8;

    // NSWindowCollectionBehavior bits
    pub const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
    pub const STATIONARY: usize = 1 << 4;
    pub const FULL_SCREEN_AUXILIARY: usize = 1 << 8;

    pub const SCREEN_SAVER_WINDOW_LEVEL: isize = 1000;

    pub unsafe fn ns_app() -> *mut AnyObject {
        let cls = objc2::class!(NSApplication);
        msg_send![cls, sharedApplication]
    }

    pub unsafe fn set_presentation_options(options: usize) {
        let app = ns_app();
        let _: () = msg_send![app, setPresentationOptions: options];
    }

    pub unsafe fn raise_window(ns_window: *mut AnyObject) {
        let _: () = msg_send![ns_window, setLevel: SCREEN_SAVER_WINDOW_LEVEL];
        let behavior = CAN_JOIN_ALL_SPACES | STATIONARY | FULL_SCREEN_AUXILIARY;
        let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
        let _: () = msg_send![ns_window, orderFrontRegardless];
    }

    pub unsafe fn reset_window(ns_window: *mut AnyObject) {
        let _: () = msg_send![ns_window, setLevel: 0isize];
    }

    pub unsafe fn frontmost_pid() -> i32 {
        let cls = objc2::class!(NSWorkspace);
        let ws: *mut AnyObject = msg_send![cls, sharedWorkspace];
        let front: *mut AnyObject = msg_send![ws, frontmostApplication];
        if front.is_null() {
            return -1;
        }
        msg_send![front, processIdentifier]
    }

    pub unsafe fn activate_self() {
        let app = ns_app();
        let _: () = msg_send![app, activateIgnoringOtherApps: true];
    }
}

/// Engage the kiosk lock on the main window and start the refocus loop.
pub fn engage(app: &AppHandle, state: &AppState) {
    if state.debug_day {
        log::info!("debug-day: kiosk engagement skipped");
        state.locked.store(true, Ordering::SeqCst);
        return;
    }
    // Safety interlock: never lock behind a webview that hasn't booted —
    // the escape hatch lives in it. The owed watcher retries once it's up.
    if !state.frontend_ready.load(Ordering::SeqCst) {
        log::warn!("kiosk engage refused: frontend not ready (white-screen guard)");
        return;
    }
    if state.locked.swap(true, Ordering::SeqCst) {
        return;
    }
    pause_media(state);
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_resizable(false);
    let _ = window.set_decorations(false);
    let _ = window.set_always_on_top(true);
    let _ = window.set_visible_on_all_workspaces(true);
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let size = monitor.size();
        let pos = monitor.position();
        let _ = window.set_position(tauri::PhysicalPosition::new(pos.x, pos.y));
        let _ = window.set_size(tauri::PhysicalSize::new(size.width, size.height));
    }
    let _ = window.set_focus();

    #[cfg(target_os = "macos")]
    {
        let win = window.clone();
        let _ = window.run_on_main_thread(move || {
            // Activate first: presentation options only apply (and may throw)
            // when set by the active application.
            let r = objc2::exception::catch(std::panic::AssertUnwindSafe(|| unsafe {
                mac::activate_self();
                if let Ok(ptr) = win.ns_window() {
                    mac::raise_window(ptr as *mut objc2::runtime::AnyObject);
                }
            }));
            if let Err(e) = r {
                log::error!("kiosk raise failed: {e:?}");
            }
            let r = objc2::exception::catch(std::panic::AssertUnwindSafe(|| unsafe {
                mac::set_presentation_options(
                    mac::HIDE_DOCK
                        | mac::HIDE_MENU_BAR
                        | mac::DISABLE_PROCESS_SWITCHING
                        | mac::DISABLE_FORCE_QUIT
                        | mac::DISABLE_SESSION_TERMINATION
                        | mac::DISABLE_HIDE_APPLICATION,
                );
            }));
            if let Err(e) = r {
                log::error!("kiosk presentation options failed: {e:?}");
            }
            log::info!("kiosk engaged");
        });
    }

    // Black out every other display with blanker windows.
    if let Ok(monitors) = window.available_monitors() {
        let primary = window.primary_monitor().ok().flatten();
        for (i, m) in monitors.iter().enumerate() {
            if let Some(p) = &primary {
                if p.position() == m.position() {
                    continue;
                }
            }
            let label = format!("blanker-{i}");
            if app.get_webview_window(&label).is_some() {
                continue;
            }
            let pos = m.position();
            let size = m.size();
            let scale = m.scale_factor();
            // NOTE: must be a real SvelteKit route — "index.html?x" makes the
            // SPA router resolve pathname /index.html and render its 404 page.
            let built = tauri::WebviewWindowBuilder::new(
                app,
                &label,
                tauri::WebviewUrl::App("blanker".into()),
            )
            .decorations(false)
            .resizable(false)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .position(pos.x as f64 / scale, pos.y as f64 / scale)
            .inner_size(size.width as f64 / scale, size.height as f64 / scale)
            .build();
            match built {
                Ok(w) => {
                    #[cfg(target_os = "macos")]
                    {
                        let w2 = w.clone();
                        let _ = w.run_on_main_thread(move || {
                            let _ = objc2::exception::catch(std::panic::AssertUnwindSafe(|| unsafe {
                                if let Ok(ptr) = w2.ns_window() {
                                    mac::raise_window(ptr as *mut objc2::runtime::AnyObject);
                                }
                            }));
                        });
                    }
                }
                Err(e) => log::warn!("blanker {label} failed: {e}"),
            }
        }
    }

    // Refocus loop: snap focus back while locked.
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let my_pid = std::process::id() as i32;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            let state = app2.state::<AppState>();
            if !state.locked.load(Ordering::SeqCst) {
                break;
            }
            // Dev back door: unlock immediately if ~/sdr-unlock exists.
            if let Some(home) = std::env::var_os("HOME") {
                if std::path::Path::new(&home).join("sdr-unlock").exists() {
                    log::warn!("~/sdr-unlock present, releasing kiosk");
                    release(&app2, &state);
                    break;
                }
            }
            let Some(window) = app2.get_webview_window("main") else { continue };
            #[cfg(target_os = "macos")]
            {
                let win = window.clone();
                let _ = app2.run_on_main_thread(move || {
                    let _ = objc2::exception::catch(std::panic::AssertUnwindSafe(|| unsafe {
                        let front = mac::frontmost_pid();
                        let visible = win.is_visible().unwrap_or(false);
                        if front != my_pid || !visible {
                            let _ = win.show();
                            mac::activate_self();
                            if let Ok(ptr) = win.ns_window() {
                                mac::raise_window(ptr as *mut objc2::runtime::AnyObject);
                            }
                            let _ = win.set_focus();
                        }
                    }));
                });
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = window.set_focus();
                let _ = my_pid;
            }
        }
    });
}

/// Release the kiosk lock and restore normal window behavior.
pub fn release(app: &AppHandle, state: &AppState) {
    state.locked.store(false, Ordering::SeqCst);
    restore_media(state);
    for (label, w) in app.webview_windows() {
        if label.starts_with("blanker-") {
            let _ = w.close();
        }
    }
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.set_always_on_top(false);
    let _ = window.set_visible_on_all_workspaces(false);
    let _ = window.set_resizable(true);
    let _ = window.set_decorations(true);
    #[cfg(target_os = "macos")]
    {
        let win = window.clone();
        let _ = window.run_on_main_thread(move || unsafe {
            if let Ok(ptr) = win.ns_window() {
                mac::reset_window(ptr as *mut objc2::runtime::AnyObject);
            }
            mac::set_presentation_options(0);
        });
    }
}

/// Verify the escape phrase (constant-time-ish), enforce attempt lockout.
pub fn verify_escape(state: &AppState, typed: &str) -> Result<bool, String> {
    let now = chrono::Utc::now().timestamp();
    {
        let mut fails = state.escape_failures.lock().unwrap();
        fails.retain(|t| now - *t < 60);
        if fails.len() >= 3 {
            return Err("too many attempts, wait 60 seconds".into());
        }
    }
    let expected = {
        let conn = state.db.0.lock().unwrap();
        crate::db::get_config(&conn, "escape_phrase")
            .ok()
            .flatten()
            .unwrap_or_default()
    };
    let a = typed.trim().as_bytes();
    let b = expected.trim().as_bytes();
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().min(b.len()) {
        diff |= (a[i] ^ b[i]) as usize;
    }
    if diff == 0 {
        Ok(true)
    } else {
        state.escape_failures.lock().unwrap().push(now);
        Ok(false)
    }
}
