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
