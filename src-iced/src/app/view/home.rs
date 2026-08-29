use crate::app::message::{HomeMessage, Message, PlaybackMessage};
use crate::app::state::{has_resume_position, ArtworkCell, ArtworkCellState, HomeSection, State};
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, space, text, Column, Row};
use iced::{Alignment, ContentFit, Element, Fill, Length};
use jellypilot_core::cards::{hero_headline, hero_metadata, item_caption};
use jellypilot_core::LoadState;
use jellypilot_media_server::VideoLibraryItem;
use jellypilot_mpv::playback::{Playable, PlaybackStartPosition};
use jellypilot_mpv::playback_session::PlaybackIntent;
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::icons::{
  icon_for_variant, icon_for_variant_disabled, icon_with_color, Icon, IconSize,
};
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};
use jellypilot_ui::{card_top_radius, full_radius, poster_card, rounded_image};
const THUMB_FRAME_WIDTH: f32 = 240.0;
const THUMB_FRAME_HEIGHT: f32 = 135.0;
const POSTER_FRAME_WIDTH: f32 = 160.0;
const POSTER_FRAME_HEIGHT: f32 = 240.0;

/// Content width used to classify initially visible home cards before a
/// measured layout exists: default window width minus the shell's outer
/// padding, sidebar, sidebar-content gap, and the home page padding.
pub(crate) const HOME_CONTENT_WIDTH: f32 = 1600.0
    - TOKENS.spacing.s3 * 2.0
    - super::shell::SIDEBAR_WIDTH
    - TOKENS.spacing.s4
    - TOKENS.spacing.s8 * 2.0;

pub(crate) const fn section_frame_size(section: HomeSection) -> (f32, f32) {
  match section {
    HomeSection::ContinueWatching | HomeSection::NextUp => (THUMB_FRAME_WIDTH, THUMB_FRAME_HEIGHT),
    HomeSection::LatestMovies | HomeSection::LatestEpisodes => {
      (POSTER_FRAME_WIDTH, POSTER_FRAME_HEIGHT)
    }
  }
}

const fn section_scroll_height(section: HomeSection) -> f32 {
  match section {
    HomeSection::ContinueWatching | HomeSection::NextUp => 280.0,
    HomeSection::LatestMovies | HomeSection::LatestEpisodes => 296.0,
  }
}

pub fn view(state: &State) -> Element<'_, Message> {
  let mut content = Column::new()
    .spacing(TOKENS.spacing.s8)
    .padding([TOKENS.spacing.s6, TOKENS.spacing.s8])
    .width(Fill);

  if let Some(item) = state.home.featured_item() {
    content = content.push(featured_hero(state, item));
  } else if home_is_loading(state) {
    content = content.push(featured_skeleton());
  }

  for section in HomeSection::ALL {
    if let Some(row) = section_view(state, section) {
      content = content.push(row);
    }
  }

  scrollable(content)
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable)
    .into()
}

fn home_is_loading(state: &State) -> bool {
  HomeSection::ALL
    .iter()
    .any(|section| matches!(state.home.section(*section), LoadState::Loading))
}

fn featured_hero<'a>(state: &'a State, item: &'a VideoLibraryItem) -> Element<'a, Message> {
  let artwork = hero_artwork(
    state,
    state.home_artwork.hero(&item.id),
    &item.name,
    220.0,
    330.0,
  );
  let mut copy = column![
    text(hero_headline(item))
      .font(SPACE_GROTESK_FONT)
      .size(42)
      .color(TOKENS.colors.onSurface),
    text(hero_metadata(item))
      .size(17)
      .color(TOKENS.colors.onSurfaceVariant),
  ]
  .spacing(TOKENS.spacing.s3)
  .width(Fill);
  if let Some(overview) = item
    .overview
    .as_deref()
    .filter(|overview| !overview.trim().is_empty())
  {
    copy = copy.push(
      text(overview)
        .size(15)
        .color(TOKENS.colors.onSurfaceVariant),
    );
  }

  let play_label = if has_resume_position(item) {
    "Resume"
  } else {
    "Play"
  };
  let play_enabled = state.playback_view.engine_available;
  let play = button(
    row![
      icon_for_variant_disabled(
        Icon::Play,
        IconSize::Md,
        ButtonVariant::Primary,
        !play_enabled,
      ),
      text(play_label),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center),
  )
  .padding([7, 14])
  .on_press_maybe(play_enabled.then(|| play_message(state, item)))
  .style(|theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Primary)
  });
  let details = button(
    row![
      icon_for_variant(Icon::Info, IconSize::Md, ButtonVariant::Outlined),
      text("Details"),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center),
  )
  .padding([7, 14])
  .on_press(Message::OpenDetail(item.clone()))
  .style(|theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
  });
  container(
    row![
      artwork,
      copy,
      column![play, details].spacing(TOKENS.spacing.s2)
    ]
    .spacing(TOKENS.spacing.s8)
    .align_y(Alignment::Center),
  )
  .padding(TOKENS.spacing.s6)
  .width(Fill)
  .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Elevated))
  .into()
}

fn featured_skeleton<'a>() -> Element<'a, Message> {
  let poster = skeleton_box(220.0, 330.0);
  let copy = column![
    skeleton_box(360.0, 44.0),
    skeleton_box(240.0, 20.0),
    skeleton_box(520.0, 72.0),
  ]
  .spacing(TOKENS.spacing.s4);
  container(row![poster, copy].spacing(TOKENS.spacing.s8))
    .padding(TOKENS.spacing.s6)
    .width(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Elevated))
    .into()
}

fn section_view(state: &State, section: HomeSection) -> Option<Element<'_, Message>> {
  match state.home.section(section) {
    LoadState::Idle => None,
    LoadState::Loading => Some(section_skeleton(section)),
    LoadState::Failed(error) => Some(section_error(section.title(), error)),
    LoadState::Ready(items) if items.is_empty() => None,
    LoadState::Ready(items) => Some(section_row(state, section, items)),
  }
}

fn section_row<'a>(
  state: &'a State,
  section: HomeSection,
  items: &'a [VideoLibraryItem],
) -> Element<'a, Message> {
  let mut cards = Row::new()
    .spacing(TOKENS.spacing.s4)
    .align_y(Alignment::Start);
  for item in items {
    cards = cards.push(video_card(state, section, item));
  }
  let cards = scrollable(cards)
    .direction(Direction::Horizontal(Scrollbar::new()))
    .height(section_scroll_height(section))
    .style(jellypilot_ui::theme::scrollable);

  column![
    text(section.title())
      .font(SPACE_GROTESK_FONT)
      .size(24)
      .color(TOKENS.colors.onSurface),
    cards,
  ]
  .spacing(TOKENS.spacing.s3)
  .into()
}

fn video_card<'a>(
  state: &'a State,
  section: HomeSection,
  item: &'a VideoLibraryItem,
) -> Element<'a, Message> {
  let (frame_width, frame_height) = section_frame_size(section);
  let is_action_card = matches!(section, HomeSection::ContinueWatching | HomeSection::NextUp);
  let radius = if is_action_card {
    card_top_radius(TOKENS.radii.xl)
  } else {
    full_radius(TOKENS.radii.lg)
  };
  let poster = card_artwork(
    state,
    state.home_artwork.card(section, &item.id),
    &item.name,
    frame_width,
    frame_height,
    radius,
  );

  let text_stack = column![
    text(&item.name)
      .size(14)
      .color(TOKENS.colors.onSurface)
      .wrapping(Wrapping::None),
    text(item_caption(item))
      .size(12)
      .color(TOKENS.colors.onSurfaceVariant)
      .wrapping(Wrapping::None),
  ]
  .spacing(TOKENS.spacing.s1)
  .width(Fill);

  if is_action_card {
    let progress_element: Option<Element<'a, Message>> =
      card_progress(section, item).map(progress_bar);

    let play_label = if has_resume_position(item) {
      "Resume"
    } else {
      "Play"
    };
    let play_enabled = state.playback_view.engine_available;
    let play = button(
      row![
        icon_for_variant_disabled(
          Icon::Play,
          IconSize::Xs,
          ButtonVariant::Primary,
          !play_enabled,
        ),
        text(play_label),
      ]
      .spacing(TOKENS.spacing.s1)
      .align_y(Alignment::Center),
    )
    .padding([6, 10])
    .on_press_maybe(play_enabled.then(|| play_message(state, item)))
    .style(|theme, status| {
      jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Primary)
    });
    let details = button(
      row![
        icon_for_variant(Icon::Info, IconSize::Xs, ButtonVariant::Text),
        text("Details"),
      ]
      .spacing(TOKENS.spacing.s1)
      .align_y(Alignment::Center),
    )
    .on_press(Message::OpenDetail(item.clone()))
    .style(|theme, status| {
      jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Text)
    });

    let copy = container(
      column![text_stack, row![play, details].spacing(TOKENS.spacing.s2),]
        .spacing(TOKENS.spacing.s3)
        .width(Fill),
    )
    .padding(iced::Padding {
      top: TOKENS.spacing.s3,
      right: TOKENS.spacing.s4,
      bottom: TOKENS.spacing.s4,
      left: TOKENS.spacing.s4,
    })
    .width(Fill);

    let mut card_column = Column::new().width(Fill).push(poster);
    if let Some(prog) = progress_element {
      card_column = card_column.push(container(prog).height(4).width(frame_width));
    }
    card_column = card_column.push(copy);

    return container(card_column)
      .width(frame_width)
      .clip(true)
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Filled))
      .into();
  }

  let copy = column![
    text(&item.name)
      .size(14)
      .color(TOKENS.colors.onSurface)
      .wrapping(Wrapping::None),
    text(item_caption(item))
      .size(12)
      .color(TOKENS.colors.onSurfaceVariant)
      .wrapping(Wrapping::None),
  ]
  .spacing(TOKENS.spacing.s1)
  .padding(iced::Padding {
    top: TOKENS.spacing.s2,
    right: 0.0,
    bottom: 0.0,
    left: 0.0,
  })
  .width(frame_width);

  poster_card(poster, copy)
    .width(frame_width)
    .on_press(Message::OpenDetail(item.clone()))
    .into()
}

fn play_message(state: &State, item: &VideoLibraryItem) -> Message {
  Message::Playback(PlaybackMessage::Intent(PlaybackIntent::Start {
    item: Playable::Library(item.clone()),
    position: if has_resume_position(item) {
      PlaybackStartPosition::Resume
    } else {
      PlaybackStartPosition::Beginning
    },
    intro: state.intro_availability(),
    selection: Box::default(),
  }))
}

fn hero_artwork<'a>(
  state: &'a State,
  cell: Option<&ArtworkCell>,
  name: &'a str,
  width: f32,
  height: f32,
) -> Element<'a, Message> {
  if let Some(cell) = cell {
    if cell.state == ArtworkCellState::Ready {
      if let Some(handle) = state.artwork_handles.get(cell.slot, &cell.image_id) {
        return rounded_image(handle.clone(), full_radius(TOKENS.radii.xl))
          .content_fit(ContentFit::Cover)
          .width(width)
          .height(height)
          .into();
      }
    }
  }

  let failed = cell.is_some_and(|cell| cell.state == ArtworkCellState::Failed);
  let placeholder_color = if failed {
    TOKENS.colors.warning
  } else {
    TOKENS.colors.onSurfaceVariant
  };
  let initial = name
    .trim()
    .chars()
    .next()
    .map(|character| character.to_uppercase().collect::<String>())
    .unwrap_or_else(|| "•".to_owned());
  container(
    column![
      icon_with_color(Icon::Movie, 42.0, placeholder_color),
      text(initial)
        .font(SPACE_GROTESK_FONT)
        .size(32)
        .color(placeholder_color),
    ]
    .spacing(TOKENS.spacing.s1)
    .align_x(Alignment::Center),
  )
  .width(width)
  .height(height)
  .center_x(Fill)
  .center_y(Fill)
  .style(|_theme| container::Style {
    background: Some(iced::Background::Color(
      TOKENS.colors.surfaceContainerLowest,
    )),
    border: iced::Border {
      radius: full_radius(TOKENS.radii.xl),
      width: 0.0,
      color: iced::Color::TRANSPARENT,
    },
    ..container::Style::default()
  })
  .into()
}

fn card_artwork<'a>(
  state: &'a State,
  cell: Option<&ArtworkCell>,
  name: &'a str,
  width: f32,
  height: f32,
  radius: iced::border::Radius,
) -> Element<'a, Message> {
  if let Some(cell) = cell {
    if cell.state == ArtworkCellState::Ready {
      if let Some(handle) = state.artwork_handles.get(cell.slot, &cell.image_id) {
        return rounded_image(handle.clone(), radius)
          .content_fit(ContentFit::Cover)
          .width(width)
          .height(height)
          .into();
      }
    }
  }

  let failed = cell.is_some_and(|cell| cell.state == ArtworkCellState::Failed);
  let placeholder_color = if failed {
    TOKENS.colors.warning
  } else {
    TOKENS.colors.onSurfaceVariant
  };
  let initial = name
    .trim()
    .chars()
    .next()
    .map(|character| character.to_uppercase().collect::<String>())
    .unwrap_or_else(|| "•".to_owned());
  let icon_dim = if width > POSTER_FRAME_WIDTH {
    42.0
  } else {
    32.0
  };
  container(
    column![
      icon_with_color(Icon::Movie, icon_dim, placeholder_color),
      text(initial)
        .font(SPACE_GROTESK_FONT)
        .size(if width > POSTER_FRAME_WIDTH { 32 } else { 24 })
        .color(placeholder_color),
    ]
    .spacing(TOKENS.spacing.s1)
    .align_x(Alignment::Center),
  )
  .width(width)
  .height(height)
  .center_x(Fill)
  .center_y(Fill)
  .style(move |_theme| container::Style {
    background: Some(iced::Background::Color(
      TOKENS.colors.surfaceContainerLowest,
    )),
    border: iced::Border {
      radius,
      width: 0.0,
      color: iced::Color::TRANSPARENT,
    },
    ..container::Style::default()
  })
  .into()
}
fn card_progress(section: HomeSection, item: &VideoLibraryItem) -> Option<f64> {
  if section != HomeSection::ContinueWatching
    && (section != HomeSection::NextUp || !has_resume_position(item))
  {
    return None;
  }
  if let Some(percentage) = item.played_percentage.filter(|value| value.is_finite()) {
    return Some(percentage.clamp(0.0, 100.0));
  }
  match (item.resume_position_seconds, item.runtime_seconds) {
    (Some(position), Some(runtime))
      if position.is_finite() && position >= 0.0 && runtime.is_finite() && runtime > 0.0 =>
    {
      Some((position / runtime * 100.0).clamp(0.0, 100.0))
    }
    _ => None,
  }
}

fn progress_bar<'a>(progress: f64) -> Element<'a, Message> {
  let filled = (progress.round() as u16).min(100);
  let remaining = 100_u16.saturating_sub(filled);
  // Zero FillPortion lays out as a non-fluid child against the full width, so
  // omit empty segments entirely at 0% and 100%.
  let mut bar = Row::new().width(Fill).height(4);
  if filled > 0 {
    bar = bar.push(
      container(space::horizontal())
        .width(Length::FillPortion(filled))
        .height(4)
        .style(|_| iced::widget::container::Style::default().background(TOKENS.colors.primary)),
    );
  }
  if remaining > 0 {
    bar = bar.push(
      container(space::horizontal())
        .width(Length::FillPortion(remaining))
        .height(4)
        .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Elevated)),
    );
  }
  bar.into()
}

fn section_skeleton<'a>(section: HomeSection) -> Element<'a, Message> {
  let (width, height) = section_frame_size(section);
  let mut cards = Row::new().spacing(TOKENS.spacing.s4);
  for _ in 0..5 {
    cards = cards.push(
      column![
        skeleton_box(width, height),
        skeleton_box(width, 18.0),
        skeleton_box(width * 0.6, 14.0),
      ]
      .spacing(TOKENS.spacing.s2),
    );
  }
  column![
    text(section.title())
      .font(SPACE_GROTESK_FONT)
      .size(24)
      .color(TOKENS.colors.onSurface),
    cards,
  ]
  .spacing(TOKENS.spacing.s3)
  .into()
}

fn section_error<'a>(title: &'static str, error: &'a str) -> Element<'a, Message> {
  let retry = button(text("Retry"))
    .padding([6, 12])
    .on_press(Message::Home(HomeMessage::Retry))
    .style(|theme, status| {
      jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
    });
  container(
    column![
      text(title)
        .font(SPACE_GROTESK_FONT)
        .size(24)
        .color(TOKENS.colors.onSurface),
      text(error).size(13).color(TOKENS.colors.error),
      retry,
    ]
    .spacing(TOKENS.spacing.s3),
  )
  .padding(TOKENS.spacing.s4)
  .width(Fill)
  .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Elevated))
  .into()
}

fn skeleton_box<'a>(width: f32, height: f32) -> Element<'a, Message> {
  container(space::horizontal())
    .width(width)
    .height(height)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Elevated))
    .into()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn section_frame_sizes_and_row_heights_match_aspect_ratios() {
    let (cw_w, cw_h) = section_frame_size(HomeSection::ContinueWatching);
    assert_eq!((cw_w, cw_h), (THUMB_FRAME_WIDTH, THUMB_FRAME_HEIGHT));
    assert_eq!(section_scroll_height(HomeSection::ContinueWatching), 280.0);

    let (mov_w, mov_h) = section_frame_size(HomeSection::LatestMovies);
    assert_eq!((mov_w, mov_h), (POSTER_FRAME_WIDTH, POSTER_FRAME_HEIGHT));
    assert_eq!(section_scroll_height(HomeSection::LatestMovies), 296.0);
  }
}
