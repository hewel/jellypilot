# JellyPilot Native Sidebar and Account Interaction Specification

**Status: Implemented; human visual and live-server acceptance pending.** Design confirmed in the 2026-09-05 interview. The native implementation follows this specification and ADRs 0032–0033.

The original reference is the external `sidebar_design_spec.md` v2.4:
`/home/hewel/.gemini/antigravity/brain/9c8bf3e5-d520-4caf-9dd1-854e63204365/sidebar_design_spec.md`.
The original remains a source reference. This document is the authority for the native implementation and contains the decisions needed to implement it independently. The original's Web code paths, simulated data, and checked acceptance items are not implementation evidence.

This specification uses iced, the [jellypilot-ui design system](design-system.md), and External MPV Playback, following
[ADR 0027](adr/0027-cross-platform-iced-frontend.md). Domain names follow [CONTEXT.md](../CONTEXT.md);
[ADR 0032](adr/0032-separate-server-favorites-and-local-watchlist.md) records list ownership, and
[ADR 0033](adr/0033-validate-before-active-profile-handoff.md) records the account-switching lifecycle.
Quoted Chinese UI labels below preserve the copy agreed during the interview; the specification itself is written in English.

## 1. Content Scope and Navigation

Home, Search, libraries, Favorites, and Watchlist all belong to the **current provider, Server URL, and server user**.
Multiple saved logins support switching, not simultaneous connections or aggregation across servers. A server name, username, or item ID alone cannot identify the account scope.

### Sidebar

Only authenticated Full mode shows the Sidebar. Control-Only retains its existing complete controller window.

| Area | Fixed and scrolling behavior | Content |
| --- | --- | --- |
| Top search | Fixed | Search input, submit action, shortcut hint or clear button |
| Personal navigation | Fixed | Home, Personal Lists |
| Library heading | Fixed | Libraries and their actual count |
| Video libraries | Independent vertical scrolling, filling the remaining height | Video libraries browsable by the current account, with actual names and types |
| Account and tools | Fixed at the bottom | Clickable identity card; Settings, Refresh, Control mode |

- At window widths of 1280 and above, the Sidebar is 240 wide; below 1280, retain the 72-wide icon-only Sidebar. Full mode retains its existing minimum window size.
- The expanded Sidebar has horizontal padding of 12. Minimum heights for search, personal navigation, library rows, and bottom tools are 36, 38, 32, and 36 respectively.
  These are iced logical dimensions. Typography, control radii, and status colors use existing semantic tokens/Catalog styles.
- The icon-only Sidebar provides full-name tooltips. Truncated long names remain available in full. The original's 15 libraries are sample data; counts and ordering come from actual responses.
- Empty, loading, or failed library states change only the library region, retaining personal navigation and account actions. A list of 50 libraries must not displace the top or bottom regions.
- Content layouts use the actual Sidebar width consistently. Home and paginated grids must no longer independently retain the old 248-wide calculation.
- Preserve the existing player bar and access to complete Now Playing controls. Playback actions continue to control external MPV.

### Appearance

Retain Charcoal / Light Clean themes, the System / Dark / Light preference, and the existing reduced-motion setting.
Use existing opaque surfaces, control styles, and overlay shadows. The original's glass blur, extra outlines, avatar gradients, and continuous breathing effects do not override design-system constraints.
Favorites and Watchlist use distinguishable heart and bookmark icons with text; status is not communicated through color alone. Any required semantic tokens belong in jellypilot-ui.
Verify each color pairing against design-system contrast requirements rather than inheriting the original's claim of "above 5.5:1".

## 2. Search, Refresh, and Keyboard Behavior

### Search

Search Movie, Series, and Episode items across all video libraries of the current account using the server's SearchTerm capability.
Do not promise original-title/tag matching, AND/OR/NOT syntax, or typo-tolerant fuzzy search.

- Use one rounded search field with a leading clickable magnifier, the placeholder "Search movies and shows…", and an inset platform shortcut keycap (⌘K on macOS, Ctrl K elsewhere). Product interface labels are English throughout.
- Typing updates only the draft. Enter or the leading magnifier displays results in the main content area; there is no separate Search button. Use existing pagination with a page size of 24 and ascending name order.
- The expanded Sidebar exposes the input directly. In the icon-only Sidebar, clicking the search icon or pressing the shortcut opens an **input popover** anchored to that icon; submission closes it, and results still appear in the main content area.
- An empty input shows the platform shortcut hint; a nonempty input provides a clear button. Whitespace-only drafts do not send requests.
- Clear or Esc while searching empties the draft and removes focus from the input. Previously submitted results retain their query until another submission or navigation.
- A new submission invalidates results from the old query. Repeated submissions do not accumulate pagination tasks. Errors provide retry, and empty results have an explicit empty state.

### Refresh

The bottom action is named Refresh. It reloads the current account's library directory and current page content; it is not a server scan task.
Personal Lists refreshes Favorites and server information for currently visible Watchlist entries without changing local Watchlist membership.
Show real request activity and coalesce repeated triggers. Preserve current navigation, retain usable content on failure, and provide retry.
If a directory refresh confirms that the current library is no longer accessible, return to Home and explain why. Refresh results from the old account must not update the new account.

### Shortcuts and Layering

| Shortcut | Scope | Behavior |
| --- | --- | --- |
| Ctrl+K / macOS Cmd+K | Authenticated Full mode, with no blocking modal or shortcut recording | Focus and select the entire search draft; open the input popover first in the icon-only Sidebar |
| Ctrl+, / macOS Cmd+, | Current application window, with no other blocking modal | Open or focus the Settings Modal; shortcut recording takes precedence while active |
| Esc | The topmost interaction layer | Close the popover/modal and restore focus to its trigger; search clears its draft as described above |

These shortcuts operate within the application window; they do not register operating-system global hotkeys. The search shortcut does not switch Control-Only into another mode.
Retain existing playback shortcuts. Shortcut recording must detect conflicts with fixed application shortcuts, and text entry must not accidentally trigger playback actions.

## 3. Personal Lists

Use one navigation entry and one overview page, with **Favorites above Watchlist**. Each section has its own count, empty state, loading/error feedback, and View all action.
Do not combine both counts into an ambiguous badge. The overview reuses Home card rows, showing one page of data per section. View all opens the corresponding paginated grid, with navigation back to the overview.

| Contract | Favorites | Watchlist |
| --- | --- | --- |
| Owner | Current Jellyfin/Emby user | Current Profile Scope on this device |
| Item types | Movies, complete series, individual episodes | Movies, complete series, individual episodes |
| Order | Name ascending | Most recently added first |
| Successful update | Server accepts favorite/unfavorite | Local records are successfully persisted |
| Cross-device behavior | Existing server-side favorite state | Independent on each device |
| Relationship to watched status | Independent | Independent; removal is manual only |

Detail views expose separate Favorites and Watchlist actions; list cards provide the corresponding removal action.
Episodes show the series title and episode number. An item can belong to both lists; changing one membership does not change the other.

Watchlist records retain a stable account scope, item ID, time added, and the minimum name/type/episode information needed to identify an unavailable item.
Online queries fill in current content, using the existing Library Image mechanism. These records do not provide offline library browsing or offline playback.
When a successful server response confirms that an item is missing or inaccessible, show it as Unavailable, retaining its count and manual removal action.
A network failure does not mean an item was deleted and must not cause bulk marking or deletion of Watchlist entries.

## 4. Account Popover and Connection Lifecycle

### Presentation and Entry Points

The identity card uses a rounded-square monogram, connection indicator, and two-line username/provider/server identity, falling back to the server address when the name is missing. In the expanded Sidebar, a separate rounded toolbar places Settings, Refresh, and Control mode in three equal columns with dividers.
The Playback Target's device identity must not masquerade as a server node code. Show actual login status and separately describe existing remote-control connection status.
"Signed in" does not mean continuously reachable; do not display unmeasured Mbps or latency.

The Account Popover is anchored above the identity card by default, start-aligned, with a target width of 368.
It may extend into the main content area. Its position and maximum height are constrained by the entire window; flip placement when space is insufficient and scroll long account lists independently.
Outside click or Esc closes it. Closing a busy menu does not itself cancel background operations.

Order the content as current identity/address with a copy action, saved logins, Startup Auto Login preference, then adding an account and lifecycle actions.
Copy the address through the native clipboard. Show Copied only after success and allow retry on failure.
Label the list Switch server / account, show the saved-account count, and indicate the current selection with a tinted row and checkmark. Profile rows include a monogram, provider badge, and server identity. Normal mode selects a profile to switch; management mode exposes each entry's Sign Out action, also accessible by keyboard. Place the add-account action across the available width, followed by paired Disconnect and Sign Out actions.
Retain offline or failed-restore records for retry or Sign Out; failure does not automatically delete them.

Full mode enters through the Sidebar. The account category in Settings reuses the same management capability so Control-Only can also add, switch, and sign out.

### Switching and Adding

1. Selecting the current account does not rebuild its connection. When choosing another account or submitting Connect and switch, first confirm that the current Playback Session will end, if one exists.
2. Validate the saved login or authenticate the new account on an isolated candidate. The old account remains the only active connection; cancellation or target validation failure preserves its connection and playback.
3. After validation, enter handoff and block repeated switching or Sign Out. If playback began during validation and stopping it has not yet been confirmed for this attempt, request confirmation first; cancellation discards the candidate.
   End the old playback and attempt its stop report while its authentication is still available. Adopt the candidate only after MPV cleanup finishes.
4. A failed report records diagnostics without bypassing cleanup. Cleanup failure prevents candidate adoption and is reported explicitly; do not claim that already-ended playback has been restored.
5. After successful adoption, clear the old account's browse state, enter the new account's Home, and record the last successfully activated saved profile. Control-Only remains in controller mode.

Adding an account uses a centered login modal that adapts to the available space in small windows. Reuse Jellyfin Quick Connect and Password Login, and Emby Password Login.
Provide an entry to restore saved logins without filling in passwords. Quick Connect requires a Server URL first and fixes that address while awaiting approval.
After cancellation, late results from the old request cannot complete a switch. Successful new authentication leads to Connect and switch, not save-only behavior.
Cancelling during authentication or confirmation discards the candidate and preserves the current connection. Once playback shutdown begins, closing the UI only closes the presentation layer;
it no longer cancels the handoff task. The runtime reports the result, without promising to roll back playback that has already ended.

Remember login inputs refers only to existing Login Prefill. Credentials remain in the OS keyring. When it is unavailable, retain the existing explicitly communicated temporary-login behavior;
do not write tokens to the Watchlist file. Separately report failures to save a login or startup selection after connecting, without promising successful restoration after restart.

### Startup Auto Login

Use the label "Automatically sign in to the last-used account at startup", meaning the **last successfully activated, restorable Saved Service Profile**.
Auth storage records successful selections, and the toggle uses native application configuration. When disabled, cold startup stays on the login selection screen; it also stays there when enabled without a valid selection.
Restore failure retains the original record for retry instead of trying other accounts in turn. Failed candidates do not overwrite the successful selection.
This behavior is distinct from network reconnection of an already-connected remote Playback Target; the two do not share this toggle.

### Disconnect and Sign Out

- **Disconnect** ends the current connection and Playback Session, retaining saved logins and Watchlist records. Stop reporting and MPV cleanup still precede clearing runtime authentication.
- **Sign Out** removes the selected login from this device. The confirmation identifies the account and offers "Also delete this account's local Watchlist", unchecked by default.
- An inactive account can be signed out directly without affecting the current connection or playback. For the active account, delete credentials securely first; after success, use its old authentication to finish stopping playback and disconnecting. Credential deletion failure preserves the current session.
- Opt-in Watchlist cleanup affects only the selected Profile Scope. Credentials and Watchlist are not one transaction: credential deletion failure prevents subsequent list cleanup; list cleanup failure explicitly reports that the login was removed but the list remains, with retry for that scope.
- Restoring the same Profile Scope makes a retained Watchlist available again. Sign Out neither invokes server-side token revocation nor deletes the server user.

## 5. Settings and Control Mode

At the existing wide-window threshold of 1280, use a centered two-column layout with a target size of 896×620 and at least 24 of space on each side. Reduce its height and scroll content when available height is insufficient.
Narrow windows and 480×760 Control-Only use a full-window, single-column category selector and content layout.
The Settings Modal preserves the current shell context, closes through its close button or Esc, does not close on backdrop clicks, and is not added to the navigation stack.
The modal and its internal selection popovers consume input by layer and restore focus.

Group existing settings into Accounts and connection, MPV, Playback, Subtitles, Shortcuts, Appearance, Storage, and Diagnostics and about.
Preserve existing capabilities and when changes take effect: Playback Target, Startup Auto Login, MPV path and extra arguments, Intro Skipper, subtitle priority, existing playback shortcuts, App Mode,
theme, reduced motion, start minimized, image cache, diagnostics, and about information.
Subtitle ordering retains Add/Move up/Move down/Remove controls. Extra arguments continue to carry advanced MPV configuration.
This work does not add IPC path editing, a hardware-decoding toggle, a filter editor, multiple dark-theme variants, or an accent-color picker.

The third bottom tool is named Control mode and switches to existing Control-Only; it does not add a floating mini window.
Follow the existing App Mode contract: release Full browse state while preserving the connection, Playback Session, tray, and remote control. Settings retains the entry to return to Full mode.

## 6. Interfaces and Ownership for Future Implementation

| Boundary | Required capabilities and constraints | Existing entry points |
| --- | --- | --- |
| media-server | Root-level, recursive Movie/Series/Episode Favorites pagination and total count for the current user; batch retrieval of Watchlist items by ID | [Request types](../crates/jellypilot-media-server/src/types.rs), [client](../crates/jellypilot-media-server/src/client.rs) |
| core | Serializable Profile Scope, local Watchlist model and storage; separate file, serialized updates, memory updated only after replacement writes succeed | [Configuration storage pattern](../crates/jellypilot-core/src/config.rs) |
| auth | Saved-login summaries, candidate validation, selected-profile deletion, and last successful activation record; secrets remain in the keyring | [Authentication and storage](../crates/jellypilot-auth/src/lib.rs) |
| iced runtime | Candidate authentication and single-active-connection handoff, account/request-generation gates, UI messages, focus, and navigation | [Login orchestration](../src-iced/src/app/login.rs), [shell view](../src-iced/src/app/view/shell.rs) |
| mpv | Attempt stop reporting with old authentication; finish MPV cleanup before allowing the new connection to take over | [Playback control](../crates/jellypilot-mpv/src/playback.rs) |
| ui | Reuse native cards, inputs, popovers, modals, and semantic styles | [Popover](../crates/jellypilot-ui/src/overlay/popover.rs), [theme](../crates/jellypilot-ui/src/theme.rs) |

Favorites queries use existing server Items capabilities: current user, recursive traversal, the three video types, IsFavorite=true, and no ParentId.
Existing per-library requests and interfaces requiring a nonempty SearchTerm cannot stand in for this query. A single library's count cannot be displayed as the total Favorites count.

Persist Watchlist in a separate, versioned `watchlist.json` under the application configuration directory rather than merging it into Settings or the credential secret.
Account-address normalization follows existing auth rules without automatically merging domain/IP aliases for the same server. Deduplicate by account scope and item ID together;
the UI does not construct keys from display names. Batch enrichment uses the current provider/user, and write failures preserve the original file and in-memory snapshot.

Bind requests and operations to their originating account scope and generation. Late results after switching, cancellation, or navigation cannot overwrite new state.
Repeated requests, concurrent membership changes, list cleanup, and saves must pass through existing request gates or serialized storage boundaries.

## 7. Acceptance Scenarios — Pending Verification

The checkboxes below track end-to-end acceptance with representative accounts and human visual review. Automated regression coverage and startup checks do not substitute for these scenarios.

### Content and Persistence

- [ ] Jellyfin and Emby Favorites requests cover all three video types and all video libraries; pagination, totals, and name ordering are correct.
- [ ] Favorites and Watchlist do not leak across servers with matching usernames, different users, or duplicate item IDs.
- [ ] Watchlist restores after restart, sorts by most recent addition, deduplicates, and removes entries only manually; watched status does not change membership.
- [ ] File-write failure preserves old records; network failure is not treated as deletion; confirmed unavailable entries remain identifiable and removable.
- [ ] Sign Out retains the list by default, honors explicit cleanup, and retries failed cleanup in the original scope without modifying other accounts.

### Accounts and Interaction

- [ ] Cancelling a switch during playback or failing target validation preserves old playback; playback started during authentication requires confirmation; successful handoff reports/cleans up before adoption, and old content does not return.
- [ ] Stop-report and MPV cleanup failures are reported separately; credential deletion failure does not incorrectly sign out the current session.
- [ ] Direct Sign Out of an inactive account leaves playback intact; new-account Connect and switch, Quick Connect cancellation, and late results behave correctly.
- [ ] Startup Auto Login restores the last successful account without switching accounts on failure; disabled, missing-record, and storage-failure states are reported correctly.
- [ ] Search requests occur only on submission; clear, empty input, repeated submission, late pagination results, empty state, and retry behave correctly.
- [ ] Refresh reloads the directory and current page, preserves usable state on failure, and returns to Home when the current library is removed.
- [ ] Shortcuts distinguish platform, App Mode, input focus, blocking modals, and shortcut recording; Esc handles only the topmost layer.

### Human Visual and Interaction Review

- [ ] At Full mode's minimum window size, both sides of the 1280 threshold, and wider windows, check the 72/240 Sidebar, content layout, and long-name tooltips.
- [ ] With 0, 15, and 50 video libraries, check independent scrolling and continued access to top navigation and bottom account/tools.
- [ ] Check the two vertically stacked list sections, their counts and View all actions, episode titles, unavailable items, and empty/error states.
- [ ] Check expanded/icon-only search inputs, Account Popover boundaries, long account lists, copy feedback, and keyboard focus restoration.
- [ ] Check settings categories, scrolling, dismissal, and complete account management in wide windows, narrow windows, and Control-Only.
- [ ] Under Dark, Light, System, and reduced-motion preferences, check readability, status differentiation, and contrast of actual color pairings.
- [ ] Switch to Control-Only and back to Full, confirming playback, tray, and remote control continue; account switching instead ends old playback through the confirmation flow.

### Validation Tiers

The [validation policy](agents/validation.md) is authoritative. This implementation requires focused checks, Suite checks for its public-interface and cross-crate changes, and native smoke for window, subscription, lifecycle, and persistence boundaries.

Automated verification completed on 2026-09-05: `bun run check` passed, workspace Rust tests passed (one subprocess helper ignored by the normal runner), and the native smoke gate exited successfully. Regression coverage includes isolated account candidates, Quit/handoff ordering, credential and MPV cleanup failures and retries, account-scoped Watchlist persistence, late settlements, and refresh/mutation ordering. Human visual review and live Jellyfin/Emby acceptance remain unverified.

```bash
bun run task rust test <crate>
bun run task rust clippy <crate>
bun run task rust fmt --check
# Final gates for cross-crate / public-interface changes
bun run check
bun run task rust test
# Desktop-boundary and persistence gate: proves startup, not appearance
xvfb-run -a bun run task iced run --smoke
```

Replace `<crate>` with the actual short name required by the validation policy. Test domain logic in display-free crates and adapters through real external interface shapes;
use controlled ordering to cover concurrency and stale results. The user performs the human visual checklist; agents do not launch the application for visual judgment or capture screenshots.
