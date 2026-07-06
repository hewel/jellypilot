# 0010. vanilla-extract typed utilities replace Tailwind over time

JellyPilot is moving from Tailwind-authored utility strings to a typed vanilla-extract styling layer built from theme tokens, Sprinkles, Recipes, and component-local CSS. UnoCSS preset-mini is allowed only as a build-time reference for stable token scales; JellyPilot does not run the UnoCSS engine, does not install an UnoCSS bundler plugin, and does not parse utility class strings at runtime or build time.

## Status

Accepted

## Context

ADR 0008 made vanilla-extract the owner of design tokens while Tailwind remained the utility alias layer. That solved token ownership, but left component styling dependent on stringly utility classes, arbitrary values, and Tailwind-specific variants that are difficult to type, audit, or gradually constrain.

## Decision

- `src/styles/vars.css.ts` remains the token owner and now includes scale tokens for spacing, type, radius, shadows, z-index, duration, easing, and breakpoints.
- `src/styles/sprinkles.css.ts` exposes a curated typed utility subset for layout, flex/grid, spacing, typography, color, radius, shadow, and predictable conditions.
- Shared UI primitives use Recipes and component-local vanilla-extract styles for semantic variants and complex CSS.
- Static composition between Sprinkles and complex component CSS uses vanilla-extract `style([className, styleObject])`; manual `.join(' ')` class composition is not part of the styling convention.
- Tailwind stays temporarily for unmigrated page and feature callsites, then can be removed after migration coverage is high enough.

## Consequences

- New reusable styling should prefer Sprinkles, Recipes, or component-local `style()` over Tailwind class strings.
- Arbitrary values are not a public utility feature. Current arbitrary Tailwind values must either become semantic tokens/Recipes or remain local component CSS during migration.
- UnoCSS remains a reference/input dependency only; adding UnoCSS runtime extraction, shortcuts, variants, icons, or rule matching requires a new decision.
