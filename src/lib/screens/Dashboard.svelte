<script lang="ts">
  import { api, type DashboardView, type ArchivedCourse } from '../ipc';
  import { app } from '../stores.svelte';
  import Markdown from '../components/Markdown.svelte';

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
</script>

<div class="dash-wrap">
  {#if viewing}
    <div class="dash-head">
      <button class="ghost" onclick={() => (viewing = null)}>← back</button>
      <h2>{viewing.title} <span class="date">({viewing.session_date})</span></h2>
    </div>
    <div class="dash-body">
      <article class="column theme-scholar reader-card">
        <Markdown markdown={viewing.markdown} />
      </article>
    </div>
  {:else}
    <div class="dash-head">
      <button class="ghost" onclick={() => { app.screen = 'idle'; app.refresh(); }}>← back</button>
      <h2>History & stats</h2>
    </div>
    {#if !data}
      <p class="sub">Loading…</p>
    {:else}
      <div class="stats">
        <div class="stat"><span class="num">{data.streak}</span><span class="lab">streak</span></div>
        <div class="stat">
          <span class="num">{data.concepts_covered}/{data.concepts_total}</span>
          <span class="lab">topics covered</span>
        </div>
        <div class="stat"><span class="num">{data.carryover_due}</span><span class="lab">questions returning</span></div>
      </div>
      <div class="dash-body">
        <table class="history">
          <thead>
            <tr><th>date</th><th>topic</th><th>status</th><th>score</th><th></th></tr>
          </thead>
          <tbody>
            {#each data.history as h}
              <tr>
                <td class="mono">{h.date}</td>
                <td>{h.concept_title ?? '—'}</td>
                <td>
                  <span
                    class="badge"
                    class:ok={h.status === 'completed'}
                    class:bad={h.status === 'skipped'}
                    class:warn={h.status === 'in_progress' || h.status === 'pending'}
                  >
                    {h.status}
                  </span>
                </td>
                <td class="mono">{scoreLabel(h.quiz_score)}</td>
                <td>
                  {#if h.concept_title}
                    <button class="ghost small" onclick={() => openCourse(h.date)}>read</button>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  {/if}
</div>

<style>
  .dash-wrap {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 32px 40px;
    min-height: 0;
    gap: 18px;
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
    font-size: 15px;
  }
  .sub {
    color: var(--muted);
  }
  .stats {
    display: flex;
    gap: 14px;
  }
  .stat {
    background: var(--surface);
    border-radius: 12px;
    padding: 14px 24px;
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .num {
    font-size: 22px;
    font-weight: 600;
    font-family: var(--font-display);
  }
  .lab {
    font-size: 10px;
    color: var(--muted);
    letter-spacing: 1px;
    text-transform: uppercase;
  }
  .dash-body {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }
  .history {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  .history th {
    text-align: left;
    color: var(--faint);
    font-weight: 500;
    font-size: 11px;
    letter-spacing: 1px;
    text-transform: uppercase;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  .history td {
    padding: 10px 12px;
    border-bottom: 1px solid var(--surface);
  }
  .ghost.small {
    padding: 4px 12px;
    font-size: 12px;
  }
  .reader-card {
    background: var(--bg);
    color: var(--fg);
    border-radius: 14px;
    padding: 32px;
    max-width: 72ch;
  }
</style>
