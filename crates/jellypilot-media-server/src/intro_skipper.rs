//! Jellyfin media segments used by the Intro Skipper playback feature.

use crate::types::ticks_to_seconds;
use jellyfin_api::models::{MediaSegmentDtoQueryResult, MediaSegmentType};

/// Intro Skipper segment kind supported by JellyPilot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntroSkipKind {
  Introduction,
  Credits,
}

/// Validated intro or outro range from Jellyfin's native media segments.
#[derive(Debug, Clone, PartialEq)]
pub struct IntroSkipRange {
  pub kind: IntroSkipKind,
  pub start_seconds: f64,
  pub end_seconds: f64,
}

/// Preserve every supported range in server order, including repeated segment types.
pub(crate) fn parse_intro_skipper_ranges(
  response: MediaSegmentDtoQueryResult,
) -> Vec<IntroSkipRange> {
  response
    .items
    .unwrap_or_default()
    .into_iter()
    .filter_map(|segment| {
      let kind = match segment.r#type? {
        MediaSegmentType::Intro => IntroSkipKind::Introduction,
        MediaSegmentType::Outro => IntroSkipKind::Credits,
        _ => return None,
      };
      let start = segment.start_ticks?;
      let end = segment.end_ticks?;
      if start < 0 || end <= start {
        return None;
      }
      let start_seconds = ticks_to_seconds(start);
      let end_seconds = ticks_to_seconds(end);
      // Very large tick values can round to the same floating-point position.
      (end_seconds > start_seconds).then_some(IntroSkipRange {
        kind,
        start_seconds,
        end_seconds,
      })
    })
    .collect()
}
