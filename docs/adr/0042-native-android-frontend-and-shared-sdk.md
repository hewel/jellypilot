# Native Android frontend with a shared Rust business SDK

_Status: Accepted, 2026-09-14; not implemented or Android-validated. Amends [ADR 0027](0027-cross-platform-iced-frontend.md)'s single-frontend scope and desktop storage assumptions, and [ADR 0037](0037-shared-library-image-disk-cache-owner.md)'s cache-owner scope for Android. The [Android client specification](../android-client-design-spec.md) records product scope and acceptance gates._

Android is an independent phone and tablet client, not a port of the iced UI. Use Kotlin and Jetpack Compose for Android presentation and system integration, while iced remains the desktop frontend. Reuse the existing [Paper Mobile designs](https://app.paper.design/file/01M1XG9QCM2M58ENY2ZVA2YTWA/5-0/5UF-0), adapting navigation and interaction to Android rather than copying desktop windows or components. Remove iced Android entry-point, IME, and window-runtime experiments from the Android preparation plan.

Keep Android in this repository, with independent build and release paths. The shared SDK is an internal module, not an independently published compatibility contract: desktop callers, Rust implementations, and generated Kotlin bindings evolve together. Splitting repositories would introduce SDK publication and cross-repository coordination without serving the chosen ownership model.

## Shared business ownership

Add `jellypilot-sdk` above the existing domain crates to own business-operation execution, and a thin `jellypilot-ffi` for UniFFI conversion, async operations, errors, and subscriptions. Desktop consumes the SDK directly in Rust, not through FFI. Shared stateful rules include account handoff, request correlation and stale-result rejection, browse operations, user-data reconciliation, playback preparation, track preferences, reporting, and remote-command policy. Kotlin ViewModels own presentation state, not another implementation of these rules.

Extract each rule with a real end-to-end consumer and migrate both frontends for that path. Do not build an entire public SDK before consuming it, and do not call an Android-only wrapper complete while desktop retains a second business implementation. Existing domain crates retain their useful policy modules; SDK orchestration does not absorb rendering, platform storage mechanisms, or desktop process control. In particular, preserve [ADR 0033](0033-validate-before-active-profile-handoff.md)'s validated profile handoff and [ADR 0038](0038-state-owned-intro-skipper-policy.md)'s state-owned Intro Skipper semantics.

The SDK defines platform-host interfaces for protected credential storage and player operations. Platform adapters execute those operations and return observations and outcomes; the SDK remains responsible for business ordering. Kotlin protects credentials using Android Keystore-backed encryption and private storage. Settings have one owner per setting: Android presentation preferences belong to the Android platform layer; shared business preferences belong to Rust.

## Independent Android player and images

Use a Kotlin player module with a thin JNI integration to libmpv and an Android Surface hosted by Compose through `AndroidView`. Do not route native video frames or Surface ownership through Rust, and do not import the desktop iced/wgpu compositor into Android. libmpv is fixed for this design: narrow the validated device/media support matrix rather than silently switching engines or requesting Provider Transcode.

Adapt observed libmpv state to Media3 `Player` using `SimpleBasePlayer` and expose a `MediaSession` for system media controls. Commands enter the same business-control path. Media3 does not become a second queue, reporting, or playback-policy authority. Foreground-only playback does not require adopting `MediaSessionService` as a background-playback architecture: keep player-instance lifetime separate from Surface attachment without adding background audio or picture-in-picture.

Android owns image fetching, origin-byte disk caching, decoding, and in-memory caching; Rust supplies image references, authentication/access rules, and fallback semantics. Do not run the Rust desktop image disk cache in parallel with the Android cache or pass decoded cover pixels over FFI. ADR 0037's shared Rust cache owner remains the desktop implementation; the Library Image Cache's best-effort, original-response, non-offline semantics remain shared.

## Consequences and rejected alternatives

- Accept two presentation stacks to obtain native Android interaction and lifecycle integration; reject waiting for or porting iced's Android runtime.
- Accept separate business FFI and player JNI interfaces rather than forcing all platform calls through Rust. These interfaces carry structured data and commands, never video frames.
- Accept SDK extraction and desktop caller migration as real work, not a thin-binding-only estimate. Avoid duplicate state ownership across Rust, ViewModel, and Media3.
- Keep a single repository, but make clean Android builds independent of the current `target/vendor/iced` materialization and desktop MPV preparation. Repository co-location must not imply desktop build prerequisites.
- Ship a controlled, signed `arm64-v8a` APK first. Dependency pinning, reproducible native builds, licenses, symbols, and applicable 16 KB compatibility remain required even without a store release.
- Correct HDR output on validated capable devices is required, not native Dolby Vision output. Correct SDR mapping is allowed elsewhere; unsupported combinations must be explicit. No Android compile, runtime, HDR, or human visual acceptance is claimed by this decision.
