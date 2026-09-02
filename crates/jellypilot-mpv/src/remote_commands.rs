//! Translation of Jellyfin remote-control commands (Play/Playstate/General)
//! into playback intents and play actions. Sunk from the iced playback
//! surface (ADR 0029): display-free and pure so both frontends share it.

use jellypilot_media_server::ticks_to_seconds;
use jellypilot_session::{
  remote_index_value, remote_volume_value, GeneralCommand, JellyfinCommand, PlayRequest,
  PlaystateRequest,
};

use crate::playback::PlaybackSelection;
use crate::playback_session::{AdjacentDirection, PlaybackIntent, SessionView, TracksView};

/// One translated remote-control command: either a playback intent applied to
/// the active session, or a request to resolve and play an item.
#[derive(Clone)]
pub enum RemoteCommandAction {
  Intent(RemotePlaybackIntent),
  Play {
    item_id: String,
    start_position_ticks: Option<i64>,
    selection: PlaybackSelection,
  },
}

/// Playback intent expressed in the provider's wire vocabulary (track indices
/// are provider stream indices until mapped onto MPV track ids).
#[derive(Clone, Copy)]
pub enum RemotePlaybackIntent {
  SetPaused(bool),
  TogglePaused,
  Seek(f64),
  SetVolume(f64),
  SetMuted(bool),
  SelectAudioStream(i64),
  SelectSubtitleStream(Option<i64>),
  Stop,
  PlayAdjacent(AdjacentDirection),
}

impl RemotePlaybackIntent {
  /// Maps provider stream indices onto the session's current MPV track ids;
  /// `None` when the required track mapping is not loaded.
  #[must_use]
  pub fn into_playback_intent(self, playback: &SessionView) -> Option<PlaybackIntent> {
    match self {
      Self::SetPaused(paused) => Some(PlaybackIntent::SetPaused(paused)),
      Self::TogglePaused => Some(PlaybackIntent::TogglePaused),
      Self::Seek(position) => Some(PlaybackIntent::Seek(position)),
      Self::SetVolume(volume) => Some(PlaybackIntent::SetVolume(volume)),
      Self::SetMuted(muted) => Some(PlaybackIntent::SetMuted(muted)),
      Self::SelectAudioStream(index) => {
        provider_track_id(playback, "audio", index).map(PlaybackIntent::SelectAudioTrack)
      }
      Self::SelectSubtitleStream(None) => Some(PlaybackIntent::SelectSubtitleTrack(None)),
      Self::SelectSubtitleStream(Some(index)) => provider_track_id(playback, "sub", index)
        .map(|id| PlaybackIntent::SelectSubtitleTrack(Some(id))),
      Self::Stop => Some(PlaybackIntent::Stop),
      Self::PlayAdjacent(direction) => Some(PlaybackIntent::PlayAdjacent(direction)),
    }
  }

  /// True when the intent ends or replaces the current item, invalidating any
  /// in-flight remote play resolution.
  #[must_use]
  pub const fn invalidates_remote_play(self) -> bool {
    matches!(self, Self::Stop | Self::PlayAdjacent(_))
  }
}

/// Translates a Jellyfin remote-control command into a playback action.
/// `playback` supplies the current mute state for toggle commands.
#[must_use]
pub fn remote_command_action(
  command: JellyfinCommand,
  playback: &SessionView,
) -> Option<RemoteCommandAction> {
  match command {
    JellyfinCommand::Play(request) => remote_play_action(request),
    JellyfinCommand::Playstate(request) => remote_playstate_action(request),
    JellyfinCommand::GeneralCommand(request) => remote_general_action(
      request,
      playback.now_playing.as_ref().map(|playing| playing.muted),
    ),
  }
}

#[must_use]
pub fn remote_play_action(request: PlayRequest) -> Option<RemoteCommandAction> {
  Some(RemoteCommandAction::Play {
    item_id: request.item_ids.first()?.clone(),
    start_position_ticks: request.start_position_ticks,
    selection: PlaybackSelection {
      media_source_id: request.media_source_id,
      audio_stream_index: request.audio_stream_index,
      subtitle_stream_index: request.subtitle_stream_index,
    },
  })
}

#[must_use]
pub fn remote_playstate_action(request: PlaystateRequest) -> Option<RemoteCommandAction> {
  let intent = match request.command.as_str() {
    "Pause" => RemotePlaybackIntent::SetPaused(true),
    "Unpause" => RemotePlaybackIntent::SetPaused(false),
    "PlayPause" => RemotePlaybackIntent::TogglePaused,
    "Seek" => RemotePlaybackIntent::Seek(ticks_to_seconds(request.seek_position_ticks?)),
    "Stop" => RemotePlaybackIntent::Stop,
    "NextTrack" => RemotePlaybackIntent::PlayAdjacent(AdjacentDirection::Next),
    "PreviousTrack" => RemotePlaybackIntent::PlayAdjacent(AdjacentDirection::Previous),
    _ => return None,
  };
  Some(RemoteCommandAction::Intent(intent))
}

#[must_use]
pub fn remote_general_action(
  request: GeneralCommand,
  muted: Option<bool>,
) -> Option<RemoteCommandAction> {
  let arguments = request.arguments.as_ref();
  let intent = match request.name.as_str() {
    "SetVolume" => RemotePlaybackIntent::SetVolume(remote_volume_value(
      arguments.and_then(|arguments| arguments.get("Volume")),
    )?),
    "ToggleMute" => RemotePlaybackIntent::SetMuted(!muted?),
    "SetAudioStreamIndex" => RemotePlaybackIntent::SelectAudioStream(remote_index_value(
      arguments.and_then(|arguments| arguments.get("Index")),
    )?),
    "SetSubtitleStreamIndex" => {
      let index = remote_index_value(arguments.and_then(|arguments| arguments.get("Index")))?;
      RemotePlaybackIntent::SelectSubtitleStream((index >= 0).then_some(index))
    }
    _ => return None,
  };
  Some(RemoteCommandAction::Intent(intent))
}

/// Resolves a provider stream index to the session's current MPV track id.
#[must_use]
fn provider_track_id(playback: &SessionView, track_type: &str, provider_index: i64) -> Option<i64> {
  let TracksView::Ready { tracks, .. } = &playback.tracks else {
    return None;
  };
  tracks
    .iter()
    .find(|track| {
      track.track_type == track_type && track.provider_index.map(i64::from) == Some(provider_index)
    })
    .map(|track| track.id)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::playback::{NowPlayingItem, TrackInfo};
  use crate::playback_session::{NowPlayingView, PlaybackSession};

  fn session_view() -> SessionView {
    PlaybackSession::default().view()
  }

  fn playstate_action(command: &str, seek_position_ticks: Option<i64>) -> RemoteCommandAction {
    remote_playstate_action(PlaystateRequest {
      command: command.to_owned(),
      seek_position_ticks,
    })
    .expect("supported command should map")
  }

  fn general_action(
    name: &str,
    arguments: Option<serde_json::Value>,
    muted: Option<bool>,
  ) -> RemoteCommandAction {
    remote_general_action(
      GeneralCommand {
        name: name.to_owned(),
        arguments,
      },
      muted,
    )
    .expect("supported command should map")
  }

  #[test]
  fn remote_playstate_commands_map_to_session_intents() {
    assert!(matches!(
      playstate_action("Pause", None),
      RemoteCommandAction::Intent(RemotePlaybackIntent::SetPaused(true))
    ));
    assert!(matches!(
      playstate_action("Unpause", None),
      RemoteCommandAction::Intent(RemotePlaybackIntent::SetPaused(false))
    ));
    assert!(matches!(
      playstate_action("PlayPause", None),
      RemoteCommandAction::Intent(RemotePlaybackIntent::TogglePaused)
    ));
    assert!(matches!(
      playstate_action("Seek", Some(125_000_000)),
      RemoteCommandAction::Intent(RemotePlaybackIntent::Seek(12.5))
    ));
    assert!(matches!(
      playstate_action("Stop", None),
      RemoteCommandAction::Intent(RemotePlaybackIntent::Stop)
    ));
    assert!(matches!(
      playstate_action("NextTrack", None),
      RemoteCommandAction::Intent(RemotePlaybackIntent::PlayAdjacent(AdjacentDirection::Next))
    ));
    assert!(matches!(
      playstate_action("PreviousTrack", None),
      RemoteCommandAction::Intent(RemotePlaybackIntent::PlayAdjacent(
        AdjacentDirection::Previous
      ))
    ));
  }

  #[test]
  fn remote_general_commands_accept_wire_values_and_map_to_session_intents() {
    for (value, expected) in [
      (serde_json::json!("52.5"), 52.5),
      (serde_json::json!(125), 100.0),
      (serde_json::json!(-5), 0.0),
    ] {
      assert!(matches!(
        general_action("SetVolume", Some(serde_json::json!({ "Volume": value })), None),
        RemoteCommandAction::Intent(RemotePlaybackIntent::SetVolume(volume))
          if volume == expected
      ));
    }
    assert!(matches!(
      general_action("ToggleMute", None, Some(false)),
      RemoteCommandAction::Intent(RemotePlaybackIntent::SetMuted(true))
    ));
    assert!(matches!(
      general_action(
        "SetAudioStreamIndex",
        Some(serde_json::json!({ "Index": "4" })),
        None,
      ),
      RemoteCommandAction::Intent(RemotePlaybackIntent::SelectAudioStream(4))
    ));
    assert!(matches!(
      general_action(
        "SetSubtitleStreamIndex",
        Some(serde_json::json!({ "Index": -1 })),
        None,
      ),
      RemoteCommandAction::Intent(RemotePlaybackIntent::SelectSubtitleStream(None))
    ));
    assert!(matches!(
      general_action(
        "SetSubtitleStreamIndex",
        Some(serde_json::json!({ "Index": 7 })),
        None,
      ),
      RemoteCommandAction::Intent(RemotePlaybackIntent::SelectSubtitleStream(Some(7)))
    ));
  }

  #[test]
  fn remote_play_carries_source_and_track_selection() {
    let action = remote_play_action(PlayRequest {
      item_ids: vec!["episode-1".to_owned(), "episode-2".to_owned()],
      start_position_ticks: Some(75_000_000),
      play_command: "PlayNow".to_owned(),
      media_source_id: Some("source-2".to_owned()),
      audio_stream_index: Some(4),
      subtitle_stream_index: Some(7),
    })
    .expect("play request should map");

    assert!(matches!(
      action,
      RemoteCommandAction::Play {
        item_id,
        start_position_ticks: Some(75_000_000),
        selection: PlaybackSelection {
          media_source_id: Some(source),
          audio_stream_index: Some(4),
          subtitle_stream_index: Some(7),
        },
      } if item_id == "episode-1" && source == "source-2"
    ));
  }

  #[test]
  fn provider_stream_index_maps_to_current_mpv_track_id() {
    let mut playback = session_view();
    playback.tracks = TracksView::Ready {
      tracks: vec![
        TrackInfo {
          id: 2,
          track_type: "audio".to_owned(),
          title: None,
          language: None,
          selected: false,
          provider_index: Some(4),
        },
        TrackInfo {
          id: 6,
          track_type: "sub".to_owned(),
          title: None,
          language: None,
          selected: false,
          provider_index: Some(7),
        },
      ],
      audio: None,
      subtitle: None,
    };

    assert!(matches!(
      RemotePlaybackIntent::SelectAudioStream(4).into_playback_intent(&playback),
      Some(PlaybackIntent::SelectAudioTrack(2))
    ));
    assert!(matches!(
      RemotePlaybackIntent::SelectSubtitleStream(Some(7)).into_playback_intent(&playback),
      Some(PlaybackIntent::SelectSubtitleTrack(Some(6)))
    ));
  }

  #[test]
  fn toggle_mute_uses_the_sessions_current_mute_state() {
    let mut playback = session_view();
    playback.now_playing = Some(NowPlayingView {
      item: NowPlayingItem {
        item_id: "episode-1".to_owned(),
        title: "Pilot".to_owned(),
        item_type: "Episode".to_owned(),
        runtime_seconds: Some(1_800.0),
        start_position_seconds: 0.0,
        play_method: "Transcode".to_owned(),
      },
      paused: false,
      position_seconds: 0.0,
      duration_seconds: Some(1_800.0),
      volume: 75.0,
      muted: true,
    });

    assert!(matches!(
      remote_command_action(
        JellyfinCommand::GeneralCommand(GeneralCommand {
          name: "ToggleMute".to_owned(),
          arguments: None,
        }),
        &playback,
      ),
      Some(RemoteCommandAction::Intent(RemotePlaybackIntent::SetMuted(
        false
      )))
    ));
  }
}
