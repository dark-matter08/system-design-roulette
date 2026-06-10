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
  timerRemaining = $state<number>(-1);
  error = $state<string>('');

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
      this.screen = sess.step as Screen;
      if (sess.step === 'done') this.screen = 'completion';
      return;
    }
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

  async init() {
    // Tell Rust the webview booted — the kiosk refuses to lock before this.
    await api.markFrontendReady().catch(() => {});
    await this.refresh();
    await onEvent('session:owed', () => this.refresh());
    await onEvent('session:state', () => this.refresh());
    await onEvent<string>('gen:status', (msg) => {
      this.genStatus = msg;
    });
    await onEvent<number>('timer:tick', (remaining) => {
      this.timerRemaining = remaining;
    });
  }
}

export const app = new AppStore();
