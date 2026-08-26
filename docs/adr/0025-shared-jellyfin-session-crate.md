# Extract the Jellyfin session protocol into a shared crate

_Status: Accepted. Amends ADR 0022's adapter placement; implements ADR 0024's shared-session decision._

## Context

ADR 0022 kept the Jellyfin `SessionManager`, WebSocket command channel, and playback-event
handling inside `src-tauri`, with extraction deferred "only when a live GTK vertical slice
consumes it." ADR 0024 decided that the GTK Playback Target slice is that consumer and that the
session behavior must not be duplicated per frontend.

The Tauri session code separates cleanly: the WebSocket protocol implementation carried no
Tauri imports, and the Intro Skipper seek/prompt evaluation depended only on playback state and
message types. The surrounding orchestration — frontend event emission, persisted
configuration, HLS proxy lifecycle, embedded player integration, and playback reporting
cadence — is inseparable from Tauri's application shell.

## Decision

Add `crates/jellypilot-session` as the shared, Tauri-free owner of the Jellyfin session
protocol surface:

- `JellyfinWebSocket` and its command/event types (`JellyfinCommand`,
  `JellyfinWebSocketEvent`): connect, restart, disconnect, event receiver, the exact
  `SessionsStart` handshake payload (`{"MessageType":"SessionsStart","Data":"1000,1000"}`),
  30-second `KeepAlive` cadence, and reconnect/cancellation semantics, moved verbatim from
  `src-tauri/src/jellyfin/websocket.rs` with their tests.
- Frontend-neutral Intro Skipper evaluation (`IntroSkipMode` × time position × ranges →
  `IntroSkipAction` seek or prompt), moved from `src-tauri/src/jellyfin/playback_events.rs`
  so each frontend applies the decision through its own MPV path (Tauri via its `MpvAction`
  channel, GTK via `MpvClient`).

The Tauri application keeps its orchestration unchanged: `SessionManager` lifecycle,
`AppHandle`/`tauri_specta` event emission, Tauri Store configuration, HLS proxy lifecycle,
embedded player integration, playback reporting cadence, and the command `State` boundary.
`src-tauri/src/jellyfin/mod.rs` keeps its existing public re-export boundary.

The GTK frontend builds its Playback Target consumer on `jellypilot-session`,
`jellypilot-media-server`, and `jellypilot-mpv` with its own MPV-only orchestration: it does
not acquire HLS, embedded playback, or engine routing concerns.

## Consequences

- Session protocol and security fixes land once in `jellypilot-session` and reach both
  frontends; the ADR 0022 "extract only with a live consumer" condition is satisfied.
- Orchestration remains intentionally per-frontend because Tauri's engine routing and GTK's
  MPV-only playback differ; what is shared is protocol fidelity and skip evaluation, not
  application flow.
- The shared crate carries no `tauri`, `tauri-specta`, or store dependencies; CI format and
  lint package lists cover it.
- WebSocket handshake behavior is pinned by an explicit payload assertion in the moved tests;
  any future change to remote-control capability declaration is a visible, reviewable event.
