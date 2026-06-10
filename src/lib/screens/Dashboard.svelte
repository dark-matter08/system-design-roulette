<script lang="ts">
  import { api, type DashboardView, type ArchivedCourse } from '../ipc';
  import { app } from '../stores.svelte';
  import Markdown from '../components/Markdown.svelte';
  import ClusterBar from '../components/ClusterBar.svelte';
  import NodeCard from '../components/NodeCard.svelte';

  let data = $state<DashboardView | null>(null);
  let viewing = $state<ArchivedCourse | null>(null);

  $effect(() => {
    api.getDashboard().then((d) => (data = d));
  });

  async function openCourse(date: string) {
    viewing = await api.getPastCourse(date);
  }

  function scoreLabel(s: number | null): string {
    return s == null ? '—' : Math.round(s * 100) + '%';
  }

  /** Mastery entries grouped by category, preserving backend order. */
  const masteryByCategory = $derived.by(() => {
    const groups = new Map<string, NonNullable<typeof data>['mastery']>();
    for (const e of data?.mastery ?? []) {
      if (!groups.has(e.category)) groups.set(e.category, []);
      groups.get(e.category)!.push(e);
    }
    return [...groups.entries()];
  });

  /** Last 16 weeks as columns of 7 days, GitHub-style. */
  const heatmap = $derived.by(() => {
    const byDate = new Map((data?.history ?? []).map((h) => [h.date, h.status]));
    const today = new Date();
    const cells: { date: string; status: string }[] = [];
    for (let i = 16 * 7 - 1; i >= 0; i--) {
      const d = new Date(today);
      d.setDate(d.getDate() - i);
      const key = d.toISOString().slice(0, 10);
      cells.push({ date: key, status: byDate.get(key) ?? 'none' });
    }
    const weeks: { date: string; status: string }[][] = [];
    for (let w = 0; w < 16; w++) weeks.push(cells.slice(w * 7, w * 7 + 7));
    return weeks;
  });
</script>

<div class="dash-wrap blueprint">
  <ClusterBar route={viewing ? 'archive' : 'cluster'} status="read-only" tone="idle" />
  {#if viewing}
    <div class="dash-inner">
      <div class="dash-head">
        <button class="ghost mono-ghost" onclick={() => (viewing = null)}>← back</button>
        <h2>{viewing.title} <span class="date mono">({viewing.session_date})</span></h2>
      </div>
      <div class="dash-body">
        <article class="column theme-scholar reader-card">
          <Markdown markdown={viewing.markdown} />
        </article>
      </div>
    </div>
  {:else}
    <div class="dash-inner">
      <div class="dash-head">
        <button class="ghost mono-ghost" onclick={() => { app.screen = 'idle'; app.refresh(); }}>← back</button>
        <h2>Cluster overview</h2>
      </div>
      {#if !data}
        <p class="sub mono">loading…</p>
      {:else}
        <div class="stats">
          <NodeCard name="uptime" badge="streak" badgeTone="teal">
            {#snippet children()}<div class="num mono">{data?.streak ?? 0}d</div>{/snippet}
          </NodeCard>
          <NodeCard name="pool-coverage" badge="topics" badgeTone="violet">
            {#snippet children()}<div class="num mono">{data?.concepts_covered ?? 0}/{data?.concepts_total ?? 0}</div>{/snippet}
          </NodeCard>
          <NodeCard name="dead-letter-queue" badge="retries due" badgeTone="amber">
            {#snippet children()}<div class="num mono">{data?.carryover_due ?? 0}</div>{/snippet}
          </NodeCard>
        </div>
        <div class="hm-block">
          <div class="meta-label">MASTERY_STORE — per-concept ledger</div>
          <div class="ms-grid">
            {#each masteryByCategory as [category, entries]}
              <div class="ms-row">
                <span class="ms-cat mono">{category}</span>
                <div class="ms-cells">
                  {#each entries as e}
                    <div
                      class="ms-cell ms-{e.state}"
                      title="{e.title} — {e.state}{e.state !== 'unseen' ? ` (${Math.round(e.score_ema * 100)}%)` : ''}"
                    ></div>
                  {/each}
                </div>
              </div>
            {/each}
          </div>
          <div class="ms-legend mono">
            <span><i class="ms-cell ms-mastered"></i> mastered</span>
            <span><i class="ms-cell ms-practicing"></i> practicing</span>
            <span><i class="ms-cell ms-introduced"></i> introduced</span>
            <span><i class="ms-cell ms-struggling"></i> struggling</span>
            <span><i class="ms-cell ms-unseen"></i> unseen</span>
          </div>
        </div>
        <div class="hm-block">
          <div class="meta-label">SESSION_LOG — last 16 weeks</div>
          <div class="heatmap" aria-label="last 16 weeks of sessions">
            {#each heatmap as week}
              <div class="hm-col">
                {#each week as cell}
                  <div
                    class="hm-cell"
                    class:done={cell.status === 'completed'}
                    class:skip={cell.status === 'skipped'}
                    title="{cell.date}: {cell.status === 'none' ? 'no session' : cell.status}"
                  ></div>
                {/each}
              </div>
            {/each}
          </div>
        </div>
        <div class="dash-body">
          <table class="history mono">
            <thead>
              <tr><th>date</th><th>topic</th><th>status</th><th>budget</th><th></th></tr>
            </thead>
            <tbody>
              {#each data.history as h}
                <tr>
                  <td>{h.date}</td>
                  <td class="topic-cell">{h.concept_title ?? '—'}</td>
                  <td>
                    <span
                      class="badge"
                      class:ok={h.status === 'completed'}
                      class:bad={h.status === 'skipped'}
                      class:warn={h.status === 'in_progress' || h.status === 'pending'}
                    >
                      {h.status === 'completed' ? '200' : h.status === 'skipped' ? '503' : h.status}
                    </span>
                  </td>
                  <td>{scoreLabel(h.quiz_score)}</td>
                  <td>
                    {#if h.concept_title}
                      <button class="ghost mono-ghost small" onclick={() => openCourse(h.date)}>read</button>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .dash-wrap {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    animation: fade-in 0.35s ease;
  }
  .dash-inner {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 18px 40px 32px;
    min-height: 0;
    gap: 16px;
    width: min(1000px, 100%);
    margin: 0 auto;
  }
  .dash-head {
    display: flex;
    align-items: center;
    gap: 16px;
  }
  .dash-head h2 {
    font-size: 24px;
  }
  .date {
    color: var(--muted);
    font-size: 13px;
  }
  .sub {
    color: var(--faint);
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 14px;
  }
  .num {
    font-size: 24px;
    text-align: center;
    color: var(--fg);
  }
  .hm-block {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .heatmap {
    display: flex;
    gap: 3px;
  }
  .hm-col {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .hm-cell {
    width: 13px;
    height: 13px;
    border-radius: 3px;
    background: var(--surface);
  }
  .hm-cell.done {
    background: var(--accent);
  }
  .hm-cell.skip {
    background: var(--bad-bg);
    border: 1px solid var(--bad-fg);
    box-sizing: border-box;
  }
  .ms-grid {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ms-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .ms-cat {
    width: 110px;
    font-size: 10px;
    color: var(--faint);
    text-align: right;
    flex-shrink: 0;
  }
  .ms-cells {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
  }
  .ms-cell {
    width: 13px;
    height: 13px;
    border-radius: 3px;
    background: var(--surface);
    display: inline-block;
  }
  .ms-cell.ms-introduced {
    background: var(--violet-bg);
  }
  .ms-cell.ms-practicing {
    background: var(--warn-bg);
    border: 1px solid var(--led-warn);
    box-sizing: border-box;
  }
  .ms-cell.ms-struggling,
  .ms-cell.ms-decayed {
    background: var(--bad-bg);
    border: 1px solid var(--led-err);
    box-sizing: border-box;
  }
  .ms-cell.ms-mastered,
  .ms-cell.ms-maintenance {
    background: var(--led-ok);
  }
  .ms-legend {
    display: flex;
    gap: 16px;
    font-size: 10px;
    color: var(--faint);
    align-items: center;
  }
  .ms-legend span {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .dash-body {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }
  .history {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  .history th {
    text-align: left;
    color: var(--faint);
    font-weight: 500;
    font-size: 10px;
    letter-spacing: 1px;
    text-transform: uppercase;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  .history td {
    padding: 9px 12px;
    border-bottom: 1px solid var(--surface);
  }
  .topic-cell {
    font-family: var(--font-body);
    font-size: 13px;
  }
  .ghost.small {
    padding: 3px 10px;
    font-size: 11px;
  }
  .reader-card {
    background: var(--bg);
    color: var(--fg);
    border-radius: 14px;
    padding: 32px;
    max-width: 72ch;
  }
</style>
