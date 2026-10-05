use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{
  button, column, container, image, mouse_area, row, scrollable, space, stack, text, Column, Row,
};
use iced::{Alignment, Background, ContentFit, Element, Fill, Size};
use jellypilot_core::browse_model::LibraryBrowseView;
use jellypilot_core::cards::{card_title, hero_headline};
use jellypilot_core::detail::DetailContent;
use jellypilot_core::LoadState;
use jellypilot_media_server::artwork::{ArtworkSizeClass, DerivedArtwork};
use jellypilot_media_server::VideoLibraryItem;
use jellypilot_ui::fonts::{DISPLAY_FONT, HEADING_FONT};
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::tv as style;
use jellypilot_ui::widgets::artwork_grid::{artwork_grid_with_overflow, ArtworkGridViewport};
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::inert::inert;
use jellypilot_ui::widgets::{motion, tv_focus};

use super::{detail as detail_state, navigation};
use super::{AppMessage, Focus, Message, State};
use crate::app::artwork::{ArtworkSurface, ImageCollection, ImageSpec};
use crate::app::home::ArtworkPlacement;
use crate::app::state::Destination;
use crate::app::view::image_observer::{
  observe_grid_viewport, observe_image, observe_images, ImageAxis,
};
use crate::i18n::media::{card_subtitle, detail_metadata, hero_metadata, show_detail_metadata};

pub fn view(state: &State) -> Element<'_, AppMessage> {
  let base = if !state.tv.settings.open && super::player::active(state) {
    super::player::view(state)
  } else {
    browse_view(state)
  };
  let list_overlay = (!super::filters::is_open(state)
    && !super::saved_browse::overlay_open(state)
    && !state.tv.settings.open
    && !state.tv.search.open
    && !super::player::active(state)
    && !super::account::modal_open(state))
  .then(|| super::lists::overlay(state))
  .flatten();
  let obscured = state.tv.search.open
    || super::saved_browse::overlay_open(state)
    || super::lists::menu_open(state)
    || super::filters::is_open(state)
    || super::player::upcoming::is_open(state);
  let mut layers = vec![if obscured { inert(base) } else { base }];
  if let Some(overlay) = list_overlay {
    layers.push(overlay);
  }
  if state.tv.search.open {
    layers.push(super::search::view(state));
  }
  if super::filters::is_open(state) {
    layers.push(super::filters::view(state));
  }
  if super::player::upcoming::is_open(state) {
    layers.push(super::player::upcoming::view(state));
  }
  if let Some(overlay) = super::saved_browse::overlay(state) {
    layers.push(overlay);
  }
  iced::widget::Stack::with_children(layers)
    .width(Fill)
    .height(Fill)
    .into()
}

fn browse_view(state: &State) -> Element<'_, AppMessage> {
  let scale = style::scale(state.shell.window_size.width);
  let width = content_width(state, scale);
  let page_footer = state.tv.settings.open
    || matches!(
      state.shell.destination,
      Destination::PersonalLists(_) | Destination::SavedBrowse
    );
  let height = navigation::viewport_height(state) + if page_footer { 80.0 * scale } else { 0.0 };
  // The viewport extends into the safe gutter. Content padding cancels its
  // translation, preserving the original slots while focused posters can grow.
  let page: Element<'_, AppMessage> = if state.tv.settings.open {
    container(super::settings::view(state))
      .width(width)
      .height(height)
      .into()
  } else if matches!(state.shell.destination, Destination::SavedBrowse) {
    container(super::saved_browse::view(state))
      .width(width)
      .height(height)
      .into()
  } else {
    tv_focus::focus(false, move |_| {
      let page = match state.shell.destination {
        Destination::Home => home(state, scale),
        Destination::Library { .. } | Destination::Search(_) => library(state, scale),
        Destination::Detail(_) => detail(state, scale),
        Destination::PersonalLists(_) => super::lists::view(state, width),
        _ => home(state, scale),
      };
      scrollable(container(page).padding(iced::Padding {
        top: style::POSTER_CLEARANCE * scale,
        left: 8.0 * scale,
        right: 8.0 * scale,
        bottom: 0.0,
      }))
      .id(navigation::scroll_id())
      .on_scroll(|viewport| AppMessage::Tv(Message::Scrolled(viewport.absolute_offset().y)))
      .width(width + 16.0 * scale)
      .height(height + style::POSTER_CLEARANCE * scale)
      .into()
    })
    .frame(Size::new(width, height), Alignment::Center, Alignment::End)
    .into()
  };
  let mut content = column![page].spacing(16.0 * scale).width(Fill).height(Fill);
  if !page_footer {
    content = content.push(
      container(
        text(context_hint(state))
          .size(style::META * scale)
          .line_height(iced::Pixels(28.0 * scale))
          .color(style::PALETTE.text.metadata),
      )
      .height(64.0 * scale)
      .align_y(Alignment::Center),
    );
  }
  let content = container(content)
    .padding(iced::Padding {
      top: style::SAFE_Y * scale,
      right: style::SAFE_X * scale,
      bottom: style::SAFE_Y * scale,
      left: style::CONTENT_INSET * scale,
    })
    .width(Fill)
    .height(Fill);
  observe_images(
    container(row![rail(state, scale), content].height(Fill).width(Fill))
      .style(style::canvas)
      .width(Fill)
      .height(Fill)
      .into(),
  )
}

pub(super) fn context_hint(state: &State) -> String {
  if !super::browse_focus_visible(state) {
    return String::new();
  }
  if matches!(
    state.tv.focus,
    Focus::DetailWatchlist | Focus::DetailFavorite
  ) && !navigation::action_enabled(state, state.tv.focus)
  {
    return state.t("tv-action-pending");
  }
  let key = match state.tv.focus {
    Focus::Header(_) if !matches!(state.shell.destination, Destination::Library { .. }) => {
      return String::new()
    }
    Focus::Grid(index) if super::navigation::grid_item(state, index).is_none() => {
      if state.full.as_ref().is_some_and(|full| {
        matches!(
          full.browse.view,
          LibraryBrowseView::Ready {
            load_more_failure: Some(_),
            ..
          }
        )
      }) {
        "tv-hint-retry"
      } else {
        return String::new();
      }
    }
    Focus::Grid(_) | Focus::Shelf { .. } => "tv-hint-media",
    Focus::Episode(_) | Focus::HeroPlay | Focus::DetailPlay => "tv-hint-play",
    Focus::HeroDetail => "tv-hint-detail",
    Focus::DetailBack => "tv-hint-back",
    Focus::HeroWatchlist | Focus::DetailWatchlist | Focus::DetailFavorite => "tv-hint-toggle",
    Focus::DetailMenu => "tv-hint-menu",
    Focus::DetailOverview if detail_state::overview_expanded(state) => "tv-hint-overview-read",
    Focus::DetailOverview => "tv-hint-overview",
    Focus::DetailEpisodesMore if !navigation::action_enabled(state, state.tv.focus) => "tv-loading",
    Focus::DetailEpisodesMore
      if matches!(detail_state::append(state), Some(LoadState::Failed(_))) =>
    {
      "tv-hint-retry"
    }
    Focus::DetailEpisodesMore => "tv-hint-load-more",
    Focus::DetailEpisodesRetry => "tv-hint-retry",
    Focus::Header(2) => "tv-hint-filter",
    Focus::Header(3) => "tv-hint-sort",
    Focus::Header(_) | Focus::Season(_) => "tv-hint-select",
    Focus::Retry => "tv-hint-retry",
    Focus::Rail(_) => "tv-hint-open",
    Focus::Lists(super::lists::Focus::Card(_)) if super::lists::focused_item(state).is_none() => {
      "tv-hint-manage-list"
    }
    Focus::Lists(super::lists::Focus::Card(_)) => "tv-hint-list-media",
    Focus::Lists(super::lists::Focus::Undo(_)) => "tv-hint-undo",
    Focus::Lists(super::lists::Focus::Added) => "tv-hint-view-list",
    Focus::Lists(super::lists::Focus::Empty) => "tv-hint-browse",
    Focus::Lists(super::lists::Focus::Retry) => "tv-hint-retry",
    Focus::Lists(super::lists::Focus::Tab(_)) => "tv-hint-select",
    Focus::SavedBrowse(super::saved_browse::Focus::Back) => "tv-hint-back",
    Focus::SavedBrowse(super::saved_browse::Focus::Reload) => "tv-hint-retry",
    Focus::SavedBrowse(super::saved_browse::Focus::Record(_, _))
      if state.saved_browse.pending.is_some() =>
    {
      "tv-action-pending"
    }
    Focus::SavedBrowse(super::saved_browse::Focus::Record(_, _)) => "tv-hint-select",
    Focus::SavedBrowse(
      super::saved_browse::Focus::Identity(_) | super::saved_browse::Focus::Summary(_, _),
    ) => return String::new(),
  };
  state.t(key)
}

pub(super) fn content_width(state: &State, scale: f32) -> f32 {
  (state.shell.window_size.width - (style::RAIL + style::CONTENT_INSET + style::SAFE_X) * scale)
    .max(1.0)
}

/// Leaves each card in its original slot while the scroll clip includes focus growth.
pub(super) fn horizontal_shelf<'a>(
  state: &'a State,
  id: iced::widget::Id,
  height: f32,
  build: impl Fn() -> Element<'a, AppMessage> + 'a,
  on_scroll: impl Fn(f32) -> AppMessage + Clone + 'a,
) -> Element<'a, AppMessage> {
  let scale = style::scale(state.shell.window_size.width);
  let width = content_width(state, scale);
  tv_focus::focus(false, move |_| {
    let on_scroll = on_scroll.clone();
    scrollable(container(build()).padding(iced::Padding {
      top: style::POSTER_CLEARANCE * scale,
      left: 8.0 * scale,
      right: 8.0 * scale,
      bottom: 0.0,
    }))
    .direction(Direction::Horizontal(Scrollbar::new()))
    .id(id.clone())
    .on_scroll(move |viewport| on_scroll(viewport.absolute_offset().x))
    .width(width + 16.0 * scale)
    .height(height + style::POSTER_CLEARANCE * scale)
    .into()
  })
  .frame(Size::new(width, height), Alignment::Center, Alignment::End)
  .into()
}

fn rail(state: &State, scale: f32) -> Element<'_, AppMessage> {
  use super::navigation::RailAction;
  let mut items = Column::new().spacing(16.0 * scale);
  for (index, destination) in navigation::rail_actions(state).into_iter().enumerate() {
    let (label, icon, selected) = match destination {
      RailAction::Search => (state.t("tv-search"), Icon::Search, false),
      RailAction::Home => (
        state.t("tv-home"),
        Icon::Home,
        !state.tv.settings.open && matches!(state.shell.destination, Destination::Home),
      ),
      RailAction::Lists => (
        state.t("tv-lists"),
        Icon::Bookmark,
        !state.tv.settings.open && matches!(state.shell.destination, Destination::PersonalLists(_)),
      ),
      RailAction::Upcoming => (state.t("viewing-queue-title"), Icon::Playlist, false),
      RailAction::SavedBrowse => (
        state.t("saved-filters-title"),
        Icon::Filter,
        matches!(state.shell.destination, Destination::SavedBrowse),
      ),
      RailAction::Library(index) => {
        let Some(shortcut) = navigation::shortcuts(state).get(index) else {
          continue;
        };
        let selected = !state.tv.settings.open
          && matches!(&state.shell.destination, Destination::Library { library_id, .. } if *library_id == shortcut.id);
        (shortcut.name.clone(), Icon::Movie, selected)
      }
      RailAction::Settings => (
        state.t("tv-settings"),
        Icon::Settings,
        state.tv.settings.open,
      ),
      RailAction::Account => (state.t("tv-account"), Icon::User, false),
      RailAction::Exit => (state.t("tv-exit"), Icon::ChevronLeft, false),
    };
    items = items.push(
      action(
        state,
        Focus::Rail(index),
        label,
        Some(icon),
        selected,
        scale,
      )
      .width(184.0 * scale),
    );
  }
  container(
    scrollable(items)
      .id(iced::widget::Id::new("tv-rail"))
      .height(Fill),
  )
  .padding(iced::Padding {
    top: style::SAFE_Y * scale,
    bottom: style::SAFE_Y * scale,
    left: style::SAFE_X * scale,
    right: 8.0 * scale,
  })
  .width(style::RAIL * scale)
  .height(Fill)
  .style(style::rail)
  .into()
}

pub(super) fn action<'a>(
  state: &State,
  focus: Focus,
  label: String,
  icon: Option<Icon>,
  selected: bool,
  scale: f32,
) -> iced::widget::Container<'a, AppMessage> {
  let focused = state.tv.focus == focus && super::browse_focus_visible(state);
  let enabled = navigation::action_enabled(state, focus);
  let width = match focus {
    Focus::Rail(_) => iced::Length::Fixed(184.0 * scale),
    Focus::Season(_) => iced::Length::Fixed(196.0 * scale),
    _ => iced::Length::Fit,
  };
  container(
    mouse_area(tv_focus::focus(focused, move |progress| {
      let color = if enabled || focused {
        style::foreground(style::PALETTE, progress, selected)
      } else {
        style::PALETTE.text.muted
      };
      let mut content = Row::new().spacing(12.0 * scale).align_y(Alignment::Center);
      if let Some(icon) = icon {
        content = content.push(icon_with_color(icon, IconSize::Custom(28.0 * scale), color));
      }
      let label = container(
        ellipsis_text(label.clone())
          .size(style::META * scale)
          .line_height(iced::Pixels(28.0 * scale))
          .font(HEADING_FONT)
          .color(color),
      );
      content = content.push(if matches!(focus, Focus::Rail(_) | Focus::Season(_)) {
        label.width(Fill)
      } else {
        label
      });
      // The check remains visible when focus temporarily replaces the selected fill.
      if selected {
        content = content.push(icon_with_color(
          Icon::Check,
          IconSize::Custom(if matches!(focus, Focus::Rail(_)) {
            16.0 * scale
          } else {
            22.0 * scale
          }),
          color,
        ));
      }
      button(
        container(content)
          .height(style::CONTROL * scale)
          .align_y(Alignment::Center)
          .padding([8.0 * scale, 16.0 * scale]),
      )
      .padding(0)
      .width(width)
      .height(style::CONTROL * scale)
      .style(style::button_progress(style::PALETTE, progress, selected))
      .on_press_maybe(enabled.then_some(AppMessage::Tv(Message::Activate(focus))))
      .into()
    }))
    .on_enter(AppMessage::Tv(Message::Focus(focus))),
  )
  .id(navigation::focus_id(focus))
}

pub(super) fn clock(scale: f32) -> Element<'static, AppMessage> {
  container(
    text(chrono::Local::now().format("%H:%M").to_string())
      .size(style::META * scale)
      .line_height(iced::Pixels(28.0 * scale))
      .color(style::PALETTE.text.metadata),
  )
  .width(112.0 * scale)
  .align_x(Alignment::End)
  .into()
}

pub(super) fn page_title(title: String, scale: f32) -> Element<'static, AppMessage> {
  row![
    container(
      ellipsis_text(title)
        .size(style::TITLE * scale)
        .line_height(iced::Pixels(48.0 * scale))
        .font(DISPLAY_FONT)
        .color(style::PALETTE.text.heading)
    )
    .width(Fill),
    clock(scale)
  ]
  .spacing(32.0 * scale)
  .align_y(Alignment::Center)
  .width(Fill)
  .into()
}

fn heading<'a>(value: String, scale: f32) -> iced::widget::Text<'a> {
  text(value)
    .size(style::TITLE * scale)
    .line_height(iced::Pixels(48.0 * scale))
    .font(DISPLAY_FONT)
    .color(style::PALETTE.text.heading)
}

fn status<'a>(state: &State, message: String, scale: f32) -> Element<'a, AppMessage> {
  column![
    text(message)
      .size(style::BODY * scale)
      .line_height(iced::Pixels(32.0 * scale))
      .color(style::PALETTE.text.body),
    action(
      state,
      Focus::Retry,
      state.t("home-retry"),
      None,
      false,
      scale
    )
  ]
  .spacing(24.0 * scale)
  .padding([36.0 * scale, 0.0])
  .into()
}

fn art<'a>(
  collection: &'a ImageCollection,
  surface: ArtworkSurface,
  spec: Option<ImageSpec>,
  width: f32,
  height: f32,
  backdrop: bool,
  axis: ImageAxis,
) -> Element<'a, AppMessage> {
  let handle = spec
    .as_ref()
    .and_then(|spec| collection.get(&spec.key))
    .and_then(|cell| cell.handle());
  let image: Element<'_, AppMessage> = match handle {
    Some(handle) => image(handle.clone())
      .width(width)
      .height(height)
      .content_fit(ContentFit::Cover)
      .border_radius(if backdrop {
        0.0
      } else {
        jellypilot_ui::tokens::TOKENS.radii.xl
      })
      .into(),
    None => container(space())
      .width(width)
      .height(height)
      .style(style::panel)
      .into(),
  };
  let image = if backdrop {
    backdrop_scrims(image)
  } else {
    image
  };
  match spec {
    Some(spec) => observe_image(image, surface, collection.epoch(), spec, axis),
    None => image,
  }
}

fn backdrop_scrims(image: Element<'_, AppMessage>) -> Element<'_, AppMessage> {
  let fade = iced::gradient::Linear::new(iced::Degrees(90.0))
    .add_stop(0.0, style::PALETTE.colors.background)
    .add_stop(0.65, style::PALETTE.colors.background.scale_alpha(0.84))
    .add_stop(1.0, style::PALETTE.colors.background.scale_alpha(0.3));
  let bottom = iced::gradient::Linear::new(iced::Degrees(180.0))
    .add_stop(0.0, style::PALETTE.colors.background.scale_alpha(0.0))
    .add_stop(0.55, style::PALETTE.colors.background.scale_alpha(0.0))
    .add_stop(1.0, style::PALETTE.colors.background);
  // Stack may grow the image to a parent's minimum size. Cover its resolved
  // bounds instead of keeping the artwork request's original fixed size.
  stack![
    image,
    container(space())
      .width(Fill)
      .height(Fill)
      .style(move |_| iced::widget::container::Style {
        background: Some(Background::Gradient(fade.into())),
        ..Default::default()
      }),
    container(space())
      .width(Fill)
      .height(Fill)
      .style(move |_| iced::widget::container::Style {
        background: Some(Background::Gradient(bottom.into())),
        ..Default::default()
      })
  ]
  .into()
}

pub(super) fn media_card<'a>(
  state: &'a State,
  item: &'a VideoLibraryItem,
  focus: Focus,
  artwork: (&'a ImageCollection, ArtworkSurface),
  spec: Option<ImageSpec>,
  geometry: (f32, bool),
  scale: f32,
) -> Element<'a, AppMessage> {
  let (collection, surface) = artwork;
  let (width, landscape) = geometry;
  let image_height = if landscape {
    width * 9.0 / 16.0
  } else {
    width * 1.5
  };
  let focused = state.tv.focus == focus && super::browse_focus_visible(state);
  let axis = if matches!(
    focus,
    Focus::Shelf { .. } | Focus::Episode(_) | Focus::Lists(_)
  ) {
    ImageAxis::Horizontal
  } else {
    ImageAxis::Vertical
  };
  let card = tv_focus::focus(focused, move |progress| {
    let factor = if motion::enabled() {
      1.0 + progress * (style::POSTER_SCALE - 1.0)
    } else {
      1.0
    };
    let expanded_width = width * factor;
    let expanded_height = image_height * factor;
    let edge = style::FOCUS_WIDTH * scale;
    let image = art(
      collection,
      surface,
      spec.clone(),
      expanded_width - edge * 2.0,
      expanded_height - edge * 2.0,
      false,
      axis,
    );
    button(
      container(image)
        .width(expanded_width)
        .height(expanded_height)
        .center_x(Fill)
        .center_y(Fill),
    )
    .padding(0)
    .width(expanded_width)
    .height(expanded_height)
    .style(style::poster(progress))
    .on_press(AppMessage::Tv(Message::Activate(focus)))
    .into()
  })
  .frame(
    Size::new(width, image_height),
    Alignment::Center,
    Alignment::End,
  );
  let content = column![
    card,
    container(
      ellipsis_text(card_title(item))
        .size(style::BODY * scale)
        .line_height(iced::Pixels(32.0 * scale))
        .font(HEADING_FONT)
        .color(style::PALETTE.text.heading)
    )
    .width(width)
    .height(36.0 * scale),
    container(
      ellipsis_text(card_subtitle(state.kernel.locale, item))
        .size(style::META * scale)
        .line_height(iced::Pixels(28.0 * scale))
        .color(style::PALETTE.text.metadata)
    )
    .width(width)
    .height(56.0 * scale),
  ]
  .spacing(8.0 * scale)
  .width(width);
  mouse_area(container(content).id(navigation::focus_id(focus)))
    .on_enter(AppMessage::Tv(Message::Focus(focus)))
    .into()
}

fn home(state: &State, scale: f32) -> Element<'_, AppMessage> {
  let Some(full) = state.full.as_ref() else {
    return status(state, state.t("tv-loading"), scale);
  };
  let mut content = Column::new().width(Fill);
  if let Some(item) = full.home.data.featured_item() {
    let hero_height = 540.0 * scale;
    let width = (state.shell.window_size.width
      - (style::RAIL + style::CONTENT_INSET + style::SAFE_X) * scale)
      .max(1.0);
    let hero = art(
      &full.home.artwork,
      ArtworkSurface::Home,
      ArtworkPlacement::HeroBackdrop.spec(item),
      width,
      hero_height,
      true,
      ImageAxis::Vertical,
    );
    let collections =
      crate::app::collections::controls(state, crate::app::collections::Source::Hero);
    let copy = column![
      container(clock(scale))
        .width(Fill)
        .height(80.0 * scale)
        .align_x(Alignment::End),
      container(
        ellipsis_text(hero_headline(item))
          .size(style::TITLE * scale)
          .line_height(iced::Pixels(48.0 * scale))
          .font(DISPLAY_FONT)
          .color(style::PALETTE.text.heading)
      )
      .width(Fill)
      .height(48.0 * scale),
      text(hero_metadata(state.kernel.locale, item))
        .size(style::META * scale)
        .line_height(iced::Pixels(28.0 * scale))
        .color(style::PALETTE.text.body),
      container(
        text(item.overview.as_deref().unwrap_or_default())
          .size(style::BODY * scale)
          .line_height(iced::Pixels(32.0 * scale))
          .color(style::PALETTE.text.body)
      )
      .width(850.0 * scale)
      .height(96.0 * scale)
      .clip(true),
      row![
        action(
          state,
          Focus::HeroPlay,
          state.t(if jellypilot_core::home_hero::has_resume_position(item) {
            "home-resume"
          } else {
            "home-play"
          }),
          Some(Icon::Play),
          false,
          scale
        ),
        action(
          state,
          Focus::HeroDetail,
          state.t("home-details"),
          None,
          false,
          scale
        ),
        action(
          state,
          Focus::HeroWatchlist,
          collections.watchlist_label,
          Some(if collections.watchlisted == Some(true) {
            Icon::BookmarkFilled
          } else {
            Icon::Bookmark
          }),
          collections.watchlisted == Some(true),
          scale
        )
      ]
      .spacing(24.0 * scale),
    ]
    .spacing(20.0 * scale);
    content = content.push(stack![hero, copy].height(hero_height));
  } else {
    content = content.push(container(page_title(state.t("tv-home"), scale)).height(100.0 * scale));
  }
  for (index, shelf) in full.home.data.rows().iter().enumerate() {
    let title = state.kernel.locale.message(&shelf.title);
    let mut section = Column::new().spacing(20.0 * scale).push(
      text(title)
        .size(style::SECTION * scale)
        .line_height(iced::Pixels(40.0 * scale))
        .font(HEADING_FONT)
        .color(style::PALETTE.text.heading),
    );
    match &shelf.items {
      LoadState::Ready(items) if !items.is_empty() => {
        let cards = move || {
          Row::with_children(items.iter().enumerate().map(|(item_index, item)| {
            media_card(
              state,
              item,
              Focus::Shelf {
                row: index,
                item: item_index,
              },
              (&full.home.artwork, ArtworkSurface::Home),
              ArtworkPlacement::Card(shelf.section).spec(item),
              (320.0 * scale, shelf.section.is_action()),
              scale,
            )
          }))
          .spacing(24.0 * scale)
          .into()
        };
        section = section.push(horizontal_shelf(
          state,
          navigation::shelf_id(index),
          if shelf.section.is_action() {
            320.0 * scale
          } else {
            620.0 * scale
          },
          cards,
          move |offset| {
            AppMessage::Tv(Message::HorizontalScrolled(
              navigation::shelf_id(index),
              offset,
            ))
          },
        ));
      }
      LoadState::Ready(_) => {
        section = section.push(
          text(state.t("tv-empty-shelf"))
            .size(style::BODY * scale)
            .line_height(iced::Pixels(32.0 * scale)),
        );
      }
      LoadState::Failed(error) => {
        section = section.push(
          text(state.kernel.locale.message(error))
            .size(style::BODY * scale)
            .line_height(iced::Pixels(32.0 * scale)),
        );
      }
      _ => {
        section = section.push(
          text(state.t("tv-loading"))
            .size(style::BODY * scale)
            .line_height(iced::Pixels(32.0 * scale)),
        );
      }
    }
    content = content.push(container(section).padding(iced::Padding {
      bottom: 36.0 * scale,
      ..Default::default()
    }));
  }
  content
    .push(action(
      state,
      Focus::Retry,
      state.t("home-retry"),
      None,
      false,
      scale,
    ))
    .into()
}

fn library(state: &State, scale: f32) -> Element<'_, AppMessage> {
  let Some(full) = state.full.as_ref() else {
    return status(state, state.t("tv-loading"), scale);
  };
  let title = if let Destination::Library { library_id, .. } = &state.shell.destination {
    navigation::shortcuts(state)
      .iter()
      .find(|library| library.id == *library_id)
      .map(|library| library.name.clone())
      .unwrap_or_else(|| state.t("browse-library"))
  } else {
    state.t("browse-library")
  };
  let selected = full.browse.filters.unwrap_or_default();
  let filters = row![
    action(
      state,
      Focus::Header(0),
      state.t("browse-all"),
      None,
      selected.played_filter() == jellypilot_media_server::VideoLibraryPlayedFilter::All,
      scale
    ),
    action(
      state,
      Focus::Header(1),
      state.t("browse-unplayed"),
      None,
      selected.played_filter() == jellypilot_media_server::VideoLibraryPlayedFilter::Unplayed,
      scale
    ),
    action(
      state,
      Focus::Header(2),
      if super::filters::active_count(&full.browse.advanced_filters) == 0 {
        state.t("tv-filter-title")
      } else {
        state.format(
          "tv-filter-active",
          &[(
            "count",
            super::filters::active_count(&full.browse.advanced_filters).into(),
          )],
        )
      },
      Some(Icon::Filter),
      super::filters::active_count(&full.browse.advanced_filters) > 0,
      scale
    ),
    action(
      state,
      Focus::Header(3),
      state.t(match selected.sort() {
        jellypilot_media_server::VideoLibrarySort::Title => "browse-sort-title",
        jellypilot_media_server::VideoLibrarySort::RecentlyAdded => "browse-sort-recently-added",
        jellypilot_media_server::VideoLibrarySort::ReleaseDate => "browse-sort-release-date",
      }),
      Some(Icon::SortAscending),
      false,
      scale
    ),
  ]
  .spacing(16.0 * scale);
  let total = match full.browse.view {
    LibraryBrowseView::Ready {
      total_record_count, ..
    } => Some(total_record_count),
    LibraryBrowseView::Empty => Some(0),
    _ => None,
  };
  let title = total.map_or(title.clone(), |count| {
    state.format(
      "tv-library-count",
      &[("title", title.into()), ("count", count.into())],
    )
  });
  let mut header = column![page_title(title, scale), filters].spacing(24.0 * scale);
  if matches!(state.shell.destination, Destination::Library { .. }) {
    header = header.push(
      row![
        action(
          state,
          Focus::Header(4),
          state.t("saved-filters-save-current"),
          Some(Icon::Bookmark),
          false,
          scale
        ),
        action(
          state,
          Focus::Header(5),
          super::saved_browse::conditions_label(state),
          Some(Icon::Filter),
          false,
          scale
        ),
      ]
      .spacing(16.0 * scale),
    );
  }
  let header = header.height(navigation::browse_header_height(state));
  let body: Element<'_, AppMessage> = match &full.browse.view {
    LibraryBrowseView::Ready {
      total_record_count,
      load_more_failure,
      ..
    } => {
      let metrics = navigation::metrics(state);
      let viewport = ArtworkGridViewport {
        offset_y: state.tv.offset - navigation::browse_header_height(state),
        height: navigation::viewport_height(state),
      };
      let grid = artwork_grid_with_overflow(
        *total_record_count as usize,
        metrics,
        viewport,
        style::GAP * scale,
        iced::Padding {
          top: style::POSTER_CLEARANCE * scale,
          left: 8.0 * scale,
          right: 8.0 * scale,
          bottom: 0.0,
        },
        move |index| {
          let focus = Focus::Grid(index as u32);
          match navigation::grid_item(state, index as u32) {
            Some(item) => {
              let spec = item.artwork_image_id.as_ref().map(|image_id| ImageSpec {
                key: item.id.clone(),
                image_id: image_id.clone(),
                size_class: ArtworkSizeClass::Card,
                derived: DerivedArtwork::default(),
              });
              media_card(
                state,
                item,
                focus,
                (&full.browse.artwork, ArtworkSurface::Browse),
                spec,
                (metrics.cell_width, false),
                scale,
              )
            }
            None => tv_focus::focus(
              state.tv.focus == focus && super::browse_focus_visible(state),
              move |progress| {
                button(
                  container(
                    text(state.t(if load_more_failure.is_some() {
                      "tv-load-failed"
                    } else {
                      "tv-loading"
                    }))
                    .size(style::BODY * scale)
                    .line_height(iced::Pixels(32.0 * scale)),
                  )
                  .center_x(Fill)
                  .center_y(Fill)
                  .width(metrics.cell_width)
                  .height(metrics.cell_height),
                )
                .style(style::button_progress(style::PALETTE, progress, false))
                .on_press(AppMessage::Tv(Message::Activate(Focus::Retry)))
                .into()
              },
            )
            .into(),
          }
        },
      );
      observe_grid_viewport(grid, full.browse.artwork.epoch())
    }
    LibraryBrowseView::Empty => status(state, state.t("browse-empty-library"), scale),
    LibraryBrowseView::Failed { .. } => status(state, state.t("browse-library-load-failed"), scale),
    LibraryBrowseView::Inactive | LibraryBrowseView::Loading => text(state.t("tv-loading"))
      .size(style::BODY * scale)
      .color(style::PALETTE.text.metadata)
      .into(),
  };
  column![header, body].width(Fill).into()
}

fn detail(state: &State, scale: f32) -> Element<'_, AppMessage> {
  let Some(full) = state.full.as_ref() else {
    return detail_notice(state.t("tv-loading"), scale);
  };
  let data = &full.detail.data;
  let back = action(
    state,
    Focus::DetailBack,
    state.t("tv-back"),
    Some(Icon::ChevronLeft),
    false,
    scale,
  );
  let content = match &data.content {
    LoadState::Ready(content) => content,
    LoadState::Failed(error) => {
      return column![
        back,
        status(state, state.kernel.locale.message(error), scale)
      ]
      .into()
    }
    _ => {
      return column![back, detail_notice(state.t("tv-loading"), scale)]
        .spacing(24.0 * scale)
        .into()
    }
  };
  let (title, metadata, overview) = match content {
    DetailContent::Item(item) => (
      item.name.clone(),
      detail_metadata(state.kernel.locale, item),
      item.overview.as_deref(),
    ),
    DetailContent::Show(show) => (
      show.name.clone(),
      show_detail_metadata(state.kernel.locale, show),
      show.overview.as_deref(),
    ),
  };
  let width = (state.shell.window_size.width
    - (style::RAIL + style::CONTENT_INSET + style::SAFE_X) * scale)
    .max(1.0);
  let backdrop = art(
    &full.detail.artwork,
    ArtworkSurface::Detail,
    crate::app::detail::hero_image_spec(content, crate::app::detail::DETAIL_BACKDROP_KEY),
    width,
    550.0 * scale,
    true,
    ImageAxis::Vertical,
  );
  let mut hero = column![
    row![back, space().width(Fill), clock(scale)]
      .width(Fill)
      .align_y(Alignment::Center),
    heading(title, scale),
    text(metadata)
      .size(style::META * scale)
      .line_height(iced::Pixels(28.0 * scale))
      .color(style::PALETTE.text.body),
  ]
  .spacing(24.0 * scale);
  if let Some(overview) = overview.filter(|copy| !copy.trim().is_empty()) {
    let expanded = data.overview_expanded;
    let copy = text(overview)
      .size(style::BODY * scale)
      .line_height(iced::Pixels(32.0 * scale))
      .color(style::PALETTE.text.body)
      .width((850.0 * scale).min(width));
    hero = hero
      .push(action(
        state,
        Focus::DetailOverview,
        state.t(if expanded {
          "tv-overview-collapse"
        } else {
          "tv-overview-expand"
        }),
        Some(if expanded {
          Icon::ChevronUp
        } else {
          Icon::ChevronDown
        }),
        false,
        scale,
      ))
      .push(
        container(copy)
          .height(if expanded {
            iced::Length::Fit
          } else {
            iced::Length::Fixed(128.0 * scale)
          })
          .clip(true)
          .id(iced::widget::Id::new("tv-detail-overview")),
      );
  }
  let mut actions = Row::new().spacing(20.0 * scale);
  if navigation::detail_playable(state).is_some() {
    actions = actions.push(action(
      state,
      Focus::DetailPlay,
      detail_state::play_label(state),
      Some(Icon::Play),
      false,
      scale,
    ));
  }
  let (item_id, favorite) = match content {
    DetailContent::Item(item) => (&item.id, item.favorite),
    DetailContent::Show(show) => (&show.id, show.favorite),
  };
  let watchlisted = full.personal_lists.watchlist_ids.contains(item_id);
  actions = actions
    .push(action(
      state,
      Focus::DetailWatchlist,
      state.t(if watchlisted {
        "detail-watchlisted"
      } else {
        "detail-watchlist-add"
      }),
      Some(if watchlisted {
        Icon::BookmarkFilled
      } else {
        Icon::Bookmark
      }),
      watchlisted,
      scale,
    ))
    .push(action(
      state,
      Focus::DetailFavorite,
      state.t(if favorite {
        "detail-favorited"
      } else {
        "detail-favorite"
      }),
      Some(if favorite {
        Icon::HeartFilled
      } else {
        Icon::Heart
      }),
      favorite,
      scale,
    ))
    .push(action(
      state,
      Focus::DetailMenu,
      state.t("tv-more"),
      Some(Icon::List),
      false,
      scale,
    ));
  hero = hero.push(actions);
  let mut page = column![
    stack![container(hero).height(iced::Length::Fit.min(570.0 * scale))].push_under(backdrop)
  ]
  .width(Fill)
  .spacing(24.0 * scale);
  if let DetailContent::Show(show) = content {
    let seasons = Row::with_children(show.seasons.iter().enumerate().map(|(index, season)| {
      action(
        state,
        Focus::Season(index),
        season.name.clone(),
        None,
        data.selected_season_id.as_deref() == Some(season.id.as_str()),
        scale,
      )
      .width(196.0 * scale)
      .into()
    }))
    .spacing(24.0 * scale);
    page = page.push(
      scrollable(seasons)
        .direction(Direction::Horizontal(Scrollbar::new()))
        .id(iced::widget::Id::new("tv-seasons"))
        .on_scroll(|viewport| {
          AppMessage::Tv(Message::HorizontalScrolled(
            iced::widget::Id::new("tv-seasons"),
            viewport.absolute_offset().x,
          ))
        })
        .height(80.0 * scale),
    );
    if show.seasons.is_empty() {
      page = page.push(detail_notice(state.t("detail-no-seasons"), scale));
    }
  }
  if let Some(episodes) = detail_state::episodes(state) {
    if !matches!(episodes, LoadState::Idle) {
      if matches!(content, DetailContent::Item(_)) {
        page = page.push(heading(state.t("detail-season-neighbors"), scale));
      }
      match episodes {
        LoadState::Ready(episodes) => {
          if episodes.episodes.is_empty() && !episodes.has_more {
            page = page.push(detail_notice(
              state.t(if matches!(content, DetailContent::Show(_)) {
                "detail-no-episodes"
              } else {
                "detail-no-neighbors"
              }),
              scale,
            ));
          } else {
            let cards = move || {
              let mut cards =
                Row::with_children(episodes.episodes.iter().enumerate().map(|(index, item)| {
                  media_card(
                    state,
                    item,
                    Focus::Episode(index),
                    (&full.detail.artwork, ArtworkSurface::Detail),
                    crate::app::detail::episode_image_spec(
                      data,
                      jellypilot_core::detail::detail_episode_key(&item.id),
                      item,
                    ),
                    (320.0 * scale, true),
                    scale,
                  )
                }))
                .spacing(24.0 * scale);
              if episodes.has_more {
                let append = detail_state::append(state);
                let label = state.t(match append {
                  Some(LoadState::Loading) => "tv-loading",
                  Some(LoadState::Failed(_)) => "detail-retry",
                  _ => "tv-load-more-episodes",
                });
                let mut more = column![action(
                  state,
                  Focus::DetailEpisodesMore,
                  label,
                  None,
                  false,
                  scale
                )]
                .spacing(24.0 * scale)
                .width(320.0 * scale);
                if let Some(LoadState::Failed(error)) = append {
                  more = more.push(detail_notice(state.kernel.locale.message(error), scale));
                }
                cards = cards.push(
                  container(more)
                    .width(320.0 * scale)
                    .height(320.0 * scale)
                    .align_y(Alignment::Center),
                );
              }
              cards.into()
            };
            page = page.push(horizontal_shelf(
              state,
              iced::widget::Id::new("tv-episodes"),
              340.0 * scale,
              cards,
              |offset| {
                AppMessage::Tv(Message::HorizontalScrolled(
                  iced::widget::Id::new("tv-episodes"),
                  offset,
                ))
              },
            ));
            if !episodes.has_more {
              page = page.push(detail_notice(state.t("tv-episodes-complete"), scale));
            }
          }
        }
        LoadState::Failed(error) => {
          page = page.push(
            column![
              detail_notice(state.kernel.locale.message(error), scale),
              action(
                state,
                Focus::DetailEpisodesRetry,
                state.t("detail-retry"),
                None,
                false,
                scale
              )
            ]
            .spacing(24.0 * scale),
          );
        }
        LoadState::Loading => page = page.push(detail_notice(state.t("tv-loading"), scale)),
        LoadState::Idle => {}
      }
    }
  }
  page.into()
}

fn detail_notice<'a>(message: String, scale: f32) -> Element<'a, AppMessage> {
  text(message)
    .size(style::BODY * scale)
    .line_height(iced::Pixels(32.0 * scale))
    .color(style::PALETTE.text.body)
    .into()
}

#[cfg(test)]
pub(super) mod tests {
  use super::*;
  use iced::advanced::{layout, renderer, renderer::Headless, widget::Tree};

  pub(crate) fn item(id: &str) -> VideoLibraryItem {
    VideoLibraryItem {
      id: id.to_owned(), name: format!("A long media title with translated characters 电视剧 {id}"), item_type: "Movie".to_owned(),
      production_year: Some(2025), premiere_date: None, community_rating: None, episode_count: None, last_played_date: None,
      runtime_seconds: Some(7200.0), played: false, favorite: false, artwork_image_id: None, backdrop_image_id: None, logo_image_id: None,
      series_poster_image_id: None, episode_thumb_image_id: None, series_thumb_image_id: None, series_backdrop_image_id: None,
      season_number: None, episode_number: None, index_number_end: None, series_id: None, series_name: None, end_year: None,
      series_continuing: false, unplayed_item_count: None, resume_position_seconds: None, played_percentage: None,
      overview: Some("A real detail synopsis can span several lines without shifting the focus and paging geometry.".to_owned()), season_poster_image_id: None,
    }
  }

  #[tokio::test]
  async fn home_long_copy_keeps_hero_actions_inside_the_hero_frame() {
    use iced::advanced::{widget, Layout};

    #[derive(Default)]
    struct Actions(Vec<iced::Rectangle>);
    impl widget::Operation for Actions {
      fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
        visit(self);
      }
      fn container(&mut self, id: Option<&widget::Id>, bounds: iced::Rectangle) {
        if [Focus::HeroPlay, Focus::HeroDetail, Focus::HeroWatchlist]
          .iter()
          .any(|focus| id == Some(&navigation::focus_id(*focus)))
        {
          self.0.push(bounds);
        }
      }
    }

    let renderer = iced::Renderer::new(
      renderer::Settings {
        font: iced::Font::DEFAULT,
        text_size: 16.0.into(),
        line_height: jellypilot_ui::fonts::DEFAULT_LINE_HEIGHT,
        metrics_hinting: false,
      },
      Some("tiny-skia"),
    )
    .await
    .expect("headless layout renderer");
    let mut state = State::boot(true);
    state.full = Some(crate::app::state::FullUi::default());
    state.shell.ui_mode = jellypilot_core::config::UiMode::Tv;
    let mut media = item("long-copy");
    media.name = "A long movie title with many words and translated characters 电视剧 ".repeat(12);
    media.overview =
      Some("A lengthy synopsis must not displace the Home playback actions. ".repeat(50));
    media.resume_position_seconds = Some(60.0);
    state
      .full
      .as_mut()
      .expect("full")
      .home
      .data
      .settle_video_home(Ok(jellypilot_media_server::VideoHome {
        continue_watching: vec![media],
        next_up: vec![],
      }));
    for bounds in [Size::new(1920.0, 1080.0), Size::new(1280.0, 720.0)] {
      state.shell.window_size = bounds;
      let scale = style::scale(bounds.width);
      let mut page = home(&state, scale);
      let mut tree = Tree::new(&page);
      tree.diff(page.as_widget_mut());
      let node = page.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(
          Size::ZERO,
          Size::new(content_width(&state, scale), f32::INFINITY),
        ),
      );
      let mut actions = Actions::default();
      page
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut actions);
      assert_eq!(actions.0.len(), 3);
      for action in actions.0 {
        assert!(
          action.height >= style::CONTROL * scale - 0.1,
          "squeezed action: {action:?}"
        );
        assert!(
          action.y + action.height <= 540.0 * scale + 0.1,
          "action outside Hero: {action:?}"
        );
      }
    }
  }

  #[tokio::test]
  async fn tv_backdrop_scrims_cover_the_image_when_the_parent_grows() {
    let renderer = iced::Renderer::new(
      renderer::Settings {
        font: iced::Font::DEFAULT,
        text_size: 16.0.into(),
        line_height: jellypilot_ui::fonts::DEFAULT_LINE_HEIGHT,
        metrics_hinting: false,
      },
      Some("tiny-skia"),
    )
    .await
    .expect("headless layout renderer");
    let handle = iced::widget::image::Handle::from_rgba(16, 9, vec![255; 16 * 9 * 4]);
    for scale in [1.0, 1280.0 / 1920.0] {
      let width = 1488.0 * scale;
      let image_height = 500.0 * scale;
      let parent_height = 540.0 * scale;
      let background = backdrop_scrims(
        image(handle.clone())
          .width(width)
          .height(image_height)
          .content_fit(ContentFit::Cover)
          .into(),
      );
      let mut hero: Element<'_, AppMessage> = stack![background, space()]
        .width(width)
        .height(parent_height)
        .into();
      let mut tree = Tree::new(&hero);
      tree.diff(hero.as_widget_mut());
      let node = hero.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(iced::Size::ZERO, iced::Size::new(width, parent_height)),
      );
      let layers = node.children()[0].children();
      let image_bounds = layers[0].bounds();
      assert_eq!(image_bounds.height, parent_height);
      for scrim in &layers[1..] {
        assert_eq!(
          scrim.bounds(),
          image_bounds,
          "uncovered backdrop band at scale {scale}"
        );
      }
    }
  }

  #[tokio::test]
  async fn tv_posters_fit_five_column_cells_and_shell_at_1080p_and_720p() {
    let renderer = iced::Renderer::new(
      renderer::Settings {
        font: iced::Font::DEFAULT,
        text_size: 16.0.into(),
        line_height: jellypilot_ui::fonts::DEFAULT_LINE_HEIGHT,
        metrics_hinting: false,
      },
      Some("tiny-skia"),
    )
    .await
    .expect("headless layout renderer");
    let mut state = State::boot(true);
    state.full = Some(crate::app::state::FullUi::default());
    state.shell.ui_mode = jellypilot_core::config::UiMode::Tv;
    state.kernel.connection = jellypilot_auth::login::ConnectionPhase::Connected;
    state.shell.destination = Destination::Library {
      library_id: "library".to_owned(),
      collection_type: "movies".to_owned(),
    };
    let media = item("one");
    for bounds in [
      iced::Size::new(1920.0, 1080.0),
      iced::Size::new(1280.0, 720.0),
    ] {
      state.shell.window_size = bounds;
      let metrics = navigation::metrics(&state);
      let scale = style::scale(bounds.width);
      let collection = &state.full.as_ref().expect("full").browse.artwork;
      let mut cell = media_card(
        &state,
        &media,
        Focus::Grid(0),
        (collection, ArtworkSurface::Browse),
        None,
        (metrics.cell_width, false),
        scale,
      );
      let mut tree = Tree::new(&cell);
      tree.diff(cell.as_widget_mut());
      let node = cell.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(
          iced::Size::ZERO,
          iced::Size::new(metrics.cell_width, 1000.0),
        ),
      );
      assert!(
        (node.size().height - metrics.cell_height).abs() < 0.5,
        "poster/copy exceeds the row pitch at {bounds:?}: {:?} versus {metrics:?}",
        node.size()
      );
      assert!(node.size().width <= metrics.cell_width + 0.1);
      let mut page = view(&state);
      let mut tree = Tree::new(&page);
      tree.diff(page.as_widget_mut());
      let node = page.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(iced::Size::ZERO, bounds),
      );
      assert_eq!(node.size(), bounds);
    }
  }

  #[tokio::test]
  async fn tv_settings_keeps_its_nested_scroll_viewport_bounded_at_1080p_and_720p() {
    use iced::advanced::{widget, Layout};

    #[derive(Default)]
    struct SettingsViewport(Option<iced::Rectangle>);

    impl widget::Operation for SettingsViewport {
      fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
        visit(self);
      }

      fn scrollable(
        &mut self,
        id: Option<&widget::Id>,
        bounds: iced::Rectangle,
        _content: iced::Rectangle,
        _translation: iced::Vector,
        _state: &mut dyn widget::operation::Scrollable,
      ) {
        if id == Some(&widget::Id::new("tv-settings-detail")) {
          self.0 = Some(bounds);
        }
      }
    }

    fn assert_finite(node: &layout::Node) {
      let bounds = node.bounds();
      assert!(
        [bounds.x, bounds.y, bounds.width, bounds.height]
          .into_iter()
          .all(f32::is_finite),
        "unbounded settings descendant: {bounds:?}"
      );
      for child in node.children() {
        assert_finite(child);
      }
    }

    let renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .expect("headless layout renderer");
    let mut state = State::boot(true);
    state.full = Some(crate::app::state::FullUi::default());
    state.shell.ui_mode = jellypilot_core::config::UiMode::Tv;
    state.kernel.connection = jellypilot_auth::login::ConnectionPhase::Connected;
    state.tv.settings.open = true;
    for bounds in [Size::new(1920.0, 1080.0), Size::new(1280.0, 720.0)] {
      state.shell.window_size = bounds;
      let mut page = view(&state);
      let mut tree = Tree::new(&page);
      tree.diff(page.as_widget_mut());
      let node = page.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, bounds),
      );
      assert_eq!(node.size(), bounds);
      assert_finite(&node);
      let mut viewport = SettingsViewport::default();
      page
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut viewport);
      let viewport = viewport.0.expect("settings detail scroll viewport");
      assert!(viewport.height > 0.0 && viewport.height < bounds.height);
      assert!(viewport.y >= 0.0 && viewport.y + viewport.height <= bounds.height);
    }
  }
}
