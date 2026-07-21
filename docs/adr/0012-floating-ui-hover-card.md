# 0012. Hand-rolled floating-ui hover-card

## Status

Accepted

## Context

ADR 0005 adopts `@ark-ui/solid` primitives and states hand-rolled primitives should not be added where an Ark primitive fits. The Ark hover-card (a zag-js state machine) misbehaves when its trigger lives inside the `@tanstack/solid-virtual` grid on the library browse route: rows are absolutely positioned, recycled, and unmounted during scroll, leaving the card anchored to stale or detached trigger elements. The zag hover-card machine does not expose the hooks needed to correct anchor lifetime from the outside.

## Decision

Replace `@ark-ui/solid/hover-card` with a hand-rolled primitive in `src/components/ui/HoverCard.tsx`, built directly on `@floating-ui/dom` (`computePosition`, `offset`, `flip`, `shift`). This is a scoped exception to ADR 0005, not a repeal: other Ark primitives remain the default.

The primitive owns its interaction contract: opens on hover after a 250ms delay or immediately on keyboard focus, closes after a 300ms grace once neither pointer nor focus is inside, keeps open while the pointer is over the card content, and closes immediately on Escape, on any document scroll (capture-phase listener, which covers the app scroll area), and on trigger unmount. Content mounts through a `Portal` only while open so consumers can gate data fetching on `onOpenChange`.

## Consequences

- JellyPilot owns hover/focus intent timing and dismiss behavior that zag previously owned; changes to those semantics are local edits, not upstream state-machine options.
- There is no position tracking during scroll — the card closes instead, which also sidesteps recycled-row anchoring entirely.
- `@floating-ui/dom` becomes a direct dependency (it was already transitive via `@zag-js/popper`).
- Future hover-card consumers reuse `src/components/ui/HoverCard.tsx`; do not reintroduce `@ark-ui/solid/hover-card` for the virtualized grid case.
