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
    /// ~/sdr-unlock exists: every lock releases instantly. Surfaced so a
    /// leftover emergency file can't silently neuter enforcement.
    pub enforcement_disarmed: bool,
    /// Scheduler paused: no launchd agent, no owed sessions, until resumed.
    pub schedule_paused: bool,
    /// Kiosk strictness: 'advisory' | 'firm' | 'hard'.
    pub kiosk_level: String,
    /// Course-generation model: 'opus' | 'sonnet' | 'haiku'.
    pub model: String,
    /// Primary CLI agent: 'claude' | 'codex' | 'custom'.
    pub agent: String,
    /// Binary path used when agent == 'custom'.
    pub custom_agent_bin: String,
}

fn valid_agent(agent: &str) -> bool {
    matches!(agent, "claude" | "codex" | "custom")
}

/// Switch the primary CLI agent (and the custom binary path when relevant).
/// Applies to the next generation; the fallback chain adapts automatically.
#[tauri::command]
pub fn set_agent(state: State<'_, AppState>, agent: String, custom_bin: Option<String>) -> CmdResult<()> {
    if !valid_agent(&agent) {
        return Err(format!("unknown agent: {agent}"));
    }
    let custom = custom_bin.unwrap_or_default();
    if agent == "custom" && custom.trim().is_empty() {
        return Err("custom agent needs a binary path".into());
    }
    if agent == "custom" && !std::path::Path::new(custom.trim()).exists() {
        return Err(format!("binary not found: {}", custom.trim()));
    }
    {
        let conn = state.db.0.lock().unwrap();
        db::set_config(&conn, "agent", &agent).map_err(err)?;
        db::set_config(&conn, "custom_agent_bin", custom.trim()).map_err(err)?;
    }
    *state.generator.agent.lock().unwrap() = agent;
    *state.generator.custom_bin.lock().unwrap() = custom.trim().to_string();
    Ok(())
}

fn valid_model(model: &str) -> bool {
    matches!(model, "opus" | "sonnet" | "haiku")
}

/// Change the course-generation model. Applies to the NEXT generation —
/// in-flight calls keep the model they started with.
#[tauri::command]
pub fn set_model(state: State<'_, AppState>, model: String) -> CmdResult<()> {
    if !valid_model(&model) {
        return Err(format!("unknown model: {model}"));
    }
    {
        let conn = state.db.0.lock().unwrap();
        db::set_config(&conn, "model", &model).map_err(err)?;
    }
    *state.generator.model.lock().unwrap() = model;
    Ok(())
}

fn valid_kiosk_level(level: &str) -> bool {
    matches!(level, "advisory" | "firm" | "hard")
}

/// Change kiosk strictness. Refused while locked — strictness can't be
/// downgraded mid-session.
#[tauri::command]
pub fn set_kiosk_level(state: State<'_, AppState>, level: String) -> CmdResult<()> {
    if !valid_kiosk_level(&level) {
        return Err(format!("unknown kiosk level: {level}"));
    }
    if state.locked.load(Ordering::SeqCst) {
        return Err("cannot change enforcement during a locked session".into());
    }
    let conn = state.db.0.lock().unwrap();
    db::set_config(&conn, "kiosk_level", &level).map_err(err)
}

/// Called by the webview on boot. Until this fires, the kiosk refuses to
/// engage (a dead webview has no escape hatch).
#[tauri::command]
pub fn mark_frontend_ready(state: State<'_, AppState>) -> CmdResult<()> {
    state.frontend_ready.store(true, Ordering::SeqCst);
    Ok(())
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
    let enforcement_disarmed = std::env::var_os("HOME")
        .map(|h| std::path::Path::new(&h).join("sdr-unlock").exists())
        .unwrap_or(false);
    let (schedule_paused, kiosk_level) = {
        let conn = state.db.0.lock().unwrap();
        (
            matches!(db::get_config(&conn, "schedule_paused"), Ok(Some(v)) if v == "1"),
            db::get_config(&conn, "kiosk_level").ok().flatten().unwrap_or_else(|| "hard".into()),
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
        enforcement_disarmed,
        schedule_paused,
        kiosk_level,
        model: state.generator.current_model(),
        agent: state.generator.current_agent(),
        custom_agent_bin: state.generator.current_custom_bin(),
    })
}

/// Pause the daily schedule entirely: launchd agent removed, owed checks
/// disabled, countdown hidden — dormant until resume_schedule.
#[tauri::command]
pub fn pause_schedule(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    {
        let conn = state.db.0.lock().unwrap();
        db::set_config(&conn, "schedule_paused", "1").map_err(err)?;
    }
    if !state.debug_day {
        crate::scheduler::uninstall()?;
    }
    let _ = app.emit("session:state", session::view(&state));
    Ok(())
}

#[tauri::command]
pub fn resume_schedule(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let (hour, minute) = {
        let conn = state.db.0.lock().unwrap();
        db::set_config(&conn, "schedule_paused", "0").map_err(err)?;
        (
            db::get_config(&conn, "schedule_hour").ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(9),
            db::get_config(&conn, "schedule_minute").ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(0),
        )
    };
    if !state.debug_day {
        crate::scheduler::install(hour, minute)?;
    }
    let _ = app.emit("session:state", session::view(&state));
    Ok(())
}

/// Ping whichever agent is currently selected (optionally an explicit one,
/// so the setup wizard can test a choice before saving it).
#[tauri::command]
pub async fn check_agent(
    state: State<'_, AppState>,
    agent: Option<String>,
    custom_bin: Option<String>,
) -> CmdResult<bool> {
    let gen = state.generator.clone();
    let which = agent.unwrap_or_else(|| gen.current_agent());
    const PING: &str = "reply with exactly: pong";
    let mut cmd = match which.as_str() {
        "codex" => {
            let bin = gen.codex_bin.clone().unwrap_or_else(|| "codex".into());
            let mut c = tokio::process::Command::new(bin);
            c.args(["exec", "--skip-git-repo-check", PING]);
            c
        }
        "custom" => {
            let bin = custom_bin.unwrap_or_else(|| gen.current_custom_bin());
            if bin.trim().is_empty() {
                return Ok(false);
            }
            let mut c = tokio::process::Command::new(bin.trim());
            c.arg(PING);
            c
        }
        _ => {
            let mut c = tokio::process::Command::new(&gen.claude_bin);
            c.args(["-p", PING, "--max-turns", "1", "--model", "haiku"]);
            c
        }
    };
    let out = cmd.current_dir(&gen.scratch_dir).output();
    match tokio::time::timeout(std::time::Duration::from_secs(120), out).await {
        Ok(Ok(o)) => Ok(o.status.success()),
        _ => Ok(false),
    }
}

#[derive(Deserialize)]
pub struct SetupInput {
    pub hour: u32,
    pub minute: u32,
    pub escape_phrase: String,
    #[serde(default)]
    pub kiosk_level: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub custom_agent_bin: Option<String>,
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
        let level = input.kiosk_level.as_deref().unwrap_or("hard");
        if !valid_kiosk_level(level) {
            return Err(format!("unknown kiosk level: {level}"));
        }
        db::set_config(&conn, "kiosk_level", level).map_err(err)?;
        let model = input.model.as_deref().unwrap_or("opus");
        if !valid_model(model) {
            return Err(format!("unknown model: {model}"));
        }
        db::set_config(&conn, "model", model).map_err(err)?;
        *state.generator.model.lock().unwrap() = model.to_string();
        let agent = input.agent.as_deref().unwrap_or("claude");
        if !valid_agent(agent) {
            return Err(format!("unknown agent: {agent}"));
        }
        let custom = input.custom_agent_bin.clone().unwrap_or_default();
        if agent == "custom" && custom.trim().is_empty() {
            return Err("custom agent needs a binary path".into());
        }
        db::set_config(&conn, "agent", agent).map_err(err)?;
        db::set_config(&conn, "custom_agent_bin", custom.trim()).map_err(err)?;
        *state.generator.agent.lock().unwrap() = agent.to_string();
        *state.generator.custom_bin.lock().unwrap() = custom.trim().to_string();
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
    let qs = session::questions_for_today(&conn, &today, &yesterday).map_err(err)?;
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

    let (questions, pending, concept_of) = {
        let conn = state.db.0.lock().unwrap();
        let qs = session::questions_for_today(&conn, &today, &yesterday).map_err(err)?;
        let pending = pending_answers(&conn, &today);
        // question_id -> (concept_id, concept_title), for grading context + mastery.
        let mut stmt = conn
            .prepare(
                "SELECT q.id, c.id, c.title FROM questions q
                 JOIN courses co ON co.id = q.course_id
                 JOIN concepts c ON c.id = co.concept_id",
            )
            .map_err(err)?;
        let concept_of: HashMap<i64, (i64, String)> = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, (r.get::<_, i64>(1)?, r.get::<_, String>(2)?))))
            .map_err(err)?
            .filter_map(|r| r.ok())
            .collect();
        (qs, pending, concept_of)
    };

    // Grade free-text via agent in one batch.
    let free_items: Vec<GradeItem> = questions
        .iter()
        .filter(|q| q.kind == "free")
        .map(|q| GradeItem {
            id: q.id,
            concept: concept_of.get(&q.id).map(|(_, t)| t.clone()).unwrap_or_default(),
            question: q.prompt.clone(),
            model_answer: q.correct_answer.clone(),
            user_answer: pending.get(&q.id).cloned().unwrap_or_default(),
        })
        .collect();
    let verdicts = state.generator.grade(&free_items).await;
    let self_assess = verdicts.is_none();
    let verdict_map: HashMap<i64, (bool, String, String)> = verdicts
        .unwrap_or_default()
        .into_iter()
        .map(|v| (v.id, (v.correct, v.feedback, v.note)))
        .collect();

    let mut items = Vec::new();
    let mut n_graded = 0usize;
    let mut n_correct = 0usize;
    // concept_id -> (correct, total) for mastery transitions after the loop.
    let mut by_concept: HashMap<i64, (usize, usize)> = HashMap::new();
    {
        let conn = state.db.0.lock().unwrap();
        for q in &questions {
            let user_answer = pending.get(&q.id).cloned().unwrap_or_default();
            let (correct, feedback): (Option<bool>, String) = if q.kind == "mcq" {
                let ok = user_answer.trim() == q.correct_answer.trim();
                (Some(ok), String::new())
            } else if let Some((ok, fb, _note)) = verdict_map.get(&q.id) {
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
                if let Some((cid, _)) = concept_of.get(&q.id) {
                    let e = by_concept.entry(*cid).or_insert((0, 0));
                    e.1 += 1;
                    if counted_correct {
                        e.0 += 1;
                    }
                }
            }
            // Teacher's per-concept observation from the grader, kept for next encounter.
            if let Some((_, _, note)) = verdict_map.get(&q.id) {
                if let Some((cid, _)) = concept_of.get(&q.id) {
                    let _ = crate::mastery::set_teacher_note(&conn, *cid, note);
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
        // Mastery transitions: one quiz encounter per concept touched today.
        for (cid, (ok, total)) in &by_concept {
            if *total > 0 {
                let _ = crate::mastery::record_quiz_outcome(&conn, *cid, &today, *ok as f64 / *total as f64);
            }
        }
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
    pub pool_unlocked: usize,
    pub pool_total: usize,
}

#[tauri::command]
pub fn finish_review(app: AppHandle, state: State<'_, AppState>) -> CmdResult<SessionView> {
    let is_pop = {
        let today = state.today();
        let conn = state.db.0.lock().unwrap();
        db::get_session(&conn, &today)
            .map_err(err)?
            .map(|s| s.session_type == "pop_quiz")
            .unwrap_or(false)
    };
    if is_pop {
        // Pop-quiz day: the audit IS the session — no new topic, done after review.
        session::complete_session(&app, &state).map_err(err)?;
    } else {
        session::set_step(&state, session::STEP_ROULETTE).map_err(err)?;
    }
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
    // Build a wheel pool from UNLOCKED concepts only: chosen + up to 11 others.
    let (unlocked, locked) = crate::roulette::pool_status(&conn).map_err(err)?;
    let pool_unlocked = unlocked.len();
    let pool_total = pool_unlocked + locked.len();
    let mut pool: Vec<String> = unlocked
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
        pool_unlocked,
        pool_total,
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

/// Audio lesson for today: returns the dialogue script (generating it live if
/// missing — ~1 min sonnet call) plus the playback engine. The frontend uses
/// speechSynthesis unless rendered files exist.
#[tauri::command]
pub async fn ensure_audio(app: AppHandle, state: State<'_, AppState>) -> CmdResult<crate::audio::AudioView> {
    let today = state.today();
    let course = {
        let conn = state.db.0.lock().unwrap();
        db::course_for_date(&conn, &today).map_err(err)?.ok_or("no course today")?
    };
    let _ = app.emit("gen:status", "writing audio script from today's course");
    session::ensure_audio_for_course(&state, &course, &today)
        .await
        .map_err(|e| format!("audio unavailable: {e}"))?;
    let conn = state.db.0.lock().unwrap();
    crate::audio::get_script(&conn, course.id)
        .map_err(err)?
        .ok_or_else(|| "audio script missing after generation".into())
}

#[tauri::command]
pub fn get_audio_enabled(state: State<'_, AppState>) -> CmdResult<bool> {
    let conn = state.db.0.lock().unwrap();
    Ok(matches!(db::get_config(&conn, "audio_enabled"), Ok(Some(v)) if v == "1"))
}

#[tauri::command]
pub fn set_audio_enabled(state: State<'_, AppState>, enabled: bool) -> CmdResult<()> {
    let conn = state.db.0.lock().unwrap();
    db::set_config(&conn, "audio_enabled", if enabled { "1" } else { "0" }).map_err(err)
}

#[derive(Serialize)]
pub struct ExitQuizQuestion {
    pub id: i64,
    pub prompt: String,
    pub choices: Vec<String>,
}

/// The 3-question exit check for TODAY's course. Generates on demand for
/// courses created before this feature (small sonnet call). Correct answers
/// never leave the backend.
#[tauri::command]
pub async fn get_exit_quiz(state: State<'_, AppState>) -> CmdResult<Vec<ExitQuizQuestion>> {
    let today = state.today();
    let (course, existing) = {
        let conn = state.db.0.lock().unwrap();
        let course = db::course_for_date(&conn, &today).map_err(err)?.ok_or("no course today")?;
        let existing = db::exit_questions_for_course(&conn, course.id).map_err(err)?;
        (course, existing)
    };
    let qs = if existing.is_empty() {
        let generated = state
            .generator
            .generate_exit_quiz(&course.markdown)
            .await
            .map_err(|e| format!("exit check unavailable: {e}"))?;
        let conn = state.db.0.lock().unwrap();
        for q in &generated {
            db::insert_exit_question(
                &conn,
                course.id,
                &q.prompt,
                &serde_json::to_string(&q.choices).map_err(err)?,
                &q.correct_answer,
                &q.explanation,
            )
            .map_err(err)?;
        }
        db::exit_questions_for_course(&conn, course.id).map_err(err)?
    } else {
        existing
    };
    Ok(qs
        .into_iter()
        .take(3)
        .map(|q| ExitQuizQuestion { id: q.id, prompt: q.prompt, choices: q.choices })
        .collect())
}

#[derive(Serialize)]
pub struct ExitQuizResult {
    pub passed: bool,
    pub correct: Vec<i64>,
    pub cooldown_seconds: i64,
}

/// Grade the exit check: all answers correct => the reading TTL unlocks now.
/// Failures get a 60s cooldown so the gate can't be brute-forced.
#[tauri::command]
pub fn submit_exit_quiz(
    app: AppHandle,
    state: State<'_, AppState>,
    answers: HashMap<i64, String>,
) -> CmdResult<ExitQuizResult> {
    let now = chrono::Utc::now().timestamp();
    {
        let mut fails = state.exit_quiz_failures.lock().unwrap();
        fails.retain(|t| now - *t < 60);
        if !fails.is_empty() {
            let wait = 60 - (now - fails.iter().max().copied().unwrap_or(now));
            return Ok(ExitQuizResult { passed: false, correct: vec![], cooldown_seconds: wait.max(1) });
        }
    }
    let today = state.today();
    let qs = {
        let conn = state.db.0.lock().unwrap();
        let course = db::course_for_date(&conn, &today).map_err(err)?.ok_or("no course today")?;
        db::exit_questions_for_course(&conn, course.id).map_err(err)?
    };
    if qs.is_empty() {
        return Err("no exit check exists for today".into());
    }
    let correct: Vec<i64> = qs
        .iter()
        .filter(|q| {
            answers
                .get(&q.id)
                .map(|a| a.trim() == q.correct_answer.trim())
                .unwrap_or(false)
        })
        .map(|q| q.id)
        .collect();
    let passed = correct.len() == qs.len();
    if passed {
        // Burn the remaining TTL: the running timer sees 0 and fires timer:done.
        let total = state.course_duration_secs();
        state.reading_remaining.store(0, Ordering::SeqCst);
        let conn = state.db.0.lock().unwrap();
        if let Ok(Some(mut s)) = db::get_session(&conn, &today) {
            s.reading_seconds = total;
            let _ = db::upsert_session(&conn, &s);
        }
        let _ = app.emit("timer:tick", 0);
        let _ = app.emit("timer:done", true);
    } else {
        state.exit_quiz_failures.lock().unwrap().push(now);
    }
    Ok(ExitQuizResult { passed, correct, cooldown_seconds: if passed { 0 } else { 60 } })
}

/// Voluntary "one more topic": re-opens today's completed session at the
/// roulette with a fresh concept and a fresh reading timer. Never locks the
/// kiosk and never becomes owed — purely user-initiated (TEACHER.md §5).
#[tauri::command]
pub fn extend_session(app: AppHandle, state: State<'_, AppState>) -> CmdResult<SessionView> {
    let today = state.today();
    let tomorrow = state.tomorrow();
    {
        let conn = state.db.0.lock().unwrap();
        let mut s = db::get_session(&conn, &today).map_err(err)?.ok_or("no session")?;
        if s.status != "completed" {
            return Err("extend is only available after today's session is complete".into());
        }
        // New spin, new course, new timer; today's earlier course stays archived.
        let c = crate::roulette::draw(&conn, &today).map_err(err)?.ok_or("empty pool")?;
        s.concept_id = Some(c.id);
        s.status = "in_progress".into();
        s.current_step = session::STEP_ROULETTE.into();
        s.reading_seconds = 0;
        db::upsert_session(&conn, &s).map_err(err)?;
        db::set_config(&conn, &format!("extended:{today}"), "1").map_err(err)?;
        // Appetite tracking for the dossier.
        let n: i64 = crate::mastery::get_profile(&conn, "multi_topic_days")
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let _ = crate::mastery::set_profile(&conn, "multi_topic_days", &(n + 1).to_string());
        // Tomorrow's quiz must also cover the extension course.
        db::jobs::requeue(&conn, "quiz", &tomorrow).map_err(err)?;
    }
    state.reading_remaining.store(0, Ordering::SeqCst);
    let v = session::view(&state);
    let _ = app.emit("session:state", v.clone());
    Ok(v)
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
    pub mastery: Vec<crate::mastery::MasteryEntry>,
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
    let mastery = crate::mastery::overview(&conn).map_err(err)?;
    Ok(DashboardView { history, streak, carryover_due, concepts_total, concepts_covered, mastery })
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
