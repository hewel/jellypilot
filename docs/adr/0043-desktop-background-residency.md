# Separate desktop Background Residency from App Mode

_Status: Accepted, 2026-09-15. Implemented and code-level validated; desktop/window-manager acceptance remains manual. Amends ADR 0030's Full-mode close lifecycle and establishes the same close/playback policy for both desktop App Modes. Android is unchanged._

[ADR 0030](0030-resource-bounded-control-only-composition.md) destroys the Control-Only window but leaves Full on a hidden-window path. [ADR 0031](0031-linux-control-only-resource-budgets.md#wayland-observation-manual-not-ndjson-evidence) records a niri/Wayland observation where that hidden window remained mapped. We choose actual window closure with a persistent runtime in both modes, while preserving Full's browsing context rather than turning Background Residency into a switch to Control-Only.

## Decision

- Closing the main window with a usable tray enters Background Residency. Explicit Quit Application ends the Playback Session, live connection, tray, and application through orderly shutdown. If the tray cannot initialize, preserve the existing safe-exit behavior rather than leaving an inaccessible process. Neither close nor quit waits for an exit animation.
- Close the actual window and release its rendering resources instead of relying on `Mode::Hidden`. Suspend invisible UI animation clocks and image loading/decoding demand. Keep authentication, the remote Playback Target, tray, and playback runtime available; do not change the saved App Mode.
- Full retains its current page, navigation history, filters, and scroll position. Preserve unsaved Settings drafts, but dismiss dialogs, popovers, and menus. Tray Show or a second launch restores that browsing context with current runtime state and on-demand images; retaining every decoded image or GPU cache is not a requirement. Control-Only remains browser-free. Existing single-instance ownership and Control-Only resource obligations remain unchanged; this decision does not claim Full meets Control-Only's memory budgets.
- On main-window close, External MPV Playback continues in its independent player window. Embedded MPV Playback pauses while retaining its Playback Session. Showing the window alone does not resume it. This playback rule is identical in Full and Control-Only.
- An explicit remote or tray Play/Resume command, or a remote request to play a new item, restores a visible embedded player before starting or resuming playback. Failed window restoration leaves embedded playback paused and reports the failure; there is no headless audio fallback. A request to show the player for explicit playback is distinct from ordinary Tray Show restoring the previous browsing context.
- Pause, Stop, volume, and seek commands do not open the main window. Automatic reconnection and late callbacks cannot wake playback or bypass the pause caused by close. External playback keeps its independent-window behavior.
- This decision does not introduce pause-on-blur, pause-on-minimize, or lock-screen policies. It does not change Android playback visibility, introduce a new process boundary, or require a new App Mode or close-behavior preference.

## Trade-offs

Keeping `Mode::Hidden` would preserve a native window but would retain the known problematic lifecycle. Reusing Control-Only by switching modes would drop Full's Library Browser state, violating the chosen restore experience. Dropping Full state and reopening at Video Home would reduce retention further but lose navigation and scroll context. Retaining lightweight browsing context while discarding window rendering resources chooses continuity over minimum background memory.

Pausing embedded playback avoids silently consuming video while no picture is visible; continuing External MPV respects its independently visible window. Explicit play restores the embedded surface so the remote Playback Target stays useful without granting automatic reconnect or stale work permission to restart playback.

## Implementation

- A requested embedded close waits for the MPV pause acknowledgement before destroying the window. The visible player, including fullscreen, remains in place with UI input disabled while waiting. Pause failure keeps the window visible and reports the failure without fabricating a paused transport state.
- Each close permanently revokes the presentation lease captured by earlier controller commands. The serialized IPC writer checks the lease before writing an unpause; the close pause is synchronously queued independently of the controller mutex. Session suspension cancels queued starts/resumes and pending adjacent authorization, and prevents automatic advancement.
- Close acknowledgements are bound to the window and close generation. Show cancels a pending close; older acknowledgements and open callbacks cannot close or replace a newer window lifecycle. An already-destroyed compositor window can only issue its pause after the loss.
- Explicit playback admission selects the player before dispatch. An admitted first start constructs the embedded video surface without waiting for a completed controller snapshot. Deferred tray and remote toggles that meant Resume retain that meaning across newer transport snapshots.

## Implementation acceptance

The implementation passed `bun run check`, the maintained Rust suites (`bun run task rust test`), focused desktop regressions, and `xvfb-run -a bun run task iced run --smoke`. Automated coverage includes wire-level lease revocation and queue ordering, session suspension, close/admission generations, restoration state, and invisible frame demand. The following end-to-end desktop checks remain human acceptance:

- In each App Mode, close the actual window, retain one tray and Playback Target, then restore via Tray Show and second launch without creating a duplicate runtime. In Full, verify page/history/filter/scroll and Settings-draft restoration, dismissed transient surfaces, and resumed on-demand images.
- Cover close during embedded playback, while already paused, and while a play request is in flight; late work must not undo the close pause. Show alone stays paused. Explicit Play/Resume/new-item commands require a visible player before playback; failed restoration stays paused and produces an error. Non-play commands and reconnect do not open a window.
- Verify External MPV continues on main-window close, explicit Quit ends playback and the connection, and tray-unavailable close exits safely. Confirm invisible UI has no motion frame loop or image demand.
- Follow the applicable focused/suite and native-boundary gates in the [validation policy](../agents/validation.md). Startup smoke alone does not prove tray, window lifecycle, retained context, or playback ordering. Desktop appearance and window-manager behavior require human acceptance. The fallback uses tray initialization success; that does not prove a desktop tray host actually displays a usable icon.

The [desktop motion contract](../design-system.md#2026-09-15-desktop-motion-contract) separately defines animation scope, timing, interruption, and reduced-motion behavior.
