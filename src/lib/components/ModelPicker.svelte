<script lang="ts">
  /** Course-generation model selector with honest trade-off explanations.
   *  Quiz/grading always run on sonnet — this picks the course author. */
  let {
    value = $bindable('opus'),
  }: { value?: string } = $props();

  const MODELS = [
    {
      id: 'opus',
      name: 'OPUS',
      tag: 'deepest · slowest',
      desc: 'The strongest reasoning — richest courses with the best trade-off analysis. A course takes ~8-12 minutes to research and write, and uses the most of your plan quota.',
    },
    {
      id: 'sonnet',
      name: 'SONNET',
      tag: 'balanced',
      desc: 'Fast and strong — very good courses in ~3-6 minutes at a fraction of the quota. The sensible daily driver if you generate live often.',
    },
    {
      id: 'haiku',
      name: 'HAIKU',
      tag: 'fastest · lightest',
      desc: 'Quickest and cheapest. Courses will be shallower and resources less curated — fine when quota is tight or you mostly want the quiz loop.',
    },
  ];
</script>

<div class="mpicker">
  {#each MODELS as m}
    <button class="mdl mono" class:active={value === m.id} onclick={() => (value = m.id)}>
      <span class="mdl-name">{m.name}</span>
      <span class="mdl-tag">{m.tag}</span>
    </button>
  {/each}
</div>
{#each MODELS.filter((m) => m.id === value) as m}
  <p class="mdl-desc">{m.desc}</p>
{/each}
<p class="mdl-note mono">applies to course writing · quizzes and grading always use sonnet</p>

<style>
  .mpicker {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }
  .mdl {
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
  .mdl:hover {
    border-color: var(--muted);
  }
  .mdl.active {
    border-color: var(--accent);
    background: var(--surface-2);
  }
  .mdl-name {
    font-size: 11px;
    letter-spacing: 1.5px;
    color: var(--muted);
  }
  .mdl.active .mdl-name {
    color: var(--accent);
  }
  .mdl-tag {
    font-size: 8.5px;
    color: var(--faint);
    letter-spacing: 0.5px;
  }
  .mdl-desc {
    font-size: 12.5px;
    color: var(--muted);
    line-height: 1.55;
    margin: 10px 0 2px;
  }
  .mdl-note {
    font-size: 9.5px;
    color: var(--faint);
    letter-spacing: 0.5px;
    margin: 4px 0 0;
  }
</style>
