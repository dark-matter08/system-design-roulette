<script lang="ts">
  let {
    pool,
    chosenIndex,
    onLanded,
  }: { pool: string[]; chosenIndex: number; onLanded: () => void } = $props();

  let rotation = $state(0);
  let spinning = $state(false);
  let landed = $state(false);

  const n = $derived(pool.length);
  const segAngle = $derived(360 / Math.max(n, 1));

  export function spin() {
    if (spinning || landed) return;
    spinning = true;
    // Pointer is at top (0deg). Land the chosen segment's center under it.
    const target = 360 * 6 + (360 - (chosenIndex * segAngle + segAngle / 2));
    rotation = target;
    setTimeout(() => {
      spinning = false;
      landed = true;
      onLanded();
    }, 5200);
  }

  function segPath(i: number): string {
    const r = 180;
    const a0 = ((i * segAngle - 90) * Math.PI) / 180;
    const a1 = (((i + 1) * segAngle - 90) * Math.PI) / 180;
    const x0 = 200 + r * Math.cos(a0);
    const y0 = 200 + r * Math.sin(a0);
    const x1 = 200 + r * Math.cos(a1);
    const y1 = 200 + r * Math.sin(a1);
    const large = segAngle > 180 ? 1 : 0;
    return `M200,200 L${x0},${y0} A${r},${r} 0 ${large},1 ${x1},${y1} Z`;
  }

  function labelTransform(i: number): string {
    const mid = i * segAngle + segAngle / 2 - 90;
    const rad = (mid * Math.PI) / 180;
    const x = 200 + 108 * Math.cos(rad);
    const y = 200 + 108 * Math.sin(rad);
    const flip = mid > 90 && mid < 270 ? 180 : 0;
    return `translate(${x} ${y}) rotate(${mid + flip})`;
  }

  function short(title: string): string {
    return title.length > 22 ? title.slice(0, 20) + '…' : title;
  }
</script>

<div class="wheel-wrap">
  <svg viewBox="0 0 400 430" class="wheel-svg">
    <!-- static chrome: dashed shard ring + boundary ticks -->
    <circle cx="200" cy="200" r="190" class="ring-outer" />
    <g
      class="wheel"
      class:spinning
      style="transform: rotate({rotation}deg); transform-origin: 200px 200px;"
    >
      {#each pool as title, i}
        <path d={segPath(i)} class="seg" class:alt={i % 2 === 1} />
        <text class="seg-label mono" text-anchor="middle" dy="4" transform={labelTransform(i)}>
          {short(title)}
        </text>
        <!-- shard LED at each segment's outer edge -->
        {@const mid = ((i * segAngle + segAngle / 2 - 90) * Math.PI) / 180}
        <circle cx={200 + 168 * Math.cos(mid)} cy={200 + 168 * Math.sin(mid)} r="2.5" class="shard-led" />
      {/each}
      <circle cx="200" cy="200" r="46" class="hub" />
      <circle cx="200" cy="200" r="46" class="hub-ring" />
      <circle cx="200" cy="178" r="3" class="hub-led" class:pulsing={spinning} />
      <text x="200" y="200" text-anchor="middle" class="hub-label mono">topic</text>
      <text x="200" y="214" text-anchor="middle" class="hub-label mono">selector</text>
    </g>
    <!-- pointer: ingress pipe from the top -->
    <line x1="200" y1="0" x2="200" y2="18" class="ptr-pipe" />
    <path d="M200,34 L190,12 L210,12 Z" class="pointer" />
  </svg>
</div>

<style>
  .wheel-wrap {
    display: flex;
    justify-content: center;
  }
  .wheel-svg {
    width: min(46vh, 420px);
  }
  .wheel {
    transition: none;
  }
  .wheel.spinning {
    transition: transform 5.2s cubic-bezier(0.12, 0.65, 0.08, 1);
  }
  .ring-outer {
    fill: none;
    stroke: var(--node-border);
    stroke-width: 1.2;
    stroke-dasharray: 5 4;
  }
  .seg {
    fill: var(--node-bg);
    stroke: var(--node-divider);
    stroke-width: 1.5;
  }
  .seg.alt {
    fill: var(--surface);
  }
  .seg-label {
    font-size: 9.5px;
    fill: var(--muted);
    letter-spacing: 0.2px;
  }
  .shard-led {
    fill: var(--led-idle);
  }
  .hub {
    fill: var(--node-bg);
  }
  .hub-ring {
    fill: none;
    stroke: var(--violet);
    stroke-width: 1.5;
    stroke-dasharray: 4 3;
  }
  .hub-led {
    fill: var(--led-ok);
  }
  .hub-led.pulsing {
    fill: var(--led-warn);
    animation: led-pulse 0.6s ease infinite;
  }
  .hub-label {
    font-size: 9px;
    letter-spacing: 1.5px;
    text-transform: uppercase;
    fill: var(--violet-fg);
  }
  .ptr-pipe {
    stroke: var(--accent);
    stroke-width: 1.5;
    stroke-dasharray: 4 3;
  }
  .pointer {
    fill: var(--accent);
  }
</style>
