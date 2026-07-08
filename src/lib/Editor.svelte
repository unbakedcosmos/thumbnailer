<script>
  import {
    app,
    GRID_PRESETS,
    selectedJob,
    syncJobConfig,
    generateSelected,
    applyConfigToBatch,
    estimateAnimatedMB,
    estimateStaticMB,
    outputFilesNote,
    fmtDuration,
    fmtMB,
    jobIsPortrait,
    writesTo,
    templateById,
    accentColor,
    borderFor,
    retryJob
  } from '$lib/state.svelte.js';

  const job = $derived(selectedJob());
  const isPortrait = $derived(job ? jobIsPortrait(job) : false);

  const showGrid = $derived(
    job ? job.config.artifacts.staticSheet || job.config.artifacts.animated : false
  );
  const showStaticPanel = $derived(
    job ? job.config.artifacts.staticSheet || job.config.artifacts.montage : false
  );
  const showAnimatedPanel = $derived(job ? job.config.artifacts.animated : false);

  const animEst = $derived(job ? estimateAnimatedMB(job) : 0);
  const overTarget = $derived(job ? animEst > job.config.animated.targetMb : false);
  const staticEst = $derived(job ? estimateStaticMB(job) : 0);
  const isPng = $derived(job?.config.static.format === 'png');

  const tpl = $derived(job ? templateById(job.config.static.templateId) : null);
  const frameOn = $derived(job?.config.static.frameOn ?? true);

  const gridIsPreset = $derived(
    job
      ? GRID_PRESETS.some((g) => g.cols === job.config.grid.cols && g.rows === job.config.grid.rows)
      : true
  );
  let customGrid = $state(false);
  $effect(() => {
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

  function previewTs(i) {
    const m = Math.floor((i * 11 + 4) / 60);
    return `${m}:${String((i * 11 + 4) % 60).padStart(2, '0')}`;
  }

  function setGrid(g) {
    job.config.grid = { cols: g.cols, rows: g.rows };
    customGrid = false;
    syncJobConfig(job);
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
  function setStaticFormat(f) {
    job.config.static.format = f;
    syncJobConfig(job);
  }
  function setAnimatedFormat(f) {
    job.config.animated.format = f;
    syncJobConfig(job);
  }
  function toggleSharpen() {
    job.config.static.sharpen = !job.config.static.sharpen;
    syncJobConfig(job);
  }
  function toggleFrame() {
    job.config.static.frameOn = !job.config.static.frameOn;
    syncJobConfig(job);
  }
  function setTarget(delta) {
    job.config.animated.targetMb = Math.min(32, Math.max(1, job.config.animated.targetMb + delta));
    syncJobConfig(job);
  }
  function setOutputMode(m) {
    job.config.outputMode = m;
    syncJobConfig(job);
  }

  // Sliders: click-to-jump + drag
  function makeSlider(get, set) {
    let el = null;
    let dragging = false;
    const apply = (e) => {
      const rect = el.getBoundingClientRect();
      set(Math.max(0, Math.min(100, Math.round(((e.clientX - rect.left) / rect.width) * 100))));
    };
    return {
      ref: (node) => {
        el = node;
      },
      down: (e) => {
        dragging = true;
        apply(e);
        e.currentTarget.setPointerCapture?.(e.pointerId);
      },
      move: (e) => {
        if (dragging) apply(e);
      },
      up: () => {
        if (dragging) {
          dragging = false;
          syncJobConfig(job);
        }
      },
      key: (e) => {
        if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
          e.preventDefault();
          set(Math.max(0, Math.min(100, get() + (e.key === 'ArrowRight' ? 2 : -2))));
          syncJobConfig(job);
        }
      }
    };
  }
  const animSlider = makeSlider(
    () => job.config.animated.quality,
    (v) => (job.config.animated.quality = v)
  );
  const staticSlider = makeSlider(
    () => job.config.static.quality,
    (v) => (job.config.static.quality = v)
  );

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
        <button class="retry" onclick={() => retryJob(job.id)}>Retry</button>
      </div>
    {:else if job.status === 'skipped'}
      <div class="banner skipped">
        <span>⚠ Skipped — {job.failReason}</span>
      </div>
    {:else if job.status === 'done' && job.degraded}
      <div class="banner degraded">
        <span>⚠ Fit at reduced quality to stay under {job.config.animated.targetMb} MB</span>
      </div>
    {/if}

    <div class="field artifacts-row">
      <span class="label">Artifacts</span>
      <div class="chips">
        {#each artifactDefs as a (a.key)}
          <button
            class="chip"
            class:on={job.config.artifacts[a.key]}
            onclick={() => toggleArtifact(a.key)}
          >
            <span class="sq" class:on={job.config.artifacts[a.key]}></span>{a.label}
          </button>
        {/each}
      </div>
    </div>

    <div class="controls">
      {#if showGrid}
        <div class="field">
          <span class="label">Grid</span>
          <div class="seg-group" role="radiogroup" aria-label="Grid dimensions">
            {#each GRID_PRESETS as g (g.label)}
              <button
                class="seg"
                class:active={!showCustom &&
                  job.config.grid.cols === g.cols &&
                  job.config.grid.rows === g.rows}
                onclick={() => setGrid(g)}>{g.label}</button
              >
            {/each}
            <button class="seg" class:active={showCustom} onclick={() => (customGrid = true)}
              >custom</button
            >
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
      {/if}
      <div class="field">
        <span class="label">Orientation</span>
        <div class="seg-group" role="radiogroup" aria-label="Orientation">
          {#each ['auto', 'portrait', 'landscape'] as o (o)}
            <button
              class="seg"
              class:active={job.config.orientation === o}
              onclick={() => setOrientation(o)}>{o[0].toUpperCase() + o.slice(1)}</button
            >
          {/each}
        </div>
      </div>
    </div>

    {#if showStaticPanel}
      <div class="panel">
        <span class="label mint">Static &amp; montage image</span>
        <div class="panel-grid">
          <div class="field">
            <span class="label">File type</span>
            <div class="seg-group">
              {#each [['png', 'PNG'], ['jpeg', 'JPEG'], ['webp', 'WebP']] as [key, lab] (key)}
                <button
                  class="seg"
                  class:active={job.config.static.format === key}
                  onclick={() => setStaticFormat(key)}>{lab}</button
                >
              {/each}
            </div>
          </div>
          <div class="field">
            <span class="label">Sharpen</span>
            <button class="chip" class:on={job.config.static.sharpen} onclick={toggleSharpen}>
              <span class="sq" class:on={job.config.static.sharpen}></span>{job.config.static
                .sharpen
                ? 'On'
                : 'Off'}
            </button>
          </div>
          <div class="field span2f">
            <span class="label">Frame</span>
            <div class="frame-row">
              <button class="chip" class:on={frameOn} onclick={toggleFrame}>
                <span class="sq" class:on={frameOn}></span>{frameOn ? 'On' : 'Off (raw grab)'}
              </button>
              {#if frameOn}
                <button class="btn-ghost" onclick={() => (app.templateGalleryOpen = true)}
                  >{tpl?.name ?? 'Classic'} · Choose…</button
                >
              {/if}
            </div>
          </div>
          <div class="field span2f">
            <span class="label">Compression quality</span>
            {#if !isPng}
              <div class="quality-row">
                <div
                  class="track"
                  use:staticSlider.ref
                  role="slider"
                  tabindex="0"
                  aria-label="Static compression quality"
                  aria-valuenow={job.config.static.quality}
                  aria-valuemin="0"
                  aria-valuemax="100"
                  onpointerdown={staticSlider.down}
                  onpointermove={staticSlider.move}
                  onpointerup={staticSlider.up}
                  onkeydown={staticSlider.key}
                >
                  <i style:width="{job.config.static.quality}%"></i>
                  <b style:left="{job.config.static.quality}%"></b>
                </div>
                <span class="est"
                  >≈ {staticEst.toFixed(1)} MB · lossy {job.config.static.format.toUpperCase()}</span
                >
              </div>
            {:else}
              <span class="dim small"
                >PNG is lossless — no quality setting · ≈ {staticEst.toFixed(1)} MB</span
              >
            {/if}
          </div>
        </div>
      </div>
    {/if}

    {#if showAnimatedPanel}
      <div class="panel">
        <span class="label mint">Animated preview</span>
        <div class="panel-grid">
          <div class="field">
            <span class="label">File type</span>
            <div class="seg-group">
              {#each [['webp', 'WebP'], ['gif', 'GIF']] as [key, lab] (key)}
                <button
                  class="seg"
                  class:active={job.config.animated.format === key}
                  onclick={() => setAnimatedFormat(key)}>{lab}</button
                >
              {/each}
            </div>
          </div>
          <div class="field">
            <span class="label">Target size</span>
            <div class="stepper">
              <button onclick={() => setTarget(-1)} aria-label="Decrease target">−</button>
              <span class="val" class:danger={overTarget}>{job.config.animated.targetMb} MB</span>
              <button onclick={() => setTarget(1)} aria-label="Increase target">+</button>
            </div>
          </div>
          <div class="field span2f">
            <span class="label">Quality</span>
            <div class="quality-row">
              <div
                class="track"
                use:animSlider.ref
                role="slider"
                tabindex="0"
                aria-label="Animated quality"
                aria-valuenow={job.config.animated.quality}
                aria-valuemin="0"
                aria-valuemax="100"
                onpointerdown={animSlider.down}
                onpointermove={animSlider.move}
                onpointerup={animSlider.up}
                onkeydown={animSlider.key}
              >
                <i style:width="{job.config.animated.quality}%"></i>
                <b style:left="{job.config.animated.quality}%"></b>
              </div>
              <span class="est" class:danger={overTarget}
                >≈ {animEst.toFixed(1)} MB est. · target {job.config.animated.targetMb} MB</span
              >
            </div>
          </div>
        </div>
      </div>
    {/if}

    <div class="field output-field">
      <span class="label">Output folder</span>
      <div class="seg-group">
        <button
          class="seg"
          class:active={job.config.outputMode === 'source'}
          onclick={() => setOutputMode('source')}>Same as source</button
        >
        <button
          class="seg"
          class:active={job.config.outputMode === 'custom'}
          onclick={() => setOutputMode('custom')}>Custom folder</button
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

    {#if showStaticPanel}
      <div class="preview-head"><span class="label">Static / montage preview</span></div>
      <div
        class="preview"
        style:border={frameOn && tpl
          ? borderFor(tpl.border, tpl.accent)
          : '1px solid var(--border)'}
      >
        {#if frameOn && tpl?.headerBand}
          <div class="band" style:color={accentColor(tpl.accent === 'none' ? 'dim' : tpl.accent)}>
            <span class="band-name">{job.name}</span>
            <span
              >{job.meta ? fmtDuration(job.meta.durationS) : '—'} · {job.meta
                ? `${job.meta.width}×${job.meta.height}`
                : '—'}</span
            >
          </div>
        {/if}
        <div class="pgrid" style:grid-template-columns="repeat({previewCols}, 1fr)">
          {#each Array(previewShown), i (i)}
            <div class="tile" style:aspect-ratio={isPortrait ? '9/16' : '16/9'}>
              {#if frameOn && tpl && tpl.timestampStyle !== 'none'}
                {#if tpl.timestampStyle === 'overlay'}
                  <span
                    class="ts overlay"
                    style:background={accentColor(tpl.accent === 'none' ? 'mint' : tpl.accent)}
                    >{previewTs(i)}</span
                  >
                {:else}
                  <span class="ts corner">{previewTs(i)}</span>
                {/if}
              {/if}
            </div>
          {/each}
        </div>
      </div>
      <div class="caption">
        <span class="dim"
          >preview · {previewShown} of {previewTotal} tiles shown · {frameOn && tpl
            ? `${tpl.name} frame`
            : 'no frame · raw grab'} · not final encode</span
        >
        <span class="est">{staticEst.toFixed(1)} MB est.</span>
      </div>
    {/if}

    {#if showAnimatedPanel}
      <div class="preview-head"><span class="label">Animated preview loop</span></div>
      <div class="preview loop-well">
        <div class="loop">▶</div>
      </div>
      <div class="caption">
        <span class="dim">looping preview · not final encode</span>
        <span class="est" class:danger={overTarget}
          >{animEst.toFixed(1)} MB est. · target {job.config.animated.targetMb} MB</span
        >
      </div>
    {/if}

    {#if job.status === 'done' && job.artifacts?.length}
      <div class="artifacts">
        {#each job.artifacts as a (a.path)}
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
      <span class="writes">→ writes to {writesTo(job)} · {outputFilesNote(job)}</span>
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
  .artifacts-row {
    margin-bottom: 16px;
  }
  .controls {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px 28px;
    margin-bottom: 14px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .panel {
    background: #121317;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: 14px 16px;
    margin-bottom: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .panel-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px 28px;
  }
  .span2f {
    grid-column: 1/3;
  }
  .label.mint {
    color: var(--accent);
  }
  .frame-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
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
  .small {
    font-size: 12px;
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
  .output-field {
    margin-bottom: 22px;
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
  .preview-head {
    margin-bottom: 6px;
  }
  .preview {
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: 14px;
    margin-bottom: 8px;
  }
  .band {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    padding: 0 2px 10px;
    margin-bottom: 10px;
    border-bottom: 1px solid var(--border);
    font-size: 11px;
    font-weight: 700;
    gap: 12px;
  }
  .band-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pgrid {
    display: grid;
    gap: 6px;
  }
  .tile {
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    position: relative;
  }
  .ts {
    position: absolute;
    right: 3px;
    bottom: 3px;
    font-size: 9px;
  }
  .ts.corner {
    color: var(--text-dim);
    font-weight: 600;
  }
  .ts.overlay {
    color: #0b120d;
    padding: 1px 4px;
    border-radius: 2px;
    font-weight: 700;
  }
  .loop-well {
    display: flex;
    justify-content: center;
  }
  .loop {
    width: 64px;
    height: 64px;
    border-radius: var(--r-full);
    border: 1px solid var(--border);
    background: var(--surface-2);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--accent);
    font-size: 20px;
  }
  .caption {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin: 6px 2px 20px;
    font-size: 12px;
    gap: 12px;
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
