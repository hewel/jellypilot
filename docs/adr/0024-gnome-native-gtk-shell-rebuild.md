# Rebuild the GTK shell as a GNOME-native interface at runtime parity

_Status: Accepted. Refines ADR 0021 and ADR 0022._

## Context

The GTK walking slice delivered by ADR 0021 reached broad feature coverage — authentication,
Saved Service Profiles, Video Home, library browsing, item details, User Data Actions, and
external MPV playback — but its shell layout predates any concrete design-language decision.
User acceptance feedback: the layout diverges from typical GTK applications to the point of
being effectively unusable, and the native frontend should hold runtime feature parity with the
Tauri application except for Embedded Web Playback.

ADR 0021 already assigns the native shell's visual language to GTK. This ADR makes that
ownership concrete as libadwaita's design language rather than plain GTK 4 style classes.

Relm4 0.9.1's `libadwaita` feature resolves to libadwaita-rs 0.7.2 on gtk4-rs 0.9.7. Declared
MSRVs (Relm4 1.75, gtk4 1.70; libadwaita 0.7.2 declares none) do not raise the workspace Rust
1.85 baseline. Relm4's `gnome_46` feature enables `adw/v1_5` and `gtk/gnome_46`, requiring
system libadwaita-1 >= 1.5 and GTK >= 4.14 — satisfied by Ubuntu 24.04 LTS, Fedora 40+, and
Debian 13. The navigation and dialog widgets the GNOME-native shell needs
(`AdwNavigationSplitView`, `AdwToolbarView` since 1.4; `AdwDialog`, `AdwPreferencesDialog`,
`AdwAlertDialog` since 1.5) set that floor; a 1.0 baseline would force deprecated
`AdwMessageDialog`/`AdwPreferencesWindow` paths.

## Decision

**Adopt libadwaita as the GTK shell's design system.** Enable Relm4 features `macros`,
`libadwaita`, and `gnome_46`. The application runs `adw::Application` with an
`AdwApplicationWindow`. The root is an `AdwToolbarView`: an `AdwHeaderBar` on top and a
playback control bar at the bottom. The body is an `AdwNavigationSplitView` whose sidebar
lists Video Home and the dynamic library destinations; narrow widths collapse the sidebar
into an overlay. Settings leaves the sidebar into the header primary menu (Preferences,
About, Quit) presented as an `AdwPreferencesDialog`. Now Playing leaves the sidebar: the
bottom bar controls the active External MPV Playback session and presents a full-page player
view. The Tauri frontend's Ambient Glow and web-oriented decoration are not reproduced;
custom GTK CSS remains limited per ADR 0021.

**Rewrite the view layer as per-page Relm4 components.** The single-component shell is
replaced progressively, one slice per view, reusing the display-free adapters unchanged
(browse model, library browse, auth storage, playback controller, artwork cache). ADR 0021's
honesty rules still apply: no fake media, no claimed connection, no claimed playback state.

**Define GTK runtime parity as the Tauri surface minus the embedded chain.** Included:
authentication (Quick Connect, Password Login, Saved Service Profiles, Login Prefill),
Library Browser (Video Home, browse and search, full detail richness, User Data Actions,
Direct Playback), Now Playing (transport, seek, volume, audio/subtitle track switching,
adjacent-episode navigation), Playback Target remote sessions, Intro Skipper with its
three-mode setting, Diagnostics, a disk Library Image Cache, and a full settings surface
(connection, MPV path and arguments, Playback Target name, subtitle language priority,
shortcut keys, cache, session). Excluded alongside Embedded Web Playback: Local Transcode,
Explicit MPV Fallback, and Playback Engine Preference/Override — GTK always presents
External MPV Playback.

**Extract a shared session crate for Playback Target and Intro Skipper.** These capabilities
need the behavior of Tauri's `SessionManager`, WebSocket command channel, and playback-event
handling. Following ADR 0022's precedent, that behavior moves into a shared crate with Tauri
retaining a thin adapter; the exact boundary is fixed when that slice lands and ADR 0022 is
amended then.

**Persist GTK configuration in a GTK-owned XDG config file.** MPV path and arguments,
Playback Target name, subtitle language priority, Intro Skipper mode, shortcut keys, and the
image-cache toggle live in the GTK package's own configuration. Tauri Store remains owned by
the Tauri application per ADR 0023.

**Land the rebuild in eight slices.** (1) libadwaita skeleton plus this ADR; (2) login view
with Login Prefill; (3) Video Home and browse views; (4) detail-page richness; (5) bottom
playback bar, full-page player, track switching, adjacent episodes; (6) shared session crate
and Playback Target; (7) Intro Skipper; (8) Diagnostics and settings completion. Each slice
keeps deterministic model tests and the display smoke gate green, and the application stays
usable at every step.

The endgame is unchanged: daily usability plus runtime parity. Packaging, release, and ADR
0022's remaining gates remain deferred.

## Consequences

- Linux CI installs `libadwaita-1-dev` (>= 1.5) alongside `libgtk-4-dev`; the build floor
  rises to GTK 4.14 and libadwaita 1.5, excluding distributions older than Ubuntu 24.04 LTS
  or Debian 13.
- libadwaita 0.7.2 declares no MSRV; the Rust 1.85 workspace contract rests on the declared
  Relm4 1.75 and gtk4 1.70 baselines, with libadwaita unspecified and compile-gated in CI.
- The monolithic shell shrinks as slices land; Now Playing and Settings keep their current
  sidebar destinations only until slices 5 and 8 provide their replacements.
- Session behavior (playback reporting, remote command handling, Intro Skipper ranges)
  gains two consumers and must not be duplicated; security fixes land in the shared crate
  once.
- GTK configuration is isolated from Tauri configuration; no cross-frontend settings
  migration is promised.
