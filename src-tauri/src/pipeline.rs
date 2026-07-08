//! Per-video artifact generation: static sheet, animated grid, montage loop —
//! each governed by the bounded auto-fit loop (PRD FR16/FR17/FR17a): every
//! emitted artifact is ≤ target or it is not emitted and reported as a failure.

use crate::extract::{extract_clip, extract_frame};
use crate::probe::{fmt_duration, probe};
use crate::render::*;
use crate::types::*;
use image::RgbImage;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const ANIM_FPS: f64 = 12.0;
const ANIM_FRAMES: usize = 30; // 2.5 s — the proven ceiling (PRD FR13)
const MONTAGE_SEGMENTS: usize = 6;
const MONTAGE_SEG_FRAMES: usize = 14; // ~1.2 s per segment @ 12 fps
const STATIC_TIMEOUT: Duration = Duration::from_secs(180);
const ANIM_TIMEOUT: Duration = Duration::from_secs(420);
const MONTAGE_TIMEOUT: Duration = Duration::from_secs(180);

pub struct GenControl {
    pub cancel: CancellationToken,
    pub overwrite: bool,
    pub progress: Box<dyn Fn(f32) + Send + Sync>,
}

impl GenControl {
    fn check(&self) -> Result<(), Failure> {
        if self.cancel.is_cancelled() {
            Err(Failure::Cancelled)
        } else {
            Ok(())
        }
    }
    fn report(&self, p: f32) {
        (self.progress)(p.clamp(0.0, 1.0));
    }
}

fn io_failure(e: &std::io::Error, what: &str) -> Failure {
    if e.kind() == std::io::ErrorKind::StorageFull {
        Failure::DiskFull(what.into())
    } else {
        Failure::DecodeError(format!("{what}: {e}"))
    }
}

/// Atomic write (PRD FR6): temp file in the destination dir, then rename, so a
/// crash never leaves a truncated artifact that idempotency mistakes for done.
fn atomic_write(dest: &Path, bytes: &[u8]) -> Result<(), Failure> {
    let tmp = dest.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io_failure(&e, "writing artifact")
    })?;
    std::fs::rename(&tmp, dest).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io_failure(&e, "finalizing artifact")
    })
}

/// Windows reserved device names get a leading underscore (PRD FR23).
fn sanitize_stem(stem: &str) -> String {
    let upper = stem.trim().to_uppercase();
    let base = upper.split('.').next().unwrap_or("");
    const RESERVED: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];
    let reserved = RESERVED.contains(&base)
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && base.chars().last().is_some_and(|c| c.is_ascii_digit()));
    if reserved {
        format!("_{stem}")
    } else {
        stem.to_string()
    }
}

pub fn output_dir(source: &Path, config: &JobConfig) -> PathBuf {
    match (&config.output_mode, &config.output_path) {
        (OutputMode::Custom, Some(p)) if !p.trim().is_empty() => PathBuf::from(p),
        // Default: a srcs/ subfolder inside the video's folder (PRD FR23)
        _ => source.parent().unwrap_or(Path::new(".")).join("srcs"),
    }
}

pub fn artifact_path(source: &Path, config: &JobConfig, kind: ArtifactKind) -> PathBuf {
    let stem = sanitize_stem(
        source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("video"),
    );
    output_dir(source, config).join(format!("{stem}{}", kind.suffix()))
}

/// Static's JPEG fallback name (never `_contact.webp` — that's the animated grid's).
fn static_jpg_path(source: &Path, config: &JobConfig) -> PathBuf {
    let stem = sanitize_stem(
        source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("video"),
    );
    output_dir(source, config).join(format!("{stem}_contact.jpg"))
}

fn exists_valid(p: &Path) -> bool {
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.len() > 0)
        .unwrap_or(false)
}

fn header_meta<'a>(title: &'a str, meta: &VideoMeta) -> HeaderMeta<'a> {
    HeaderMeta {
        title,
        duration: fmt_duration(meta.duration_s),
        resolution: format!("{}×{}", meta.width, meta.height),
        fps: format!("{:.0}", meta.fps),
    }
}

/// Evenly-sampled timestamps across the video (PRD FR8).
fn sample_times(duration: f64, n: u32) -> Vec<f64> {
    (0..n)
        .map(|i| duration * (i as f64 + 0.5) / n as f64)
        .collect()
}

// ---------------------------------------------------------------- clip store

/// Extracted tile clips spooled to disk so the auto-fit loop can re-encode
/// without re-running ffmpeg, and memory stays ~one frame at a time (PRD FR4).
struct ClipStore {
    dir: tempfile::TempDir,
    files: Vec<std::fs::File>,
    w: u32,
    h: u32,
    frames: usize,
}

impl ClipStore {
    fn new(w: u32, h: u32, frames: usize) -> std::io::Result<Self> {
        Ok(ClipStore {
            dir: tempfile::tempdir()?,
            files: Vec::new(),
            w,
            h,
            frames,
        })
    }

    fn push_clip(&mut self, frames: &[RgbImage]) -> std::io::Result<()> {
        let path = self
            .dir
            .path()
            .join(format!("tile{}.raw", self.files.len()));
        let mut f = std::fs::File::create(&path)?;
        for fr in frames {
            f.write_all(fr.as_raw())?;
        }
        f.flush()?;
        self.files.push(std::fs::File::open(&path)?);
        Ok(())
    }

    fn frame(&mut self, tile: usize, frame: usize) -> std::io::Result<RgbImage> {
        let len = (self.w * self.h * 3) as usize;
        let mut buf = vec![0u8; len];
        let f = &mut self.files[tile];
        f.seek(SeekFrom::Start((frame.min(self.frames - 1) * len) as u64))?;
        f.read_exact(&mut buf)?;
        Ok(RgbImage::from_raw(self.w, self.h, buf).expect("sized"))
    }
}

// ---------------------------------------------------------------- encoding

fn encode_png(img: &RgbImage) -> Result<Vec<u8>, Failure> {
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| Failure::DecodeError(format!("png encode: {e}")))?;
    Ok(out.into_inner())
}

fn encode_jpeg(img: &RgbImage, q: u8) -> Result<Vec<u8>, Failure> {
    let mut out = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, q);
    enc.encode_image(img)
        .map_err(|e| Failure::DecodeError(format!("jpeg encode: {e}")))?;
    Ok(out)
}

fn rgb_to_rgba(img: &RgbImage) -> Vec<u8> {
    let mut out = Vec::with_capacity((img.width() * img.height() * 4) as usize);
    for p in img.pixels() {
        out.extend_from_slice(&[p.0[0], p.0[1], p.0[2], 255]);
    }
    out
}

struct AnimFrames<'a> {
    store: &'a mut ClipStore,
    /// Source frame indices to use (fps/loop degradation picks a subset)
    indices: Vec<usize>,
    fps: f64,
}

/// Compose + encode one animated-webp attempt at the given layout and quality.
#[allow(clippy::too_many_arguments)]
fn encode_animated(
    l: &SheetLayout,
    fonts: &Fonts,
    hm: &HeaderMeta,
    times: &[f64],
    show_timestamps: bool,
    af: &mut AnimFrames,
    webp_q: f32,
    ctl: &GenControl,
    prog_base: f32,
    prog_span: f32,
) -> Result<Vec<u8>, Failure> {
    let chrome = render_chrome(l, fonts, hm);
    let mut enc = webp_animation::Encoder::new_with_options(
        (l.card_w, l.card_h),
        webp_animation::EncoderOptions {
            encoding_config: Some(webp_animation::EncodingConfig {
                quality: webp_q,
                method: 4,
                encoding_type: webp_animation::EncodingType::Lossy(
                    webp_animation::LossyEncodingConfig::default(),
                ),
            }),
            ..Default::default()
        },
    )
    .map_err(|e| Failure::DecodeError(format!("webp encoder: {e}")))?;

    let n = af.indices.len();
    for (j, &src_idx) in af.indices.iter().enumerate() {
        ctl.check()?;
        let mut frame = chrome.clone();
        for (tile, &t) in times.iter().enumerate().take((l.cols * l.rows) as usize) {
            let img = af
                .store
                .frame(tile, src_idx)
                .map_err(|e| io_failure(&e, "clip spool"))?;
            blit_tile(&mut frame, l, tile as u32, &img);
            if show_timestamps {
                draw_timestamp(&mut frame, l, tile as u32, fonts, &fmt_timestamp(t));
            }
        }
        let ts_ms = (j as f64 * 1000.0 / af.fps) as i32;
        enc.add_frame(&rgb_to_rgba(&frame), ts_ms)
            .map_err(|e| Failure::DecodeError(format!("webp frame: {e}")))?;
        ctl.report(prog_base + prog_span * (j + 1) as f32 / n as f32);
    }
    let end_ms = (n as f64 * 1000.0 / af.fps) as i32;
    let data = enc
        .finalize(end_ms)
        .map_err(|e| Failure::DecodeError(format!("webp finalize: {e}")))?;
    Ok(data.to_vec())
}

/// One rung of the animated auto-fit ladder (PRD FR17: quality → fps → loop →
/// resolution, bounded, with quality-gate floors FR17a).
struct FitStep {
    q: f32,
    fps: f64,
    n_frames: usize,
    scale: f64,
}

fn anim_ladder(quality: u8) -> Vec<FitStep> {
    let base_q = (55.0 + 0.4 * quality as f32).min(92.0);
    let q2 = (base_q - 14.0).max(38.0);
    let q3 = (base_q - 28.0).max(38.0);
    vec![
        FitStep {
            q: base_q,
            fps: ANIM_FPS,
            n_frames: ANIM_FRAMES,
            scale: 1.0,
        },
        FitStep {
            q: q2,
            fps: ANIM_FPS,
            n_frames: ANIM_FRAMES,
            scale: 1.0,
        },
        FitStep {
            q: q3,
            fps: ANIM_FPS,
            n_frames: ANIM_FRAMES,
            scale: 1.0,
        },
        FitStep {
            q: q3,
            fps: 8.0,
            n_frames: 20,
            scale: 1.0,
        },
        FitStep {
            q: q3,
            fps: 8.0,
            n_frames: 16,
            scale: 1.0,
        },
        FitStep {
            q: q3,
            fps: 8.0,
            n_frames: 16,
            scale: 0.8,
        },
        FitStep {
            q: q3,
            fps: 8.0,
            n_frames: 16,
            scale: 0.65,
        },
    ]
}

/// Map a rung's fps/frame budget onto the indices of the extracted 12fps clip.
fn frame_indices(step: &FitStep) -> Vec<usize> {
    (0..step.n_frames)
        .map(|j| ((j as f64 * ANIM_FPS / step.fps).round() as usize).min(ANIM_FRAMES - 1))
        .collect()
}

// ---------------------------------------------------------------- artifacts

pub struct JobInput<'a> {
    pub source: &'a Path,
    pub title: &'a str,
    pub config: &'a JobConfig,
    pub meta: &'a VideoMeta,
    pub fonts: &'a Fonts,
}

async fn generate_static(
    inp: &JobInput<'_>,
    ctl: &GenControl,
    p0: f32,
    span: f32,
) -> Result<ProducedArtifact, Failure> {
    let cfg = inp.config;
    let meta = inp.meta;
    let target = (cfg.target_mb * 1_000_000.0) as u64;
    let aspect = tile_aspect(cfg.orientation, meta);
    // Quality slider scales the static render: 2× at 62, range ~1.3×–2.6×
    let scale_mult = 0.65 + 0.011 * cfg.quality as f64;
    let l = static_layout(cfg.grid, aspect, scale_mult);
    let n = cfg.grid.tiles();
    let times = sample_times(meta.duration_s, n);
    let hm = header_meta(inp.title, meta);

    // Extract each tile's frame at full tile resolution
    let inner_w = l.tile_w - 2 * l.hairline;
    let inner_h = l.tile_h - 2 * l.hairline;
    let mut tiles: Vec<RgbImage> = Vec::with_capacity(n as usize);
    for (i, &t) in times.iter().enumerate() {
        ctl.check()?;
        tiles.push(extract_frame(inp.source, t, inner_w, inner_h, meta.hdr).await?);
        ctl.report(p0 + span * 0.7 * (i + 1) as f32 / n as f32);
    }

    let compose = |scale_mult: f64| -> RgbImage {
        let l = static_layout(cfg.grid, aspect, scale_mult);
        let mut img = render_chrome(&l, inp.fonts, &hm);
        for (i, tile) in tiles.iter().enumerate() {
            blit_tile(&mut img, &l, i as u32, tile);
            if cfg.timestamps {
                draw_timestamp(&mut img, &l, i as u32, inp.fonts, &fmt_timestamp(times[i]));
            }
        }
        img
    };

    // Auto-fit ladder for static (PRD FR17): PNG → JPEG quality → resolution
    let full = compose(scale_mult);
    ctl.report(p0 + span * 0.85);
    let png = encode_png(&full)?;
    let dest_png = artifact_path(inp.source, cfg, ArtifactKind::Static);
    if png.len() as u64 <= target {
        atomic_write(&dest_png, &png)?;
        return Ok(ProducedArtifact {
            kind: ArtifactKind::Static,
            path: dest_png.to_string_lossy().into(),
            bytes: png.len() as u64,
            degraded: false,
        });
    }

    let attempts: Vec<(f64, u8)> = vec![
        (scale_mult, 88),
        (scale_mult, 78),
        (scale_mult, 68),
        (scale_mult * 0.85, 78),
        (scale_mult * 0.7, 75),
        (scale_mult * 0.55, 72),
    ];
    let dest_jpg = static_jpg_path(inp.source, cfg);
    for (i, (sm, q)) in attempts.iter().enumerate() {
        ctl.check()?;
        let img = if (*sm - scale_mult).abs() < f64::EPSILON {
            full.clone()
        } else {
            compose(*sm)
        };
        let jpg = encode_jpeg(&img, *q)?;
        if jpg.len() as u64 <= target {
            // Quality floor (FR17a): the smallest rung still counts, below it we fail
            atomic_write(&dest_jpg, &jpg)?;
            // Remove a stale PNG from a previous config so both names don't linger
            let _ = std::fs::remove_file(&dest_png);
            ctl.report(p0 + span);
            return Ok(ProducedArtifact {
                kind: ArtifactKind::Static,
                path: dest_jpg.to_string_lossy().into(),
                bytes: jpg.len() as u64,
                degraded: i > 0,
            });
        }
    }
    Err(Failure::QualityFloor(format!(
        "static sheet ≥ {:.1} MB at smallest allowed render",
        cfg.target_mb
    )))
}

async fn generate_animated(
    inp: &JobInput<'_>,
    ctl: &GenControl,
    p0: f32,
    span: f32,
) -> Result<ProducedArtifact, Failure> {
    let cfg = inp.config;
    let meta = inp.meta;
    let target = (cfg.target_mb * 1_000_000.0) as u64;
    let aspect = tile_aspect(cfg.orientation, meta);
    let l0 = animated_layout(cfg.grid, aspect, cfg.quality, 1.0);
    let n = cfg.grid.tiles();
    // Keep clip starts clear of the very end of the file
    let clip_len = ANIM_FRAMES as f64 / ANIM_FPS;
    let times: Vec<f64> = sample_times(meta.duration_s, n)
        .into_iter()
        .map(|t| t.min((meta.duration_s - clip_len - 0.1).max(0.0)))
        .collect();
    let hm = header_meta(inp.title, meta);

    let inner_w = l0.tile_w - 2 * l0.hairline;
    let inner_h = l0.tile_h - 2 * l0.hairline;
    let mut store =
        ClipStore::new(inner_w, inner_h, ANIM_FRAMES).map_err(|e| io_failure(&e, "temp spool"))?;
    for (i, &t) in times.iter().enumerate() {
        ctl.check()?;
        let frames = extract_clip(
            inp.source,
            t,
            ANIM_FPS,
            ANIM_FRAMES,
            inner_w,
            inner_h,
            meta.hdr,
        )
        .await?;
        store
            .push_clip(&frames)
            .map_err(|e| io_failure(&e, "temp spool"))?;
        ctl.report(p0 + span * 0.55 * (i + 1) as f32 / n as f32);
    }

    let dest = artifact_path(inp.source, cfg, ArtifactKind::Animated);
    let ladder = anim_ladder(cfg.quality);
    let n_steps = ladder.len();
    for (i, step) in ladder.iter().enumerate() {
        ctl.check()?;
        let l = animated_layout(cfg.grid, aspect, cfg.quality, step.scale);
        let mut af = AnimFrames {
            store: &mut store,
            indices: frame_indices(step),
            fps: step.fps,
        };
        let base = p0 + span * (0.55 + 0.45 * i as f32 / n_steps as f32);
        let sp = span * 0.45 / n_steps as f32;
        let bytes = encode_animated(
            &l,
            inp.fonts,
            &hm,
            &times,
            cfg.timestamps,
            &mut af,
            step.q,
            ctl,
            base,
            sp,
        )?;
        if bytes.len() as u64 <= target {
            atomic_write(&dest, &bytes)?;
            ctl.report(p0 + span);
            return Ok(ProducedArtifact {
                kind: ArtifactKind::Animated,
                path: dest.to_string_lossy().into(),
                bytes: bytes.len() as u64,
                degraded: i > 0,
            });
        }
    }
    Err(Failure::QualityFloor(format!(
        "animated grid ≥ {:.1} MB at fps/resolution floor",
        cfg.target_mb
    )))
}

async fn generate_montage(
    inp: &JobInput<'_>,
    ctl: &GenControl,
    p0: f32,
    span: f32,
) -> Result<ProducedArtifact, Failure> {
    let cfg = inp.config;
    let meta = inp.meta;
    let target = (cfg.target_mb * 1_000_000.0) as u64;
    let aspect = tile_aspect(cfg.orientation, meta);
    // Single cell (PRD FR14): bare frames, no chrome — sequential clips in one frame
    let long = 420.0 + 2.4 * cfg.quality as f64;
    let (w, h) = if aspect >= 1.0 {
        (long as u32, (long / aspect) as u32)
    } else {
        ((long * aspect) as u32, long as u32)
    };
    let (w, h) = (w & !1, h & !1);

    let seg_len = MONTAGE_SEG_FRAMES as f64 / ANIM_FPS;
    let times: Vec<f64> = sample_times(meta.duration_s, MONTAGE_SEGMENTS as u32)
        .into_iter()
        .map(|t| t.min((meta.duration_s - seg_len - 0.1).max(0.0)))
        .collect();

    let mut all_frames: Vec<RgbImage> = Vec::new();
    for (i, &t) in times.iter().enumerate() {
        ctl.check()?;
        let frames =
            extract_clip(inp.source, t, ANIM_FPS, MONTAGE_SEG_FRAMES, w, h, meta.hdr).await?;
        all_frames.extend(frames);
        ctl.report(p0 + span * 0.6 * (i + 1) as f32 / times.len() as f32);
    }

    let dest = artifact_path(inp.source, cfg, ArtifactKind::Montage);
    let base_q = (55.0 + 0.4 * cfg.quality as f32).min(92.0);
    // quality → fps → length → resolution, same governed order as the grid
    let attempts: [(f32, usize, f64); 6] = [
        (base_q, 1, 1.0),
        ((base_q - 14.0).max(38.0), 1, 1.0),
        ((base_q - 28.0).max(38.0), 1, 1.0),
        ((base_q - 28.0).max(38.0), 2, 1.0), // half fps
        ((base_q - 28.0).max(38.0), 2, 0.8),
        ((base_q - 28.0).max(38.0), 2, 0.65),
    ];
    for (i, (q, stride, sc)) in attempts.iter().enumerate() {
        ctl.check()?;
        let (sw, sh) = (((w as f64 * sc) as u32) & !1, ((h as f64 * sc) as u32) & !1);
        let fps_out = ANIM_FPS / *stride as f64;
        let mut enc = webp_animation::Encoder::new_with_options(
            (sw.max(2), sh.max(2)),
            webp_animation::EncoderOptions {
                encoding_config: Some(webp_animation::EncodingConfig {
                    quality: *q,
                    method: 4,
                    encoding_type: webp_animation::EncodingType::Lossy(
                        webp_animation::LossyEncodingConfig::default(),
                    ),
                }),
                ..Default::default()
            },
        )
        .map_err(|e| Failure::DecodeError(format!("webp encoder: {e}")))?;
        let picked: Vec<&RgbImage> = all_frames.iter().step_by(*stride).collect();
        for (j, fr) in picked.iter().enumerate() {
            ctl.check()?;
            let img = if *sc < 1.0 {
                image::imageops::resize(
                    *fr,
                    sw.max(2),
                    sh.max(2),
                    image::imageops::FilterType::Triangle,
                )
            } else {
                (*fr).clone()
            };
            enc.add_frame(&rgb_to_rgba(&img), (j as f64 * 1000.0 / fps_out) as i32)
                .map_err(|e| Failure::DecodeError(format!("webp frame: {e}")))?;
        }
        let data = enc
            .finalize((picked.len() as f64 * 1000.0 / fps_out) as i32)
            .map_err(|e| Failure::DecodeError(format!("webp finalize: {e}")))?;
        if data.len() as u64 <= target {
            atomic_write(&dest, &data)?;
            ctl.report(p0 + span);
            return Ok(ProducedArtifact {
                kind: ArtifactKind::Montage,
                path: dest.to_string_lossy().into(),
                bytes: data.len() as u64,
                degraded: i > 0,
            });
        }
    }
    Err(Failure::QualityFloor(format!(
        "montage ≥ {:.1} MB at floor",
        cfg.target_mb
    )))
}

// ---------------------------------------------------------------- job entry

type ArtifactFut<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<ProducedArtifact, Failure>> + Send + 'a>,
>;

pub async fn run_job(
    source: &Path,
    config: &JobConfig,
    fonts: &Fonts,
    ctl: &GenControl,
) -> Result<(VideoMeta, JobOutcome), Failure> {
    let meta = probe(source).await?;
    ctl.report(0.05);

    let title = source
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("video");

    let out_dir = output_dir(source, config);
    std::fs::create_dir_all(&out_dir).map_err(|e| io_failure(&e, "creating output folder"))?;

    let inp = JobInput {
        source,
        title,
        config,
        meta: &meta,
        fonts,
    };

    // Idempotency (FR24): existing valid artifacts are skipped unless overwrite
    let mut skipped = Vec::new();
    let mut wanted: Vec<ArtifactKind> = Vec::new();
    let a = &config.artifacts;
    for (on, kind) in [
        (a.static_sheet, ArtifactKind::Static),
        (a.animated, ArtifactKind::Animated),
        (a.montage, ArtifactKind::Montage),
    ] {
        if !on {
            continue;
        }
        let existing = match kind {
            ArtifactKind::Static => {
                exists_valid(&artifact_path(source, config, kind))
                    || exists_valid(&static_jpg_path(source, config))
            }
            _ => exists_valid(&artifact_path(source, config, kind)),
        };
        if existing && !ctl.overwrite {
            skipped.push(kind);
        } else {
            wanted.push(kind);
        }
    }

    let weights: f32 = wanted
        .iter()
        .map(|k| match k {
            ArtifactKind::Static => 1.0,
            ArtifactKind::Animated => 3.0,
            ArtifactKind::Montage => 1.5,
        })
        .sum();

    let mut artifacts = Vec::new();
    let mut p0 = 0.05f32;
    for kind in wanted {
        let w = match kind {
            ArtifactKind::Static => 1.0,
            ArtifactKind::Animated => 3.0,
            ArtifactKind::Montage => 1.5,
        } / weights
            * 0.95;
        let (fut, budget): (ArtifactFut<'_>, Duration) = match kind {
            ArtifactKind::Static => (Box::pin(generate_static(&inp, ctl, p0, w)), STATIC_TIMEOUT),
            ArtifactKind::Animated => (Box::pin(generate_animated(&inp, ctl, p0, w)), ANIM_TIMEOUT),
            ArtifactKind::Montage => (
                Box::pin(generate_montage(&inp, ctl, p0, w)),
                MONTAGE_TIMEOUT,
            ),
        };
        let art = tokio::time::timeout(budget, fut)
            .await
            .map_err(|_| Failure::Timeout(format!("{:?} artifact exceeded time budget", kind)))??;
        artifacts.push(art);
        p0 += w;
    }
    ctl.report(1.0);

    Ok((
        meta,
        JobOutcome {
            artifacts,
            skipped_existing: skipped,
        },
    ))
}
