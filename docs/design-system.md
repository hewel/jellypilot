# JellyPilot Design System

JellyPilot’s durable design contract is **UI Core** (`@jellypilot/ui`) plus **Atomic CSS** (`@jellypilot/atomic-css`). App Composition owns product routes and media workflows; it does not invent parallel primitives or utility owners.


Authoritative domain terms live in `CONTEXT.md`. Architecture decisions: ADR 0011 (UI Core), ADR 0012 (Atomic CSS), ADR 0013 (theme).

## Principles

- **Desktop media companion**: deliberate surfaces for a Jellyfin Playback Target — not generic mobile Material or decorative “AI taste”.
- **One UI contract**: Theme Presets, Recipes, and Component Families live in UI Core; low-level utilities compile through Atomic CSS only.
- **Package-neutral UI Core**: no Tauri, Effect, TanStack Router, or media-provider dependencies inside `@jellypilot/ui`.
- **No fake state**: never show fake media artwork, fake playback progress, or pretend controls.
- **Accessible by default**: normal text contrast ≥ 4.5:1; large text and meaningful icons ≥ 3:1. Good-enough desktop a11y: keyboard operation, focus, escape/outside close for Layers, disabled states, roles/labels.

## Astryx Baseline and Solid adaptations

- Baseline: Astryx **v0.1.4** commit `7b76c68ae07cbc798e3b7f4125eb99ec596f0779` (MIT).
- Parity means selected-family visual, variant, interaction, keyboard, and accessibility parity after Solid adaptations — not React API or pixel parity.
- Global Solid adaptations:
  - DOM-aligned property names (`class` / `classList` / `style` / refs / ARIA); no `className` or StyleX.
  - Controlled durable values and visibility; private trigger-derived popup state for Selector, Menu, Tooltip, HoverCard.
  - Callbacks receive next value first, typed reason/event details second.
  - Icons via JSX slots (no bundled icon registry).
  - Button is action-only; Link is navigation-only with one UIRoot-level navigation adapter (native anchor default).
- Selective algorithm adaptations (focus trapping, collection navigation, layer dismissal, etc.) keep MIT attribution and are marked in package provenance docs when implemented.
- Map Astryx TabList → Tabs, DropdownMenu → Menu. UIRoot is a JellyPilot addition.

## Selected v1 Component Families

| Group | Families |
|---|---|
| Foundation | UIRoot, Theme, Text, Heading, VisuallyHidden, Link |
| Actions / display | Button, IconButton, ToggleButton, Card, Badge, Spinner, Skeleton, ProgressBar |
| Forms / selection | TextInput, TextArea, CheckboxInput, Switch, Slider, Selector, SegmentedControl, Tabs |
| Disclosure / layers | Collapsible, Dialog, AlertDialog, Popover, Menu, HoverCard, Tooltip |
| Feedback | Toast (`useToast`) |

One typed registry drives public exports, catalog navigation, and parity tracking.

## Stable selectors

- Public tests and catalog automation use **documented `data-*` attributes** only.
- Generated Atomic CSS classes and private DOM nesting are not part of the public contract.
- Prefer user-observable roles, names, state, focus, and callbacks over CSS implementation details.

## Theme Preference and Theme Presets

- **Theme Preference**: durable `system | light | dark` in App Composition config (default `system`).
- **Resolved Theme**: concrete `light | dark` applied at the document root (e.g. `data-theme`), including OS updates while Preference is `system`.
- **Theme Presets**: complete build-time Neutral and JellyPilot descriptors. Neutral uses system fonts; JellyPilot owns Figtree.
- Nested Theme scopes propagate through Layers/portals.
- Appearance: Settings control + authenticated-shell cycle (System → Light → Dark → System).
- Startup: do not mount routed content before config loads; config failure shows a stable System-themed retry surface.

## Atomic CSS authoring

- Style with `atomic({ ... })` object markers in style modules. The root marker throws at runtime if the compiler plugin is missing: `atomic() must be compiled by pluginAtomic()`.
- Source Preset: `@unocss/preset-mini` `66.7.4` via a private Preset Adapter into Atomic Schema (not full preset parity).
- Project Theme (UI Core semantic tokens/conditions) merges through one workspace configuration seam.
- Recipes own semantic variants (size, tone, orientation). Atomic utilities supply stable non-conflicting base/layout rules inside Recipes.
- Local unlayered vanilla-extract declarations override atomic utilities.
- At most one `atomic()` call per class composition.
- Component/app tests assert behavior and documented selectors — **not** generated class names. Exact CSS/manifest checks belong in atomic compiler fixtures.

### First-release Atomic limits (explicit)

Unsupported in v1: full preset-mini surface, arbitrary selectors, group/peer/parent/`has`, custom breakpoints, container/supports/print/motion/contrast/orientation variants, shared-variable shadow/ring/filter/gradient systems, per-entry pruning, non-Rsbuild hosts.

## Accessibility target

- Keyboard-operable families; predictable focus in menus, tabs, selectors, dialogs.
- Escape and outside dismissal affect only the topmost eligible Layer.
- Focus restored after a Layer closes.
- Reduced-motion: nonessential motion reduced.
- Catalog usable at ≥360px; JellyPilot routes accepted at ≥1280×720.

## Dependency boundary

| Package | May depend on | Must not depend on |
|---|---|---|
| `@jellypilot/ui` | solid-js, vanilla-extract, `@jellypilot/atomic-css`, Floating UI, preset fonts | Tauri, Effect, TanStack, media providers, App Composition |
| `@jellypilot/atomic-css` | pinned host peers, preset-mini (adapter), compile-time tooling | Browser runtime CSS machinery, UI Core components |
| App Composition | public UI Core and Atomic CSS entrypoints | UI Core private modules, compiler internals |

## Completed cutover

- UI Core owns the Project Theme through the public `@jellypilot/ui/theme/project` style entrypoint.
- Atomic CSS owns the only low-level utility compiler; its private preset adapter is the sole `@unocss/preset-mini` consumer.
- App Composition imports public UI Core primitives or style entrypoints only. Ark, Sprinkles, local generic primitives, and app-local theme owners are removed.

## Out of scope

- Installing Astryx packages or StyleX.
- Another headless UI dependency to replace Ark.
- Pixel-perfect Astryx reproduction.
- Runtime theme creation or runtime CSS injection.
- Automated external-MPV playback and pixel-baseline screenshot infrastructure.
