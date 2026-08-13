# Store GTK saved service profiles in Linux Secret Service

_Status: Accepted. Refines ADR 0009 and satisfies the GTK Saved Service Profiles gate retained by ADR 0022._

The GTK frontend must restore Jellyfin and Emby sessions without remembering a password or writing
an access token to an application data file. Tauri Store remains owned by the Tauri application and
is not a secure-credential adapter for the Linux-native frontend.

Keep saved-profile lifecycle in `src-gtk`. Store a versioned collection of `SavedSession` values as
one opaque binary secret in Linux Secret Service, addressed only by fixed application and schema
identifiers. Provider, server URL, user identity, device identity, and access token remain inside
the secret payload; no token-bearing index or plaintext fallback is written.

Use keyring 3.6 with its synchronous Secret Service adapter because it supports the workspace's
Rust 1.85 baseline. Run every credential-store operation on a named worker thread so GTK remains
responsive and application shutdown does not join a keyring prompt. Serialize read-modify-write
operations inside the GTK auth-storage module. Clear temporary serialized bytes and inactive token
copies before their allocations are released.

A successful Password Login or Jellyfin Quick Connect login creates or refreshes a Saved Service
Profile. The GTK login screen lists redacted summaries and restores a selected profile on an
isolated media-server client; the live client changes only after token validation succeeds.
Disconnect ends the live connection but retains saved profiles. Sign Out removes the active profile
first and disconnects only after secure deletion succeeds. Restore failures retain the saved
profile for retry or explicit removal.

Secret Service being absent, locked, or declined is a recoverable condition. Password Login and
Quick Connect can continue as ephemeral live sessions, but JellyPilot reports that they were not
saved and never falls back to a file, local storage, environment variable, or command-line secret.
The GTK startup smoke path does not access Secret Service.

## Consequences

- GTK supports multiple saved Jellyfin and Emby profiles without persisting passwords or plaintext
  access tokens.
- Credential-store availability can depend on the user's desktop session and unlocked keyring.
- Tauri profile migration, Flatpak secret portals, and cross-frontend profile sharing remain
  separate migration and packaging decisions.
- Saved sessions are still untrusted input and must pass media-server URL and user-identity
  validation before adoption.
