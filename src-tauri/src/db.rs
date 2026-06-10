use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Db(pub Mutex<Connection>);

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, DbError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concept {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub category: String,
    pub weight: f64,
    pub times_picked: i64,
    pub last_picked_date: Option<String>,
    pub tier: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub date: String,
    pub concept_id: Option<i64>,
    pub status: String,
    pub current_step: String,
    pub quiz_score: Option<f64>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub reading_seconds: i64,
    /// 'lesson' (default) or 'pop_quiz' — chosen by the nightly planner.
    pub session_type: String,
    /// Teacher's one-line reason for the chosen type (shown in UI).
    pub plan_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Course {
    pub id: i64,
    pub session_date: String,
    pub concept_id: i64,
    pub markdown: String,
    pub resources_json: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub id: i64,
    pub course_id: i64,
    pub prompt: String,
    pub kind: String,
    pub choices_json: Option<String>,
    pub correct_answer: String,
    pub explanation: String,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attempt {
    pub question_id: i64,
    pub session_date: String,
    pub user_answer: String,
    pub correct: bool,
    pub grader_feedback: String,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS concepts (
    id INTEGER PRIMARY KEY,
    slug TEXT UNIQUE NOT NULL,
    title TEXT NOT NULL,
    category TEXT NOT NULL,
    weight REAL NOT NULL DEFAULT 1.0,
    times_picked INTEGER NOT NULL DEFAULT 0,
    last_picked_date TEXT,
    active INTEGER NOT NULL DEFAULT 1,
    tier INTEGER NOT NULL DEFAULT 0,
    prereqs_json TEXT NOT NULL DEFAULT '[]'
);
CREATE TABLE IF NOT EXISTS sessions (
    date TEXT PRIMARY KEY,
    concept_id INTEGER REFERENCES concepts(id),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending','in_progress','completed','skipped')),
    current_step TEXT NOT NULL DEFAULT 'quiz',
    quiz_score REAL,
    started_at TEXT,
    completed_at TEXT,
    reading_seconds INTEGER NOT NULL DEFAULT 0,
    session_type TEXT NOT NULL DEFAULT 'lesson',
    plan_reason TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS courses (
    id INTEGER PRIMARY KEY,
    session_date TEXT NOT NULL,
    concept_id INTEGER NOT NULL REFERENCES concepts(id),
    markdown TEXT NOT NULL,
    resources_json TEXT NOT NULL DEFAULT '[]',
    source TEXT NOT NULL CHECK(source IN ('claude','codex','fallback')),
    generated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS questions (
    id INTEGER PRIMARY KEY,
    course_id INTEGER NOT NULL REFERENCES courses(id),
    prompt TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('mcq','free')),
    choices_json TEXT,
    correct_answer TEXT NOT NULL,
    explanation TEXT NOT NULL,
    origin TEXT NOT NULL DEFAULT 'fresh' CHECK(origin IN ('fresh','carryover'))
);
CREATE TABLE IF NOT EXISTS attempts (
    id INTEGER PRIMARY KEY,
    question_id INTEGER NOT NULL REFERENCES questions(id),
    session_date TEXT NOT NULL,
    user_answer TEXT NOT NULL,
    correct INTEGER NOT NULL,
    grader_feedback TEXT NOT NULL DEFAULT '',
    graded_by TEXT NOT NULL DEFAULT 'local'
);
CREATE TABLE IF NOT EXISTS carryover (
    question_id INTEGER PRIMARY KEY REFERENCES questions(id),
    failed_on TEXT NOT NULL,
    times_failed INTEGER NOT NULL DEFAULT 1,
    scheduled_for TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS exit_questions (
    id INTEGER PRIMARY KEY,
    course_id INTEGER NOT NULL REFERENCES courses(id),
    prompt TEXT NOT NULL,
    choices_json TEXT NOT NULL,
    correct_answer TEXT NOT NULL,
    explanation TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS mastery (
    concept_id INTEGER PRIMARY KEY REFERENCES concepts(id),
    state TEXT NOT NULL DEFAULT 'unseen'
        CHECK(state IN ('unseen','introduced','practicing','struggling','mastered','maintenance','decayed')),
    score_ema REAL NOT NULL DEFAULT 0,
    encounters INTEGER NOT NULL DEFAULT 0,
    last_seen_date TEXT,
    next_review_date TEXT,
    review_interval_days INTEGER NOT NULL DEFAULT 7,
    teacher_notes TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS profile (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS generation_jobs (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('course','quiz')),
    target_date TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued'
        CHECK(status IN ('queued','running','done','failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    created_at TEXT NOT NULL,
    finished_at TEXT,
    UNIQUE(kind, target_date)
);
"#;

pub fn open(path: &PathBuf) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    conn.execute_batch(SCHEMA)?;
    // Column migrations for pre-existing databases (IF NOT EXISTS only covers
    // new tables). Duplicate-column errors mean already migrated — ignore.
    for ddl in [
        "ALTER TABLE concepts ADD COLUMN tier INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE concepts ADD COLUMN prereqs_json TEXT NOT NULL DEFAULT '[]'",
        "ALTER TABLE sessions ADD COLUMN session_type TEXT NOT NULL DEFAULT 'lesson'",
        "ALTER TABLE sessions ADD COLUMN plan_reason TEXT NOT NULL DEFAULT ''",
    ] {
        let _ = conn.execute_batch(ddl);
    }
    Ok(conn)
}

pub fn seed_concepts(conn: &Connection, seed_json: &str) -> Result<usize> {
    #[derive(Deserialize)]
    struct SeedConcept {
        slug: String,
        title: String,
        category: String,
        #[serde(default)]
        tier: i64,
        #[serde(default)]
        prereqs: Vec<String>,
    }
    let seeds: Vec<SeedConcept> = serde_json::from_str(seed_json)?;
    let mut inserted = 0;
    for s in seeds {
        let prereqs_json = serde_json::to_string(&s.prereqs)?;
        inserted += conn.execute(
            "INSERT OR IGNORE INTO concepts (slug, title, category, tier, prereqs_json) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![s.slug, s.title, s.category, s.tier, prereqs_json],
        )?;
        // Curriculum metadata always refreshes from seed (existing installs
        // pick up tier/prereq changes); progress columns are never touched.
        conn.execute(
            "UPDATE concepts SET title = ?2, category = ?3, tier = ?4, prereqs_json = ?5 WHERE slug = ?1",
            params![s.slug, s.title, s.category, s.tier, prereqs_json],
        )?;
    }
    Ok(inserted)
}

pub fn get_config(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM config WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    Ok(match rows.next()? {
        Some(row) => Some(row.get(0)?),
        None => None,
    })
}

pub fn set_config(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO config (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn get_session(conn: &Connection, date: &str) -> Result<Option<Session>> {
    let mut stmt = conn.prepare(
        "SELECT date, concept_id, status, current_step, quiz_score, started_at, completed_at, reading_seconds,
                session_type, plan_reason
         FROM sessions WHERE date = ?1",
    )?;
    let mut rows = stmt.query(params![date])?;
    Ok(match rows.next()? {
        Some(r) => Some(Session {
            date: r.get(0)?,
            concept_id: r.get(1)?,
            status: r.get(2)?,
            current_step: r.get(3)?,
            quiz_score: r.get(4)?,
            started_at: r.get(5)?,
            completed_at: r.get(6)?,
            reading_seconds: r.get(7)?,
            session_type: r.get(8)?,
            plan_reason: r.get(9)?,
        }),
        None => None,
    })
}

pub fn upsert_session(conn: &Connection, s: &Session) -> Result<()> {
    conn.execute(
        "INSERT INTO sessions (date, concept_id, status, current_step, quiz_score, started_at, completed_at, reading_seconds, session_type, plan_reason)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(date) DO UPDATE SET
            concept_id = excluded.concept_id,
            status = excluded.status,
            current_step = excluded.current_step,
            quiz_score = excluded.quiz_score,
            started_at = excluded.started_at,
            completed_at = excluded.completed_at,
            reading_seconds = excluded.reading_seconds,
            session_type = excluded.session_type,
            plan_reason = excluded.plan_reason",
        params![
            s.date, s.concept_id, s.status, s.current_step,
            s.quiz_score, s.started_at, s.completed_at, s.reading_seconds,
            s.session_type, s.plan_reason
        ],
    )?;
    Ok(())
}

pub fn insert_course(
    conn: &Connection,
    session_date: &str,
    concept_id: i64,
    markdown: &str,
    resources_json: &str,
    source: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO courses (session_date, concept_id, markdown, resources_json, source, generated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
        params![session_date, concept_id, markdown, resources_json, source],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn course_for_date(conn: &Connection, session_date: &str) -> Result<Option<Course>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_date, concept_id, markdown, resources_json, source
         FROM courses WHERE session_date = ?1 ORDER BY id DESC LIMIT 1",
    )?;
    let mut rows = stmt.query(params![session_date])?;
    Ok(match rows.next()? {
        Some(r) => Some(Course {
            id: r.get(0)?,
            session_date: r.get(1)?,
            concept_id: r.get(2)?,
            markdown: r.get(3)?,
            resources_json: r.get(4)?,
            source: r.get(5)?,
        }),
        None => None,
    })
}

pub fn insert_question(
    conn: &Connection,
    course_id: i64,
    prompt: &str,
    kind: &str,
    choices_json: Option<&str>,
    correct_answer: &str,
    explanation: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO questions (course_id, prompt, kind, choices_json, correct_answer, explanation)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![course_id, prompt, kind, choices_json, correct_answer, explanation],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn questions_for_course(conn: &Connection, course_id: i64) -> Result<Vec<Question>> {
    let mut stmt = conn.prepare(
        "SELECT id, course_id, prompt, kind, choices_json, correct_answer, explanation, origin
         FROM questions WHERE course_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map(params![course_id], row_to_question)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn row_to_question(r: &rusqlite::Row) -> std::result::Result<Question, rusqlite::Error> {
    Ok(Question {
        id: r.get(0)?,
        course_id: r.get(1)?,
        prompt: r.get(2)?,
        kind: r.get(3)?,
        choices_json: r.get(4)?,
        correct_answer: r.get(5)?,
        explanation: r.get(6)?,
        origin: r.get(7)?,
    })
}

/// Today's quiz = fresh questions generated from yesterday's course + carryover due today.
pub fn quiz_for_date(conn: &Connection, date: &str, yesterday: &str) -> Result<Vec<Question>> {
    let mut out: Vec<Question> = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT q.id, q.course_id, q.prompt, q.kind, q.choices_json, q.correct_answer, q.explanation, 'carryover'
         FROM carryover c JOIN questions q ON q.id = c.question_id
         WHERE c.scheduled_for <= ?1 ORDER BY c.failed_on",
    )?;
    let rows = stmt.query_map(params![date], row_to_question)?;
    for q in rows {
        out.push(q?);
    }
    let mut stmt = conn.prepare(
        "SELECT q.id, q.course_id, q.prompt, q.kind, q.choices_json, q.correct_answer, q.explanation, q.origin
         FROM questions q JOIN courses co ON co.id = q.course_id
         WHERE co.session_date = ?1
           AND q.id NOT IN (SELECT question_id FROM carryover)
           AND q.id NOT IN (SELECT question_id FROM attempts)
         ORDER BY q.id",
    )?;
    let rows = stmt.query_map(params![yesterday], row_to_question)?;
    for q in rows {
        out.push(q?);
    }
    Ok(out)
}

/// Exit-check questions: 3 MCQs on TODAY's course that unlock the reader
/// early. Deliberately separate from `questions` (tomorrow's quiz).
pub fn insert_exit_question(
    conn: &Connection,
    course_id: i64,
    prompt: &str,
    choices_json: &str,
    correct_answer: &str,
    explanation: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO exit_questions (course_id, prompt, choices_json, correct_answer, explanation)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![course_id, prompt, choices_json, correct_answer, explanation],
    )?;
    Ok(conn.last_insert_rowid())
}

#[derive(Debug, Clone, Serialize)]
pub struct ExitQuestion {
    pub id: i64,
    pub prompt: String,
    pub choices: Vec<String>,
    pub correct_answer: String,
    pub explanation: String,
}

pub fn exit_questions_for_course(conn: &Connection, course_id: i64) -> Result<Vec<ExitQuestion>> {
    let mut stmt = conn.prepare(
        "SELECT id, prompt, choices_json, correct_answer, explanation
         FROM exit_questions WHERE course_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map(params![course_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, prompt, choices_json, correct_answer, explanation) = row?;
        out.push(ExitQuestion {
            id,
            prompt,
            choices: serde_json::from_str(&choices_json).unwrap_or_default(),
            correct_answer,
            explanation,
        });
    }
    Ok(out)
}

/// Breadth sample for a pop-quiz day: previously-attempted questions from
/// quizzed concepts, prioritizing struggling/decayed then review-due, random
/// within a band. Excludes anything already in today's base set.
pub fn pop_quiz_sample(conn: &Connection, date: &str, exclude: &[i64], limit: i64) -> Result<Vec<Question>> {
    let exclude_csv = exclude.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT q.id, q.course_id, q.prompt, q.kind, q.choices_json, q.correct_answer, q.explanation, q.origin
         FROM questions q
         JOIN courses co ON co.id = q.course_id
         JOIN mastery m ON m.concept_id = co.concept_id
         WHERE q.id IN (SELECT DISTINCT question_id FROM attempts)
           AND q.id NOT IN (SELECT question_id FROM carryover)
           {}
         ORDER BY CASE
             WHEN m.state IN ('struggling','decayed') THEN 0
             WHEN m.next_review_date IS NOT NULL AND m.next_review_date <= ?1 THEN 1
             ELSE 2 END,
           RANDOM()
         LIMIT ?2",
        if exclude_csv.is_empty() { String::new() } else { format!("AND q.id NOT IN ({exclude_csv})") }
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![date, limit], row_to_question)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn record_attempt(conn: &Connection, a: &Attempt) -> Result<()> {
    conn.execute(
        "INSERT INTO attempts (question_id, session_date, user_answer, correct, grader_feedback)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![a.question_id, a.session_date, a.user_answer, a.correct as i64, a.grader_feedback],
    )?;
    Ok(())
}

pub fn attempts_for_session(conn: &Connection, date: &str) -> Result<Vec<Attempt>> {
    let mut stmt = conn.prepare(
        "SELECT question_id, session_date, user_answer, correct, grader_feedback
         FROM attempts WHERE session_date = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map(params![date], |r| {
        Ok(Attempt {
            question_id: r.get(0)?,
            session_date: r.get(1)?,
            user_answer: r.get(2)?,
            correct: r.get::<_, i64>(3)? != 0,
            grader_feedback: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// Question failed today: schedule (or reschedule) it for tomorrow.
pub fn push_carryover(conn: &Connection, question_id: i64, today: &str, tomorrow: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO carryover (question_id, failed_on, scheduled_for)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(question_id) DO UPDATE SET
            times_failed = times_failed + 1,
            failed_on = excluded.failed_on,
            scheduled_for = excluded.scheduled_for",
        params![question_id, today, tomorrow],
    )?;
    Ok(())
}

pub fn clear_carryover(conn: &Connection, question_id: i64) -> Result<()> {
    conn.execute("DELETE FROM carryover WHERE question_id = ?1", params![question_id])?;
    Ok(())
}

pub fn carryover_count(conn: &Connection, due_by: &str) -> Result<i64> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM carryover WHERE scheduled_for <= ?1",
        params![due_by],
        |r| r.get(0),
    )?;
    Ok(n)
}

/// Pool restricted to least-picked active concepts: full pool exhausts before repeats.
pub fn roulette_pool(conn: &Connection) -> Result<Vec<Concept>> {
    let mut stmt = conn.prepare(
        "SELECT id, slug, title, category, weight, times_picked, last_picked_date, tier
         FROM concepts
         WHERE active = 1
           AND times_picked = (SELECT MIN(times_picked) FROM concepts WHERE active = 1)",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Concept {
            id: r.get(0)?,
            slug: r.get(1)?,
            title: r.get(2)?,
            category: r.get(3)?,
            weight: r.get(4)?,
            times_picked: r.get(5)?,
            last_picked_date: r.get(6)?,
            tier: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn all_concepts(conn: &Connection) -> Result<Vec<Concept>> {
    let mut stmt = conn.prepare(
        "SELECT id, slug, title, category, weight, times_picked, last_picked_date, tier
         FROM concepts WHERE active = 1 ORDER BY title",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Concept {
            id: r.get(0)?,
            slug: r.get(1)?,
            title: r.get(2)?,
            category: r.get(3)?,
            weight: r.get(4)?,
            times_picked: r.get(5)?,
            last_picked_date: r.get(6)?,
            tier: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn get_concept(conn: &Connection, id: i64) -> Result<Option<Concept>> {
    let mut stmt = conn.prepare(
        "SELECT id, slug, title, category, weight, times_picked, last_picked_date, tier
         FROM concepts WHERE id = ?1",
    )?;
    let mut rows = stmt.query(params![id])?;
    Ok(match rows.next()? {
        Some(r) => Some(Concept {
            id: r.get(0)?,
            slug: r.get(1)?,
            title: r.get(2)?,
            category: r.get(3)?,
            weight: r.get(4)?,
            times_picked: r.get(5)?,
            last_picked_date: r.get(6)?,
            tier: r.get(7)?,
        }),
        None => None,
    })
}

pub fn mark_concept_picked(conn: &Connection, id: i64, date: &str) -> Result<()> {
    conn.execute(
        "UPDATE concepts SET times_picked = times_picked + 1, last_picked_date = ?2 WHERE id = ?1",
        params![id, date],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub date: String,
    pub status: String,
    pub quiz_score: Option<f64>,
    pub concept_title: Option<String>,
}

pub fn history(conn: &Connection, limit: i64) -> Result<Vec<HistoryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT s.date, s.status, s.quiz_score,
                CASE WHEN s.status = 'pending' THEN NULL ELSE c.title END
         FROM sessions s LEFT JOIN concepts c ON c.id = s.concept_id
         WHERE s.status != 'pending' OR s.date <= date('now', 'localtime')
         ORDER BY s.date DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |r| {
        Ok(HistoryEntry {
            date: r.get(0)?,
            status: r.get(1)?,
            quiz_score: r.get(2)?,
            concept_title: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// Consecutive completed days ending today or yesterday.
pub fn streak(conn: &Connection, today: &str) -> Result<i64> {
    let mut stmt =
        conn.prepare("SELECT date FROM sessions WHERE status = 'completed' ORDER BY date DESC")?;
    let dates: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let today_d = match chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Ok(0),
    };
    let mut streak = 0i64;
    let mut expect = today_d;
    for ds in &dates {
        let d = match chrono::NaiveDate::parse_from_str(ds, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => continue,
        };
        if streak == 0 && d == today_d.pred_opt().unwrap_or(today_d) {
            expect = d;
        }
        if d == expect {
            streak += 1;
            expect = d.pred_opt().unwrap_or(d);
        } else if d < expect {
            break;
        }
    }
    Ok(streak)
}

pub mod jobs {
    use super::*;

    pub fn enqueue(conn: &Connection, kind: &str, target_date: &str) -> Result<()> {
        conn.execute(
            "INSERT INTO generation_jobs (kind, target_date, status, created_at)
             VALUES (?1, ?2, 'queued', datetime('now'))
             ON CONFLICT(kind, target_date) DO UPDATE SET
                status = CASE WHEN generation_jobs.status = 'done' THEN 'done' ELSE 'queued' END",
            params![kind, target_date],
        )?;
        Ok(())
    }

    /// Force a job back to queued even if it already ran (used when an
    /// extended session adds a course after tomorrow's quiz was generated).
    pub fn requeue(conn: &Connection, kind: &str, target_date: &str) -> Result<()> {
        conn.execute(
            "INSERT INTO generation_jobs (kind, target_date, status, created_at)
             VALUES (?1, ?2, 'queued', datetime('now'))
             ON CONFLICT(kind, target_date) DO UPDATE SET status = 'queued', attempts = 0",
            params![kind, target_date],
        )?;
        Ok(())
    }

    pub fn next_queued(conn: &Connection) -> Result<Option<(i64, String, String)>> {
        let mut stmt = conn.prepare(
            "SELECT id, kind, target_date FROM generation_jobs
             WHERE status = 'queued' AND attempts < 3 ORDER BY id LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        Ok(match rows.next()? {
            Some(r) => Some((r.get(0)?, r.get(1)?, r.get(2)?)),
            None => None,
        })
    }

    pub fn mark(conn: &Connection, id: i64, status: &str, error: Option<&str>) -> Result<()> {
        conn.execute(
            "UPDATE generation_jobs SET status = ?2, error = ?3,
                attempts = attempts + CASE WHEN ?2 IN ('failed','done') THEN 1 ELSE 0 END,
                finished_at = CASE WHEN ?2 IN ('failed','done') THEN datetime('now') ELSE finished_at END
             WHERE id = ?1",
            params![id, status, error],
        )?;
        Ok(())
    }
}
