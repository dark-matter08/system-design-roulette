<script lang="ts">
  /** Primary CLI agent selector. CLAUDE gets the model picker, web research
   *  and streaming logs; codex/cursor/gemini are first-class with their
   *  correct non-interactive invocations; CUSTOM runs any other CLI. */
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
      desc: 'Runs `codex exec` and reads the final message. Uses whatever model your codex CLI is configured with. Needs codex installed and a valid ~/.codex/config.toml. Claude (if present) is the JSON-repair fallback.',
    },
    {
      id: 'cursor',
      name: 'CURSOR',
      tag: 'cursor-agent cli',
      desc: 'Runs `cursor-agent -p --output-format text`. Needs cursor-agent installed and authenticated (`cursor-agent login` or CURSOR_API_KEY).',
    },
    {
      id: 'gemini',
      name: 'GEMINI',
      tag: 'google gemini cli',
      desc: 'Runs `gemini -p`. Needs the gemini CLI installed and authenticated.',
    },
    {
      id: 'custom',
      name: 'CUSTOM',
      tag: 'any other cli',
      desc: 'Any CLI that prints the answer to stdout. Use {prompt} in the command to place the prompt (lets you add flags); without it the prompt is appended as the final argument.',
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
    placeholder={'e.g. /usr/local/bin/mycli --print {prompt}'}
    bind:value={customBin}
  />
{/if}

<style>
  .apicker {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    margin-top: 8px;
  }
  .agt {
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
