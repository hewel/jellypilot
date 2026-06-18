# JMSR Control Room Design System

JMSR uses a desktop-first Control Room design system: dark-only, clean OLED surfaces, selective cinematic glass, and clear operational state. The interface should feel like a reliable media companion for a Jellyfin Playback Target, not a generic mobile settings app.

Do not manually rebuild controls from raw utility piles when a component class exists. Use shared classes from `src/index.css` for buttons, inputs, cards, text styles, status surfaces, and diagnostics patterns.

## Principles

- **Desktop-first media control**: optimize for the default 800×600 Tauri window while remaining usable down to 360px width without horizontal scroll.
- **Clean OLED first**: default surfaces are solid and high-contrast. Glass, gradients, and glow are reserved for app shell and hero/brand moments.
- **Operational clarity**: every status uses text and icon, not color alone.
- **No fake state**: never show fake media artwork, fake playback progress, or pretend controls.
- **Accessible by default**: normal text contrast must be at least 4.5:1. Large text and meaningful icons must be at least 3:1.

## Color System

All components use semantic tokens. `#4f46e5` is the JMSR brand seed and primary filled-action background; it is not used as small text on near-black surfaces because contrast is insufficient.

| Token | Tailwind Class | Hex | Usage |
|---|---|---:|---|
| Primary | `text-primary`, `bg-primary` | `#4f46e5` | Primary filled actions, JMSR identity |
| On Primary | `text-on-primary` | `#ffffff` | Text/icons on primary surfaces |
| Primary Container | `bg-primary-container` | `#1b1c3b` | Indigo tonal surfaces |
| On Primary Container | `text-on-primary-container` | `#e0e2ff` | Text on indigo tonal surfaces |
| Secondary | `text-secondary`, `bg-secondary` | `#818cf8` | Jellyfin/server/session accent |
| On Secondary | `text-on-secondary` | `#0b0a24` | Text on indigo surfaces |
| Secondary Container | `bg-secondary-container` | `#1f2152` | Indigo tonal surfaces |
| On Secondary Container | `text-on-secondary-container` | `#e0e2ff` | Text on indigo tonal surfaces |
| Tertiary | `text-tertiary`, `bg-tertiary` | `#4fe3b1` | Healthy/ready/success state |
| On Tertiary | `text-on-tertiary` | `#001f16` | Text on healthy surfaces |
| Tertiary Container | `bg-tertiary-container` | `#06382a` | Healthy tonal surfaces |
| On Tertiary Container | `text-on-tertiary-container` | `#bfffe8` | Text on healthy tonal surfaces |
| Warning | `text-warning`, `bg-warning` | `#f6c768` | Degraded/retryable warning state |
| On Warning | `text-on-warning` | `#2a1a00` | Text on warning surfaces |
| Warning Container | `bg-warning-container` | `#3f2e08` | Warning tonal surfaces |
| On Warning Container | `text-on-warning-container` | `#ffe7a8` | Text on warning tonal surfaces |
| Error | `text-error`, `bg-error` | `#ff6b7a` | Failure/destructive state |
| On Error | `text-on-error` | `#330006` | Text on error surfaces |
| Error Container | `bg-error-container` | `#4b1119` | Error tonal surfaces |
| On Error Container | `text-on-error-container` | `#ffd9de` | Text on error tonal surfaces |
| Background | `bg-background` | `#05060a` | App shell background |
| Surface | `bg-surface` | `#0b0d14` | Base surface |
| Surface Low | `bg-surface-container-low` | `#0a0c12` | Low-depth cards |
| Surface | `bg-surface-container` | `#111420` | Default cards |
| Surface High | `bg-surface-container-high` | `#161b2a` | Inputs, inset controls |
| Surface Highest | `bg-surface-container-highest` | `#22293e` | Overlays and emphasized panels |
| On Surface | `text-on-surface` | `#f3f6ff` | Primary text |
| On Surface Variant | `text-on-surface-variant` | `#aeb8cc` | Secondary text, labels |
| Outline | `border-outline` | `#5c6c8c` | Strong borders/focus support |
| Outline Variant | `border-outline-variant` | `#262e42` | Subtle dividers |
| Brand Glow | `bg-brand-glow` | `#4f46e5` | Ambient indigo glow only |
| Console Grid | `bg-console-grid` | `#1a2240` | Subtle shell/grid accents |

## Color Semantics

- Indigo means JMSR identity, primary app action, local control, and Jellyfin server/session/connection.
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

Use the shared text helpers: `text-display-*`, `text-headline-*`, `text-title-*`, `text-body-*`, and `text-label-*`.

## Components

### Buttons

- `btn-primary`: primary purple filled action. One primary action per section or state.
- `btn-secondary` / `btn-tonal`: medium-emphasis action.
- `btn-outlined`: lower-emphasis action.
- `btn-text`: inline/subtle action.
- `btn-icon`: icon-only action with an accessible label.

### Inputs

- Use filled/inset fields for Login and configuration forms.
- Use outlined/tonal controls for compact selectors such as HTTP/HTTPS.
- Every field has a visible label. Errors appear near the field.

### Cards and Surfaces

- `card-filled`: default solid panel.
- `card-elevated`: emphasized/hero or important panel.
- `card-outlined`: panel requiring extra separation.
- Hero surfaces may use selective gradient/glass. Diagnostics and dense text must remain solid.

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
