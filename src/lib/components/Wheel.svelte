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
    <g
      class="wheel"
      class:spinning
      style="transform: rotate({rotation}deg); transform-origin: 200px 200px;"
    >
      {#each pool as title, i}
        <path d={segPath(i)} class="seg" class:alt={i % 2 === 1} />
        <text class="seg-label" text-anchor="middle" dy="4" transform={labelTransform(i)}>
          {short(title)}
        </text>
      {/each}
      <circle cx="200" cy="200" r="42" class="hub" />
      <text x="200" y="206" text-anchor="middle" class="hub-label">SDR</text>
    </g>
    <path d="M200,8 L188,34 L212,34 Z" class="pointer" />
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
  .seg {
    fill: var(--surface);
    stroke: var(--bg);
    stroke-width: 2;
  }
  .seg.alt {
    fill: var(--surface-2);
  }
  .seg-label {
    font-family: var(--font-body);
    font-size: 11px;
    fill: var(--muted);
  }
  .hub {
    fill: var(--accent);
  }
  .hub-label {
    font-family: var(--font-display);
    font-size: 15px;
    font-weight: 600;
    fill: var(--accent-fg);
  }
  .pointer {
    fill: var(--accent);
  }
</style>
