# Add a parallel native GTK frontend for Linux

_Status: Accepted. Extends ADR 0018 while retaining the Tauri application during migration. The frontend endgame and the Rust 1.85 floor are superseded by ADR 0027._

## Context

JellyPilot's production desktop application uses a Solid interface in a Tauri WebView. Replacing
that interface in one rewrite would couple framework replacement to authentication, persistence,
Library Browser transport, Playback Target behavior, external MPV process control, packaging, and
release validation.

ADR 0018 already places Library Browser paging policy in the Tauri-free `jellypilot-core` crate and
explicitly permits a future Rust-native interface to link that crate without its WASM adapter. The
remaining media-server and application adapters are still owned by `src-tauri`; pretending they
are reusable before a second real consumer exists would produce speculative interfaces.

Relm4 0.10 and 0.11 require a newer Rust compiler than JellyPilot's declared Rust 1.85 baseline.
Relm4 0.9.1 supports that baseline and uses GTK 4, so it provides the component and message model
without raising the workspace minimum Rust version.

## Decision

Add `src-gtk` as a parallel Linux frontend package named `jellypilot-gtk`. It uses Relm4 0.9.1 on
GTK 4 and links `jellypilot-core` directly. The production Tauri frontend remains intact until the
native path reaches explicit feature, runtime, packaging, and release parity.

Keep GTK widgets and Relm4 messages in the shell adapter. Keep the native Library Browser adapter
free of GTK so it can be tested without a display server. That adapter owns item payloads, executes
the portable reducer's ordered commands, and correlates page settlements with reducer tokens.
Media-server I/O remains an environment adapter and will be extracted from Tauri only when a live
GTK vertical slice consumes it.

Let GTK own the native shell's visual language. The application inherits the active GTK theme's
palette, typography, focus treatment, controls, and motion instead of reproducing the Tauri
frontend's web-oriented surfaces. Custom GTK CSS is limited to layout details that widget
properties and standard GTK style classes cannot express.

The first walking slice provides a native window, persistent Sidebar destinations for Video Home,
Now Playing, and Settings, and an honest status for each unavailable adapter. It must not render
fake media, claim a connection, or claim playback state. Its direct reducer integration is covered
by deterministic model tests; GTK startup requires a separate Linux-native smoke gate because the
existing native E2E harness is Tauri-specific.

The GTK package is Linux-only product work. Its GTK and Relm4 dependencies and implementation are
target-gated so cross-platform workspace checks compile only an explanatory stub and never require
GTK. Linux workspace gates compile the real application, and Linux CI installs GTK 4 development
headers.

## Consequences

- Tauri and GTK can advance independently while sharing only proven framework-free Rust policy.
- A GTK regression cannot silently change the production Tauri interface during early migration.
- The Rust 1.85 workspace contract is preserved, at the cost of starting on Relm4 0.9.1 rather
  than the latest Relm4 API.
- Authentication, Saved Service Profiles, live Library Browser transport, external MPV JSON IPC,
  and packaging remain explicit later migration gates.
- The GTK shell can compile and its model can be tested without a live Jellyfin or Emby server;
  runtime acceptance still needs a Linux display-server smoke test.
