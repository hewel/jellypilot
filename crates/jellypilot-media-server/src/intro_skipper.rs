//! Intro Skipper plugin range parsing.

use serde::Deserialize;
use std::collections::HashMap;

/// Intro Skipper segment kind supported by JellyPilot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntroSkipKind {
  Introduction,
  Credits,
}

/// Validated Intro Skipper plugin segment data.
#[derive(Debug, Clone, PartialEq)]
pub struct IntroSkipRange {
  pub kind: IntroSkipKind,
  pub start_seconds: f64,
  pub end_seconds: f64,
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
    })
  }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct IntroSkipperPluginSegment {
  pub(crate) start: f64,
  pub(crate) end: f64,
}

pub(crate) type IntroSkipperPluginResponse = HashMap<String, IntroSkipperPluginSegment>;

/// Parse valid Introduction ranges from the Intro Skipper plugin response.
pub(crate) fn parse_intro_skipper_ranges(
  response: IntroSkipperPluginResponse,
) -> Vec<IntroSkipRange> {
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
  fn ignores_malformed_ranges_with_non_positive_or_reversed_bounds() {
    let response = HashMap::from([
      ("Introduction".to_string(), plugin_segment(-1.0, 80.0)),
      ("Credits".to_string(), plugin_segment(1200.0, 0.0)),
    ]);

    let ranges = parse_intro_skipper_ranges(response);

    assert!(ranges.is_empty());
  }

  #[test]
  fn empty_response_has_no_active_ranges() {
    let ranges = parse_intro_skipper_ranges(HashMap::new());

    assert!(ranges.is_empty());
  }
}
