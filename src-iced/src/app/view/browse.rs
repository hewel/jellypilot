use super::image_observer::{observe_grid_viewport, observe_image, ImageAxis};
use crate::app::artwork::{ArtworkSurface, ImageStatus};
use crate::app::artwork::{ImageCell, ImageSpec};
use crate::app::browse::ViewMode;
use crate::app::message::{BrowseMessage, Message};
use crate::app::state::{Destination, State};
use crate::app::{accounts, collections};
use crate::i18n::media::{item_caption, media_type};
use crate::i18n::Localizer;
use iced::widget::{
  column, container, progress_bar, row, scrollable, space, stack, text, Column, Row,
};
use iced::{Alignment, Color, ContentFit, Element, Fill, Length};
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
use jellypilot_ui::overlay::{focus_tooltip, popover, PopoverOptions, TooltipOptions};
use jellypilot_ui::tokens::{ThemePalette, TOKENS};
use jellypilot_ui::variants::ButtonVariant;
use jellypilot_ui::widgets::artwork_grid::{artwork_grid, ArtworkGridMetrics};
use jellypilot_ui::widgets::artwork_progress::ArtworkProgress;
use jellypilot_ui::widgets::control_button::{control_button, control_button_content};
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::library;
use jellypilot_ui::widgets::skeleton::{
  skeleton_block, skeleton_block_with_radius, skeleton_panel,
};
use jellypilot_ui::{full_radius, poster_card, rounded_image};

pub(crate) const PAGE_PADDING: f32 = TOKENS.spacing.s9;

/// Browse content keeps the Paper inset on desktop and reflows on compact windows.
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

pub(crate) const CARD_COPY_HEIGHT: f32 = 50.0;
pub(crate) const GRID_COLUMN_GAP: f32 = TOKENS.spacing.s5;
const LIST_ROW_HEIGHT: f32 = 81.0;
const NARROW_LIST_ROW_HEIGHT: f32 = 112.0;
const LIST_COLUMNS_MIN_WIDTH: f32 = 860.0;

pub(crate) fn browse_metrics(available_width: f32, mode: ViewMode) -> ArtworkGridMetrics {
  let width = available_width.max(1.0);
  if mode == ViewMode::List {
    let height = if width < LIST_COLUMNS_MIN_WIDTH {
      NARROW_LIST_ROW_HEIGHT
    } else {
      LIST_ROW_HEIGHT
    };
    return ArtworkGridMetrics {
      columns: 1,
      cell_width: width,
      cell_height: height,
      row_height: height,
    };
  }
  let columns = ((width + GRID_COLUMN_GAP) / (160.0 + GRID_COLUMN_GAP))
    .floor()
    .max(1.0) as usize;
  let cell_width = (width - GRID_COLUMN_GAP * columns.saturating_sub(1) as f32) / columns as f32;
  let cell_height = card_artwork_height(cell_width) + CARD_COPY_HEIGHT;
  ArtworkGridMetrics {
    columns,
    cell_width,
    cell_height,
    row_height: cell_height + TOKENS.spacing.s6,
  }
}
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
  let mut header = Column::new().spacing(TOKENS.spacing.s4).push(
    text(heading)
      .font(DISPLAY_FONT)
      .size(36)
      .line_height(iced::widget::text::LineHeight::Absolute(40.0.into()))
      .color(state.palette().text.heading),
  );
  if matches!(state.shell.destination, Destination::Library { .. }) {
    header = header.push(toolbar(state));
  }
  header = header.push(presentation_bar(state));

  column![
    container(header)
      .padding(iced::Padding {
        top: TOKENS.spacing.s7,
        right: page_padding(class),
        bottom: TOKENS.spacing.s2,
        left: page_padding(class),
      })
      .width(Fill),
    browse_body(state, class),
  ]
  .height(Fill)
  .width(Fill)
  .into()
}

fn toolbar(state: &State) -> Element<'_, Message> {
  let browse = &state.full.as_ref().expect("FullUi required").browse;
  let filters = browse
    .filters
    .unwrap_or_else(|| state.kernel.settings.snapshot().browse_filters());
  let sort_trigger = control_button(
    Some(Icon::ChevronDown),
    Some(sort_label(state.kernel.locale, filters.sort())),
    ButtonVariant::Pill,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s2)
  .padding([8, 12])
  .min_height(32.0)
  .label_size(12.0)
  .trailing_icon(true)
  .on_press(Message::Browse(BrowseMessage::SortMenuToggled));
  let (direction_icon, direction_label) = match filters.sort_direction() {
    VideoLibrarySortDirection::Ascending => (Icon::SortAscending, state.t("browse-ascending")),
    VideoLibrarySortDirection::Descending => (Icon::SortDescending, state.t("browse-descending")),
  };
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
    control_button(
      Some(direction_icon),
      Some(direction_label),
      ButtonVariant::Text
    )
    .padding([6, 10])
    .width(Fill)
    .label_fill(true)
    .label_size(12.0)
    .on_press(Message::Browse(BrowseMessage::SortDirectionToggled)),
  ]
  .spacing(TOKENS.spacing.s1)
  .width(Fill);
  let sort = popover(
    sort_trigger,
    sort_menu,
    browse.sort_menu_open,
    PopoverOptions {
      width: Some(210.0),
      ..PopoverOptions::default()
    },
    Message::Browse(BrowseMessage::SortMenuDismissed),
  );
  let favorites = control_button(
    Some(if filters.favorites_only() {
      Icon::HeartFilled
    } else {
      Icon::Heart
    }),
    Some(state.t("browse-favorites")),
    if filters.favorites_only() {
      ButtonVariant::PillActive
    } else {
      ButtonVariant::Pill
    },
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s2)
  .padding([8, 12])
  .min_height(32.0)
  .label_size(12.0)
  .on_press(Message::Browse(BrowseMessage::FavoritesToggled));
  row![
    sort,
    played_option(
      Icon::CircleDot,
      state.t("browse-all"),
      VideoLibraryPlayedFilter::All,
      filters.played_filter()
    ),
    played_option(
      Icon::CircleCheck,
      state.t("browse-played"),
      VideoLibraryPlayedFilter::Played,
      filters.played_filter()
    ),
    played_option(
      Icon::Circle,
      state.t("browse-unplayed"),
      VideoLibraryPlayedFilter::Unplayed,
      filters.played_filter()
    ),
    favorites,
  ]
  .spacing(TOKENS.spacing.s2_5)
  .align_y(Alignment::Center)
  .wrap()
  .into()
}

fn presentation_bar(state: &State) -> Element<'_, Message> {
  let browse = &state.full.as_ref().expect("FullUi required").browse;
  let count = match &browse.view {
    LibraryBrowseView::Ready {
      total_record_count, ..
    } => state.format(
      "browse-item-count",
      &[("count", (*total_record_count).into())],
    ),
    LibraryBrowseView::Empty => state.format("browse-item-count", &[("count", 0.into())]),
    _ => String::new(),
  };
  let segments = row![
    mode_button(state, ViewMode::Grid, Icon::Grid, "browse-grid"),
    mode_button(state, ViewMode::List, Icon::List, "browse-list"),
  ]
  .spacing(TOKENS.spacing.s0_5);
  row![
    text(count).size(12).color(state.palette().text.metadata),
    space::horizontal(),
    container(segments)
      .padding(TOKENS.spacing.s0_5)
      .style(library::segments),
  ]
  .align_y(Alignment::Center)
  .width(Fill)
  .into()
}

fn mode_button<'a>(
  state: &'a State,
  mode: ViewMode,
  icon: Icon,
  label: &str,
) -> Element<'a, Message> {
  let selected = state.full.as_ref().expect("FullUi required").browse.mode == mode;
  let control = control_button(
    Some(icon),
    None,
    if selected {
      ButtonVariant::Secondary
    } else {
      ButtonVariant::Text
    },
  )
  .icon_size(IconSize::Xs)
  .padding([5, 6])
  .width(26.into())
  .min_height(24.0)
  .style(|theme, variant, status| {
    library::segment(theme, status, variant == ButtonVariant::Secondary)
  })
  .on_press(Message::Browse(BrowseMessage::ViewModeSelected(mode)));
  focus_tooltip(control, state.t(label), TooltipOptions::default())
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
    ButtonVariant::PillActive
  } else {
    ButtonVariant::Pill
  };
  control_button(Some(icon), Some(label), variant)
    .icon_size(IconSize::Sm)
    .spacing(7.0)
    .padding([8, 12])
    .min_height(32.0)
    .label_size(12.0)
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
  let metrics = browse_metrics(
    available_width,
    state.full.as_ref().expect("FullUi required").browse.mode,
  );
  let surface = &state.full.as_ref().expect("FullUi required").browse;
  let viewport = surface.grid_viewport(state.shell.window_size);
  let grid = artwork_grid(
    total_record_count as usize,
    metrics,
    viewport,
    GRID_COLUMN_GAP,
    |index| match item_at(visible_start, items, index) {
      Some(item) if surface.mode == ViewMode::List => {
        video_row(state, item, index, metrics.cell_width)
      }
      Some(item) => video_card(
        state,
        item,
        metrics.cell_width,
        skeleton_phase,
        reduced_motion,
      ),
      None if surface.mode == ViewMode::List => {
        list_skeleton(metrics.cell_width, skeleton_phase, reduced_motion)
      }
      None => skeleton_cell(metrics.cell_width, skeleton_phase, reduced_motion),
    },
  );
  let grid = observe_grid_viewport(grid, surface.artwork.epoch());
  let mut content = Column::new().width(Fill);
  if surface.mode == ViewMode::List && available_width >= LIST_COLUMNS_MIN_WIDTH {
    content = content.push(list_heading(state));
  }
  let content = container(content.push(grid))
    .padding(iced::Padding {
      top: 0.0,
      right: padding,
      bottom: TOKENS.spacing.s10,
      left: padding,
    })
    .width(Fill);

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
  let mode = state.full.as_ref().expect("FullUi required").browse.mode;
  let metrics = browse_metrics(
    grid_available_width(state.shell.window_size.width, class),
    mode,
  );
  let grid = if mode == ViewMode::Grid {
    browse_skeleton_grid(metrics, skeleton_phase, reduced_motion)
  } else {
    Column::with_children(
      (0..12).map(|_| list_skeleton(metrics.cell_width, skeleton_phase, reduced_motion)),
    )
    .into()
  };
  let content = Column::new()
    .width(Fill)
    .push(container(grid).padding([0.0, padding]).width(Fill));

  scrollable(content)
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable)
    .into()
}

fn browse_skeleton_grid<'a>(
  metrics: ArtworkGridMetrics,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let total_cells = LIBRARY_BROWSE_PAGE_SIZE as usize;
  let row_count = total_cells.div_ceil(metrics.columns);
  let mut grid = Column::new().spacing(TOKENS.spacing.s6).width(Fill);

  for row_index in 0..row_count {
    let start = row_index * metrics.columns;
    let end = (start + metrics.columns).min(total_cells);
    let mut row = Row::new().spacing(GRID_COLUMN_GAP);
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
    skeleton_block(cell_width * 0.6, 16.0, skeleton_phase, reduced_motion),
  ]
  .spacing(TOKENS.spacing.s2)
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
  let card = control_button_content(
    move |_| {
      poster_card(
        item_artwork(
          state,
          item,
          artwork_height,
          false,
          skeleton_phase,
          reduced_motion,
        ),
        column![
          ellipsis_text(&item.name)
            .font(HEADING_FONT)
            .size(14)
            .line_height(iced::Pixels(18.0))
            .color(palette.text.secondary),
          ellipsis_text(item_caption(state.kernel.locale, item))
            .size(12)
            .line_height(iced::Pixels(16.0))
            .color(palette.text.metadata),
        ]
        .spacing(TOKENS.spacing.s2)
        .padding(iced::Padding {
          top: TOKENS.spacing.s2,
          ..iced::Padding::ZERO
        })
        .width(Fill),
      )
      .width(Fill)
      .into()
    },
    ButtonVariant::Text,
  )
  .padding(0)
  .width(Fill)
  .style(jellypilot_ui::widgets::button::media_artwork)
  .on_press(Message::OpenDetail(Box::new(item.clone())));
  let mut overlay = Column::new()
    .push(row![space::horizontal(), favorite_button(state, item)])
    .push(space::vertical());
  if let Some(rating) = item.community_rating.filter(|rating| rating.is_finite()) {
    overlay = overlay.push(
      container(
        row![
          text("★").size(12).line_height(iced::Pixels(14.0)).color(
            jellypilot_ui::tokens::DARK_PALETTE
              .colors
              .onWarningContainer
          ),
          text(format!("{rating:.1}"))
            .font(HEADING_FONT)
            .size(12)
            .line_height(iced::Pixels(14.0)),
        ]
        .spacing(TOKENS.spacing.s1),
      )
      .padding([4, 8])
      .style(library::rating),
    );
  }
  let mut card = stack![
    card,
    container(overlay)
      .padding(TOKENS.spacing.s2)
      .width(Fill)
      .height(artwork_height),
  ];
  if let Some(progress) =
    item_progress(item).filter(|progress| *progress > 0.0 && *progress < 100.0)
  {
    card = card.push(
      container(ArtworkProgress::new(
        f64::from(progress),
        artwork_height,
        4.0,
        full_radius(TOKENS.radii.xl),
        state
          .full
          .as_ref()
          .expect("FullUi required")
          .browse
          .artwork
          .get(&item.id)
          .and_then(ImageCell::handle)
          .cloned(),
        library::artwork_progress_style(&jellypilot_ui::theme::theme(state.theme_mode()), true),
      ))
      .width(Fill)
      .height(artwork_height)
      .align_y(Alignment::End),
    );
  }
  focus_tooltip(card, item.name.clone(), TooltipOptions::default())
}

fn item_artwork<'a>(
  state: &'a State,
  item: &'a VideoLibraryItem,
  height: f32,
  compact: bool,
  phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let browse = &state.full.as_ref().expect("FullUi required").browse;
  let image = artwork(
    state,
    browse.artwork.get(&item.id),
    &item.name,
    height,
    compact,
    phase,
    reduced_motion,
  );
  if let Some(image_id) = &item.artwork_image_id {
    observe_image(
      image,
      ArtworkSurface::Browse,
      browse.artwork.epoch(),
      ImageSpec {
        key: item.id.clone(),
        image_id: image_id.clone(),
        size_class: ArtworkSizeClass::Card,
        derived: DerivedArtwork::default(),
      },
      ImageAxis::Vertical,
    )
  } else {
    image
  }
}

fn favorite_button<'a>(state: &'a State, item: &'a VideoLibraryItem) -> Element<'a, Message> {
  let full = state.full.as_ref().expect("FullUi required");
  let enabled = state.kernel.client.is_some()
    && !state.shell.quit_requested
    && !accounts::content_mutations_blocked(&state.accounts)
    && !collections::busy(full, &item.id);
  let action = enabled.then(|| {
    Message::Collections(collections::CollectionMessage::Favorite {
      session: state.kernel.request_gate.current_session(),
      item_id: item.id.clone(),
      favorite: !item.favorite,
    })
  });
  let control = control_button_content(
    move |_| {
      icon_with_color(
        if item.favorite {
          Icon::HeartFilled
        } else {
          Icon::Heart
        },
        IconSize::Sm,
        if item.favorite {
          state.palette().colors.favorite
        } else {
          state.palette().text.metadata
        },
      )
      .into()
    },
    ButtonVariant::Icon,
  )
  .padding(12)
  .width(40.into())
  .min_height(40.0)
  .style(|theme, _, status| library::row(theme, status))
  .on_press_maybe(action);
  focus_tooltip(
    control,
    state.t(if item.favorite {
      "browse-unfavorite"
    } else {
      "browse-favorite"
    }),
    TooltipOptions::default(),
  )
}

fn item_progress(item: &VideoLibraryItem) -> Option<f32> {
  if item.played {
    return Some(100.0);
  }
  if let Some(value) = item.played_percentage.filter(|value| value.is_finite()) {
    return Some(value.clamp(0.0, 100.0) as f32);
  }
  match (item.resume_position_seconds, item.runtime_seconds) {
    (Some(position), Some(runtime))
      if position.is_finite() && position >= 0.0 && runtime.is_finite() && runtime > 0.0 =>
    {
      Some((position / runtime * 100.0).clamp(0.0, 100.0) as f32)
    }
    _ => None,
  }
}

fn progress_caption(state: &State, item: &VideoLibraryItem) -> String {
  if item.played {
    state.t("browse-played")
  } else if let Some(progress) = item_progress(item).filter(|progress| *progress > 0.0) {
    state.format(
      "browse-progress-percent",
      &[("percent", f64::from(progress.round()).into())],
    )
  } else {
    state.t("browse-unplayed")
  }
}

fn row_progress<'a>(state: &'a State, item: &VideoLibraryItem) -> Element<'a, Message> {
  let mut content = Row::new()
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center);
  if let Some(progress) =
    item_progress(item).filter(|progress| *progress > 0.0 && *progress < 100.0)
  {
    content = content.push(
      progress_bar(0.0..=100.0, progress)
        .girth(4)
        .length(Fill)
        .style(library::progress),
    );
  }
  content
    .push(
      text(progress_caption(state, item))
        .size(12)
        .color(if item.played {
          state.palette().colors.tertiary
        } else {
          state.palette().text.metadata
        }),
    )
    .width(170)
    .into()
}

fn list_heading(state: &State) -> Element<'_, Message> {
  let label = |id: &str, width: Length| {
    text(state.t(id))
      .size(12)
      .line_height(iced::Pixels(14.0))
      .color(state.palette().text.metadata)
      .width(width)
  };
  let headings = row![
    text("#")
      .size(12)
      .line_height(iced::Pixels(14.0))
      .color(state.palette().text.metadata)
      .width(36),
    space::horizontal().width(40),
    label("browse-column-title", Fill),
    label("browse-column-year", 90.into()),
    label("browse-column-rating", 90.into()),
    label("browse-column-progress", 170.into()),
    space::horizontal().width(40),
  ]
  .spacing(TOKENS.spacing.s4)
  .align_y(Alignment::Center);
  column![
    container(headings).padding([10, 12]).height(34).width(Fill),
    iced::widget::rule::horizontal(1).style(library::divider),
  ]
  .into()
}

fn row_contents<'a>(
  state: &'a State,
  item: &'a VideoLibraryItem,
  index: usize,
  width: f32,
) -> Element<'a, Message> {
  let narrow = width < LIST_COLUMNS_MIN_WIDTH;
  let palette = state.palette();
  let kind = media_type(state.kernel.locale, &item.item_type);
  let subtitle = match item.episode_count {
    Some(count) => state.format(
      "browse-kind-episodes",
      &[("kind", kind.into()), ("count", count.into())],
    ),
    None => kind,
  };
  let mut copy = column![
    ellipsis_text(&item.name)
      .font(HEADING_FONT)
      .size(14)
      .line_height(iced::Pixels(18.0))
      .color(palette.text.secondary),
    ellipsis_text(subtitle)
      .size(12)
      .line_height(iced::Pixels(16.0))
      .color(palette.text.metadata),
  ]
  .spacing(TOKENS.spacing.s1)
  .width(Fill);
  let rating = item.community_rating.filter(|rating| rating.is_finite());
  if narrow {
    let mut metadata = Row::new()
      .spacing(TOKENS.spacing.s2)
      .align_y(Alignment::Center);
    if let Some(year) = item.production_year {
      metadata = metadata.push(text(year).size(12).color(palette.text.metadata));
    }
    if let Some(rating) = rating {
      metadata = metadata.push(
        text(format!("★ {rating:.1}"))
          .size(12)
          .color(palette.colors.warning),
      );
    }
    copy = copy.push(metadata).push(
      text(progress_caption(state, item))
        .size(12)
        .color(palette.text.metadata),
    );
  }
  let poster = container(item_artwork(
    state,
    item,
    60.0,
    true,
    state.shell.skeleton_phase,
    state.kernel.settings.snapshot().reduced_motion(),
  ))
  .width(40)
  .height(60);
  let mut main = row![
    text(index + 1)
      .size(12)
      .color(palette.text.metadata)
      .width(if narrow { 24 } else { 36 }),
    poster,
    copy,
  ]
  .spacing(TOKENS.spacing.s4)
  .align_y(Alignment::Center)
  .width(Fill);
  if !narrow {
    main = main
      .push(
        text(
          item
            .production_year
            .map_or_else(String::new, |year| year.to_string()),
        )
        .size(12)
        .color(palette.text.body)
        .width(90),
      )
      .push(
        text(rating.map_or_else(String::new, |rating| format!("★ {rating:.1}")))
          .size(12)
          .color(palette.colors.warning)
          .width(90),
      )
      .push(row_progress(state, item));
  }
  main.into()
}

fn video_row<'a>(
  state: &'a State,
  item: &'a VideoLibraryItem,
  index: usize,
  width: f32,
) -> Element<'a, Message> {
  let height = browse_metrics(width, ViewMode::List).cell_height;
  let open = control_button_content(
    move |_| row_contents(state, item, index, width),
    ButtonVariant::Text,
  )
  .padding([10, 0])
  .min_height(height - 1.0)
  .width(Fill)
  .style(|theme, _, status| library::row(theme, status))
  .on_press(Message::OpenDetail(Box::new(item.clone())));
  let open = focus_tooltip(open, item.name.clone(), TooltipOptions::default());
  let row = container(
    row![open, favorite_button(state, item)]
      .spacing(TOKENS.spacing.s4)
      .align_y(Alignment::Center),
  )
  .padding([0, 12])
  .width(Fill);
  column![
    row,
    iced::widget::rule::horizontal(1).style(library::divider)
  ]
  .into()
}

fn list_skeleton<'a>(width: f32, phase: f32, reduced_motion: bool) -> Element<'a, Message> {
  let height = browse_metrics(width, ViewMode::List).cell_height;
  container(
    row![
      skeleton_block(36.0, 14.0, phase, reduced_motion),
      skeleton_block(40.0, 60.0, phase, reduced_motion),
      column![
        skeleton_block((width * 0.4).min(300.0), 18.0, phase, reduced_motion),
        skeleton_block((width * 0.25).min(160.0), 14.0, phase, reduced_motion),
      ]
      .spacing(TOKENS.spacing.s1),
    ]
    .spacing(TOKENS.spacing.s4)
    .align_y(Alignment::Center),
  )
  .padding([10, 12])
  .width(Fill)
  .height(height)
  .into()
}

fn artwork<'a>(
  state: &'a State,
  cell: Option<&ImageCell>,
  name: &'a str,
  height: f32,
  compact: bool,
  phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let palette = state.palette();
  let radius = full_radius(if compact {
    TOKENS.radii.lg
  } else {
    TOKENS.radii.xl
  });
  if let Some(cell) = cell {
    if cell.state == ImageStatus::Ready {
      if let Some(handle) = cell.handle() {
        return stack![
          rounded_image(handle.clone(), radius)
            .content_fit(ContentFit::Cover)
            .width(Fill)
            .height(height),
          container(space::horizontal())
            .width(Fill)
            .height(height)
            .style(move |theme| library::artwork_outline(theme, compact)),
        ]
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
        icon_with_color(
          Icon::Movie,
          IconSize::Custom(if compact { 20.0 } else { 36.0 }),
          placeholder_color
        ),
        text(initial)
          .font(HEADING_FONT)
          .size(if compact { 14 } else { 24 })
          .color(placeholder_color),
      ]
      .spacing(TOKENS.spacing.s1)
      .align_x(Alignment::Center),
    )
    .width(Fill)
    .height(height)
    .center_x(Fill)
    .align_y(Alignment::Center)
    .style(move |theme| {
      let mut style = library::poster_placeholder(theme);
      style.border.radius = radius;
      style
    })
    .into();
  }

  skeleton_panel(
    Fill,
    height,
    palette.colors.surfaceContainerLowest,
    radius,
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
  async fn browse_presentations_fit_their_paging_cells_at_desktop_and_narrow_widths() {
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
    let mut item = video_item("long-title");
    item.name =
      "A long library title that must not push the favorite action off the edge".to_owned();
    item.community_rating = Some(8.7);
    item.episode_count = Some(18);
    item.played_percentage = Some(45.0);
    for width in [360.0, 700.0, 860.0, 1148.0] {
      for mode in [ViewMode::Grid, ViewMode::List] {
        let metrics = browse_metrics(width, mode);
        let mut cell = match mode {
          ViewMode::Grid => video_card(&state, &item, metrics.cell_width, 0.0, true),
          ViewMode::List => video_row(&state, &item, 0, metrics.cell_width),
        };
        let mut tree = Tree::new(&cell);
        tree.diff(cell.as_widget_mut());
        let node = cell.as_widget_mut().layout(
          &mut tree,
          &renderer,
          &layout::Limits::new(Size::ZERO, Size::new(metrics.cell_width, 1000.0)),
        );
        assert!(
          (node.size().height - metrics.cell_height).abs() < 0.1,
          "{mode:?} content clips or overlaps its next paging row at width {width}: {:?}",
          node.size(),
        );
        assert_fits_width(&node);
        let mut placeholder = match mode {
          ViewMode::Grid => skeleton_cell(metrics.cell_width, 0.0, true),
          ViewMode::List => list_skeleton(metrics.cell_width, 0.0, true),
        };
        let mut tree = Tree::new(&placeholder);
        tree.diff(placeholder.as_widget_mut());
        let loading = placeholder.as_widget_mut().layout(
          &mut tree,
          &renderer,
          &layout::Limits::new(Size::ZERO, Size::new(metrics.cell_width, 1000.0)),
        );
        assert!(
          (loading.size().height - node.size().height).abs() < 0.1,
          "loading and loaded presentation must not shift the viewport"
        );
      }
    }
  }

  fn assert_fits_width(node: &layout::Node) {
    for child in node.children() {
      assert!(
        child.bounds().x >= -0.1
          && child.bounds().x + child.size().width <= node.size().width + 0.1,
        "content escapes its horizontal bounds: parent={:?}, child={:?}",
        node.bounds(),
        child.bounds(),
      );
      assert_fits_width(child);
    }
  }

  #[test]
  fn progress_uses_server_completion_and_rejects_invalid_ratios() {
    let mut item = video_item("progress");
    assert_eq!(item_progress(&item), None);
    item.played_percentage = Some(f64::NAN);
    item.resume_position_seconds = Some(45.0);
    item.runtime_seconds = Some(100.0);
    assert_eq!(item_progress(&item), Some(45.0));
    item.runtime_seconds = Some(0.0);
    assert_eq!(item_progress(&item), None);
    item.played = true;
    assert_eq!(item_progress(&item), Some(100.0));
  }

  fn video_item(id: &str) -> VideoLibraryItem {
    VideoLibraryItem {
      premiere_date: None,
      community_rating: None,
      episode_count: None,
      last_played_date: None,
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
