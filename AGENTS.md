# AGENTS.md

Cross-platform Jellyfin/Emby companion app built with a pinned iced fork (ADR 0027 native architecture). Fully custom-drawn
native UI; Linux defaults to Embedded MPV Playback with the pinned mpv fork. External MPV
Playback remains the default on other platforms and an explicit Linux option. See README's
embedded build/selection contract. No webview/Tauri.

## Role Separation

**scoped-implementer**

* Only implements the requested changes.
* Inspects its changes and runs applicable checks; independent approval belongs to a reviewer.

**reviewer**

* Only finds bugs and explains why the code may fail.
* Does not edit code, write fixes, or implement changes.

## Stack

- **Application**: safe Rust + iced in `src-iced` (`jellypilot-iced` library); private `jellypilot-launcher` binary `jellypilot` owns the two trusted engine/surface handoffs
- **Design system**: `crates/jellypilot-ui` — tokens, theme/Catalog styles, custom widgets, overlay
- **Domain crates**: `jellypilot-core` (display-free browse/config/diagnostics),
  `jellypilot-media-server` (Jellyfin/Emby HTTP + artwork), `jellypilot-auth` (login + keyring),
  `jellypilot-mpv` (process + JSON IPC), `jellypilot-session` (WebSocket remote sessions)
- **Embedded host**: `crates/jellypilot-mpv-host` owns Vulkan/libmpv FFI; `src-iced/src/embedded` supplies the closed official compositor and player primitive
- **Generated API clients**: `crates/media-server-api/{jellyfin,emby}` (OpenAPI, regenerate on demand via
  `cargo xtask api generate <jellyfin|emby>`; the Java generator runs only on demand)
- **Tooling**: native Rust `xtask`; desktop builds and checks need neither Bun nor Java
- **Optional Reicon MCP**: pinned `reicon-mcp@1.1.102` runs through `npx`; Node.js/npm are
  required only for that integration, not for normal builds or checks

## Where to Look

| Task | Location | Notes |
|---|---|---|
| App shell, state, update | `src-iced/src/app/` | `state.rs`, `message.rs`, `update.rs`, `subscriptions.rs` |
| App screens | `src-iced/src/app/view/` | shell, login, home, browse, detail, player, settings |
| System tray | `src-iced/src/tray.rs` | tray-icon menu, sync from SessionView |
| Theme and tokens | `crates/jellypilot-ui/src/{theme,tokens}.rs` | Catalog styles in `widgets/` and `overlay/` |
| Display-free logic | `crates/jellypilot-core/src/` | browse model, config, request gate, diagnostics, artwork planner |
| Media server client | `crates/jellypilot-media-server/src/` | `client.rs`, `types.rs`, `artwork.rs` |
| MPV IPC | `crates/jellypilot-mpv/src/` | `client.rs`, `playback.rs`, `protocol.rs` |
| Remote sessions | `crates/jellypilot-session/src/` | WebSocket command channel |
| Task dispatcher | `xtask/src/` | Native Rust CLI via `cargo xtask` |
| Add test | beside the code (`#[cfg(test)]`) | display-free logic tested in its crate |

## Commands

```bash
cargo xtask check                    # maintained Rust fmt/clippy + xtask tests
cargo xtask test                     # tests in both Rust workspaces
cargo xtask fmt [--check]             # rustfmt on maintained Rust packages
cargo xtask rust check [crate]        # cargo check (both workspaces or named crates)
cargo xtask rust clippy [crate]       # cargo clippy -D warnings
cargo xtask rust test [crate]         # cargo test
cargo xtask iced run [--smoke] [--release]  # run the app; --smoke exits after first frame
cargo xtask iced hot                      # hot-reload dev run via cargo-hot (dev feature)
cargo xtask api generate <jellyfin|emby>    # optional OpenAPI regeneration; requires Java
cargo xtask api update <jellyfin|emby>      # optional schema update and regeneration
cargo xtask monitor --pid <pid> --out <target/path> # Linux raw resource sampler
cargo xtask hooks install                 # optional: activate repository .githooks
```

Crate short names include `auth`, `core`, `media-server`, `mpv`, `session`, `iced` (ui + app), and `xtask`.

## Conventions

- **Rust style**: 2-space indent (`rustfmt.toml`); workspace lints forbid `unsafe_code` and deny
  clippy warnings, with root `clippy.toml` disallowed APIs and `xtask` guardrail tests
  (`xtask/src/guardrails.rs`). Select focused or broader gates from `docs/agents/validation.md`;
  that policy is authoritative for verification scope, including the final pass.
- **Cargo entry points**: use `cargo xtask` for maintained build, format, lint, and test workflows.
  Cargo and native Rust xtask commands are permitted; ordinary builds do not regenerate API clients.
- **Display-free logic lives in `jellypilot-core`** and is tested there; `src-iced` keeps
  orchestration (Tasks, subscriptions, view). Follow ADR 0028's replace-don't-layer testing:
  adapters are tested through their external seam.
- **Domain language**: root `GLOSSARY.md` + `docs/adr/` — see [docs/agents/domain.md](docs/agents/domain.md).
  ADRs record accepted decisions; do not re-litigate them, amend them with new ADRs.
- **Styling**: the only styling system is `jellypilot-ui` tokens + Catalog styles (ADR 0027).
  Do not add CSS frameworks, per-widget ad-hoc colors outside tokens, or a second theme mechanism.
  Component patterns and accepted visual decisions live in `docs/design-system.md`; new UI work must
  follow it, and deviations from it require updating the doc in the same change.
- **Validation**: proportionate verification policy in
  [docs/agents/validation.md](docs/agents/validation.md); scale test scope to blast radius and
  never skip verification.

## Git & Command Safety

- **Git**: Never run `git stash`, `git reset`, or any destructive/broad git command. The only
  permitted git mutation is staging and committing specific named files in one step
  (`git add <files> && git commit`); no `git add -A`, `git add .`, or unstaged-sweeping commands.
- **Verification scope**: prefer the focused tier. Run broader checks or the native smoke gate
  when `docs/agents/validation.md` requires them for the requested change; this is part of
  authorized implementation. Release builds and unrelated full suites need task-specific justification.

## Anti-Patterns

- **Agent visual verification**: all visual verification is performed by humans, never by agents.
  Agents must not launch the app to "check how it looks", capture screenshots, toggle system
  settings (gsettings/color scheme), or inject input to drive the desktop. Agents verify code-level
  only — focused checks/lints, format, type checks, and the native smoke gate
  (`xvfb-run -a cargo xtask iced run --smoke`) — and report a concrete visual checklist (where to
  look, what should appear) for the user to review.
- **Treating the project as a web app**: this is cross-platform desktop software (Windows, macOS,
  Linux) drawn by iced; there is no DOM, no CSS, no browser target. Do not propose web tooling,
  webview shells, or browser-based verification.
- **Webview-era resurrection**: Tauri/Solid.js, the old embedded playback chain, ffmpeg sidecars,
  and specta bindings remain retired. The newly accepted embedded option is the pinned Linux
  Vulkan SDR mpv host, not the excluded GStreamer experiment or a revival of the retired stack.

## Agent Skills

- **Issue tracker**: GitHub Issues for `hewel/jmsr` — see [docs/agents/issue-tracker.md](docs/agents/issue-tracker.md)
- **Triage labels**: five-label vocabulary — see [docs/agents/triage-labels.md](docs/agents/triage-labels.md)
- **Domain docs**: root `GLOSSARY.md` + `docs/adr/` — see [docs/agents/domain.md](docs/agents/domain.md)
- **Feature map & statecharts**: surface → Glossary → module index and lifecycle statechart rule — see [docs/agents/feature-map.md](docs/agents/feature-map.md)

## Docs

- iced 0.14: https://docs.rs/iced/0.14.0
- tray-icon: https://docs.rs/tray-icon
- keyring: https://docs.rs/keyring
- Jellyfin API: https://api.jellyfin.org
