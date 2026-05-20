# JMSR - Next Steps

## High Priority

### v1.0.0 Release Prep ✅
- [x] Version bumped to 1.0.0 across all manifests (package.json, Cargo.toml, tauri.conf.json)
- [x] CI/CD workflows created:
  - `.github/workflows/ci.yml` - Runs on push/PR: Biome lint, Clippy, tests
  - `.github/workflows/release.yml` - Runs on `v*` tags: cross-platform builds, GitHub Release
- [x] MPV spawning fixes (Windows):
  - `ensure_mpv_exe()` swaps `mpv.com` → `mpv.exe` to prevent black console window
  - `canonicalize_path()` resolves Scoop symlinks to fix error 448
  - Added `--border`, `--title-bar`, `--osc` flags for proper window decorations
  - `config_detect_mpv()` strips `\\?\` prefix for clean UI display
- [x] WebSocket auto-reconnect with exponential backoff (1s → 2s → 5s → 10s → 30s → 60s)
  - On disconnect: clears playback context, shows warning toast
  - On reconnect: re-reports capabilities, shows success notification
- [x] Lucide icons integration (`lucide-solid@0.562.0`):
  - `LoginPage.tsx`: `Loader2` spinner
  - `SettingsPage.tsx`: `RefreshCw`, `CheckCircle`, `Play`, `Keyboard`
  - `Toast.tsx`: `Check`, `X`, `AlertTriangle`, `Info`
- [x] Footer versions updated to 1.0.0

### Config Persistence to Disk ✅
- [x] Add `tauri-plugin-store` for file-based config storage
- [x] Load config on app startup (in lib.rs setup)
- [x] Save config when user clicks "Save Settings"
- [x] Store in platform-appropriate location (handled by tauri-plugin-store)

### Apply Config Changes Live ✅
- [x] When device name changes, re-register Jellyfin session capabilities
- [x] When MPV path/args change, update MpvClient state (applies on next spawn)
- [x] Config applied at startup from stored values

### Series-based Track Persistence ✅
**Goal**: Automatically select audio and subtitle tracks for TV series based on the user's last manual selection for that series. (e.g., If user selects "Japanese" audio for S01E01, S01E02 should automatically start with Japanese audio).

**Status**: ✅ Complete (in-memory + disk persistence)

#### Implementation Details
- [x] `TrackPreference` struct in `types.rs` with `audio_language`, `subtitle_language`, `is_subtitle_enabled`
- [x] `find_stream_by_lang()` helper in `types.rs`
- [x] `series_preferences: HashMap<String, TrackPreference>` in `SessionState`
- [x] `current_series_id` and `current_media_streams` tracking in `SessionState`
- [x] Apply preferences in `handle_play()` when playing series episodes
- [x] Save preferences in `handle_general_command()` for `SetAudioStreamIndex` and `SetSubtitleStreamIndex`
- [x] `MpvAction::SetAudioTrack` and `MpvAction::SetSubtitleTrack` variants
- [x] `SessionManager::new()` accepts `AppHandle` for store access
- [x] `load_preferences_from_store()` loads from `preferences.json` on init
- [x] `save_preferences_static()` saves to disk when user changes tracks

### Auto-Play Next Episode ✅
**Goal**: When an episode finishes playing naturally (EOF), automatically start the next episode in the series.

**Status**: ✅ Complete

#### Implementation Details
- [x] Added `EpisodesResponse` struct in `types.rs` for parsing episodes API
- [x] Added `get_next_episode()` method in `client.rs` using `/Shows/{seriesId}/Episodes` endpoint
- [x] Added `current_item: Option<MediaItem>` to `SessionState` to track what's playing
- [x] Added `start_mpv_event_listener()` in `session.rs` that:
  - Spawns background task listening to MPV events
  - Detects `end-file` events with `reason: "eof"` (natural end, not user stop)
  - Reports playback stopped to Jellyfin
  - Fetches and plays next episode automatically
- [x] Track preferences from series-based persistence apply to auto-played episodes

## Medium Priority

### Debug Web UI Pause Issue ✅
- [x] Investigate: "When I click play button on web, MPV stays paused"
- [x] Fixed: `PlayPause` handler now queries actual MPV pause state instead of internal state
- [x] Root cause: Internal state could get stale if user paused via MPV keyboard (spacebar)
- [x] Solution: Query `mpv.get_pause()` directly for `PlayPause` toggle commands

### NextTrack Command Support ✅
- [x] Added handler for `NextTrack` playstate command from Jellyfin web UI
- [x] Reports playback stop for current item, then fetches and plays next episode

### PreviousTrack Command Support ✅
- [x] Added `get_previous_episode()` method to `client.rs`
- [x] Added handler for `PreviousTrack` playstate command from Jellyfin web UI
- [x] Reports playback stop for current item, then fetches and plays previous episode

### MPV Keyboard Shortcuts for Next/Previous Episode ✅
- [x] Added `args` field to `MpvEvent` to parse `client-message` events
- [x] Added `handle_client_message_event()` to process MPV script messages
- [x] Supports `jmsr-next` and `jmsr-prev` commands via MPV keybindings
- [x] Refactored event listener with helper methods for cleaner code
- [x] **Out of the box**: JMSR auto-creates `input.conf` in config directory on first run
  - Windows: `%APPDATA%\jmsr\input.conf`
  - macOS: `~/Library/Application Support/jmsr/input.conf`
  - Linux: `~/.config/jmsr/input.conf`
- [x] Default keybindings:
  - `Shift+n` - Next episode
  - `Shift+p` - Previous episode
- [x] Users can customize the keybindings by editing the generated `input.conf`

### Error Handling Improvements ✅
- [x] Show user-friendly error messages in UI when commands fail
- [x] Handle MPV connection loss gracefully (auto-reconnect or notify user)
- [x] Handle Jellyfin session timeout/disconnect

### Event-Driven Progress Reporting ✅
**Goal**: Replace 5-second polling with MPV property observation for immediate UI sync.

**Implementation Details**:
- [x] Added `observe_property` and `unobserve_property` commands in `protocol.rs`
- [x] Added `observe_property()` method in `MpvClient`
- [x] Added `is_muted` field to `PlaybackSession` in `types.rs`
- [x] Refactored `start_mpv_event_listener()` to:
  - Set up property observations for `pause`, `volume`, `mute`, `time-pos`
  - Handle `property-change` events immediately
  - Report to Jellyfin immediately for pause/volume/mute changes
  - Throttle `time-pos` reporting to every 5 seconds
- [x] Volume sync now bidirectional (MPV ↔ Jellyfin web UI)
- [x] Stop command now properly reports `PlaybackStopInfo` to Jellyfin

## Low Priority

### Code Cleanup ✅
- [x] Remove unused Rust imports (jellyfin/mod.rs, mpv/mod.rs)
- [x] Remove dead code warnings (unused methods in session.rs, client.rs)
- [x] Add `#[allow(dead_code)]` for API response struct fields (may be used later)

### Oracle Review Fixes ✅
- [x] HTTP `get()`/`post()` now check status before JSON parsing
- [x] Token logging fixed - `api_key` redacted from logged URLs via `redact_url()` helper
- [x] Lifecycle bug fixed - `JellyfinWebSocket` now uses `CancellationToken` and `reset_channels()` for proper reconnection
- [x] MPV death handling - `clear_playback_context()` reports stop to Jellyfin and clears state when MPV disconnects

### Testing
- [ ] Add frontend tests for SettingsPage form validation
- [ ] Add Rust unit tests for config validation
- [ ] Add integration tests for MPV IPC

### Future Features
- [x] System tray controls (play/pause, next, previous, mute)
- [ ] [Quick Connect](https://jellyfin.org/docs/general/server/quick-connect/) - Login via code from another device
- [ ] [Intro Skipper](https://github.com/intro-skipper/intro-skipper) integration - Auto-skip intros/credits
- [ ] Full-featured client UI - Browse libraries, manage media like other Jellyfin clients
- [ ] Embedded player - Optional built-in video player without external MPV dependency
- [ ] MPRIS support - Linux media player integration for desktop controls
- [ ] Keyboard shortcuts for playback control
- [ ] Multiple server support
- [ ] Playlist/queue display in UI
- [ ] Transcoding settings UI

## Completed

- [x] Core playback (titles, tracks, pause sync)
- [x] AppConfig struct with validation
- [x] Config Tauri commands (get/set/default/detect)
- [x] Configurable MPV path and extra args
- [x] Configurable device name
- [x] Settings UI with @tanstack/solid-form
- [x] PlayPause toggle command support
- [x] Config persistence to disk (tauri-plugin-store)
- [x] Apply config changes live (MPV path/args, device name)
- [x] Series-based track persistence (audio/subtitle preferences per series)
- [x] Auto-play next episode when current finishes naturally
- [x] System tray controls (play/pause, next, previous, mute)
