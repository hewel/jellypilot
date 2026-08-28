use std::fmt;

use iced::widget::{button, column, container, image, pick_list, row, slider, space, text, Column};
use iced::{Alignment, ContentFit, Element, Fill, Length};
use jellypilot_mpv::playback::{Playable, TrackInfo};
use jellypilot_mpv::playback_session::{
  AdjacentAvailability, AdjacentDirection, PlaybackIntent, TracksView,
};
use jellypilot_mpv::player::format_duration;
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};

use crate::app::message::{HomeMessage, Message, PlaybackMessage};
use crate::app::state::{ArtworkCellState, Destination, State};

#[derive(Clone, Debug, Eq, PartialEq)]
struct TrackChoice {
  id: Option<i64>,
  label: String,
}

impl fmt::Display for TrackChoice {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.label)
  }
}

pub fn bar(state: &State) -> Option<Element<'_, Message>> {
  let now_playing = state.playback_view.now_playing.as_ref()?;
  let duration = now_playing
    .duration_seconds
    .filter(|duration| duration.is_finite() && *duration > 0.0);
  let position = state.seek_preview.unwrap_or(now_playing.position_seconds);
  let metadata = column![
    text(&now_playing.item.title)
      .font(SPACE_GROTESK_FONT)
      .size(18)
      .color(TOKENS.colors.onSurface),
    text(playback_caption(state))
      .size(12)
      .color(TOKENS.colors.onSurfaceVariant),
  ]
  .spacing(TOKENS.spacing.s0_5)
  .width(Length::FillPortion(2));
  let controls = compact_transport(state);
  let open = button(text("Tracks & Now Playing"))
    .padding([8, 12])
    .on_press(Message::Home(HomeMessage::Navigate(
      Destination::NowPlaying,
    )))
    .style(|theme, status| {
      jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
    });
  let volume = slider(
    0.0..=100.0,
    state.volume_preview.unwrap_or(now_playing.volume),
    |value| Message::Playback(PlaybackMessage::VolumeChanged(value)),
  )
  .on_release(Message::Playback(PlaybackMessage::VolumeReleased))
  .step(1.0)
  .width(120);
  let top = row![
    playback_artwork(state, 56.0, 56.0),
    metadata,
    controls,
    text(if now_playing.muted { "Muted" } else { "Volume" })
      .size(12)
      .color(TOKENS.colors.onSurfaceVariant),
    volume,
    open,
  ]
  .spacing(TOKENS.spacing.s3)
  .align_y(Alignment::Center)
  .width(Fill);

  let mut content = Column::new().spacing(TOKENS.spacing.s2).push(top);
  if let Some(duration) = duration {
    content = content.push(
      row![
        text(format_duration(position)).size(11),
        slider(0.0..=duration, position, |value| {
          Message::Playback(PlaybackMessage::SeekChanged(value))
        })
        .on_release(Message::Playback(PlaybackMessage::SeekReleased))
        .step(1.0)
        .width(Fill),
        text(format_duration(duration)).size(11),
      ]
      .spacing(TOKENS.spacing.s2)
      .align_y(Alignment::Center),
    );
  }

  Some(
    container(content)
      .padding([TOKENS.spacing.s2, TOKENS.spacing.s3])
      .width(Fill)
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Elevated))
      .into(),
  )
}

pub fn page(state: &State) -> Element<'_, Message> {
  let Some(now_playing) = state.playback_view.now_playing.as_ref() else {
    return container(text("No active playback."))
      .width(Fill)
      .height(Fill)
      .center_x(Fill)
      .center_y(Fill)
      .into();
  };
  let duration = now_playing
    .duration_seconds
    .filter(|duration| duration.is_finite() && *duration > 0.0);
  let position = state.seek_preview.unwrap_or(now_playing.position_seconds);
  let volume = state.volume_preview.unwrap_or(now_playing.volume);

  let header = row![
    playback_artwork(state, 180.0, 180.0),
    column![
      text("Now Playing").size(13).color(TOKENS.colors.primary),
      text(&now_playing.item.title)
        .font(SPACE_GROTESK_FONT)
        .size(36)
        .color(TOKENS.colors.onSurface),
      text(playback_caption(state))
        .size(15)
        .color(TOKENS.colors.onSurfaceVariant),
    ]
    .spacing(TOKENS.spacing.s2)
    .width(Fill),
  ]
  .spacing(TOKENS.spacing.s6)
  .align_y(Alignment::Center);

  let timeline: Element<'_, Message> = if let Some(duration) = duration {
    column![
      row![
        text(format_duration(position)).size(12),
        space::horizontal(),
        text(format_duration(duration)).size(12),
      ],
      slider(0.0..=duration, position, |value| {
        Message::Playback(PlaybackMessage::SeekChanged(value))
      })
      .on_release(Message::Playback(PlaybackMessage::SeekReleased))
      .step(1.0),
    ]
    .spacing(TOKENS.spacing.s2)
    .into()
  } else {
    text("Timeline unavailable")
      .size(13)
      .color(TOKENS.colors.onSurfaceVariant)
      .into()
  };

  let volume_controls = row![
    button(text(if now_playing.muted { "Unmute" } else { "Mute" }))
      .padding([8, 12])
      .on_press_maybe((!state.playback_view.busy).then_some(Message::Playback(
        PlaybackMessage::Intent(PlaybackIntent::SetMuted(!now_playing.muted)),
      )))
      .style(|theme, status| {
        jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
      }),
    text("Volume")
      .size(13)
      .color(TOKENS.colors.onSurfaceVariant),
    slider(0.0..=100.0, volume, |value| {
      Message::Playback(PlaybackMessage::VolumeChanged(value))
    })
    .on_release(Message::Playback(PlaybackMessage::VolumeReleased))
    .step(1.0)
    .width(Fill),
    text(format!("{volume:.0}%")).size(12),
  ]
  .spacing(TOKENS.spacing.s3)
  .align_y(Alignment::Center);

  let content = column![
    header,
    timeline,
    full_transport(state),
    volume_controls,
    track_controls(state),
  ]
  .spacing(TOKENS.spacing.s6)
  .padding([TOKENS.spacing.s8, TOKENS.spacing.s10])
  .width(Fill);

  container(content)
    .width(Fill)
    .height(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Filled))
    .into()
}

fn compact_transport(state: &State) -> Element<'_, Message> {
  let Some(now_playing) = state.playback_view.now_playing.as_ref() else {
    return space::horizontal().width(0).into();
  };
  row![
    adjacent_button(state, AdjacentDirection::Previous, "Previous"),
    button(text(if now_playing.paused { "Play" } else { "Pause" }))
      .padding([8, 13])
      .on_press_maybe((!state.playback_view.busy).then_some(Message::Playback(
        PlaybackMessage::Intent(PlaybackIntent::TogglePaused),
      )))
      .style(|theme, status| {
        jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Primary)
      }),
    adjacent_button(state, AdjacentDirection::Next, "Next"),
  ]
  .spacing(TOKENS.spacing.s1)
  .align_y(Alignment::Center)
  .into()
}

fn full_transport(state: &State) -> Element<'_, Message> {
  let Some(now_playing) = state.playback_view.now_playing.as_ref() else {
    return space::horizontal().width(0).into();
  };
  row![
    adjacent_button(state, AdjacentDirection::Previous, "Previous episode"),
    button(text(if now_playing.paused { "Play" } else { "Pause" }))
      .padding([11, 20])
      .on_press_maybe((!state.playback_view.busy).then_some(Message::Playback(
        PlaybackMessage::Intent(PlaybackIntent::TogglePaused),
      )))
      .style(|theme, status| {
        jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Primary)
      }),
    button(text("Stop"))
      .padding([11, 18])
      .on_press_maybe((!state.playback_view.busy).then_some(Message::Playback(
        PlaybackMessage::Intent(PlaybackIntent::Stop),
      )))
      .style(|theme, status| {
        jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
      }),
    adjacent_button(state, AdjacentDirection::Next, "Next episode"),
  ]
  .spacing(TOKENS.spacing.s2)
  .align_y(Alignment::Center)
  .into()
}

fn adjacent_button<'a>(
  state: &State,
  direction: AdjacentDirection,
  label: &'a str,
) -> Element<'a, Message> {
  let availability = match direction {
    AdjacentDirection::Previous => &state.playback_view.adjacent.previous,
    AdjacentDirection::Next => &state.playback_view.adjacent.next,
  };
  button(text(label))
    .padding([8, 12])
    .on_press_maybe(
      (matches!(availability, AdjacentAvailability::Available { .. }) && !state.playback_view.busy)
        .then_some(Message::Playback(PlaybackMessage::Intent(
          PlaybackIntent::PlayAdjacent(direction),
        ))),
    )
    .style(|theme, status| {
      jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
    })
    .into()
}

fn track_controls(state: &State) -> Element<'_, Message> {
  let TracksView::Ready {
    tracks,
    audio,
    subtitle,
  } = &state.playback_view.tracks
  else {
    return text(match &state.playback_view.tracks {
      TracksView::Loading => "Loading audio and subtitle tracks…",
      TracksView::Unavailable => "Audio and subtitle tracks are unavailable.",
      TracksView::Ready { .. } => "Audio and subtitle tracks are ready.",
    })
    .size(13)
    .color(TOKENS.colors.onSurfaceVariant)
    .into();
  };

  let audio_options = track_choices(tracks, "audio", false);
  let selected_audio = audio_options
    .iter()
    .find(|choice| choice.id == *audio)
    .cloned();
  let subtitle_options = track_choices(tracks, "sub", true);
  let selected_subtitle = subtitle_options
    .iter()
    .find(|choice| choice.id == *subtitle)
    .cloned();
  let audio_picker = pick_list(audio_options, selected_audio, |choice| {
    Message::Playback(PlaybackMessage::Intent(PlaybackIntent::SelectAudioTrack(
      choice.id.unwrap_or_default(),
    )))
  })
  .placeholder("No audio tracks")
  .width(Fill);
  let subtitle_picker = pick_list(subtitle_options, selected_subtitle, |choice| {
    Message::Playback(PlaybackMessage::Intent(
      PlaybackIntent::SelectSubtitleTrack(choice.id),
    ))
  })
  .placeholder("Subtitles off")
  .width(Fill);

  row![
    column![
      text("Audio").size(12).color(TOKENS.colors.onSurfaceVariant),
      audio_picker,
    ]
    .spacing(TOKENS.spacing.s1)
    .width(Length::FillPortion(1)),
    column![
      text("Subtitles")
        .size(12)
        .color(TOKENS.colors.onSurfaceVariant),
      subtitle_picker,
    ]
    .spacing(TOKENS.spacing.s1)
    .width(Length::FillPortion(1)),
  ]
  .spacing(TOKENS.spacing.s4)
  .into()
}

fn track_choices(tracks: &[TrackInfo], track_type: &str, include_off: bool) -> Vec<TrackChoice> {
  let mut choices = Vec::with_capacity(tracks.len() + usize::from(include_off));
  if include_off {
    choices.push(TrackChoice {
      id: None,
      label: "Off".to_owned(),
    });
  }
  choices.extend(
    tracks
      .iter()
      .filter(|track| track.track_type == track_type)
      .map(|track| TrackChoice {
        id: Some(track.id),
        label: track_label(track),
      }),
  );
  choices
}

fn track_label(track: &TrackInfo) -> String {
  match (track.title.as_deref(), track.language.as_deref()) {
    (Some(title), Some(language)) => format!("{title} · {language}"),
    (Some(title), None) => title.to_owned(),
    (None, Some(language)) => language.to_owned(),
    (None, None) => format!("Track {}", track.id),
  }
}

fn playback_caption(state: &State) -> String {
  let Some(playable) = state.playback_playable.as_ref() else {
    return state
      .playback_view
      .now_playing
      .as_ref()
      .map(|view| view.item.item_type.clone())
      .unwrap_or_default();
  };
  match playable {
    Playable::Library(item) => media_caption(
      &item.item_type,
      item.series_name.as_deref(),
      item.season_number,
      item.episode_number,
    ),
    Playable::Detail(item) => media_caption(
      &item.item_type,
      item.series_name.as_deref(),
      item.season_number,
      item.episode_number,
    ),
    Playable::Media(item) => media_caption(
      &item.item_type,
      item.series_name.as_deref(),
      item.parent_index_number,
      item.index_number,
    ),
  }
}

fn media_caption(
  item_type: &str,
  series_name: Option<&str>,
  season: Option<i32>,
  episode: Option<i32>,
) -> String {
  match (series_name, season, episode) {
    (Some(series), Some(season), Some(episode)) => {
      format!("{series} · S{season:02}E{episode:02}")
    }
    (Some(series), _, _) => format!("{series} · {item_type}"),
    _ => item_type.to_owned(),
  }
}

fn playback_artwork(state: &State, width: f32, height: f32) -> Element<'_, Message> {
  if let Some(cell) = &state.playback_artwork {
    if cell.state == ArtworkCellState::Ready {
      if let Some(handle) = state.artwork_handles.get(cell.slot, &cell.image_id) {
        return container(
          image(handle.clone())
            .content_fit(ContentFit::Cover)
            .width(Fill)
            .height(Fill),
        )
        .width(width)
        .height(height)
        .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Elevated))
        .into();
      }
    }
  }
  container(text("♪").font(SPACE_GROTESK_FONT).size(28))
    .width(width)
    .height(height)
    .center_x(Fill)
    .center_y(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Elevated))
    .into()
}
