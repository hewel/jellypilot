# JellyPilot Design System

JellyPilot uses a desktop-first media companion design system with first-class Light, Dark, and System themes. The interface should feel deliberate, quiet, and reliable for browsing media, configuring playback, and operating MPV. Astryx is a token and visual reference only: JellyPilot does not install Astryx packages, StyleX, React UI, or another headless UI runtime.

Use the typed vanilla-extract styling layer for shared UI and application surfaces: design tokens in `src/styles/vars.css.ts`, atomic utilities from `src/styles/sprinkles.css.ts`, semantic component variants via Recipes, global styles, and component-local `.css.ts` files for complex CSS. Compose static vanilla-extract classes with `style([sprinkles(...), { ... }])` rather than manual `.join(' ')` strings; use Solid `classList` only for runtime conditional classes in `.tsx`. The former utility-CSS bridge is not installed. Reusable visual patterns are components under `src/components/ui`, not global `@layer` class APIs.

## Principles

- **Desktop-first media control**: optimize for the default 800×600 Tauri window while remaining usable down to 360px width without horizontal scroll.
- **Solid surfaces first**: default surfaces are calm, readable, and theme-aware. Decorative glass, radial gradients, and glow are not part of the default visual language.
- **Operational clarity**: every status uses text and icon, not color alone.
- **No fake state**: never show fake media artwork, fake playback progress, or pretend controls.
- **Accessible by default**: normal text contrast must be at least 4.5:1. Large text and meaningful icons must be at least 3:1.

## Token Contract

All components use semantic tokens. The source of truth is the vanilla-extract contract in `src/styles/vars.css.ts`; Sprinkles, Recipes, and component-local CSS consume that contract directly. `:root` defines the light theme and `[data-theme='dark']` defines the dark theme. System mode resolves to one of those two document states.

The token contract is organized around these groups:

| Group | Usage |
|---|---|
| `background` | App, page, panel, raised, inset, overlay, and scrim surfaces. |
| `text` | Primary, secondary, muted, inverse, accent, and status text. |
| `icon` | Icon colors that track text and status semantics. |
| `border` | Subtle, default, strong, focus, and status borders. |
| `accent` | Primary and secondary interactive fills, hover/pressed states, subtle fills, and borders. |
| `status` | Success, warning, danger, and info backgrounds, text, and borders. |
| `data` | Distinct colors for charts, counts, and dense comparison UI. |
| `space` | Shared spacing scale. |
| `size` | Control, icon, sidebar, and drawer dimensions. |
| `radius` | Semantic radius scale. |
| `shadow` | Elevation shadows without glow-as-state. |
| `motion` | Duration and easing tokens. |
| `font` | Figtree body/heading family plus monospace fallback. |
| `typeScale` | Named size and line-height values for display, headline, title, body, and label text. |

The legacy `vars.color.*`, `fontSize`, `lineHeight`, `borderRadius`, `duration`, and `easing` aliases remain temporarily so existing components compile during the migration. New styling should prefer the semantic groups above.

## Color Semantics

- Indigo means JellyPilot identity, primary app action, local control, and active media-server/session connection.
- Teal/green means generic healthy or ready.
- Amber means degraded, waiting for recovery, or retryable warning.
- Red means failure or destructive action.
- Quick Connect waiting is a normal state and should use indigo, not amber.

## Typography

Bundled local Fontsource variable fonts are used. No network font imports. Body text and headings use Figtree.

```css
body { font-family: 'Figtree Variable', ui-sans-serif, system-ui, sans-serif; }
h1, h2, h3, .brand-type { font-family: 'Figtree Variable', ui-sans-serif, system-ui, sans-serif; }
.code, .diagnostic-value { font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; }
```
Apply typography through Sprinkles, Recipes, or component-local vanilla-extract styles. The previous global helper classes are gone; use the tokenized values below.

| Style | Atoms | Default color |
|---|---|---|
| Display medium | `font-display text-[45px] leading-[52px] font-bold tracking-tight` | — |
| Display small | `font-display text-[36px] leading-[44px] font-bold tracking-tight` | — |
| Headline large | `font-display text-[32px] leading-[40px] font-bold tracking-tight` | — |
| Headline medium | `font-display text-[28px] leading-[36px] font-bold tracking-tight` | — |
| Headline small | `font-display text-[24px] leading-[32px] font-bold tracking-tight` | — |
| Title large | `text-[22px] leading-[28px] font-bold` | `text-on-surface` |
| Title medium | `text-[16px] leading-[24px] font-semibold` | `text-on-surface` |
| Title small | `text-[14px] leading-[20px] font-semibold` | `text-on-surface` |
| Body large | `text-[16px] leading-[24px]` | `text-on-surface-variant` |
| Body medium | `text-[14px] leading-[20px]` | `text-on-surface-variant` |
| Body small | `text-[12px] leading-[16px]` | `text-on-surface-variant/80` |
| Label large | `text-[14px] leading-[20px] font-semibold tracking-wide uppercase` | — |
| Label medium | `text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase` | `text-on-surface-variant` |
| Label small | `text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase` | `text-on-surface-variant/90` |

## Components

### Buttons

- Use the `<Button>` component (`src/components/ui/Button.tsx`) for `primary`, `secondary`, `tonal`, `outlined`, `text`, and `icon` variants. One primary action per section or state.
- For local primitive triggers that should look like a button, compose the primitive behavior with `<Button>` instead of reaching for helper classes.

### Inputs

- Use `<Field>` with `<FieldControl>` / `<FieldTextarea>` (`src/components/ui/FieldControl.tsx`) with `variant="filled"` (Login and configuration) or `variant="outlined"` (compact selectors). Field labels, descriptions, errors, and control IDs are wired by the local primitive.
- `<TextField>` (`src/components/ui/TextField.tsx`) wraps `FieldControl` with label, error, and hint for plain forms.
- Every field has a visible label. Errors appear near the field.

### Cards and Surfaces

- Use `<Card>` / `<CardLink>` (`src/components/ui/Card.tsx`) with `variant="filled"` (default solid panel), `"elevated"` (hero/emphasized), or `"outlined"` (extra separation). `<CardLink>` is the card-styled anchor for navigational cards.
- Hero surfaces should use real media, solid panels, and tokenized scrims. Diagnostics and dense text remain solid.

### Layout helpers

- `<ConsoleShell>`, `<ConsoleContainer>`, and `<ConsoleGrid>` (`src/components/ui/ConsoleLayout.tsx`) compose the authenticated shell, centered content column, and two-column console grid.
- `<SectionCard>` wraps a `Card` with an icon + title header.
- `<JellyPilotSelect>` and `<StatusBadge>` are the select and status-pill components.

### Concentric Border Radius

When rounded elements are nested, the inner and outer curves must stay concentric so the spacing around the corner remains visually uniform.

Use this rule:

```text
inner radius = parent radius - padding
```

Examples:

| Scenario | Parent Radius | Padding / Gap | Inner Radius |
|---|---:|---:|---:|
| Outer card corner | `16px` | `4px` | `12px` |
| Tab transition | `12px` | `6px` | `6px` |
| Nested button | `16px` | `8px` | `8px` |

If padding is greater than or equal to the parent radius, use `0` for the inner radius. Do not use negative radius values.

### Text-Icon Badge Alignment

For compact badges, pills, and buttons that nest text with a trailing icon, derive horizontal padding and icon compensation from one vertical padding value. This keeps the control optically balanced across font-size and line-height changes.

Define only the vertical padding:

- `--py = 0.875em`

Derive horizontal padding from the text line box and cap height:

- `--px = --py + (1lh - 1cap) / 2`
- `padding-block = --py`
- `padding-inline = --px`

Size the icon to the line box and compensate the trailing edge:

- `icon width = 1lh`
- `icon height = 1lh`
- `icon trailing margin = --py - --px`

Use this pattern when the component needs text and icon edges to feel equally inset. Do not hardcode separate horizontal values unless the component has a deliberate asymmetric layout.

### Status Tiles

Each tile has icon, label, value, and supporting text. Status tiles are read-only unless explicitly styled as actions.

### Diagnostics

Diagnostics are a user-facing support view, not a developer console. Use normal cards with terminal-adjacent details: mono timestamps, level badges, compact rows, no fake terminal prompts or chrome.

## Typed Utility Layer

`src/styles/sprinkles.css.ts` is the only generic typed utility API. It intentionally covers a stable subset:

- Layout: display, position, overflow, width, height, min/max width, min height.
- Flex/Grid: direction, wrap, alignment, justification, grow/shrink, gap, row gap, column gap.
- Spacing: margin and padding longhands plus `p`, `px`, `py`, `pt`, `pr`, `pb`, `pl`, `m`, `mx`, `my`, `mt`, `mr`, `mb`, `ml`.
- Typography: font size, font weight, line height, text alignment.
- Visual: semantic color, background color, border color, border radius, box shadow, opacity, z-index.
- Conditions: `base`, `sm`, `md`, `lg`, `xl`, `2xl`, `hover`, `focus`, `active`, `disabled`, and `dark` via `[data-theme='dark'] &`. Prefer theme-scoped token values over duplicating light/dark CSS branches.

Do not make Sprinkles parse class strings. Do not add arbitrary values as a public utility feature. Temporary `legacy*` values in Sprinkles exist only to bridge current application needs and should shrink as repeated patterns become semantic tokens or Recipes. Complex selectors, primitive `data-*` state styling, gradients, transforms, filters, animations, and one-off layout math belong in Recipes or component-local `style()` blocks.

When a component-local class needs both Sprinkles and complex CSS, use vanilla-extract style composition:

```ts
export const panel = style([
  sprinkles({ display: 'flex', gap: '4' }),
  { backdropFilter: 'blur(12px)' },
]);
```

Do not manually join generated classes in `.css.ts` files. In Solid `.tsx`, use `classList` for runtime conditional class toggles.

UnoCSS preset-mini is a build-time reference for token scale design only. JellyPilot does not use the UnoCSS runtime engine, extractor, shortcuts, variants, icon presets, or class-to-CSS matcher.

## Layout

- `<640px`: single-column compact layout.
- `640–1023px`: two-column status grid where space allows.
- `≥1024px`: wider console grid and expanded hero/status composition.
- Use CSS grid for macro layout and flex/stack inside components.
- No sticky viewport action bars. Keep actions contextual.

## Motion

- 150–220ms transitions for hover, focus, and state changes.
- Animate opacity and transform only; do not animate layout dimensions.
- Avoid decorative looping animation except subtle indeterminate waiting indicators.
- Respect `prefers-reduced-motion`.

## Icons

Use Lucide Solid only for structural icons. Do not use emoji as icons. Standard visual sizes are 16, 20, and 24px, but interactive targets must remain comfortable and keyboard-accessible.

## Out of Scope

- UI sounds or haptics.
- Raw URL playback controls.
- Fake artwork or fake playback state.
