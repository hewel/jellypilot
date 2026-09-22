//! TV list presentation over the shared, account-fenced list and mutation services.

use iced::widget::{button, column, container, mouse_area, row, space, stack, text, Column, Row};
use iced::{Alignment, Element, Fill, Task};
use jellypilot_core::item_actions::{Action, Receipt};
use jellypilot_core::request_gate::SessionToken;
use jellypilot_core::tv_navigation::Input;
use jellypilot_core::undo_notices::UndoPause;
use jellypilot_media_server::{VideoLibraryItem, VideoUserDataAction};
use jellypilot_mpv::playback::Playable;
use jellypilot_ui::fonts::HEADING_FONT;
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::tv as style;
use jellypilot_ui::widgets::notice_interaction::{notice_interaction, NoticeInteraction};
use jellypilot_ui::widgets::tv_focus;

use super::{AppMessage, Focus as TvFocus, State};
use crate::app::artwork::ArtworkSurface;
use crate::app::item_actions::{self, Origin};
use crate::app::message::HomeMessage;
use crate::app::personal_lists::{
  self, Kind, ListEntry, ListPage, PersonalListsMessage, Route, PAGE_SIZE,
};
use crate::app::state::Destination;
use crate::app::{list_playback, undo};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Focus {
  Tab(Kind),
  Card(usize),
  Empty,
  Retry,
  Undo(u64),
  Added,
}

#[derive(Clone)]
pub enum Message {
  Scrolled(Kind, f32),
  MenuChoose {
    session: SessionToken,
    serial: u64,
    row: usize,
  },
  MenuFocus(usize),
  CloseMenu,
  Undo {
    session: SessionToken,
    id: u64,
  },
  Dismiss {
    session: SessionToken,
    id: u64,
  },
  NoticeInteraction(u64, NoticeInteraction),
  ViewAdded(SessionToken),
}

#[derive(Default)]
struct Position {
  index: usize,
  offset: f32,
  item_id: Option<String>,
}

struct Menu {
  session: SessionToken,
  serial: u64,
  id: String,
  name: String,
  item: Option<VideoLibraryItem>,
  origin: TvFocus,
  destination: Destination,
  row: usize,
  error: bool,
  pending: Option<Action>,
}

struct Added {
  kind: Kind,
  item_id: String,
  destination: Destination,
}

struct RemovalPosition {
  item_id: String,
  kind: Kind,
  index: usize,
  destination: Destination,
}

struct Reveal {
  kind: Kind,
  item_id: String,
}

#[derive(Default)]
pub struct Surface {
  session: Option<SessionToken>,
  watchlist: Position,
  favorites: Position,
  menu: Option<Menu>,
  serial: u64,
  added: Option<Added>,
  removed: Vec<RemovalPosition>,
  reveal: Option<Reveal>,
  initialized: bool,
  paused_undo: Option<u64>,
  presented: Option<Route>,
  switch_to: Option<Kind>,
}

fn kind(state: &State) -> Kind {
  if matches!(
    state.shell.destination,
    Destination::PersonalLists(Route::Favorites)
  ) {
    Kind::Favorites
  } else {
    Kind::Watchlist
  }
}

fn route(kind: Kind) -> Route {
  if kind == Kind::Favorites {
    Route::Favorites
  } else {
    Route::Watchlist
  }
}

fn list_page(state: &State, kind: Kind) -> Option<&ListPage> {
  let lists = &state.full.as_ref()?.personal_lists;
  Some(if kind == Kind::Favorites {
    &lists.favorites
  } else {
    &lists.watchlist
  })
}

fn position(surface: &Surface, kind: Kind) -> &Position {
  if kind == Kind::Favorites {
    &surface.favorites
  } else {
    &surface.watchlist
  }
}

fn position_mut(surface: &mut Surface, kind: Kind) -> &mut Position {
  if kind == Kind::Favorites {
    &mut surface.favorites
  } else {
    &mut surface.watchlist
  }
}

fn entry(state: &State, index: usize) -> Option<&ListEntry> {
  let page = list_page(state, kind(state))?;
  page.entries.get(index.checked_sub(page.offset)?)
}

pub fn focused_item(state: &State) -> Option<VideoLibraryItem> {
  let TvFocus::Lists(Focus::Card(index)) = state.tv.focus else {
    return None;
  };
  entry(state, index)?.item.clone()
}

pub fn menu_open(state: &State) -> bool {
  state.tv.lists.menu.is_some()
}

pub fn open_focused_menu(state: &mut State) -> Task<AppMessage> {
  let TvFocus::Lists(Focus::Card(index)) = state.tv.focus else {
    return Task::none();
  };
  let Some(entry) = entry(state, index) else {
    return Task::none();
  };
  let (id, name, item) = (entry.id.clone(), entry.name.clone(), entry.item.clone());
  open(state, id, name, item)
}

pub fn open_menu(state: &mut State, item: VideoLibraryItem) -> Task<AppMessage> {
  open(state, item.id.clone(), item.name.clone(), Some(item))
}

fn open(
  state: &mut State,
  id: String,
  name: String,
  item: Option<VideoLibraryItem>,
) -> Task<AppMessage> {
  if !state.tv_mode()
    || state.shell.quit_requested
    || crate::app::accounts::content_mutations_blocked(&state.kernel)
  {
    return Task::none();
  }
  state.tv.lists.serial = state.tv.lists.serial.wrapping_add(1);
  state.tv.lists.menu = Some(Menu {
    session: state.kernel.request_gate.current_session(),
    serial: state.tv.lists.serial,
    id,
    name,
    item,
    origin: state.tv.focus,
    destination: state.shell.destination.clone(),
    row: if matches!(
      state.shell.destination,
      Destination::PersonalLists(Route::Favorites)
    ) {
      1
    } else {
      0
    },
    error: false,
    pending: None,
  });
  Task::none()
}

fn close_menu(state: &mut State) {
  if let Some(menu) = state.tv.lists.menu.take() {
    if menu.destination == state.shell.destination
      && state.kernel.request_gate.is_current_session(menu.session)
    {
      state.tv.focus = menu.origin;
    }
  }
}

pub fn toggle_watchlist(state: &mut State, item: VideoLibraryItem) -> Task<AppMessage> {
  if state
    .full
    .as_ref()
    .is_none_or(|full| !full.personal_lists.membership_loaded)
  {
    return Task::none();
  }
  item_actions::update(
    state,
    item_actions::Message::WatchlistToggle(Box::new(item)),
  )
}

fn window(state: &mut State, kind: Kind, index: usize) -> Task<AppMessage> {
  load_range(state, kind, visible_range(state, kind, index))
}

fn load_range(state: &mut State, kind: Kind, visible: std::ops::Range<usize>) -> Task<AppMessage> {
  let Some(full) = state.full.as_mut() else {
    return Task::none();
  };
  personal_lists::load_window(
    &mut full.personal_lists,
    &mut state.kernel,
    &state.watchlist,
    kind,
    visible,
  )
}

fn visible_range(state: &State, kind: Kind, index: usize) -> std::ops::Range<usize> {
  let scale = style::scale(state.shell.window_size.width);
  let pitch = 344.0 * scale;
  let available = super::view::content_width(state, scale);
  let offset = position(&state.tv.lists, kind).offset;
  let start = (offset / pitch).floor().max(0.0) as usize;
  let end = ((offset + available) / pitch).ceil().max(0.0) as usize;
  start.min(index)..end.max(index.saturating_add(1))
}

fn scroll_id(kind: Kind) -> iced::widget::Id {
  iced::widget::Id::from(format!("tv-lists-{kind:?}"))
}

fn scroll(state: &mut State, kind: Kind, index: usize) -> Task<AppMessage> {
  let scale = style::scale(state.shell.window_size.width);
  let pitch = 344.0 * scale;
  let available =
    state.shell.window_size.width - (style::RAIL + style::CONTENT_INSET + style::SAFE_X) * scale;
  let saved = position_mut(&mut state.tv.lists, kind);
  let left = index as f32 * pitch;
  let right = left + 320.0 * scale;
  if left < saved.offset {
    saved.offset = left;
  }
  if right > saved.offset + available {
    saved.offset = (right - available + 24.0 * scale).max(0.0);
  }
  iced::widget::operation::scroll_to(
    scroll_id(kind),
    iced::widget::operation::AbsoluteOffset {
      x: saved.offset,
      y: 0.0,
    },
  )
}

fn body_focus(state: &State, kind: Kind) -> Focus {
  let Some(page) = list_page(state, kind) else {
    return Focus::Retry;
  };
  if page.total > 0 {
    Focus::Card(position(&state.tv.lists, kind).index.min(page.total - 1))
  } else if page.error.is_some() {
    Focus::Retry
  } else {
    Focus::Empty
  }
}

fn set_focus(state: &mut State, focus: Focus) -> Task<AppMessage> {
  state.tv.focus = TvFocus::Lists(focus);
  if let Focus::Card(index) = focus {
    let kind = kind(state);
    let item_id = entry(state, index).map(|entry| entry.id.clone());
    let saved = position_mut(&mut state.tv.lists, kind);
    saved.index = index;
    saved.item_id = item_id;
    sync_pause(state);
    return Task::batch([scroll(state, kind, index), window(state, kind, index)]);
  }
  sync_pause(state);
  Task::none()
}

fn sync_pause(state: &mut State) {
  let focused = match state.tv.focus {
    TvFocus::Lists(Focus::Undo(id)) if state.tv_mode() && !menu_open(state) => Some(id),
    _ => None,
  };
  if focused == state.tv.lists.paused_undo {
    return;
  }
  state.kernel.undo.advance();
  if let Some(id) = state.tv.lists.paused_undo.take() {
    state
      .kernel
      .undo
      .queue
      .set_paused(id, UndoPause::Focused, false);
  }
  if let Some(id) = focused {
    state
      .kernel
      .undo
      .queue
      .set_paused(id, UndoPause::Focused, true);
  }
  state.tv.lists.paused_undo = focused;
}

pub fn leave(state: &mut State) {
  if let Some(id) = state.tv.lists.paused_undo.take() {
    state
      .kernel
      .undo
      .queue
      .set_paused(id, UndoPause::Focused, false);
  }
  state.tv.lists.menu = None;
  state.tv.lists.added = None;
  state.tv.lists.reveal = None;
}

pub fn focus_feedback(state: &mut State) -> bool {
  let undo = state.kernel.undo.queue.visible().next();
  if let Some(id) = undo {
    state.tv.content_focus = state.tv.focus;
    state.tv.focus = TvFocus::Lists(Focus::Undo(id));
    sync_pause(state);
    true
  } else if state.tv.lists.added.is_some() {
    state.tv.content_focus = state.tv.focus;
    state.tv.focus = TvFocus::Lists(Focus::Added);
    true
  } else {
    false
  }
}

pub fn activate(state: &mut State, focus: Focus) -> Task<AppMessage> {
  if menu_open(state) {
    return Task::none();
  }
  match focus {
    Focus::Tab(kind) => {
      if kind == self::kind(state) {
        return set_focus(state, body_focus(state, kind));
      }
      state.tv.lists.switch_to = Some(kind);
      crate::app::update::route_message(
        state,
        AppMessage::Home(HomeMessage::Navigate(Destination::PersonalLists(route(
          kind,
        )))),
      )
    }
    Focus::Card(index) => {
      let Some(entry) = entry(state, index) else {
        return Task::none();
      };
      let id = entry.id.clone();
      let kind = kind(state);
      let saved = position_mut(&mut state.tv.lists, kind);
      saved.index = index;
      saved.item_id = Some(id.clone());
      let destination = Destination::Detail(id);
      crate::app::update::route_message(state, AppMessage::Home(HomeMessage::Navigate(destination)))
    }
    Focus::Empty => {
      let destination = super::navigation::shortcuts(state)
        .first()
        .map(|library| Destination::Library {
          library_id: library.id.clone(),
          collection_type: library.collection_type.clone(),
        })
        .unwrap_or(Destination::Home);
      crate::app::update::route_message(state, AppMessage::Home(HomeMessage::Navigate(destination)))
    }
    Focus::Retry => {
      let kind = kind(state);
      crate::app::update::route_message(
        state,
        AppMessage::PersonalLists(PersonalListsMessage::Retry(kind)),
      )
    }
    Focus::Undo(id) => undo::update(state, undo::Message::Restore(id)),
    Focus::Added => view_added(state),
  }
}

pub fn input(state: &mut State, input: Input) -> Task<AppMessage> {
  if menu_open(state) {
    match input {
      Input::Back => close_menu(state),
      Input::Up => {
        if let Some(menu) = &mut state.tv.lists.menu {
          menu.row = 0;
        }
      }
      Input::Down => {
        if let Some(menu) = &mut state.tv.lists.menu {
          menu.row = 1;
        }
      }
      Input::Confirm => {
        if let Some(menu) = state.tv.lists.menu.as_ref() {
          return update(
            state,
            Message::MenuChoose {
              session: menu.session,
              serial: menu.serial,
              row: menu.row,
            },
          );
        }
      }
      _ => {}
    }
    return Task::none();
  }
  let kind = kind(state);
  let focus = match state.tv.focus {
    TvFocus::Lists(focus) => focus,
    _ => body_focus(state, kind),
  };
  if input == Input::Back {
    match focus {
      Focus::Undo(id) => {
        let task = undo::update(state, undo::Message::Dismiss(id));
        state.tv.focus = state.tv.content_focus;
        sync_pause(state);
        return task;
      }
      Focus::Added => {
        state.tv.lists.added = None;
        state.tv.focus = state.tv.content_focus;
        return Task::none();
      }
      _ => return crate::app::shell::navigate_back(state),
    }
  }
  if input == Input::Confirm {
    return activate(state, focus);
  }
  if input == Input::PlayPause {
    if let Some(item) = focused_item(state) {
      match list_playback::target(state, &item) {
        list_playback::Target::Ready(item) => {
          let command = super::navigation::play(state, Playable::Library(item.clone()));
          return crate::app::update::route_message(state, command);
        }
        list_playback::Target::Failed(_) => {
          return crate::app::update::route_message(
            state,
            AppMessage::ListPlayback(list_playback::Message::Retry(item.id)),
          )
        }
        _ => {}
      }
    }
    return Task::none();
  }
  let next = match (focus, input) {
    (Focus::Tab(Kind::Watchlist), Input::Right) => Focus::Tab(Kind::Favorites),
    (Focus::Tab(Kind::Favorites), Input::Left) => Focus::Tab(Kind::Watchlist),
    (Focus::Tab(_), Input::Down) => body_focus(state, kind),
    (Focus::Card(index), Input::Right) => Focus::Card(
      (index + 1).min(list_page(state, kind).map_or(0, |page| page.total.saturating_sub(1))),
    ),
    (Focus::Card(index), Input::Left) if index > 0 => Focus::Card(index - 1),
    (Focus::Card(_) | Focus::Empty | Focus::Retry, Input::Up) => Focus::Tab(kind),
    (Focus::Card(_) | Focus::Empty, Input::Down)
      if list_page(state, kind).is_some_and(|page| page.error.is_some()) =>
    {
      Focus::Retry
    }
    (Focus::Card(_) | Focus::Empty | Focus::Retry, Input::Down) => {
      if focus_feedback(state) {
        return Task::none();
      }
      focus
    }
    (Focus::Undo(id), Input::Down) => {
      let ids: Vec<_> = state.kernel.undo.queue.visible().collect();
      ids
        .iter()
        .position(|candidate| *candidate == id)
        .and_then(|index| ids.get(index + 1))
        .copied()
        .map(Focus::Undo)
        .unwrap_or(focus)
    }
    (Focus::Undo(id), Input::Up) => {
      let ids: Vec<_> = state.kernel.undo.queue.visible().collect();
      if let Some(previous) = ids
        .iter()
        .position(|candidate| *candidate == id)
        .and_then(|index| index.checked_sub(1))
        .and_then(|index| ids.get(index))
        .copied()
      {
        Focus::Undo(previous)
      } else {
        state.tv.focus = state.tv.content_focus;
        sync_pause(state);
        return Task::none();
      }
    }
    (Focus::Added, Input::Up) => {
      state.tv.focus = state.tv.content_focus;
      return Task::none();
    }
    (_, Input::Left) => {
      state.tv.content_focus = state.tv.focus;
      state.tv.focus = TvFocus::Rail(
        super::navigation::rail_actions(state)
          .iter()
          .position(|action| matches!(action, super::navigation::RailAction::Lists))
          .unwrap_or(0),
      );
      sync_pause(state);
      return Task::none();
    }
    _ => focus,
  };
  set_focus(state, next)
}

pub fn update(state: &mut State, message: Message) -> Task<AppMessage> {
  match message {
    Message::Scrolled(kind, offset) => {
      position_mut(&mut state.tv.lists, kind).offset = offset.max(0.0);
      Task::none()
    }
    Message::CloseMenu => {
      close_menu(state);
      Task::none()
    }
    Message::MenuFocus(row) => {
      if let Some(menu) = &mut state.tv.lists.menu {
        menu.row = row.min(1);
      }
      Task::none()
    }
    Message::MenuChoose {
      session,
      serial,
      row,
    } => {
      if !state.kernel.request_gate.is_current_session(session) {
        return Task::none();
      }
      let Some(menu) = &state.tv.lists.menu else {
        return Task::none();
      };
      if menu.serial != serial
        || menu.session != session
        || menu.destination != state.shell.destination
        || menu.pending.is_some()
        || item_actions::busy(&state.kernel, &menu.id)
      {
        return Task::none();
      }
      if row == 0
        && state
          .full
          .as_ref()
          .is_none_or(|full| !full.personal_lists.membership_loaded)
      {
        return Task::none();
      }
      let (id, item) = (menu.id.clone(), menu.item.clone());
      let watchlisted = state
        .full
        .as_ref()
        .is_some_and(|full| full.personal_lists.watchlist_ids.contains(&id));
      let action = if row == 0 {
        Action::Watchlist(!watchlisted)
      } else {
        let Some(item) = &item else {
          return Task::none();
        };
        Action::Favorite(!item.favorite)
      };
      if matches!(action, Action::Watchlist(true)) && item.is_none() {
        return Task::none();
      }
      if matches!(action, Action::Watchlist(false) | Action::Favorite(false)) {
        remember_removal(
          state,
          &id,
          if matches!(action, Action::Favorite(_)) {
            Kind::Favorites
          } else {
            Kind::Watchlist
          },
        );
      }
      if let Some(menu) = &mut state.tv.lists.menu {
        menu.pending = Some(action);
        menu.error = false;
      }
      let task = match action {
        Action::Favorite(value) => item_actions::start_server(
          state,
          id.clone(),
          if value {
            VideoUserDataAction::Favorite
          } else {
            VideoUserDataAction::Unfavorite
          },
          Origin::Other,
        ),
        Action::Watchlist(_) => match item {
          Some(item) => toggle_watchlist(state, item),
          None => item_actions::update(state, item_actions::Message::WatchlistRemove(id.clone())),
        },
        _ => Task::none(),
      };
      if !item_actions::busy(&state.kernel, &id) {
        if let Some(menu) = &mut state.tv.lists.menu {
          menu.pending = None;
          menu.error = true;
        }
      }
      task
    }
    Message::Undo { session, id } if state.kernel.request_gate.is_current_session(session) => {
      undo::update(state, undo::Message::Restore(id))
    }
    Message::Dismiss { session, id } if state.kernel.request_gate.is_current_session(session) => {
      let task = undo::update(state, undo::Message::Dismiss(id));
      if state.tv.focus == TvFocus::Lists(Focus::Undo(id)) {
        state.tv.focus = state.tv.content_focus;
        sync_pause(state);
      }
      task
    }
    Message::NoticeInteraction(id, interaction) => {
      state.kernel.undo.advance();
      state
        .kernel
        .undo
        .queue
        .set_paused(id, UndoPause::Hovered, interaction.hovered);
      state.kernel.undo.queue.set_paused(
        id,
        UndoPause::Focused,
        interaction.focused || state.tv.focus == TvFocus::Lists(Focus::Undo(id)),
      );
      Task::none()
    }
    Message::ViewAdded(session) if state.kernel.request_gate.is_current_session(session) => {
      view_added(state)
    }
    _ => Task::none(),
  }
}

fn remember_removal(state: &mut State, item_id: &str, kind: Kind) {
  let Some(page) = list_page(state, kind) else {
    return;
  };
  let Some(index) = page
    .entries
    .iter()
    .position(|entry| entry.id == item_id)
    .map(|index| page.offset + index)
  else {
    return;
  };
  state
    .tv
    .lists
    .removed
    .retain(|saved| saved.item_id != item_id || saved.kind != kind);
  state.tv.lists.removed.push(RemovalPosition {
    item_id: item_id.to_owned(),
    kind,
    index,
    destination: state.shell.destination.clone(),
  });
  if state.tv.lists.removed.len() > 3 {
    state.tv.lists.removed.remove(0);
  }
}

/// Runs only after the shared executor accepts the current account's receipt.
/// Data/counts have already been updated by the shared list projection.
pub(crate) fn item_settled(
  state: &mut State,
  receipt: &Receipt,
  origin: Origin,
  success: bool,
) -> Task<AppMessage> {
  if !state.tv_mode()
    || !state
      .kernel
      .request_gate
      .is_current_session(receipt.session())
  {
    return Task::none();
  }
  let same_menu = state.tv.lists.menu.as_ref().is_some_and(|menu| {
    menu.id == receipt.item_id()
      && menu.session == receipt.session()
      && menu.pending == Some(receipt.action())
  });
  if !success {
    if same_menu {
      if let Some(menu) = &mut state.tv.lists.menu {
        menu.pending = None;
        menu.error = true;
      }
    }
    return Task::none();
  }
  let (kind, added) = match receipt.action() {
    Action::Favorite(value) => (Kind::Favorites, value),
    Action::Watchlist(value) => (Kind::Watchlist, value),
    _ => return Task::none(),
  };
  if same_menu {
    close_menu(state);
  }
  if matches!(origin, Origin::Undo(_)) {
    if matches!(state.shell.destination, Destination::PersonalLists(_)) && kind == self::kind(state)
    {
      let index = state
        .tv
        .lists
        .removed
        .iter()
        .find(|saved| saved.item_id == receipt.item_id() && saved.kind == kind)
        .map_or(0, |saved| saved.index);
      state.tv.lists.reveal = Some(Reveal {
        kind,
        item_id: receipt.item_id().to_owned(),
      });
      return set_focus(state, Focus::Card(index));
    }
  } else if added {
    state.tv.lists.added = Some(Added {
      kind,
      item_id: receipt.item_id().to_owned(),
      destination: state.shell.destination.clone(),
    });
  } else if matches!(state.shell.destination, Destination::PersonalLists(_))
    && kind == self::kind(state)
  {
    let Some(removed) = state.tv.lists.removed.iter().find(|saved| {
      saved.item_id == receipt.item_id()
        && saved.kind == kind
        && saved.destination == state.shell.destination
    }) else {
      return Task::none();
    };
    let index = removed.index;
    if !same_menu && state.tv.focus != TvFocus::Lists(Focus::Card(index)) {
      return Task::none();
    }
    position_mut(&mut state.tv.lists, kind).item_id = None;
    let total = list_page(state, kind).map_or(0, |page| page.total);
    return set_focus(
      state,
      if total == 0 {
        Focus::Empty
      } else {
        Focus::Card(index.min(total - 1))
      },
    );
  }
  Task::none()
}

fn view_added(state: &mut State) -> Task<AppMessage> {
  let Some(added) = state.tv.lists.added.take() else {
    return Task::none();
  };
  let index = state
    .full
    .as_ref()
    .and_then(|full| personal_lists::watchlist_index(&full.personal_lists, &added.item_id))
    .filter(|_| added.kind == Kind::Watchlist)
    .unwrap_or(0);
  state.tv.lists.reveal = Some(Reveal {
    kind: added.kind,
    item_id: added.item_id,
  });
  position_mut(&mut state.tv.lists, added.kind).index = index;
  let task = crate::app::update::route_message(
    state,
    AppMessage::Home(HomeMessage::Navigate(Destination::PersonalLists(route(
      added.kind,
    )))),
  );
  Task::batch([task, set_focus(state, Focus::Card(index))])
}

pub fn reconcile(state: &mut State) -> Task<AppMessage> {
  let session = state.kernel.request_gate.current_session();
  if state.tv.lists.session != Some(session) {
    leave(state);
    state.tv.lists = Surface {
      session: Some(session),
      ..Surface::default()
    };
  }
  if state
    .tv
    .lists
    .menu
    .as_ref()
    .is_some_and(|menu| menu.destination != state.shell.destination)
  {
    state.tv.lists.menu = None;
  }
  if state
    .tv
    .lists
    .added
    .as_ref()
    .is_some_and(|added| added.destination != state.shell.destination)
  {
    state.tv.lists.added = None;
  }
  if matches!(state.tv.focus, TvFocus::Lists(Focus::Undo(id)) if !state.kernel.undo.notices.contains_key(&id))
    || (state.tv.focus == TvFocus::Lists(Focus::Added) && state.tv.lists.added.is_none())
  {
    state.tv.focus = state.tv.content_focus;
  }
  sync_pause(state);
  if !matches!(state.shell.destination, Destination::PersonalLists(_)) {
    state.tv.lists.presented = None;
    state.tv.lists.reveal = None;
    return Task::none();
  }
  let kind = kind(state);
  if state
    .tv
    .lists
    .reveal
    .as_ref()
    .is_some_and(|reveal| reveal.kind != kind)
  {
    state.tv.lists.reveal = None;
  }
  let entering = state.tv.lists.presented != Some(route(kind));
  state.tv.lists.presented = Some(route(kind));
  if state.tv.lists.switch_to == Some(kind) {
    state.tv.lists.switch_to = None;
    state.tv.focus = TvFocus::Lists(body_focus(state, kind));
  }
  let mut tasks = Vec::new();
  if !state.tv.lists.initialized {
    state.tv.lists.initialized = true;
    if let Some(full) = state.full.as_mut() {
      let other = if kind == Kind::Favorites {
        Route::Watchlist
      } else {
        Route::Favorites
      };
      tasks.push(personal_lists::start(
        &mut full.personal_lists,
        &mut state.kernel,
        &state.watchlist,
        other,
      ));
    }
  }
  let Some(page) = list_page(state, kind) else {
    return Task::batch(tasks);
  };
  if let Some(reveal) = &state.tv.lists.reveal {
    if reveal.kind == kind && !page.loading && page.error.is_none() {
      if let Some(index) = page
        .entries
        .iter()
        .position(|entry| entry.id == reveal.item_id)
        .map(|index| index + page.offset)
      {
        state.tv.lists.reveal = None;
        tasks.push(set_focus(state, Focus::Card(index)));
        return Task::batch(tasks);
      }
      let next = page.offset + PAGE_SIZE;
      if next < page.total {
        tasks.push(load_range(state, kind, next..next + 1));
        return Task::batch(tasks);
      }
      state.tv.lists.reveal = None;
    } else if reveal.kind == kind {
      return Task::batch(tasks);
    }
  }
  let Some(page) = list_page(state, kind) else {
    return Task::batch(tasks);
  };
  let next = match state.tv.focus {
    TvFocus::Lists(Focus::Card(index)) if page.total > 0 => {
      let relocated = position(&state.tv.lists, kind)
        .item_id
        .as_ref()
        .and_then(|id| page.entries.iter().position(|entry| &entry.id == id))
        .map(|index| index + page.offset);
      Focus::Card(relocated.unwrap_or(index).min(page.total - 1))
    }
    TvFocus::Lists(Focus::Card(_)) if !page.loading => body_focus(state, kind),
    TvFocus::Lists(Focus::Undo(id)) if !state.kernel.undo.notices.contains_key(&id) => {
      body_focus(state, kind)
    }
    TvFocus::Lists(focus) => focus,
    TvFocus::Rail(_) => return Task::batch(tasks),
    _ => body_focus(state, kind),
  };
  if state.tv.focus != TvFocus::Lists(next) {
    tasks.push(set_focus(state, next));
  }
  if let Focus::Card(index) = next {
    if let Some(entry) = entry(state, index) {
      position_mut(&mut state.tv.lists, kind).item_id = Some(entry.id.clone());
    }
    tasks.push(window(state, kind, index));
    if entering {
      tasks.push(scroll(state, kind, index));
    }
  }
  Task::batch(tasks)
}

fn dispatch(message: Message) -> AppMessage {
  AppMessage::Tv(super::Message::Lists(message))
}

pub fn view(state: &State, width: f32) -> Element<'_, AppMessage> {
  let scale = style::scale(state.shell.window_size.width);
  let kind = kind(state);
  let tabs = [Kind::Watchlist, Kind::Favorites].into_iter().fold(
    Row::new().spacing(16.0 * scale),
    |row, candidate| {
      let count = list_page(state, candidate)
        .and_then(|page| page.known_total)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "—".to_owned());
      row.push(super::view::action(
        state,
        TvFocus::Lists(Focus::Tab(candidate)),
        format!("{}   {count}", title(state, candidate)),
        None,
        candidate == kind,
        scale,
      ))
    },
  );
  let mut content = column![
    super::view::page_title(state.t("tv-lists-title"), scale),
    tabs
  ]
  .spacing(28.0 * scale)
  .width(width);
  let Some(page) = list_page(state, kind) else {
    return content.into();
  };
  if let Some(error) = &page.error {
    content = content.push(
      row![
        text(state.kernel.locale.message(error))
          .size(style::META * scale)
          .color(style::PALETTE.colors.error),
        super::view::action(
          state,
          TvFocus::Lists(Focus::Retry),
          state.t("home-retry"),
          None,
          false,
          scale
        )
      ]
      .spacing(16.0 * scale),
    );
  }
  if page.total == 0 {
    let body = if page.loading {
      column![text(state.t("tv-loading")).size(style::BODY * scale)]
    } else if page.error.is_some() {
      column![]
    } else {
      column![
        text(state.t("tv-lists-empty-title"))
          .size(style::SECTION * scale)
          .font(HEADING_FONT),
        text(state.t("tv-lists-empty-body"))
          .size(style::BODY * scale)
          .color(style::PALETTE.text.metadata),
        super::view::action(
          state,
          TvFocus::Lists(Focus::Empty),
          state.t("tv-lists-browse"),
          Some(Icon::Grid),
          false,
          scale
        )
      ]
      .spacing(24.0 * scale)
      .align_x(Alignment::Center)
    };
    return content
      .push(
        container(body)
          .width(Fill)
          .height(620.0 * scale)
          .center_x(Fill)
          .center_y(Fill),
      )
      .push(
        text(super::view::context_hint(state))
          .size(style::META * scale)
          .color(style::PALETTE.text.metadata),
      )
      .into();
  }
  if page.loading && page.entries.is_empty() {
    content = content.push(text(state.t("tv-loading")).size(style::BODY * scale));
  }
  let carousel = super::view::horizontal_shelf(
    state,
    scroll_id(kind),
    640.0 * scale,
    move || carousel_cards(state, kind, scale),
    move |offset| dispatch(Message::Scrolled(kind, offset)),
  );
  content
    .push(carousel)
    .push(
      text(super::view::context_hint(state))
        .size(style::META * scale)
        .color(style::PALETTE.text.metadata),
    )
    .into()
}

fn carousel_cards(state: &State, kind: Kind, scale: f32) -> Element<'_, AppMessage> {
  let Some(full) = state.full.as_ref() else {
    return space().into();
  };
  let Some(page) = list_page(state, kind) else {
    return space().into();
  };
  let pitch = 344.0 * scale;
  let card_width = 320.0 * scale;
  let mut cards = Row::new().spacing(24.0 * scale);
  if page.offset > 0 {
    cards = cards.push(space().width((page.offset as f32 * pitch - 24.0 * scale).max(0.0)));
  }
  for (local, entry) in page.entries.iter().enumerate() {
    let focus = TvFocus::Lists(Focus::Card(page.offset + local));
    let card = if let Some(item) = &entry.item {
      let spec = personal_lists::tv_artwork_spec(kind, entry);
      super::view::media_card(
        state,
        item,
        focus,
        (&full.personal_lists.artwork, ArtworkSurface::PersonalLists),
        spec,
        (card_width, false),
        scale,
      )
    } else {
      column![
        container(space())
          .width(card_width)
          .height(480.0 * scale)
          .style(style::panel),
        super::view::action(state, focus, entry.name.clone(), None, false, scale).width(card_width),
        text(state.t(if page.loading {
          "tv-loading"
        } else {
          "tv-lists-unavailable"
        }))
        .size(style::META * scale)
      ]
      .spacing(12.0 * scale)
      .into()
    };
    cards = cards.push(card);
  }
  let remaining = page.total.saturating_sub(page.offset + page.entries.len());
  if remaining > 0 {
    cards = cards.push(space().width((remaining as f32 * pitch - 24.0 * scale).max(0.0)));
  }
  cards.into()
}

fn title(state: &State, kind: Kind) -> String {
  state.t(if kind == Kind::Favorites {
    "tv-lists-favorites"
  } else {
    "tv-lists-watchlist"
  })
}

fn control<'a>(
  _state: &'a State,
  label: String,
  focused: bool,
  enabled: bool,
  message: AppMessage,
  scale: f32,
) -> Element<'a, AppMessage> {
  tv_focus::focus(focused, move |progress| {
    button(
      container(
        text(label.clone())
          .size(style::BODY * scale)
          .font(HEADING_FONT),
      )
      .padding([12.0 * scale, 20.0 * scale])
      .center_y(64.0 * scale),
    )
    .padding(0)
    .style(style::button_progress(style::PALETTE, progress, false))
    .on_press_maybe(enabled.then_some(message.clone()))
    .into()
  })
  .into()
}

pub fn overlay(state: &State) -> Option<Element<'_, AppMessage>> {
  let scale = style::scale(state.shell.window_size.width);
  let session = state.kernel.request_gate.current_session();
  if let Some(menu) = &state.tv.lists.menu {
    let watchlisted = state
      .full
      .as_ref()
      .is_some_and(|full| full.personal_lists.watchlist_ids.contains(&menu.id));
    let favorite = menu.item.as_ref().is_some_and(|item| item.favorite);
    let mut actions = column![text(&menu.name)
      .size(style::SECTION * scale)
      .font(HEADING_FONT)]
    .spacing(24.0 * scale);
    for (row, label) in [
      (
        0,
        state.t(if watchlisted {
          "tv-lists-remove-watchlist"
        } else {
          "tv-lists-add-watchlist"
        }),
      ),
      (
        1,
        state.t(if favorite {
          "tv-lists-remove-favorites"
        } else {
          "tv-lists-add-favorites"
        }),
      ),
    ] {
      let enabled = menu.pending.is_none()
        && (menu.item.is_some() || (row == 0 && watchlisted))
        && (row != 0
          || state
            .full
            .as_ref()
            .is_some_and(|full| full.personal_lists.membership_loaded));
      let message = dispatch(Message::MenuChoose {
        session,
        serial: menu.serial,
        row,
      });
      let action: Element<'_, AppMessage> = tv_focus::focus(menu.row == row, move |progress| {
        let icon = if row == 0 {
          Icon::Bookmark
        } else {
          Icon::Heart
        };
        let content = row![
          icon_with_color(
            icon,
            IconSize::Custom(28.0 * scale),
            style::foreground(style::PALETTE, progress, false)
          ),
          text(label.clone())
            .size(style::BODY * scale)
            .font(HEADING_FONT)
        ]
        .spacing(16.0 * scale)
        .align_y(Alignment::Center);
        button(
          container(content)
            .padding([12.0 * scale, 20.0 * scale])
            .width(Fill)
            .height(72.0 * scale)
            .align_y(Alignment::Center),
        )
        .padding(0)
        .width(Fill)
        .style(style::button_progress(style::PALETTE, progress, false))
        .on_press_maybe(enabled.then_some(message.clone()))
        .into()
      })
      .into();
      actions = actions.push(
        mouse_area(container(action).width(Fill)).on_enter(dispatch(Message::MenuFocus(row))),
      );
    }
    if menu.error {
      actions = actions.push(
        text(state.t("tv-lists-action-failed"))
          .size(style::META * scale)
          .color(style::PALETTE.colors.error),
      );
    }
    if menu.pending.is_some() {
      actions = actions.push(text(state.t("common-loading")).size(style::META * scale));
    }
    actions = actions.push(control(
      state,
      state.t("common-close"),
      false,
      true,
      dispatch(Message::CloseMenu),
      scale,
    ));
    let panel = container(actions)
      .width(520.0 * scale)
      .padding(32.0 * scale)
      .style(style::panel);
    let shade = mouse_area(
      container(space())
        .width(Fill)
        .height(Fill)
        .style(style::scrim),
    )
    .on_press(dispatch(Message::CloseMenu));
    let positioned = container(panel)
      .width(Fill)
      .height(Fill)
      .align_x(Alignment::End)
      .padding(iced::Padding {
        top: 288.0 * scale,
        right: 96.0 * scale,
        bottom: 60.0 * scale,
        left: 0.0,
      });
    return Some(stack![shade, positioned].into());
  }
  let mut notices = Column::new().spacing(12.0 * scale).width(960.0 * scale);
  let mut any = false;
  for id in state.kernel.undo.queue.visible() {
    let Some(notice) = state.kernel.undo.notices.get(&id) else {
      continue;
    };
    any = true;
    let mut copy = column![
      text(state.kernel.locale.message(&notice.removal.message())).size(style::BODY * scale)
    ];
    if notice.failed {
      copy = copy.push(
        text(state.t("lists-undo-failed"))
          .size(style::META * scale)
          .color(style::PALETTE.colors.error),
      );
    }
    let row = row![
      copy.width(Fill),
      control(
        state,
        state.t(if notice.pending {
          "common-loading"
        } else if notice.failed {
          "lists-retry-undo"
        } else {
          "lists-undo"
        }),
        state.tv.focus == TvFocus::Lists(Focus::Undo(id)),
        !notice.pending,
        dispatch(Message::Undo { session, id }),
        scale
      ),
      control(
        state,
        state.t("common-close"),
        false,
        !notice.pending,
        dispatch(Message::Dismiss { session, id }),
        scale
      )
    ]
    .spacing(16.0 * scale)
    .align_y(Alignment::Center);
    notices = notices.push(notice_interaction(
      container(row)
        .padding([8.0 * scale, 24.0 * scale])
        .style(style::panel),
      move |interaction| dispatch(Message::NoticeInteraction(id, interaction)),
    ));
  }
  if let Some(added) = &state.tv.lists.added {
    any = true;
    let message = state.t(if added.kind == Kind::Favorites {
      "tv-lists-added-favorite"
    } else {
      "tv-lists-added-watchlist"
    });
    notices = notices.push(
      container(
        row![
          text(message).size(style::BODY * scale).width(Fill),
          control(
            state,
            state.t("tv-lists-view"),
            state.tv.focus == TvFocus::Lists(Focus::Added),
            true,
            dispatch(Message::ViewAdded(session)),
            scale
          )
        ]
        .spacing(24.0 * scale)
        .align_y(Alignment::Center),
      )
      .padding([16.0 * scale, 24.0 * scale])
      .style(style::panel),
    );
  }
  any.then(|| {
    container(notices)
      .width(Fill)
      .height(Fill)
      .align_x(Alignment::Center)
      .align_y(Alignment::End)
      .padding(iced::Padding {
        bottom: 112.0 * scale,
        left: (style::RAIL + style::CONTENT_INSET) * scale,
        right: style::SAFE_X * scale,
        top: style::SAFE_Y * scale,
      })
      .into()
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app::test_support::{BrowseFixture, FixtureReply};
  use iced::futures::StreamExt;
  use jellypilot_core::config::UiMode;
  use jellypilot_core::watchlist::{WatchlistRecord, WatchlistStore};
  use std::time::{Duration, SystemTime, UNIX_EPOCH};

  fn item(id: &str) -> VideoLibraryItem {
    VideoLibraryItem {
      id: id.to_owned(),
      name: id.to_owned(),
      item_type: "Movie".to_owned(),
      premiere_date: None,
      production_year: Some(2026),
      runtime_seconds: None,
      community_rating: None,
      episode_count: None,
      last_played_date: None,
      played: false,
      favorite: true,
      artwork_image_id: None,
      backdrop_image_id: None,
      logo_image_id: None,
      series_poster_image_id: None,
      episode_thumb_image_id: None,
      series_thumb_image_id: None,
      series_backdrop_image_id: None,
      season_poster_image_id: None,
      season_number: None,
      episode_number: None,
      index_number_end: None,
      series_id: None,
      series_name: None,
      end_year: None,
      series_continuing: false,
      unplayed_item_count: None,
      resume_position_seconds: None,
      played_percentage: None,
      overview: None,
    }
  }

  fn state(fixture: &BrowseFixture) -> State {
    let mut state = State::boot(true);
    state.full = Some(crate::app::state::FullUi::default());
    state.shell.ui_mode = UiMode::Tv;
    state.shell.destination = Destination::PersonalLists(Route::Watchlist);
    state.kernel.connection = jellypilot_auth::login::ConnectionPhase::Connected;
    state.kernel.client = Some(fixture.client());
    state.tv.lists.session = Some(state.kernel.request_gate.current_session());
    state.tv.lists.initialized = true;
    state
  }

  fn watchlist(state: &mut State, ids: &[&str]) -> std::path::PathBuf {
    let scope = personal_lists::active_scope(&state.kernel).expect("scope");
    let path = std::env::temp_dir().join(format!(
      "jellypilot-tv-lists-{}-{}.json",
      std::process::id(),
      SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos()
    ));
    let mut store = WatchlistStore::for_test(path.clone()).expect("store");
    for (index, id) in ids.iter().enumerate() {
      store
        .add(
          WatchlistRecord::from_item(scope.clone(), &item(id), (ids.len() - index) as u64)
            .expect("record"),
        )
        .expect("save");
    }
    let records = store.records_for(&scope);
    state.watchlist = personal_lists::Runtime::for_test(store);
    let full = state.full.as_mut().expect("full");
    drop(personal_lists::start(
      &mut full.personal_lists,
      &mut state.kernel,
      &state.watchlist,
      Route::Watchlist,
    ));
    drop(personal_lists::apply_watchlist_snapshot(
      &mut full.personal_lists,
      &mut state.kernel,
      &state.watchlist,
      &scope,
      0,
      records,
    ));
    for entry in &mut full.personal_lists.watchlist.entries {
      entry.item = Some(item(&entry.id));
    }
    full.personal_lists.watchlist.loading = false;
    full.personal_lists.favorites.entries = ids
      .iter()
      .map(|id| personal_lists::entry_from_item(item(id)))
      .collect();
    full.personal_lists.favorites.total = ids.len();
    full.personal_lists.favorites.known_total = Some(ids.len());
    path
  }

  async fn settle_write(state: &mut State, task: Task<AppMessage>) {
    if let Some(mut stream) = iced_runtime::task::into_stream(task) {
      while let Some(action) = stream.next().await {
        if let iced_runtime::Action::Output(AppMessage::ItemActions(message)) = action {
          drop(item_actions::update(state, message));
        }
      }
    }
  }

  #[tokio::test]
  async fn watchlist_removal_and_undo_use_real_store_preserving_other_collection_order_and_focus() {
    let fixture = BrowseFixture::new();
    let mut state = state(&fixture);
    watchlist(&mut state, &["first", "middle", "last"]);
    drop(set_focus(&mut state, Focus::Card(1)));
    drop(open_focused_menu(&mut state));
    let task = input(&mut state, Input::Confirm);
    settle_write(&mut state, task).await;
    assert!(!menu_open(&state));
    let lists = &state.full.as_ref().expect("full").personal_lists;
    assert_eq!(
      lists
        .watchlist
        .entries
        .iter()
        .map(|entry| entry.id.as_str())
        .collect::<Vec<_>>(),
      ["first", "last"]
    );
    assert_eq!(lists.watchlist.known_total, Some(2));
    assert_eq!(lists.favorites.total, 3);
    assert_eq!(state.tv.focus, TvFocus::Lists(Focus::Card(1)));
    let notice = state.kernel.undo.queue.visible().next().expect("undo");
    assert!(focus_feedback(&mut state));
    let task = input(&mut state, Input::Confirm);
    settle_write(&mut state, task).await;
    let lists = &state.full.as_ref().expect("full").personal_lists;
    assert_eq!(
      lists
        .watchlist
        .entries
        .iter()
        .map(|entry| entry.id.as_str())
        .collect::<Vec<_>>(),
      ["first", "middle", "last"]
    );
    assert_eq!(lists.watchlist.known_total, Some(3));
    assert_eq!(lists.favorites.total, 3);
    assert_eq!(state.tv.focus, TvFocus::Lists(Focus::Card(1)));
    assert!(!state.kernel.undo.notices.contains_key(&notice));
  }

  #[tokio::test]
  async fn cancelled_menu_and_stale_account_activation_cannot_remove_an_item() {
    let fixture = BrowseFixture::new();
    let mut state = state(&fixture);
    watchlist(&mut state, &["retained"]);
    drop(set_focus(&mut state, Focus::Card(0)));
    drop(open_focused_menu(&mut state));
    let menu = state.tv.lists.menu.as_ref().expect("menu");
    let old = Message::MenuChoose {
      session: menu.session,
      serial: menu.serial,
      row: 0,
    };
    drop(input(&mut state, Input::Back));
    let task = update(&mut state, old.clone());
    settle_write(&mut state, task).await;
    assert_eq!(list_page(&state, Kind::Watchlist).expect("page").total, 1);
    drop(open_focused_menu(&mut state));
    state.kernel.request_gate.disconnect();
    let task = update(&mut state, old);
    settle_write(&mut state, task).await;
    assert_eq!(list_page(&state, Kind::Watchlist).expect("page").total, 1);
    assert!(state.kernel.undo.notices.is_empty());
  }

  #[tokio::test]
  async fn failed_watchlist_write_keeps_count_and_menu_then_retry_can_remove_last_item() {
    let fixture = BrowseFixture::new();
    let mut state = state(&fixture);
    let path = watchlist(&mut state, &["retained"]);
    let blocked = path.with_extension("json.tmp");
    std::fs::create_dir(&blocked).expect("block atomic replacement");
    drop(set_focus(&mut state, Focus::Card(0)));
    drop(open_focused_menu(&mut state));
    let task = input(&mut state, Input::Confirm);
    settle_write(&mut state, task).await;
    assert!(state.tv.lists.menu.as_ref().expect("menu remains").error);
    assert_eq!(
      list_page(&state, Kind::Watchlist)
        .expect("page")
        .known_total,
      Some(1)
    );
    assert!(state.kernel.undo.notices.is_empty());
    std::fs::remove_dir(blocked).expect("release test failure");
    let task = input(&mut state, Input::Confirm);
    settle_write(&mut state, task).await;
    assert_eq!(
      list_page(&state, Kind::Watchlist)
        .expect("page")
        .known_total,
      Some(0)
    );
    assert_eq!(state.tv.focus, TvFocus::Lists(Focus::Empty));
    assert_eq!(state.kernel.undo.notices.len(), 1);
    assert!(focus_feedback(&mut state));
  }

  #[test]
  fn focused_undo_pauses_the_shared_expiry_and_leaving_releases_it() {
    let fixture = BrowseFixture::new();
    let mut state = state(&fixture);
    state.kernel.undo.queue.push(9);
    state.kernel.undo.notices.insert(
      9,
      undo::Notice {
        removal: undo::Removal::Favorite {
          item_id: "one".to_owned(),
          name: "One".to_owned(),
        },
        pending: false,
        failed: false,
      },
    );
    assert!(focus_feedback(&mut state));
    assert!(state
      .kernel
      .undo
      .queue
      .advance(Duration::from_secs(60))
      .is_empty());
    leave(&mut state);
    assert_eq!(
      state.kernel.undo.queue.advance(Duration::from_secs(69)),
      [9]
    );
  }

  #[tokio::test]
  async fn horizontal_navigation_crosses_bounded_pages_without_changing_global_count_or_focus() {
    let mut fixture = BrowseFixture::new();
    let mut state = state(&fixture);
    state.shell.destination = Destination::PersonalLists(Route::Favorites);
    let full = state.full.as_mut().expect("full");
    let task = personal_lists::start(
      &mut full.personal_lists,
      &mut state.kernel,
      &state.watchlist,
      Route::Favorites,
    );
    let completions = fixture
      .run_task(task, |_| FixtureReply::Page {
        total: 60,
        artwork: false,
      })
      .await;
    for completion in completions {
      drop(crate::app::update::route_message(&mut state, completion));
    }
    for index in [23, 24, 10, 0] {
      let before = list_page(&state, Kind::Favorites)
        .expect("page")
        .entries
        .iter()
        .map(|entry| entry.id.clone())
        .collect::<Vec<_>>();
      let task = set_focus(&mut state, Focus::Card(index));
      // Starting another read never blanks the still-visible previous window.
      assert_eq!(
        list_page(&state, Kind::Favorites)
          .expect("page")
          .entries
          .iter()
          .map(|entry| entry.id.clone())
          .collect::<Vec<_>>(),
        before
      );
      let completions = fixture
        .run_task(task, |_| FixtureReply::Page {
          total: 60,
          artwork: false,
        })
        .await;
      for completion in completions {
        drop(crate::app::update::route_message(&mut state, completion));
      }
      drop(reconcile(&mut state));
      assert_eq!(state.tv.focus, TvFocus::Lists(Focus::Card(index)));
      let visible = visible_range(&state, Kind::Favorites, index);
      let page = list_page(&state, Kind::Favorites).expect("page");
      assert!(
        page.offset <= visible.start,
        "missing left visible cards at {index}: {visible:?}, offset {}",
        page.offset
      );
      assert!(
        page.offset + page.entries.len() >= visible.end,
        "missing right peek at {index}: {visible:?}, window {}..{}",
        page.offset,
        page.offset + page.entries.len()
      );
      assert!(page.entries.len() <= PAGE_SIZE);
      assert_eq!(page.known_total, Some(60));
    }
  }
}
