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
    --embedded selects staged libmpv and baseline; explicit env overrides must be absolute:
    JELLYPILOT_LIBMPV, JELLYPILOT_MPV_BASELINE. External playback remains the default.
  iced regress <tray|external|gpu|all> [--file <media>] [--out <report-dir>]
    Linux opt-in native probes; gpu/all requires a real readable local media file before startup.
    Fresh run-identified reports: target/native-regression by default. Color acceptance stays human.

Embedded mpv dependency (Linux Vulkan):
  mpv build [--source <checkout>]
    Requires pinned clean mpv source, Meson >=1.3, Ninja, C/C++ compiler, pkg-config,
    Vulkan development headers/loader, FFmpeg, libplacebo >=7.360.1 and libass.
    Without --source, fetches the pinned revision from git@github.com:hewel/mpv.git.
    Stages target/embedded-mpv/{lib/jellypilot/libmpv.so,share/jellypilot/mpv-baseline.conf,manifest.json}.
    Source/options pinned; host dependency versions recorded, not bit-reproducible.

Local video experiment:
  iced local-video run [--smoke] [--release] [--file <path> | --url-env | --url <http(s)url>]
    --url-env reads JELLYPILOT_VIDEO_URL; --url exposes it in caller argv/history.
  iced local-video check
  iced local-video clippy
  iced local-video test [filter]
  iced local-video fmt [--check]

Utilities:
  api
  monitor --pid <pid> --out <target/path> [--samples 301] [--interval-ms 1000] [--label <text>]

Crate short names:
  core           jellypilot-core
  media-server   jellypilot-media-server
  mpv            jellypilot-mpv
  mpv-host       jellypilot-mpv-host
  session        jellypilot-session
  iced           jellypilot-ui, jellypilot-mpv-host, jellypilot-iced, jellypilot-launcher`;
