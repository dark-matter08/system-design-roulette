<script lang="ts">
  import '../lib/theme.css';
  import { app } from '../lib/stores.svelte';
  import type { ReviewData } from '../lib/ipc';
  import SetupWizard from '../lib/screens/SetupWizard.svelte';
  import Idle from '../lib/screens/Idle.svelte';
  import Quiz from '../lib/screens/Quiz.svelte';
  import AnswerReview from '../lib/screens/AnswerReview.svelte';
  import Roulette from '../lib/screens/Roulette.svelte';
  import CourseReader from '../lib/screens/CourseReader.svelte';
  import Completion from '../lib/screens/Completion.svelte';
  import Dashboard from '../lib/screens/Dashboard.svelte';
  import EscapeHatch from '../lib/components/EscapeHatch.svelte';

  let reviewData = $state<ReviewData | null>(null);
  const isBlanker =
    typeof location !== 'undefined' && new URLSearchParams(location.search).has('blanker');

  $effect(() => {
    if (!isBlanker) app.init();
  });

  // Block common quit/close shortcuts while locked.
  function onKeydown(e: KeyboardEvent) {
    if (app.session?.locked && e.metaKey && ['q', 'w', 'h', 'm'].includes(e.key.toLowerCase())) {
      e.preventDefault();
      e.stopPropagation();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} oncontextmenu={(e) => app.session?.locked && e.preventDefault()} />

<div id="app-root" class="theme-noir">
  {#if isBlanker}
    <div style="flex: 1; background: #000;"></div>
  {:else if app.screen === 'loading'}
    <div class="screen"><p>…</p></div>
  {:else if app.screen === 'setup'}
    <SetupWizard />
  {:else if app.screen === 'idle'}
    <Idle />
  {:else if app.screen === 'quiz'}
    <Quiz onreview={(d) => (reviewData = d)} />
  {:else if app.screen === 'review'}
    <AnswerReview data={reviewData} />
  {:else if app.screen === 'roulette'}
    <Roulette />
  {:else if app.screen === 'course'}
    <CourseReader />
  {:else if app.screen === 'completion'}
    <Completion />
  {:else if app.screen === 'dashboard'}
    <Dashboard />
  {/if}

  {#if app.session?.status === 'in_progress'}
    <EscapeHatch />
  {/if}

  {#if app.error}
    <div class="error-toast" role="alert">
      {app.error}
      <button onclick={() => (app.error = '')}>×</button>
    </div>
  {/if}
</div>

<style>
  .error-toast {
    position: fixed;
    bottom: 16px;
    left: 16px;
    background: var(--bad-bg);
    color: var(--bad-fg);
    border-radius: 10px;
    padding: 10px 14px;
    font-size: 13px;
    display: flex;
    gap: 12px;
    align-items: center;
    max-width: 480px;
    z-index: 60;
  }
  .error-toast button {
    background: none;
    border: none;
    color: inherit;
    font-size: 16px;
    cursor: pointer;
  }
</style>
