# Use Embedded Web Playback by default with local FFmpeg HLS

_Status: Accepted. Supersedes the external-player-only parts of ADR 0004 and ADR 0006; their connection prerequisite and unified authenticated shell decisions remain in force._

JellyPilot will start new Playback Sessions with Embedded Web Playback by default. External MPV Playback remains an optional engine for users who want their MPV configuration, scripts, and shaders. This changes the earlier assumption that every Library Browser launch and Playback Target session must run through external MPV, without deleting the history or the still-valid parts of those decisions.

## Engine selection

The persisted Playback Engine Preference is either Embedded Web or External MPV and applies only when the next Playback Session starts. Changing it never migrates the current session. A launch may carry a Playback Engine Override without changing the preference.

Embedded startup, codec, or runtime failure never switches engines automatically. JellyPilot shows the failure and offers an explicit **Play in MPV** action that starts a new External MPV Playback session from the last known position. That action does not change the global preference. This keeps engine changes observable and prevents a missing codec or WebView regression from unexpectedly opening an external process.

## Embedded media path

The embedded engine does not request a Provider Transcode from Jellyfin or Emby. It obtains the selected original/direct media source and runs a local FFmpeg process that produces a rolling HLS presentation with fragmented MP4 segments. Segments target four seconds and the retained playlist window is approximately sixty seconds. Playback reporting still identifies the original media-server item and selected source; Local Transcode is an implementation detail of the Playback Target, not a server playback mode.

The first slice supports Movies and Episodes with one initial audio selection:

- SDR output is H.264 with `yuv420p` pixel format.
- HDR output preserves HEVC Main 10 HDR and does not tone-map to SDR.
- Audio output is AAC at 192 kbit/s for stereo or 384 kbit/s for multichannel.
- The existing audio preference selects the initial stream; subtitles are disabled.

The first slice excludes audio-only media, live TV, DRM, subtitles, in-session track switching, Intro Skipper, next/previous episode commands, automatic next episode, and adaptive bitrate ladders. These are explicit scope boundaries, not silent fallbacks to a Provider Transcode.

HDR HEVC is capability-gated. If the active WebView cannot play the preserved HEVC Main 10 HLS output, JellyPilot fails visibly and offers Explicit MPV Fallback. It does not tone-map, request a Provider Transcode, or silently change engine.

## Loopback proxy security

One `127.0.0.1`-only service may host both proxy capabilities, but their authority remains separate:

- `/source/<nonce>` streams only the selected provider source to FFmpeg, forwards byte ranges, and keeps provider credentials out of browser-visible URLs, FFmpeg arguments, and TypeScript state. This route is FFmpeg-only and does not enable browser CORS.
- `/hls/<nonce>/<allowlisted-file>` serves only the generated playlist, initialization fragment, and media segments with the correct MIME type. It accepts only the production Tauri origins and the loopback development origin; wildcard CORS is forbidden.

Every Local Transcode generation uses independent cryptographically random source and HLS nonces, so the browser-visible HLS capability cannot authorize the provider-source route. Both namespaces reject path traversal, unknown routes, unknown files, expired generations, and unexpected origins. Session teardown revokes both capabilities and removes rolling output. The nonces are capability material: they must not be logged, persisted, reused across sessions, or exposed outside their respective local media paths.

## Packaging and licensing

Embedded Web Playback requires a compatible FFmpeg executable. Release packages must bundle one or declare an explicit runtime dependency and must verify the expected executable rather than silently falling back to a Provider Transcode. MPV is optional unless the user selects External MPV Playback.

FFmpeg is normally LGPL-licensed, while builds that enable GPL components are governed by the GPL for that FFmpeg binary. Packagers must record the exact FFmpeg build/configuration they distribute and provide the corresponding notices, license text, and source offer required by that build; `--enable-nonfree` output is not a redistributable release option. JellyPilot's own source remains MIT-licensed and invokes FFmpeg as a separate process. See [FFmpeg's license and legal considerations](https://ffmpeg.org/legal.html); packagers remain responsible for their distribution's compliance.

## Consequences

- Embedded playback becomes the primary product path without removing the MPV-centered workflow.
- Server CPU policy no longer decides whether the first embedded slice can play a source; local device capability and packaged FFmpeg do.
- The app must report WebView codec capability failures before presenting playback as started.
- Playback lifecycle, progress, stop, and resume logic must be engine-neutral even where MPV-only features remain.
- Security tests must prove source credentials never enter WebView/FFmpeg-visible URLs and that both nonce namespaces fail closed after teardown.
