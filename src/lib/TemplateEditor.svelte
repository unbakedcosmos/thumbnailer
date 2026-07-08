<script>
  import { app, selectedJob, syncJobConfig, saveTemplate } from '$lib/state.svelte.js';

  const draft = $derived(app.templateEditor);

  function close() {
    app.templateEditor = null;
  }

  async function save() {
    const saved = await saveTemplate($state.snapshot(draft));
    // Saving assigns the template to the currently-selected file (CHANGELOG §2)
    const job = selectedJob();
    if (job) {
      job.config.static.templateId = saved.id;
      syncJobConfig(job);
    }
    close();
  }

  const borderOpts = [
    ['none', 'None'],
    ['hairline', 'Hairline'],
    ['thick', 'Thick']
  ];
  const tsOpts = [
    ['none', 'None'],
    ['corner', 'Corner'],
    ['overlay', 'Overlay']
  ];
  const accentOpts = [
    ['mint', 'Mint'],
    ['white', 'White'],
    ['none', 'None']
  ];
</script>

{#if draft}
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
      aria-label="Edit template"
      tabindex="-1"
      onkeydown={() => {}}
    >
      <div class="head">
        <span class="title">{draft.id ? 'Edit template' : 'New template'}</span>
        <button class="x" onclick={close} aria-label="Close">✕</button>
      </div>
      <div class="body">
        <div class="field">
          <span class="label">Name</span>
          <input
            spellcheck="false"
            value={draft.name}
            onchange={(e) => (draft.name = e.currentTarget.value)}
          />
        </div>
        <div class="field">
          <span class="label">Header band</span>
          <div class="seg-group">
            <button
              class="seg"
              class:active={draft.headerBand}
              onclick={() => (draft.headerBand = true)}>On</button
            >
            <button
              class="seg"
              class:active={!draft.headerBand}
              onclick={() => (draft.headerBand = false)}>Off</button
            >
          </div>
        </div>
        <div class="field">
          <span class="label">Border</span>
          <div class="seg-group">
            {#each borderOpts as [key, lab] (key)}
              <button
                class="seg"
                class:active={draft.border === key}
                onclick={() => (draft.border = key)}>{lab}</button
              >
            {/each}
          </div>
        </div>
        <div class="field">
          <span class="label">Timestamps</span>
          <div class="seg-group">
            {#each tsOpts as [key, lab] (key)}
              <button
                class="seg"
                class:active={draft.timestampStyle === key}
                onclick={() => (draft.timestampStyle = key)}>{lab}</button
              >
            {/each}
          </div>
        </div>
        <div class="field">
          <span class="label">Accent</span>
          <div class="seg-group">
            {#each accentOpts as [key, lab] (key)}
              <button
                class="seg"
                class:active={draft.accent === key}
                onclick={() => (draft.accent = key)}>{lab}</button
              >
            {/each}
          </div>
        </div>
        <div class="foot">
          <button class="btn-primary" onclick={save}>Save template</button>
          <button class="btn-ghost" onclick={close}>Cancel</button>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    z-index: 12;
    padding-top: 56px;
  }
  .panel {
    width: 420px;
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: 22px 24px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 18px;
    border-bottom: 1px solid var(--border-strong);
    padding-bottom: 12px;
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
  .body {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  input {
    font-family: inherit;
    font-size: 12px;
    color: var(--text);
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 7px 10px;
    width: 100%;
  }
  .foot {
    display: flex;
    gap: 10px;
    border-top: 1px solid var(--border);
    padding-top: 16px;
  }
</style>
