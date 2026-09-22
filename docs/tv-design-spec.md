# TV presentation

The [Paper TV designs](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/6-0)
define the living-room PC presentation. The native interface covers Home, Library,
Detail, Personal Lists, the player and Settings, plus TV-sized account forms and Search.
Authentication, saved accounts, collections, undo and preferences reuse the existing
application services. Text entry supports a directional on-screen keyboard and native IME.
Implementation and automated validation do not establish television visual acceptance.

## Ownership and entry

`app/tv` owns TV layout, input, navigation focus and player chrome. The existing account,
SDK, browser, image pipeline and Playback Session remain shared. Display-free focus and
player interaction rules live in `jellypilot-core`; scoped TV styles live in
`jellypilot-ui`. Root wiring selects one presentation and one keyboard subscription.

Presentation (`Desktop` or `Tv`) is separate from App Mode (`Full` or `ControlOnly`).
The in-app choice is persisted only after a successful settings write. `--tv` is an
invocation override, forwarded to an existing process by the single-instance channel.
TV enters fullscreen and takes priority over Start Minimized, while preserving the
configured desktop App Mode. Exiting returns to the desktop window geometry and capability.
Switching presentation does not reconnect accounts or create another player.

The Playback Backend stays unchanged. Video inside the TV interface requires Embedded
MPV Playback; External MPV retains its own player window, with the TV surface acting as
a controller. Source-level support for Linux/Windows embedded presentation is not a claim
of television or hardware validation.

## Presentation and input

The source canvas is 1920×1080, with a 96px horizontal / 60px vertical safe inset, a
288px navigation region, content beginning at x336, and a five-column library grid.
Typography and controls scale together from this composition. The current focus
revision replaces universal rings with light filled controls and dark foregrounds.
Posters grow 3% from the bottom center with a 2px light edge, without moving their slots
or labels. Main transport grows from 80px to 84px; seek grows from a 6px to 8px track and
16px to 24px thumb. Transitions take 175ms and are interruptible; reduced motion applies
the final state immediately. Indigo and checked indicators retain selection independently.
Runtime metadata and
artwork come from the connected server, never from the design's fixture values.

Directional keys move focus; Enter confirms; Esc/Backspace returns. Keyboard-style
remotes use the same mapping. Gamepad and HDMI-CEC adapters are not included. Browsing
preserves source focus and scroll on return from Detail or playback. Library navigation
keeps the column when moving vertically and clamps to the final row's existing items;
focus-driven loading uses the shared paged browser and bounded artwork materialization.

The 2026-09-22 subtraction revision uses short contextual action hints instead of a
persistent directional-key tutorial. Long-press actions, Undo and seek commit/cancel
remain discoverable. Normal server/connection information lives in Account; page headers
retain the clock, and actual failures retain their recovery feedback.

Library exposes All, Unwatched, Filters and Sort, with the result count only in the
header. Filters are a draft: Cancel or Back discards changes; Apply commits once and
returns focus to the trigger. The active dimension count appears on the trigger.
Quality, production country and genre options come from this library's metadata.
Neither audio language nor localization settings are evidence of production country.
Series have no episode-wide quality index, so only country/genre filtering is available.
Filters survive a TV Detail round trip, reset for a different library, and are cleared
when restoring browsing in Desktop. They are not global saved desktop preferences.

Both server adapters build a bounded complete metadata snapshot for advanced filters,
then apply filtering, count and pagination to the same snapshot. Loading, timeout,
incomplete responses and capacity limits are explicit states; partial scans never
become successful results. Unfiltered browsing retains the normal server paging path.
Scans use pages of 100 and stop above 20,000 items, 4 MiB per response, 32 MiB total
metadata or 60 seconds. Facet freshness is 60 seconds; an active browse snapshot is
stable until an explicit refresh, changed query or confirmed user-data mutation.
A continuation cannot silently start a new snapshot after invalidation. Retry rebuilds
from page zero, preserving the visible window until replacement; returning from Detail
also refreshes advanced results without discarding scroll. Nominal HD,
Full HD and 4K bands use source width (1280, 1920, 3840), with height fallback when width
is absent, preserving cropped widescreen 4K. The 4K Dolby Vision option requires both
facts on the same video stream/version. Facet loading does not evict an active query.

Filter, playback settings and decoder panels anchor at the top-right safe inset and
shrink to content. Their reference widths are 768, 744 and 768 respectively, with 40px
padding and 24px gaps. Short windows scroll internal content while retaining reachable
actions. Playback settings keeps an accessible 64px Close icon in the title row,
yielding roughly 504px instead of the 488px reference with no separate Close control;
localized text may increase that height. Decoder normal/failure reference heights are
561/689px. These are content-driven measurements, not fixed panel heights.

Holding Enter for 650ms on a card opens its actions without also activating the card
on release. Menu or Shift+F10 opens the same menu. Leaving the target, losing window
focus or entering another input layer retires the pending hold. Menus confine focus
and restore their source; a candidate selection is not an applied collection change.

Watchlist and Favorites retain separate counts, focus and horizontal scroll. Their
320×480 poster shelves show four complete cards and a next-card peek. Bounded loading
windows cover the visible range across page boundaries. Removing an item preserves the
other collection and playback history, focuses the surviving neighbor and offers the
shared eight-second Undo. Focusing the feedback or hiding it behind playback, search,
settings or another modal pauses expiry. Failure retains the
previous state with a reachable retry; undo restores the item, order and focus.

Settings use the design's category/detail columns and 3/3/2 playback grouping, with
contextual help that does not change row height. Choices separate focus from saved
selection, and only confirmed writes update the displayed value. Failed writes retain
the old preference; Retry repeats the same intended change. Decoder hardware/software
and cache preferences apply on the next media load. With no explicit decoder preference,
the row reports the player default rather than claiming hardware decoding. Auto-next
controls natural EOF advancement; explicit Next still works. Progress sync changes the
passive report interval; transport changes and stopping continue to report immediately.
External audio passthrough applies on the next MPV process. Image enhancement, display
refresh-rate switching and embedded passthrough show their unsupported capability.

Account confirmations default to Cancel and use the existing account reducer directly;
they cannot turn a delayed confirmation for one profile into authorization for another.
Canceling sign-out preserves saved credentials and local data.

The player owns these rules separately from desktop shortcuts:

- The first direction/confirm press in the pure-video state reveals controls without
  executing another action. Dedicated play/pause operates directly.
- Playing controls hide after five idle seconds. Pause, progress preview and panels
  retain their required presentation. Back hides controls before exiting playback.
- Progress preview advances in ten-second candidates inside actual seekable intervals.
  Confirm commits once; Back/Down cancels. Seeking never implicitly resumes playback.
- Track panels separate the focused candidate from the applied selection. Back cancels
  and restores the trigger; confirmation uses the existing track-control path.
- Speed and information reflect the current engine. Session intro-skip changes do not
  write global or per-series preferences. Missing capabilities remain unavailable.

## Acceptance

Automated checks cover focus boundaries, page/scroll restoration, preview commit/cancel,
panel return, input exclusivity, invocation/config persistence, single-instance delivery,
and asynchronous player identity. Focused crate gates precede workspace checks and both
desktop/TV native startup smoke gates. No screenshot or agent-driven desktop input is used.

Human acceptance on the actual television must check:

1. Enter TV from Settings and `--tv`, including while the process is already running;
   confirm fullscreen and successful return to the previous desktop window.
2. Read titles and controls from the sofa; inspect safe edges, filled focus controls and poster edges at the
   television's configured display scale and resolution.
3. Traverse a large library through unloaded rows with the remote; open an item and
   return to the same card and scroll position. Exercise loading, empty and failed loads.
4. Play, pause, reveal/hide controls, preview/cancel/confirm a seek, select tracks and
   return from panels; verify direction keys never also change desktop volume or seek.
5. Close/reopen the window during playback and switch presentation; confirm the same
   account/session remains authoritative and the established background pause rules hold.
6. Browse both personal lists across loading-window boundaries, remove the final item,
   focus Undo, then restore it; verify the other list and playback progress are unchanged.
7. Move a settings candidate without applying it, cancel, then confirm and reopen it.
   Exercise a failed save and retry. Verify decoder/cache changes on the next playback
   and compare reported decoder facts without assuming the requested mode succeeded.
8. Enter text using both on-screen controls and native IME, search and return, add or
   switch accounts, and cancel sign-out with local-data removal selected.
9. In Library, open Filters, change a candidate and cancel; reopen and apply multiple
   dimensions. Confirm one truthful result count, stable paging, trigger focus return,
   Detail round-trip retention and no hidden filter after returning to Desktop.
10. Compare compact playback/decoder/filter panels in normal and short windows;
    inspect contextual hints, failed-save retry and long localized text.
