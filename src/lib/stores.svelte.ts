import { api, onEvent, type AppStateView, type SessionView } from './ipc';

export type Screen =
  | 'loading'
  | 'setup'
  | 'idle'
  | 'quiz'
  | 'review'
  | 'roulette'
  | 'course'
  | 'completion'
  | 'dashboard';

class AppStore {
  state = $state<AppStateView | null>(null);
  screen = $state<Screen>('loading');
  genStatus = $state<string>('');
  genLog = $state<string[]>([]);
  timerRemaining = $state<number>(-1);
  error = $state<string>('');
  /** User stepped away from an UNLOCKED in-progress session (early start /
   *  extension). Cleared the moment the session is owed or locked. */
  stepAway = $state(false);

  get session(): SessionView | null {
    return this.state?.session ?? null;
  }

  async refresh() {
    try {
      this.state = await api.getAppState();
      this.route();
    } catch (e) {
      this.error = String(e);
    }
  }

  /** Decide which screen to show from authoritative Rust state. */
  route() {
    const s = this.state;
    if (!s) return;
    if (!s.onboarded) {
      this.screen = 'setup';
      return;
    }
    const sess = s.session;
    if (sess.status === 'in_progress') {
      // Enforcement always wins; voluntary sessions can be stepped away from.
      if (sess.locked || s.owed) this.stepAway = false;
      if (this.stepAway) {
        if (this.screen === 'loading') this.screen = 'idle';
        return;
      }
      this.screen = sess.step as Screen;
      if (sess.step === 'done') this.screen = 'completion';
      return;
    }
    this.stepAway = false;
    if ((sess.status === 'completed' || sess.status === 'skipped') && this.screen !== 'dashboard') {
      this.screen = 'completion';
      return;
    }
    if (s.owed) {
      // Session owed: idle screen shows the "begin" lock-in state.
      this.screen = 'idle';
      return;
    }
    // Not owed, nothing in progress: leave user-navigated screens (dashboard)
    // alone, but transient screens (loading, setup) must land somewhere.
    if (this.screen === 'loading' || this.screen === 'setup') this.screen = 'idle';
  }

  /** Leave an unlocked in-progress session for the idle/dashboard screens. */
  leaveSession() {
    if (this.session?.locked) return;
    this.stepAway = true;
    this.screen = 'idle';
  }

  /** Return to the in-progress session at its saved step. */
  resumeSession() {
    this.stepAway = false;
    this.route();
  }

  async init() {
    // Tell Rust the webview booted — the kiosk refuses to lock before this.
    await api.markFrontendReady().catch(() => {});
    await this.refresh();
    await onEvent('session:owed', () => this.refresh());
    await onEvent('session:state', () => this.refresh());
    await onEvent<string>('gen:status', (msg) => {
      this.genStatus = msg;
    });
    await onEvent<string>('gen:log', (line) => {
      const ts = new Date().toTimeString().slice(0, 8);
      this.genLog = [...this.genLog.slice(-49), `${ts}  ${line}`];
    });
    await onEvent<number>('timer:tick', (remaining) => {
      this.timerRemaining = remaining;
    });
  }
}

export const app = new AppStore();
