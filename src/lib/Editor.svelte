<script>
  import {
    app,
    GRID_PRESETS,
    selectedJob,
    syncJobConfig,
    generateSelected,
    applyConfigToBatch,
    estimateMB,
    fmtDuration,
    fmtMB,
    jobIsPortrait,
    writesTo
  } from '$lib/state.svelte.js';
  import { invoke } from '@tauri-apps/api/core';

  const job = $derived(selectedJob());
  const isPortrait = $derived(job ? jobIsPortrait(job) : false);
  const est = $derived(job ? estimateMB(job) : 0);
  const overTarget = $derived(job ? est > job.config.targetMb : false);

  const gridIsPreset = $derived(
    job ? GRID_PRESETS.some((g) => g.cols === job.config.grid.cols && g.rows === job.config.grid.rows) : true
  );
  let customGrid = $state(false);
  $effect(() => {
    // Reset the custom flag when switching files
    void job?.id;
    customGrid = false;
  });
  const showCustom = $derived(customGrid || !gridIsPreset);

  // Preview renders a capped sample (≤3 rows), not the literal tile count
  const previewCols = $derived(job?.config.grid.cols ?? 3);
  const previewTotal = $derived(job ? job.config.grid.cols * job.config.grid.rows : 0);
  const previewShown = $derived(Math.min(previewTotal, previewCols * 3));

  const orientLabel = $derived.by(() => {
    if (!job) return '';
    const shape = isPortrait ? 'portrait · 9:16' : 'landscape · 16:9';
    return `${shape} · ${job.config.orientation}`;
  });

  function setGrid(g) {
    job.config.grid = { cols: g.cols, rows: g.rows };
    customGrid = false;
    syncJobConfig(job);
  }
  function setCustom() {
    customGrid = true;
  }
  function bumpGrid(key, delta, min, max) {
    job.config.grid[key] = Math.min(max, Math.max(min, job.config.grid[key] + delta));
    syncJobConfig(job);
  }
  function setOrientation(o) {
    job.config.orientation = o;
    syncJobConfig(job);
  }
  function toggleArtifact(key) {
    job.config.artifacts[key] = !job.config.artifacts[key];
    syncJobConfig(job);
  }
  function setTarget(delta) {
    job.config.targetMb = Math.min(32, Math.max(1, job.config.targetMb + delta));
    syncJobConfig(job);
  }
  function setOutputMode(m) {
    job.config.outputMode = m;
    syncJobConfig(job);
  }

  // Quality slider: click-to-jump + drag (HANDOFF)
  let trackEl = $state(null);
  let dragging = $state(false);
  function qualityFromEvent(e) {
    const rect = trackEl.getBoundingClientRect();
    const pct = Math.max(0, Math.min(100, Math.round(((e.clientX - rect.left) / rect.width) * 100)));
    job.config.quality = pct;
  }
  function onTrackDown(e) {
    dragging = true;
    qualityFromEvent(e);
    e.currentTarget.setPointerCapture?.(e.pointerId);
  }
  function onTrackMove(e) {
    if (dragging) qualityFromEvent(e);
  }
  function onTrackUp() {
    if (dragging) {
      dragging = false;
      syncJobConfig(job);
    }
  }
  function onTrackKey(e) {
    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      e.preventDefault();
      job.config.quality = Math.max(0, Math.min(100, job.config.quality + (e.key === 'ArrowRight' ? 2 : -2)));
      syncJobConfig(job);
    }
  }

  function retry() {
    invoke('generate_one', { id: job.id, config: $state.snapshot(job.config) });
  }

  const artifactDefs = [
    { key: 'staticSheet', label: 'Static' },
    { key: 'animated', label: 'Animated' },
    { key: 'montage', label: 'Montage' }
  ];
</script>

{#if job}
  <section>
    {#if job.status === 'failed'}
      <div class="banner failed">
        <span>✗ Failed — {job.failReason}</span>
        <button class="retry" onclick={retry}>Retry</button>
      </div>
    {:else if job.status === 'skipped'}
      <div class="banner skipped">
        <span>⚠ Skipped — {job.failReason}</span>
      </div>
    {:else if job.status === 'done' && job.degraded}
      <div class="banner degraded">
        <span>⚠ Fit at reduced quality to stay under {job.config.targetMb} MB</span>
      </div>
    {/if}

    <div class="controls">
      <div class="field">
        <span class="label">Grid</span>
        <div class="seg-group" role="radiogroup" aria-label="Grid dimensions">
          {#each GRID_PRESETS as g}
            <button
              class="seg"
              class:active={!showCustom && job.config.grid.cols === g.cols && job.config.grid.rows === g.rows}
              onclick={() => setGrid(g)}>{g.label}</button
            >
          {/each}
          <button class="seg" class:active={showCustom} onclick={setCustom}>custom</button>
        </div>
        {#if showCustom}
          <div class="custom-grid">
            <div class="stepper">
              <button onclick={() => bumpGrid('cols', -1, 1, 6)}>−</button>
              <span class="val">{job.config.grid.cols}</span>
              <button onclick={() => bumpGrid('cols', 1, 1, 6)}>+</button>
            </div>
            <span class="x">×</span>
            <div class="stepper">
              <button onclick={() => bumpGrid('rows', -1, 1, 16)}>−</button>
              <span class="val">{job.config.grid.rows}</span>
              <button onclick={() => bumpGrid('rows', 1, 1, 16)}>+</button>
            </div>
          </div>
        {/if}
      </div>

      <div class="field">
        <span class="label">Orientation</span>
        <div class="seg-group" role="radiogroup" aria-label="Orientation">
          {#each ['auto', 'portrait', 'landscape'] as o}
            <button class="seg" class:active={job.config.orientation === o} onclick={() => setOrientation(o)}
              >{o[0].toUpperCase() + o.slice(1)}</button
            >
          {/each}
        </div>
      </div>

      <div class="field span2">
        <span class="label">Quality</span>
        <div class="quality-row">
          <div
            class="track"
            bind:this={trackEl}
            role="slider"
            tabindex="0"
            aria-label="Quality"
            aria-valuenow={job.config.quality}
            aria-valuemin="0"
            aria-valuemax="100"
            onpointerdown={onTrackDown}
            onpointermove={onTrackMove}
            onpointerup={onTrackUp}
            onkeydown={onTrackKey}
          >
            <i style:width="{job.config.quality}%"></i>
            <b style:left="{job.config.quality}%"></b>
          </div>
          <span class="est" class:danger={overTarget}>≈ {est.toFixed(1)} MB</span>
        </div>
      </div>

      <div class="field">
        <span class="label">Target size</span>
        <div class="stepper">
          <button onclick={() => setTarget(-1)} aria-label="Decrease target">−</button>
          <span class="val" class:danger={overTarget}>{job.config.targetMb} MB</span>
          <button onclick={() => setTarget(1)} aria-label="Increase target">+</button>
        </div>
      </div>

      <div class="field">
        <span class="label">Artifacts</span>
        <div class="chips">
          {#each artifactDefs as a}
            <button class="chip" class:on={job.config.artifacts[a.key]} onclick={() => toggleArtifact(a.key)}>
              <span class="sq" class:on={job.config.artifacts[a.key]}></span>{a.label}
            </button>
          {/each}
        </div>
      </div>

      <div class="field span2">
        <span class="label">Output folder</span>
        <div class="seg-group">
          <button class="seg" class:active={job.config.outputMode === 'source'} onclick={() => setOutputMode('source')}
            >Same as source</button
          >
          <button class="seg" class:active={job.config.outputMode === 'custom'} onclick={() => setOutputMode('custom')}
            >Custom folder</button
          >
        </div>
        {#if job.config.outputMode === 'custom'}
          <input
            spellcheck="false"
            placeholder="Folder path"
            value={job.config.outputPath ?? ''}
            onchange={(e) => {
              job.config.outputPath = e.currentTarget.value;
              syncJobConfig(job);
            }}
          />
        {/if}
      </div>
    </div>

    <div class="filerow">
      <span class="filename">{job.name}</span>
      <span class="pill">{orientLabel}</span>
    </div>
    <div class="metarow">
      <div class="m">
        <div class="label">Duration</div>
        <div class="v">{job.meta ? fmtDuration(job.meta.durationS) : '…'}</div>
      </div>
      <div class="m">
        <div class="label">Resolution</div>
        <div class="v">{job.meta ? `${job.meta.width}×${job.meta.height}` : '…'}</div>
      </div>
      <div class="m">
        <div class="label">FPS</div>
        <div class="v">{job.meta ? Math.round(job.meta.fps) : '…'}</div>
      </div>
      <div class="m">
        <div class="label">Codec</div>
        <div class="v">{job.meta?.codec ?? '…'}</div>
      </div>
    </div>

    <div class="preview">
      <div class="pgrid" style:grid-template-columns="repeat({previewCols}, 1fr)">
        {#each Array(previewShown) as _, i (i)}
          <div class="tile" style:aspect-ratio={isPortrait ? '9/16' : '16/9'}></div>
        {/each}
      </div>
    </div>
    <div class="caption">
      <span class="dim">preview · {previewShown} of {previewTotal} tiles shown · not final encode</span>
      <span class="est" class:danger={overTarget}>{est.toFixed(1)} MB est. · target {job.config.targetMb} MB</span>
    </div>

    {#if job.status === 'done' && job.artifacts?.length}
      <div class="artifacts">
        {#each job.artifacts as a}
          <span class="art"
            >{a.path.split(/[\\/]/).pop()} · <b class="mint">{fmtMB(a.bytes)}</b>{#if a.degraded}
              <span class="warning"> · reduced quality</span>{/if}</span
          >
        {/each}
      </div>
    {/if}

    <div class="actions">
      <button class="btn-primary" onclick={generateSelected} disabled={job.status === 'running'}>
        {job.status === 'running' ? `Encoding ${Math.round(job.pct)}%` : 'Generate this file'}
      </button>
      <button class="btn-ghost" onclick={applyConfigToBatch}>Apply config to batch</button>
      <span class="writes">→ writes to {writesTo(job)} · _contact.png · _contact.webp</span>
    </div>
  </section>
{/if}

<style>
  section {
    flex: 1;
    min-width: 0;
    overflow: auto;
    padding: 22px 26px;
    background: var(--bg);
  }
  .banner {
    background: var(--card);
    border: 1px solid;
    border-radius: var(--r-md);
    padding: 14px 16px;
    margin-bottom: 18px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 12px;
  }
  .banner.failed {
    border-color: var(--danger);
    color: var(--danger);
  }
  .banner.skipped,
  .banner.degraded {
    border-color: var(--warning);
    color: var(--warning);
  }
  .retry {
    font-size: 12px;
    border-radius: var(--r-sm);
    padding: 6px 12px;
    cursor: pointer;
    border: 1px solid var(--danger);
    background: transparent;
    color: var(--danger);
  }
  .controls {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px 28px;
    margin-bottom: 22px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .span2 {
    grid-column: 1/3;
  }
  .custom-grid {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .custom-grid .x {
    color: var(--text-dim);
  }
  .quality-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .track {
    position: relative;
    height: 6px;
    background: var(--surface-2);
    border-radius: var(--r-full);
    flex: 1;
    cursor: pointer;
    touch-action: none;
  }
  .track i {
    position: absolute;
    left: 0;
    top: 0;
    height: 100%;
    background: var(--accent);
    border-radius: var(--r-full);
  }
  .track b {
    position: absolute;
    top: 50%;
    width: 12px;
    height: 12px;
    background: var(--text);
    border-radius: var(--r-full);
    transform: translate(-50%, -50%);
  }
  .est {
    color: var(--accent);
    font-weight: 700;
    font-size: 12px;
    white-space: nowrap;
  }
  .est.danger,
  .val.danger {
    color: var(--danger);
  }
  .chips {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
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
  .filerow {
    font-size: 15px;
    font-weight: 700;
    display: flex;
    align-items: baseline;
    gap: 12px;
    min-width: 0;
  }
  .filename {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pill {
    font-size: 9px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--accent);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 2px 6px;
    font-weight: 600;
    flex: 0 0 auto;
  }
  .metarow {
    display: flex;
    gap: 22px;
    margin: 10px 0 18px;
  }
  .m .v {
    font-size: 13px;
    font-weight: 700;
    color: var(--accent);
  }
  .preview {
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: 14px;
    margin-bottom: 8px;
  }
  .pgrid {
    display: grid;
    gap: 6px;
  }
  .tile {
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
  }
  .caption {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin: 6px 2px 20px;
    font-size: 12px;
  }
  .dim {
    color: var(--text-dim);
  }
  .artifacts {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 16px;
    font-size: 12px;
    color: var(--text);
  }
  .mint {
    color: var(--accent);
  }
  .warning {
    color: var(--warning);
  }
  .actions {
    display: flex;
    gap: 10px;
    align-items: center;
    border-top: 1px solid var(--border);
    padding-top: 16px;
    flex-wrap: wrap;
  }
  .btn-primary:disabled {
    opacity: 0.7;
    cursor: default;
  }
  .writes {
    color: var(--accent);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
