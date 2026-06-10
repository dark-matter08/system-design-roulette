use crate::db::{self, Session};
use crate::generator::GeneratedQuestion;
use crate::state::AppState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

pub const STEP_QUIZ: &str = "quiz";
pub const STEP_REVIEW: &str = "review";
pub const STEP_ROULETTE: &str = "roulette";
pub const STEP_COURSE: &str = "course";
pub const STEP_DONE: &str = "done";

#[derive(Debug, Clone, Serialize)]
pub struct SessionView {
    pub date: String,
    pub status: String,
    pub step: String,
    pub quiz_score: Option<f64>,
    pub streak: i64,
    pub locked: bool,
}

pub fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Is a session owed right now? (scheduled time passed, today not completed/skipped)
pub fn session_owed(state: &AppState) -> bool {
    let today = state.today();
    let conn = state.db.0.lock().unwrap();
    let onboarded = matches!(db::get_config(&conn, "onboarded"), Ok(Some(v)) if v == "1");
    if !onboarded {
        return false;
    }
    if let Ok(Some(s)) = db::get_session(&conn, &today) {
        if s.status == "completed" || s.status == "skipped" {
            return false;
        }
    }
    if state.debug_day {
        return true;
    }
    let hour: u32 = db::get_config(&conn, "schedule_hour")
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(9);
    let minute: u32 = db::get_config(&conn, "schedule_minute")
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let now = chrono::Local::now();
    let sched = now
        .date_naive()
        .and_hms_opt(hour, minute, 0)
        .unwrap_or_else(|| now.naive_local());
    now.naive_local() >= sched
}

/// Get or create today's session row and return it.
pub fn ensure_today_session(state: &AppState) -> db::Result<Session> {
    let today = state.today();
    let conn = state.db.0.lock().unwrap();
    if let Some(s) = db::get_session(&conn, &today)? {
        return Ok(s);
    }
    let s = Session {
        date: today,
        concept_id: None,
        status: "pending".into(),
        current_step: STEP_QUIZ.into(),
        quiz_score: None,
        started_at: None,
        completed_at: None,
        reading_seconds: 0,
    };
    db::upsert_session(&conn, &s)?;
    Ok(s)
}

/// Begin (or resume) today's session: pick the right starting step.
pub fn start_session(state: &AppState) -> db::Result<Session> {
    let mut s = ensure_today_session(state)?;
    if s.status == "completed" || s.status == "skipped" {
        return Ok(s);
    }
    let today = state.today();
    let yesterday = state.yesterday();
    let conn = state.db.0.lock().unwrap();
    if s.status == "pending" {
        // No quiz available (day 1, or nothing generated yesterday) -> straight to roulette.
        let quiz = db::quiz_for_date(&conn, &today, &yesterday)?;
        s.current_step = if quiz.is_empty() { STEP_ROULETTE.into() } else { STEP_QUIZ.into() };
        s.status = "in_progress".into();
        s.started_at = Some(now_iso());
        db::upsert_session(&conn, &s)?;
    }
    Ok(s)
}

pub fn set_step(state: &AppState, step: &str) -> db::Result<Session> {
    let today = state.today();
    let conn = state.db.0.lock().unwrap();
    let mut s = db::get_session(&conn, &today)?.expect("session exists");
    s.current_step = step.to_string();
    db::upsert_session(&conn, &s)?;
    Ok(s)
}

/// Validated forward-only transitions.
pub fn allowed_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        (STEP_QUIZ, STEP_REVIEW)
            | (STEP_REVIEW, STEP_ROULETTE)
            | (STEP_ROULETTE, STEP_COURSE)
            | (STEP_COURSE, STEP_DONE)
    )
}

pub fn view(state: &AppState) -> SessionView {
    let today = state.today();
    let conn = state.db.0.lock().unwrap();
    let s = db::get_session(&conn, &today).ok().flatten();
    let streak = db::streak(&conn, &today).unwrap_or(0);
    match s {
        Some(s) => SessionView {
            date: s.date,
            status: s.status,
            step: s.current_step,
            quiz_score: s.quiz_score,
            streak,
            locked: state.locked.load(std::sync::atomic::Ordering::SeqCst),
        },
        None => SessionView {
            date: today,
            status: "pending".into(),
            step: STEP_QUIZ.into(),
            quiz_score: None,
            streak,
            locked: false,
        },
    }
}

/// Mark today completed, enqueue tomorrow's pregeneration, release the lock.
pub fn complete_session(app: &AppHandle, state: &AppState) -> db::Result<Session> {
    let today = state.today();
    let tomorrow = state.tomorrow();
    let s = {
        let conn = state.db.0.lock().unwrap();
        let mut s = db::get_session(&conn, &today)?.expect("session exists");
        s.status = "completed".into();
        s.current_step = STEP_DONE.into();
        s.completed_at = Some(now_iso());
        db::upsert_session(&conn, &s)?;
        db::jobs::enqueue(&conn, "course", &tomorrow)?;
        db::jobs::enqueue(&conn, "quiz", &tomorrow)?;
        s
    };
    state.gen_notify.notify_one();
    crate::kiosk::release(app, state);
    let _ = app.emit("session:state", view(state));
    Ok(s)
}

/// Store generated quiz questions for `target_date`, linked to the course studied the day before.
pub fn persist_quiz_questions(
    conn: &rusqlite::Connection,
    course_id: i64,
    questions: &[GeneratedQuestion],
) -> db::Result<()> {
    for q in questions {
        let choices = q
            .choices
            .as_ref()
            .map(|c| serde_json::to_string(c))
            .transpose()?;
        db::insert_question(
            conn,
            course_id,
            &q.prompt,
            if q.kind == "mcq" { "mcq" } else { "free" },
            choices.as_deref(),
            &q.correct_answer,
            &q.explanation,
        )?;
    }
    Ok(())
}

/// Background generation worker: drains the generation_jobs queue forever.
pub async fn generation_worker(app: AppHandle) {
    let state = app.state::<AppState>();
    loop {
        let job = {
            let conn = state.db.0.lock().unwrap();
            db::jobs::next_queued(&conn).ok().flatten()
        };
        let Some((job_id, kind, target_date)) = job else {
            tokio::select! {
                _ = state.gen_notify.notified() => {},
                _ = tokio::time::sleep(std::time::Duration::from_secs(3600)) => {},
            }
            continue;
        };
        {
            let conn = state.db.0.lock().unwrap();
            let _ = db::jobs::mark(&conn, job_id, "running", None);
        }
        let _ = app.emit("gen:status", format!("generating {kind} for {target_date}"));
        let result = run_generation_job(&app, &state, &kind, &target_date).await;
        {
            let conn = state.db.0.lock().unwrap();
            match &result {
                Ok(_) => {
                    let _ = db::jobs::mark(&conn, job_id, "done", None);
                }
                Err(e) => {
                    let _ = db::jobs::mark(&conn, job_id, "failed", Some(&e.to_string()));
                    // failed jobs retry on next wakeup (attempts capped in next_queued)
                    let _ = db::jobs::enqueue(&conn, &kind, &target_date);
                }
            }
        }
        let _ = app.emit(
            "gen:status",
            match &result {
                Ok(_) => format!("{kind} for {target_date} ready"),
                Err(e) => format!("{kind} for {target_date} failed: {e}"),
            },
        );
    }
}

async fn run_generation_job(
    _app: &AppHandle,
    state: &tauri::State<'_, AppState>,
    kind: &str,
    target_date: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match kind {
        "course" => {
            ensure_course_for_date(state, target_date).await?;
            Ok(())
        }
        "quiz" => {
            // Quiz for target_date tests the course read the day before it.
            let prev = chrono::NaiveDate::parse_from_str(target_date, "%Y-%m-%d")?
                .pred_opt()
                .ok_or("date underflow")?
                .format("%Y-%m-%d")
                .to_string();
            let course = {
                let conn = state.db.0.lock().unwrap();
                db::course_for_date(&conn, &prev)?
            };
            let Some(course) = course else {
                return Err(format!("no course for {prev}, cannot build quiz").into());
            };
            let existing = {
                let conn = state.db.0.lock().unwrap();
                db::questions_for_course(&conn, course.id)?
            };
            if !existing.is_empty() {
                return Ok(());
            }
            let (questions, _src) = state.generator.generate_quiz(&course.markdown).await?;
            let conn = state.db.0.lock().unwrap();
            persist_quiz_questions(&conn, course.id, &questions)?;
            Ok(())
        }
        other => Err(format!("unknown job kind {other}").into()),
    }
}

/// Make sure a course exists for `date`: draw a concept if needed, generate, persist.
pub async fn ensure_course_for_date(
    state: &tauri::State<'_, AppState>,
    date: &str,
) -> Result<db::Course, Box<dyn std::error::Error + Send + Sync>> {
    {
        let conn = state.db.0.lock().unwrap();
        if let Some(c) = db::course_for_date(&conn, date)? {
            return Ok(c);
        }
    }
    // Draw (or reuse) the concept for that date's session.
    let concept = {
        let conn = state.db.0.lock().unwrap();
        let existing = db::get_session(&conn, date)?;
        let concept_id = existing.as_ref().and_then(|s| s.concept_id);
        match concept_id {
            Some(id) => db::get_concept(&conn, id)?.ok_or("concept missing")?,
            None => {
                let c = crate::roulette::draw(&conn, date)?.ok_or("empty concept pool")?;
                let mut s = existing.unwrap_or(Session {
                    date: date.to_string(),
                    concept_id: None,
                    status: "pending".into(),
                    current_step: STEP_QUIZ.into(),
                    quiz_score: None,
                    started_at: None,
                    completed_at: None,
                    reading_seconds: 0,
                });
                s.concept_id = Some(c.id);
                db::upsert_session(&conn, &s)?;
                c
            }
        }
    };
    let (course, source) = state
        .generator
        .generate_course(&concept.title, &concept.category)
        .await?;
    let resources_json = serde_json::to_string(&course.resources)?;
    let course_row = {
        let conn = state.db.0.lock().unwrap();
        let id = db::insert_course(&conn, date, concept.id, &course.markdown, &resources_json, &source)?;
        db::course_for_date(&conn, date)?.ok_or("course vanished")?;
        let _ = id;
        db::course_for_date(&conn, date)?.unwrap()
    };
    // Mirror to a human-greppable markdown file.
    let mirror = state
        .data_dir
        .join("courses")
        .join(format!("{}-{}.md", date, concept.slug));
    let _ = std::fs::create_dir_all(mirror.parent().unwrap());
    let _ = std::fs::write(&mirror, &course_row.markdown);
    Ok(course_row)
}

/// Authoritative course reading timer. Emits timer:tick {remaining} every second.
pub async fn run_course_timer(app: AppHandle) {
    use std::sync::atomic::Ordering;
    let state = app.state::<AppState>();
    if state.timer_running.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut persist_counter = 0;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if state.timer_paused.load(Ordering::SeqCst) {
            continue;
        }
        let remaining = state.reading_remaining.load(Ordering::SeqCst);
        if remaining <= 0 {
            let _ = app.emit("timer:tick", 0);
            break;
        }
        let next = remaining - 1;
        state.reading_remaining.store(next, Ordering::SeqCst);
        let _ = app.emit("timer:tick", next);
        persist_counter += 1;
        if persist_counter % 10 == 0 {
            let today = state.today();
            let total = state.course_duration_secs();
            let conn = state.db.0.lock().unwrap();
            if let Ok(Some(mut s)) = db::get_session(&conn, &today) {
                s.reading_seconds = total - next;
                let _ = db::upsert_session(&conn, &s);
            }
        }
        if next <= 0 {
            break;
        }
    }
    state.timer_running.store(false, Ordering::SeqCst);
    let _ = app.emit("timer:done", true);
}
