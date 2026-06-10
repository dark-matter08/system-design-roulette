<script lang="ts">
  import { api, type CourseView } from '../ipc';
  import { app } from '../stores.svelte';
  import Markdown from '../components/Markdown.svelte';
  import TimerBar from '../components/TimerBar.svelte';

  let course = $state<CourseView | null>(null);

  const remaining = $derived(
    app.timerRemaining >= 0 ? app.timerRemaining : (course?.remaining_seconds ?? 1)
  );
  const done = $derived(remaining <= 0);

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
        <h2>{course.title}</h2>
        {#if course.source === 'fallback'}
          <span class="badge warn">bundled course — agent was unreachable</span>
        {/if}
      </div>
      <TimerBar {remaining} total={course.total_seconds} />
    </header>
    <div class="reader-body">
      <article class="column">
        <Markdown markdown={course.markdown} locked />
        {#if course.resources.length > 0}
          <section class="resources">
            <h3>Reading list ({course.resources.length})</h3>
            <p class="fine">
              Links unlock after the session — they'll open in your browser from the
              completion screen.
            </p>
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
          <button class="cta" onclick={finish} disabled={!done}>
            {done ? 'Complete session' : 'Keep reading — timer running'}
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
    padding: 16px 24px 0;
  }
  .title-row h2 {
    font-size: 20px;
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
    font-size: 12px;
    color: var(--faint);
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
