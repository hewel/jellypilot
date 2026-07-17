use std::path::PathBuf;
use std::sync::Arc;

mod auth_profiles;
mod command;
mod config;
mod image_cache;
mod image_ref;
mod jellyfin;
mod mpv;
mod now_playing;
mod playback_control;
mod tray;

use command::{ConfigState, JellyfinState, MpvState};
pub use config::AppConfig;
use image_cache::{ImageCache, ImageCacheState};
use jellyfin::JellyfinClient;
use mpv::MpvClient;
use parking_lot::RwLock;
use tauri::{Manager, WindowEvent};
use tauri_plugin_log::{Target, TargetKind};

#[cfg(all(feature = "webdriver", not(debug_assertions)))]
compile_error!("JELLYPILOT_WEBDRIVER_REQUIRES_DEBUG_ASSERTIONS");

fn logging_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
  tauri_plugin_log::Builder::default()
    .level(log::LevelFilter::Info)
    .targets([
      Target::new(TargetKind::Stdout),
      Target::new(TargetKind::Webview),
    ])
    .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  let builder = command::specta_builder();

  // Create config state with defaults (will be updated in setup after store is available)
  let config = Arc::new(RwLock::new(AppConfig::default()));
  let config_state = ConfigState(config.clone());
  let config_for_setup = config.clone();
  let image_cache_state = ImageCacheState::empty();
  let image_cache_for_setup = image_cache_state.0.clone();
  let image_cache_for_protocol = image_cache_state.clone();

  // Create MPV client state
  let mpv_client = Arc::new(MpvClient::new(None));
  let mpv_state = MpvState(mpv_client.clone());
  let mpv_for_setup = mpv_client.clone();

  // Create Jellyfin client state
  let jellyfin_client = Arc::new(JellyfinClient::new());
  let jellyfin_for_setup = jellyfin_client.clone();
  let jellyfin_for_protocol = jellyfin_client.clone();
  let jellyfin_state = JellyfinState::new(jellyfin_client, mpv_client);
  let config_for_protocol = config.clone();

  let app_builder = tauri::Builder::default()
    .register_asynchronous_uri_scheme_protocol(
      "jellypilot-image",
      move |_ctx, request, responder| {
        let client = jellyfin_for_protocol.clone();
        let config = config_for_protocol.clone();
        let image_cache_state = image_cache_for_protocol.clone();
        let token = request.uri().path().trim_start_matches('/').to_string();
        tauri::async_runtime::spawn(async move {
          responder.respond(
            image_cache::image_response_for_token(client, config, image_cache_state, token).await,
          );
        });
      },
    )
    .manage(config_state)
    .manage(image_cache_state)
    .manage(mpv_state)
    .manage(jellyfin_state)
    .invoke_handler(builder.invoke_handler())
    .plugin(tauri_plugin_store::Builder::new().build());

  #[cfg(feature = "webdriver")]
  let app_builder = app_builder
    .plugin(logging_plugin())
    .plugin(tauri_plugin_wdio::init())
    .plugin(tauri_plugin_wdio_webdriver::init());

  app_builder
    .setup(move |app| {
      #[cfg(not(feature = "webdriver"))]
      app.handle().plugin(logging_plugin())?;

      // Load config from disk (store plugin is now available)
      let loaded_config = command::load_config_from_store(app.handle());
      match app.path().app_cache_dir() {
        Ok(cache_dir) => {
          *image_cache_for_setup.write() = Some(Arc::new(ImageCache::new(cache_dir)));
        }
        Err(e) => {
          log::warn!(
            "Failed to resolve app cache directory for image cache: {}",
            e
          );
        }
      }

      // Apply loaded config to MPV client
      let mpv_path = loaded_config
        .mpv_path
        .as_ref()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
      mpv_for_setup.set_mpv_path(mpv_path);
      mpv_for_setup.set_extra_args(loaded_config.mpv_args.clone());

      // Apply loaded config to Jellyfin client
      jellyfin_for_setup.set_device_name(loaded_config.device_name.clone());

      // Store config in state
      *config_for_setup.write() = loaded_config;

      // Setup system tray
      if let Err(e) = tray::setup_tray(app) {
        log::error!("Failed to setup system tray: {}", e);
      }

      builder.mount_events(app);
      Ok(())
    })
    .on_window_event(|window, event| {
      // Hide window to tray on close instead of quitting
      if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
      }
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
