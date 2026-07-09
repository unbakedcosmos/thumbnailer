//! End-to-end pipeline tests on synthetic footage: animated size guarantee,
//! orientation-aware grids, static formats, frame templates, montage loop,
//! robustness on truncated files, idempotent re-runs, batch engine isolation.

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
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc2=size={w}x{h}:rate=30:duration={dur}"),
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
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
        artifacts: ArtifactSet {
            static_sheet: true,
            animated: true,
            montage: true,
        },
        ..Default::default()
    }
}

fn classic() -> FrameTemplate {
    FrameTemplate::default()
}

fn ctl() -> GenControl {
    GenControl {
        cancel: CancellationToken::new(),
        overwrite: false,
        progress: Box::new(|_| {}),
    }
}

#[tokio::test]
async fn landscape_produces_all_artifacts() {
    let tmp = tempfile::tempdir().unwrap();
    let video = make_video(tmp.path(), "Chair Play [5587459].mp4", 1280, 720, 12.0);
    let fonts = Fonts::load();
    let config = test_config(GridDims { cols: 3, rows: 3 });

    let (meta, outcome) = run_job(&video, &config, &classic(), &fonts, &ctl())
        .await
        .expect("job succeeds");
    assert_eq!(meta.width, 1280);
    assert!(!meta.is_portrait());
    assert_eq!(outcome.artifacts.len(), 3, "static + animated + montage");

    // Naming convention (CHANGELOG §3 + restored montage loop): montage is
    // animated, so it follows the animated format
    let srcs = tmp.path().join("srcs");
    assert!(srcs.join("Chair Play [5587459]_contact.png").exists());
    assert!(srcs.join("Chair Play [5587459]_animated.webp").exists());
    assert!(srcs.join("Chair Play [5587459]_montage.webp").exists());

    // Animated grid and montage loop are both size-gated (≤ target on disk)
    for kind in [ArtifactKind::Animated, ArtifactKind::Montage] {
        let a = outcome.artifacts.iter().find(|a| a.kind == kind).unwrap();
        assert!(a.bytes as f64 <= config.animated.target_mb * 1e6);
    }

    // Static sheet: 2× render, decodes
    let png = outcome
        .artifacts
        .iter()
        .find(|a| a.kind == ArtifactKind::Static)
        .unwrap();
    let img = image::open(&png.path).expect("static sheet decodes");
    assert!(img.width() > 800, "2× render is crisp, got {}", img.width());

    // Montage: an animated WebP loop (RIFF/WEBP container, multi-frame)
    let mont = outcome
        .artifacts
        .iter()
        .find(|a| a.kind == ArtifactKind::Montage)
        .unwrap();
    let mbytes = std::fs::read(&mont.path).unwrap();
    assert_eq!(&mbytes[0..4], b"RIFF", "montage is a webp container");
    assert_eq!(&mbytes[8..12], b"WEBP");
    assert!(
        mbytes.windows(4).any(|w| w == b"ANIM"),
        "montage webp is animated (ANIM chunk present)"
    );
}

#[tokio::test]
async fn portrait_gets_orientation_aware_tiles() {
    let tmp = tempfile::tempdir().unwrap();
    let video = make_video(tmp.path(), "Backstage Vert [5588204].mp4", 720, 1280, 10.0);
    let fonts = Fonts::load();
    let mut config = test_config(GridDims { cols: 3, rows: 3 });
    config.artifacts = ArtifactSet {
        static_sheet: true,
        animated: false,
        montage: false,
    };

    let (meta, outcome) = run_job(&video, &config, &classic(), &fonts, &ctl())
        .await
        .expect("job succeeds");
    assert!(meta.is_portrait());

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
async fn static_formats_and_frame_toggle() {
    let tmp = tempfile::tempdir().unwrap();
    let video = make_video(tmp.path(), "Quick Clip [5589801].mp4", 640, 360, 6.0);
    let fonts = Fonts::load();
    let mut config = test_config(GridDims { cols: 2, rows: 2 });
    config.artifacts = ArtifactSet {
        static_sheet: true,
        animated: false,
        montage: false,
    };

    // JPEG format → _contact.jpg, and the PNG sibling never exists
    config.static_cfg.format = StaticFormat::Jpeg;
    config.static_cfg.quality = 70;
    let (_, out_jpg) = run_job(&video, &config, &classic(), &fonts, &ctl())
        .await
        .unwrap();
    let jpg_path = PathBuf::from(&out_jpg.artifacts[0].path);
    assert!(jpg_path.to_string_lossy().ends_with("_contact.jpg"));

    // Switching to WebP with overwrite OFF preserves everything: the webp is
    // written (its name is free) and the prior jpg is left untouched — Off never
    // deletes existing artifacts.
    config.static_cfg.format = StaticFormat::Webp;
    let (_, out_webp) = run_job(&video, &config, &classic(), &fonts, &ctl())
        .await
        .unwrap();
    assert!(out_webp.artifacts[0].path.ends_with("_contact.webp"));
    assert!(
        jpg_path.exists(),
        "overwrite-off preserves the prior format"
    );

    // With overwrite ON, a format switch sweeps the stale sibling so only the
    // current format remains.
    let ctl_ow0 = GenControl {
        cancel: CancellationToken::new(),
        overwrite: true,
        progress: Box::new(|_| {}),
    };
    let (_, _) = run_job(&video, &config, &classic(), &fonts, &ctl_ow0)
        .await
        .unwrap();
    assert!(
        !jpg_path.exists(),
        "stale jpg sibling removed on overwrite format switch"
    );

    // Frame off = raw grab: grid only, no header band → shorter than framed.
    // Same size sheet with band would differ in height.
    config.static_cfg.format = StaticFormat::Png;
    config.static_cfg.frame_on = true;
    let ctl_ow = || GenControl {
        cancel: CancellationToken::new(),
        overwrite: true,
        progress: Box::new(|_| {}),
    };
    let (_, framed) = run_job(&video, &config, &classic(), &fonts, &ctl_ow())
        .await
        .unwrap();
    let framed_img = image::open(&framed.artifacts[0].path).unwrap();
    config.static_cfg.frame_on = false;
    let (_, raw) = run_job(&video, &config, &classic(), &fonts, &ctl_ow())
        .await
        .unwrap();
    let raw_img = image::open(&raw.artifacts[0].path).unwrap();
    assert!(
        framed_img.height() > raw_img.height(),
        "header band adds height: framed {} vs raw {}",
        framed_img.height(),
        raw_img.height()
    );
}

#[tokio::test]
async fn animated_gif_format_encodes() {
    let tmp = tempfile::tempdir().unwrap();
    let video = make_video(tmp.path(), "Sunset Loop [5588120].mp4", 640, 360, 6.0);
    let fonts = Fonts::load();
    let mut config = test_config(GridDims { cols: 2, rows: 2 });
    config.artifacts = ArtifactSet {
        static_sheet: false,
        animated: true,
        montage: false,
    };
    config.animated.format = AnimatedFormat::Gif;
    config.animated.quality = 40;

    let (_, outcome) = run_job(&video, &config, &classic(), &fonts, &ctl())
        .await
        .unwrap();
    let gif = &outcome.artifacts[0];
    assert!(gif.path.ends_with("_animated.gif"));
    let bytes = std::fs::read(&gif.path).unwrap();
    assert_eq!(&bytes[0..3], b"GIF", "valid GIF header");
    assert!(bytes.len() as f64 <= config.animated.target_mb * 1e6);
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
    let err = run_job(&video, &config, &classic(), &fonts, &ctl())
        .await
        .expect_err("must fail");
    assert!(
        matches!(err, Failure::Unreadable(_)),
        "typed reason is unreadable (FR5), got: {err:?}"
    );
}

#[tokio::test]
async fn rerun_appends_numbered_copy_unless_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let video = make_video(tmp.path(), "Quick Clip [5589801].mp4", 640, 360, 6.0);
    let fonts = Fonts::load();
    let mut config = test_config(GridDims { cols: 2, rows: 2 });
    config.artifacts = ArtifactSet {
        static_sheet: true,
        animated: false,
        montage: false,
    };

    let (_, first) = run_job(&video, &config, &classic(), &fonts, &ctl())
        .await
        .unwrap();
    assert_eq!(first.artifacts.len(), 1);
    let produced = PathBuf::from(&first.artifacts[0].path);
    assert!(produced.to_string_lossy().ends_with("_contact.png"));
    let mtime1 = std::fs::metadata(&produced).unwrap().modified().unwrap();

    // Second run without overwrite: the original is preserved untouched and a
    // numbered copy is written alongside it (append mode, not skip/clobber).
    let (_, second) = run_job(&video, &config, &classic(), &fonts, &ctl())
        .await
        .unwrap();
    assert_eq!(second.artifacts.len(), 1);
    assert!(second.skipped_existing.is_empty());
    let copy = PathBuf::from(&second.artifacts[0].path);
    assert!(
        copy.to_string_lossy().ends_with("_contact (1).png"),
        "expected numbered copy, got {}",
        copy.display()
    );
    assert!(copy.exists() && copy != produced);
    // Original bytes and mtime unchanged.
    assert_eq!(
        std::fs::metadata(&produced).unwrap().modified().unwrap(),
        mtime1
    );

    // A third no-overwrite run appends the next number.
    let (_, third) = run_job(&video, &config, &classic(), &fonts, &ctl())
        .await
        .unwrap();
    assert!(PathBuf::from(&third.artifacts[0].path)
        .to_string_lossy()
        .ends_with("_contact (2).png"));

    // With overwrite: the canonical file is replaced in place (no new copy).
    let ctl_ow = GenControl {
        cancel: CancellationToken::new(),
        overwrite: true,
        progress: Box::new(|_| {}),
    };
    let (_, fourth) = run_job(&video, &config, &classic(), &fonts, &ctl_ow)
        .await
        .unwrap();
    assert_eq!(fourth.artifacts.len(), 1);
    assert!(PathBuf::from(&fourth.artifacts[0].path)
        .to_string_lossy()
        .ends_with("_contact.png"));
}

#[tokio::test]
async fn impossible_target_never_emits_oversize_animated() {
    let tmp = tempfile::tempdir().unwrap();
    let video = make_video(tmp.path(), "Big Motion [5589301].mp4", 1280, 720, 10.0);
    let fonts = Fonts::load();
    let mut config = test_config(GridDims { cols: 3, rows: 3 });
    config.artifacts = ArtifactSet {
        static_sheet: false,
        animated: true,
        montage: false,
    };
    config.animated.quality = 95;
    config.animated.target_mb = 0.05; // 50 KB — unreachable

    let result = run_job(&video, &config, &classic(), &fonts, &ctl()).await;
    match result {
        Err(Failure::QualityFloor(_)) => {}
        Err(other) => panic!("expected quality-floor failure, got {other:?}"),
        Ok((_, outcome)) => {
            for a in &outcome.artifacts {
                assert!(a.bytes as f64 <= config.animated.target_mb * 1e6);
            }
        }
    }
    // Whatever happened, no oversize animated file exists on disk (FR16)
    if let Ok(rd) = std::fs::read_dir(tmp.path().join("srcs")) {
        for f in rd.flatten() {
            assert!(
                f.metadata().unwrap().len() as f64 <= config.animated.target_mb * 1_000_000.0,
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

    let cfg = JobConfig {
        grid: GridDims { cols: 2, rows: 2 },
        artifacts: ArtifactSet {
            static_sheet: true,
            animated: false,
            montage: false,
        },
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

    // A fully-finished batch (nothing left to run) is NOT resumed — the app
    // opens fresh and the stale manifest is cleared, so a clean close means a
    // clean start rather than re-showing last session's results.
    let engine2 = Engine::new(Arc::new(|_, _| {}), data.path().to_path_buf());
    assert!(
        engine2.load_manifest().is_none(),
        "completed batch starts fresh, not resumed"
    );
    assert!(
        !data.path().join("manifest.json").exists(),
        "stale manifest cleared on fresh start"
    );
    let (jobs2, _) = engine2.jobs_snapshot();
    assert!(
        jobs2.is_empty(),
        "fresh engine has no jobs after clean close"
    );
}

/// Crash-resume (FR6): a batch with genuinely unfinished work IS restored, with
/// mid-flight jobs reset to queued and the batch left paused for the user.
#[tokio::test]
async fn manifest_resumes_unfinished_work() {
    let tmp = tempfile::tempdir().unwrap();
    make_video(tmp.path(), "Backstage Vert [5588204].mp4", 320, 240, 2.0);
    make_video(tmp.path(), "Chair Play [5587459].mp4", 320, 240, 2.0);
    let data = tempfile::tempdir().unwrap();

    // add_paths persists a manifest with queued jobs (no encoding needed).
    let engine = Engine::new(Arc::new(|_, _| {}), data.path().to_path_buf());
    assert_eq!(engine.add_paths(vec![tmp.path().to_path_buf()]), 2);

    // A fresh engine over the same data dir brings the queued work back.
    let engine2 = Engine::new(Arc::new(|_, _| {}), data.path().to_path_buf());
    let restored = engine2
        .load_manifest()
        .expect("unfinished work is restored");
    assert_eq!(restored.total, 2);
    assert_eq!(restored.status, BatchStatus::Paused, "resumes paused (FR6)");
    let (jobs2, _) = engine2.jobs_snapshot();
    assert_eq!(jobs2.len(), 2);
    assert!(jobs2.iter().all(|j| j.status == JobStatus::Queued));
}

#[tokio::test]
async fn template_store_crud_and_builtin_protection() {
    let data = tempfile::tempdir().unwrap();
    let engine = Engine::new(Arc::new(|_, _| {}), data.path().to_path_buf());

    let list = engine.templates.list();
    assert_eq!(list.len(), 3, "ships Classic / Minimal / Bold");
    assert!(list.iter().all(|t| t.builtin));

    // Built-ins can't be edited or deleted
    assert!(engine.templates.save(FrameTemplate::default()).is_err());
    assert!(engine.templates.delete("classic").is_err());

    // Save a custom, survives a new store instance (templates.json)
    let custom = FrameTemplate {
        id: "".into(),
        name: "Poster".into(),
        header_band: true,
        border: BorderStyle::Thick,
        timestamp_style: TimestampStyle::Overlay,
        accent: AccentChoice::White,
        builtin: false,
    };
    let saved = engine.templates.save(custom).unwrap();
    assert!(!saved.id.is_empty());

    let engine2 = Engine::new(Arc::new(|_, _| {}), data.path().to_path_buf());
    assert_eq!(engine2.templates.list().len(), 4);
    assert_eq!(engine2.templates.get(&saved.id).name, "Poster");

    engine2.templates.delete(&saved.id).unwrap();
    assert_eq!(engine2.templates.list().len(), 3);
}
