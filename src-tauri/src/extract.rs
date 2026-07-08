//! Frame and clip extraction: ffmpeg decodes, scales, letterboxes and pipes
//! raw RGB24 back to us. All grid composition happens in Rust (render.rs) so
//! the static and animated artifacts share one template executor (PRD §5).

use crate::ffmpeg::{base_command, ffmpeg_path, os_path};
use crate::types::Failure;
use image::RgbImage;
use std::path::Path;
use std::process::Stdio;
use std::sync::OnceLock;
use tokio::io::AsyncReadExt;

/// Whether this ffmpeg build has zscale (needed for real HDR tonemapping, NFR3).
fn has_zscale() -> bool {
    static Z: OnceLock<bool> = OnceLock::new();
    *Z.get_or_init(|| {
        std::process::Command::new(ffmpeg_path())
            .args(["-hide_banner", "-filters"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(" zscale "))
            .unwrap_or(false)
    })
}

/// Scale + letterbox chain into an exact w×h tile. Letterbox bars use
/// surface-2 (#1f2126) per the design's tile-placeholder token.
fn filter_chain(w: u32, h: u32, fps: Option<f64>, hdr: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(f) = fps {
        // fps filter also normalizes variable-frame-rate sources (NFR3)
        parts.push(format!("fps={f:.3}"));
    }
    if hdr && has_zscale() {
        // Tonemap PQ/HLG to SDR so tiles aren't washed out or grey
        parts.push(
            "zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=hable,\
             zscale=t=bt709:m=bt709:r=tv"
                .into(),
        );
    }
    parts.push(format!(
        "scale={w}:{h}:force_original_aspect_ratio=decrease:flags=bicubic"
    ));
    parts.push(format!("pad={w}:{h}:-1:-1:color=0x1f2126"));
    parts.push("setsar=1".into());
    parts.push("format=rgb24".into());
    parts.join(",")
}

fn classify(stderr: &str) -> Failure {
    let s = stderr.to_lowercase();
    if s.contains("moov atom not found") || s.contains("invalid data found") {
        Failure::Unreadable("truncated".into())
    } else if s.contains("decoder") && s.contains("not found") {
        Failure::UnsupportedCodec(
            stderr.lines().next().unwrap_or("decoder not found").trim().to_string(),
        )
    } else if s.contains("no space left") {
        Failure::DiskFull("while decoding".into())
    } else {
        let line = stderr
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("decode failed")
            .trim();
        Failure::DecodeError(line.chars().take(120).collect())
    }
}

/// Run ffmpeg, streaming exactly-sized RGB24 frames from stdout.
/// Returns as many complete frames as the file yielded.
async fn run_rawvideo(args: Vec<std::ffi::OsString>, w: u32, h: u32, max_frames: usize) -> Result<Vec<RgbImage>, Failure> {
    let mut child = base_command(ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
        .args(args)
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Failure::DecodeError(format!("ffmpeg failed to start: {e}")))?;

    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let stderr_task = tokio::spawn(async move {
        let mut buf = String::new();
        let _ = stderr.read_to_string(&mut buf).await;
        buf
    });

    let frame_len = (w * h * 3) as usize;
    let mut frames = Vec::new();
    let mut buf = vec![0u8; frame_len];
    'read: while frames.len() < max_frames {
        let mut filled = 0;
        while filled < frame_len {
            match stdout.read(&mut buf[filled..]).await {
                Ok(0) => break 'read,
                Ok(n) => filled += n,
                Err(_) => break 'read,
            }
        }
        if filled < frame_len {
            break;
        }
        frames.push(RgbImage::from_raw(w, h, buf.clone()).expect("sized buffer"));
    }
    drop(stdout);

    let status = child.wait().await.map_err(|e| Failure::DecodeError(e.to_string()))?;
    let errtxt = stderr_task.await.unwrap_or_default();
    if frames.is_empty() {
        return Err(if !status.success() || !errtxt.trim().is_empty() {
            classify(&errtxt)
        } else {
            Failure::DecodeError("no frames decoded".into())
        });
    }
    Ok(frames)
}

/// One still frame at `t` seconds, letterboxed into w×h.
pub async fn extract_frame(path: &Path, t: f64, w: u32, h: u32, hdr: bool) -> Result<RgbImage, Failure> {
    let vf = filter_chain(w, h, None, hdr);
    let args: Vec<std::ffi::OsString> = vec![
        "-ss".into(), format!("{t:.3}").into(),
        "-i".into(), os_path(path),
        "-frames:v".into(), "1".into(),
        "-vf".into(), vf.into(),
    ];
    let frames = run_rawvideo(args, w, h, 1).await?;
    Ok(frames.into_iter().next().unwrap())
}

/// A short clip starting at `t`: `n_frames` frames at `fps`, letterboxed into
/// w×h. If the source runs out early, the last frame is repeated so every
/// tile has a uniform frame count.
pub async fn extract_clip(
    path: &Path,
    t: f64,
    fps: f64,
    n_frames: usize,
    w: u32,
    h: u32,
    hdr: bool,
) -> Result<Vec<RgbImage>, Failure> {
    let dur = n_frames as f64 / fps + 0.25;
    let vf = filter_chain(w, h, Some(fps), hdr);
    let args: Vec<std::ffi::OsString> = vec![
        "-ss".into(), format!("{t:.3}").into(),
        "-t".into(), format!("{dur:.3}").into(),
        "-i".into(), os_path(path),
        "-vf".into(), vf.into(),
        "-frames:v".into(), n_frames.to_string().into(),
    ];
    let mut frames = run_rawvideo(args, w, h, n_frames).await?;
    while frames.len() < n_frames {
        let last = frames.last().unwrap().clone();
        frames.push(last);
    }
    Ok(frames)
}
