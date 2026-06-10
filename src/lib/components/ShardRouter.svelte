<script lang="ts">
  /**
   * Topic selection as request routing — replaces the casino wheel.
   * The unlocked pool is a rack of shard cards; "spinning" dispatches a
   * request whose scanner cursor hops across the rack, decelerating until it
   * lands on the (pre-decided) shard. Locked topics render as greyed
   * provisioning stubs so the rack visibly grows with the curriculum.
   */
  let {
    pool,
    chosenIndex,
    lockedCount = 0,
    onLanded,
  }: { pool: string[]; chosenIndex: number; lockedCount?: number; onLanded: () => void } = $props();

  let cursor = $state(-1); // card currently under the scanner
  let landedIdx = $state(-1);
  let dispatching = $state(false);
  let pings = $state<Set<number>>(new Set()); // cards recently scanned (LED afterglow)

  export function spin() {
    if (dispatching || landedIdx >= 0) return;
    dispatching = true;
    // Hop schedule: fast scan easing out, guaranteed to end on chosenIndex.
    const hops: number[] = [];
    let interval = 70;
    let t = 0;
    const times: number[] = [];
    while (interval < 620) {
      t += interval;
      times.push(t);
      interval *= 1.13;
    }
    let prev = -1;
    for (let i = 0; i < times.length; i++) {
      let next: number;
      if (i >= times.length - 3) {
        // Final approach: neighbors of the target, then the target itself.
        next = i === times.length - 1 ? chosenIndex : (chosenIndex + (times.length - 1 - i)) % pool.length;
      } else {
        do {
          next = Math.floor(Math.random() * pool.length);
        } while (next === prev && pool.length > 1);
      }
      hops.push(next);
      prev = next;
    }
    hops.forEach((idx, i) => {
      setTimeout(() => {
        cursor = idx;
        pings = new Set([...pings, idx]);
        setTimeout(() => {
          pings.delete(idx);
          pings = new Set(pings);
        }, 350);
        if (i === hops.length - 1) {
          setTimeout(() => {
            landedIdx = idx;
            dispatching = false;
            onLanded();
          }, 450);
        }
      }, times[i]);
    });
  }

  const stubCount = $derived(Math.min(lockedCount, 6));
</script>

<div class="router">
  <div class="ingress mono" class:active={dispatching}>
    <span class="ing-label">{dispatching ? '⇣ routing request' : landedIdx >= 0 ? '⇣ routed' : '⇣ request queued'}</span>
    <svg class="ing-pipe" viewBox="0 0 100 26" preserveAspectRatio="none" aria-hidden="true">
      <path d="M 50 0 L 50 26" />
    </svg>
  </div>

  <div class="rack" class:landed={landedIdx >= 0}>
    {#each pool as title, i}
      <div
        class="shard mono"
        class:scan={cursor === i && landedIdx < 0}
        class:ping={pings.has(i)}
        class:hit={landedIdx === i}
        class:dim={landedIdx >= 0 && landedIdx !== i}
      >
        <div class="shard-top">
          <span class="led"></span>
          <span class="sid">shard-{String(i).padStart(2, '0')}</span>
        </div>
        <div class="stitle">{title}</div>
      </div>
    {/each}
    {#each Array(stubCount) as _, i}
      <div class="shard stub mono" class:dim={landedIdx >= 0}>
        <div class="shard-top">
          <span class="led idle"></span>
          <span class="sid">shard-{String(pool.length + i).padStart(2, '0')}</span>
        </div>
        <div class="stitle">provisioning…</div>
      </div>
    {/each}
  </div>
</div>

<style>
  .router {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: min(880px, 94vw);
  }
  .ingress {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }
  .ing-label {
    font-size: 10px;
    letter-spacing: 1.5px;
    text-transform: uppercase;
    color: var(--faint);
  }
  .ingress.active .ing-label {
    color: var(--accent);
  }
  .ing-pipe {
    width: 60px;
    height: 22px;
  }
  .ing-pipe path {
    stroke: var(--node-border);
    stroke-width: 1.5;
    stroke-dasharray: 4 3;
  }
  .ingress.active .ing-pipe path {
    stroke: var(--accent);
    animation: pipe-flow 0.5s linear infinite;
  }
  @keyframes pipe-flow {
    to {
      stroke-dashoffset: -7;
    }
  }
  .rack {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 10px;
    width: 100%;
    margin-top: 8px;
  }
  .shard {
    background: var(--node-bg);
    border: 1px solid var(--node-border);
    border-radius: 8px;
    padding: 9px 12px 10px;
    transition: border-color 0.12s ease, background 0.12s ease, opacity 0.4s ease, transform 0.25s ease;
    min-height: 62px;
  }
  .shard-top {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-bottom: 5px;
  }
  .led {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--led-idle);
    transition: background 0.15s ease, box-shadow 0.15s ease;
  }
  .sid {
    font-size: 8.5px;
    letter-spacing: 1px;
    color: var(--faint);
    text-transform: uppercase;
  }
  .stitle {
    font-size: 11.5px;
    color: var(--muted);
    line-height: 1.4;
  }
  /* scanner states */
  .shard.scan {
    border-color: var(--accent);
    background: var(--surface-2);
  }
  .shard.scan .led {
    background: var(--accent);
    box-shadow: 0 0 8px var(--accent);
  }
  .shard.ping .led {
    background: var(--led-warn);
  }
  .shard.hit {
    border-color: var(--accent);
    background: var(--surface-2);
    transform: scale(1.045);
    box-shadow: 0 0 0 1px var(--accent), 0 8px 32px rgba(239, 159, 39, 0.18);
  }
  .shard.hit .led {
    background: var(--led-ok);
    box-shadow: 0 0 8px var(--led-ok);
  }
  .shard.hit .stitle {
    color: var(--fg);
  }
  .shard.hit .sid {
    color: var(--accent);
  }
  .shard.dim {
    opacity: 0.32;
  }
  .shard.stub {
    border-style: dashed;
    background: transparent;
  }
  .shard.stub .stitle {
    color: var(--faint);
    font-style: italic;
  }
  .led.idle {
    background: var(--led-idle);
    opacity: 0.5;
  }
</style>
