# 0008. Styling architecture: vanilla-extract tokens, Tailwind utilities, UI components

## Status

Accepted

## Context

The styling layer had accreted three overlapping mechanisms: Tailwind atomic utilities, a large set of reusable `@layer components` helper classes in `src/index.css` (typography, cards, inputs, console layout, Ark primitive styling, brand glow, animations), and vanilla-extract styles used only by `Button.css.ts`. Design token values lived as literal `@theme` entries in `src/index.css`, while `src/styles/vars.css.ts` defined only an inert `createGlobalThemeContract` that assigned no values. The result was competing sources of truth for tokens, global class APIs that were imported across the codebase as if they were a component library, and component-owned CSS that other files reached into directly (e.g. importing `Button.css` style exports from a sibling component).

## Decision

- **vanilla-extract owns design tokens.** `src/styles/vars.css.ts` is the single source of token values, using `createGlobalThemeContract` + `createGlobalTheme(':root', ...)` with `--jmsr-*` CSS variable names. It is imported for its side effect in `src/index.tsx`.
- **Tailwind is the utility alias layer.** `src/index.css` uses `@theme inline` to alias `--color-*` / `--font-*` utility tokens to the `--jmsr-*` variables. No token values are defined in `src/index.css`.
- **Atomic styling is applied directly as Tailwind utilities** in components. Typography, brand gradients, glows, and animations that were global helper classes are now inline Tailwind atoms at their callsites (animations reference `@keyframes` kept in `src/index.css` via arbitrary animation utilities).
- **Reusable visual patterns are components under `src/components/ui`**, not global `@layer` class APIs: `Button`, `Card`/`CardLink`, `FieldControl`/`FieldTextarea`/`TextField`, `ConsoleShell`/`ConsoleContainer`/`ConsoleGrid`, `SectionCard`, `JmsrSelect`, `StatusBadge`.
- **Component-local vanilla-extract CSS is allowed only for non-atomic CSS** that Tailwind utilities cannot express cleanly (e.g. multi-stop card gradients and compound shadows in `Card.css.ts`). Atomic border/padding/radius classes stay on the component itself so callers can override them with Tailwind utilities.
- **No global `@layer components` class APIs.** `src/index.css` keeps only the Tailwind import, `@theme inline` aliases, `body`, `@keyframes`, and scrollbar pseudo-element rules.

## Consequences

- Token values have one owner; changing a color updates both Tailwind utilities and component-local CSS through `vars`.
- Adding a reusable styled element means adding or extending a `src/components/ui` component, not a global class. Future agents must not reintroduce `@layer components` helper classes.
- Components must not import another component's `.css.ts` style exports; consume the component API or move the behavior into a shared component.
- Callers override component defaults with Tailwind utilities; when overriding a property the component also sets (e.g. card padding), the caller must force precedence (Tailwind `!` modifier) because both are now utilities at the same cascade layer.
