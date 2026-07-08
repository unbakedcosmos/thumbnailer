# thumbnailer.

Bulk video → **static contact sheet** + **animated WebP preview**, every artifact
guaranteed under a user-set size target. Tauri 2 shell, SvelteKit webview, Rust +
ffmpeg core. Built from a BMAD spec set (PRD, EXPERIENCE, DESIGN, design handoff —
kept locally, not committed).

## What it does

Drop a folder on the window (or **+ Add folder**). Every video becomes a queue row.
Tune grid (3×9 default), orientation (Auto reads each source's aspect and fits
without cropping), quality, target size (default 8 MB — a hard ceiling, not a wish),
and which artifacts to make. **Start batch** processes the whole queue unattended
with encode-aware concurrency; per-file failures are typed and isolated, the batch
never stops for one bad file, and nothing oversize is ever written — an artifact
that can't fit at the quality floor is reported, not shipped.

Artifacts land in a `srcs/` subfolder next to each video:

- `<basename>_contact.png` — static sheet (2× render; falls back to `_contact.jpg`
  if PNG can't meet the target)
- `<basename>_contact.webp` — animated grid, every tile a 2.5 s / 12 fps loop
- `<basename>_loop.webp` — single-cell montage of sequential clips

Close or crash mid-batch and the manifest restores completed work on relaunch
("Resumed — N left"); re-runs are idempotent unless Overwrite is on.

## Keyboard

↑/↓ select row · Enter generate selected · Space pause/resume batch · F toggle
follow-running · Esc close settings.

## Build & run

Prereqs: Rust (stable), Node 20+, ffmpeg + ffprobe on PATH (or drop static builds
in a `binaries/` dir next to the executable — that's the bundling seam; the build
must include `libwebp_anim` for probing only; animated encoding itself is done
in-process via libwebp).

```sh
npm install
npm run tauri dev      # develop
npm run tauri build    # package (deb/appimage on Linux, nsis on Windows)
cd src-tauri && cargo test   # pipeline + engine integration tests (needs ffmpeg)
```

v1 targets Windows per the PRD; the codebase is cross-platform and is developed
and verified on Linux. Windows-specific handling (long paths via `\\?\`, reserved
device names, no console flash on spawned encodes) is in place behind `cfg(windows)`.

## Architecture (src-tauri/src)

| Module        | Role                                                                                                            |
| ------------- | --------------------------------------------------------------------------------------------------------------- |
| `types.rs`    | The template spec (PRD §5) + failure taxonomy (FR5)                                                             |
| `theme.rs`    | Design tokens + bundled JetBrains Mono                                                                          |
| `ffmpeg.rs`   | Binary discovery (sidecar seam → PATH), Windows path/CLI safety                                                 |
| `probe.rs`    | ffprobe → duration/resolution/fps/codec/HDR, typed failures                                                     |
| `extract.rs`  | Frame + clip extraction over raw RGB pipes (VFR-safe, HDR tonemap when zscale exists)                           |
| `render.rs`   | The template executor: CSS-unit layout, header band, tile chrome, timestamps — shared by every artifact         |
| `pipeline.rs` | Per-video generation + bounded auto-fit ladders (quality → fps → loop → resolution), atomic writes, idempotency |
| `queue.rs`    | Batch engine: concurrency, pause/stop/retry, manifest resume, events                                            |
| `commands.rs` | Tauri IPC surface                                                                                               |

The frontend (`src/lib/*.svelte`) recreates `Thumbnailer.dc.html` from the design
handoff: queue rail, editor pane, settings overlay, empty state — tokens verbatim
from DESIGN.md, real progress from Rust events.

## Spec deviations worth knowing

- **Pause** stops dequeuing new files; in-flight encodes finish (an ffmpeg encode
  can't be frozen without losing its work). Stop cancels in-flight and resets
  running rows to queued.
- **Static fallback** is JPEG (`_contact.jpg`), not WebP, so the name never
  collides with the animated grid's `_contact.webp` (PRD OQ5 resolution).
- **Quality-floor misses surface as `skipped`** (warning) per the prototype's
  behavior; hard errors (unreadable/decode/timeout/disk-full) are `failed`.
