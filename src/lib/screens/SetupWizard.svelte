<script lang="ts">
  import { api } from '../ipc';
  import { app } from '../stores.svelte';
  import ClusterBar from '../components/ClusterBar.svelte';
  import NodeCard from '../components/NodeCard.svelte';
  import StatusLED from '../components/StatusLED.svelte';
  import TimePicker from '../components/TimePicker.svelte';

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
      error = 'escape phrase must be at least 40 characters';
      return;
    }
    if (phrase.trim() !== phrase2.trim()) {
      error = 'phrases do not match';
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

<div class="boot blueprint">
  <ClusterBar route="bootstrap" status="awaiting deploy" tone="warn" />
  <div class="boot-body">
    <h1>Bootstrap your training cluster</h1>
    <p class="sub">
      Three services to configure. Deploy wires them together — there is no rollback until
      tomorrow.
    </p>

    <div class="topo">
      <div class="row">
        <NodeCard icon="⏱" name="cron-scheduler" badge=":launchd" badgeTone="amber">
          {#snippet children()}
            <div class="meta-label">FIRE_AT — daily trigger</div>
            <div class="time-row">
              <TimePicker bind:value={time} />
              <span class="hint">local time<br />retry: on-wake</span>
            </div>
          {/snippet}
        </NodeCard>

        <NodeCard
          icon="⚡"
          name="agent-backend"
          badge={agentStatus === 'ok' ? 'healthy' : agentStatus === 'fail' ? 'degraded' : 'unknown'}
          badgeTone={agentStatus === 'ok' ? 'teal' : agentStatus === 'fail' ? 'red' : 'muted'}
        >
          {#snippet children()}
            <div class="meta-label">HEALTHCHECK — claude -p ping</div>
            <div class="health-row">
              {#if agentStatus === 'ok'}
                <StatusLED tone="ok" label="200 OK" />
                <span class="hint">fallback: codex → bundled</span>
              {:else if agentStatus === 'fail'}
                <StatusLED tone="err" label="unreachable" />
                <span class="hint">bundled courses will serve</span>
              {:else if agentStatus === 'checking'}
                <StatusLED tone="pending" label="probing…" />
              {:else}
                <button class="ghost mono-ghost" onclick={checkAgent}>run healthcheck</button>
              {/if}
            </div>
          {/snippet}
        </NodeCard>
      </div>

      <svg class="gap-pipes" viewBox="0 0 100 46" preserveAspectRatio="none" aria-hidden="true">
        <path d="M 25 0 L 25 20 L 50 20 L 50 46" />
        <path d="M 75 0 L 75 20 L 50 20" />
        <path d="M 50 38 L 48.6 33 L 51.4 33 Z" class="arrow" />
      </svg>

      <NodeCard icon="🔒" name="enforcement-service" badge="kiosk · level 1000" badgeTone="violet" accent="var(--violet)">
        {#snippet children()}
          <div class="enf">
            <div>
              <div class="meta-label">SLO — daily session completion</div>
              <div class="enf-desc">full-screen lock · ~38 min · no Cmd+Tab, no Force Quit, no mercy</div>
            </div>
            <div class="enf-io">ingress: cron<br />egress: streak++</div>
          </div>
        {/snippet}
      </NodeCard>
    </div>

    <div class="break-glass">
      <div class="bg-tag">BREAK<br />GLASS</div>
      <div class="bg-fields">
        <div class="bg-label">ESCAPE_PHRASE — circuit breaker · trips streak to 0 · min 40 chars</div>
        <input class="bg-input mono" type="text" bind:value={phrase} />
        <input
          class="bg-input mono"
          type="text"
          placeholder="type it again to confirm"
          bind:value={phrase2}
        />
      </div>
    </div>

    {#if error}<p class="error mono">✗ {error}</p>{/if}

    <div class="deploy-row">
      <button class="cta mono-cta" onclick={finish} disabled={submitting}>
        {submitting ? '… deploying' : '▲ deploy to prod'}
      </button>
      <span class="hint">
        writes launchd plist · pre-generates day-1 course<br />
        first session: today at {time} (or now, if {time} already passed)
      </span>
    </div>
  </div>
</div>

<style>
  .boot {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    animation: fade-in 0.35s ease;
  }
  .boot-body {
    width: min(860px, 92vw);
    margin: 0 auto;
    padding: 30px 24px 60px;
  }
  h1 {
    font-size: 28px;
    margin-bottom: 4px;
  }
  .sub {
    color: var(--muted);
    font-size: 13px;
    margin: 0 0 26px;
  }
  .topo {
    position: relative;
  }
  .gap-pipes {
    display: block;
    height: 46px;
    width: 100%;
    pointer-events: none;
  }
  .gap-pipes path {
    stroke: var(--violet);
    stroke-width: 1.5px;
    fill: none;
    stroke-dasharray: 5 4;
    vector-effect: non-scaling-stroke;
  }
  .gap-pipes path.arrow {
    fill: var(--violet);
    stroke: none;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px;
  }
  .time-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 6px;
  }
  .hint {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--faint);
    line-height: 1.6;
  }
  .health-row {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 8px;
    min-height: 30px;
  }
  .enf {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 14px;
    align-items: center;
  }
  .enf-desc {
    font-size: 13px;
    color: var(--muted);
    margin-top: 4px;
  }
  .enf-io {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--faint);
    text-align: right;
    line-height: 1.6;
  }
  .break-glass {
    margin-top: 18px;
    background: #1f1316;
    border: 1px dashed #793030;
    border-radius: 8px;
    padding: 12px 14px;
    display: flex;
    gap: 14px;
    align-items: flex-start;
  }
  .bg-tag {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--led-err);
    border: 1px solid #793030;
    border-radius: 4px;
    padding: 6px 8px;
    text-align: center;
    line-height: 1.5;
    margin-top: 14px;
  }
  .bg-fields {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .bg-label {
    font-family: var(--font-mono);
    font-size: 10px;
    color: #a05050;
    letter-spacing: 0.5px;
  }
  .bg-input {
    font-family: var(--font-mono);
    font-size: 12px;
    color: #d8b0a8;
    background: var(--bg);
    border: 1px solid #3a2228;
    border-radius: 6px;
    padding: 8px 12px;
  }
  .bg-input:focus {
    border-color: #793030;
  }
  .error {
    color: var(--bad-fg);
    font-size: 12px;
    margin: 14px 0 0;
  }
  .deploy-row {
    margin-top: 22px;
    display: flex;
    align-items: center;
    gap: 16px;
  }
</style>
