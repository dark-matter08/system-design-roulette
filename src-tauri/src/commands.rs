use crate::db::{self, Attempt};
use crate::generator::GradeItem;
use crate::session::{self, SessionView};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, State};

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[derive(Serialize)]
pub struct AppStateView {
    pub onboarded: bool,
    pub session: SessionView,
    pub owed: bool,
    pub schedule_hour: u32,
    pub schedule_minute: u32,
    pub agent_ok: Option<bool>,
    pub debug_day: bool,
}

#[tauri::command]
pub fn get_app_state(state: State<'_, AppState>) -> CmdResult<AppStateView> {
    let (onboarded, hour, minute) = {
        let conn = state.db.0.lock().unwrap();
        (
            matches!(db::get_config(&conn, "onboarded"), Ok(Some(v)) if v == "1"),
            db::get_config(&conn, "schedule_hour").ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(9),
            db::get_config(&conn, "schedule_minute").ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(0),
        )
    };
    Ok(AppStateView {
        onboarded,
        session: session::view(&state),
        owed: session::session_owed(&state),
        schedule_hour: hour,
        schedule_minute: minute,
        agent_ok: None,
        debug_day: state.debug_day,
    })
}

#[tauri::command]
pub async fn check_agent(state: State<'_, AppState>) -> CmdResult<bool> {
    let gen = state.generator.clone();
    let out = tokio::process::Command::new(&gen.claude_bin)
        .args(["-p", "--max-turns", "1", "--model", "haiku", "reply with exactly: pong"])
        .current_dir(&gen.scratch_dir)
        .output();
    match tokio::time::timeout(std::time::Duration::from_secs(90), out).await {
        Ok(Ok(o)) => Ok(o.status.success()),
        _ => Ok(false),
    }
}

#[derive(Deserialize)]
pub struct SetupInput {
    pub hour: u32,
    pub minute: u32,
    pub escape_phrase: String,
}

#[tauri::command]
pub async fn complete_setup(
    app: AppHandle,
    state: State<'_, AppState>,
    input: SetupInput,
) -> CmdResult<AppStateView> {
    if input.escape_phrase.trim().len() < 40 {
        return Err("escape phrase must be at least 40 characters".into());
    }
    let today = state.today();
    {
        let conn = state.db.0.lock().unwrap();
        db::set_config(&conn, "schedule_hour", &input.hour.to_string()).map_err(err)?;
        db::set_config(&conn, "schedule_minute", &input.minute.to_string()).map_err(err)?;
        db::set_config(&conn, "escape_phrase", input.escape_phrase.trim()).map_err(err)?;
        db::set_config(&conn, "onboarded", "1").map_err(err)?;
        // Day-1 course generates immediately so the first session is instant.
        db::jobs::enqueue(&conn, "course", &today).map_err(err)?;
    }
    if !state.debug_day {
        crate::scheduler::install(input.hour, input.minute)?;
    }
    state.gen_notify.notify_one();
    let _ = app.emit("session:state", session::view(&state));
    get_app_state(state)
}

#[tauri::command]
pub fn update_schedule(state: State<'_, AppState>, hour: u32, minute: u32) -> CmdResult<()> {
    {
        let conn = state.db.0.lock().unwrap();
        db::set_config(&conn, "schedule_hour", &hour.to_string()).map_err(err)?;
        db::set_config(&conn, "schedule_minute", &minute.to_string()).map_err(err)?;
    }
    if !state.debug_day {
        crate::scheduler::install(hour, minute)?;
    }
    Ok(())
}

#[tauri::command]
pub fn start_session(app: AppHandle, state: State<'_, AppState>) -> CmdResult<SessionView> {
    let s = session::start_session(&state).map_err(err)?;
    if s.status == "in_progress" {
        crate::kiosk::engage(&app, &state);
    }
    let v = session::view(&state);
    let _ = app.emit("session:state", v.clone());
    Ok(v)
}

#[derive(Serialize)]
pub struct QuizQuestionView {
    pub id: i64,
    pub prompt: String,
    pub kind: String,
    pub choices: Option<Vec<String>>,
    pub origin: String,
    pub answered: bool,
    pub draft: Option<String>,
}

#[tauri::command]
pub fn get_quiz(state: State<'_, AppState>) -> CmdResult<Vec<QuizQuestionView>> {
    let today = state.today();
    let yesterday = state.yesterday();
    let conn = state.db.0.lock().unwrap();
    let pending = pending_answers(&conn, &today);
    let qs = db::quiz_for_date(&conn, &today, &yesterday).map_err(err)?;
    Ok(qs
        .into_iter()
        .map(|q| {
            let draft = pending.get(&q.id).cloned();
            QuizQuestionView {
                id: q.id,
                prompt: q.prompt,
                kind: q.kind,
                choices: q
                    .choices_json
                    .as_deref()
                    .and_then(|c| serde_json::from_str(c).ok()),
                origin: q.origin,
                answered: draft.is_some(),
                draft,
            }
        })
        .collect())
}

fn pending_key(date: &str) -> String {
    format!("pending_answers:{date}")
}

fn pending_answers(conn: &rusqlite::Connection, date: &str) -> HashMap<i64, String> {
    db::get_config(conn, &pending_key(date))
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn submit_answer(state: State<'_, AppState>, question_id: i64, answer: String) -> CmdResult<()> {
    let today = state.today();
    let conn = state.db.0.lock().unwrap();
    let mut pending = pending_answers(&conn, &today);
    pending.insert(question_id, answer);
    db::set_config(
        &conn,
        &pending_key(&today),
        &serde_json::to_string(&pending).map_err(err)?,
    )
    .map_err(err)?;
    Ok(())
}

#[derive(Serialize, Clone)]
pub struct ReviewItem {
    pub question_id: i64,
    pub prompt: String,
    pub kind: String,
    pub user_answer: String,
    pub correct: Option<bool>,
    pub feedback: String,
    pub correct_answer: String,
    pub explanation: String,
    pub returns_tomorrow: bool,
}

#[derive(Serialize, Clone)]
pub struct ReviewData {
    pub items: Vec<ReviewItem>,
    pub score: f64,
    pub self_assess: bool,
}

#[tauri::command]
pub async fn finish_quiz(app: AppHandle, state: State<'_, AppState>) -> CmdResult<ReviewData> {
    let today = state.today();
    let yesterday = state.yesterday();
    let tomorrow = state.tomorrow();

    let (questions, pending) = {
        let conn = state.db.0.lock().unwrap();
        let qs = db::quiz_for_date(&conn, &today, &yesterday).map_err(err)?;
        let pending = pending_answers(&conn, &today);
        (qs, pending)
    };

    // Grade free-text via agent in one batch.
    let free_items: Vec<GradeItem> = questions
        .iter()
        .filter(|q| q.kind == "free")
        .map(|q| GradeItem {
            id: q.id,
            question: q.prompt.clone(),
            model_answer: q.correct_answer.clone(),
            user_answer: pending.get(&q.id).cloned().unwrap_or_default(),
        })
        .collect();
    let verdicts = state.generator.grade(&free_items).await;
    let self_assess = verdicts.is_none();
    let verdict_map: HashMap<i64, (bool, String)> = verdicts
        .unwrap_or_default()
        .into_iter()
        .map(|v| (v.id, (v.correct, v.feedback)))
        .collect();

    let mut items = Vec::new();
    let mut n_graded = 0usize;
    let mut n_correct = 0usize;
    {
        let conn = state.db.0.lock().unwrap();
        for q in &questions {
            let user_answer = pending.get(&q.id).cloned().unwrap_or_default();
            let (correct, feedback): (Option<bool>, String) = if q.kind == "mcq" {
                let ok = user_answer.trim() == q.correct_answer.trim();
                (Some(ok), String::new())
            } else if let Some((ok, fb)) = verdict_map.get(&q.id) {
                (Some(*ok), fb.clone())
            } else {
                (None, "grader unavailable — self-assess against the model answer".into())
            };
            let counted_correct = correct.unwrap_or(true); // self-assess counts as pass, never blocks
            if correct.is_some() {
                n_graded += 1;
                if counted_correct {
                    n_correct += 1;
                }
            }
            db::record_attempt(
                &conn,
                &Attempt {
                    question_id: q.id,
                    session_date: today.clone(),
                    user_answer: user_answer.clone(),
                    correct: counted_correct,
                    grader_feedback: feedback.clone(),
                },
            )
            .map_err(err)?;
            let failed = correct == Some(false);
            if failed {
                db::push_carryover(&conn, q.id, &today, &tomorrow).map_err(err)?;
            } else if q.origin == "carryover" {
                db::clear_carryover(&conn, q.id).map_err(err)?;
            }
            items.push(ReviewItem {
                question_id: q.id,
                prompt: q.prompt.clone(),
                kind: q.kind.clone(),
                user_answer,
                correct,
                feedback,
                correct_answer: q.correct_answer.clone(),
                explanation: q.explanation.clone(),
                returns_tomorrow: failed,
            });
        }
        let score = if n_graded > 0 {
            n_correct as f64 / n_graded as f64
        } else {
            1.0
        };
        let mut s = db::get_session(&conn, &today).map_err(err)?.ok_or("no session")?;
        s.quiz_score = Some(score);
        s.current_step = session::STEP_REVIEW.into();
        db::upsert_session(&conn, &s).map_err(err)?;
        // Clear pending answers.
        db::set_config(&conn, &pending_key(&today), "{}").map_err(err)?;
        let _ = app.emit("session:state", ());
        return Ok(ReviewData { items, score, self_assess });
    }
}

#[tauri::command]
pub fn get_review(state: State<'_, AppState>) -> CmdResult<ReviewData> {
    let today = state.today();
    let yesterday = state.yesterday();
    let conn = state.db.0.lock().unwrap();
    let attempts = db::attempts_for_session(&conn, &today).map_err(err)?;
    let mut by_q: HashMap<i64, &Attempt> = HashMap::new();
    for a in &attempts {
        by_q.insert(a.question_id, a);
    }
    // Reconstruct from questions answered today (carryover rows may already be cleared).
    let mut items = Vec::new();
    let mut n_correct = 0usize;
    let mut stmt = conn
        .prepare(
            "SELECT q.id, q.prompt, q.kind, q.correct_answer, q.explanation, q.origin
             FROM questions q JOIN attempts a ON a.question_id = q.id
             WHERE a.session_date = ?1 ORDER BY a.id",
        )
        .map_err(err)?;
    let rows = stmt
        .query_map([&today], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })
        .map_err(err)?;
    for row in rows {
        let (id, prompt, kind, correct_answer, explanation, _origin) = row.map_err(err)?;
        if let Some(a) = by_q.get(&id) {
            if a.correct {
                n_correct += 1;
            }
            items.push(ReviewItem {
                question_id: id,
                prompt,
                kind,
                user_answer: a.user_answer.clone(),
                correct: Some(a.correct),
                feedback: a.grader_feedback.clone(),
                correct_answer,
                explanation,
                returns_tomorrow: !a.correct,
            });
        }
    }
    let score = if items.is_empty() { 1.0 } else { n_correct as f64 / items.len() as f64 };
    let _ = yesterday;
    Ok(ReviewData { items, score, self_assess: false })
}

#[derive(Serialize)]
pub struct RouletteView {
    pub pool: Vec<String>,
    pub chosen_index: usize,
    pub concept_title: String,
    pub concept_category: String,
}

#[tauri::command]
pub fn finish_review(app: AppHandle, state: State<'_, AppState>) -> CmdResult<SessionView> {
    session::set_step(&state, session::STEP_ROULETTE).map_err(err)?;
    let v = session::view(&state);
    let _ = app.emit("session:state", v.clone());
    Ok(v)
}

/// Wheel data: pool of titles + the index of today's (pre-decided) concept.
#[tauri::command]
pub fn get_roulette(state: State<'_, AppState>) -> CmdResult<RouletteView> {
    let today = state.today();
    let conn = state.db.0.lock().unwrap();
    // Reuse pre-drawn concept if pregen already picked one, else draw now.
    let concept = {
        let existing = db::get_session(&conn, &today).map_err(err)?.and_then(|s| s.concept_id);
        match existing {
            Some(id) => db::get_concept(&conn, id).map_err(err)?.ok_or("concept missing")?,
            None => {
                let c = crate::roulette::draw(&conn, &today).map_err(err)?.ok_or("empty pool")?;
                let mut s = db::get_session(&conn, &today).map_err(err)?.ok_or("no session")?;
                s.concept_id = Some(c.id);
                db::upsert_session(&conn, &s).map_err(err)?;
                c
            }
        }
    };
    // Build a wheel pool: chosen + up to 11 other titles.
    let mut pool: Vec<String> = db::all_concepts(&conn)
        .map_err(err)?
        .into_iter()
        .filter(|c| c.id != concept.id)
        .take(11)
        .map(|c| c.title)
        .collect();
    let chosen_index = (chrono::Utc::now().timestamp() as usize) % (pool.len() + 1);
    pool.insert(chosen_index.min(pool.len()), concept.title.clone());
    Ok(RouletteView {
        pool,
        chosen_index,
        concept_title: concept.title,
        concept_category: concept.category,
    })
}

#[derive(Serialize)]
pub struct CourseView {
    pub title: String,
    pub markdown: String,
    pub resources: serde_json::Value,
    pub source: String,
    pub remaining_seconds: i64,
    pub total_seconds: i64,
}

/// Generate today's course if missing (slow path), then return it. Frontend shows
/// gen:status events while this runs.
#[tauri::command]
pub async fn ensure_course(app: AppHandle, state: State<'_, AppState>) -> CmdResult<CourseView> {
    let today = state.today();
    let _ = app.emit("gen:status", "checking course");
    let course = session::ensure_course_for_date(&state, &today)
        .await
        .map_err(|e| e.to_string())?;
    let concept_title = {
        let conn = state.db.0.lock().unwrap();
        db::get_concept(&conn, course.concept_id)
            .map_err(err)?
            .map(|c| c.title)
            .unwrap_or_default()
    };
    let total = state.course_duration_secs();
    let s = {
        let conn = state.db.0.lock().unwrap();
        db::get_session(&conn, &today).map_err(err)?.ok_or("no session")?
    };
    let remaining = (total - s.reading_seconds).max(0);
    Ok(CourseView {
        title: concept_title,
        markdown: course.markdown,
        resources: serde_json::from_str(&course.resources_json).unwrap_or(serde_json::json!([])),
        source: course.source,
        remaining_seconds: remaining,
        total_seconds: total,
    })
}

#[tauri::command]
pub fn start_course(app: AppHandle, state: State<'_, AppState>) -> CmdResult<SessionView> {
    let today = state.today();
    let total = state.course_duration_secs();
    let remaining = {
        let conn = state.db.0.lock().unwrap();
        let s = db::get_session(&conn, &today).map_err(err)?.ok_or("no session")?;
        (total - s.reading_seconds).max(0)
    };
    session::set_step(&state, session::STEP_COURSE).map_err(err)?;
    state.reading_remaining.store(remaining, Ordering::SeqCst);
    if !state.timer_running.load(Ordering::SeqCst) && remaining > 0 {
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            session::run_course_timer(app2).await;
        });
    }
    let v = session::view(&state);
    let _ = app.emit("session:state", v.clone());
    Ok(v)
}

#[tauri::command]
pub fn finish_course(app: AppHandle, state: State<'_, AppState>) -> CmdResult<SessionView> {
    let remaining = state.reading_remaining.load(Ordering::SeqCst);
    if remaining > 0 {
        return Err(format!("{remaining} seconds of reading remain"));
    }
    session::complete_session(&app, &state).map_err(err)?;
    Ok(session::view(&state))
}

#[tauri::command]
pub fn escape_session(app: AppHandle, state: State<'_, AppState>, phrase: String) -> CmdResult<bool> {
    match crate::kiosk::verify_escape(&state, &phrase)? {
        true => {
            let today = state.today();
            {
                let conn = state.db.0.lock().unwrap();
                if let Ok(Some(mut s)) = db::get_session(&conn, &today) {
                    s.status = "skipped".into();
                    s.completed_at = Some(session::now_iso());
                    let _ = db::upsert_session(&conn, &s);
                }
            }
            crate::kiosk::release(&app, &state);
            let _ = app.emit("session:state", session::view(&state));
            Ok(true)
        }
        false => Ok(false),
    }
}

#[tauri::command]
pub fn get_escape_phrase(state: State<'_, AppState>) -> CmdResult<String> {
    let conn = state.db.0.lock().unwrap();
    Ok(db::get_config(&conn, "escape_phrase").map_err(err)?.unwrap_or_default())
}

#[derive(Serialize)]
pub struct DashboardView {
    pub history: Vec<db::HistoryEntry>,
    pub streak: i64,
    pub carryover_due: i64,
    pub concepts_total: i64,
    pub concepts_covered: i64,
}

#[tauri::command]
pub fn get_dashboard(state: State<'_, AppState>) -> CmdResult<DashboardView> {
    let today = state.today();
    let tomorrow = state.tomorrow();
    let conn = state.db.0.lock().unwrap();
    let history = db::history(&conn, 120).map_err(err)?;
    let streak = db::streak(&conn, &today).map_err(err)?;
    let carryover_due = db::carryover_count(&conn, &tomorrow).map_err(err)?;
    let concepts_total: i64 = conn
        .query_row("SELECT COUNT(*) FROM concepts WHERE active = 1", [], |r| r.get(0))
        .map_err(err)?;
    let concepts_covered: i64 = conn
        .query_row("SELECT COUNT(*) FROM concepts WHERE times_picked > 0", [], |r| r.get(0))
        .map_err(err)?;
    Ok(DashboardView { history, streak, carryover_due, concepts_total, concepts_covered })
}

#[derive(Serialize)]
pub struct ArchivedCourse {
    pub session_date: String,
    pub title: String,
    pub markdown: String,
    pub resources: serde_json::Value,
}

#[tauri::command]
pub fn get_past_course(state: State<'_, AppState>, date: String) -> CmdResult<Option<ArchivedCourse>> {
    let conn = state.db.0.lock().unwrap();
    let Some(course) = db::course_for_date(&conn, &date).map_err(err)? else {
        return Ok(None);
    };
    let title = db::get_concept(&conn, course.concept_id)
        .map_err(err)?
        .map(|c| c.title)
        .unwrap_or_default();
    Ok(Some(ArchivedCourse {
        session_date: course.session_date,
        title,
        markdown: course.markdown,
        resources: serde_json::from_str(&course.resources_json).unwrap_or(serde_json::json!([])),
    }))
}

/// After the session, open all of today's resource links in the default browser.
#[tauri::command]
pub fn open_resources(app: AppHandle, state: State<'_, AppState>) -> CmdResult<usize> {
    if state.locked.load(Ordering::SeqCst) {
        return Err("locked — resources unlock after the session".into());
    }
    let today = state.today();
    let urls: Vec<String> = {
        let conn = state.db.0.lock().unwrap();
        let course = db::course_for_date(&conn, &today).map_err(err)?;
        course
            .map(|c| {
                serde_json::from_str::<Vec<crate::generator::Resource>>(&c.resources_json)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|r| r.url)
                    .collect()
            })
            .unwrap_or_default()
    };
    let n = urls.len();
    for url in urls {
        use tauri_plugin_opener::OpenerExt;
        let _ = app.opener().open_url(url, None::<String>);
    }
    Ok(n)
}
