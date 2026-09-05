# Validation Agent Policy

This document is the authority for verification scope, including the final pass. Select checks
that exercise the changed contract. Existing authorization for implementation includes these
checks; do not add an approval pause solely because the applicable tier is broader.

## Tiers

1. **Focused** — the default for localized edits. For Rust behavior, use tests in the touched
   crate (`bun run task rust test <crate>`), focused clippy (`bun run task rust clippy <crate>`),
   and `bun run task rust fmt --check`. For scripts, use the relevant Bun tests plus
   `bun run task typecheck`, `bun run task lint`, and `bun run task fmt --check` as applicable.
   Documentation and instruction-only changes need link, command, metadata, and consistency
   checks; they do not require application builds or tests. Use this tier when no cross-crate
   contract changes.
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
- A failing, timed-out, or cancelled verification run is not a pass. Investigate the failure and
  rerun after a relevant fix or changed prerequisite. If an external prerequisite prevents
  verification, report the exact blocker and completed checks; do not retry unchanged failures indefinitely.
- When a change spans tiers, verify each completed step at its own tier and the finished work at
  the highest applicable tier. Reuse passing results for unchanged inputs; repeat or broaden
  checks only after relevant changes, failures, or unresolved concerns.
- If the user interrupts a verification run as excessive, drop to the lowest tier that still
  exercises the changed contract and state what was and was not verified.
