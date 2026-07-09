pub mod commands;
pub mod extract;
pub mod ffmpeg;
pub mod pipeline;
pub mod probe;
pub mod queue;
pub mod render;
pub mod templates;
pub mod theme;
pub mod types;

use queue::Engine;
use std::sync::Arc;
use tauri::{Emitter as _, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let emitter: queue::Emitter = Arc::new(move |event, payload| {
                let _ = handle.emit(event, payload);
            });
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("thumbnailer"));
            // Where the guided prompt drops a user-supplied ffmpeg/ffprobe.
            ffmpeg::set_bundle_dir(data_dir.join("binaries"));
            let engine = Engine::new(emitter, data_dir);
            // Dev/test hook: pre-load paths without the native folder dialog
            // (used by headless UI verification; harmless in production).
            if let Ok(paths) = std::env::var("THUMBNAILER_ADD_PATHS") {
                let eng = engine.clone();
                tauri::async_runtime::spawn(async move {
                    let paths: Vec<std::path::PathBuf> =
                        paths.split(';').map(std::path::PathBuf::from).collect();
                    let eng2 = eng.clone();
                    let _ = tokio::task::spawn_blocking(move || eng2.add_paths(paths)).await;
                    eng.spawn_probes();
                });
            }
            app.manage(engine);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::add_paths,
            commands::get_state,
            commands::load_persisted,
            commands::start_batch,
            commands::pause_batch,
            commands::stop_batch,
            commands::clear_queue,
            commands::remove_job,
            commands::generate_one,
            commands::set_job_config,
            commands::apply_config_all,
            commands::get_settings,
            commands::set_settings,
            commands::ffmpeg_status,
            commands::list_templates,
            commands::save_template,
            commands::delete_template,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
