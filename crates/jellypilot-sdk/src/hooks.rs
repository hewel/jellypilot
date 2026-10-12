//! Platform-provided teardown seam for profile transitions.

/// Platform work that must settle before the SDK swaps the active profile.
///
/// The hook runs while the previous profile is still fully active. For
/// activation and disconnect, returning `false` (or unwinding with a panic)
/// declines the transition and keeps the previous profile active.
/// Sign-out has already deleted protected credentials: a declined hook is
/// reported in [`crate::SignOutOutcome::teardown_error`]. Authentication stays
/// available for cleanup retry, while new playback and content writes remain
/// blocked. Successful retry through [`crate::Sdk::disconnect`] ends the scope.
///
/// Desktop uses this to finish playback and remote teardown; Android uses it
/// to end the player session and clear its recovery point. The hook is
/// invoked for `activate_candidate`, `disconnect`, and `sign_out` whenever a
/// profile is currently active.
#[allow(
    clippy::double_must_use,
    reason = "async-trait injects must_use on boxed futures; rust-clippy#17529"
)]
#[async_trait::async_trait]
pub trait SdkHooks: Send + Sync {
    /// Returns `true` when teardown finished and the handoff may proceed.
    async fn before_profile_handoff(&self) -> bool;

    /// Deletes a signed-out profile's device-local Watchlist through a
    /// platform's existing storage owner, when it has one.
    ///
    /// Desktop returns `Some` from its serialized Watchlist adapter. `None`
    /// delegates to the SDK's real Watchlist store; Android uses this default.
    /// Called only after protected credential deletion, including retries.
    async fn remove_watchlist(
        &self,
        _scope: &jellypilot_core::watchlist::ProfileScope,
    ) -> Option<Result<(), String>> {
        None
    }
}
