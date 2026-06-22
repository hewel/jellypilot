# Intro Skipper uses the plugin endpoint

JellyPilot integrates with the Intro Skipper plugin by fetching ranges from the plugin's `IntroSkipperSegments` endpoint when playback starts, then applying client-side silent skips for introduction and credit ranges only. We chose the plugin endpoint over Jellyfin's generic media-segment API because this feature is explicitly plugin-scoped and should follow the plugin's Introduction/Credits semantics for the first implementation.
