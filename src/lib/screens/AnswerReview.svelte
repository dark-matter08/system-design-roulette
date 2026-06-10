<script lang="ts">
  import { api, type ReviewData } from '../ipc';
  import { app } from '../stores.svelte';
  import ClusterBar from '../components/ClusterBar.svelte';
  import NodeCard from '../components/NodeCard.svelte';
  import MetaBadge from '../components/MetaBadge.svelte';

  let { data = null }: { data?: ReviewData | null } = $props();

  let review = $state<ReviewData | null>(null);
  let idx = $state(0);
  let dwell = $state(10);

  const current = $derived(review?.items[idx]);
  const isLast = $derived(review ? idx === review.items.length - 1 : false);

  $effect(() => {
    if (data) {
      review = data;
    } else {
      api.getReview().then((r) => (review = r));
    }
  });

  $effect(() => {
    idx;
    dwell = app.state?.debug_day ? 1 : 10;
    const id = setInterval(() => {
      dwell = Math.max(0, dwell - 1);
      if (dwell === 0) clearInterval(id);
    }, 1000);
    return () => clearInterval(id);
  });

  async function next() {
    if (!review) return;
    if (isLast) {
      await api.finishReview();
      await app.refresh();
    } else {
      idx += 1;
    }
  }
</script>

<div class="review-wrap blueprint">
  <ClusterBar route="quiz/trace" status="responses graded" tone="ok" />
  {#if !review}
    <div class="center"><p class="sub mono">loading trace…</p></div>
  {:else if review.items.length === 0}
    <div class="center"><p class="sub mono">no requests today</p></div>
  {:else if current}
    <div class="review-body">
      <div class="head-row">
        <span class="mono head-meta">
          trace {idx + 1}/{review.items.length} · error budget {Math.round(review.score * 100)}%
        </span>
        {#if review.self_assess}
          <MetaBadge tone="amber">{#snippet children()}⚠ grader offline — self-assess{/snippet}</MetaBadge>
        {/if}
      </div>

      <NodeCard
        icon={current.correct === false ? '✗' : current.correct === true ? '✓' : '◌'}
        name={`response trace — /quiz/${idx + 1}`}
        badge={current.correct === true
          ? '200 OK'
          : current.correct === false
            ? '422 → DLQ · retries tomorrow'
            : 'ungraded — self-assess'}
        badgeTone={current.correct === true ? 'teal' : current.correct === false ? 'red' : 'amber'}
        accent={current.correct === false ? 'var(--led-err)' : current.correct === true ? '#2b4a3f' : 'var(--led-warn)'}
      >
        {#snippet children()}
          <h2 class="prompt">{current.prompt}</h2>
          <div class="trace">
            <div class="trace-line">
              <span class="trace-key mono">your_answer</span>
              <p>{current.user_answer || '(blank)'}</p>
            </div>
            <div class="trace-line">
              <span class="trace-key mono">expected</span>
              <p>{current.correct_answer}</p>
            </div>
            {#if current.feedback}
              <div class="trace-line">
                <span class="trace-key mono">grader_log</span>
                <p>{current.feedback}</p>
              </div>
            {/if}
            <div class="trace-line explain">
              <span class="trace-key mono">why</span>
              <p>{current.explanation}</p>
            </div>
          </div>
        {/snippet}
      </NodeCard>

      <div class="actions">
        <button class="cta mono-cta" onclick={next} disabled={dwell > 0}>
          {dwell > 0 ? `read · ${dwell}s` : isLast ? '→ to the wheel' : '→ next trace'}
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .review-wrap {
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
  .sub {
    color: var(--faint);
    font-size: 12px;
  }
  .review-body {
    width: min(780px, 92vw);
    margin: 0 auto;
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding: 24px;
    gap: 14px;
  }
  .head-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .head-meta {
    font-size: 11px;
    color: var(--faint);
  }
  .prompt {
    font-size: 17px;
    line-height: 1.45;
    margin-bottom: 14px;
  }
  .trace {
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-height: 50vh;
    overflow-y: auto;
  }
  .trace-line {
    display: grid;
    grid-template-columns: 110px 1fr;
    gap: 12px;
    align-items: baseline;
  }
  .trace-key {
    font-size: 10px;
    color: var(--faint);
    letter-spacing: 0.5px;
  }
  .trace-line p {
    margin: 0;
    font-size: 14px;
  }
  .explain p {
    color: var(--muted);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
