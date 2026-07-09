<script>
  import { app, recheckFfmpeg, openFfmpegDownload, openFfmpegFolder } from '$lib/state.svelte.js';

  function dismiss() {
    app.ffmpegPromptDismissed = true;
  }
</script>

<div class="backdrop" role="presentation">
  <div class="panel" role="dialog" aria-label="ffmpeg required" aria-modal="true">
    <div class="head">
      <span class="title"><span class="warn">ffmpeg</span> is required</span>
      <button class="x" onclick={dismiss} aria-label="Dismiss">✕</button>
    </div>

    <div class="body">
      <p class="lead">
        Thumbnailer uses <b>ffmpeg</b> and <b>ffprobe</b> to read your videos. Neither was found on this
        system.
      </p>
      <p class="how">
        Install ffmpeg (on Linux: <code>apt install ffmpeg</code>; macOS:
        <code>brew install ffmpeg</code>), <b>or</b> download a static build and drop both
        <code>ffmpeg</code>
        and
        <code>ffprobe</code> into this folder — then <b>Re-check</b>:
      </p>
      {#if app.ffmpegBinDir}
        <code class="path">{app.ffmpegBinDir}</code>
      {/if}

      <div class="actions">
        <button class="btn-primary" onclick={openFfmpegDownload}>Download ffmpeg</button>
        {#if app.ffmpegBinDir}
          <button class="btn-ghost" onclick={openFfmpegFolder}>Open folder</button>
        {/if}
        <button class="btn-ghost" onclick={recheckFfmpeg} disabled={app.ffmpegChecking}>
          {app.ffmpegChecking ? 'Checking…' : 'Re-check'}
        </button>
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
    z-index: 20;
    padding-top: 64px;
  }
  .panel {
    width: 480px;
    max-width: calc(100vw - 32px);
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
  }
  .title {
    font-size: 15px;
    font-weight: 700;
  }
  .warn {
    color: var(--danger);
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
    gap: 12px;
  }
  .lead {
    font-size: 13px;
    line-height: 1.5;
  }
  .how {
    font-size: 12px;
    color: var(--text-dim);
    line-height: 1.6;
  }
  code {
    font-family: inherit;
    background: var(--surface-2);
    border-radius: 3px;
    padding: 1px 5px;
    font-size: 12px;
    color: var(--text);
  }
  .path {
    display: block;
    padding: 8px 10px;
    word-break: break-all;
    color: var(--accent);
  }
  .actions {
    display: flex;
    gap: 10px;
    margin-top: 6px;
    flex-wrap: wrap;
  }
  .btn-primary {
    font-family: inherit;
    font-size: 12px;
    font-weight: 700;
    border-radius: 3px;
    padding: 7px 14px;
    cursor: pointer;
    border: 1px solid var(--accent);
    background: var(--accent);
    color: var(--accent-fg);
  }
  .btn-ghost {
    font-family: inherit;
    font-size: 12px;
    border-radius: 3px;
    padding: 7px 14px;
    cursor: pointer;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text);
  }
  .btn-ghost:hover {
    background: var(--surface-2);
  }
  .btn-ghost:disabled {
    opacity: 0.6;
    cursor: default;
  }
</style>
