# Active Playback Tools

Accepted 2026-10-05 under the user's delegated batch-4 decisions. This batch
adds secondary text subtitles and an A/B loop to Desktop and native TV, using
the existing Playback Session and MPV controller. Android's player and remote
controller are not extended in this batch. Detail-page track information stays
read-only.

Implementation and independent backend/UI review are complete. Focused tests,
the cross-crate suite and nonvisual startup smoke passed; see
[delivery evidence](playback-features-spec.md#active-playback-tools-evidence).
Human media and visual acceptance remains pending.

## Secondary text subtitles

The existing subtitle menu distinguishes Primary subtitle and Secondary text
subtitle. Primary selection and its saved preference retain their current
meaning. Secondary selection is temporary for the current media, defaults to
Off, and is never written as the primary preference or reported as the server's
primary subtitle stream.

Offer only tracks that MPV actually loaded and whose codec identifies a text
format. Unknown and bitmap formats are not text candidates. A secondary track
must differ from the selected primary track, and dual-text selection requires
a known text primary. If the primary is turned off or changed to a non-text
track, clear the secondary selection. Promoting the current secondary track to
primary clears that secondary role before selecting it as primary. Re-read
the roles after mutation: a successful MPV command alone does not prove that
selection changed.

The secondary text uses MPV's existing upper-screen placement and stripped ASS
styling. Preserve primary styling and subtitle preferences. This is not an
automatic subtitle collision solver; overlapping primary ASS positioning,
long lines, aspect ratios and subtitles inside the player blur region require
human acceptance. Missing eligible tracks show an explanation, not dummy
choices. Busy, unavailable and failed reads remain distinct from Off.
If an external MPV action removes the known text primary while a secondary
selection remains configured, Off stays available. Only selecting a new
secondary track requires an eligible primary; capability loss must not prevent
clearing an existing selection.

Use file-local ownership for secondary settings. Stop, media replacement and
profile retirement clear the app's selection and stale requests. Do not allow
a late command for a previous media generation to select the same numeric
track ID in its replacement. Existing provider external-subtitle loading is
reused; this batch does not add local-file import, transcription, translation,
subtitle search or per-track styling controls.

## A/B loop

The playback-tools panel offers Mark A, Mark B, repeat on/off, Restart from A,
and Clear. Mark A captures the displayed current media position and starts a
new interval, clearing B. Mark B requires a valid A and a later position; it
installs that interval for unlimited repetition after confirmed settlement.
Marking either point never seeks or resumes. Restart from A is the only loop
action that explicitly seeks, and it preserves pause state.

Enable interval editing only for an active, fully seekable media with a known
finite positive duration. Require finite points with 0 <= A < B <= duration.
Unknown or cache-only seekability is unavailable for this first implementation;
do not infer whole-file seeking from duration or a buffered range. Show unset
points as an em dash and explain unavailable controls. Disabling repeat retains
the points; Clear removes both. Turning repeat back on reinstalls B according
to MPV's confirmed property ordering, without implicitly jumping to A.

If seekability or duration later becomes unavailable, disable new point marks,
Enable and Restart. Existing repeat can still be disabled and existing points
cleared while their properties and current-file identity remain available.
Refresh and Close remain reachable; losing seek capability must not trap the
user in an already configured interval.

The panel shows observed configuration, not a claim that a decoder has already
completed a loop. Seeking past B does not promise an immediate jump back;
Restart from A provides the explicit action. Failed writes/readback expose
feedback and refresh actual state rather than optimistically claiming success.
No configurable repeat count, persistent presets or automatic pause is added.

Loop properties belong to the current media through file-local options, and
are reset for each app-controlled load. Existing global MPV options must not
leak a previous app-owned interval into the next item. Closing the panel leaves
the interval active; Stop/replacement/profile retirement retire it. Window
close keeps the existing suspension contract and cannot resume playback.

## Shared controller and interface contract

All mutations use the existing serialized controller route and playback
identity/admission checks. Revalidate track IDs, ranges and active media before
touching MPV; late results cannot update another media's UI. A/B writes are
ordered, and no second polling loop, playback state machine or host ABI is
introduced. Missing properties disable only the affected feature.

Desktop uses existing independent Floating Popovers, reachable Close controls,
40px minimum targets, bounded scrolling and narrow-window wrapping. Subtitle
menus retain their 240px width and existing track-list scrolling. A/B uses
min(320px, viewport width minus 32px), with no forced minimum; the complete
panel including padding is height-constrained, with Close outside the scrolling
body. These Desktop tools render above an inert background: Subtitles is
anchored at the surface's lower-right 16px safe margin and opens upward; A/B is
anchored at its upper-right 16px margin and opens downward. They do not track
the triggering button's exact horizontal position. TV panels retain 744px times
the existing scale, 96px/60px horizontal/vertical safe margins, and 40px padding /
24px gaps at 1080p. Close remains fixed
and the body scrolls. In windows shorter than 480px times the scale, TV reduces
padding to 16px and the header gap to 12px while retaining its 64px reference
controls and a reachable scrolling body. TV uses the established 64px-at-1080p
controls and remote focus rules. Keep the existing
main control bar: subtitle tools are reached through Subtitles, and A/B through
Playback settings. Desktop embedded playback keeps its existing playback-options
gear available at wide as well as narrow widths. The external/control-only
player's existing settings gear opens a small menu containing playback tools
and application settings; the existing global settings shortcuts and Sidebar
entry retain their behavior. TV's subtitle role page opens a primary or secondary
track subpage; Back returns to that role. A/B Back returns to its Settings entry.
Close exits the panel chain and restores the invoking main control. A pending
operation retains focus; successful selection retains the action or a safe Close
target. Playback shortcuts do not
leak through an open panel, and opening a panel does not change pause state.

Tests must exercise actual adapter commands/readback, primary-secondary role
conflicts, unsupported codecs, invalid/reversed points, disable/re-enable,
partial failure, stale media/profile work and next-media cleanup. Interface
tests cover unknown/busy states, real track identity and keyboard/TV navigation.
Apply the cross-crate Suite and native smoke tiers. The engine's existing
headless IPC probe is supporting evidence; real dual-text readability and
repeated seeks in the native player remain human acceptance.
