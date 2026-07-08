// App state + Tauri IPC wiring (HANDOFF State Management). Real progress comes
// from Rust-side job events — no client-side timers.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';

export const GRID_PRESETS = [
  { label: '3×9', cols: 3, rows: 9 },
  { label: '3×6', cols: 3, rows: 6 },
  { label: '3×4', cols: 3, rows: 4 },
  { label: '2×12', cols: 2, rows: 12 },
  { label: '2×6', cols: 2, rows: 6 }
];

// Ships with sensible defaults (PRD FR21); applied to newly added files.
export const PRESETS = {
  Small: { quality: 40, targetMb: 4 },
  Balanced: { quality: 62, targetMb: 8 },
  'Max quality': { quality: 85, targetMb: 12 }
};

export const app = $state({
  jobs: [],
  batch: { status: 'idle', total: 0, done: 0, failed: 0, skipped: 0, running: 0 },
  selectedId: null,
  settingsOpen: false,
  follow: true,
  resumedNote: null,
  ffmpegVersion: null,
  settings: { concurrency: 2, preset: 'Balanced', overwrite: false }
});

export function selectedJob() {
  return app.jobs.find((j) => j.id === app.selectedId) ?? null;
}

function mergeJob(job) {
  const i = app.jobs.findIndex((j) => j.id === job.id);
  if (i >= 0) app.jobs[i] = job;
  else app.jobs.push(job);
}

let initialized = false;

export async function init() {
  if (initialized) return;
  initialized = true;

  await listen('job-update', (e) => mergeJob(e.payload));
  await listen('batch-update', (e) => {
    app.batch = e.payload;
  });
  await listen('queue-sync', (e) => {
    app.jobs = e.payload;
    if (app.selectedId == null && app.jobs.length) app.selectedId = app.jobs[0].id;
  });

  await getCurrentWebview().onDragDropEvent((e) => {
    if (e.payload.type === 'drop' && e.payload.paths?.length) {
      addPaths(e.payload.paths);
    }
  });

  app.settings = await invoke('get_settings');
  invoke('ffmpeg_version').then((v) => (app.ffmpegVersion = v));

  // Crash/close resume (PRD FR6): restore instead of restarting.
  const restored = await invoke('load_persisted');
  if (restored && restored.total > 0) {
    const left = restored.total - restored.done - restored.failed - restored.skipped;
    if (left > 0) app.resumedNote = `Resumed — ${left} left`;
  }
  const state = await invoke('get_state');
  app.jobs = state.jobs;
  app.batch = state.batch;
  if (app.jobs.length && app.selectedId == null) app.selectedId = app.jobs[0].id;
}

export async function addPaths(paths) {
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- local snapshot, not reactive state
  const known = new Set(app.jobs.map((j) => j.id));
  await invoke('add_paths', { paths });
  const state = await invoke('get_state');
  app.jobs = state.jobs;
  app.batch = state.batch;
  // Default preset applies to newly added files (HANDOFF Settings)
  const preset = PRESETS[app.settings.preset];
  if (preset) {
    for (const j of app.jobs) {
      if (!known.has(j.id)) {
        j.config.quality = preset.quality;
        j.config.targetMb = preset.targetMb;
        invoke('set_job_config', { id: j.id, config: $state.snapshot(j.config) });
      }
    }
  }
  if (app.selectedId == null && app.jobs.length) app.selectedId = app.jobs[0].id;
}

export async function pickFolder() {
  const dir = await open({ directory: true, multiple: false, title: 'Add folder' });
  if (dir) await addPaths([dir]);
}

export function syncJobConfig(job) {
  invoke('set_job_config', { id: job.id, config: $state.snapshot(job.config) });
}

export function generateSelected() {
  const job = selectedJob();
  if (!job || job.status === 'running') return;
  invoke('generate_one', { id: job.id, config: $state.snapshot(job.config) });
}

export function applyConfigToBatch() {
  const job = selectedJob();
  if (!job) return;
  const cfg = $state.snapshot(job.config);
  for (const j of app.jobs) j.config = structuredClone(cfg);
  invoke('apply_config_all', { config: cfg });
}

export const startBatch = () => invoke('start_batch');
export const pauseBatch = () => invoke('pause_batch');
export const stopBatch = () => invoke('stop_batch');

export function saveSettings() {
  invoke('set_settings', { settings: $state.snapshot(app.settings) });
}

// ------------------------------------------------------------ helpers

export function fmtDuration(s) {
  if (s == null) return '—';
  const t = Math.round(s);
  const p = (n) => String(n).padStart(2, '0');
  return `${p(Math.floor(t / 3600))}:${p(Math.floor((t % 3600) / 60))}:${p(t % 60)}`;
}

export function fmtMB(bytes) {
  return (bytes / 1e6).toFixed(1) + ' MB';
}

export function jobIsPortrait(job) {
  const o = job.config.orientation;
  if (o === 'portrait') return true;
  if (o === 'landscape') return false;
  return job.meta ? job.meta.height > job.meta.width : false;
}

/// Live size estimate (mint mono-num readout) — an estimate, not the encode.
export function estimateMB(job) {
  const c = job.config;
  const tiles = c.grid.cols * c.grid.rows;
  const q = c.quality / 100;
  let mb = 0;
  if (c.artifacts.animated) mb += (0.6 + 9.0 * q * q) * (tiles / 27);
  if (c.artifacts.staticSheet) mb += 0.3 + 2.2 * q * (tiles / 27);
  if (c.artifacts.montage) mb += 0.2 + 0.9 * q;
  return Math.max(0.1, mb);
}

export function writesTo(job) {
  if (job.config.outputMode === 'custom' && job.config.outputPath) return job.config.outputPath;
  const p = job.path ?? '';
  const sep = p.includes('\\') ? '\\' : '/';
  const dir = p.slice(0, p.lastIndexOf(sep) + 1);
  return dir + 'srcs' + sep;
}
