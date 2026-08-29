export const TASK_HELP = `JellyPilot task dispatcher

Usage:
  bun run task <command> [options]

Daily commands:
  dev [--rsdoctor] [--skip-setup]
  build [--rsdoctor] [--skip-setup]
  preview [--skip-setup]
  test [--watch] [--all] [--skip-setup]
  check
  fmt [--check]
  lint [--fix]
  typecheck [--skip-setup]

Rust:
  rust fmt [--check]
  rust check [crate...]
  rust clippy [crate...]
  rust test [crate...]

WASM and sidecars:
  wasm install
  wasm build [--dev|--release]
  ffmpeg prepare [--verify] [--target <rust-target-triple>]
  iced run [--smoke] [--release]

Native E2E (delegated to e2e/cli.ts; --skip-setup skips the setup chain):
  e2e build [args...]
  e2e test [args...]
  e2e typecheck [args...]
  e2e isolation [args...]
  e2e verify [args...]
  e2e clean [args...]

Utilities:
  api
  panda codegen
  review panda-tauri
  review parity [args...]

Crate short names:
  core           jellypilot-core
  core-wasm      jellypilot-core-wasm
  media-server   jellypilot-media-server
  mpv            jellypilot-mpv
  session        jellypilot-session
  playback-core  jellypilot-playback-core
  iced           jellypilot-ui, jellypilot-iced`;
