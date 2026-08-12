# Share proven media-server and MPV adapters across native frontends

_Status: Accepted. Refines ADR 0021 while retaining frontend-owned session lifecycles._

The live GTK Library Browser makes JellyPilot's Jellyfin/Emby HTTP client and external MPV JSON
IPC transport proven multi-frontend dependencies. Keeping either implementation under `src-tauri`
would make the GTK application depend on a Tauri-owned filesystem boundary or duplicate
authentication, playback reporting, process control, and security behavior.

Move the concrete, UI-independent Jellyfin/Emby client into `jellypilot-media-server` and the
external MPV process/IPC implementation into `jellypilot-mpv`. Both crates expose narrow domain
facets rather than framework traits: login, library, and playback for media-server I/O, and process
plus transport control for MPV. Tauri keeps compatibility reexports while its commands,
`SessionManager`, persistence, HLS, event delivery, and embedded player remain Tauri adapters. GTK
owns its Relm4 messages, widget lifetimes, request correlation, and one-current-item playback
controller.

Library Images remain opaque signed references. A frontend asks the media-server library facet to
prepare and fetch a reference; the client validates its provider, active server origin, scheme,
port, and reverse-proxy base path immediately before attaching the current token. Authenticated
image requests never follow redirects. Raw authenticated HTTP helpers and token-bearing URLs are
not public adapter APIs.

MPV startup is transactional and serialized. A failed IPC handshake reaps the child and cleans the
endpoint before returning, repeated starts cannot replace a live child, and command or process logs
exclude argument values and authenticated media URLs.

## Consequences

- GTK and Tauri share provider behavior and MPV security fixes without sharing UI or application
  lifecycle code.
- The generated provider SDKs keep their existing paths until their generator and ADR are changed;
  their location does not make the shared client depend on Tauri at runtime.
- Saved Service Profiles, Quick Connect, Playback Engine Preference, embedded playback, and release
  packaging remain explicit GTK migration gates. Tauri remains the production application until
  those gates and native runtime acceptance reach parity.
