//! Scoped removal receipts outlive pages; the queue owns only presentation time.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use iced::Task;
use jellypilot_core::item_actions::{Action, Receipt};
use jellypilot_core::request_gate::SessionToken;
use jellypilot_core::undo_notices::{UndoNoticeQueue, UndoPause};
use jellypilot_core::watchlist::{ProfileScope, WatchlistRecord};
use jellypilot_media_server::{VideoLibraryItem, VideoUserDataAction};
use jellypilot_sdk::item_actions::{ItemActionError, WriteOutcome};
use jellypilot_sdk::HistoryRemoval;
use jellypilot_sdk::SdkError;
use jellypilot_ui::widgets::notice_interaction::NoticeInteraction;

use super::kernel::Kernel;
use super::message::Message as AppMessage;
use super::state::{NoticeLevel, State};
use super::{accounts, item_actions, personal_lists};
use crate::i18n::UiText;

#[derive(Clone)]
pub(crate) enum Removal {
  Favorite {
    item_id: String,
    name: String,
  },
  Watchlist(WatchlistRecord),
  History {
    item_id: String,
    name: String,
    receipt: Arc<HistoryRemoval>,
  },
}

impl Removal {
  pub fn item_id(&self) -> &str {
    match self {
      Self::Favorite { item_id, .. } | Self::History { item_id, .. } => item_id,
      Self::Watchlist(record) => record.item_id(),
    }
  }

  pub fn message(&self) -> UiText {
    let (key, name) = match self {
      Self::Favorite { name, .. } => ("lists-removed-favorite", name.as_str()),
      Self::Watchlist(record) => ("lists-removed-watchlist", record.name()),
      Self::History { name, .. } => ("lists-removed-history", name.as_str()),
    };
    UiText::new(key).arg("name", name.to_owned())
  }

  fn matches_action(&self, action: Action) -> bool {
    matches!(
      (self, action),
      (Self::Favorite { .. }, Action::Favorite(_)) | (Self::Watchlist(_), Action::Watchlist(_))
    )
  }
}

pub(crate) struct Notice {
  pub removal: Removal,
  pub pending: bool,
  pub failed: bool,
}

pub(crate) struct Runtime {
  pub queue: UndoNoticeQueue,
  pub notices: HashMap<u64, Notice>,
  pending_history: HashSet<String>,
  playback_after_history: HashSet<String>,
  clock: Instant,
  next_id: u64,
}

impl Default for Runtime {
  fn default() -> Self {
    Self {
      queue: UndoNoticeQueue::new(3),
      notices: HashMap::new(),
      pending_history: HashSet::new(),
      playback_after_history: HashSet::new(),
      clock: Instant::now(),
      next_id: 0,
    }
  }
}

impl Runtime {
  pub fn clear(&mut self) {
    self.queue.clear();
    self.notices.clear();
    self.pending_history.clear();
    self.playback_after_history.clear();
  }

  pub fn advance(&mut self) {
    for id in self.queue.advance(self.clock.elapsed()) {
      self.notices.remove(&id);
    }
  }

  fn dismiss(&mut self, id: u64) {
    self.queue.dismiss(id);
    self.notices.remove(&id);
  }

  fn push(&mut self, removal: Removal) {
    self.advance();
    self.next_id = self.next_id.wrapping_add(1);
    let id = self.next_id;
    self.notices.insert(
      id,
      Notice {
        removal,
        pending: false,
        failed: false,
      },
    );
    self.queue.push(id);
  }

  fn retire_action(&mut self, item_id: &str, action: Action) {
    let obsolete: Vec<_> = self
      .notices
      .iter()
      .filter(|(_, notice)| {
        notice.removal.item_id() == item_id && notice.removal.matches_action(action)
      })
      .map(|(id, _)| *id)
      .collect();
    for id in obsolete {
      self.dismiss(id);
    }
  }

  fn retire_history(&mut self, item_id: &str) {
    let obsolete: Vec<_> = self.notices.iter().filter(|(_, notice)| matches!(&notice.removal, Removal::History { item_id: id, .. } if id == item_id)).map(|(id, _)| *id).collect();
    for id in obsolete {
      self.dismiss(id);
    }
  }
}

/// Focus follows only a control that is still keyboard-focused when its
/// successful write arrives. Pointer activation clears native control focus.
#[derive(Clone)]
pub(crate) struct FocusContext {
  source: String,
  destination: super::state::Destination,
  session: SessionToken,
  kind: personal_lists::Kind,
  preferred: Vec<String>,
  undo: bool,
}

fn list_page(state: &State, kind: personal_lists::Kind) -> Option<&personal_lists::ListPage> {
  let lists = &state.full.as_ref()?.personal_lists;
  Some(match kind {
    personal_lists::Kind::Favorites => &lists.favorites,
    personal_lists::Kind::Watchlist => &lists.watchlist,
    personal_lists::Kind::History => &lists.history,
  })
}

fn kind_visible(destination: &super::state::Destination, kind: personal_lists::Kind) -> bool {
  use personal_lists::{Kind, Route};
  matches!(
    (destination, kind),
    (super::state::Destination::PersonalLists(Route::Overview), _)
      | (
        super::state::Destination::PersonalLists(Route::Favorites),
        Kind::Favorites
      )
      | (
        super::state::Destination::PersonalLists(Route::Watchlist),
        Kind::Watchlist
      )
      | (
        super::state::Destination::PersonalLists(Route::History),
        Kind::History
      )
  )
}

fn focus_surface_visible(state: &State) -> bool {
  state.full.is_some()
    && state.shell.window_id.is_some()
    && state.shell.images_visible
    && !state.shell.player_fullscreen
    && !state.shell.settings_open
    && !super::view::account::modal_open(state)
}

pub(crate) fn removal_focus(
  state: &State,
  kind: personal_lists::Kind,
  item_id: &str,
) -> Option<FocusContext> {
  if !focus_surface_visible(state) || !kind_visible(&state.shell.destination, kind) {
    return None;
  }
  let entries = &list_page(state, kind)?.entries;
  let position = entries.iter().position(|entry| entry.id == item_id)?;
  let preferred = entries
    .iter()
    .skip(position + 1)
    .chain(entries[..position].iter().rev())
    .map(|entry| entry.id.clone())
    .collect();
  Some(FocusContext {
    source: format!(
      "personal-remove-{}",
      personal_lists::artwork_key(kind, item_id)
    ),
    destination: state.shell.destination.clone(),
    session: state.kernel.request_gate.current_session(),
    kind,
    preferred,
    undo: false,
  })
}

pub(crate) fn restore_focus(state: &State, id: u64) -> Option<FocusContext> {
  if !focus_surface_visible(state)
    || !state
      .kernel
      .undo
      .queue
      .visible()
      .any(|visible| visible == id)
  {
    return None;
  }
  let removal = &state.kernel.undo.notices.get(&id)?.removal;
  let kind = match removal {
    Removal::Favorite { .. } => personal_lists::Kind::Favorites,
    Removal::Watchlist(_) => personal_lists::Kind::Watchlist,
    Removal::History { .. } => personal_lists::Kind::History,
  };
  Some(FocusContext {
    source: format!("undo-restore-{id}"),
    destination: state.shell.destination.clone(),
    session: state.kernel.request_gate.current_session(),
    kind,
    preferred: vec![removal.item_id().to_owned()],
    undo: true,
  })
}

pub(crate) fn capture_focus(context: FocusContext) -> Task<Option<FocusContext>> {
  iced::widget::operation::is_focused(context.source.clone())
    .map(move |focused| focused.then_some(context.clone()))
}

pub(crate) fn restore_captured_focus(
  state: &State,
  context: Option<FocusContext>,
) -> Task<AppMessage> {
  if !focus_surface_visible(state) {
    return Task::none();
  }
  context
    .and_then(|context| focus_target(state, context))
    .map_or_else(Task::none, |target| {
      iced::advanced::widget::operate(RestoreVacantFocus {
        target: target.into(),
        occupied: false,
      })
    })
}

/// Both traversals run inside the same native widget operation. A user who
/// focused another control while the write settled always keeps that focus.
struct RestoreVacantFocus {
  target: iced::widget::Id,
  occupied: bool,
}

impl<T: Send + 'static> iced::advanced::widget::Operation<T> for RestoreVacantFocus {
  fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<T>)) {
    visit(self);
  }

  fn focusable(
    &mut self,
    _: Option<&iced::widget::Id>,
    _: iced::Rectangle,
    state: &mut dyn iced::advanced::widget::operation::Focusable,
  ) {
    self.occupied |= state.is_focused();
  }

  fn finish(&self) -> iced::advanced::widget::operation::Outcome<T> {
    if self.occupied {
      iced::advanced::widget::operation::Outcome::None
    } else {
      iced::advanced::widget::operation::Outcome::Chain(Box::new(
        iced::advanced::widget::operation::focusable::focus(self.target.clone()),
      ))
    }
  }
}

fn focus_target(state: &State, context: FocusContext) -> Option<String> {
  if state.shell.destination != context.destination
    || !state
      .kernel
      .request_gate
      .is_current_session(context.session)
    || state.shell.quit_requested
    || state.shell.settings_open
    || super::view::account::modal_open(state)
    || accounts::content_mutations_blocked(&state.kernel)
  {
    return None;
  }
  if context.undo {
    if let Some(id) = state.kernel.undo.queue.visible().find(|id| {
      state
        .kernel
        .undo
        .notices
        .get(id)
        .is_some_and(|notice| !notice.pending)
    }) {
      return Some(format!("undo-restore-{id}"));
    }
  }
  if !kind_visible(&state.shell.destination, context.kind) {
    return None;
  }
  let page = list_page(state, context.kind)?;
  for id in context.preferred {
    if let Some(entry) = page.entries.iter().find(|entry| entry.id == id) {
      let prefix = if entry.item.is_some() {
        "personal-open"
      } else {
        "personal-remove"
      };
      return Some(format!(
        "{prefix}-{}",
        personal_lists::artwork_key(context.kind, &id)
      ));
    }
  }
  let id = if matches!(
    state.shell.destination,
    super::state::Destination::PersonalLists(personal_lists::Route::Overview)
  ) {
    format!("personal-view-all-{:?}", context.kind)
  } else {
    format!("personal-back-{:?}", context.kind)
  };
  Some(id)
}

#[derive(Clone)]
pub(crate) enum Message {
  Focused {
    completion: Box<Message>,
    focus: Option<FocusContext>,
  },
  Tick,
  Interaction(u64, NoticeInteraction),
  Dismiss(u64),
  Restore(u64),
  RemoveHistory(Box<VideoLibraryItem>),
  HistoryRemoved {
    session: SessionToken,
    scope: ProfileScope,
    item: Box<VideoLibraryItem>,
    result: Result<Arc<HistoryRemoval>, SdkError>,
  },
  HistoryRestored {
    item_id: String,
    session: SessionToken,
    id: u64,
    result: Result<bool, SdkError>,
  },
  PlaybackRestored {
    session: SessionToken,
    item_id: String,
    result: Result<bool, SdkError>,
  },
}

pub(crate) fn reconcile(state: &mut State) {
  let undo = &mut state.kernel.undo;
  undo.advance();
  if state.shell.quit_requested {
    undo.clear();
    return;
  }
  // Reserve the responsive player, top navigation, and space around notices.
  let capacity = if state.full.is_none()
    || state.shell.window_id.is_none()
    || state.shell.player_fullscreen
    || state.shell.settings_open
    || super::view::account::modal_open(state)
  {
    0
  } else {
    ((state.shell.window_size.height - 310.0).max(0.0) / 100.0).floor() as usize
  };
  state.kernel.undo.queue.set_capacity(capacity.min(3));
}

pub(crate) fn history_busy(kernel: &Kernel, item_id: &str) -> bool {
  kernel.undo.pending_history.contains(item_id)
}

pub(crate) fn update(state: &mut State, message: Message) -> Task<AppMessage> {
  state.kernel.undo.advance();
  if let Message::Focused { completion, focus } = message {
    return apply(state, *completion, focus);
  }
  let context = match &message {
    Message::HistoryRemoved {
      session,
      item,
      result: Ok(_),
      ..
    } if state.kernel.request_gate.is_current_session(*session) => {
      removal_focus(state, personal_lists::Kind::History, &item.id)
    }
    Message::HistoryRestored {
      session,
      id,
      result: Ok(_),
      ..
    } if state.kernel.request_gate.is_current_session(*session) => restore_focus(state, *id),
    _ => None,
  };
  if let Some(context) = context {
    return capture_focus(context).map(move |focus| {
      AppMessage::Undo(Message::Focused {
        completion: Box::new(message.clone()),
        focus,
      })
    });
  }
  apply(state, message, None)
}

fn apply(state: &mut State, message: Message, focus: Option<FocusContext>) -> Task<AppMessage> {
  match message {
    Message::Focused { .. } => Task::none(),
    Message::Tick => Task::none(),
    Message::Interaction(id, interaction) => {
      state
        .kernel
        .undo
        .queue
        .set_paused(id, UndoPause::Hovered, interaction.hovered);
      state
        .kernel
        .undo
        .queue
        .set_paused(id, UndoPause::Focused, interaction.focused);
      Task::none()
    }
    Message::Dismiss(id) => {
      state.kernel.undo.dismiss(id);
      Task::none()
    }
    Message::Restore(id) => restore(state, id),
    Message::RemoveHistory(item) => remove_history(state, item),
    Message::HistoryRemoved {
      session,
      scope,
      item,
      result,
    } => {
      if !state.kernel.request_gate.is_current_session(session)
        || personal_lists::active_scope(&state.kernel).ok().as_ref() != Some(&scope)
        || state.shell.quit_requested
      {
        return Task::none();
      }
      let followup = finish_history(&mut state.kernel, &item.id);
      let task = match result {
        Ok(receipt) => {
          state.kernel.undo.retire_history(&item.id);
          if receipt.changed() {
            state.kernel.undo.push(Removal::History {
              item_id: item.id.clone(),
              name: item.name.clone(),
              receipt,
            });
          }
          let task = refresh_history(state, Some(&item.id));
          Task::batch([task, restore_captured_focus(state, focus)])
        }
        Err(error) => history_error(state, error),
      };
      Task::batch([task, followup])
    }
    Message::HistoryRestored {
      item_id,
      session,
      id,
      result,
    } => {
      if !state.kernel.request_gate.is_current_session(session) {
        return Task::none();
      }
      let followup = finish_history(&mut state.kernel, &item_id);
      let task = if state.kernel.undo.queue.contains(id) {
        match result {
          Ok(_) => {
            state.kernel.undo.dismiss(id);
            let task = refresh_history(state, None);
            Task::batch([task, restore_captured_focus(state, focus)])
          }
          Err(_) => {
            failed(state, id);
            Task::none()
          }
        }
      } else {
        Task::none()
      };
      Task::batch([task, followup])
    }
    Message::PlaybackRestored {
      session,
      item_id,
      result,
    } => {
      if !state.kernel.request_gate.is_current_session(session) {
        return Task::none();
      }
      let followup = finish_history(&mut state.kernel, &item_id);
      let task = match result {
        Ok(changed) => {
          state.kernel.undo.retire_history(&item_id);
          if changed {
            refresh_history(state, None)
          } else {
            Task::none()
          }
        }
        Err(error) => history_error(state, error),
      };
      Task::batch([task, followup])
    }
  }
}

fn remove_history(state: &mut State, item: Box<VideoLibraryItem>) -> Task<AppMessage> {
  if state.shell.quit_requested
    || accounts::content_mutations_blocked(&state.kernel)
    || history_busy(&state.kernel, &item.id)
  {
    return Task::none();
  }
  let Ok(scope) = personal_lists::active_scope(&state.kernel) else {
    return Task::none();
  };
  let sdk = Arc::clone(&state.kernel.sdk);
  let Ok(token) = sdk.new_operation_token() else {
    return Task::none();
  };
  let session = state.kernel.request_gate.current_session();
  state.kernel.undo.pending_history.insert(item.id.clone());
  let item_id = item.id.clone();
  let baseline = item.last_played_date.clone();
  Task::perform(
    async move {
      sdk
        .hide_desktop_history_item(token, item_id, baseline)
        .await
    },
    move |result| {
      AppMessage::Undo(Message::HistoryRemoved {
        session,
        scope,
        item,
        result,
      })
    },
  )
}

fn restore(state: &mut State, id: u64) -> Task<AppMessage> {
  if state.shell.quit_requested || accounts::content_mutations_blocked(&state.kernel) {
    return Task::none();
  }
  let Some(notice) = state.kernel.undo.notices.get(&id) else {
    return Task::none();
  };
  if notice.pending
    || item_actions::busy(&state.kernel, notice.removal.item_id())
    || history_busy(&state.kernel, notice.removal.item_id())
  {
    return Task::none();
  }
  let removal = notice.removal.clone();
  let task = match removal {
    Removal::Favorite { item_id, .. } => item_actions::start_server(
      state,
      item_id,
      VideoUserDataAction::Favorite,
      item_actions::Origin::Undo(id),
    ),
    Removal::Watchlist(record) => {
      item_actions::sync_scope(&mut state.kernel);
      let Ok(admission) = state
        .kernel
        .item_actions
        .begin(record.item_id(), Action::Watchlist(true))
      else {
        return Task::none();
      };
      let receipt = admission.receipt().clone();
      let runtime = state.watchlist.clone();
      let epoch = runtime.scope_epoch(record.scope());
      Task::perform(
        async move {
          runtime
            .restore_removed(admission, record, epoch)
            .await
            .map(WriteOutcome::Watchlist)
        },
        move |result| {
          AppMessage::ItemActions(item_actions::Message::Settled {
            receipt,
            origin: item_actions::Origin::Undo(id),
            result,
            removed: None,
          })
        },
      )
    }
    Removal::History {
      receipt, item_id, ..
    } => {
      state.kernel.undo.pending_history.insert(item_id.clone());
      let session = state.kernel.request_gate.current_session();
      Task::perform(async move { receipt.undo().await }, move |result| {
        AppMessage::Undo(Message::HistoryRestored {
          item_id,
          session,
          id,
          result,
        })
      })
    }
  };
  if let Some(notice) = state.kernel.undo.notices.get(&id) {
    if matches!(notice.removal, Removal::Favorite { .. })
      && !item_actions::busy(&state.kernel, notice.removal.item_id())
    {
      return task;
    }
  }
  if let Some(notice) = state.kernel.undo.notices.get_mut(&id) {
    notice.pending = true;
    notice.failed = false;
  }
  state
    .kernel
    .undo
    .queue
    .set_paused(id, UndoPause::Pending, true);
  task
}

/// Called only for acknowledged writes; newer same-collection intentions retire old Undo.
pub(crate) fn item_settled(
  state: &mut State,
  receipt: &Receipt,
  origin: item_actions::Origin,
  result: &Result<WriteOutcome, ItemActionError>,
  removed: Option<Removal>,
) {
  if result.is_err() {
    if let item_actions::Origin::Undo(id) = origin {
      failed(state, id);
    }
    return;
  }
  state
    .kernel
    .undo
    .retire_action(receipt.item_id(), receipt.action());
  if !state.shell.quit_requested {
    if let Some(removed) = removed {
      state.kernel.undo.push(removed);
    }
  }
}

fn failed(state: &mut State, id: u64) {
  if let Some(notice) = state.kernel.undo.notices.get_mut(&id) {
    notice.pending = false;
    notice.failed = true;
  }
  state
    .kernel
    .undo
    .queue
    .set_paused(id, UndoPause::Pending, false);
}

fn refresh_history(state: &mut State, hidden_item: Option<&str>) -> Task<AppMessage> {
  let Some(full) = state.full.as_mut() else {
    return Task::none();
  };
  personal_lists::history_changed(
    &mut full.personal_lists,
    &mut state.kernel,
    &state.watchlist,
    hidden_item,
  )
}

fn history_error(state: &mut State, error: SdkError) -> Task<AppMessage> {
  if matches!(error, SdkError::Stale | SdkError::Cancelled) {
    return Task::none();
  }
  state.kernel.show_toast(
    NoticeLevel::Error,
    UiText::new("lists-history-update-error"),
  )
}

pub(crate) fn playback_started(kernel: &mut Kernel, item_id: String) -> Task<AppMessage> {
  if kernel.undo.pending_history.contains(&item_id) {
    kernel.undo.playback_after_history.insert(item_id);
    return Task::none();
  }
  let sdk = Arc::clone(&kernel.sdk);
  let Ok(token) = sdk.new_operation_token() else {
    return Task::none();
  };
  kernel.undo.pending_history.insert(item_id.clone());
  let session = kernel.request_gate.current_session();
  let restore_id = item_id.clone();
  Task::perform(
    async move {
      sdk
        .restore_desktop_history_for_playback(token, restore_id)
        .await
    },
    move |result| {
      AppMessage::Undo(Message::PlaybackRestored {
        session,
        item_id,
        result,
      })
    },
  )
}

fn finish_history(kernel: &mut Kernel, item_id: &str) -> Task<AppMessage> {
  kernel.undo.pending_history.remove(item_id);
  if kernel.undo.playback_after_history.remove(item_id) {
    playback_started(kernel, item_id.to_owned())
  } else {
    Task::none()
  }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod focus_tests {
  use super::super::state::{Destination, FullUi};
  use super::*;
  use personal_lists::{Kind, Route};

  #[test]
  fn delayed_removal_focus_does_not_cross_navigation_or_account_boundaries() {
    let mut state = State::boot(true);
    state.full = Some(FullUi::default());
    state.shell.destination = Destination::PersonalLists(Route::Overview);
    let context = FocusContext {
      source: "removed-control".to_owned(),
      destination: state.shell.destination.clone(),
      session: state.kernel.request_gate.current_session(),
      kind: Kind::History,
      preferred: Vec::new(),
      undo: false,
    };
    assert_eq!(
      focus_target(&state, context.clone()).as_deref(),
      Some("personal-view-all-History")
    );
    state.shell.destination = Destination::Home;
    assert!(focus_target(&state, context.clone()).is_none());
    state.shell.destination = context.destination.clone();
    state.kernel.request_gate.disconnect();
    assert!(focus_target(&state, context).is_none());
  }

  #[test]
  fn undo_focus_skips_pending_notices_and_does_not_enter_a_modal() {
    let mut state = State::boot(true);
    state.full = Some(FullUi::default());
    state.shell.destination = Destination::Home;
    for item_id in ["first", "second"] {
      state.kernel.undo.push(Removal::Favorite {
        item_id: item_id.to_owned(),
        name: item_id.to_owned(),
      });
    }
    state
      .kernel
      .undo
      .notices
      .get_mut(&1)
      .expect("first notice")
      .pending = true;
    let context = FocusContext {
      source: "undo-restore-0".to_owned(),
      destination: Destination::Home,
      session: state.kernel.request_gate.current_session(),
      kind: Kind::Favorites,
      preferred: vec!["restored-item".to_owned()],
      undo: true,
    };
    assert_eq!(
      focus_target(&state, context.clone()).as_deref(),
      Some("undo-restore-2")
    );
    state.shell.settings_open = true;
    assert!(focus_target(&state, context).is_none());
  }
  #[tokio::test]
  async fn focus_selected_after_capture_is_preserved_during_restoration() {
    use iced::advanced::{
      renderer::Headless,
      widget::operation::{self, Operation},
    };
    use iced_runtime::user_interface::{Cache, UserInterface};
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    let content = iced::widget::column![
      jellypilot_ui::control_button(
        None,
        Some("Next card".to_owned()),
        jellypilot_ui::variants::ButtonVariant::Text
      )
      .id("next-card")
      .on_press(()),
      iced::widget::text_input("Search", "")
        .id("search")
        .on_input(|_| ()),
    ];
    let mut ui = UserInterface::build(
      content,
      iced::Size::new(400.0, 200.0),
      Cache::new(),
      &mut renderer,
    );
    // The originating control was removed, but a newer keyboard/pointer action
    // has placed focus in Search before the delayed successful settlement.
    ui.operate(&renderer, &mut operation::focusable::focus("search".into()));
    let mut restore = RestoreVacantFocus {
      target: "next-card".into(),
      occupied: false,
    };
    ui.operate(&renderer, &mut restore);
    let outcome: operation::Outcome<()> = restore.finish();
    assert!(matches!(outcome, operation::Outcome::None));
    let mut focused = operation::focusable::find_focused();
    ui.operate(&renderer, &mut operation::black_box(&mut focused));
    assert!(
      matches!(focused.finish(), operation::Outcome::Some(id) if id == iced::widget::Id::new("search"))
    );

    ui.operate(&renderer, &mut operation::focusable::unfocus());
    let mut restore = RestoreVacantFocus {
      target: "next-card".into(),
      occupied: false,
    };
    ui.operate(&renderer, &mut restore);
    let operation::Outcome::Chain(mut next) = <RestoreVacantFocus as Operation>::finish(&restore)
    else {
      panic!("vacant focus should be restored")
    };
    ui.operate(&renderer, next.as_mut());
    let mut focused = operation::focusable::find_focused();
    ui.operate(&renderer, &mut operation::black_box(&mut focused));
    assert!(
      matches!(focused.finish(), operation::Outcome::Some(id) if id == iced::widget::Id::new("next-card"))
    );
  }
}
