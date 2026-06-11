<script lang="ts">
  import { api, type QuizQuestionView, type ReviewData } from '../ipc';
  import { app } from '../stores.svelte';
  import ClusterBar from '../components/ClusterBar.svelte';
  import NodeCard from '../components/NodeCard.svelte';
  import StatusLED from '../components/StatusLED.svelte';
  import { Inbox, TriangleAlert, Zap, ArrowUp } from 'lucide-svelte';

  let questions = $state<QuizQuestionView[]>([]);
  let idx = $state(0);
  let answer = $state('');
  let loading = $state(true);
  let grading = $state(false);

  let { onreview }: { onreview?: (data: ReviewData) => void } = $props();

  const current = $derived(questions[idx]);
  const progress = $derived(questions.length ? (idx / questions.length) * 100 : 0);
  const isAudit = $derived(app.session?.session_type === 'pop_quiz');

  $effect(() => {
    api.getQuiz().then((qs) => {
      questions = qs;
      const firstUnanswered = qs.findIndex((q) => !q.answered);
      idx = firstUnanswered === -1 ? qs.length : firstUnanswered;
      loading = false;
      if (qs.length === 0) {
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

<div class="quiz-wrap blueprint">
  <ClusterBar
    route={isAudit ? 'audit' : 'quiz'}
    status={isAudit ? 'surprise compliance check · no new topic today' : 'session locked'}
    tone="warn"
  />
  {#if loading}
    <div class="center"><StatusLED tone="pending" label="loading requests…" /></div>
  {:else if grading}
    <div class="center">
      <StatusLED tone="pending" label="grading in flight" />
      <p class="sub mono">free-text answers dispatched to agent-backend · rubric grading</p>
    </div>
  {:else if current}
    <div class="quiz-body">
      <div class="progress-track"><div class="progress-fill" style="width: {progress}%"></div></div>
      <div class="spacer"></div>
      {#if isAudit && app.session?.plan_reason}
        <div class="audit-note mono"><Zap size={11} /> POP QUIZ — {app.session.plan_reason}</div>
      {/if}
      <NodeCard
        Icon={Inbox}
        name={`incoming request — POST /quiz/${idx + 1} of ${questions.length}`}
        badge={current.origin === 'carryover' ? 'retry · from DLQ' : current.kind === 'mcq' ? 'multiple choice' : 'free text'}
        badgeTone={current.origin === 'carryover' ? 'amber' : 'muted'}
        accent={current.origin === 'carryover' ? 'var(--led-warn)' : 'var(--node-border)'}
      >
        {#snippet children()}
          {#if current.origin === 'carryover'}
            <div class="dlq-note mono"><TriangleAlert size={11} /> you failed this before — it returns until you pass it</div>
          {/if}
          <h2 class="prompt">{current.prompt}</h2>
          {#if current.kind === 'mcq' && current.choices}
            <div class="choices">
              {#each current.choices as choice, i}
                <button class="choice" class:selected={answer === choice} onclick={() => (answer = choice)}>
                  <span class="choice-key mono">{String.fromCharCode(65 + i)}</span>
                  {choice}
                </button>
              {/each}
            </div>
          {:else}
            <textarea rows="4" placeholder="2-4 sentences — graded against a rubric" bind:value={answer}></textarea>
          {/if}
          <div class="actions">
            <button class="cta mono-cta" onclick={submit} disabled={!answer.trim()}>
              <ArrowUp size={13} />{idx === questions.length - 1 ? 'send & grade all' : 'send response'}
            </button>
          </div>
        {/snippet}
      </NodeCard>
    </div>
  {/if}
</div>

<style>
  .quiz-wrap {
    flex: 1;
    display: flex;
    flex-direction: column;
    animation: fade-in 0.35s ease;
  }
  .center {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
  }
  .sub {
    color: var(--faint);
    font-size: 11px;
  }
  .quiz-body {
    width: min(760px, 92vw);
    margin: 0 auto;
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding: 24px;
  }
  .progress-track {
    border-radius: 3px;
    overflow: hidden;
  }
  .spacer {
    height: 18px;
  }
  .audit-note {
    font-size: 11px;
    color: var(--violet-fg);
    background: var(--violet-bg);
    border: 1px dashed var(--violet);
    border-radius: 6px;
    padding: 7px 12px;
    margin-bottom: 14px;
    align-self: flex-start;
  }
  .dlq-note {
    font-size: 11px;
    color: var(--warn-fg);
    background: var(--warn-bg);
    border-radius: 5px;
    padding: 6px 10px;
    margin-bottom: 12px;
    display: inline-block;
  }
  .prompt {
    font-size: 20px;
    line-height: 1.5;
    margin: 6px 0 18px;
  }
  .choices {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .choice {
    text-align: left;
    background: var(--bg);
    border: 1px solid var(--node-border);
    color: var(--fg);
    border-radius: 8px;
    padding: 12px 14px;
    font-family: var(--font-body);
    font-size: 14px;
    cursor: pointer;
    transition: border-color 0.15s ease;
    display: flex;
    gap: 12px;
    align-items: baseline;
  }
  .choice:hover {
    border-color: var(--muted);
  }
  .choice.selected {
    border-color: var(--accent);
    background: var(--surface-2);
  }
  .choice-key {
    font-size: 11px;
    color: var(--faint);
    border: 1px solid var(--node-border);
    border-radius: 4px;
    padding: 1px 7px;
    flex-shrink: 0;
  }
  .choice.selected .choice-key {
    color: var(--accent);
    border-color: var(--accent);
  }
  textarea {
    background: var(--bg);
    border-color: var(--node-border);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 16px;
  }
</style>
