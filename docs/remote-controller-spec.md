# Android Remote Controller

Accepted addition, 2026-10-05. Implemented; automated verification and independent
review are complete, with real-device integration acceptance pending. This is batch 2 of
the [playback feature delivery plan](playback-features-spec.md), and is distinct
from Android's existing foreground Playback Target receiver.

## User flow

Account offers a Remote control entry. The user chooses an available target on
the current media server and sees that target's real Now Playing information.
Pause, resume, stop, seek and volume are available only when supported by that
target. Details offer an explicit Play on another device action for a playable
movie or episode. Ordinary Play continues to use the local player; choosing a
remote target does not silently replace the application's default playback path.

Both entries open the same independent, back-navigable page. The detail entry
retains the real item and start position while the user chooses a target, then
requires an explicit Play on this device action; selection never sends PlayNow.
The page identifies the current server and target, uses at least 48dp button and
slider targets, and remains a scrollable single column centered on tablets.

The first snapshot exposes text and playback state, without a media-image
reference. Unknown pause state does not become an invented Play/Pause action;
unknown position or volume displays an em dash. Each known capability remains
independent, so Stop may still be available when pause state is unknown.

The phone sends one movie or episode with PlayNow, using the selected detail's
real playback identity and applicable start position. It does not prepare local
playback, open a local video surface, or send local playback reports. A series
without a real playback target cannot be sent as an arbitrary first episode.

The first version supports one selected target, within the current authenticated
server connection. It does not add pairing, discovery across servers, a direct
phone-to-PC socket, TV key emulation, or a server-independent remote service.

## Target discovery and commands

Use each provider's existing generated Sessions and command APIs through the
media-server facade and shared SDK. Queries fix ControllableByUserId to the
authenticated user, and exclude the local device identity. A target need not
have the same signed-in user when the server explicitly authorizes its control.
The local receiver's registration capability is not a permission to control
another device, and must not disable otherwise authorized discovery.

Target identity includes session ID and device ID. Selection is accepted only
from the current server-authorized result. Missing targets, rejected permission,
and failed refreshes disable stale controls and expose a retryable state.
Jellyfin's IsActive and Emby's available session fields are interpreted according
to their actual contracts; do not invent a shared online flag unsupported by a
provider.

Now Playing and position come from the selected target's session snapshot.
Seek requires an actual seekable target and valid duration. Volume requires the
target's SetVolume capability and stays within 0–100. Slider drags preview only;
release sends one command. Commands are serialized and acknowledged feedback is
distinct from the refreshed target state: HTTP acceptance alone does not prove
that the target started or reached the requested position.

Accepted feedback means Command sent / Updating. Actual playing state continues
to come from refreshed snapshots. A lost target or failed refresh disables the
previous controls until retry and fresh authorization succeed.

## Scope and lifecycle

The SDK binds a controller to the authenticated scope epoch, its own generation,
and the selected target. Recheck write admission after target refresh and before
sending a command; an account handoff or pending credential cleanup must block
new writes. Closing the controller, replacing its target or scope, backgrounding,
and device locking cancel waiting work. A command already accepted by the server
cannot be recalled, and the interface must not claim that it was.

Generations are unique across controller instances in the process, and changing
the selected target's Now Playing identity also retires the previous generation.
Buttons capture their rendered generation and target; sliders retain the identity
from the start of the drag. Neither silently retargets an old action at release.

Refresh only while the controller page is visible and the phone is unlocked,
with a bounded interval and at most one refresh in flight. Refresh after accepted
commands. Returning to the page revalidates the target and never replays old
commands. This feature does not change local playback lifecycle or allow Android
background playback.

Use existing provider authentication headers and redirect policy. Never return
credentials or stream URLs in UI target records or diagnostics. Jellyfin UUID
item identifiers and Emby's generated numeric identifiers retain their provider
validation and serialization; do not build one untyped HTTP command for both.

## Verification

Provider HTTP fixtures cover permission filters, authentication headers, target
identity, PlayNow, seek ticks, general-command arguments, and rejected/failed
requests. SDK tests coordinate target/profile changes at async boundaries and
prove that obsolete commands do not reach a new target. Android host tests cover
selection, capability-driven actions, proper item identity, page/visibility
lifetime and unchanged local playback. Follow the cross-crate suite tier and
`bun run task android check`.

Real phone-to-PC/TV control, server permission policy and network timing require
human integration acceptance. No agent screenshots or native input injection.

The focused provider suite passes 236 tests and SDK passes 98, including 12 new
remote-controller scenarios. The FFI compiles and the three Rust crates pass
Clippy. `bun run task android check` passes 117 host tests (108 app, 9 player),
including 17 new controller, entry and screen tests, and Android lint. These are
host checks, not actual phone-to-target playback evidence.

Independent Rust review found no remaining Standards or Spec issues. Android
review caught the server heading using a username for an unsaved login; the page
now uses the actual Active Profile's server name, falling back to its provider.
The fix was independently rechecked and included in the final Android check.

The final shared pass with the progressive-blur addition also passes
`bun run check`, workspace Rust tests (1537 distinct tests, one existing ignored;
the D-Bus case runs once more in a private child), and native startup smoke.
