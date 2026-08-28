# Replace the Tauri and GTK frontends with a cross-platform iced application

_Status: Accepted. Supersedes the frontend endgame of ADR 0021 and ADR 0024; schedules ADR 0019 for retirement._

## Context

ADR 0021 added a parallel GTK frontend for Linux; ADR 0024 rebuilt it as a GNOME-native
libadwaita interface, and that rebuild has now landed (eight slices, page reducer modules
per ADR 0026). User acceptance of the rebuilt result: not ideal. A structured decision
review identified the root requirements as (a) one native frontend that also reaches
Windows and macOS — the GTK package is Linux-only by design — and (b) a distinctive,
fully custom visual language, which ADR 0021's premise ("let the native toolkit own the
visual language") forbids. With the GNOME-integration premise abandoned, maintaining two
frontend stacks (Tauri webview + GTK) has no remaining justification over one custom-drawn
stack.

Framework facts verified for this decision: iced 0.14.0 (released 2025-12-07) requires
Rust 1.88; the workspace floor is 1.85. iced 0.14 ships working IME, animation APIs,
reactive rendering, system dark/light detection, and Catalog-based custom theming. It has
no built-in list/grid virtualization, foundational-but-partial AccessKit accessibility,
sctk-adwaita client-side decorations on GNOME Wayland, and no turnkey libmpv embedding
(the ecosystem video crate is GStreamer-based, not mpv-compatible). Release cadence is
6–14 months with high API churn between releases.

Decisions taken in the review: switch directly without a prototype; freeze the Tauri
frontend until acceptance gates; extract the GTK package's display-free logic and then
delete it; drop the embedded playback chain; port the Panda design tokens as the v0 visual
baseline and run a dedicated design pass after feature completion; build a component
library first, seeded from the local `cottid` iced project; adopt dirs+keyring persistence
with no Tauri Store migration; keep tray parity; and gate the cutover in two stages.

## Decision

**Adopt iced 0.14 as the single future frontend, replacing both GTK and Tauri.** Pin the
crates.io `iced` 0.14 release (features `advanced`, `svg`, `tokio`; optional `hot` reload
behind a debug feature); do not track git master. Raise the workspace Rust floor from
1.85 to 1.88, amending the MSRV contracts recorded in ADR 0021 and ADR 0024. The new
application lives in `src-iced` (package `jellypilot-iced`), developed Linux-first, and
becomes the primary application when the Tauri package retires.

**Own the visual language.** The application is fully custom-drawn; no native-widget or
system-theme inheritance is a goal. The v0 baseline ports the Tauri frontend's Panda
semantic tokens into an iced `Theme` (Catalog traits per widget), keeping brand
continuity; Inter/Space Grotesk and Lucide SVG icons are bundled assets. A dedicated
design pass after feature completion removes residual generic feel and is the visual
acceptance gate. System window decorations are kept for now, matching current Tauri
behavior; a custom titlebar may be reconsidered at that design pass. The design is
dark-first, matching the current product.

**Freeze the Tauri frontend.** `src-tauri` and `src/` receive bugfixes only — no new
features — until the stage-2 gate retires them. The embedded playback chain
(ADR 0019) is end-of-life: Embedded Web Playback, Local Transcode, Explicit MPV
Fallback, and Playback Engine Preference/Override are not ported. The iced application
always presents External MPV Playback, as GTK already did per ADR 0024.

**Extract the GTK package's display-free logic, then delete `src-gtk`.** The browse
model, playback session state machine, request gate, auth-storage policy, and artwork
cache policy — already display-free and test-covered per ADR 0026 — move into shared
crates; the Relm4 widget layer is discarded. `src-gtk` is deleted as soon as the
extraction lands; it never reaches production, and the remaining GTK migration gates in
ADR 0022 and ADR 0024 are void. With the MSRV floor unblocked, keyring moves from the
pinned 3.6.3 to the 4.x line.

**Build the component library first as `crates/jellypilot-ui`.** Seeded from the local
`cottid` project's iced 0.14 patterns (design tokens, theme, custom widgets,
popover/tooltip overlay positioning), the crate owns the design system and the hard
widgets before application pages exist: buttons, badges, form fields with IME, custom
scrollables, dialogs, and a viewport-sliced virtualized artwork grid — iced has no
built-in virtualization, and library poster grids are the load-bearing case.

**Persist configuration and secrets without Tauri Store.** Application configuration
(including per-series track preferences and library filter state, currently in Tauri's
`preferences.json`) lives in a TOML file under the platform config directory; access
tokens live in the OS keychain via keyring, continuing the ADR 0023 approach. No
migration from Tauri Store: settings and saved profiles start fresh.

**Keep tray parity.** The system tray menu (transport controls, show window, quit),
close-to-tray, and start-minimized behavior are reproduced with the standalone
`tray-icon` crate.

**Gate the cutover in two stages.** Stage 1 (daily-driver switch): Linux feature parity
with the ADR 0024 list minus the embedded chain, plus tray, persistence, and the native
startup smoke gate; the user switches daily usage then. Stage 2 (production release):
Windows/macOS/Linux CI builds, installers via cargo-packager or cargo-dist, and the test
gates below. Windows and macOS compile gates run in CI from the first iced slice as an
engineering default, not as a release gate. ADR 0021's honesty rules (no fake media, no
claimed connection, no claimed playback state) apply unchanged.

**Land the migration in eleven slices.** (1) This ADR, the MSRV bump, and workspace
scaffolding (`crates/jellypilot-ui`, `src-iced` skeleton). (2) Extract display-free GTK
modules into crates; delete `src-gtk`. (3) `jellypilot-ui` foundations: tokens/theme,
buttons, fields, scrollables, badges, overlay popovers and tooltips. (4) Application
shell, configuration, keyring auth storage, and the login surface (Quick Connect,
Password Login, Saved Service Profiles). (5) Video Home and browse with the virtualized
artwork grid and the artwork loading/cache pipeline. (6) Detail richness and User Data
Actions. (7) External MPV playback, the player bar, Now Playing, and the tray. (8)
Playback Target remote sessions and Intro Skipper on `jellypilot-session`. (9) Settings
and Diagnostics; this completes the stage-1 gate. (10) The design pass. (11) Windows and
macOS bring-up, packaging, and the stage-2 gate; Tauri retirement follows.

## Consequences

- The workspace Rust floor rises to 1.88; every crate, CI job, and ADR that records the
  1.85 contract (0021, 0024) is amended.
- The GTK frontend never ships. ADR 0026's page-reducer split pays off as extraction
  seams rather than as a production architecture; the GTK visual rebuild is written off.
- iced 0.14's AccessKit integration is foundational and partial: accessibility regresses
  relative to libadwaita. Accepted as the cost of a fully custom-drawn interface.
- Poster-grid virtualization is hand-built in `jellypilot-ui`; its performance bounds
  library views and must be stress-tested with large libraries.
- Each future iced upgrade is a planned refactor, not a routine dependency bump; upgrade
  cost is budgeted per release given the 6–14 month cadence and high API churn.
- The WebdriverIO E2E harness is retired with Tauri (iced exposes no DOM). Behavioral
  verification rests on display-free reducer/model tests plus the Linux native smoke
  gate until richer iced UI testing matures.
- When the Tauri package retires, the embedded chain and its webview-only infrastructure
  are deleted with it: `src-tauri/src/embedded_player/`, `src-tauri/src/hls_proxy/`,
  `src-tauri/src/image_proxy.rs`, `crates/jellypilot-playback-core`, the
  `EmbeddedPlayer` web components, the `jellypilot-core-wasm` bridge, and the
  ffmpeg/ffprobe sidecars — roughly 9,000 LOC of webview-era infrastructure, after
  which packaging no longer carries sidecar binaries.
- Existing users re-enter settings and re-authenticate on the iced build; no Tauri Store
  data is imported.
- `cottid` is a pattern and code reference only, not a dependency; what is ported is
  adapted to JellyPilot's design tokens and component needs.
