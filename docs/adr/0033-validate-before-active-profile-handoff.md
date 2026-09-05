# Validate the target profile before replacing the active connection

_Status: Accepted, 2026-09-05. Amends [ADR 0009](0009-saved-service-profiles.md)'s switch ordering, active-only Sign Out naming, and Settings-only management entry; carries forward [ADR 0023](0023-secure-gtk-saved-service-profiles.md)'s isolated validation and secure deletion into the iced runtime of [ADR 0027](0027-cross-platform-iced-frontend.md). Implementation and acceptance remain pending in the [native sidebar specification](../sidebar-design-spec.md)._

An offline or invalid saved login should not destroy a working connection or its playback.
Authenticate the target on an isolated candidate while the old profile remains the sole active
runtime connection. When playback is active, obtain confirmation before starting a switch or
connect-and-switch attempt. Authentication failure or cancellation before handoff leaves the old session intact;
successful validation permits the handoff, but does not itself activate a second runtime session.

During handoff, finish the old Playback Session, attempt its stop report while its authentication
is still available, and complete MPV cleanup before adopting the candidate. Stop-report failure is
reported and does not bypass cleanup; cleanup failure prevents candidate adoption and cannot be
described as restoring playback that has already stopped. This is more coordination than
disconnect-first restore, but preserves the working session through the common failure point of
target authentication. It also avoids the larger model of simultaneous active server accounts.
Persist the last successfully activated saved profile for Startup Auto Login; failed candidates
never replace that choice.

Expose one account-management capability through the Sidebar Account Popover in Full mode and
Settings in Control-Only Mode. Sign Out can select any saved profile and means local credential
removal, not server-side token revocation. Removing an inactive profile leaves the active session
alone. For the active profile, secure deletion must succeed before disconnection; keep in-memory
authentication long enough to report and stop its playback before clearing it. Watchlist deletion
is separately opt-in per [ADR 0032](0032-separate-server-favorites-and-local-watchlist.md), and failures
of the two stores are reported separately rather than implying an atomic deletion.

The runtime remains single-active and independent of the UI composition, as required by
[ADR 0030](0030-resource-bounded-control-only-composition.md). App Mode changes preserve the current
profile and Playback Session; they do not use the Profile Switch lifecycle.
