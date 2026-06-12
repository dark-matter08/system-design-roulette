import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface SessionView {
  date: string;
  status: 'pending' | 'in_progress' | 'completed' | 'skipped';
  step: 'quiz' | 'review' | 'roulette' | 'course' | 'done';
  quiz_score: number | null;
  streak: number;
  locked: boolean;
  session_type: 'lesson' | 'pop_quiz';
  plan_reason: string;
}

export interface AppStateView {
  onboarded: boolean;
  session: SessionView;
  owed: boolean;
  schedule_hour: number;
  schedule_minute: number;
  debug_day: boolean;
  enforcement_disarmed: boolean;
  schedule_paused: boolean;
  kiosk_level: 'advisory' | 'firm' | 'hard';
  model: 'opus' | 'sonnet' | 'haiku';
  agent: 'claude' | 'codex' | 'custom';
  custom_agent_bin: string;
}

export interface QuizQuestionView {
  id: number;
  prompt: string;
  kind: 'mcq' | 'free';
  choices: string[] | null;
  origin: 'fresh' | 'carryover';
  answered: boolean;
  draft: string | null;
}

export interface ReviewItem {
  question_id: number;
  prompt: string;
  kind: string;
  user_answer: string;
  correct: boolean | null;
  feedback: string;
  correct_answer: string;
  explanation: string;
  returns_tomorrow: boolean;
}

export interface ReviewData {
  items: ReviewItem[];
  score: number;
  self_assess: boolean;
}

export interface RouletteView {
  pool: string[];
  chosen_index: number;
  concept_title: string;
  concept_category: string;
  pool_unlocked: number;
  pool_total: number;
}

export interface Resource {
  title: string;
  url: string;
  type?: string;
  why?: string;
}

export interface CourseView {
  title: string;
  markdown: string;
  resources: Resource[];
  source: string;
  remaining_seconds: number;
  total_seconds: number;
}

export interface HistoryEntry {
  date: string;
  status: string;
  quiz_score: number | null;
  concept_title: string | null;
}

export interface MasteryEntry {
  concept_id: number;
  slug: string;
  title: string;
  category: string;
  state:
    | 'unseen'
    | 'introduced'
    | 'practicing'
    | 'struggling'
    | 'mastered'
    | 'maintenance'
    | 'decayed';
  score_ema: number;
}

export interface DashboardView {
  history: HistoryEntry[];
  streak: number;
  carryover_due: number;
  concepts_total: number;
  concepts_covered: number;
  mastery: MasteryEntry[];
}

export interface AudioLine {
  speaker: 'teacher' | 'student';
  text: string;
  file?: string | null;
}

export interface AudioView {
  lines: AudioLine[];
  engine: 'speech' | 'vibevoice';
}

export interface ExitQuizQuestion {
  id: number;
  prompt: string;
  choices: string[];
}

export interface ExitQuizResult {
  passed: boolean;
  correct: number[];
  cooldown_seconds: number;
}

export interface ArchivedCourse {
  session_date: string;
  title: string;
  markdown: string;
  resources: Resource[];
}

export const isTauri =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

const realApi = {
  markFrontendReady: () => invoke<void>('mark_frontend_ready'),
  getAppState: () => invoke<AppStateView>('get_app_state'),
  checkAgent: (agent?: string, customBin?: string) =>
    invoke<boolean>('check_agent', { agent, customBin }),
  completeSetup: (
    hour: number,
    minute: number,
    escapePhrase: string,
    kioskLevel = 'hard',
    model = 'opus',
    agent = 'claude',
    customAgentBin = ''
  ) =>
    invoke<AppStateView>('complete_setup', {
      input: {
        hour,
        minute,
        escape_phrase: escapePhrase,
        kiosk_level: kioskLevel,
        model,
        agent,
        custom_agent_bin: customAgentBin,
      },
    }),
  setKioskLevel: (level: string) => invoke<void>('set_kiosk_level', { level }),
  setModel: (model: string) => invoke<void>('set_model', { model }),
  setAgent: (agent: string, customBin?: string) =>
    invoke<void>('set_agent', { agent, customBin }),
  updateSchedule: (hour: number, minute: number) =>
    invoke<void>('update_schedule', { hour, minute }),
  pauseSchedule: () => invoke<void>('pause_schedule'),
  resumeSchedule: () => invoke<void>('resume_schedule'),
  startSession: () => invoke<SessionView>('start_session'),
  getQuiz: () => invoke<QuizQuestionView[]>('get_quiz'),
  submitAnswer: (questionId: number, answer: string) =>
    invoke<void>('submit_answer', { questionId, answer }),
  finishQuiz: () => invoke<ReviewData>('finish_quiz'),
  getReview: () => invoke<ReviewData>('get_review'),
  finishReview: () => invoke<SessionView>('finish_review'),
  getRoulette: () => invoke<RouletteView>('get_roulette'),
  ensureCourse: () => invoke<CourseView>('ensure_course'),
  startCourse: () => invoke<SessionView>('start_course'),
  finishCourse: () => invoke<SessionView>('finish_course'),
  escapeSession: (phrase: string) => invoke<boolean>('escape_session', { phrase }),
  extendSession: () => invoke<SessionView>('extend_session'),
  ensureAudio: () => invoke<AudioView>('ensure_audio'),
  getAudioEnabled: () => invoke<boolean>('get_audio_enabled'),
  setAudioEnabled: (enabled: boolean) => invoke<void>('set_audio_enabled', { enabled }),
  getExitQuiz: () => invoke<ExitQuizQuestion[]>('get_exit_quiz'),
  submitExitQuiz: (answers: Record<number, string>) =>
    invoke<ExitQuizResult>('submit_exit_quiz', { answers }),
  getEscapePhrase: () => invoke<string>('get_escape_phrase'),
  getDashboard: () => invoke<DashboardView>('get_dashboard'),
  getPastCourse: (date: string) =>
    invoke<ArchivedCourse | null>('get_past_course', { date }),
  openResources: () => invoke<number>('open_resources'),
};

/** Demo mode: outside Tauri (plain `vite dev`), serve canned data from mock.ts. */
import { mockApi, mockListen } from './mock';

export const api: typeof realApi = isTauri ? realApi : (mockApi as unknown as typeof realApi);

export function onEvent<T>(name: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  if (!isTauri) {
    return Promise.resolve(mockListen(name, (p) => handler(p as T)));
  }
  return listen<T>(name, (e) => handler(e.payload));
}
