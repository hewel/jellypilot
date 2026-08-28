use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{button, column, container, image, row, scrollable, space, text, Column, Row};
use iced::{Alignment, ContentFit, Element, Fill, Length};
use jellypilot_core::cards::{card_frame_size, hero_headline, hero_metadata, item_caption};
use jellypilot_core::LoadState;
use jellypilot_media_server::VideoLibraryItem;
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};

use crate::app::message::{HomeMessage, Message};
use crate::app::state::{has_resume_position, ArtworkCell, ArtworkCellState, HomeSection, State};

const POSTER_WIDTH: f32 = 160.0;
const POSTER_HEIGHT: f32 = 240.0;
const CARD_HEIGHT: f32 = 320.0;

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
  let artwork = artwork(
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

  container(
    row![artwork, copy]
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
    LoadState::Loading => Some(section_skeleton(section.title())),
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
    .height(CARD_HEIGHT)
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
  let (frame_width, frame_height) = card_frame_size(item);
  let frame_width = frame_width as f32;
  let frame_height = frame_height as f32;
  let poster = artwork(
    state,
    state.home_artwork.card(section, &item.id),
    &item.name,
    frame_width,
    frame_height,
  );
  let mut content = column![
    poster,
    text(&item.name).size(14).color(TOKENS.colors.onSurface),
    text(item_caption(item))
      .size(12)
      .color(TOKENS.colors.onSurfaceVariant),
  ]
  .spacing(TOKENS.spacing.s2)
  .width(frame_width);
  if let Some(progress) = card_progress(section, item) {
    content = content.push(progress_bar(progress));
  }

  container(content)
    .width(frame_width)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Filled))
    .into()
}

fn artwork<'a>(
  state: &'a State,
  cell: Option<&ArtworkCell>,
  name: &'a str,
  width: f32,
  height: f32,
) -> Element<'a, Message> {
  if let Some(cell) = cell {
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

  let failed = cell.is_some_and(|cell| cell.state == ArtworkCellState::Failed);
  let initial = name
    .trim()
    .chars()
    .next()
    .map(|character| character.to_uppercase().collect::<String>())
    .unwrap_or_else(|| "•".to_owned());
  container(
    text(initial)
      .font(SPACE_GROTESK_FONT)
      .size(if width > POSTER_WIDTH { 54 } else { 38 })
      .color(if failed {
        TOKENS.colors.warning
      } else {
        TOKENS.colors.onSurfaceVariant
      }),
  )
  .width(width)
  .height(height)
  .center_x(Fill)
  .center_y(Fill)
  .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Elevated))
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

fn section_skeleton<'a>(title: &'static str) -> Element<'a, Message> {
  let mut cards = Row::new().spacing(TOKENS.spacing.s4);
  for _ in 0..5 {
    cards = cards.push(
      column![
        skeleton_box(POSTER_WIDTH, POSTER_HEIGHT),
        skeleton_box(POSTER_WIDTH, 18.0),
        skeleton_box(96.0, 14.0),
      ]
      .spacing(TOKENS.spacing.s2),
    );
  }
  column![
    text(title)
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
    .padding([8, 14])
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
