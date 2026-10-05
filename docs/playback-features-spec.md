# Playback Feature Delivery

Accepted on 2026-10-05. The user authorized implementation in the order below,
design decisions, and coherent local commits. This document separates accepted
scope from delivery and human acceptance; it does not replace the existing
playback, account, or platform contracts.

## Delivery order

| Batch | Accepted scope | Delivery status |
| --- | --- | --- |
| 1 | Desktop seek thumbnails and Linux MPRIS | Implemented; automated checks and independent review passed; human acceptance pending |
| 1, addition | Progressive blur for embedded player controls | Removed at the user's request on 2026-10-05; Full controls retain their ordinary dark scrim |
| 2 | Android controls an authorized living-room PC/TV Playback Target through the media server | Implemented; focused checks and independent review passed; human integration pending; [contract](remote-controller-spec.md) |
| 3 | Editable, temporary viewing queue containing movies and episodes within one Profile Scope | Implemented; automated tests and independent review passed; human acceptance pending; [accepted contract](viewing-queue-spec.md) |
| 4 | Secondary text subtitles and A/B loop for the active Playback Session | Implemented; automated checks and independent review passed; human acceptance pending; [contract](playback-tools-spec.md) |
| 5 | Named, device-local saved browse filters within one Profile Scope | Implemented; automated checks and independent review passed; human acceptance pending; [contract](saved-browse-filters-spec.md) |

Later batches must use the established server and playback adapters. They do not
authorize a new pairing service, background Android playback, Android TV, or a
cross-server queue. Saved filters remain independent of Favorites and Watchlist.
All five batches are implemented. Their contracts and automated evidence are
recorded below; actual device and visual acceptance remain separate.

## Saved filters evidence

Desktop and native TV share profile-scoped local definitions and complete
queries across Detail and presentation changes. Focused tests passed 318 core,
106 SDK and 676 iced tests. The final cross-crate pass completed `bun run check`,
1638 independent workspace Rust tests and nonvisual native startup smoke,
with zero failures. The existing ignored MPV lifecycle helper is launched by
its parent tests; the private D-Bus test also executes once in a child and is
not counted twice.

Independent backend and UI review have no remaining findings. Review fixes
cover ordinary TV Clear/Remove query updates, stable record focus after list
replacement, and retry after an interrupted account handoff. Regression tests
exercise actual HTTP queries, retained widget state and SDK handoff hooks.
Clear restores BrowsePreferences defaults while retaining the applied baseline;
deleting an applied definition retains its query and labels it deleted.

Desktop long-text layout, TV directional reading and OSK, real-server missing
libraries and unavailable facets, and account transition feedback remain human
acceptance. Full command logs, file inventory and review reports are linked from
`/tmp/jellypilot-saved-filters-result.md`.

The completed Paper handoff `/tmp/media-streamer-saved-filters-handoff.md`
records 27 new states (13 Desktop, 14 TV), finish OK and independent static
review with no remaining findings. Inventories are Desktop 82, TV 80 and
unchanged Mobile 86. The previous 69/66 IDs remain present. The TV naming form,
complete-condition panel, short-window gaps and OSK Done/Save distinction follow
the native contract; these static references add no native requirements.

## Active playback tools evidence

Desktop and native TV share current-file secondary text subtitles and A/B
configuration through the existing serialized MPV controller. Focused checks
passed 256 MPV tests, 310 core tests and 656 iced tests. The final cross-crate
pass completed `bun run check`, 1602 independent workspace Rust tests and native
startup smoke, with zero failures and one existing ignored test. The private
D-Bus test also executes once in a child.

Independent backend and UI review have no remaining findings. UI review closed
same-file panel close/reopen stale gestures and secondary-Off capability-loss
regressions, with actual widget/input tests. Real subtitle readability, primary
ASS overlap, repeated media loops, remote focus and narrow-window appearance
remain human acceptance. The engine's headless IPC contract and Paper states
are supporting evidence, not native visual or decoder acceptance.

The completed Paper handoff `/tmp/media-streamer-playback-tools-handoff.md`
records 35 new states (18 Desktop, 17 TV), finish OK and static review with no
remaining findings. Inventories are Desktop 69, TV 66 and unchanged Mobile 86.
Desktop's 240px subtitle panel has primary and secondary sections sharing one
280px-max scrolling body; TV retains the role-to-track navigation chain.

## Viewing queue evidence

The Desktop/native TV queue passed 640 focused iced tests, 245 MPV tests and
308 core tests, with focused Clippy. The final cross-crate pass completed
`bun run check` and 1573 independent workspace Rust tests, zero failures and one
existing ignored test; the private D-Bus test also executes once in a child.
The final nonvisual native startup smoke also passed.
Independent backend and UI reviews have no remaining findings. They closed
late-EOF priority, cross-series Intro initialization, keyboard traversal,
editing-focus and pointer-state transfer regressions through observable tests.

The Paper handoff `/tmp/media-streamer-viewing-queue-handoff.md` records 24 new
states (12 Desktop, 12 TV), finish OK, and final inventories of Desktop 51,
TV 49 and unchanged Mobile 86. Desktop sorting focuses Close at a disabled
boundary; TV retains the same disabled editing action. Both keep sorting from
turning into Play or Remove. Native input, real media continuity and visual
acceptance remain separate from these static design references.

## Seek Preview

The first implementation targets the Desktop embedded player's interactive
timeline. Hovering or dragging may display an existing Jellyfin Trickplay image
for the current item and actual media source. It does not decode video or ask
the server to generate new images. Missing metadata, unsupported providers,
loading, and failed requests retain the existing time and real chapter labels.

Preview work is scoped to the authenticated connection and current playback
identity. A replaced item, source, or profile must never display an old image.
Requests, decoded dimensions, response sizes, and memory retention are bounded.
The preview must not contain or log access credentials.

The image fits within 240×135 logical pixels and the measured timeline width,
preserving its source aspect ratio. A 12px corner radius, 8px tooltip padding and
8px image-to-text gap retain the existing 12px monospace time. Real chapter text
may wrap and increase the tooltip height; 256×179 is only a short-caption 16:9
Paper reference. The existing viewport clamp remains authoritative. Loading,
missing-image and failed-image states all use the same time/chapter fallback,
and an earlier candidate image must not be substituted for the current one.

Hovering never seeks. Drag release retains the existing single committed seek;
fullscreen transitions and playback replacement continue to cancel stale drafts.
The tooltip stays within the viewport and its picture does not obscure its time.
Existing 40px seek targets and 44px main transport controls remain unchanged.
Android and TV thumbnail interfaces are not included in this first batch.

## Linux system media controls

MPRIS exposes the current Playback Session through the session D-Bus. Available
transport controls, position, rate and seeking capabilities reflect the actual
player; unsupported capabilities are not advertised. Metadata excludes
credential-bearing media and artwork URLs.

Commands are revalidated against the active authenticated connection and
playback generation before execution. SetPosition additionally checks the track
identity. Profile handoff, stale queued commands, and replacement playback must
not control a different item. Explicit resume of hidden embedded playback must
restore its window through the existing lifecycle route before playing.

MPRIS is Linux-only. A missing session bus must not prevent application startup
or playback. It introduces no second transport state machine or busy polling.
The initial service does not promise MPRIS TrackList, Playlists or OpenUri.

Next/Previous are advertised only while playing and a real next item or previous
episode is available; paused calls cannot accidentally resume playback. Observed rate remains
accurate even when an external MPV configuration exceeds the app's 0.25–4 range.
The declared range includes that observed rate; new settings use MPRIS best-fit
semantics within the app's supported range. Successful seek settlement emits
Seeked only once, and rejected or stale receipts cannot produce a new signal.

A missing bus, another owner of the service name, or a lost bus connection
disables this integration until application restart. Name acquisition neither
replaces another instance nor queues behind it. The startup smoke path omits the
live bus integration; protocol tests use an isolated D-Bus service and client.

## Embedded player progressive blur

Removed at the user's request on 2026-10-05. Full controls use the existing
340px bottom scrim without requesting video sampling or a blur effect. The
player-specific blur parameters and backdrop widget integration have been
removed. The scrim remains below every control, including in short windows.
Minimal/Full visibility, independent top controls and Floating Popovers,
focus/drag retention, seek previews and input semantics are unchanged.

The compatible iced revision `3cf722cd8c33ca2afe1ae88cfc8ce20f31fd1a2c` remains
pinned. Its published `player-progressive-backdrop` branch and prior remote cold
preparation are historical dependency evidence, not an enabled player effect.
Other accepted image/modal blur treatments remain independent of this removal.
The earlier Paper blur states are superseded for the native player.

## Verification and acceptance

Follow [the validation policy](agents/validation.md): focused behavior tests and
clippy first; cross-crate changes require the suite tier, and subscription/window
wiring also requires native smoke. Review must specifically check stale playback
commands, profile isolation, bounded thumbnail decoding, and hidden-window
resume. Native appearance, actual media/control integration, display color and
device performance remain human acceptance; neither Paper nor smoke proves them.

Preserve unrelated Android and other local changes. Commit completed slices
with explicit paths; no JellyPilot push is authorized. The later explicit
approval covers only the iced `player-progressive-backdrop` candidate branch.

### First-batch evidence

The preview and MPRIS additions passed `bun run check`, workspace Rust tests
(1519 tests, one existing ignored test; the D-Bus test also executes once in a
private child process), and native startup smoke. After the final MPRIS receipt
and signal refinements, the affected iced checks passed again: 624 tests
(163 UI, 10 embedded-host, 451 app), focused clippy and format checks.

Independent preview review found no Standards or Spec issues. MPRIS review found
small-seek notification, paused next/previous, and rate-range inconsistencies;
all were fixed. Follow-up review also caught duplicate settlement notification,
which now records only an actually applied controller effect and has a replay
regression. Final review has no remaining findings.

The first-batch Paper handoff is `/tmp/media-streamer-player-features-handoff.md`:
nine new Desktop boards, 39 total, finish returned OK with token hash `28f17849`.
Its [contract board](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/1-0/QMK-0)
records the adaptive preview and provisional blur material. The preview contract
remains current; the player blur material is superseded by the removal above.
Paper completion is separate from native human acceptance.
