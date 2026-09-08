use super::image_observer::{observe_grid_viewport, observe_image, ImageAxis};
use crate::app::artwork::{ArtworkSurface, ImageStatus};
use crate::app::artwork::{ImageCell, ImageSpec};
use crate::app::message::{BrowseMessage, Message};
use crate::app::state::{Destination, State};
use crate::i18n::media::item_caption;
use crate::i18n::Localizer;
use iced::widget::{button, column, container, row, scrollable, stack, text, Column, Row};
use iced::{Alignment, Color, ContentFit, Element, Fill};
use jellypilot_core::browse_model::{LibraryBrowseView, LibraryItemSlot};
use jellypilot_core::diagnostics::sanitize_message;
use jellypilot_core::{LibraryBrowseFailure, LIBRARY_BROWSE_PAGE_SIZE};
use jellypilot_media_server::artwork::{ArtworkSizeClass, DerivedArtwork};
use jellypilot_media_server::{
  VideoLibraryItem, VideoLibraryPlayedFilter, VideoLibrarySort, VideoLibrarySortDirection,
};
use jellypilot_ui::fonts::{DISPLAY_FONT, HEADING_FONT};
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::layout::SizeClass;
use jellypilot_ui::overlay::{popover, PopoverOptions};
use jellypilot_ui::tokens::{ThemePalette, TOKENS};
use jellypilot_ui::variants::ButtonVariant;
use jellypilot_ui::widgets::artwork_grid::{artwork_grid, ArtworkGridMetrics};
use jellypilot_ui::widgets::control_button::control_button;
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::skeleton::{
  skeleton_block, skeleton_block_with_radius, skeleton_panel,
};
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
/// The shell must seed this from the actual opened window and track later resizes.
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
  let library_label = state.t("browse-library");
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
        .map_or(library_label.as_str(), |shortcut| shortcut.name.as_str()),
      jellypilot_core::LoadState::Idle
      | jellypilot_core::LoadState::Loading
      | jellypilot_core::LoadState::Failed(_) => library_label.as_str(),
    },
    Destination::Search(query) => query,
    Destination::Home
    | Destination::PersonalLists(_)
    | Destination::Detail(_)
    | Destination::NowPlaying => library_label.as_str(),
  };
  let heading = match &state.shell.destination {
    Destination::Search(_) => state.format("browse-search-results", &[("query", title.into())]),
    Destination::Home
    | Destination::PersonalLists(_)
    | Destination::Library { .. }
    | Destination::Detail(_)
    | Destination::NowPlaying => title.to_owned(),
  };
  let mut header = Column::new().spacing(TOKENS.spacing.s3).push(
    text(heading)
      .font(DISPLAY_FONT)
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
  let filters = state
    .full
    .as_ref()
    .expect("FullUi required")
    .browse
    .filters
    .unwrap_or_else(|| state.kernel.settings.snapshot().browse_filters());
  let sort_trigger = control_button(
    Some(Icon::Sliders),
    Some(state.format(
      "browse-sort",
      &[(
        "sort",
        sort_label(state.kernel.locale, filters.sort()).into(),
      )],
    )),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 12])
  .on_press(Message::Browse(BrowseMessage::SortMenuToggled));
  let sort_menu = column![
    sort_option(state.t("browse-sort-title"), VideoLibrarySort::Title),
    sort_option(
      state.t("browse-sort-recently-added"),
      VideoLibrarySort::RecentlyAdded
    ),
    sort_option(
      state.t("browse-sort-release-date"),
      VideoLibrarySort::ReleaseDate
    ),
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
    VideoLibrarySortDirection::Ascending => (Icon::SortAscending, state.t("browse-ascending")),
    VideoLibrarySortDirection::Descending => (Icon::SortDescending, state.t("browse-descending")),
  };
  let direction = control_button(
    Some(direction_icon),
    Some(direction_label),
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
        text(state.t("browse-favorites-on")),
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
      Some(state.t("browse-favorites-off")),
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
      state.t("browse-all"),
      VideoLibraryPlayedFilter::All,
      filters.played_filter(),
    ),
    played_option(
      Icon::CircleCheck,
      state.t("browse-played"),
      VideoLibraryPlayedFilter::Played,
      filters.played_filter(),
    ),
    played_option(
      Icon::Circle,
      state.t("browse-unplayed"),
      VideoLibraryPlayedFilter::Unplayed,
      filters.played_filter(),
    ),
    favorites,
  ]
  .spacing(TOKENS.spacing.s2)
  .align_y(Alignment::Center)
  .into()
}

fn sort_option(label: String, sort: VideoLibrarySort) -> Element<'static, Message> {
  control_button(None, Some(label), ButtonVariant::Text)
    .padding([6, 10])
    .width(Fill)
    .label_fill(true)
    .on_press(Message::Browse(BrowseMessage::SortChanged(sort)))
    .into()
}

fn played_option(
  icon: Icon,
  label: String,
  value: VideoLibraryPlayedFilter,
  selected: VideoLibraryPlayedFilter,
) -> Element<'static, Message> {
  let variant = if value == selected {
    ButtonVariant::TonalActive
  } else {
    ButtonVariant::Tonal
  };
  control_button(Some(icon), Some(label), variant)
    .icon_size(IconSize::Sm)
    .spacing(TOKENS.spacing.s1_5)
    .padding([6, 12])
    .on_press(Message::Browse(BrowseMessage::PlayedFilterChanged(value)))
    .into()
}

fn browse_body<'a>(state: &'a State, class: SizeClass) -> Element<'a, Message> {
  let padding = page_padding(class);
  let browse = &state.full.as_ref().expect("FullUi required").browse;
  match &browse.view {
    LibraryBrowseView::Inactive => {
      empty_surface(state.palette(), state.t("browse-choose-library"), padding)
    }
    LibraryBrowseView::Loading => browse_loading_skeleton(state, class),
    LibraryBrowseView::Empty => {
      let body = match &state.shell.destination {
        Destination::Search(query) => empty_surface(
          state.palette(),
          state.format("browse-no-results", &[("query", query.as_str().into())]),
          padding,
        ),
        Destination::Home
        | Destination::PersonalLists(_)
        | Destination::Library { .. }
        | Destination::Detail(_)
        | Destination::NowPlaying => {
          empty_surface(state.palette(), state.t("browse-empty-library"), padding)
        }
      };
      let mut surface = stack![body].width(Fill).height(Fill);
      if let Some(banner) = failure_overlay(
        state.palette(),
        state.kernel.locale,
        browse.data.refresh_failure(),
        browse.data.is_refreshing(),
      ) {
        surface = surface.push(banner);
      }
      surface.into()
    }
    LibraryBrowseView::Failed {
      message,
      retryable,
      retry_busy,
    } => failure_surface(
      state.palette(),
      state.kernel.locale,
      matches!(state.shell.destination, Destination::Search(_)),
      message,
      *retryable,
      *retry_busy,
      padding,
    ),
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
      browse.data.refresh_failure().or(load_more_failure.as_ref()),
      *retry_busy || browse.data.is_refreshing(),
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
  let surface = &state.full.as_ref().expect("FullUi required").browse;
  let viewport = surface.grid_viewport(state.shell.window_size);
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
  let grid = observe_grid_viewport(grid, surface.artwork.epoch());
  let content = Column::new()
    .width(Fill)
    .push(
      row![
        text(state.format("browse-item-count", &[("count", total_record_count.into())]))
          .size(13)
          .color(state.palette().text.metadata),
      ]
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
  if let Some(banner) = failure_overlay(
    state.palette(),
    state.kernel.locale,
    load_more_failure,
    retry_busy,
  ) {
    surface = surface.push(banner);
  }
  surface.into()
}

/// Builds the viewport-pinned banner for a refresh or incremental load failure.
fn failure_overlay<'a>(
  palette: &'static ThemePalette,
  locale: Localizer,
  load_more_failure: Option<&'a LibraryBrowseFailure>,
  retry_busy: bool,
) -> Option<Element<'a, Message>> {
  load_more_failure.map(|failure| failure_banner(palette, locale, failure, retry_busy))
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

pub(crate) fn card_artwork_height(cell_width: f32) -> f32 {
  cell_width * POSTER_FRAME_HEIGHT / POSTER_FRAME_WIDTH
}

fn skeleton_cell<'a>(
  cell_width: f32,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let artwork_height = card_artwork_height(cell_width);
  let poster = skeleton_block_with_radius(
    cell_width,
    artwork_height,
    full_radius(TOKENS.radii.xl),
    skeleton_phase,
    reduced_motion,
  );
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
  let artwork = if let Some(image_id) = &item.artwork_image_id {
    observe_image(
      artwork,
      ArtworkSurface::Browse,
      state
        .full
        .as_ref()
        .expect("FullUi required")
        .browse
        .artwork
        .epoch(),
      ImageSpec {
        key: item.id.clone(),
        image_id: image_id.clone(),
        size_class: ArtworkSizeClass::Card,
        derived: DerivedArtwork::default(),
      },
      ImageAxis::Vertical,
    )
  } else {
    artwork
  };
  let copy = column![
    ellipsis_text(&item.name)
      .size(14)
      .color(palette.text.heading),
    ellipsis_text(item_caption(state.kernel.locale, item))
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
  cell: Option<&ImageCell>,
  name: &'a str,
  height: f32,
  phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let palette = state.palette();
  if let Some(cell) = cell {
    if cell.state == ImageStatus::Ready {
      if let Some(handle) = cell.handle() {
        return rounded_image(handle.clone(), full_radius(TOKENS.radii.xl))
          .content_fit(ContentFit::Cover)
          .width(Fill)
          .height(height)
          .into();
      }
    }
  }

  let placeholder_color = match cell.map(|cell| cell.state) {
    // No active demand also uses the neutral placeholder. Keep its image bounds
    // unchanged across admission and removal to avoid visibility feedback.
    None => Some(palette.text.metadata),
    Some(ImageStatus::Failed) => Some(palette.colors.warning),
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
          .font(HEADING_FONT)
          .size(24)
          .color(placeholder_color),
      ]
      .spacing(TOKENS.spacing.s1)
      .align_x(Alignment::Center),
    )
    .width(Fill)
    .height(height)
    .center_x(Fill)
    .align_y(Alignment::Center)
    .style(|_theme| container::Style {
      background: Some(iced::Background::Color(
        palette.colors.surfaceContainerLowest,
      )),
      border: iced::Border {
        smoothing: jellypilot_ui::widgets::container::SURFACE_SMOOTHING,
        radius: full_radius(TOKENS.radii.xl),
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
    full_radius(TOKENS.radii.xl),
    phase,
    reduced_motion,
  )
  .into()
}
fn failure_surface<'a>(
  palette: &'static ThemePalette,
  locale: Localizer,
  is_search: bool,
  message: &'a str,
  retryable: bool,
  retry_busy: bool,
  padding: f32,
) -> Element<'a, Message> {
  let retry = control_button(
    Some(Icon::Refresh),
    Some(if retry_busy {
      locale.text("browse-retrying")
    } else {
      locale.text("browse-retry")
    }),
    ButtonVariant::Primary,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 12])
  .on_press_maybe(retry_action(retryable, retry_busy));
  container(
    column![
      text(locale.text(if is_search {
        "browse-search-load-failed"
      } else {
        "browse-library-load-failed"
      }))
      .font(DISPLAY_FONT)
      .size(24)
      .color(palette.text.heading),
      text(sanitize_message(message))
        .size(14)
        .color(palette.colors.error),
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
  locale: Localizer,
  failure: &'a LibraryBrowseFailure,
  retry_busy: bool,
) -> Element<'a, Message> {
  let colors = palette.colors;
  let retry = control_button(
    Some(Icon::Refresh),
    Some(if retry_busy {
      locale.text("browse-retrying")
    } else {
      locale.text("browse-retry")
    }),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Xs)
  .spacing(TOKENS.spacing.s1)
  .padding([6, 10])
  .on_press_maybe(retry_action(failure.retryable, retry_busy));

  let banner = container(
    row![
      text(locale.format(
        "browse-load-more-failed",
        &[("details", sanitize_message(&failure.message).into())]
      ))
      .size(13),
      retry,
    ]
    .spacing(TOKENS.spacing.s3)
    .align_y(Alignment::Center),
  )
  .padding(TOKENS.spacing.s4)
  .style(move |_theme| container::Style {
    background: Some(iced::Background::Color(colors.errorContainer)),
    text_color: Some(colors.onErrorContainer),
    border: iced::Border {
      smoothing: jellypilot_ui::widgets::container::SURFACE_SMOOTHING,
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

fn sort_label(locale: Localizer, sort: VideoLibrarySort) -> String {
  locale.text(match sort {
    VideoLibrarySort::Title => "browse-sort-title",
    VideoLibrarySort::RecentlyAdded => "browse-sort-recently-added",
    VideoLibrarySort::ReleaseDate => "browse-sort-release-date",
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use iced::advanced::{layout, renderer, renderer::Headless, widget::Tree};
  use iced::futures::StreamExt;
  use iced::{Font, Size};

  #[test]
  fn failure_banner_overlays_only_when_a_load_more_failure_is_present() {
    let failure = LibraryBrowseFailure {
      message: "Could not load more items.".to_owned(),
      retryable: true,
    };
    assert!(
      failure_overlay(
        &jellypilot_ui::tokens::DARK_PALETTE,
        Localizer::default(),
        Some(&failure),
        false
      )
      .is_some(),
      "a ready surface with a load-more failure must pin the banner overlay"
    );
    assert!(
      failure_overlay(
        &jellypilot_ui::tokens::DARK_PALETTE,
        Localizer::default(),
        Some(&failure),
        true
      )
      .is_some(),
      "the banner stays pinned while a retry is in flight"
    );
    assert!(
      failure_overlay(
        &jellypilot_ui::tokens::DARK_PALETTE,
        Localizer::default(),
        None,
        false
      )
      .is_none(),
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

  #[tokio::test]
  async fn browse_rows_keep_complete_equal_posters_at_responsive_window_widths() {
    let renderer = iced::Renderer::new(
      renderer::Settings {
        font: Font::DEFAULT,
        text_size: 16.0.into(),
        line_height: jellypilot_ui::fonts::DEFAULT_LINE_HEIGHT,
        metrics_hinting: false,
      },
      Some("tiny-skia"),
    )
    .await
    .expect("software layout renderer");
    let mut state = State::boot(true);
    state.kernel.settings = jellypilot_core::config::SettingsStore::default();
    state.full = Some(crate::app::state::FullUi::default());
    state.shell.window_size = Size::new(1760.0, 900.0);
    let window_id = iced::window::Id::unique();
    let items: Vec<_> = (0..LIBRARY_BROWSE_PAGE_SIZE)
      .map(|index| LibraryItemSlot {
        item: Some(video_item(&index.to_string())),
      })
      .collect();

    for (index, width) in [
      1280.0, 800.0, 1024.0, 1279.5, 1440.0, 1760.0, 1919.5, 1920.0,
    ]
    .into_iter()
    .enumerate()
    {
      let size = Size::new(width, 900.0);
      let event = if index == 0 {
        iced::window::Event::Opened {
          position: None,
          size,
          scale_factor: 1.25,
        }
      } else {
        iced::window::Event::Resized(size)
      };
      let input = iced::advanced::subscription::Event::Interaction {
        window: window_id,
        event: iced::Event::Window(event),
        status: iced::event::Status::Ignored,
      };
      let streams = iced::advanced::subscription::into_recipes(crate::app::subscription(&state))
        .into_iter()
        .map(|recipe| recipe.stream(Box::pin(iced::futures::stream::iter(vec![input.clone()]))));
      let messages: Vec<_> = iced::futures::stream::select_all(streams).collect().await;
      for message in messages {
        drop(crate::app::update(&mut state, message));
      }
      if index == 0 {
        // iced delivers Opened before the open task resolves with the window id.
        drop(crate::app::update(
          &mut state,
          Message::Window(crate::app::message::WindowMessage::ShowRequested(Some(
            window_id,
          ))),
        ));
      }
      let class = SizeClass::from_width(width);
      let content_width =
        width - super::super::shell::sidebar_width(class) - super::super::shell::HAIRLINE_WIDTH;
      let metrics = skeleton_grid_metrics(width, class);
      let limits = layout::Limits::new(Size::ZERO, Size::new(content_width, 700.0));
      let mut ready = ready_surface(
        &state,
        &items,
        0,
        LIBRARY_BROWSE_PAGE_SIZE,
        None,
        false,
        class,
      );
      let mut tree = Tree::new(&ready);
      tree.diff(ready.as_widget_mut());
      let ready_node = ready.as_widget_mut().layout(&mut tree, &renderer, &limits);
      let scroll = &ready_node.children()[0];
      let grid_container = &scroll.children()[0].children()[1];
      let grid = &grid_container.children()[0];
      assert!(
        (grid.size().width - grid_available_width(width, class)).abs() < 0.01,
        "render and paging widths diverged at window width {width}"
      );
      // The first and last grid children are virtualization spacers.
      for row in &grid.children()[1..grid.children().len() - 1] {
        for cell in row.children() {
          let card = &cell.children()[0];
          let poster = &card.children()[0];
          assert!(
            cell.bounds().x >= 0.0
              && cell.bounds().x + cell.size().width <= grid.size().width + 0.01
              && (cell.size().width - metrics.cell_width).abs() < 0.01
              && (card.size().width - metrics.cell_width).abs() < 0.01
              && (poster.size().width - metrics.cell_width).abs() < 0.01
              && (poster.size().height - card_artwork_height(metrics.cell_width)).abs() < 0.01
              && poster.size().height <= card.size().height,
            "incomplete or unequal poster at window width {width}: cell={:?}, card={:?}, poster={:?}",
            cell.bounds(),
            card.bounds(),
            poster.bounds(),
          );
        }
      }
      assert_eq!(grid.children()[1].children().len(), metrics.columns);

      let mut loading = browse_loading_skeleton(&state, class);
      let mut tree = Tree::new(&loading);
      tree.diff(loading.as_widget_mut());
      let loading_node = loading
        .as_widget_mut()
        .layout(&mut tree, &renderer, &limits);
      let skeleton_grid = &loading_node.children()[0].children()[0].children()[0];
      let skeleton_row = &skeleton_grid.children()[0];
      for (skeleton, loaded) in skeleton_row
        .children()
        .iter()
        .zip(grid.children()[1].children())
      {
        assert!(
          (skeleton.size().width - loaded.size().width).abs() < 0.01
            && (skeleton.size().height - loaded.size().height).abs() < 0.01,
          "loading and loaded cells shift geometry at window width {width}"
        );
      }
      assert_eq!(skeleton_row.children().len(), metrics.columns);
    }
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
}
