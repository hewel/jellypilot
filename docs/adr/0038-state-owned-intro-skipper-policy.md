# Own Intro Skipper policy state in one display-free module

_Status: Accepted and implemented, 2026-09-08. Amends [ADR 0025](0025-shared-jellyfin-session-crate.md)'s Intro Skipper evaluation ownership and [ADR 0002](0002-intro-skipper-plugin-boundary.md)'s silent-only description; follows [ADR 0027](0027-cross-platform-iced-frontend.md)'s display-free extraction direction and the repository's core placement convention._

Before this change, Intro Skipper policy crossed three modules: media-server owned mutable range flags and lookahead evaluation, session dispatched modes, and `PlaybackSession` applied another range predicate and owned prompt state. The caller's exact-start predicate overrode the lower evaluator's one-second lookahead. Moving only the dispatcher or unifying predicates would have left most policy knowledge exposed.

Concentrate eligibility, consumed-range bookkeeping, mode changes, and manual-prompt eligibility and expiry in one display-free Intro Skipper module in `jellypilot-core`. Its interface receives playback observations and relevant execution outcomes and produces policy decisions; callers do not inspect or mutate policy flags. This creates depth through state ownership, locality for timing rules, and leverage for playback orchestration and tests. Do not add an interchangeable policy adapter or a new dependency from media-server to core.

## Preserved behavior

- Retain Automatic, Manual, and Off. Automatic remains the default and is silent. Manual requires a live prompt before a user-invoked skip; preserve prompt activation after successful presentation, expiry, and dismissal. Off disables Intro Skipper behavior and clears active policy state as today.
- A range becomes eligible at its exact start and is no longer eligible at its end. Remove the lower evaluator's one-second lookahead rather than changing active playback to skip before a plugin range starts.
- Preserve one automatic seek attempt per fetched range per Playback Session. Consume eligibility when the seek action is issued, not after MPV reports success; failure remains reported and does not re-arm the range. Do not introduce automatic retries or success-only consumption.
- Preserve existing mode-transition behavior. Seeking back into a consumed range does not re-arm it.
- Credit Skip seeks past the plugin credit range; it does not directly start another episode. Natural end of playback remains responsible for episode advance.

## Seam placement

The media-server adapter retains plugin fetching, response parsing, and validated range data, without playback-policy flags. The new core module owns stateful policy. `PlaybackSession` retains command ordering, request correlation, stale fetch and execution rejection, teardown, and natural-end handling. The MPV adapter retains command execution and prompt rendering. Do not move the entire Playback Session reducer or remote Playback Target lifecycle.

The old evaluation paths and policy re-exports are removed, without parallel evaluators or compatibility aliases. ADR 0002's plugin endpoint and Introduction/Credits scope remain accepted. ADR 0025's WebSocket ownership remains unchanged. Manual mode is an intentional existing user capability, not a reason to make Automatic mode display a prompt.

## Verification

The policy interface is the test surface for exact-start/end eligibility, once-only consumption and failed attempts, prompt activation/expiry/dismissal, and mode transitions. Keep `PlaybackSession` tests for the integration seam: stale results, command precedence, teardown, and natural-end episode advance. Replace redundant helper assertions rather than repeating policy cases at every call hop. Preserve the concrete plugin and MPV adapter tests; no new trait is needed for policy testing.

`jellypilot_core::intro_skipper::IntroSkipper` now owns the policy, and playback callers use its decisions and opaque prompt correlation. Core behavior tests, retained Playback Session integration tests, and an executable policy smoke scenario exercise the contract. `CONTEXT.md` records the mode, timing, and one-attempt semantics.
