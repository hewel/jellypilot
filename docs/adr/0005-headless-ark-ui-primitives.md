# 0005. Adopt headless Ark UI primitives

## Status

Accepted

## Context

JellyPilot needs accessible, predictable interactive controls for login, settings, diagnostics, dialogs, playback controls, and ordered language preferences. The app already owns a dark Control Room visual system through Panda CSS and `docs/design-system.md`; replacing it with a styled component kit would blur the product identity and destabilize existing component APIs.

## Decision

Use `@ark-ui/solid` as the headless primitive layer for Solid controls. Ark UI owns state-machine behavior, ARIA wiring, focus management, and `data-scope` / `data-part` styling hooks. JellyPilot keeps ownership of layout, Panda tokens and classes, domain behavior, persistence, and command timing.

TanStack Form remains the value and validation owner for forms. Ark Field only supplies accessible field structure.

## Consequences

- Interactive primitives can be adopted without importing a generic visual theme.
- Owner-local Panda classes target Ark data attributes while preserving Control Room styling.
- Tests should assert user behavior first, with small data-scope/data-part smoke checks for adopted Ark families.
- Hand-rolled primitives should not be added where an adopted Ark primitive already fits the control semantics.
