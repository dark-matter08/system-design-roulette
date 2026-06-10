# Design system — "the UI is the diagram"

The app teaches system design, so every screen is drawn as a system topology.
No metaphor is decorative: each component maps honestly to what it does, and the
vocabulary doubles as daily reinforcement of the concepts being learned.

## Vocabulary map

| App concept | UI vocabulary | Component |
|---|---|---|
| Setup wizard | bootstrap your training cluster | NodeCard topology + pipes |
| Daily schedule | `cron-scheduler` node, FIRE_AT | NodeCard |
| Agent CLI check | `agent-backend` healthcheck, 200 OK, p50 | StatusLED + MetaBadge |
| Kiosk lock | `enforcement-service`, kiosk · level 1000 | NodeCard (violet accent) |
| Escape phrase | BREAK GLASS circuit breaker | BreakGlass (dashed red) |
| Submit/commit actions | `▲ DEPLOY`, `SEND RESPONSE`, `ACK` | .cta.mono-cta |
| Streak | `uptime 17d` | MetaBadge teal |
| Carryover questions | `DLQ: n` (dead letter queue) | MetaBadge amber |
| Course timer | `TTL 27:14` | MetaBadge violet / TimerBar |
| Quiz score | `error budget` | MetaBadge |
| Quiz question | incoming request `POST /quiz/q2` | NodeCard request inspector |
| Background generation | `shard: generating` | StatusLED pending |
| Dashboard | cluster overview + request log | stat nodes + log table |

## Tokens (theme.css)

Two themes: **noir** (every screen) and **scholar** (course reader body only).
Topology chrome on top of noir:

- Blueprint grid: `radial-gradient(circle, var(--grid-dot) 1px, transparent 1px)` 22px.
- Node: bg `#1b1721`, border `#3a3344`, header divider `#2b2434`.
- LEDs: ok `#9fe1cb`, warn `#fac775`, err `#f09595`, pending pulses.
- Mono scale: 10px META_LABELS, 11px labels/badges, 13px values, 22px display digits.
- Serif (Fraunces) reserved for one editorial headline per screen.
- Pipes: 1.5px dashed SVG paths in node-accent colors; arrows optional.

## Components (src/lib/components/)

- `ClusterBar.svelte` — top status strip: `sdr://<route> · cluster: <host>` left, status right.
- `NodeCard.svelte` — header (icon, name, badge) + body snippet; `accent` prop colors the border.
- `StatusLED.svelte` — `tone: ok|warn|err|idle|pending` (pending pulses).
- `MetaBadge.svelte` — mono pill, `tone: teal|amber|violet|red|muted`.
- `BreakGlass.svelte` — circuit-breaker escape hatch (replaces EscapeHatch visuals).
- `.cta.mono-cta` — deploy-style button (mono, uppercase, ▲ prefix where apt).

## Rules

- Sentence case prose; SCREAMING_SNAKE only for mono meta-labels (FIRE_AT, TTL).
- Every screen gets: blueprint grid + ClusterBar + one serif headline max.
- Metaphors must be honest — never label something with a concept it doesn't implement.
- Course reader body stays scholar (paper, 68ch, 1.8 line-height) — reading comfort wins;
  only its header bar speaks topology.
