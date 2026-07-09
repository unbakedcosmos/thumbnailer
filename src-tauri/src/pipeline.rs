//! Per-video artifact generation. The animated preview is size-governed by a
//! bounded auto-fit ladder (PRD FR16/FR17/FR17a): it is ≤ target or it is not
//! emitted. Static & montage images are format/quality-driven, not size-gated
//! (CHANGELOG §1) — their knobs are file type, compression and the sheet frame.

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

fn stem_of(source: &Path) -> String {
    sanitize_stem(
        source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("video"),
    )
}

pub fn artifact_path(source: &Path, config: &JobConfig, kind: ArtifactKind) -> PathBuf {
    output_dir(source, config).join(format!("{}{}", stem_of(source), kind.suffix(config)))
}

/// Resolve where an artifact is actually written. Overwrite ON replaces the
/// canonical path in place. Overwrite OFF preserves any existing file and
/// writes the next free `name (N).ext` variant instead (never clobbers, never
/// silently skips — the user always gets fresh output alongside the old).
fn resolve_dest(source: &Path, config: &JobConfig, kind: ArtifactKind, overwrite: bool) -> PathBuf {
    let base = artifact_path(source, config, kind);
    if overwrite || !exists_valid(&base) {
        return base;
    }
    let dir = base.parent().unwrap_or_else(|| Path::new("."));
    let stem = base
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("artifact");
    let ext = base.extension().and_then(|s| s.to_str());
    for n in 1..100_000 {
        let name = match ext {
            Some(e) => format!("{stem} ({n}).{e}"),
            None => format!("{stem} ({n})"),
        };
        let cand = dir.join(name);
        if !cand.exists() {
            return cand;
        }
    }
    base
}

/// Remove artifacts of the same kind written under a different format choice,
/// so a format switch doesn't leave both `_contact.png` and `_contact.jpg`.
fn remove_stale_siblings(source: &Path, config: &JobConfig, kind: ArtifactKind, keep: &Path) {
    let dir = output_dir(source, config);
    let stem = stem_of(source);
    for suffix in kind.all_suffixes() {
        let p = dir.join(format!("{stem}{suffix}"));
        if p != keep {
            let _ = std::fs::remove_file(&p);
        }
    }
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

fn encode_webp_static(img: &RgbImage, q: f32) -> Vec<u8> {
    webp::Encoder::from_rgb(img.as_raw(), img.width(), img.height())
        .encode(q)
        .to_vec()
}

/// Map the 0–100 compression slider onto each lossy codec's own quality scale.
fn jpeg_quality(quality: u8) -> u8 {
    (30 + quality as u32 * 65 / 100) as u8
}
fn webp_quality(quality: u8) -> f32 {
    40.0 + quality as f32 * 0.55
}

/// Bake the compression quality into a single tile's *media* by round-tripping
/// it through the chosen lossy codec. The sheet frame (borders, header band,
/// timestamps) is composited on top and encoded separately at high quality
/// (`encode_static_sheet`), so the quality slider softens only the video content
/// inside each cell — never the chrome. PNG is lossless (its slider is hidden),
/// so tiles pass through untouched. Any codec hiccup falls back to the raw tile.
fn degrade_tile_media(tile: &RgbImage, format: StaticFormat, quality: u8) -> RgbImage {
    match format {
        StaticFormat::Png => tile.clone(),
        StaticFormat::Jpeg => encode_jpeg(tile, jpeg_quality(quality))
            .ok()
            .and_then(|b| image::load_from_memory_with_format(&b, image::ImageFormat::Jpeg).ok())
            .map(|d| d.to_rgb8())
            .unwrap_or_else(|| tile.clone()),
        StaticFormat::Webp => webp::Decoder::new(&encode_webp_static(tile, webp_quality(quality)))
            .decode()
            .map(|img| img.to_image().to_rgb8())
            .unwrap_or_else(|| tile.clone()),
    }
}

/// Encode the composed sheet. The frame/chrome is kept crisp at a high fixed
/// quality; the per-tile media has already been degraded to the user's setting
/// (see `degrade_tile_media`). PNG stays lossless.
fn encode_static_sheet(img: &RgbImage, format: StaticFormat) -> Result<Vec<u8>, Failure> {
    const SHEET_Q: u8 = 95;
    match format {
        StaticFormat::Png => encode_png(img),
        StaticFormat::Jpeg => encode_jpeg(img, SHEET_Q),
        StaticFormat::Webp => Ok(encode_webp_static(img, SHEET_Q as f32)),
    }
}

fn rgb_to_rgba(img: &RgbImage) -> Vec<u8> {
    let mut out = Vec::with_capacity((img.width() * img.height() * 4) as usize);
    for p in img.pixels() {
        out.extend_from_slice(&[p.0[0], p.0[1], p.0[2], 255]);
    }
    out
}

/// Shared byte sink for GifEncoder, which never hands its writer back.
#[derive(Clone, Default)]
struct SharedBuf(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Incremental animated encoder over composed frames — WebP or GIF.
enum AnimEncoder {
    Webp(Box<webp_animation::Encoder>, f64, usize),
    Gif(image::codecs::gif::GifEncoder<SharedBuf>, f64, SharedBuf),
}

impl AnimEncoder {
    fn new(format: AnimatedFormat, w: u32, h: u32, webp_q: f32, fps: f64) -> Result<Self, Failure> {
        match format {
            AnimatedFormat::Webp => {
                let enc = webp_animation::Encoder::new_with_options(
                    (w, h),
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
                Ok(AnimEncoder::Webp(Box::new(enc), fps, 0))
            }
            AnimatedFormat::Gif => {
                let buf = SharedBuf::default();
                let mut enc = image::codecs::gif::GifEncoder::new_with_speed(buf.clone(), 12);
                enc.set_repeat(image::codecs::gif::Repeat::Infinite)
                    .map_err(|e| Failure::DecodeError(format!("gif encoder: {e}")))?;
                Ok(AnimEncoder::Gif(enc, fps, buf))
            }
        }
    }

    fn add_frame(&mut self, frame: &RgbImage) -> Result<(), Failure> {
        match self {
            AnimEncoder::Webp(enc, fps, n) => {
                let ts_ms = (*n as f64 * 1000.0 / *fps) as i32;
                enc.add_frame(&rgb_to_rgba(frame), ts_ms)
                    .map_err(|e| Failure::DecodeError(format!("webp frame: {e}")))?;
                *n += 1;
                Ok(())
            }
            AnimEncoder::Gif(enc, fps, _) => {
                let rgba =
                    image::RgbaImage::from_raw(frame.width(), frame.height(), rgb_to_rgba(frame))
                        .expect("sized");
                let delay = image::Delay::from_numer_denom_ms((1000.0 / *fps).round() as u32, 1);
                enc.encode_frame(image::Frame::from_parts(rgba, 0, 0, delay))
                    .map_err(|e| Failure::DecodeError(format!("gif frame: {e}")))
            }
        }
    }

    fn finish(self) -> Result<Vec<u8>, Failure> {
        match self {
            AnimEncoder::Webp(enc, fps, n) => {
                let end_ms = (n as f64 * 1000.0 / fps) as i32;
                Ok(enc
                    .finalize(end_ms)
                    .map_err(|e| Failure::DecodeError(format!("webp finalize: {e}")))?
                    .to_vec())
            }
            AnimEncoder::Gif(enc, _, buf) => {
                drop(enc); // flushes trailer into the shared buffer
                Ok(std::mem::take(&mut *buf.0.lock().unwrap()))
            }
        }
    }
}

struct AnimFrames<'a> {
    store: &'a mut ClipStore,
    /// Source frame indices to use (fps/loop degradation picks a subset)
    indices: Vec<usize>,
    fps: f64,
}

/// Compose + encode one animated attempt at the given layout and quality.
#[allow(clippy::too_many_arguments)]
fn encode_animated(
    l: &SheetLayout,
    fonts: &Fonts,
    hm: &HeaderMeta,
    frame_tpl: &FrameTemplate,
    times: &[f64],
    af: &mut AnimFrames,
    format: AnimatedFormat,
    webp_q: f32,
    ctl: &GenControl,
    prog_base: f32,
    prog_span: f32,
) -> Result<Vec<u8>, Failure> {
    let chrome = render_chrome(l, fonts, hm, frame_tpl);
    let mut enc = AnimEncoder::new(format, l.card_w, l.card_h, webp_q, af.fps)?;

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
            draw_timestamp(
                &mut frame,
                l,
                tile as u32,
                fonts,
                &fmt_timestamp(t),
                frame_tpl.timestamp_style,
                frame_tpl.accent,
            );
        }
        enc.add_frame(&frame)?;
        ctl.report(prog_base + prog_span * (j + 1) as f32 / n as f32);
    }
    enc.finish()
}

/// One rung of the animated auto-fit ladder (PRD FR17: quality → fps → loop →
/// resolution, bounded, with quality-gate floors FR17a).
struct FitStep {
    q: f32,
    fps: f64,
    n_frames: usize,
    scale: f64,
}

fn anim_ladder(quality: u8, format: AnimatedFormat) -> Vec<FitStep> {
    let base_q = (55.0 + 0.4 * quality as f32).min(92.0);
    let q2 = (base_q - 14.0).max(38.0);
    let q3 = (base_q - 28.0).max(38.0);
    let mut steps = vec![
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
    ];
    if format == AnimatedFormat::Gif {
        // GIF has no quality knob — collapse the quality rungs so the ladder
        // is fps → loop → resolution only
        steps.remove(2);
        steps.remove(1);
    }
    steps
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
    pub template: &'a FrameTemplate,
    pub fonts: &'a Fonts,
}

/// Compose a framed sheet image: extract one frame per tile, degrade each tile's
/// media to the compression quality, then blit it into the template chrome. The
/// chrome itself is encoded crisp afterwards (`encode_static_sheet`), so quality
/// governs the video content only, not the frame.
async fn compose_still_sheet(
    inp: &JobInput<'_>,
    grid: GridDims,
    ctl: &GenControl,
    p0: f32,
    span: f32,
) -> Result<RgbImage, Failure> {
    let cfg = inp.config;
    let meta = inp.meta;
    let aspect = tile_aspect(cfg.orientation, meta);
    let frame_tpl = effective_frame(cfg.static_cfg.frame_on, inp.template);
    let l = static_layout(grid, aspect, 1.0, frame_tpl.header_band);
    let n = grid.tiles();
    let times = sample_times(meta.duration_s, n);
    let hm = header_meta(inp.title, meta);

    let inner_w = l.tile_w - 2 * l.hairline;
    let inner_h = l.tile_h - 2 * l.hairline;
    let mut img = render_chrome(&l, inp.fonts, &hm, &frame_tpl);
    for (i, &t) in times.iter().enumerate() {
        ctl.check()?;
        let tile = extract_frame(
            inp.source,
            t,
            inner_w,
            inner_h,
            meta.hdr,
            cfg.static_cfg.sharpen,
        )
        .await?;
        let tile = degrade_tile_media(&tile, cfg.static_cfg.format, cfg.static_cfg.quality);
        blit_tile(&mut img, &l, i as u32, &tile);
        draw_timestamp(
            &mut img,
            &l,
            i as u32,
            inp.fonts,
            &fmt_timestamp(t),
            frame_tpl.timestamp_style,
            frame_tpl.accent,
        );
        ctl.report(p0 + span * 0.85 * (i + 1) as f32 / n as f32);
    }
    Ok(img)
}

async fn generate_still(
    inp: &JobInput<'_>,
    ctl: &GenControl,
    p0: f32,
    span: f32,
) -> Result<ProducedArtifact, Failure> {
    let cfg = inp.config;
    let img = compose_still_sheet(inp, cfg.grid, ctl, p0, span).await?;
    let bytes = encode_static_sheet(&img, cfg.static_cfg.format)?;
    let dest = resolve_dest(inp.source, cfg, ArtifactKind::Static, ctl.overwrite);
    atomic_write(&dest, &bytes)?;
    if ctl.overwrite {
        remove_stale_siblings(inp.source, cfg, ArtifactKind::Static, &dest);
    }
    ctl.report(p0 + span);
    Ok(ProducedArtifact {
        kind: ArtifactKind::Static,
        path: dest.to_string_lossy().into(),
        bytes: bytes.len() as u64,
        degraded: false,
    })
}

/// Single-cell montage (PRD FR14, restored per user decision): one cell
/// playing sequential clips back to back — an animated loop, bare frames,
/// size-governed like the animated grid and sharing its format/target.
async fn generate_montage(
    inp: &JobInput<'_>,
    ctl: &GenControl,
    p0: f32,
    span: f32,
) -> Result<ProducedArtifact, Failure> {
    let cfg = inp.config;
    let meta = inp.meta;
    let anim = &cfg.animated;
    let target = (anim.target_mb * 1_000_000.0) as u64;
    let aspect = tile_aspect(cfg.orientation, meta);
    let long = 420.0 + 2.4 * anim.quality as f64;
    let (w, h) = if aspect >= 1.0 {
        (long as u32, (long / aspect) as u32)
    } else {
        ((long * aspect) as u32, long as u32)
    };
    let (w, h) = ((w & !1).max(2), (h & !1).max(2));

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

    let dest = resolve_dest(inp.source, cfg, ArtifactKind::Montage, ctl.overwrite);
    let base_q = (55.0 + 0.4 * anim.quality as f32).min(92.0);
    let q3 = (base_q - 28.0).max(38.0);
    // quality → fps → resolution, same governed order as the grid
    let mut attempts: Vec<(f32, usize, f64)> = vec![
        (base_q, 1, 1.0),
        ((base_q - 14.0).max(38.0), 1, 1.0),
        (q3, 1, 1.0),
        (q3, 2, 1.0), // half fps
        (q3, 2, 0.8),
        (q3, 2, 0.65),
    ];
    if anim.format == AnimatedFormat::Gif {
        attempts.remove(2);
        attempts.remove(1);
    }
    for (i, (q, stride, sc)) in attempts.iter().enumerate() {
        ctl.check()?;
        let (sw, sh) = (
            (((w as f64 * sc) as u32) & !1).max(2),
            (((h as f64 * sc) as u32) & !1).max(2),
        );
        let fps_out = ANIM_FPS / *stride as f64;
        let mut enc = AnimEncoder::new(anim.format, sw, sh, *q, fps_out)?;
        for fr in all_frames.iter().step_by(*stride) {
            ctl.check()?;
            if *sc < 1.0 {
                let scaled =
                    image::imageops::resize(fr, sw, sh, image::imageops::FilterType::Triangle);
                enc.add_frame(&scaled)?;
            } else {
                enc.add_frame(fr)?;
            }
        }
        let data = enc.finish()?;
        if data.len() as u64 <= target {
            atomic_write(&dest, &data)?;
            if ctl.overwrite {
                remove_stale_siblings(inp.source, cfg, ArtifactKind::Montage, &dest);
            }
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
        anim.target_mb
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
    let anim = &cfg.animated;
    let target = (anim.target_mb * 1_000_000.0) as u64;
    let aspect = tile_aspect(cfg.orientation, meta);
    // The animated grid always carries the full frame chrome (it's the
    // shareable preview); the frame toggle governs the still image only.
    let frame_tpl = inp.template.clone();
    let l0 = animated_layout(cfg.grid, aspect, anim.quality, 1.0);
    let n = cfg.grid.tiles();
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

    let dest = resolve_dest(inp.source, cfg, ArtifactKind::Animated, ctl.overwrite);
    let ladder = anim_ladder(anim.quality, anim.format);
    let n_steps = ladder.len();
    for (i, step) in ladder.iter().enumerate() {
        ctl.check()?;
        let l = animated_layout(cfg.grid, aspect, anim.quality, step.scale);
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
            &frame_tpl,
            &times,
            &mut af,
            anim.format,
            step.q,
            ctl,
            base,
            sp,
        )?;
        if bytes.len() as u64 <= target {
            atomic_write(&dest, &bytes)?;
            if ctl.overwrite {
                remove_stale_siblings(inp.source, cfg, ArtifactKind::Animated, &dest);
            }
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
        "animated preview ≥ {:.1} MB at fps/resolution floor",
        anim.target_mb
    )))
}

// ---------------------------------------------------------------- job entry

type ArtifactFut<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<ProducedArtifact, Failure>> + Send + 'a>,
>;

pub async fn run_job(
    source: &Path,
    config: &JobConfig,
    template: &FrameTemplate,
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
        template,
        fonts,
    };

    // Every enabled artifact is produced. With overwrite ON the canonical file
    // is replaced in place; with overwrite OFF an existing file is preserved and
    // a numbered `name (N).ext` copy is written instead (see resolve_dest), so a
    // re-run never clobbers and never silently no-ops.
    let skipped = Vec::new();
    let mut wanted: Vec<ArtifactKind> = Vec::new();
    let a = &config.artifacts;
    for (on, kind) in [
        (a.static_sheet, ArtifactKind::Static),
        (a.animated, ArtifactKind::Animated),
        (a.montage, ArtifactKind::Montage),
    ] {
        if on {
            wanted.push(kind);
        }
    }

    let weight_of = |k: &ArtifactKind| match k {
        ArtifactKind::Static => 1.0f32,
        ArtifactKind::Animated => 3.0,
        ArtifactKind::Montage => 1.5,
    };
    let weights: f32 = wanted.iter().map(weight_of).sum();

    let mut artifacts = Vec::new();
    let mut p0 = 0.05f32;
    for kind in wanted {
        let w = weight_of(&kind) / weights * 0.95;
        let (fut, budget): (ArtifactFut<'_>, Duration) = match kind {
            ArtifactKind::Static => (Box::pin(generate_still(&inp, ctl, p0, w)), STATIC_TIMEOUT),
            ArtifactKind::Animated => (Box::pin(generate_animated(&inp, ctl, p0, w)), ANIM_TIMEOUT),
            ArtifactKind::Montage => (
                Box::pin(generate_montage(&inp, ctl, p0, w)),
                MONTAGE_TIMEOUT,
            ),
        };
        let art = tokio::time::timeout(budget, fut)
            .await
            .map_err(|_| Failure::Timeout(format!("{kind:?} artifact exceeded time budget")))??;
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
