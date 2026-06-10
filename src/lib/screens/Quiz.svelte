<script lang="ts">
  import { api, type QuizQuestionView, type ReviewData } from '../ipc';
  import { app } from '../stores.svelte';

  let questions = $state<QuizQuestionView[]>([]);
  let idx = $state(0);
  let answer = $state('');
  let loading = $state(true);
  let grading = $state(false);

  let { onreview }: { onreview?: (data: ReviewData) => void } = $props();

  const current = $derived(questions[idx]);
  const progress = $derived(questions.length ? (idx / questions.length) * 100 : 0);

  $effect(() => {
    api.getQuiz().then((qs) => {
      questions = qs;
      // resume at first unanswered question
      const firstUnanswered = qs.findIndex((q) => !q.answered);
      idx = firstUnanswered === -1 ? qs.length : firstUnanswered;
      loading = false;
      if (qs.length === 0) {
        // nothing to quiz (day 1) — go straight to roulette
        api.finishReview().then(() => app.refresh());
      }
    });
  });

  async function submit() {
    if (!current || !answer.trim()) return;
    await api.submitAnswer(current.id, answer.trim());
    answer = '';
    idx += 1;
    if (idx >= questions.length) {
      grading = true;
      const review = await api.finishQuiz();
      grading = false;
      onreview?.(review);
      await app.refresh();
    }
  }
</script>

<div class="screen">
  {#if loading}
    <p class="sub">Loading quiz…</p>
  {:else if grading}
    <span class="kicker">grading</span>
    <h1>Checking your answers…</h1>
    <p class="sub">Free-text answers are graded by your agent. Hold on.</p>
    <div class="spinner"></div>
  {:else if current}
    <div class="quiz">
      <div class="quiz-head">
        <span class="sub">Question {idx + 1} of {questions.length}</span>
        {#if current.origin === 'carryover'}
          <span class="badge warn">carried over — you missed this before</span>
        {/if}
      </div>
      <div class="progress-track"><div class="progress-fill" style="width: {progress}%"></div></div>
      <h2 class="prompt">{current.prompt}</h2>
      {#if current.kind === 'mcq' && current.choices}
        <div class="choices">
          {#each current.choices as choice}
            <button
              class="choice"
              class:selected={answer === choice}
              onclick={() => (answer = choice)}
            >
              {choice}
            </button>
          {/each}
        </div>
      {:else}
        <textarea
          rows="4"
          placeholder="Type your answer — 2 to 4 sentences"
          bind:value={answer}
        ></textarea>
      {/if}
      <div class="quiz-actions">
        <button class="cta" onclick={submit} disabled={!answer.trim()}>
          {idx === questions.length - 1 ? 'Submit & grade' : 'Submit answer'}
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .quiz {
    width: min(720px, 90vw);
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .quiz-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .sub {
    color: var(--muted);
    font-size: 13px;
  }
  .prompt {
    font-size: 22px;
    line-height: 1.5;
    margin-top: 10px;
  }
  .choices {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .choice {
    text-align: left;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--fg);
    border-radius: 10px;
    padding: 14px 16px;
    font-family: var(--font-body);
    font-size: 14px;
    cursor: pointer;
    transition: border-color 0.15s ease;
  }
  .choice:hover {
    border-color: var(--muted);
  }
  .choice.selected {
    border-color: var(--accent);
    background: var(--surface-2);
  }
  .quiz-actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 6px;
  }
  .spinner {
    width: 28px;
    height: 28px;
    margin-top: 24px;
    border: 3px solid var(--surface-2);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
