# Android Client Design Specification

_Status: Accepted design, 2026-09-14; mobile product implementation added on 2026-09-21 and synchronized with the 2026-09-22 Mobile design. Validation and pending acceptance are recorded below. The user deferred physical-device testing until completion; the earlier bring-up device pass does not validate these new flows._

Architecture: [ADR 0042](adr/0042-native-android-frontend-and-shared-sdk.md). Domain terminology: [CONTEXT.md](../CONTEXT.md). Visual source: [Paper, Media Streamer / Mobile](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/5-0/5UF-0). Repository investigation used `22555b4`; the earlier proposal cited `92fba0e3` and is not current implementation evidence.

## Product scope

Android is an independent native phone and tablet frontend using Kotlin and Jetpack Compose. It preserves JellyPilot's brand and business semantics, not desktop layout, window behavior, or renderer integration. Continue using the same Git repository; independent frontend, build, and release paths do not require a second repository.

The first release inherits currently implemented business capabilities that apply outside the desktop host. A login/playback demo is an intermediate gate, not the complete deliverable.

| Area | First-release contract |
| --- | --- |
| Authentication | Known Server URL, Jellyfin Quick Connect and Password Login, Emby Password Login, Login Prefill, Saved Service Profiles, Startup Auto Login. |
| Account management | Multiple saved profiles, one active Profile Scope, validated Profile Switch, Disconnect, Sign Out, and separate opt-in Watchlist deletion. |
| Video Home | Featured Item, Continue Watching, Next Up, latest library content, library navigation, and the established episode-versus-parent-series action targets. Preserve selection semantics while using the mobile prototype's presentation. |
| Library Browser | Search, library paging, current filters and sort choices, movie/series/episode details, seasons and episodes, and currently supported related/cast content. |
| Personal Lists | Device-local Watchlist, server Favorites, and server Watch History; preserve their distinct ownership and retention rules. |
| User Data Actions | Favorite/unfavorite and played/unplayed changes become authoritative after server acceptance, with consistent state across visible and retained content. |
| Playback | Direct Playback, server resume and play-from-beginning, audio/subtitle selection including supported external subtitles, existing track/language preferences, original-audio preference, applicable season-volume memory, episode selection, previous/next episode, and natural-end episode advance. |
| Intro Skipper | Existing Automatic, Manual, and Off semantics, including once-per-range automatic attempts and natural-end-driven episode advance after credit skipping. Respect provider capabilities. |
| Remote control | Foreground Playback Target with the currently supported provider command set, including starting playback and controlling the active item. This is not a phone-to-desktop remote-controller feature. |
| Platform controls | MediaSession-backed system media controls, audio-focus handling, headphone-disconnection handling, standard Android navigation/text/focus input, and media keys. |
| Settings and support | Dark/light/system theme, English/Simplified Chinese and UI Language Preference, reduced-motion preference, applicable business playback settings, target naming, image-cache control, and sanitized diagnostics/export. Platform-specific controls are not copied mechanically. |
| Android addition | Local Playback Recovery Point and a distinct Restore Local Playback action for interrupted playback. |

Provider parity means preserving implemented capability distinctions, not promising identical Jellyfin and Emby behavior. Quick Connect and the currently implemented Intro Skipper support are Jellyfin capabilities. The existing client does not imply offline media downloads, Watchlist cloud sync, a full media-server replacement, or arbitrary new search syntax.

### Explicit exclusions

- Android TV and remote-focus television layouts.
- Fold-posture-specific Split View/Tabletop behavior. Ordinary phone/tablet window-size adaptation remains required, including on foldable devices.
- Background audio, picture-in-picture, and remote playback while the app is not visible or the device is locked.
- Desktop App Mode/Control-Only Mode, tray, close-to-tray, start-minimized behavior, external MPV process selection, and the desktop embedded compositor.
- Arbitrary `mpv.conf`, Lua scripts, user shaders, and user-supplied player arguments. The Android player uses an application-managed baseline.
- Editable MPV keyboard bindings or an Android-specific playback-hotkey feature. Standard input and system media keys remain in scope.
- Provider Transcode, a second player engine, public SDK publication, and public/store release as first-release gates.

## Prototype and presentation contract

The supplied Mobile page contains phone Home, Library, Series detail, landscape Player and Settings designs, light variants, tablet layouts, and foldable explorations. Reuse its tokens, typography, content hierarchy, and visual intent; do not restart visual design or treat it as an iced component port.

Phone and tablet layouts adapt to available window size. Navigation, back behavior, touch targets, text input, accessibility, loading/error states, and state restoration use Android-appropriate behavior. Pixel dimensions, hover behavior, desktop navigation stacks, and desktop paging/prefetch constants are not portable product requirements. Kotlin ViewModels may retain presentation state; they do not independently decide paging admission or stale-result validity.

Entering the player requests sensor landscape, allowing either landscape direction; leaving restores
the Activity's previous orientation policy. Rotation and window-size changes update the Compose
layout without restarting the Activity or interrupting playback. Actual backgrounding and locking
still pause playback. The player remains adaptive when Android ignores orientation requests in
multi-window or on [large screens](https://developer.android.com/develop/adaptive-apps/guides/app-orientation-aspect-ratio-resizability).

Playback uses immersive system bars, including track-selection windows: status and navigation
bars are hidden, edge swipes reveal transient bars, and leaving playback restores browsing bars.
The phone player follows the 2026-09-22 [landscape control reference](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/5-0/627-0).
Full controls have transparent Back and Settings targets at the top, borderless central
back-ten-seconds / play-pause / forward-ten-seconds controls (56/72/56 dp), and bottom title,
episode, queue/audio/subtitle entries, timeline and elapsed/remaining time. The timeline has a
4 dp rail and 12 dp thumb. Android targets remain at least 48 dp; large text and narrow windows
adapt without removing actions. Previous/next episode actions remain available in the queue.
The prior tablet transport layout remains separate from this phone revision.

Phone Minimal is a clean picture, with no progress line or pause badge. Tapping empty picture
toggles controls without changing playback. Pausing reveals Full; explicit dismissal can hide it
again while remaining paused. Full hides after three seconds of actual playback, but never during
slider interaction, an open panel, or accessibility focus. No synthetic chapter markers are drawn
when the player has no chapter data. Automatic-skip Undo appears at bottom right, remains actionable
for eight seconds (extended for focus/accessibility), and does not force Full controls to appear.

Landscape selection panels use a 340 dp surface, 16 dp outside margins and corner radius, a
fixed 16/24 sp heading and back/close controls, and scrolling 56 dp rows that grow with text.
Selected tracks use neutral fill and a check, including the portrait preplay sheets. Settings
contains volume, a separate playback-speed page, session skip, picture brightness and video
information; nested pages preserve the opening control's focus when closed.
The cinema-only roles in `PilotTheme.kt` use flat translucency without video backdrop blur;
Material slider input/semantics are retained with custom track/thumb visuals. Human acceptance
owns fidelity to the reference, subtitle output and actual display-cutout/gesture behavior.

Manual picture brightness is an application-owned OUTPUT shader multiplying video RGB from
20–100%, after video color mapping and before subtitle overlay. It does not change system/window
brightness, dim controls or subtitles, or expose user shaders/configuration. It defaults to 100%;
brightness and speed stay with the open player during continuous media replacement and reset on
exit. This is a signal multiplier, not calibrated screen luminance or
HDR acceptance. Custom double-tap, swipe and long-press playback gestures, their HUD, help and
preferences are excluded from this design synchronization at the user's request.

A prototype entry is not automatic feature authorization. In particular, the current “快捷键” entry must not become a dead settings item: the implemented Android settings navigation must omit or revise it to match the standard-input-only decision. Likewise “MPV” does not authorize arbitrary configuration. Missing prototype pages do not silently remove inherited business capabilities. Paper itself was not modified in this design session; final visual acceptance is human-owned.

### Mobile design synchronization — 2026-09-22

The current snapshot adds full-screen server connection and account login as two steps. The
public server probe preserves ports and reverse-proxy paths, reports actual server identity,
and never sends saved credentials or changes the active profile. Explicit product metadata can
identify Jellyfin/Emby; otherwise the account step asks for the server type. Password remains
optional and memory-only: failed attempts retain it, while changing address, exiting the flow,
or successful activation clears it. Back cancels outstanding requests; old completions cannot
activate a profile. Authentication refusal and an unreachable server have different feedback.
When public information is restricted (HTTP 401/403), an explicit manual-login action requires
the user to choose the server type. It preserves Emby's supported password-authentication fallback
without inventing a server name or showing an unverified connection as identified.

Home, library controls, poster/episode rows, search and personal-list tabs follow the updated
spacing and hierarchy. Library menus retain real filtering/sort state. Account overview separates
user name, server name and address; Settings leads to its independent page, while Personal Lists
remains in the main navigation. Saved
connection health is distinct from selected profile identity. Settings groups remain scrollable
with large text and preserve History, local recovery, diagnostics and account management even
where those inherited capabilities have no dedicated design board. Native semantic color roles
in `PilotTheme.kt` supply filter, account/settings and neutral track-selection surfaces in both
themes; this is the Android projection, not a second desktop styling system.

The later 2026-09-22 subtraction pass keeps server/account management in My, removes its
duplicate Personal Lists shortcut and the duplicate account group in Settings. Player Audio
remains a direct control, with no duplicate More entry; nested panels restore their actual
opening control. Gesture Help uses separate header Back and Close actions, without a redundant
acknowledgment footer. Remove self-evident login/subtitle-off descriptions while preserving
address requirements, failures and actual track state. Tablet short seasons already display the
complete episode list without redundant preview-play, expand or episode-search controls.
The focused five Compose test classes passed 25 tests and Android lint reported zero errors;
physical-device and visual acceptance remain pending.

The synchronization also fences season queries when replacing pending pagination, so changing
season immediately starts its first page and late old-season results cannot overwrite it.
MediaSession discards unconfirmed play/seek intents on admission loss, generation replacement
and terminal states, preventing those intents from reappearing in a later session.

### Earlier mobile design integration checkpoint — 2026-09-21

The user identified the mobile design as substantially settled and approved implementation with
all recommended choices below. Physical-device testing is deferred until the complete Android
implementation is ready. These decisions do not claim implementation or visual acceptance.

At that checkpoint the [Paper Mobile page](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/5-0)
contains 40 interface boards and one interaction guide. Its companion design package defines
mobile interaction and shared playback/list behavior; historical handoff notes are subordinate to
those current specifications. Phone navigation is Home / Personal Lists / Library / Account,
with Search as a tool entry. Tablet and foldable reference boards now live on the separate
Pad&Duo page. The existing exclusion of fold-posture-specific behavior still applies.

- Dark phone Home, Series detail, and Movie detail use the settled immersive composition.
  Light Home and Series remain explicitly marked as awaiting that adaptation; Library and
  Settings are aligned across themes. Derive missing light compositions during implementation
  from the settled dark structure and existing light tokens, retaining structural theme parity.
- The first-release capabilities above still include authentication, Watch History, and Restore
  Local Playback even where the phone canvas has no dedicated board. Extend the established
  mobile components for these flows without requiring additional Paper designs.
- The mobile skip specification describes a session-only toggle and automatic-skip undo.
  [ADR 0045](adr/0045-desktop-series-intro-skipper-preferences.md)'s persisted series preference
  is explicitly desktop-only; sharing playback execution must not silently import that policy
  into Android. Integration must preserve the platform's accepted semantics.

At the initial checkpoint, Android Home was a basic poster grid and the independent URL/file
player did not establish reporting or server resume. The implementation below replaces those
bring-up surfaces with the mobile product and shared playback path; real-server/device acceptance
remains a separate result from protocol-fixture and code checks.

The approved test approach combines focused shared-Rust regressions, deterministic protocol
fixtures for failed/delayed/stale server operations, Compose semantics and restoration tests,
and existing JNI/Keystore/lifecycle instrumentation. Add a real test-server playback/report/resume
flow to verify the full boundary. Use controlled clocks for chrome/undo timing, and distinguish
Activity recreation from actual process-loss recovery. Human review continues to own appearance,
subtitle rendering, and SDR/HDR output on recorded physical-device/media combinations.

The pre-implementation instrumentation sources contained ten tests: six native-player, three
lifecycle, and one Keystore test. The nine-test pass recorded below is historical evidence for
that earlier suite; no tests were rerun for this checkpoint. Android tooling is installed, but
the existing adb server reported no attached devices during this inspection. Selection of the
first physical acceptance device and the remaining phone/tablet/output matrix is deferred until
implementation is complete, at the user's request. Continue automated checks in the meantime;
report unavailable device-dependent checks separately, without treating them as passed.

## Playback lifecycle

“Foreground” for Android playback means the app interface is visible and the device is unlocked, not merely that the process exists. A visible split-screen app remains eligible without input focus. Audio focus is a separate system constraint and must be honored.

| Event | Required outcome |
| --- | --- |
| User starts an item locally or via an eligible remote command | Resolve it within the active Profile Scope and present the playback page; commands and player observations carry the current operation/session identity. |
| User explicitly leaves the playback page | End the Playback Session, attempt the stop report, release playback resources, and clear that session's Local Playback Recovery Point. |
| App becomes invisible or device locks | Pause and retain the session while its owner survives; preserve the Local Playback Recovery Point. Do not interpret this as an explicit stop. |
| App becomes visible/unlocked again | Remain paused until an explicit eligible play command; never auto-resume merely because visibility returned. |
| Surface is destroyed/recreated by rotation or window changes | Detach/rebind safely without declaring a playback end, creating a duplicate business session, or resetting its position. Surface existence alone does not determine business lifetime. |
| Playback finishes naturally | End the item and clear its recovery point; existing natural-end next-episode rules remain applicable. Any next-item start remains subject to foreground eligibility. |
| Profile Switch succeeds | Finish old playback and remote teardown before activating the candidate, preserving ADR 0033's failure ordering; clear recovery for the ended playback. |
| Candidate authentication fails or is cancelled before handoff | Keep the old active profile and Playback Session. |
| Process is reclaimed | The live player/session no longer exists. Do not claim that final stop reporting succeeded or that reopening resurrects the old session. |

The player instance and its Surface have separate ownership. Neither a Composable disposal callback nor every Surface loss may unconditionally destroy the player. Conversely, retaining an instance must not allow background audio or background remote starts. Surface detach must wait for a safe native handoff rather than assuming an option/property update has synchronously ended native use.

Remote command admission is rechecked when executing pending work, not only when receiving it. Commands arriving while ineligible are not saved for playback after returning. Remote registration/availability should reflect the Android lifecycle, but a stale server-side target listing must never bypass local admission checks. Stop/cleanup operations must remain possible even when starts are disallowed.

Media3 exposes actual player facts and the commands currently available. Buffering, desired play state, actual playing, seek completion, natural end, errors, and tracks must not be collapsed into one playing boolean. System media commands enter the same business-control path as local/remote controls and obey the same visibility/lock restrictions. A MediaSession does not imply background playback or a MediaSessionService requirement.

## Local interruption recovery

A Local Playback Recovery Point is a device-local, Profile-Scope-owned item reference and last observed position for interrupted Android playback. It is not media caching, server Watch History, or a second global resume authority. Persist sufficient recovery data while the process is alive; correctness must not depend on receiving a process-death callback.

- **Restore Local Playback** uses the local position and requires an explicit user action. If the previous session is gone, validate the active profile, prepare playback again, and create a new Playback Session.
- **Continue Watching** and ordinary server-resume paths continue to use server progress. Do not take the larger of local and server positions or silently merge them.
- If the phone stopped at 20 minutes and another device advanced to 35 minutes, the explicitly labeled local recovery entry uses 20 minutes; Continue Watching uses the server's position.
- Temporary backgrounding, locking, or process reclamation retains recovery. Explicit player-page exit, natural completion, or a successful switch that ends that playback clears its recovery point. Sign Out removes recovery for that profile, independently of the optional Watchlist deletion choice.
- Failure to authenticate or resolve an item is a real error, not permission to use old credentials or a stored secret-bearing media URL. Ordinary persisted recovery data contains no access token or authenticated playback URL.

## Shared SDK and platform ownership

Desktop and Android share account lifecycle, User Data Actions, Library Browser execution, playback preparation/report serialization, and remote command translation. Android owns the physical JNI player, eligible-target lifecycle, and Compose presentation:

```text
crates/jellypilot-sdk/       # shared account, browse, collection, playback and remote semantics
crates/jellypilot-ffi/       # UniFFI conversion and external object/operation lifetime
android/app/                # Compose, navigation, ViewModels, Android application wiring
android/core-bridge/        # Kotlin SDK wrapper and generated bindings
android/player-mpv/         # libmpv JNI, Surface ownership, Media3 adapter
scripts/                    # existing task dispatcher extended with Android builds/checks
```

`jellypilot-sdk` orchestrates existing domain modules instead of copying them. It owns account/session state and operation ordering, network tasks, request correlation, cancellation policy, and confirmed content changes. Move reusable business execution out of iced orchestration and desktop-coupled player code along actual consumer paths. Each migrated rule must have one implementation used by both desktop and Android; this does not require desktop to adopt Android foreground-only presentation behavior or use FFI.

User Data Actions use `jellypilot-sdk::item_actions` on desktop and through SDK/UniFFI methods on Android. Favorite, Played, and Watchlist writes share per-item admission; different items remain independent. Token validation and admission are atomic with SDK profile transitions, and server flags are accepted only after confirmation. Desktop keeps an item busy through queued result delivery. Watchlist storage adapters carry admission into the actual blocking write, so cancelling an async waiter cannot release it early. Android writes have their own per-item lifetime instead of occupying the browse/search query slot; pending controls and confirmed-result projections remain presentation concerns.

Library Browser execution uses `jellypilot-sdk::browse::Browser` on desktop and scope-bound `BrowseSession` objects through UniFFI on Android. The SDK executes pagination effects, cancels superseded HTTP work, rejects stale deliveries, and reconciles confirmed user-data writes. Platforms provide viewport demand and render bounded sparse windows; they do not append pages or own a second paging queue. Refresh retains usable content until its replacement is ready, and suspension preserves unfinished work for navigation return. Desktop retains geometry and artwork presentation in navigation history; Android retains grid scroll and search text across detail/player navigation. Closing an Android session calls `shutdown()` before releasing its UniFFI handle.

Account lifecycle uses the same SDK for isolated authentication, activation, credential persistence, Startup Auto Login selection, Disconnect, and Sign Out. Native client, identity, and request-gate fields project committed SDK outcomes. Desktop retains its physical playback and remote teardown adapters; an always-active subscription delivers SDK handoff requests, and both teardown receipts must settle before adoption or disconnection. Android keeps its player and lifecycle admission adapter. Neither platform pauses existing playback merely because credential deletion has begun.

Desktop playback commands capture SDK admission before dispatch. Account transitions permanently revoke queued starts/resumes, including across a failed deletion that reopens admission. Already executing physical playback work drains before protected deletion or teardown begins, avoiding cancellation of a partially loaded player. Cleanup commands remain executable while playback admission is closed.

Sign Out securely removes the selected Saved Service Profile first. Failed credential deletion preserves the connection and playback; successful deletion is never rolled back. If subsequent teardown fails, authentication remains connected for cleanup retry while new playback and content writes are blocked. `disconnect()` retries that cleanup without deleting credentials again, and ends the scope only after successful teardown. Signing out an inactive profile preserves the active profile. Watchlist deletion remains separately opt-in, uses the platform's existing store, and reports independent failures with an explicit retry. A profile with pending Watchlist cleanup cannot reactivate until cleanup succeeds. SDK `close()` is terminal: committed work retains authentication until settlement, then authentication is released even when teardown failed.

Automated account/store/protocol tests and native startup smoke cover these adapters; they do not establish real-server/device or human visual acceptance required by the later gates.

| Responsibility | Owner |
| --- | --- |
| Login/profile transitions, provider compatibility, authenticated media queries | Rust SDK and existing auth/media-server/domain modules. |
| Browse request admission, paging semantics, stale rejection, user-data reconciliation | Rust SDK; platforms provide demand and render snapshots. |
| Playback preparation, track preferences, reporting rules, episode/Intro Skipper policy | Shared Rust business owner. |
| Actual libmpv instance, JNI event handling, Surface attachment, Android playback capabilities | Android player module. It executes host operations and returns correlated facts/outcomes. |
| System media presentation and commands | Media3 adapter over the same playback facts/control path; no independent business queue or reporting loop. |
| Protected credential storage and platform directories | Kotlin platform adapter injected through the SDK host interface. Keystore protects keys; encrypted credential data lives in private storage. |
| Android theme/navigation/interaction preferences | Kotlin platform storage, one authoritative copy per setting. |
| Shared business settings and recovery rules | Rust ownership, with platform-provided storage location/mechanism where needed. |
| Image references, access/authentication rules, fallback order | Shared Rust semantics. |
| Android image download, original-response disk cache, decode, memory cache | Android image module only; no parallel Rust image cache or cross-FFI RGBA pipeline. |

### Interface invariants

- Use structured domain requests, results, snapshots, and typed errors rather than exporting every internal type or reducing all failures to display strings. `Disconnect` and `Sign Out` remain distinct operations, not an ambiguous `logout()`.
- A playback plan describes media access, start position, media-source identity, subtitle access, and provider playback-session identity. It does not launch a desktop player or carry a Surface. Secret-bearing access data is available only to modules that need it and is excluded from ordinary UI state, logs, and persistent recovery records.
- A plan is bound to the originating Profile Scope and operation/session generation. Late preparation, native events, and callbacks from an ended player or replaced profile cannot mutate the current session or load obsolete media.
- UniFFI handles conversion and asynchronous calling, not business cancellation. SDK-owned cancellation and stale-result rejection must be explicit. Cancelling a consumer does not prove a server mutation was rolled back; irreversible handoff cleanup must not be abandoned as though nothing happened.
- Own the SDK and required Tokio runtime at application/process scope, not per Compose recomposition. Bind work to the relevant profile, operation, or content lifetime. Subscriptions have an explicit close path, and restarting a collector must not duplicate business operations.
- Platform player/credential adapters return outcomes to the SDK. Kotlin does not independently reconstruct the profile-handoff sequence, track preference precedence, or progress-report cadence. Platform callbacks must not require blocking the Android UI thread on Rust/network work.
- Do not move video frames, raw cover rasters, or desktop viewport geometry across the business interface. Large libraries use bounded page/data delivery rather than full-library snapshots.
- Generated bindings and native libraries are reproducible artifacts from pinned sources/tooling, not hand-edited parallel implementations. No independently versioned public SDK compatibility promise is made.

## libmpv and output acceptance

Keep the player integration `Kotlin → thin JNI → libmpv`, with Compose hosting the Android video view through `AndroidView`/`SurfaceView`. Business control may cross the SDK interface, but native rendering and Surface ownership do not route through Rust or desktop iced/wgpu interop.

The engine is fixed to libmpv with an application-managed configuration. Do not silently choose another engine or request Provider Transcode if a sample fails. Narrow and document the supported device/media matrix instead.

For metadata-confirmed Dolby Vision Profile 5 samples, validated HDR-capable targets must deliver correct HDR output; native Dolby Vision signaling is not required. On other supported devices, correct SDR mapping is acceptable. Opening the file, decoding frames, or reporting a codec name is not proof of correct color or HDR presentation. When neither accepted output path works, report unsupported playback explicitly.

Validate audio tracks, external/styled subtitles as supported, seeking, buffering/error transitions, player-instance cleanup, Surface recreation, and foreground restrictions alongside the media/output matrix. Human device testing owns color, HDR and visual acceptance. Record device/OS, media metadata, native dependency versions and actual output evidence; do not infer these results from desktop playback or emulator startup.

## Build, delivery, and acceptance gates

The first release remains a controlled, signed `arm64-v8a` APK for verified devices. Bring-up declares minimum API 26 and compile/target API 37; dependency pins are recorded below. These build inputs are not device-compatibility guarantees. The concrete device/sample matrix and signed release acceptance remain outstanding.

Keep a monorepo with independently runnable Android preparation/build/release tasks. A fresh Android build must not require materializing `target/vendor/iced`, preparing desktop MPV assets, or installing desktop UI dependencies. The implementation separates the display-free root Cargo workspace from the desktop workspace under `src-iced`; Android dispatcher builds now pass. A clean-room Android-only build has not yet been exercised, so Gate 4 isolation acceptance remains open. Continue using the repository task dispatcher for Cargo operations; do not introduce undocumented direct-Cargo workflows.

Pin and reproduce Rust cross-compilation, UniFFI generation, libmpv and transitive native builds, JNI packaging, APK signing/upgrades, and native symbol retention. Retain dependency provenance and applicable license/source obligations. Check all packaged native libraries for 16 KB compatibility, APK alignment, and actual runtime behavior on an applicable environment; Kotlin UI and controlled distribution do not exempt native dependencies.

| Gate | Deliverable and evidence required |
| --- | --- |
| 1 — Risk bring-up | Two independently verifiable paths: Compose → Rust login/query with real error propagation, cancellation and stale-account rejection; independent libmpv playback with tracks/subtitles/seek, Surface recreation, and P5 HDR/SDR device evidence. Neither is the finished client. |
| 2 — Shared vertical path | Real-server account → browse → detail → playback → reporting → resume flow; desktop consumes the same migrated business rules. Verify failure/cancellation during profile handoff, late events after session replacement, and reporting failures without fabricated success. |
| 3 — Product completeness | All applicable capability groups above, phone/tablet prototype integration, system MediaSession, eligible remote control, and local interruption recovery. Exercise visible split-screen, lock/background, explicit exit, recreation, and process-loss/manual recovery scenarios. |
| 4 — Controlled delivery | Clean Android-only build, pinned/reproducible native dependencies and bindings, signed install and upgrade, symbols/licenses, 16 KB evidence, documented device/output matrix, and human UI/color acceptance. |

For each gate record pass/fail/unavailable separately. Missing device, sample, or output evidence is not a pass. Desktop changes require the relevant repository gates from [validation policy](agents/validation.md); Android commands and checks must be added as working dispatcher entries during implementation, not invented here. Documentation-only recording does not require an application build.

## Phone playback gestures and launcher icon — 2026-09-22

The follow-up to the mobile synchronization enables phone picture gestures from the
[Mobile design](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/5-0).
Single tap controls chrome visibility; side double taps seek ±10 seconds and accumulate same-side
taps; horizontal drag previews one seek and latches cancellation after dragging down; vertical
left/right gestures adjust picture brightness/player volume; center hold temporarily selects 2×.
Visible controls remain available. The device-local **Player gestures** switch defaults on, and
the Picture and gestures panel includes an optional, scrollable help page.

Recognition reserves system insets and at least 24dp at each edge. Buttons and entire actionable
overlays own their hit regions. A common ancestor cancels recognition synchronously when a second
pointer arrives, including one on a control. Touch exploration retains Full chrome and disables
custom gestures. Recognition/adjustment suspend auto-hide; feedback uses the compact passive
Mobile indicators and a 130ms opacity transition, disabled by Reduced motion.

Seek preview captures the playback position and intended paused/playing state, pauses the native
clock and automatic marker decisions, and commits once on release. Cancellation restores the
origin. Explicit pause, focus/background loss and media replacement revoke the old transaction's
permission to resume. Token/generation checks reject stale releases. An explicit seek suppresses
automatic handling of markers containing its target until that marker is left. Temporary speed
restores the captured base rate before pause, interruption or replacement. Volume and seeking
availability are confirmed by the native player and also govern visible and MediaSession controls.

The launcher uses the existing transparent JellyPilot mark over a separate graphite background,
with adaptive/round and API 33 monochrome resources. The original mark's aspect ratio and protected
66dp circle are preserved; launcher masks and themed rendering still need human acceptance.

Native subtitles remain in the video compositor. Passive Compose feedback reserves the lower
64dp picture lane and is behind actionable overlays; arbitrarily positioned ASS subtitles can
still overlap it. Check unusual subtitle placement, safe areas, large text, brightness/color and
gesture feel on a real device. Automated interaction/native checks do not establish appearance.

| Follow-up verification | Result |
| --- | --- |
| Shared bindings / ARM64 FFI | Regenerated and rebuilt through the dispatcher. |
| `bun run check` / Rust workspace tests | Passed; 1,391 reported passes, zero failures, one ignored lifecycle helper. |
| Android host tests / lint | 64 app cases and four player cases passed; lint has zero errors and 55 warnings. |
| Android native instrumentation | All 34 cases passed on API 35 x86_64 with ARM64 translation, including four gesture transaction cases and the existing admission/lifecycle fixtures. |
| APK | Debug signature and 16 KB ZIP alignment passed; all 13 ARM64 libraries are uncompressed and have ≥16 KB ELF load alignment. Packaged adaptive/round/monochrome icon references are present. |

Current debug APK: `android/app/build/outputs/apk/debug/app-debug.apk`, **567,013,170 bytes**,
SHA256 `437a671a66b77d50206d825f4f11692dd493f78a954d0ea41dea154d15c88fa5`.
Actual 16 KB-page runtime, real-device visuals/audio/HDR and controlled release remain separate.

## Mobile synchronization verification — 2026-09-22

The preceding phone design synchronization excluded custom playback gestures. It included the updated Home,
library/search, details and tracks, personal lists, account/settings, two-step server login and
landscape player. Related inspection fixed season replacement admission, account-to-library
navigation, unconfirmed MediaSession intents, duplicate MediaSession IDs, and native file
observations arriving after visible playback state was retired. A stopped file's cache event
can no longer change an empty snapshot back to Ready; late picture/speed commands cannot change
the reset values. Each player owner has its own MediaSession ID and coordinates readiness with
resource closure.

| Check | Result and scope |
| --- | --- |
| `bun run check` / `bun run task rust test` | Passed across the maintained shared/desktop workspaces; 1,387 reported test passes, zero failures, one ignored helper exercised by lifecycle tests. No desktop startup boundary changed. |
| Android bindings and ARM64 Rust build | Regenerated from the final shared API and cross-compiled through the dispatcher. |
| Android host tests | 40 app/Compose/Media3 cases and four player cases passed. |
| `:app:lintDebug` | Passed with zero errors and 56 warnings. |
| `:app:connectedDebugAndroidTest` | All 30 cases passed on API 35 x86_64 with `libndk_translation` ARM64 support. A final two-case native picture/settings rerun also passed after tightening rejection of commands after stop. |
| APK packaging | Debug signature, 16 KB ZIP alignment, and all 13 ARM64 libraries' uncompressed packaging and ≥16 KB ELF load alignment passed. Actual 16 KB-page runtime remains unverified. |

The HTTP/native fixtures verify public-probe restrictions and password-login fallback, cancellation
without replacing the active account, real season switching, playback/report/resume, picture-setting
readback and retention through continuous replacement, explicit-exit reset, independent resource
owners, immersive rotation, and late-event rejection. Shader property/track checks are not a visual
proof of brightness, subtitle composition or HDR. Human phone/tablet, real-server, output and
controlled-release acceptance remain separate; use [the device checklist](android-testing.md).

Pre-gesture debug APK: `android/app/build/outputs/apk/debug/app-debug.apk`, **566,221,610 bytes**,
SHA256 `2d78266825060365d4bb05d4488116fdd0af725074bca27dcbb6e39c99684d4e`.
It retains native symbols and uses the development signing key.

## Earlier mobile implementation and verification — 2026-09-21

The Android product now includes the four-destination phone shell, adaptive tablet rail,
immersive Home and details, retained Library/Search state, seasons and episodes, preplay and
in-player track selection, account and presentation settings, and recovery. Authentication,
queries, collection mutations, preferences and playback plans use the shared SDK through UniFFI.
Home uses the existing Featured Item policy and authoritative parent-series action state.

Watchlist batch Undo restores original records and ordering. Favorites accept only confirmed
server writes, including partial failure. Android History hiding is local to its Profile Scope;
its paginated SDK query skips hidden records without changing Played or server resume state.

The Android coordinator connects shared playback/reporting to the actual JNI host. Loads start
paused, confirm initial volume/tracks, then resume within one admitted physical operation.
Admission survives cancelled FFI waiters until native completion or confirmed disposal. Profile
transitions revoke queued intents before credential work, while failure before teardown preserves
ongoing playback. Visibility loss acknowledges pause; Surface changes do not end the session.
Unexpected native termination preserves recovery, unlike explicit exit or natural completion.
Remote commands and system media controls use the same session path. Android skip Undo switches
only that session to Manual.

| Check recorded on 2026-09-21 | Evidence |
| --- | --- |
| `bun run check` | Passed: 27 dispatcher tests and maintained Rust formatting/clippy. |
| `bun run task rust test` | 1,343 passed, zero failed, one ignored helper invoked by lifecycle tests. |
| `xvfb-run -a bun run task iced run --smoke` | Passed: desktop startup only. |
| `bun run task android check` | Passed: five Compose host tests, four player host tests, lint with zero errors and 50 warnings. |
| `bun run task android build` | Passed: fresh bindings, ARM64 Rust bridge, pinned native dependency pipeline and debug APK assembly. |
| Android instrumentation | 16 unique cases passed on API 35 Google APIs x86_64 with verified `libndk_translation` ARM64 support: 14 baseline passes, the corrected external-subtitle fixture, and the added full playback flow. The focused reruns replaced two invalid fixture assumptions; no final case is failing or skipped. |
| Full playback flow | Real HTTP fixture → Rust/UniFFI → Android coordinator → libmpv passed: login, Home/detail queries, native playback, paused seek, track confirmation, start/progress/stop payloads, recovery clearing, and a second start using server resume. This is a protocol fixture, not a deployed Jellyfin/Emby server acceptance result. |
| APK packaging | Signature verification and 16 KB ZIP alignment passed; all 13 native libraries are ARM64, uncompressed, with at least 16 KB ELF load alignment. This is not a 16 KB-page runtime result. |

Focused independent source review covered native admission/cleanup, profile transitions, stale
terminal events, cross-profile Undo, partial collection operations and interruption recovery.
The identified issues were fixed; runtime tests remain separate evidence.

The artifact recorded for that pass was a debug-signed development APK, version `2.2.2-android-preview`, retaining
native symbols: `android/app/build/outputs/apk/debug/app-debug.apk`, 564,620,403 bytes,
SHA256 `217213b48adf3bf2ceb75653d63c139d7b9c86df9fcef2003cd19b08c26e4657`.
The final UI edits also passed `:app:lintDebug`; the APK was rebuilt by the final instrumentation
run and its signature/alignment rechecked. The temporary emulator was shut down after testing.
See [Android verification and device acceptance](android-testing.md) for commands
and the human checklist. The earlier physical-device pass below applies only to risk bring-up.
Current real-server/device UI, audio, HDR/SDR, process-loss, install/upgrade and controlled-release
acceptance must be recorded separately; the user requested those physical checks after implementation.

## Bring-up implementation and evidence

Recorded on 2026-09-15. The Android application contains Compose account/browse/search/detail wiring, a generated UniFFI/JNA bridge, Android Keystore-backed encrypted credentials outside backup storage, scoped artwork requests, and an independent libmpv player with Media3 and Surface ownership. The player probe does **not** create media-server Playback Sessions or reporting/resume flows. Desktop SDK migration, the shared playback vertical path, complete phone/tablet product behavior, and controlled release delivery remain later gates.

### Reproduction

With the Android SDK and pinned toolchain installed, set `ANDROID_HOME` or `ANDROID_SDK_ROOT` and use:

```bash
bun run task android doctor
bun run task android build
bun run task android check
android/gradlew -p android :app:connectedDebugAndroidTest --no-daemon
```

`android build` generates Kotlin bindings, cross-compiles the ARM64 Rust bridge, builds the pinned native dependency chain/JNI, and assembles the debug APK. `android check` regenerates bindings and runs Gradle lint/unit checks. Device instrumentation requires an attached ARM64-capable Android environment; its synthetic media assets are generated with host FFmpeg/libx264. Use `ANDROID_SERIAL` to select the intended test device. Instrumentation drives the application's own lifecycle and Surface without screenshots or injected input.

The debug artifact is `android/app/build/outputs/apk/debug/app-debug.apk` (approximately 494 MiB with unstripped native symbols). It is not the controlled release artifact. Dependency authorities are the [version catalog](../android/gradle/libs.versions.toml), [Gradle wrapper](../android/gradle/wrapper/gradle-wrapper.properties), and [native dependency manifest](../tools/android/mpv-deps.json): Gradle 9.7.1, AGP 9.4.0, Kotlin/Compose compiler 2.4.0, Compose BOM 2026.09.00, Media3 1.11.1, UniFFI 0.32.1, and NDK 28.2.13676358. The native manifest pins source hashes/commits and the Android-only forced-demuxer patch; generated native provenance and license material accompany the build.

### Observed results

| Check | Result and scope |
| --- | --- |
| `bun run check` and `bun run task rust test` | Pass: dispatcher checks and relevant Rust workspace gates, including the separated desktop workspace. |
| `xvfb-run -a bun run task iced run --smoke` | Pass: desktop first-frame smoke, not human visual acceptance. |
| `bun run task android build` | Pass: complete dispatcher entry through native build and debug APK assembly. This was not a clean-room build. |
| `bun run task android check` | Pass: Android lint/unit checks; warnings are not release acceptance. |
| Complete Android instrumentation suite | **9 passed, 0 failed, 0 skipped** on API 35 `JellyPilotGate1X86`, an x86_64 emulator running the packaged ARM64 libraries through translation. Six native-player tests cover relative HLS/DASH, rejection of directory/M3U expansion, denied Media3 play, Surface detach/rebind, borrowed-descriptor lifetime/seek/track selection, and generation retirement. Two lifecycle tests cover background/recreation remaining paused and cancellation of a real Binder provider whose descriptor returns late. One real Keystore test covers reopen, encrypted storage, and tamper rejection. |
| Android/JNA → ARM64 Rust → real LAN server | Pass: temporary login, three libraries, browse/pagination, detail, search, artwork, typed not-found/cancellation, stale-scope rejection, and logout. This exercises the real bridge/server path, not human interaction with the Compose UI. Credentials were supplied privately at runtime; the probe did not change favorite/watched state or submit playback reports. The temporary probe is not part of the permanent suite. |
| APK native alignment | Pass: all **13** packaged native libraries are ARM64, uncompressed, with 16 KB-aligned ELF load segments and ZIP data offsets. Actual 16 KB-page runtime behavior remains unavailable. |
| Physical-device manual verification | Pass, as reported by the user after the USB/device-debugging workflow. The device model, OS version, exact scenarios, and sample/HDR metadata were not supplied with this result; no additional coverage is inferred. |

Source/API reviews found no remaining findings in the reviewed SDK transaction, Android adapter, native lifetime, and final test-fixture changes. Review is not a substitute for the runtime results or missing device evidence.

### Human acceptance scope and remaining evidence

The user's manual pass is accepted as the human verification result, not an agent reproduction. The checklist below defines the device/output matrix to record; the general pass report does not establish separate results for every listed scenario.

- On the intended physical phone and tablet, inspect account/browse/detail/player layouts, light/dark appearance, Latin/CJK fonts and weights, clipping, and navigation.
- Confirm actual video/audio output, audio selection, external SRT and styled subtitle appearance, seeking, and Surface reconstruction with representative files.
- Exercise visible split-screen, screen lock/unlock, background/return, audio-focus loss, headset removal, and explicit exit. Returning to eligibility must not resume playback without a new user action.
- For metadata-confirmed HDR10 and Dolby Vision Profile 5 samples, record device/OS, media metadata, pinned native versions, and human-observed HDR output or correct SDR mapping. Emulator clocks, codec names, and successful file opening do not establish color correctness or hardware decoding.
- Gate 4 additionally needs a clean Android-only build, controlled signing/install/upgrade, release symbol/license handling, and an actual 16 KB-page runtime.

No screenshot, visual judgment, physical-device output, or HDR acceptance was performed by the agent. Physical-device manual verification was reported passed by the user; detailed P5 HDR/SDR evidence for Gate 1 remains unrecorded. Gates 2–4 have not been accepted.

## Source evidence and recording scope

Existing extraction points include [account handoff](../src-iced/src/app/accounts.rs), [browse operations](../src-iced/src/app/browse.rs), [confirmed collection changes](../src-iced/src/app/collections.rs), [player controller](../crates/jellypilot-mpv/src/playback.rs), [playback state machine](../crates/jellypilot-mpv/src/playback_session.rs), and [remote runtime](../src-iced/src/app/playback/remote.rs). Playback preparation/reporting and remote command translation now have shared SDK consumers on both platforms; the desktop host and Android JNI owner retain their distinct physical lifecycles. [ADR 0033](adr/0033-validate-before-active-profile-handoff.md), [ADR 0038](adr/0038-state-owned-intro-skipper-policy.md), and [ADR 0039](adr/0039-native-media-segments-for-intro-skipper.md) remain relevant contracts.

Primary references checked during design:

- [UniFFI async/future support](https://mozilla.github.io/uniffi-rs/latest/futures.html): Rust async/Kotlin suspend interoperability; library-specific cancellation must be designed explicitly.
- [Media3 Player interface](https://developer.android.com/media/media3/session/player): custom `SimpleBasePlayer`, MediaSession integration, and distinctions among buffering, requested play and actual playing.
- [mpv-android](https://github.com/mpv-android/mpv-android): an integration reference, explicitly not an importable AAR.
- [BaseMPVView source](https://github.com/mpv-android/mpv-android/blob/master/app/src/main/java/is/xyz/mpv/BaseMPVView.kt): Surface integration reference; its detach-race warning is not a proven-safe implementation to copy verbatim.
- [Android 16 KB support](https://developer.android.com/guide/practices/page-sizes): native-library, package and runtime validation.

The design references above establish the accepted direction. Current implementation and measured limits are recorded in the bring-up section; they do not imply completion of later gates, a signed release, Paper changes, or a commit.
