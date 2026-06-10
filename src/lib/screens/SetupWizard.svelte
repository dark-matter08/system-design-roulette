<script lang="ts">
  import { api } from '../ipc';
  import { app } from '../stores.svelte';

  let time = $state('19:00');
  let phrase = $state('I am choosing to skip my training today and I accept the broken streak');
  let phrase2 = $state('');
  let agentStatus = $state<'idle' | 'checking' | 'ok' | 'fail'>('idle');
  let submitting = $state(false);
  let error = $state('');

  async function checkAgent() {
    agentStatus = 'checking';
    agentStatus = (await api.checkAgent().catch(() => false)) ? 'ok' : 'fail';
  }

  async function finish() {
    error = '';
    if (phrase.trim().length < 40) {
      error = 'Escape phrase must be at least 40 characters.';
      return;
    }
    if (phrase.trim() !== phrase2.trim()) {
      error = 'Phrases do not match.';
      return;
    }
    const [h, m] = time.split(':').map(Number);
    submitting = true;
    try {
      await api.completeSetup(h, m, phrase.trim());
      await app.refresh();
    } catch (e) {
      error = String(e);
    } finally {
      submitting = false;
    }
  }
</script>

<div class="screen">
  <div class="wizard">
    <span class="kicker">system design roulette</span>
    <h1>Setup your daily session</h1>
    <p class="sub">
      Every day at the time you choose, this app takes over your screen for a quiz on
      yesterday's topic and a 30-minute course on a new one. There is no closing it.
    </p>

    <label class="field">
      <span>Session time (daily)</span>
      <input type="time" bind:value={time} />
    </label>

    <label class="field">
      <span>Escape phrase — your only way out (min 40 chars)</span>
      <input type="text" bind:value={phrase} />
    </label>
    <label class="field">
      <span>Type it again</span>
      <input type="text" bind:value={phrase2} placeholder="Repeat the escape phrase" />
    </label>

    <div class="agent-row">
      <button class="ghost" onclick={checkAgent} disabled={agentStatus === 'checking'}>
        {agentStatus === 'checking' ? 'checking claude…' : 'test agent connection'}
      </button>
      {#if agentStatus === 'ok'}<span class="badge ok">claude responds</span>{/if}
      {#if agentStatus === 'fail'}<span class="badge bad">claude unreachable — bundled fallback courses will be used</span>{/if}
    </div>

    {#if error}<p class="error">{error}</p>{/if}

    <button class="cta" onclick={finish} disabled={submitting}>
      {submitting ? 'Installing…' : 'Lock it in'}
    </button>
    <p class="fine">
      Installs a launch agent that fires at {time} daily. Your first course generates in
      the background now.
    </p>
  </div>
</div>

<style>
  .wizard {
    max-width: 560px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  h1 {
    font-size: 34px;
  }
  .sub {
    color: var(--muted);
    margin: 0;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .field span {
    font-size: 12px;
    color: var(--muted);
  }
  .agent-row {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .error {
    color: var(--bad-fg);
    margin: 0;
    font-size: 13px;
  }
  .fine {
    font-size: 12px;
    color: var(--faint);
    margin: 0;
  }
</style>
