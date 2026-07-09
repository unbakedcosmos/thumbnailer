<script>
  import { app, saveSettings, PRESETS } from '$lib/state.svelte.js';

  function bumpConcurrency(d) {
    app.settings.concurrency = Math.min(8, Math.max(1, app.settings.concurrency + d));
    saveSettings();
  }
  function setPreset(p) {
    app.settings.preset = p;
    saveSettings();
  }
  function bumpDefaultTarget(d) {
    app.settings.defaultTargetMb = Math.min(32, Math.max(1, app.settings.defaultTargetMb + d));
    saveSettings();
  }
  function toggleOverwrite() {
    app.settings.overwrite = !app.settings.overwrite;
    saveSettings();
  }
  function close() {
    app.settingsOpen = false;
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
    aria-label="Settings"
    tabindex="-1"
    onkeydown={() => {}}
  >
    <div class="head">
      <span class="title">Settings</span>
      <button class="x" onclick={close} aria-label="Close settings">✕</button>
    </div>

    <div class="body">
      <div class="row">
        <div>
          <div class="label">Concurrency</div>
          <div class="help">files encoding at once</div>
        </div>
        <div class="stepper">
          <button onclick={() => bumpConcurrency(-1)} aria-label="Fewer">−</button>
          <span class="val">{app.settings.concurrency}</span>
          <button onclick={() => bumpConcurrency(1)} aria-label="More">+</button>
        </div>
      </div>

      <div class="row">
        <div>
          <div class="label">Default preset</div>
          <div class="help">used for newly added files</div>
        </div>
        <div class="seg-group">
          {#each Object.keys(PRESETS) as p (p)}
            <button
              class="seg"
              class:active={app.settings.preset === p}
              onclick={() => setPreset(p)}>{p}</button
            >
          {/each}
        </div>
      </div>

      <div class="row">
        <div>
          <div class="label">Default target size</div>
          <div class="help">used for newly added files · per-file override in editor</div>
        </div>
        <div class="stepper">
          <button onclick={() => bumpDefaultTarget(-1)} aria-label="Smaller">−</button>
          <span class="val">{app.settings.defaultTargetMb} MB</span>
          <button onclick={() => bumpDefaultTarget(1)} aria-label="Larger">+</button>
        </div>
      </div>

      <div class="row">
        <div>
          <div class="label">Overwrite existing</div>
          <div class="help">
            {app.settings.overwrite
              ? 'replaces prior artifacts in place'
              : 'keeps prior artifacts — writes a numbered copy, e.g. “(1)”'}
          </div>
        </div>
        <button class="chip" class:on={app.settings.overwrite} onclick={toggleOverwrite}>
          <span class="sq" class:on={app.settings.overwrite}></span>{app.settings.overwrite
            ? 'On'
            : 'Off'}
        </button>
      </div>

      <div class="foot">
        {#if app.ffmpegVersion}
          ffmpeg <span class="mint">{app.ffmpegVersion}</span> · offline, no install required
        {:else}
          <span class="danger">ffmpeg not found — encoding unavailable</span>
        {/if}
      </div>
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
    z-index: 10;
    padding-top: 64px;
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
  }
  .x {
    background: transparent;
    border: none;
    color: var(--text-dim);
    font-size: 16px;
    cursor: pointer;
  }
  .x:hover {
    color: var(--text);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }
  .help {
    font-size: 12px;
    color: var(--text-dim);
    margin-top: 2px;
  }
  .chip {
    font-size: 12px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 6px 11px;
    color: var(--text-dim);
    cursor: pointer;
    display: flex;
    gap: 7px;
    align-items: center;
    background: transparent;
  }
  .chip.on {
    border-color: var(--accent);
    color: var(--text);
  }
  .sq {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: var(--text-dim);
  }
  .sq.on {
    background: var(--accent);
  }
  .foot {
    border-top: 1px solid var(--border);
    padding-top: 14px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .mint {
    color: var(--accent);
    font-weight: 700;
  }
  .danger {
    color: var(--danger);
  }
</style>
