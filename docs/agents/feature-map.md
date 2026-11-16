# Feature Map & Lifecycle Statecharts

User-facing surface index mapping JellyPilot product areas and [Glossary](../../GLOSSARY.md) terms directly to view, orchestration, and display-free domain modules.

## Surface-to-Module Map

| User Surface / Flow | Canonical Glossary Terms | Desktop View (`src-iced/src/app/`) | Desktop Orchestration (`src-iced/src/`) | Core & Domain (`crates/`) |
|---|---|---|---|---|
| **Login & Accounts** | `Server URL`, `Quick Connect`, `Password Login`, `Saved Service Profile`, `Profile Switch`, `Account Popover`, `Disconnect`, `Sign Out` | `view/login.rs`, `view/account.rs`, `view/modal_dismiss.rs` | `app/login.rs`, `app/accounts.rs`, `app/avatars.rs` | `jellypilot-auth/`, `jellypilot-sdk/src/{account,quick_connect}.rs`, `jellypilot-core/src/request_gate.rs` |
| **Desktop Shell & Modes** | `App Mode`, `Control-Only Mode`, `Sidebar`, `Settings Modal`, `Background Residency`, `Quit Application`, `UI Language` | `view/mod.rs`, `view/shell.rs`, `view/settings.rs`, `view/undo_notice.rs` | `lib.rs`, `runner.rs`, `tray.rs`, `instance.rs`, `i18n.rs`, `app/shell.rs`, `app/settings.rs`, `app/kernel.rs` | `jellypilot-core/src/{config,settings,locale,rendering,window_geometry,undo_notices}.rs`, `jellypilot-ui/` |
| **Video Home & Hero** | `Video Home`, `Featured Item`, `Hero Selection Rail`, `Continue Watching`, `Next Up`, `Title Logo`, `Episode Still` | `view/home.rs`, `view/motion.rs` | `app/home.rs`, `app/motion.rs` | `jellypilot-core/src/{home_hero,cards}.rs`, `jellypilot-media-server/src/home.rs` |
| **Library, Detail & Lists** | `Library Browser`, `Saved Filters`, `Personal Lists`, `Watchlist`, `Favorites`, `Watch History`, `Watch History Removal`, `User Data Action`, `Media Specifications` | `view/browse.rs`, `view/detail.rs`, `view/detail/{overview,chrome}.rs`, `view/personal_lists.rs`, `view/saved_browse.rs`, `view/scroll_memory.rs` | `app/browse.rs`, `app/detail.rs`, `app/personal_lists.rs`, `app/saved_browse.rs`, `app/collections.rs`, `app/item_actions.rs`, `app/undo.rs` | `jellypilot-core/src/{browse,browse_model,browse_window,reducer,model,detail,history,watchlist,saved_browse,collections,item_actions}.rs`, `jellypilot-sdk/src/{browse,history,watchlist,saved_browse,item_actions}.rs` |
| **Artwork Pipeline** | `Library Image`, `Library Image Cache`, `Library Image Raster` | `view/image_observer.rs`, `view/image_transition.rs` | `app/artwork.rs` | `jellypilot-core/src/image_lifecycle.rs`, `jellypilot-media-server/src/{artwork,artwork_cache,image_ref}.rs`, `jellypilot-sdk/src/image.rs` |
| **Playback, Tools & PiP** | `Playback Session`, `Embedded MPV Playback`, `External MPV Playback`, `Now Playing`, `Direct Playback`, `Viewing Queue`, `Intro Skipper`, `Seek Preview` | `view/player.rs`, `view/player_info.rs`, `view/playback_tools.rs`, `view/viewing_queue.rs`, `view/pip.rs` | `app/playback.rs`, `app/playback/{tools,viewing_queue}.rs`, `app/list_playback.rs`, `app/embedded_player.rs`, `app/embedded_player/seek_preview.rs`, `app/pip.rs`, `app/system_media.rs`, `embedded/`, `preview.rs` | `jellypilot-mpv/`, `jellypilot-mpv-host/`, `jellypilot-core/src/{viewing_queue,intro_skipper,seek_preview,audio_tracks,volume_memory,now_playing_adjustments,danmaku}.rs`, `jellypilot-sdk/src/playback/` |
| **Cast Target & Diagnostics** | `Playback Target`, `Remote Controller`, `Diagnostics` | `view/settings.rs` | `app/playback/remote.rs`, `regression.rs` | `jellypilot-session/`, `jellypilot-core/src/{diagnostics,logs,player_logs}.rs`, `jellypilot-sdk/src/{remote,remote_control}.rs`, `jellypilot-launcher/src/diagnostics.rs` |
| **TV Presentation** | `UiMode::Tv` | `tv/{view,account,navigation,search,lists,detail,player,filters,saved_browse,settings}.rs` | `tv/mod.rs`, `tv/input.rs`, `tv/text_entry.rs` | `jellypilot-ui/src/{tv,widgets/tv_focus,widgets/tv_player}.rs`, `jellypilot-core/src/{tv_navigation,tv_player}.rs` |

## Lifecycle Statechart Rule

Before modifying cross-generation or cross-surface state transitions, include a Mermaid `stateDiagram-v2` in the design or plan and review it before writing code. Cover these boundaries:

- `Playback Session` admission, window close/reopen, and lease revocation (`Background Residency`, PiP handoff)
- `Profile Switch` / `Disconnect` / `Sign Out` generation gates (`RequestGate`)
- Remote `Playback Target` resource lifecycle (`src-iced/src/app/playback/remote.rs`)
- `IntroSkipper`, `ViewingQueue`, and `LibraryBrowseCore` state transitions

Each diagram should make explicit:

1. **States and ownership** (what resources or surfaces are live in each state).
2. **Generation / lease tokens** guarding async completions.
3. **Stale-event rejection edges** (showing how out-of-order callbacks or late completions are ignored without resurrecting closed sessions).
