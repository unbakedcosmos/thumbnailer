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
        // A file produces exactly one output type (chosen in the editor); the
        // static contact sheet is the default. The set stays a struct so the
        // pipeline is unchanged — the UI just keeps a single member on.
        ArtifactSet {
            static_sheet: true,
            animated: false,
            montage: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    #[default]
    Source,
    Custom,
}

/// Encoder effort — trades encode time for quality/size. A global setting.
/// Drives WebP method + sharp-YUV, JPEG progressive, and how many candidate
/// frames the static sheet scans to pick the sharpest. Fast ≈ the pre-quality-
/// pass behaviour; Quality is the slow, best path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Fast,
    #[default]
    Balanced,
    Quality,
}

impl Effort {
    /// Candidate frames scanned per still tile to pick the sharpest (1 = just
    /// grab the frame at the timestamp).
    pub fn sharp_candidates(self) -> usize {
        match self {
            Effort::Fast => 1,
            Effort::Balanced => 3,
            Effort::Quality => 5,
        }
    }
    /// libwebp method (0–6): higher = slower but smaller/better.
    pub fn webp_method(self) -> i32 {
        match self {
            Effort::Fast => 3,
            Effort::Balanced => 4,
            Effort::Quality => 6,
        }
    }
    /// Same, typed for webp-animation's `usize` method field.
    pub fn webp_anim_method(self) -> usize {
        self.webp_method() as usize
    }
    /// Sharper (slower) RGB→YUV conversion — off on Fast.
    pub fn sharp_yuv(self) -> bool {
        !matches!(self, Effort::Fast)
    }
    /// Progressive JPEG (smaller, a little slower) — off on Fast.
    pub fn jpeg_progressive(self) -> bool {
        !matches!(self, Effort::Fast)
    }
}

/// Static / montage image format (CHANGELOG §1: File type PNG / JPEG / WebP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StaticFormat {
    #[default]
    Png,
    Jpeg,
    Webp,
}

impl StaticFormat {
    pub fn ext(&self) -> &'static str {
        match self {
            StaticFormat::Png => "png",
            StaticFormat::Jpeg => "jpg",
            StaticFormat::Webp => "webp",
        }
    }
}

/// Animated preview format (CHANGELOG §1: WebP / GIF).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AnimatedFormat {
    #[default]
    Webp,
    Gif,
}

impl AnimatedFormat {
    pub fn ext(&self) -> &'static str {
        match self {
            AnimatedFormat::Webp => "webp",
            AnimatedFormat::Gif => "gif",
        }
    }
}

/// Static & montage image knobs (CHANGELOG §1.3): format / sharpen / frame /
/// compression quality. Static output is NOT size-gated — the target gate
/// belongs to the animated preview only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StaticConfig {
    pub format: StaticFormat,
    /// Compression quality for JPEG/WebP (PNG is lossless — ignored)
    pub quality: u8,
    /// Post-process sharpen on extracted frames
    pub sharpen: bool,
    /// Frame toggle: off = raw grab (hairline border, no band, no timestamps)
    pub frame_on: bool,
    /// Sheet-frame template id (templates are user data, CHANGELOG §2)
    pub template_id: String,
}

impl Default for StaticConfig {
    fn default() -> Self {
        StaticConfig {
            format: StaticFormat::Png,
            quality: 80,
            sharpen: false,
            frame_on: true,
            template_id: "classic".into(),
        }
    }
}

/// Animated preview knobs (CHANGELOG §1.4): format / target gate / quality.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AnimatedConfig {
    pub format: AnimatedFormat,
    /// 0–100 slider: size ↔ crispness
    pub quality: u8,
    /// Hard size ceiling in MB — the size-gate control (PRD FR16)
    pub target_mb: f64,
}

impl Default for AnimatedConfig {
    fn default() -> Self {
        AnimatedConfig {
            format: AnimatedFormat::Webp,
            quality: 62,
            target_mb: 8.0,
        }
    }
}

/// The per-file config (CHANGELOG §1 build note: grid/orientation shared,
/// static and animated knobs split into their own panels).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct JobConfig {
    pub grid: GridDims,
    pub orientation: OrientationMode,
    pub artifacts: ArtifactSet,
    #[serde(rename = "static")]
    pub static_cfg: StaticConfig,
    pub animated: AnimatedConfig,
    pub output_mode: OutputMode,
    pub output_path: Option<String>,
}

// ---------------------------------------------------------------- templates

/// Sheet-frame template (CHANGELOG §2): controls only the frame around the
/// static sheet — grid/quality stay separate controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BorderStyle {
    None,
    #[default]
    Hairline,
    Thick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TimestampStyle {
    None,
    #[default]
    Corner,
    Overlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AccentChoice {
    #[default]
    Mint,
    White,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FrameTemplate {
    pub id: String,
    pub name: String,
    pub header_band: bool,
    pub border: BorderStyle,
    pub timestamp_style: TimestampStyle,
    pub accent: AccentChoice,
    pub builtin: bool,
}

impl Default for FrameTemplate {
    fn default() -> Self {
        FrameTemplate {
            id: "classic".into(),
            name: "Classic".into(),
            header_band: true,
            border: BorderStyle::Hairline,
            timestamp_style: TimestampStyle::Corner,
            accent: AccentChoice::Mint,
            builtin: true,
        }
    }
}

/// The three shipped built-ins (CHANGELOG §2): cannot be edited or deleted.
pub fn builtin_templates() -> Vec<FrameTemplate> {
    vec![
        FrameTemplate::default(),
        FrameTemplate {
            id: "minimal".into(),
            name: "Minimal".into(),
            header_band: false,
            border: BorderStyle::Hairline,
            timestamp_style: TimestampStyle::None,
            accent: AccentChoice::None,
            builtin: true,
        },
        FrameTemplate {
            id: "bold".into(),
            name: "Bold".into(),
            header_band: true,
            border: BorderStyle::Thick,
            timestamp_style: TimestampStyle::Overlay,
            accent: AccentChoice::Mint,
            builtin: true,
        },
    ]
}

/// The frame actually applied to a render: frame-off drops to a raw grab
/// (hairline border, no band, no timestamps) regardless of template.
pub fn effective_frame(frame_on: bool, tpl: &FrameTemplate) -> FrameTemplate {
    if frame_on {
        tpl.clone()
    } else {
        FrameTemplate {
            id: "raw".into(),
            name: "raw grab".into(),
            header_band: false,
            border: BorderStyle::Hairline,
            timestamp_style: TimestampStyle::None,
            accent: AccentChoice::None,
            builtin: true,
        }
    }
}

// ---------------------------------------------------------------- probe

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
    /// Output naming convention (CHANGELOG §3): suffix + extension follow the
    /// per-artifact file-type choices, so names never collide. Montage is an
    /// animated sequential loop, so it follows the animated format.
    pub fn suffix(&self, config: &JobConfig) -> String {
        match self {
            ArtifactKind::Static => format!("_contact.{}", config.static_cfg.format.ext()),
            ArtifactKind::Animated => format!("_animated.{}", config.animated.format.ext()),
            ArtifactKind::Montage => format!("_montage.{}", config.animated.format.ext()),
        }
    }

    /// Every extension this artifact could have been written with — used to
    /// clean up stale siblings when the user switches formats. Montage lists
    /// the legacy still extensions too so old grabs get swept.
    pub fn all_suffixes(&self) -> Vec<String> {
        match self {
            ArtifactKind::Static => ["png", "jpg", "webp"]
                .iter()
                .map(|e| format!("_contact.{e}"))
                .collect(),
            ArtifactKind::Animated => ["webp", "gif"]
                .iter()
                .map(|e| format!("_animated.{e}"))
                .collect(),
            ArtifactKind::Montage => ["webp", "gif", "png", "jpg"]
                .iter()
                .map(|e| format!("_montage.{e}"))
                .collect(),
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
