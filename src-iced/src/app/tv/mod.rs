//! TV presentation: one focus owner, shared browse/detail/playback services.

pub(crate) mod account;
mod detail;
pub(crate) mod filters;
mod input;
pub(crate) mod lists;
mod navigation;
pub(crate) mod player;
pub(crate) mod saved_browse;
pub(crate) mod search;
pub(crate) mod settings;
pub(crate) mod text_entry;
mod view;

pub use input::keyboard;
pub use view::view;

use iced::{Subscription, Task};
use jellypilot_core::config::UiMode;
use jellypilot_core::tv_navigation::Input;

use super::message::Message as AppMessage;
use super::state::{Destination, State};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Focus {
  Rail(usize),
  #[default]
  HeroPlay,
  HeroDetail,
  HeroWatchlist,
  Shelf {
    row: usize,
    item: usize,
  },
  Header(usize),
  Grid(u32),
  DetailBack,
  DetailPlay,
  DetailWatchlist,
  DetailFavorite,
  DetailMenu,
  DetailOverview,
  DetailEpisodesMore,
  DetailEpisodesRetry,
  Lists(lists::Focus),
  SavedBrowse(saved_browse::Focus),
  Season(usize),
  Episode(usize),
  Retry,
}

#[derive(Clone)]
pub enum Message {
  Input(Input),
  Player(player::Message),
  Account(account::Message),
  Lists(lists::Message),
  Settings(settings::Message),
  Search(search::Message),
  Filters(filters::Message),
  SavedBrowse(saved_browse::Message),
  ConfirmPressed,
  ConfirmReleased,
  ConfirmHeld(u64),
  CancelPress,
  OpenMenu,
  ClockTick,
  Focus(Focus),
  Activate(Focus),
  Scrolled(f32),
  Revealed {
    focus: Focus,
    destination: Destination,
    session: jellypilot_core::request_gate::SessionToken,
    y: f32,
    horizontal: Option<(iced::widget::Id, f32)>,
  },
  HorizontalScrolled(iced::widget::Id, f32),
  OverviewMoved {
    destination: Destination,
    session: jellypilot_core::request_gate::SessionToken,
    direction: Input,
    offset: Option<f32>,
  },
}

#[derive(Default)]
pub struct Surface {
  pub player: player::Surface,
  pub account: account::Surface,
  pub lists: lists::Surface,
  pub settings: settings::Surface,
  pub search: search::Surface,
  pub filters: filters::Surface,
  pub saved_browse: saved_browse::Surface,
  pub focus: Focus,
  content_focus: Focus,
  pub offset: f32,
  destination: Option<Destination>,
  memory: Vec<Memory>,
  horizontal: Vec<(iced::widget::Id, f32)>,
  was_playing: bool,
  focused_item: Option<String>,
  press: Option<ConfirmPress>,
  next_press: u64,
  detail_page: Option<(String, Option<String>, usize)>,
  detail_overview_expanded: bool,
}

struct ConfirmPress {
  token: u64,
  focus: Focus,
  destination: Destination,
  session: jellypilot_core::request_gate::SessionToken,
}

pub(super) fn browse_focus_visible(state: &State) -> bool {
  !player::upcoming::is_open(state)
    && !saved_browse::overlay_open(state)
    && !filters::is_open(state)
    && !state.tv.settings.open
    && !state.tv.search.open
    && !account::modal_open(state)
    && !lists::menu_open(state)
}

fn context_target(state: &State) -> bool {
  matches!(state.tv.focus, Focus::Lists(lists::Focus::Card(_)))
    || (matches!(
      state.tv.focus,
      Focus::Grid(_) | Focus::Shelf { .. } | Focus::Episode(_)
    ) && navigation::focused_item(state).is_some())
}

fn open_menu(state: &mut State) -> Task<AppMessage> {
  if matches!(state.shell.destination, Destination::PersonalLists(_)) {
    lists::open_focused_menu(state)
  } else if let Some(item) = navigation::focused_item(state).cloned() {
    lists::open_menu(state, item)
  } else {
    Task::none()
  }
}

fn route_input(state: &mut State, input: Input) -> Task<AppMessage> {
  if saved_browse::overlay_open(state) {
    return saved_browse::input(state, input);
  }
  if player::upcoming::is_open(state) {
    return player::upcoming::input(state, input);
  }
  if filters::is_open(state) {
    return filters::input(state, input);
  }
  if state.tv.search.open {
    return search::input(state, input);
  }
  if state.tv.settings.open {
    return settings::input(state, input);
  }
  if lists::menu_open(state) {
    return lists::input(state, input);
  }
  if player::active(state) {
    return player::input(state, input);
  }
  if matches!(state.tv.focus, Focus::Lists(_)) {
    return lists::input(state, input);
  }
  if matches!(state.shell.destination, Destination::SavedBrowse)
    && !matches!(state.tv.focus, Focus::Rail(_))
  {
    return saved_browse::input(state, input);
  }
  if input == Input::Down && lists::focus_feedback(state) {
    return Task::none();
  }
  navigation::input(state, input)
}

struct Memory {
  destination: Destination,
  focus: Focus,
  offset: f32,
  horizontal: Vec<(iced::widget::Id, f32)>,
  focused_item: Option<String>,
}

fn remember_horizontal(state: &mut State, id: iced::widget::Id, offset: f32) {
  if let Some((_, saved)) = state
    .tv
    .horizontal
    .iter_mut()
    .find(|(saved, _)| saved == &id)
  {
    *saved = offset;
  } else {
    state.tv.horizontal.push((id, offset));
  }
}

pub fn update(state: &mut State, message: Message) -> Task<AppMessage> {
  if let Message::Account(message) = message {
    return account::update(state, message);
  }
  if state.tv_mode()
    && (account::modal_open(state)
      || state.kernel.connection != jellypilot_auth::login::ConnectionPhase::Connected)
  {
    state.tv.press = None;
    return match message {
      Message::Input(input) => account::input(state, input),
      Message::ConfirmPressed => account::input(state, Input::Confirm),
      _ => Task::none(),
    };
  }
  if let Message::Player(message) = message {
    return player::update(state, message);
  }
  if !state.tv_mode()
    || state.kernel.connection != jellypilot_auth::login::ConnectionPhase::Connected
    || state.shell.quit_requested
    || super::accounts::content_mutations_blocked(&state.kernel)
    || super::accounts::blocking_modal(&state.accounts)
    || state.shell.pending_close.is_some()
    || state.shell.settings_open
    || state.shell.account_popover_open
  {
    return Task::none();
  }
  match message {
    Message::Player(_) | Message::Account(_) => Task::none(),
    Message::ClockTick => Task::none(),
    Message::Lists(message) => lists::update(state, message),
    Message::Settings(message) => settings::update(state, message),
    Message::Search(message) => search::update(state, message),
    Message::Filters(message) => filters::update(state, message),
    Message::SavedBrowse(message) => saved_browse::update(state, message),
    Message::Input(input) => {
      state.tv.press = None;
      route_input(state, input)
    }
    Message::CancelPress => {
      state.tv.press = None;
      Task::none()
    }
    Message::ConfirmPressed => {
      if state.tv.press.is_some() {
        return Task::none();
      }
      if !browse_focus_visible(state) || player::active(state) || !context_target(state) {
        return route_input(state, Input::Confirm);
      }
      state.tv.next_press = state.tv.next_press.wrapping_add(1);
      let token = state.tv.next_press;
      state.tv.press = Some(ConfirmPress {
        token,
        focus: state.tv.focus,
        destination: state.shell.destination.clone(),
        session: state.kernel.request_gate.current_session(),
      });
      Task::perform(
        async move {
          tokio::time::sleep(std::time::Duration::from_millis(650)).await;
          token
        },
        |token| AppMessage::Tv(Message::ConfirmHeld(token)),
      )
    }
    Message::ConfirmReleased => {
      let Some(press) = state.tv.press.take() else {
        return Task::none();
      };
      if browse_focus_visible(state)
        && !player::active(state)
        && press.focus == state.tv.focus
        && press.destination == state.shell.destination
        && press.session == state.kernel.request_gate.current_session()
      {
        route_input(state, Input::Confirm)
      } else {
        Task::none()
      }
    }
    Message::ConfirmHeld(token) => {
      let current = state.tv.press.as_ref().is_some_and(|press| {
        press.token == token
          && press.focus == state.tv.focus
          && press.destination == state.shell.destination
          && press.session == state.kernel.request_gate.current_session()
      });
      if current && browse_focus_visible(state) && !player::active(state) {
        state.tv.press = None;
        open_menu(state)
      } else {
        Task::none()
      }
    }
    Message::OpenMenu => {
      state.tv.press = None;
      if browse_focus_visible(state) && !player::active(state) {
        open_menu(state)
      } else {
        Task::none()
      }
    }
    Message::Focus(focus) => {
      if !browse_focus_visible(state) {
        return Task::none();
      }
      state.tv.focus = focus;
      state.tv.focused_item = None;
      Task::none()
    }
    Message::Activate(focus) => {
      if !browse_focus_visible(state) {
        return Task::none();
      }
      state.tv.focus = focus;
      state.tv.focused_item = None;
      navigation::activate(state, focus)
    }
    Message::Scrolled(offset) => {
      state.tv.offset = offset;
      Task::none()
    }
    Message::Revealed {
      focus,
      destination,
      session,
      y,
      horizontal,
    } => {
      if focus != state.tv.focus
        || destination != state.shell.destination
        || session != state.kernel.request_gate.current_session()
      {
        return Task::none();
      }
      state.tv.offset = y;
      if let Some((id, offset)) = &horizontal {
        remember_horizontal(state, id.clone(), *offset);
      }
      let horizontal = horizontal.map_or_else(Task::none, |(id, x)| {
        iced::widget::operation::scroll_to(
          id,
          iced::widget::operation::AbsoluteOffset { x, y: 0.0 },
        )
      });
      Task::batch([navigation::restore_scroll(state), horizontal])
    }
    Message::HorizontalScrolled(id, offset) => {
      remember_horizontal(state, id, offset);
      Task::none()
    }
    Message::OverviewMoved {
      destination,
      session,
      direction,
      offset,
    } => {
      if destination != state.shell.destination
        || session != state.kernel.request_gate.current_session()
        || state.tv.focus != Focus::DetailOverview
        || !detail::overview_expanded(state)
      {
        return Task::none();
      }
      if let Some(offset) = offset {
        state.tv.offset = offset;
        navigation::restore_scroll(state)
      } else {
        state.tv.focus = if direction == Input::Down {
          detail::first_action(state)
        } else {
          Focus::DetailBack
        };
        navigation::reveal_measured(state)
      }
    }
  }
}

pub fn reconcile(state: &mut State) -> Task<AppMessage> {
  if !state.tv_mode() {
    filters::close(state);
    saved_browse::leave(state);
    return Task::none();
  }
  if state.shell.pending_close.is_some()
    || state.shell.quit_requested
    || super::accounts::content_mutations_blocked(&state.kernel)
  {
    state.tv.press = None;
  }
  let account = account::reconcile(state);
  let settings = settings::reconcile(state);
  filters::reconcile(state);
  let destination = state.shell.destination.clone();
  let mut restore = false;
  if state.tv.destination.as_ref() != Some(&destination) {
    if let Some(previous) = state.tv.destination.take() {
      state
        .tv
        .memory
        .retain(|saved| saved.destination != previous);
      state.tv.memory.push(Memory {
        destination: previous,
        focus: state.tv.focus,
        offset: state.tv.offset,
        horizontal: std::mem::take(&mut state.tv.horizontal),
        focused_item: state.tv.focused_item.take(),
      });
      if state.tv.memory.len() > 32 {
        state.tv.memory.remove(0);
      }
    }
    let saved = state
      .tv
      .memory
      .iter()
      .find(|saved| saved.destination == destination);
    state.tv.horizontal = saved
      .map(|saved| saved.horizontal.clone())
      .unwrap_or_default();
    state.tv.focused_item = saved.and_then(|saved| saved.focused_item.clone());
    let (focus, offset) = saved
      .map(|saved| (saved.focus, saved.offset))
      .unwrap_or_else(|| {
        (
          match destination {
            Destination::Detail(_) => Focus::DetailPlay,
            Destination::Library { .. } | Destination::Search(_) => Focus::Header(0),
            Destination::PersonalLists(_) => Focus::Lists(lists::Focus::Card(0)),
            Destination::SavedBrowse => Focus::SavedBrowse(saved_browse::Focus::Back),
            _ => Focus::HeroPlay,
          },
          0.0,
        )
      });
    state.tv.focus = focus;
    state.tv.content_focus = focus;
    state.tv.offset = offset;
    state.tv.destination = Some(destination);
    restore = true;
  }
  let playing = player::active(state);
  restore |= state.tv.was_playing && !playing;
  state.tv.was_playing = playing;
  let previous_focus = state.tv.focus;
  restore |= detail::reconcile_page(state);
  navigation::reconcile_focus(state);
  restore |= previous_focus != state.tv.focus;
  let lists = lists::reconcile(state);
  let saved_browse = saved_browse::reconcile(state);
  let player = player::reconcile(state);
  let browse = navigation::sync_browse(state);
  Task::batch([
    account,
    settings,
    lists,
    saved_browse,
    player,
    browse,
    if restore {
      navigation::restore_scroll(state).chain(navigation::reveal_measured(state))
    } else {
      Task::none()
    },
  ])
}

pub fn subscription(state: &State) -> Subscription<AppMessage> {
  let clock = if state.shell.window_id.is_some()
    && state.shell.images_visible
    && (!player::active(state) || state.tv.settings.open)
  {
    iced::time::every(std::time::Duration::from_secs(30))
      .map(|_| AppMessage::Tv(Message::ClockTick))
  } else {
    Subscription::none()
  };
  Subscription::batch([player::subscription(state), clock])
}

/// Removes presentation geometry before desktop browse resumes.
pub fn leave(state: &mut State) -> Task<AppMessage> {
  state.tv.press = None;
  lists::leave(state);
  settings::close(state);
  filters::close(state);
  saved_browse::leave(state);
  let search = search::close(state);
  let player = player::leave(state);
  if let Some(full) = state.full.as_mut() {
    full.browse.presentation_grid = None;
    full.browse.grid_viewport = None;
    full.browse.viewport.offset_y = 0.0;
    return Task::batch([
      player,
      search,
      super::browse::sync_scroll_window(
        &mut full.browse,
        &mut state.kernel,
        state.shell.window_size,
      ),
    ]);
  }
  Task::batch([player, search])
}

fn exit() -> AppMessage {
  AppMessage::UiModeSelected(UiMode::Desktop)
}

#[cfg(test)]
mod tests {
  use super::*;
  use jellypilot_core::LoadState;

  fn state() -> State {
    let mut state = State::boot(true);
    state.full = Some(super::super::state::FullUi::default());
    state.shell.ui_mode = UiMode::Tv;
    state.kernel.connection = jellypilot_auth::login::ConnectionPhase::Connected;
    state
  }

  fn focused_card() -> State {
    let mut state = state();
    state.full.as_mut().expect("full").home.data.rows[0].items = LoadState::Ready(vec![
      view::tests::item("first"),
      view::tests::item("second"),
    ]);
    state.tv.focus = Focus::Shelf { row: 0, item: 0 };
    state
  }

  #[tokio::test]
  async fn short_confirmation_opens_detail_but_a_hold_opens_only_the_menu() {
    use iced::futures::StreamExt;
    let mut state = focused_card();
    drop(update(&mut state, Message::ConfirmPressed));
    let token = state.tv.press.as_ref().expect("held candidate").token;
    let released = update(&mut state, Message::ConfirmReleased);
    let mut stream = iced_runtime::task::into_stream(released).expect("short activation");
    assert!(
      matches!(stream.next().await, Some(iced_runtime::Action::Output(AppMessage::OpenDetail(item))) if item.id == "first")
    );
    drop(update(&mut state, Message::ConfirmHeld(token)));
    assert!(!lists::menu_open(&state));

    drop(update(&mut state, Message::ConfirmPressed));
    let token = state.tv.press.as_ref().expect("held candidate").token;
    drop(update(&mut state, Message::ConfirmHeld(token)));
    assert!(lists::menu_open(&state));
    assert!(
      iced_runtime::task::into_stream(update(&mut state, Message::ConfirmReleased)).is_none()
    );
    assert_eq!(state.shell.destination, Destination::Home);
  }

  #[test]
  fn focus_movement_or_modal_entry_retires_long_press_before_it_can_act() {
    let mut state = focused_card();
    drop(update(&mut state, Message::ConfirmPressed));
    let token = state.tv.press.as_ref().expect("held candidate").token;
    drop(update(&mut state, Message::Input(Input::Right)));
    drop(update(&mut state, Message::ConfirmHeld(token)));
    assert!(!lists::menu_open(&state));
    assert_eq!(state.tv.focus, Focus::Shelf { row: 0, item: 1 });
    assert!(
      iced_runtime::task::into_stream(update(&mut state, Message::ConfirmReleased)).is_none()
    );

    drop(update(&mut state, Message::ConfirmPressed));
    let token = state.tv.press.as_ref().expect("held candidate").token;
    drop(settings::open(&mut state));
    drop(update(&mut state, Message::ConfirmHeld(token)));
    assert!(!lists::menu_open(&state));
    assert!(
      iced_runtime::task::into_stream(update(&mut state, Message::ConfirmReleased)).is_none()
    );
  }

  #[tokio::test]
  async fn returning_from_detail_keeps_origin_grid_focus_and_scroll() {
    let mut state = state();
    state.shell.destination = Destination::Library {
      library_id: "library".to_owned(),
      collection_type: "movies".to_owned(),
    };
    let mut fixture = crate::app::test_support::BrowseFixture::new();
    state.kernel.client = Some(fixture.client());
    let source = jellypilot_core::browse_model::BrowseSource::Search {
      session: state.kernel.request_gate.current_session(),
      query: "TV library".to_owned(),
    };
    let full = state.full.as_mut().expect("full");
    let task = crate::app::browse::start(&mut full.browse, &mut state.kernel, Some(source));
    crate::app::test_support::drain_task(
      &mut full.browse,
      &mut state.kernel,
      &mut fixture,
      state.shell.window_size,
      task,
      &mut |_| crate::app::test_support::FixtureReply::Page {
        total: 1000,
        artwork: false,
      },
    )
    .await;
    drop(reconcile(&mut state));
    state.tv.focus = Focus::Grid(12);
    state.tv.offset = 400.0;
    let origin = state.shell.destination.clone();
    state.shell.destination = Destination::Detail("detail".to_owned());
    drop(reconcile(&mut state));
    state.shell.destination = origin;
    drop(reconcile(&mut state));
    assert_eq!(state.tv.focus, Focus::Grid(12));
    assert_eq!(state.tv.offset, 400.0);
  }

  #[test]
  fn home_refresh_retains_item_identity_and_directional_input_can_leave_it() {
    let mut state = state();
    let first = view::tests::item("first");
    let second = view::tests::item("second");
    state.full.as_mut().expect("full").home.data.rows[0].items =
      LoadState::Ready(vec![first.clone(), second.clone()]);
    state.tv.focus = Focus::Shelf { row: 0, item: 1 };
    navigation::reconcile_focus(&mut state);
    state.full.as_mut().expect("full").home.data.rows[0].items =
      LoadState::Ready(vec![second, first]);
    navigation::reconcile_focus(&mut state);
    assert_eq!(state.tv.focus, Focus::Shelf { row: 0, item: 0 });
    drop(update(&mut state, Message::Input(Input::Right)));
    navigation::reconcile_focus(&mut state);
    assert_eq!(state.tv.focus, Focus::Shelf { row: 0, item: 1 });
    state.shell.ui_mode = UiMode::Desktop;
    drop(update(&mut state, Message::Input(Input::Left)));
    assert_eq!(state.tv.focus, Focus::Shelf { row: 0, item: 1 });
  }

  #[test]
  fn detail_return_restores_horizontal_source_and_relocates_reordered_home_item() {
    let mut state = state();
    let selected = view::tests::item("selected");
    let other = view::tests::item("other");
    state.full.as_mut().expect("full").home.data.rows[0].items =
      LoadState::Ready(vec![other.clone(), selected.clone()]);
    drop(reconcile(&mut state));
    drop(update(&mut state, Message::Input(Input::Right)));
    drop(reconcile(&mut state));
    assert_eq!(state.tv.focus, Focus::Shelf { row: 0, item: 1 });
    assert_eq!(state.tv.focused_item.as_deref(), Some("selected"));
    let shelf_id = navigation::shelf_id(0);
    remember_horizontal(&mut state, shelf_id.clone(), 344.0);
    state.shell.destination = Destination::Detail("selected".to_owned());
    drop(reconcile(&mut state));
    state.full.as_mut().expect("full").home.data.rows[0].items =
      LoadState::Ready(vec![selected, other]);
    state.shell.destination = Destination::Home;
    drop(reconcile(&mut state));
    assert_eq!(state.tv.focus, Focus::Shelf { row: 0, item: 0 });
    assert_eq!(state.tv.horizontal, vec![(shelf_id, 344.0)]);
  }
}
