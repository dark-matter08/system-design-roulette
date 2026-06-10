<script lang="ts">
  import { api, type ReviewData } from '../ipc';
  import { app } from '../stores.svelte';

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
    // min-dwell per question before Next enables
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

<div class="screen">
  {#if !review}
    <p class="sub">Loading results…</p>
  {:else if review.items.length === 0}
    <p class="sub">No quiz today.</p>
  {:else if current}
    <div class="review">
      <div class="review-head">
        <span class="sub">
          Results · {Math.round(review.score * 100)}% ·
          {idx + 1} of {review.items.length}
        </span>
        {#if review.self_assess}
          <span class="badge warn">grader offline — self-assess</span>
        {/if}
      </div>

      <div
        class="card"
        style="border-left-color: {current.correct === false
          ? 'var(--bad-fg)'
          : current.correct === true
            ? 'var(--ok-fg)'
            : 'var(--warn-fg)'}"
      >
        <div class="card-head">
          <h2 class="prompt">{current.prompt}</h2>
          {#if current.correct === true}
            <span class="badge ok">correct</span>
          {:else if current.correct === false}
            <span class="badge bad">returns tomorrow</span>
          {:else}
            <span class="badge warn">self-assess</span>
          {/if}
        </div>

        <div class="block">
          <span class="block-label">your answer</span>
          <p>{current.user_answer || '(blank)'}</p>
        </div>
        <div class="block">
          <span class="block-label">correct answer</span>
          <p>{current.correct_answer}</p>
        </div>
        {#if current.feedback}
          <div class="block">
            <span class="block-label">grader feedback</span>
            <p>{current.feedback}</p>
          </div>
        {/if}
        <div class="block explain">
          <span class="block-label">why</span>
          <p>{current.explanation}</p>
        </div>
      </div>

      <div class="review-actions">
        <button class="cta" onclick={next} disabled={dwell > 0}>
          {dwell > 0
            ? `read for ${dwell}s…`
            : isLast
              ? 'To the wheel'
              : 'Next question'}
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .review {
    width: min(760px, 92vw);
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .review-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .sub {
    color: var(--muted);
    font-size: 13px;
  }
  .card {
    background: var(--surface);
    border-left: 3px solid var(--border);
    border-radius: 0 12px 12px 0;
    padding: 22px 24px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-height: 64vh;
    overflow-y: auto;
  }
  .card-head {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    align-items: flex-start;
  }
  .prompt {
    font-size: 18px;
    line-height: 1.45;
  }
  .block {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .block-label {
    font-size: 10px;
    letter-spacing: 2px;
    text-transform: uppercase;
    color: var(--faint);
  }
  .block p {
    margin: 0;
    font-size: 14px;
    color: var(--fg);
  }
  .explain p {
    color: var(--muted);
  }
  .review-actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
