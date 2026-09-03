# Make Control-Only a resource-bounded UI composition

_Status: Accepted. Amends ADR 0029's Control-Only seam and keeps ADR 0027's single iced frontend._

Control-Only is JellyPilot's lowest-idle-overhead App Mode, not a smaller rendering branch over the full application state. It keeps complete Now Playing, Login, Settings, tray, and remote Playback Target behavior while excluding the Library Browser, and switching modes must not interrupt the current Playback Session, MPV IPC, remote Playback Target, or tray identity.

## Decision

Use one OS process with a stable runtime and two mutually exclusive UI compositions. The runtime exclusively owns authentication and Saved Service Profiles, the media-server connection, remote Playback Target, Playback Session and MPV control, Now Playing, tray, persisted settings, and diagnostics. `FullUi` owns Home, Browse, and Detail state; `ControlUi` owns only the complete controller surface. Entering Control-Only destroys `FullUi` and its Library Browser work. A small shared UI state preserves Login and Settings drafts without retaining the old UI composition.

Only Control-Only changes its close lifecycle. Closing its window destroys the window and suspends window events, frame clocks, UI ticks, and Artwork fetching or decoding while the iced daemon and runtime continue without a window. Tray Show recreates the controller from the current runtime snapshot and must present an interactive first frame within one second. Full mode keeps its existing close-to-hidden behavior.

The runtime is single-instance. A second launch activates the existing instance and exits instead of creating a duplicate tray, Playback Target, or settings writer. If the tray cannot initialize, JellyPilot reports the failure and a window close performs the existing orderly MPV and remote-session shutdown; it must not leave a background process with no reopening path.

## Resource gate

Before measuring any candidate, define a reproducible Linux protocol for two signed-in, no-playback states: window visible and zero-window. The protocol fixes the repetition count, a five-minute stabilization period, a five-minute observation period, aggregation, and the measurement-noise calculation for RSS/PSS, CPU time and wakeups, and attributable GPU use on the same host and desktop session. Measure the current implementation in every state it can actually produce — the current code has no zero-window state, so its baseline is window-visible only — then freeze per-state absolute and relative budgets before evaluating the candidate under the same protocol. The candidate passes only when it meets every frozen budget without regressing behavior or the one-second reopen limit. Keep the protocol as a local release check rather than a noisy absolute CI gate.

The Linux baseline and frozen budgets from this gate are recorded in ADR 0031.

If the same-process composition and zero-window daemon miss any frozen budget, reopen the process boundary and compare a persistent core with an exit-capable UI client and lighter tray ownership; cost attribution guides that comparison but does not waive the escalation. Do not introduce a second UI toolkit merely to preserve the one-process shape.
