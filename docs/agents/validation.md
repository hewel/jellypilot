# Validation Agent Policy

Scale verification to the change's blast radius. Verification is never optional — the tier is the
variable, not the existence. Running the widest gate for every edit wastes minutes; running nothing
ships regressions.

## Tiers

1. **Focused** — the default for localized edits. Run the narrowest test filter that exercises the
   changed contract: `cargo test --manifest-path src-tauri/Cargo.toml <module-or-name>` for Rust,
   `bun run test <path-or-name>` for frontend, plus formatting for touched files. Use when the
   change is confined to one module and alters no cross-module contract.
2. **Suite** — for contract or cross-module changes: focused tests first, then
   `bun run task test --all` and `bun run check`. Use when exported signatures, shared types,
   generated bindings, or more than one module change.
3. **Native** — for desktop-boundary crossings: add `bun run task e2e build` and the focused permanent
   spec per [e2e.md](e2e.md) when acceptance crosses Tauri startup, IPC, post-mount routing,
   desktop interaction, or sandboxed persistence.

## Rules

- Do not re-run suites the change cannot affect: no frontend suite for backend-only edits, no E2E
  for logic-only edits, no Rust suite for frontend-only edits.
- Prefer cached incremental reruns. A focused `cargo test <filter>` after a warm build is seconds;
  reserve full gates for the final pass of multi-step work, not for every intermediate edit.
- A failing, timed-out, or cancelled verification run is not a pass. Re-run the focused tier to
  green before yielding.
- When a change spans tiers, verify each completed step at its own tier and the finished work at
  the highest applicable tier exactly once.
- If the user interrupts a verification run as excessive, drop to the lowest tier that still
  exercises the changed contract and state what was and was not verified.
