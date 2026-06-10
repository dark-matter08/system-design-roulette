<script lang="ts">
  import { api, type CourseView } from '../ipc';
  import { app } from '../stores.svelte';
  import Markdown from '../components/Markdown.svelte';

  let course = $state<CourseView | null>(null);

  const remaining = $derived(
    app.timerRemaining >= 0 ? app.timerRemaining : (course?.remaining_seconds ?? 1)
  );
  const done = $derived(remaining <= 0);
  const mm = $derived(Math.floor(Math.max(remaining, 0) / 60));
  const ss = $derived(Math.max(remaining, 0) % 60);
  const pct = $derived(
    course && course.total_seconds > 0
      ? ((course.total_seconds - remaining) / course.total_seconds) * 100
      : 0
  );

  $effect(() => {
    api.ensureCourse().then((c) => (course = c));
  });

  async function finish() {
    try {
      await api.finishCourse();
      await app.refresh();
    } catch (e) {
      app.error = String(e);
    }
  }
</script>

<div class="reader theme-scholar">
  {#if !course}
    <div class="screen"><p>Loading course…</p></div>
  {:else}
    <header class="reader-head">
      <div class="title-row">
        <div class="title-left">
          <span class="node-tag mono">📖 course-reader</span>
          <h2>{course.title}</h2>
          {#if course.source === 'fallback'}
            <span class="badge warn">bundled — agent was unreachable</span>
          {/if}
        </div>
        <span class="ttl mono" class:done>
          {done ? 'TTL 0 — unlocked' : `TTL ${mm}:${String(ss).padStart(2, '0')}`}
        </span>
      </div>
      <div class="progress-track"><div class="progress-fill" style="width: {pct}%"></div></div>
    </header>
    <div class="reader-body">
      <article class="column">
        <Markdown markdown={course.markdown} locked />
        {#if course.resources.length > 0}
          <section class="resources">
            <h3>Reading list ({course.resources.length})</h3>
            <p class="fine mono">egress queue — links unlock after the session completes</p>
            <ul>
              {#each course.resources as r}
                <li>
                  <span class="r-title">{r.title}</span>
                  {#if r.why}<span class="r-why"> — {r.why}</span>{/if}
                </li>
              {/each}
            </ul>
          </section>
        {/if}
        <div class="finish-row">
          <button class="cta mono-cta" onclick={finish} disabled={!done}>
            {done ? '✓ complete session' : 'keep reading — TTL running'}
          </button>
        </div>
      </article>
    </div>
  {/if}
</div>

<style>
  .reader {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    color: var(--fg);
    min-height: 0;
    animation: fade-in 0.5s ease;
  }
  .reader-head {
    border-bottom: 1px solid var(--border);
    background: var(--bg);
  }
  .title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 14px 24px 10px;
  }
  .title-left {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .node-tag {
    font-size: 10px;
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 3px 8px;
    letter-spacing: 0.5px;
  }
  .title-row h2 {
    font-size: 19px;
  }
  .ttl {
    font-size: 14px;
    color: var(--accent);
    white-space: nowrap;
  }
  .ttl.done {
    color: var(--ok-fg);
  }
  .reader-body {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }
  .column {
    max-width: 68ch;
    margin: 0 auto;
    padding: 32px 24px 80px;
  }
  .resources {
    margin-top: 40px;
    border-top: 1px solid var(--border);
    padding-top: 16px;
  }
  .resources h3 {
    font-size: 18px;
  }
  .fine {
    font-size: 10px;
    color: var(--faint);
    letter-spacing: 0.5px;
  }
  .r-title {
    font-weight: 600;
  }
  .r-why {
    color: var(--muted);
  }
  .finish-row {
    margin-top: 48px;
    display: flex;
    justify-content: center;
  }
</style>
