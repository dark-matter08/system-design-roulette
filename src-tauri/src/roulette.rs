use crate::db::{self, Concept};
use rand::Rng;
use rusqlite::Connection;

/// Weighted random draw over the least-picked active concepts.
/// Marks the concept picked for `date` and returns it.
pub fn draw(conn: &Connection, date: &str) -> db::Result<Option<Concept>> {
    let pool = db::roulette_pool(conn)?;
    if pool.is_empty() {
        return Ok(None);
    }
    let total: f64 = pool.iter().map(|c| c.weight.max(0.01)).sum();
    let mut roll = rand::thread_rng().gen_range(0.0..total);
    let mut chosen = pool.last().cloned().expect("non-empty pool");
    for c in &pool {
        roll -= c.weight.max(0.01);
        if roll <= 0.0 {
            chosen = c.clone();
            break;
        }
    }
    db::mark_concept_picked(conn, chosen.id, date)?;
    Ok(Some(chosen))
}
