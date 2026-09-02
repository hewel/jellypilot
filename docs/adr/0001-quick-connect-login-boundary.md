# Quick Connect stays inside login

Quick Connect authenticates JellyPilot to a known single Jellyfin Server URL from the Login screen. It does not discover servers or select between servers inside the Quick Connect flow, because this keeps the first Quick Connect slice aligned with explicit user-entered server URLs. Richer server and account management is handled separately through Saved Service Profiles; see ADR 0009.

The native GTK login screen follows the same boundary. It shows the public approval code, polls for
up to five minutes on an isolated client, and allows cancellation without adopting partial session
state. The server-issued secret remains in the async workflow, is zeroized when that workflow ends,
and is never placed in a widget, command debug output, or user-facing error. A successful approval
uses the same secure Saved Service Profile path as password authentication.

Zeroization covers JellyPilot-owned pairing-secret and access-token storage. The HTTP client must
transiently materialize the secret in Jellyfin's query or request-body protocol; those transport
buffers remain bounded to the request lifetime and are excluded from logs and errors, but
allocator-level erasure inside reqwest and the generated SDK is best-effort rather than guaranteed.
