//! Tauri IPC surface (HANDOFF State Management: scan + probe are Rust-side
//! commands; progress arrives as events, never client-side timers).

use crate::queue::{BatchView, Engine, Job, Settings};
use crate::types::{FrameTemplate, JobConfig};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

type Eng<'a> = State<'a, Arc<Engine>>;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FullState {
    pub jobs: Vec<Job>,
    pub batch: BatchView,
}

#[tauri::command]
pub async fn add_paths(engine: Eng<'_>, paths: Vec<String>) -> Result<usize, String> {
    let e = engine.inner().clone();
    let added = tokio::task::spawn_blocking(move || {
        e.add_paths(paths.into_iter().map(PathBuf::from).collect())
    })
    .await
    .map_err(|e| e.to_string())?;
    engine.inner().spawn_probes();
    Ok(added)
}

#[tauri::command]
pub async fn get_state(engine: Eng<'_>) -> Result<FullState, String> {
    let (jobs, batch) = engine.jobs_snapshot();
    Ok(FullState { jobs, batch })
}

/// Restore the crash/close manifest (FR6). Returns the batch view if a
/// previous session was restored, so the UI can say "Resumed — N left".
#[tauri::command]
pub async fn load_persisted(engine: Eng<'_>) -> Result<Option<BatchView>, String> {
    Ok(engine.inner().load_manifest())
}

#[tauri::command]
pub async fn start_batch(engine: Eng<'_>) -> Result<(), String> {
    engine.inner().start_batch();
    Ok(())
}

#[tauri::command]
pub async fn pause_batch(engine: Eng<'_>) -> Result<(), String> {
    engine.pause_batch();
    Ok(())
}

#[tauri::command]
pub async fn stop_batch(engine: Eng<'_>) -> Result<(), String> {
    engine.stop_batch();
    Ok(())
}

#[tauri::command]
pub async fn clear_queue(engine: Eng<'_>) -> Result<(), String> {
    engine.clear();
    Ok(())
}

#[tauri::command]
pub async fn remove_job(engine: Eng<'_>, id: u64) -> Result<(), String> {
    engine.remove_job(id);
    Ok(())
}

#[tauri::command]
pub async fn generate_one(
    engine: Eng<'_>,
    id: u64,
    config: Option<JobConfig>,
) -> Result<(), String> {
    engine.inner().generate_one(id, config);
    Ok(())
}

#[tauri::command]
pub async fn set_job_config(engine: Eng<'_>, id: u64, config: JobConfig) -> Result<(), String> {
    engine.set_job_config(id, config);
    Ok(())
}

#[tauri::command]
pub async fn apply_config_all(engine: Eng<'_>, config: JobConfig) -> Result<(), String> {
    engine.apply_config_all(config);
    Ok(())
}

#[tauri::command]
pub async fn get_settings(engine: Eng<'_>) -> Result<Settings, String> {
    Ok(engine.settings.lock().unwrap().clone())
}

#[tauri::command]
pub async fn set_settings(engine: Eng<'_>, settings: Settings) -> Result<(), String> {
    *engine.settings.lock().unwrap() = settings;
    engine.save_settings();
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegStatus {
    /// ffmpeg version string, or null if ffmpeg couldn't be run.
    pub version: Option<String>,
    /// True only when BOTH ffmpeg and ffprobe are usable — the app needs both.
    pub ready: bool,
    /// Writable folder the user can drop ffmpeg/ffprobe into (for the prompt).
    pub bin_dir: Option<String>,
}

/// Re-probed on demand (discovery is uncached) so a dropped binary is seen live.
#[tauri::command]
pub async fn ffmpeg_status() -> Result<FfmpegStatus, String> {
    let version = crate::ffmpeg::ffmpeg_version().await;
    let ready = version.is_some() && crate::ffmpeg::ffprobe_ok().await;
    let bin_dir = crate::ffmpeg::bin_dir().map(|p| p.to_string_lossy().into_owned());
    Ok(FfmpegStatus {
        version,
        ready,
        bin_dir,
    })
}

// Frame templates are user data (CHANGELOG §2): persisted in templates.json,
// shared across batches and sessions.

#[tauri::command]
pub async fn list_templates(engine: Eng<'_>) -> Result<Vec<FrameTemplate>, String> {
    Ok(engine.templates.list())
}

#[tauri::command]
pub async fn save_template(
    engine: Eng<'_>,
    template: FrameTemplate,
) -> Result<FrameTemplate, String> {
    engine.templates.save(template)
}

#[tauri::command]
pub async fn delete_template(engine: Eng<'_>, id: String) -> Result<(), String> {
    engine.templates.delete(&id)
}
