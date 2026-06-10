<script lang="ts">
  import { api } from '../ipc';
  import { app } from '../stores.svelte';

  const owed = $derived(app.state?.owed ?? false);
  const streak = $derived(app.session?.streak ?? 0);
  const hour = $derived(app.state?.schedule_hour ?? 9);
  const minute = $derived(app.state?.schedule_minute ?? 0);

  let countdown = $state('');
  let editing = $state(false);
  let saved = $state(false);
  let newTime = $state('09:00');

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
</script>

<div class="screen">
  <span class="kicker">system design roulette</span>
  {#if owed}
    <h1>Your session starts now</h1>
    <p class="sub">This screen stays until the work is done.</p>
    <div class="pills">
      <span class="pill">🔥 {streak} day streak</span>
      <span class="pill">⏱ ~38 min total</span>
    </div>
    <button class="cta" onclick={begin}>Begin today's session</button>
  {:else}
    <h1>Next session in <span class="mono countdown">{countdown}</span></h1>
    <div class="pills">
      <span class="pill">🔥 {streak} day streak</span>
      <span class="pill">daily at {String(hour).padStart(2, '0')}:{String(minute).padStart(2, '0')}</span>
    </div>
    <div class="actions">
      <button class="ghost" onclick={() => (app.screen = 'dashboard')}>history & stats</button>
      <button class="ghost" onclick={begin}>start early</button>
      <button class="ghost" onclick={() => (editing = !editing)}>change time</button>
    </div>
    {#if editing}
      <div class="edit-row">
        <input type="time" bind:value={newTime} style="width: 140px;" />
        <button class="ghost" onclick={saveTime}>{saved ? 'saved ✓' : 'save'}</button>
      </div>
    {/if}
  {/if}
</div>

<style>
  h1 {
    font-size: 36px;
    margin: 16px 0 8px;
    text-align: center;
  }
  .countdown {
    color: var(--accent);
  }
  .sub {
    color: var(--muted);
  }
  .pills {
    display: flex;
    gap: 10px;
    margin: 20px 0 28px;
  }
  .actions {
    display: flex;
    gap: 12px;
  }
  .edit-row {
    display: flex;
    gap: 10px;
    margin-top: 16px;
    align-items: center;
  }
</style>
