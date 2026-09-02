# Changelog

All notable changes to JellyPilot are documented in this file.

## [Unreleased]

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
