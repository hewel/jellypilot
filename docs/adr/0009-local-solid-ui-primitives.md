# 0009. Adopt local Solid UI primitives

## Status

Accepted

## Context

JellyPilot originally adopted headless Ark UI primitives to migrate controls without taking on a visual component kit. After the design-system reset, the app had enough local button, field, dialog, select, and styling infrastructure to keep the interactive surface in-repo and remove the broad Ark/Zag transitive dependency set.

## Decision

Use local Solid primitives in `src/components/ui` for reusable controls and route/component-local primitives for narrow one-off controls. These primitives own the DOM structure, ARIA attributes, keyboard or pointer behavior needed by the app, and state data attributes consumed by vanilla-extract styles.

TanStack Form remains the owner for form values and validation. Component-local CSS remains the owner for visual presentation.

## Consequences

- The frontend no longer depends on the Ark UI Solid package or the Zag state-machine packages.
- Tests assert user-facing roles, labels, checked/expanded/selected state, and absence of legacy Ark scope markers where useful.
- Shared primitives should stay small and headless. Add a reusable primitive only when at least two call sites need the same behavior or when the behavior is risky enough to centralize.
- Component-owned styles stay beside the component; shared UI primitives expose class hooks rather than importing another component's `.css.ts` file.
