//! The learner model: per-concept mastery ledger + the dossier the Teacher
//! reads before every generation call. See docs/TEACHER.md §2.
//!
//! Lifecycle: unseen → introduced → practicing → mastered → maintenance,
//! with struggling/decayed detours. Transitions are computed here, at
//! grading/completion time, from data the app already records.

use crate::db::{DbError, Result};
use rusqlite::{params, Connection};
use serde::Serialize;

/// Mastered requires this score on the current AND running (ema) record...
const MASTER_SCORE: f64 = 0.8;
/// ...across at least this many quiz encounters...
const MASTER_ENCOUNTERS: i64 = 2;
/// ...with at least this many days between the last two encounters.
const MASTER_GAP_DAYS: i64 = 7;
/// Below this, a quiz encounter marks the concept struggling.
const STRUGGLE_SCORE: f64 = 0.5;
/// Spaced-repetition review intervals once mastered.
const REVIEW_INTERVALS: &[i64] = &[7, 21, 60];

#[derive(Debug, Clone, Serialize)]
pub struct Mastery {
    pub concept_id: i64,
    pub state: String,
    pub score_ema: f64,
    pub encounters: i64,
    pub last_seen_date: Option<String>,
    pub next_review_date: Option<String>,
    pub review_interval_days: i64,
    pub teacher_notes: String,
}

fn default_row(concept_id: i64) -> Mastery {
    Mastery {
        concept_id,
        state: "unseen".into(),
        score_ema: 0.0,
        encounters: 0,
        last_seen_date: None,
        next_review_date: None,
        review_interval_days: 7,
        teacher_notes: String::new(),
    }
}

pub fn get(conn: &Connection, concept_id: i64) -> Result<Mastery> {
    let mut stmt = conn.prepare(
        "SELECT concept_id, state, score_ema, encounters, last_seen_date,
                next_review_date, review_interval_days, teacher_notes
         FROM mastery WHERE concept_id = ?1",
    )?;
    let mut rows = stmt.query(params![concept_id])?;
    Ok(match rows.next()? {
        Some(r) => Mastery {
            concept_id: r.get(0)?,
            state: r.get(1)?,
            score_ema: r.get(2)?,
            encounters: r.get(3)?,
            last_seen_date: r.get(4)?,
            next_review_date: r.get(5)?,
            review_interval_days: r.get(6)?,
            teacher_notes: r.get(7)?,
        },
        None => default_row(concept_id),
    })
}

fn upsert(conn: &Connection, m: &Mastery) -> Result<()> {
    conn.execute(
        "INSERT INTO mastery (concept_id, state, score_ema, encounters, last_seen_date,
                              next_review_date, review_interval_days, teacher_notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(concept_id) DO UPDATE SET
            state = excluded.state,
            score_ema = excluded.score_ema,
            encounters = excluded.encounters,
            last_seen_date = excluded.last_seen_date,
            next_review_date = excluded.next_review_date,
            review_interval_days = excluded.review_interval_days,
            teacher_notes = excluded.teacher_notes",
        params![
            m.concept_id, m.state, m.score_ema, m.encounters, m.last_seen_date,
            m.next_review_date, m.review_interval_days, m.teacher_notes
        ],
    )?;
    Ok(())
}

fn days_between(earlier: &str, later: &str) -> i64 {
    let parse = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d");
    match (parse(earlier), parse(later)) {
        (Ok(a), Ok(b)) => (b - a).num_days(),
        _ => 0,
    }
}

fn add_days(date: &str, days: i64) -> String {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|d| (d + chrono::Duration::days(days)).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|_| date.to_string())
}

/// Course read to completion: unseen → introduced. Later states unaffected
/// (re-reading a topic never demotes it).
pub fn record_course_read(conn: &Connection, concept_id: i64, date: &str) -> Result<()> {
    let mut m = get(conn, concept_id)?;
    if m.state == "unseen" {
        m.state = "introduced".into();
    }
    m.last_seen_date = Some(date.to_string());
    upsert(conn, &m)
}

/// A quiz encounter for this concept: `score` is the fraction correct of
/// today's questions belonging to it. Drives all state transitions.
pub fn record_quiz_outcome(conn: &Connection, concept_id: i64, date: &str, score: f64) -> Result<Mastery> {
    let mut m = get(conn, concept_id)?;
    let prev_seen = m.last_seen_date.clone();
    m.encounters += 1;
    m.score_ema = if m.encounters <= 1 { score } else { 0.6 * score + 0.4 * m.score_ema };

    let gap_ok = prev_seen
        .as_deref()
        .map(|p| days_between(p, date) >= MASTER_GAP_DAYS)
        .unwrap_or(false);

    m.state = match m.state.as_str() {
        "mastered" | "maintenance" => {
            if score < MASTER_SCORE {
                // Maintenance check failed: knowledge decayed, re-enters practice.
                m.review_interval_days = REVIEW_INTERVALS[0];
                m.next_review_date = None;
                "decayed".into()
            } else {
                // Passed review: advance the spaced-repetition interval.
                let next_interval = REVIEW_INTERVALS
                    .iter()
                    .find(|&&i| i > m.review_interval_days)
                    .copied()
                    .unwrap_or(*REVIEW_INTERVALS.last().unwrap());
                m.review_interval_days = next_interval;
                m.next_review_date = Some(add_days(date, next_interval));
                "maintenance".into()
            }
        }
        _ => {
            if score >= MASTER_SCORE && m.score_ema >= MASTER_SCORE && m.encounters >= MASTER_ENCOUNTERS && gap_ok {
                m.review_interval_days = REVIEW_INTERVALS[0];
                m.next_review_date = Some(add_days(date, REVIEW_INTERVALS[0]));
                "mastered".into()
            } else if score < STRUGGLE_SCORE {
                "struggling".into()
            } else {
                "practicing".into()
            }
        }
    };
    m.last_seen_date = Some(date.to_string());
    upsert(conn, &m)?;
    Ok(m)
}

/// Persist the Teacher's observation about the student on this concept
/// (written by the grading call; injected back on the next encounter).
pub fn set_teacher_note(conn: &Connection, concept_id: i64, note: &str) -> Result<()> {
    let note = note.trim();
    if note.is_empty() {
        return Ok(());
    }
    let mut m = get(conn, concept_id)?;
    m.teacher_notes = note.chars().take(280).collect();
    upsert(conn, &m)
}

#[derive(Debug, Clone, Serialize)]
pub struct MasteryEntry {
    pub concept_id: i64,
    pub slug: String,
    pub title: String,
    pub category: String,
    pub state: String,
    pub score_ema: f64,
}

/// Every active concept with its mastery state (unseen when never touched).
pub fn overview(conn: &Connection) -> Result<Vec<MasteryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.slug, c.title, c.category,
                COALESCE(m.state, 'unseen'), COALESCE(m.score_ema, 0)
         FROM concepts c LEFT JOIN mastery m ON m.concept_id = c.id
         WHERE c.active = 1 ORDER BY c.category, c.title",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(MasteryEntry {
            concept_id: r.get(0)?,
            slug: r.get(1)?,
            title: r.get(2)?,
            category: r.get(3)?,
            state: r.get(4)?,
            score_ema: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>().map_err(DbError::from)?)
}

pub fn get_profile(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM profile WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    Ok(match rows.next()? {
        Some(r) => Some(r.get(0)?),
        None => None,
    })
}

pub fn set_profile(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO profile (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// The learner dossier: ~1 page of markdown compiled fresh from the ledger,
/// prepended to every Teacher call. This is the agent's entire memory.
pub fn build_dossier(conn: &Connection, today: &str) -> Result<String> {
    let days_taught: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sessions WHERE status = 'completed'",
        [],
        |r| r.get(0),
    )?;
    let streak = crate::db::streak(conn, today).unwrap_or(0);
    let all = overview(conn)?;

    let list = |state: &str| -> Vec<&MasteryEntry> { all.iter().filter(|e| e.state == state).collect() };
    let titles = |es: &[&MasteryEntry]| -> String {
        es.iter().map(|e| e.slug.as_str()).collect::<Vec<_>>().join(", ")
    };

    let mastered: Vec<&MasteryEntry> = all
        .iter()
        .filter(|e| e.state == "mastered" || e.state == "maintenance")
        .collect();
    let practicing = list("practicing");
    let introduced = list("introduced");

    let mut out = String::new();
    out.push_str(&format!(
        "Day {} of teaching this student. Current streak: {} day(s).\n",
        days_taught + 1,
        streak
    ));
    if let Ok(Some(n)) = get_profile(conn, "multi_topic_days") {
        out.push_str(&format!(
            "Voluntary extra-topic sessions taken: {n} — this student sometimes asks for more.\n"
        ));
    }
    if !mastered.is_empty() {
        out.push_str(&format!("MASTERED ({}): {}\n", mastered.len(), titles(&mastered)));
    }

    // Struggling + decayed carry their notes — this is what the Teacher must address.
    let mut needs_work: Vec<String> = Vec::new();
    for e in all.iter().filter(|e| e.state == "struggling" || e.state == "decayed") {
        let m = get(conn, e.concept_id)?;
        let mut line = format!("{} ({}, score {:.0}%", e.slug, e.state, m.score_ema * 100.0);
        if !m.teacher_notes.is_empty() {
            line.push_str(&format!(", notes: \"{}\"", m.teacher_notes));
        }
        line.push(')');
        needs_work.push(line);
    }
    if !needs_work.is_empty() {
        out.push_str(&format!("STRUGGLING ({}): {}\n", needs_work.len(), needs_work.join("; ")));
    }
    if !practicing.is_empty() {
        out.push_str(&format!("PRACTICING ({}): {}\n", practicing.len(), titles(&practicing)));
    }
    if !introduced.is_empty() {
        out.push_str(&format!("INTRODUCED, NOT YET QUIZZED ({}): {}\n", introduced.len(), titles(&introduced)));
    }

    // Due for spaced review.
    let mut stmt = conn.prepare(
        "SELECT c.slug, m.last_seen_date FROM mastery m JOIN concepts c ON c.id = m.concept_id
         WHERE m.next_review_date IS NOT NULL AND m.next_review_date <= ?1",
    )?;
    let due: Vec<String> = stmt
        .query_map(params![today], |r| {
            let slug: String = r.get(0)?;
            let last: Option<String> = r.get(1)?;
            Ok(match last {
                Some(l) => format!("{slug} (last seen {l})"),
                None => slug,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !due.is_empty() {
        out.push_str(&format!("DUE FOR REVIEW ({}): {}\n", due.len(), due.join(", ")));
    }

    // Recent courses give continuity ("as we saw when we covered X").
    let mut stmt = conn.prepare(
        "SELECT co.session_date, c.title FROM courses co JOIN concepts c ON c.id = co.concept_id
         ORDER BY co.session_date DESC LIMIT 5",
    )?;
    let recent: Vec<String> = stmt
        .query_map([], |r| {
            Ok(format!("{} — {}", r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !recent.is_empty() {
        out.push_str("RECENT COURSES:\n");
        for l in recent {
            out.push_str(&format!("  {l}\n"));
        }
    }
    if let Ok(Some(weak)) = get_profile(conn, "learning_notes") {
        out.push_str(&format!("PROFILE NOTES: {weak}\n"));
    }
    Ok(out)
}
