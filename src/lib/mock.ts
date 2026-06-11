/**
 * Demo-mode API: used automatically when the frontend runs outside Tauri
 * (plain `vite dev` in a browser). Lets you develop and screenshot every
 * screen without the Rust backend. Same shapes as ipc.ts.
 */
import type {
  AppStateView,
  ArchivedCourse,
  CourseView,
  DashboardView,
  QuizQuestionView,
  ReviewData,
  RouletteView,
  SessionView,
} from './ipc';

type Handler = (payload: unknown) => void;
const listeners = new Map<string, Handler[]>();

export function mockEmit(name: string, payload: unknown) {
  for (const h of listeners.get(name) ?? []) h(payload);
}

export function mockListen(name: string, handler: Handler): () => void {
  const arr = listeners.get(name) ?? [];
  arr.push(handler);
  listeners.set(name, arr);
  return () => listeners.set(name, (listeners.get(name) ?? []).filter((h) => h !== handler));
}

const COURSE_MD = `## Why this matters

Any system that spreads keys across N nodes — caches, distributed databases, message partitions — must answer: what happens when N changes? Naive \`hash(key) mod N\` remaps nearly every key when a node joins or leaves, which at scale means a self-inflicted cache wipe or a rebalancing storm. Consistent hashing is the standard fix and a building block you will reuse in countless designs.

## Core mechanics

### The ring

Map the hash space (say 0 to 2^64-1) onto a conceptual ring. Hash each node onto the ring; hash each key onto the ring; a key belongs to the first node clockwise from it. When a node joins, it takes keys only from its clockwise successor; when it leaves, its keys go only to its successor. Expected fraction of keys that move: **K/N — the theoretical minimum**.

### The variance problem

With one point per node, the gaps between nodes are wildly uneven — some nodes own several times their fair share. Worse, a departing node dumps its entire range onto one successor.

### Virtual nodes

Hash each physical node onto the ring at many points (typically 100-1000 "vnodes"). Load variance shrinks roughly with the square root of the vnode count; a leaving node's ranges scatter across many successors; heterogeneous hardware gets proportional vnode counts.

\`\`\`
ring positions:  n1@073  n2@194  n1@277  n3@402  n2@558  n3@691 ...
key "user:42" -> hash 230 -> first clockwise vnode = n1@277 -> node n1
\`\`\`

## Trade-offs and failure modes

- Vnode count is a dial: more vnodes = smoother distribution but bigger routing tables.
- Consistent hashing balances *key counts*, not *load*: one celebrity key still overwhelms its owner.
- Ring membership must itself be consistent: gossip lag means brief routing divergence.

## Interview framing

Lead with the mod-N failure ("adding one node remaps (N-1)/N of all keys"), then the ring, then immediately volunteer vnodes. Close with the hot-key caveat: consistent hashing solves *placement*, not *load skew*.`;

const RESOURCES = [
  {
    title: 'Consistent Hashing and Random Trees (original paper)',
    url: 'https://www.cs.princeton.edu/courses/archive/fall09/cos518/papers/chash.pdf',
    type: 'paper',
    why: 'The 1997 paper that introduced the ring.',
  },
  {
    title: 'Amazon Dynamo paper',
    url: 'https://www.allthingsdistributed.com/files/amazon-dynamo-sosp2007.pdf',
    type: 'paper',
    why: 'Popularized vnodes + replication on the ring.',
  },
  {
    title: 'Cassandra: virtual nodes',
    url: 'https://cassandra.apache.org/doc/latest/cassandra/architecture/dynamo.html',
    type: 'docs',
    why: 'Production vnode trade-offs.',
  },
];

const params =
  typeof location !== 'undefined' ? new URLSearchParams(location.search) : new URLSearchParams();
const jump = params.get('step');

const state = {
  step: (jump ?? 'quiz') as SessionView['step'],
  status: (jump === 'done'
    ? 'completed'
    : jump
      ? 'in_progress'
      : 'pending') as SessionView['status'],
  score: jump ? 2 / 3 : (null as number | null),
  remaining: 30,
  timerId: 0 as ReturnType<typeof setInterval> | 0,
};

function session(): SessionView {
  return {
    date: new Date().toISOString().slice(0, 10),
    status: state.status,
    step: state.step,
    quiz_score: state.score,
    streak: 17,
    // &unlocked previews voluntary (early-start/extension) sessions.
    locked: state.status === 'in_progress' && !params.has('unlocked'),
    // ?type=pop_quiz previews an audit day in the browser demo.
    session_type: params.get('type') === 'pop_quiz' ? 'pop_quiz' : 'lesson',
    plan_reason:
      params.get('type') === 'pop_quiz' ? 'review debt: 4 topics due — surprise audit' : '',
  };
}

let setupCompleted = false;
let mockPaused = params.has('paused');
let mockKioskLevel = (params.get('kiosk') ?? 'hard') as AppStateView['kiosk_level'];

function appState(): AppStateView {
  const inSetup = location.search.includes('setup') && !setupCompleted;
  return {
    onboarded: !inSetup,
    session: session(),
    // After completing setup the session is not owed yet (scheduled time is
    // in the future) — mirrors the real backend so routing bugs reproduce.
    owed: !setupCompleted && !params.has('unlocked'),
    schedule_hour: 19,
    schedule_minute: 0,
    debug_day: true,
    enforcement_disarmed: params.has('disarmed'),
    schedule_paused: mockPaused,
    kiosk_level: mockKioskLevel,
  };
}

const QUESTIONS: QuizQuestionView[] = [
  {
    id: 1,
    prompt:
      'A node leaves a consistent-hash ring that has NO virtual nodes. What happens to its keys?',
    kind: 'mcq',
    choices: [
      'They are redistributed evenly across all remaining nodes',
      'They all move to the single clockwise successor, doubling its load',
      'They are lost until the node returns',
      'Half go to the predecessor and half to the successor',
    ],
    origin: 'carryover',
    answered: false,
    draft: null,
  },
  {
    id: 2,
    prompt: 'Why does consistent hashing NOT solve the hot partition (celebrity key) problem?',
    kind: 'mcq',
    choices: [
      'Because it balances where keys live, not how much traffic each key receives',
      'Because hot keys hash to multiple nodes simultaneously',
      'Because virtual nodes concentrate hot keys',
      'It does solve it, by spreading requests across replicas',
    ],
    origin: 'fresh',
    answered: false,
    draft: null,
  },
  {
    id: 3,
    prompt:
      'Your cluster replicates each key to 3 nodes by walking clockwise from the key and taking the next 3 vnodes. What bug does this have and how do you fix it?',
    kind: 'free',
    choices: null,
    origin: 'fresh',
    answered: false,
    draft: null,
  },
];

const REVIEW: ReviewData = {
  score: 2 / 3,
  self_assess: false,
  items: [
    {
      question_id: 1,
      prompt: QUESTIONS[0].prompt,
      kind: 'mcq',
      user_answer: 'They all move to the single clockwise successor, doubling its load',
      correct: true,
      feedback: '',
      correct_answer: 'They all move to the single clockwise successor, doubling its load',
      explanation:
        'Without vnodes, one node owns one contiguous arc, and the whole arc transfers to its successor — the rebalancing hotspot that virtual nodes exist to fix.',
      returns_tomorrow: false,
    },
    {
      question_id: 3,
      prompt: QUESTIONS[2].prompt,
      kind: 'free',
      user_answer: 'The replicas might be unbalanced so you should shuffle the ring.',
      correct: false,
      feedback:
        'You missed the core issue: consecutive vnodes can belong to the same physical machine, silently reducing fault tolerance.',
      correct_answer:
        'Consecutive vnodes can belong to the same physical machine, so the three replicas may land on fewer than three physical nodes. Fix: skip vnodes whose physical node is already in the replica set.',
      explanation:
        'The distinct-physical-node constraint is the classic vnode replication gotcha from the Dynamo lineage.',
      returns_tomorrow: true,
    },
  ],
};

export const mockApi = {
  getAppState: async () => appState(),
  checkAgent: async () => true,
  completeSetup: async () => {
    setupCompleted = true;
    return appState();
  },
  updateSchedule: async () => {},
  setKioskLevel: async (level: string) => {
    mockKioskLevel = level as AppStateView['kiosk_level'];
  },
  pauseSchedule: async () => {
    mockPaused = true;
  },
  resumeSchedule: async () => {
    mockPaused = false;
  },
  startSession: async () => {
    state.status = 'in_progress';
    state.step = 'quiz';
    return session();
  },
  getQuiz: async () => QUESTIONS,
  submitAnswer: async (id: number) => {
    const q = QUESTIONS.find((q) => q.id === id);
    if (q) q.answered = true;
  },
  finishQuiz: async () => {
    state.step = 'review';
    state.score = REVIEW.score;
    return REVIEW;
  },
  getReview: async () => REVIEW,
  finishReview: async () => {
    state.step = 'roulette';
    mockEmit('session:state', session());
    return session();
  },
  getRoulette: async (): Promise<RouletteView> => ({
    pool: [
      'Backpressure & load shedding',
      'Kafka internals',
      'CAP in practice',
      'Consistent hashing and virtual nodes',
      'LSM trees vs B-trees',
      'Design a rate limiter',
      'Circuit breakers',
      'Quorum reads/writes',
      'Event sourcing & CQRS',
      'CDNs and edge caching',
      'Design a news feed',
      'SLOs and error budgets',
    ],
    chosen_index: 3,
    concept_title: 'Consistent hashing and virtual nodes',
    concept_category: 'storage',
    pool_unlocked: 31,
    pool_total: 72,
  }),
  ensureCourse: async (): Promise<CourseView> => {
    // Demo the live agent log the way a real generation streams it.
    const feed = [
      'spawn: agent · model opus',
      'tool: WebSearch consistent hashing virtual nodes production',
      'tool: WebFetch https://blog.discord.com/scaling-elixir',
      'tool: WebSearch jump hash vs ring hash benchmark',
      'draft: 2,400 chars written',
      'draft: 9,100 chars written',
      'done: agent returned 21,348 chars',
    ];
    for (const line of feed) {
      mockEmit('gen:log', line);
      await new Promise((r) => setTimeout(r, 350));
    }
    return {
      title: 'Consistent hashing and virtual nodes',
      markdown: COURSE_MD,
      resources: RESOURCES,
      source: 'claude',
      remaining_seconds: state.remaining,
      total_seconds: 30 * 60,
    };
  },
  startCourse: async () => {
    state.step = 'course';
    state.remaining = 27 * 60 + 14;
    if (!state.timerId) {
      state.timerId = setInterval(() => {
        state.remaining = Math.max(0, state.remaining - 1);
        mockEmit('timer:tick', state.remaining);
      }, 1000);
    }
    return session();
  },
  finishCourse: async () => {
    state.step = 'done';
    state.status = 'completed';
    mockEmit('session:state', session());
    return session();
  },
  escapeSession: async () => true,
  getEscapePhrase: async () =>
    'I am choosing to skip my training today and I accept the broken streak',
  getDashboard: async (): Promise<DashboardView> => {
    const topics = [
      'Kafka internals: partitions, offsets, ISR',
      'Design a distributed rate limiter',
      'CAP theorem in practice',
      'LSM trees vs B-trees',
      'Backpressure and load shedding',
      'Caching strategies: write-through, aside, back',
      'Consensus: Raft and leader election',
      'Design a news feed (fan-out strategies)',
      'Observability: metrics, logs, traces',
      'Database sharding and partition strategies',
    ];
    const history = [];
    const today = new Date();
    for (let i = 1; i <= 36; i++) {
      const d = new Date(today);
      d.setDate(d.getDate() - i);
      const skipped = i === 9 || i === 23;
      history.push({
        date: d.toISOString().slice(0, 10),
        status: skipped ? 'skipped' : 'completed',
        quiz_score: skipped ? null : Math.round((0.5 + ((i * 37) % 50) / 100) * 100) / 100,
        concept_title: topics[i % topics.length],
      });
    }
    const states = [
      'mastered', 'maintenance', 'practicing', 'practicing', 'struggling',
      'introduced', 'decayed', 'unseen', 'unseen', 'unseen',
    ] as const;
    const categories = ['fundamentals', 'storage', 'caching', 'messaging', 'resilience', 'architecture'];
    const mastery = Array.from({ length: 72 }, (_, i) => ({
      concept_id: i + 1,
      slug: `concept-${i + 1}`,
      title: topics[i % topics.length],
      category: categories[Math.floor(i / 12)],
      state: states[(i * 7) % states.length],
      score_ema: ((i * 13) % 100) / 100,
    }));
    return {
      history,
      streak: 17,
      carryover_due: 2,
      concepts_total: 72,
      concepts_covered: 36,
      mastery,
    };
  },
  getPastCourse: async (): Promise<ArchivedCourse | null> => ({
    session_date: new Date().toISOString().slice(0, 10),
    title: 'Consistent hashing and virtual nodes',
    markdown: COURSE_MD,
    resources: RESOURCES,
  }),
  openResources: async () => RESOURCES.length,
  markFrontendReady: async () => {},
  ensureAudio: async () => ({
    engine: 'speech' as const,
    lines: [
      { speaker: 'teacher' as const, text: "Alright — today we're talking about consistent hashing. Before I explain anything: you've got ten cache servers and a million keys. How do you decide which key lives where?" },
      { speaker: 'student' as const, text: 'Easy, hash the key and take it modulo ten?' },
      { speaker: 'teacher' as const, text: "Perfect answer — and completely wrong the moment anything changes. Add an eleventh server and almost every key now maps somewhere new. You just wiped your own cache." },
      { speaker: 'student' as const, text: 'Wait, all of them? Not just a tenth?' },
      { speaker: 'teacher' as const, text: 'Nearly all. Modulo arithmetic reshuffles everything when N changes. Consistent hashing fixes exactly this: put the servers on a ring, hash each key onto the ring, and a key belongs to the first server clockwise from it.' },
      { speaker: 'student' as const, text: 'So when a server joins, it only steals keys from its clockwise neighbor?' },
      { speaker: 'teacher' as const, text: "Now you've got it — about K over N keys move, which is the theoretical minimum." },
    ],
  }),
  getAudioEnabled: async () => false,
  setAudioEnabled: async () => {},
  getExitQuiz: async () => [
    {
      id: 1,
      prompt: 'A ring with 3 physical nodes and no virtual nodes loses one node. Roughly how much of the keyspace remaps?',
      choices: ['~1/3, all of it onto one neighbor', '~1/3, spread evenly', '~2/3 onto both neighbors', 'All keys remap'],
    },
    {
      id: 2,
      prompt: 'Virtual nodes primarily exist to…',
      choices: [
        'smooth load distribution and failure spread',
        'reduce memory usage of the ring',
        'make lookups O(1)',
        'avoid hash collisions',
      ],
    },
    {
      id: 3,
      prompt: 'Compared to mod-N hashing, consistent hashing wins because…',
      choices: [
        'adding a node remaps only ~K/N keys',
        'it never remaps any keys',
        'it requires no coordination at all',
        'it makes hot keys impossible',
      ],
    },
  ],
  submitExitQuiz: async (answers: Record<number, string>) => {
    const key: Record<number, string> = {
      1: '~1/3, all of it onto one neighbor',
      2: 'smooth load distribution and failure spread',
      3: 'adding a node remaps only ~K/N keys',
    };
    const correct = Object.entries(key)
      .filter(([id, a]) => answers[Number(id)] === a)
      .map(([id]) => Number(id));
    const passed = correct.length === 3;
    if (passed) {
      state.remaining = 0;
      mockEmit('timer:tick', 0);
      mockEmit('timer:done', true);
    }
    return { passed, correct, cooldown_seconds: passed ? 0 : 60 };
  },
  extendSession: async (): Promise<SessionView> => {
    state.status = 'in_progress';
    state.step = 'roulette';
    return session();
  },
};
