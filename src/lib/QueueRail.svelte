<script>
  import { app, fmtMB } from '$lib/state.svelte.js';

  let listEl;

  function meta(job) {
    switch (job.status) {
      case 'running':
        return { text: Math.round(job.pct) + '%', cls: 'mint' };
      case 'done': {
        // Headline figure: the largest artifact — the number the size
        // guarantee governs, always ≤ target (FR18)
        const largest = job.artifacts?.reduce((m, a) => Math.max(m, a.bytes), 0) ?? 0;
        return { text: largest ? fmtMB(largest) : 'done', cls: 'dim' };
      }
      case 'failed':
        return { text: 'failed', cls: 'danger' };
      case 'skipped':
        return { text: "can't fit", cls: 'warning' };
      default:
        return { text: '—', cls: 'dim' };
    }
  }

  // Skipped-because-exists reads differently from can't-fit
  function metaFor(job) {
    const m = meta(job);
    if (job.status === 'skipped' && job.failReason === 'already exists') m.text = 'exists';
    return m;
  }

  // Follow the running row during a batch (EXPERIENCE queue-row, toggle `F`)
  $effect(() => {
    if (!app.follow || !listEl) return;
    const running = app.jobs.find((j) => j.status === 'running');
    if (running) {
      const el = listEl.querySelector(`[data-id="${running.id}"]`);
      el?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    }
  });
</script>

<aside>
  <div class="rail-head">
    <span class="label">Queue · {app.jobs.length}</span>
    <button class="follow" class:on={app.follow} onclick={() => (app.follow = !app.follow)}
      >follow ▸running</button
    >
  </div>
  <div class="list" bind:this={listEl}>
    {#each app.jobs as job (job.id)}
      {@const m = metaFor(job)}
      <div
        class="row"
        class:selected={job.id === app.selectedId}
        data-id={job.id}
        role="option"
        aria-selected={job.id === app.selectedId}
        tabindex="-1"
        onclick={() => (app.selectedId = job.id)}
        onkeydown={(e) => e.key === 'Enter' && (app.selectedId = job.id)}
        title={job.failReason ?? job.name}
      >
        <span class="dot {job.status}" aria-hidden="true"></span>
        <span class="name">{job.name}</span>
        <span class="meta {m.cls}">{m.text}</span>
      </div>
    {/each}
  </div>
</aside>

<style>
  aside {
    flex: 0 0 34%;
    max-width: 400px;
    min-width: 300px;
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg);
  }
  .rail-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 14px 8px;
  }
  .follow {
    font-size: 12px;
    color: var(--text-dim);
    background: none;
    border: none;
    cursor: pointer;
    padding: 0;
  }
  .follow.on {
    color: var(--accent);
  }
  .list {
    overflow: auto;
    padding: 0 8px 12px;
    flex: 1;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: var(--r-sm);
    cursor: pointer;
    border-left: 2px solid transparent;
  }
  .row:hover {
    background: var(--card);
  }
  .row.selected {
    background: #1b1d22;
    border-left-color: var(--accent);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: var(--r-full);
    flex: 0 0 auto;
    background: var(--text-dim);
  }
  .dot.done {
    background: var(--accent);
  }
  .dot.running {
    background: var(--accent);
    animation: tn-pulse 1.4s infinite;
  }
  .dot.failed {
    background: var(--danger);
  }
  .dot.skipped {
    background: var(--warning);
  }
  .name {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    font-size: 11px;
    flex: 0 0 auto;
  }
  .meta.dim {
    color: var(--text-dim);
  }
  .meta.mint {
    color: var(--accent);
  }
  .meta.danger {
    color: var(--danger);
  }
  .meta.warning {
    color: var(--warning);
  }
</style>
