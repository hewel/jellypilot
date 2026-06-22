# 0007. Use generated Jellyfin OpenAPI client behind JellyPilot facades

## Status

Accepted

## Context

JellyPilot needs broad Jellyfin HTTP coverage for session validation, Video Home rows, library browsing, search, details, Show seasons and episodes, playback target resolution, and user-data mutations. Hand-written HTTP paths make this surface harder to keep consistent with Jellyfin contracts.

The app still has runtime boundaries that are not fully covered by the generated HTTP client: Jellyfin WebSocket command handling, Intro Skipper plugin endpoints, and MPV JSON IPC.

## Decision

Commit the generated Jellyfin OpenAPI Rust client under `src-tauri/jellyfin-api` and use it for supported Jellyfin HTTP endpoints. JellyPilot-owned facades such as `JellyfinLogin`, `JellyfinPlayback`, and `JellyfinLibrary` remain the boundary exposed to commands and frontend bindings.

Generated models do not cross the Tauri command boundary directly. Commands return JellyPilot-owned DTOs generated into `src/bindings.ts` by `tauri-specta`.

Manual HTTP remains allowed for WebSocket/plugin/runtime exceptions where the generated client is unavailable or not the right abstraction.

## Consequences

- Jellyfin HTTP calls gain typed request/response coverage while keeping frontend contracts stable.
- `src/bindings.ts` remains generated from JellyPilot command DTOs, not Jellyfin OpenAPI DTOs.
- Regeneration changes must be reviewed as generated-client updates and kept behind the facade boundary.
- Manual exceptions should be narrow and documented in the owning facade or module.
