use std::path::PathBuf;
use std::sync::Arc;

mod auth_profiles;
pub mod avif_encode;
mod avif_worker;
mod command;
mod config;
mod hls_proxy;
mod image_cache;
mod image_proxy;
mod image_ref;
mod jellyfin;
mod mpv;
mod now_playing;
mod playback_control;
mod tray;

use command::{ConfigState, JellyfinState, MpvState};
pub use config::AppConfig;
use hls_proxy::{HlsProxy, HlsProxyState};
use image_cache::{ImageCache, ImageCacheState};
use image_proxy::{ImageProxy, ImageProxyState};
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
  let hls_proxy_state = HlsProxyState::default();
  let hls_proxy_for_setup = hls_proxy_state.clone();
  let image_proxy_state = ImageProxyState::new();
  let image_proxy_for_setup = image_proxy_state.clone();
  let foreground_gate = Arc::new(avif_worker::ForegroundGate::new());
  let avif_capability = avif_worker::AvifCapability::new();

  // Create MPV client state
  let mpv_client = Arc::new(MpvClient::new(None));
  let mpv_state = MpvState(mpv_client.clone());
  let mpv_for_setup = mpv_client.clone();

  // Create Jellyfin client state
  let jellyfin_client = Arc::new(JellyfinClient::new());
  let jellyfin_for_setup = jellyfin_client.clone();
  let jellyfin_state = JellyfinState::new(jellyfin_client, mpv_client, hls_proxy_state);

  let app_builder = tauri::Builder::default()
    .manage(config_state)
    .manage(image_cache_state)
    .manage(image_proxy_state)
    .manage(mpv_state)
    .manage(jellyfin_state)
    .manage(avif_capability.clone())
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
      let image_cache = match app.path().app_cache_dir() {
        Ok(cache_dir) => {
          mpv_for_setup.set_demuxer_cache_dir(cache_dir.clone());
          hls_proxy_for_setup.install(HlsProxy::start(Some(cache_dir.join("hls"))));
          match tauri::async_runtime::block_on(ImageCache::init(
            cache_dir.clone(),
            image_cache::IMAGE_CACHE_MAX_BYTES,
          )) {
            Ok(cache) => {
              image_cache_for_setup.write().replace(Arc::clone(&cache));
              // Start the background AVIF conversion worker for this cache dir.
              // The loop owns/releases the cache-dir lock itself: it yields to
              // another enabled process when disabled and recovers durable
              // state when it (re)acquires the lock.
              let _worker = avif_worker::ConversionWorker::start(
                Arc::clone(&cache),
                cache_dir,
                Arc::clone(&foreground_gate),
                avif_capability.clone(),
                config.clone(),
              );
              // Keep the worker alive for the app lifetime.
              std::mem::forget(_worker);
              Some(cache)
            }
            Err(e) => {
              log::warn!(
                "Image cache unavailable ({}); serving images from origin",
                e
              );
              None
            }
          }
        }
        Err(e) => {
          log::warn!(
            "Failed to resolve app cache directory for media caches: {}",
            e
          );
          hls_proxy_for_setup.install(HlsProxy::start(None));
          None
        }
      };
      let image_proxy_res = ImageProxy::start(
        jellyfin_for_setup.clone(),
        image_cache,
        config.clone(),
        Arc::clone(&foreground_gate),
        avif_capability.clone(),
      );
      if let Err(e) = &image_proxy_res {
        log::warn!("Failed to start localhost image proxy: {}", e);
      }
      image_proxy_for_setup.install(image_proxy_res);

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
