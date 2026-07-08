<script>
  import {
    app,
    pickFolder,
    pickFiles,
    startBatch,
    pauseBatch,
    stopBatch
  } from '$lib/state.svelte.js';

  const settled = $derived(app.batch.done + app.batch.failed + app.batch.skipped);
  const pct = $derived(app.batch.total ? (settled / app.batch.total) * 100 : 0);
</script>

<header>
  <div class="wordmark">thumbnailer<span class="dot-accent">.</span></div>
  <button class="btn-ghost" onclick={pickFolder}>+ Add folder</button>
  <button class="btn-ghost" onclick={pickFiles}>+ Add files</button>
  <div class="spacer"></div>

  {#if app.resumedNote}
    <span class="resumed">{app.resumedNote}</span>
  {/if}

  {#if app.batch.status === 'idle'}
    <span class="summary dim">No batch loaded</span>
  {:else if app.batch.status === 'ready'}
    <span class="summary">{app.batch.total} queued · not started</span>
  {:else}
    <span class="summary">
      <b class="mint">{app.batch.done}</b> / {app.batch.total} done{#if app.batch.failed > 0}<span
          class="danger"
        >
          · {app.batch.failed} failed</span
        >{/if}{#if app.batch.skipped > 0}<span class="warning">
          · {app.batch.skipped} skipped</span
        >{/if}
    </span>
  {/if}

  <div
    class="bar"
    role="progressbar"
    aria-valuenow={Math.round(pct)}
    aria-valuemin="0"
    aria-valuemax="100"
  >
    <i style:width="{pct}%"></i>
  </div>

  {#if app.batch.status === 'ready'}
    <button class="btn-primary" onclick={startBatch}>Start batch</button>
  {:else if app.batch.status === 'running'}
    <button class="btn-ghost" onclick={pauseBatch}>Pause</button>
    <button class="btn-ghost" onclick={stopBatch}>Stop</button>
  {:else if app.batch.status === 'paused'}
    <button class="btn-primary" onclick={startBatch}>Resume</button>
    <button class="btn-ghost" onclick={stopBatch}>Stop</button>
  {/if}

  <button
    class="gear"
    title="Settings"
    aria-label="Settings"
    onclick={() => (app.settingsOpen = true)}>⚙</button
  >
</header>

<style>
  header {
    display: flex;
    align-items: center;
    gap: 18px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border-strong);
    background: var(--card);
    flex: 0 0 auto;
  }
  .wordmark {
    font-size: 15px;
    font-weight: 700;
  }
  .dot-accent {
    color: var(--accent);
  }
  .spacer {
    flex: 1;
  }
  .summary {
    font-size: 12px;
  }
  .summary.dim {
    color: var(--text-dim);
  }
  .mint {
    color: var(--accent);
    font-weight: 700;
  }
  .danger {
    color: var(--danger);
  }
  .warning {
    color: var(--warning);
  }
  .resumed {
    font-size: 12px;
    color: var(--warning);
  }
  .bar {
    width: 180px;
    height: 6px;
    background: var(--surface-2);
    border-radius: var(--r-full);
    overflow: hidden;
  }
  .bar i {
    display: block;
    height: 100%;
    background: var(--accent);
    border-radius: var(--r-full);
    transition: width 0.25s;
  }
  .gear {
    font-size: 14px;
    border-radius: var(--r-sm);
    width: 28px;
    height: 28px;
    cursor: pointer;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-dim);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .gear:hover {
    color: var(--text);
    background: var(--surface-2);
  }
</style>
