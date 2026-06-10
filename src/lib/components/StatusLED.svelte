<script lang="ts">
  let {
    tone = 'idle',
    label = '',
  }: { tone?: 'ok' | 'warn' | 'err' | 'idle' | 'pending'; label?: string } = $props();

  const colors: Record<string, string> = {
    ok: 'var(--led-ok)',
    warn: 'var(--led-warn)',
    err: 'var(--led-err)',
    idle: 'var(--led-idle)',
    pending: 'var(--led-warn)',
  };
</script>

<span class="led-wrap">
  <span class="led" class:pulse={tone === 'pending'} style="background: {colors[tone]};"></span>
  {#if label}<span class="led-label">{label}</span>{/if}
</span>

<style>
  .led-wrap {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .led {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    display: inline-block;
    flex-shrink: 0;
  }
  .led.pulse {
    animation: led-pulse 1.4s ease-in-out infinite;
  }
  .led-label {
    font-family: var(--font-mono);
    font-size: 13px;
    color: var(--fg);
  }
</style>
