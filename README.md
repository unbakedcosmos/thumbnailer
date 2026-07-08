# thumbnailer.

Bulk video → **static contact sheet** + **animated preview**, the animated artifact
guaranteed under a user-set size target. Tauri 2 shell, SvelteKit webview, Rust +
ffmpeg core. Built from a BMAD spec set (PRD, EXPERIENCE, DESIGN, design handoff r2 —
kept locally, not committed).

## What it does

Drop a folder on the window (or **+ Add folder** / **+ Add files**). Every video
becomes a queue row. Grid (3×9 default) and orientation (Auto reads each source's
aspect and fits without cropping) are shared; beyond that the two outputs have
their own panels:

- **Static image** — file type (PNG / JPEG / WebP), compression quality
  (hidden for lossless PNG), post-process **sharpen**, and a **frame**: sheet
  templates (Classic / Minimal / Bold built-ins + your own) controlling the header
  band, border weight, per-tile timestamp style (corner / overlay chips) and
  accent. Frame off = raw grab. Not size-gated.
- **Animated preview & montage** — file type (WebP / GIF), quality, and the
  **target size** stepper (default 8 MB — a hard ceiling, not a wish). Bounded
  auto-fit degrades quality → fps → loop length → resolution; below the floor the
  file is reported as can't-fit, never written oversize.

**Start batch** processes the whole queue unattended with encode-aware concurrency;
per-file failures are typed and isolated. The queue rail filters All / Issues and
failed rows carry an inline retry pill.

Artifacts land in a `srcs/` subfolder next to each video (extensions follow the
file-type choices):

- `<basename>_contact.{png|jpg|webp}` — static sheet (2× render)
- `<basename>_animated.{webp|gif}` — animated grid, every tile a 2.5 s / 12 fps loop
- `<basename>_montage.{webp|gif}` — single-cell loop of ~6 sequential clips

Templates are user data in `templates.json` next to the app settings — they persist
across sessions and batches. Close or crash mid-batch and the manifest restores
completed work on relaunch ("Resumed — N left"); re-runs are idempotent unless
Overwrite is on, and switching formats cleans up stale same-kind siblings.

## Keyboard

↑/↓ select row · Enter generate selected · Space pause/resume batch · F toggle
follow-running · Esc close modals.

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

## Spec interpretations worth knowing

- **Pause** stops dequeuing new files; in-flight encodes finish (an ffmpeg encode
  can't be frozen without losing its work). Stop cancels in-flight and resets
  running rows to queued.
- **Montage** is the original PRD FR14 artifact (user decision overriding the
  ambiguous r2 wording): a single cell playing ~6 sequential clips back to back,
  animated, bare frames, sharing the animated panel's format/quality/target.
- **Quality-floor misses surface as `skipped`** (warning) per the prototype's
  behavior; hard errors (unreadable/decode/timeout/disk-full) are `failed`.
- The **animated grid keeps its frame chrome** regardless of the frame toggle —
  the toggle governs the still image (r2 puts Frame in the static panel);
  GIF's auto-fit ladder skips the quality rungs (no quality knob in GIF).
