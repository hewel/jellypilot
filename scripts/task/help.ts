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
  iced run [--smoke] [--release] (env: JELLYPILOT_SMOKE_SIZE=WxH)

Utilities:
  api

Crate short names:
  core           jellypilot-core
  core-wasm      jellypilot-core-wasm
  media-server   jellypilot-media-server
  mpv            jellypilot-mpv
  session        jellypilot-session
  playback-core  jellypilot-playback-core
  iced           jellypilot-ui, jellypilot-iced`;
