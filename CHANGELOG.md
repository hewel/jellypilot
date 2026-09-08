# Changelog

All notable changes to JellyPilot are documented in this file.

## [Unreleased]

### Added
- Added English, Simplified Chinese, and Follow system interface preferences shared by Login and Settings, with immediate relocalization of screens, retained feedback, and available tray controls without resetting login or playback.
- Added embedded Fluent resources, localized count/duration grammar, and font license notices in Settings.
- Added default-enabled per-season player volume memory, stored locally per server and account, with a Playback setting that pauses saving/restoring without deleting records. Unknown seasons and movies use MPV startup volume; temporary mute carries across continuous episode playback only.
- Added an isolated, reusable `jellypilot-player` crate for GStreamer local-video playback and an iced/wgpu surface, with a standalone demo and private worker/frame/shutdown ownership. Production playback remains External MPV Playback; no GStreamer dependency was added to the application workspace.
- Added explicit HTTP(S)/HLS sources to the isolated player/demo, with a cancellable same-origin authenticated relay, bounded playlist rewriting, network deadlines, buffering/seek-window state, and a credential-safe `--url-env` launch path. Production Jellyfin/Emby playback remains External MPV Playback.

### Changed
- Replaced bundled Inter/Space Grotesk typography with the original Manrope V5 and MiSans variable fonts, including mixed-script fallback and explicit body/heading weights.
- Refined the native Sidebar and Account Popover with scoped light/dark surfaces, a separate library-count badge, and segmented tools.
- Simplified the Account Popover into a quick menu: show the current account once, quiet the address and actions, and move preferences and saved-login removal to a direct Accounts Settings entry.
- Widened the default Full-mode window from 1600×900 to 1760×900 logical pixels without changing its height, minimum size, or Control-Only sizing.
- Reworked Video Home with a full-width, aspect-preserving Backdrop behind scrolling content, a detail-style bottom gradient, and a subdued thumbnail selection rail with hover/focus information. Added manual selection, series-grouped continuation candidates, and selection retained across same-profile refreshes; kept the separate Continue Watching row directly resumable and visible in compact windows.
- Made Library Image loading follow measured visibility, with immediate visible-image admission and one viewport of prefetch before and after scrolling across Home, Browse, Detail, and Personal Lists.
- Moved Library/Search restoration and refresh ownership into the display-free browse model, retaining complete usable results and scroll position while replacement data loads.
- Consolidated Intro Skipper eligibility and prompt lifetime into one display-free policy, preserving Automatic, Manual, and Off modes, exact-start skipping, and one automatic attempt per range.

### Fixed
- Added Jellyfin 12.0 compatibility: updated the generated API client, migrated API and playback/subtitle/WebSocket authentication away from disabled legacy forms, and redacted the modern `ApiKey` query parameter in diagnostics. Emby authentication remains unchanged.
- Restored history-specific vertical and horizontal scroll positions when returning to Home, browse results, details, and Personal Lists. Back navigation retains loaded content and query context; new searches and filter changes still start at the top.
- Avoided repeated overview text shaping on detail pages during playback updates, preventing long layout stalls while scrolling.
- Kept account avatars fixed-size, fitted short account menus to their content, and exposed truncated names and addresses through hover and keyboard-focus hints.
- Centered Sidebar control icons and labels, restricted button focus feedback to keyboard use, and prevented trigger tooltips from overlapping open popovers.
- Preserved the Account trigger's keyboard focus return when Settings is refocused with Ctrl/Cmd+, after entering through Manage accounts.
- Isolated shared image demand between pages and Now Playing, cancelled work when its final consumer leaves, and released native image handles outside the visible range without adding another pixel cache.
- Kept Browse poster placeholders at the image's fixed height, preventing repeated image admission and cancellation at prefetch boundaries and the resulting redraw-invalidation loop while scrolling.
- Rejected late browse deliveries across history restoration and model recreation, kept failed refreshes retryable, and tied refresh activity to the captured page/query rather than the lifetime of its first task.
- Sized Browse columns from the actual opened window instead of the requested startup size, preventing clipped posters when the window manager constrains the default window.
- Let detail hero content grow beyond the backdrop's fixed 16:9 frame and wrap action rows, keeping buttons and labels readable at narrower widths and with longer localized copy.
- Shared disk-cache coordination between Library Images and account avatars, keeping their authentication and in-memory lifecycles independent while coordinating cache clearing, writes, and eviction.

## [2.0.0] - 2026-09-02

### Added
- Rebuilt JellyPilot as a native Rust application with iced, including Jellyfin and Emby sign-in, saved profiles, library browsing, search, filters, media details, and favorite/played user-data actions.
- Added External MPV Playback with episode queues, server-hosted subtitles, preferred subtitle languages, automatic next-episode playback, and Jellyfin Intro Skipper support.
- Added Jellyfin cast-target discovery and Jellyfin/Emby remote transport control across the player bar, Control-Only window, and system tray.
- Added a disk-cached artwork pipeline, light and dark themes, Control-Only mode, and native Windows, macOS, and Linux packaging.

### Changed
- Consolidated the application into a display-free Rust workspace with a custom iced presentation layer, OS-keychain authentication, and resilient media-server sessions.
- Raised the minimum supported Rust version to 1.98 and added the native Arch release artifact that the forthcoming `jellypilot-bin` AUR package will consume.

### Fixed
- Loaded server-hosted external subtitle tracks into MPV and applied the selected default subtitle.
- Hardened artwork loading, WebSocket reconnection, MPV lifecycle handling, and playback progress synchronization.

### Breaking Changes
- Removed the Tauri/Solid.js webview frontend and its embedded web playback and local FFmpeg/HLS pipeline.
- An external MPV executable is required for all playback; `PATH` is the default discovery mechanism, and a custom executable path can be selected in Settings.
- Existing Tauri settings and saved service profiles are not migrated; connections and preferences must be configured again.

## [1.4.2] - 2026-07-23

### Added
- Emby media server support: login, library browsing, playback, and progress reporting through a provider-neutral session facade.
- Collapsible sidebar with persisted preference and compositor-animated FLIP transitions.
- Virtualized large browse grids with prefetched paging and persisted filter state.
- Redesigned item detail pages for desktop with back navigation and scroll restore.
- Disk-cached artwork scoped to the active service connection.
- Saved service profile switching.
- AUR package (`jellypilot`) for Arch Linux source-built installation.

### Changed
- Completed full Panda CSS styling cutover across all application surfaces, replacing vanilla-extract and utility bridges.
- Replaced floating controls with a persistent sidebar.
- Migrated data flows to solid-query with structured Effect Exit boundaries.
- Upgraded to TypeScript 7.

### Fixed
- Cleared closed MPV IPC connections to prevent stale socket references.
- Decoupled artwork loading from data fetches to avoid layout churn.
- Stabilized browse card layout and corrected virtual grid row height estimates.
- Redirected stale browse routes on server change and avoided reload churn.
- Kept tall service dialog content reachable and fixed Settings modal nesting.
- Fixed Windows CI race condition in release workflow (`bun install --ignore-scripts`).

### Maintenance
- Added rsdoctor build diagnostics.
- Added isolated native WebDriver E2E harness and Tauri parity evidence workflow.
- Consolidated agent documentation and Effect rules into docs/agents/.

## [1.4.1] - 2026-06-21

### Added
- Media info hover-cards for detailed movie and series views.
- Playback stream selection and episodes-first series detail page hierarchy.
- Manual MPV skip prompt for the Intro Skipper integration.

### Changed
- Redesigned the application frontend with sticky segment group navbars, horizontal header layouts, and modern layout spacing.
- Migrated navigation and layout architecture to TanStack Router file-based nested routing.
- Adopted vanilla-extract for design tokens and component-specific styling.
- Migrated playback controls to a clean header drawer and consolidated search filters and sort controls using Ark UI Menu and Toggle.

### Fixed
- Aligned browse filter-error mock to resolve backend error code mismatches.
- Fixed aspect ratios for library home cards to match active category rows.
- Resolved type boundaries and error handling for operations console, quick connect, and password command failures using Effect.

### Maintenance
- Migrated from Biome to Oxc linting/formatting and updated Tailwind to Rsbuild plugin integration.
- Migrated library data workflows to structured, typed Effect Exit results.
- Configured local release note reader workflow to replace git-cliff.
- Updated default episode switching keyboard shortcuts to `Shift+>` and `Shift+<` and moved shortcuts display to the right panel.

[2.0.0]: https://github.com/hewel/jellypilot/compare/v1.4.2...v2.0.0
[1.4.2]: https://github.com/hewel/jellypilot/compare/v1.4.1...v1.4.2
[1.4.1]: https://github.com/hewel/jellypilot/compare/v1.4.0...v1.4.1
