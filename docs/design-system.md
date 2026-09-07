# JellyPilot Design System (iced)

JellyPilot uses a desktop-first design system: a Neon Indigo accent over Charcoal (dark) and Light Clean (light) surface systems — flat, clean surfaces and clear operational state in both modes. The interface should feel like a reliable media companion for a Jellyfin Playback Target, not a generic mobile settings app.

The design system lives in `crates/jellypilot-ui`: tokens in `tokens.rs` (`TOKENS`), variant enums in `variants.rs`, and the widget catalog in `widgets/`. Views under `src-iced/src/app/view/` compose those pieces; they never invent new token values.

**2026-09-06 Sidebar surface revision: implemented; human visual acceptance pending.** The scoped rules below describe the Sidebar and Account Popover treatment. Existing default tokens and unrelated surfaces are unchanged; see the [native Sidebar specification](sidebar-design-spec.md#appearance) for geometry and pending human acceptance.

## Principles

- **Clean and flat first**: ordinary surfaces are flat, solid, and opaque — Charcoal keeps 4–7% lightness instead of OLED pure black. Depth comes from the two semantic shadow tiers on floating layers. The scoped native blur treatments below are exceptions, not a general glass surface system. Permitted structural outlines identify boundaries, not elevation.
- **Visual restraint**: separation is whitespace first, shell hairlines second. The Sidebar surface revision permits a small, explicit set of structural outlines and toolbar separators, not blanket element framing.
- **Operational clarity**: every status uses text and icon, not color alone.
- **No fake state**: never show fake media artwork, fake playback progress, or pretend controls.
- **Accessible by default**: normal text contrast must be at least 4.5:1. Large text and meaningful icons must be at least 3:1.

## Surface Roles

Every container has a semantic surface role (`SurfaceVariant`, styled by `widgets/container.rs`). Base roles remain fully opaque and borderless by default. The accepted Sidebar surface treatments below refine those defaults through jellypilot-ui styles; they are not a second styling system.

| Role | Background | Radius | Shadow | Use |
|---|---|---:|---|---|
| `Canvas` | `background` | 0 | none | Flush with the window: shell root, page content, inline content groups separated by whitespace |
| `Block` | `surfaceContainerLowest` | 0 | none | Docked blocks: sidebar, player bar |
| `Raised` | `surfaceContainerHigh` | `lg` (8) | `raised_high` | Standalone floating card: the login card |
| `Floating` | `surfaceContainerHigh` | `lg` (8) | `raised_high` + 1px `outlineVariant` edge | Layered floating cards: popovers, modal cards, toasts (with severity fills), floating prompts (Skip Intro) |

Inline content (home hero and action cards, detail episode/next-up/summary rows, settings sections and rows, saved sign-ins) is **flat Canvas with whitespace separation** — no card chrome. Skeleton placeholders are flat `surfaceContainerLow`↔`surfaceContainerHigh` breathing blocks, radius `lg`, no border or shadow.

## Shell Hairlines and Structural Boundaries

The shell keeps two 1px `outlineVariant` dividers, both built as explicit divider containers in `view/shell.rs` (iced has no per-edge borders):

1. A vertical hairline between the sidebar and the content area.
2. A horizontal hairline above the player bar.

Outside the scoped Sidebar treatment below, surfaces retain their borderless defaults. Badges, media cards, and ordinary navigation rows do not gain outlines. The exception is layered floating surfaces — popovers, modal cards, toasts, and floating prompts — which carry a 1px `outlineVariant` structural edge alongside their shadow so they read as a separate layer above content; the standalone login card, tooltips, and the scroll-to-bottom indicator stay borderless. Decorative primary-tinted halo borders remain prohibited; functional focus and error indications are distinct from decorative structure. Outline width encodes persistence: 1px marks persistent structural or functional state (boundaries, floating-layer edges, field focus, field error); 2px is reserved for the transient keyboard-only button focus ring.

### Sidebar Surface Revision

**Implemented scoped treatment; human visual acceptance pending.** Scope: the entire Sidebar, including search, navigation, library heading/rows, bottom identity card, toolbar, and Account Popover. It does not extend to Home, Settings, add-account or confirmation modals. Shared account functionality is not permission to restyle its Settings presentation.

- Keep the general 6/8 radius scale and default palette values unchanged. Represent the new treatments with semantic tokens and Catalog styles in jellypilot-ui, not per-widget literals, a global token replacement, or a separate theme mechanism.
- Target radius 12 for Sidebar search, personal navigation, bottom identity card, and toolbar; radius 8 for library rows. The Account Popover targets radius 20 with padding 12 and radius 8 inset rows/controls. Geometry and dimensions are authoritative in the [native Sidebar specification](sidebar-design-spec.md#accepted-visual-targets).
- Light-mode Account Popover: white opaque surface, quiet transparent menu rows, pale neutral hover surfaces, and a soft floating shadow. Dark mode uses corresponding Charcoal roles with preserved hierarchy and contrast. Do not globally recolor the shared `Raised` role to achieve this.
- Permit low-contrast 1px structural outlines on the Account Popover outer edge, bottom identity card, and necessary inset controls, plus fine separators between the bottom toolbar's three segments. These boundaries use theme-aware neutral semantics. Do not frame every row, badge, or action button, and do not add Account Popover section rules by default.
- Search and docked controls remain visually quiet; they do not inherit prominent `Raised` elevation merely to achieve rounded geometry. Account Popover elevation uses the existing semantic shadow vocabulary, without glass blur or decorative looping effects.
- Selected Sidebar navigation rows use a pale indigo fill with readable accent content. The Account Popover instead shows the current identity once in its header and lists only alternatives. Fixed-size avatars must not absorb spare row width; Settings retains its existing profile-selection presentation.
- Opening the Account Popover does not itself create a strong purple identity-card outline. Preserve separately visible keyboard focus and error states; the ban on decorative outlines must not suppress functional accessibility feedback.
- The Account Popover is a quick menu: identity/address, alternate accounts, then Add account, Manage accounts, and connected-only Disconnect. Startup Auto Login, remote-control details, and Sign Out remain in Accounts and connection Settings with their existing controls and semantics.

The revision restores the reference's surface hierarchy, not every reference detail: no sample account roles, fake Mbps, avatar gradients, or new product capabilities. Implementation must leave unrelated default components unchanged and pass human visual acceptance in both themes.

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
The Sidebar surface revision adds scoped role targets of 12 and 20 without changing these existing token values or their default consumers.

## Buttons

Variants (`ButtonVariant`, styled by `widgets/button.rs`): `Primary`, `Secondary`, `Tonal`, `TonalActive`, `Text`, `Icon`. Defaults use radius `md` and cast no shadow. The accepted Sidebar treatment refines geometry through semantic Catalog styles without changing every consumer of these variants.
Simple controls with an optional icon and label use the status-aware `control_button` widget; whole-control hover drives both icon and label through the variant's content colors.
Composite Sidebar and profile rows use `control_button_content` for whole-control hover and keyboard focus. Existing iced `button` consumers and fixed-color icons (the favorited heart, the theme toggle) retain their established `button_variant` treatment.

- **Secondary** is the borderless tint chip — filled `secondaryContainer` with `onSecondaryContainer` content. It exists ONLY as the active state of a switch group (sidebar destinations, three-way selectors like login method or Intro Mode). Never use it for actions; actions are Tonal or Primary.
- **Tonal** is the default quiet control: `control` fill with `onControl` content at rest, then `controlHover` fill with `onControlHover` content on hover. It has no border.
- **TonalActive** is the selected/on state of a tonal control: always `controlHover` fill with `onControlHover` content. Toggle call sites use the `TonalActive`/`Tonal` pair.
- **Primary** keeps its 10% hover brightness lift; one primary action per section or state.
- **Text** is the neutral ghost vocabulary: `text.body` content on a transparent background, then `control` fill with `text.heading` content on hover. It belongs to navigation-like, switch-group contexts (sidebar destinations, selector rows). Indigo accent text marks ONLY the active/selected state. Actions never use Text — they are Tonal or Primary.
- **Sidebar menu actions** opt into a scoped Tonal/Icon Catalog treatment: transparent at rest, neutral hover/press feedback, and minimum 40px hit height. The copy icon has a 40×40 target. This exception avoids a stack of filled buttons in the Account Popover without changing other Tonal controls.
- **Button focus**: focus rings and focus-triggered hints appear only for keyboard interaction. Pointer presses clear button focus, including presses captured by overlays; pointer-origin dismissal must not create hidden button focus that later reappears. This does not change native text-input focus or caret behavior. The ring is 2px per the outline width rule in Shell Hairlines and Structural Boundaries.

## Fields, Badges, Overlays

- **Fields**: opaque `control` fill at rest and `controlHover` fill when focused, radius `md`, no idle border by default. Functional feedback remains: `text_input::Status::Focused` draws a 1px `primary` border and an invalid field draws a 1px `error` border. Sidebar structural boundaries are a separate, scoped treatment; they do not replace visible focus or error feedback.
- **Badges**: opaque container fills (`tertiaryContainer` / `warningContainer` / `surfaceContainerHigh`), radius `md`, no border.
- **Popover**: opaque `surfaceContainerHigh`, `raised_high` shadow, radius `lg`, and the floating-layer 1px `outlineVariant` edge. The Account Popover keeps its scoped Sidebar treatment with the same outline color.
- **Tooltip**: `raised` shadow, radius `md`, no border. Opt-in Sidebar full-value hints also appear on keyboard focus and wrap long unbroken values within their bounded surface. An open popover suppresses its trigger's hover and focus hints while preserving hints within the popover content.
- **Toast**: floating-layer treatment (opaque severity container fill, `raised_high` shadow, radius `lg`, 1px `outlineVariant` edge). Severity is shown by icon, text color, and fill — the structural edge is always neutral.
- **Modal cards** (settings, add-account, confirmation): `Floating` role. Narrow or control-only layouts that render edge-to-edge use `Canvas` and stay borderless.

## Media Cards

`PosterCard` draws no hover or press overlay, lift, or tint — the artwork and copy render exactly as provided, and interaction only publishes the press message. Media images use radius `lg`.

The detail hero keeps its backdrop scrim, simplified to two stops: transparent at the top → `surfaceContainerLowest` at 0.85 alpha at the bottom.

### Native Image and Modal Blur

- Home Hero actions sample only the original Backdrop, at its full-width natural-aspect `Contain` frame with the same theme fade. Native image blur sits below local semantic button tint, sharp labels/icons, and focus borders. Missing artwork retains normal opaque button surfaces; Title Logos are never the blur source.
- Card progress samples the original artwork with native blur. The full card frame, radius, smoothing, and snap policy define the mask; a separate rectangular reveal exposes the bottom four logical pixels. Track tint precedes the percentage-clipped played color, with selection/focus treatment above both. Updating progress does not change blur parameters.
- Wide Settings and account dialogs blur the live lower scene before dimming it; dialogs and higher Toasts remain sharp. Account dialogs retain the underlying widget tree while shielding its input, focus traversal, and overlays. Account-over-Settings uses one scene marker. Narrow and Control-Only full-screen Settings, and ordinary Popovers, keep their existing presentation.
- Modal backdrop blur uses the shared mode-independent `TOKENS.modal.backdrop_blur_sigma` token (`40.0`, approximate Gaussian sigma in logical pixels). Hero/card image blur and shadow radii are separate semantics.
- Opening or closing Settings retains the background page's scroll position by keeping its stack ancestry stable. A primary mouse press or touch on the dimmed backdrop dismisses only the topmost dialog through its existing Close/Cancel action. The complete dialog bounds, including passive content and padding, shield dismissal; full-screen Settings has no backdrop dismissal target.
- This integration pins the iced fork to `f812ae504989444eb57d9f8669c1ab5a2e326c95`. WGPU uses isolated targets and downsampled Gaussian blur; software uses a premultiplied three-box approximation. Active rendition axes are capped at 1024 pixels on GPU and 2048 in software; backend results are not pixel-identical.
- Matching sources and effective blur parameters can share cached results. Position, opacity, mask, tint, or progress-only changes do not require another convolution; source, crop, scale, and sigma changes can miss. Scene-cache hits neither freeze the background nor eliminate all lower-scene drawing. Large software-blurred scenes remain expensive.
- Advanced WGPU drawing with `Renderer::draw(None)` rejects positive scene blur; the ordinary window compositor supplies a clear color. Human acceptance must check both themes, scrolling/resize alignment, the card's bottom corners, live modal backgrounds, sharp Toasts, and keyboard focus. Code-level/headless checks do not establish visual acceptance.

## Slop Prohibitions

- **No translucency without blur.** Surfaces, fields, and badges are 100% opaque semantic colors. (Text placeholders, selection, and disabled-state alpha are not surfaces.)
- **No blanket element-wrapping outlines.** Keep the two shell hairlines, scoped Sidebar structural boundaries, and functional focus/error feedback; do not extend outlines to unrelated surfaces.
- **No decorative tinted borders.** Primary halo and severity framing remain prohibited. This does not prohibit visible functional focus or invalid-field indicators.
- **No hover overlay lifts.** No white overlay rectangles, ghost panels, or elevation changes on hover/press; hover feedback is a fill change on the control itself.

## Color Semantics

The locked palette is a **Neon Indigo** accent over two surface systems: **Charcoal** (dark: near-zero-chroma deep charcoal, 4–7% lightness, never OLED pure black) and **Light Clean** (light: cold-white canvas, pure-white surfaces). Concrete values live in `tokens.rs` (`DARK_PALETTE` / `LIGHT_PALETTE`) and are pinned by contract tests.

- Indigo `#6366f1` (`primary`) means JellyPilot identity and primary app action in both modes. It is a fill and focus color, not general-purpose small text: when accent text marks an active/selected state, it uses `secondary` — `#818cf8` on dark, the deeper `#4f46e5` on light.
- Control roles keep reusable controls neutral: `control` / `controlHover` are the rest and hover/focus fills, while `onControl` / `onControlHover` are their corresponding content colors.
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


## Icons

All UI icons are vendored from the Reicon set (MIT, `crates/jellypilot-ui/assets/icons/`, see [ADR 0034](adr/0034-reicon-icon-set.md)) and render on a 24×24 grid. The default weight is Outline; the Filled weight marks active state only where the vocabulary already pairs them (favorited heart, watchlist bookmark, the played-filter disc). Icons are consumed exclusively through the semantic `Icon` enum and the `icon*` helpers in `jellypilot-ui`, which tint via `currentColor` — never hardcode colors in vendored SVGs, and extend the enum from Reicon rather than importing one-off artwork.

## Motion

- Skeleton placeholders breathe between two opaque surface tones; under reduced motion (or a non-finite phase) they render the static `surfaceContainerLow` block.
- Avoid decorative looping animation except subtle indeterminate waiting indicators.
- Respect the user's reduce-motion setting.

## Out of Scope

- UI sounds or haptics.
- Raw URL playback controls.
- Fake artwork or fake playback state.
