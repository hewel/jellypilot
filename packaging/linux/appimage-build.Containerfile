# Build from the repository root; only this directory and rust-toolchain.toml are
# needed in the build context. Mount the checkout at /work when running the image.
FROM docker.io/library/ubuntu:22.04

SHELL ["/bin/bash", "-euo", "pipefail", "-c"]
ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/opt/cargo \
    RUSTUP_HOME=/opt/rustup \
    XDG_CACHE_HOME=/opt/packager-cache \
    APPIMAGE_EXTRACT_AND_RUN=1 \
    JELLYPILOT_APPIMAGE_BUILD=1 \
    PKG_CONFIG_PATH=/usr/local/lib/pkgconfig:/usr/local/share/pkgconfig \
    PATH=/opt/cargo/bin:/usr/local/bin:/usr/local/sbin:/usr/sbin:/usr/bin:/sbin:/bin

# All binary development/runtime dependencies come from signed Jammy archives.
# In particular, do not install another distribution's FFmpeg or Vulkan binaries.
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git build-essential pkg-config cmake ninja-build \
      python3 python3-pip python3-jinja2 python3-markupsafe \
      binutils patchelf file unzip xz-utils nasm yasm \
      libgtk-3-dev libayatana-appindicator3-1 libxkbcommon-dev libwayland-dev wayland-protocols \
      libfuse2 libssl-dev libffi-dev libexpat1-dev libxml2-dev \
      libvulkan-dev liblcms2-dev \
      libass-dev libluajit-5.1-dev libmujs-dev libarchive-dev libbluray-dev \
      libasound2-dev libpulse-dev libjack-jackd2-dev librubberband-dev \
      libuchardet-dev libzimg-dev libdrm-dev libgbm-dev libegl1-mesa-dev \
      libgl1-mesa-dev libva-dev libvdpau-dev libmfx-dev \
      libx11-dev libxext-dev libxfixes-dev libxrandr-dev libxpresent-dev \
      libxinerama-dev libxss-dev libxv-dev libxkbcommon-x11-dev libxcb-shape0-dev \
      libgnutls28-dev libaom-dev libdav1d-dev libx264-dev libx265-dev \
      libvpx-dev libopus-dev libvorbis-dev libmp3lame-dev libsoxr-dev \
      libopenjp2-7-dev libwebp-dev libsmbclient-dev libssh-dev \
      zlib1g-dev libbz2-dev liblzma-dev \
    && rm -rf /var/lib/apt/lists/*

# Jammy's Meson is too old for the pinned mpv/libplacebo. Install the pinned
# platform-independent Python package; native libraries are built below.
RUN python3 -m pip install --no-cache-dir meson==1.7.2
COPY packaging/linux/build-appimage-deps.sh /opt/build-appimage-deps.sh
RUN bash /opt/build-appimage-deps.sh shaderc
RUN bash /opt/build-appimage-deps.sh wayland
RUN bash /opt/build-appimage-deps.sh protocols
RUN bash /opt/build-appimage-deps.sh vulkan
RUN bash /opt/build-appimage-deps.sh ffmpeg

# rustup verifies Rust's distribution checksums. Keep this toolchain in the image,
# rather than borrowing host-compiled tools with an unknown glibc baseline.
COPY rust-toolchain.toml /opt/rust-toolchain.toml
# Pin the official rustup bootstrap as well. TLS 1.2 avoids Jammy curl/OpenSSL
# handshake failures seen against the Rust CDN; certificate checking stays on.
RUN curl --proto '=https' --tlsv1.2 --tls-max 1.2 --http1.1 -fsSL \
      https://static.rust-lang.org/rustup/archive/1.28.2/x86_64-unknown-linux-gnu/rustup-init \
      -o /tmp/rustup-init \
    && echo '20a06e644b0d9bd2fbdbfd52d42540bdde820ea7df86e92e533c073da0cdd43c  /tmp/rustup-init' | sha256sum --check \
    && chmod 0755 /tmp/rustup-init \
    && /tmp/rustup-init -y --profile minimal --default-toolchain none --no-modify-path \
    && cd /opt && rustup show \
    && cargo install cargo-packager --version 0.11.8 --locked \
    && rm /tmp/rustup-init

# cargo-packager 0.11.8 otherwise downloads AppRun with mode 0764. Seed its exact
# cache location and executable permissions, including the two deployment tools.
RUN tools="${XDG_CACHE_HOME}/.cargo-packager/AppImage"; mkdir -p "$tools"; \
    curl -fL --retry 5 https://github.com/tauri-apps/binary-releases/releases/download/apprun-old/AppRun-x86_64 \
      -o "$tools/AppRun-x86_64"; \
    curl -fL --retry 5 https://github.com/tauri-apps/binary-releases/releases/download/linuxdeploy/linuxdeploy-x86_64.AppImage \
      -o "$tools/linuxdeploy-x86_64.AppImage"; \
    curl -fL --retry 5 https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/continuous/linuxdeploy-plugin-appimage-x86_64.AppImage \
      -o "$tools/linuxdeploy-plugin-appimage.AppImage"; \
    printf '%s  %s\n' \
      f30140a43a0a59e46db21bdefdf749b9e9f2c6946e92afabbacf98b8ae73fb4f "$tools/AppRun-x86_64" \
      e762bea85c8eb0d4b3508d46e5c1f037f717d0f9303ae3b4aafc8b04991fa1ef "$tools/linuxdeploy-x86_64.AppImage" \
      49d6a17160675a6bd1781699aae6bdf7692d98552e02a3671d2183d10547842e "$tools/linuxdeploy-plugin-appimage.AppImage" \
      | sha256sum --check; \
    chmod 0755 "$tools/"*

# Meson asks GCC for static library search paths; ldconfig alone doesn't expose
# the pinned glslang/SPIRV archives under the native dependency prefix.
ENV LIBRARY_PATH=/usr/local/lib
WORKDIR /work
CMD ["bash"]
