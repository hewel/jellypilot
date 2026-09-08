# Intro Skipper uses the plugin endpoint

_The silent-only description below is amended by [ADR 0038](0038-state-owned-intro-skipper-policy.md): retain Automatic, Manual, and Off, with Automatic remaining silent. The plugin endpoint and Introduction/Credits scope remain unchanged; the policy consolidation is implemented._

JellyPilot integrates with the Intro Skipper plugin by fetching ranges from the plugin's `IntroSkipperSegments` endpoint when playback starts, then applying client-side silent skips for introduction and credit ranges only. We chose the plugin endpoint over Jellyfin's generic media-segment API because this feature is explicitly plugin-scoped and should follow the plugin's Introduction/Credits semantics for the first implementation.
