<script lang="ts">
  import { api, type RouletteView } from '../ipc';
  import { app } from '../stores.svelte';
  import Wheel from '../components/Wheel.svelte';
  import ClusterBar from '../components/ClusterBar.svelte';
  import StatusLED from '../components/StatusLED.svelte';
  import MetaBadge from '../components/MetaBadge.svelte';

  let data = $state<RouletteView | null>(null);
  let wheel = $state<ReturnType<typeof Wheel>>();
  let phase = $state<'ready' | 'spinning' | 'landed' | 'generating'>('ready');

  $effect(() => {
    api.getRoulette().then((r) => (data = r));
  });

  function spin() {
    phase = 'spinning';
    wheel?.spin();
  }

  async function landed() {
    phase = 'landed';
  }

  async function toCourse() {
    phase = 'generating';
    try {
      await api.ensureCourse();
      await api.startCourse();
      await app.refresh();
    } catch (e) {
      app.error = String(e);
      phase = 'landed';
    }
  }
</script>

<div class="roulette blueprint">
  <ClusterBar route="topic-selector" status="weighted-random · no repeats until pool drains" tone="ok" />
  {#if !data}
    <div class="center"><StatusLED tone="pending" label="loading pool…" /></div>
  {:else}
    <div class="roulette-body">
      <div class="meta-label">
        TOPIC_SELECTOR — pool: {data.pool_unlocked}/{data.pool_total} unlocked · {data.pool_total -
          data.pool_unlocked} provisioning
      </div>
      <Wheel bind:this={wheel} pool={data.pool} chosenIndex={data.chosen_index} onLanded={landed} />
      {#if phase === 'ready'}
        <button class="cta mono-cta" onclick={spin}>⟳ spin</button>
      {:else if phase === 'spinning'}
        <div class="mono dim">selecting…</div>
      {:else if phase === 'landed'}
        <h1 class="topic">{data.concept_title}</h1>
        <MetaBadge tone="violet">{#snippet children()}shard: {data?.concept_category}{/snippet}</MetaBadge>
        <button class="cta mono-cta" onclick={toCourse}>→ start the course</button>
      {:else}
        <h1 class="topic">{data.concept_title}</h1>
        <div class="gen">
          <StatusLED tone="pending" label={app.genStatus || 'shard: generating'} />
          <p class="fine mono">
            first generation researches real resources via your agent — can take a few minutes
          </p>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .roulette {
    flex: 1;
    display: flex;
    flex-direction: column;
    animation: fade-in 0.35s ease;
  }
  .center {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .roulette-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
    padding: 18px;
  }
  .topic {
    font-size: 28px;
    text-align: center;
    margin: 0;
  }
  .dim {
    color: var(--faint);
    font-size: 12px;
  }
  .gen {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
  }
  .fine {
    font-size: 10px;
    color: var(--faint);
    max-width: 420px;
    text-align: center;
    margin: 0;
  }
</style>
