use crate::state::AppState;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager};

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
    if state.locked.swap(true, Ordering::SeqCst) {
        return;
    }
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
        let _ = window.run_on_main_thread(move || unsafe {
            if let Ok(ptr) = win.ns_window() {
                mac::raise_window(ptr as *mut objc2::runtime::AnyObject);
            }
            mac::set_presentation_options(
                mac::HIDE_DOCK
                    | mac::HIDE_MENU_BAR
                    | mac::DISABLE_PROCESS_SWITCHING
                    | mac::DISABLE_FORCE_QUIT
                    | mac::DISABLE_SESSION_TERMINATION
                    | mac::DISABLE_HIDE_APPLICATION,
            );
        });
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
                let _ = app2.run_on_main_thread(move || unsafe {
                    let front = mac::frontmost_pid();
                    if front != my_pid {
                        mac::activate_self();
                        if let Ok(ptr) = win.ns_window() {
                            mac::raise_window(ptr as *mut objc2::runtime::AnyObject);
                        }
                        let _ = win.set_focus();
                    }
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
