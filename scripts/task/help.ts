export const TASK_HELP = `JellyPilot task dispatcher

Usage:
  bun run task <command> [options]

Daily commands:
  check
  fmt [--check]
  lint [--fix]
  typecheck

Rust:
  rust fmt [--check]
  rust check [crate...]
  rust clippy [crate...]
  rust test [crate...]

Application:
  iced run [--smoke] [--release] [--embedded] (env: JELLYPILOT_SMOKE_SIZE=WxH)
  iced build [--release]          (build the official launcher without running it)
  iced hot                       (hot reload via cargo-hot, dev feature)
  iced prepare [--source <checkout>] (exact published fork revision from iced-source.json)
    All maintained Cargo tasks automatically use the same prepared target/vendor iced source.
    --embedded forces the pinned staged libmpv; explicit env overrides must be absolute:
    JELLYPILOT_LIBMPV, JELLYPILOT_MPV_BASELINE. Linux defaults to Embedded MPV Playback.
  iced regress <tray|external|gpu|all> [--file <media>] [--out <report-dir>] [--hwdec <no|vaapi|vaapi-copy>]
    Linux opt-in native probes; gpu/all requires a real readable local media file before startup.
    Fresh run-identified reports: target/native-regression by default. Color acceptance stays human.

Embedded mpv dependency (Linux/Windows Vulkan):
  mpv build [--source <checkout>]
    Requires pinned clean mpv source, Meson >=1.3, Ninja, C/C++ compiler, pkg-config,
    Vulkan development headers/loader, FFmpeg, libplacebo >=7.360.1 and libass.
    Without --source, fetches the pinned revision from https://github.com/hewel/mpv.git.
    Stages target/embedded-mpv/{lib/jellypilot/libmpv.so|libmpv-2.dll,share/jellypilot/mpv-baseline.conf,manifest.json}.
    Source/options pinned; host dependency versions recorded, not bit-reproducible.

Windows release package:
  package windows --runtime-dir <MSYS2 UCRT64 bin directory>
    Requires a fresh mpv build; installs cargo-packager 0.11.8 in target/tools if unavailable.
    Collects the DLL dependency closure and licenses, builds the release launcher,
    and packages NSIS with embedded resources. Output: target/release/bundle/*.exe.

Utilities:
  api
  monitor --pid <pid> --out <target/path> [--samples 301] [--interval-ms 1000] [--label <text>]

Android (no iced/desktop preparation; SDK at ANDROID_HOME or ~/Android/Sdk):
  android doctor                 (report JDK/SDK/NDK/cmake/rustup/artifact prerequisites)
  android bindings               (regenerate UniFFI Kotlin bindings into core-bridge/build)
  android rust [--release]       (cross-build libjellypilot_ffi.so for aarch64-linux-android26)
  android mpv                    (pinned native libmpv pipeline -> target/android/mpv/arm64-v8a)
  android build [--release]      (bindings + rust + mpv, then Gradle assemble; never skips natives)
  android check                  (Gradle lint + unit tests with fresh bindings)

Crate short names:
  auth           jellypilot-auth
  core           jellypilot-core
  media-server   jellypilot-media-server
  mpv            jellypilot-mpv
  mpv-host       jellypilot-mpv-host
  session        jellypilot-session
  sdk            jellypilot-sdk
  ffi            jellypilot-ffi
  iced           jellypilot-ui, jellypilot-mpv-host, jellypilot-iced, jellypilot-launcher`;
