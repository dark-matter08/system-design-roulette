<script lang="ts">
  import { api } from '../ipc';
  import { app } from '../stores.svelte';

  const session = $derived(app.session);
  const skipped = $derived(session?.status === 'skipped');
  let resourcesOpened = $state(false);

  async function openResources() {
    const n = await api.openResources().catch(() => 0);
    resourcesOpened = n > 0;
  }
</script>

<div class="screen">
  {#if skipped}
    <span class="kicker">session skipped</span>
    <h1>Streak broken.</h1>
    <p class="sub">Today is marked as skipped. The wheel spins again tomorrow.</p>
  {:else}
    <span class="kicker">session complete</span>
    <h1>Done for today.</h1>
    <div class="stats">
      <div class="stat">
        <span class="num">
          {session?.quiz_score != null ? Math.round(session.quiz_score * 100) + '%' : '—'}
        </span>
        <span class="lab">quiz score</span>
      </div>
      <div class="stat">
        <span class="num">{session?.streak ?? 0}</span>
        <span class="lab">day streak</span>
      </div>
    </div>
    {#if app.genStatus}
      <p class="fine">{app.genStatus}</p>
    {:else}
      <p class="fine">pre-generating tomorrow's course in the background…</p>
    {/if}
    <div class="actions">
      <button class="cta" onclick={openResources}>
        {resourcesOpened ? 'Resources opened ✓' : 'Open reading list in browser'}
      </button>
      <button class="ghost" onclick={() => (app.screen = 'dashboard')}>history & stats</button>
    </div>
  {/if}
</div>

<style>
  h1 {
    font-size: 38px;
    margin: 14px 0 22px;
  }
  .sub {
    color: var(--muted);
  }
  .stats {
    display: flex;
    gap: 14px;
    margin-bottom: 22px;
  }
  .stat {
    background: var(--surface);
    border-radius: 12px;
    padding: 16px 26px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }
  .num {
    font-size: 26px;
    font-weight: 600;
    font-family: var(--font-display);
  }
  .lab {
    font-size: 11px;
    color: var(--muted);
    letter-spacing: 1px;
    text-transform: uppercase;
  }
  .fine {
    font-size: 12px;
    color: var(--faint);
    margin-bottom: 20px;
  }
  .actions {
    display: flex;
    gap: 12px;
    align-items: center;
  }
</style>
