# Saved Browse Filters

Accepted 2026-10-05 under the user's delegated batch-5 decisions. Implementation
follows the active-playback-tools batch. Desktop and native TV share named,
device-local saved filters within the existing Profile Scope. The label is
Saved filters / 保存的筛选; these are saved queries, not frozen media lists.

## Saved identity and contents

A saved filter has a stable ID, a name, the exact Profile Scope, library ID and
collection type, a display-only library-name snapshot, and complete
BrowsePreferences. Persist existing sort/direction, played state, Favorites
filter, quality, country and genre values. Do not save result items, credentials,
page numbers, scroll position, view mode or search input. This adds no server
playlist and does not modify Favorites or Watchlist.

Names are trimmed, non-empty and at most 80 Unicode characters; reject control
characters. Within a Profile Scope, compare normalized lowercase names to
prevent accidental duplicate names. Keep at most 50 saved filters per scope.
Rename keeps the stable ID. A changed current query does not automatically
overwrite a saved filter; saving again requires a distinct name.

Reuse the established storage root and atomic JSON persistence. A failed write
must retain both the previous file and current in-memory list. Validate active
scope, operation token and account handoff inside the existing SDK mutation
boundary. Account names and library IDs alone cannot establish ownership.
Successful account changes expose only the new scope's records; a failed login
must not destroy the previous account's records or current query.

## Save, manage and apply

An ordinary library Browse toolbar offers Save current filters. A name form
shows the actual library and a readable summary before saving. Saving does not
change the query. A Saved filters entry in Desktop personal navigation and the
TV rail opens the same scope's management view, also reachable when a referenced
library has disappeared. Each record supports Apply, Rename and Delete.
Deleting affects only the local definition and never media or server state.

Apply resolves the saved library ID and collection type against refreshed
shortcuts for the active account. On success, install the route and all saved
preferences atomically, reset the viewport and issue one new browse query.
Cancel the old query and reject late pages through the existing Browser route.
Do not briefly load default conditions before applying individual fields.

If a successful directory read cannot find that exact library/type, show
Library unavailable and retain the definition for rename/delete. A failed
directory request instead offers retry. Do not fall back to another library or
silently broaden conditions. An old genre/country value remains a real filter
even if current facet options omit it; zero results remain a valid result.

## Complete conditions across presentations

Ordinary browsing retains its established defaults. A saved-filter application
is an explicit exception to Desktop's advanced-filter clearing and TV's
played/Favorites normalization. Detail navigation and Desktop/TV switches must
retain all applied conditions and the saved-filter context.

Both interfaces show the complete active condition summary and provide removal
or clear actions. Existing ordinary filter controls remain usable. Conditions
that a presentation does not normally offer still have a readable, focusable
summary/removal control. Changing one condition affects the current query and
marks it Modified without rewriting the saved record. Applying it again restores
the full saved query. Explicit navigation to a different ordinary library exits
this saved-filter context and follows the existing browse defaults.

Desktop uses 40px minimum targets, wrapping names/conditions and bounded
scrolling. TV uses the established scaled 64px controls and directional focus.
Saving/renaming failures retain the entered name for correction or retry.
Pending actions cannot target a newly selected record after a list refresh;
focus and callbacks use stable record IDs. Delete settles on a nearby record or
a safe Back/Close target when empty. All user-visible text supports both locales.

## Evidence and acceptance

Tests cover scope isolation with identical library IDs, stable rename/remove,
invalid names and limits, persistence failure, stale queued writes, restart
round trips of every supported condition, atomic Apply and late-page rejection,
unavailable-library versus network error, and detail/presentation round trips.
Use existing browse/configuration tests at their real boundaries; do not add a
second query engine or mirror field-copying code with superficial tests.

Cross-crate Suite and native smoke apply. Humans verify long names/conditions,
narrow windows, TV focus, themes/locales and results against real server data.
Static Paper boards and code checks do not establish native visual acceptance.
