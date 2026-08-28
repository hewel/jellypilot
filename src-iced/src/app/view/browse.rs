use iced::widget::{button, column, container, image, row, scrollable, space, text, Column};
use iced::{Alignment, ContentFit, Element, Fill};
use jellypilot_core::browse_model::{LibraryBrowseView, LibraryItemSlot};
use jellypilot_core::cards::{card_frame_size, item_caption};
use jellypilot_core::{LibraryBrowseFailure, LibraryBrowseMode};
use jellypilot_media_server::{
  VideoLibraryItem, VideoLibraryPlayedFilter, VideoLibrarySort, VideoLibrarySortDirection,
};
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::overlay::{popover, PopoverOptions};
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};
use jellypilot_ui::widgets::artwork_grid::{artwork_grid, ArtworkGridMetrics, ArtworkGridViewport};

use crate::app::message::{BrowseMessage, Message};
use crate::app::state::{ArtworkCell, ArtworkCellState, Destination, State};

const PAGE_PADDING: f32 = 32.0;
const CARD_COPY_HEIGHT: f32 = 48.0;

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
    Destination::Home => "Library",
  };
  let heading = match &state.destination {
    Destination::Search(_) => format!("Search results for “{title}”"),
    Destination::Home | Destination::Library { .. } => title.to_owned(),
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
  let sort_trigger = button(text(format!("Sort: {}", sort_label(filters.sort()))))
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
  let direction = button(text(match filters.sort_direction() {
    VideoLibrarySortDirection::Ascending => "Ascending ↑",
    VideoLibrarySortDirection::Descending => "Descending ↓",
  }))
  .padding([9, 13])
  .on_press(Message::Browse(BrowseMessage::SortDirectionToggled))
  .style(|theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
  });
  let favorites = button(text(if filters.favorites_only() {
    "Favorites: On"
  } else {
    "Favorites: Off"
  }))
  .padding([9, 13])
  .on_press(Message::Browse(BrowseMessage::FavoritesToggled))
  .style(move |theme, status| {
    jellypilot_ui::theme::button_variant(
      theme,
      status,
      if filters.favorites_only() {
        ButtonVariant::Secondary
      } else {
        ButtonVariant::Outlined
      },
    )
  });

  row![
    sort,
    direction,
    played_option(
      "All",
      VideoLibraryPlayedFilter::All,
      filters.played_filter()
    ),
    played_option(
      "Played",
      VideoLibraryPlayedFilter::Played,
      filters.played_filter(),
    ),
    played_option(
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
  label: &'static str,
  value: VideoLibraryPlayedFilter,
  selected: VideoLibraryPlayedFilter,
) -> Element<'static, Message> {
  button(text(label))
    .padding([9, 13])
    .on_press(Message::Browse(BrowseMessage::PlayedFilterChanged(value)))
    .style(move |theme, status| {
      jellypilot_ui::theme::button_variant(
        theme,
        status,
        if value == selected {
          ButtonVariant::Secondary
        } else {
          ButtonVariant::Outlined
        },
      )
    })
    .into()
}

fn browse_body(state: &State) -> Element<'_, Message> {
  match &state.browse_view {
    LibraryBrowseView::Inactive => empty_surface("Choose a library to browse.".to_owned()),
    LibraryBrowseView::Loading => empty_surface("Loading library…".to_owned()),
    LibraryBrowseView::Empty => match &state.destination {
      Destination::Search(query) => empty_surface(format!("No results for “{query}”.")),
      Destination::Home | Destination::Library { .. } => {
        empty_surface("This library has no matching items.".to_owned())
      }
    },
    LibraryBrowseView::Failed {
      message,
      retryable,
      retry_busy,
    } => failure_surface(message, *retryable, *retry_busy),
    LibraryBrowseView::Ready {
      visible_items,
      mode,
      total_record_count,
      is_fetching_more,
      can_load_next: _,
      load_more_failure,
      retry_busy,
    } => ready_surface(
      state,
      visible_items,
      *mode,
      *total_record_count,
      *is_fetching_more,
      load_more_failure.as_ref(),
      *retry_busy,
    ),
  }
}

fn ready_surface<'a>(
  state: &'a State,
  items: &'a [LibraryItemSlot],
  mode: LibraryBrowseMode,
  total_record_count: u32,
  is_fetching_more: bool,
  load_more_failure: Option<&'a LibraryBrowseFailure>,
  retry_busy: bool,
) -> Element<'a, Message> {
  let available_width = (state.browse_viewport.width - PAGE_PADDING * 2.0).max(1.0);
  let metrics = ArtworkGridMetrics::for_width(available_width);
  let viewport = ArtworkGridViewport {
    offset_y: state.browse_viewport.offset_y,
    height: state.browse_viewport.height,
  };
  let mut grid = Some(artwork_grid(items, available_width, viewport, |slot| {
    browse_slot(state, slot, metrics.cell_width, metrics.cell_height)
  }));
  let mut content = Column::new().width(Fill);

  for section in body_sections(
    mode,
    state.browse.display_range().is_some(),
    is_fetching_more,
    load_more_failure.is_some(),
  ) {
    match section {
      BodySection::VirtualNavigation => {
        if let Some(range) = state.browse.display_range() {
          let previous = button(text("Previous page"))
            .padding([9, 13])
            .style(|theme, status| {
              jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
            });
          let previous = if let Some(message) = previous_action(state.browse.can_load_previous()) {
            previous.on_press(message)
          } else {
            previous
          };
          content = content.push(
            row![
              previous,
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
      BodySection::LoadingMore => {
        content = content.push(status_line("Loading more…"));
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
/// assert the rendered order the user sees (virtual navigation must stay
/// above the grid or auto tail-advance makes it unreachable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodySection {
  VirtualNavigation,
  Grid,
  LoadingMore,
  InlineFailure,
}

fn body_sections(
  mode: LibraryBrowseMode,
  has_display_range: bool,
  is_fetching_more: bool,
  has_failure: bool,
) -> Vec<BodySection> {
  let mut sections = Vec::with_capacity(4);
  if matches!(mode, LibraryBrowseMode::Virtual) && has_display_range {
    sections.push(BodySection::VirtualNavigation);
  }
  sections.push(BodySection::Grid);
  if is_fetching_more {
    sections.push(BodySection::LoadingMore);
  }
  if has_failure {
    sections.push(BodySection::InlineFailure);
  }
  sections
}

fn previous_action(enabled: bool) -> Option<Message> {
  enabled.then_some(Message::Browse(BrowseMessage::LoadPrevious))
}

fn browse_slot<'a>(
  state: &'a State,
  slot: &'a LibraryItemSlot,
  cell_width: f32,
  cell_height: f32,
) -> Element<'a, Message> {
  let Some(item) = &slot.item else {
    return container(space::vertical())
      .width(Fill)
      .height(Fill)
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Filled))
      .into();
  };
  video_card(state, item, cell_width, cell_height)
}

fn video_card<'a>(
  state: &'a State,
  item: &'a VideoLibraryItem,
  cell_width: f32,
  cell_height: f32,
) -> Element<'a, Message> {
  let (frame_width, frame_height) = card_frame_size(item);
  let artwork_height = (cell_width * frame_height as f32 / frame_width as f32)
    .min((cell_height - CARD_COPY_HEIGHT).max(1.0));
  let artwork = artwork(
    state,
    state.browse_artwork.get(&item.id),
    &item.name,
    artwork_height,
  );
  container(
    column![
      artwork,
      text(&item.name).size(14).color(TOKENS.colors.onSurface),
      text(item_caption(item))
        .size(12)
        .color(TOKENS.colors.onSurfaceVariant),
    ]
    .spacing(TOKENS.spacing.s1)
    .width(Fill),
  )
  .width(Fill)
  .height(Fill)
  .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Filled))
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
        return container(
          image(handle.clone())
            .content_fit(ContentFit::Cover)
            .width(Fill)
            .height(Fill),
        )
        .width(Fill)
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
      .size(38)
      .color(if failed {
        TOKENS.colors.warning
      } else {
        TOKENS.colors.onSurfaceVariant
      }),
  )
  .width(Fill)
  .height(height)
  .center_x(Fill)
  .center_y(Fill)
  .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Elevated))
  .into()
}

fn failure_surface(message: &str, retryable: bool, retry_busy: bool) -> Element<'_, Message> {
  let retry = button(text(if retry_busy { "Retrying…" } else { "Retry" }))
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
  let retry = button(text(if retry_busy { "Retrying…" } else { "Retry" }))
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

fn status_line(label: &str) -> Element<'_, Message> {
  container(text(label).size(13).color(TOKENS.colors.onSurfaceVariant))
    .padding([TOKENS.spacing.s3, PAGE_PADDING])
    .width(Fill)
    .into()
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
  fn virtual_navigation_is_rendered_before_the_grid() {
    let sections = body_sections(LibraryBrowseMode::Virtual, true, true, true);
    let navigation = sections
      .iter()
      .position(|section| *section == BodySection::VirtualNavigation);
    let grid = sections
      .iter()
      .position(|section| *section == BodySection::Grid);
    assert!(
      navigation.is_some_and(|nav| grid.is_some_and(|grid| nav < grid)),
      "virtual navigation must precede the grid: {sections:?}"
    );
    assert_eq!(
      body_sections(LibraryBrowseMode::Normal, true, true, true),
      vec![
        BodySection::Grid,
        BodySection::LoadingMore,
        BodySection::InlineFailure
      ]
    );
    assert_eq!(
      body_sections(LibraryBrowseMode::Virtual, false, false, false),
      vec![BodySection::Grid]
    );
  }

  #[test]
  fn previous_action_is_available_for_a_later_virtual_window() {
    assert!(matches!(
      previous_action(true),
      Some(Message::Browse(BrowseMessage::LoadPrevious))
    ));
  }

  #[test]
  fn non_retryable_incremental_failure_has_no_retry_action() {
    assert!(retry_action(false, false).is_none());
  }
}
