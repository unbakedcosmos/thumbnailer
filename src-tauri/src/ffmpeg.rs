//! ffmpeg/ffprobe discovery and invocation helpers.
//!
//! Resolution order: a sidecar `binaries/` dir next to the executable (the
//! bundled-ffmpeg seam for the Windows build, NFR7) → system PATH.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::OnceLock;
use tokio::process::Command;

fn sidecar(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let cand = dir.join("binaries").join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    });
    cand.is_file().then_some(cand)
}

pub fn ffmpeg_path() -> PathBuf {
    static P: OnceLock<PathBuf> = OnceLock::new();
    P.get_or_init(|| sidecar("ffmpeg").unwrap_or_else(|| PathBuf::from("ffmpeg")))
        .clone()
}

pub fn ffprobe_path() -> PathBuf {
    static P: OnceLock<PathBuf> = OnceLock::new();
    P.get_or_init(|| sidecar("ffprobe").unwrap_or_else(|| PathBuf::from("ffprobe")))
        .clone()
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
