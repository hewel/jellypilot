use crate::app::message::{BrowseMessage, Message};
use crate::app::state::{ArtworkCell, ArtworkCellState, Destination, State};
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, space, text, Column};
use iced::{Alignment, ContentFit, Element, Fill};
use jellypilot_core::browse_model::{LibraryBrowseView, LibraryItemSlot};
use jellypilot_core::cards::item_caption;
use jellypilot_core::{LibraryBrowseFailure, LIBRARY_BROWSE_PAGE_SIZE};
use jellypilot_media_server::{
  VideoLibraryItem, VideoLibraryPlayedFilter, VideoLibrarySort, VideoLibrarySortDirection,
};
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::icons::{
  icon_for_variant, icon_for_variant_disabled, icon_with_color, Icon, IconSize,
};
use jellypilot_ui::overlay::{popover, PopoverOptions};
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::ButtonVariant;
use jellypilot_ui::widgets::artwork_grid::{artwork_grid, ArtworkGridMetrics, ArtworkGridViewport};
use jellypilot_ui::{full_radius, rounded_image};

const PAGE_PADDING: f32 = 32.0;
const CARD_COPY_HEIGHT: f32 = 68.0;
pub fn view(state: &State) -> Element<'_, Message> {
  let title = match &state.destination {
    Destination::Library { library_id, .. } => match &state.home.shortcuts {
      jellypilot_core::LoadState::Ready(shortcuts) => shortcuts
        .iter()
        .find(|shortcut| shortcut.id == *library_id)
        .map_or("Library", |shortcut| shortcut.name.as_str()),
      jellypilot_core::LoadState::Idle
      | jellypilot_core::LoadState::Loading
      | jellypilot_core::LoadState::Failed(_) => "Library",
    },
    Destination::Search(query) => query,
    Destination::Home | Destination::Detail(_) | Destination::Settings => "Library",
  };
  let heading = match &state.destination {
    Destination::Search(_) => format!("Search results for “{title}”"),
    Destination::Home
    | Destination::Library { .. }
    | Destination::Detail(_)
    | Destination::Settings => title.to_owned(),
  };
  let mut header = Column::new().spacing(TOKENS.spacing.s3).push(
    text(heading)
      .font(SPACE_GROTESK_FONT)
      .size(34)
      .color(TOKENS.colors.onSurface),
  );
  if matches!(state.destination, Destination::Library { .. }) {
    header = header.push(toolbar(state));
  }

  column![
    container(header)
      .padding([TOKENS.spacing.s5, TOKENS.spacing.s8])
      .width(Fill),
    browse_body(state),
  ]
  .height(Fill)
  .width(Fill)
  .into()
}

fn toolbar(state: &State) -> Element<'_, Message> {
  let filters = state.settings.snapshot().browse_filters();
  let sort_trigger = button(
    row![
      icon_for_variant(Icon::Sliders, IconSize::Sm, ButtonVariant::Outlined),
      text(format!("Sort: {}", sort_label(filters.sort()))),
    ]
    .spacing(TOKENS.spacing.s1_5)
    .align_y(Alignment::Center),
  )
  .padding([9, 13])
  .on_press(Message::Browse(BrowseMessage::SortMenuToggled))
  .style(|theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
  });
  let sort_menu = column![
    sort_option("Title", VideoLibrarySort::Title),
    sort_option("Recently added", VideoLibrarySort::RecentlyAdded),
    sort_option("Release date", VideoLibrarySort::ReleaseDate),
  ]
  .spacing(TOKENS.spacing.s1)
  .width(Fill);
  let sort = popover(
    sort_trigger,
    sort_menu,
    state.browse_sort_menu_open,
    PopoverOptions {
      width: Some(190.0),
      ..PopoverOptions::default()
    },
    Message::Browse(BrowseMessage::SortMenuDismissed),
  );
  let (direction_icon, direction_label) = match filters.sort_direction() {
    VideoLibrarySortDirection::Ascending => (Icon::SortAscending, "Ascending"),
    VideoLibrarySortDirection::Descending => (Icon::SortDescending, "Descending"),
  };
  let direction = button(
    row![
      icon_for_variant(direction_icon, IconSize::Sm, ButtonVariant::Outlined),
      text(direction_label),
    ]
    .spacing(TOKENS.spacing.s1_5)
    .align_y(Alignment::Center),
  )
  .padding([9, 13])
  .on_press(Message::Browse(BrowseMessage::SortDirectionToggled))
  .style(|theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
  });
  let (fav_icon, fav_label, fav_variant) = if filters.favorites_only() {
    (Icon::HeartFilled, "Favorites: On", ButtonVariant::Secondary)
  } else {
    (Icon::Heart, "Favorites: Off", ButtonVariant::Outlined)
  };
  let favorites = button(
    row![
      icon_for_variant(fav_icon, IconSize::Sm, fav_variant),
      text(fav_label),
    ]
    .spacing(TOKENS.spacing.s1_5)
    .align_y(Alignment::Center),
  )
  .padding([9, 13])
  .on_press(Message::Browse(BrowseMessage::FavoritesToggled))
  .style(move |theme, status| jellypilot_ui::theme::button_variant(theme, status, fav_variant));

  row![
    sort,
    direction,
    played_option(
      Icon::CircleDot,
      "All",
      VideoLibraryPlayedFilter::All,
      filters.played_filter(),
    ),
    played_option(
      Icon::CircleCheck,
      "Played",
      VideoLibraryPlayedFilter::Played,
      filters.played_filter(),
    ),
    played_option(
      Icon::Circle,
      "Unplayed",
      VideoLibraryPlayedFilter::Unplayed,
      filters.played_filter(),
    ),
    favorites,
  ]
  .spacing(TOKENS.spacing.s2)
  .align_y(Alignment::Center)
  .into()
}

fn sort_option(label: &'static str, sort: VideoLibrarySort) -> Element<'static, Message> {
  button(text(label).width(Fill))
    .padding([8, 10])
    .width(Fill)
    .on_press(Message::Browse(BrowseMessage::SortChanged(sort)))
    .style(|theme, status| jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Text))
    .into()
}

fn played_option(
  icon: Icon,
  label: &'static str,
  value: VideoLibraryPlayedFilter,
  selected: VideoLibraryPlayedFilter,
) -> Element<'static, Message> {
  let variant = if value == selected {
    ButtonVariant::Secondary
  } else {
    ButtonVariant::Outlined
  };
  button(
    row![icon_for_variant(icon, IconSize::Sm, variant), text(label),]
      .spacing(TOKENS.spacing.s1_5)
      .align_y(Alignment::Center),
  )
  .padding([9, 13])
  .on_press(Message::Browse(BrowseMessage::PlayedFilterChanged(value)))
  .style(move |theme, status| jellypilot_ui::theme::button_variant(theme, status, variant))
  .into()
}

fn browse_body(state: &State) -> Element<'_, Message> {
  match &state.browse_view {
    LibraryBrowseView::Inactive => empty_surface("Choose a library to browse.".to_owned()),
    LibraryBrowseView::Loading => empty_surface("Loading library…".to_owned()),
    LibraryBrowseView::Empty => match &state.destination {
      Destination::Search(query) => empty_surface(format!("No results for “{query}”.")),
      Destination::Home
      | Destination::Library { .. }
      | Destination::Detail(_)
      | Destination::Settings => empty_surface("This library has no matching items.".to_owned()),
    },
    LibraryBrowseView::Failed {
      message,
      retryable,
      retry_busy,
    } => failure_surface(message, *retryable, *retry_busy),
    LibraryBrowseView::Ready {
      visible_items,
      total_record_count,
      load_more_failure,
      retry_busy,
      ..
    } => ready_surface(
      state,
      visible_items,
      *total_record_count,
      load_more_failure.as_ref(),
      *retry_busy,
    ),
  }
}

fn ready_surface<'a>(
  state: &'a State,
  items: &'a [LibraryItemSlot],
  total_record_count: u32,
  load_more_failure: Option<&'a LibraryBrowseFailure>,
  retry_busy: bool,
) -> Element<'a, Message> {
  let available_width = (state.browse_viewport.width - PAGE_PADDING * 2.0).max(1.0);
  let metrics = ArtworkGridMetrics::for_cards(available_width, CARD_COPY_HEIGHT);
  let viewport = ArtworkGridViewport {
    offset_y: state.browse_viewport.offset_y,
    height: state.browse_viewport.height,
  };
  let mut grid = Some(artwork_grid(items, metrics, viewport, |slot| {
    browse_slot(state, slot, metrics.cell_width)
  }));
  let mut content = Column::new().width(Fill);

  for section in body_sections(
    state.browse.display_range().is_some() && total_record_count > LIBRARY_BROWSE_PAGE_SIZE,
    load_more_failure.is_some(),
  ) {
    match section {
      BodySection::Pagination => {
        if let Some(range) = state.browse.display_range() {
          let prev_enabled = state.browse.can_load_previous();
          let previous = button(
            row![
              icon_for_variant_disabled(
                Icon::ChevronLeft,
                IconSize::Sm,
                ButtonVariant::Outlined,
                !prev_enabled,
              ),
              text("Previous"),
            ]
            .spacing(TOKENS.spacing.s1_5)
            .align_y(Alignment::Center),
          )
          .padding([9, 13])
          .style(|theme, status| {
            jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
          });
          let previous = if let Some(message) = previous_action(prev_enabled) {
            previous.on_press(message)
          } else {
            previous
          };
          let next_enabled = state.browse.can_load_next();
          let next = button(
            row![
              text("Next"),
              icon_for_variant_disabled(
                Icon::ChevronRight,
                IconSize::Sm,
                ButtonVariant::Outlined,
                !next_enabled,
              ),
            ]
            .spacing(TOKENS.spacing.s1_5)
            .align_y(Alignment::Center),
          )
          .padding([9, 13])
          .style(|theme, status| {
            jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
          });
          let next = if let Some(message) = next_action(next_enabled) {
            next.on_press(message)
          } else {
            next
          };
          content = content.push(
            row![
              previous,
              next,
              text(format!(
                "Items {}–{} of {total_record_count}",
                range.start.saturating_add(1),
                range.end,
              ))
              .size(13)
              .color(TOKENS.colors.onSurfaceVariant),
            ]
            .spacing(TOKENS.spacing.s3)
            .padding([TOKENS.spacing.s3, PAGE_PADDING])
            .align_y(Alignment::Center),
          );
        }
      }
      BodySection::Grid => {
        if let Some(grid) = grid.take() {
          content = content.push(container(grid).padding([0.0, PAGE_PADDING]).width(Fill));
        }
      }
      BodySection::InlineFailure => {
        if let Some(failure) = load_more_failure {
          content = content.push(inline_failure(failure, retry_busy));
        }
      }
    }
  }

  scrollable(content)
    .id(state.browse_scroll_id.clone())
    .on_scroll(|viewport| Message::Browse(BrowseMessage::Scrolled(viewport)))
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable)
    .into()
}

/// Composition order of the ready surface's sections, factored so tests can
/// assert the rendered order the user sees (pagination must stay above the grid).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodySection {
  Pagination,
  Grid,
  InlineFailure,
}

fn body_sections(has_display_range: bool, has_failure: bool) -> Vec<BodySection> {
  let mut sections = Vec::with_capacity(3);
  if has_display_range {
    sections.push(BodySection::Pagination);
  }
  sections.push(BodySection::Grid);
  if has_failure {
    sections.push(BodySection::InlineFailure);
  }
  sections
}

fn previous_action(enabled: bool) -> Option<Message> {
  enabled.then_some(Message::Browse(BrowseMessage::LoadPrevious))
}

fn next_action(enabled: bool) -> Option<Message> {
  enabled.then_some(Message::Browse(BrowseMessage::LoadNext))
}

fn browse_slot<'a>(
  state: &'a State,
  slot: &'a LibraryItemSlot,
  cell_width: f32,
) -> Element<'a, Message> {
  let Some(item) = &slot.item else {
    return space::vertical().width(Fill).height(Fill).into();
  };
  video_card(state, item, cell_width)
}

const POSTER_FRAME_WIDTH: f32 = 160.0;
const POSTER_FRAME_HEIGHT: f32 = 240.0;

fn video_card<'a>(
  state: &'a State,
  item: &'a VideoLibraryItem,
  cell_width: f32,
) -> Element<'a, Message> {
  let artwork_height = cell_width * POSTER_FRAME_HEIGHT / POSTER_FRAME_WIDTH;
  let artwork = artwork(
    state,
    state.browse_artwork.get(&item.id),
    &item.name,
    artwork_height,
  );
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
  .width(Fill);

  container(
    button(column![artwork, copy].width(Fill))
      .padding(1)
      .width(Fill)
      .height(Fill)
      .on_press(Message::OpenDetail(item.clone()))
      .style(jellypilot_ui::theme::poster_card_button),
  )
  .width(Fill)
  .height(Fill)
  .clip(true)
  .into()
}

fn artwork<'a>(
  state: &'a State,
  cell: Option<&ArtworkCell>,
  name: &'a str,
  height: f32,
) -> Element<'a, Message> {
  if let Some(cell) = cell {
    if cell.state == ArtworkCellState::Ready {
      if let Some(handle) = state.artwork_handles.get(cell.slot, &cell.image_id) {
        return rounded_image(handle.clone(), full_radius(TOKENS.radii.x2l))
          .content_fit(ContentFit::Cover)
          .width(Fill)
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
      icon_with_color(Icon::Movie, IconSize::Custom(36.0), placeholder_color),
      text(initial)
        .font(SPACE_GROTESK_FONT)
        .size(24)
        .color(placeholder_color),
    ]
    .spacing(TOKENS.spacing.s1)
    .align_x(Alignment::Center),
  )
  .width(Fill)
  .height(height)
  .center_x(Fill)
  .center_y(Fill)
  .style(|_theme| container::Style {
    background: Some(iced::Background::Color(
      TOKENS.colors.surfaceContainerLowest,
    )),
    border: iced::Border {
      radius: full_radius(TOKENS.radii.x2l),
      width: 0.0,
      color: iced::Color::TRANSPARENT,
    },
    ..container::Style::default()
  })
  .into()
}
fn failure_surface(message: &str, retryable: bool, retry_busy: bool) -> Element<'_, Message> {
  let retry_enabled = retryable && !retry_busy;
  let retry = button(
    row![
      icon_for_variant_disabled(
        Icon::Refresh,
        IconSize::Sm,
        ButtonVariant::Primary,
        !retry_enabled,
      ),
      text(if retry_busy { "Retrying…" } else { "Retry" }),
    ]
    .spacing(TOKENS.spacing.s1_5)
    .align_y(Alignment::Center),
  )
  .padding([9, 14])
  .style(|theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Primary)
  });
  let retry = if let Some(message) = retry_action(retryable, retry_busy) {
    retry.on_press(message)
  } else {
    retry
  };
  container(
    column![
      text("Could not load this library")
        .font(SPACE_GROTESK_FONT)
        .size(24)
        .color(TOKENS.colors.onSurface),
      text(message).size(14).color(TOKENS.colors.error),
      retry,
    ]
    .spacing(TOKENS.spacing.s3),
  )
  .padding(PAGE_PADDING)
  .width(Fill)
  .height(Fill)
  .into()
}

fn inline_failure(failure: &LibraryBrowseFailure, retry_busy: bool) -> Element<'_, Message> {
  let retry_enabled = failure.retryable && !retry_busy;
  let retry = button(
    row![
      icon_for_variant_disabled(
        Icon::Refresh,
        IconSize::Xs,
        ButtonVariant::Outlined,
        !retry_enabled,
      ),
      text(if retry_busy { "Retrying…" } else { "Retry" }),
    ]
    .spacing(TOKENS.spacing.s1)
    .align_y(Alignment::Center),
  )
  .padding([8, 12])
  .style(|theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
  });
  let retry = if let Some(message) = retry_action(failure.retryable, retry_busy) {
    retry.on_press(message)
  } else {
    retry
  };
  row![
    text(&failure.message).size(13).color(TOKENS.colors.error),
    retry,
  ]
  .spacing(TOKENS.spacing.s3)
  .padding([TOKENS.spacing.s3, PAGE_PADDING])
  .align_y(Alignment::Center)
  .into()
}

fn retry_action(retryable: bool, retry_busy: bool) -> Option<Message> {
  (retryable && !retry_busy).then_some(Message::Browse(BrowseMessage::Retry))
}

fn empty_surface(message: String) -> Element<'static, Message> {
  container(text(message).size(16).color(TOKENS.colors.onSurfaceVariant))
    .padding(PAGE_PADDING)
    .width(Fill)
    .height(Fill)
    .into()
}

const fn sort_label(sort: VideoLibrarySort) -> &'static str {
  match sort {
    VideoLibrarySort::Title => "Title",
    VideoLibrarySort::RecentlyAdded => "Recently added",
    VideoLibrarySort::ReleaseDate => "Release date",
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn pagination_is_rendered_before_the_grid() {
    let sections = body_sections(true, true);
    let navigation = sections
      .iter()
      .position(|section| *section == BodySection::Pagination);
    let grid = sections
      .iter()
      .position(|section| *section == BodySection::Grid);
    assert!(
      navigation.is_some_and(|nav| grid.is_some_and(|grid| nav < grid)),
      "pagination must precede the grid: {sections:?}"
    );
    assert_eq!(body_sections(false, false), vec![BodySection::Grid]);
  }

  #[test]
  fn single_page_results_render_no_pagination_chrome() {
    // Callers pass `has_display_range = display_range.is_some() && total >
    // PAGE_SIZE`, so a one-window library must not render the section.
    assert_eq!(
      body_sections(false, true),
      vec![BodySection::Grid, BodySection::InlineFailure]
    );
  }

  #[test]
  fn previous_action_is_available_for_a_later_virtual_window() {
    assert!(matches!(
      previous_action(true),
      Some(Message::Browse(BrowseMessage::LoadPrevious))
    ));
    assert!(previous_action(false).is_none());
  }

  #[test]
  fn next_action_is_available_when_not_on_last_window() {
    assert!(matches!(
      next_action(true),
      Some(Message::Browse(BrowseMessage::LoadNext))
    ));
    assert!(next_action(false).is_none());
  }

  #[test]
  fn non_retryable_incremental_failure_has_no_retry_action() {
    assert!(retry_action(false, false).is_none());
  }
}
