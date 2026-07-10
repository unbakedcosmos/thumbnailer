//! Generate artifacts for one video from the CLI (dev/inspection tool).
//! Usage: cargo run --example gensheet -- <video> [cols] [rows] [quality] [target_mb] [template]

use thumbnailer_lib::pipeline::{run_job, GenControl};
use thumbnailer_lib::render::Fonts;
use thumbnailer_lib::types::*;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = std::path::PathBuf::from(
        args.get(1)
            .expect("usage: gensheet <video> [cols] [rows] [q] [mb] [template]"),
    );
    let cols: u32 = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(3);
    let rows: u32 = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(9);
    let quality: u8 = args.get(4).map(|s| s.parse().unwrap()).unwrap_or(62);
    let target_mb: f64 = args.get(5).map(|s| s.parse().unwrap()).unwrap_or(8.0);
    let template_id = args.get(6).cloned().unwrap_or_else(|| "classic".into());

    let config = JobConfig {
        grid: GridDims { cols, rows },
        artifacts: ArtifactSet {
            static_sheet: true,
            animated: true,
            montage: true,
        },
        animated: AnimatedConfig {
            quality,
            target_mb,
            ..Default::default()
        },
        static_cfg: StaticConfig {
            template_id: template_id.clone(),
            ..Default::default()
        },
        ..Default::default()
    };
    let template = builtin_templates()
        .into_iter()
        .find(|t| t.id == template_id)
        .unwrap_or_default();
    let fonts = Fonts::load();
    let ctl = GenControl {
        cancel: CancellationToken::new(),
        overwrite: true,
        effort: thumbnailer_lib::types::Effort::default(),
        progress: Box::new(|p| eprint!("\r{:5.1}%", p * 100.0)),
    };
    let started = std::time::Instant::now();
    match run_job(&path, &config, &template, &fonts, &ctl).await {
        Ok((meta, outcome)) => {
            eprintln!(
                "\n{}×{} {:.1}s {} fps {}",
                meta.width, meta.height, meta.duration_s, meta.fps, meta.codec
            );
            for a in outcome.artifacts {
                println!(
                    "{} · {:.2} MB{}",
                    a.path,
                    a.bytes as f64 / 1e6,
                    if a.degraded { " · degraded" } else { "" }
                );
            }
            eprintln!("wall: {:.1}s", started.elapsed().as_secs_f64());
        }
        Err(f) => {
            eprintln!("\nFAILED: {f}");
            std::process::exit(1);
        }
    }
}
