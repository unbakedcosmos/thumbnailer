// App state + Tauri IPC wiring (HANDOFF State Management). Real progress comes
// from Rust-side job events — no client-side timers.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open, confirm } from '@tauri-apps/plugin-dialog';
import { openUrl, revealItemInDir } from '@tauri-apps/plugin-opener';

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
  applyToAll: true, // editor edits propagate to every queued file by default
  queueFilter: 'all', // 'all' | 'issues'
  resumedNote: null,
  ffmpegVersion: null,
  ffmpegReady: null, // null = still checking; true/false once probed
  ffmpegBinDir: null,
  ffmpegChecking: false,
  ffmpegPromptDismissed: false,
  settings: {
    concurrency: 2,
    preset: 'Balanced',
    overwrite: false,
    defaultTargetMb: 8,
    effort: 'balanced'
  },
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
  recheckFfmpeg();

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

// Empty the whole queue (cancels anything running). Confirmed first because it
// throws away every added file and any in-flight encode.
export async function clearQueue() {
  if (!app.jobs.length) return;
  const running = app.batch.status === 'running';
  const ok = await confirm(
    running
      ? 'A batch is running. Clear the queue and cancel it?'
      : 'Remove all files from the queue?',
    { title: 'Clear queue', kind: 'warning' }
  );
  if (!ok) return;
  await invoke('clear_queue');
  app.jobs = [];
  app.batch = { status: 'idle', total: 0, done: 0, failed: 0, skipped: 0, running: 0 };
  app.selectedId = null;
  app.resumedNote = null;
  app.queueFilter = 'all';
}

// Drop one file from the queue; keep the selection sensible if it was selected.
export async function removeJob(id) {
  await invoke('remove_job', { id });
  app.jobs = app.jobs.filter((j) => j.id !== id);
  if (app.selectedId === id) app.selectedId = app.jobs[0]?.id ?? null;
  if (!app.jobs.length) app.resumedNote = null;
}

export function syncJobConfig(job) {
  const cfg = $state.snapshot(job.config);
  if (app.applyToAll) {
    // Apply-to-all default: push the edited config onto every other queued file.
    for (const j of app.jobs) if (j.id !== job.id) j.config = structuredClone(cfg);
    invoke('apply_config_all', { config: cfg });
  } else {
    invoke('set_job_config', { id: job.id, config: cfg });
  }
}

// Persist one job's config only — never fans out to the batch. Used for silent
// normalization (e.g. coercing a legacy multi-artifact config to single-select)
// where touching other files would be surprising.
export function syncJobConfigLocal(job) {
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

// ------------------------------------------------------------ ffmpeg detection

/// Re-probe ffmpeg/ffprobe (discovery is uncached, so a just-dropped binary is
/// picked up live). Drives the guided "ffmpeg not found" prompt.
export async function recheckFfmpeg() {
  app.ffmpegChecking = true;
  try {
    const s = await invoke('ffmpeg_status');
    app.ffmpegVersion = s.version;
    app.ffmpegReady = s.ready;
    app.ffmpegBinDir = s.binDir;
  } finally {
    app.ffmpegChecking = false;
  }
}

export function openFfmpegDownload() {
  return openUrl('https://ffmpeg.org/download.html');
}

export async function openFfmpegFolder() {
  if (app.ffmpegBinDir) await revealItemInDir(app.ffmpegBinDir);
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

// Estimates are pre-encode guesses, calibrated against measured output on the
// demo library (see src-tauri/tests). They scale with tile count, quality and
// format; the animated/montage previews are size-gated (pipeline.rs), so their
// estimate is clamped at the target — the encoder never emits above it.

/// Animated-grid estimate. Per-tile cost grows with quality; the total is
/// capped at the target since the auto-fit ladder guarantees it fits.
export function estimateAnimatedMB(job) {
  const c = job.config;
  const q = c.animated.quality / 100;
  const tiles = c.grid.cols * c.grid.rows;
  // ~1.5 MB floor + ~8.6 MB/full-quality for a 27-tile WebP grid, scaled linearly
  // by tile count (the ladder holds tile resolution ∝ quality, not grid size).
  const raw = (1.5 + 8.6 * q) * (tiles / 27) * (c.animated.format === 'gif' ? 1.8 : 1);
  return Math.max(0.1, Math.min(raw, c.animated.targetMb));
}

/// Single-cell montage loop estimate (~6 sequential clips). Also target-gated,
/// though a single cell rarely reaches it.
export function estimateMontageMB(job) {
  const c = job.config;
  const q = c.animated.quality / 100;
  const raw = (0.3 + q * 0.9) * (c.animated.format === 'gif' ? 1.8 : 1);
  return Math.max(0.05, Math.min(raw, c.animated.targetMb));
}

/// Static sheet estimate — one composed image. The sheet frame is always
/// encoded crisp (high fixed quality) and only the media inside each tile is
/// degraded to the quality setting, so size is driven mostly by format + tile
/// count and is nearly flat in quality (measured on the demo library). WebP is
/// markedly smaller than JPEG; PNG is lossless.
export function estimateStaticMB(job) {
  const c = job.config;
  const tiles = c.grid.cols * c.grid.rows;
  const q = c.static.quality / 100;
  const perTile =
    c.static.format === 'png'
      ? 0.05 // lossless — flat (sharper frames since lanczos + sharpest-pick)
      : c.static.format === 'webp'
        ? 0.018 + 0.002 * q
        : 0.052 + 0.006 * q; // jpeg — crisp-frame floor dominates
  return Math.max(0.05, tiles * perTile);
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
