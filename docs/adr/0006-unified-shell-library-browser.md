# 0006. Adopt a unified authenticated shell with Library Browser

## Status

Accepted

## Context

JellyPilot started as an Operations Console for a Jellyfin Playback Target. The app now also needs authenticated browsing flows for Video Home, Movies, Shows, item details, season/episode hierarchy, playback launch, and User Data Actions.

These flows should not redefine JellyPilot as a complete Jellyfin client or embedded player. Playback still runs through the existing external MPV Playback Target path, and Now Playing remains the user-facing read model for the active external player session.

## Decision

Use one authenticated shell for Library, Now Playing, Settings, and Diagnostics. Library Browser is the default authenticated route and uses live Jellyfin data for browsing, search, details, playback launch, and User Data Actions.

Library playback actions call typed Tauri commands that route through the existing SessionManager playback path. User Data Actions call Jellyfin user-data endpoints and refresh visible detail state only after server success.

## Consequences

- Users can browse and launch video playback from JellyPilot without leaving the Playback Target model.
- Operations Console behavior is preserved under Settings and Diagnostics instead of remaining the whole app surface.
- Library Browser can grow in vertical slices while sharing Now Playing, Saved Service Profile, and typed command boundaries.
- Follow-up work must not describe JellyPilot as a full Jellyfin replacement unless embedded playback and broader client features are actually implemented.
