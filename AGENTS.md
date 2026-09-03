# AGENTS.md

Cross-platform Jellyfin/Emby companion app built with iced 0.14 (ADR 0027). Fully custom-drawn
native UI; playback is always External MPV Playback controlled through JSON IPC — no embedded
playback, no libmpv, no webview, no Tauri.

## Role Separation

**scoped-implementer**

* Only implements the requested changes.
* Does not review or approve its own code.

**reviewer**

* Only finds bugs and explains why the code may fail.
* Does not edit code, write fixes, or implement changes.

## Stack

- **Application**: Rust + iced 0.14 in `src-iced` (package `jellypilot-iced`, binary `jellypilot`)
- **Design system**: `crates/jellypilot-ui` — tokens, theme/Catalog styles, custom widgets, overlay
- **Domain crates**: `jellypilot-core` (display-free browse/config/diagnostics),
  `jellypilot-media-server` (Jellyfin/Emby HTTP + artwork), `jellypilot-auth` (login + keyring),
  `jellypilot-mpv` (process + JSON IPC), `jellypilot-session` (WebSocket remote sessions)
- **Generated API clients**: `crates/media-server-api/{jellyfin,emby}` (OpenAPI, regenerate via
  `bun run task api`)
- **Tooling**: Bun exists only for the `scripts/task.ts` dispatcher; Oxc formats/lints `scripts/**`;
  there is no JavaScript frontend

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
| Task dispatcher | `scripts/task.ts` + `scripts/task/` | Effect-based CLI; the only permitted cargo entry |
| Add test | beside the code (`#[cfg(test)]`) | display-free logic tested in its crate |

## Commands

```bash
bun run check                    # oxfmt/oxlint + scripts typecheck/tests + cargo fmt/clippy (workspace)
bun run task fmt [--check]       # oxfmt on package.json + scripts/**
bun run task lint [--fix]        # oxlint on scripts/**
bun run task typecheck           # tsc on scripts/**
bun run task rust check [crate]  # cargo check (workspace or named crates)
bun run task rust clippy [crate] # cargo clippy -D warnings
bun run task rust test [crate]   # cargo test
bun run task iced run [--smoke] [--release]  # run the app; --smoke exits after first frame
bun run task iced hot                      # hot-reload dev run via cargo-hot (dev feature)
bun run task api                 # regenerate OpenAPI clients
bun run task monitor --pid <pid> --out <target/path> # Linux raw resource sampler for a running process
```

Crate short names: `auth`, `core`, `media-server`, `mpv`, `session`, `iced` (ui + app).

## Conventions

- **Rust style**: 2-space indent (`rustfmt.toml`); workspace lints forbid `unsafe_code` and deny
  clippy warnings. Run focused gates (`bun run task rust clippy iced`) while iterating and
  `bun run check` once at the end.
- **No cargo directly**: use the `scripts/task.ts` dispatcher (`bun run task rust …`); never invoke
  `cargo` by hand except inside hooks that already do.
- **Display-free logic lives in `jellypilot-core`** and is tested there; `src-iced` keeps
  orchestration (Tasks, subscriptions, view). Follow ADR 0028's replace-don't-layer testing:
  adapters are tested through their external seam.
- **Domain language**: root `CONTEXT.md` + `docs/adr/` — see [docs/agents/domain.md](docs/agents/domain.md).
  ADRs record accepted decisions; do not re-litigate them, amend them with new ADRs.
- **Styling**: the only styling system is `jellypilot-ui` tokens + Catalog styles (ADR 0027).
  Do not add CSS frameworks, per-widget ad-hoc colors outside tokens, or a second theme mechanism.
- **Validation**: proportionate verification policy in
  [docs/agents/validation.md](docs/agents/validation.md); scale test scope to blast radius and
  never skip verification.

## Git & Command Safety

- **Git**: Never run `git stash`, `git reset`, or any destructive/broad git command. The only
  permitted git mutation is staging and committing specific named files in one step
  (`git add <files> && git commit`); no `git add -A`, `git add .`, or unstaged-sweeping commands.
- **No slow commands**: prefer fast, focused commands (clippy on touched crates, filtered
  `cargo test`). Avoid full builds and long-running verification unless the user explicitly asks.

## Anti-Patterns

- **Agent visual verification**: all visual verification is performed by humans, never by agents.
  Agents must not launch the app to "check how it looks", capture screenshots, toggle system
  settings (gsettings/color scheme), or inject input to drive the desktop. Agents verify code-level
  only — focused checks/lints, format, type checks, and the native smoke gate
  (`xvfb-run -a bun run task iced run --smoke`) — and report a concrete visual checklist (where to
  look, what should appear) for the user to review.
- **Treating the project as a web app**: this is cross-platform desktop software (Windows, macOS,
  Linux) drawn by iced; there is no DOM, no CSS, no browser target. Do not propose web tooling,
  webview shells, or browser-based verification.
- **Webview-era resurrection**: the Tauri/Solid.js stack, embedded playback chain, ffmpeg sidecars,
  and specta bindings were deleted (ADR 0027 retirement). Do not reintroduce them; playback is
  External MPV Playback only.

## Agent Skills

- **Issue tracker**: GitHub Issues for `hewel/jmsr` — see [docs/agents/issue-tracker.md](docs/agents/issue-tracker.md)
- **Triage labels**: five-label vocabulary — see [docs/agents/triage-labels.md](docs/agents/triage-labels.md)
- **Domain docs**: root `CONTEXT.md` + `docs/adr/` — see [docs/agents/domain.md](docs/agents/domain.md)

## Docs

- iced 0.14: https://docs.rs/iced/0.14.0
- tray-icon: https://docs.rs/tray-icon
- keyring: https://docs.rs/keyring
- Jellyfin API: https://api.jellyfin.org
