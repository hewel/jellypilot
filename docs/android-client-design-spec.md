# Android Client Design Specification

_Status: Accepted design, 2026-09-14; risk bring-up implemented and automated checks recorded on 2026-09-15. The user subsequently reported physical-device manual verification passed. The device/sample/HDR matrix is not yet recorded; this is not the finished Android client._

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

A prototype entry is not automatic feature authorization. In particular, the current “快捷键” entry must not become a dead settings item: the implemented Android settings navigation must omit or revise it to match the standard-input-only decision. Likewise “MPV” does not authorize arbitrary configuration. Missing prototype pages do not silently remove inherited business capabilities. Paper itself was not modified in this design session; final visual acceptance is human-owned.

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

The following paths now contain the risk bring-up implementation. Desktop adoption of the shared business SDK remains a Gate 2 migration:

```text
crates/jellypilot-sdk/       # shared business-operation owner; desktop adoption is pending
crates/jellypilot-ffi/       # UniFFI conversion and external object/operation lifetime
android/app/                # Compose, navigation, ViewModels, Android application wiring
android/core-bridge/        # Kotlin SDK wrapper and generated bindings
android/player-mpv/         # libmpv JNI, Surface ownership, Media3 adapter
scripts/                    # existing task dispatcher extended with Android builds/checks
```

`jellypilot-sdk` orchestrates existing domain modules instead of copying them. It owns account/session state and operation ordering, network tasks, request correlation, cancellation policy, and confirmed content changes. Move reusable business execution out of iced orchestration and desktop-coupled player code along actual consumer paths. Each migrated rule must have one implementation used by both desktop and Android; this does not require desktop to adopt Android foreground-only presentation behavior or use FFI.

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

Existing extraction points include [account handoff](../src-iced/src/app/accounts.rs), [browse operations](../src-iced/src/app/browse.rs), [confirmed collection changes](../src-iced/src/app/collections.rs), [player controller](../crates/jellypilot-mpv/src/playback.rs), [playback state machine](../crates/jellypilot-mpv/src/playback_session.rs), and [remote runtime](../src-iced/src/app/playback/remote.rs). Their presence is reuse evidence, not proof that they already form an Android SDK. [ADR 0033](adr/0033-validate-before-active-profile-handoff.md), [ADR 0038](adr/0038-state-owned-intro-skipper-policy.md), and [ADR 0039](adr/0039-native-media-segments-for-intro-skipper.md) remain relevant contracts.

Primary references checked during design:

- [UniFFI async/future support](https://mozilla.github.io/uniffi-rs/latest/futures.html): Rust async/Kotlin suspend interoperability; library-specific cancellation must be designed explicitly.
- [Media3 Player interface](https://developer.android.com/media/media3/session/player): custom `SimpleBasePlayer`, MediaSession integration, and distinctions among buffering, requested play and actual playing.
- [mpv-android](https://github.com/mpv-android/mpv-android): an integration reference, explicitly not an importable AAR.
- [BaseMPVView source](https://github.com/mpv-android/mpv-android/blob/master/app/src/main/java/is/xyz/mpv/BaseMPVView.kt): Surface integration reference; its detach-race warning is not a proven-safe implementation to copy verbatim.
- [Android 16 KB support](https://developer.android.com/guide/practices/page-sizes): native-library, package and runtime validation.

The design references above establish the accepted direction. Current implementation and measured limits are recorded in the bring-up section; they do not imply completion of later gates, a signed release, Paper changes, or a commit.
