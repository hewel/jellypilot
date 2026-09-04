# JellyPilot Design System (iced)

JellyPilot uses a desktop-first design system: a Neon Indigo accent over Charcoal (dark) and Light Clean (light) surface systems — flat, clean surfaces and clear operational state in both modes. The interface should feel like a reliable media companion for a Jellyfin Playback Target, not a generic mobile settings app.

The design system lives in `crates/jellypilot-ui`: tokens in `tokens.rs` (`TOKENS`), variant enums in `variants.rs`, and the widget catalog in `widgets/`. Views under `src-iced/src/app/view/` compose those pieces; they never invent new token values.

## Principles

- **Clean and flat first**: surfaces are flat, solid, and opaque — Charcoal keeps 4–7% lightness instead of OLED pure black. Depth comes from exactly two shadow tiers on floating layers, never from translucency or outlines.
- **Visual restraint**: separation is whitespace first, the two shell hairlines second. Nothing else draws a line.
- **Operational clarity**: every status uses text and icon, not color alone.
- **No fake state**: never show fake media artwork, fake playback progress, or pretend controls.
- **Accessible by default**: normal text contrast must be at least 4.5:1. Large text and meaningful icons must be at least 3:1.

## Surface Roles

Every container is exactly one role (`SurfaceVariant`, styled by `widgets/container.rs`). All roles are fully opaque and borderless.

| Role | Background | Radius | Shadow | Use |
|---|---|---:|---|---|
| `Canvas` | `background` | 0 | none | Flush with the window: shell root, page content, inline content groups separated by whitespace |
| `Block` | `surfaceContainerLowest` | 0 | none | Docked blocks: sidebar, player bar |
| `Raised` | `surfaceContainerHigh` | `lg` (8) | `raised_high` | Floating layers: login card, intro prompt, toasts, popovers |

Inline content (home hero and action cards, detail episode/next-up/summary rows, settings sections and rows, saved sign-ins) is **flat Canvas with whitespace separation** — no card chrome. Skeleton placeholders are flat `surfaceContainerLow`↔`surfaceContainerHigh` breathing blocks, radius `lg`, no border or shadow.

## The Two-Hairline Rule

The application draws exactly two lines, both 1px `outlineVariant`, both built as explicit divider containers in `view/shell.rs` (iced has no per-edge borders):

1. A vertical hairline between the sidebar and the content area.
2. A horizontal hairline above the player bar.

No other element may draw a border or divider. Badges, toasts, popovers, containers, and cards never have outlines; primary-tinted halo borders are gone.

## Shadows

Two semantic tiers (`Shadows` in `tokens.rs`); everything else was deleted.

| Token | Offset / Blur | Alpha | Use |
|---|---|---:|---|
| `none` | — | — | Flush surfaces, controls |
| `raised` | y 2, blur 8 | 0.45 dark / 0.06 light | Small floating chrome: tooltips, scroll-to-bottom indicator |
| `raised_high` | y 8, blur 24 | 0.65 dark / 0.10 light | Floating layers: popovers, toasts, `Raised` surfaces |

Buttons never cast a shadow. `ShadowToken` keeps the CSS spread/inset fields; the `iced()` conversion maps offset, blur, and color.

## Radii

The scale is `none` (0), `sm` (2), `md` (6), `lg` (8), `full` (9999). Usage mapping:

| Radius | Use |
|---|---|
| `none` (0) | Docked blocks (sidebar, player bar), canvas |
| `sm` (2) | Small inline chrome (toast dismiss button) |
| `md` (6) | Controls: buttons, fields, badges, tooltips |
| `lg` (8) | Floating layers, media images, poster artwork, skeletons |
| `full` | Scrollbars, status dots |

Nested rounding follows the concentric rule: inner radius = parent radius − padding, floored at 0.

## Buttons

Variants (`ButtonVariant`, styled by `widgets/button.rs`): `Primary`, `Secondary`, `Tonal`, `TonalActive`, `Text`, `Icon`. All use radius `md` and cast no shadow.

- **Secondary** is the ghost chip — filled `secondaryContainer` with a 1px `outlineVariant` border. It exists ONLY as the active state of a switch group (sidebar destinations, three-way selectors like login method or Intro Mode). Never use it for actions; actions are Tonal or Primary.
- **Tonal** is the default quiet control: quiet `surfaceContainerLow` fill at rest, `surfaceContainerHigh` on hover, no border.
- **TonalActive** is the selected/on state of a tonal control: always filled with `surfaceContainerHigh`. Toggle call sites use the `TonalActive`/`Tonal` pair.
- **Primary** keeps its 10% hover brightness lift; one primary action per section or state.
- **Text** is the ghost vocabulary: tinted `secondary` text on a transparent background. It belongs to navigation-like, switch-group contexts (sidebar destinations, selector rows) where an indigo readout is the "not selected" state. Actions never use Text — they are Tonal or Primary.

## Fields, Badges, Overlays

- **Fields**: opaque `surfaceContainerHigh` fill, radius `md`, no idle border. The single border exemption is functional: `text_input::Status::Focused` draws a 1px `primary` border (accessibility), and an invalid field draws a 1px `error` border. This exemption applies to text inputs only.
- **Badges**: opaque container fills (`tertiaryContainer` / `warningContainer` / `surfaceContainerHigh`), radius `md`, no border.
- **Popover**: opaque `surfaceContainerHigh`, `raised_high` shadow, radius `lg`, no border.
- **Tooltip**: `raised` shadow, radius `md`, no border.
- **Toast**: `Raised` role (opaque severity container fill, `raised_high` shadow, radius `lg`). Severity is shown by icon and text color, never by a border.

## Media Cards

`PosterCard` draws no hover or press overlay, lift, or tint — the artwork and copy render exactly as provided, and interaction only publishes the press message. Media images use radius `lg`.

The detail hero keeps its backdrop scrim, simplified to two stops: transparent at the top → `surfaceContainerLowest` at 0.85 alpha at the bottom.

## Slop Prohibitions

- **No translucency without blur.** Surfaces, fields, and badges are 100% opaque semantic colors. (Text placeholders, selection, and disabled-state alpha are not surfaces.)
- **No element-wrapping outlines.** Lines exist only as the two shell hairlines, plus the field focus/error exemption.
- **No tinted borders.** The `primary`-at-20% halo and all severity borders are deleted.
- **No hover overlay lifts.** No white overlay rectangles, ghost panels, or elevation changes on hover/press; hover feedback is a fill change on the control itself.

## Color Semantics

The locked palette is a **Neon Indigo** accent over two surface systems: **Charcoal** (dark: near-zero-chroma deep charcoal, 4–7% lightness, never OLED pure black) and **Light Clean** (light: cold-white canvas, pure-white surfaces). Concrete values live in `tokens.rs` (`DARK_PALETTE` / `LIGHT_PALETTE`) and are pinned by contract tests.

- Indigo `#6366f1` (`primary`) means JellyPilot identity and primary app action in both modes. It is a fill and focus color, not small text: accent text uses `secondary` — `#818cf8` on dark, the deeper `#4f46e5` on light.
- Emerald means healthy/ready (`tertiary`: `#34d399` dark / `#047857` light); amber means ratings and degraded or retryable states (`warning`: `#fbbf24` / `#b45309`); red means failure or destructive (`error`: `#ff6b7a` dark, dark red on light). The favorited heart uses the rose `favorite` accent (`#f87171` / `#e11d48`).
- Dark mode uses the bright 400-series status steps; light mode drops to 700-series steps so text and icons hold the 4.5:1 floor on the light canvas.

## Text Hierarchy

Five semantic rungs (`ThemePalette.text`), resolved per mode:

| Rung | Dark | Light | Use |
|---|---|---|---|
| `heading` | `#ffffff` | `#0f172a` | Page titles, item names, primary values |
| `secondary` | `#f4f4f5` | `#1e293b` | Cast and genre values, important subtitles, setting names |
| `body` | `#d4d4d8` | `#475569` | Overviews, descriptions, long-form reading text |
| `metadata` | `#a1a1aa` | `#64748b` | Labels, years, timestamps, captions, empty-state messages |
| `muted` | `#71717a` | `#94a3b8` | Auxiliary hints and placeholders only |

`heading` through `metadata` hold at least 4.5:1 contrast on their mode's canvas. `muted` is exempt from the floor: it marks non-essential text (device IDs, version strings, placeholders, loading hints) and must never carry information the user has to read.

## Typography

Bundled local fonts only; no network font imports. Body text uses Inter (`sans`), headlines and brand type use Space Grotesk (`display`, exposed as `SPACE_GROTESK_FONT`), diagnostics values use the mono stack. Sizes and weights come from the `font_sizes`, `line_heights`, and `font_weights` tokens.

## Motion

- Skeleton placeholders breathe between two opaque surface tones; under reduced motion (or a non-finite phase) they render the static `surfaceContainerLow` block.
- Avoid decorative looping animation except subtle indeterminate waiting indicators.
- Respect the user's reduce-motion setting.

## Out of Scope

- UI sounds or haptics.
- Raw URL playback controls.
- Fake artwork or fake playback state.
