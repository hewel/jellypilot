# Split the iced update god-module into per-surface modules

_Status: Accepted. Applies ADR 0026's page-reducer pattern to the iced frontend as its permanent architecture._

## Context

`src-iced/src/app/update.rs` grew to 6,763 lines (3,846 implementation, 2,917 tests)
routing nine unrelated concerns — login, home, browse, detail, playback, remote Playback
Target sessions, settings, tray, and window/diagnostics — through free functions over one
56-field `pub` State struct. Every surface can mutate every other surface's fields, so any
browse edit carries an app-wide blast radius, understanding Video Home requires reading the
same file as MPV IPC and Quick Connect, and the future control-only mode has no seam at
which to gate the Library Browser.

Three facts make the split cheap now. The per-surface message enums
(`HomeMessage`, `BrowseMessage`, `DetailMessage`, `SettingsMessage`, `PlaybackMessage`,
`RemoteMessage`, `LoginMessage`) are already modular; only state and update logic are
fused. The update.rs clusters already correspond one-to-one with the `view/` modules. And
ADR 0026 validated this exact page-reducer pattern on the GTK frontend — it enabled the
lossless extraction that seeded the shared crates.

## Decision

**Split `update.rs` into per-surface modules.** `src-iced/src/app/` gains
`login.rs`, `home.rs`, `browse.rs`, `detail.rs`, `playback.rs`, `settings.rs`, and
`shell.rs`. Each surface module owns:

- a **slice struct** (`login::Surface`, …) holding exactly the state fields its cluster
  reads and writes, no longer `pub`-visible to other surfaces;
- an `update(&mut Surface, &mut Kernel, message) -> Task<Message>` entry point;
- the unit tests that already cover its cluster, moved with it.

**Shared machinery lives in one `Kernel` struct** (`app/kernel.rs`), passed `&mut` to
every surface update: `auth_store`, `client`, `connection`, `connected_identity`,
`active_profile`, `request_gate`, `diagnostics`, user notifications (`notice`,
`active_toast`, `next_toast_id`), `tray`, and the artwork machinery (`artwork_adapter`,
`artwork_binder`, `artwork_handles`) that ADR 0028's streaming pipeline shares across
surfaces including the player bar. The per-surface pixel maps (`home_artwork`,
`browse_artwork`, `detail_artwork`) live in their slices.

**The remote Playback Target cluster folds into the playback surface.** `remote_*` state
shares `SessionView` with local playback and every remote command terminates in a playback
controller action; a separate module would only widen the interface. The top-level
`update()` reduces to routing: `Message::Home(msg) => home::update(&mut state.home, &mut
state.kernel, msg)`, and so on.

**Migrate surface by surface, green at every commit**: kernel extraction first (mechanical
field moves), then login → settings → home → detail → browse → playback → shell. Each
migration also sinks display-free pure decision functions in its cluster (e.g.
`initial_season`, `adjacent_index`, visible-range math) into `jellypilot-core` with
display-free tests, continuing ADR 0027's verification model. `view/` is unchanged; views
keep reading `State`, which becomes `{ kernel, login, home, browse, detail, playback,
settings, shell }`.

## Consequences

- `State` shrinks from 56 flat `pub` fields to seven slice structs plus the kernel; a
  surface's bugs and tests gain locality in one module, and the interface each surface
  exposes is its message enum plus its slice — nothing else.
- The split is the enabling seam for a future control-only mode: gating the Library
  Browser becomes withholding the home/browse/detail slices and their subscriptions, not
  editing a god-module.
- Tests move but are not rewritten; behavior is unchanged commit by commit, verified by
  the existing suite staying green plus the native smoke gate.
- The kernel is deliberately small and boring. Anything only one surface touches belongs
  in that surface's slice; anything a second surface needs earns its kernel place — one
  adapter means a hypothetical seam, two mean a real one.
