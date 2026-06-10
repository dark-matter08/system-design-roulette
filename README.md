# System Design Roulette

A macOS app that hard-locks your laptop once a day and forces a 30-minute system-design
learning session. AI does the teaching; you do the learning; the lock does the discipline.

## The daily loop

1. **Quiz** — questions generated from yesterday's course. Questions you fail return
   tomorrow (and the day after, until you pass them).
2. **Answer review** — graded results with explanations; failed questions are flagged
   "returns tomorrow".
3. **Roulette** — a wheel lands on a system-design concept from a ~70-topic pool.
   No repeats until the whole pool is exhausted. (The topic is pre-drawn at generation
   time; the wheel is honest theater.)
4. **Course** — a ~30-minute course on the concept with real web resources, read under
   an enforced timer. External links are queued to a post-session reading list.
5. **Completion** — score, streak, and tomorrow's content pre-generates in the background
   so the next session starts instantly.

## Content generation

Shells out to your local agent CLI headlessly — no API keys:

- `claude -p` (Claude Code non-interactive, WebSearch-enabled) — primary
- `codex exec` — fallback
- bundled static courses — last resort (the session never blocks on generation)

Free-text quiz answers are graded by the agent; MCQs grade locally. If the grader is
unreachable, answers fall back to self-assessment against the model answer.

## Enforcement

At your configured time (launchd `StartCalendarInterval` + catch-up on wake/login),
the window goes full-screen at the screensaver window level, hides the Dock and menu
bar, disables Cmd+Tab process switching and Force Quit while frontmost, and re-grabs
focus every 300 ms. Quit/close are blocked while locked.

Escape hatch: a dim "emergency exit" link reveals a long non-copyable phrase you must
type exactly. Skipping marks the day failed and resets your streak. Three failed
attempts locks the input for 60 s.

**Dev back door:** `touch ~/sdr-unlock` releases the kiosk within a second (remove the
file afterwards). Keep this until you trust the lock.

## Development

```bash
npm install
npm run tauri dev                      # dev mode (normal window)
npm run tauri build -- --debug --bundles app   # debuggable .app bundle
cd src-tauri && cargo test             # core-loop integration tests
```

### Test/debug flags

- `--debug-day` — session always owed, 30-second course timer, kiosk + launchd disabled
- `--triggered` — how launchd launches it (kiosk auto-engages if a session is owed)
- `SDR_DATE=YYYY-MM-DD` — override "today" to simulate multi-day loops
- `SDR_CLAUDE_BIN=/path` — override the claude binary (point at `/nonexistent` to test fallbacks)
- `SDR_CODEX_BIN=none|/path` — disable or override codex

Data lives in `~/Library/Application Support/com.darkmatter.system-design-roulette/`
(SQLite DB + mirrored course markdown under `courses/`).

The launchd agent is installed by the setup wizard at
`~/Library/LaunchAgents/com.darkmatter.system-design-roulette.plist`. Remove with:

```bash
launchctl bootout gui/$(id -u)/com.darkmatter.system-design-roulette
rm ~/Library/LaunchAgents/com.darkmatter.system-design-roulette.plist
```
