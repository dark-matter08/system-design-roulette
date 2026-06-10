<script lang="ts">
  /**
   * Two-host audio lesson player. Engine 'vibevoice' plays rendered per-line
   * files via the asset protocol; engine 'speech' uses the webview's native
   * speechSynthesis with two distinct system voices. Zero network either way.
   */
  import { onDestroy } from 'svelte';
  import { isTauri } from '../ipc';

  interface Line {
    speaker: string;
    text: string;
    file?: string | null;
  }
  let { lines, engine }: { lines: Line[]; engine: string } = $props();

  let idx = $state(0);
  let playing = $state(false);
  let rate = $state(1.0);
  const RATES = [0.8, 1.0, 1.2, 1.5];

  let audioEl: HTMLAudioElement | null = null;
  let convertFileSrc: ((p: string) => string) | null = null;
  if (isTauri) {
    import('@tauri-apps/api/core').then((m) => (convertFileSrc = m.convertFileSrc));
  }

  function voices(): { teacher: SpeechSynthesisVoice | null; student: SpeechSynthesisVoice | null } {
    const all = speechSynthesis.getVoices().filter((v) => v.lang.startsWith('en'));
    const pick = (names: string[]) => all.find((v) => names.some((n) => v.name.includes(n))) ?? null;
    return {
      teacher: pick(['Daniel', 'Aaron', 'Alex']) ?? all[0] ?? null,
      student: pick(['Samantha', 'Karen', 'Moira']) ?? all[1] ?? all[0] ?? null,
    };
  }

  function playLine(i: number) {
    if (i >= lines.length) {
      playing = false;
      idx = lines.length - 1;
      return;
    }
    idx = i;
    const line = lines[i];
    if (engine === 'vibevoice' && line.file && convertFileSrc) {
      audioEl?.pause();
      audioEl = new Audio(convertFileSrc(line.file));
      audioEl.playbackRate = rate;
      audioEl.onended = () => playing && playLine(i + 1);
      audioEl.play().catch(() => speakLine(i));
    } else {
      speakLine(i);
    }
  }

  function speakLine(i: number) {
    speechSynthesis.cancel();
    const line = lines[i];
    const u = new SpeechSynthesisUtterance(line.text);
    const v = voices();
    u.voice = line.speaker === 'teacher' ? v.teacher : v.student;
    u.rate = rate;
    u.onend = () => playing && playLine(i + 1);
    speechSynthesis.speak(u);
  }

  function toggle() {
    if (playing) {
      playing = false;
      speechSynthesis.cancel();
      audioEl?.pause();
    } else {
      playing = true;
      playLine(idx);
    }
  }

  function skip(d: number) {
    const next = Math.min(Math.max(idx + d, 0), lines.length - 1);
    speechSynthesis.cancel();
    audioEl?.pause();
    if (playing) playLine(next);
    else idx = next;
  }

  function cycleRate() {
    rate = RATES[(RATES.indexOf(rate) + 1) % RATES.length];
    if (playing) playLine(idx); // restart current line at the new rate
  }

  onDestroy(() => {
    speechSynthesis.cancel();
    audioEl?.pause();
  });
</script>

<div class="player">
  <div class="controls">
    <button class="pbtn mono" onclick={() => skip(-1)} aria-label="previous line">⏮</button>
    <button class="pbtn main mono" onclick={toggle}>{playing ? '⏸' : '▶'}</button>
    <button class="pbtn mono" onclick={() => skip(1)} aria-label="next line">⏭</button>
    <button class="pbtn mono rate" onclick={cycleRate}>{rate}×</button>
  </div>
  <div class="now">
    <span class="who mono" class:student={lines[idx]?.speaker === 'student'}>
      {lines[idx]?.speaker === 'student' ? '🎓 student' : '👨‍🏫 teacher'}
    </span>
    <span class="line-text">{lines[idx]?.text}</span>
  </div>
  <div class="meta mono">
    <span>{idx + 1}/{lines.length}</span>
    <span class="eng">{engine === 'vibevoice' ? '◉ vibevoice' : '◉ system speech'}</span>
  </div>
</div>

<style>
  .player {
    display: flex;
    align-items: center;
    gap: 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 8px 14px;
    margin: 14px 24px 0;
  }
  .controls {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }
  .pbtn {
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--fg);
    border-radius: 6px;
    font-size: 12px;
    padding: 5px 10px;
    cursor: pointer;
  }
  .pbtn.main {
    border-color: var(--accent);
    color: var(--accent);
    font-size: 13px;
  }
  .pbtn.rate {
    font-size: 10px;
    color: var(--muted);
  }
  .pbtn:hover {
    border-color: var(--muted);
  }
  .now {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  .who {
    font-size: 10px;
    color: var(--accent);
    white-space: nowrap;
    flex-shrink: 0;
  }
  .who.student {
    color: var(--muted);
  }
  .line-text {
    font-size: 13px;
    color: var(--fg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    font-size: 10px;
    color: var(--faint);
    display: flex;
    gap: 12px;
    flex-shrink: 0;
  }
</style>
