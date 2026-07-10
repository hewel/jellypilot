# 0011. UI Core is a private source-workspace package

## Status

Accepted

## Context

JellyPilot currently owns application-local Solid primitives, design tokens, and route-specific interaction implementations. That duplicates accessibility work and leaves no independently verifiable UI contract. PRD #103 requires a coherent Solid component library adapted from Astryx while remaining package-neutral and desktop-WebView portable.

## Decision

- Add private workspace package `@jellypilot/ui` (UI Core) compiled from source by consumers; no independent release tooling.
- Keep JellyPilot as the root App Composition application; UI Core and its catalog are workspace members sharing one lockfile.
- Pin Astryx Baseline to v0.1.4 commit `7b76c68ae07cbc798e3b7f4125eb99ec596f0779` under MIT provenance. Selected families are adapted to Solid; React APIs, StyleX, and pixel parity are out of scope.
- Allow UI Core dependencies only on Solid, vanilla-extract, `@jellypilot/atomic-css`, Floating UI positioning, and preset font assets.
- Forbid Tauri, Effect, TanStack, Jellyfin/Emby, application composition, and public icon-registry dependencies in UI Core.
- UIRoot owns system-mode resolution, the topmost Layer registry, portal host, theme-aware portals, and toast viewport. Exactly one UIRoot per owner document.
- Theme authoring is build-time only: complete Neutral and JellyPilot Theme Presets produce opaque runtime descriptors.
- Stable public selectors are documented `data-*` attributes; generated classes and DOM nesting remain private.
- Invalid public configurations throw typed `UIInvariantError` in all builds.
- Adopt UI Core atomically in App Composition without compatibility reexports or legacy token aliases.

## Consequences

- Component, keyboard, and accessibility work lands in UI Core; domain workflows stay in App Composition.
- Catalog and package tests import public entrypoints only.
- ADR 0005 (Ark UI primitives) is superseded for new work; remaining Ark usage is migration debt until contraction.
- ADR 0003 dark-only Control Room identity is superseded by Theme Preference / Resolved Theme and Theme Presets.
