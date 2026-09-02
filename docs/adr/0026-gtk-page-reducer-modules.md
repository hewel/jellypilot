# Split the GTK shell into page reducer modules

_Status: Accepted. Amends ADR 0024._

## Context

ADR 0024's rebuild plan says to replace the monolithic Relm4 shell with per-page
`relm4::Component` impls, one slice per view. That letter collides with the
convention already proven in-tree: `BrowseModel` and `PlaybackSession` are plain
reducer modules (state + input → effects) behind the single Relm4 component.
Per-page components would add sender/output machinery and a second command
runtime without making display-free policy easier to test.

Pages remain part of the GTK shell adapter (ADR 0021). Widget code is not
display-free; the page's pure policy stays in testable free functions.

## Decision

Continue ADR 0024's per-page extraction, but extract pages as reducer modules
rather than `relm4::Component` impls. Each page owns its state, widgets, and
message/event enums, and exposes `handle` / `handle_event` → `Vec<Effect>`. The
shell keeps the one Relm4 component, dispatches messages, executes effects, and
retains shared auth state (`connection`, saved profiles, the active profile),
`RequestGate`, Disconnect, and post-login bootstrap.

Login is the first slice and the template for Home, Browse, Detail, Settings,
and Diagnostics. Those pages land only after this template is reviewed.

## Consequences

- Display-free policy stays synchronously testable without a display server or
  per-page Relm4 lifecycle.
- Cross-page concerns stay in the shell; pages do not borrow the whole
  `AppModel`. Effects are the only cross-subsystem seam.
- ADR 0024's runtime-parity goal is unchanged; only the component shape is
  amended. Of its eight slices, the bottom bar, shared session crate, and
  Intro Skipper have already landed; what remains is extracting the six views
  (Login, then Home, Browse, Detail, Settings, Diagnostics) from the monolith.
