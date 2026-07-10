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
  const EFFORTS = [
    { key: 'fast', label: 'Fast' },
    { key: 'balanced', label: 'Balanced' },
    { key: 'quality', label: 'Quality' },
    { key: 'custom', label: 'Custom' }
  ];
  const EFFORT_HELP = {
    fast: 'quickest encodes — lower encoder effort',
    balanced: 'sharper frames + better compression',
    quality: 'best quality/size — much slower encodes',
    custom: 'full manual control of the knobs below'
  };
  function setEffort(e) {
    app.settings.effort = e;
    saveSettings();
  }
  // --- advanced (Custom) knobs ---
  const a = () => app.settings.advanced;
  function setAdv(key, val) {
    app.settings.advanced[key] = val;
    saveSettings();
  }
  function bumpAdv(key, d, min, max, round1) {
    let v = (a()[key] ?? 0) + d;
    v = Math.min(max, Math.max(min, v));
    if (round1) v = Math.round(v * 10) / 10;
    setAdv(key, v);
  }
  function bumpTrim(key, dPct) {
    const pct = Math.round((a()[key] ?? 0) * 100) + dPct;
    setAdv(key, Math.min(40, Math.max(0, pct)) / 100);
  }
  const SCALERS = ['lanczos', 'bicubic', 'spline', 'area'];
  const SUBS = [
    ['s444', '4:4:4'],
    ['s422', '4:2:2'],
    ['s420', '4:2:0']
  ];
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
          <div class="label">Encode effort</div>
          <div class="help">{EFFORT_HELP[app.settings.effort ?? 'balanced']}</div>
        </div>
        <div class="seg-group">
          {#each EFFORTS as e (e.key)}
            <button
              class="seg"
              class:active={(app.settings.effort ?? 'balanced') === e.key}
              onclick={() => setEffort(e.key)}>{e.label}</button
            >
          {/each}
        </div>
      </div>

      {#if app.settings.effort === 'custom'}
        <div class="adv">
          <div class="adv-row">
            <span class="adv-label">Scaler</span>
            <div class="seg-group sm">
              {#each SCALERS as s (s)}
                <button
                  class="seg"
                  class:active={a().scaler === s}
                  onclick={() => setAdv('scaler', s)}>{s}</button
                >
              {/each}
            </div>
          </div>
          <div class="adv-row">
            <span class="adv-label">Sharpest-frame scan</span>
            <div class="stepper">
              <button onclick={() => bumpAdv('sharpCandidates', -1, 1, 15)}>−</button>
              <span class="val">{a().sharpCandidates}</span>
              <button onclick={() => bumpAdv('sharpCandidates', 1, 1, 15)}>+</button>
            </div>
          </div>
          <div class="adv-row">
            <span class="adv-label">WebP method (0–6)</span>
            <div class="stepper">
              <button onclick={() => bumpAdv('webpMethod', -1, 0, 6)}>−</button>
              <span class="val">{a().webpMethod}</span>
              <button onclick={() => bumpAdv('webpMethod', 1, 0, 6)}>+</button>
            </div>
          </div>
          <div class="adv-row">
            <span class="adv-label">Sharp-YUV</span>
            <button
              class="chip"
              class:on={a().sharpYuv}
              onclick={() => setAdv('sharpYuv', !a().sharpYuv)}
            >
              <span class="sq" class:on={a().sharpYuv}></span>{a().sharpYuv ? 'On' : 'Off'}
            </button>
          </div>
          <div class="adv-row">
            <span class="adv-label">JPEG progressive</span>
            <button
              class="chip"
              class:on={a().jpegProgressive}
              onclick={() => setAdv('jpegProgressive', !a().jpegProgressive)}
            >
              <span class="sq" class:on={a().jpegProgressive}></span>{a().jpegProgressive
                ? 'On'
                : 'Off'}
            </button>
          </div>
          <div class="adv-row">
            <span class="adv-label">Sheet chroma</span>
            <div class="seg-group sm">
              {#each SUBS as [key, lab] (key)}
                <button
                  class="seg"
                  class:active={a().sheetSubsampling === key}
                  onclick={() => setAdv('sheetSubsampling', key)}>{lab}</button
                >
              {/each}
            </div>
          </div>
          <div class="adv-row">
            <span class="adv-label">Sheet quality</span>
            <div class="stepper">
              <button onclick={() => bumpAdv('sheetQuality', -5, 60, 100)}>−</button>
              <span class="val">{a().sheetQuality}</span>
              <button onclick={() => bumpAdv('sheetQuality', 5, 60, 100)}>+</button>
            </div>
          </div>
          <div class="adv-row">
            <span class="adv-label">Sharpen strength</span>
            <div class="stepper">
              <button onclick={() => bumpAdv('sharpenAmount', -0.1, 0, 3, true)}>−</button>
              <span class="val">{a().sharpenAmount.toFixed(1)}</span>
              <button onclick={() => bumpAdv('sharpenAmount', 0.1, 0, 3, true)}>+</button>
            </div>
          </div>
          <div class="adv-row">
            <span class="adv-label">Head / tail trim</span>
            <div class="trim">
              <div class="stepper">
                <button onclick={() => bumpTrim('headTrim', -2)}>−</button>
                <span class="val">{Math.round((a().headTrim ?? 0) * 100)}%</span>
                <button onclick={() => bumpTrim('headTrim', 2)}>+</button>
              </div>
              <div class="stepper">
                <button onclick={() => bumpTrim('tailTrim', -2)}>−</button>
                <span class="val">{Math.round((a().tailTrim ?? 0) * 100)}%</span>
                <button onclick={() => bumpTrim('tailTrim', 2)}>+</button>
              </div>
            </div>
          </div>
        </div>
      {/if}

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
  .adv {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 14px;
    background: #121317;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
  }
  .adv-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    font-size: 12px;
  }
  .adv-label {
    color: var(--text-dim);
  }
  .seg-group.sm .seg {
    padding: 3px 7px;
    font-size: 11px;
    text-transform: capitalize;
  }
  .trim {
    display: flex;
    gap: 8px;
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
