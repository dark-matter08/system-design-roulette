pub mod audio;
pub mod commands;
pub mod db;
pub mod generator;
pub mod kiosk;
pub mod mastery;
pub mod roulette;
pub mod scheduler;
pub mod session;
pub mod state;

use state::AppState;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Mutex;
use tauri::{Emitter, Manager};

const SEED_CONCEPTS: &str = include_str!("../seed/concepts.json");

fn resolve_claude_bin() -> String {
    if let Ok(p) = std::env::var("SDR_CLAUDE_BIN") {
        return p;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        format!("{home}/.local/bin/claude"),
        "/usr/local/bin/claude".to_string(),
        "/opt/homebrew/bin/claude".to_string(),
    ];
    for c in &candidates {
        if std::path::Path::new(c).exists() {
            return c.clone();
        }
    }
    "claude".to_string()
}

fn resolve_codex_bin(conn: &rusqlite::Connection) -> Option<String> {
    match std::env::var("SDR_CODEX_BIN").as_deref() {
        Ok("none") => return None,
        Ok(p) => return Some(p.to_string()),
        _ => {}
    }
    if let Ok(Some(saved)) = db::get_config(conn, "codex_bin") {
        if std::path::Path::new(&saved).exists() {
            return Some(saved);
        }
    }
    let out = std::process::Command::new("zsh")
        .args(["-lc", "which codex"])
        .output()
        .ok()?;
    if out.status.success() {
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !path.is_empty() {
            let _ = db::set_config(conn, "codex_bin", &path);
            return Some(path);
        }
    }
    None
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args: Vec<String> = std::env::args().collect();
    let triggered = args.iter().any(|a| a == "--triggered");
    let debug_day = args.iter().any(|a| a == "--debug-day");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch (e.g. launchd firing while we run) just surfaces the window.
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
            let state = app.state::<AppState>();
            if session::session_owed(&state) {
                let _ = app.emit("session:owed", true);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("app data dir resolvable");
            std::fs::create_dir_all(&data_dir)?;
            let conn = db::open(&data_dir.join("roulette.db"))?;
            db::seed_concepts(&conn, SEED_CONCEPTS)?;
            let codex_bin = resolve_codex_bin(&conn);
            // Primary model for course generation: config 'model' (default opus),
            // SDR_MODEL env wins for testing.
            let model = std::env::var("SDR_MODEL")
                .ok()
                .or_else(|| db::get_config(&conn, "model").ok().flatten())
                .unwrap_or_else(|| "opus".to_string());
            // Live agent-activity feed: generator -> broadcast -> gen:log events.
            let (log_tx, mut log_rx) = tokio::sync::broadcast::channel::<String>(64);
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    while let Ok(line) = log_rx.recv().await {
                        let _ = handle.emit("gen:log", line);
                    }
                });
            }
            let generator = generator::Generator::new(
                resolve_claude_bin(),
                codex_bin,
                data_dir.join("scratch"),
                model,
                Some(log_tx),
            );
            app.manage(AppState {
                db: db::Db(Mutex::new(conn)),
                generator,
                data_dir,
                locked: AtomicBool::new(false),
                reading_remaining: AtomicI64::new(0),
                timer_running: AtomicBool::new(false),
                timer_paused: AtomicBool::new(false),
                debug_day,
                escape_failures: Mutex::new(Vec::new()),
                exit_quiz_failures: Mutex::new(Vec::new()),
                prev_muted: Mutex::new(None),
                frontend_ready: AtomicBool::new(false),
                gen_notify: tokio::sync::Notify::new(),
            });

            // Self-heal the launchd plist if it points at a stale binary path
            // (e.g. setup completed from a dev build).
            if !debug_day {
                let state = app.state::<AppState>();
                let conn = state.db.0.lock().unwrap();
                let onboarded = matches!(db::get_config(&conn, "onboarded"), Ok(Some(v)) if v == "1");
                let paused = matches!(db::get_config(&conn, "schedule_paused"), Ok(Some(v)) if v == "1");
                if onboarded && !paused {
                    let hour: u32 = db::get_config(&conn, "schedule_hour")
                        .ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(9);
                    let minute: u32 = db::get_config(&conn, "schedule_minute")
                        .ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(0);
                    drop(conn);
                    scheduler::ensure_current(hour, minute);
                }
            }

            // Background generation worker.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                session::generation_worker(handle).await;
            });

            // Owed-session watcher: checks every 60s (covers app-already-running case).
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    let state = handle.state::<AppState>();
                    if session::session_owed(&state) && !state.locked.load(Ordering::SeqCst) {
                        let _ = handle.emit("session:owed", true);
                        kiosk::engage(&handle, &state);
                    }
                }
            });

            // Launched by launchd at the scheduled time (or at load for catch-up).
            if triggered {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    let state = handle.state::<AppState>();
                    if session::session_owed(&state) {
                        let _ = handle.emit("session:owed", true);
                        kiosk::engage(&handle, &state);
                    }
                    // Kick the pregen queue on every triggered launch (wake catch-up).
                    state.gen_notify.notify_one();
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    let state = window.app_handle().state::<AppState>();
                    if state.locked.load(Ordering::SeqCst) {
                        api.prevent_close();
                    }
                }
                tauri::WindowEvent::Focused(false) => {
                    let state = window.app_handle().state::<AppState>();
                    if state.locked.load(Ordering::SeqCst) {
                        let _ = window.set_focus();
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::mark_frontend_ready,
            commands::get_app_state,
            commands::check_agent,
            commands::complete_setup,
            commands::update_schedule,
            commands::set_kiosk_level,
            commands::pause_schedule,
            commands::resume_schedule,
            commands::start_session,
            commands::get_quiz,
            commands::submit_answer,
            commands::finish_quiz,
            commands::get_review,
            commands::finish_review,
            commands::get_roulette,
            commands::ensure_course,
            commands::start_course,
            commands::finish_course,
            commands::ensure_audio,
            commands::get_audio_enabled,
            commands::set_audio_enabled,
            commands::get_exit_quiz,
            commands::submit_exit_quiz,
            commands::extend_session,
            commands::escape_session,
            commands::get_escape_phrase,
            commands::get_dashboard,
            commands::get_past_course,
            commands::open_resources,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let state = app.state::<AppState>();
                if state.locked.load(Ordering::SeqCst) {
                    api.prevent_exit();
                }
            }
        });
}
