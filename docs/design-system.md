# JellyPilot Control Room Design System

JellyPilot uses a desktop-first Control Room design system: dark-only, clean OLED surfaces, selective cinematic glass, and clear operational state. The interface should feel like a reliable media companion for a Jellyfin Playback Target, not a generic mobile settings app.

Panda CSS is the sole styling system ([ADR 0011](adr/0011-panda-css-styling.md)). Application styles live in owner-local `.styles.ts` modules and use semantic Panda tokens.

**Viewports:** Supported production Tauri sizes are **1280×720** (minimum) and **1600×900** (default). **800×600**, **640×720**, and **360×720** are responsive stress targets exercised with the review-only Tauri config, not production window sizes.

Reusable visual patterns are components under `src/components/ui`, not global `@layer` class APIs. Compose Panda classes with Solid `class` / `classList` (and `cx` when needed).

## Principles

- **Desktop-first media control**: optimize for the production default **1600×900** Tauri window (minimum **1280×720**) while remaining resilient at review stress widths **800×600**, **640×720**, and **360×720** without horizontal scroll.
- **Clean OLED first**: default surfaces are solid and high-contrast. Glass, gradients, and glow are reserved for app shell and hero/brand moments.
- **Operational clarity**: every status uses text and icon, not color alone.
- **No fake state**: never show fake media artwork, fake playback progress, or pretend controls.
- **Accessible by default**: normal text contrast must be at least 4.5:1. Large text and meaningful icons must be at least 3:1.

## Color System

All components use semantic tokens. Panda owns token values in `panda.config.ts` and `src/styles/theme-tokens.ts`. `#4f46e5` is the JellyPilot brand seed and primary filled-action background; it is not used as small text on near-black surfaces because contrast is insufficient.

| Token | Hex | Usage |
|---|---:|---|
| Primary | `#4f46e5` | Primary filled actions, JellyPilot identity |
| On Primary | `#ffffff` | Text/icons on primary surfaces |
| Primary Container | `#1b1c3b` | Indigo tonal surfaces |
| On Primary Container | `#e0e2ff` | Text on indigo tonal surfaces |
| Secondary | `#818cf8` | Jellyfin/server/session accent |
| On Secondary | `#0b0a24` | Text on indigo surfaces |
| Secondary Container | `#1f2152` | Indigo tonal surfaces |
| On Secondary Container | `#e0e2ff` | Text on indigo tonal surfaces |
| Tertiary | `#4fe3b1` | Healthy/ready/success state |
| On Tertiary | `#001f16` | Text on healthy surfaces |
| Tertiary Container | `#06382a` | Healthy tonal surfaces |
| On Tertiary Container | `#bfffe8` | Text on healthy tonal surfaces |
| Warning | `#f6c768` | Degraded/retryable warning state |
| On Warning | `#2a1a00` | Text on warning surfaces |
| Warning Container | `#3f2e08` | Warning tonal surfaces |
| On Warning Container | `#ffe7a8` | Text on warning surfaces |
| Error | `#ff6b7a` | Failure/destructive state |
| On Error | `#330006` | Text on error surfaces |
| Error Container | `#4b1119` | Error tonal surfaces |
| On Error Container | `#ffd9de` | Text on error surfaces |
| Background | `#05060a` | App shell background |
| Surface | `#0b0d14` | Base surface |
| Surface Low | `#0a0c12` | Low-depth cards |
| Surface Container | `#111420` | Default cards |
| Surface High | `#161b2a` | Inputs, inset controls |
| Surface Highest | `#22293e` | Overlays and emphasized panels |
| On Surface | `#f3f6ff` | Primary text |
| On Surface Variant | `#aeb8cc` | Secondary text, labels |
| Outline | `#5c6c8c` | Strong borders/focus support |
| Outline Variant | `#262e42` | Subtle dividers |
| Primary Gradient End | `#7a7eff` | Primary button/track gradient endpoint |
| Secondary Gradient End | `#0b4b60` | Secondary/tonal button gradient endpoint |

## Color Semantics

- Indigo means JellyPilot identity, primary app action, local control, and Jellyfin server/session/connection.
- Teal/green means generic healthy or ready.
- Amber means degraded, waiting for recovery, or retryable warning.
- Red means failure or destructive action.
- Quick Connect waiting is a normal state and should use indigo, not amber.

## Typography

Bundled local Fontsource variable fonts are used. No network font imports.

```css
body { font-family: 'Inter Variable', ui-sans-serif, system-ui, sans-serif; }
h1, h2, h3, .brand-type { font-family: 'Space Grotesk Variable', 'Inter Variable', ui-sans-serif, system-ui, sans-serif; }
.code, .diagnostic-value { font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; }
```
Apply typography in owner-local Panda styles with the tokenized properties below.

| Style | Panda properties | Default color |
|---|---|---|
| Display medium | `fontFamily: 'display'`, `fontSize: '45'`, `lineHeight: '52'`, `fontWeight: 'bold'` | — |
| Display small | `fontFamily: 'display'`, `fontSize: '36'`, `lineHeight: '44'`, `fontWeight: 'bold'` | — |
| Headline large | `fontFamily: 'display'`, `fontSize: '32'`, `lineHeight: '40'`, `fontWeight: 'bold'` | — |
| Headline medium | `fontFamily: 'display'`, `fontSize: '28'`, `lineHeight: '36'`, `fontWeight: 'bold'` | — |
| Headline small | `fontFamily: 'display'`, `fontSize: '24'`, `lineHeight: '32'`, `fontWeight: 'bold'` | — |
| Title large | `fontSize: '22'`, `lineHeight: '28'`, `fontWeight: 'bold'` | `onSurface` |
| Title medium | `fontSize: '16'`, `lineHeight: '24'`, `fontWeight: 'semibold'` | `onSurface` |
| Title small | `fontSize: '14'`, `lineHeight: '20'`, `fontWeight: 'semibold'` | `onSurface` |
| Body large | `fontSize: '16'`, `lineHeight: '24'` | `onSurfaceVariant` |
| Body medium | `fontSize: '14'`, `lineHeight: '20'` | `onSurfaceVariant` |
| Body small | `fontSize: '12'`, `lineHeight: '16'` | `onSurfaceVariant/80` |
| Label large | `fontSize: '14'`, `lineHeight: '20'`, `fontWeight: 'semibold'` | — |
| Label medium | `fontSize: '12'`, `lineHeight: '16'`, `fontWeight: 'bold'` | `onSurfaceVariant` |
| Label small | `fontSize: '11'`, `lineHeight: '16'`, `fontWeight: 'bold'` | `onSurfaceVariant/90` |

## Components

### Buttons

- Use the `<Button>` component (`src/components/ui/Button.tsx`) for `primary`, `secondary`, `tonal`, `outlined`, `text`, and `icon` variants. One primary action per section or state.
- For Ark triggers that should look like a button (collapsible, dialog, tags-input delete), render `<Button>` through the Ark part's `asChild` prop instead of reaching for a helper class.

### Inputs

- Use `<FieldControl>` / `<FieldTextarea>` (`src/components/ui/FieldControl.tsx`) with `variant="filled"` (Login and configuration) or `variant="outlined"` (compact selectors). In Ark Field parts, render them through `asChild` so Ark keeps owning ARIA/focus.
- `<TextField>` (`src/components/ui/TextField.tsx`) wraps `FieldControl` with label, error, and hint for plain forms.
- Every field has a visible label. Errors appear near the field.

### Cards and Surfaces

- Use `<Card>` / `<CardLink>` (`src/components/ui/Card.tsx`) with `variant="filled"` (default solid panel), `"elevated"` (hero/emphasized), or `"outlined"` (extra separation). `<CardLink>` is the card-styled anchor for navigational cards.
- Hero surfaces may use selective gradient/glass (the non-atomic gradients live in `Card.styles.ts`). Diagnostics and dense text must remain solid.

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

## Panda CSS Authoring

- Keep styles beside their owner in `Component.styles.ts` or route `*.styles.ts` modules.
- Import `css`, `cva`, `sva`, or `cx` from `@styled-system/css`; use patterns from `@styled-system/patterns` only when they reduce repetition.
- Use semantic color, spacing, typography, radius, shadow, duration, easing, and z-index tokens. Add a reusable token before introducing a repeated literal.
- Use Panda conditions and nested selectors for responsive rules, interaction states, and Ark `data-*` states.
- In Solid `.tsx`, use `class` for static classes, `classList` for conditional maps, and `cx` for generated class composition.

```ts
import { css } from '@styled-system/css';

export const panel = css({
  backdropFilter: '[blur(12px)]',
  display: 'flex',
  gap: '4',
});
```

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

- Light mode.
- UI sounds or haptics.
- Raw URL playback controls.
- Fake artwork or fake playback state.
