<script lang="ts">
  import { api } from '../ipc';
  import { app } from '../stores.svelte';
  import ClusterBar from '../components/ClusterBar.svelte';
  import NodeCard from '../components/NodeCard.svelte';
  import MetaBadge from '../components/MetaBadge.svelte';
  import TimePicker from '../components/TimePicker.svelte';
  import EnforcementPicker from '../components/EnforcementPicker.svelte';

  const owed = $derived(app.state?.owed ?? false);
  const streak = $derived(app.session?.streak ?? 0);
  const hour = $derived(app.state?.schedule_hour ?? 9);
  const minute = $derived(app.state?.schedule_minute ?? 0);

  let countdown = $state('');
  let editing = $state(false);
  let saved = $state(false);
  let newTime = $state('09:00');
  let editingEnf = $state(false);
  let enfLevel = $state('hard');
  let enfSaved = $state(false);

  $effect(() => {
    enfLevel = app.state?.kiosk_level ?? 'hard';
  });

  async function saveEnf() {
    try {
      await api.setKioskLevel(enfLevel);
      enfSaved = true;
      setTimeout(() => {
        enfSaved = false;
        editingEnf = false;
      }, 1200);
      await app.refresh();
    } catch (e) {
      app.error = String(e);
    }
  }

  $effect(() => {
    newTime = `${String(hour).padStart(2, '0')}:${String(minute).padStart(2, '0')}`;
  });

  async function saveTime() {
    const [h, m] = newTime.split(':').map(Number);
    await api.updateSchedule(h, m);
    saved = true;
    setTimeout(() => {
      saved = false;
      editing = false;
    }, 1200);
    await app.refresh();
  }

  $effect(() => {
    const tick = () => {
      const now = new Date();
      const next = new Date();
      next.setHours(hour, minute, 0, 0);
      if (next <= now) next.setDate(next.getDate() + 1);
      const diff = Math.floor((next.getTime() - now.getTime()) / 1000);
      const h = Math.floor(diff / 3600);
      const m = Math.floor((diff % 3600) / 60);
      const s = diff % 60;
      countdown = `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
    };
    tick();
    const id = setInterval(tick, 1000);
    return () => clearInterval(id);
  });

  async function begin() {
    await api.startSession();
    await app.refresh();
  }

  async function pauseSched() {
    await api.pauseSchedule().catch((e) => (app.error = String(e)));
    await app.refresh();
  }

  async function resumeSched() {
    await api.resumeSchedule().catch((e) => (app.error = String(e)));
    await app.refresh();
  }
</script>

<div class="idle blueprint">
  <ClusterBar
    route={owed ? 'incident' : 'cluster'}
    status={owed ? 'session owed — lock imminent' : 'all systems nominal'}
    tone={owed ? 'warn' : 'ok'}
  />
  {#if app.state?.enforcement_disarmed}
    <div class="disarmed mono">
      ⚠ ENFORCEMENT DISARMED — ~/sdr-unlock exists; every lock releases instantly. Delete the file to re-arm.
    </div>
  {/if}
  <div class="idle-body">
    {#if owed}
      {@const isAudit = app.session?.session_type === 'pop_quiz'}
      <div class="meta-label">INCIDENT — P1 · {isAudit ? 'surprise audit due' : 'training session due'}</div>
      <h1>{isAudit ? 'Pop quiz. No new topic today.' : 'Your session starts now'}</h1>
      <p class="sub">This screen stays until the work is done.</p>
      <div class="badges">
        <MetaBadge tone="teal">{#snippet children()}● uptime {streak}d{/snippet}</MetaBadge>
        <MetaBadge tone="violet">{#snippet children()}est. {isAudit ? '15' : '38'} min{/snippet}</MetaBadge>
      </div>
      <button class="cta mono-cta" onclick={begin}>▲ ack &amp; begin session</button>
    {:else}
      <h1>Cluster idle</h1>
      <p class="sub">Next session deploys automatically. Showing up is the whole job.</p>
      <div class="node-wrap">
        {#if app.state?.schedule_paused}
          <NodeCard icon="⏸" name="cron-scheduler" badge="paused" badgeTone="red" accent="var(--led-err)">
            {#snippet children()}
              <div class="meta-label">SCHEDULER PAUSED</div>
              <div class="paused-note">No sessions will fire — launchd agent removed.</div>
              <button class="cta mono-cta resume-sched" onclick={resumeSched}>▶ resume schedule</button>
            {/snippet}
          </NodeCard>
        {:else}
          <NodeCard icon="⏱" name="cron-scheduler" badge=":launchd" badgeTone="amber">
            {#snippet children()}
              <div class="meta-label">NEXT_FIRE — T-minus</div>
              <div class="count mono">{countdown}</div>
              <div class="sched mono">daily at {String(hour).padStart(2, '0')}:{String(minute).padStart(2, '0')}</div>
            {/snippet}
          </NodeCard>
        {/if}
      </div>
      <div class="badges">
        <MetaBadge tone="teal">{#snippet children()}● uptime {streak}d{/snippet}</MetaBadge>
      </div>
      {#if app.session?.status === 'in_progress'}
        <button class="cta mono-cta resume" onclick={() => app.resumeSession()}>
          ▶ resume session — paused at {app.session.step}
        </button>
      {/if}
      <div class="actions">
        <button class="ghost mono-ghost" onclick={() => (app.screen = 'dashboard')}>cluster overview</button>
        {#if app.session?.status !== 'in_progress'}
          <button class="ghost mono-ghost" onclick={begin}>deploy early</button>
        {/if}
        <button class="ghost mono-ghost" onclick={() => (editing = !editing)}>reschedule</button>
        <button class="ghost mono-ghost" onclick={() => (editingEnf = !editingEnf)}>
          🔒 enforcement: {app.state?.kiosk_level ?? 'hard'}
        </button>
        {#if !app.state?.schedule_paused}
          <button class="ghost mono-ghost" onclick={pauseSched}>⏸ pause schedule</button>
        {/if}
      </div>
      {#if editingEnf}
        <div class="enf-edit">
          <EnforcementPicker bind:value={enfLevel} />
          <div class="enf-actions">
            <button class="ghost mono-ghost" onclick={saveEnf}>{enfSaved ? 'saved ✓' : 'apply'}</button>
          </div>
        </div>
      {/if}
      {#if editing}
        <div class="edit-row">
          <TimePicker bind:value={newTime} />
          <button class="ghost mono-ghost" onclick={saveTime}>{saved ? 'saved ✓' : 'apply'}</button>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .idle {
    flex: 1;
    display: flex;
    flex-direction: column;
    animation: fade-in 0.35s ease;
  }
  .idle-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }
  h1 {
    font-size: 34px;
    margin: 10px 0 6px;
    text-align: center;
  }
  .sub {
    color: var(--muted);
    font-size: 13px;
    margin: 0 0 22px;
  }
  .badges {
    display: flex;
    gap: 10px;
    margin-bottom: 24px;
  }
  .node-wrap {
    width: 320px;
    margin-bottom: 18px;
  }
  .count {
    font-size: 30px;
    color: var(--accent);
    margin: 4px 0 2px;
  }
  .sched {
    font-size: 11px;
    color: var(--faint);
  }
  .actions {
    display: flex;
    gap: 10px;
  }
  .resume {
    margin-bottom: 16px;
  }
  .enf-edit {
    width: min(620px, 90vw);
    margin-top: 16px;
    background: var(--node-bg);
    border: 1px solid var(--node-border);
    border-radius: 10px;
    padding: 14px 16px;
  }
  .enf-actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 10px;
  }
  .paused-note {
    font-size: 12px;
    color: var(--muted);
    margin: 4px 0 12px;
  }
  .resume-sched {
    font-size: 11px;
    padding: 8px 18px;
  }
  .disarmed {
    background: var(--bad-bg);
    color: var(--bad-fg);
    border-bottom: 1px dashed var(--led-err);
    font-size: 11px;
    letter-spacing: 0.5px;
    text-align: center;
    padding: 7px 16px;
  }
  .edit-row {
    display: flex;
    gap: 10px;
    margin-top: 16px;
    align-items: center;
  }
</style>
