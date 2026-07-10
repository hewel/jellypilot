# 0012. Publishable Atomic CSS compiler for typed utilities

## Status

Accepted

## Context

The curated Sprinkles surface cannot faithfully represent preset-derived utility semantics, static arbitrary values, shared-variable transform families, or explicit conflict metadata. Parallel app and UI Core utility layers would create duplicate token ownership and an inconsistent atomic cutover. PRD #132 specifies one low-level styling engine shared by UI Core and App Composition while vanilla-extract remains the only CSS emitter.

## Decision

- Add publishable ESM package `@jellypilot/atomic-css` (versioned independently; first release `0.1.0`, Node `>=20.17`, MIT).
- Public surface is only: package root (`atomic()` marker), `preset-mini` subpath, Rsbuild subpath (`pluginAtomic()`), and `atomic-css` CLI. Schema internals, compiler, registry, emitter, and diagnostics stay private.
- Pin Source Preset adapter to `@unocss/preset-mini` `66.7.4`. Do not claim full preset-mini parity or a public adapter SDK.
- Normalize supported semantics into a versioned Atomic Schema; merge consumer Project Theme tokens and conditions through one workspace configuration seam.
- `atomic()` is a compile-time marker. Uncompiled runtime throws exactly: `atomic() must be compiled by pluginAtomic()`.
- Fail closed for unsupported or dynamic behavior: no runtime class generation, no arbitrary selectors in v1, no silent host version drift.
- Pin exact Rsbuild, Rspack, vanilla-extract Webpack plugin, and vanilla-extract CSS host versions; resolved-version drift fails with a specified diagnostic.
- Emit cascade layers `atomic.preflight` then `atomic.utilities`. Semantic component-local vanilla-extract declarations remain unlayered and override utilities.
- Real-file transport is the deterministic baseline. Virtual transport is released only when cold-build and watch fixtures match that baseline.
- UI Core remains the semantic Theme Preset and Recipe owner. Atomic CSS owns only low-level preset normalization and static utility generation.
- Legacy `src/styles/vars.css.ts` / Sprinkles remain migration inputs until contraction removes them after every consumer migrates.

## Consequences

- UI Core foundation tickets consume Atomic Schema instead of inventing a second Sprinkles owner.
- ADR 0010 remains the historical migration record for typed utilities; this ADR is the target architecture. Full supersession of the Sprinkles/Uno restriction completes at the contraction ticket (#127), not at package introduction.
- Exact CSS/module assertions live in atomic compiler fixtures; component and app tests assert behavior and documented selectors, not generated class names.
