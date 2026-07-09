# Changelog

All notable changes to Thumbnailer. Versions are the git tags that trigger a
release build; hyphenated tags (e.g. `v0.3.0-beta`) publish as prereleases.

## v0.3.2-beta

### Added

- **ffmpeg auto-detect + guided prompt.** On startup the app checks for ffmpeg and
  ffprobe; if either is missing it shows a prompt explaining how to install one, a
  button to open the official download page, and a writable `<app-data>/binaries/`
  folder to drop a static build into — then **Re-check** picks it up live (no
  restart). Discovery is re-evaluated per call: `<app-data>/binaries` →
  `<exe-dir>/binaries` → `<exe-dir>` → PATH.

## v0.3.1-beta

### Changed

- **Editor layout matches the r3 handoff.** Output folder moved up to share the
  top row with Output type; Grid/Orientation sit in the row below.
- **Config applies to all files by default.** An "Apply to all files" toggle
  (on by default) in the editor propagates every setting change across the whole
  queue; turn it off to tweak a single file (the old "Apply config to batch"
  becomes a one-shot "Apply config once").
- **Preview region box.** The preview now sits inside an outer container
  (`#0d0e11`, 8px radius) so it reads as a distinct block regardless of the
  chosen sheet frame — separating the UI divider from the artifact's own frame.

## v0.3.0-beta

### Added

- **App icon** — the "t + nail" mark (direction 2c): JetBrains Mono `t` with a
  mint nail hanging where the wordmark's period sits, on the squircle gradient
  ground. Full desktop icon set (`.ico` / `.icns` / PNGs) plus a webview favicon.

### Changed

- **Single-select Output type.** The editor's three independent artifact toggles
  are now one segmented **Output type** control — a file produces exactly one of
  Static / Animated / Montage. New files default to Static; the grid is hidden for
  Montage (it's a single cell). Montage stays an animated loop.
- **Overwrite = Off now appends instead of skipping.** A re-run preserves the
  existing file and writes the next free numbered copy (`_contact (1).png`, `(2)`,
  …) rather than skipping it, and never clobbers. This applies to the batch and
  the per-file Generate/Retry button alike. Overwrite On replaces in place and
  sweeps stale same-kind siblings on a format switch.
- **Reopen starts fresh.** A batch that finished no longer reloads last session on
  relaunch — the stale manifest is cleared. Genuinely unfinished (queued) work is
  still restored for crash-resume.
- **Size estimates recalibrated.** Estimates are measured against real output:
  animated/montage clamp at the size target (the encoder never exceeds it) and
  read as "size-capped" at the ceiling; static is per-format (PNG flat, JPEG and
  the much-smaller WebP scale with quality).

## v0.2.1-beta

- Restored Montage as the sequential animated loop (PRD FR14); fixed inverted
  editor previews (animated shows the grid, montage one cell).

## v0.2.0

- Design handoff r2: split Static/Animated control panels, sheet-frame templates
  (Classic/Minimal/Bold + user templates), output formats (PNG/JPEG/WebP,
  WebP/GIF), queue All/Issues filter, inline retry, follow-running, keyboard
  shortcuts, default target size, and Add files.

## v0.1.0

- Initial release: bulk video → static contact sheets + animated WebP previews,
  size-gated with a bounded auto-fit ladder; tokio batch engine with typed
  per-file failures and crash-resume.
