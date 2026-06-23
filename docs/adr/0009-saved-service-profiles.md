# 0009. Store multiple saved service profiles in Tauri Store

## Status

Accepted

## Context

JellyPilot originally treated authentication as a single Saved Session. The Emby support path and Settings-based service management need users to add another login while already authenticated, switch between saved services, and keep failed profiles visible for retry or removal.

The app still has one live media-server session at a time. Switching profiles should stop the current live session before restoring the selected profile.

## Decision

Store saved service profiles in backend-owned Tauri Store state. Each profile contains the persisted session needed for restore, while the frontend receives only redacted summaries.

Settings owns the management UI:

- Add Service opens the existing login flow in an embedded dialog.
- Activate switches the one active profile.
- Sign out removes only the active profile.
- Remove deletes a selected saved profile.

Restore failures are recorded on the profile summary and do not delete the profile automatically.

## Consequences

- Frontend code no longer owns durable auth-session storage except for legacy migration.
- Multiple services can be retained while preserving the single-active-session runtime model.
- Failed or offline profiles remain inspectable and removable instead of forcing the user back to Login.
