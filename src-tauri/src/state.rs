use crate::db::Db;
use crate::generator::Generator;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64};
use std::sync::Mutex;

pub struct AppState {
    pub db: Db,
    pub generator: Generator,
    pub data_dir: PathBuf,
    /// Kiosk lock currently engaged.
    pub locked: AtomicBool,
    /// Remaining required reading seconds for today's course step.
    pub reading_remaining: AtomicI64,
    /// Course timer task is running.
    pub timer_running: AtomicBool,
    /// Pause flag (system sleep).
    pub timer_paused: AtomicBool,
    /// --debug-day: shortened timer, no kiosk, ignore schedule.
    pub debug_day: bool,
    /// Failed escape attempts (rate limiting the hatch).
    pub escape_failures: Mutex<Vec<i64>>,
    /// Failed early-exit quiz attempts (60s cooldown).
    pub exit_quiz_failures: Mutex<Vec<i64>>,
    /// System mute state before the lock engaged (None = not captured).
    pub prev_muted: Mutex<Option<bool>>,
    /// Webview has booted and called mark_frontend_ready. The kiosk NEVER
    /// engages before this: a dead webview has no escape hatch, and locking
    /// behind one bricks the machine at every login.
    pub frontend_ready: AtomicBool,
    /// Background generation worker wakeup.
    pub gen_notify: tokio::sync::Notify,
}

impl AppState {
    /// Today's date, overridable for testing via SDR_DATE=YYYY-MM-DD.
    pub fn today(&self) -> String {
        if let Ok(d) = std::env::var("SDR_DATE") {
            if chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").is_ok() {
                return d;
            }
        }
        chrono::Local::now().format("%Y-%m-%d").to_string()
    }

    pub fn yesterday(&self) -> String {
        let t = chrono::NaiveDate::parse_from_str(&self.today(), "%Y-%m-%d")
            .unwrap_or_else(|_| chrono::Local::now().date_naive());
        t.pred_opt().unwrap_or(t).format("%Y-%m-%d").to_string()
    }

    pub fn tomorrow(&self) -> String {
        let t = chrono::NaiveDate::parse_from_str(&self.today(), "%Y-%m-%d")
            .unwrap_or_else(|_| chrono::Local::now().date_naive());
        t.succ_opt().unwrap_or(t).format("%Y-%m-%d").to_string()
    }

    pub fn course_duration_secs(&self) -> i64 {
        if self.debug_day {
            30
        } else {
            30 * 60
        }
    }
}
