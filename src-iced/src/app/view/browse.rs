use crate::app::message::{BrowseMessage, Message};
use crate::app::state::{ArtworkCell, ArtworkCellState, Destination, State};
use iced::widget::{button, column, container, row, scrollable, stack, text, Column, Row};
use iced::{Alignment, Color, ContentFit, Element, Fill};
use jellypilot_core::browse_model::{LibraryBrowseView, LibraryItemSlot};
use jellypilot_core::cards::item_caption;
use jellypilot_core::{LibraryBrowseFailure, LIBRARY_BROWSE_PAGE_SIZE};
use jellypilot_media_server::{
  VideoLibraryItem, VideoLibraryPlayedFilter, VideoLibrarySort, VideoLibrarySortDirection,
};
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::layout::SizeClass;
use jellypilot_ui::overlay::{popover, PopoverOptions};
use jellypilot_ui::tokens::{ThemePalette, TOKENS};
use jellypilot_ui::variants::ButtonVariant;
use jellypilot_ui::widgets::artwork_grid::{artwork_grid, ArtworkGridMetrics, ArtworkGridViewport};
use jellypilot_ui::widgets::control_button::control_button;
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::skeleton::{skeleton_block, skeleton_panel};
use jellypilot_ui::{full_radius, poster_card, rounded_image};

pub(crate) const PAGE_PADDING: f32 = 32.0;

/// Horizontal page padding for browse screens, tier-dependent:
/// [`SizeClass::Compact`] uses tighter spacing ([`TOKENS.spacing.s4`] = 16.0),
/// while [`SizeClass::Standard`] and [`SizeClass::Wide`] use [`PAGE_PADDING`] (32.0).
pub(crate) fn page_padding(class: SizeClass) -> f32 {
  match class {
    SizeClass::Compact => TOKENS.spacing.s4,
    SizeClass::Standard | SizeClass::Wide => PAGE_PADDING,
  }
}
/// Grid width derived from the tracked window size: window minus the tier-dependent sidebar, the shell
/// hairline, and the tier-dependent page padding.
///
/// The scrollable's `on_scroll` viewport is NOT used for width: iced only
/// publishes it when the content overflows the viewport, so a maximized
/// window whose grid fits vertically would keep reporting a stale width.
/// `state.shell.window_size` follows every resize event and never goes stale.
pub(crate) fn grid_available_width(window_width: f32, class: SizeClass) -> f32 {
  (window_width
    - super::shell::sidebar_width(class)
    - super::shell::HAIRLINE_WIDTH
    - page_padding(class) * 2.0)
    .max(1.0)
}

pub(crate) const CARD_COPY_HEIGHT: f32 = 46.0;
pub fn view(state: &State) -> Element<'_, Message> {
  let class = SizeClass::from_width(state.shell.window_size.width);
  let title = match &state.shell.destination {
    Destination::Library { library_id, .. } => match &state
      .full
      .as_ref()
      .expect("FullUi required")
      .home
      .data
      .shortcuts
    {
      jellypilot_core::LoadState::Ready(shortcuts) => shortcuts
        .iter()
        .find(|shortcut| shortcut.id == *library_id)
        .map_or("Library", |shortcut| shortcut.name.as_str()),
      jellypilot_core::LoadState::Idle
      | jellypilot_core::LoadState::Loading
      | jellypilot_core::LoadState::Failed(_) => "Library",
    },
    Destination::Search(query) => query,
    Destination::Home
    | Destination::PersonalLists(_)
    | Destination::Detail(_)
    | Destination::NowPlaying => "Library",
  };
  let heading = match &state.shell.destination {
    Destination::Search(_) => format!("Search results for “{title}”"),
    Destination::Home
    | Destination::PersonalLists(_)
    | Destination::Library { .. }
    | Destination::Detail(_)
    | Destination::NowPlaying => title.to_owned(),
  };
  let mut header = Column::new().spacing(TOKENS.spacing.s3).push(
    text(heading)
      .font(SPACE_GROTESK_FONT)
      .size(34)
      .color(state.palette().text.heading),
  );
  if matches!(state.shell.destination, Destination::Library { .. }) {
    header = header.push(toolbar(state));
  }

  column![
    container(header)
      .padding([TOKENS.spacing.s5, TOKENS.spacing.s8])
      .width(Fill),
    browse_body(state, class),
  ]
  .height(Fill)
  .width(Fill)
  .into()
}

fn toolbar(state: &State) -> Element<'_, Message> {
  let filters = state.kernel.settings.snapshot().browse_filters();
  let sort_trigger = control_button(
    Some(Icon::Sliders),
    Some(format!("Sort: {}", sort_label(filters.sort()))),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 12])
  .on_press(Message::Browse(BrowseMessage::SortMenuToggled));
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
    state
      .full
      .as_ref()
      .expect("FullUi required")
      .browse
      .sort_menu_open,
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
  let direction = control_button(
    Some(direction_icon),
    Some(direction_label.to_owned()),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 12])
  .on_press(Message::Browse(BrowseMessage::SortDirectionToggled));
  // Favorited state keeps the heart in the fixed rose `favorite` accent
  // across hover; the off state is an ordinary Tonal `control_button`.
  let favorites: Element<'_, Message> = if filters.favorites_only() {
    button(
      row![
        icon_with_color(
          Icon::HeartFilled,
          IconSize::Sm,
          state.palette().colors.favorite
        ),
        text("Favorites: On"),
      ]
      .spacing(TOKENS.spacing.s1_5)
      .align_y(Alignment::Center),
    )
    .padding([6, 12])
    .on_press(Message::Browse(BrowseMessage::FavoritesToggled))
    .style(|theme, status| {
      jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::TonalActive)
    })
    .into()
  } else {
    control_button(
      Some(Icon::Heart),
      Some("Favorites: Off".to_owned()),
      ButtonVariant::Tonal,
    )
    .icon_size(IconSize::Sm)
    .spacing(TOKENS.spacing.s1_5)
    .padding([6, 12])
    .on_press(Message::Browse(BrowseMessage::FavoritesToggled))
    .into()
  };

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
  control_button(None, Some(label.to_owned()), ButtonVariant::Text)
    .padding([6, 10])
    .width(Fill)
    .label_fill(true)
    .on_press(Message::Browse(BrowseMessage::SortChanged(sort)))
    .into()
}

fn played_option(
  icon: Icon,
  label: &'static str,
  value: VideoLibraryPlayedFilter,
  selected: VideoLibraryPlayedFilter,
) -> Element<'static, Message> {
  let variant = if value == selected {
    ButtonVariant::TonalActive
  } else {
    ButtonVariant::Tonal
  };
  control_button(Some(icon), Some(label.to_owned()), variant)
    .icon_size(IconSize::Sm)
    .spacing(TOKENS.spacing.s1_5)
    .padding([6, 12])
    .on_press(Message::Browse(BrowseMessage::PlayedFilterChanged(value)))
    .into()
}

fn browse_body<'a>(state: &'a State, class: SizeClass) -> Element<'a, Message> {
  let padding = page_padding(class);
  match &state.full.as_ref().expect("FullUi required").browse.view {
    LibraryBrowseView::Inactive => empty_surface(
      state.palette(),
      "Choose a library to browse.".to_owned(),
      padding,
    ),
    LibraryBrowseView::Loading => browse_loading_skeleton(state, class),
    LibraryBrowseView::Empty => match &state.shell.destination {
      Destination::Search(query) => empty_surface(
        state.palette(),
        format!("No results for “{query}”."),
        padding,
      ),
      Destination::Home
      | Destination::PersonalLists(_)
      | Destination::Library { .. }
      | Destination::Detail(_)
      | Destination::NowPlaying => empty_surface(
        state.palette(),
        "This library has no matching items.".to_owned(),
        padding,
      ),
    },
    LibraryBrowseView::Failed {
      message,
      retryable,
      retry_busy,
    } => failure_surface(state.palette(), message, *retryable, *retry_busy, padding),
    LibraryBrowseView::Ready {
      visible_items,
      visible_start,
      total_record_count,
      load_more_failure,
      retry_busy,
      ..
    } => ready_surface(
      state,
      visible_items,
      *visible_start,
      *total_record_count,
      load_more_failure.as_ref(),
      *retry_busy,
      class,
    ),
  }
}

fn ready_surface<'a>(
  state: &'a State,
  items: &'a [LibraryItemSlot],
  visible_start: u32,
  total_record_count: u32,
  load_more_failure: Option<&'a LibraryBrowseFailure>,
  retry_busy: bool,
  class: SizeClass,
) -> Element<'a, Message> {
  let skeleton_phase = state.shell.skeleton_phase;
  let reduced_motion = state.kernel.settings.snapshot().reduced_motion();
  let padding = page_padding(class);
  let available_width = grid_available_width(state.shell.window_size.width, class);
  let metrics = ArtworkGridMetrics::for_cards(available_width, CARD_COPY_HEIGHT);
  // The count row above the grid is short enough that the grid's overscan
  // absorbs it, so no scroll margin is subtracted here.
  let viewport = ArtworkGridViewport::from_scroll_geometry(
    state
      .full
      .as_ref()
      .expect("FullUi required")
      .browse
      .viewport
      .offset_y,
    state
      .full
      .as_ref()
      .expect("FullUi required")
      .browse
      .viewport
      .height,
    0.0,
  );
  let grid = artwork_grid(
    total_record_count as usize,
    metrics,
    viewport,
    |index| match item_at(visible_start, items, index) {
      Some(item) => video_card(
        state,
        item,
        metrics.cell_width,
        skeleton_phase,
        reduced_motion,
      ),
      None => skeleton_cell(metrics.cell_width, skeleton_phase, reduced_motion),
    },
  );
  let content = Column::new()
    .width(Fill)
    .push(
      row![text(format!("{total_record_count} items"))
        .size(13)
        .color(state.palette().text.metadata),]
      .padding([TOKENS.spacing.s3, padding])
      .align_y(Alignment::Center),
    )
    .push(container(grid).padding([0.0, padding]).width(Fill));

  let body = scrollable(content)
    .id(
      state
        .full
        .as_ref()
        .expect("FullUi required")
        .browse
        .scroll_id
        .clone(),
    )
    .on_scroll(|viewport| Message::Browse(BrowseMessage::Scrolled(viewport)))
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable);

  // The failure banner stacks above the scrollable (the shell-toast pattern)
  // so it stays pinned to the viewport's bottom edge at any scroll position.
  let mut surface = stack![body].width(Fill).height(Fill);
  if let Some(banner) = failure_overlay(state.palette(), load_more_failure, retry_busy) {
    surface = surface.push(banner);
  }
  surface.into()
}

/// Builds the viewport-pinned failure banner when the ready surface carries
/// an incremental load-more failure, or `None` when the tail loaded cleanly.
fn failure_overlay<'a>(
  palette: &'static ThemePalette,
  load_more_failure: Option<&'a LibraryBrowseFailure>,
  retry_busy: bool,
) -> Option<Element<'a, Message>> {
  load_more_failure.map(|failure| failure_banner(palette, failure, retry_busy))
}

/// Maps a global item index into the sparse window of slots that starts at
/// `visible_start`. Indexes before the window, beyond it, or landing on an
/// unloaded slot yield `None` and render as skeleton cells.
fn item_at(
  visible_start: u32,
  slots: &[LibraryItemSlot],
  index: usize,
) -> Option<&VideoLibraryItem> {
  let slot_index = index.checked_sub(visible_start as usize)?;
  slots.get(slot_index)?.item.as_ref()
}

fn browse_loading_skeleton<'a>(state: &'a State, class: SizeClass) -> Element<'a, Message> {
  let skeleton_phase = state.shell.skeleton_phase;
  let reduced_motion = state.kernel.settings.snapshot().reduced_motion();
  let padding = page_padding(class);
  let metrics = skeleton_grid_metrics(state.shell.window_size.width, class);
  let grid = browse_skeleton_grid(metrics, skeleton_phase, reduced_motion);
  let content = Column::new()
    .width(Fill)
    .push(container(grid).padding([0.0, padding]).width(Fill));

  scrollable(content)
    .id(
      state
        .full
        .as_ref()
        .expect("FullUi required")
        .browse
        .scroll_id
        .clone(),
    )
    .on_scroll(|viewport| Message::Browse(BrowseMessage::Scrolled(viewport)))
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable)
    .into()
}

pub(crate) fn skeleton_grid_metrics(window_width: f32, class: SizeClass) -> ArtworkGridMetrics {
  let available_width = grid_available_width(window_width, class);
  ArtworkGridMetrics::for_cards(available_width, CARD_COPY_HEIGHT)
}

fn browse_skeleton_grid<'a>(
  metrics: ArtworkGridMetrics,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let total_cells = LIBRARY_BROWSE_PAGE_SIZE as usize;
  let row_count = total_cells.div_ceil(metrics.columns);
  let mut grid = Column::new().spacing(TOKENS.spacing.s4).width(Fill);

  for row_index in 0..row_count {
    let start = row_index * metrics.columns;
    let end = (start + metrics.columns).min(total_cells);
    let mut row = Row::new().spacing(TOKENS.spacing.s4);
    for _ in start..end {
      row = row.push(
        container(skeleton_cell(
          metrics.cell_width,
          skeleton_phase,
          reduced_motion,
        ))
        .width(metrics.cell_width)
        .height(metrics.cell_height),
      );
    }
    grid = grid.push(row);
  }

  grid.into()
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SkeletonCellDimensions {
  pub cell_width: f32,
  pub cell_height: f32,
  pub artwork_height: f32,
}

#[cfg(test)]
pub(crate) fn skeleton_cell_dimensions(metrics: ArtworkGridMetrics) -> SkeletonCellDimensions {
  SkeletonCellDimensions {
    cell_width: metrics.cell_width,
    cell_height: metrics.cell_height,
    artwork_height: card_artwork_height(metrics.cell_width),
  }
}

pub(crate) fn card_artwork_height(cell_width: f32) -> f32 {
  cell_width * POSTER_FRAME_HEIGHT / POSTER_FRAME_WIDTH
}

fn skeleton_cell<'a>(
  cell_width: f32,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let artwork_height = card_artwork_height(cell_width);
  let poster = skeleton_block(cell_width, artwork_height, skeleton_phase, reduced_motion);
  let copy = column![
    skeleton_block(cell_width, 18.0, skeleton_phase, reduced_motion),
    skeleton_block(cell_width * 0.6, 14.0, skeleton_phase, reduced_motion),
  ]
  .spacing(TOKENS.spacing.s1)
  .padding(iced::Padding {
    top: TOKENS.spacing.s2,
    right: 0.0,
    bottom: 0.0,
    left: 0.0,
  })
  .width(Fill);

  column![poster, copy].width(Fill).into()
}

const POSTER_FRAME_WIDTH: f32 = 160.0;
const POSTER_FRAME_HEIGHT: f32 = 240.0;

fn video_card<'a>(
  state: &'a State,
  item: &'a VideoLibraryItem,
  cell_width: f32,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let palette = state.palette();
  let artwork_height = card_artwork_height(cell_width);
  let artwork = artwork(
    state,
    state
      .full
      .as_ref()
      .expect("FullUi required")
      .browse
      .artwork
      .get(&item.id),
    &item.name,
    artwork_height,
    skeleton_phase,
    reduced_motion,
  );
  let copy = column![
    ellipsis_text(&item.name)
      .size(14)
      .color(palette.text.heading),
    ellipsis_text(item_caption(item))
      .size(12)
      .color(palette.text.metadata),
  ]
  .spacing(TOKENS.spacing.s1)
  .padding(iced::Padding {
    top: TOKENS.spacing.s2,
    right: 0.0,
    bottom: 0.0,
    left: 0.0,
  })
  .width(Fill);

  poster_card(artwork, copy)
    .width(Fill)
    .on_press(Message::OpenDetail(item.clone()))
    .into()
}

fn artwork<'a>(
  state: &'a State,
  cell: Option<&ArtworkCell>,
  name: &'a str,
  height: f32,
  phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let palette = state.palette();
  if let Some(cell) = cell {
    if cell.state == ArtworkCellState::Ready {
      if let Some(handle) = state.kernel.artwork_handles.get(cell.slot, &cell.image_id) {
        return rounded_image(handle.clone(), full_radius(TOKENS.radii.lg))
          .content_fit(ContentFit::Cover)
          .width(Fill)
          .height(height)
          .into();
      }
    }
  }

  let placeholder_color = match cell.map(|cell| cell.state) {
    // No planned load (the server carries no image for this slot): settle on
    // a neutral placeholder instead of shimmering forever.
    None => Some(palette.text.metadata),
    Some(ArtworkCellState::Failed) => Some(palette.colors.warning),
    _ => None,
  };
  if let Some(placeholder_color) = placeholder_color {
    let initial = name
      .trim()
      .chars()
      .next()
      .map(|character| character.to_uppercase().collect::<String>())
      .unwrap_or_else(|| "•".to_owned());
    return container(
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
        palette.colors.surfaceContainerLowest,
      )),
      border: iced::Border {
        radius: full_radius(TOKENS.radii.lg),
        width: 0.0,
        color: iced::Color::TRANSPARENT,
      },
      ..container::Style::default()
    })
    .into();
  }

  skeleton_panel(
    Fill,
    height,
    palette.colors.surfaceContainerLowest,
    full_radius(TOKENS.radii.lg),
    phase,
    reduced_motion,
  )
  .into()
}
fn failure_surface<'a>(
  palette: &'static ThemePalette,
  message: &'a str,
  retryable: bool,
  retry_busy: bool,
  padding: f32,
) -> Element<'a, Message> {
  let retry = control_button(
    Some(Icon::Refresh),
    Some(if retry_busy {
      "Retrying…".to_owned()
    } else {
      "Retry".to_owned()
    }),
    ButtonVariant::Primary,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 12])
  .on_press_maybe(retry_action(retryable, retry_busy));
  container(
    column![
      text("Could not load this library")
        .font(SPACE_GROTESK_FONT)
        .size(24)
        .color(palette.text.heading),
      text(message).size(14).color(palette.colors.error),
      retry,
    ]
    .spacing(TOKENS.spacing.s3),
  )
  .padding(padding)
  .width(Fill)
  .height(Fill)
  .into()
}

/// Incremental load-more failure pinned to the bottom edge of the grid
/// viewport. The outer fill container anchors the banner bottom-center with
/// an `s4` offset; the banner itself is a flat, opaque error-container fill
/// with no border or shadow.
fn failure_banner<'a>(
  palette: &'static ThemePalette,
  failure: &'a LibraryBrowseFailure,
  retry_busy: bool,
) -> Element<'a, Message> {
  let colors = palette.colors;
  let retry = control_button(
    Some(Icon::Refresh),
    Some(if retry_busy {
      "Retrying…".to_owned()
    } else {
      "Retry".to_owned()
    }),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Xs)
  .spacing(TOKENS.spacing.s1)
  .padding([6, 10])
  .on_press_maybe(retry_action(failure.retryable, retry_busy));

  let banner = container(
    row![text(&failure.message).size(13), retry,]
      .spacing(TOKENS.spacing.s3)
      .align_y(Alignment::Center),
  )
  .padding(TOKENS.spacing.s4)
  .style(move |_theme| container::Style {
    background: Some(iced::Background::Color(colors.errorContainer)),
    text_color: Some(colors.onErrorContainer),
    border: iced::Border {
      color: Color::TRANSPARENT,
      width: 0.0,
      radius: TOKENS.radii.md.into(),
    },
    ..container::Style::default()
  });

  container(banner)
    .width(Fill)
    .height(Fill)
    .padding(iced::Padding {
      top: 0.0,
      right: TOKENS.spacing.s4,
      bottom: TOKENS.spacing.s4,
      left: TOKENS.spacing.s4,
    })
    .align_x(Alignment::Center)
    .align_y(Alignment::End)
    .into()
}

fn retry_action(retryable: bool, retry_busy: bool) -> Option<Message> {
  (retryable && !retry_busy).then_some(Message::Browse(BrowseMessage::Retry))
}

fn empty_surface(
  palette: &ThemePalette,
  message: String,
  padding: f32,
) -> Element<'static, Message> {
  container(text(message).size(16).color(palette.text.metadata))
    .padding(padding)
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
  fn failure_banner_overlays_only_when_a_load_more_failure_is_present() {
    let failure = LibraryBrowseFailure {
      message: "Could not load more items.".to_owned(),
      retryable: true,
    };
    assert!(
      failure_overlay(&jellypilot_ui::tokens::DARK_PALETTE, Some(&failure), false).is_some(),
      "a ready surface with a load-more failure must pin the banner overlay"
    );
    assert!(
      failure_overlay(&jellypilot_ui::tokens::DARK_PALETTE, Some(&failure), true).is_some(),
      "the banner stays pinned while a retry is in flight"
    );
    assert!(
      failure_overlay(&jellypilot_ui::tokens::DARK_PALETTE, None, false).is_none(),
      "a clean tail renders no overlay"
    );
  }

  #[test]
  fn item_at_maps_global_indexes_into_the_sparse_window() {
    let slots = vec![
      LibraryItemSlot {
        item: Some(video_item("item-1")),
      },
      LibraryItemSlot { item: None },
    ];

    assert!(item_at(24, &slots, 23).is_none(), "before the window");
    assert_eq!(
      item_at(24, &slots, 24).map(|item| item.id.as_str()),
      Some("item-1"),
      "in-range hit"
    );
    assert!(item_at(24, &slots, 25).is_none(), "unloaded slot");
    assert!(item_at(24, &slots, 26).is_none(), "beyond the window");
    assert!(item_at(0, &[], 0).is_none(), "empty window");
  }

  #[test]
  fn non_retryable_incremental_failure_has_no_retry_action() {
    assert!(retry_action(false, false).is_none());
  }

  #[test]
  fn browse_card_metrics_match_exact_poster_and_copy_height() {
    let metrics = ArtworkGridMetrics::for_cards(640.0, CARD_COPY_HEIGHT);
    assert_eq!(CARD_COPY_HEIGHT, 46.0);
    assert_eq!(
      metrics.cell_height,
      metrics.cell_width * 1.5 + CARD_COPY_HEIGHT
    );
  }

  #[test]
  fn page_padding_matches_tier_contract() {
    assert_eq!(page_padding(SizeClass::Compact), TOKENS.spacing.s4);
    assert_eq!(page_padding(SizeClass::Compact), 16.0);
    assert_eq!(page_padding(SizeClass::Standard), PAGE_PADDING);
    assert_eq!(page_padding(SizeClass::Standard), 32.0);
    assert_eq!(page_padding(SizeClass::Wide), PAGE_PADDING);
    assert_eq!(page_padding(SizeClass::Wide), 32.0);
  }
  #[test]
  fn grid_available_width_matches_sidebar_contract() {
    // 1600×900 default window, 240px sidebar, 1px hairline, 2×32 page padding.
    assert_eq!(grid_available_width(1600.0, SizeClass::Standard), 1295.0);
  }

  #[test]
  fn grid_available_width_compact_uses_rail_and_narrow_padding() {
    // 1024 - 72 rail - 1 hairline - 2×16 page padding.
    assert_eq!(grid_available_width(1024.0, SizeClass::Compact), 919.0);
  }

  #[test]
  fn grid_available_width_never_falls_below_one() {
    assert_eq!(grid_available_width(100.0, SizeClass::Compact), 1.0);
  }

  #[test]
  fn skeleton_grid_metrics_match_loaded_grid_metrics_at_same_window_width() {
    for (width, class) in [
      (1024.0, SizeClass::Compact),
      (1600.0, SizeClass::Standard),
      (1920.0, SizeClass::Wide),
    ] {
      let expected =
        ArtworkGridMetrics::for_cards(grid_available_width(width, class), CARD_COPY_HEIGHT);
      let actual = skeleton_grid_metrics(width, class);
      assert_eq!(actual, expected);
      assert_eq!(actual.columns, expected.columns);
      assert_eq!(actual.cell_width, expected.cell_width);
      assert_eq!(actual.cell_height, expected.cell_height);
      assert_eq!(actual.row_height, expected.row_height);
    }
  }

  #[test]
  fn skeleton_cell_dimensions_match_card_geometry() {
    let metrics = ArtworkGridMetrics::for_cards(1248.0, CARD_COPY_HEIGHT);
    let dims = skeleton_cell_dimensions(metrics);
    assert_eq!(dims.cell_width, metrics.cell_width);
    assert_eq!(dims.cell_height, metrics.cell_height);
    assert_eq!(dims.artwork_height, metrics.cell_width * 1.5);
    assert_eq!(dims.cell_height, dims.artwork_height + CARD_COPY_HEIGHT);
  }

  #[test]
  fn card_artwork_height_matches_video_card_aspect_ratio() {
    assert_eq!(card_artwork_height(160.0), 240.0);
    assert_eq!(card_artwork_height(200.0), 300.0);
  }

  #[test]
  fn skeleton_grid_row_and_cell_count_covers_full_page() {
    let metrics = ArtworkGridMetrics::for_cards(1248.0, CARD_COPY_HEIGHT);
    let total_cells = LIBRARY_BROWSE_PAGE_SIZE as usize;
    let row_count = total_cells.div_ceil(metrics.columns);
    assert_eq!(total_cells, 24);
    assert!(row_count >= 1);
    assert_eq!(row_count, (24_usize).div_ceil(metrics.columns));
  }

  #[test]
  fn browse_view_renders_in_loading_state() {
    let mut state = State::boot(false);
    state.shell.skeleton_phase = 0.42;
    state.full.as_mut().unwrap().browse.view = LibraryBrowseView::Loading;
    let _element = view(&state);
  }

  #[test]
  fn browse_view_renders_with_unloaded_slots_in_ready_state() {
    let mut state = State::boot(false);
    state.shell.skeleton_phase = 0.42;
    state.full.as_mut().unwrap().browse.view = LibraryBrowseView::Ready {
      visible_items: vec![
        LibraryItemSlot { item: None },
        LibraryItemSlot { item: None },
      ],
      visible_start: 0,
      mode: jellypilot_core::LibraryBrowseMode::Normal,
      total_record_count: 50,
      is_fetching_more: false,
      load_more_failure: None,
      retry_busy: false,
    };
    let _element = view(&state);
  }

  fn video_item(id: &str) -> VideoLibraryItem {
    VideoLibraryItem {
      logo_image_id: None,
      id: id.to_owned(),
      name: format!("Movie {id}"),
      item_type: "Movie".to_owned(),
      production_year: Some(2024),
      runtime_seconds: None,
      played: false,
      favorite: false,
      artwork_image_id: None,
      backdrop_image_id: None,
      series_poster_image_id: None,
      episode_thumb_image_id: None,
      series_thumb_image_id: None,
      series_backdrop_image_id: None,
      season_number: None,
      episode_number: None,
      series_id: None,
      series_name: None,
      resume_position_seconds: None,
      played_percentage: None,
      overview: None,
      index_number_end: None,
      season_poster_image_id: None,
      end_year: None,
      series_continuing: false,
      unplayed_item_count: None,
    }
  }

  #[test]
  fn browse_view_renders_cards_with_loading_and_failed_artwork_cells() {
    let mut state = State::boot(false);
    state.shell.skeleton_phase = 0.5;
    let item_1 = video_item("item-1");
    let item_2 = video_item("item-2");
    let slot_1 = state
      .kernel
      .artwork_binder
      .bind(jellypilot_core::artwork_binder::ArtworkSurface::Browse);
    let slot_2 = state
      .kernel
      .artwork_binder
      .bind(jellypilot_core::artwork_binder::ArtworkSurface::Browse);
    state.full.as_mut().unwrap().browse.artwork.insert(
      "item-1".to_owned(),
      ArtworkCell {
        slot: slot_1,
        image_id: "img-1".to_owned(),
        state: ArtworkCellState::Loading,
      },
    );
    state.full.as_mut().unwrap().browse.artwork.insert(
      "item-2".to_owned(),
      ArtworkCell {
        slot: slot_2,
        image_id: "img-2".to_owned(),
        state: ArtworkCellState::Failed,
      },
    );
    state.full.as_mut().unwrap().browse.view = LibraryBrowseView::Ready {
      visible_items: vec![
        LibraryItemSlot { item: Some(item_1) },
        LibraryItemSlot { item: Some(item_2) },
      ],
      visible_start: 0,
      mode: jellypilot_core::LibraryBrowseMode::Normal,
      total_record_count: 2,
      is_fetching_more: false,
      load_more_failure: None,
      retry_busy: false,
    };
    let _element = view(&state);
  }
}
