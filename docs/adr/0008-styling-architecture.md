# 0008. Styling architecture: vanilla-extract tokens, utility bridge, UI components

## Status

Superseded by ADR 0010

## Context

The styling layer had accreted three overlapping mechanisms: an atomic utility bridge, a large set of reusable global helper classes in `src/index.css` (typography, cards, inputs, console layout, Ark primitive styling, brand glow, animations), and vanilla-extract styles used only by `Button.css.ts`. Design token values lived in global CSS entries, while `src/styles/vars.css.ts` defined only an inert `createGlobalThemeContract` that assigned no values. The result was competing sources of truth for tokens, global class APIs that were imported across the codebase as if they were a component library, and component-owned CSS that other files reached into directly (e.g. importing `Button.css` style exports from a sibling component).

## Decision

- **vanilla-extract owns design tokens.** `src/styles/vars.css.ts` is the single source of token values, using `createGlobalThemeContract` + `createGlobalTheme(':root', ...)` with `--jellypilot-*` CSS variable names. It is imported for its side effect in `src/index.tsx`.
- **Historical utility bridge.** This ADR originally kept a utility alias layer while token ownership moved to vanilla-extract. ADR 0010 removed that bridge after page and feature callsites migrated to Sprinkles, Recipes, and component-local `.css.ts` files.
- **Atomic styling now flows through typed vanilla-extract APIs.** Typography, spacing, colors, responsive rules, and stateful component styling should use Sprinkles, Recipes, global styles, or component-local CSS.
- **Reusable visual patterns are components under `src/components/ui`**, not global `@layer` class APIs: `Button`, `Card`/`CardLink`, `FieldControl`/`FieldTextarea`/`TextField`, `ConsoleShell`/`ConsoleContainer`/`ConsoleGrid`, `SectionCard`, `JellyPilotSelect`, `StatusBadge`.
- **Component-local vanilla-extract CSS owns component styling** that is not a stable Sprinkles utility or Recipe variant.
- **No global component class APIs.** `src/index.css` keeps base global CSS such as `body`, shared keyframes, and scrollbar pseudo-element rules.

## Consequences

- Token values have one owner; changing a color updates Sprinkles, Recipes, global styles, and component-local CSS through `vars`.
- Adding a reusable styled element means adding or extending a `src/components/ui` component, not a global class. Future agents must not reintroduce `@layer components` helper classes.
- Components must not import another component's `.css.ts` style exports; consume the component API or move the behavior into a shared component.
- Callers should prefer explicit component APIs or local vanilla-extract classes over cascade-order overrides.
