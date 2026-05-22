//! Jellyfin Play request resolution for the playback target session.

use super::types::*;

/// User preferences and feature flags that affect Play request resolution.
pub struct PlayResolutionConfig<'a> {
  pub preferred_subtitle_languages: &'a [String],
  pub intro_skipper_enabled: bool,
}

/// Resolved Jellyfin and MPV playback choices for a Play request.
pub struct PlayResolution<'a> {
  pub audio_stream_index: Option<i32>,
  pub subtitle_stream_index: Option<i32>,
  pub mpv_audio_index: Option<i32>,
  pub mpv_subtitle_index: Option<i32>,
  pub external_subtitle_stream: Option<&'a MediaStream>,
  pub start_position: f64,
  pub position_ticks: i64,
  pub play_method: &'static str,
  pub should_fetch_intro_skipper_ranges: bool,
}

/// Resolve the local playback choices for a Jellyfin Play request.
pub fn resolve_play_request<'a>(
  request: &PlayRequest,
  item: &MediaItem,
  playback_info: &PlaybackInfoResponse,
  media_source: &'a MediaSource,
  series_preference: Option<&TrackPreference>,
  config: PlayResolutionConfig<'_>,
) -> PlayResolution<'a> {
  let mut audio_index = request.audio_stream_index;
  if audio_index.is_none() {
    if let Some(pref) = series_preference {
      if let Some(ref lang) = pref.audio_language {
        if let Some(idx) = find_stream_by_preference(
          &media_source.media_streams,
          "Audio",
          lang,
          pref.audio_title.as_deref(),
        ) {
          audio_index = Some(idx);
        }
      }
    }
  }

  let subtitle_index = select_subtitle_stream_index(
    request.subtitle_stream_index,
    series_preference,
    &media_source.media_streams,
    config.preferred_subtitle_languages,
  );

  let external_subtitle_stream = subtitle_index.and_then(|idx| {
    if idx < 0 {
      None
    } else {
      media_source.media_streams.iter().find(|stream| {
        stream.stream_type == "Subtitle" && stream.index == idx && stream.is_external
      })
    }
  });

  let mpv_audio_index = audio_index.map(|idx| {
    if idx < 0 {
      idx
    } else {
      jellyfin_to_mpv_track_index(&media_source.media_streams, "Audio", idx)
    }
  });

  let mpv_subtitle_index = if external_subtitle_stream.is_some() {
    None
  } else {
    subtitle_index.map(|idx| {
      if idx < 0 {
        idx
      } else {
        jellyfin_to_mpv_track_index(&media_source.media_streams, "Subtitle", idx)
      }
    })
  };

  PlayResolution {
    audio_stream_index: audio_index,
    subtitle_stream_index: subtitle_index,
    mpv_audio_index,
    mpv_subtitle_index,
    external_subtitle_stream,
    start_position: request
      .start_position_ticks
      .map(ticks_to_seconds)
      .unwrap_or(0.0),
    position_ticks: request.start_position_ticks.unwrap_or(0),
    play_method: play_method(media_source),
    should_fetch_intro_skipper_ranges: config.intro_skipper_enabled
      && item.item_type == "Episode"
      && playback_info.play_session_id.is_some(),
  }
}

/// Convert Jellyfin stream index to MPV track index.
/// Jellyfin uses absolute indices across all streams; MPV uses 1-based indices per track type.
pub fn jellyfin_to_mpv_track_index(
  streams: &[MediaStream],
  stream_type: &str,
  jellyfin_index: i32,
) -> i32 {
  let mut mpv_index = 0;
  for stream in streams {
    if stream.stream_type == stream_type {
      mpv_index += 1;
      if stream.index == jellyfin_index {
        return mpv_index;
      }
    }
  }
  1
}

fn play_method(media_source: &MediaSource) -> &'static str {
  if media_source.supports_direct_play {
    "DirectPlay"
  } else if media_source.supports_direct_stream {
    "DirectStream"
  } else {
    "Transcode"
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn request(audio: Option<i32>, subtitle: Option<i32>) -> PlayRequest {
    PlayRequest {
      item_ids: vec!["item-1".into()],
      start_position_ticks: Some(50_000_000),
      play_command: "PlayNow".into(),
      media_source_id: None,
      audio_stream_index: audio,
      subtitle_stream_index: subtitle,
    }
  }

  fn item(item_type: &str) -> MediaItem {
    MediaItem {
      id: "item-1".into(),
      name: "Episode".into(),
      item_type: item_type.into(),
      series_id: Some("series-1".into()),
      series_name: Some("Series".into()),
      season_name: Some("Season 1".into()),
      index_number: Some(1),
      parent_index_number: Some(1),
      run_time_ticks: None,
      overview: None,
    }
  }

  fn stream(index: i32, stream_type: &str, language: Option<&str>) -> MediaStream {
    MediaStream {
      index,
      stream_type: stream_type.into(),
      codec: None,
      language: language.map(str::to_string),
      display_title: None,
      is_default: false,
      is_external: false,
    }
  }

  fn external_subtitle(index: i32, language: &str) -> MediaStream {
    MediaStream {
      is_external: true,
      codec: Some("srt".into()),
      ..stream(index, "Subtitle", Some(language))
    }
  }

  fn media_source(streams: Vec<MediaStream>) -> MediaSource {
    MediaSource {
      id: "source-1".into(),
      path: None,
      protocol: "Http".into(),
      container: None,
      run_time_ticks: None,
      media_streams: streams,
      supports_direct_play: true,
      supports_direct_stream: false,
      supports_transcoding: false,
    }
  }

  fn playback_info() -> PlaybackInfoResponse {
    PlaybackInfoResponse {
      media_sources: Vec::new(),
      play_session_id: Some("play-1".into()),
    }
  }

  fn pref(audio_language: Option<&str>, subtitle_language: Option<&str>) -> TrackPreference {
    TrackPreference {
      audio_language: audio_language.map(str::to_string),
      audio_title: None,
      subtitle_language: subtitle_language.map(str::to_string),
      subtitle_title: None,
      subtitle_preference_set: subtitle_language.is_some(),
      is_subtitle_enabled: subtitle_language.is_some(),
    }
  }

  fn resolve<'a>(
    request: &PlayRequest,
    item: &MediaItem,
    playback_info: &PlaybackInfoResponse,
    media_source: &'a MediaSource,
    series_preference: Option<&TrackPreference>,
    preferred_subtitle_languages: &'a [String],
    intro_skipper_enabled: bool,
  ) -> PlayResolution<'a> {
    resolve_play_request(
      request,
      item,
      playback_info,
      media_source,
      series_preference,
      PlayResolutionConfig {
        preferred_subtitle_languages,
        intro_skipper_enabled,
      },
    )
  }

  #[test]
  fn explicit_track_choices_take_precedence_over_saved_and_global_preferences() {
    let source = media_source(vec![
      stream(0, "Video", None),
      stream(1, "Audio", Some("eng")),
      stream(2, "Audio", Some("jpn")),
      stream(3, "Subtitle", Some("eng")),
      stream(4, "Subtitle", Some("jpn")),
    ]);
    let request = request(Some(1), Some(3));
    let item = item("Episode");
    let playback_info = playback_info();
    let series_pref = pref(Some("jpn"), Some("jpn"));
    let global = vec!["jpn".into()];

    let resolution = resolve(
      &request,
      &item,
      &playback_info,
      &source,
      Some(&series_pref),
      &global,
      true,
    );

    assert_eq!(resolution.audio_stream_index, Some(1));
    assert_eq!(resolution.subtitle_stream_index, Some(3));
  }

  #[test]
  fn saved_series_preferences_take_precedence_over_global_subtitle_preferences() {
    let source = media_source(vec![
      stream(1, "Audio", Some("eng")),
      stream(2, "Audio", Some("jpn")),
      stream(3, "Subtitle", Some("eng")),
      stream(4, "Subtitle", Some("jpn")),
    ]);
    let request = request(None, None);
    let item = item("Episode");
    let playback_info = playback_info();
    let series_pref = pref(Some("jpn"), Some("jpn"));
    let global = vec!["eng".into()];

    let resolution = resolve(
      &request,
      &item,
      &playback_info,
      &source,
      Some(&series_pref),
      &global,
      true,
    );

    assert_eq!(resolution.audio_stream_index, Some(2));
    assert_eq!(resolution.subtitle_stream_index, Some(4));
  }

  #[test]
  fn global_subtitle_preferences_apply_when_request_and_series_do_not_select_subtitles() {
    let source = media_source(vec![
      stream(1, "Audio", Some("eng")),
      stream(2, "Subtitle", Some("eng")),
    ]);
    let request = request(None, None);
    let item = item("Episode");
    let playback_info = playback_info();
    let global = vec!["eng".into()];

    let resolution = resolve(
      &request,
      &item,
      &playback_info,
      &source,
      None,
      &global,
      true,
    );

    assert_eq!(resolution.subtitle_stream_index, Some(2));
  }

  #[test]
  fn external_subtitle_selection_uses_external_action_not_internal_mpv_track() {
    let source = media_source(vec![
      stream(1, "Audio", Some("eng")),
      external_subtitle(2, "eng"),
    ]);
    let request = request(None, Some(2));
    let item = item("Episode");
    let playback_info = playback_info();

    let resolution = resolve(&request, &item, &playback_info, &source, None, &[], true);

    assert!(resolution.external_subtitle_stream.is_some());
    assert_eq!(resolution.mpv_subtitle_index, None);
  }

  #[test]
  fn jellyfin_stream_indices_convert_to_mpv_type_local_indices() {
    let source = media_source(vec![
      stream(0, "Video", None),
      stream(3, "Audio", Some("eng")),
      stream(5, "Audio", Some("jpn")),
      stream(7, "Subtitle", Some("eng")),
      stream(9, "Subtitle", Some("jpn")),
    ]);
    let request = request(Some(5), Some(9));
    let item = item("Episode");
    let playback_info = playback_info();

    let resolution = resolve(&request, &item, &playback_info, &source, None, &[], true);

    assert_eq!(resolution.mpv_audio_index, Some(2));
    assert_eq!(resolution.mpv_subtitle_index, Some(2));
  }

  #[test]
  fn intro_skipper_ranges_are_requested_only_for_enabled_episode_playback() {
    let source = media_source(vec![stream(1, "Audio", Some("eng"))]);
    let request = request(None, None);
    let episode = item("Episode");
    let playback_info = playback_info();

    let enabled = resolve(&request, &episode, &playback_info, &source, None, &[], true);
    let disabled = resolve(
      &request,
      &episode,
      &playback_info,
      &source,
      None,
      &[],
      false,
    );
    let movie = item("Movie");
    let non_episode = resolve(&request, &movie, &playback_info, &source, None, &[], true);

    assert!(enabled.should_fetch_intro_skipper_ranges);
    assert!(!disabled.should_fetch_intro_skipper_ranges);
    assert!(!non_episode.should_fetch_intro_skipper_ranges);
  }
}
