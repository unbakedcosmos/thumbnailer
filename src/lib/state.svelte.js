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
  Small: { animQuality: 40, targetMb: 4, staticQuality: 60 },
  Balanced: { animQuality: 62, targetMb: 8, staticQuality: 80 },
  'Max quality': { animQuality: 85, targetMb: 12, staticQuality: 92 }
};

export const VIDEO_FILTER = {
  name: 'Videos',
  extensions: [
    'mp4',
    'm4v',
    'mov',
    'mkv',
    'webm',
    'avi',
    'wmv',
    'flv',
    'ts',
    'm2ts',
    'mpg',
    'mpeg',
    'mts',
    '3gp'
  ]
};

export const app = $state({
  jobs: [],
  batch: { status: 'idle', total: 0, done: 0, failed: 0, skipped: 0, running: 0 },
  selectedId: null,
  settingsOpen: false,
  follow: true,
  queueFilter: 'all', // 'all' | 'issues'
  resumedNote: null,
  ffmpegVersion: null,
  settings: { concurrency: 2, preset: 'Balanced', overwrite: false, defaultTargetMb: 8 },
  templates: [],
  templateGalleryOpen: false,
  templateEditor: null // { id, name, headerBand, border, timestampStyle, accent, isNew }
});

export function selectedJob() {
  return app.jobs.find((j) => j.id === app.selectedId) ?? null;
}

export function visibleJobs() {
  if (app.queueFilter === 'issues') {
    return app.jobs.filter((j) => j.status === 'failed' || j.status === 'skipped');
  }
  return app.jobs;
}

export function templateById(id) {
  return app.templates.find((t) => t.id === id) ?? app.templates[0] ?? null;
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
  app.templates = await invoke('list_templates');
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
        j.config.animated.quality = preset.animQuality;
        j.config.static.quality = preset.staticQuality;
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

// "+ Add files": single/multi-file picker, not just folder scan (CHANGELOG §3)
export async function pickFiles() {
  const files = await open({ multiple: true, title: 'Add files', filters: [VIDEO_FILTER] });
  if (files?.length) await addPaths(files);
}

export function syncJobConfig(job) {
  invoke('set_job_config', { id: job.id, config: $state.snapshot(job.config) });
}

export function generateSelected() {
  const job = selectedJob();
  if (!job || job.status === 'running') return;
  invoke('generate_one', { id: job.id, config: $state.snapshot(job.config) });
}

export function retryJob(id) {
  const job = app.jobs.find((j) => j.id === id);
  if (!job || job.status === 'running') return;
  app.selectedId = id;
  invoke('generate_one', { id, config: $state.snapshot(job.config) });
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

// ------------------------------------------------------------ templates

export async function saveTemplate(draft) {
  const saved = await invoke('save_template', { template: draft });
  const i = app.templates.findIndex((t) => t.id === saved.id);
  if (i >= 0) app.templates[i] = saved;
  else app.templates.push(saved);
  return saved;
}

export async function deleteTemplate(id) {
  await invoke('delete_template', { id });
  app.templates = app.templates.filter((t) => t.id !== id);
  // Files pointing at the deleted template fall back to Classic
  for (const j of app.jobs) {
    if (j.config.static.templateId === id) {
      j.config.static.templateId = 'classic';
      syncJobConfig(j);
    }
  }
}

/// Import stand-in (CHANGELOG §2: thinnest possible entry point; real flow TBD)
export async function importTemplate() {
  await saveTemplate({
    id: '',
    name: 'Imported template',
    headerBand: true,
    border: 'hairline',
    timestampStyle: 'overlay',
    accent: 'white',
    builtin: false
  });
}

export function accentColor(a) {
  return a === 'mint' ? '#9fe8b0' : a === 'white' ? '#eef0f4' : '#5b606c';
}

export function borderFor(border, accent) {
  if (border === 'none') return 'none';
  if (border === 'thick') return '2px solid ' + accentColor(accent);
  return '1px solid #2a2c33';
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

/// Animated estimate (mint mono-num readout) — an estimate, not the encode.
/// Covers the grid when Animated is on, plus the single-cell montage loop.
export function estimateAnimatedMB(job) {
  const c = job.config;
  const q = c.animated.quality / 100;
  let base = 0;
  if (c.artifacts.animated) {
    const tiles = c.grid.cols * c.grid.rows;
    base += Math.max(0.8, q * 9.5) * (tiles / 27);
  }
  if (c.artifacts.montage) base += 0.3 + q * 0.9;
  return (c.animated.format === 'gif' ? 1.8 : 1) * Math.max(0.2, base);
}

/// Static sheet estimate (per prototype: tiles × factor, PNG fixed).
export function estimateStaticMB(job) {
  const c = job.config;
  const effTiles = c.grid.cols * c.grid.rows;
  const isPng = c.static.format === 'png';
  const factor = isPng ? 1 : 0.15 + (c.static.quality / 100) * 0.8;
  return Math.max(0.3, effTiles * 0.14 * factor);
}

const EXT = { png: 'png', jpeg: 'jpg', webp: 'webp', gif: 'gif' };

export function outputFilesNote(job) {
  const c = job.config;
  const parts = [];
  if (c.artifacts.staticSheet) parts.push('_contact.' + EXT[c.static.format]);
  if (c.artifacts.animated) parts.push('_animated.' + EXT[c.animated.format]);
  if (c.artifacts.montage) parts.push('_montage.' + EXT[c.animated.format]);
  return parts.length ? parts.join(' · ') : 'no artifacts selected';
}

export function writesTo(job) {
  if (job.config.outputMode === 'custom' && job.config.outputPath) return job.config.outputPath;
  const p = job.path ?? '';
  const sep = p.includes('\\') ? '\\' : '/';
  const dir = p.slice(0, p.lastIndexOf(sep) + 1);
  return dir + 'srcs' + sep;
}
