# Tauri Window-Chrome Theming

Research for [Research cross-platform Tauri window-chrome theming](https://github.com/hewel/jellypilot/issues/164), against JellyPilot `main` at `3f83f1a0ebfe0eeda25e737307baa981825a02d1`, Tauri `2.11.2`, and `tauri-utils` `2.9.2`.

## Conclusion

JellyPilot should keep native window decorations on Linux, Windows, and macOS. Its supported appearance contract is:

1. JellyPilot fully owns the webview client area through Panda and Solid.
2. JellyPilot synchronizes the selected Color Mode (`light | dark`) to Tauri's native window theme.
3. JellyPilot synchronizes an **opaque** window/webview background color to the selected appearance's canvas color before the window becomes visible.
4. The operating system, desktop environment, or window manager owns the exact title-bar material, control glyphs, spacing, border, corner, and shadow.
5. Design Theme (`controlRoom | braun`) does not promise a different native title-bar construction. It affects JellyPilot-rendered content and the opaque canvas underlay; Color Mode is the only native-decoration input.

This provides a consistent light/dark relationship without replacing platform controls or promising pixels Tauri does not control.

## Current JellyPilot state

- [`src-tauri/tauri.conf.json`](../../src-tauri/tauri.conf.json) declares one configuration-created main window at 1600×900 with a 1280×720 minimum. It does not set `create`, `visible`, `decorations`, `transparent`, `theme`, `titleBarStyle`, `shadow`, or `backgroundColor`, so Tauri defaults apply.
- Tauri's generated `2.11.2` configuration schema defaults `create`, `visible`, `decorations`, and `shadow` to `true`; `transparent` defaults to `false`; `theme` and `backgroundColor` are unset.
- [`src-tauri/src/lib.rs`](../../src-tauri/src/lib.rs) loads persisted application configuration in `setup`, after the configuration-declared window has been created. It does not fetch the main window or set native appearance before setup completes.
- The current close path hides that same window to the tray. Future appearance coordination therefore must handle both initial reveal and later re-show; it must not treat every tray restore as a new window.
- [`src-tauri/capabilities/default.json`](../../src-tauri/capabilities/default.json) permits frontend `hide`, `show`, focus, and size operations, but does not grant `core:window:allow-set-theme` or `core:window:allow-set-background-color`.
- The maintained native WebDriver harness is Linux-only. It can prove startup ordering and the Linux contract, but Windows and macOS native-decoration behavior requires platform builds and spot checks.

## What Tauri can control

Tauri exposes three distinct layers that must not be conflated.

### JellyPilot-rendered client area

HTML/CSS owns the webview's document content. Panda can render all four planned appearances consistently inside that client area, including JellyPilot-owned headers, dialogs, sidebars, overlays, and focus states.

This is the only layer where the project can promise exact design tokens and component materials across all three desktop platforms.

### Native window hint and opaque underlay

Tauri `2.11.2` exposes:

- [`WebviewWindow::set_theme`](https://docs.rs/tauri/2.11.2/tauri/webview/struct.WebviewWindow.html#method.set_theme), accepting Light, Dark, or reset-to-system. On Linux and macOS this setting is app-wide rather than window-specific.
- [`WebviewWindow::set_background_color`](https://docs.rs/tauri/2.11.2/tauri/webview/struct.WebviewWindow.html#method.set_background_color), which sets the window background and, where supported, the webview underlay.
- Configuration-time `theme` and `backgroundColor` fields in Tauri's [`WindowConfig`](https://docs.rs/tauri-utils/latest/tauri_utils/config/struct.WindowConfig.html).

These APIs are enough for a safe native appearance contract:

- pass explicit Light or Dark, never System, because System mode is outside this map;
- use an opaque appearance canvas color so startup, resize gaps, and the native window layer do not flash a contradictory default;
- keep HTML's `color-scheme` and Panda root state synchronized with the same Color Mode.

`set_theme` is a platform appearance hint, not a title-bar palette API. It can align native controls with Light or Dark, but it does not let JellyPilot specify title-bar hex colors, glyphs, control positions, or compositor materials.

### Native decorations and effects

Tauri can enable or disable decorations through `decorations`, but disabling them removes the platform title bar and borders. Tauri's official [custom-titlebar guide](https://v2.tauri.app/learn/window-customization/) then requires JellyPilot to provide drag regions and window controls itself. The guide also warns that a custom macOS title bar loses native window behaviors such as moving or aligning the window.

That path would turn window management into JellyPilot-rendered UI and create platform-specific accessibility, keyboard, focus, hit-target, maximization, snapping, and drag-region obligations. A visual theme does not justify that scope.

Tauri also exposes `shadow`, `transparent`, and `windowEffects`, but their support is intentionally uneven:

- shadow control is unsupported on Linux;
- on Windows, disabling shadow has no effect on decorated windows;
- window effects require transparency and are unsupported on Linux;
- a transparent macOS window requires Tauri's private-API feature and prevents App Store acceptance;
- Windows ignores or coerces many non-opaque background alpha values;
- macOS does not implement `set_background_color` for the webview layer, so document CSS still owns that layer.

JellyPilot must therefore avoid transparent-window and native-effect dependencies in the shared appearance contract.

## Platform contract

| Surface | Linux | Windows | macOS |
|---|---|---|---|
| Webview content | Exact Panda appearance | Exact Panda appearance | Exact Panda appearance |
| Native decorations | Keep enabled; compositor/window manager owns exact result | Keep enabled; Windows owns controls, border, corners, and shadow | Keep enabled; AppKit owns traffic lights, title bar, and window behaviors |
| Light/Dark hint | `set_theme`; app-wide | `set_theme`; window-specific behavior is supported | `set_theme`; app-wide, macOS 10.14+ |
| Opaque background | Supported baseline; verify WebKitGTK startup | Supported; window alpha is ignored, so use opaque colors | Window background can be set; webview document background remains CSS-owned |
| Custom title-bar style | No cross-platform Tauri style API | No native title-bar palette/style API | `titleBarStyle`, `hiddenTitle`, and traffic-light position are macOS-only |
| Shadow/effects | Shadow and effects unsupported | Decorated shadow cannot be disabled; effects require transparency and have caveats | Effects/transparency are platform-specific; full window transparency requires private API |
| Acceptance promise | Mode-aligned best effort, no pixel parity | Mode-aligned native controls, no custom color promise | Mode-aligned native controls, no custom material promise |

### Linux

Linux is the least deterministic native-chrome surface because decoration rendering depends on GTK and the active desktop/window-manager path. Runtime theme is app-wide. JellyPilot can require that it requests the selected Light/Dark mode and paints a matching opaque client area, but it cannot require a particular title-bar color, button shape, shadow, or client-versus-server-side decoration outcome.

The maintained Linux E2E path should prove:

- the first visible webview frame uses the persisted appearance;
- a runtime Color Mode change updates the document and calls the native synchronization path;
- tray hide/show retains the selected appearance;
- failure falls back to Control Room Dark without leaving a partially themed frame.

It should not use screenshot pixels from the desktop title bar as a portable assertion.

### Windows

Windows supports the initial configuration theme and runtime `set_theme`. The normal decorated window retains Windows-owned caption buttons, snapping, border, corners, and shadow. JellyPilot should not promise a Braun-colored native title bar or attempt to remove the decorated shadow.

An opaque `backgroundColor` is safe. Alpha is not a portable material mechanism: the window layer ignores it, and WebView2 coerces translucent values on supported modern Windows versions. Acrylic/Mica or other `windowEffects` are therefore optional platform experiments outside the shared four-appearance contract, not required output.

### macOS

macOS exposes additional title-bar options—Visible, Transparent, or Overlay—plus hidden title and traffic-light positioning. Those are platform-specific layout policies, not theme variants. The shared baseline should remain the standard visible decorated title bar.

Tauri's official transparent-titlebar example requires programmatic window construction and direct AppKit background configuration. Full transparent-window support uses a private API that prevents App Store acceptance. Neither is necessary for Light/Dark alignment, so neither belongs in the required appearance contract.

JellyPilot may later prototype a transparent native title bar as a macOS-only enhancement, but it must degrade to the standard decorated window and cannot block the four-appearance design.

## Startup and switching constraints

The persisted appearance must be known before both the webview and native chrome become visible. Current Tauri defaults create and show the configured window immediately, while JellyPilot loads persisted configuration during `setup`. The implementation design must close that ordering gap using one of Tauri's supported startup shapes:

1. configure the main window with `visible: false`, load and validate the appearance, set native theme/background plus the frontend's initial appearance state, and only then show it; or
2. configure `create: false`, load appearance first, then build the configured window programmatically with the resolved native properties.

This research does not choose between those two persistence/first-paint architectures; [Choose appearance persistence and first-paint coordination](https://github.com/hewel/jellypilot/issues/168) owns that decision. It does establish these acceptance constraints:

- no visible system-default title bar paired with the opposite JellyPilot Color Mode during startup;
- no transparent or unpainted webview/window gap;
- native and document modes change as one user action;
- if native synchronization fails, the document remains usable and reports/falls back truthfully rather than blocking the app;
- tray restoration does not re-read or reset appearance unexpectedly;
- frontend-driven native setters require explicit Tauri ACL permissions, while a Rust-owned synchronization path does not broaden the frontend window capability surface.

## Safe fallback

Control Room Dark is the fallback at every layer:

- Panda/document root: Control Room + Dark;
- HTML `color-scheme`: dark;
- Tauri native theme: Dark;
- opaque window/webview underlay: the Control Room Dark background token;
- decorations: enabled, standard platform title bar;
- transparency/effects: disabled.

If a platform ignores the native theme hint, JellyPilot still remains readable because the webview and opaque underlay are authoritative. The fallback must never disable decorations or depend on unsupported effects to appear complete.

## Downstream decisions

The following are now fixed inputs for later tickets:

- Native chrome follows **Color Mode only**. Design Theme does not replace or recolor platform window controls.
- Standard native decorations remain enabled on all supported platforms.
- The shared contract uses explicit Light/Dark plus an opaque background; it excludes System mode, transparent windows, window effects, and custom title bars.
- Exact native title-bar pixel parity across Linux, Windows, and macOS is not an acceptance criterion.
- The first-paint architecture must apply native and document appearance before first show.
- Linux native E2E owns startup/switching behavior; Windows and macOS require representative build/spot-check evidence for native chrome.
- macOS Transparent/Overlay title bars and Windows material effects are optional future enhancements, not blockers or appearance variants.

No new Wayfinder ticket is required. Persistence and first-paint coordination is already mapped, and native cross-platform proof belongs in the existing native acceptance prototype.

## Sources

### JellyPilot

- [`src-tauri/tauri.conf.json`](../../src-tauri/tauri.conf.json)
- [`src-tauri/tauri.panda-review.conf.json`](../../src-tauri/tauri.panda-review.conf.json)
- [`src-tauri/tauri.webdriver.conf.json`](../../src-tauri/tauri.webdriver.conf.json)
- [`src-tauri/Cargo.toml`](../../src-tauri/Cargo.toml)
- [`Cargo.lock`](../../Cargo.lock)
- [`src-tauri/src/lib.rs`](../../src-tauri/src/lib.rs)
- [`src-tauri/capabilities/default.json`](../../src-tauri/capabilities/default.json)
- [`docs/agents/e2e.md`](../agents/e2e.md)
- Installed Tauri CLI `2.11.2` schema at `node_modules/@tauri-apps/cli/config.schema.json`

### Tauri primary documentation

- [Tauri v2 configuration reference](https://v2.tauri.app/reference/config/)
- [Tauri v2 window customization](https://v2.tauri.app/learn/window-customization/)
- [Tauri `2.11.2` `WebviewWindow`](https://docs.rs/tauri/2.11.2/tauri/webview/struct.WebviewWindow.html)
- [Tauri `WindowConfig`](https://docs.rs/tauri-utils/latest/tauri_utils/config/struct.WindowConfig.html)
- [Tauri core window permissions](https://v2.tauri.app/reference/acl/core-permissions/)
