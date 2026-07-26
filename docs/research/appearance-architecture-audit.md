# Appearance Architecture Audit

Research for [Audit the current appearance architecture and retired theme work](https://github.com/hewel/jellypilot/issues/163), against JellyPilot `main` at `b99905dd2aed241ac98a3f4e731c83db0317e831` and the local Braun reference at `/home/hewel/Projects/braun-ui-kit`.

## Current architecture

### Styling and token ownership

JellyPilot has one active appearance: Control Room Dark.

- [ADR 0011](../adr/0011-panda-css-styling.md) makes Panda CSS the sole styling system. Panda owns token literals, application globals, and shared keyframes; owner-local `.styles.ts` modules consume `@styled-system/*`. Legacy vanilla-extract, Sprinkles, Recipes, UnoCSS, and authored `--jellypilot-*` variables are explicitly outside the active architecture.
- [`src/styles/theme-tokens.ts`](../../src/styles/theme-tokens.ts) is the canonical literal source. It defines one dark raw palette, one semantic color-role map, one typography stack, and the shared spacing, type, radius, shadow, motion, z-index, and breakpoint scales.
- [`panda.config.ts`](../../panda.config.ts) converts those literals into Panda core and semantic tokens. It uses `presets: []`, `strictTokens: true`, `cssVarRoot: ':root'`, and one `theme` object. It has no Panda `themes` collection and no conditional light/dark values.
- Panda global CSS paints `body` from `background` and `onSurface`, applies the shared sans font, and neutralizes motion under `prefers-reduced-motion`. There is no appearance provider or theme-specific global selector.
- [`docs/design-system.md`](../design-system.md) remains the active visual contract: desktop-first Control Room, dark-only OLED surfaces, selective glass for overlays, semantic status colors, local Inter/Space Grotesk fonts, Ark UI behavior, and supported native Tauri viewport targets. Light mode is explicitly listed as out of scope in that current document.

The completed Panda migration and later visual cleanup are both ancestors of current `main`:

- [`refactor(ui): complete Panda styling cutover`](https://github.com/hewel/jellypilot/commit/5e2b9b50bf61c8befda530294b49b99139808ab3) removed the legacy styling stack and made the current Panda ownership authoritative.
- [`style(design): replace glow and gradient signatures with flat Control Room surfaces`](https://github.com/hewel/jellypilot/commit/d30bf6d15e421f9445f15c5c575b152b4ecf33a6) deliberately removed decorative glow, gradients, and most blur while retaining image fades, overlay glass, focus rings, skeletons, and meaningful waiting indicators.

### Root application and first paint

There is no current root appearance state.

- [`src/index.tsx`](../../src/index.tsx) imports bundled Inter and Space Grotesk assets, imports the generated application CSS, and immediately calls `mountApplication()`.
- [`src/mountApplication.tsx`](../../src/mountApplication.tsx) synchronously renders `<App />` into `#root`; it has no configuration or appearance gate.
- [`src/routes/__root.tsx`](../../src/routes/__root.tsx) owns the scroll shell only. It does not add theme attributes, classes, context, or a provider.
- A repository search found no application use of `data-theme`, `data-panda-theme`, `ThemePreference`, `themePreference`, or an appearance store. The generated document therefore starts with the sole Control Room token table; there is no wrong-theme flash today because no alternate can be selected.

Adding selectable appearance creates a first-paint requirement that does not currently exist. Applying a persisted choice only after Solid mounts would permit the default token table to paint first. The later persistence decision must therefore choose an explicit pre-mount or stable-gate strategy and prove it in native Tauri startup, not assume normal component reactivity is early enough.

### Persistence seams

Two active persistence mechanisms are available, but neither currently owns appearance:

- [`src-tauri/src/config.rs`](../../src-tauri/src/config.rs) and the generated [`AppConfig`](../../src/bindings.ts) define typed application configuration. They contain no design-theme or color-mode fields. [`src/effects/config.ts`](../../src/effects/config.ts) exposes typed Effect wrappers for fetching and saving the complete configuration.
- [`src/utils/sidebarPreferences.ts`](../../src/utils/sidebarPreferences.ts) is a smaller frontend preference pattern using Tauri Store's `preferences.json`, a shared Solid signal, best-effort hydration, and a serialized write queue. The app-shell tests prove both write-through and restoration for the sidebar state.

Neither seam should be selected by this audit. The persistence/first-paint ticket must compare lifecycle, failure truthfulness, write coordination, generated binding cost, and availability before the first routed frame. In particular, simply copying the sidebar preference utility would not by itself solve first paint.

### Shared components and tests

- Shared visual behavior belongs in components under `src/components/ui`; cross-component private style imports and broad style barrels are forbidden by current guidance.
- Ark UI remains the interaction owner for dialogs and other primitives. For example, [`SettingsModal.tsx`](../../src/components/SettingsModal.tsx) composes Ark Dialog, Solid Portal, JellyPilot Button, and owner-local Panda styles. Appearance controls must extend this stack rather than revive local replacements.
- [`tests/token-contract.test.ts`](../../tests/token-contract.test.ts) protects the exact current Control Room palette and scale. It does not exercise theme selection or runtime switching.
- [`docs/agents/style-tests.md`](../agents/style-tests.md) permits theme tests as a high-value exception but still prefers user-observable state, accessibility, and usability over generated classes or exact implementation details.

## Retired theme work

Three useful implementations exist only in abandoned branch history:

1. [`feat(theme): persist app theme preference`](https://github.com/hewel/jellypilot/commit/850efe04835c36086206b901a2c32bdcb33f31bd) added `ThemePreference = system | light | dark` to Rust `AppConfig`, resolved System through `matchMedia`, and wrote a resolved `data-theme` plus `color-scheme` to the document root.
2. [`feat(config): theme preference and first-paint config gate`](https://github.com/hewel/jellypilot/commit/b0b1564a7c9c4bb2ebe85ae4e586b349044b7312) added a configuration coordinator with confirmed and desired snapshots, a single in-flight save, latest-pending-intent coalescing, rollback on final failure, and a boot gate that withheld routed content until configuration loaded.
3. [`feat(shell): cut over appearance controls and shell dialogs`](https://github.com/hewel/jellypilot/commit/dfd8c52f55ce07002c8c0753016f8bffa29469df) added a System/Light/Dark cycle control and Settings appearance UI on the abandoned UI Core stack.

None of these commits is an ancestor of current `main`. The associated [UI Core pull request](https://github.com/hewel/jellypilot/pull/138) was closed without merge. The tracker record is unambiguous:

- [PRD: Reforge JellyPilot design system and remove Ark UI](https://github.com/hewel/jellypilot/issues/93) and [Persist Light/Dark/System Theme Preference](https://github.com/hewel/jellypilot/issues/94) were superseded during the UI Core program.
- [Coordinate app configuration and first-paint theme state](https://github.com/hewel/jellypilot/issues/123) records the branch implementation but is historical evidence only.
- [PRD: Solid UI Core — Astryx v0.1.4 component library and atomic JellyPilot adoption](https://github.com/hewel/jellypilot/issues/103) was explicitly abandoned on 2026-07-15. Its closure says its branch, decisions, acceptance criteria, and previously completed children must not guide new work.
- The later [Panda migration map](https://github.com/hewel/jellypilot/issues/139) targeted unchanged `main`, prohibited reviving UI Core, preserved Ark, and intentionally excluded redesign and light mode. Its resolved [token ownership decision](https://github.com/hewel/jellypilot/issues/142) is the direct ancestor of today's Panda-only architecture.

The abandoned code is therefore a set of implementation experiments, not a base to merge or a contract to restore.

## Braun reference

The local Braun kit is a React 19 + Tailwind 4 + Vite specimen, not a JellyPilot-compatible package.

- `/home/hewel/Projects/braun-ui-kit/design.md:11-18` defines the useful design intent: physical depth, meaningful LEDs, restrained Braun orange, paired human/readout typography, and motion only in response to intent.
- `/home/hewel/Projects/braun-ui-kit/design.md:44-83` and `src/index.css:23-65` implement one semantic variable vocabulary with separate light and dark value tables. This is strong evidence that the Braun language can support both modes without changing component semantics.
- `/home/hewel/Projects/braun-ui-kit/design.md:87-125` defines hardware status colors and named material concepts such as recessed wells, raised plates, acrylic panels, tactile keycaps, and rotary controls.
- `/home/hewel/Projects/braun-ui-kit/design.md:163-203` describes restrained motion, reduced-motion handling, keyboard models, native form participation, and test intent. These are claims and examples from the specimen, not proof of JellyPilot accessibility or WebKit behavior.
- `/home/hewel/Projects/braun-ui-kit/src/App.tsx:8-28` stores `light | dark` in browser `localStorage`, defaults to light, and toggles `.light`/`.dark` after React effects run. That mechanism is not suitable evidence for JellyPilot's Tauri persistence or first-paint requirements.
- `/home/hewel/Projects/braun-ui-kit/package.json:15-36` confirms the implementation is React/Tailwind/Vite. Its `className` APIs, Tailwind utility strings, custom ARIA widgets, and global material classes cannot be copied into JellyPilot's Solid/Ark/Panda architecture.

The reference has no root license file, is not a Git worktree with traceable provenance, and contains no bundled font files or reusable image assets. Its `index.html` loads Archivo and JetBrains Mono from Google Fonts at runtime. JellyPilot's current contract instead bundles local Fontsource fonts and forbids network font imports. Consequently:

- concepts, measurements, palette candidates, and interaction questions may be studied;
- source code, CSS, and named assets must not be copied until provenance and reuse permission are explicitly established;
- any chosen Braun fonts must be acquired as locally bundled, independently licensed dependencies, with their exact weights and bundle cost verified;
- the reference's statement that both modes meet WCAG AA must be independently measured in JellyPilot rather than inherited as fact.

The live preview at `http://localhost:5173/` is useful for visual comparison, but JellyPilot acceptance must occur in native Tauri/WebKit. The preview is not a verification environment for production behavior.

## Reusable findings

The following findings can safely guide downstream decisions without reviving the abandoned stack:

1. **Keep two independent appearance axes.** The planned contract is Design Theme (`control-room | braun`) plus Color Mode (`light | dark`). The retired single `ThemePreference` enum cannot represent four combinations and its `system` value conflicts with the settled scope.
2. **Preserve semantic consumption.** Components should continue consuming semantic Panda roles. The four appearance combinations should change values and narrowly defined material/component variants, not force raw palette use or per-route color branches.
3. **Keep one root truth.** Both axes need observable root state so normal descendants and portaled Ark content inherit the same appearance. The exact selector/Panda theme mechanism remains a downstream decision.
4. **Reuse coordination invariants, not branch code.** Confirmed-versus-desired state, latest-intent-wins writes, truthful failure handling, and a stable first frame are valuable requirements demonstrated by the retired coordinator. They need a fresh implementation against current Effect, Solid, Panda, Ark, and Tauri APIs.
5. **Treat Control Room Dark as the compatibility baseline.** Its current semantic values, desktop behavior, and token tests define the fallback. Control Room Light is new design work, not a mechanical inversion.
6. **Adapt Braun as a bounded material language.** Meaningful indicators, tactile press states, recessed/raised hierarchy, Braun orange, dual typography, and reduced motion are promising. Expensive blur, decorative gradients/glow, custom widget behavior, and global utility classes need selective prototypes rather than wholesale adoption.
7. **Keep persistence undecided until the first-paint boundary is proven.** Typed Rust `AppConfig` and Tauri Store are both viable inputs. The selected solution must be readable early enough to avoid a wrong-theme frame and must have explicit default, corruption, and save-failure behavior.
8. **Make tests follow the product contract.** Add token completeness/default tests, root-state switching and persistence behavior, accessible appearance controls, and sparse layout/theme evidence. Do not assert generated Panda class names or copy the Braun kit's implementation tests.

## Superseded findings

These historical choices must not be carried forward:

- System mode or `prefers-color-scheme` as a third user mode.
- A single Light/Dark/System `ThemePreference` field as the complete appearance model.
- UI Core, Atomic CSS, vanilla-extract, Sprinkles, Recipes, local Ark replacements, Figtree, or an Astryx-derived component package.
- Restoring the retired `ThemeSync`, `ConfigGate`, `ConfigCoordinatorProvider`, or `ThemeCycleControl` files by merge/cherry-pick.
- Directly porting React components, Tailwind utilities, `className` contracts, custom ARIA widgets, global material classes, or browser `localStorage` from the Braun kit.
- Network-loaded fonts or assuming the local kit's missing license permits source/asset reuse.
- Treating current Control Room tokens as dark values that can be naively inverted into an acceptable light design.
- Browser-preview validation as a substitute for native Tauri/WebKit evidence.

## Downstream constraints

Later Wayfinder tickets should treat the following as fixed inputs:

- The default and failure fallback is Control Room Dark.
- The selectable matrix is exactly Control Room Light, Control Room Dark, Braun Light, and Braun Dark.
- Panda remains the sole styling engine and semantic token owner; owner-local style modules and current shared-component boundaries remain in force.
- Ark UI remains the interaction primitive layer. Appearance work must preserve its focus, keyboard, portal, and dismissal behavior.
- The external MPV window and OS-native tray/menu styling remain outside the appearance matrix. Supported Tauri native window chrome is handled by the dedicated native-chrome research ticket.
- Existing Control Room Dark behavior and current production viewports are regression baselines. The documented 800×600, 640×720, and 360×720 sizes are review stress targets.
- Appearance must be restored before routed content can visibly paint in the wrong combination.
- Theme switching must immediately update the whole application, including portaled overlays, while persistence failure remains truthful and recoverable.
- Braun fonts and any copied implementation details remain blocked on independent provenance/license resolution; concept-level adaptation is allowed.
- The new token/material contract must account for every semantic role in all four combinations and retain accessible status meaning without color alone.
- Runtime acceptance belongs in native Tauri/WebKit with reduced-motion, keyboard, contrast, responsive, and performance evidence.

This audit did not uncover a missing investigation ticket. Persistence/first paint, token/material structure, Braun component boundaries, native chrome, representative prototypes, and native accessibility/performance are already represented by the existing map.

## Sources

### Current repository

- [`panda.config.ts`](../../panda.config.ts)
- [`src/styles/theme-tokens.ts`](../../src/styles/theme-tokens.ts)
- [`docs/design-system.md`](../design-system.md)
- [`docs/adr/0011-panda-css-styling.md`](../adr/0011-panda-css-styling.md)
- [`docs/agents/style-tests.md`](../agents/style-tests.md)
- [`src/index.tsx`](../../src/index.tsx)
- [`src/mountApplication.tsx`](../../src/mountApplication.tsx)
- [`src/routes/__root.tsx`](../../src/routes/__root.tsx)
- [`src-tauri/src/config.rs`](../../src-tauri/src/config.rs)
- [`src/bindings.ts`](../../src/bindings.ts)
- [`src/effects/config.ts`](../../src/effects/config.ts)
- [`src/utils/sidebarPreferences.ts`](../../src/utils/sidebarPreferences.ts)
- [`src/components/SettingsModal.tsx`](../../src/components/SettingsModal.tsx)
- [`tests/token-contract.test.ts`](../../tests/token-contract.test.ts)
- [`tests/app-shell.test.tsx`](../../tests/app-shell.test.tsx)

### History and tracker

- [Closed, unmerged UI Core pull request](https://github.com/hewel/jellypilot/pull/138)
- [PRD: Reforge JellyPilot design system and remove Ark UI](https://github.com/hewel/jellypilot/issues/93)
- [Persist Light/Dark/System Theme Preference](https://github.com/hewel/jellypilot/issues/94)
- [PRD: Solid UI Core — Astryx v0.1.4 component library and atomic JellyPilot adoption](https://github.com/hewel/jellypilot/issues/103)
- [Coordinate app configuration and first-paint theme state](https://github.com/hewel/jellypilot/issues/123)
- [Wayfinder: Migrate JellyPilot's styling engine to Panda CSS](https://github.com/hewel/jellypilot/issues/139)
- [Choose token ownership and CSS-variable compatibility](https://github.com/hewel/jellypilot/issues/142)
- Retired commits [`850efe0`](https://github.com/hewel/jellypilot/commit/850efe04835c36086206b901a2c32bdcb33f31bd), [`b0b1564`](https://github.com/hewel/jellypilot/commit/b0b1564a7c9c4bb2ebe85ae4e586b349044b7312), and [`dfd8c52`](https://github.com/hewel/jellypilot/commit/dfd8c52f55ce07002c8c0753016f8bffa29469df)

### Local Braun reference

- `/home/hewel/Projects/braun-ui-kit/design.md`
- `/home/hewel/Projects/braun-ui-kit/src/index.css`
- `/home/hewel/Projects/braun-ui-kit/src/App.tsx`
- `/home/hewel/Projects/braun-ui-kit/src/types.ts`
- `/home/hewel/Projects/braun-ui-kit/index.html`
- `/home/hewel/Projects/braun-ui-kit/package.json`
