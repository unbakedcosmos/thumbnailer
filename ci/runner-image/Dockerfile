# Prebaked job image for the thumbnailer CI/release workflows.
#
# Eliminates the per-run apt-get and Node downloads that dominate cold runs on
# an ephemeral job container. Build it on the runner host, then point a runner
# label at it in the forgejo-runner config.yml:
#
#   docker build -t thumbnailer-ci:latest ci/runner-image
#
#   # runner config.yml → runner.labels:
#   #   - "ci-amd64:docker://thumbnailer-ci:latest"
#
# The workflow steps stay unchanged — apt-get sees everything installed and
# finishes in seconds, setup-node finds the matching Node in the tool cache,
# and the actions/cache + rust-cache steps keep handling npm/cargo/xwin state.

FROM node:22-bookworm

ENV DEBIAN_FRONTEND=noninteractive

# Everything the CI and release workflows apt-get on every run
RUN apt-get update && apt-get install -y --no-install-recommends \
    libwebkit2gtk-4.1-dev build-essential pkg-config libssl-dev libgtk-3-dev \
    libayatana-appindicator3-dev librsvg2-dev libxdo-dev ffmpeg curl file \
    patchelf nsis clang lld llvm git git-lfs ca-certificates zstd \
    && rm -rf /var/lib/apt/lists/*

# Rust toolchain with the Windows cross-target and lint components preinstalled.
# dtolnay/rust-toolchain notices the matching default toolchain and is a no-op.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    --default-toolchain stable \
    --component clippy rustfmt \
    --target x86_64-pc-windows-msvc
ENV PATH="/root/.cargo/bin:${PATH}"
