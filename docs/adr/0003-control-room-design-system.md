# 0003. Adopt JellyPilot Control Room design system

## Status

Superseded by ADR 0011 and ADR 0013

## Context

JellyPilot needed an owned media-control visual identity instead of a generic mobile Material feel.

## Decision (historical)

JellyPilot replaced Material Design 3 as the governing visual language with a desktop-first Control Room design system: dark-only, clean OLED surfaces with selective cinematic glass, indigo `#4f46e5` as the JellyPilot brand primary, and `#818cf8` as the Jellyfin/server secondary accent.

## Consequences

- Historical dark-only and glow/glass direction guided early UI work.
- Superseded for theme mode (System/Light/Dark), token ownership (UI Core Theme Presets), and primitive ownership (UI Core + Atomic CSS).
