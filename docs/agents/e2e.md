# Native E2E Agent Policy

JellyPilot's maintained native harness is Linux-only and drives the compiled debug Tauri
application through the embedded WebDriver provider. It is an agent verification boundary, not a
user-facing runbook.

## When to cross the desktop boundary

Run a focused permanent native spec when acceptance depends on Tauri startup, a real or controlled
IPC call, routing after the application mounts, native WebView interaction, or persistence resolved
through sandboxed Tauri paths. Isolated Rust helpers, browser-only behavior, and styling changes use
their cheaper focused checks unless their acceptance criterion explicitly crosses that boundary.

## Command contract

- `bun run build:e2e` validates Bun 1.3.14, Node 22.11+, Rust 1.85+, builds the WebDriver-only
  frontend and debug Tauri binary, and writes a hash manifest under `.artifacts/e2e/build/`.
- `bun run test:e2e` reuses that build and refuses missing, changed, or mismatched output. It never
  rebuilds. Use `--spec e2e/specs/<name>.e2e.ts` for one permanent spec and `--headed` to use the
  current display while retaining all storage/process isolation.
- `bun run typecheck:e2e` checks the harness and permanent specs independently.
- `bun run e2e` runs typecheck, build, then all permanent native specs.
- `bun run check:e2e-isolation` builds the normal optimized unbundled application in a separate
  target and proves that normal config, dependency graph, frontend, and binary exclude automation.
- `bun run e2e:verify` runs the complete native path followed by production isolation.
- `bun run e2e:clean` removes only the sentinel-owned `.artifacts/e2e/` tree.

Build and isolation operations have a 15-minute cap. Each spec, including teardown, has a five-minute
cap. Specs run one at a time, each in a new application process, embedded-driver port, HOME, TMPDIR,
XDG root set, WebView state, and private Xvfb display. Automatic session and spec retries are off.

## Permanent spec rules

Permanent specs live in `e2e/specs/`, have behavior names, and are discovered by default. Keep one
scenario per fresh application lifetime. Select UI through accessible names, labels, semantic roles,
and visible text; do not add production `data-testid` attributes or selector-only markup.

The application installs controlled IPC fixtures before its one-shot Solid mount. Fixture successes
return raw Rust wire payloads and fixture failures reject with raw `CommandError` values so generated
bindings keep ownership of their typed result conversion. Unknown commands fail closed.

Real IPC is a deliberate safety boundary. `SafeRealCommand` currently contains only
`config_default`. Expanding it requires reviewing the command for external network, credential,
playback, filesystem, and persistent-state effects. Never bypass the controlled invoke interface or
edit generated bindings for a test.

## Failure evidence and ownership

Passing sandboxes are removed after process, port, display, and storage teardown checks. A failure can
retain only a scrubbed bundle under `.artifacts/e2e/failures/`: summary, exact tool versions, build
fingerprint, screenshot when a session exists, frontend/backend/runner logs, fixture summary, and
teardown diagnostics. Binaries, full sandboxes, environment dumps, and page source are excluded.

Evidence is redacted and scanned before an atomic move. If credential-shaped material remains, no
bundle is retained. Only the five newest sentinel-owned failure bundles survive. Cleanup and pruning
must never inspect, mutate, or remove normal JellyPilot storage or unrelated `.artifacts/` content.
Harness maintainers may run `e2e/fixtures/controlled-failure.e2e.ts` explicitly to exercise this path;
the fixture is excluded from default discovery and is expected to return nonzero.
