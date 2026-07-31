//! Playback Target MPV action executor behind the MPV JSON IPC seam.
//!
//! One executor translates Playback Target actions into MPV commands through
//! the shared `MpvClient` (socket IPC in production, duplex IPC in tests).
//! Jellyfin orchestration stays upstream in the session; ADR 0004 is
//! preserved because MPV only ever starts from this authenticated playback
//! path, and start/load failures surface the existing user-facing
//! notifications without adding a frontend event type.

use std::sync::Arc;

use parking_lot::RwLock;

use super::session::redact_url;
use crate::command::AppNotification;
use crate::config::AppConfig;
use crate::mpv::{has_mpv_option, MpvClient};

/// Actions to perform on MPV.
#[derive(Debug, Clone)]
pub enum MpvAction {
  /// Load and play a URL.
  Play {
    url: String,
    start_position: f64,
    title: String,
    audio_index: Option<i32>,
    subtitle_index: Option<i32>,
    play_method: &'static str,
  },
  /// Add an external subtitle file.
  AddExternalSubtitle(String),
  /// Pause playback.
  Pause,
  /// Resume playback.
  Resume,
  /// Seek to position (seconds).
  Seek(f64),
  /// Show text on MPV's on-screen display.
  ShowText { text: String, duration_ms: i64 },
  /// Stop playback.
  Stop,
  /// Set volume (0-100).
  SetVolume(i32),
  /// Toggle mute.
  ToggleMute,
  /// Toggle fullscreen.
  ToggleFullscreen,
  /// Set audio track by stream index.
  SetAudioTrack(i32),
  /// Set subtitle track by stream index (-1 to disable).
  SetSubtitleTrack(i32),
}

const DIRECT_PLAYBACK_CACHE_OPTIONS: [(&str, &str); 8] = [
  ("cache", "cache=yes"),
  ("cache-on-disk", "cache-on-disk=yes"),
  ("demuxer-max-bytes", "demuxer-max-bytes=256MiB"),
  ("demuxer-max-back-bytes", "demuxer-max-back-bytes=128MiB"),
  ("demuxer-seekable-cache", "demuxer-seekable-cache=yes"),
  ("cache-pause", "cache-pause=yes"),
  ("cache-pause-initial", "cache-pause-initial=yes"),
  ("cache-pause-wait", "cache-pause-wait=3"),
];

fn direct_playback_file_options(play_method: &str, configured_args: &[String]) -> Vec<String> {
  if !matches!(play_method, "DirectPlay" | "DirectStream") {
    return Vec::new();
  }

  DIRECT_PLAYBACK_CACHE_OPTIONS
    .iter()
    .filter(|(name, _)| !has_mpv_option(configured_args, name))
    .map(|(_, setting)| (*setting).to_owned())
    .collect()
}

/// Executes Playback Target MPV actions through the MPV JSON IPC seam.
pub struct MpvActionExecutor {
  mpv: MpvClient,
  config: Arc<RwLock<AppConfig>>,
  on_mpv_started: Arc<dyn Fn() + Send + Sync>,
  notify_error: Arc<dyn Fn(String) + Send + Sync>,
}

impl MpvActionExecutor {
  pub fn new(
    mpv: MpvClient,
    config: Arc<RwLock<AppConfig>>,
    on_mpv_started: impl Fn() + Send + Sync + 'static,
    notify_error: impl Fn(String) + Send + Sync + 'static,
  ) -> Self {
    Self {
      mpv,
      config,
      on_mpv_started: Arc::new(on_mpv_started),
      notify_error: Arc::new(notify_error),
    }
  }

  /// Wire the session-owned notification callback without exposing the
  /// executor to Tauri state.
  pub fn from_session(
    mpv: MpvClient,
    config: Arc<RwLock<AppConfig>>,
    app_handle: tauri::AppHandle,
    on_mpv_started: impl Fn() + Send + Sync + 'static,
  ) -> Self {
    Self::new(mpv, config, on_mpv_started, move |message| {
      AppNotification::error(&app_handle, message)
    })
  }

  /// Execute one Playback Target action through the MPV IPC seam.
  pub async fn execute(&self, action: MpvAction) {
    match action {
      MpvAction::Play {
        url,
        start_position,
        title,
        audio_index,
        subtitle_index,
        play_method,
      } => {
        log::info!(
          "MpvAction::Play received, url={}, title={}",
          redact_url(&url),
          title
        );
        // Start MPV if not already running; MPV starts only from this
        // authenticated playback path (ADR 0004).
        if !self.mpv.is_connected() {
          log::info!("MPV not connected, starting...");
          if let Err(e) = self.mpv.start().await {
            log::error!("Failed to start MPV: {}", e);
            (self.notify_error)(format!("Failed to start MPV: {}", e));
            return;
          }
          (self.on_mpv_started)();
          log::info!("MPV started successfully");
        }

        let file_options = {
          let config = self.config.read();
          direct_playback_file_options(play_method, &config.mpv_args)
        };

        // Load the file with all options (start position, audio/subtitle tracks)
        // This ensures tracks are set atomically with the file load, avoiding race conditions
        log::info!(
          "Loading file into MPV: {} (start={}, aid={:?}, sid={:?}, play_method={}, file_options={:?})",
          redact_url(&url),
          start_position,
          audio_index,
          subtitle_index,
          play_method,
          file_options
        );
        if let Err(e) = self
          .mpv
          .loadfile_with_options(
            &url,
            Some(start_position),
            audio_index.map(|i| i as i64),
            subtitle_index.map(|i| i as i64),
            file_options,
          )
          .await
        {
          log::error!("Failed to load file: {}", e);
          (self.notify_error)(format!("Failed to load media: {}", e));
          return;
        }
        log::info!("File loaded successfully");

        // Set the media title (shown in MPV window)
        if let Err(e) = self
          .mpv
          .set_property_string("force-media-title", &title)
          .await
        {
          log::warn!("Failed to set media title: {}", e);
        }

        log::info!("Started playback: {} - {}", title, redact_url(&url));
      }
      MpvAction::Pause => {
        log::info!("MpvAction::Pause - setting pause=true");
        if let Err(e) = self.mpv.set_pause(true).await {
          log::error!("Failed to pause: {}", e);
        } else {
          log::info!("MPV paused successfully");
        }
      }
      MpvAction::Resume => {
        log::info!("MpvAction::Resume - setting pause=false");
        if let Err(e) = self.mpv.set_pause(false).await {
          log::error!("Failed to resume: {}", e);
        } else {
          log::info!("MPV resumed successfully");
        }
      }
      MpvAction::Seek(position) => {
        if let Err(e) = self.mpv.seek(position).await {
          log::error!("Failed to seek: {}", e);
        }
      }
      MpvAction::ShowText { text, duration_ms } => {
        if let Err(e) = self.mpv.show_text(&text, duration_ms).await {
          log::warn!("Failed to show MPV text: {}", e);
        }
      }
      MpvAction::Stop => {
        log::info!("MpvAction::Stop - quitting MPV gracefully");
        if let Err(e) = self.mpv.quit().await {
          log::warn!("Failed to quit MPV gracefully: {}, forcing stop", e);
          self.mpv.stop().await;
        }
      }
      MpvAction::SetVolume(volume) => {
        if let Err(e) = self.mpv.set_volume(volume as f64).await {
          log::error!("Failed to set volume: {}", e);
        }
      }
      MpvAction::ToggleMute => {
        if let Err(e) = self.mpv.toggle_mute().await {
          log::error!("Failed to toggle mute: {}", e);
        }
      }
      MpvAction::ToggleFullscreen => {
        if let Err(e) = self.mpv.toggle_fullscreen().await {
          log::error!("Failed to toggle fullscreen: {}", e);
        }
      }
      MpvAction::SetAudioTrack(index) => {
        // index is already MPV's 1-based track ID
        if let Err(e) = self.mpv.set_audio_track(index as i64).await {
          log::error!("Failed to set audio track: {}", e);
        }
      }
      MpvAction::SetSubtitleTrack(index) => {
        if index == -1 {
          // Disable subtitles
          if let Err(e) = self.mpv.disable_track("sid").await {
            log::error!("Failed to disable subtitles: {}", e);
          }
        } else {
          // index is already MPV's 1-based track ID
          if let Err(e) = self.mpv.set_subtitle_track(index as i64).await {
            log::error!("Failed to set subtitle track: {}", e);
          }
        }
      }
      MpvAction::AddExternalSubtitle(url) => {
        log::info!("MpvAction::AddExternalSubtitle: {}", redact_url(&url));
        if let Err(e) = self.mpv.sub_add(&url, true).await {
          log::error!("Failed to add external subtitle: {}", e);
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::mpv::MpvIpc;
  use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, BufReader};

  #[test]
  fn direct_play_adds_cache_profile_when_user_has_no_overrides() {
    assert_eq!(
      direct_playback_file_options("DirectPlay", &[]),
      vec![
        "cache=yes",
        "cache-on-disk=yes",
        "demuxer-max-bytes=256MiB",
        "demuxer-max-back-bytes=128MiB",
        "demuxer-seekable-cache=yes",
        "cache-pause=yes",
        "cache-pause-initial=yes",
        "cache-pause-wait=3",
      ]
    );
  }

  #[test]
  fn direct_stream_adds_cache_profile_when_user_has_no_overrides() {
    assert_eq!(
      direct_playback_file_options("DirectStream", &[]),
      vec![
        "cache=yes",
        "cache-on-disk=yes",
        "demuxer-max-bytes=256MiB",
        "demuxer-max-back-bytes=128MiB",
        "demuxer-seekable-cache=yes",
        "cache-pause=yes",
        "cache-pause-initial=yes",
        "cache-pause-wait=3",
      ]
    );
  }

  #[test]
  fn direct_cache_profile_preserves_explicit_user_options() {
    let configured_args = vec![
      "--cache=no".to_string(),
      "--cache-on-disk=no".to_string(),
      "--demuxer-max-bytes=512MiB".to_string(),
      "--demuxer-seekable-cache=no".to_string(),
      "--no-cache-pause".to_string(),
      "--no-cache-pause-initial".to_string(),
    ];

    assert_eq!(
      direct_playback_file_options("DirectPlay", &configured_args),
      vec!["demuxer-max-back-bytes=128MiB", "cache-pause-wait=3",]
    );
  }

  #[test]
  fn transcode_does_not_add_direct_cache_profile() {
    assert!(direct_playback_file_options("Transcode", &[]).is_empty());
  }

  type WireLog = Arc<parking_lot::Mutex<Vec<serde_json::Value>>>;

  /// MPV test seam harness: a recording duplex peer answering every command.
  struct DuplexMpv {
    executor: MpvActionExecutor,
    wire: WireLog,
    notifications: Arc<parking_lot::Mutex<Vec<String>>>,
    mpv_started: Arc<parking_lot::Mutex<u32>>,
    peer: tokio::task::JoinHandle<()>,
  }

  impl DuplexMpv {
    async fn new(mpv_args: Vec<String>) -> Self {
      Self::with_peer_behavior(mpv_args, |_| None).await
    }

    /// `override_response` may return a replacement response payload for a
    /// command; `None` falls back to the default success reply.
    async fn with_peer_behavior(
      mpv_args: Vec<String>,
      override_response: impl Fn(&serde_json::Value) -> Option<serde_json::Value> + Send + 'static,
    ) -> Self {
      let mpv = MpvClient::new(None);
      let (client_stream, peer_stream) = duplex(64 * 1024);
      let (reader, writer) = tokio::io::split(client_stream);
      let ipc = MpvIpc::from_io_for_test(reader, writer)
        .await
        .expect("test IPC should be constructed");
      mpv.install_ipc_for_test(ipc);

      let wire: WireLog = Arc::new(parking_lot::Mutex::new(Vec::new()));
      let peer_wire = Arc::clone(&wire);
      let (peer_reader, mut peer_writer) = tokio::io::split(peer_stream);
      let peer = tokio::spawn(async move {
        let mut lines = BufReader::new(peer_reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
          let Ok(command) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
          };
          peer_wire.lock().push(command.clone());
          let request_id = command.get("request_id").and_then(|id| id.as_i64());
          if let Some(request_id) = request_id {
            let mut response = override_response(&command)
              .unwrap_or_else(|| serde_json::json!({"error": "success", "data": null}));
            response["request_id"] = serde_json::json!(request_id);
            let _ = peer_writer
              .write_all(format!("{}\n", response).as_bytes())
              .await;
          }
        }
      });

      let notifications = Arc::new(parking_lot::Mutex::new(Vec::new()));
      let mpv_started = Arc::new(parking_lot::Mutex::new(0_u32));
      let executor = MpvActionExecutor::new(
        mpv,
        Arc::new(RwLock::new(AppConfig {
          mpv_args,
          ..Default::default()
        })),
        {
          let mpv_started = Arc::clone(&mpv_started);
          move || *mpv_started.lock() += 1
        },
        {
          let notifications = Arc::clone(&notifications);
          move |message| notifications.lock().push(message)
        },
      );

      Self {
        executor,
        wire,
        notifications,
        mpv_started,
        peer,
      }
    }

    fn commands(&self) -> Vec<Vec<serde_json::Value>> {
      self
        .wire
        .lock()
        .iter()
        .filter_map(|entry| entry.get("command").and_then(|c| c.as_array()).cloned())
        .collect()
    }

    fn abort(self) {
      self.peer.abort();
    }
  }

  #[tokio::test]
  async fn play_loads_file_with_start_tracks_and_direct_cache_options() {
    let harness = DuplexMpv::new(Vec::new()).await;

    harness
      .executor
      .execute(MpvAction::Play {
        url: "https://jellyfin.example.com/Videos/episode-1/stream".to_string(),
        start_position: 120.0,
        title: "The Pilot".to_string(),
        audio_index: Some(2),
        subtitle_index: Some(-1),
        play_method: "DirectPlay",
      })
      .await;

    let commands = harness.commands();
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0][0], serde_json::json!("loadfile"));
    assert_eq!(
      commands[0][1],
      serde_json::json!("https://jellyfin.example.com/Videos/episode-1/stream")
    );
    assert_eq!(commands[0][2], serde_json::json!("replace"));
    let options = commands[0][4].as_str().expect("loadfile options string");
    for expected in [
      "start=120",
      "aid=2",
      "sid=no",
      "cache=yes",
      "cache-on-disk=yes",
      "demuxer-max-bytes=256MiB",
      "demuxer-max-back-bytes=128MiB",
      "demuxer-seekable-cache=yes",
      "cache-pause=yes",
      "cache-pause-initial=yes",
      "cache-pause-wait=3",
    ] {
      assert!(
        options.contains(expected),
        "options missing {expected}: {options}"
      );
    }
    assert_eq!(
      commands[1],
      vec![
        serde_json::json!("set_property"),
        serde_json::json!("force-media-title"),
        serde_json::json!("The Pilot"),
      ]
    );
    // The IPC was already connected, so no process start fired.
    assert_eq!(*harness.mpv_started.lock(), 0);

    harness.abort();
  }

  #[tokio::test]
  async fn transport_actions_map_to_the_same_mpv_commands() {
    let harness = DuplexMpv::new(Vec::new()).await;

    for action in [
      MpvAction::Pause,
      MpvAction::Resume,
      MpvAction::Seek(42.5),
      MpvAction::SetVolume(64),
      MpvAction::ToggleMute,
      MpvAction::ToggleFullscreen,
      MpvAction::SetAudioTrack(3),
      MpvAction::SetSubtitleTrack(-1),
      MpvAction::SetSubtitleTrack(5),
      MpvAction::AddExternalSubtitle("https://jellyfin.example.com/subtitles/1.srt".to_string()),
      MpvAction::ShowText {
        text: "Skipped intro".to_string(),
        duration_ms: 1500,
      },
    ] {
      harness.executor.execute(action).await;
    }

    let expected: Vec<Vec<serde_json::Value>> = vec![
      vec![
        serde_json::json!("set_property"),
        serde_json::json!("pause"),
        serde_json::json!(true),
      ],
      vec![
        serde_json::json!("set_property"),
        serde_json::json!("pause"),
        serde_json::json!(false),
      ],
      vec![
        serde_json::json!("seek"),
        serde_json::json!(42.5),
        serde_json::json!("absolute"),
      ],
      vec![
        serde_json::json!("set_property"),
        serde_json::json!("volume"),
        serde_json::json!(64.0),
      ],
      vec![serde_json::json!("cycle"), serde_json::json!("mute")],
      vec![serde_json::json!("cycle"), serde_json::json!("fullscreen")],
      vec![
        serde_json::json!("set_property"),
        serde_json::json!("aid"),
        serde_json::json!(3),
      ],
      vec![
        serde_json::json!("set_property"),
        serde_json::json!("sid"),
        serde_json::json!("no"),
      ],
      vec![
        serde_json::json!("set_property"),
        serde_json::json!("sid"),
        serde_json::json!(5),
      ],
      vec![
        serde_json::json!("sub-add"),
        serde_json::json!("https://jellyfin.example.com/subtitles/1.srt"),
        serde_json::json!("select"),
      ],
      vec![
        serde_json::json!("show-text"),
        serde_json::json!("Skipped intro"),
        serde_json::json!(1500),
      ],
    ];
    assert_eq!(harness.commands(), expected);

    harness.abort();
  }

  #[tokio::test]
  async fn play_load_failure_surfaces_existing_notification() {
    let harness = DuplexMpv::with_peer_behavior(Vec::new(), |command| {
      let is_loadfile = command
        .get("command")
        .and_then(|c| c.as_array())
        .and_then(|parts| parts.first())
        .and_then(|name| name.as_str())
        == Some("loadfile");
      is_loadfile.then(|| serde_json::json!({"error": "playback rejected"}))
    })
    .await;

    harness
      .executor
      .execute(MpvAction::Play {
        url: "https://jellyfin.example.com/Videos/episode-1/stream".to_string(),
        start_position: 0.0,
        title: "The Pilot".to_string(),
        audio_index: None,
        subtitle_index: None,
        play_method: "Transcode",
      })
      .await;

    let notifications = harness.notifications.lock();
    assert_eq!(notifications.len(), 1);
    assert!(
      notifications[0].starts_with("Failed to load media: "),
      "unexpected notification: {}",
      notifications[0]
    );
    drop(notifications);

    harness.abort();
  }
}
