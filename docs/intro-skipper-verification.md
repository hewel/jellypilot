# Intro Skipper Verification Notes

Manual verification target: a real Jellyfin server with the Intro Skipper plugin installed and JMSR connected as a cast target.

## Scenarios

- Intro Skip: play an episode with a valid `Introduction` segment and confirm MPV seeks to the segment end when playback reaches the intro or the 1-second lookahead window.
- Credit Skip: play an episode with a valid `Credits` segment and confirm MPV seeks to the credit segment end without triggering a direct next-episode command.
- Disabled Setting: turn off Operations Console -> Automation -> Automatic Intro Skip, start a new playback item, and confirm neither intro nor credit segments are fetched or applied.
- Missing Segment Item: play an item with no Intro Skipper data and confirm playback starts normally with no user-facing error.
- Missing Plugin or Endpoint Failure: disable/uninstall the plugin or block `Episode/{id}/IntroSkipperSegments`, start playback, and confirm playback starts normally with only a backend warning log.
- Next Episode After Credits: after Credit Skip lands near the end of playback, let MPV reach natural EOF and confirm the existing EOF-driven next-episode transition handles advancement.

## Scope Guard

This verification intentionally excludes Jellyfin generic media segments, skip prompts, overlays, per-series preferences, and direct next-episode behavior from Credit Skip.
