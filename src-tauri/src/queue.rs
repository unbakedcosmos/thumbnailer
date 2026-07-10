//! Batch engine (PRD §6.1): encode-aware bounded concurrency, per-file error
//! isolation, pause/stop/retry, persisted manifest for crash resume (FR6),
//! and live events to the UI. Decoupled from Tauri via the `Emitter` alias so
//! integration tests can drive it headless.

use crate::pipeline::{run_job, GenControl};
use crate::render::Fonts;
use crate::templates::TemplateStore;
use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

pub type Emitter = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

pub const VIDEO_EXTENSIONS: [&str; 14] = [
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "wmv", "flv", "ts", "m2ts", "mpg", "mpeg", "mts",
    "3gp",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Done,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BatchStatus {
    #[default]
    Idle,
    Ready,
    Running,
    Paused,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: u64,
    pub path: PathBuf,
    pub name: String,
    pub status: JobStatus,
    pub pct: f32,
    pub config: JobConfig,
    pub meta: Option<VideoMeta>,
    pub fail_reason: Option<String>,
    pub artifacts: Vec<ProducedArtifact>,
    pub degraded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchView {
    pub status: BatchStatus,
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub skipped: usize,
    pub running: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub concurrency: usize,
    pub preset: String,
    pub overwrite: bool,
    /// Seeds the animated target for newly-added files (CHANGELOG §3)
    pub default_target_mb: f64,
    /// Encode effort (speed ↔ quality/size). Global; default Balanced.
    #[serde(default)]
    pub effort: Effort,
    /// Manual knobs, active only when `effort == Custom`.
    #[serde(default)]
    pub advanced: Advanced,
}

impl Default for Settings {
    fn default() -> Self {
        // Encode-aware default (FR4): ffmpeg encodes are CPU/RAM bound — low N,
        // never the downloader-style I/O fan-out.
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Settings {
            concurrency: (cores / 2).clamp(1, 4),
            preset: "Balanced".into(),
            overwrite: false,
            default_target_mb: 8.0,
            effort: Effort::default(),
            advanced: Advanced::default(),
        }
    }
}

#[derive(Default)]
struct EngineState {
    jobs: Vec<Job>,
    batch: BatchStatus,
    cancels: HashMap<u64, CancellationToken>,
    scheduler_alive: bool,
}

pub struct Engine {
    state: Arc<Mutex<EngineState>>,
    pub settings: Arc<Mutex<Settings>>,
    pub templates: TemplateStore,
    emitter: Emitter,
    fonts: Arc<Fonts>,
    next_id: AtomicU64,
    data_dir: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    batch: BatchStatus,
    jobs: Vec<Job>,
}

impl Engine {
    pub fn new(emitter: Emitter, data_dir: PathBuf) -> Arc<Self> {
        let _ = std::fs::create_dir_all(&data_dir);
        let settings = Self::load_settings(&data_dir);
        Arc::new(Engine {
            state: Arc::new(Mutex::new(EngineState::default())),
            settings: Arc::new(Mutex::new(settings)),
            templates: TemplateStore::new(&data_dir),
            emitter,
            fonts: Arc::new(Fonts::load()),
            next_id: AtomicU64::new(1),
            data_dir,
        })
    }

    fn settings_path(dir: &Path) -> PathBuf {
        dir.join("settings.json")
    }
    fn manifest_path(&self) -> PathBuf {
        self.data_dir.join("manifest.json")
    }

    fn load_settings(dir: &Path) -> Settings {
        std::fs::read(Self::settings_path(dir))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self) {
        let s = self.settings.lock().unwrap().clone();
        if let Ok(b) = serde_json::to_vec_pretty(&s) {
            let _ = std::fs::write(Self::settings_path(&self.data_dir), b);
        }
    }

    // ---------------------------------------------------------- events

    fn emit_job(&self, job: &Job) {
        (self.emitter)("job-update", serde_json::to_value(job).unwrap());
    }

    fn batch_view(st: &EngineState) -> BatchView {
        BatchView {
            status: st.batch,
            total: st.jobs.len(),
            done: st
                .jobs
                .iter()
                .filter(|j| j.status == JobStatus::Done)
                .count(),
            failed: st
                .jobs
                .iter()
                .filter(|j| j.status == JobStatus::Failed)
                .count(),
            skipped: st
                .jobs
                .iter()
                .filter(|j| j.status == JobStatus::Skipped)
                .count(),
            running: st
                .jobs
                .iter()
                .filter(|j| j.status == JobStatus::Running)
                .count(),
        }
    }

    fn emit_batch(&self, st: &EngineState) {
        (self.emitter)(
            "batch-update",
            serde_json::to_value(Self::batch_view(st)).unwrap(),
        );
    }

    fn emit_queue(&self, st: &EngineState) {
        (self.emitter)("queue-sync", serde_json::to_value(&st.jobs).unwrap());
    }

    // ---------------------------------------------------------- manifest

    fn save_manifest(&self) {
        let st = self.state.lock().unwrap();
        let m = Manifest {
            version: 1,
            batch: st.batch,
            jobs: st.jobs.clone(),
        };
        drop(st);
        if let Ok(b) = serde_json::to_vec(&m) {
            let tmp = self.manifest_path().with_extension("tmp");
            if std::fs::write(&tmp, b).is_ok() {
                let _ = std::fs::rename(&tmp, self.manifest_path());
            }
        }
    }

    /// Restore the persisted queue after a crash/close (FR6). Only genuinely
    /// unfinished work is brought back: jobs that were mid-flight come back as
    /// queued, and completed work is never redone. A batch that already
    /// finished (nothing left to run) is NOT restored — the app opens fresh and
    /// the stale manifest is cleared, so a clean close means a clean start.
    pub fn load_manifest(self: &Arc<Self>) -> Option<BatchView> {
        let bytes = std::fs::read(self.manifest_path()).ok()?;
        let mut m: Manifest = serde_json::from_slice(&bytes).ok()?;

        // Mid-flight jobs from a crash resume as queued.
        for j in m.jobs.iter_mut() {
            if j.status == JobStatus::Running {
                j.status = JobStatus::Queued;
                j.pct = 0.0;
            }
        }
        // Fresh start unless there is actual work left to run.
        let pending = m.jobs.iter().any(|j| j.status == JobStatus::Queued);
        if !pending {
            let _ = std::fs::remove_file(self.manifest_path());
            return None;
        }

        let mut st = self.state.lock().unwrap();
        st.jobs = m.jobs;
        let max_id = st.jobs.iter().map(|j| j.id).max().unwrap_or(0);
        self.next_id.store(max_id + 1, Ordering::SeqCst);
        st.batch = BatchStatus::Paused;
        let view = Self::batch_view(&st);
        self.emit_queue(&st);
        self.emit_batch(&st);
        Some(view)
    }

    // ---------------------------------------------------------- queue ops

    /// Expand dropped/picked paths into video files (dirs walk recursively,
    /// skipping `srcs` output folders and dotfiles), then enqueue.
    pub fn add_paths(self: &Arc<Self>, paths: Vec<PathBuf>) -> usize {
        let mut files: Vec<PathBuf> = Vec::new();
        for p in paths {
            if p.is_dir() {
                walk_videos(&p, &mut files, 0);
            } else if is_video(&p) {
                files.push(p);
            }
        }
        files.sort();
        let mut default_config = JobConfig::default();
        default_config.animated.target_mb = self.settings.lock().unwrap().default_target_mb;
        let mut added = 0;
        {
            let mut st = self.state.lock().unwrap();
            let existing: std::collections::HashSet<PathBuf> =
                st.jobs.iter().map(|j| j.path.clone()).collect();
            for f in files {
                if existing.contains(&f) {
                    continue;
                }
                let id = self.next_id.fetch_add(1, Ordering::SeqCst);
                st.jobs.push(Job {
                    id,
                    name: f
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("video")
                        .to_string(),
                    path: f,
                    status: JobStatus::Queued,
                    pct: 0.0,
                    config: default_config.clone(),
                    meta: None,
                    fail_reason: None,
                    artifacts: Vec::new(),
                    degraded: false,
                });
                added += 1;
            }
            if st.batch == BatchStatus::Idle && !st.jobs.is_empty() {
                st.batch = BatchStatus::Ready;
            }
            if st.batch == BatchStatus::Complete && added > 0 {
                st.batch = BatchStatus::Paused;
            }
            self.emit_queue(&st);
            self.emit_batch(&st);
        }
        self.save_manifest();
        added
    }

    /// Backfill metadata asynchronously (HANDOFF: probes resolve after rows
    /// appear). Must be called from within the tokio runtime.
    pub fn spawn_probes(self: &Arc<Self>) {
        let me = self.clone();
        tokio::spawn(async move {
            let ids: Vec<(u64, PathBuf)> = {
                let st = me.state.lock().unwrap();
                st.jobs
                    .iter()
                    .filter(|j| j.meta.is_none() && j.status == JobStatus::Queued)
                    .map(|j| (j.id, j.path.clone()))
                    .collect()
            };
            for chunk in ids.chunks(3) {
                let mut set = tokio::task::JoinSet::new();
                for (id, path) in chunk.iter().cloned() {
                    set.spawn(async move { (id, crate::probe::probe(&path).await) });
                }
                while let Some(Ok((id, res))) = set.join_next().await {
                    let mut st = me.state.lock().unwrap();
                    if let Some(job) = st.jobs.iter_mut().find(|j| j.id == id) {
                        match res {
                            Ok(meta) => job.meta = Some(meta),
                            Err(f) => {
                                // Unreadable at probe time — fail fast, zero silent skips (SM2)
                                if job.status == JobStatus::Queued {
                                    job.status = JobStatus::Failed;
                                    job.fail_reason = Some(f.to_string());
                                }
                            }
                        }
                        let j = job.clone();
                        drop(st);
                        me.emit_job(&j);
                        let st = me.state.lock().unwrap();
                        me.emit_batch(&st);
                    }
                }
            }
            me.save_manifest();
        });
    }

    pub fn set_job_config(&self, id: u64, config: JobConfig) {
        let mut st = self.state.lock().unwrap();
        if let Some(j) = st.jobs.iter_mut().find(|j| j.id == id) {
            j.config = config;
        }
    }

    pub fn apply_config_all(&self, config: JobConfig) {
        let mut st = self.state.lock().unwrap();
        for j in st.jobs.iter_mut() {
            j.config = config.clone();
        }
        self.emit_queue(&st);
    }

    pub fn clear(&self) {
        let mut st = self.state.lock().unwrap();
        for c in st.cancels.values() {
            c.cancel();
        }
        st.cancels.clear();
        st.jobs.clear();
        st.batch = BatchStatus::Idle;
        self.emit_queue(&st);
        self.emit_batch(&st);
        drop(st);
        self.save_manifest();
    }

    /// Drop a single file from the queue (cancelling it first if it's running).
    /// Emptying the queue this way resets the batch to Idle, same as `clear`.
    pub fn remove_job(&self, id: u64) {
        let mut st = self.state.lock().unwrap();
        if let Some(c) = st.cancels.remove(&id) {
            c.cancel();
        }
        st.jobs.retain(|j| j.id != id);
        if st.jobs.is_empty() {
            st.batch = BatchStatus::Idle;
        }
        self.emit_queue(&st);
        self.emit_batch(&st);
        drop(st);
        self.save_manifest();
    }

    pub fn jobs_snapshot(&self) -> (Vec<Job>, BatchView) {
        let st = self.state.lock().unwrap();
        (st.jobs.clone(), Self::batch_view(&st))
    }

    // ---------------------------------------------------------- batch control

    pub fn start_batch(self: &Arc<Self>) {
        {
            let mut st = self.state.lock().unwrap();
            if st.batch == BatchStatus::Running {
                return;
            }
            st.batch = BatchStatus::Running;
            self.emit_batch(&st);
        }
        self.ensure_scheduler();
    }

    /// Pause stops dequeuing new files; in-flight encodes finish (an encode
    /// can't be frozen mid-ffmpeg without losing its work).
    pub fn pause_batch(&self) {
        let mut st = self.state.lock().unwrap();
        if st.batch == BatchStatus::Running {
            st.batch = BatchStatus::Paused;
            self.emit_batch(&st);
        }
    }

    /// Stop cancels in-flight work and resets running rows to queued (HANDOFF).
    pub fn stop_batch(&self) {
        let mut st = self.state.lock().unwrap();
        for c in st.cancels.values() {
            c.cancel();
        }
        st.batch = BatchStatus::Ready;
        self.emit_batch(&st);
    }

    /// Retry / generate-one: requeue a single file and process it now,
    /// independent of the batch (EXPERIENCE: the queue keeps draining behind it).
    pub fn generate_one(self: &Arc<Self>, id: u64, config: Option<JobConfig>) {
        {
            let mut st = self.state.lock().unwrap();
            let Some(j) = st.jobs.iter_mut().find(|j| j.id == id) else {
                return;
            };
            if j.status == JobStatus::Running {
                return;
            }
            if let Some(c) = config {
                j.config = c;
            }
            j.status = JobStatus::Queued;
            j.pct = 0.0;
            j.fail_reason = None;
            let j = j.clone();
            self.emit_job(&j);
            self.emit_batch(&st);
        }
        let me = self.clone();
        // Per-file Generate honors the same overwrite policy as the batch: ON
        // replaces the file, OFF preserves it and writes a numbered copy.
        let overwrite = self.settings.lock().unwrap().overwrite;
        tokio::spawn(async move { me.run_one(id, overwrite).await });
    }

    fn ensure_scheduler(self: &Arc<Self>) {
        {
            let mut st = self.state.lock().unwrap();
            if st.scheduler_alive {
                return;
            }
            st.scheduler_alive = true;
        }
        let me = self.clone();
        tokio::spawn(async move {
            loop {
                let next: Option<u64> = {
                    let mut st = me.state.lock().unwrap();
                    if st.batch != BatchStatus::Running {
                        st.scheduler_alive = false;
                        break;
                    }
                    let cap = me.settings.lock().unwrap().concurrency.max(1);
                    let running = st
                        .jobs
                        .iter()
                        .filter(|j| j.status == JobStatus::Running)
                        .count();
                    if running >= cap {
                        None
                    } else {
                        let id = st
                            .jobs
                            .iter()
                            .find(|j| j.status == JobStatus::Queued)
                            .map(|j| j.id);
                        if id.is_none() && running == 0 {
                            st.batch = BatchStatus::Complete;
                            st.scheduler_alive = false;
                            me.emit_batch(&st);
                            drop(st);
                            me.save_manifest();
                            break;
                        }
                        id
                    }
                };
                if let Some(id) = next {
                    // Mark running before spawning so the loop doesn't double-pick.
                    // Re-check Queued: a per-file Generate may have claimed it
                    // between our find and this lock.
                    let claimed = {
                        let mut st = me.state.lock().unwrap();
                        match st
                            .jobs
                            .iter_mut()
                            .find(|j| j.id == id && j.status == JobStatus::Queued)
                        {
                            Some(j) => {
                                j.status = JobStatus::Running;
                                j.pct = 0.0;
                                let j = j.clone();
                                self_emit(&me, &st, &j);
                                true
                            }
                            None => false,
                        }
                    };
                    if claimed {
                        let me2 = me.clone();
                        let overwrite = me.settings.lock().unwrap().overwrite;
                        tokio::spawn(async move { me2.run_marked(id, overwrite).await });
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(120)).await;
            }
        });
    }

    async fn run_one(self: &Arc<Self>, id: u64, overwrite: bool) {
        {
            let mut st = self.state.lock().unwrap();
            let Some(j) = st
                .jobs
                .iter_mut()
                .find(|j| j.id == id && j.status == JobStatus::Queued)
            else {
                return;
            };
            j.status = JobStatus::Running;
            j.pct = 0.0;
            let j = j.clone();
            self.emit_job(&j);
            self.emit_batch(&st);
        }
        self.run_marked(id, overwrite).await;
    }

    /// Run a job already marked Running. Per-file error isolation (FR5): every
    /// outcome is recorded; nothing here can take down the batch.
    async fn run_marked(self: &Arc<Self>, id: u64, overwrite: bool) {
        let (path, config) = {
            let st = self.state.lock().unwrap();
            let Some(j) = st.jobs.iter().find(|j| j.id == id) else {
                return;
            };
            (j.path.clone(), j.config.clone())
        };

        let token = CancellationToken::new();
        self.state.lock().unwrap().cancels.insert(id, token.clone());

        let me = self.clone();
        let progress: Box<dyn Fn(f32) + Send + Sync> = Box::new(move |p| {
            let mut st = me.state.lock().unwrap();
            if let Some(j) = st.jobs.iter_mut().find(|j| j.id == id) {
                let new_pct = (p * 100.0).round();
                if (new_pct - j.pct).abs() >= 1.0 {
                    j.pct = new_pct;
                    let j = j.clone();
                    drop(st);
                    me.emit_job(&j);
                }
            }
        });

        let params = {
            let s = self.settings.lock().unwrap();
            EncodeParams::resolve(s.effort, &s.advanced)
        };
        let ctl = GenControl {
            cancel: token.clone(),
            overwrite,
            params,
            progress,
        };
        let template = self.templates.get(&config.static_cfg.template_id);
        let result = run_job(&path, &config, &template, &self.fonts, &ctl).await;

        let mut st = self.state.lock().unwrap();
        st.cancels.remove(&id);
        if let Some(j) = st.jobs.iter_mut().find(|j| j.id == id) {
            match result {
                Ok((meta, outcome)) => {
                    j.meta = Some(meta);
                    j.degraded = outcome.artifacts.iter().any(|a| a.degraded);
                    if outcome.artifacts.is_empty() && !outcome.skipped_existing.is_empty() {
                        j.status = JobStatus::Skipped;
                        j.fail_reason = Some("already exists".into());
                    } else {
                        j.status = JobStatus::Done;
                        j.fail_reason = None;
                        j.artifacts = outcome.artifacts;
                    }
                    j.pct = 100.0;
                }
                Err(Failure::Cancelled) => {
                    j.status = JobStatus::Queued;
                    j.pct = 0.0;
                }
                Err(f @ Failure::QualityFloor(_)) => {
                    // EXPERIENCE state model: can't-fit-at-floor surfaces as a
                    // warning skip — never a silent oversize file (FR16)
                    j.status = JobStatus::Skipped;
                    j.fail_reason = Some(f.to_string());
                    j.pct = 100.0;
                }
                Err(f) => {
                    j.status = JobStatus::Failed;
                    j.fail_reason = Some(f.to_string());
                    j.pct = 100.0;
                }
            }
            let j = j.clone();
            self.emit_job(&j);
        }
        self.emit_batch(&st);
        drop(st);
        self.save_manifest();
    }
}

fn self_emit(me: &Arc<Engine>, st: &EngineState, j: &Job) {
    me.emit_job(j);
    me.emit_batch(st);
}

fn is_video(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Streaming-ish recursive walk (FR3): depth-capped, skips hidden dirs and
/// `srcs` output folders so we never enqueue our own artifacts.
fn walk_videos(dir: &Path, out: &mut Vec<PathBuf>, depth: u32) {
    if depth > 12 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            if name.eq_ignore_ascii_case("srcs") {
                continue;
            }
            walk_videos(&p, out, depth + 1);
        } else if is_video(&p) {
            out.push(p);
        }
    }
}
