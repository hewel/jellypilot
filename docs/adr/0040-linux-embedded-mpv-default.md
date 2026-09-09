# Default Linux playback to the pinned Embedded MPV fork

_Status: Accepted. Supersedes ADR 0027's "always presents External MPV Playback" for Linux only. Does not revive ADR 0019's retired web player or local FFmpeg HLS pipeline._

Linux packaged and unpackaged installs start **Embedded MPV Playback** using the host-enabled libmpv and baseline from `tools/embedded-mpv/source.json` (the project-owned mpv fork). That is the product path the fork exists for: playback inside JellyPilot's player surface, not an opt-in around a system `mpv`.

External MPV Playback remains available on Linux (Settings or `--external`) for users who want their own MPV configuration, scripts, and shaders. It stays the default on Windows and macOS, where the embedded host is unavailable. Missing pinned assets or a Vulkan `Rgb10a2Unorm` surface is an error; there is no system-libmpv or automatic External fallback.

Existing Linux settings files that still record External are migrated once to Embedded. A later explicit External selection is kept.
