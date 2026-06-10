<script lang="ts">
  import { marked } from 'marked';
  import DOMPurify from 'dompurify';

  let { markdown = '', locked = false }: { markdown: string; locked?: boolean } = $props();

  const html = $derived(
    DOMPurify.sanitize(marked.parse(markdown, { async: false }) as string)
  );

  function interceptLinks(node: HTMLElement) {
    const handler = (e: Event) => {
      const a = (e.target as HTMLElement).closest('a');
      if (a) {
        e.preventDefault();
        // Links never navigate the kiosk webview; resources open post-session.
      }
    };
    node.addEventListener('click', handler);
    return { destroy: () => node.removeEventListener('click', handler) };
  }
</script>

<div class="md" class:locked use:interceptLinks>
  {@html html}
</div>

<style>
  .md {
    user-select: text;
    font-size: 16px;
    line-height: 1.8;
  }
  .md :global(h1),
  .md :global(h2),
  .md :global(h3) {
    font-family: var(--font-display);
    font-weight: 500;
    margin: 1.6em 0 0.5em;
    line-height: 1.3;
  }
  .md :global(h2) {
    font-size: 24px;
    border-bottom: 1px solid var(--border);
    padding-bottom: 8px;
  }
  .md :global(h3) {
    font-size: 19px;
  }
  .md :global(p) {
    margin: 0 0 1em;
  }
  .md :global(code) {
    font-family: var(--font-mono);
    font-size: 0.88em;
    background: var(--surface);
    padding: 2px 6px;
    border-radius: 5px;
  }
  .md :global(pre) {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 16px;
    overflow-x: auto;
  }
  .md :global(pre code) {
    background: none;
    padding: 0;
  }
  .md :global(a) {
    color: var(--accent);
    text-decoration: none;
    border-bottom: 1px dashed var(--accent);
    cursor: default;
  }
  .md :global(blockquote) {
    border-left: 3px solid var(--accent);
    margin: 1em 0;
    padding: 4px 0 4px 18px;
    color: var(--muted);
  }
  .md :global(ul),
  .md :global(ol) {
    padding-left: 1.4em;
  }
  .md :global(li) {
    margin-bottom: 0.4em;
  }
  .md :global(table) {
    border-collapse: collapse;
    margin: 1em 0;
  }
  .md :global(th),
  .md :global(td) {
    border: 1px solid var(--border);
    padding: 8px 12px;
    text-align: left;
  }
</style>
