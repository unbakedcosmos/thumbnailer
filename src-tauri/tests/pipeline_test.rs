//! End-to-end pipeline tests on synthetic footage (PRD success metrics:
//! size guarantee, orientation-aware grids, robustness on truncated files,
//! idempotent re-runs, batch engine with zero silent skips).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use thumbnailer_lib::pipeline::{run_job, GenControl};
use thumbnailer_lib::queue::{BatchStatus, Engine, JobStatus};
use thumbnailer_lib::render::Fonts;
use thumbnailer_lib::types::*;
use tokio_util::sync::CancellationToken;

fn make_video(dir: &Path, name: &str, w: u32, h: u32, dur: f64) -> PathBuf {
    let path = dir.join(name);
    let status = Command::new("ffmpeg")
        .args([
            "-y", "-f", "lavfi", "-i",
            &format!("testsrc2=size={w}x{h}:rate=30:duration={dur}"),
            "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p",
        ])
        .arg(&path)
        .output()
        .expect("ffmpeg runs");
    assert!(status.status.success(), "fixture encode failed");
    path
}

fn test_config(grid: GridDims) -> JobConfig {
    JobConfig {
        grid,
        artifacts: ArtifactSet { static_sheet: true, animated: true, montage: true },
        ..Default::default()
    }
}

fn ctl() -> GenControl {
    GenControl {
        cancel: CancellationToken::new(),
        overwrite: false,
        progress: Box::new(|_| {}),
    }
}

fn assert_under_target(outcome: &JobOutcome, target_mb: f64) {
    for a in &outcome.artifacts {
        let on_disk = std::fs::metadata(&a.path).expect("artifact exists").len();
        assert_eq!(on_disk, a.bytes, "reported size matches disk");
        assert!(
            on_disk as f64 <= target_mb * 1_000_000.0,
            "{} is {} bytes, over the {target_mb} MB hard target (FR16)",
            a.path,
            on_disk
        );
        assert!(on_disk > 0, "artifact is not empty");
    }
}

#[tokio::test]
async fn landscape_produces_all_artifacts_under_target() {
    let tmp = tempfile::tempdir().unwrap();
    let video = make_video(tmp.path(), "Chair Play [5587459].mp4", 1280, 720, 12.0);
    let fonts = Fonts::load();
    let config = test_config(GridDims { cols: 3, rows: 3 });

    let (meta, outcome) = run_job(&video, &config, &fonts, &ctl()).await.expect("job succeeds");
    assert_eq!(meta.width, 1280);
    assert!(!meta.is_portrait());
    assert_eq!(outcome.artifacts.len(), 3, "static + animated + montage");
    assert_under_target(&outcome, config.target_mb);

    // Artifacts land in srcs/ next to the source, named by convention (FR23)
    let srcs = tmp.path().join("srcs");
    assert!(srcs.join("Chair Play [5587459]_contact.png").exists()
        || srcs.join("Chair Play [5587459]_contact.jpg").exists());
    assert!(srcs.join("Chair Play [5587459]_contact.webp").exists());
    assert!(srcs.join("Chair Play [5587459]_loop.webp").exists());

    // Static sheet: landscape tiles → sheet wider than tall for a 3×3 grid
    let png = outcome.artifacts.iter().find(|a| a.kind == ArtifactKind::Static).unwrap();
    let img = image::open(&png.path).expect("static sheet decodes");
    assert!(img.width() > 800, "2× render is crisp, got {}", img.width());
}

#[tokio::test]
async fn portrait_gets_orientation_aware_tiles() {
    let tmp = tempfile::tempdir().unwrap();
    let video = make_video(tmp.path(), "Backstage Vert [5588204].mp4", 720, 1280, 10.0);
    let fonts = Fonts::load();
    let mut config = test_config(GridDims { cols: 3, rows: 3 });
    config.artifacts = ArtifactSet { static_sheet: true, animated: false, montage: false };

    let (meta, outcome) = run_job(&video, &config, &fonts, &ctl()).await.expect("job succeeds");
    assert!(meta.is_portrait());
    assert_under_target(&outcome, config.target_mb);

    // 3 cols of 9:16 tiles → the sheet must be taller than wide (FR8a)
    let sheet = &outcome.artifacts[0];
    let img = image::open(&sheet.path).unwrap();
    assert!(
        img.height() > img.width(),
        "portrait grid should be tall, got {}×{}",
        img.width(),
        img.height()
    );
}

#[tokio::test]
async fn truncated_file_fails_as_unreadable() {
    let tmp = tempfile::tempdir().unwrap();
    let video = make_video(tmp.path(), "Broken Grab [5588999].mp4", 640, 360, 5.0);
    // Chop the file: mp4 moov atom sits at the end → unreadable
    let bytes = std::fs::read(&video).unwrap();
    std::fs::write(&video, &bytes[..bytes.len() / 10]).unwrap();

    let fonts = Fonts::load();
    let config = test_config(GridDims { cols: 2, rows: 2 });
    let err = run_job(&video, &config, &fonts, &ctl()).await.expect_err("must fail");
    assert!(
        matches!(err, Failure::Unreadable(_)),
        "typed reason is unreadable (FR5), got: {err:?}"
    );
    // And nothing was silently written (FR16 counter-metric)
    assert!(!tmp.path().join("srcs").exists() || std::fs::read_dir(tmp.path().join("srcs")).unwrap().count() == 0);
}

#[tokio::test]
async fn rerun_is_idempotent_until_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let video = make_video(tmp.path(), "Quick Clip [5589801].mp4", 640, 360, 6.0);
    let fonts = Fonts::load();
    let mut config = test_config(GridDims { cols: 2, rows: 2 });
    config.artifacts = ArtifactSet { static_sheet: true, animated: false, montage: false };

    let (_, first) = run_job(&video, &config, &fonts, &ctl()).await.unwrap();
    assert_eq!(first.artifacts.len(), 1);
    let produced = PathBuf::from(&first.artifacts[0].path);
    let mtime1 = std::fs::metadata(&produced).unwrap().modified().unwrap();

    // Second run without overwrite: skipped, artifact untouched (FR24)
    let (_, second) = run_job(&video, &config, &fonts, &ctl()).await.unwrap();
    assert!(second.artifacts.is_empty());
    assert_eq!(second.skipped_existing, vec![ArtifactKind::Static]);
    assert_eq!(std::fs::metadata(&produced).unwrap().modified().unwrap(), mtime1);

    // With overwrite: regenerated
    let ctl_ow = GenControl {
        cancel: CancellationToken::new(),
        overwrite: true,
        progress: Box::new(|_| {}),
    };
    let (_, third) = run_job(&video, &config, &fonts, &ctl_ow).await.unwrap();
    assert_eq!(third.artifacts.len(), 1);
}

#[tokio::test]
async fn impossible_target_never_emits_oversize() {
    let tmp = tempfile::tempdir().unwrap();
    let video = make_video(tmp.path(), "Big Motion [5589301].mp4", 1280, 720, 10.0);
    let fonts = Fonts::load();
    let mut config = test_config(GridDims { cols: 3, rows: 3 });
    config.quality = 95;
    config.target_mb = 0.05; // 50 KB — unreachable for a 3×3 sheet

    let result = run_job(&video, &config, &fonts, &ctl()).await;
    match result {
        Err(Failure::QualityFloor(_)) => {}
        Err(other) => panic!("expected quality-floor failure, got {other:?}"),
        Ok((_, outcome)) => {
            // If anything was emitted it must genuinely be under target (FR16)
            assert_under_target(&outcome, config.target_mb);
        }
    }
    // Whatever happened, no oversize file exists on disk
    if let Ok(rd) = std::fs::read_dir(tmp.path().join("srcs")) {
        for f in rd.flatten() {
            assert!(
                f.metadata().unwrap().len() as f64 <= config.target_mb * 1_000_000.0,
                "silent oversize artifact: {:?}",
                f.path()
            );
        }
    }
}

#[tokio::test]
async fn batch_engine_isolates_failures_and_completes() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    make_video(tmp.path(), "good1.mp4", 640, 360, 5.0);
    make_video(tmp.path(), "good2.mp4", 360, 640, 5.0);
    let bad = make_video(tmp.path(), "bad.mp4", 640, 360, 5.0);
    let bytes = std::fs::read(&bad).unwrap();
    std::fs::write(&bad, &bytes[..2048]).unwrap();

    let emitter: thumbnailer_lib::queue::Emitter = Arc::new(|_, _| {});
    let engine = Engine::new(emitter, data.path().to_path_buf());
    engine.settings.lock().unwrap().concurrency = 2;

    let added = engine.add_paths(vec![tmp.path().to_path_buf()]);
    assert_eq!(added, 3, "recursive folder scan finds all videos (FR1/FR3)");

    // Give every job a fast config
    let cfg = JobConfig {
        grid: GridDims { cols: 2, rows: 2 },
        artifacts: ArtifactSet { static_sheet: true, animated: false, montage: false },
        ..Default::default()
    };
    engine.apply_config_all(cfg);
    engine.start_batch();

    let mut waited = 0;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        waited += 1;
        let (_, batch) = engine.jobs_snapshot();
        if batch.status == BatchStatus::Complete {
            break;
        }
        assert!(waited < 240, "batch did not complete in 2 minutes");
    }

    let (jobs, batch) = engine.jobs_snapshot();
    assert_eq!(batch.done, 2, "good files done");
    assert_eq!(batch.failed, 1, "bad file failed, batch continued (FR5)");
    let failed = jobs.iter().find(|j| j.status == JobStatus::Failed).unwrap();
    assert!(failed.fail_reason.is_some(), "zero silent skips (SM2)");

    // Manifest persisted → a fresh engine resumes state (FR6)
    let engine2 = Engine::new(Arc::new(|_, _| {}), data.path().to_path_buf());
    let restored = engine2.load_manifest().expect("manifest restores");
    assert_eq!(restored.done, 2);
    assert_eq!(restored.failed, 1);
}
