# Use two-state desktop Intro Skipper preferences with series overrides

_Status: Accepted 2026-09-19; implemented and code-level validated, human visual acceptance pending. Supersedes the desktop Automatic / Manual / Off and manual-prompt lifetime/re-entry requirements in [ADR 0038](0038-state-owned-intro-skipper-policy.md), retaining its single display-free policy owner. Preserves [ADR 0039](0039-native-media-segments-for-intro-skipper.md)'s native range source and provider boundaries. Does not revise the [Android client contract](../android-client-design-spec.md)._

The updated [desktop player design](../design-system.md#embedded-player-synchronization) exposes one Automatic / Manual toggle: turning automatic skipping off must retain a manual action, not disable the feature. Use the same two choices for the desktop global Intro Skipper Setting, and remember a series-specific choice within its Profile Scope across episodes, later playback, and application restarts; a current-session-only choice would not preserve the user's intent for the series.

## Preference and migration contract

- Automatic remains the default. Migrate an existing Off preference to Manual, preserving existing Automatic and Manual values. This intentionally introduces a manual prompt for former Off users but never opts them into automatic skipping.
- A Series Intro Skipper Preference takes precedence over the global setting. Persist only values different from the global setting: selecting the global value clears that series' override, and changing the global setting also clears every override that now matches it. No separate reset action or permanently pinned same-as-default value exists.
- Normalization is deliberately lossy: with global Automatic and a series override of Manual, changing global to Manual removes the override; changing global back to Automatic then makes that series follow Automatic. This favors one consistent “same means follow” rule over preserving a hidden series exception.
- Keep the capability episode-only and retain provider capability checks. Movies and content without an identified series do not expose the player toggle; this decision does not add movie range fetching, per-movie preferences, or server-side preference synchronization.

## Range behavior

In desktop Manual mode, the action remains available for the entire current stay inside a real intro/credit range, rather than for three seconds, unless dismissed or used. After playback leaves and re-enters that range, the manual action is available again, including after a previous dismissal or successful manual skip. The independent prompt must not force complete embedded-player controls to remain visible.

Automatic mode remains silent and retains one automatic seek attempt per fetched range per Playback Session, consumed when issued even if it fails; seeking back does not re-arm it. Preserve exact-start-inclusive/end-exclusive range eligibility and independent handling of multiple ranges. Credit Skip seeks to the range end; only normal end-of-playback handling advances to another episode. Ordinary chapters are not inferred skip ranges, and absent server ranges never become synthetic ones.

## Delivery boundary

The global and series rules are implemented as desktop playback policy, not an embedded-only presentation hack; the home/footer Player Bar and External MPV controller are not visually redesigned. Series overrides share the existing atomic settings file. Android UI and policy migration remain outside this decision: the shared policy's legacy constructor retains timed prompts and Off behavior, while desktop sessions select whole-range prompts.

Code-level acceptance exercised legacy preference migration, Profile Scope isolation and restart persistence, normalization after both global and series changes, whole-range manual availability and re-entry, once-only automatic attempts including failure, and the existing natural-end episode-advance boundary. Focused regressions and clippy, `bun run check`, workspace Rust tests, and the isolated headless native smoke gate passed. Real MPV accepted the persistent prompt's signed-32-bit duration; the embedded surface owns its native prompt without duplicating MPV OSD. Native visual acceptance remains human-only and follows the checklist in the design contract.
