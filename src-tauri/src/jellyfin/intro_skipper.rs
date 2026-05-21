//! Intro Skipper plugin range parsing and skip decisions.

use serde::Deserialize;
use std::collections::HashMap;

const LOOKAHEAD_SECONDS: f64 = 1.0;

/// Intro Skipper segment kind supported by JMSR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntroSkipKind {
  Introduction,
  Credits,
}

/// Active Intro Skipper range for the current playback session.
#[derive(Debug, Clone, PartialEq)]
pub struct IntroSkipRange {
  pub kind: IntroSkipKind,
  pub start_seconds: f64,
  pub end_seconds: f64,
  pub skipped: bool,
}

impl IntroSkipRange {
  fn new(kind: IntroSkipKind, start_seconds: f64, end_seconds: f64) -> Option<Self> {
    if !start_seconds.is_finite()
      || !end_seconds.is_finite()
      || start_seconds < 0.0
      || end_seconds <= start_seconds
    {
      return None;
    }

    Some(Self {
      kind,
      start_seconds,
      end_seconds,
      skipped: false,
    })
  }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct IntroSkipperPluginSegment {
  pub start: f64,
  pub end: f64,
}

pub type IntroSkipperPluginResponse = HashMap<String, IntroSkipperPluginSegment>;

/// Parse valid Introduction ranges from the Intro Skipper plugin response.
pub fn parse_intro_skipper_ranges(response: IntroSkipperPluginResponse) -> Vec<IntroSkipRange> {
  response
    .into_iter()
    .filter_map(|(kind, segment)| match kind.as_str() {
      "Introduction" => {
        IntroSkipRange::new(IntroSkipKind::Introduction, segment.start, segment.end)
      }
      "Credits" => IntroSkipRange::new(IntroSkipKind::Credits, segment.start, segment.end),
      _ => None,
    })
    .collect()
}

/// Return the seek target when playback is inside a skippable range.
pub fn evaluate_skip(position_seconds: f64, ranges: &mut [IntroSkipRange]) -> Option<f64> {
  if !position_seconds.is_finite() {
    return None;
  }

  ranges
    .iter_mut()
    .find(|range| {
      !range.skipped
        && position_seconds >= range.start_seconds - LOOKAHEAD_SECONDS
        && position_seconds < range.end_seconds
    })
    .map(|range| {
      range.skipped = true;
      range.end_seconds
    })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn intro_range(start_seconds: f64, end_seconds: f64) -> IntroSkipRange {
    range(IntroSkipKind::Introduction, start_seconds, end_seconds)
  }

  fn credit_range(start_seconds: f64, end_seconds: f64) -> IntroSkipRange {
    range(IntroSkipKind::Credits, start_seconds, end_seconds)
  }

  fn range(kind: IntroSkipKind, start_seconds: f64, end_seconds: f64) -> IntroSkipRange {
    IntroSkipRange {
      kind,
      start_seconds,
      end_seconds,
      skipped: false,
    }
  }

  fn plugin_segment(start: f64, end: f64) -> IntroSkipperPluginSegment {
    IntroSkipperPluginSegment { start, end }
  }

  #[test]
  fn parses_valid_introduction_range() {
    let response = HashMap::from([("Introduction".to_string(), plugin_segment(12.5, 82.0))]);

    let ranges = parse_intro_skipper_ranges(response);

    assert_eq!(ranges, vec![intro_range(12.5, 82.0)]);
  }

  #[test]
  fn parses_valid_credit_range() {
    let response = HashMap::from([("Credits".to_string(), plugin_segment(1200.0, 1260.0))]);

    let ranges = parse_intro_skipper_ranges(response);

    assert_eq!(ranges, vec![credit_range(1200.0, 1260.0)]);
  }

  #[test]
  fn ignores_invalid_and_unsupported_ranges() {
    let response = HashMap::from([
      ("Introduction".to_string(), plugin_segment(90.0, 80.0)),
      ("Preview".to_string(), plugin_segment(0.0, 30.0)),
      ("Recap".to_string(), plugin_segment(1.0, 20.0)),
      ("Commercial".to_string(), plugin_segment(40.0, 70.0)),
      ("Unknown".to_string(), plugin_segment(10.0, 20.0)),
    ]);

    let ranges = parse_intro_skipper_ranges(response);

    assert!(ranges.is_empty());
  }

  #[test]
  fn empty_response_has_no_active_ranges() {
    let ranges = parse_intro_skipper_ranges(HashMap::new());

    assert!(ranges.is_empty());
  }

  #[test]
  fn first_entry_into_unskipped_range_returns_seek_target_once() {
    let mut ranges = vec![intro_range(10.0, 80.0)];

    assert_eq!(evaluate_skip(10.0, &mut ranges), Some(80.0));
    assert!(ranges[0].skipped);
    assert_eq!(evaluate_skip(10.5, &mut ranges), None);
  }

  #[test]
  fn resume_or_manual_seek_into_unskipped_range_still_skips() {
    let mut ranges = vec![intro_range(10.0, 80.0)];

    assert_eq!(evaluate_skip(42.0, &mut ranges), Some(80.0));
  }

  #[test]
  fn manual_seek_back_into_already_skipped_range_does_not_skip_again() {
    let mut ranges = vec![intro_range(10.0, 80.0)];

    assert_eq!(evaluate_skip(10.0, &mut ranges), Some(80.0));
    assert_eq!(evaluate_skip(20.0, &mut ranges), None);
  }

  #[test]
  fn new_playback_range_set_resets_skipped_state() {
    let mut first_session_ranges = vec![intro_range(10.0, 80.0)];
    assert_eq!(evaluate_skip(10.0, &mut first_session_ranges), Some(80.0));

    let mut next_session_ranges = vec![intro_range(10.0, 80.0)];

    assert_eq!(evaluate_skip(10.0, &mut next_session_ranges), Some(80.0));
  }

  #[test]
  fn returns_seek_target_inside_half_open_range() {
    let mut at_start = vec![intro_range(10.0, 80.0)];
    let mut before_end = vec![intro_range(10.0, 80.0)];
    let mut at_end = vec![intro_range(10.0, 80.0)];

    assert_eq!(evaluate_skip(10.0, &mut at_start), Some(80.0));
    assert_eq!(evaluate_skip(79.99, &mut before_end), Some(80.0));
    assert_eq!(evaluate_skip(80.0, &mut at_end), None);
  }

  #[test]
  fn returns_seek_target_inside_one_second_lookahead_window() {
    let mut inside_lookahead = vec![intro_range(10.0, 80.0)];
    let mut outside_lookahead = vec![intro_range(10.0, 80.0)];

    assert_eq!(evaluate_skip(9.0, &mut inside_lookahead), Some(80.0));
    assert_eq!(evaluate_skip(8.99, &mut outside_lookahead), None);
  }

  #[test]
  fn position_near_range_end_does_not_retrigger_after_skip() {
    let mut ranges = vec![intro_range(10.0, 80.0)];

    assert_eq!(evaluate_skip(79.5, &mut ranges), Some(80.0));
    assert_eq!(evaluate_skip(79.75, &mut ranges), None);
  }

  #[test]
  fn credit_range_uses_same_seek_rules() {
    let mut lookahead_ranges = vec![credit_range(1200.0, 1260.0)];
    let mut start_ranges = vec![credit_range(1200.0, 1260.0)];

    assert_eq!(evaluate_skip(1199.0, &mut lookahead_ranges), Some(1260.0));
    assert_eq!(evaluate_skip(1200.0, &mut start_ranges), Some(1260.0));
  }
}
