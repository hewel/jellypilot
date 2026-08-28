use crate::playback::TrackInfo;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrackKind {
  Audio,
  Subtitle,
}

#[must_use]
pub fn selected_track_id(
  tracks: &[TrackInfo],
  kind: TrackKind,
  selected: u32,
) -> Option<Option<i64>> {
  let track_type = match kind {
    TrackKind::Audio => "audio",
    TrackKind::Subtitle => "sub",
  };
  if kind == TrackKind::Subtitle && selected == 0 {
    return Some(None);
  }
  let index = if kind == TrackKind::Subtitle {
    selected.checked_sub(1)?
  } else {
    selected
  };
  tracks
    .iter()
    .filter(|track| track.track_type == track_type)
    .nth(index as usize)
    .map(|track| Some(track.id))
}

#[must_use]
pub fn runtime_seconds_to_ticks(seconds: Option<f64>) -> Option<i64> {
  seconds
    .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
    .map(|seconds| (seconds * 10_000_000.0).round() as i64)
}

#[must_use]
pub fn format_duration(seconds: f64) -> String {
  let seconds = seconds.max(0.0).round() as u64;
  format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn track_selection_maps_filtered_rows_and_subtitle_off() {
    let tracks = vec![
      TrackInfo {
        id: 3,
        track_type: "audio".to_owned(),
        title: Some("English".to_owned()),
        language: Some("eng".to_owned()),
        selected: true,
      },
      TrackInfo {
        id: 8,
        track_type: "sub".to_owned(),
        title: Some("Spanish".to_owned()),
        language: Some("spa".to_owned()),
        selected: false,
      },
    ];

    assert_eq!(
      selected_track_id(&tracks, TrackKind::Audio, 0),
      Some(Some(3))
    );
    assert_eq!(
      selected_track_id(&tracks, TrackKind::Subtitle, 0),
      Some(None)
    );
    assert_eq!(
      selected_track_id(&tracks, TrackKind::Subtitle, 1),
      Some(Some(8))
    );
    assert_eq!(selected_track_id(&tracks, TrackKind::Audio, 1), None);
  }
}
