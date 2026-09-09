use std::fmt;

use crate::app::collections::{self, Source};
use crate::app::message::{Message, PlaybackMessage, SettingsMessage};
use crate::app::playback::{QueueState, PLAYER_IMAGE_KEY};
use crate::app::shell::SETTINGS_TRIGGER_ID;
use crate::app::state::State;
use crate::i18n::Localizer;
use iced::widget::{
  button, column, container, responsive, row, scrollable, slider, space, text, Column,
};
use iced::{Alignment, ContentFit, Element, Fill, Length};
use jellypilot_core::config::AppMode;
use jellypilot_media_server::IntroSkipKind;
use jellypilot_mpv::playback::{Playable, TrackInfo};
use jellypilot_mpv::playback_session::{
  AdjacentAvailability, AdjacentDirection, NowPlayingView, PlaybackIntent, TracksView,
};
use jellypilot_mpv::player::format_duration;
use jellypilot_ui::fonts::{DISPLAY_FONT, HEADING_FONT};
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::overlay::{popover, tooltip, Placement, PopoverOptions, TooltipOptions};
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};
use jellypilot_ui::widgets::control_button::control_button;
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::{full_radius, rounded_image};

pub(super) fn video_surface<'a>() -> Element<'a, Message> {
  iced::widget::mouse_area(crate::embedded::view())
    .on_press(Message::Playback(PlaybackMessage::Intent(Box::new(
      PlaybackIntent::TogglePaused,
    ))))
    .into()
}

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
  Some(
    responsive(move |bounds| bar_content(state, now_playing, bounds.width))
      .height(Length::Shrink)
      .into(),
  )
}

fn bar_content<'a>(
  state: &'a State,
  now_playing: &'a NowPlayingView,
  width: f32,
) -> Element<'a, Message> {
  let collection = collections::controls(state, Source::NowPlaying);
  let favorite = tooltip(
    control_button(
      Some(if collection.favorite == Some(true) {
        Icon::HeartFilled
      } else {
        Icon::Heart
      }),
      None,
      if collection.favorite == Some(true) {
        ButtonVariant::TonalActive
      } else {
        ButtonVariant::Tonal
      },
    )
    .icon_size(IconSize::Sm)
    .width(Length::Fixed(40.0))
    .min_height(40.0)
    .on_press_maybe(collection.favorite_action),
    collection.favorite_label,
    TooltipOptions::default(),
  );
  let identity = row![
    playback_artwork(state, 32.0, 48.0),
    column![
      ellipsis_text(&now_playing.item.title)
        .font(HEADING_FONT)
        .size(12)
        .color(state.palette().text.heading),
      ellipsis_text(playback_caption(state))
        .size(11)
        .color(state.palette().text.metadata),
    ]
    .spacing(TOKENS.spacing.s0_5)
    .width(Fill),
    favorite,
  ]
  .spacing(TOKENS.spacing.s3)
  .align_y(Alignment::Center);
  let fullscreen = tooltip(
    control_button(Some(Icon::ArrowsMaximize), None, ButtonVariant::Tonal)
      .icon_size(IconSize::Sm)
      .width(Length::Fixed(40.0))
      .min_height(40.0)
      .on_press_maybe((!state.playback.view.busy).then_some(Message::Playback(
        PlaybackMessage::Intent(Box::new(PlaybackIntent::ToggleFullscreen)),
      ))),
    state.t("player-toggle-fullscreen"),
    TooltipOptions::default(),
  );
  let tools = row![
    queue_popover(state, true),
    audio_popover(state, true),
    subtitle_popover(state, true),
    volume_controls(state, now_playing),
    fullscreen,
  ]
  .spacing(TOKENS.spacing.s1_5)
  .align_y(Alignment::Center);
  let position = state
    .playback
    .seek_preview
    .unwrap_or(now_playing.position_seconds);
  let duration = now_playing
    .duration_seconds
    .filter(|value| value.is_finite() && *value > 0.0);
  let timeline: Element<'_, Message> = match duration {
    Some(duration) => seek_row(position, duration),
    None => text(format_duration(position))
      .size(11)
      .color(state.palette().text.metadata)
      .into(),
  };
  let center = row![transport(state, now_playing), timeline]
    .spacing(TOKENS.spacing.s3)
    .align_y(Alignment::Center)
    .width(Fill);
  let mut content = Column::new().spacing(TOKENS.spacing.s2);
  if width >= 1150.0 {
    content = content.push(
      row![identity.width(280), center, tools]
        .spacing(TOKENS.spacing.s4)
        .align_y(Alignment::Center),
    );
  } else if width >= 640.0 {
    content = content
      .push(
        row![identity.width(Fill), tools]
          .spacing(TOKENS.spacing.s4)
          .align_y(Alignment::Center),
      )
      .push(center);
  } else {
    content = content.push(identity.width(Fill)).push(center).push(tools);
  }
  if crate::embedded::enabled() {
    content = content.push(
      control_button(
        Some(Icon::Movie),
        Some(state.t("player-show-video")),
        ButtonVariant::Tonal,
      )
      .on_press(Message::Home(crate::app::message::HomeMessage::Navigate(
        crate::app::state::Destination::NowPlaying,
      ))),
    );
  }
  if let Some(prompt) = intro_prompt(state) {
    content = content.push(prompt);
  }
  container(content)
    .padding([TOKENS.spacing.s2, TOKENS.spacing.s6])
    .width(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Block))
    .into()
}

/// Title and series/episode caption shared by the bar and the full-window
/// compact player.
fn now_playing_metadata<'a>(
  state: &State,
  now_playing: &'a NowPlayingView,
  title_size: f32,
  caption_size: f32,
) -> Column<'a, Message> {
  column![
    text(&now_playing.item.title)
      .font(HEADING_FONT)
      .size(title_size)
      .color(state.palette().text.heading),
    text(playback_caption(state))
      .size(caption_size)
      .color(state.palette().text.metadata),
  ]
  .spacing(TOKENS.spacing.s0_5)
}

/// Prev/play-pause/stop/next transport shared by the bar and the compact
/// full-window player.
fn transport<'a>(state: &'a State, now_playing: &NowPlayingView) -> Element<'a, Message> {
  let is_paused = now_playing.paused;
  let play_pause_icon = if is_paused { Icon::Play } else { Icon::Pause };
  let play_pause_label = state.t(if is_paused {
    "common-play"
  } else {
    "common-pause"
  });
  let play_pause_button = control_button(Some(play_pause_icon), None, ButtonVariant::Primary)
    .icon_size(IconSize::Lg)
    .padding([7, 11])
    .on_press(Message::Playback(PlaybackMessage::Intent(Box::new(
      PlaybackIntent::TogglePaused,
    ))));
  let play_pause = tooltip(
    play_pause_button,
    play_pause_label,
    TooltipOptions::default(),
  );

  let stop_button = control_button(Some(Icon::Stop), None, ButtonVariant::Tonal)
    .padding([6, 10])
    .on_press(Message::Playback(PlaybackMessage::Intent(Box::new(
      PlaybackIntent::Stop,
    ))));
  let stop = tooltip(
    stop_button,
    state.t("common-stop"),
    TooltipOptions::default(),
  );

  row![
    adjacent_button(
      state,
      AdjacentDirection::Previous,
      &state.t("common-previous")
    ),
    play_pause,
    stop,
    adjacent_button(state, AdjacentDirection::Next, &state.t("common-next")),
  ]
  .spacing(TOKENS.spacing.s1_5)
  .align_y(Alignment::Center)
  .into()
}

/// Episode queue and audio/subtitle popover triggers shared by the bar and the
/// compact player.
fn track_selection(state: &State) -> Element<'_, Message> {
  row![
    queue_popover(state, false),
    audio_popover(state, false),
    subtitle_popover(state, false)
  ]
  .spacing(TOKENS.spacing.s1_5)
  .align_y(Alignment::Center)
  .into()
}

/// Mute toggle and volume slider shared by the bar and the compact player.
fn volume_controls<'a>(state: &'a State, now_playing: &NowPlayingView) -> Element<'a, Message> {
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
  let mute_label = state.t(if now_playing.muted {
    "player-unmute"
  } else {
    "player-mute"
  });
  let mute_button = control_button(Some(mute_icon), None, ButtonVariant::Tonal)
    .padding([6, 10])
    .on_press(Message::Playback(PlaybackMessage::Intent(Box::new(
      PlaybackIntent::SetMuted(!now_playing.muted),
    ))));
  let mute = tooltip(mute_button, mute_label, TooltipOptions::default());

  row![mute, volume_slider]
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center)
    .into()
}

/// Position/slider/duration seek row shared by the bar and the compact
/// full-window player.
fn seek_row(position: f64, duration: f64) -> Element<'static, Message> {
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
  .align_y(Alignment::Center)
  .into()
}

/// Full-window Now Playing view for Control-Only mode: a large poster,
/// metadata, seek, transport, volume, and track menus, with a gear button to
/// Settings. Without an active playback session it shows an honest idle
/// state — never fake media.
pub fn full(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let settings_button = control_button(Some(Icon::Settings), None, ButtonVariant::Tonal)
    .id(SETTINGS_TRIGGER_ID)
    .padding([6, 10])
    .on_press(Message::Settings(SettingsMessage::Open));
  let header = row![
    space::horizontal(),
    tooltip(
      control_button(Some(Icon::ArrowsMaximize), None, ButtonVariant::Tonal,)
        .padding([6, 10])
        .on_press(Message::Settings(SettingsMessage::AppModeSelected(
          AppMode::Full
        ))),
      state.t("player-full-library"),
      TooltipOptions::default(),
    ),
    tooltip(
      settings_button,
      state.t("common-settings"),
      TooltipOptions::default()
    ),
  ]
  .width(Fill)
  .spacing(TOKENS.spacing.s2)
  .align_y(Alignment::Center);

  let body: Element<'_, Message> = match state.playback.view.now_playing.as_ref() {
    Some(now_playing) => {
      let duration = now_playing
        .duration_seconds
        .filter(|duration| duration.is_finite() && *duration > 0.0);
      let position = state
        .playback
        .seek_preview
        .unwrap_or(now_playing.position_seconds);
      let mut content = Column::new()
        .spacing(TOKENS.spacing.s3)
        .align_x(Alignment::Center)
        .width(Fill)
        .push(if crate::embedded::enabled() {
          container(video_surface())
            .width(Fill)
            .height(Length::FillPortion(1))
            .into()
        } else {
          playback_artwork(state, 200.0, 300.0)
        })
        .push(
          now_playing_metadata(state, now_playing, 22.0, 13.0)
            .align_x(Alignment::Center)
            .width(Fill),
        );
      if let Some(duration) = duration {
        content = content.push(seek_row(position, duration));
      }
      if let Some(prompt) = intro_prompt(state) {
        content = content.push(prompt);
      }
      content
        .push(transport(state, now_playing))
        .push(
          row![track_selection(state), volume_controls(state, now_playing)]
            .spacing(TOKENS.spacing.s3)
            .align_y(Alignment::Center),
        )
        .into()
    }
    None => column![
      icon_with_color(
        Icon::Movie,
        IconSize::Custom(40.0),
        palette.colors.onSurfaceVariant,
      ),
      text("JellyPilot")
        .font(DISPLAY_FONT)
        .size(26)
        .color(palette.text.heading),
      text(state.t("player-waiting"))
        .size(13)
        .color(palette.text.metadata),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_x(Alignment::Center)
    .into(),
  };

  container(
    column![
      header,
      container(body)
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center),
    ]
    .width(Fill)
    .height(Fill),
  )
  .padding(TOKENS.spacing.s3)
  .width(Fill)
  .height(Fill)
  .into()
}

fn intro_prompt(state: &State) -> Option<Element<'_, Message>> {
  let prompt = state.playback.view.intro_prompt?;
  let palette = state.palette();
  let label = state.t(match prompt.kind {
    IntroSkipKind::Introduction => "player-skip-intro",
    IntroSkipKind::Credits => "player-skip-credits",
  });
  let actions = row![
    control_button(
      Some(Icon::Next),
      Some(state.t("player-skip")),
      ButtonVariant::Primary,
    )
    .icon_size(IconSize::Sm)
    .spacing(TOKENS.spacing.s1_5)
    .padding([6, 12])
    .on_press(Message::Playback(PlaybackMessage::Intent(Box::new(
      PlaybackIntent::SkipIntro,
    )))),
    control_button(
      Some(Icon::Close),
      Some(state.t("common-dismiss")),
      ButtonVariant::Tonal,
    )
    .icon_size(IconSize::Xs)
    .spacing(TOKENS.spacing.s1_5)
    .on_press(Message::Playback(PlaybackMessage::Intent(Box::new(
      PlaybackIntent::DismissIntro,
    )))),
  ]
  .spacing(TOKENS.spacing.s2);
  Some(
    container(row![
      row![
        icon_with_color(Icon::IntroSkip, IconSize::Md, palette.colors.primary),
        text(label)
          .font(HEADING_FONT)
          .size(16)
          .color(palette.text.secondary),
      ]
      .spacing(TOKENS.spacing.s2)
      .align_y(Alignment::Center),
      space::horizontal(),
      actions,
    ])
    .padding([TOKENS.spacing.s2, TOKENS.spacing.s3])
    .width(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Floating))
    .into(),
  )
}

fn adjacent_button<'a>(
  state: &State,
  direction: AdjacentDirection,
  label: &str,
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
  let btn = control_button(Some(icon_variant), None, ButtonVariant::Tonal)
    .padding([6, 10])
    .on_press_maybe(
      available.then_some(Message::Playback(PlaybackMessage::Intent(Box::new(
        PlaybackIntent::PlayAdjacent(direction),
      )))),
    );
  tooltip(btn, label.to_owned(), TooltipOptions::default())
}

fn audio_popover(state: &State, icon_only: bool) -> Element<'_, Message> {
  let palette = state.palette();
  let has_audio_choices = match &state.playback.view.tracks {
    TracksView::Ready { tracks, .. } => tracks.iter().any(|track| track.track_type == "audio"),
    TracksView::Loading | TracksView::Unavailable => false,
  };
  let audio_btn_variant = if state.playback.audio_menu_open {
    ButtonVariant::TonalActive
  } else {
    ButtonVariant::Tonal
  };
  let trigger = control_button(
    Some(Icon::AudioTrack),
    (!icon_only).then(|| state.t("player-audio")),
    audio_btn_variant,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press_maybe(
    has_audio_choices.then_some(Message::Playback(PlaybackMessage::AudioMenuToggled)),
  );
  let trigger = if icon_only {
    trigger.width(Length::Fixed(40.0)).min_height(40.0)
  } else {
    trigger
  };
  let trigger = menu_hint(trigger.into(), state.t("player-audio"), icon_only);
  let menu = match &state.playback.view.tracks {
    TracksView::Ready { tracks, audio, .. } => {
      let choices = track_choices(state.kernel.locale, tracks, "audio", false);
      if choices.is_empty() {
        column![text(state.t("player-no-audio"))
          .size(12)
          .color(palette.text.metadata)]
        .spacing(TOKENS.spacing.s1)
        .width(Fill)
      } else {
        let mut col = Column::new().spacing(TOKENS.spacing.s1).width(Fill);
        for choice in choices {
          let active = choice.id == *audio;
          let id = choice.id.unwrap_or_default();
          let active_marker: Element<'_, Message> = if active {
            icon_with_color(Icon::Check, IconSize::Xs, palette.colors.primary).into()
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
              jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Text)
            }),
          );
        }
        col
      }
    }
    TracksView::Loading => column![text(state.t("player-loading-audio"))
      .size(12)
      .color(palette.text.metadata)]
    .spacing(TOKENS.spacing.s1)
    .width(Fill),
    TracksView::Unavailable => column![text(state.t("player-unavailable-audio"))
      .size(12)
      .color(palette.text.metadata)]
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

fn subtitle_popover(state: &State, icon_only: bool) -> Element<'_, Message> {
  let palette = state.palette();
  let has_subtitle_choices = match &state.playback.view.tracks {
    TracksView::Ready { tracks, .. } => tracks.iter().any(|track| track.track_type == "sub"),
    TracksView::Loading | TracksView::Unavailable => false,
  };
  let sub_btn_variant = if state.playback.subtitle_menu_open {
    ButtonVariant::TonalActive
  } else {
    ButtonVariant::Tonal
  };
  let trigger = control_button(
    Some(Icon::Subtitles),
    (!icon_only).then(|| state.t("player-subtitles")),
    sub_btn_variant,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press_maybe(
    has_subtitle_choices.then_some(Message::Playback(PlaybackMessage::SubtitleMenuToggled)),
  );
  let trigger = if icon_only {
    trigger.width(Length::Fixed(40.0)).min_height(40.0)
  } else {
    trigger
  };
  let trigger = menu_hint(trigger.into(), state.t("player-subtitles"), icon_only);
  let menu = match &state.playback.view.tracks {
    TracksView::Ready {
      tracks, subtitle, ..
    } => {
      let choices = track_choices(state.kernel.locale, tracks, "sub", true);
      let mut col = Column::new().spacing(TOKENS.spacing.s1).width(Fill);
      for choice in choices {
        let active = choice.id == *subtitle;
        let active_marker: Element<'_, Message> = if active {
          icon_with_color(Icon::Check, IconSize::Xs, palette.colors.primary).into()
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
            jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Text)
          }),
        );
      }
      col
    }
    TracksView::Loading => column![text(state.t("player-loading-subtitles"))
      .size(12)
      .color(palette.text.metadata)]
    .spacing(TOKENS.spacing.s1)
    .width(Fill),
    TracksView::Unavailable => column![text(state.t("player-unavailable-subtitles"))
      .size(12)
      .color(palette.text.metadata)]
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

/// Maximum height of the episode queue list before it scrolls.
const QUEUE_MENU_MAX_HEIGHT: f32 = 280.0;

/// Current-season episode queue popover shared by the bar and the compact
/// player. Rows follow season episode order; the actively playing episode is
/// marked and not selectable.
fn queue_popover(state: &State, icon_only: bool) -> Element<'_, Message> {
  let palette = state.palette();
  let available = !matches!(state.playback.queue, QueueState::Unavailable);
  let queue_btn_variant = if state.playback.queue_menu_open {
    ButtonVariant::TonalActive
  } else {
    ButtonVariant::Tonal
  };
  let trigger = control_button(
    Some(Icon::Playlist),
    (!icon_only).then(|| state.t("player-queue")),
    queue_btn_variant,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press_maybe(available.then_some(Message::Playback(PlaybackMessage::QueueMenuToggled)));
  let trigger = if icon_only {
    trigger.width(Length::Fixed(40.0)).min_height(40.0)
  } else {
    trigger
  };
  let trigger = menu_hint(trigger.into(), state.t("player-queue"), icon_only);

  let menu: Element<'_, Message> = match &state.playback.queue {
    QueueState::Ready(items) => {
      if items.is_empty() {
        column![text(state.t("player-no-episodes"))
          .size(12)
          .color(palette.text.metadata)]
        .spacing(TOKENS.spacing.s1)
        .width(Fill)
        .into()
      } else {
        let current_id = state
          .playback
          .view
          .now_playing
          .as_ref()
          .map(|view| view.item.item_id.as_str());
        let mut rows = Column::new().spacing(TOKENS.spacing.s1).width(Fill);
        for item in items {
          let is_current = current_id == Some(item.id.as_str());
          let row_variant = if is_current {
            ButtonVariant::Secondary
          } else {
            ButtonVariant::Text
          };
          let mut label = row![]
            .spacing(TOKENS.spacing.s2)
            .align_y(Alignment::Center)
            .width(Fill);
          if let (Some(season), Some(episode)) = (item.season_number, item.episode_number) {
            label = label.push(
              text(format!("S{season:02}E{episode:02}"))
                .size(11)
                .color(palette.text.metadata),
            );
          }
          label = label.push(text(&item.name).width(Fill).size(13));
          let marker: Element<'_, Message> = if is_current {
            icon_with_color(Icon::Check, IconSize::Xs, palette.colors.primary).into()
          } else {
            space::horizontal().width(14).into()
          };
          rows = rows.push(
            button(row![label, marker].align_y(Alignment::Center))
              .padding([6, 10])
              .width(Fill)
              .on_press_maybe((!is_current).then_some(Message::Playback(
                PlaybackMessage::QueueItemSelected(item.clone()),
              )))
              .style(move |theme, status| {
                jellypilot_ui::theme::button_variant(theme, status, row_variant)
              }),
          );
        }
        container(
          scrollable(rows)
            .width(Fill)
            .style(jellypilot_ui::theme::scrollable),
        )
        .height(Length::Fit.max(QUEUE_MENU_MAX_HEIGHT))
        .into()
      }
    }
    QueueState::Loading => column![text(state.t("player-loading-episodes"))
      .size(12)
      .color(palette.text.metadata)]
    .spacing(TOKENS.spacing.s1)
    .width(Fill)
    .into(),
    QueueState::Unavailable | QueueState::Failed => {
      column![text(state.t("player-unavailable-queue"))
        .size(12)
        .color(palette.text.metadata)]
      .spacing(TOKENS.spacing.s1)
      .width(Fill)
      .into()
    }
  };

  popover(
    trigger,
    menu,
    state.playback.queue_menu_open,
    PopoverOptions {
      placement: Placement::Above,
      width: Some(320.0),
      ..PopoverOptions::default()
    },
    Message::Playback(PlaybackMessage::QueueMenuDismissed),
  )
}

fn menu_hint(
  trigger: Element<'_, Message>,
  label: String,
  icon_only: bool,
) -> Element<'_, Message> {
  if !icon_only {
    return trigger;
  }
  tooltip(trigger, label, TooltipOptions::default())
}

fn track_choices(
  locale: Localizer,
  tracks: &[TrackInfo],
  track_type: &str,
  include_off: bool,
) -> Vec<TrackChoice> {
  let mut choices = Vec::with_capacity(tracks.len() + usize::from(include_off));
  if include_off {
    choices.push(TrackChoice {
      id: None,
      label: locale.text("common-off"),
    });
  }
  choices.extend(
    tracks
      .iter()
      .filter(|track| track.track_type == track_type)
      .map(|track| TrackChoice {
        id: Some(track.id),
        label: track_label(locale, track),
      }),
  );
  choices
}

fn track_label(locale: Localizer, track: &TrackInfo) -> String {
  match (track.title.as_deref(), track.language.as_deref()) {
    (Some(title), Some(language)) => format!("{title} · {language}"),
    (Some(title), None) => title.to_owned(),
    (None, Some(language)) => language.to_owned(),
    (None, None) => locale.format("player-track", &[("number", track.id.into())]),
  }
}

fn playback_caption(state: &State) -> String {
  let Some(playable) = state.playback.playable.as_ref() else {
    return state
      .playback
      .view
      .now_playing
      .as_ref()
      .map(|view| media_type(state.kernel.locale, &view.item.item_type))
      .unwrap_or_default();
  };
  match playable {
    Playable::Library(item) => media_caption(
      state.kernel.locale,
      &item.item_type,
      item.series_name.as_deref(),
      item.season_number,
      item.episode_number,
    ),
    Playable::Detail(item) => media_caption(
      state.kernel.locale,
      &item.item_type,
      item.series_name.as_deref(),
      item.season_number,
      item.episode_number,
    ),
    Playable::Media(item) => media_caption(
      state.kernel.locale,
      &item.item_type,
      item.series_name.as_deref(),
      item.parent_index_number,
      item.index_number,
    ),
  }
}

fn media_caption(
  locale: Localizer,
  item_type: &str,
  series_name: Option<&str>,
  season: Option<i32>,
  episode: Option<i32>,
) -> String {
  let item_type = media_type(locale, item_type);
  match (series_name, season, episode) {
    (Some(series), Some(season), Some(episode)) => {
      format!("{series} · S{season:02}E{episode:02}")
    }
    (Some(series), _, _) => format!("{series} · {item_type}"),
    _ => item_type,
  }
}

fn media_type(locale: Localizer, item_type: &str) -> String {
  locale.text(match item_type {
    "Movie" => "player-movie",
    "Episode" => "player-episode",
    "Series" => "player-series",
    "Season" => "player-season",
    "Video" => "player-video",
    "Audio" => "player-audio",
    "MusicVideo" => "player-music-video",
    _ => "player-media",
  })
}

fn playback_artwork(state: &State, width: f32, height: f32) -> Element<'_, Message> {
  let palette = state.palette();
  if let Some(handle) = state
    .playback
    .artwork
    .get(PLAYER_IMAGE_KEY)
    .and_then(|cell| cell.handle())
  {
    return rounded_image(handle.clone(), full_radius(TOKENS.radii.lg))
      .content_fit(ContentFit::Cover)
      .width(width)
      .height(height)
      .into();
  }
  container(icon_with_color(
    Icon::Movie,
    IconSize::Custom(26.0),
    palette.colors.onSurfaceVariant,
  ))
  .width(width)
  .height(height)
  .align_x(Alignment::Center)
  .align_y(Alignment::Center)
  .style(|_theme| container::Style {
    background: Some(iced::Background::Color(
      palette.colors.surfaceContainerLowest,
    )),
    border: iced::Border {
      smoothing: jellypilot_ui::widgets::container::SURFACE_SMOOTHING,
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
  use jellypilot_core::intro_skipper::IntroSkipMode;
  use jellypilot_media_server::VideoLibraryItem;
  use jellypilot_mpv::playback::{
    NowPlayingItem, Playable, PlaybackOutcome, PlaybackRefreshOutcome, PlaybackRefreshState,
    PlaybackSnapshot, PlaybackStartPosition,
  };
  use jellypilot_mpv::playback_session::{
    ControllerSettlement, IntroAvailability, IntroPromptView, NowPlayingView, PlaybackEffect,
    PlaybackEvent, PlaybackInput, PlaybackIntent,
  };
  use jellypilot_mpv::PlayerState;
  use std::time::Instant;

  #[cfg(target_os = "linux")]
  #[test]
  fn video_surface_toggles_only_on_left_press_inside_picture() {
    use iced::advanced::{layout, renderer, renderer::Headless, widget::Tree, Layout, Shell};
    use iced::{mouse, Event, Point, Rectangle, Size};

    let renderer = iced::futures::executor::block_on(iced::Renderer::new(
      renderer::Settings {
        font: iced::Font::DEFAULT,
        text_size: 16.0.into(),
        line_height: jellypilot_ui::fonts::DEFAULT_LINE_HEIGHT,
        metrics_hinting: false,
      },
      Some("tiny-skia"),
    ))
    .expect("headless renderer");
    let bounds = Rectangle::with_size(Size::new(320.0, 180.0));
    let mut content = video_surface();
    let mut tree = Tree::empty();
    tree.diff(content.as_widget_mut());
    let node = content.as_widget_mut().layout(
      &mut tree,
      &renderer,
      &layout::Limits::new(bounds.size(), bounds.size()),
    );
    for (event, cursor, toggles) in [
      (
        mouse::Event::ButtonPressed(mouse::Button::Left),
        Point::new(160.0, 90.0),
        true,
      ),
      (
        mouse::Event::ButtonReleased(mouse::Button::Left),
        Point::new(160.0, 90.0),
        false,
      ),
      (
        mouse::Event::ButtonPressed(mouse::Button::Right),
        Point::new(160.0, 90.0),
        false,
      ),
      (
        mouse::Event::ButtonPressed(mouse::Button::Left),
        Point::new(350.0, 90.0),
        false,
      ),
      (
        mouse::Event::ButtonPressed(mouse::Button::Left),
        Point::new(160.0, 90.0),
        true,
      ),
    ] {
      let mut messages = iced::advanced::shell::Bus::new();
      content.as_widget_mut().update(
        &mut tree,
        &Event::Mouse(event),
        Layout::new(&node),
        mouse::Cursor::Available(cursor),
        &renderer,
        &mut Shell::new(
          &iced::window::Headless,
          iced::advanced::shell::Waker::noop(),
          &mut messages,
        ),
        &bounds,
      );
      let toggles_emitted = messages
        .drain()
        .filter(|(message, _)| {
          matches!(
            message,
            Message::Playback(PlaybackMessage::Intent(intent))
              if matches!(**intent, PlaybackIntent::TogglePaused)
          )
        })
        .count();
      assert_eq!(toggles_emitted, usize::from(toggles));
    }
  }

  #[tokio::test]
  async fn closing_an_icon_menu_preserves_keyboard_activation_of_its_trigger() {
    use iced::advanced::{renderer::Headless, widget};
    use iced_runtime::user_interface::{Cache, UserInterface};
    struct Focus;
    impl widget::Operation for Focus {
      fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
        visit(self);
      }
      fn focusable(
        &mut self,
        _: Option<&widget::Id>,
        _: iced::Rectangle,
        state: &mut dyn widget::operation::Focusable,
      ) {
        state.focus();
      }
    }
    let mut state = State::boot(false);
    state.playback.view.tracks = TracksView::Ready {
      tracks: vec![TrackInfo {
        id: 1,
        track_type: "audio".to_owned(),
        title: Some("Audio".to_owned()),
        language: None,
        selected: true,
        provider_index: None,
      }],
      audio: Some(1),
      subtitle: None,
    };
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    let bounds = iced::Size::new(400.0, 400.0);
    let mut ui = UserInterface::build(
      audio_popover(&state, true),
      bounds,
      Cache::new(),
      &mut renderer,
    );
    ui.operate(&renderer, &mut Focus);
    let mut cache = ui.into_cache();
    for open in [true, false] {
      state.playback.audio_menu_open = open;
      cache = UserInterface::build(audio_popover(&state, true), bounds, cache, &mut renderer)
        .into_cache();
    }
    let mut ui = UserInterface::build(audio_popover(&state, true), bounds, cache, &mut renderer);
    let enter = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
      key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
      modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
      physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Enter),
      location: iced::keyboard::Location::Standard,
      modifiers: iced::keyboard::Modifiers::NONE,
      text: None,
      repeat: false,
    });
    let mut bus = iced::advanced::shell::Bus::new();
    let _ = ui.update(
      &iced::window::Headless,
      &iced::advanced::shell::Waker::noop(),
      &[enter],
      iced::mouse::Cursor::Unavailable,
      &mut renderer,
      &mut bus,
    );
    assert!(bus.drain().any(|(message, _)| matches!(
      message,
      Message::Playback(PlaybackMessage::AudioMenuToggled)
    )));
  }

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
  fn bar_keeps_playing_across_position_settlement() {
    let mut state = State::boot(false);
    let now = Instant::now();
    state.playback.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::EngineAvailability(true))),
      now,
    );
    let start_effects = state.playback.session.handle(
      PlaybackInput::Intent(Box::new(PlaybackIntent::Start {
        item: Playable::Library(VideoLibraryItem {
          logo_image_id: None,
          id: "episode-1".to_owned(),
          name: "Pilot Episode".to_owned(),
          item_type: "Episode".to_owned(),
          production_year: None,
          runtime_seconds: Some(2_400.0),
          played: false,
          favorite: false,
          artwork_image_id: None,
          backdrop_image_id: None,
          series_poster_image_id: Some("test-artwork-image".to_owned()),
          episode_thumb_image_id: None,
          series_thumb_image_id: None,
          series_backdrop_image_id: None,
          season_number: Some(1),
          episode_number: Some(1),
          series_id: Some("series-1".to_owned()),
          series_name: Some("Series".to_owned()),
          resume_position_seconds: None,
          played_percentage: None,
          overview: None,
          index_number_end: None,
          season_poster_image_id: None,
          end_year: None,
          series_continuing: false,
          unplayed_item_count: None,
        }),
        position: PlaybackStartPosition::Beginning,
        intro: IntroAvailability {
          mode: IntroSkipMode::Off,
          skipper_available: false,
        },
        selection: Box::default(),
      })),
      now,
    );
    let [PlaybackEffect::Controller(start_id, _)] = start_effects.as_slice() else {
      panic!("expected start controller effect");
    };
    state.playback.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::ControllerSettled {
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
      })),
      now,
    );
    state.playback.view = state.playback.session.view();

    assert!(bar(&state).is_some());

    // Issue tick intent to trigger refresh
    let tick_effects = state
      .playback
      .session
      .handle(PlaybackInput::Intent(Box::new(PlaybackIntent::Tick)), now);
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
    let audio_choices = track_choices(Localizer::default(), &tracks, "audio", false);
    assert_eq!(audio_choices.len(), 2);
    assert_eq!(audio_choices[0].id, Some(1));
    assert_eq!(audio_choices[0].label, "English Stereo · eng");
    assert_eq!(audio_choices[1].id, Some(2));
    assert_eq!(audio_choices[1].label, "Spanish · spa");

    let sub_choices = track_choices(Localizer::default(), &tracks, "sub", true);
    assert_eq!(sub_choices.len(), 2);
    assert_eq!(sub_choices[0].id, None);
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
  fn locale_changes_track_fallbacks_without_changing_media_data_or_selection() {
    use jellypilot_core::locale::UiLanguage;
    let tracks = vec![
      TrackInfo {
        id: 7,
        track_type: "sub".to_owned(),
        title: Some("字幕 · English SDH".to_owned()),
        language: Some("eng".to_owned()),
        selected: true,
        provider_index: Some(4),
      },
      TrackInfo {
        id: 8,
        track_type: "sub".to_owned(),
        title: None,
        language: None,
        selected: false,
        provider_index: Some(5),
      },
    ];
    let english = track_choices(Localizer::new(UiLanguage::English), &tracks, "sub", true);
    let chinese = track_choices(
      Localizer::new(UiLanguage::SimplifiedChinese),
      &tracks,
      "sub",
      true,
    );
    assert_eq!(
      english.iter().map(|choice| choice.id).collect::<Vec<_>>(),
      chinese.iter().map(|choice| choice.id).collect::<Vec<_>>()
    );
    assert_eq!(english[1].label, chinese[1].label);
    assert_ne!(english[0].label, chinese[0].label);
    assert_ne!(english[2].label, chinese[2].label);
  }
}
