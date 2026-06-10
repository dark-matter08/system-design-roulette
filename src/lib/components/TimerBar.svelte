<script lang="ts">
  let { remaining, total }: { remaining: number; total: number } = $props();

  const pct = $derived(total > 0 ? ((total - remaining) / total) * 100 : 0);
  const mm = $derived(Math.floor(Math.max(remaining, 0) / 60));
  const ss = $derived(Math.max(remaining, 0) % 60);
</script>

<div class="timer-bar">
  <div class="row">
    <span class="label">required reading</span>
    <span class="clock mono">
      {#if remaining > 0}
        {mm}:{String(ss).padStart(2, '0')}
      {:else}
        done
      {/if}
    </span>
  </div>
  <div class="progress-track">
    <div class="progress-fill" style="width: {pct}%"></div>
  </div>
</div>

<style>
  .timer-bar {
    width: 100%;
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 24px;
  }
  .label {
    font-size: 11px;
    letter-spacing: 2px;
    text-transform: uppercase;
    color: var(--faint);
  }
  .clock {
    font-size: 15px;
    color: var(--accent);
  }
</style>
