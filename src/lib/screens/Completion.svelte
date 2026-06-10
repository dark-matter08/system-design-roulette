<script lang="ts">
  import { api } from '../ipc';
  import { app } from '../stores.svelte';
  import ClusterBar from '../components/ClusterBar.svelte';
  import NodeCard from '../components/NodeCard.svelte';
  import StatusLED from '../components/StatusLED.svelte';

  const session = $derived(app.session);
  const skipped = $derived(session?.status === 'skipped');
  let resourcesOpened = $state(false);

  async function openResources() {
    const n = await api.openResources().catch(() => 0);
    resourcesOpened = n > 0;
  }

  async function extend() {
    try {
      await api.extendSession();
      await app.refresh();
    } catch (e) {
      app.error = String(e);
    }
  }
</script>

<div class="done blueprint">
  <ClusterBar
    route={skipped ? 'postmortem' : 'deploy/complete'}
    status={skipped ? 'circuit breaker tripped' : 'session shipped'}
    tone={skipped ? 'err' : 'ok'}
  />
  <div class="done-body">
    {#if skipped}
      <div class="meta-label">POSTMORTEM — streak reset to 0</div>
      <h1>Circuit breaker tripped.</h1>
      <p class="sub">Today is marked skipped. The wheel spins again tomorrow.</p>
    {:else}
      <div class="meta-label">DEPLOY COMPLETE — session shipped</div>
      <h1>Done for today.</h1>
      <div class="stats">
        <NodeCard name="error-budget" badge="quiz" badgeTone="muted">
          {#snippet children()}
            <div class="num mono">
              {session?.quiz_score != null ? Math.round(session.quiz_score * 100) + '%' : '—'}
            </div>
          {/snippet}
        </NodeCard>
        <NodeCard name="uptime" badge="streak" badgeTone="teal">
          {#snippet children()}
            <div class="num mono">{session?.streak ?? 0}d</div>
          {/snippet}
        </NodeCard>
      </div>
      <div class="pregen">
        <StatusLED tone="pending" label={app.genStatus || "shard: pre-generating tomorrow's course"} />
      </div>
      <div class="actions">
        <button class="cta mono-cta" onclick={openResources}>
          {resourcesOpened ? '✓ egress queue flushed' : '⇡ open reading list in browser'}
        </button>
        <button class="ghost mono-ghost" onclick={() => (app.screen = 'dashboard')}>cluster overview</button>
      </div>
      <button class="ghost mono-ghost extend" onclick={extend}>
        ▲ extend session — one more topic (voluntary, no lock)
      </button>
    {/if}
  </div>
</div>

<style>
  .done {
    flex: 1;
    display: flex;
    flex-direction: column;
    animation: fade-in 0.35s ease;
  }
  .done-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }
  h1 {
    font-size: 36px;
    margin: 10px 0 22px;
  }
  .sub {
    color: var(--muted);
  }
  .stats {
    display: flex;
    gap: 14px;
    margin-bottom: 22px;
    min-width: 360px;
  }
  .stats :global(.node) {
    flex: 1;
  }
  .num {
    font-size: 26px;
    color: var(--fg);
    text-align: center;
  }
  .pregen {
    margin-bottom: 22px;
  }
  .actions {
    display: flex;
    gap: 12px;
    align-items: center;
  }
  .extend {
    margin-top: 18px;
    border-style: dashed;
    color: var(--violet-fg);
    border-color: var(--violet);
  }
</style>
