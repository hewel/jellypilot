//! Platform-provided teardown seam for profile transitions.

/// Platform work that must settle before the SDK swaps the active profile.
///
/// The hook runs while the previous profile is still fully active. For
/// activation and disconnect, returning `false` (or unwinding with a panic)
/// declines the transition and keeps the previous profile active.
/// Sign-out has already deleted protected credentials: a declined hook is
/// reported in [`crate::SignOutOutcome::teardown_error`], and session cleanup
/// still completes. This follows ADR 0033's commit boundaries.
///
/// Desktop uses this to finish playback and remote teardown; Android uses it
/// to end the player session and clear its recovery point. The hook is
/// invoked for `activate_candidate`, `disconnect`, and `sign_out` whenever a
/// profile is currently active.
#[async_trait::async_trait]
pub trait SdkHooks: Send + Sync {
    /// Returns `true` when teardown finished and the handoff may proceed.
    async fn before_profile_handoff(&self) -> bool;
}
