//! Portable MPV transport state and collection.

use serde::{Deserialize, Serialize};

use crate::{MpvClient, MpvError, PropertyValue};

/// Player transport state.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
  pub connected: bool,
  pub paused: bool,
  pub muted: bool,
  pub time_pos: f64,
  pub duration: f64,
  pub volume: f64,
}

impl Default for PlayerState {
  fn default() -> Self {
    Self {
      connected: false,
      paused: true,
      muted: false,
      time_pos: 0.0,
      duration: 0.0,
      volume: 100.0,
    }
  }
}

/// Authoritative transport snapshot maintained from MPV property observations.
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
  /// Record one observed MPV property change.
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

  /// Mark transport connected before the first property observation arrives.
  pub fn mark_connected(&mut self) {
    self.connected = true;
  }

  /// Return the current muted state.
  pub fn muted(&self) -> bool {
    self.muted
  }

  /// Re-seed transport for a newly started playback session.
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

  /// Project the snapshot, falling back to the media runtime for duration.
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

type PropertyQueryResult = Result<PropertyValue, MpvError>;

/// A best-effort MPV property sample with connectivity kept separate from
/// individual property availability.
///
/// MPV can reject a valid property query while its IPC connection remains
/// healthy (for example, `duration` for a live stream). Consumers should merge
/// this sample over their last coherent state instead of replacing missing
/// values with transport defaults.
#[derive(Debug, Clone)]
pub struct PlayerStateSample {
  connected: bool,
  paused: Option<bool>,
  muted: Option<bool>,
  time_pos: Option<f64>,
  duration: Option<f64>,
  volume: Option<f64>,
}

impl PlayerStateSample {
  fn disconnected() -> Self {
    Self {
      connected: false,
      paused: None,
      muted: None,
      time_pos: None,
      duration: None,
      volume: None,
    }
  }

  /// Whether MPV's IPC transport remained connected after the sample.
  pub fn is_connected(&self) -> bool {
    self.connected
  }

  /// Merge successfully observed properties over a previous coherent state.
  pub fn merge(self, previous: &PlayerState) -> PlayerState {
    if !self.connected {
      return PlayerState::default();
    }

    PlayerState {
      connected: true,
      paused: self.paused.unwrap_or(previous.paused),
      muted: self.muted.unwrap_or(previous.muted),
      time_pos: self.time_pos.unwrap_or(previous.time_pos),
      duration: self.duration.unwrap_or(previous.duration),
      volume: self.volume.unwrap_or(previous.volume),
    }
  }
}

struct PropertySample {
  paused: PropertyQueryResult,
  time_pos: PropertyQueryResult,
  duration: PropertyQueryResult,
  volume: PropertyQueryResult,
  muted: PropertyQueryResult,
}

impl PropertySample {
  fn into_sample(self, connected: bool) -> PlayerStateSample {
    let paused = match self.paused {
      Ok(PropertyValue::Bool(paused)) => Some(paused),
      Ok(_) => None,
      Err(error) => {
        log::warn!("Failed to get pause property: {error}");
        None
      }
    };
    let time_pos = match self.time_pos {
      Ok(PropertyValue::Number(position)) if position.is_finite() => Some(position),
      Ok(_) => None,
      Err(error) => {
        log::warn!("Failed to get time-pos property: {error}");
        None
      }
    };
    let duration = match self.duration {
      Ok(PropertyValue::Number(duration)) if duration.is_finite() => Some(duration),
      Ok(_) => None,
      Err(error) => {
        log::warn!("Failed to get duration property: {error}");
        None
      }
    };
    let volume = match self.volume {
      Ok(PropertyValue::Number(volume)) if volume.is_finite() => Some(volume.clamp(0.0, 100.0)),
      Ok(_) => None,
      Err(error) => {
        log::warn!("Failed to get volume property: {error}");
        None
      }
    };
    let muted = match self.muted {
      Ok(PropertyValue::Bool(muted)) => Some(muted),
      Ok(_) => None,
      Err(error) => {
        log::warn!("Failed to get mute property: {error}");
        None
      }
    };

    PlayerStateSample {
      connected,
      paused,
      muted,
      time_pos,
      duration,
      volume,
    }
  }
}

/// Collect a partial state sample directly from MPV properties.
pub async fn collect_player_state_sample(mpv: &MpvClient) -> PlayerStateSample {
  if !mpv.is_connected() {
    return PlayerStateSample::disconnected();
  }

  let (paused_res, time_pos_res, duration_res, volume_res, muted_res) = tokio::join!(
    mpv.get_property("pause"),
    mpv.get_property("time-pos"),
    mpv.get_property("duration"),
    mpv.get_property("volume"),
    mpv.get_property("mute"),
  );

  let sample = PropertySample {
    paused: paused_res,
    time_pos: time_pos_res,
    duration: duration_res,
    volume: volume_res,
    muted: muted_res,
  };
  sample.into_sample(mpv.is_connected())
}

/// Collect the current state directly from MPV properties.
///
/// Callers that retain transport state should prefer
/// [`collect_player_state_sample`] and merge it over their last coherent value.
pub async fn collect_player_state(mpv: &MpvClient) -> PlayerState {
  collect_player_state_sample(mpv)
    .await
    .merge(&PlayerState::default())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn successful_property_sample() -> PropertySample {
    PropertySample {
      paused: Ok(PropertyValue::Bool(false)),
      time_pos: Ok(PropertyValue::Number(42.5)),
      duration: Ok(PropertyValue::Number(1420.0)),
      volume: Ok(PropertyValue::Number(64.0)),
      muted: Ok(PropertyValue::Bool(false)),
    }
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

  #[test]
  fn property_sample_preserves_successful_fields_when_one_property_is_unavailable() {
    let mut sample = successful_property_sample();
    sample.duration = Err(MpvError::CommandFailed);
    let previous = PlayerState {
      connected: true,
      paused: true,
      muted: true,
      time_pos: 5.0,
      duration: 900.0,
      volume: 12.0,
    };

    let player = sample.into_sample(true).merge(&previous);

    assert!(player.connected);
    assert!(!player.paused);
    assert_eq!(player.time_pos, 42.5);
    assert_eq!(player.duration, 900.0);
    assert_eq!(player.volume, 64.0);
  }

  #[test]
  fn disconnected_sample_does_not_project_partial_property_values() {
    let mut sample = successful_property_sample();
    sample.duration = Err(MpvError::IpcTimeout);

    let player = sample.into_sample(false).merge(&PlayerState {
      connected: true,
      paused: false,
      muted: true,
      time_pos: 42.0,
      duration: 100.0,
      volume: 50.0,
    });

    assert!(!player.connected);
    assert_eq!(player.time_pos, 0.0);
  }

  #[test]
  fn property_sample_marks_state_connected_when_every_query_succeeds() {
    let sample = successful_property_sample().into_sample(true);
    let player = sample.merge(&PlayerState::default());

    assert!(player.connected);
  }
}
