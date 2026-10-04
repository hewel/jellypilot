use iced::widget::operation::{self, AbsoluteOffset};
use iced::Task;
use jellypilot_core::browse_model::LibraryBrowseView;
use jellypilot_core::browse_window::visible_display_range;
use jellypilot_core::detail::DetailContent;
use jellypilot_core::tv_navigation::{grid_move, reveal_offset, GridMove, Input};
use jellypilot_core::LoadState;
use jellypilot_media_server::{
  VideoLibraryItem, VideoLibraryPlayedFilter, VideoLibraryShortcut, VideoLibrarySort,
};
use jellypilot_mpv::playback::{Playable, PlaybackStartPosition};
use jellypilot_mpv::playback_session::PlaybackIntent;
use jellypilot_ui::tv as style;
use jellypilot_ui::widgets::artwork_grid::{ArtworkGridMetrics, ArtworkGridViewport};

use super::{detail, AppMessage, Focus, State};
use crate::app::message::{BrowseMessage, DetailMessage, HomeMessage, PlaybackMessage};
use crate::app::state::Destination;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RailAction {
  Search,
  Home,
  Lists,
  Upcoming,
  Library(usize),
  Settings,
  Account,
  Exit,
}

pub(super) fn rail_actions(state: &State) -> Vec<RailAction> {
  [
    RailAction::Search,
    RailAction::Home,
    RailAction::Lists,
    RailAction::Upcoming,
  ]
  .into_iter()
  .chain((0..shortcuts(state).len()).map(RailAction::Library))
  .chain([RailAction::Settings, RailAction::Account, RailAction::Exit])
  .collect()
}

pub(super) fn focused_item(state: &State) -> Option<&VideoLibraryItem> {
  match state.tv.focus {
    Focus::Grid(index) => grid_item(state, index),
    Focus::Shelf { row, item } => shelf_item(state, row, item),
    Focus::Episode(index) => episodes(state).get(index),
    Focus::HeroPlay | Focus::HeroDetail | Focus::HeroWatchlist => {
      state.full.as_ref()?.home.data.featured_item()
    }
    Focus::DetailPlay | Focus::DetailMenu | Focus::DetailWatchlist | Focus::DetailFavorite => {
      let Destination::Detail(id) = &state.shell.destination else {
        return None;
      };
      state.full.as_ref()?.detail.items.get(id)
    }
    _ => None,
  }
}

pub(super) fn shortcuts(state: &State) -> &[VideoLibraryShortcut] {
  state
    .full
    .as_ref()
    .and_then(|full| match &full.home.data.shortcuts {
      LoadState::Ready(shortcuts) => Some(shortcuts.as_slice()),
      _ => None,
    })
    .unwrap_or_default()
}

pub(super) fn total(state: &State) -> u32 {
  state
    .full
    .as_ref()
    .map_or(0, |full| match full.browse.view {
      LibraryBrowseView::Ready {
        total_record_count, ..
      } => total_record_count,
      _ => 0,
    })
}

pub(super) fn reconcile_focus(state: &mut State) {
  let focus = match state.tv.focus {
    Focus::Rail(index) => Focus::Rail(index.min(rail_actions(state).len() - 1)),
    Focus::Grid(index) if total(state) > 0 => Focus::Grid(index.min(total(state) - 1)),
    Focus::Grid(_) => Focus::Header(0),
    Focus::Shelf { row, item } => {
      let relocated = state.tv.focused_item.as_ref().and_then(|id| {
        state.full.as_ref().and_then(|full| {
          full
            .home
            .data
            .rows()
            .iter()
            .enumerate()
            .find_map(|(row, shelf)| match &shelf.items {
              LoadState::Ready(items) => items
                .iter()
                .position(|item| &item.id == id)
                .map(|item| Focus::Shelf { row, item }),
              _ => None,
            })
        })
      });
      relocated.unwrap_or_else(|| {
        if shelf_len(state, row) > 0 {
          Focus::Shelf {
            row,
            item: item.min(shelf_len(state, row) - 1),
          }
        } else {
          first_shelf(state).unwrap_or(Focus::Retry)
        }
      })
    }
    Focus::HeroPlay | Focus::HeroDetail | Focus::HeroWatchlist
      if state
        .full
        .as_ref()
        .is_none_or(|full| full.home.data.featured_item().is_none()) =>
    {
      first_shelf(state).unwrap_or(Focus::Retry)
    }
    focus if matches!(state.shell.destination, Destination::Detail(_)) => {
      detail::normalize(state, focus)
    }
    other => other,
  };
  state.tv.focus = focus;
  state.tv.focused_item = match focus {
    Focus::Shelf { row, item } => shelf_item(state, row, item),
    Focus::Grid(index) => grid_item(state, index),
    Focus::Episode(index) => episodes(state).get(index),
    _ => None,
  }
  .map(|item| item.id.clone());
}

pub(super) fn grid_item(state: &State, index: u32) -> Option<&VideoLibraryItem> {
  let LibraryBrowseView::Ready {
    visible_items,
    visible_start,
    ..
  } = &state.full.as_ref()?.browse.view
  else {
    return None;
  };
  visible_items
    .get(index.checked_sub(*visible_start)? as usize)?
    .item
    .as_ref()
}

pub(super) fn shelf_item(state: &State, row: usize, item: usize) -> Option<&VideoLibraryItem> {
  let row = state.full.as_ref()?.home.data.rows().get(row)?;
  match &row.items {
    LoadState::Ready(items) => items.get(item),
    _ => None,
  }
}

fn shelf_len(state: &State, row: usize) -> usize {
  state
    .full
    .as_ref()
    .and_then(|full| full.home.data.rows().get(row))
    .map_or(0, |row| match &row.items {
      LoadState::Ready(items) => items.len(),
      _ => 0,
    })
}

fn shelf_neighbor(state: &State, row: usize, forward: bool) -> Option<usize> {
  let len = state.full.as_ref()?.home.data.rows().len();
  if forward {
    (row.saturating_add(1)..len).find(|&index| shelf_len(state, index) > 0)
  } else {
    (0..row).rev().find(|&index| shelf_len(state, index) > 0)
  }
}

fn first_shelf(state: &State) -> Option<Focus> {
  let len = state.full.as_ref()?.home.data.rows().len();
  (0..len)
    .find(|&index| shelf_len(state, index) > 0)
    .map(|row| Focus::Shelf { row, item: 0 })
}

pub(super) fn episodes(state: &State) -> &[VideoLibraryItem] {
  match detail::episodes(state) {
    Some(LoadState::Ready(page)) => &page.episodes,
    _ => &[],
  }
}

pub(super) fn seasons(state: &State) -> &[jellypilot_media_server::VideoSeason] {
  state
    .full
    .as_ref()
    .and_then(|full| match &full.detail.data.content {
      LoadState::Ready(DetailContent::Show(show)) => Some(show.seasons.as_slice()),
      _ => None,
    })
    .unwrap_or_default()
}

pub(super) fn play(state: &State, item: Playable) -> AppMessage {
  let resume = match &item {
    Playable::Library(item) => jellypilot_core::home_hero::has_resume_position(item),
    Playable::Detail(item) => item.can_resume,
    Playable::Media(_) => false,
  };
  AppMessage::Playback(PlaybackMessage::Intent(Box::new(PlaybackIntent::Start {
    item,
    position: if resume {
      PlaybackStartPosition::Resume
    } else {
      PlaybackStartPosition::Beginning
    },
    intro: state.kernel.intro_availability(),
    selection: Box::default(),
  })))
}

pub(super) fn detail_playable(state: &State) -> Option<Playable> {
  match &state.full.as_ref()?.detail.data.content {
    LoadState::Ready(DetailContent::Item(item)) if item.can_play => {
      Some(Playable::from(item.as_ref().clone()))
    }
    LoadState::Ready(DetailContent::Show(show)) if show.can_play => show
      .next_episode
      .as_ref()
      .map(|item| Playable::from(item.clone())),
    _ => None,
  }
}

pub(super) fn action_enabled(state: &State, focus: Focus) -> bool {
  match focus {
    Focus::Header(_) => matches!(state.shell.destination, Destination::Library { .. }),
    Focus::HeroWatchlist => {
      crate::app::collections::controls(state, crate::app::collections::Source::Hero)
        .watchlist_action
        .is_some()
    }
    Focus::HeroPlay | Focus::DetailPlay => state.playback.view.engine_available,
    Focus::DetailWatchlist | Focus::DetailFavorite => match &state.shell.destination {
      Destination::Detail(id) => {
        !crate::app::item_actions::busy(&state.kernel, id)
          && (focus != Focus::DetailWatchlist
            || state
              .full
              .as_ref()
              .is_some_and(|full| full.personal_lists.membership_loaded))
      }
      _ => false,
    },
    Focus::DetailEpisodesMore => {
      detail::has_more(state) && !matches!(detail::append(state), Some(LoadState::Loading))
    }
    Focus::DetailEpisodesRetry => detail::retry_episodes(state),
    _ => true,
  }
}

pub(super) fn activate(state: &mut State, focus: Focus) -> Task<AppMessage> {
  if !action_enabled(state, focus) {
    return Task::none();
  }
  if focus == Focus::DetailOverview {
    return Task::done(AppMessage::Detail(DetailMessage::OverviewToggled));
  }
  if let Focus::Lists(focus) = focus {
    return super::lists::activate(state, focus);
  }
  if let Focus::Rail(index) = focus {
    let Some(action) = rail_actions(state).get(index).copied() else {
      return Task::none();
    };
    match action {
      RailAction::Search => return super::search::open(state),
      RailAction::Upcoming => return super::player::upcoming::open(state),
      RailAction::Settings => return super::settings::open(state),
      RailAction::Account => {
        return super::settings::open_category(state, super::settings::Category::Account)
      }
      _ => {}
    }
  }
  if focus == Focus::Header(2) {
    return super::filters::open(state);
  }
  if focus == Focus::DetailMenu {
    return focused_item(state)
      .cloned()
      .map_or_else(Task::none, |item| super::lists::open_menu(state, item));
  }
  let message = match focus {
    Focus::Rail(index) => match rail_actions(state).get(index) {
      Some(RailAction::Home) => Some(AppMessage::Home(HomeMessage::Navigate(Destination::Home))),
      Some(RailAction::Lists) => Some(AppMessage::Home(HomeMessage::Navigate(
        Destination::PersonalLists(crate::app::personal_lists::Route::Watchlist),
      ))),
      Some(RailAction::Library(index)) => shortcuts(state).get(*index).map(|library| {
        AppMessage::Home(HomeMessage::Navigate(Destination::Library {
          library_id: library.id.clone(),
          collection_type: library.collection_type.clone(),
        }))
      }),
      Some(RailAction::Exit) => Some(super::exit()),
      _ => None,
    },
    Focus::HeroPlay => state
      .full
      .as_ref()
      .and_then(|full| full.home.data.featured_item())
      .map(|item| play(state, Playable::from(item.clone()))),
    Focus::HeroDetail => state
      .full
      .as_ref()
      .and_then(|full| full.home.data.featured_item())
      .map(|item| AppMessage::OpenDetail(Box::new(item.clone()))),
    Focus::HeroWatchlist => {
      crate::app::collections::controls(state, crate::app::collections::Source::Hero)
        .watchlist_action
    }
    Focus::Shelf { row, item } => {
      shelf_item(state, row, item).map(|item| AppMessage::OpenDetail(Box::new(item.clone())))
    }
    Focus::Grid(index) => grid_item(state, index)
      .map(|item| AppMessage::OpenDetail(Box::new(item.clone())))
      .or_else(|| {
        state
          .full
          .as_ref()
          .and_then(|full| match &full.browse.view {
            LibraryBrowseView::Ready {
              load_more_failure: Some(failure),
              retry_busy: false,
              ..
            } if failure.retryable => Some(AppMessage::Browse(BrowseMessage::Retry)),
            _ => None,
          })
      }),
    Focus::Header(0) => Some(AppMessage::Browse(BrowseMessage::PlayedFilterChanged(
      VideoLibraryPlayedFilter::All,
    ))),
    Focus::Header(1) => Some(AppMessage::Browse(BrowseMessage::PlayedFilterChanged(
      VideoLibraryPlayedFilter::Unplayed,
    ))),
    Focus::Header(3) => Some(AppMessage::Browse(BrowseMessage::SortChanged(
      if state
        .full
        .as_ref()
        .and_then(|full| full.browse.filters)
        .unwrap_or_default()
        .sort()
        == VideoLibrarySort::RecentlyAdded
      {
        VideoLibrarySort::Title
      } else {
        VideoLibrarySort::RecentlyAdded
      },
    ))),
    Focus::Header(_) => None,
    Focus::DetailBack => Some(AppMessage::Detail(DetailMessage::Back)),
    Focus::DetailPlay => detail_playable(state).map(|item| play(state, item)),
    Focus::DetailWatchlist => Some(AppMessage::ItemActions(
      crate::app::item_actions::Message::DetailWatchlist,
    )),
    Focus::DetailFavorite => Some(AppMessage::ItemActions(
      crate::app::item_actions::Message::Detail(crate::app::state::UserDataActionKind::Favorite),
    )),
    Focus::DetailMenu | Focus::Lists(_) | Focus::DetailOverview => None,
    Focus::DetailEpisodesMore => Some(AppMessage::Detail(detail::episode_message(state, true))),
    Focus::DetailEpisodesRetry => Some(AppMessage::Detail(detail::episode_message(state, false))),
    Focus::Season(index) => seasons(state)
      .get(index)
      .map(|season| AppMessage::Detail(DetailMessage::SeasonSelected(season.id.clone()))),
    Focus::Episode(index) => episodes(state)
      .get(index)
      .map(|item| play(state, Playable::from(item.clone()))),
    Focus::Retry => Some(match state.shell.destination {
      Destination::Detail(_) => AppMessage::Detail(DetailMessage::Retry),
      Destination::Library { .. } | Destination::Search(_) => {
        AppMessage::Browse(BrowseMessage::Retry)
      }
      _ => AppMessage::Home(HomeMessage::Retry),
    }),
  };
  message.map_or_else(Task::none, Task::done)
}

fn browse_retry_available(state: &State) -> bool {
  state.full.as_ref().is_some_and(|full| {
    matches!(
      full.browse.view,
      LibraryBrowseView::Failed { .. } | LibraryBrowseView::Empty
    )
  })
}

pub(super) fn input(state: &mut State, input: Input) -> Task<AppMessage> {
  if input == Input::Confirm {
    return activate(state, state.tv.focus);
  }
  if input == Input::Back {
    if matches!(state.shell.destination, Destination::Detail(_)) {
      return Task::done(AppMessage::Detail(DetailMessage::Back));
    }
    if matches!(state.tv.focus, Focus::Rail(_)) {
      return Task::done(super::exit());
    }
    state.tv.content_focus = state.tv.focus;
    state.tv.focus = Focus::Rail(1);
    return Task::none();
  }
  if input == Input::PlayPause {
    if state.tv.focus == Focus::DetailPlay {
      return detail_playable(state)
        .map(|item| play(state, item))
        .map_or_else(Task::none, Task::done);
    }
    let item = match state.tv.focus {
      Focus::Grid(index) => grid_item(state, index),
      Focus::Shelf { row, item } => shelf_item(state, row, item),
      Focus::Episode(index) => episodes(state).get(index),
      Focus::HeroPlay | Focus::HeroDetail => state
        .full
        .as_ref()
        .and_then(|full| full.home.data.featured_item()),
      _ => None,
    };
    return item
      .map(|item| play(state, Playable::from(item.clone())))
      .map_or_else(Task::none, Task::done);
  }
  let old = state.tv.focus;
  if matches!(state.shell.destination, Destination::Detail(_)) && !matches!(old, Focus::Rail(_)) {
    if old == Focus::DetailOverview
      && detail::overview_expanded(state)
      && matches!(input, Input::Up | Input::Down)
    {
      return detail::read_overview(state, input);
    }
    state.tv.focus = detail::navigate(state, input);
    state.tv.focused_item = None;
    return reveal(state);
  }
  let next = match (old, input) {
    (Focus::Rail(index), Input::Up) => Focus::Rail(index.saturating_sub(1)),
    (Focus::Rail(index), Input::Down) => {
      Focus::Rail((index + 1).min(rail_actions(state).len() - 1))
    }
    (Focus::Rail(_), Input::Right) => state.tv.content_focus,
    (Focus::HeroPlay, Input::Right) => Focus::HeroDetail,
    (Focus::HeroDetail, Input::Right) => Focus::HeroWatchlist,
    (Focus::HeroWatchlist, Input::Left) => Focus::HeroDetail,
    (Focus::HeroDetail, Input::Left) => Focus::HeroPlay,
    (Focus::HeroPlay, Input::Left) => Focus::Rail(0),
    (Focus::HeroPlay | Focus::HeroDetail | Focus::HeroWatchlist, Input::Down) => {
      first_shelf(state).unwrap_or(Focus::Retry)
    }
    (Focus::Shelf { row: _, item: 0 }, Input::Left) => Focus::Rail(0),
    (Focus::Shelf { row, item }, Input::Left) => Focus::Shelf {
      row,
      item: item.saturating_sub(1),
    },
    (Focus::Shelf { row, item }, Input::Right) => Focus::Shelf {
      row,
      item: (item + 1).min(shelf_len(state, row).saturating_sub(1)),
    },
    (Focus::Shelf { row, item }, Input::Down) => shelf_neighbor(state, row, true)
      .map(|row| Focus::Shelf {
        row,
        item: item.min(shelf_len(state, row).saturating_sub(1)),
      })
      .unwrap_or(Focus::Retry),
    (Focus::Shelf { row, item }, Input::Up) => shelf_neighbor(state, row, false)
      .map(|row| Focus::Shelf {
        row,
        item: item.min(shelf_len(state, row).saturating_sub(1)),
      })
      .unwrap_or(Focus::HeroPlay),
    (Focus::Grid(index), direction) => match grid_move(index, total(state), direction) {
      GridMove::Item(index) => Focus::Grid(index),
      GridMove::Rail => Focus::Rail(0),
      GridMove::Header => Focus::Header(0),
    },
    (Focus::Header(0), Input::Left) => Focus::Rail(0),
    (Focus::Header(index), Input::Left) => Focus::Header(index.saturating_sub(1)),
    (Focus::Header(index), Input::Right) => Focus::Header((index + 1).min(3)),
    (Focus::Header(_), Input::Down) if total(state) > 0 => Focus::Grid(0),
    (Focus::Header(_), Input::Down) if browse_retry_available(state) => Focus::Retry,
    (Focus::Retry, Input::Up) => match state.shell.destination {
      Destination::Detail(_) if !seasons(state).is_empty() => Focus::Season(0),
      Destination::Detail(_) => Focus::DetailBack,
      Destination::Library { .. } | Destination::Search(_) => Focus::Header(0),
      _ => first_shelf(state).unwrap_or(Focus::HeroPlay),
    },
    (Focus::Retry, Input::Left) => Focus::Rail(0),
    _ => old,
  };
  if matches!(next, Focus::Rail(_)) && !matches!(old, Focus::Rail(_)) {
    state.tv.content_focus = old;
  }
  state.tv.focus = next;
  state.tv.focused_item = None;
  reveal(state)
}

pub(super) fn metrics(state: &State) -> ArtworkGridMetrics {
  let scale = style::scale(state.shell.window_size.width);
  let available =
    state.shell.window_size.width - (style::RAIL + style::CONTENT_INSET + style::SAFE_X) * scale;
  let width = ((available - style::GAP * scale * 4.0) / 5.0).max(1.0);
  ArtworkGridMetrics {
    columns: 5,
    cell_width: width,
    cell_height: width * 1.5 + 108.0 * scale,
    row_height: width * 1.5 + 132.0 * scale,
  }
}

pub(super) fn viewport_height(state: &State) -> f32 {
  (state.shell.window_size.height - 200.0 * style::scale(state.shell.window_size.width)).max(1.0)
}

pub(super) fn sync_browse(state: &mut State) -> Task<AppMessage> {
  if !matches!(
    state.shell.destination,
    Destination::Library { .. } | Destination::Search(_)
  ) {
    return Task::none();
  }
  let metrics = metrics(state);
  let scale = style::scale(state.shell.window_size.width);
  let viewport = ArtworkGridViewport {
    offset_y: state.tv.offset - 184.0 * scale,
    height: viewport_height(state),
  };
  let total = total(state);
  if let Some(full) = state.full.as_mut() {
    full.browse.presentation_grid = Some(metrics);
    full.browse.viewport.offset_y = state.tv.offset;
    full.browse.grid_viewport = Some(viewport);
    if total > 0 {
      return crate::app::browse::set_display_range(
        &mut full.browse,
        &mut state.kernel,
        visible_display_range(
          viewport.offset_y,
          viewport.height,
          5,
          metrics.row_height,
          total,
        ),
      );
    }
  }
  Task::none()
}

pub(super) fn scroll_id() -> iced::widget::Id {
  iced::widget::Id::new("tv-content")
}
pub(super) fn shelf_id(row: usize) -> iced::widget::Id {
  iced::widget::Id::from(format!("tv-shelf-{row}"))
}
pub(super) fn focus_id(focus: Focus) -> iced::widget::Id {
  iced::widget::Id::from(format!("tv-focus-{focus:?}"))
}

pub(super) fn restore_scroll(state: &State) -> Task<AppMessage> {
  Task::batch(
    std::iter::once(operation::scroll_to(
      scroll_id(),
      AbsoluteOffset {
        x: 0.0,
        y: state.tv.offset,
      },
    ))
    .chain(
      state.tv.horizontal.iter().map(|(id, offset)| {
        operation::scroll_to(id.clone(), AbsoluteOffset { x: *offset, y: 0.0 })
      }),
    ),
  )
}

fn reveal(state: &mut State) -> Task<AppMessage> {
  if let Focus::Rail(index) = state.tv.focus {
    return operation::scroll_to(
      iced::widget::Id::new("tv-rail"),
      AbsoluteOffset {
        x: 0.0,
        y: (index.saturating_sub(3) as f32) * 80.0 * style::scale(state.shell.window_size.width),
      },
    );
  }
  let Focus::Grid(index) = state.tv.focus else {
    return reveal_measured(state);
  };
  let scale = style::scale(state.shell.window_size.width);
  let metrics = metrics(state);
  let top = 184.0 * scale + (index / 5) as f32 * metrics.row_height;
  state.tv.offset = reveal_offset(
    top,
    top + metrics.cell_height,
    state.tv.offset,
    viewport_height(state),
  );
  Task::batch([restore_scroll(state), sync_browse(state)])
}

/// Nonvirtual shelves use their actual layout, including translated text and nested scrolls.
pub(super) fn reveal_measured(state: &State) -> Task<AppMessage> {
  if matches!(state.tv.focus, Focus::Rail(_)) {
    return Task::none();
  }
  use iced::advanced::widget;
  struct Reveal {
    scale: f32,
    focus: Focus,
    destination: Destination,
    session: jellypilot_core::request_gate::SessionToken,
    target: Option<iced::Rectangle>,
    viewport: Option<(iced::Rectangle, iced::Rectangle)>,
    horizontal: Option<(iced::widget::Id, Option<(iced::Rectangle, iced::Rectangle)>)>,
    offset: f32,
  }
  impl widget::Operation<AppMessage> for Reveal {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation<AppMessage>)) {
      visit(self);
    }
    fn container(&mut self, id: Option<&widget::Id>, bounds: iced::Rectangle) {
      if id == Some(&focus_id(self.focus)) {
        self.target = Some(
          if matches!(
            self.focus,
            Focus::Shelf { .. } | Focus::Episode(_) | Focus::Lists(super::lists::Focus::Card(_))
          ) {
            let scale = self.scale;
            iced::Rectangle {
              x: bounds.x - 8.0 * scale,
              y: bounds.y - style::POSTER_CLEARANCE * scale,
              width: bounds.width + 16.0 * scale,
              height: bounds.height + style::POSTER_CLEARANCE * scale,
            }
          } else {
            bounds
          },
        );
      }
    }
    fn scrollable(
      &mut self,
      id: Option<&widget::Id>,
      bounds: iced::Rectangle,
      content: iced::Rectangle,
      _translation: iced::Vector,
      _state: &mut dyn widget::operation::Scrollable,
    ) {
      if id == Some(&scroll_id()) {
        self.viewport = Some((bounds, content));
      }
      if let Some((horizontal_id, geometry)) = &mut self.horizontal {
        if id == Some(horizontal_id) {
          *geometry = Some((bounds, content));
        }
      }
    }
    fn finish(&self) -> widget::operation::Outcome<AppMessage> {
      let (Some(target), Some((viewport, content))) = (self.target, self.viewport) else {
        return widget::operation::Outcome::None;
      };
      let top = target.y - content.y;
      let y = reveal_offset(top, top + target.height, self.offset, viewport.height);
      let horizontal = self.horizontal.as_ref().and_then(|(id, geometry)| {
        let (viewport, content) = (*geometry)?;
        let left = target.x - content.x;
        let offset = (viewport.x - content.x).max(0.0);
        Some((
          id.clone(),
          reveal_offset(left, left + target.width, offset, viewport.width),
        ))
      });
      widget::operation::Outcome::Some(AppMessage::Tv(super::Message::Revealed {
        focus: self.focus,
        destination: self.destination.clone(),
        session: self.session,
        y,
        horizontal,
      }))
    }
  }
  let horizontal = match state.tv.focus {
    Focus::Shelf { row, .. } => Some(shelf_id(row)),
    Focus::Season(_) => Some(iced::widget::Id::new("tv-seasons")),
    Focus::Episode(_) | Focus::DetailEpisodesMore => Some(iced::widget::Id::new("tv-episodes")),
    _ => None,
  };
  widget::operate(Reveal {
    focus: state.tv.focus,
    destination: state.shell.destination.clone(),
    session: state.kernel.request_gate.current_session(),
    target: None,
    viewport: None,
    horizontal: horizontal.map(|id| (id, None)),
    offset: state.tv.offset,
    scale: style::scale(state.shell.window_size.width),
  })
}
