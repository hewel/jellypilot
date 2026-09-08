# Intro Skipper uses the plugin endpoint

_The silent-only description below is amended by [ADR 0038](0038-state-owned-intro-skipper-policy.md). The plugin-only endpoint boundary is superseded by [ADR 0039](0039-native-media-segments-for-intro-skipper.md): read native Jellyfin intro/outro segments while retaining Automatic, Manual, and Off._

JellyPilot integrates with the Intro Skipper plugin by fetching ranges from the plugin's `IntroSkipperSegments` endpoint when playback starts, then applying client-side silent skips for introduction and credit ranges only. We chose the plugin endpoint over Jellyfin's generic media-segment API because this feature is explicitly plugin-scoped and should follow the plugin's Introduction/Credits semantics for the first implementation.
