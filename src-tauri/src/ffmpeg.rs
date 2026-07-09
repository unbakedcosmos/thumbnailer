//! ffmpeg/ffprobe discovery and invocation helpers.
//!
//! The app does not bundle ffmpeg; it auto-detects it and (via the guided prompt)
//! lets the user drop a binary into a writable app-data `binaries/` folder.
//! Resolution order, re-evaluated on every call so a freshly-added binary is
//! picked up without a restart:
//!   1. `<app-data>/binaries/ffmpeg[.exe]`  — where the prompt drops it
//!   2. `<exe-dir>/binaries/ffmpeg[.exe]`   — manual "next to the app" seam
//!   3. `<exe-dir>/ffmpeg[.exe]`
//!   4. system PATH (dev)

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::OnceLock;
use tokio::process::Command;

/// Writable folder (app data dir) a user can drop ffmpeg/ffprobe into. Set once
/// at startup from `lib.rs`, where the resolved app-data path is known.
static BUNDLE_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn set_bundle_dir(dir: PathBuf) {
    let _ = BUNDLE_DIR.set(dir);
}

/// The drop folder, created if missing, for the "open folder" prompt action.
pub fn bin_dir() -> Option<PathBuf> {
    let d = BUNDLE_DIR.get()?.clone();
    let _ = std::fs::create_dir_all(&d);
    Some(d)
}

fn os_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn discover(name: &str) -> Option<PathBuf> {
    let file = os_name(name);
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Some(d) = BUNDLE_DIR.get() {
        cands.push(d.join(&file));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            cands.push(dir.join("binaries").join(&file));
            cands.push(dir.join(&file));
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

pub fn ffmpeg_path() -> PathBuf {
    discover("ffmpeg").unwrap_or_else(|| PathBuf::from("ffmpeg"))
}

pub fn ffprobe_path() -> PathBuf {
    discover("ffprobe").unwrap_or_else(|| PathBuf::from("ffprobe"))
}

/// On Windows, prefix a path with `\\?\` when it exceeds classic MAX_PATH
/// so long library paths survive (PRD FR23).
pub fn os_path(p: &std::path::Path) -> std::ffi::OsString {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let s = p.as_os_str();
        if s.encode_wide().count() > 240 && !p.starts_with(r"\\?\") {
            let mut long = std::ffi::OsString::from(r"\\?\");
            long.push(s);
            return long;
        }
        s.to_os_string()
    }
    #[cfg(not(windows))]
    {
        p.as_os_str().to_os_string()
    }
}

pub fn base_command(bin: PathBuf) -> Command {
    let mut cmd = Command::new(bin);
    cmd.stdin(Stdio::null());
    cmd.kill_on_drop(true);
    #[cfg(windows)]
    {
        // No console window flash per spawned encode on Windows
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// First line of `ffmpeg -version`, e.g. "ffmpeg version 5.1.9-0+deb12u1 …" → "5.1.9".
pub async fn ffmpeg_version() -> Option<String> {
    let out = base_command(ffmpeg_path())
        .arg("-version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    let first = String::from_utf8_lossy(&out.stdout);
    let first = first.lines().next()?;
    let ver = first.split_whitespace().nth(2)?;
    Some(ver.split('-').next().unwrap_or(ver).to_string())
}

/// ffprobe is needed for metadata; a usable ffmpeg without it is still broken.
pub async fn ffprobe_ok() -> bool {
    base_command(ffprobe_path())
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}
