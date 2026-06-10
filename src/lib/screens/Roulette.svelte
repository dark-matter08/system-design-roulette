<script lang="ts">
  import { api, type RouletteView } from '../ipc';
  import { app } from '../stores.svelte';
  import Wheel from '../components/Wheel.svelte';

  let data = $state<RouletteView | null>(null);
  let wheel = $state<ReturnType<typeof Wheel>>();
  let phase = $state<'ready' | 'spinning' | 'landed' | 'generating'>('ready');

  $effect(() => {
    api.getRoulette().then((r) => (data = r));
    // Kick off course generation in the background immediately — the wheel
    // animation and reveal hide (some of) the latency.
  });

  function spin() {
    phase = 'spinning';
    wheel?.spin();
  }

  let confetti = $state<{ x: number; delay: number; color: string; drift: number }[]>([]);

  async function landed() {
    phase = 'landed';
    const colors = ['#ef9f27', '#9fe1cb', '#f0997b', '#cecbf6', '#fac775'];
    confetti = Array.from({ length: 60 }, (_, i) => ({
      x: Math.random() * 100,
      delay: Math.random() * 0.6,
      drift: (Math.random() - 0.5) * 200,
      color: colors[i % colors.length],
    }));
    setTimeout(() => (confetti = []), 3500);
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

<div class="screen">
  {#each confetti as c}
    <span
      class="confetti"
      style="left: {c.x}%; animation-delay: {c.delay}s; background: {c.color}; --drift: {c.drift}px;"
    ></span>
  {/each}
  {#if !data}
    <p class="sub">Preparing the wheel…</p>
  {:else}
    <span class="kicker">today's topic</span>
    <Wheel bind:this={wheel} pool={data.pool} chosenIndex={data.chosen_index} onLanded={landed} />
    {#if phase === 'ready'}
      <button class="cta" onclick={spin}>Spin</button>
    {:else if phase === 'spinning'}
      <p class="sub">…</p>
    {:else if phase === 'landed'}
      <h1 class="topic">{data.concept_title}</h1>
      <p class="sub">category: {data.concept_category}</p>
      <button class="cta" onclick={toCourse}>Start the course</button>
    {:else}
      <h1 class="topic">{data.concept_title}</h1>
      <div class="gen">
        <div class="spinner"></div>
        <p class="sub">{app.genStatus || 'forging your course…'}</p>
        <p class="fine">
          If this is the first time today's course is generated it can take a few
          minutes — your agent is researching real resources.
        </p>
      </div>
    {/if}
  {/if}
</div>

<style>
  .topic {
    font-size: 30px;
    margin: 18px 0 4px;
    text-align: center;
  }
  .sub {
    color: var(--muted);
    margin: 4px 0 18px;
  }
  .fine {
    font-size: 12px;
    color: var(--faint);
    max-width: 420px;
    text-align: center;
  }
  .gen {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }
  .spinner {
    width: 28px;
    height: 28px;
    border: 3px solid var(--surface-2);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .confetti {
    position: fixed;
    top: -12px;
    width: 9px;
    height: 9px;
    border-radius: 2px;
    pointer-events: none;
    animation: confetti-fall 2.8s ease-in forwards;
    z-index: 40;
  }
  @keyframes confetti-fall {
    to {
      transform: translateY(105vh) translateX(var(--drift)) rotate(720deg);
      opacity: 0.7;
    }
  }
</style>
