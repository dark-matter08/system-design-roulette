//! Curriculum-aware topic selection (TEACHER.md §3).
//!
//! The wheel only shows *unlocked* concepts: a concept unlocks when at least
//! 70% of its prerequisites have been quizzed at least once (mastery state
//! practicing or beyond). Tier 0 has no prerequisites — that's the day-1
//! wheel — and the pool visibly grows as the student progresses.

use crate::db::{self, Concept};
use rand::Rng;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

/// Fraction of a concept's prereqs that must be practiced before it unlocks.
const UNLOCK_THRESHOLD: f64 = 0.7;
/// Draw-weight multiplier for concepts in a category the student is
/// struggling in or that is due for review (steers the wheel toward debt).
const HOT_CATEGORY_BOOST: f64 = 2.0;

/// Mastery states that count as "the student has practiced this".
fn is_practiced(state: &str) -> bool {
    matches!(state, "practicing" | "struggling" | "mastered" | "maintenance" | "decayed")
}

fn prereqs_map(conn: &Connection) -> db::Result<HashMap<i64, Vec<String>>> {
    let mut stmt = conn.prepare("SELECT id, prereqs_json FROM concepts WHERE active = 1")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = HashMap::new();
    for row in rows {
        let (id, json) = row?;
        out.insert(id, serde_json::from_str(&json).unwrap_or_default());
    }
    Ok(out)
}

fn states_by_slug(conn: &Connection) -> db::Result<HashMap<String, String>> {
    let mut stmt = conn.prepare(
        "SELECT c.slug, COALESCE(m.state, 'unseen')
         FROM concepts c LEFT JOIN mastery m ON m.concept_id = c.id
         WHERE c.active = 1",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    Ok(rows.collect::<std::result::Result<HashMap<_, _>, _>>()?)
}

/// All active concepts split into (unlocked, locked).
pub fn pool_status(conn: &Connection) -> db::Result<(Vec<Concept>, Vec<Concept>)> {
    let all = db::all_concepts(conn)?;
    let prereqs = prereqs_map(conn)?;
    let states = states_by_slug(conn)?;
    let mut unlocked = Vec::new();
    let mut locked = Vec::new();
    for c in all {
        let reqs = prereqs.get(&c.id).cloned().unwrap_or_default();
        let open = if reqs.is_empty() {
            true
        } else {
            let practiced = reqs
                .iter()
                .filter(|s| states.get(*s).map(|st| is_practiced(st)).unwrap_or(false))
                .count();
            practiced as f64 / reqs.len() as f64 >= UNLOCK_THRESHOLD
        };
        if open {
            unlocked.push(c);
        } else {
            locked.push(c);
        }
    }
    Ok((unlocked, locked))
}

/// Categories the student owes attention: any struggling/decayed concept,
/// or one whose spaced review is due.
fn hot_categories(conn: &Connection, today: &str) -> db::Result<HashSet<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT c.category
         FROM mastery m JOIN concepts c ON c.id = m.concept_id
         WHERE m.state IN ('struggling','decayed')
            OR (m.next_review_date IS NOT NULL AND m.next_review_date <= ?1)",
    )?;
    let rows = stmt.query_map([today], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<std::result::Result<HashSet<_>, _>>()?)
}

/// Weighted random draw over the least-picked *unlocked* concepts, biased
/// toward categories with open debt. Marks the pick and returns it.
pub fn draw(conn: &Connection, date: &str) -> db::Result<Option<Concept>> {
    let (unlocked, _) = pool_status(conn)?;
    if unlocked.is_empty() {
        return Ok(None);
    }
    // No repeats until the unlocked pool exhausts a full lap.
    let min_picked = unlocked.iter().map(|c| c.times_picked).min().unwrap_or(0);
    let pool: Vec<&Concept> = unlocked.iter().filter(|c| c.times_picked == min_picked).collect();
    let hot = hot_categories(conn, date).unwrap_or_default();
    let weight_of = |c: &Concept| -> f64 {
        let base = c.weight.max(0.01);
        if hot.contains(&c.category) { base * HOT_CATEGORY_BOOST } else { base }
    };
    let total: f64 = pool.iter().map(|c| weight_of(c)).sum();
    let mut roll = rand::thread_rng().gen_range(0.0..total);
    let mut chosen = (*pool.last().expect("non-empty pool")).clone();
    for c in &pool {
        roll -= weight_of(c);
        if roll <= 0.0 {
            chosen = (*c).clone();
            break;
        }
    }
    db::mark_concept_picked(conn, chosen.id, date)?;
    Ok(Some(chosen))
}
