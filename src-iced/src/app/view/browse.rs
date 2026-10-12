use super::image_observer::{observe_grid_viewport, observe_image, ImageAxis};
use crate::app::artwork::{ArtworkSurface, ImageStatus};
use crate::app::artwork::{ImageCell, ImageSpec};
use crate::app::browse::ViewMode;
use crate::app::list_playback::Target;
use crate::app::message::PlaybackMessage;
use crate::app::message::{BrowseMessage, Message};
use crate::app::state::{Destination, State};
use crate::app::{accounts, collections, list_playback};
use crate::i18n::media::{item_caption, media_type};
use crate::i18n::Localizer;
use iced::advanced::{layout, mouse, overlay, renderer, widget, Layout, Shell, Widget};
use iced::widget::Widget as _;
use iced::widget::{
  column, container, hover, progress_bar, row, scrollable, space, stack, text, Column, Row,
};
use iced::{
  Alignment, Color, ContentFit, Element, Event, Fill, Length, Rectangle, Size, Theme, Vector,
};
use jellypilot_core::browse_model::{LibraryBrowseView, LibraryItemSlot};
use jellypilot_core::cards::{episode_card_code, is_episode_item};
use jellypilot_core::diagnostics::sanitize_message;
use jellypilot_core::home_hero::has_resume_position;
use jellypilot_core::{LibraryBrowseFailure, LIBRARY_BROWSE_PAGE_SIZE};
use jellypilot_media_server::artwork::{ArtworkSizeClass, DerivedArtwork};
use jellypilot_media_server::{
  VideoLibraryItem, VideoLibraryPlayedFilter, VideoLibrarySort, VideoLibrarySortDirection,
};
use jellypilot_mpv::playback::{Playable, PlaybackStartPosition};
use jellypilot_mpv::playback_session::PlaybackIntent;
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

pub(crate) const CARD_COPY_HEIGHT: f32 = 74.0;
pub(crate) const GRID_COLUMN_GAP: f32 = TOKENS.spacing.s5;
const LIST_ROW_HEIGHT: f32 = 81.0;
const NARROW_LIST_ROW_HEIGHT: f32 = 132.0;
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
    | Destination::SavedBrowse
    | Destination::PersonalLists(_)
    | Destination::Detail(_)
    | Destination::NowPlaying => library_label.as_str(),
  };
  let heading = match &state.shell.destination {
    Destination::Search(_) => state.format("browse-search-results", &[("query", title.into())]),
    Destination::Home
    | Destination::SavedBrowse
    | Destination::PersonalLists(_)
    | Destination::Library { .. }
    | Destination::Detail(_)
    | Destination::NowPlaying => title.to_owned(),
  };
  let mut header = Column::<Element<'_, Message>>::new()
    .spacing(TOKENS.spacing.s4)
    .push(
      (text(heading)
        .font(DISPLAY_FONT)
        .size(36)
        .line_height(iced::widget::text::LineHeight::Absolute(40.0.into()))
        .color(state.palette().text.heading))
      .boxed(),
    );
  if matches!(state.shell.destination, Destination::Library { .. }) {
    header = header.push((toolbar(state)).boxed());
    if state
      .full
      .as_ref()
      .is_some_and(|full| full.browse.saved.is_some())
    {
      header = header.push((super::saved_browse::active_summary(state)).boxed());
    }
  }
  header = header.push((presentation_bar(state)).boxed());

  let navigation = container(header)
    .padding(iced::Padding {
      top: TOKENS.spacing.s7,
      right: page_padding(class),
      bottom: TOKENS.spacing.s2,
      left: page_padding(class),
    })
    .width(Fill);
  let browse = &state.full.as_ref().expect("FullUi required").browse;
  let body = browse_scroll(
    browse_body(state, class),
    navigation.boxed(),
    matches!(browse.view, LibraryBrowseView::Ready { .. }).then(|| browse.scroll_id.clone()),
  );
  let mut surface = stack![body].width(Fill).height(Fill);
  let (failure, busy) = match &browse.view {
    LibraryBrowseView::Ready {
      load_more_failure,
      retry_busy,
      ..
    } => (
      browse
        .browser
        .model()
        .refresh_failure()
        .or(load_more_failure.as_ref()),
      *retry_busy || browse.browser.model().is_refreshing(),
    ),
    LibraryBrowseView::Empty => (
      browse.browser.model().refresh_failure(),
      browse.browser.model().is_refreshing(),
    ),
    _ => (None, false),
  };
  // Keep retry banners outside the scrolling scene and its navigation capture.
  if let Some(banner) = failure_overlay(state.palette(), state.kernel.locale, failure, busy) {
    surface = surface.push(banner);
  }
  surface.boxed()
}

fn browse_scroll<'a>(
  content: Element<'a, Message>,
  navigation: Element<'a, Message>,
  ready_id: Option<iced::widget::Id>,
) -> Element<'a, Message> {
  let mut body = scrollable(jellypilot_ui::widgets::sticky_navigation::header(
    content, navigation, true,
  ))
  .smooth_scroll(false)
  .width(Fill)
  .height(Fill)
  .style(jellypilot_ui::theme::scrollable);
  if let Some(id) = &ready_id {
    body = body
      .id(id.clone())
      .on_scroll(|viewport| Message::Browse(BrowseMessage::Scrolled(viewport.viewport)));
  }
  iced::advanced::Widget::_boxed(BrowseScroll {
    content: body.boxed(),
    ready_id,
  })
}

struct BrowseScroll<'a> {
  content: Element<'a, Message>,
  ready_id: Option<iced::widget::Id>,
}

struct ScrollIdentity(Option<iced::widget::Id>);

impl widget::Meta for BrowseScroll<'_> {}

impl Widget<Message, Theme, iced::Renderer> for BrowseScroll<'_> {
  fn tag(&self) -> widget::tree::Tag {
    widget::tree::Tag::of::<ScrollIdentity>()
  }

  fn state(&self) -> widget::tree::State {
    widget::tree::State::new(ScrollIdentity(self.ready_id.clone()))
  }

  fn diff(&mut self, tree: &mut widget::Tree) {
    let previous = tree.state.downcast_mut::<ScrollIdentity>();
    if previous.0 != self.ready_id {
      previous.0 = self.ready_id.clone();
      // Offset restoration cannot cancel a Scrollable drag or touch gesture.
      // Replace only its own state: persistent toolbar focus belongs to its
      // children and must survive result settlement or presentation changes.
      if let Some(scrollable) = tree.children.first_mut() {
        scrollable.state = self.content.state();
      }
    }
    tree.diff_children(std::slice::from_mut(&mut self.content));
  }

  fn size(&self) -> Size<Length> {
    self.content.size()
  }

  fn layout(
    &mut self,
    tree: &mut widget::Tree,
    renderer: &iced::Renderer,
    limits: &layout::Limits,
  ) {
    self.content.layout(&mut tree.children[0], renderer, limits);
    tree.size = tree.children[0].size;
    tree.children[0].translation = iced::Vector::ZERO;
  }

  fn operate(
    &mut self,
    tree: &mut widget::Tree,
    layout: Layout,
    viewport: &Rectangle,
    renderer: &iced::Renderer,
    operation: &mut dyn widget::Operation,
  ) {
    self
      .content
      .operate(&mut tree.children[0], layout, viewport, renderer, operation);
  }

  fn update(
    &mut self,
    tree: &mut widget::Tree,
    event: &Event,
    layout: Layout,
    cursor: mouse::Cursor,
    renderer: &iced::Renderer,
    shell: &mut Shell<'_, Message>,
    viewport: &Rectangle,
  ) {
    self.content.update(
      &mut tree.children[0],
      event,
      layout,
      cursor,
      renderer,
      shell,
      viewport,
    );
  }

  fn draw(
    &self,
    tree: &widget::Tree,
    renderer: &mut iced::Renderer,
    theme: &Theme,
    style: &renderer::Style,
    layout: Layout,
    cursor: mouse::Cursor,
    viewport: &Rectangle,
  ) {
    self.content.draw(
      &tree.children[0],
      renderer,
      theme,
      style,
      layout,
      cursor,
      viewport,
    );
  }

  fn mouse_interaction(
    &self,
    tree: &widget::Tree,
    layout: Layout,
    cursor: mouse::Cursor,
    viewport: &Rectangle,
    renderer: &iced::Renderer,
  ) -> mouse::Interaction {
    self
      .content
      .mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
  }

  fn overlay<'a>(
    &'a mut self,
    tree: &'a mut widget::Tree,
    layout: Layout,
    renderer: &iced::Renderer,
    viewport: &Rectangle,
    translation: Vector,
    window: iced::Size,
  ) -> Vec<overlay::Element<'a, Message, Theme, iced::Renderer>> {
    self.content.overlay(
      &mut tree.children[0],
      layout,
      renderer,
      viewport,
      translation,
      window,
    )
  }
}

fn toolbar(state: &State) -> Element<'_, Message> {
  let browse = &state.full.as_ref().expect("FullUi required").browse;
  let filters = browse
    .filters
    .unwrap_or_else(|| state.kernel.settings.snapshot().browse_filters());
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
  .min_height(40.0)
  .label_size(12.0)
  .on_press(Message::Browse(BrowseMessage::FavoritesToggled));
  row![
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
    super::saved_browse::save_button(state),
  ]
  .spacing(TOKENS.spacing.s2_5)
  .align_y(Alignment::Center)
  .wrap()
  .boxed()
}

fn sort_control(state: &State) -> Element<'_, Message> {
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
  .min_height(40.0)
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
  sort
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
  let mut controls = row![
    text(count).size(12).color(state.palette().text.metadata),
    space::horizontal()
  ];
  if matches!(state.shell.destination, Destination::Library { .. }) {
    controls = controls.push((sort_control(state)).boxed());
  }
  controls
    .push(
      (container(segments)
        .padding(TOKENS.spacing.s0_5)
        .style(library::segments))
      .boxed(),
    )
    .spacing(TOKENS.spacing.s3)
    .align_y(Alignment::Center)
    .width(Fill)
    .boxed()
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
  .width(Length::Fixed(40.0))
  .min_height(40.0)
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
    .boxed()
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
    .min_height(40.0)
    .label_size(12.0)
    .on_press(Message::Browse(BrowseMessage::PlayedFilterChanged(value)))
    .boxed()
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
        | Destination::SavedBrowse
        | Destination::Library { .. }
        | Destination::Detail(_)
        | Destination::NowPlaying => {
          empty_surface(state.palette(), state.t("browse-empty-library"), padding)
        }
      };
      body
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
      ..
    } => ready_surface(
      state,
      visible_items,
      *visible_start,
      *total_record_count,
      class,
    ),
  }
}

fn ready_surface<'a>(
  state: &'a State,
  items: &'a [LibraryItemSlot],
  visible_start: u32,
  total_record_count: u32,
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
  let mut content = Column::<Element<'_, Message>>::new().width(Fill);
  if surface.mode == ViewMode::List && available_width >= LIST_COLUMNS_MIN_WIDTH {
    content = content.push((list_heading(state)).boxed());
  }
  let content = container(content.push((grid).boxed()))
    .padding(iced::Padding {
      top: 0.0,
      right: padding,
      bottom: TOKENS.spacing.s10,
      left: padding,
    })
    .width(Fill);

  // Grid/list is a user structural change: data refreshes and scrolling do not re-animate.
  super::motion::transition(content, surface_mode_key(surface_mode(state)))
}

fn surface_mode(state: &State) -> ViewMode {
  state.full.as_ref().expect("FullUi required").browse.mode
}

fn surface_mode_key(mode: ViewMode) -> u64 {
  match mode {
    ViewMode::Grid => 0,
    ViewMode::List => 1,
  }
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
    .boxed()
  };
  let content = Column::<Element<'_, Message>>::new()
    .width(Fill)
    .push((container(grid).padding([0.0, padding]).width(Fill)).boxed());

  content.boxed()
}

fn browse_skeleton_grid<'a>(
  metrics: ArtworkGridMetrics,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
  let total_cells = LIBRARY_BROWSE_PAGE_SIZE as usize;
  let row_count = total_cells.div_ceil(metrics.columns);
  let mut grid = Column::<Element<'_, Message>>::new()
    .spacing(TOKENS.spacing.s6)
    .width(Fill);

  for row_index in 0..row_count {
    let start = row_index * metrics.columns;
    let end = (start + metrics.columns).min(total_cells);
    let mut row = Row::<Element<'_, Message>>::new().spacing(GRID_COLUMN_GAP);
    for _ in start..end {
      row = row.push(
        (container(skeleton_cell(
          metrics.cell_width,
          skeleton_phase,
          reduced_motion,
        ))
        .width(metrics.cell_width)
        .height(metrics.cell_height))
        .boxed(),
      );
    }
    grid = grid.push((row).boxed());
  }

  grid.boxed()
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
  .width(Fill)
  .height(CARD_COPY_HEIGHT);

  column![poster, copy].width(Fill).boxed()
}

const POSTER_FRAME_WIDTH: f32 = 160.0;
const POSTER_FRAME_HEIGHT: f32 = 240.0;

pub(crate) fn play_button<'a>(
  state: &'a State,
  item: &'a VideoLibraryItem,
  circular: bool,
  control_id: String,
) -> Element<'a, Message> {
  let target = list_playback::target(state, item);
  let (icon, label, action) = match target {
    Target::Ready(target) => {
      let label = state.t(if target.played {
        "detail-replay"
      } else if has_resume_position(target) {
        "detail-resume"
      } else {
        "detail-play"
      });
      let enabled = state.playback.view.engine_available
        && !state.shell.quit_requested
        && !accounts::content_mutations_blocked(&state.kernel);
      let action = enabled.then(|| {
        Message::Playback(PlaybackMessage::Intent(Box::new(PlaybackIntent::Start {
          item: Playable::Library(target.clone()),
          position: if has_resume_position(target) {
            PlaybackStartPosition::Resume
          } else {
            PlaybackStartPosition::Beginning
          },
          intro: state.kernel.intro_availability(),
          selection: Box::default(),
        })))
      });
      (
        if target.played {
          Icon::Refresh
        } else {
          Icon::Play
        },
        label,
        action,
      )
    }
    Target::Failed(_) => (
      Icon::Refresh,
      state.t("lists-retry"),
      Some(Message::ListPlayback(list_playback::Message::Retry(
        item.id.clone(),
      ))),
    ),
    Target::Loading => (Icon::Play, state.t("common-loading"), None),
    Target::NoNextUp => (Icon::Play, state.t("list-playback-no-target"), None),
    Target::Unavailable => (Icon::Play, state.t("list-playback-unavailable"), None),
  };
  let tooltip = match target {
    Target::Failed(error) => format!("{}: {error}", state.t("list-playback-failed")),
    Target::Ready(target) => format!("{label}: {}", target.name),
    _ => label.clone(),
  };
  let mut control = control_button(
    Some(icon),
    (!circular).then_some(label),
    if circular {
      ButtonVariant::Primary
    } else {
      ButtonVariant::Text
    },
  )
  .icon_size(IconSize::Sm)
  .label_size(12.0)
  .padding(if circular { 12 } else { 6 })
  .min_height(if circular { 48.0 } else { 40.0 })
  .id(iced::widget::Id::from(control_id))
  .on_press_maybe(action);
  if circular {
    control = control
      .width(Length::Fixed(48.0))
      .icon_size(IconSize::Custom(18.0))
      .content_centered(true)
      .style(library::card_play);
  }
  focus_tooltip(control, tooltip, TooltipOptions::default())
}

pub(crate) fn resolved_progress(state: &State, item: &VideoLibraryItem) -> Option<f32> {
  match list_playback::target(state, item) {
    Target::Ready(target) => item_progress(target),
    _ => None,
  }
}

pub(crate) fn remaining_caption(state: &State, item: &VideoLibraryItem) -> Option<String> {
  let runtime = item
    .runtime_seconds
    .filter(|value| value.is_finite() && *value > 0.0)?;
  let position = item
    .resume_position_seconds
    .filter(|value| value.is_finite() && *value > 0.0)?;
  (!item.played && runtime > position).then(|| {
    state.format(
      "detail-remaining",
      &[(
        "duration",
        state.kernel.locale.duration(runtime - position).into(),
      )],
    )
  })
}

fn playback_caption(state: &State, item: &VideoLibraryItem) -> String {
  match list_playback::target(state, item) {
    Target::Ready(target) => {
      let time = remaining_caption(state, target).unwrap_or_else(|| {
        state.t(if target.played {
          "browse-played"
        } else {
          "list-playback-no-progress"
        })
      });
      match episode_card_code(target) {
        Some(code) => format!("{code} · {time}"),
        None => time,
      }
    }
    Target::Loading => state.t("common-loading"),
    Target::NoNextUp => state.t("list-playback-no-target"),
    Target::Failed(_) => state.t("list-playback-failed"),
    Target::Unavailable => state.t("list-playback-unavailable"),
  }
}

fn card_copy<'a>(state: &'a State, item: &'a VideoLibraryItem) -> Element<'a, Message> {
  let episode = is_episode_item(item);
  let title = if episode {
    item.series_name.as_deref().unwrap_or(&item.name)
  } else {
    &item.name
  };
  let mut copy = column![ellipsis_text(title)
    .font(HEADING_FONT)
    .size(14)
    .line_height(iced::Pixels(18.0))
    .color(state.palette().text.secondary)]
  .spacing(TOKENS.spacing.s1)
  .width(Fill);
  if episode {
    copy = copy.push(
      (row![
        text(
          episode_card_code(item)
            .unwrap_or_else(|| media_type(state.kernel.locale, &item.item_type))
        )
        .size(12)
        .line_height(iced::Pixels(16.0))
        .color(state.palette().text.metadata),
        ellipsis_text(&item.name)
          .size(12)
          .line_height(iced::Pixels(16.0))
          .color(state.palette().text.metadata),
      ]
      .spacing(TOKENS.spacing.s2)
      .width(Fill))
      .boxed(),
    );
    let activity = remaining_caption(state, item).unwrap_or_else(|| {
      item.last_played_date.as_deref().map_or_else(
        || state.t("list-playback-no-progress"),
        |stamp| crate::i18n::media::history_timestamp(state.kernel.locale, Some(stamp)),
      )
    });
    copy = copy.push(
      (text(activity)
        .size(12)
        .line_height(iced::Pixels(16.0))
        .color(state.palette().text.metadata))
      .boxed(),
    );
  } else {
    copy = copy.push(
      (ellipsis_text(item_caption(state.kernel.locale, item))
        .size(12)
        .line_height(iced::Pixels(16.0))
        .color(state.palette().text.metadata))
      .boxed(),
    );
  }
  container(copy)
    .padding(iced::Padding {
      top: TOKENS.spacing.s2,
      ..iced::Padding::ZERO
    })
    .height(CARD_COPY_HEIGHT)
    .width(Fill)
    .boxed()
}

fn video_card<'a>(
  state: &'a State,
  item: &'a VideoLibraryItem,
  cell_width: f32,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> Element<'a, Message> {
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
        card_copy(state, item),
      )
      .width(Fill)
      .boxed()
    },
    ButtonVariant::Text,
  )
  .padding(0)
  .width(Fill)
  .style(jellypilot_ui::widgets::button::media_artwork)
  .on_press(Message::OpenDetail(Box::new(item.clone())));
  let mut overlay = Column::<Element<'_, Message>>::new().push((space::vertical()).boxed());
  if let Some(rating) = item.community_rating.filter(|rating| rating.is_finite()) {
    overlay = overlay.push(
      (container(
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
      .style(library::rating))
      .boxed(),
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
    resolved_progress(state, item).filter(|progress| *progress > 0.0 && *progress < 100.0)
  {
    card = card.push(
      (container(ArtworkProgress::new(
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
        library::artwork_progress_style(&state.native_theme(), true),
      ))
      .width(Fill)
      .height(artwork_height)
      .align_y(Alignment::End))
      .boxed(),
    );
  }
  let actions = stack![
    container(play_button(
      state,
      item,
      true,
      format!("browse-card-play-{}", item.id)
    ))
    .center_x(Fill)
    .center_y(Fill),
    container(favorite_button(state, item))
      .padding(TOKENS.spacing.s2)
      .width(Fill)
      .height(Fill)
      .align_x(Alignment::End)
      .align_y(Alignment::Start),
  ]
  .width(Fill)
  .height(artwork_height);
  hover(card, actions).boxed()
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
  let enabled = state.kernel.client.is_some()
    && !state.shell.quit_requested
    && !accounts::content_mutations_blocked(&state.kernel)
    && !super::super::item_actions::busy(&state.kernel, &item.id);
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
      .boxed()
    },
    ButtonVariant::Icon,
  )
  .padding(12)
  .width(Length::Fixed(40.0))
  .min_height(40.0)
  .style(|theme, _, status| library::row(theme, status))
  .id(iced::widget::Id::from(format!(
    "browse-favorite-{}",
    item.id
  )))
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

fn row_progress<'a>(
  state: &'a State,
  item: &'a VideoLibraryItem,
  narrow: bool,
) -> Element<'a, Message> {
  let metadata = text(playback_caption(state, item))
    .size(12)
    .color(state.palette().text.metadata);
  let play = play_button(state, item, false, format!("browse-row-play-{}", item.id));
  if narrow {
    return row![metadata, space::horizontal(), play]
      .spacing(TOKENS.spacing.s2)
      .align_y(Alignment::Center)
      .width(Fill)
      .boxed();
  }
  let mut content = column![metadata].spacing(0).width(220);
  if let Some(progress) =
    resolved_progress(state, item).filter(|value| *value > 0.0 && *value < 100.0)
  {
    content = content.push(
      (progress_bar(0.0..=100.0, progress)
        .girth(4)
        .length(Fill)
        .style(library::progress))
      .boxed(),
    );
  }
  content.push((play).boxed()).boxed()
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
    space::horizontal().width(Length::Fixed(40.0)),
    label("browse-column-title", Fill),
    label("browse-column-year", 90.into()),
    label("browse-column-rating", 90.into()),
    label("browse-column-progress", 220.into()),
    space::horizontal().width(Length::Fixed(40.0)),
  ]
  .spacing(TOKENS.spacing.s4)
  .align_y(Alignment::Center);
  column![
    container(headings).padding([10, 12]).height(34).width(Fill),
    iced::widget::rule::horizontal(1).style(library::divider),
  ]
  .boxed()
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
    let mut metadata = Row::<Element<'_, Message>>::new()
      .spacing(TOKENS.spacing.s2)
      .align_y(Alignment::Center);
    if let Some(year) = item.production_year {
      metadata = metadata.push((text(year).size(12).color(palette.text.metadata)).boxed());
    }
    if let Some(rating) = rating {
      metadata = metadata.push(
        (text(format!("★ {rating:.1}"))
          .size(12)
          .color(palette.colors.warning))
        .boxed(),
      );
    }
    copy = copy.push((metadata).boxed());
  }
  let poster = container(item_artwork(
    state,
    item,
    60.0,
    true,
    state.shell.skeleton_phase,
    state.kernel.settings.snapshot().reduced_motion(),
  ))
  .width(Length::Fixed(40.0))
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
        (text(
          item
            .production_year
            .map_or_else(String::new, |year| year.to_string()),
        )
        .size(12)
        .color(palette.text.body)
        .width(90))
        .boxed(),
      )
      .push(
        (text(rating.map_or_else(String::new, |rating| format!("★ {rating:.1}")))
          .size(12)
          .color(palette.colors.warning)
          .width(90))
        .boxed(),
      );
  }
  main.boxed()
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
  .min_height(if width < LIST_COLUMNS_MIN_WIDTH {
    height - 41.0
  } else {
    height - 1.0
  })
  .width(Fill)
  .style(|theme, _, status| library::row(theme, status))
  .on_press(Message::OpenDetail(Box::new(item.clone())));
  let open = focus_tooltip(open, item.name.clone(), TooltipOptions::default());
  let narrow = width < LIST_COLUMNS_MIN_WIDTH;
  let mut main = row![open]
    .spacing(TOKENS.spacing.s4)
    .align_y(Alignment::Center);
  if !narrow {
    main = main.push((row_progress(state, item, false)).boxed());
  }
  main = main.push((favorite_button(state, item)).boxed());
  let mut content = Column::<Element<'_, Message>>::new().push((main).boxed());
  if narrow {
    content = content.push((row_progress(state, item, true)).boxed());
  }
  column![
    container(content).padding([0, 12]).width(Fill),
    iced::widget::rule::horizontal(1).style(library::divider)
  ]
  .boxed()
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
  .boxed()
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
        .boxed();
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
    .boxed();
  }

  skeleton_panel(
    Fill,
    height,
    palette.colors.surfaceContainerLowest,
    radius,
    phase,
    reduced_motion,
  )
  .boxed()
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
  .boxed()
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
    .boxed()
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
    .boxed()
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

  fn scroll_fixture(
    memory: &super::super::scroll_memory::ScrollMemory,
    id: Option<&'static str>,
  ) -> Element<'static, Message> {
    super::super::scroll_memory::remember(
      browse_scroll(
        space::vertical().height(1800).boxed(),
        container(space::horizontal())
          .width(Fill)
          .height(56)
          .boxed(),
        id.map(iced::widget::Id::new),
      ),
      memory,
    )
  }

  fn page_offset(
    ui: &mut iced_runtime::user_interface::UserInterface<'_, Message, iced::Theme, iced::Renderer>,
    renderer: &mut iced::Renderer,
    id: &'static str,
  ) -> f32 {
    struct Probe {
      id: iced::widget::Id,
      offset: Option<f32>,
    }
    impl iced::advanced::widget::Operation for Probe {
      fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation)) {
        visit(self);
      }
      fn scrollable(
        &mut self,
        id: Option<&iced::widget::Id>,
        _: iced::Rectangle,
        _: iced::Size,
        translation: iced::Vector,
        _: &mut dyn iced::advanced::widget::operation::Scrollable,
      ) {
        if id == Some(&self.id) {
          self.offset = Some(translation.y);
        }
      }
    }
    drop(crate::app::test_support::redraw_ui(ui, renderer));
    let mut probe = Probe {
      id: iced::widget::Id::new(id),
      offset: None,
    };
    ui.operate(renderer, &mut probe);
    probe.offset.expect("identified ready page")
  }

  fn set_page_offset(
    ui: &mut iced_runtime::user_interface::UserInterface<'_, Message, iced::Theme, iced::Renderer>,
    renderer: &iced::Renderer,
    id: &'static str,
    y: f32,
  ) {
    ui.operate(
      renderer,
      &mut iced::advanced::widget::operation::scrollable::scroll_to::<()>(
        iced::widget::Id::new(id),
        iced::advanced::widget::operation::scrollable::AbsoluteOffset {
          x: None,
          y: Some(y),
        },
        iced::advanced::widget::operation::Animation::Instant,
      ),
    );
  }

  #[tokio::test]
  async fn scrolling_sticky_loading_does_not_move_first_results_or_consume_restoration() {
    use iced_runtime::user_interface::{Cache, UserInterface};
    let mut renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .expect("software layout renderer");
    let memory = super::super::scroll_memory::ScrollMemory::default();
    let size = Size::new(320.0, 180.0);
    let mut ready = UserInterface::build(
      scroll_fixture(&memory, Some("results")),
      size,
      Cache::new(),
      &mut renderer,
    );
    set_page_offset(&mut ready, &renderer, "results", 450.0);
    let mut loading = UserInterface::build(
      scroll_fixture(&memory, None),
      size,
      ready.into_cache(),
      &mut renderer,
    );
    let mut messages = iced::advanced::shell::Bus::new();
    loading.update(
      &iced::window::Headless,
      &iced::advanced::shell::Waker::noop(),
      &[iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
        delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y: -90.0 },
      })],
      iced::mouse::Cursor::Available(iced::Point::new(200.0, 120.0)),
      &mut renderer,
      &mut messages,
    );
    assert!(
      messages.is_empty(),
      "skeletons must not update result viewport state"
    );
    let mut first_results = UserInterface::build(
      scroll_fixture(&memory, Some("new-query")),
      size,
      loading.into_cache(),
      &mut renderer,
    );
    assert_eq!(
      page_offset(&mut first_results, &mut renderer, "new-query"),
      0.0
    );
    let mut restored = UserInterface::build(
      scroll_fixture(&memory, Some("results")),
      size,
      first_results.into_cache(),
      &mut renderer,
    );
    assert_eq!(page_offset(&mut restored, &mut renderer, "results"), 450.0);
  }

  #[tokio::test]
  async fn result_identity_changes_preserve_toolbar_keyboard_activation() {
    use iced::advanced::widget::operation::focusable;
    use iced_runtime::user_interface::{Cache, UserInterface};
    let mut renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .expect("software layout renderer");
    let memory = super::super::scroll_memory::ScrollMemory::default();
    let page = |id: Option<&'static str>| {
      super::super::scroll_memory::remember(
        browse_scroll(
          space::vertical().height(1800).boxed(),
          container(
            control_button(None, Some("List".to_owned()), ButtonVariant::Text)
              .id("presentation-list")
              .on_press(Message::Browse(BrowseMessage::ViewModeSelected(
                ViewMode::List,
              ))),
          )
          .width(Fill)
          .height(56)
          .boxed(),
          id.map(iced::widget::Id::new),
        ),
        &memory,
      )
    };
    let size = Size::new(320.0, 180.0);
    let mut ui = UserInterface::build(page(Some("grid")), size, Cache::new(), &mut renderer);
    ui.operate(
      &renderer,
      &mut focusable::focus::<()>(iced::widget::Id::new("presentation-list")),
    );
    for id in [None, Some("list"), Some("grid")] {
      ui = UserInterface::build(page(id), size, ui.into_cache(), &mut renderer);
      let mut messages = iced::advanced::shell::Bus::new();
      ui.update(
        &iced::window::Headless,
        &iced::advanced::shell::Waker::noop(),
        &[Event::Keyboard(iced::keyboard::Event::KeyPressed {
          key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
          modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
          physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Enter),
          location: iced::keyboard::Location::Standard,
          modifiers: iced::keyboard::Modifiers::NONE,
          text: None,
          repeat: false,
        })],
        mouse::Cursor::Unavailable,
        &mut renderer,
        &mut messages,
      );
      let produced = messages
        .drain()
        .map(|(message, _)| message)
        .collect::<Vec<_>>();
      assert!(
        matches!(
          produced.as_slice(),
          [Message::Browse(BrowseMessage::ViewModeSelected(
            ViewMode::List
          ))]
        ),
        "the persistent toolbar focus must remain keyboard operable"
      );
    }
  }

  #[tokio::test]
  async fn loading_touch_gesture_cannot_scroll_the_settled_result_surface() {
    use iced_runtime::user_interface::{Cache, UserInterface};
    let mut renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .expect("software layout renderer");
    let memory = super::super::scroll_memory::ScrollMemory::default();
    let size = Size::new(320.0, 180.0);
    let mut loading = UserInterface::build(
      scroll_fixture(&memory, None),
      size,
      Cache::new(),
      &mut renderer,
    );
    let mut messages = iced::advanced::shell::Bus::new();
    let mut send = |ui: &mut UserInterface<'_, Message, iced::Theme, iced::Renderer>,
                    renderer: &mut iced::Renderer,
                    phase,
                    position| {
      let id = iced::touch::Finger(7);
      let event = match phase {
        0 => iced::touch::Event::FingerPressed { id, position },
        1 => iced::touch::Event::FingerMoved { id, position },
        _ => iced::touch::Event::FingerLost { id, position },
      };
      ui.update(
        &iced::window::Headless,
        &iced::advanced::shell::Waker::noop(),
        &[iced::Event::Touch(event)],
        iced::mouse::Cursor::Available(position),
        renderer,
        &mut messages,
      );
    };
    send(
      &mut loading,
      &mut renderer,
      0,
      iced::Point::new(200.0, 140.0),
    );
    send(
      &mut loading,
      &mut renderer,
      1,
      iced::Point::new(200.0, 100.0),
    );
    let mut ready = UserInterface::build(
      scroll_fixture(&memory, Some("results")),
      size,
      loading.into_cache(),
      &mut renderer,
    );
    send(&mut ready, &mut renderer, 1, iced::Point::new(200.0, 60.0));
    send(&mut ready, &mut renderer, 2, iced::Point::new(200.0, 60.0));
    assert_eq!(page_offset(&mut ready, &mut renderer, "results"), 0.0);
    assert!(
      messages.is_empty(),
      "departed loading gestures must not publish result scrolls"
    );
  }

  #[tokio::test]
  async fn sticky_sparse_grid_measures_its_reserved_origin_and_full_scroll_viewport() {
    use iced_runtime::user_interface::{Cache, UserInterface};
    let mut renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .expect("software layout renderer");
    let metrics = ArtworkGridMetrics {
      columns: 2,
      cell_width: 150.0,
      cell_height: 150.0,
      row_height: 170.0,
    };
    let grid = artwork_grid(
      100,
      metrics,
      jellypilot_ui::widgets::artwork_grid::ArtworkGridViewport {
        offset_y: 0.0,
        height: 180.0,
      },
      20.0,
      move |_| space::horizontal().width(150).height(150).boxed(),
    );
    let mut ui = UserInterface::build(
      super::super::image_observer::observe_images(browse_scroll(
        iced::widget::column![space::vertical().height(40), observe_grid_viewport(grid, 7)].boxed(),
        container(space::horizontal())
          .width(Fill)
          .height(56)
          .boxed(),
        Some(iced::widget::Id::new("grid")),
      )),
      Size::new(320.0, 180.0),
      Cache::new(),
      &mut renderer,
    );
    for (scroll, expected) in [(0.0, -96.0), (600.0, 504.0)] {
      set_page_offset(&mut ui, &renderer, "grid", scroll);
      let mut messages = iced::advanced::shell::Bus::new();
      ui.update(
        &iced::window::Headless,
        &iced::advanced::shell::Waker::noop(),
        &[iced::Event::Window(iced::window::Event::RedrawRequested(
          iced::time::Instant::now(),
        ))],
        iced::mouse::Cursor::Unavailable,
        &mut renderer,
        &mut messages,
      );
      let geometry = messages.drain().find_map(|(message, _)| match message {
        Message::Browse(BrowseMessage::GridViewportMeasured {
          epoch: 7,
          offset_y,
          height,
        }) => Some((offset_y, height)),
        _ => None,
      });
      assert_eq!(geometry, Some((expected, 180.0)));
    }
  }

  #[tokio::test]
  async fn scrolling_sparse_artwork_settles_after_measurement_and_image_admission() {
    use iced_runtime::user_interface::{Cache, Change, UserInterface};
    use jellypilot_ui::widgets::artwork_grid::ArtworkGridViewport;

    fn page(viewport: ArtworkGridViewport) -> Element<'static, Message> {
      let grid = artwork_grid(
        100,
        ArtworkGridMetrics {
          columns: 2,
          cell_width: 150.0,
          cell_height: 150.0,
          row_height: 170.0,
        },
        viewport,
        20.0,
        |index| {
          observe_image(
            space::horizontal().width(150).height(150).boxed(),
            ArtworkSurface::Browse,
            7,
            ImageSpec {
              key: format!("poster-{index}"),
              image_id: index.to_string(),
              size_class: ArtworkSizeClass::Card,
              derived: DerivedArtwork::default(),
            },
            ImageAxis::Vertical,
          )
        },
      );
      super::super::image_observer::observe_images(browse_scroll(
        iced::widget::column![space::vertical().height(40), observe_grid_viewport(grid, 7)].boxed(),
        container(space::horizontal())
          .width(Fill)
          .height(56)
          .boxed(),
        Some(iced::widget::Id::new("grid")),
      ))
    }

    let mut renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .expect("software layout renderer");
    let size = Size::new(320.0, 180.0);
    let mut viewport = ArtworkGridViewport {
      offset_y: 0.0,
      height: size.height,
    };
    let mut ui = UserInterface::build(page(viewport), size, Cache::new(), &mut renderer);
    let redraw = Event::Window(iced::window::Event::RedrawRequested(
      iced::time::Instant::now(),
    ));
    let mut rounds = Vec::new();
    for phase in ["initial", "wheel"] {
      if phase == "wheel" {
        let mut bus = iced::advanced::shell::Bus::new();
        ui.update(
          &iced::window::Headless,
          &iced::advanced::shell::Waker::noop(),
          &[Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -600.0 },
          })],
          mouse::Cursor::Available(iced::Point::new(200.0, 120.0)),
          &mut renderer,
          &mut bus,
        );
        assert!(bus.drain().any(|(message, _)| matches!(message,
          Message::Browse(BrowseMessage::Scrolled(viewport))
            if viewport.absolute_offset().y == 600.0
        )));
      }
      rounds.clear();
      for _ in 0..3 {
        let mut bus = iced::advanced::shell::Bus::new();
        let (state, _) = ui.update(
          &iced::window::Headless,
          &iced::advanced::shell::Waker::noop(),
          std::slice::from_ref(&redraw),
          mouse::Cursor::Unavailable,
          &mut renderer,
          &mut bus,
        );
        let mut images = 0;
        let mut measured = false;
        let mut scrolls = 0;
        for (message, _) in bus.drain() {
          match message {
            Message::ImageObserved { .. } => images += 1,
            Message::Browse(BrowseMessage::GridViewportMeasured {
              epoch: 7,
              offset_y,
              height,
            }) => {
              viewport = ArtworkGridViewport { offset_y, height };
              measured = true;
            }
            Message::Browse(BrowseMessage::Scrolled(_)) => scrolls += 1,
            other => panic!("unexpected redraw message: {other:?}"),
          }
        }
        let stable = matches!(
          state,
          iced_runtime::user_interface::State::Updated {
            change: Change::None,
            ..
          }
        );
        rounds.push((images, measured, scrolls, stable));
        if images == 0 && !measured && scrolls == 0 && stable {
          break;
        }
        // Same sequence as winit: process geometry/identity reports and rebuild
        // with the real cache before retrying the same presentation timestamp.
        ui = UserInterface::build(page(viewport), size, ui.into_cache(), &mut renderer);
      }
      assert_eq!(
        rounds.last(),
        Some(&(0, false, 0, true)),
        "{phase}: {rounds:?}"
      );
    }
    assert!(rounds[0].1, "wheel changes measured grid geometry");
    assert!(rounds[1].0 > 0, "new materialized images publish demand");
    assert_eq!(viewport.offset_y, 504.0);
  }

  #[tokio::test]
  async fn sticky_grid_and_list_restore_their_own_scroll_positions() {
    use iced_runtime::user_interface::{Cache, UserInterface};
    let mut renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .expect("software layout renderer");
    let memory = super::super::scroll_memory::ScrollMemory::default();
    let size = Size::new(320.0, 180.0);
    let mut grid = UserInterface::build(
      scroll_fixture(&memory, Some("grid")),
      size,
      Cache::new(),
      &mut renderer,
    );
    set_page_offset(&mut grid, &renderer, "grid", 600.0);
    let mut list = UserInterface::build(
      scroll_fixture(&memory, Some("list")),
      size,
      grid.into_cache(),
      &mut renderer,
    );
    assert_eq!(page_offset(&mut list, &mut renderer, "list"), 0.0);
    set_page_offset(&mut list, &renderer, "list", 810.0);
    let mut grid = UserInterface::build(
      scroll_fixture(&memory, Some("grid")),
      size,
      list.into_cache(),
      &mut renderer,
    );
    assert_eq!(page_offset(&mut grid, &mut renderer, "grid"), 600.0);
    let mut list = UserInterface::build(
      scroll_fixture(&memory, Some("list")),
      size,
      grid.into_cache(),
      &mut renderer,
    );
    assert_eq!(page_offset(&mut list, &mut renderer, "list"), 810.0);
  }

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
  async fn artwork_play_and_replay_glyphs_are_centered_in_their_targets() {
    fn glyph_bounds(layout: iced::advanced::Layout, tree: &Tree) -> Option<iced::Rectangle> {
      if tree.children.is_empty() {
        let bounds = layout.bounds();
        return (bounds.width > 0.0 && bounds.height > 0.0).then_some(bounds);
      }
      layout
        .iter(&tree.children)
        .find_map(|(layout, tree)| glyph_bounds(layout, tree))
    }

    let renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .expect("software layout renderer");
    let mut state = crate::app::update::tests::test_state();
    let mut item = video_item("play-target");
    for enabled in [false, true] {
      state.playback.view.engine_available = enabled;
      for played in [false, true] {
        item.played = played;
        let mut control = play_button(&state, &item, true, "artwork-play".to_owned());
        let mut tree = Tree::new(&control);
        tree.diff(&mut control);
        let node = crate::app::test_support::layout_widget(
          &mut control,
          &mut tree,
          &renderer,
          &layout::Limits::new(Size::ZERO, Size::new(175.0, 262.0)),
        );
        let glyph = glyph_bounds(node.layout(), &tree).expect("play/replay glyph");
        assert_eq!(
          glyph.center(),
          node.bounds().center(),
          "artwork action must center its glyph with enabled={enabled}, played={played}"
        );
        assert_eq!(node.size(), Size::new(48.0, 48.0));
        assert_eq!(glyph.size(), Size::new(18.0, 18.0));
      }
    }
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
        tree.diff(&mut cell);
        let node = crate::app::test_support::layout_widget(
          &mut cell,
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
        tree.diff(&mut placeholder);
        let loading = crate::app::test_support::layout_widget(
          &mut placeholder,
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

  fn assert_fits_width(node: &crate::app::test_support::LayoutGeometry) {
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
