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
}
