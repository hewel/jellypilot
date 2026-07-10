# 0013. Theme Preference, Resolved Theme, and build-time Theme Presets

## Status

Accepted

## Context

JellyPilot’s visual system was dark-only Control Room. Product requirements need System/Light/Dark preference, independent Neutral and JellyPilot Theme Presets, and first-paint theme resolution without routed content flashing the wrong mode.

## Decision

- Theme Preference is the durable config value `system | light | dark`, default `system`, owned by App Composition configuration.
- Resolved Theme is the concrete `light | dark` mode applied to the document root (for example `data-theme`) after resolving System against OS preference.
- Theme Presets are complete build-time descriptors. Neutral uses system fonts; JellyPilot owns Figtree. Nested Theme scopes propagate active descriptor and Resolved Theme through Layers/portals.
- App Composition coordinates startup load, optimistic desired state, serialized saves, latest-intent-wins coalescing, and confirmed-state rollback for appearance.
- Do not mount routed content before configuration loads; on failure show a stable System-themed retry surface.
- Preserve Astryx base CSS variable names exactly in UI Core presets; map the shared semantic Project Theme into Atomic Schema without legacy aliases after cutover.

## Consequences

- ADR 0003 dark-only Control Room decision is superseded for theme mode and token ownership direction.
- Appearance controls live in Settings plus a compact shell cycle control in the authenticated shell.
- Theme tests cover preference persistence, system resolution, document application, and rollback — not private CSS implementation details.
