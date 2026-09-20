# Keep desktop History removal local and let new viewing restore visibility

_Status: Accepted product decision 2026-09-21; implemented in the desktop synchronization. Does not
change the Android client contract._

Watch History combines the current server user's played and resumable movies and
episodes; clearing server progress to remove a row would change playback intent.
Desktop History removal therefore changes only device-local visibility within its
Profile Scope, independently of Favorites, Watchlist, Played and server progress.
Persist hiding across restarts, but restore visibility after a reliably newer
server last-played observation or successful new local Playback Session for that
item. The server signal includes updates from other clients; it is not proof of
a separately identified external playback event.

This chooses persistent hiding of the observed history state over both a
session-only dismissal and indefinite suppression of an item after rewatching.
Unavailable or incomparable server timestamps keep the item hidden; neither the
local clock, a repeated read nor a changed progress/Played value substitutes for
new-viewing evidence. Undo also restores visibility without a server mutation.

Each removal has an independent, transient Undo opportunity that survives ordinary
page navigation, but ends when its account connection ends or the application exits.
Expiring Undo does not undo the persisted removal. Existing item-only hidden
records cannot be assumed to contain a playback observation; implementation must
preserve their hidden state and Android behavior when evolving shared storage.
The [desktop synchronization specification](../desktop-design-sync-spec.md) records
the confirmed interaction, independent notice queue and truthful-count rules.
