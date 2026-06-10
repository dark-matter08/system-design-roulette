//! Simulates the multi-day data loop without the GUI: day 1 course -> day 2 quiz
//! with a deliberate failure -> carryover -> day 3 quiz includes the failed question.

use system_design_roulette_lib::db::{self, Attempt};

const SEED: &str = include_str!("../seed/concepts.json");

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn test_db() -> rusqlite::Connection {
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("sdr-test-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let conn = db::open(&dir.join("test.db")).unwrap();
    db::seed_concepts(&conn, SEED).unwrap();
    conn
}

#[test]
fn seed_pool_loads() {
    let conn = test_db();
    let pool = db::roulette_pool(&conn).unwrap();
    assert!(pool.len() >= 70, "expected >= 70 seeded concepts, got {}", pool.len());
}

#[test]
fn roulette_exhausts_unlocked_pool_before_repeats() {
    let conn = test_db();
    // Fresh install: only no-prereq (tier 0) concepts are on the wheel.
    let (unlocked, locked) = system_design_roulette_lib::roulette::pool_status(&conn).unwrap();
    assert!(unlocked.len() >= 10, "day-1 wheel too small: {}", unlocked.len());
    assert!(!locked.is_empty(), "everything unlocked on day 1 defeats the curriculum");
    assert!(unlocked.iter().all(|c| c.tier == 0), "fresh db should unlock only tier 0");

    let mut seen = std::collections::HashSet::new();
    for i in 0..unlocked.len() {
        let c = system_design_roulette_lib::roulette::draw(&conn, &format!("2026-01-{:02}", (i % 28) + 1))
            .unwrap()
            .expect("pool non-empty");
        assert!(seen.insert(c.id), "concept {} repeated before unlocked pool exhausted", c.slug);
        assert_eq!(c.tier, 0, "locked concept {} drawn", c.slug);
    }
    // Unlocked pool exhausted: next draw must still work (second lap).
    let again = system_design_roulette_lib::roulette::draw(&conn, "2026-02-01").unwrap();
    assert!(again.is_some());
}

#[test]
fn concepts_unlock_at_seventy_percent_prereqs() {
    use system_design_roulette_lib::{mastery, roulette};
    let conn = test_db();
    // quorums requires cap-theorem + consistency-models.
    let id_of = |slug: &str| -> i64 {
        conn.query_row("SELECT id FROM concepts WHERE slug = ?1", [slug], |r| r.get(0)).unwrap()
    };
    let quorums = id_of("quorums");
    let locked_ids = |conn: &rusqlite::Connection| -> std::collections::HashSet<i64> {
        roulette::pool_status(conn).unwrap().1.into_iter().map(|c| c.id).collect()
    };
    assert!(locked_ids(&conn).contains(&quorums), "quorums should start locked");

    // One of two prereqs practiced: 50% < 70%, still locked.
    mastery::record_quiz_outcome(&conn, id_of("cap-theorem"), "2026-06-02", 1.0).unwrap();
    assert!(locked_ids(&conn).contains(&quorums), "50% prereqs must not unlock");

    // Both practiced: unlocked.
    mastery::record_quiz_outcome(&conn, id_of("consistency-models"), "2026-06-03", 1.0).unwrap();
    assert!(!locked_ids(&conn).contains(&quorums), "100% prereqs must unlock quorums");
}

#[test]
fn full_three_day_carryover_loop() {
    let conn = test_db();
    let (d1, d2, d3) = ("2026-06-01", "2026-06-02", "2026-06-03");

    // Day 1: a course is generated and stored.
    let concept = system_design_roulette_lib::roulette::draw(&conn, d1).unwrap().unwrap();
    let course_id = db::insert_course(&conn, d1, concept.id, "# Course", "[]", "fallback").unwrap();

    // Pregen: quiz questions for day 2 from day 1's course.
    let q1 = db::insert_question(&conn, course_id, "Q1 mcq", "mcq", Some(r#"["a","b","c","d"]"#), "a", "because a").unwrap();
    let q2 = db::insert_question(&conn, course_id, "Q2 mcq", "mcq", Some(r#"["a","b","c","d"]"#), "b", "because b").unwrap();
    let q3 = db::insert_question(&conn, course_id, "Q3 free", "free", None, "model answer", "explained").unwrap();

    // Day 2: quiz pulls exactly those three questions.
    let quiz = db::quiz_for_date(&conn, d2, d1).unwrap();
    assert_eq!(quiz.len(), 3);

    // User passes q1, fails q2, passes q3.
    for (qid, correct) in [(q1, true), (q2, false), (q3, true)] {
        db::record_attempt(&conn, &Attempt {
            question_id: qid,
            session_date: d2.into(),
            user_answer: "x".into(),
            correct,
            grader_feedback: String::new(),
        }).unwrap();
        if !correct {
            db::push_carryover(&conn, qid, d2, d3).unwrap();
        }
    }
    assert_eq!(db::carryover_count(&conn, d3).unwrap(), 1);

    // Day 2 course + day 3 fresh questions.
    let concept2 = system_design_roulette_lib::roulette::draw(&conn, d2).unwrap().unwrap();
    let course2 = db::insert_course(&conn, d2, concept2.id, "# Course 2", "[]", "fallback").unwrap();
    let q4 = db::insert_question(&conn, course2, "Q4 mcq", "mcq", Some(r#"["a","b","c","d"]"#), "c", "because c").unwrap();

    // Day 3 quiz = carryover q2 first, then fresh q4. Attempted q1/q3 must NOT reappear.
    let quiz3 = db::quiz_for_date(&conn, d3, d2).unwrap();
    let ids: Vec<i64> = quiz3.iter().map(|q| q.id).collect();
    assert_eq!(ids, vec![q2, q4], "day-3 quiz should be carryover then fresh");
    assert_eq!(quiz3[0].origin, "carryover");

    // User finally passes q2 -> carryover cleared.
    db::record_attempt(&conn, &Attempt {
        question_id: q2,
        session_date: d3.into(),
        user_answer: "b".into(),
        correct: true,
        grader_feedback: String::new(),
    }).unwrap();
    db::clear_carryover(&conn, q2).unwrap();
    assert_eq!(db::carryover_count(&conn, "2026-06-04").unwrap(), 0);
}

#[test]
fn streak_counts_consecutive_completed_days() {
    let conn = test_db();
    for date in ["2026-06-07", "2026-06-08", "2026-06-09"] {
        db::upsert_session(&conn, &db::Session {
            date: date.into(),
            concept_id: None,
            status: "completed".into(),
            current_step: "done".into(),
            quiz_score: Some(1.0),
            started_at: None,
            completed_at: None,
            reading_seconds: 1800,
        }).unwrap();
    }
    // Today completed: streak 3. (2026-06-09 = "today")
    assert_eq!(db::streak(&conn, "2026-06-09").unwrap(), 3);
    // Gap day breaks it.
    assert_eq!(db::streak(&conn, "2026-06-11").unwrap(), 0);
}

#[test]
fn fallback_courses_parse_and_pick() {
    let fb = system_design_roulette_lib::generator::pick_fallback("Rate limiting algorithms");
    assert_eq!(fb.slug, "rate-limiting");
    assert!(fb.questions.len() >= 3);
    let any = system_design_roulette_lib::generator::pick_fallback("Some unknown topic");
    assert!(!any.markdown.is_empty());
}

#[test]
fn json_payload_parser_handles_fenced_and_prose() {
    #[derive(serde::Deserialize)]
    struct T { x: i64 }
    let fenced = "Here you go:\n```json\n{\"x\": 5}\n```\nthanks";
    assert_eq!(system_design_roulette_lib::generator::parse_json_payload::<T>(fenced).unwrap().x, 5);
    let bare = "prefix {\"x\": 7} suffix";
    assert_eq!(system_design_roulette_lib::generator::parse_json_payload::<T>(bare).unwrap().x, 7);
}

#[test]
fn json_parser_survives_embedded_code_fences_in_markdown() {
    // Real failure mode: course markdown contains ``` blocks inside the JSON string,
    // so the first closing fence is NOT the end of the payload.
    #[derive(serde::Deserialize)]
    struct Course { markdown: String }
    let raw = "```json\n{\"markdown\": \"intro\\n```\\ncode here\\n```\\noutro\"}\n```";
    let c = system_design_roulette_lib::generator::parse_json_payload::<Course>(raw).unwrap();
    assert!(c.markdown.contains("code here"));
}

#[test]
fn mastery_lifecycle_transitions() {
    use system_design_roulette_lib::mastery;
    let conn = test_db();
    let concept = db::all_concepts(&conn).unwrap()[0].clone();
    let id = concept.id;

    // Course read: unseen -> introduced.
    mastery::record_course_read(&conn, id, "2026-06-01").unwrap();
    assert_eq!(mastery::get(&conn, id).unwrap().state, "introduced");

    // First quiz, good score: introduced -> practicing (mastery needs 2 spaced encounters).
    let m = mastery::record_quiz_outcome(&conn, id, "2026-06-02", 1.0).unwrap();
    assert_eq!(m.state, "practicing");

    // Second strong encounter only 1 day later: gap < 7d, still practicing.
    let m = mastery::record_quiz_outcome(&conn, id, "2026-06-03", 1.0).unwrap();
    assert_eq!(m.state, "practicing");

    // Third strong encounter 8 days later: mastered, review scheduled +7d.
    let m = mastery::record_quiz_outcome(&conn, id, "2026-06-11", 1.0).unwrap();
    assert_eq!(m.state, "mastered");
    assert_eq!(m.next_review_date.as_deref(), Some("2026-06-18"));

    // Passed maintenance check: interval advances 7 -> 21.
    let m = mastery::record_quiz_outcome(&conn, id, "2026-06-18", 1.0).unwrap();
    assert_eq!(m.state, "maintenance");
    assert_eq!(m.review_interval_days, 21);

    // Failed maintenance check: decayed.
    let m = mastery::record_quiz_outcome(&conn, id, "2026-07-09", 0.0).unwrap();
    assert_eq!(m.state, "decayed");

    // Bad score from a non-mastered state: struggling.
    let other = db::all_concepts(&conn).unwrap()[1].clone();
    mastery::record_course_read(&conn, other.id, "2026-06-01").unwrap();
    let m = mastery::record_quiz_outcome(&conn, other.id, "2026-06-02", 0.2).unwrap();
    assert_eq!(m.state, "struggling");
}

#[test]
fn dossier_reflects_ledger_and_notes() {
    use system_design_roulette_lib::mastery;
    let conn = test_db();
    let concepts = db::all_concepts(&conn).unwrap();
    let (a, b) = (concepts[0].clone(), concepts[1].clone());

    mastery::record_course_read(&conn, a.id, "2026-06-01").unwrap();
    mastery::record_quiz_outcome(&conn, a.id, "2026-06-02", 0.3).unwrap();
    mastery::set_teacher_note(&conn, a.id, "confuses term with index").unwrap();
    mastery::record_course_read(&conn, b.id, "2026-06-02").unwrap();
    db::insert_course(&conn, "2026-06-02", b.id, "# C", "[]", "fallback").unwrap();

    let d = mastery::build_dossier(&conn, "2026-06-03").unwrap();
    assert!(d.contains("STRUGGLING (1)"), "dossier missing struggling section: {d}");
    assert!(d.contains("confuses term with index"), "dossier missing teacher note: {d}");
    assert!(d.contains(&format!("{}", b.slug)) || d.contains("INTRODUCED"), "dossier missing introduced: {d}");
    assert!(d.contains("RECENT COURSES"), "dossier missing recent courses: {d}");
    // Empty ledger on a fresh db produces a dossier too (day 1) - never errors.
    let fresh = test_db();
    let d0 = mastery::build_dossier(&fresh, "2026-06-01").unwrap();
    assert!(d0.contains("Day 1 of teaching"));
}

#[test]
fn teacher_preamble_wraps_dossier() {
    // with_teacher is private; verify through the public prompt constant contract instead:
    // TEACHER_PROMPT must carry the dossier placeholder exactly once.
    let n = system_design_roulette_lib::generator::TEACHER_PROMPT.matches("{{DOSSIER}}").count();
    assert_eq!(n, 1);
}
