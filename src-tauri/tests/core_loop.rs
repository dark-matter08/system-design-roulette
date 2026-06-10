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
fn roulette_exhausts_pool_before_repeats() {
    let conn = test_db();
    let total = db::all_concepts(&conn).unwrap().len();
    let mut seen = std::collections::HashSet::new();
    for i in 0..total {
        let c = system_design_roulette_lib::roulette::draw(&conn, &format!("2026-01-{:02}", (i % 28) + 1))
            .unwrap()
            .expect("pool non-empty");
        assert!(seen.insert(c.id), "concept {} repeated before pool exhausted", c.slug);
    }
    // Pool exhausted: next draw must still work (second lap).
    let again = system_design_roulette_lib::roulette::draw(&conn, "2026-02-01").unwrap();
    assert!(again.is_some());
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
