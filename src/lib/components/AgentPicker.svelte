<script lang="ts">
  /** Primary CLI agent selector. CLAUDE gets the model picker and streaming
   *  logs; CODEX uses the OpenAI Codex CLI; CUSTOM accepts any binary that
   *  takes the prompt as its final argument and prints the answer. */
  let {
    agent = $bindable('claude'),
    customBin = $bindable(''),
  }: { agent?: string; customBin?: string } = $props();

  const AGENTS = [
    {
      id: 'claude',
      name: 'CLAUDE',
      tag: 'claude code cli',
      desc: 'Full support: model choice (opus/sonnet/haiku), web research for course resources, live agent log, JSON repair. Needs the claude CLI installed and authenticated.',
    },
    {
      id: 'codex',
      name: 'CODEX',
      tag: 'openai codex cli',
      desc: 'Runs courses through `codex exec`. Uses whatever model your codex CLI is configured with. No live log streaming; claude (if installed) remains the fallback repair engine.',
    },
    {
      id: 'custom',
      name: 'CUSTOM',
      tag: 'any cli — cursor, gemini…',
      desc: 'Any binary that accepts the prompt as its final argument and prints the answer to stdout (e.g. cursor-agent, gemini). Output should end with one fenced JSON block as instructed by the prompt.',
    },
  ];
</script>

<div class="apicker">
  {#each AGENTS as a}
    <button class="agt mono" class:active={agent === a.id} onclick={() => (agent = a.id)}>
      <span class="agt-name">{a.name}</span>
      <span class="agt-tag">{a.tag}</span>
    </button>
  {/each}
</div>
{#each AGENTS.filter((a) => a.id === agent) as a}
  <p class="agt-desc">{a.desc}</p>
{/each}
{#if agent === 'custom'}
  <input
    class="agt-bin mono"
    type="text"
    placeholder="/absolute/path/to/agent-binary"
    bind:value={customBin}
  />
{/if}

<style>
  .apicker {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
  .agt {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
    align-items: flex-start;
    background: var(--bg);
    border: 1px solid var(--node-border);
    border-radius: 7px;
    padding: 8px 11px;
    cursor: pointer;
    text-align: left;
  }
  .agt:hover {
    border-color: var(--muted);
  }
  .agt.active {
    border-color: var(--violet);
    background: var(--surface-2);
  }
  .agt-name {
    font-size: 11px;
    letter-spacing: 1.5px;
    color: var(--muted);
  }
  .agt.active .agt-name {
    color: var(--violet-fg);
  }
  .agt-tag {
    font-size: 8.5px;
    color: var(--faint);
    letter-spacing: 0.5px;
  }
  .agt-desc {
    font-size: 12.5px;
    color: var(--muted);
    line-height: 1.55;
    margin: 10px 0 2px;
  }
  .agt-bin {
    margin-top: 8px;
    font-size: 12px;
    background: var(--bg);
    border: 1px solid var(--node-border);
    border-radius: 6px;
    padding: 8px 12px;
  }
</style>
