//! Focused MPV event decisions for playback target session handling.

use std::time::{Duration, Instant};

use super::types::{seconds_to_ticks, PlaybackSession};
use crate::playback_control::AdjacentDirection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyReportDecision {
  Ignore,
  ReportNow,
  ReportWhenThrottleElapsed,
}

pub fn property_report_decision(property_name: &str) -> PropertyReportDecision {
  match property_name {
    "pause" | "volume" | "mute" => PropertyReportDecision::ReportNow,
    "time-pos" => PropertyReportDecision::ReportWhenThrottleElapsed,
    _ => PropertyReportDecision::Ignore,
  }
}

pub fn should_report_progress(
  decision: PropertyReportDecision,
  now: Instant,
  last_report: Instant,
  report_interval: Duration,
) -> bool {
  match decision {
    PropertyReportDecision::Ignore => false,
    PropertyReportDecision::ReportNow => true,
    PropertyReportDecision::ReportWhenThrottleElapsed => {
      now.duration_since(last_report) >= report_interval
    }
  }
}

pub fn apply_property_update(
  playback: &mut PlaybackSession,
  property_name: &str,
  data: &serde_json::Value,
) {
  match property_name {
    "pause" => {
      if let Some(paused) = data.as_bool() {
        playback.is_paused = paused;
      }
    }
    "volume" => {
      if let Some(volume) = data.as_f64() {
        playback.volume = volume as i32;
      }
    }
    "mute" => {
      if let Some(muted) = data.as_bool() {
        playback.is_muted = muted;
      }
    }
    "time-pos" => {
      if let Some(position) = data.as_f64() {
        playback.position_ticks = seconds_to_ticks(position);
      }
    }
    _ => {}
  }
}

pub fn is_natural_end(reason: Option<&str>) -> bool {
  reason == Some("eof")
}

pub fn client_message_direction(args: &[String]) -> Option<AdjacentDirection> {
  match args.first().map(String::as_str) {
    Some("jellypilot-next") => Some(AdjacentDirection::Next),
    Some("jellypilot-prev") => Some(AdjacentDirection::Previous),
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::jellyfin::intro_skipper::IntroSkipRange;

  fn playback() -> PlaybackSession {
    PlaybackSession {
      item_id: "item-1".into(),
      media_source_id: Some("source-1".into()),
      play_session_id: Some("play-1".into()),
      intro_skipper_ranges: Vec::<IntroSkipRange>::new(),
      position_ticks: 0,
      is_paused: false,
      is_muted: false,
      volume: 100,
      audio_stream_index: None,
      subtitle_stream_index: None,
    }
  }

  #[test]
  fn pause_volume_mute_and_time_position_update_playback_session() {
    let mut playback = playback();

    apply_property_update(&mut playback, "pause", &serde_json::json!(true));
    apply_property_update(&mut playback, "volume", &serde_json::json!(42.9));
    apply_property_update(&mut playback, "mute", &serde_json::json!(true));
    apply_property_update(&mut playback, "time-pos", &serde_json::json!(12.5));

    assert!(playback.is_paused);
    assert_eq!(playback.volume, 42);
    assert!(playback.is_muted);
    assert_eq!(playback.position_ticks, 125_000_000);
  }

  #[test]
  fn progress_reporting_is_immediate_for_state_changes_and_throttled_for_time_position() {
    let now = Instant::now();
    let interval = Duration::from_secs(5);

    assert!(should_report_progress(
      property_report_decision("pause"),
      now,
      now,
      interval
    ));
    assert!(!should_report_progress(
      property_report_decision("time-pos"),
      now + Duration::from_secs(4),
      now,
      interval
    ));
    assert!(should_report_progress(
      property_report_decision("time-pos"),
      now + Duration::from_secs(5),
      now,
      interval
    ));
  }

  #[test]
  fn natural_end_and_keyboard_shortcuts_map_to_adjacent_playback_decisions() {
    assert!(is_natural_end(Some("eof")));
    assert!(!is_natural_end(Some("stop")));
    assert_eq!(
      client_message_direction(&["jellypilot-next".into()]),
      Some(AdjacentDirection::Next)
    );
    assert_eq!(
      client_message_direction(&["jellypilot-prev".into()]),
      Some(AdjacentDirection::Previous)
    );
    assert_eq!(client_message_direction(&["other".into()]), None);
  }
}
