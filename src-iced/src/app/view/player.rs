use std::fmt;

use crate::app::collections::{self, Source};
use crate::app::embedded_player;
use crate::app::message::{Message, PlaybackMessage, SettingsMessage};
use crate::app::playback::{QueueState, PLAYER_IMAGE_KEY, PLAYER_THUMBNAIL_KEY};
use crate::app::shell::SETTINGS_TRIGGER_ID;
use crate::app::state::State;
use crate::i18n::Localizer;
use iced::widget::{
  button, column, container, mouse_area, opaque, responsive, row, scrollable, slider, space, stack,
  text, themer, Column,
};
use iced::{Alignment, ContentFit, Element, Fill, Length};
use jellypilot_core::config::AppMode;
use jellypilot_media_server::IntroSkipKind;
use jellypilot_mpv::playback::{Playable, TrackInfo};
use jellypilot_mpv::playback_session::{
  AdjacentAvailability, AdjacentDirection, NowPlayingView, PlaybackIntent, TracksView,
};
use jellypilot_mpv::player::format_duration;
use jellypilot_ui::fonts::{DISPLAY_FONT, HEADING_FONT, MONO_FONT};
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::overlay::{
  focus_tooltip, popover, tooltip, Placement, PopoverAppearance, PopoverOptions, TooltipOptions,
};
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};
use jellypilot_ui::widgets::control_button::{control_button, control_button_content};
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::embedded_player as cinema;
use jellypilot_ui::widgets::inert::inert;
use jellypilot_ui::widgets::tracked_slider::{tracked_slider, Event as SliderEvent};
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
    audio_popover(state, true, false),
    subtitle_popover(state, true, false),
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
    audio_popover(state, false, false),
    subtitle_popover(state, false, false)
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
    Icon::VolumeLow
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
  if crate::embedded::enabled() && state.playback.view.now_playing.is_some() {
    return embedded(state);
  }
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
        .push(playback_artwork(state, 200.0, 300.0))
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

/// Shared embedded composition for normal, fullscreen, and Control-Only playback.
pub fn embedded(state: &State) -> Element<'_, Message> {
  responsive(move |bounds| -> Element<'_, Message> {
    let visible = embedded_player::controls_visible(state);
    let back_visible = embedded_player::back_visible(state);
    let mut layers = stack![
      if state.playback.view.busy || embedded_player::input_blocked(state) {
        inert(video_surface())
      } else {
        video_surface()
      }
    ]
    .width(Fill)
    .height(Fill);
    if back_visible {
      layers = layers.push(
        container(space::horizontal())
          .width(Fill)
          .height(160)
          .style(|_| cinema::scrim(true)),
      );
      let back = embedded_action(
        Icon::ChevronLeft,
        state.t("player-back"),
        ButtonVariant::Tonal,
        Some(Message::EmbeddedPlayer(embedded_player::Message::Back)),
      );
      layers = layers.push(
        container(back)
          .padding([TOKENS.spacing.s6, embedded_inset(bounds.width)])
          .width(Fill),
      );
    }
    if visible {
      layers = layers.push(
        container(
          container(space::horizontal())
            .width(Fill)
            .height(340)
            .style(|_| cinema::scrim(false)),
        )
        .width(Fill)
        .height(Fill)
        .align_y(Alignment::End),
      );
      if let Some(now_playing) = state.playback.view.now_playing.as_ref() {
        layers = layers.push(
          container(embedded_bar(
            state,
            now_playing,
            bounds.width - 2.0 * embedded_inset(bounds.width),
          ))
          .padding(embedded_inset(bounds.width))
          .width(Fill)
          .height(Fill)
          .align_y(Alignment::End),
        );
      }
    }
    if let Some(feedback) = embedded_player::feedback(state) {
      layers = layers.push(
        container(
          container(text(feedback).size(TOKENS.font_sizes.s16))
            .padding([TOKENS.spacing.s3, TOKENS.spacing.s5])
            .style(cinema::popover),
        )
        .center(Fill),
      );
    }
    themer(
      Some(cinema::theme()),
      mouse_area(layers)
        .on_move(move |position| {
          Message::EmbeddedPlayer(embedded_player::Message::PointerMoved {
            position,
            bounds,
            controls_height: controls_reveal_height(bounds.width),
          })
        })
        .interaction(if embedded_player::cursor_visible(state) {
          iced::mouse::Interaction::Idle
        } else {
          iced::mouse::Interaction::Hidden
        }),
    )
    .into()
  })
  .into()
}

fn embedded_volume_icon(muted: bool, volume: f64) -> Icon {
  if muted || volume <= 0.0 {
    Icon::VolumeMuted
  } else if volume <= 33.0 {
    Icon::VolumeLow
  } else if volume <= 66.0 {
    Icon::VolumeMedium
  } else {
    Icon::VolumeLoud
  }
}

fn embedded_action<'a>(
  icon: Icon,
  label: String,
  variant: ButtonVariant,
  action: Option<Message>,
) -> Element<'a, Message> {
  let volume_icon = matches!(
    icon,
    Icon::VolumeLow | Icon::VolumeMedium | Icon::VolumeLoud | Icon::VolumeMuted
  );
  let size = if variant == ButtonVariant::Primary {
    44.0
  } else if matches!(
    icon,
    Icon::ChevronLeft | Icon::ArrowsMaximize | Icon::ArrowsMinimize
  ) {
    36.0
  } else if volume_icon {
    34.0
  } else {
    40.0
  };
  focus_tooltip(
    control_button(Some(icon), None, variant)
      .style(
        if matches!(
          icon,
          Icon::ChevronLeft | Icon::ArrowsMaximize | Icon::ArrowsMinimize
        ) {
          cinema::framed_control
        } else if volume_icon {
          cinema::volume_control
        } else {
          cinema::control
        },
      )
      .icon_size(IconSize::Custom(match icon {
        Icon::Previous | Icon::Next => 22.0,
        Icon::ChevronLeft | Icon::ArrowsMaximize | Icon::ArrowsMinimize => 16.0,
        _ => 18.0,
      }))
      .padding(0)
      .width(Length::Fixed(size))
      .min_height(size.max(36.0))
      .content_centered(true)
      .on_press_maybe(action),
    label,
    TooltipOptions::default(),
  )
}

fn embedded_bar<'a>(
  state: &'a State,
  now_playing: &'a NowPlayingView,
  width: f32,
) -> Element<'a, Message> {
  let ready = !state.playback.view.busy;
  let intent =
    |intent| ready.then_some(Message::Playback(PlaybackMessage::Intent(Box::new(intent))));
  let identity = embedded_identity(state, now_playing, width > 900.0 - 2.0 * TOKENS.spacing.s9);
  let adjacent = |direction, icon, label| {
    let availability = match direction {
      AdjacentDirection::Previous => &state.playback.view.adjacent.previous,
      AdjacentDirection::Next => &state.playback.view.adjacent.next,
    };
    embedded_action(
      icon,
      state.t(label),
      ButtonVariant::Tonal,
      matches!(availability, AdjacentAvailability::Available { .. })
        .then(|| intent(PlaybackIntent::PlayAdjacent(direction)))
        .flatten(),
    )
  };
  let transport = row![
    adjacent(
      AdjacentDirection::Previous,
      Icon::Previous,
      "common-previous"
    ),
    embedded_action(
      if now_playing.paused {
        Icon::Play
      } else {
        Icon::Pause
      },
      state.t(if now_playing.paused {
        "common-play"
      } else {
        "common-pause"
      }),
      ButtonVariant::Primary,
      intent(PlaybackIntent::TogglePaused),
    ),
    adjacent(AdjacentDirection::Next, Icon::Next, "common-next"),
  ]
  .spacing(TOKENS.spacing.s2)
  .align_y(Alignment::Center);
  let tracks = row![
    audio_popover(state, true, true),
    subtitle_popover(state, true, true)
  ]
  .spacing(TOKENS.spacing.s2)
  .align_y(Alignment::Center);
  let volume: Element<'_, Message> = tracked_slider(
    slider(
      0.0..=100.0,
      state.playback.volume_preview.unwrap_or(now_playing.volume),
      SliderEvent::Changed,
    )
    .step(1.0)
    .on_release(SliderEvent::DragEnded)
    .height(36)
    .width(84.0)
    .style(if ready {
      cinema::volume
    } else {
      cinema::unavailable_slider
    }),
    embedded_player::is_volume_dragging(state),
    |event| {
      Message::Playback(match event {
        SliderEvent::DragStarted => PlaybackMessage::VolumeDragStarted,
        SliderEvent::Changed(value) => PlaybackMessage::VolumeChanged(value),
        SliderEvent::DragEnded => PlaybackMessage::VolumeReleased,
        SliderEvent::Adjusted(value) => PlaybackMessage::VolumeAdjusted(value),
      })
    },
  );
  let volume = if ready || embedded_player::is_volume_dragging(state) {
    volume
  } else {
    inert(volume)
  };
  let output = row![
    embedded_action(
      embedded_volume_icon(
        now_playing.muted,
        state.playback.volume_preview.unwrap_or(now_playing.volume),
      ),
      state.t(if now_playing.muted {
        "player-unmute"
      } else {
        "player-mute"
      }),
      ButtonVariant::Tonal,
      intent(PlaybackIntent::SetMuted(!now_playing.muted)),
    ),
    volume,
  ]
  .align_y(Alignment::Center);
  let tools = row![
    // The mute target occupies both 8px visual gaps without moving its glyph.
    row![tracks, output].align_y(Alignment::Center),
    container(space::horizontal())
      .width(1)
      .height(18)
      .style(cinema::separator),
    embedded_action(
      if state.shell.player_fullscreen {
        Icon::ArrowsMinimize
      } else {
        Icon::ArrowsMaximize
      },
      state.t("player-fullscreen-shortcut"),
      ButtonVariant::Tonal,
      intent(PlaybackIntent::ToggleFullscreen)
    ),
  ]
  .spacing(TOKENS.spacing.s2)
  .align_y(Alignment::Center)
  .into();
  let controls = embedded_controls(identity, transport.into(), tools, width);
  let card = opaque(
    container(column![embedded_timeline(state, now_playing), controls].spacing(TOKENS.spacing.s4))
      .padding([TOKENS.spacing.s5, TOKENS.spacing.s6])
      .width(Fill)
      .style(cinema::surface),
  );
  let mut content = Column::new().spacing(TOKENS.spacing.s3);
  if let Some(prompt) = embedded_intro_prompt(state) {
    content = content.push(container(prompt).width(Fill).align_x(Alignment::End));
  }
  content.push(card).into()
}

const EMBEDDED_TRANSPORT_WIDTH: f32 = 140.0;
const EMBEDDED_TOOLS_WIDTH: f32 = 251.0;

fn embedded_inset(width: f32) -> f32 {
  if width < 600.0 {
    TOKENS.spacing.s3
  } else {
    TOKENS.spacing.s9
  }
}

fn embedded_controls_height(card_width: f32) -> f32 {
  let width = card_width - 2.0 * TOKENS.spacing.s6;
  if width >= EMBEDDED_TRANSPORT_WIDTH + 2.0 * EMBEDDED_TOOLS_WIDTH {
    if card_width > 900.0 - 2.0 * TOKENS.spacing.s9 {
      48.0
    } else {
      44.0
    }
  } else if width >= EMBEDDED_TRANSPORT_WIDTH + EMBEDDED_TOOLS_WIDTH + TOKENS.spacing.s4 {
    44.0 + TOKENS.spacing.s3 + 44.0
  } else {
    44.0 + TOKENS.spacing.s3 + 44.0 + TOKENS.spacing.s3 + 36.0
  }
}

fn controls_reveal_height(width: f32) -> f32 {
  let inset = embedded_inset(width);
  let card = 2.0 * TOKENS.spacing.s5
    + 14.0
    + TOKENS.spacing.s4
    + embedded_controls_height(width - 2.0 * inset);
  card + inset + 48.0
}

fn embedded_controls<'a>(
  identity: Element<'a, Message>,
  transport: Element<'a, Message>,
  tools: Element<'a, Message>,
  card_width: f32,
) -> Element<'a, Message> {
  let width = (card_width - 2.0 * TOKENS.spacing.s6).max(0.0);
  if width >= EMBEDDED_TRANSPORT_WIDTH + 2.0 * EMBEDDED_TOOLS_WIDTH {
    // Fixed equal side slots prevent intrinsic metadata width from moving play.
    let side = (width - EMBEDDED_TRANSPORT_WIDTH) / 2.0;
    row![
      container(identity).width(side),
      transport,
      container(tools).width(side).align_x(Alignment::End),
    ]
    .align_y(Alignment::Center)
    .width(Fill)
    .into()
  } else if width >= EMBEDDED_TRANSPORT_WIDTH + EMBEDDED_TOOLS_WIDTH + TOKENS.spacing.s4 {
    column![
      identity,
      row![transport, space::horizontal(), tools].align_y(Alignment::Center),
    ]
    .spacing(TOKENS.spacing.s3)
    .width(Fill)
    .into()
  } else {
    column![identity, transport, tools]
      .spacing(TOKENS.spacing.s3)
      .align_x(Alignment::Center)
      .width(Fill)
      .into()
  }
}

fn embedded_identity<'a>(
  state: &'a State,
  now_playing: &'a NowPlayingView,
  show_thumbnail: bool,
) -> Element<'a, Message> {
  let palette = &jellypilot_ui::tokens::DARK_PALETTE;
  let title = match state.playback.playable.as_ref() {
    Some(Playable::Library(item)) => item.name.as_str(),
    Some(Playable::Detail(item)) => item.name.as_str(),
    Some(Playable::Media(item)) => item.name.as_str(),
    None => now_playing.item.title.as_str(),
  };
  let caption = embedded_caption(state, now_playing);
  let title = control_button_content(
    move |_| {
      row![
        ellipsis_text(title)
          .font(iced::Font {
            weight: iced::font::Weight::Semibold,
            ..HEADING_FONT
          })
          .size(14)
          .line_height(iced::Pixels(20.0))
          .color(palette.text.heading),
        icon_with_color(
          Icon::ChevronUp,
          IconSize::Custom(14.0),
          palette.text.metadata
        ),
      ]
      .spacing(TOKENS.spacing.s1_5)
      .align_y(Alignment::Center)
      .into()
    },
    if state.playback.queue_menu_open {
      ButtonVariant::TonalActive
    } else {
      ButtonVariant::Tonal
    },
  )
  .style(cinema::control)
  .padding([TOKENS.spacing.s1, TOKENS.spacing.s1_5])
  .width(Length::Shrink)
  .min_height(28.0)
  .on_press_maybe(
    (!state.playback.view.busy && !matches!(state.playback.queue, QueueState::Unavailable))
      .then_some(Message::Playback(PlaybackMessage::QueueMenuToggled)),
  );
  let identity = column![
    title,
    container(
      ellipsis_text(caption)
        .size(12)
        .line_height(iced::Pixels(16.0))
        .color(palette.text.metadata)
    )
    .padding([0.0, TOKENS.spacing.s1_5]),
  ];
  let identity = if show_thumbnail {
    row![embedded_thumbnail(state), identity]
      .spacing(TOKENS.spacing.s3)
      .align_y(Alignment::Center)
      .width(Fill)
  } else {
    row![identity].width(Fill)
  };
  popover(
    identity,
    queue_content(state),
    state.playback.queue_menu_open,
    PopoverOptions {
      placement: Placement::Above,
      width: Some(320.0),
      appearance: PopoverAppearance::EmbeddedPlayer,
      ..PopoverOptions::default()
    },
    Message::Playback(PlaybackMessage::QueueMenuDismissed),
  )
}

fn embedded_caption(state: &State, now_playing: &NowPlayingView) -> String {
  let episode = match state.playback.playable.as_ref() {
    Some(Playable::Library(item)) => (
      item.series_name.as_deref(),
      item.season_number,
      item.episode_number,
    ),
    Some(Playable::Detail(item)) => (
      item.series_name.as_deref(),
      item.season_number,
      item.episode_number,
    ),
    Some(Playable::Media(item)) => (
      item.series_name.as_deref(),
      item.parent_index_number,
      item.index_number,
    ),
    None => (None, None, None),
  };
  let mut caption = match episode {
    (Some(series), Some(season), Some(episode)) => format!("{series} · S{season}:E{episode}"),
    (Some(series), _, _) => series.to_owned(),
    _ => media_type(state.kernel.locale, &now_playing.item.item_type),
  };
  if let Some(duration) = now_playing
    .duration_seconds
    .filter(|duration| duration.is_finite() && *duration > 0.0)
  {
    let minutes = ((duration - now_playing.position_seconds).max(0.0) / 60.0).ceil() as i64;
    caption.push_str(" · ");
    caption.push_str(&state.format("player-remaining-minutes", &[("minutes", minutes.into())]));
  }
  caption
}

fn embedded_thumbnail(state: &State) -> Element<'_, Message> {
  if let Some(handle) = state
    .playback
    .artwork
    .get(PLAYER_THUMBNAIL_KEY)
    .and_then(|cell| cell.handle())
  {
    return rounded_image(handle.clone(), full_radius(TOKENS.radii.md))
      .content_fit(ContentFit::Cover)
      .width(85)
      .height(48)
      .into();
  }
  container(icon_with_color(
    Icon::Movie,
    IconSize::Custom(22.0),
    jellypilot_ui::tokens::DARK_PALETTE.text.metadata,
  ))
  .width(85)
  .height(48)
  .align_x(Alignment::Center)
  .align_y(Alignment::Center)
  .into()
}
fn embedded_intro_prompt(state: &State) -> Option<Element<'_, Message>> {
  let prompt = state.playback.view.intro_prompt?;
  let ready = !state.playback.view.busy;
  let skip = control_button(
    Some(Icon::IntroSkip),
    Some(state.t(match prompt.kind {
      IntroSkipKind::Introduction => "player-skip-intro",
      IntroSkipKind::Credits => "player-skip-credits",
    })),
    ButtonVariant::Primary,
  )
  .label_size(13.0)
  .icon_size(IconSize::Sm)
  .min_height(40.0)
  .on_press_maybe(
    ready.then_some(Message::Playback(PlaybackMessage::Intent(Box::new(
      PlaybackIntent::SkipIntro,
    )))),
  );
  let dismiss = embedded_action(
    Icon::Close,
    state.t("common-dismiss"),
    ButtonVariant::Tonal,
    ready.then_some(Message::Playback(PlaybackMessage::Intent(Box::new(
      PlaybackIntent::DismissIntro,
    )))),
  );
  Some(opaque(
    container(
      row![skip, dismiss]
        .spacing(TOKENS.spacing.s2)
        .align_y(Alignment::Center),
    )
    .padding(TOKENS.spacing.s2)
    .style(cinema::popover),
  ))
}

fn embedded_timeline<'a>(state: &'a State, now_playing: &NowPlayingView) -> Element<'a, Message> {
  let position = state
    .playback
    .seek_preview
    .unwrap_or(now_playing.position_seconds);
  let Some(duration) = now_playing
    .duration_seconds
    .filter(|v| v.is_finite() && *v > 0.0)
  else {
    return text(format_duration(position))
      .font(MONO_FONT)
      .size(12)
      .line_height(iced::Pixels(14.0))
      .into();
  };
  let timeline = responsive(move |bounds| {
    let target = state
      .playback
      .seek_preview
      .or(embedded_player::seek_hover(state))
      .unwrap_or(position);
    let rail = mouse_area(tracked_slider(
      slider(
        0.0..=duration,
        position.clamp(0.0, duration),
        SliderEvent::Changed,
      )
      .step(1.0)
      .height(14)
      .width(Fill)
      .on_release(SliderEvent::DragEnded)
      .style(if state.playback.view.busy {
        cinema::unavailable_slider
      } else {
        cinema::timeline
      }),
      embedded_player::is_seek_dragging(state),
      |event| {
        Message::Playback(match event {
          SliderEvent::DragStarted => PlaybackMessage::SeekDragStarted,
          SliderEvent::Changed(value) => PlaybackMessage::SeekChanged(value),
          SliderEvent::DragEnded => PlaybackMessage::SeekReleased,
          SliderEvent::Adjusted(value) => PlaybackMessage::SeekAdjusted(value),
        })
      },
    ))
    .on_move(move |point| {
      Message::EmbeddedPlayer(embedded_player::Message::SeekHovered(Some(
        (f64::from(point.x / bounds.width.max(1.0)) * duration).clamp(0.0, duration),
      )))
    })
    .on_exit(Message::EmbeddedPlayer(
      embedded_player::Message::SeekHovered(None),
    ));
    let rail: Element<'_, Message> = iced::widget::tooltip(
      rail,
      text(format_duration(target)).font(MONO_FONT).size(12),
      iced::widget::tooltip::Position::FollowCursor,
    )
    .delay(std::time::Duration::ZERO)
    .gap(TOKENS.spacing.s3)
    .padding(TOKENS.spacing.s2)
    .snap_within_viewport(true)
    .style(cinema::popover)
    .into();
    if state.playback.view.busy && !embedded_player::is_seek_dragging(state) {
      inert(rail)
    } else {
      rail
    }
  })
  .height(14);
  row![
    text(format_duration(position))
      .font(MONO_FONT)
      .size(12)
      .line_height(iced::Pixels(14.0)),
    timeline,
    text(format_duration(duration))
      .font(MONO_FONT)
      .size(12)
      .line_height(iced::Pixels(14.0)),
  ]
  .spacing(TOKENS.spacing.s3)
  .align_y(Alignment::Center)
  .into()
}

fn intro_prompt(state: &State) -> Option<Element<'_, Message>> {
  let prompt = state.playback.view.intro_prompt?;
  let palette = playback_palette(state);
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

fn audio_popover(state: &State, icon_only: bool, embedded: bool) -> Element<'_, Message> {
  let palette = playback_palette(state);
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
    Some(if embedded {
      Icon::HeadphonesSquare
    } else {
      Icon::AudioTrack
    }),
    (!icon_only).then(|| state.t("player-audio")),
    audio_btn_variant,
  )
  .style(if embedded {
    cinema::framed_control
  } else {
    jellypilot_ui::widgets::button::style
  })
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press_maybe(
    (has_audio_choices && (!embedded || !state.playback.view.busy))
      .then_some(Message::Playback(PlaybackMessage::AudioMenuToggled)),
  );
  let trigger = if icon_only {
    let size = if embedded { 36.0 } else { 40.0 };
    trigger
      .width(Length::Fixed(size))
      .min_height(size)
      .padding(if embedded { [0, 0] } else { [6, 10] })
      .content_centered(embedded)
  } else {
    trigger
  };
  let trigger = menu_hint(trigger.into(), state.t("player-audio"), icon_only);
  let menu = match &state.playback.view.tracks {
    TracksView::Ready { tracks, audio, .. } => {
      let choices = track_choices(state.kernel.locale, tracks, "audio", false);
      let original_language = state
        .playback
        .view
        .now_playing
        .as_ref()
        .and_then(|view| view.item.original_language.as_deref());
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
          let is_original = original_language.is_some_and(|original| {
            choice
              .id
              .and_then(|id| tracks.iter().find(|track| track.id == id))
              .and_then(|track| track.language.as_deref())
              .is_some_and(|language| jellypilot_media_server::languages_match(language, original))
          });
          col = col.push(
            button({
              let mut row = row![text(choice.label).width(Fill).size(13)];
              if is_original {
                row = row.push(space::horizontal().width(TOKENS.spacing.s2)).push(
                  text(state.t("player-original-track"))
                    .size(11)
                    .color(palette.text.metadata),
                );
              }
              row.push(active_marker).align_y(Alignment::Center)
            })
            .padding(if embedded { [11, 10] } else { [6, 10] })
            .width(Fill)
            .on_press_maybe(
              (!embedded || !state.playback.view.busy)
                .then_some(Message::Playback(PlaybackMessage::AudioTrackSelected(id))),
            )
            .style(move |theme, status| {
              jellypilot_ui::theme::button_variant(
                theme,
                status,
                if embedded && active {
                  ButtonVariant::PillActive
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
  let menu: Element<'_, Message> = if embedded {
    container(scrollable(menu).style(jellypilot_ui::theme::scrollable))
      .height(Length::Fit.max(QUEUE_MENU_MAX_HEIGHT))
      .into()
  } else {
    menu.into()
  };

  popover(
    trigger,
    menu,
    state.playback.audio_menu_open,
    PopoverOptions {
      placement: Placement::Above,
      width: Some(240.0),
      appearance: if embedded {
        PopoverAppearance::EmbeddedPlayer
      } else {
        PopoverAppearance::Default
      },
      ..PopoverOptions::default()
    },
    Message::Playback(PlaybackMessage::AudioMenuDismissed),
  )
}

fn subtitle_popover(state: &State, icon_only: bool, embedded: bool) -> Element<'_, Message> {
  let palette = playback_palette(state);
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
  .style(if embedded {
    cinema::framed_control
  } else {
    jellypilot_ui::widgets::button::style
  })
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press_maybe(
    (has_subtitle_choices && (!embedded || !state.playback.view.busy))
      .then_some(Message::Playback(PlaybackMessage::SubtitleMenuToggled)),
  );
  let trigger = if icon_only {
    let size = if embedded { 36.0 } else { 40.0 };
    trigger
      .width(Length::Fixed(size))
      .min_height(size)
      .padding(if embedded { [0, 0] } else { [6, 10] })
      .content_centered(embedded)
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
          .padding(if embedded { [11, 10] } else { [6, 10] })
          .width(Fill)
          .on_press_maybe(
            (!embedded || !state.playback.view.busy).then_some(Message::Playback(
              PlaybackMessage::SubtitleTrackSelected(choice.id),
            )),
          )
          .style(move |theme, status| {
            jellypilot_ui::theme::button_variant(
              theme,
              status,
              if embedded && active {
                ButtonVariant::PillActive
              } else {
                ButtonVariant::Text
              },
            )
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
  let menu: Element<'_, Message> = if embedded {
    container(scrollable(menu).style(jellypilot_ui::theme::scrollable))
      .height(Length::Fit.max(QUEUE_MENU_MAX_HEIGHT))
      .into()
  } else {
    menu.into()
  };

  popover(
    trigger,
    menu,
    state.playback.subtitle_menu_open,
    PopoverOptions {
      placement: Placement::Above,
      width: Some(240.0),
      appearance: if embedded {
        PopoverAppearance::EmbeddedPlayer
      } else {
        PopoverAppearance::Default
      },
      ..PopoverOptions::default()
    },
    Message::Playback(PlaybackMessage::SubtitleMenuDismissed),
  )
}

/// Maximum height of the episode queue list before it scrolls.
const QUEUE_MENU_MAX_HEIGHT: f32 = 280.0;
const QUEUE_SCROLL_ID: &str = "player-queue";
const QUEUE_CURRENT_ID: &str = "player-queue-current";

/// Reveal the active row after the popover is laid out. Measure the row instead
/// of multiplying an index by a fixed height: localized titles can wrap.
pub(crate) fn reveal_current_queue_item() -> iced::Task<Message> {
  use iced::advanced::widget;

  struct Reveal {
    queue_id: widget::Id,
    current_id: widget::Id,
    viewport: Option<(iced::Rectangle, iced::Rectangle)>,
    current: Option<iced::Rectangle>,
  }
  impl<T: 'static> widget::Operation<T> for Reveal {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation<T>)) {
      visit(self);
    }

    fn scrollable(
      &mut self,
      id: Option<&widget::Id>,
      bounds: iced::Rectangle,
      content: iced::Rectangle,
      _translation: iced::Vector,
      _state: &mut dyn widget::operation::Scrollable,
    ) {
      if id == Some(&self.queue_id) {
        self.viewport = Some((bounds, content));
      }
    }

    fn container(&mut self, id: Option<&widget::Id>, bounds: iced::Rectangle) {
      if id == Some(&self.current_id) {
        self.current = Some(bounds);
      }
    }

    fn finish(&self) -> widget::operation::Outcome<T> {
      let (Some((viewport, content)), Some(current)) = (self.viewport, self.current) else {
        return widget::operation::Outcome::None;
      };
      let inset = ((viewport.height - current.height) / 2.0).max(0.0);
      widget::operation::Outcome::Chain(Box::new(widget::operation::scrollable::scroll_to(
        self.queue_id.clone(),
        scrollable::AbsoluteOffset {
          x: None,
          y: Some((current.y - content.y - inset).max(0.0)),
        },
      )))
    }
  }
  widget::operate(Reveal {
    queue_id: widget::Id::new(QUEUE_SCROLL_ID),
    current_id: widget::Id::new(QUEUE_CURRENT_ID),
    viewport: None,
    current: None,
  })
}

/// Current-season episode queue popover shared by the bar and the compact
/// player. Rows follow season episode order; the actively playing episode is
/// marked and not selectable.
fn queue_popover(state: &State, icon_only: bool) -> Element<'_, Message> {
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
  let menu = queue_content(state);

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

fn playback_palette(state: &State) -> &'static jellypilot_ui::tokens::ThemePalette {
  if embedded_player::active(state) {
    &jellypilot_ui::tokens::DARK_PALETTE
  } else {
    state.palette()
  }
}

fn queue_content(state: &State) -> Element<'_, Message> {
  let palette = playback_palette(state);
  let embedded = embedded_player::active(state);
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
          let row = button(row![label, marker].align_y(Alignment::Center))
            .padding(if embedded { [11, 10] } else { [6, 10] })
            .width(Fill)
            .on_press_maybe(
              (!is_current && (!embedded || !state.playback.view.busy)).then_some(
                Message::Playback(PlaybackMessage::QueueItemSelected(Box::new(item.clone()))),
              ),
            )
            .style(move |theme, status| {
              jellypilot_ui::theme::button_variant(theme, status, row_variant)
            });
          rows = rows.push(if is_current {
            Element::from(container(row).id(QUEUE_CURRENT_ID).width(Fill))
          } else {
            row.into()
          });
        }
        container(
          scrollable(rows)
            .id(QUEUE_SCROLL_ID)
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
  menu
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
  let palette = playback_palette(state);
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
  async fn picture_click_dismisses_queue_without_toggling_playback() {
    use iced::advanced::renderer::Headless;
    use iced::{mouse, Event, Point, Size};
    use iced_runtime::user_interface::{Cache, UserInterface};

    let mut state = State::boot(false);
    state.playback.view.now_playing = Some(test_now_playing());
    state.playback.queue = QueueState::Ready(Vec::new());
    state.playback.queue_menu_open = true;
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    let mut cache = Cache::new();
    for menu_open in [true, false] {
      state.playback.queue_menu_open = menu_open;
      let mut ui = UserInterface::build(
        embedded(&state),
        Size::new(1100.0, 700.0),
        cache,
        &mut renderer,
      );
      let mut bus = iced::advanced::shell::Bus::new();
      let _ = ui.update(
        &iced::window::Headless,
        &iced::advanced::shell::Waker::noop(),
        &[Event::Mouse(mouse::Event::ButtonPressed(
          mouse::Button::Left,
        ))],
        mouse::Cursor::Available(Point::new(550.0, 200.0)),
        &mut renderer,
        &mut bus,
      );
      let messages: Vec<_> = bus.drain().map(|(message, _)| message).collect();
      assert_eq!(
        messages.iter().any(|message| matches!(
          message,
          Message::Playback(PlaybackMessage::QueueMenuDismissed)
        )),
        menu_open,
      );
      assert_eq!(
        messages.iter().any(|message| matches!(
          message, Message::Playback(PlaybackMessage::Intent(intent))
            if matches!(**intent, PlaybackIntent::TogglePaused)
        )),
        !menu_open,
      );
      cache = ui.into_cache();
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
      audio_popover(&state, true, false),
      bounds,
      Cache::new(),
      &mut renderer,
    );
    ui.operate(&renderer, &mut Focus);
    let mut cache = ui.into_cache();
    for open in [true, false] {
      state.playback.audio_menu_open = open;
      cache = UserInterface::build(
        audio_popover(&state, true, false),
        bounds,
        cache,
        &mut renderer,
      )
      .into_cache();
    }
    let mut ui = UserInterface::build(
      audio_popover(&state, true, false),
      bounds,
      cache,
      &mut renderer,
    );
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

  #[tokio::test]
  async fn queue_trigger_includes_padding_but_excludes_caption_and_unused_width() {
    use iced::advanced::renderer::Headless;
    use iced::{mouse, Event, Point};
    use iced_runtime::user_interface::{Cache, UserInterface};

    let mut state = State::boot(false);
    state.playback.queue = QueueState::Ready(Vec::new());
    let mut playing = test_now_playing();
    playing.item.title = "Pilot".to_owned();
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    let mut ui = UserInterface::build(
      embedded_identity(&state, &playing, false),
      iced::Size::new(500.0, 44.0),
      Cache::new(),
      &mut renderer,
    );
    for (position, opens_queue) in [
      (Point::new(2.0, 2.0), true),
      (Point::new(6.0, 10.0), true),
      (Point::new(400.0, 10.0), false),
      (Point::new(6.0, 36.0), false),
    ] {
      let mut bus = iced::advanced::shell::Bus::new();
      let _ = ui.update(
        &iced::window::Headless,
        &iced::advanced::shell::Waker::noop(),
        &[
          Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
          Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ],
        mouse::Cursor::Available(position),
        &mut renderer,
        &mut bus,
      );
      assert_eq!(
        bus
          .drain()
          .filter(|(message, _)| matches!(
            message,
            Message::Playback(PlaybackMessage::QueueMenuToggled)
          ))
          .count(),
        usize::from(opens_queue),
        "queue activation at {position:?}",
      );
    }
  }

  #[tokio::test]
  async fn mute_target_captures_its_padding_but_not_subtitles_or_volume_slider() {
    use iced::advanced::renderer::Headless;
    use iced::{mouse, Event, Point};
    use iced_runtime::user_interface::{Cache, UserInterface};

    let state = State::boot(false);
    let mut playing = test_now_playing();
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    for (volume, muted) in [
      (0.0, false),
      (33.0, false),
      (66.0, false),
      (67.0, false),
      (67.0, true),
    ] {
      playing.volume = volume;
      playing.muted = muted;
      let mut ui = UserInterface::build(
        embedded_bar(&state, &playing, 900.0),
        iced::Size::new(900.0, 118.0),
        Cache::new(),
        &mut renderer,
      );
      // Right-aligned tools end at x=876; mute spans x=705..739.
      for (x, mutes) in [(706.0, true), (738.0, true), (704.0, false), (740.0, false)] {
        let mut bus = iced::advanced::shell::Bus::new();
        let _ = ui.update(
          &iced::window::Headless,
          &iced::advanced::shell::Waker::noop(),
          &[
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
          ],
          mouse::Cursor::Available(Point::new(x, 74.0)),
          &mut renderer,
          &mut bus,
        );
        assert_eq!(
          bus
            .drain()
            .filter(|(message, _)| matches!(
              message,
              Message::Playback(PlaybackMessage::Intent(intent))
                if matches!(**intent, PlaybackIntent::SetMuted(target) if target != playing.muted)
            ))
            .count(),
          usize::from(mutes),
          "mute activation at x={x}",
        );
      }
    }
  }

  #[tokio::test]
  async fn embedded_card_keeps_transport_centered_and_reflows_only_when_tools_need_space() {
    use iced::advanced::{layout, renderer::Headless, widget::Tree, Layout};
    use iced::{Point, Rectangle, Size};

    fn boxes(layout: Layout<'_>, result: &mut Vec<Rectangle>) {
      result.push(layout.bounds());
      for child in layout.children() {
        boxes(child, result);
      }
    }

    let state = State::boot(false);
    let mut now_playing = test_now_playing();
    now_playing.item.title =
      "An unusually long episode title that must truncate, not move play ".repeat(8);
    now_playing.duration_seconds = Some(360_000.0);
    let renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    for (window_width, expected_height) in [
      (1099.0, 118.0),
      (900.0, 114.0),
      (768.0, 114.0),
      (700.0, 170.0),
      (400.0, 218.0),
    ] {
      let card_width = window_width - 2.0 * embedded_inset(window_width);
      let mut content = embedded_bar(&state, &now_playing, card_width);
      let mut tree = Tree::new(&content);
      tree.diff(&mut content);
      let node = content.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::new(card_width, 0.0), Size::new(card_width, 900.0)),
      );
      assert_eq!(
        node.size().height,
        expected_height,
        "window width {window_width}"
      );
      let mut measured = Vec::new();
      boxes(Layout::new(&node), &mut measured);
      for frame in measured
        .iter()
        .filter(|bounds| bounds.size() == Size::new(36.0, 36.0))
      {
        for glyph in measured.iter().filter(|bounds| {
          bounds.size() == Size::new(16.0, 16.0) && frame.contains(bounds.center())
        }) {
          assert!(
            (glyph.center().x - frame.center().x).abs() < 0.5,
            "framed track icons must be centered: {frame:?} / {glyph:?}"
          );
        }
      }
      assert!(
        measured
          .iter()
          .all(|bounds| bounds.x >= -0.5 && bounds.x + bounds.width <= card_width + 0.5),
        "controls and long timestamps must not overflow at {window_width}: {measured:?}"
      );
      let primary = measured
        .iter()
        .find(|bounds| bounds.size() == Size::new(44.0, 44.0))
        .expect("primary play control remains full size");
      if window_width != 700.0 {
        assert!(
          (primary.center().x - Point::new(card_width / 2.0, 0.0).x).abs() < 0.5,
          "play must stay centered despite long metadata at {window_width}: {primary:?}"
        );
      }
      assert!(
        controls_reveal_height(window_width) >= expected_height + embedded_inset(window_width),
        "pointer reveal region must contain the entire responsive card"
      );
    }
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
        original_language: None,
      },
      paused: false,
      position_seconds: 120.0,
      duration_seconds: Some(2_400.0),
      volume: 85.0,
      muted: false,
    }
  }

  #[tokio::test]
  async fn embedded_timeline_previews_without_seeking_until_release_and_blocks_busy_input() {
    use iced::advanced::renderer::Headless;
    use iced::{mouse, Event, Point};
    use iced_runtime::user_interface::{Cache, UserInterface};

    let mut state = State::boot(false);
    let now_playing = test_now_playing();
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    for busy in [false, true] {
      state.playback.view.busy = busy;
      state.playback.seek_dragging = false;
      state.playback.seek_preview = None;
      let mut ui = UserInterface::build(
        embedded_timeline(&state, &now_playing),
        iced::Size::new(600.0, 14.0),
        Cache::new(),
        &mut renderer,
      );
      let mut bus = iced::advanced::shell::Bus::new();
      let hover = Point::new(300.0, 7.0);
      let _ = ui.update(
        &iced::window::Headless,
        &iced::advanced::shell::Waker::noop(),
        &[Event::Mouse(mouse::Event::CursorMoved { position: hover })],
        mouse::Cursor::Available(hover),
        &mut renderer,
        &mut bus,
      );
      let hover_messages: Vec<_> = bus.drain().map(|(message, _)| message).collect();
      assert!(
        !hover_messages
          .iter()
          .any(|message| matches!(message, Message::Playback(_))),
        "hovering must not alter playback"
      );
      assert_eq!(
        hover_messages.iter().any(|message| matches!(
          message,
          Message::EmbeddedPlayer(embedded_player::Message::SeekHovered(Some(_)))
        )),
        !busy
      );

      for (event, cursor) in [
        (mouse::Event::ButtonPressed(mouse::Button::Left), hover),
        (
          mouse::Event::CursorMoved {
            position: Point::new(450.0, 7.0),
          },
          Point::new(450.0, 7.0),
        ),
      ] {
        let _ = ui.update(
          &iced::window::Headless,
          &iced::advanced::shell::Waker::noop(),
          &[Event::Mouse(event)],
          mouse::Cursor::Available(cursor),
          &mut renderer,
          &mut bus,
        );
      }
      let drag_messages: Vec<_> = bus.drain().map(|(message, _)| message).collect();
      assert!(
        !drag_messages
          .iter()
          .any(|message| matches!(message, Message::Playback(PlaybackMessage::SeekReleased))),
        "dragging is a preview, not an immediate seek"
      );
      assert_eq!(
        drag_messages.iter().any(|message| matches!(
          message, Message::Playback(PlaybackMessage::SeekChanged(value)) if *value > 120.0
        )),
        !busy
      );

      // A remote command can make playback busy while the pointer is still held.
      // The existing drag must retain its widget state and deliver its release.
      let cache = ui.into_cache();
      state.playback.seek_dragging = !busy;
      state.playback.seek_preview = drag_messages.iter().rev().find_map(|message| {
        if let Message::Playback(PlaybackMessage::SeekChanged(position)) = message {
          Some(*position)
        } else {
          None
        }
      });
      state.playback.view.busy = true;
      let mut ui = UserInterface::build(
        embedded_timeline(&state, &now_playing),
        iced::Size::new(600.0, 14.0),
        cache,
        &mut renderer,
      );

      let _ = ui.update(
        &iced::window::Headless,
        &iced::advanced::shell::Waker::noop(),
        &[Event::Mouse(mouse::Event::ButtonReleased(
          mouse::Button::Left,
        ))],
        mouse::Cursor::Available(Point::new(450.0, 7.0)),
        &mut renderer,
        &mut bus,
      );
      assert_eq!(
        bus
          .drain()
          .filter(|(message, _)| matches!(
            message,
            Message::Playback(PlaybackMessage::SeekReleased)
          ))
          .count(),
        usize::from(!busy)
      );
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
          community_rating: None,
          episode_count: None,
          last_played_date: None,
          premiere_date: None,
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
              original_language: None,
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
                  original_language: None,
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
