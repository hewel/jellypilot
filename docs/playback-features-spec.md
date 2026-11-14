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

The Desktop embedded player's complete timeline uses the shared
`iced::widget::timeline` widget and the standalone `iced_mpv_preview` decoder.
Existing server Trickplay metadata and atlases take priority for the current item
and actual media source. Unavailable metadata or atlas images permit independent,
on-demand decoding only when the current source is fully seekable and the decoder
is available. Loading and decoder failure retain time and real chapter/range
labels. Authentication/session failures block fallback rather than initiating
another authenticated read. Nonseekable streams use time-only hints; an unknown
or invalid duration retains the elapsed-time display instead of inventing a range.
No request asks the server to generate Trickplay images.

The decoder uses the playback controller's already-resolved `AuthenticatedUrl`,
borrowed only while binding a revocable source capability. It preserves query
credentials verbatim: no new headers, URL synthesis, playback negotiation, or
main stream/transcode-session changes occur on hover. Targets and returned frame
times use the main mpv normalized timeline unchanged, including mpv's default
HLS/TS start-time rebasing. Do not add raw PTS or infer offsets from URLs.

Preview work belongs to the authenticated session, actual playback file and source
lease generation. Source admission, decoder binding and mailbox mutation follow
that lock order. Revocation immediately invalidates the matching mailbox before
terminal cleanup awaits; an old source cannot clear a newer binding. Native work
already admitted into FFI is not interruptible, but revoked work cannot be newly
admitted or published. Cancellation and source retirement clear the displayed
image. Callback owners are released outside the source lock, and the runner keeps
the decoder owner alive until the iced UI runtime has returned.

Requests, decoded dimensions, response sizes and retained frames are bounded.
Hover cancellation can retain reusable media/cache state; source replacement
retires it. A failed decoder generation does not retry on every target change.
Transient command-busy periods cancel image demand without changing the media
generation or keyboard focus. Window exit retires the decoder binding before
discarding its presentation state; reopening cannot reuse an old timeline generation.
Neither preview identities, diagnostics nor rendered content expose credentials.

Images preserve aspect ratio inside a fixed 240×135 logical-pixel slot, shrinking
proportionally only when the measured timeline is narrower. The normal card is
256×179: 8px padding, an 8px image-to-text gap and a fixed 20px line with bold
14px monospace time and real chapter/range labels. Long labels truncate rather
than changing height. The explicit Paper contours are 12px outside and 8px on
the image; they do not change shared radius tokens. The opaque `#191A21` surface
has a black 45% shadow at y=8px with 24px blur; no video blur is sampled.
The shared widget anchors above the visual rail, not pointer Y, and clamps to
the viewport. Card bottom clearance is 22px from the 4px rail's center (24px
from its bottom edge), implemented as 2px above the 40px input region. New image
aspect ratios cannot move the card top. No stale image substitutes for a pending
or failed target; absent images keep time/real-label hints. Genuine buffer,
chapter and intro markers retain their existing data sources. Volume remains
an independent slider, now opened from the compact control row.

Hover never seeks. Pointer/touch drag release commits one seek only when MPV
reports `seekable=true` and `partially-seekable=false`. Focused timeline arrows
commit one-second steps, Shift uses ten seconds, and Home/End select range
endpoints. A temporarily disabled, already-focused timeline consumes these seek
keys without commands or global-shortcut fallthrough; enabled increments resume
after the command settles. Wheel motion does not seek through the timeline.
Escape/Tab, focus or touch loss, hiding,
fullscreen transitions and playback replacement cancel unfinished demand without
undoing an already-submitted seek or an independent volume gesture. Hidden or
removed widgets cannot publish cancellation themselves, so product lifecycle
handling retires their demand. The 40px timeline target and 44px main transport
remain unchanged. Android, TV and external-player thumbnail interfaces are outside
this integration.

The native decoder adapter is available for Linux/Windows embedded builds when
their configured libmpv can provide it; this is not a cross-platform runtime
acceptance claim. The published source pin now includes the shared widget and
preview package, and all four temporary consumer paths were restored. The complete
materialized source matched the accepted candidate, so its provider/input/lifecycle
matrix was reused. The formal-source binary additionally exercised independent
stream previews while unpaused: the main clock advanced without another main seek,
PlaybackInfo request or main-stream request, followed by normal Quit and preview
worker termination. Acceptance used authenticated HTTP fixtures and a GPU-backed
headless Gamescope/Xwayland compositor on Linux. This does not establish production
Jellyfin authorization, physical Niri behavior, display color or other platforms.

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

The `player-progressive-backdrop` branch, revision
`3cf722cd8c33ca2afe1ae88cfc8ce20f31fd1a2c` and its prior remote cold preparation
are historical dependency evidence, not the current source pin or an enabled player
effect. Current published pins are recorded in `tools/embedded-mpv/*source.json`.
Other accepted image/modal blur treatments remain independent of this removal.
The earlier Paper blur states are superseded for the native player.

## Native picture-in-picture

Embedded Playback can transfer its existing video presentation to one auxiliary native
window from **More → Picture-in-picture**. It retains the same MPV host, media,
Playback Session, position and track selections; it does not reload media, change Playback
Backend or create another App Mode. Only one window controls the video copy target,
presentation color target and presentation-surface idle binding at a time.

- Full mode keeps the main library usable while the small window plays. Control-Only
  retains its selected mode. Entering TV mode first returns video to the main player.
- Return (also Escape in the small window) waits for the main presentation to become
  ready before retiring the small window. A closed main window is reopened; prior
  fullscreen state is restored. Playing stays playing and paused stays paused.
- Close pauses and waits for the controller acknowledgement before closing. Failed
  pause retains the small window and reports the error. A newer explicit Pause cancels
  any deferred Resume rather than replaying it when a window later opens.
- While the selected PiP window remains, closing main does not pause or quit, even
  without a tray. This is the multi-window exception to the main-only close behavior
  recorded in ADR 0043. Explicit Quit still ends playback and all windows.
- Opening failure leaves or restores a live main presentation. If main has disappeared
  after selection, the small window is retained until pause succeeds. Failed Return
  keeps the small window rather than leaving unpaused playback without a presentation.
- Window IDs, transition generations and playback-session/media identity fence stale
  callbacks and controls. End of playback, account handoff and Quit retire PiP.

The entry is limited to Linux and Windows embedded builds outside TV mode. External MPV
keeps its independent player window; macOS does not advertise this embedded feature.
Window-manager placement is best effort: Wayland does not grant the application arbitrary
positioning or portable always-on-top control. On niri, make the window floating manually
or match its stable title `JellyPilot — Picture-in-Picture` in a floating window rule.
This does not provide cross-workspace stickiness. No desktop configuration is changed.

See the [small-window design contract](design-system.md#native-picture-in-picture)
for controls and the manual acceptance checklist. Automated lifecycle tests and the
native dual-surface GPU probe cover different boundaries: the probe transfers a real
decoded picture through the presentation API, not the production PiP coordinator or
desktop pointer interactions.

Validation passed: workspace `bun run check` and Rust tests, focused iced tests/Clippy
after the presentation-initialization correction, and isolated native startup smoke.
The clean native GPU run `2dc58f8d-f0e1-4a71-917e-6ebbf64f57ea` passed both video
transfers, last-window close/reopen and teardown with exactly one host created and
dropped; its report is `target/pip-native-regression-final/report.json`. This run used
`assets/promo/demo.webm` with `--hwdec no`, not hardware-decoder acceptance.
Independent lifecycle and presentation reviews have no remaining established findings.

Two earlier native runs timed out in paused-frame settling before PiP; a diagnostic run
and the final run without instrumentation passed. The timeout cause was not established,
so this evidence does not establish absence of intermittent presentation stalls.
Desktop appearance/placement and Windows runtime behavior remain human acceptance.

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
records the earlier adaptive preview and provisional blur material. The fixed-slot
preview contract above supersedes its variable-height geometry; the player blur
material remains removed. Paper completion is separate from native human acceptance.
