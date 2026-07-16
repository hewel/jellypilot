# 0011. Panda CSS is the styling system

JellyPilot migrates from vanilla-extract (tokens, Sprinkles, Recipes, component-local `.css.ts`) to Panda CSS with owner-local `.styles.ts` modules, semantic tokens, and Solid-native class composition.

## Status

Accepted

## Context

ADR 0010 established a typed vanilla-extract layer after removing the utility-CSS bridge. That layer solved token ownership and type safety, but still left a dual surface of Sprinkles, Recipes, and component CSS, plus a build-time Uno scale input. Wayfinder research and prototype work confirmed Panda CSS 1.11.4 fits Rsbuild/PostCSS, Solid, Rstest, and Tauri without a second runtime.

## Decision

- Panda owns design-token literals, true application globals, and shared keyframes.
- Application styles use owner-local `Component.styles.ts` / route `Route.styles.ts` modules that import `@styled-system/*`.
- Generated artifacts live in ignored root `styled-system/` and are regenerated via `panda:codegen` (and consumer prehooks / `prepare`).
- During staged coexistence, `.css.ts` remains vanilla-extract-only and `.styles.ts` is Panda-only. No DOM element receives classes from both engines.
- The legacy `--jellypilot-*` variable surface is a temporary alias bridge to Panda variables until final cutover.
- ADR 0010 is superseded by this decision for active styling guidance. Historical ADRs remain on disk as superseded history.

## Consequences

- New styling must use Panda tokens, `css` / colocated `cva` / `sva`, and Solid `class` / `classList`.
- Cross-component private style imports remain forbidden; shared visuals become owned components or patterns.
- Final cutover removes vanilla-extract packages, Sprinkles, Recipes, the variable bridge, and every `.css.ts` module.
