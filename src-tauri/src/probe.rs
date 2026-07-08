//! ffprobe metadata extraction (PRD §10) with the failure taxonomy applied.

use crate::ffmpeg::{base_command, ffprobe_path, os_path};
use crate::types::{Failure, VideoMeta};
use std::path::Path;
use std::process::Stdio;

pub async fn probe(path: &Path) -> Result<VideoMeta, Failure> {
    let out = base_command(ffprobe_path())
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(os_path(path))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| Failure::Unreadable(format!("ffprobe failed to run: {e}")))?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        let s = stderr.to_lowercase();
        return Err(
            if s.contains("moov atom not found") || s.contains("invalid data") {
                Failure::Unreadable("truncated".into())
            } else {
                Failure::Unreadable(first_line(&stderr))
            },
        );
    }

    let v: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|_| Failure::Unreadable("unparseable probe output".into()))?;

    let streams = v["streams"].as_array().cloned().unwrap_or_default();
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1)
        .ok_or_else(|| Failure::UnsupportedCodec("no video stream".into()))?;

    let width = video["width"].as_u64().unwrap_or(0) as u32;
    let height = video["height"].as_u64().unwrap_or(0) as u32;
    if width == 0 || height == 0 {
        return Err(Failure::UnsupportedCodec(
            "video stream has no dimensions".into(),
        ));
    }

    let codec = video["codec_name"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    let pix_fmt = video["pix_fmt"].as_str().unwrap_or("").to_string();
    let transfer = video["color_transfer"].as_str().unwrap_or("");
    let hdr = matches!(transfer, "smpte2084" | "arib-std-b67");

    let fps = parse_rate(video["avg_frame_rate"].as_str().unwrap_or(""))
        .or_else(|| parse_rate(video["r_frame_rate"].as_str().unwrap_or("")))
        .unwrap_or(0.0);

    let duration_s = v["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse::<f64>().ok())
        .or_else(|| video["duration"].as_str().and_then(|d| d.parse().ok()))
        .unwrap_or(0.0);
    if duration_s <= 0.0 {
        return Err(Failure::Unreadable(
            "no duration (truncated or still being written)".into(),
        ));
    }

    Ok(VideoMeta {
        duration_s,
        width,
        height,
        fps,
        codec,
        pix_fmt,
        hdr,
    })
}

fn parse_rate(r: &str) -> Option<f64> {
    let (n, d) = r.split_once('/')?;
    let n: f64 = n.parse().ok()?;
    let d: f64 = d.parse().ok()?;
    if d == 0.0 || n == 0.0 {
        None
    } else {
        Some(n / d)
    }
}

fn first_line(s: &str) -> String {
    let line = s.lines().next().unwrap_or("unknown error").trim();
    // Strip the "path: " prefix ffmpeg puts in front of the reason
    match line.rsplit_once(": ") {
        Some((_, tail)) if !tail.is_empty() => tail.to_string(),
        _ => line.to_string(),
    }
}

pub fn fmt_duration(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}
