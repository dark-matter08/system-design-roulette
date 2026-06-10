# The Teacher — continuous teaching agent

> Spec for evolving the generator from a **one-shot course author** into a
> **persistent teacher** that knows what you've mastered, plans what you learn
> next, varies how it teaches, and exists for the lifetime of the install.

## 1. Role

Today the agent is stateless: each `claude -p` call knows the topic and nothing
else. The Teacher inverts this. Every invocation receives a **learner dossier**
(compact, generated from the knowledge base) and acts in one standing role:

> *You are this student's long-term system design teacher. You have taught them
> for N days. You know what they have mastered, what they struggle with, and
> what comes next. Your job is to move them to staff-engineer-level judgment —
> not to cover topics, but to build durable understanding.*

The Teacher decides — within guardrails — what kind of day today is, how hard
to push, and what to revisit. The app remains the authority on *enforcement*
(lock, timer, streak); the Teacher becomes the authority on *pedagogy*.

**Default model: `opus`** (Opus 4.8 via the claude CLI). Configurable in
settings; automatic downgrade to `sonnet` on failure/timeout, then the existing
codex → bundled fallback chain. Grading and small calls stay on `sonnet`
(cheap, latency-sensitive, rubric-bound).

## 2. Knowledge base (the learner model)

A per-concept mastery ledger, derived from data we already collect (attempts,
carryover, scores) plus notes the Teacher writes after each grading pass.

### 2.1 Mastery lifecycle

Each concept moves through a state machine:

```
unseen → introduced → practicing → mastered → maintenance
                ↘ struggling ↗            ↘ decayed → (re-enters practicing)
```

| State        | Meaning                                                | Transition rule (initial tuning)                       |
| ------------ | ------------------------------------------------------ | ------------------------------------------------------ |
| unseen       | never taught                                            | —                                                       |
| introduced   | course read, first quiz not yet taken                   | session completed                                       |
| practicing   | quizzed at least once, not yet consistent               | first quiz on the topic                                 |
| struggling   | repeated misses                                         | same question failed 2×, or topic quiz score < 50%      |
| mastered     | consistent demonstrated understanding                   | ≥ 80% across 2 quiz encounters ≥ 7 days apart           |
| maintenance  | mastered; only spaced pop-quiz checks                   | automatic after mastered                                |
| decayed      | maintenance check failed                                | pop-quiz miss on a mastered topic                       |

### 2.2 Storage

New tables (SQLite, alongside existing ones):

```sql
mastery   (concept_id PK, state, score_ema, encounters, last_seen_date,
           next_review_date,      -- spaced repetition: 7d → 21d → 60d
           teacher_notes)         -- ≤ 280 chars, written by the agent
profile   (key PK, value)         -- durable free-form: 'weak_areas',
                                  -- 'learning_style_notes', 'days_taught', 'goals'
```

`teacher_notes` is the agent's memory of *you* on that topic ("confuses
linearizability with serializability; ASCII diagrams landed well"). The
grading call is extended to emit these notes; the next encounter with that
topic injects them back. The knowledge base is therefore **self-maintaining**:
no call ever depends on conversation history, only on the dossier built from
these tables.

### 2.3 The dossier

A ~1-page markdown block compiled by Rust and prepended to every Teacher call:

```
Day 47 of teaching. Streak 12d. Multi-topic days used: 9.
MASTERED (11): consistent-hashing, caching-strategies, …
STRUGGLING (2): consensus-raft (score 40%, notes: "confuses term vs index"),
                exactly-once-delivery (failed carryover ×2)
PRACTICING (6): …    DUE FOR REVIEW (3): cap-theorem (last seen 21d ago), …
RECENT COURSES: [last 5 titles + one-line summaries]
PROFILE: weak_areas=consensus, formal consistency models;
         responds well to concrete numbers and failure stories.
```

## 3. Curriculum: the roulette grows up

The flat least-picked-random wheel becomes a **progressive curriculum** while
keeping the roulette ritual (the wheel stays; what changes is what's on it).

- Concepts gain `tier` (0–3) and `prereqs` (slugs) in `concepts.json`.
  Tier 0 = fundamentals; tier 3 = synthesis topics (design Twitter/Uber-style
  composites, multi-region architectures).
- **The wheel only shows unlocked concepts**: tier N unlocks when ~70% of its
  prereq set is `practicing+`. Early days the wheel is small and fundamental;
  it visibly *grows* over the lifetime of the app — progress you can see.
- Weighting within unlocked: struggling-adjacent and due-for-review-adjacent
  topics get higher weight; the Teacher can also pin tomorrow's topic during
  pre-generation ("they just failed quorum questions twice — next lesson:
  quorums revisited via a different angle") with a stated reason, surfaced in
  the UI as `scheduler override — reason: …`.
- Pool exhausted ≠ done: tier-3 synthesis topics are generative (the Teacher
  invents composite design exercises from mastered components), so the
  curriculum never runs dry.

## 4. Session types: not every day is a lecture

At pre-generation time the Teacher (not the app) picks tomorrow's session type
from its dossier, within app-enforced bounds:

| Type            | Cadence (guardrail)            | Shape                                                                 |
| --------------- | ------------------------------ | --------------------------------------------------------------------- |
| **lesson**      | default                        | today's flow: quiz on yesterday → roulette → 30-min course             |
| **pop-quiz**    | ~1 in 5 days, never 2 in a row | no new topic. 10–14 questions sampled across `maintenance` + `due_for_review` + DLQ. Shorter (~15 min). Misses demote mastery. |
| **design-lab**  | unlocked at tier 2; ~1 in 7    | a project day: one realistic prompt ("design a multi-tenant rate limiter for …") combining ≥3 mastered concepts. User writes a design; Teacher grades against a rubric it generated with the exercise. |
| **remediation** | when ≥2 concepts `struggling`  | re-teaches a struggling concept **from a different angle** (the Teacher knows what didn't land from its notes), plus targeted drill. |

The session FSM gains these as variants of the existing steps — pop-quiz is
quiz+review without roulette/course; design-lab is course-reader with a free-
text editor and a grading pass. Enforcement, timer, and streak semantics are
identical across types.

## 5. Elastic days: "one more topic"

After Completion, if the user has time, an **`▲ extend session`** action:

- spins again (same wheel rules), generates live behind the progress screen,
  runs a full read cycle; quiz questions for the extra topic join tomorrow's
  quiz like any other.
- No limit, but extension is always *user-initiated* — the lock never demands
  more than the one scheduled session. Extensions feed the dossier
  (`multi-topic days`) so the Teacher learns the user's appetite and may
  suggest ("you usually extend on Sundays — today's wheel has two related
  topics queued").
- Counts toward pool coverage and uptime stats; shown on the dashboard as
  stacked cells.

## 6. Pipeline changes (mapping to today's code)

| Today                                   | Becomes                                                                 |
| --------------------------------------- | ----------------------------------------------------------------------- |
| `generate_course(title, category)`      | `teach(dossier, directive)` — directive = session type + topic + angle  |
| flat `course.txt` prompt                | `teacher_system.txt` (role, standing) + per-type templates (lesson / pop-quiz / design-lab / remediation), all receiving the dossier |
| quiz from course text only              | quiz prompt also gets mastery context → can mix in 1–2 spiral-review questions from older material |
| grade → verdicts                        | grade → verdicts **+ teacher_notes + mastery signals** (one call)        |
| roulette = least-picked random          | unlocked-tier weighted draw + optional Teacher override w/ reason        |
| `--model sonnet` hardcoded              | `config.model` (default `opus`), per-call-class override, auto-downgrade |
| pre-gen = tomorrow's course + quiz      | pre-gen = **plan tomorrow** (type + topic + course/quiz as needed) — one nightly "planning" invocation |

Everything stays headless one-shot CLI calls; continuity lives entirely in
SQLite + the dossier. No daemon-resident agent, no conversation state to lose.

## 7. UI vocabulary (topology language)

- Knowledge base node: `mastery-store` — dashboard gets a per-category mastery
  grid (unseen→maintenance as LED states) replacing/augmenting pool-coverage.
- Pop-quiz day ClusterBar: `sdr://audit · surprise compliance check`.
- Design-lab: `sdr://loadtest · scenario exercise`.
- Teacher override on the wheel: `scheduler override` MetaBadge with reason.
- Wheel growth: locked tiers rendered as greyed "provisioning…" slots.

## 8. Build order

1. **M1 — Knowledge base**: `mastery` + `profile` tables, transitions computed
   from existing attempts at grading time, dossier builder, dashboard mastery
   grid. (No prompt changes yet; pure substrate.)
2. **M2 — Teacher voice**: `teacher_system.txt` + dossier injected into
   course/quiz/grade prompts; grading emits `teacher_notes`; model → opus
   default with downgrade chain.
3. **M3 — Curriculum**: tiers + prereqs in seed data, unlock logic, weighted
   wheel, Teacher topic-override in nightly planning call.
4. **M4 — Session types**: pop-quiz day first (cheapest, pure reuse), then
   remediation, then design-lab (needs free-text editor + rubric grading).
5. **M5 — Elastic days**: extend-session loop + dossier appetite tracking.

Each milestone ships independently behind the existing fallback chain — if any
Teacher call fails, the app degrades to exactly today's behavior.
