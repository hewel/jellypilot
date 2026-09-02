# Validation Agent Policy

Scale verification to the change's blast radius. Verification is never optional — the tier is the
variable, not the existence. Running the widest gate for every edit wastes minutes; running nothing
ships regressions.

## Tiers

1. **Focused** — the default for localized edits. Run the narrowest gate that exercises the changed
   contract: `bun run task rust test <crate>` for the touched crate, `bun run task rust clippy
   <crate>`, plus `bun run task fmt` for touched script files. Use when the change is confined to
   one crate and alters no cross-crate contract.
2. **Suite** — for contract or cross-crate changes: focused gates first, then `bun run check`
   (fmt + lint + scripts typecheck + workspace clippy) and `bun run task rust test` (workspace).
   Use when public types, crate interfaces, or more than one crate change.
3. **Native smoke** — for desktop-boundary crossings: add
   `xvfb-run -a bun run task iced run --smoke` (builds the app, renders one frame, exits) when
   acceptance crosses application startup, window/shell wiring, subscriptions, tray behavior, or
   configuration persistence. The smoke gate proves startup, not appearance — visual acceptance
   remains human (see root AGENTS.md).

## Diagnosing Smoke and Playback Failures

When the smoke gate or MPV playback fails, follow this route instead of ad-hoc spelunking:

- **App logging**: `JELLYPILOT_LOG` (tracing EnvFilter syntax) controls app diagnostics; default
  `warn`, output goes to **stderr** (`src-iced/src/main.rs`). Typical values: `error`, `warn`,
  `info`, `debug`, `trace`, or module-scoped `jellypilot_iced=debug`. Re-run a failing gate as
  `JELLYPILOT_LOG=debug bun run task iced run --smoke`.
- **Stream anatomy**: the dispatcher pipes child output through, so cargo compile lines come first
  and app tracing starts after cargo's `Running …` line. Compile errors belong to cargo (they name
  the crate and file); failures after `Running` belong to the app.
- **Failure segment**: a failing smoke gate ends with an `=== iced smoke [FAILED] ===` segment
  (command, exit code, hint). Absence of that segment means the failure happened before the app
  started — inspect the cargo/tooling output above it.
- **Playback**: MPV runs as an external process over JSON IPC. A missing binary surfaces as the
  named error `MPV executable not found`; MPV's own diagnostics go to its stderr. App-side IPC
  errors are visible at `JELLYPILOT_LOG=debug`.

## Rules

- Do not re-run suites the change cannot affect: no workspace clippy for a `scripts/**`-only edit,
  no smoke gate for display-free logic in `jellypilot-core`.
- Prefer cached incremental reruns. A focused `bun run task rust test <crate>` after a warm build is
  seconds; reserve full gates for the final pass of multi-step work, not for every intermediate
  edit.
- A failing, timed-out, or cancelled verification run is not a pass. Re-run the focused tier to
  green before yielding.
- When a change spans tiers, verify each completed step at its own tier and the finished work at
  the highest applicable tier exactly once.
- If the user interrupts a verification run as excessive, drop to the lowest tier that still
  exercises the changed contract and state what was and was not verified.
