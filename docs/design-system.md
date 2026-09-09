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

### Temporary Opaque Approximation for Translucent Borders

- When a component needs the appearance of a translucent border, the current workaround is an **opaque, preblended border color**, not actual border alpha. For example, a neutral gray can approximate translucent white over a dark surface; apply the same principle to black or colored outlines over other surfaces.
- Derive the approximation from the intended foreground color, opacity, and reference backing surface: `result = opacity × foreground + (1 − opacity) × backing`, in the color space used for the intended composition, then render the resulting color with alpha `1`. There is no single gray that works over every background.
- Use semantic tokens and Catalog styles. Choose the reference backing surface deliberately and account for light/dark themes and hover/pressed surface changes; do not scatter hard-coded gray borders across widgets.
- This is only an approximation over artwork, gradients, blur, or other changing backgrounds. It does not reproduce true transparency, fix antialiasing/coverage defects, or establish that iced's renderer is correct. Exact translucent-border rendering remains a separate renderer investigation, not a dependency change authorized by this workaround.
- The strategy does not add borders to otherwise borderless components. Hero glass currently remains decoratively borderless; keyboard focus and error indications retain their independent semantics and visibility. This records an allowed temporary approach, not a completed migration of existing outlines.
- **Known visual issue — deferred, not resolved:** after removing the Hero glass decorative border, human inspection of a supplied close-up still found a visible edge/corner artifact. The background/blur mask is a suspected cause, not a confirmed diagnosis. Removing the border did not fully eliminate the visible defect. Record only for now; no further rendering investigation or fix is authorized by this observation.

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

The scale in `tokens.rs` is `none` (0), `sm` (2), `md` (6), `lg` (8), `xl` (12), `x2l` (20), `full` (9999) — `tokens.rs` is authoritative. Usage mapping:

| Radius | Use |
|---|---|
| `none` (0) | Docked blocks (sidebar), canvas |
| `sm` (2) | Small inline chrome (toast dismiss button) |
| `md` (6) | Small controls: compact icon buttons, badges, tooltips, list-view thumbnail artwork |
| `lg` (8) | Library-row level chrome, skeletons |
| `xl` (12) | Media cards and poster artwork, buttons, fields, popovers, floating player bar |
| `x2l` (20) | Modal cards (settings, account popover) |
| `full` | Scrollbars, status dots, toggle switches |

Nested rounding follows the concentric rule: inner radius = parent radius − padding, floored at 0.
The Sidebar surface revision's scoped role targets of 12 and 20 correspond to `xl` and `x2l`.

## Buttons

Variants (`ButtonVariant`, styled by `widgets/button.rs`): `Primary`, `Secondary`, `Tonal`, `TonalActive`, `Text`, `Icon`, `Pill`, `PillActive`. Buttons cast no shadow. Default radius is `xl` (12), with `lg` (8) for pills and explicit S-tier overrides. Scoped Sidebar Catalog geometry remains authoritative for its controls.
Simple controls with an optional icon and label use the status-aware `control_button` widget; whole-control hover drives both icon and label through the variant's content colors.
Composite Sidebar and profile rows use `control_button_content` for whole-control hover and keyboard focus. Existing iced `button` consumers and fixed-color icons (the favorited heart, the theme toggle) retain their established `button_variant` treatment.

- **Secondary** is the borderless tint chip — filled `secondaryContainer` with `onSecondaryContainer` content. It exists ONLY as the active state of a switch group (sidebar destinations, three-way selectors like login method or Intro Mode). Never use it for actions; actions are Tonal or Primary.
- **Tonal** is the quiet action control: `surfaceContainerHigh` fill, `onControl` content, and a 1px `borderSubtle` edge at rest; `controlHover`/`onControlHover` on hover and `surfaceContainer` while pressed.
- **TonalActive** is the selected/on state of a tonal control: always `controlHover` fill with `onControlHover` content. Toggle call sites use the `TonalActive`/`Tonal` pair.
- **Primary** holds one primary action per section or state. Its hover/pressed use the `primaryHover`/`primaryPressed` tokens accepted in the 2026-09-09 controls sync, superseding the 10% hover brightness lift.
- **Text** is the neutral ghost vocabulary: `text.body` content on a transparent background, then `control` fill with `text.heading` content on hover. It belongs to navigation-like, switch-group contexts (sidebar destinations, selector rows). Indigo accent text marks ONLY the active/selected state. Actions never use Text — they are Tonal or Primary.
- **Sidebar menu actions** opt into a scoped Tonal/Icon Catalog treatment: transparent at rest, neutral hover/press feedback, and minimum 40px hit height. The copy icon has a 40×40 target. This exception avoids a stack of filled buttons in the Account Popover without changing other Tonal controls.
- **Button focus**: focus rings and focus-triggered hints appear only for keyboard interaction. Pointer presses clear button focus, including presses captured by overlays; pointer-origin dismissal must not create hidden button focus that later reappears. Native text-input focus and caret behavior are unchanged. The ring is 2px; Primary controls use `secondary` for contrast with their fill, while other variants use `primary`.

## Fields, Badges, Overlays

- **Fields**: opaque `surfaceContainerHigh` fill, radius `xl`, and a 1px `borderSubtle` idle border. Settings values use 12px text, with monospace for path/parameter values. Focus retains `controlHover` fill and a 1px `primary` border; invalid fields retain a 1px `error` border. Paper's 2px outer field-focus ring is deliberately not adopted.
- **Status tags**: H24, 10px inline padding, radius `full`, vertically centered 12px/600 label and 6px status dot. Success/Warning/Neutral/Error use opaque semantic container fills without borders. Large non-tag Quick Connect panels retain the separate badge surface treatment.
- **Popover**: opaque `surfaceContainerHigh`, `raised_high` shadow, radius `lg`, and the floating-layer 1px `outlineVariant` edge. The Account Popover keeps its scoped Sidebar treatment with the same outline color.
- **Tooltip**: `raised` shadow, radius `md`, no border. Opt-in Sidebar full-value hints also appear on keyboard focus and wrap long unbroken values within their bounded surface. An open popover suppresses its trigger's hover and focus hints while preserving hints within the popover content.
- **Toast**: floating-layer treatment (opaque severity container fill, `raised_high` shadow, radius `lg`, 1px `outlineVariant` edge). Severity is shown by icon, text color, and fill — the structural edge is always neutral.
- **Modal cards** (settings, add-account, confirmation): `Floating` role. Narrow or control-only layouts that render edge-to-edge use `Canvas` and stay borderless.

## Media Cards

`PosterCard` draws no hover or press overlay, lift, or tint — the artwork and copy render exactly as provided, and interaction only publishes the press message. Media images use radius `lg`.

Detail Backdrops use the two-layer canvas scrim and fixed-height composition specified in [Accepted Paper Detail Composition](#accepted-paper-detail-composition).

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

Missing icon assets may be obtained directly from [Reicon](https://reicon.dev); reuse suitable vendored glyphs first rather than drawing approximations. Add retrieved assets through the existing semantic icon pipeline and preserve their source and license attribution.

## Motion

- Skeleton placeholders breathe between two opaque surface tones; under reduced motion (or a non-finite phase) they render the static `surfaceContainerLow` block.
- Avoid decorative looping animation except subtle indeterminate waiting indicators.
- Respect the user's reduce-motion setting.

## Out of Scope

- UI sounds or haptics.
- Raw URL playback controls.
- Fake artwork or fake playback state.

## 2026-09-08 Paper Design Sync (Media Streamer file)

**Design reference accepted in the Paper file; implementation acceptance pending.** The Paper file ("Media Streamer") is the visual reference for the home, library browse, detail pages, player bar, control-only window, account popover, and settings window. Where this section differs from older text above, this section wins for new work; `tokens.rs` remains authoritative for token values.

### Surfaces and Edges

- **Image outline**: all artwork (posters, episode thumbnails, avatars, cast photos) carries the `imageOutline` token's 1px pure-white 10% outline. Never a tinted near-white — tinted outlines pick up the surface beneath and read as dirt. Full-bleed hero backdrops and transparent title logos are exempt.
- **Quiet control edge**: secondary/tonal controls filled `surfaceContainerHigh` gain a 1px neutral structural edge (`border-subtle`, white 8%) so they hold their shape over imagery and canvas. Primary buttons stay borderless — the fill is the boundary. One primary action per section stands.
- **Sidebar background** uses a Paper-only `sidebar-bg` (#0F1016) between `surfaceContainerLowest` and `surfaceContainerLow`; add a semantic token when implemented. `border-subtle` (white 8%) stays a Paper-only semantic until the code has an equivalent translucent neutral.

### Player Bar

- Floating card, radius `xl`, 1px `border-subtle` edge, margins 12 off the content edges — not docked full-width.
- Three zones: now-playing (poster 2:3, never square-cropped; title + remaining time; favorite) | transport + integrated progress | icon-only cluster (queue, audio, subtitles, volume, fullscreen). No stop button; labels live in tooltips.
- Progress: 6px track with native-blurred sampling, times flanking the track, buffered layer, knob + time bubble on hover. Card progress bars share the 6px blurred track.
- Intro/outro media segments on the track: 9% white wash plus a 2px tick at each skip boundary. No amber fills, no always-visible skip buttons; the skip affordance appears only while the playhead is inside a segment.
- Queue/audio/subtitle popovers: radius `xl`, `surfaceContainer` fill, 1px `border-subtle`, no header titles, selected row = `primaryContainer` + `secondary` check, max-height with scroll fade + thin scrollbar. Volume slider is permanent (no expand animation).

### Control-Only Window

Minimum viable chrome around 400×580: ambient blurred-artwork backdrop (dimmed to 55%), flexible poster area (artwork grows with the window; never letterbox the UI), one-line episode title + series subline, 6px progress with persistent knob, ghost prev/next + 52px primary play/pause, icon-only queue/audio/subtitles + permanent volume slider pinned to the bottom. Static color extraction is the fallback if live blur is too expensive.

### Content Patterns

- **Long overviews**: clamp to 2 lines with an end-fade and a 展开/收起 toggle, shown only when the text actually overflows (measured, not character-counted). No animation.
- **Library browse**: grid/list segmented toggle; grid posters carry watch-progress bars; long titles single-line ellipsis; infinite scroll shows a loading indicator. List view lanes: index, 40×60 poster, title + episode count, year, rating (amber, tabular-nums), watch progress, favorite — fixed-width slots keep the lanes aligned.
- **Empty/loading/focus**: empty states are centered icon + message + one primary recovery action; loading uses flat breathing skeleton blocks; keyboard focus is a 2px `primary` outer ring.
- **Detail pages** share one skeleton (full-bleed hero with two-layer scrim — bottom fade to canvas plus a left darkening lane for the logo/legibility — action row, info columns, cast carousel, similar posters). 接下来观看 exists only on the series page, never on the single-episode page. Episode rows show watched check + 重看, in-progress bar + 继续, or plain 播放.

### Settings and Account Popover

- Settings is a floating modal card (radius `x2l`, `raised_high`, 1px `border-subtle`) over a dimmed blurred backdrop: 208px section nav with the shared active treatment (`primaryContainer` fill, `secondary` content), content column per section.
- Boolean settings use real switches (40×22 track, 18px knob) — never 开启/关闭 text buttons. Destructive actions (`退出登录`) render in `error` and sit right; recovery actions sit left.
- The Account Popover shows the current identity zero times in its switch list — the header is the server row (URL + copy + 已连接 badge); only alternative accounts are listed. The sidebar identity card carries the switch affordance (transfer-v icon), a structural 1px edge, and the human-readable `Jellyfin · 10.0.0.27` subline, not the raw device ID.

## 2026-09-09 Paper Controls Sync (组件库 02 按钮与控件)

**Controls implementation delivered; human visual acceptance pending.** Scope: the controls artboard only — buttons, icon buttons, filter/season pills, toggle, status tags, inputs, and interaction states. Transition animations are explicitly excluded from this delivery; the season-selector popover remains deferred. Where this section differs from older text above, this section wins for new work; `tokens.rs` remains authoritative for token values. The pill active treatment was synced back into the Paper file in both themes.

### Terminology

- Paper's "SECONDARY · 次按钮" is the quiet action button and maps to the code's **Tonal** variant. The code's `Secondary` variant keeps its existing meaning — the switch-group active chip — and is not the Paper secondary button.

### Size Ladder (统一规范 · 尺寸阶梯)

| Tier | Geometry | Radius | Font | Use |
|---|---|---|---|---|
| M button | H36, PX16, gap 8, icon 15px | `xl` (12) | 14px/600 (Tonal 500) | Primary and Tonal action buttons |
| S button | H32, PY7, PX12 | `lg` (8) | 12px | Compact in-row buttons (episode-card 播放, inline save) |
| Icon button | 36×36, icon 16px | `xl` (12) | — | Icon-only actions; carousel arrows stay an 18px link-style exception |
| Pill | H32, PY8, PX12, gap 7 | `lg` (8) | 12px | Filter/sort pills and the Season Pill |
| Toggle | 40×22 track, 18px knob | `full` | — | Boolean settings |
| Status Tag | H24, PX10, 6px dot | `full` | 12px/600 | Connection/remote-control status |

### Buttons and States

- Primary: implemented `primaryHover` (`#787df8` dark / `#5457e8` light) and `primaryPressed` (`#5562ce` dark / light `secondary` `#4f46e5`) in `tokens.rs`, replacing the 10% hover brightness lift.
- Disabled: `control` fill with `text.muted` content, non-interactive — supersedes the legacy 50% alpha scaling, for buttons and fields alike.
- Pressed has a distinct fill (Primary → `primaryPressed`; Tonal → `surfaceContainer`; glass → white/18%). Custom ControlButton styles also supply the actual icon and label colors.
- State transitions remain instantaneous. The Paper 175ms easing treatment is explicitly excluded from this implementation.

### Tonal (Paper Secondary Button)

- Global retarget: `surfaceContainerHigh` fill with a 1px `border-subtle` structural edge and `onControl` content at weight 500; hover uses `controlHover` fill with `onControlHover` content; pressed uses `surfaceContainer`. This applies to every Tonal action button (browse toolbar, settings, detail action rows); the Sidebar menu-action exception stays as scoped. Light mode uses the Paper light counterparts (`surface-container-high` fill, `light-outline-variant` edge) until a translucent light `border-subtle` equivalent is accepted.

### Icon Buttons

- Icon controls use radius `xl`; the reference geometry is 36×36 with a 16px icon, with existing scoped sizes retained where specified. Canvas controls use the neutral Tonal treatment. Hero glass applies over imagery only: white/10% fill and native blur(10), with no decorative border in any interaction state; hover white/24%, pressed white/18%. Its labels/icons are white and blur masks match the glass controls' radii. Functional keyboard focus remains separately visible. Primary actions stay solid and do not request a glass mask. Missing artwork uses ordinary opaque styles.

### Pills

- **Filter Pill**: default `surfaceContainerHigh` fill + 1px `borderSubtle` edge, 12px text. `PillActive` uses `primaryContainer` fill with `secondary` content; browse filters and season buttons now use the `Pill`/`PillActive` pair. Other switch groups retain their existing active treatments.
- **Season Pill**: same pill spec plus a trailing chevron. Detail pages now use one selected-season trigger and a bounded native popover; see [Accepted Paper Detail Composition](#accepted-paper-detail-composition).

### Toggle

- Implemented: 40×22 borderless track, 18px knob, off track `surfaceContainerHighest`, and disabled `control` track with muted knob. The 40px hit target remains.

### Status Tag

- Account and settings status displays use the shared `status_tag` constructor: vertically centered dot and label in H24, with Error mapping for failures. The legacy badge surface remains only for non-tag uses such as Quick Connect's large code/progress panels; it is not a second status-tag implementation.

### Inputs

- Accepted target: `surfaceContainerHigh` fill, radius `xl`, 1px `border-subtle` idle border, 12px text, mono stack for path/parameter values, and inline save buttons at S tier (`lg` radius). The sidebar search frame already uses radius `xl` and the Ctrl-K keycap exists.
- **Deliberate deviation from Paper**: field focus keeps the existing 1px `primary` inset border for any modality; the Paper 2px outer focus ring is not adopted for fields, and the outline-width encoding (1px functional field focus/error, 2px keyboard focus rings) is unchanged. The Paper file's focused-search drawing retains the 2px ring — the design file is not authoritative for this detail.

### Delivery and Verification

- New palette roles: `primaryHover`, `primaryPressed`, `borderSubtle`, `imageOutline`, and `sidebarBg`. Light `borderSubtle` preserves the code palette's `outlineVariant` value (`#e7ecf3`), rather than silently recoloring the existing role to Paper's different value.
- `sidebarBg` applies only to the full/compact shell Sidebar. Other `Block` consumers retain `surfaceContainerLowest`. `imageOutline` is available as a token; applying artwork outlines belongs to the separate artwork/card scope.
- Scoped Sidebar menu actions remain borderless at rest; their Catalog radii survive unless a caller explicitly uses `.radius(...)`.
- Post-review verification: `bun run check` and `bun run task rust test iced` passed. Workspace Rust tests passed before the review fixes; the final fixes were rechecked in the affected iced group. No application visual acceptance is claimed.

## Accepted Paper Home Composition

The [Home specification](home-hero-design-spec.md#accepted-paper-home-synchronization) supersedes the older Home/player/Sidebar geometry above. Human visual acceptance remains separate from code-level verification.

- Hero foreground: fixed 440 logical pixels, lower-left identity/actions and lower-right compact selection rail. The original-aspect, full-width Backdrop is independently sized and scrolls with Home; Logo remains preferred over text.
- Keep real Home sections and independent direct resume. Continue Watching may require vertical scrolling; selection never explicitly scrolls the page vertically.
- Hero Favorite/Watchlist target the movie or episode's parent series. The player-bar Favorite independently targets Now Playing's movie/series. Unknown target/status disables mutation with an explanatory hint.
- Sidebar: expanded 220, compact 72; expanded padding 16 vertically/12 horizontally, gaps 8, 38-pixel navigation/library/tool targets. Account trigger is 54 high; Account Popover is 320 wide, radius 16, padding 10.
- Player bar remains docked, with metadata/Favorite, transport/seek, and queue/audio/subtitle/volume/fullscreen controls. Preserve direct Stop; reflow at narrow widths rather than discarding controls.
- Fullscreen operates the current playback backend. External MPV receives its fullscreen command. Embedded playback uses the scoped composition below, preserving the concealed browser's widget state when fullscreen is entered from browsing.
- Dark and light themes resolve separately through semantic tokens/Catalogs. Hero glass controls stay borderless, including disabled states; keyboard focus remains distinct.

## 2026-09-09 Embedded Player

**Implemented; human visual acceptance pending.** References: [Desktop - 播放器 Player](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/1-0/5QE-0) and [responsive player frames](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/4-0/735-0), not the neighboring Apple Blur variant. Scope is embedded playback only; the browser's docked playback bar and external Control-Only screen retain their existing treatment.

- Windowed and fullscreen playback share uncropped, aspect-preserving video and the same controls. The standalone player omits the Sidebar and duplicate docked bar. Back stops successfully before restoring the source page and leaving fullscreen.
- Floating chrome uses scoped `jellypilot-ui::widgets::embedded_player` Catalog styles, independently of the browser theme. Wide composition uses 36px outer insets, 24px horizontal/20px vertical card padding, and `x2l` radius. The 14px timeline row, 16px gap and 48px controls row produce a 118px card. At ≤900 logical pixels the thumbnail is hidden and the card is 114px tall; the single controls row and centered transport remain through the 768px reference. Only smaller widths reflow controls; below 600px, outer insets reduce to 12px.
- The left identity uses real 85×48 landscape artwork, the unprefixed media title, series/episode metadata and remaining time. Metadata truncates within symmetric side slots, without displacing transport. The title/chevron queue trigger is content-sized with 6px horizontal and 4px vertical padding around its 20px line, giving a 28px-high target. The subtitle is separately aligned with the title text and is not clickable; unused side-slot width remains noninteractive. There is no duplicate queue icon on the right. Audio/subtitle menus remain anchored to their controls; Intro skip/dismiss appears above the card.
- Primary play/pause is 44×44 with an 18px glyph; previous/next have 40px targets and 22px glyphs. Framed Back/audio/subtitle/fullscreen controls use the reference's 36px targets, 12px radius and centered 16px glyphs. The [Right Zone reference](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/1-0/6Y8-0) retains its 251×36 extent and 8px visual gaps: audio, subtitles, volume, a 1×18 separator, and fullscreen. Volume retains the 18px glyph and 84px rail. Its 34×36 mute target incorporates the 8px whitespace on each side of the glyph without moving the glyph or rail or overlapping adjacent targets. The square-headband and sound-wave glyphs use the supplied reference geometry, leaving external-player icons unchanged.
- Centered control buttons apply spacing only between actual icon/label content, not between the content and its centering spacers. Glyph dimensions and pointer-target dimensions are independent.
- Embedded volume follows the [four-state reference](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/4-0/7GC-0): crossed speaker for mute or zero volume, bare speaker through 33%, one wave through 66%, two waves above 66%. These numeric boundaries are an implementation choice; the reference does not specify them. Drag previews update the glyph immediately. All states remain 18px within the unchanged 34×36 target; rest uses white at 85%, hover/press uses white at 100%, and neither paints a background block. Keyboard focus indication remains available.
- Timeline uses a 4px track and persistent 14px indigo knob with a 2px white border; volume uses an 84px white 4px track and reveals its small knob on hover/drag. Timestamps use monospace type. No fabricated buffering or chapter markers are shown when real data is unavailable.
- **Explicit user decision:** the card uses rgba(24,24,30,0.82) without blur for now. This is a scoped exception to the general no-translucency-without-blur rule. Top 160px/bottom 340px video scrims are restored. Borders retain the documented opaque preblend approximation; no new blur pipeline is applied.
- Controls hide after three idle seconds during playback. Hidden transport reveals only near the bottom control area, including a 48px approach band derived from the responsive card height. Movement over the picture neither reveals nor prolongs the bar. Back has an independent top-left reveal region and timeout. The cursor reappears on any pointer motion and has its own three-second idle deadline, independent of either control region. Pause, drag, open menus and intro prompts hold controls and cursor visible. Seek/volume feedback expires independently. Timeline hover shows time, dragging previews a target, and release submits one seek; no video-thumbnail generation is introduced.
- Left/Right seek five seconds, Up/Down change volume five percentage points within 0–100%, and held arrows repeat. F toggles fullscreen without repeating; Esc leaves fullscreen after menu/modal priority. Space or one video click toggles pause; no double-click action. Browsing and captured input do not trigger these player keys.
- Windowed Settings is composed above the player. Search is not opened into an absent Sidebar; fullscreen does not open hidden Settings. Busy controls do not begin new slider drags, but an existing drag retains release delivery through asynchronous commands.
- Slider drag lifetime follows actual pointer press/release, including pressing the existing knob without changing its value. Keyboard/wheel adjustments submit immediately without holding a drag open. An actual fullscreen transition cancels unfinished previews and resets the slider's private interaction state, so a later release cannot submit a cancelled target. Clicking the picture while a player menu is open dismisses the menu without toggling playback.

Human acceptance: check windowed/fullscreen video proportions, 1099/900/768/400px layouts, true transport centering, thumbnail removal at ≤900px, title-triggered queue and track menus, readable translucent chrome and scrims under both browser themes, bottom-only reveal and independent Back reveal, three-second hiding/cursor recovery, hover/drag times, held arrows, F/Esc, and Back restoring the source page after playback stops. Headless layout tests measure actual card/control bounds and exercise pointer events; native smoke proves startup, not appearance or actual media presentation.

## Accepted Paper Detail Composition

**Implemented; human visual acceptance pending.** References: [single episode](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/1-0/102-0), [series](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/1-0/2BU-0), and [movie](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/1-0/2I6-0). This section supersedes older Detail geometry and scrim descriptions, not the Home or player compositions.

- The hero is full-bleed within the content lane, 520 logical pixels high, with centered `Cover` artwork. Unlike actual playback video, decorative Detail Backdrops are deliberately cropped. Logos preserve their intrinsic aspect near the lower-left; unavailable logos fall back to the real title.
- Two independent canvas-colored scrims provide legibility: bottom fade (top-to-bottom alpha 0.30, 0.28 at 45%, 0.72 at 78%, 1.0 at the bottom) and left darkening (0.55 at the left to transparent at 55% width). Both resolve through the active theme.
- Back sits 18px from the top/left, with native 10px blur sampled from the same full hero frame, a scoped 40% canvas tint, and no decorative border. Its localized label, keyboard focus and disabled behavior remain sharp and independent of the blur.
- Metadata, overview and actions live **below** the hero on Canvas. Sections start with 28px top spacing and 36px horizontal insets; content widths below 600px use 18px insets. Metadata is 14px/18px; the overview is 16px/24px. Actions use the 36px M tier with 10px inter-button spacing and wrap instead of disappearing.
- Overview disclosure measures actual shaped text, clamps to two lines with an end fade, and exposes More/Less only on overflow. Expanding text moves subsequent content without resizing the hero. These prose-disclosure links are not primary action buttons.
- Summary information uses a 360px genre/creator column and a remaining-width cast summary separated by 40px, stacking when narrow. The summary lists four real cast credits plus a localized remaining count.
- Series pages retain Next Up and replace the horizontal season-button strip with one 32px Season Pill and a bounded, keyboard-operable popover. Choosing the current season closes the menu without reloading; changing seasons preserves exact server IDs. Pending season loads disable selection, and leaving/reloading Detail closes the menu.
- The episode reference contains series-level sample copy; production episode pages keep the actual episode identity and existing current-season neighbor navigation rather than presenting that sample as real data. Next Up remains series-only. Movies have no episode shelf. All three types include Cast and Similar; file-level media information remains available after those core sections.
- Episode rows are flat, with 300×169 artwork, 20px between lanes, 16px titles, 12px metadata and 12px/20px two-line overviews. Below 720px content width, rows stack without losing actions. Play, Resume and Replay retain their actual playback start semantics. Watched rows show a check; in-progress artwork carries the reference's **4px** progress rail, a Detail-specific exception to the older generic 6px target.
- Cast cells are 96px wide with 72px round portraits, 24px spacing, wrapped 12px names/roles, and honest missing-art placeholders. Jellyfin and Emby supply real names, roles and tagged portrait references; no reference assets or fabricated credits are bundled. Each credit has a separate visibility slot even when multiple roles share an image, while the underlying image cache remains shared.
- Similar cards are 150×225 posters with 18px spacing and real title/year/type metadata. Both Cast and Similar scroll horizontally, retain measured visibility admission, and show explicit empty states. Decorative outlines and placeholders preserve the requested image box; they must not inflate an unbounded scroll layout.
- Premiere dates preserve the server's calendar date rather than shifting midnight through the desktop time zone. Missing dates, portraits and roles remain absent or use the established empty presentation.

Human acceptance: compare a movie, series and episode against the references in both themes; check hero crop/logo/scrims, below-hero copy, season popover and its dismissal, Next Up only on series, Play/Resume/Replay, long-text disclosure, real cast photos/roles, horizontal shelves, and narrow-width reflow. Headless tests exercise layout and pointer/state behavior; they do not establish visual acceptance.
