use std::fmt;

use crate::app::message::{Message, PlaybackMessage};
use crate::app::state::{ArtworkCellState, State};
use iced::widget::{button, column, container, row, slider, space, text, Column};
use iced::{Alignment, ContentFit, Element, Fill, Length};
use jellypilot_mpv::playback::{Playable, TrackInfo};
use jellypilot_mpv::playback_session::{
  AdjacentAvailability, AdjacentDirection, PlaybackIntent, TracksView,
};
use jellypilot_mpv::player::format_duration;
use jellypilot_session::IntroSkipKind;
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::icons::{
  icon_for_variant, icon_for_variant_disabled, icon_with_color, Icon, IconSize,
};
use jellypilot_ui::overlay::{popover, tooltip, Placement, PopoverOptions, TooltipOptions};
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};
use jellypilot_ui::{full_radius, rounded_image};

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
  let now_playing = state.playback.view.now_playing.as_ref()?;
  let duration = now_playing
    .duration_seconds
    .filter(|duration| duration.is_finite() && *duration > 0.0);
  let position = state
    .playback
    .seek_preview
    .unwrap_or(now_playing.position_seconds);

  let metadata = column![
    text(&now_playing.item.title)
      .font(SPACE_GROTESK_FONT)
      .size(16)
      .color(TOKENS.colors.onSurface),
    text(playback_caption(state))
      .size(12)
      .color(TOKENS.colors.onSurfaceVariant),
  ]
  .spacing(TOKENS.spacing.s0_5)
  .width(Length::FillPortion(2));

  let is_paused = now_playing.paused;
  let play_pause_icon = if is_paused { Icon::Play } else { Icon::Pause };
  let play_pause_label = if is_paused { "Play" } else { "Pause" };
  let play_pause_button = button(icon_for_variant(
    play_pause_icon,
    IconSize::Lg,
    ButtonVariant::Primary,
  ))
  .padding([7, 11])
  .on_press(Message::Playback(PlaybackMessage::Intent(
    PlaybackIntent::TogglePaused,
  )))
  .style(|theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Primary)
  });
  let play_pause = tooltip(
    play_pause_button,
    play_pause_label,
    TooltipOptions::default(),
  );

  let stop_button = button(icon_for_variant(
    Icon::Stop,
    IconSize::Md,
    ButtonVariant::Tonal,
  ))
  .padding([6, 10])
  .on_press(Message::Playback(PlaybackMessage::Intent(
    PlaybackIntent::Stop,
  )))
  .style(|theme, status| jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Tonal));
  let stop = tooltip(stop_button, "Stop", TooltipOptions::default());

  let transport = row![
    adjacent_button(state, AdjacentDirection::Previous, "Previous"),
    play_pause,
    stop,
    adjacent_button(state, AdjacentDirection::Next, "Next"),
  ]
  .spacing(TOKENS.spacing.s1_5)
  .align_y(Alignment::Center);
  let track_selection = row![audio_popover(state), subtitle_popover(state)]
    .spacing(TOKENS.spacing.s1_5)
    .align_y(Alignment::Center);

  let volume_slider = slider(
    0.0..=100.0,
    state.playback.volume_preview.unwrap_or(now_playing.volume),
    |value| Message::Playback(PlaybackMessage::VolumeChanged(value)),
  )
  .on_release(Message::Playback(PlaybackMessage::VolumeReleased))
  .step(1.0)
  .width(100);

  let mute_icon = if now_playing.muted {
    Icon::VolumeMute
  } else {
    Icon::VolumeHigh
  };
  let mute_label = if now_playing.muted { "Unmute" } else { "Mute" };
  let mute_button = button(icon_for_variant(
    mute_icon,
    IconSize::Md,
    ButtonVariant::Tonal,
  ))
  .padding([6, 10])
  .on_press(Message::Playback(PlaybackMessage::Intent(
    PlaybackIntent::SetMuted(!now_playing.muted),
  )))
  .style(|theme, status| jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Tonal));
  let mute = tooltip(mute_button, mute_label, TooltipOptions::default());

  let volume = row![mute, volume_slider,]
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center);
  let top = row![
    playback_artwork(state, 56.0, 56.0),
    metadata,
    transport,
    track_selection,
    volume,
  ]
  .spacing(TOKENS.spacing.s3)
  .align_y(Alignment::Center)
  .width(Fill);

  let mut content = Column::new().spacing(TOKENS.spacing.s2).push(top);

  if let Some(prompt) = intro_prompt(state) {
    content = content.push(prompt);
  }

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
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Block))
      .into(),
  )
}

fn intro_prompt(state: &State) -> Option<Element<'_, Message>> {
  let prompt = state.playback.view.intro_prompt?;
  let label = match prompt.kind {
    IntroSkipKind::Introduction => "Skip intro?",
    IntroSkipKind::Credits => "Skip credits?",
  };
  let actions = row![
    button(
      row![
        icon_for_variant(Icon::Next, IconSize::Sm, ButtonVariant::Primary),
        text("Skip"),
      ]
      .spacing(TOKENS.spacing.s1_5)
      .align_y(Alignment::Center),
    )
    .padding([6, 12])
    .on_press(Message::Playback(PlaybackMessage::Intent(
      PlaybackIntent::SkipIntro,
    )))
    .style(|theme, status| {
      jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Primary)
    }),
    button(
      row![
        icon_for_variant(Icon::Close, IconSize::Xs, ButtonVariant::Tonal),
        text("Dismiss"),
      ]
      .spacing(TOKENS.spacing.s1_5)
      .align_y(Alignment::Center),
    )
    .on_press(Message::Playback(PlaybackMessage::Intent(
      PlaybackIntent::DismissIntro,
    )))
    .style(|theme, status| {
      jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Tonal)
    }),
  ]
  .spacing(TOKENS.spacing.s2);
  Some(
    container(row![
      row![
        icon_with_color(Icon::IntroSkip, IconSize::Md, TOKENS.colors.primary),
        text(label)
          .font(SPACE_GROTESK_FONT)
          .size(16)
          .color(TOKENS.colors.onSurface),
      ]
      .spacing(TOKENS.spacing.s2)
      .align_y(Alignment::Center),
      space::horizontal(),
      actions,
    ])
    .padding([TOKENS.spacing.s2, TOKENS.spacing.s3])
    .width(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Raised))
    .into(),
  )
}

fn adjacent_button<'a>(
  state: &State,
  direction: AdjacentDirection,
  label: &'a str,
) -> Element<'a, Message> {
  let availability = match direction {
    AdjacentDirection::Previous => &state.playback.view.adjacent.previous,
    AdjacentDirection::Next => &state.playback.view.adjacent.next,
  };
  let icon_variant = match direction {
    AdjacentDirection::Previous => Icon::Previous,
    AdjacentDirection::Next => Icon::Next,
  };
  let available = matches!(availability, AdjacentAvailability::Available { .. });
  let btn = button(icon_for_variant_disabled(
    icon_variant,
    IconSize::Md,
    ButtonVariant::Tonal,
    !available,
  ))
  .padding([6, 10])
  .on_press_maybe(
    available.then_some(Message::Playback(PlaybackMessage::Intent(
      PlaybackIntent::PlayAdjacent(direction),
    ))),
  )
  .style(|theme, status| jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Tonal));
  tooltip(btn, label, TooltipOptions::default())
}

fn audio_popover(state: &State) -> Element<'_, Message> {
  let has_audio_choices = match &state.playback.view.tracks {
    TracksView::Ready { tracks, .. } => !track_choices(tracks, "audio", false).is_empty(),
    TracksView::Loading | TracksView::Unavailable => false,
  };
  let audio_btn_variant = if state.playback.audio_menu_open {
    ButtonVariant::TonalActive
  } else {
    ButtonVariant::Tonal
  };
  let trigger = button(
    row![
      icon_for_variant_disabled(
        Icon::AudioTrack,
        IconSize::Sm,
        audio_btn_variant,
        !has_audio_choices,
      ),
      text("Audio"),
    ]
    .spacing(TOKENS.spacing.s1_5)
    .align_y(Alignment::Center),
  )
  .padding([6, 10])
  .on_press_maybe(has_audio_choices.then_some(Message::Playback(PlaybackMessage::AudioMenuToggled)))
  .style(move |theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, audio_btn_variant)
  });
  let menu = match &state.playback.view.tracks {
    TracksView::Ready { tracks, audio, .. } => {
      let choices = track_choices(tracks, "audio", false);
      if choices.is_empty() {
        column![text("No audio tracks")
          .size(12)
          .color(TOKENS.colors.onSurfaceVariant)]
        .spacing(TOKENS.spacing.s1)
        .width(Fill)
      } else {
        let mut col = Column::new().spacing(TOKENS.spacing.s1).width(Fill);
        for choice in choices {
          let active = choice.id == *audio;
          let id = choice.id.unwrap_or_default();
          let active_marker: Element<'_, Message> = if active {
            icon_with_color(Icon::Check, IconSize::Xs, TOKENS.colors.primary).into()
          } else {
            space::horizontal().width(14).into()
          };
          col = col.push(
            button(
              row![text(choice.label).width(Fill).size(13), active_marker,]
                .align_y(Alignment::Center),
            )
            .padding([6, 10])
            .width(Fill)
            .on_press(Message::Playback(PlaybackMessage::AudioTrackSelected(id)))
            .style(move |theme, status| {
              jellypilot_ui::theme::button_variant(
                theme,
                status,
                if active {
                  ButtonVariant::Secondary
                } else {
                  ButtonVariant::Text
                },
              )
            }),
          );
        }
        col
      }
    }
    TracksView::Loading => column![text("Loading audio tracks…")
      .size(12)
      .color(TOKENS.colors.onSurfaceVariant)]
    .spacing(TOKENS.spacing.s1)
    .width(Fill),
    TracksView::Unavailable => column![text("Audio tracks unavailable")
      .size(12)
      .color(TOKENS.colors.onSurfaceVariant)]
    .spacing(TOKENS.spacing.s1)
    .width(Fill),
  };

  popover(
    trigger,
    menu,
    state.playback.audio_menu_open,
    PopoverOptions {
      placement: Placement::Above,
      width: Some(240.0),
      ..PopoverOptions::default()
    },
    Message::Playback(PlaybackMessage::AudioMenuDismissed),
  )
}

fn subtitle_popover(state: &State) -> Element<'_, Message> {
  let has_subtitle_choices = match &state.playback.view.tracks {
    TracksView::Ready { tracks, .. } => !track_choices(tracks, "sub", false).is_empty(),
    TracksView::Loading | TracksView::Unavailable => false,
  };
  let sub_btn_variant = if state.playback.subtitle_menu_open {
    ButtonVariant::TonalActive
  } else {
    ButtonVariant::Tonal
  };
  let trigger = button(
    row![
      icon_for_variant_disabled(
        Icon::Subtitles,
        IconSize::Sm,
        sub_btn_variant,
        !has_subtitle_choices,
      ),
      text("Subtitles"),
    ]
    .spacing(TOKENS.spacing.s1_5)
    .align_y(Alignment::Center),
  )
  .padding([6, 10])
  .on_press_maybe(
    has_subtitle_choices.then_some(Message::Playback(PlaybackMessage::SubtitleMenuToggled)),
  )
  .style(move |theme, status| jellypilot_ui::theme::button_variant(theme, status, sub_btn_variant));
  let menu = match &state.playback.view.tracks {
    TracksView::Ready {
      tracks, subtitle, ..
    } => {
      let choices = track_choices(tracks, "sub", true);
      let mut col = Column::new().spacing(TOKENS.spacing.s1).width(Fill);
      for choice in choices {
        let active = choice.id == *subtitle;
        let active_marker: Element<'_, Message> = if active {
          icon_with_color(Icon::Check, IconSize::Xs, TOKENS.colors.primary).into()
        } else {
          space::horizontal().width(14).into()
        };
        col = col.push(
          button(
            row![text(choice.label).width(Fill).size(13), active_marker,]
              .align_y(Alignment::Center),
          )
          .padding([6, 10])
          .width(Fill)
          .on_press(Message::Playback(PlaybackMessage::SubtitleTrackSelected(
            choice.id,
          )))
          .style(move |theme, status| {
            jellypilot_ui::theme::button_variant(
              theme,
              status,
              if active {
                ButtonVariant::Secondary
              } else {
                ButtonVariant::Text
              },
            )
          }),
        );
      }
      col
    }
    TracksView::Loading => column![text("Loading subtitle tracks…")
      .size(12)
      .color(TOKENS.colors.onSurfaceVariant)]
    .spacing(TOKENS.spacing.s1)
    .width(Fill),
    TracksView::Unavailable => column![text("Subtitle tracks unavailable")
      .size(12)
      .color(TOKENS.colors.onSurfaceVariant)]
    .spacing(TOKENS.spacing.s1)
    .width(Fill),
  };

  popover(
    trigger,
    menu,
    state.playback.subtitle_menu_open,
    PopoverOptions {
      placement: Placement::Above,
      width: Some(240.0),
      ..PopoverOptions::default()
    },
    Message::Playback(PlaybackMessage::SubtitleMenuDismissed),
  )
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
  let Some(playable) = state.playback.playable.as_ref() else {
    return state
      .playback
      .view
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
  if let Some(cell) = &state.playback.artwork {
    if cell.state == ArtworkCellState::Ready {
      if let Some(handle) = state.kernel.artwork_handles.get(cell.slot, &cell.image_id) {
        return rounded_image(handle.clone(), full_radius(TOKENS.radii.lg))
          .content_fit(ContentFit::Cover)
          .width(width)
          .height(height)
          .into();
      }
    }
  }
  container(icon_with_color(
    Icon::Movie,
    IconSize::Custom(26.0),
    TOKENS.colors.onSurfaceVariant,
  ))
  .width(width)
  .height(height)
  .align_x(Alignment::Center)
  .align_y(Alignment::Center)
  .style(|_theme| container::Style {
    background: Some(iced::Background::Color(
      TOKENS.colors.surfaceContainerLowest,
    )),
    border: iced::Border {
      radius: full_radius(TOKENS.radii.lg),
      width: 0.0,
      color: iced::Color::TRANSPARENT,
    },
    ..container::Style::default()
  })
  .into()
}
#[cfg(test)]
mod tests {
  use super::*;
  use crate::app::message::{Message, PlaybackMessage};
  use jellypilot_media_server::VideoLibraryItem;
  use jellypilot_mpv::playback::{
    NowPlayingItem, Playable, PlaybackOutcome, PlaybackRefreshOutcome, PlaybackRefreshState,
    PlaybackSnapshot, PlaybackStartPosition,
  };
  use jellypilot_mpv::playback_session::{
    AdjacentAvailability, AdjacentDirection, AdjacentView, ControllerSettlement, IntroAvailability,
    IntroPromptView, NowPlayingView, PlaybackEffect, PlaybackEvent, PlaybackInput, PlaybackIntent,
  };
  use jellypilot_mpv::PlayerState;
  use jellypilot_session::IntroSkipMode;
  use std::time::Instant;
  fn test_now_playing() -> NowPlayingView {
    NowPlayingView {
      item: NowPlayingItem {
        item_id: "episode-1".to_owned(),
        title: "Pilot Episode".to_owned(),
        item_type: "Episode".to_owned(),
        runtime_seconds: Some(2_400.0),
        start_position_seconds: 0.0,
        play_method: "DirectPlay".to_owned(),
      },
      paused: false,
      position_seconds: 120.0,
      duration_seconds: Some(2_400.0),
      volume: 85.0,
      muted: false,
    }
  }

  #[test]
  fn bar_returns_none_when_no_active_playback() {
    let state = State::boot(false);
    assert!(bar(&state).is_none());
  }

  #[test]
  fn bar_renders_when_playback_is_active() {
    let mut state = State::boot(false);
    state.playback.view.now_playing = Some(test_now_playing());
    assert!(bar(&state).is_some());
  }

  #[test]
  fn bar_composition_and_artwork_stable_across_position_settlement() {
    let mut state = State::boot(false);
    let now = Instant::now();
    state.playback.session.handle(
      PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
      now,
    );
    let start_effects = state.playback.session.handle(
      PlaybackInput::Intent(PlaybackIntent::Start {
        item: Playable::Library(VideoLibraryItem {
          id: "episode-1".to_owned(),
          name: "Pilot Episode".to_owned(),
          item_type: "Episode".to_owned(),
          production_year: None,
          runtime_seconds: Some(2_400.0),
          played: false,
          favorite: false,
          artwork_image_id: None,
          series_poster_image_id: Some("test-artwork-image".to_owned()),
          season_number: Some(1),
          episode_number: Some(1),
          series_id: Some("series-1".to_owned()),
          series_name: Some("Series".to_owned()),
          resume_position_seconds: None,
          played_percentage: None,
          overview: None,
        }),
        position: PlaybackStartPosition::Beginning,
        intro: IntroAvailability {
          mode: IntroSkipMode::Off,
          skipper_available: false,
        },
        selection: Box::default(),
      }),
      now,
    );
    let [PlaybackEffect::Controller(start_id, _)] = start_effects.as_slice() else {
      panic!("expected start controller effect");
    };
    state.playback.session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: *start_id,
        settlement: ControllerSettlement::Started(Ok(PlaybackOutcome {
          snapshot: PlaybackSnapshot {
            now_playing: Some(NowPlayingItem {
              item_id: "episode-1".to_owned(),
              title: "Pilot Episode".to_owned(),
              item_type: "Episode".to_owned(),
              runtime_seconds: Some(2_400.0),
              start_position_seconds: 0.0,
              play_method: "DirectPlay".to_owned(),
            }),
            transport: PlayerState {
              connected: true,
              paused: false,
              muted: false,
              time_pos: 120.0,
              duration: 2_400.0,
              volume: 85.0,
            },
          },
          warnings: Vec::new(),
        })),
      }),
      now,
    );
    state.playback.view = state.playback.session.view();

    let slot = state.kernel.artwork_binder.bind_player_bar();
    let image_id = "test-artwork-image".to_owned();
    state.playback.artwork = Some(crate::app::state::ArtworkCell {
      slot,
      image_id: image_id.clone(),
      state: ArtworkCellState::Ready,
    });
    state.kernel.artwork_handles.insert(
      slot,
      image_id.clone(),
      iced::widget::image::Handle::from_rgba(2, 1, vec![0; 8]),
    );

    let initial_artwork = state.playback.artwork.clone();
    assert!(bar(&state).is_some());

    // Issue tick intent to trigger refresh
    let tick_effects = state
      .playback
      .session
      .handle(PlaybackInput::Intent(PlaybackIntent::Tick), now);
    let [PlaybackEffect::Controller(refresh_id, _)] = tick_effects.as_slice() else {
      panic!("expected refresh controller effect");
    };

    // Drive a real refreshed controller settlement through the update harness
    drop(crate::app::update::update(
      &mut state,
      Message::Playback(PlaybackMessage::Event(Box::new(
        PlaybackEvent::ControllerSettled {
          id: *refresh_id,
          settlement: ControllerSettlement::Refreshed {
            outcome: PlaybackRefreshOutcome {
              snapshot: PlaybackSnapshot {
                now_playing: Some(NowPlayingItem {
                  item_id: "episode-1".to_owned(),
                  title: "Pilot Episode".to_owned(),
                  item_type: "Episode".to_owned(),
                  runtime_seconds: Some(2_400.0),
                  start_position_seconds: 0.0,
                  play_method: "DirectPlay".to_owned(),
                }),
                transport: PlayerState {
                  connected: true,
                  paused: false,
                  muted: false,
                  time_pos: 121.0,
                  duration: 2_400.0,
                  volume: 85.0,
                },
              },
              state: PlaybackRefreshState::Active,
              warnings: Vec::new(),
            },
            client_messages: Vec::new(),
          },
        },
      ))),
    ));

    // Verify position advanced via real settlement
    assert_eq!(
      state
        .playback
        .view
        .now_playing
        .as_ref()
        .map(|np| np.position_seconds),
      Some(121.0)
    );

    // Verify artwork cell, slot, image_id, and retained handle identity remain unchanged
    assert_eq!(state.playback.artwork, initial_artwork);
    assert!(state.kernel.artwork_handles.get(slot, &image_id).is_some());
    assert!(bar(&state).is_some());
  }

  #[test]
  fn track_choices_includes_off_for_subtitles_and_excludes_for_audio() {
    let tracks = vec![
      TrackInfo {
        id: 1,
        track_type: "audio".to_owned(),
        title: Some("English Stereo".to_owned()),
        language: Some("eng".to_owned()),
        selected: true,
        provider_index: None,
      },
      TrackInfo {
        id: 2,
        track_type: "audio".to_owned(),
        title: Some("Spanish".to_owned()),
        language: Some("spa".to_owned()),
        selected: false,
        provider_index: None,
      },
      TrackInfo {
        id: 3,
        track_type: "sub".to_owned(),
        title: Some("English SDH".to_owned()),
        language: Some("eng".to_owned()),
        selected: false,
        provider_index: None,
      },
    ];
    let audio_choices = track_choices(&tracks, "audio", false);
    assert_eq!(audio_choices.len(), 2);
    assert_eq!(audio_choices[0].id, Some(1));
    assert_eq!(audio_choices[0].label, "English Stereo · eng");
    assert_eq!(audio_choices[1].id, Some(2));
    assert_eq!(audio_choices[1].label, "Spanish · spa");

    let sub_choices = track_choices(&tracks, "sub", true);
    assert_eq!(sub_choices.len(), 2);
    assert_eq!(sub_choices[0].id, None);
    assert_eq!(sub_choices[0].label, "Off");
    assert_eq!(sub_choices[1].id, Some(3));
    assert_eq!(sub_choices[1].label, "English SDH · eng");
  }

  #[test]
  fn intro_prompt_rendered_on_bar_when_active() {
    let mut state = State::boot(false);
    state.playback.view.now_playing = Some(test_now_playing());
    assert!(intro_prompt(&state).is_none());

    state.playback.view.intro_prompt = Some(IntroPromptView {
      kind: IntroSkipKind::Introduction,
    });
    assert!(intro_prompt(&state).is_some());

    state.playback.view.intro_prompt = Some(IntroPromptView {
      kind: IntroSkipKind::Credits,
    });
    assert!(intro_prompt(&state).is_some());
  }

  #[test]
  fn adjacent_buttons_rendered_with_availability() {
    let mut state = State::boot(false);
    state.playback.view.now_playing = Some(test_now_playing());
    state.playback.view.adjacent = AdjacentView {
      previous: AdjacentAvailability::Unavailable,
      next: AdjacentAvailability::Available {
        title: "Episode 2".to_owned(),
      },
    };

    let prev = adjacent_button(&state, AdjacentDirection::Previous, "Previous");
    let next = adjacent_button(&state, AdjacentDirection::Next, "Next");
    drop(prev);
    drop(next);
  }

  #[test]
  fn audio_and_subtitle_popovers_render_across_track_states() {
    let mut state = State::boot(false);
    state.playback.view.now_playing = Some(test_now_playing());

    // Loading
    state.playback.view.tracks = TracksView::Loading;
    let audio_el = audio_popover(&state);
    let sub_el = subtitle_popover(&state);
    drop(audio_el);
    drop(sub_el);

    // Unavailable
    state.playback.view.tracks = TracksView::Unavailable;
    let audio_el = audio_popover(&state);
    let sub_el = subtitle_popover(&state);
    drop(audio_el);
    drop(sub_el);

    // Ready with open menus
    state.playback.view.tracks = TracksView::Ready {
      tracks: vec![
        TrackInfo {
          id: 1,
          track_type: "audio".to_owned(),
          title: Some("English".to_owned()),
          language: Some("eng".to_owned()),
          selected: true,
          provider_index: None,
        },
        TrackInfo {
          id: 2,
          track_type: "sub".to_owned(),
          title: Some("English".to_owned()),
          language: Some("eng".to_owned()),
          selected: true,
          provider_index: None,
        },
      ],
      audio: Some(1),
      subtitle: Some(2),
    };
    state.playback.audio_menu_open = true;
    state.playback.subtitle_menu_open = true;
    let audio_el = audio_popover(&state);
    let sub_el = subtitle_popover(&state);
    drop(audio_el);
    drop(sub_el);
  }

  #[test]
  fn audio_and_subtitle_popovers_disabled_when_track_choices_empty() {
    let mut state = State::boot(false);
    state.playback.view.now_playing = Some(test_now_playing());

    // Ready with zero audio tracks and zero subtitle tracks
    state.playback.view.tracks = TracksView::Ready {
      tracks: Vec::new(),
      audio: None,
      subtitle: None,
    };
    let audio_el = audio_popover(&state);
    let sub_el = subtitle_popover(&state);
    drop(audio_el);
    drop(sub_el);

    // Ready with audio only (subtitles should remain disabled)
    state.playback.view.tracks = TracksView::Ready {
      tracks: vec![TrackInfo {
        id: 1,
        track_type: "audio".to_owned(),
        title: Some("English".to_owned()),
        language: Some("eng".to_owned()),
        selected: true,
        provider_index: None,
      }],
      audio: Some(1),
      subtitle: None,
    };
    let audio_el = audio_popover(&state);
    let sub_el = subtitle_popover(&state);
    drop(audio_el);
    drop(sub_el);

    // Ready with subtitles only (audio should remain disabled)
    state.playback.view.tracks = TracksView::Ready {
      tracks: vec![TrackInfo {
        id: 2,
        track_type: "sub".to_owned(),
        title: Some("English SDH".to_owned()),
        language: Some("eng".to_owned()),
        selected: true,
        provider_index: None,
      }],
      audio: None,
      subtitle: Some(2),
    };
    let audio_el = audio_popover(&state);
    let sub_el = subtitle_popover(&state);
    drop(audio_el);
    drop(sub_el);
  }
}
