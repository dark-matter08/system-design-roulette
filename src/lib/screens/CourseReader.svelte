<script lang="ts">
  import { api, type CourseView, type ExitQuizQuestion } from '../ipc';
  import { app } from '../stores.svelte';
  import Markdown from '../components/Markdown.svelte';

  let course = $state<CourseView | null>(null);

  const remaining = $derived(
    app.timerRemaining >= 0 ? app.timerRemaining : (course?.remaining_seconds ?? 1)
  );
  const done = $derived(remaining <= 0);
  const mm = $derived(Math.floor(Math.max(remaining, 0) / 60));
  const ss = $derived(Math.max(remaining, 0) % 60);
  const pct = $derived(
    course && course.total_seconds > 0
      ? ((course.total_seconds - remaining) / course.total_seconds) * 100
      : 0
  );

  // ---- reading prefs ----
  let fontSize = $state(Number(localStorage.getItem('sdr-font-size')) || 15);
  function bumpFont(d: number) {
    fontSize = Math.min(21, Math.max(13, fontSize + d));
    localStorage.setItem('sdr-font-size', String(fontSize));
  }

  // ---- TOC + scrollspy ----
  interface TocEntry {
    id: string;
    label: string;
    read: boolean;
    active: boolean;
  }
  let toc = $state<TocEntry[]>([]);
  let bodyEl = $state<HTMLElement | null>(null);
  let scrollPct = $state(0);

  function buildToc() {
    if (!bodyEl) return;
    const hs = [...bodyEl.querySelectorAll('article h2')] as HTMLElement[];
    hs.forEach((h, i) => (h.id = h.id || `sec-${i}`));
    toc = hs.map((h) => ({ id: h.id, label: h.textContent ?? '', read: false, active: false }));
    onScroll();
  }

  function onScroll() {
    if (!bodyEl) return;
    const max = bodyEl.scrollHeight - bodyEl.clientHeight;
    scrollPct = max > 0 ? (bodyEl.scrollTop / max) * 100 : 100;
    const fold = bodyEl.scrollTop + bodyEl.clientHeight * 0.35;
    let activeIdx = 0;
    toc.forEach((t, i) => {
      const el = bodyEl!.querySelector(`#${CSS.escape(t.id)}`) as HTMLElement | null;
      if (!el) return;
      if (el.offsetTop < fold) activeIdx = i;
      // A section counts as read once you've scrolled past its end.
      const next = toc[i + 1] && (bodyEl!.querySelector(`#${CSS.escape(toc[i + 1].id)}`) as HTMLElement | null);
      const end = next ? next.offsetTop : bodyEl!.scrollHeight - 40;
      if (bodyEl!.scrollTop + bodyEl!.clientHeight >= end) t.read = true;
    });
    toc.forEach((t, i) => (t.active = i === activeIdx));
  }

  function jump(id: string) {
    bodyEl?.querySelector(`#${CSS.escape(id)}`)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }

  $effect(() => {
    api.ensureCourse().then((c) => {
      course = c;
      // Markdown renders next tick; build the TOC after paint.
      setTimeout(buildToc, 80);
    });
  });

  // ---- exit check ("prove it") ----
  let exitOpen = $state(false);
  let exitQs = $state<ExitQuizQuestion[]>([]);
  let exitAnswers = $state<Record<number, string>>({});
  let exitLoading = $state(false);
  let exitMsg = $state('');
  let cooldown = $state(0);

  async function openExit() {
    exitOpen = true;
    exitMsg = '';
    if (exitQs.length === 0) {
      exitLoading = true;
      try {
        exitQs = await api.getExitQuiz();
      } catch (e) {
        exitMsg = String(e);
      } finally {
        exitLoading = false;
      }
    }
  }

  async function submitExit() {
    exitMsg = '';
    try {
      const r = await api.submitExitQuiz(exitAnswers);
      if (r.passed) {
        exitOpen = false;
      } else if (r.cooldown_seconds > 0 && r.correct.length === 0 && Object.keys(exitAnswers).length === 0) {
        startCooldown(r.cooldown_seconds);
      } else {
        exitMsg = `${r.correct.length}/${exitQs.length} — the breaker holds. keep reading.`;
        startCooldown(r.cooldown_seconds);
        exitAnswers = {};
      }
    } catch (e) {
      exitMsg = String(e);
    }
  }

  function startCooldown(s: number) {
    cooldown = s;
    const id = setInterval(() => {
      cooldown -= 1;
      if (cooldown <= 0) clearInterval(id);
    }, 1000);
  }

  async function finish() {
    try {
      await api.finishCourse();
      await app.refresh();
    } catch (e) {
      app.error = String(e);
    }
  }
</script>

<div class="reader theme-scholar">
  {#if !course}
    <div class="screen"><p>Loading course…</p></div>
  {:else}
    <header class="reader-head">
      <div class="title-row">
        <div class="title-left">
          <span class="node-tag mono">📖 course-reader</span>
          <h2>{course.title}</h2>
          {#if course.source === 'fallback'}
            <span class="badge warn">bundled — agent was unreachable</span>
          {/if}
        </div>
        <div class="head-right">
          <div class="font-ctl mono">
            <button onclick={() => bumpFont(-1)} aria-label="smaller text">A−</button>
            <button onclick={() => bumpFont(1)} aria-label="larger text">A+</button>
          </div>
          {#if !done}
            <button class="ghost mono-ghost prove" onclick={openExit}>⚡ prove it — exit early</button>
          {/if}
          <span class="ttl mono" class:done>
            {done ? 'TTL 0 — unlocked' : `TTL ${mm}:${String(ss).padStart(2, '0')}`}
          </span>
        </div>
      </div>
      <div class="progress-track"><div class="progress-fill" style="width: {pct}%"></div></div>
    </header>

    <div class="reader-layout">
      <nav class="toc">
        <div class="toc-label mono">SECTIONS · {Math.round(scrollPct)}% scrolled</div>
        {#each toc as t}
          <button class="toc-item" class:active={t.active} onclick={() => jump(t.id)}>
            <span class="toc-check mono">{t.read ? '✓' : '·'}</span>
            {t.label}
          </button>
        {/each}
      </nav>

      <div class="reader-body" bind:this={bodyEl} onscroll={onScroll}>
        <article class="column" style="font-size: {fontSize}px">
          <Markdown markdown={course.markdown} locked />
          {#if course.resources.length > 0}
            <section class="resources">
              <h3>Reading list ({course.resources.length})</h3>
              <p class="fine mono">egress queue — links unlock after the session completes</p>
              <ul>
                {#each course.resources as r}
                  <li>
                    <span class="r-title">{r.title}</span>
                    {#if r.why}<span class="r-why"> — {r.why}</span>{/if}
                  </li>
                {/each}
              </ul>
            </section>
          {/if}
          <div class="finish-row">
            <button class="cta mono-cta" onclick={finish} disabled={!done}>
              {done ? '✓ complete session' : 'keep reading — TTL running'}
            </button>
          </div>
        </article>
      </div>
    </div>

    {#if exitOpen}
      <div class="exit-overlay">
        <div class="exit-panel">
          <div class="exit-head mono">
            <span class="exit-tag">EXIT CHECK</span>
            <span>3/3 correct unlocks the TTL — prove you absorbed today's course</span>
            <button class="exit-close" onclick={() => (exitOpen = false)}>×</button>
          </div>
          {#if exitLoading}
            <p class="mono dim">generating exit check from today's course…</p>
          {:else if cooldown > 0}
            <p class="mono cool">⏳ breaker cooling down — retry in {cooldown}s</p>
          {:else}
            {#each exitQs as q, qi}
              <div class="exit-q">
                <p class="exit-prompt"><span class="mono qnum">{qi + 1}.</span> {q.prompt}</p>
                <div class="exit-choices">
                  {#each q.choices as c}
                    <button
                      class="exit-choice"
                      class:selected={exitAnswers[q.id] === c}
                      onclick={() => (exitAnswers = { ...exitAnswers, [q.id]: c })}
                    >
                      {c}
                    </button>
                  {/each}
                </div>
              </div>
            {/each}
            <div class="exit-actions">
              {#if exitMsg}<span class="mono exit-msg">{exitMsg}</span>{/if}
              <button
                class="cta mono-cta"
                onclick={submitExit}
                disabled={Object.keys(exitAnswers).length < exitQs.length || exitQs.length === 0}
              >
                ⇡ submit — unlock early
              </button>
            </div>
          {/if}
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .reader {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    color: var(--fg);
    min-height: 0;
    animation: fade-in 0.5s ease;
  }
  .reader-head {
    border-bottom: 1px solid var(--border);
    background: var(--bg);
  }
  .title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 14px 24px 10px;
  }
  .title-left {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    min-width: 0;
  }
  .head-right {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-shrink: 0;
  }
  .font-ctl {
    display: flex;
    gap: 2px;
  }
  .font-ctl button {
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--muted);
    font-family: var(--font-mono);
    font-size: 11px;
    padding: 3px 8px;
    cursor: pointer;
  }
  .font-ctl button:first-child {
    border-radius: 5px 0 0 5px;
  }
  .font-ctl button:last-child {
    border-radius: 0 5px 5px 0;
  }
  .font-ctl button:hover {
    color: var(--fg);
  }
  .prove {
    border-color: var(--accent);
    color: var(--accent);
  }
  .node-tag {
    font-size: 10px;
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 3px 8px;
    letter-spacing: 0.5px;
  }
  .title-row h2 {
    font-size: 19px;
  }
  .ttl {
    font-size: 14px;
    color: var(--accent);
    white-space: nowrap;
  }
  .ttl.done {
    color: var(--ok-fg);
  }
  .reader-layout {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .toc {
    width: 230px;
    flex-shrink: 0;
    padding: 26px 8px 26px 20px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .toc-label {
    font-size: 9px;
    letter-spacing: 1px;
    color: var(--faint);
    margin-bottom: 10px;
  }
  .toc-item {
    text-align: left;
    background: none;
    border: none;
    border-left: 2px solid transparent;
    color: var(--muted);
    font-family: var(--font-body);
    font-size: 12.5px;
    padding: 6px 8px;
    cursor: pointer;
    display: flex;
    gap: 8px;
    align-items: baseline;
    line-height: 1.35;
  }
  .toc-item:hover {
    color: var(--fg);
  }
  .toc-item.active {
    color: var(--fg);
    border-left-color: var(--accent);
    background: var(--surface);
    border-radius: 0 6px 6px 0;
  }
  .toc-check {
    font-size: 10px;
    color: var(--ok-fg);
    width: 10px;
    flex-shrink: 0;
  }
  .reader-body {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    scroll-behavior: smooth;
  }
  .column {
    max-width: 68ch;
    margin: 0 auto;
    padding: 32px 24px 80px;
  }
  .resources {
    margin-top: 40px;
    border-top: 1px solid var(--border);
    padding-top: 16px;
  }
  .resources h3 {
    font-size: 18px;
  }
  .fine {
    font-size: 10px;
    color: var(--faint);
    letter-spacing: 0.5px;
  }
  .r-title {
    font-weight: 600;
  }
  .r-why {
    color: var(--muted);
  }
  .finish-row {
    margin-top: 48px;
    display: flex;
    justify-content: center;
  }
  /* ---- exit check ---- */
  .exit-overlay {
    position: fixed;
    inset: 0;
    background: rgba(20, 17, 24, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 70;
  }
  .exit-panel {
    width: min(640px, 92vw);
    max-height: 84vh;
    overflow-y: auto;
    background: var(--bg);
    border: 1px solid var(--border);
    border-top: 3px solid var(--accent);
    border-radius: 12px;
    padding: 18px 22px 22px;
    box-shadow: 0 18px 60px rgba(0, 0, 0, 0.4);
  }
  .exit-head {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 10.5px;
    color: var(--muted);
    margin-bottom: 14px;
  }
  .exit-tag {
    color: var(--accent);
    border: 1px solid var(--accent);
    border-radius: 4px;
    padding: 2px 7px;
    letter-spacing: 1px;
    font-size: 9px;
    white-space: nowrap;
  }
  .exit-close {
    margin-left: auto;
    background: none;
    border: none;
    color: var(--muted);
    font-size: 18px;
    cursor: pointer;
  }
  .exit-q {
    margin-bottom: 16px;
  }
  .exit-prompt {
    font-size: 14.5px;
    margin: 0 0 8px;
    line-height: 1.5;
  }
  .qnum {
    color: var(--accent);
    font-size: 12px;
  }
  .exit-choices {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .exit-choice {
    text-align: left;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--fg);
    border-radius: 7px;
    padding: 8px 12px;
    font-family: var(--font-body);
    font-size: 13px;
    cursor: pointer;
  }
  .exit-choice:hover {
    border-color: var(--muted);
  }
  .exit-choice.selected {
    border-color: var(--accent);
    background: var(--surface-2);
  }
  .exit-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 14px;
    margin-top: 6px;
  }
  .exit-msg {
    font-size: 11px;
    color: var(--bad-fg);
  }
  .dim {
    color: var(--faint);
    font-size: 12px;
  }
  .cool {
    color: var(--warn-fg);
    font-size: 13px;
  }
</style>
