<script>
  import {
    app,
    selectedJob,
    syncJobConfig,
    deleteTemplate,
    importTemplate,
    borderFor
  } from '$lib/state.svelte.js';

  const job = $derived(selectedJob());

  function close() {
    app.templateGalleryOpen = false;
  }
  function pick(id) {
    if (!job) return;
    job.config.static.templateId = id;
    syncJobConfig(job);
    close();
  }
  function startNew() {
    app.templateEditor = {
      id: '',
      name: 'My template',
      headerBand: true,
      border: 'hairline',
      timestampStyle: 'corner',
      accent: 'mint',
      builtin: false
    };
  }
  function duplicate(t) {
    app.templateEditor = { ...t, id: '', name: t.name + ' copy', builtin: false };
  }
  function edit(t) {
    if (t.builtin) return;
    app.templateEditor = { ...t };
  }
</script>

<div
  class="backdrop"
  onclick={close}
  onkeydown={(e) => e.key === 'Escape' && close()}
  role="presentation"
>
  <div
    class="panel"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-label="Choose template"
    tabindex="-1"
    onkeydown={() => {}}
  >
    <div class="head">
      <span class="title">Choose template</span>
      <button class="x" onclick={close} aria-label="Close">✕</button>
    </div>
    <div class="toolbar">
      <button class="btn-ghost" onclick={startNew}>+ New template</button>
      <button class="btn-ghost" onclick={importTemplate}>+ Import template</button>
    </div>
    <div class="cards">
      {#each app.templates as t (t.id)}
        {@const active = job?.config.static.templateId === t.id}
        <div
          class="card"
          class:active
          onclick={() => pick(t.id)}
          onkeydown={(e) => e.key === 'Enter' && pick(t.id)}
          role="button"
          tabindex="0"
        >
          <div class="swatch" class:sel={active}>
            {#if t.headerBand}
              <div class="sw-band"></div>
            {/if}
            <div
              class="sw-body"
              style:border={borderFor(t.border, t.accent) === 'none'
                ? 'none'
                : borderFor(t.border, t.accent)}
            >
              <div class="cell"></div>
              <div class="cell"></div>
              <div class="cell"></div>
            </div>
          </div>
          <div class="card-foot">
            <span class="name">{t.name}</span>
            <div class="ops">
              <button
                class="op"
                onclick={(e) => {
                  e.stopPropagation();
                  duplicate(t);
                }}>dup</button
              >
              {#if !t.builtin}
                <button
                  class="op"
                  onclick={(e) => {
                    e.stopPropagation();
                    edit(t);
                  }}>edit</button
                >
                <button
                  class="op del"
                  onclick={(e) => {
                    e.stopPropagation();
                    deleteTemplate(t.id);
                  }}>del</button
                >
              {/if}
            </div>
          </div>
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    z-index: 11;
    padding-top: 56px;
  }
  .panel {
    width: 600px;
    max-height: 640px;
    display: flex;
    flex-direction: column;
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: 22px 24px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;
    border-bottom: 1px solid var(--border-strong);
    padding-bottom: 12px;
    flex: 0 0 auto;
  }
  .title {
    font-size: 15px;
    font-weight: 700;
    white-space: nowrap;
  }
  .x {
    background: transparent;
    border: none;
    color: var(--text-dim);
    font-size: 16px;
    cursor: pointer;
  }
  .toolbar {
    display: flex;
    gap: 8px;
    margin-bottom: 16px;
    flex: 0 0 auto;
  }
  .cards {
    overflow: auto;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    border-radius: 5px;
    cursor: pointer;
  }
  .card.active {
    background: #1b1d22;
  }
  .swatch {
    width: 100%;
    height: 54px;
    border-radius: 4px;
    background: var(--bg);
    border: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .swatch.sel {
    border-color: var(--accent);
  }
  .sw-band {
    height: 9px;
    background: var(--card);
    border-bottom: 1px solid var(--border);
    flex: 0 0 auto;
  }
  .sw-body {
    flex: 1;
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 2px;
    padding: 4px;
    border-radius: 2px;
    margin: 2px;
  }
  .cell {
    background: var(--surface-2);
    border-radius: 1px;
  }
  .card-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }
  .name {
    font-size: 12px;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ops {
    display: flex;
    gap: 4px;
    flex: 0 0 auto;
  }
  .op {
    font-size: 10px;
    color: var(--text-dim);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 2px 6px;
    cursor: pointer;
    background: transparent;
  }
  .op.del {
    color: var(--danger);
  }
</style>
