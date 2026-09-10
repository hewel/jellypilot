<div align="center">

<img src="assets/promo/brand-light.webp" alt="JellyPilot — a native Jellyfin & Emby desktop client with embedded MPV playback." width="100%" />

# JellyPilot

[![CI](https://github.com/hewel/jellypilot/actions/workflows/ci.yml/badge.svg)](https://github.com/hewel/jellypilot/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-1.98+-orange?logo=rust)](https://www.rust-lang.org/)
[![iced](https://img.shields.io/badge/iced-pinned_fork-blue)](https://iced.rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

**English** · [简体中文](README_zh.md)

**A native desktop client for Jellyfin & Emby — real MPV playback with no Electron, no WebView, and no forced transcoding.**

Custom-drawn in safe Rust with [iced](https://iced.rs/): lightweight, cross-platform, and responsive. Linux plays through the pinned embedded MPV fork; Windows and macOS drive your own MPV. Direct Play from your server, every time.

</div>

---

## 🎥 Demo

<a href="assets/promo/demo.webm">
  <img src="assets/promo/demo-poster.webp" alt="JellyPilot demo — a 65-second tour of library browsing, series details, and embedded MPV playback with floating controls on Linux." width="100%" />
</a>

<p align="center"><sub>▶ Watch the 65-second demo: launch → browse → series detail → embedded MPV playback with floating controls.</sub></p>

## 📖 Overview

**JellyPilot** is a native desktop client for [Jellyfin](https://jellyfin.org/) and [Emby](https://emby.media/): a complete library browser, player, and cast receiver — not a wrapper around the web interface. Native UI and real MPV playback, with no browser in between.

- **🖥️ A full desktop client, not a web wrapper**: Browse movies and series, curate Favorites, Watchlist, and Watch History, and switch between multiple servers and accounts — all in a fast, custom-drawn native interface.
- **🎬 Embedded MPV playback (Linux default)**: The pinned, host-enabled mpv fork renders right inside the player window — 10-bit Vulkan surface, direct VAAPI hardware decoding, and Dolby Vision Profile 5 SDR tone-mapping — with floating controls and zero interference from your system MPV configuration.
- **🚀 External MPV playback (Windows & macOS default, optional on Linux)**: Drives a standalone MPV process over JSON IPC. Your personal `mpv.conf`, custom GLSL shaders, input scripts, and profiles remain fully in charge.
- **📺 Cast receiver & remote control**: Automatically discovered on your local network as a native cast target by Jellyfin clients. Transport commands, playback progress, and track changes sync bidirectionally across the main window, player bar, and system tray.

## 🖼️ Screenshots

### Full library and playback

<a href="assets/screenshots/readme-home.webp">
  <img src="assets/screenshots/readme-home.webp" alt="JellyPilot dark theme home screen with Continue Watching, Next Up, and the playback control bar." width="100%" />
</a>

<p align="center"><sub>Dark theme · Home and playback controls</sub></p>

### Library browsing

<a href="assets/screenshots/readme-library.webp">
  <img src="assets/screenshots/readme-library.webp" alt="JellyPilot dark theme series library with filters, a poster grid, and playback controls." width="100%" />
</a>

<p align="center"><sub>Dark theme · Series library and filters</sub></p>

### Embedded player

<a href="assets/screenshots/readme-player.webp">
  <img src="assets/screenshots/readme-player.webp" alt="JellyPilot embedded MPV player with on-screen transport controls, timeline, and audio and subtitle track selection." width="100%" />
</a>

<p align="center"><sub>Embedded MPV playback · On-screen controls and track selection</sub></p>

## ✨ Features

| Feature | Description |
| :--- | :--- |
| 🎞️ **Multi-Server & Account Management** | Connect to multiple Jellyfin and Emby servers with saved profiles, OS keychain credential storage, Quick Connect, and instant account switching |
| 📚 **Rich Browsing & Personal Lists** | Movies and series libraries with live filters, virtualized poster grids, and cached artwork; account-scoped Watchlist, Favorites, and detailed Watch History |
| 🎬 **Dual-Mode MPV Playback Engine** | Embedded MPV by default on Linux (10-bit Vulkan surface, direct VAAPI hardware decoding, Dolby Vision Profile 5 SDR tone-mapping); External MPV on Windows/macOS over JSON IPC (your configs, shaders, and scripts stay in charge) |
| ⏭️ **Smart Playback & Binge-Watching** | In-player episode drawer, automatic/manual native intro skipping, per-season volume memory, automatic next-episode progression, responsive floating controls, and customizable shortcuts |
| 💬 **Audio Tracks & External Subtitles** | Server-hosted external subtitles, embedded audio/subtitle stream switcher, and configurable preferred subtitle language order |
| 📺 **Cast Receiver & Remote Sync** | Discovered as a native cast device in Jellyfin; bidirectional playback command mirroring, progress reporting, and full system tray background integration |
| 🎨 **Thoughtfully Crafted Native UI** | Light and Dark themes, title logo hero headers, native corner smoothing, and backdrop blur; instant English & Simplified Chinese switching, plus a compact Control-Only mode |
| 🍏 **Pure Native Architecture & Diagnostics** | 100% safe Rust and iced with zero WebViews or Electron overhead; built-in diagnostic event viewer, MPV player log capture, and one-click support bundle export |

## 🧩 Server Support

| Server       | Supported | Capabilities                                                                                                                                   |
| :----------- | :-------- | :--------------------------------------------------------------------------------------------------------------------------------------------- |
| **Jellyfin** | ✅        | Password login, Quick Connect, saved profiles, multi-account switching, library browsing, personal lists (Favorites, Watchlist, Watch History), user data actions, embedded & external MPV playback, cast target registration, remote control, and native Intro Skipper |
| **Emby**     | ✅        | Password login, saved profiles, multi-account switching, library browsing, personal lists (Favorites, Watchlist, Watch History), user data actions, embedded & external MPV playback, remote control, and playback progress reporting |

Emby support uses the same library, personal lists, and player workflow as Jellyfin where the server APIs are compatible. Jellyfin-specific features such as Quick Connect and Intro Skipper are not advertised for Emby connections.

### Jellyfin 12+ Compatibility
Jellyfin 12.0 does not require legacy authorization to be enabled: JellyPilot uses the standard `Authorization` header for API requests and `ApiKey` query parameters for playback, subtitles, and remote sessions. Modern `ApiKey` values are redacted in exported diagnostics.
Enter the server's actual base URL, including any configured reverse-proxy base path; Jellyfin 12 removed the automatic `/emby` and `/mediabrowser` route aliases.

### Native Media Segments & Intro Skipper
Intro Skipper reads Jellyfin's native media segments, including ranges published by the Intro Skipper plugin or server-side scheduled tasks. Automatic, Manual, and Off modes apply to intro/credit ranges independently. The deprecated plugin endpoint is no longer used. If no native segment ranges are available, playback continues without skipping; ensure the server's segment extraction or plugin synchronization has populated them.

## 🗺️ Roadmap

- [ ] **MPRIS support** — Linux desktop media-player integration for keys and widgets

## 🚀 Quick Start

### Runtime Prerequisites

- **Embedded Playback (Linux default)**: Requires the pinned host-enabled `libmpv` build and baseline configuration (provided by packaged installs or built via `bun run task mpv build`), along with a Vulkan driver supporting `Rgb10a2Unorm` presentation surfaces. Missing capability triggers an explicit error rather than falling back to an unverified configuration or external player.
- **External Playback (Windows & macOS default, optional on Linux)**: Requires [MPV](https://mpv.io/) with Lua scripting support, discoverable on `PATH` or configured explicitly in **Settings → Playback**. A bundled Lua hook captures volume and temporary mute states before MPV resets options at the end of playback.

### Installation

#### Linux

All packaged Linux distributions install the pinned mpv fork (`/usr/lib/jellypilot/libmpv.so` and `/usr/share/jellypilot/mpv-baseline.conf`) and start **Embedded MPV Playback** by default. External MPV remains an option in Settings.

##### Arch Linux

Install the prebuilt package from the AUR:

```bash
paru -S jellypilot-bin
```

Or build from source via the AUR:

```bash
paru -S jellypilot
```

`yay` and other AUR helpers work the same way. The two packages conflict; pick one.

##### Debian / Ubuntu (.deb)

Download the latest `jellypilot_*_amd64.deb` from [GitHub Releases](https://github.com/hewel/jellypilot/releases), then install it:

```bash
sudo apt install ./jellypilot_*_amd64.deb
```

##### Universal Linux (AppImage)

Download the standalone `jellypilot-*-x86_64.AppImage` from [GitHub Releases](https://github.com/hewel/jellypilot/releases), make it executable, and run:

```bash
chmod +x jellypilot-*-x86_64.AppImage
./jellypilot-*-x86_64.AppImage
```

#### Windows

Download `jellypilot-*-setup.exe` from [GitHub Releases](https://github.com/hewel/jellypilot/releases) and run the installer.

> [!NOTE]
> Windows uses **External MPV Playback** by default. Install [MPV](https://mpv.io/) and ensure it is available on `PATH`, or specify its executable path in **Settings → Playback**.

#### macOS

Download `jellypilot-*.dmg` from [GitHub Releases](https://github.com/hewel/jellypilot/releases), open the disk image, and drag JellyPilot to Applications.

> [!NOTE]
> macOS uses **External MPV Playback** by default. Install [MPV](https://mpv.io/) (e.g. `brew install mpv`) and ensure it is available on `PATH`, or specify its executable path in **Settings → Playback**.

#### Build from Source

<details>
<summary>Development prerequisites</summary>

- [Rust](https://rustup.rs/) 1.98 or newer
- [Bun](https://bun.sh/) 1.3.14 or newer (task dispatcher only — there is no JavaScript frontend)
- Linux GUI: GTK 3, `libxkbcommon`, and Wayland development packages (`libgtk-3-dev`, `libxkbcommon-dev`, `libwayland-dev`, `wayland-protocols`)
- Linux Embedded MPV (optional if using external MPV): Meson >= 1.3, Ninja, C/C++ compiler, pkg-config, Vulkan development headers/loader, FFmpeg dev libraries, `libplacebo` >= 7.360.1, and `libass`.

</details>

```bash
git clone https://github.com/hewel/jellypilot.git
cd jellypilot
bun install --frozen-lockfile

# Optional (Linux only): build and stage the pinned mpv fork for Embedded Playback
bun run task mpv build

# Build the release launcher binary
bun run task iced build --release
```

The release binary is `target/release/jellypilot`.

Maintained Cargo tasks prepare the repository-owned `target/vendor/iced` before building.
`tools/embedded-mpv/iced-source.json` is the authority for the published fork revision and
remote. Preparation fetches that exact commit remotely, verifies HEAD and tracked cleanliness,
and makes all iced crates use the same checkout. There is no checked-in patch or fallback
revision. Existing owned vendor checkouts are preserved when the stable link changes.
An inaccessible commit fails explicitly. An optional local source can provide the same
committed contents without being modified; it is not the remote cold-prepare acceptance path:

```bash
bun run task iced prepare --source /absolute/path/to/iced
```

### Embedded MPV: build, select, and recover

This accepted integration supersedes ADR 0027's external-only playback restriction, not its
native iced/no-webview decision. Linux defaults to Embedded MPV Playback with the pinned
fork, including a one-time migration of existing settings that still recorded External.
Windows and macOS keep External. In **Settings → Playback**, select External if you want
your own MPV process, configuration, shaders, and scripts; restart to apply. Switching
preserves the external executable and arguments. **Show video** opens the player surface
while browsing. The same player, transport, queue, volume, subtitles, and remote-session
controller are reused.

The embedded player uses one full-window video surface with floating controls in both windowed
and fullscreen playback; the video keeps its aspect ratio without cropping.

| Input | Embedded player action |
|---|---|
| Left / Right | Seek −5 / +5 seconds; held keys repeat |
| Up / Down | Volume +5 / −5 percentage points, limited to 0–100%; held keys repeat |
| F | Toggle fullscreen; held-key repeats ignored |
| Esc | Leave fullscreen; an open menu or modal takes priority |
| Space / click video | Toggle pause; Space repeats ignored |
| Back button | Stop playback, then restore the source page and leave fullscreen |

These keys apply only while the embedded player is visible, not while browsing. Captured input,
menus, modals and shortcut capture take priority; the listed unmodified keys are reserved in the
player. Other configured episode/intro bindings remain available. Search is unavailable within
the standalone player; Settings can open over windowed playback.

The cursor reappears on any pointer movement and hides after three idle seconds while playing,
independently of the controls. Moving near the bottom control area (including a 48px approach
band) reveals the bar; movement over the middle of the picture does not reveal or prolong it.
The top-left Back button has its own reveal region and three-second timeout. Pausing,
dragging, open menus and an intro prompt keep controls visible. Keyboard seek and volume
changes show brief feedback. Hovering the timeline previews its time; dragging changes
the target preview and seeks once on release. Seek availability retains the existing positive,
finite-duration check, not a separate backend seekability signal. A failed Stop keeps the player
open so Back can be retried.

Slider keyboard/wheel adjustments submit immediately rather than waiting for a mouse release.
Switching fullscreen during a drag cancels its unfinished preview; releasing afterward does not
submit the cancelled target. Clicking the picture outside an open player menu only dismisses
that menu and does not pause or resume playback.

The bar uses a translucent background without blur. Its transport stays centered in one row
down to 768 logical pixels; at 900 pixels and below, the landscape thumbnail is hidden.

```bash
# Linux prerequisites: Meson >=1.3, Ninja, C/C++ compiler, pkg-config,
# Vulkan development headers/loader, FFmpeg, libplacebo >=7.360.1, libass.
bun run task mpv build --source /absolute/path/to/mpv
bun run task iced run
```

`tools/embedded-mpv/source.json` is the authority for the mpv revision, baseline and Meson
options. The supplied source must be at that revision with no tracked changes. Without
`--source`, the task fetches that revision from the configured fork; an inaccessible
revision is a hard prerequisite failure. No source commit or push is performed.

The build stages `target/embedded-mpv/lib/jellypilot/libmpv.so`,
`target/embedded-mpv/share/jellypilot/mpv-baseline.conf`, and `manifest.json`.
The manifest records source, configuration, tool/dependency versions and artifact hashes.
Source and options are pinned; host libraries/compiler and auto-selected dependencies are
recorded, **not pinned**, so this is not a bit-reproducible or self-contained distribution.
Vulkan headers may be supplied explicitly through `CFLAGS=-I/absolute/sdk/include` (and
dependencies through `PKG_CONFIG_PATH`); neither is silently obtained from another build tree.

Development run/hot commands pass staged asset paths for the Linux Embedded default and for
saved Embedded settings. Absolute `JELLYPILOT_LIBMPV` and `JELLYPILOT_MPV_BASELINE` overrides are
supported; only trusted files implementing the pinned host ABI may be loaded. A directly
launched binary looks beside itself, then in the executable prefix (`../lib/jellypilot` and
`../share/jellypilot`), so `/usr/bin/jellypilot` loads `/usr/lib/jellypilot/libmpv.so`.
Linux packages ship that pinned fork; they do not use system libmpv. Missing Vulkan or
host assets is an error, not a fallback to system libmpv or External MPV. To start External
MPV instead, launch `jellypilot --external` or select External in Settings and restart.

The host retains the actual enabled Vulkan feature chain and shares its device/queue with
the official iced renderer. Video reaches a private 10-bit texture through three ordered
command buffers: tracked `COPY_DST` transition, raw copy, tracked `RESOURCE` transition.
Queue submissions, acquisition/presentation/discard and image/screenshot submits share a
gate; device polling and synchronous mpv calls stay outside it. MPV owns playback time.
The private launcher contains only the two unsafe engine/surface handoffs; Vulkan/libmpv
FFI lives in `jellypilot-mpv-host`, while `src-iced` retains its unsafe-code prohibition.
Direct VAAPI hardware decoding is supported via DMA-BUF import extensions on the shared
Vulkan device when supported by hardware and drivers. This does not claim HDR presentation,
zero-copy playback, or downstream compositor presentation feedback. Non-Linux embedded startup
is explicitly unavailable.
The daemon factory retains the host/device resources across last-window close; reopening
creates a new surface and renderer for the same playback session. Daemon exit terminates
mpv before removing its process-private IPC directory.

### Fork maintenance and joint acceptance

The locked combination is the **JellyPilot commit and its working-tree state**, `Cargo.lock`,
`tools/embedded-mpv/iced-source.json`, and `tools/embedded-mpv/source.json`, together with the
staged `target/embedded-mpv/manifest.json` and the actual library/baseline hashes. The manifests
and lockfile are the source of truth; this document intentionally has no second version table.
The native manifest records host tools and dependencies, not a bit-reproducible environment.
An override library is not attested by the staged manifest: retain its actual hash and origin.

For local direct VAAPI candidates, the product enables supported DMA-BUF import extensions
on the shared Vulkan device. Direct decoding additionally requires the host-enabled mpv
implementation that resolves a DRM render node from that same physical device; extension
support alone does not establish hardware decoding. The RGB10A2 three-slot output, private
texture copy and queue synchronization are unchanged. A dirty fork candidate must be loaded
with the explicit asset overrides above, retaining its HEAD, dirty patch and actual library
hash; the published source pin does not describe those uncommitted changes.

GPU regression reports include `decoderSamples` read from the actual embedded IPC client:
`hwdec-current`, requested `hwdec`, input/output formats and dropped-frame counters at each
settled lifecycle stage. Optional `--hwdec <no|vaapi|vaapi-copy>` on `gpu`/`all` exercises
the host's explicit playback-option override after the baseline. Check the actual decoder
and interop logs to distinguish direct VAAPI from CPU copy-back; the lifecycle pass alone
does not assert a decoder. For diagnostic logging, use a full trusted baseline copy with
`log-file` and `msg-level` settings, retaining its hash, rather than a nested include wrapper.
The runner retains audio-session endpoint lookup while keeping app configuration and IPC
private, respecting existing PipeWire/Pulse routing overrides. For real audio acceptance,
set one strict backend in the full candidate baseline (for example `ao=pipewire`, without
a fallback comma). GPU samples record `current-ao`, `audio-params` and `audio-out-params`,
and reject a mismatch with that explicit backend. Report negotiated channels separately
from physical speakers and human listening acceptance.

For an upstream synchronization, work on a disposable sync branch in each affected fork.
Read the [iced maintenance guide](https://github.com/hewel/iced/blob/main/FORK_MAINTENANCE.md)
and [mpv maintenance guide](https://github.com/hewel/mpv/blob/iced-player/DOCS/fork-maintenance.md).
Review the upstream range and the fork's actual ABI/source changes; do not invent an ABI
version bump when no ABI contract changes. Publish the candidate fork commits through the
normal owner workflow, update the application manifests as one candidate combination, then:

1. In a fresh application checkout, run `bun install --frozen-lockfile` and
   `bun run task iced prepare` **without `--source`**. Preparation must fetch the manifest's
   exact published commit and all iced packages must resolve under `target/vendor/iced`.
2. Build the pinned mpv with `bun run task mpv build`, preserving its generated manifest and
   any explicit SDK/dependency inputs. Run the applicable [project gates](docs/agents/validation.md).
3. Run `bun run task iced regress all --file /absolute/path/to/real-moving-clip.mp4`.
   This is the sole joint-probe entry; [native regression policy](docs/agents/validation.md#opt-in-native-regressions)
   defines prerequisites and evidence. A missing GPU fixture is rejected before preparation,
   build or application startup. Normal run/hot/smoke commands do not enable these probes.
4. Complete the separate [manual color comparison](#manual-three-way-color-comparison).
   Review the exact candidate combination, current-run automatic report, human record and
   explicitly unavailable coverage before accepting a sync. A passing lifecycle probe alone
   is not color, HDR presentation, hardware decoding or Dolby Vision acceptance.

The default local artifact directory is `target/native-regression/`: `report.json` aggregates
only the requested scenarios; `tray.json`, `external.json` and `gpu.json` carry individual
results. Every automatic report has a fresh `runId`; every scenario is `pass`, `fail` or
`unavailable`. Check both the command exit status and that identity, not an old file's presence.
Unrequested scenario files can belong to earlier runs. Preserve reports outside this directory
before the next invocation if needed. These ignored local artifacts are not published docs.

Rollback means restoring the **last accepted combination**, not mixing one old library with
new host bindings. Preserve the pre-sync application revision, source manifests/lockfile,
baseline, native build manifest and binary hashes before changing pins. In a separate checkout
of that accepted application revision, prepare its iced pin and rebuild/stage its mpv pin
with the recorded inputs, or restore its verified archived artifacts. Keep the candidate tree
and local work intact. Start with `jellypilot --external` if embedded prerequisites are absent,
then rerun the applicable gates and joint acceptance before calling embedded recovery complete.

### Manual three-way color comparison

This is a **human** protocol, separate from the automatic GPU lifecycle probe. Keep
`target/native-regression/color-comparison.json` as a human-authored record; the regression
command never writes it, captures reference images, changes display settings, or updates a
reference to make a candidate pass. Retain the old accepted native binary and baseline before
the sync; if they are unavailable, mark that comparison `unavailable`, not equivalent.

Compare **old accepted native mpv / candidate native mpv / candidate embedded JellyPilot**.
The native executables must come from the respective recorded fork revisions, not an
unidentified system `mpv`. The application mpv build stages libmpv only; obtain native comparison
binaries from the corresponding fork build workflow and record their source/binary hashes.
Use the same explicitly loaded, hash-recorded baseline (`--no-config --include=<baseline>`
for native mpv) and record the embedded host backend differences; do not silently change color,
tone-mapping, scaling or hardware-decoding options between columns.

For each of **SDR**, **HDR10**, and **Dolby Vision Profile 5**:

- Use a real authorized local fixture. Record its hash and ffprobe stream metadata: codec,
  dimensions, pixel format, color primaries/transfer/matrix/range, and HDR/DOVI side data.
  HDR10 requires actual PQ/BT.2020/HDR metadata evidence. Profile 5 requires an actual DOVI
  configuration record identifying `dv_profile=5`; a filename, HEVC 10-bit stream, BT.2020 tag,
  synthetic pattern or another DV profile is not proof. Missing evidence or fixture is
  `unavailable`; do not synthesize a substitute and claim Profile 5 coverage.
- Fix and record the same timestamp/frame, physical video-picture dimensions (not logical
  window size), monitor, compositor session, display mode/HDR state, scaling and baseline
  hash for all three columns. Pause at the agreed timestamp, wait for the decoded frame,
  and compare the same picture region with overlays removed equally.
- The human checks hue/skin tones, neutral grays, shadow detail, highlights/clipping,
  saturation and gradients. Record what was actually observed and the compared column pair,
  with `pass`, `fail` or `unavailable`, reviewer and timestamp. Do not promote screenshot
  byte differences or an automated decode success to a visual conclusion.
- Identify the run and candidate/accepted artifacts in the human record and link any
  intentionally captured human evidence. Automatic `report.json` continues to describe only
  automated coverage. Approval or replacement of an old reference is a separate explicit
  human decision; retain the old record rather than overwriting it during a sync.

### Usage

1. **Launch JellyPilot** from your application menu or terminal.
2. **Choose a server type**: Jellyfin or Emby on the login screen.
3. **Authenticate** with your Server URL and credentials; Jellyfin also supports Quick Connect.
4. **Manage accounts**: Add multiple server accounts and switch between them anytime via the sidebar profile menu.
5. **Browse and curate**: Browse movies and series with live filters, explore Personal Lists (Watchlist, Favorites, Watch History), and view rich details.
6. **Play or cast**: Start playback directly in JellyPilot (Embedded MPV on Linux, External MPV on Windows/macOS), or cast to "JellyPilot" from another Jellyfin client.
7. **Control playback**: Use responsive on-screen player controls, keyboard shortcuts, the player bar, system tray, or remote media server sessions. Open the episode queue to jump anywhere in the current season.
8. **Switch app modes**: Toggle between Full library mode and Control-Only mode (a compact standalone controller window) in Settings.

**Season volume memory** is enabled by default under **Settings → Playback**. Player volume
adjustments are remembered per season on this device, separately for each server and account.
The next episode, a manually selected episode, or a later session restores that season's volume
before playback starts. Movies and episodes without a saved, reliably identified season use
MPV's startup volume instead. This remembers the player's volume, not system volume, and does
not normalize audio loudness.

Turning the switch off stops saving and restoring without deleting existing records or changing
the current volume. Turning it back on restores saved values on the next load. Temporary mute
is retained during continuous episode playback, but is not saved for a later playback session.

**Diagnostics and player logs** under **Settings → Diagnostics** provide retained event logging
with category and level filtering. Enabling **Capture player logs** records raw embedded or external
MPV log entries across playback events. You can export complete support logs with a single click.

## 🏗️ Architecture

One Rust workspace: `jellypilot-ui` owns the custom iced presentation layer, while the domain and infrastructure crates remain display-free and test-covered.

```mermaid
flowchart LR
    subgraph JellyPilot[JellyPilot — iced application]
        App[src-iced shell and screens]
        UI[jellypilot-ui design system]
        Core[jellypilot-core browse/config/diagnostics]
        Session[jellypilot-session WebSocket remote sessions]
        MS[jellypilot-media-server HTTP + artwork]
        Auth[jellypilot-auth login + keyring]
        Mpv[jellypilot-mpv process lifecycle + IPC]
        App --> UI
        App --> Core
        App --> Session
        App --> MS
        App --> Auth
        App --> Mpv
    end

    Session <-->|WebSocket| Server[Jellyfin / Emby server]
    MS <-->|REST| Server
    Server -->|original/direct source| MPV[External MPV process]
    Mpv <-->|JSON IPC| MPV
    Mpv <-->|JSON IPC, Linux default| Host[Embedded MPV host]
    Server -->|original/direct source| Host
    Host -->|10-bit Vulkan copy and sample| App
```

- `src-iced` — the application: shell, screens, tray, subscriptions, orchestration, embedded compositor, and player primitives.
- `crates/jellypilot-launcher` — executable-only entry point and private unsafe engine/surface handoffs.
- `crates/jellypilot-mpv-host` — Linux host ABI, retained Vulkan device/features, DMA-BUF import extensions for VAAPI, bounded producer images, and GPU copy lifecycle.
- `crates/jellypilot-ui` — the design system: tokens, theme/Catalog styles, custom widgets, overlay, and Reicon iconography.
- `crates/jellypilot-core` — display-free browse model, personal lists, configuration, request gate, diagnostics, player logs, and artwork load planning.
- `crates/jellypilot-media-server` — Jellyfin/Emby HTTP adapter over the generated OpenAPI clients in `crates/media-server-api/`.
- `crates/media-server-api` — generated OpenAPI client bindings for Jellyfin and Emby APIs.
- `crates/jellypilot-auth` — login workflows, session persistence, and OS keychain token storage.
- `crates/jellypilot-mpv` — MPV process lifecycle, JSON IPC protocol, and player log capture.
- `crates/jellypilot-session` — media-server WebSocket remote-control sessions.

## 💻 Development

### Commands

| Task                        | Command                                      |
| :-------------------------- | :------------------------------------------- |
| **Run the app**             | `bun run task iced run`                      |
| **Startup smoke gate**      | `xvfb-run -a bun run task iced run --smoke`  |
| **Build release binary**    | `bun run task iced build --release`          |
| **Build embedded MPV**      | `bun run task mpv build`                     |
| **Native regression probes**| `bun run task iced regress all --file <clip>`|
| **Check everything**        | `bun run check`                              |
| **Format code**             | `bun run task rust fmt && bun run task fmt`  |
| **Rust tests**              | `bun run task rust test`                     |
| **Rust clippy**             | `bun run task rust clippy`                   |
| **Regenerate API clients**  | `bun run task api`                           |

### Conventions

- **Rust**: formatting is enforced by `bun run task rust fmt`; `unsafe_code` is forbidden workspace-wide; clippy warnings are errors.
- **Display-free logic** lives in `jellypilot-core` and is tested there; `src-iced` keeps orchestration and views.
- **Domain language**: [CONTEXT.md](CONTEXT.md) is the glossary; [docs/adr/](docs/adr/) records architecture decisions.
- **Design and promo artwork**: logo sources, screenshots, fonts, and the renderer live in the separate `jellypilot-design` project. From that checkout, run `bun run promo`, then preview publication with `bun run sync:app -- /absolute/path/to/jellypilot`; add `--write` to copy the selected exports. This repository keeps the published files in `assets/promo/` and `assets/screenshots/`, plus the logo used by those assets. `assets/promo/demo.webm` and `demo-poster.webp` are a recorded demo, not design-project exports.

## 📜 Project History

Releases ≤ 1.4.x shipped a Tauri/Solid.js frontend with an embedded web player and a local FFmpeg HLS pipeline. That stack was retired per [ADR 0027](docs/adr/0027-cross-platform-iced-frontend.md). Linux now defaults to the pinned Embedded MPV fork ([ADR 0040](docs/adr/0040-linux-embedded-mpv-default.md)); settings and saved profiles start fresh — no Tauri Store data is imported.

## 🙏 Credits

- [MPV](https://mpv.io/) — the best media player in existence.
- [iced](https://iced.rs/) — the cross-platform GUI library this app is drawn with.
- [Jellyfin](https://jellyfin.org/) and [Emby](https://emby.media/) — the media servers JellyPilot is built for.
