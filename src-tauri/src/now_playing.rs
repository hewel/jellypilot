//! Now Playing read model shared by direct queries and session event emission.

use crate::command::{
  AdjacentEpisodeUnavailableReason, NowPlayingMedia, NowPlayingState, NowPlayingStatus, PlayerState,
};
use crate::jellyfin::MediaItem;
use crate::mpv::{MpvClient, PropertyValue};

/// Playback context used to derive user-facing adjacent episode availability.
pub struct PlaybackContext<'a> {
  pub has_active_session: bool,
  pub current_item: Option<&'a MediaItem>,
}

/// Authoritative Now Playing transport snapshot maintained from MPV property
/// observations. The session event loop feeds every observed property change
/// into this snapshot so Now Playing projections never resample MPV properties
/// on the emit path.
#[derive(Debug, Clone, Copy, Default)]
pub struct TransportSnapshot {
  connected: bool,
  paused: bool,
  muted: bool,
  time_pos: f64,
  volume: f64,
  observed_duration: Option<f64>,
}

impl TransportSnapshot {
  /// Record one observed MPV property change. Unknown properties and invalid
  /// values are ignored; any recognized observation marks transport connected.
  pub fn apply_property(&mut self, property_name: &str, data: &serde_json::Value) {
    match property_name {
      "pause" => {
        if let Some(paused) = data.as_bool() {
          self.paused = paused;
          self.connected = true;
        }
      }
      "volume" => {
        if let Some(volume) = data.as_f64() {
          if volume.is_finite() {
            self.volume = volume.clamp(0.0, 100.0);
            self.connected = true;
          }
        }
      }
      "mute" => {
        if let Some(muted) = data.as_bool() {
          self.muted = muted;
          self.connected = true;
        }
      }
      "time-pos" => {
        if let Some(position) = data.as_f64() {
          if position.is_finite() {
            self.time_pos = position;
            self.connected = true;
          }
        }
      }
      "duration" => {
        self.observed_duration = match data.as_f64() {
          Some(duration) if duration.is_finite() && duration >= 0.0 => Some(duration),
          _ => None,
        };
        self.connected = true;
      }
      _ => {}
    }
  }

  /// Drop stale transport state when playback disconnects or goes idle.
  pub fn clear(&mut self) {
    *self = Self::default();
  }

  /// Mark transport connected when a command path starts the MPV process
  /// before any property observation has landed.
  pub fn mark_connected(&mut self) {
    self.connected = true;
  }

  /// Current muted state, for command paths that toggle mute and need the
  /// resulting value before the observation lands.
  pub fn muted(&self) -> bool {
    self.muted
  }

  /// Re-seed transport for a newly started playback session: per-item
  /// position/duration reset while process-level volume and mute persist.
  pub fn reset_for_new_session(&mut self, start_position_seconds: f64) {
    *self = Self {
      connected: true,
      paused: false,
      muted: self.muted,
      time_pos: if start_position_seconds.is_finite() {
        start_position_seconds.max(0.0)
      } else {
        0.0
      },
      volume: self.volume,
      observed_duration: None,
    };
  }

  /// Project the snapshot into the Now Playing player state. Duration falls
  /// back to the current media runtime, then zero, so the projection always
  /// carries a finite non-negative duration.
  pub fn project(&self, media_runtime_seconds: Option<f64>) -> PlayerState {
    let duration = self
      .observed_duration
      .filter(|duration| duration.is_finite() && *duration >= 0.0)
      .or_else(|| media_runtime_seconds.filter(|runtime| runtime.is_finite() && *runtime >= 0.0))
      .unwrap_or(0.0);

    PlayerState {
      connected: self.connected,
      paused: self.paused,
      muted: self.muted,
      time_pos: self.time_pos,
      duration,
      volume: self.volume,
    }
  }
}

/// Collect the current MPV player state used by the Now Playing read model.
pub async fn collect_player_state(mpv: &MpvClient) -> PlayerState {
  if !mpv.is_connected() {
    return PlayerState::default();
  }

  let (paused_res, time_pos_res, duration_res, volume_res, muted_res) = tokio::join!(
    mpv.get_property("pause"),
    mpv.get_property("time-pos"),
    mpv.get_property("duration"),
    mpv.get_property("volume"),
    mpv.get_property("mute"),
  );

  let paused = match paused_res {
    Ok(PropertyValue::Bool(b)) => b,
    Ok(_) => true,
    Err(e) => {
      log::warn!("Failed to get pause property: {}", e);
      true
    }
  };

  let time_pos = match time_pos_res {
    Ok(PropertyValue::Number(n)) if n.is_finite() => n,
    Ok(_) => 0.0,
    Err(e) => {
      log::warn!("Failed to get time-pos property: {}", e);
      0.0
    }
  };

  let duration = match duration_res {
    Ok(PropertyValue::Number(n)) if n.is_finite() => n,
    Ok(_) => 0.0,
    Err(e) => {
      log::warn!("Failed to get duration property: {}", e);
      0.0
    }
  };

  let volume = match volume_res {
    Ok(PropertyValue::Number(n)) if n.is_finite() => n.clamp(0.0, 100.0),
    Ok(_) => 100.0,
    Err(e) => {
      log::warn!("Failed to get volume property: {}", e);
      100.0
    }
  };

  let muted = match muted_res {
    Ok(PropertyValue::Bool(b)) => b,
    Ok(_) => false,
    Err(e) => {
      log::warn!("Failed to get mute property: {}", e);
      false
    }
  };

  PlayerState {
    connected: true,
    paused,
    muted,
    time_pos,
    duration,
    volume,
  }
}

/// Build the user-facing Now Playing state from player and Jellyfin session state.
pub fn build_now_playing_state(
  player: PlayerState,
  context: PlaybackContext<'_>,
) -> NowPlayingState {
  let media = context.current_item.map(|item| NowPlayingMedia {
    item_id: item.id.clone(),
    name: item.name.clone(),
    item_type: item.item_type.clone(),
    series_name: item.series_name.clone(),
    season_number: item.parent_index_number,
    episode_number: item.index_number,
  });

  let unavailable_reason = if !context.has_active_session {
    Some(AdjacentEpisodeUnavailableReason::NoSession)
  } else {
    match context.current_item {
      None => Some(AdjacentEpisodeUnavailableReason::NoCurrentItem),
      Some(item) if item.item_type != "Episode" => {
        Some(AdjacentEpisodeUnavailableReason::NotEpisode)
      }
      Some(_) => None,
    }
  };

  let can_play_adjacent = unavailable_reason.is_none();
  let status = if !player.connected {
    NowPlayingStatus::Offline
  } else if media.is_none() && player.duration <= 0.0 {
    NowPlayingStatus::Idle
  } else if player.paused {
    NowPlayingStatus::Paused
  } else {
    NowPlayingStatus::Playing
  };

  NowPlayingState {
    status,
    player,
    media,
    can_play_next: can_play_adjacent,
    can_play_previous: can_play_adjacent,
    next_unavailable_reason: unavailable_reason.clone(),
    previous_unavailable_reason: unavailable_reason,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn player(connected: bool, paused: bool, duration: f64) -> PlayerState {
    PlayerState {
      connected,
      paused,
      muted: false,
      time_pos: 12.0,
      duration,
      volume: 80.0,
    }
  }

  fn item(item_type: &str) -> MediaItem {
    MediaItem {
      id: "item-1".into(),
      name: "Test Item".into(),
      item_type: item_type.into(),
      series_id: Some("series-1".into()),
      series_name: Some("Series".into()),
      season_name: Some("Season 1".into()),
      index_number: Some(2),
      parent_index_number: Some(1),
      run_time_ticks: Some(1_000),
      overview: None,
    }
  }

  fn state(
    player: PlayerState,
    has_active_session: bool,
    current_item: Option<&MediaItem>,
  ) -> NowPlayingState {
    build_now_playing_state(
      player,
      PlaybackContext {
        has_active_session,
        current_item,
      },
    )
  }

  #[test]
  fn selects_offline_status_when_player_is_disconnected() {
    let state = state(player(false, true, 0.0), false, None);

    assert!(matches!(state.status, NowPlayingStatus::Offline));
  }

  #[test]
  fn selects_idle_status_without_media_or_duration() {
    let state = state(player(true, true, 0.0), true, None);

    assert!(matches!(state.status, NowPlayingStatus::Idle));
  }

  #[test]
  fn selects_paused_status_when_connected_media_is_paused() {
    let episode = item("Episode");
    let state = state(player(true, true, 120.0), true, Some(&episode));

    assert!(matches!(state.status, NowPlayingStatus::Paused));
  }

  #[test]
  fn selects_playing_status_when_connected_media_is_not_paused() {
    let episode = item("Episode");
    let state = state(player(true, false, 120.0), true, Some(&episode));

    assert!(matches!(state.status, NowPlayingStatus::Playing));
  }

  #[test]
  fn adjacent_episode_is_unavailable_without_active_session() {
    let state = state(player(false, true, 0.0), false, None);

    assert!(!state.can_play_next);
    assert!(!state.can_play_previous);
    assert!(matches!(
      state.next_unavailable_reason,
      Some(AdjacentEpisodeUnavailableReason::NoSession)
    ));
    assert!(matches!(
      state.previous_unavailable_reason,
      Some(AdjacentEpisodeUnavailableReason::NoSession)
    ));
  }

  #[test]
  fn adjacent_episode_is_unavailable_without_current_item() {
    let state = state(player(true, true, 0.0), true, None);

    assert!(!state.can_play_next);
    assert!(!state.can_play_previous);
    assert!(matches!(
      state.next_unavailable_reason,
      Some(AdjacentEpisodeUnavailableReason::NoCurrentItem)
    ));
  }

  #[test]
  fn adjacent_episode_is_unavailable_for_non_episode_playback() {
    let movie = item("Movie");
    let state = state(player(true, false, 120.0), true, Some(&movie));

    assert!(!state.can_play_next);
    assert!(!state.can_play_previous);
    assert!(matches!(
      state.next_unavailable_reason,
      Some(AdjacentEpisodeUnavailableReason::NotEpisode)
    ));
  }

  #[test]
  fn adjacent_episode_is_available_for_episode_playback() {
    let episode = item("Episode");
    let state = state(player(true, false, 120.0), true, Some(&episode));

    assert!(state.can_play_next);
    assert!(state.can_play_previous);
    assert!(state.next_unavailable_reason.is_none());
    assert!(state.previous_unavailable_reason.is_none());
  }

  fn observed(properties: &[(&str, serde_json::Value)]) -> TransportSnapshot {
    let mut snapshot = TransportSnapshot::default();
    for (name, data) in properties {
      snapshot.apply_property(name, data);
    }
    snapshot
  }

  #[test]
  fn snapshot_projects_pause_observation() {
    let snapshot = observed(&[("pause", serde_json::json!(true))]);
    let player = snapshot.project(None);

    assert!(player.connected);
    assert!(player.paused);
  }

  #[test]
  fn snapshot_projects_volume_observation_clamped_to_player_range() {
    let loud = observed(&[("volume", serde_json::json!(130.0))]);
    assert_eq!(loud.project(None).volume, 100.0);

    let negative = observed(&[("volume", serde_json::json!(-5.0))]);
    assert_eq!(negative.project(None).volume, 0.0);

    let normal = observed(&[("volume", serde_json::json!(64.0))]);
    assert_eq!(normal.project(None).volume, 64.0);
  }

  #[test]
  fn snapshot_projects_mute_observation() {
    let snapshot = observed(&[("mute", serde_json::json!(true))]);

    assert!(snapshot.project(None).muted);
  }

  #[test]
  fn snapshot_projects_position_observation() {
    let snapshot = observed(&[("time-pos", serde_json::json!(42.5))]);

    assert_eq!(snapshot.project(None).time_pos, 42.5);
  }

  #[test]
  fn snapshot_projects_duration_observation() {
    let snapshot = observed(&[("duration", serde_json::json!(1420.0))]);

    assert_eq!(snapshot.project(Some(60.0)).duration, 1420.0);
  }

  #[test]
  fn snapshot_duration_falls_back_to_media_runtime_when_observation_missing() {
    let snapshot = observed(&[("pause", serde_json::json!(false))]);

    assert_eq!(snapshot.project(Some(1500.0)).duration, 1500.0);
  }

  #[test]
  fn snapshot_duration_falls_back_to_media_runtime_when_observation_invalid() {
    for invalid in [
      serde_json::json!(-1.0),
      serde_json::json!(f64::NAN),
      serde_json::json!(f64::INFINITY),
      serde_json::json!(null),
    ] {
      let snapshot = observed(&[("duration", invalid)]);
      assert_eq!(snapshot.project(Some(1500.0)).duration, 1500.0);
    }
  }

  #[test]
  fn snapshot_duration_falls_back_to_zero_without_observation_or_runtime() {
    let snapshot = observed(&[("pause", serde_json::json!(false))]);
    let player = snapshot.project(None);

    assert_eq!(player.duration, 0.0);
    assert!(player.duration.is_finite());
    assert!(player.duration >= 0.0);
  }

  #[test]
  fn snapshot_ignores_invalid_property_values() {
    let snapshot = observed(&[
      ("pause", serde_json::json!(true)),
      ("pause", serde_json::json!(null)),
      ("time-pos", serde_json::json!(42.5)),
      ("time-pos", serde_json::json!("later")),
      ("volume", serde_json::json!(64.0)),
      ("volume", serde_json::json!(f64::NAN)),
    ]);
    let player = snapshot.project(None);

    assert!(player.paused);
    assert_eq!(player.time_pos, 42.5);
    assert_eq!(player.volume, 64.0);
  }

  #[test]
  fn snapshot_ignores_unknown_properties_without_marking_connected() {
    let snapshot = observed(&[("chapter", serde_json::json!(3))]);

    assert!(!snapshot.project(None).connected);
  }

  #[test]
  fn cleared_snapshot_projects_disconnected_zeroed_transport() {
    let mut snapshot = observed(&[
      ("pause", serde_json::json!(true)),
      ("volume", serde_json::json!(64.0)),
      ("mute", serde_json::json!(true)),
      ("time-pos", serde_json::json!(42.5)),
      ("duration", serde_json::json!(1420.0)),
    ]);

    snapshot.clear();
    let player = snapshot.project(Some(1500.0));

    assert!(!player.connected);
    assert!(!player.paused);
    assert!(!player.muted);
    assert_eq!(player.time_pos, 0.0);
    assert_eq!(player.volume, 0.0);
    assert_eq!(player.duration, 1500.0);
  }

  #[test]
  fn new_session_reseed_preserves_volume_and_mute() {
    let mut snapshot = observed(&[
      ("pause", serde_json::json!(true)),
      ("volume", serde_json::json!(64.0)),
      ("mute", serde_json::json!(true)),
      ("time-pos", serde_json::json!(42.5)),
      ("duration", serde_json::json!(1420.0)),
    ]);

    snapshot.reset_for_new_session(120.0);
    let player = snapshot.project(Some(1500.0));

    assert!(player.connected);
    assert!(!player.paused);
    assert!(player.muted);
    assert_eq!(player.time_pos, 120.0);
    assert_eq!(player.volume, 64.0);
    assert_eq!(player.duration, 1500.0);
  }

  #[test]
  fn new_session_reseed_sanitizes_invalid_start_position() {
    let mut snapshot = observed(&[("pause", serde_json::json!(true))]);

    snapshot.reset_for_new_session(f64::NAN);
    assert_eq!(snapshot.project(None).time_pos, 0.0);

    snapshot.reset_for_new_session(-5.0);
    assert_eq!(snapshot.project(None).time_pos, 0.0);
  }

  #[test]
  fn mark_connected_projects_idle_connected_transport() {
    let mut snapshot = TransportSnapshot::default();
    snapshot.mark_connected();
    let player = snapshot.project(None);

    assert!(player.connected);
    assert_eq!(player.duration, 0.0);
  }
}
