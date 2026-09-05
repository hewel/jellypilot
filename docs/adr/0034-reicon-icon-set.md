# Reicon replaces Tabler as the vendored icon set

_Status: Accepted and implemented, 2026-09-06. Supersedes the "Lucide SVG icons are bundled
assets" note in [ADR 0027](0027-cross-platform-iced-frontend.md), which the implementation had
already deviated from by vendoring Tabler icons._

The UI icon set is vendored from [Reicon](https://reicon.dev) (MIT, 2,600+ icons, Outline and
Filled weights on a 24×24 grid) instead of Tabler. Reicon's rounded, geometric, fill-based glyphs
match the JellyPilot logo's visual language; Tabler's thinner stroke-based line style did not.
Reicon also ships a first-party MCP server/CLI (`reicon-mcp`, configured as a project MCP server
in `.omp/mcp.json`) that agents use to search the catalog and inspect SVG markup when extending
the set.

The semantic `Icon` enum, asset filenames, and `icon*` helpers in `jellypilot-ui` are unchanged;
only the vendored SVG bytes, the attribution header comments, and `assets/icons/LICENSE` (now
Reicon's MIT license) changed. Outline remains the default weight; Filled is used only where the
existing vocabulary pairs weights for state (favorited heart, watchlist bookmark) or where a
solid marker is required (the played-filter disc uses `record` Filled, since Reicon's
`record-circle` is a voicemail-style glyph, not a dotted circle).

Most Tabler names map to identical Reicon names. The renamed mappings, for future maintenance:
`movie`→`clapperboard`, `device-tv`→`tv`, `photo`→`gallery`, `adjustments`→`sliders`,
`circle`→`record` (Outline), `circle-dot`→`record` (Filled), `circle-check`→`check-circle`,
`sort-ascending`/`sort-descending`→`sort-asc`/`sort-desc`, `qrcode`→`qr`,
`picture-in-picture`→`pip`, `player-skip-back`/`player-skip-forward`→`skip-prev`/`skip-next`,
`player-play`/`player-pause`/`player-stop`→`play`/`pause`/`stop`, `arrows-maximize`→`maximize`.

Unlike Tabler's stroke artwork, Reicon paints with filled paths; both use `currentColor`, so
iced's `svg::Style::color` tinting is unaffected. The icon asset test asserts every vendored SVG
paints with `currentColor` so a hardcoded fill cannot silently break theme tinting.
