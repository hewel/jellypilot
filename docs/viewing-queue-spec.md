# Temporary Viewing Queue

Accepted 2026-10-05 under the user's delegated batch-3 decisions. Implementation,
automated behavior checks and independent review are complete; human acceptance
remains pending. The first interface is for Desktop and native TV (living-room
PC), sharing their existing playback session and backend. It does not send a
remote playlist or change Android's foreground-only playback contract.

## Product behavior

The Viewing Queue, shown as 待播队列 / Up next, contains real Movie and Episode
items that the user chooses within the current Profile Scope. It is temporary
and device-local; no disk persistence, server playlist, cross-server entries,
Favorites or Watchlist mutation is involved. The existing current-season episode
list remains separately labelled and usable.

Details offer Play next and Add to queue for their actual playable movie or
episode; neither starts playback. A series requires its resolved episode target.
Adding an item opens the queue so it can be reviewed and managed even when
playback is stopped. The player also exposes the queue while playing. Current
playback is displayed separately from the remaining items.

The list supports Play now, move up, move down, remove and clear. Editing does not
seek, stop or switch the currently playing item. The first Up and last Down
actions are disabled. Use stable entry identities, never visible row indices,
and reject stale edits after the queue or Profile Scope changes. An in-flight
start disables conflicting edits until settlement. Keep at most 100 entries;
adding the same media again repositions the existing upcoming entry instead of
creating an indistinguishable duplicate.

Each entry carries its explicitly selected Resume/Beginning position. Play now
uses the existing start route. A successful start consumes that exact entry;
failure leaves it queued with retryable feedback. Do not silently skip failed
items, optimistically remove them, or consume a different row after reorder.

## Playback integration

Manual Next first chooses the queued head, then falls back to the existing next
episode lookup if the queue is empty. Previous remains Previous episode; this
feature does not invent a playback history. Natural EOF uses the same next-item
choice and still requires the existing auto-next setting, current admission and
visible-playback lifetime. The setting's label becomes Automatically play next
item, describing its existing option plus the newly supported queue.

Ordinary Play and Stop preserve the remaining queue. Closing the embedded window
preserves it while the session is suspended, and cannot trigger automatic
advancement or hidden playback. Exit, explicit Disconnect, and successful
Profile Switch clear it; a failed candidate login must not discard the current
profile's queue. A stale settlement must never consume a new profile's entry.

Use the existing Playback Session's command serialization, completion handling,
SDK admission, presentation lease and reporting. A small display-free queue model
owns ordering and consumption claims; it is not another playback state machine.
Keep the episode-list cache distinct from the upcoming queue. MPRIS, tray, remote
Next and TV navigation consume the same effective next-item projection; existing
restrictions such as paused MPRIS Next remain unchanged.

## Presentation and verification

Desktop controls use independent 40px minimum targets, existing sharp Floating
Popover styling and scroll containment. The native Desktop panel sits inside a
16px safe margin, with width limited to 520px or the viewport minus 32px, and
height limited to 640px or the viewport minus 64px. It has no forced minimum
width. Titles and actions use separate lines, actions wrap without shrinking
their 40px targets, and the body (current playback, feedback and entries) scrolls
beneath the fixed heading/Close and above the reachable fixed Clear control.
TV actions use the established native
focus and minimum-size contract. Loading/failed starts do not discard focus;
remove/reorder settles on a valid nearby item. Empty, pending, failed/retry and
capacity states remain explicit without sample media.

Desktop sorting keeps the same editing action while it remains available; at
the first/last boundary it focuses Close instead of another media action. TV
keeps the same Up/Down focus at that boundary with activation disabled, and
directional navigation can leave it. Repeated Enter must never turn a sorting
action into Play or Remove. Explicit Remove may focus the nearby Remove action.

The TV side panel uses an 800px reference width and existing native scale,
with 96px horizontal and 60px vertical safe margins at 1080p. Close remains in
the header and the footer holds the action hint. Current playback, entries,
Clear and feedback share one scrolling region so long messages cannot push
controls outside the viewport; focus movement reveals the selected entry/Clear.
A Sidebar entry keeps it available after Stop. Focus follows stable entry ID
and action across reorder; removal or successful consumption selects the nearest
valid row, or Close for an empty list. Closing restores the invoking control.

Core tests cover stable identity, reorder, duplicate reposition, capacity and
claim settlement. Playback tests cover natural EOF/manual Next, failed or stale
starts, Stop, suspended playback and account retirement. Interface tests cover
real detail targets, stale messages and focus after list changes. Follow the
cross-crate Suite and native smoke tiers; appearance and real playback continuity
remain human acceptance.
