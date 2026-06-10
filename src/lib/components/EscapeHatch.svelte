<script lang="ts">
  import { api } from '../ipc';
  import { app } from '../stores.svelte';

  let open = $state(false);
  let phrase = $state('');
  let typed = $state('');
  let error = $state('');

  async function reveal() {
    phrase = await api.getEscapePhrase();
    open = true;
  }

  async function attempt() {
    error = '';
    const ok = await api.escapeSession(typed).catch((e) => {
      error = String(e);
      return false;
    });
    if (ok) {
      open = false;
      await app.refresh();
    } else if (!error) {
      error = 'phrase does not match';
      typed = '';
    }
  }

  function noPaste(e: ClipboardEvent) {
    e.preventDefault();
  }
</script>

<div class="hatch">
  {#if !open}
    <button class="hatch-link" onclick={reveal}>emergency exit</button>
  {:else}
    <div class="hatch-panel">
      <p class="hatch-warn">Skipping marks today as failed and resets your streak.</p>
      <svg class="phrase-svg" viewBox="0 0 640 28" preserveAspectRatio="xMidYMid meet">
        <text x="320" y="19" text-anchor="middle">{phrase}</text>
      </svg>
      <input
        type="text"
        placeholder="Type the phrase above exactly"
        bind:value={typed}
        onpaste={noPaste}
        onkeydown={(e) => e.key === 'Enter' && attempt()}
      />
      <div class="hatch-actions">
        <button class="ghost" onclick={() => (open = false)}>never mind</button>
        <button class="ghost" onclick={attempt}>skip today</button>
      </div>
      {#if error}<p class="hatch-error">{error}</p>{/if}
    </div>
  {/if}
</div>

<style>
  .hatch {
    position: fixed;
    bottom: 14px;
    right: 18px;
    z-index: 50;
    max-width: 460px;
  }
  .hatch-link {
    background: none;
    border: none;
    color: var(--faint);
    font-size: 11px;
    cursor: pointer;
    opacity: 0.6;
  }
  .hatch-link:hover {
    opacity: 1;
  }
  .hatch-panel {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .hatch-warn {
    margin: 0;
    font-size: 12px;
    color: var(--bad-fg);
  }
  .phrase-svg {
    width: 100%;
    pointer-events: none;
  }
  .phrase-svg text {
    font-family: var(--font-mono);
    font-size: 13px;
    fill: var(--muted);
  }
  .hatch-actions {
    display: flex;
    justify-content: space-between;
  }
  .hatch-error {
    margin: 0;
    font-size: 12px;
    color: var(--bad-fg);
  }
</style>
