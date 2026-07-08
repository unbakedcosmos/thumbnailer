use serde::{Deserialize, Serialize};

/// Orientation policy for tile geometry (PRD FR8a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OrientationMode {
    #[default]
    Auto,
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridDims {
    pub cols: u32,
    pub rows: u32,
}

impl Default for GridDims {
    fn default() -> Self {
        // PRD FR8: default 3×9 = 27 tiles
        GridDims { cols: 3, rows: 9 }
    }
}

impl GridDims {
    pub fn tiles(&self) -> u32 {
        self.cols * self.rows
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactSet {
    pub static_sheet: bool,
    pub animated: bool,
    pub montage: bool,
}

impl Default for ArtifactSet {
    fn default() -> Self {
        ArtifactSet { static_sheet: true, animated: true, montage: false }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    #[default]
    Source,
    Custom,
}

/// The template spec (PRD §5): single source of truth every artifact is derived from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JobConfig {
    pub grid: GridDims,
    pub orientation: OrientationMode,
    /// 0–100 slider: size ↔ crispness (PRD FR20)
    pub quality: u8,
    /// Per-artifact hard size ceiling in MB (PRD FR16, default 8)
    pub target_mb: f64,
    pub artifacts: ArtifactSet,
    pub output_mode: OutputMode,
    pub output_path: Option<String>,
    /// Timestamp overlay per tile (PRD FR9, toggleable)
    pub timestamps: bool,
}

impl Default for JobConfig {
    fn default() -> Self {
        JobConfig {
            grid: GridDims::default(),
            orientation: OrientationMode::default(),
            quality: 62,
            target_mb: 8.0,
            artifacts: ArtifactSet::default(),
            output_mode: OutputMode::default(),
            output_path: None,
            timestamps: true,
        }
    }
}

/// What ffprobe tells us about a source file (PRD §10).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoMeta {
    pub duration_s: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub codec: String,
    pub pix_fmt: String,
    /// True when transfer characteristics look like HDR (PQ/HLG) — NFR3
    pub hdr: bool,
}

impl VideoMeta {
    pub fn is_portrait(&self) -> bool {
        self.height > self.width
    }
    pub fn aspect(&self) -> f64 {
        if self.height == 0 {
            16.0 / 9.0
        } else {
            self.width as f64 / self.height as f64
        }
    }
}

/// Failure taxonomy (PRD FR5). Display strings follow the EXPERIENCE.md voice:
/// terminal-plain, state the fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "kebab-case")]
pub enum Failure {
    Unreadable(String),
    UnsupportedCodec(String),
    DecodeError(String),
    /// Couldn't reach the target size even at the quality floor (FR17/FR17a)
    QualityFloor(String),
    DiskFull(String),
    Timeout(String),
    Cancelled,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Unreadable(d) => write!(f, "unreadable — {d}"),
            Failure::UnsupportedCodec(d) => write!(f, "unsupported codec — {d}"),
            Failure::DecodeError(d) => write!(f, "decode error — {d}"),
            Failure::QualityFloor(d) => write!(f, "can't reach target at quality floor — {d}"),
            Failure::DiskFull(d) => write!(f, "disk full — {d}"),
            Failure::Timeout(d) => write!(f, "timeout — {d}"),
            Failure::Cancelled => write!(f, "cancelled"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactKind {
    Static,
    Animated,
    Montage,
}

impl ArtifactKind {
    /// Output naming convention (PRD FR23). Static falls back to `_contact.jpg`
    /// (not .webp) so it never collides with the animated grid's name.
    pub fn suffix(&self) -> &'static str {
        match self {
            ArtifactKind::Static => "_contact.png",
            ArtifactKind::Animated => "_contact.webp",
            ArtifactKind::Montage => "_loop.webp",
        }
    }
}

/// One produced artifact, reported back to the UI (PRD FR18).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProducedArtifact {
    pub kind: ArtifactKind,
    pub path: String,
    pub bytes: u64,
    /// True when auto-fit had to degrade below the requested quality to fit
    pub degraded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobOutcome {
    pub artifacts: Vec<ProducedArtifact>,
    /// Artifacts skipped because they already existed (idempotent re-run, FR24)
    pub skipped_existing: Vec<ArtifactKind>,
}
