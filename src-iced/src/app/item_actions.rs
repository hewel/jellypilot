//! Shared item-write execution and accepted-change propagation; surfaces own projections.
//!
//! Admission, request execution, confirmation, and delivery acknowledgment
//! live in `jellypilot_sdk::item_actions::ItemActions`, the same executor the
//! SDK exposes to FFI consumers. This module keeps presentation: admission
//! side effects (mutation prep), queued-result delivery, and toast/inline
//! error reporting.

use std::sync::Arc;

use iced::Task;
use jellypilot_core::item_actions::{Action, Receipt};
use jellypilot_media_server::{VideoLibraryItem, VideoUserDataAction};
use jellypilot_sdk::item_actions::{
  Admission, ItemActionError, WatchlistSnapshot, WatchlistStorage, WatchlistWrite,
  WatchlistWriteAction, WriteOutcome,
};

use super::kernel::Kernel;
use super::message::Message as AppMessage;
use super::state::{Destination, NoticeLevel, State, UserDataActionKind};
use super::{accounts, browse, collections, detail, personal_lists};
use crate::i18n::UiText;

#[derive(Clone, Copy)]
pub(crate) enum Origin {
  Detail { generation: u64 },
  Other,
  Undo(u64),
}

#[derive(Clone)]
pub(crate) enum Message {
  SettledFocused {
    completion: Box<Message>,
    focus: Option<super::undo::FocusContext>,
  },
  Detail(UserDataActionKind),
  DetailWatchlist,
  WatchlistToggle(Box<VideoLibraryItem>),
  WatchlistRemove(String),
  Settled {
    receipt: Receipt,
    origin: Origin,
    result: Result<WriteOutcome, ItemActionError>,
    removed: Option<Box<super::undo::Removal>>,
  },
}

/// Called at connection transitions and operation admission/settlement, not each frame.
pub(crate) fn sync_scope(kernel: &mut Kernel) {
  kernel.item_actions.set_scope(
    kernel.request_gate.current_session(),
    personal_lists::active_scope(kernel).ok(),
  );
}

pub(crate) fn busy(kernel: &Kernel, item_id: &str) -> bool {
  kernel.item_actions.pending(item_id).is_some()
}

pub(crate) fn update(state: &mut State, message: Message) -> Task<AppMessage> {
  match message {
    Message::Detail(kind) => {
      let Some(full) = state.full.as_ref() else {
        return Task::none();
      };
      let Destination::Detail(current_id) = &state.shell.destination else {
        return Task::none();
      };
      let Some((item_id, action)) = detail::action_target(&full.detail, kind) else {
        return Task::none();
      };
      if &item_id != current_id {
        return Task::none();
      }
      let origin = Origin::Detail {
        generation: detail::view_generation(&full.detail),
      };
      start_server(state, item_id, action, origin)
    }
    Message::DetailWatchlist => {
      let Some(full) = state.full.as_ref() else {
        return Task::none();
      };
      let Destination::Detail(item_id) = &state.shell.destination else {
        return Task::none();
      };
      let Some(item) = full.detail.items.get(item_id).cloned() else {
        return Task::none();
      };
      start_watchlist(state, item.id.clone(), Some(item))
    }
    Message::WatchlistToggle(item) => start_watchlist(state, item.id.clone(), Some(*item)),
    Message::WatchlistRemove(item_id) => start_watchlist(state, item_id, None),
    Message::Settled {
      receipt,
      origin,
      result,
      removed,
    } => {
      sync_scope(&mut state.kernel);
      let focus = if result.is_ok() && state.kernel.item_actions.is_current(&receipt) {
        match origin {
          Origin::Undo(id) => super::undo::restore_focus(state, id),
          _ => match receipt.action() {
            Action::Favorite(false) => {
              super::undo::removal_focus(state, personal_lists::Kind::Favorites, receipt.item_id())
            }
            Action::Watchlist(false) => {
              super::undo::removal_focus(state, personal_lists::Kind::Watchlist, receipt.item_id())
            }
            _ => None,
          },
        }
      } else {
        None
      };
      if let Some(focus) = focus {
        super::undo::capture_focus(focus).map(move |focus| {
          AppMessage::ItemActions(Message::SettledFocused {
            completion: Box::new(Message::Settled {
              receipt: receipt.clone(),
              origin,
              result: result.clone(),
              removed: removed.clone(),
            }),
            focus,
          })
        })
      } else {
        settle(state, receipt, origin, result, removed, None)
      }
    }
    Message::SettledFocused { completion, focus } => {
      if let Message::Settled {
        receipt,
        origin,
        result,
        removed,
      } = *completion
      {
        settle(state, receipt, origin, result, removed, focus)
      } else {
        Task::none()
      }
    }
  }
}

fn blocked(state: &mut State, origin: Origin) -> Option<Task<AppMessage>> {
  if state.shell.quit_requested {
    return Some(Task::none());
  }
  if accounts::content_mutations_blocked(&state.kernel) {
    let key = match origin {
      Origin::Detail { .. } => "shell-account-change-item",
      Origin::Other | Origin::Undo(_) => "shell-account-change-lists",
    };
    return Some(
      state
        .kernel
        .show_toast(NoticeLevel::Warning, UiText::new(key)),
    );
  }
  None
}

pub(crate) fn start_server(
  state: &mut State,
  item_id: String,
  request_action: VideoUserDataAction,
  origin: Origin,
) -> Task<AppMessage> {
  if let Some(task) = blocked(state, origin) {
    return task;
  }
  let action = jellypilot_sdk::item_actions::server_action(request_action);
  let (Some(client), Ok(scope)) = (
    state.kernel.client.as_ref().map(Arc::clone),
    personal_lists::active_scope(&state.kernel),
  ) else {
    state
      .kernel
      .item_actions
      .set_scope(state.kernel.request_gate.current_session(), None);
    return report_failure(
      state,
      &item_id,
      action,
      origin,
      ItemActionError::Failed(jellypilot_sdk::SdkError::Request(
        "No active media-server profile".to_owned(),
      )),
    );
  };
  state
    .kernel
    .item_actions
    .set_scope(state.kernel.request_gate.current_session(), Some(scope));
  let Ok(admission) = state.kernel.item_actions.begin(&item_id, action) else {
    return Task::none();
  };
  let removed = (action == Action::Favorite(false)).then(|| {
    Box::new(super::undo::Removal::Favorite {
      item_id: item_id.clone(),
      name: item_name(state, &item_id),
    })
  });
  prepare(state, &admission);
  let receipt = admission.receipt().clone();
  let executor = state.kernel.item_actions.clone();
  Task::perform(
    async move {
      executor
        .run_server(admission, client)
        .await
        .map(WriteOutcome::Server)
    },
    move |result| {
      AppMessage::ItemActions(Message::Settled {
        receipt,
        origin,
        result,
        removed,
      })
    },
  )
}

/// Watchlist storage adapter over the desktop runtime, which owns the store
/// and its scope-epoch fencing and carries the admission into its blocking
/// worker.
struct StoreWatchlist {
  worker: personal_lists::Runtime,
  scope_epoch: u64,
}

impl WatchlistStorage for StoreWatchlist {
  async fn apply(
    &self,
    admission: Admission,
    write: WatchlistWrite,
  ) -> Result<WatchlistSnapshot, ItemActionError> {
    self
      .worker
      .apply_write(admission, write, self.scope_epoch)
      .await
  }
}

fn start_watchlist(
  state: &mut State,
  item_id: String,
  item: Option<VideoLibraryItem>,
) -> Task<AppMessage> {
  if let Some(task) = blocked(state, Origin::Other) {
    return task;
  }
  let Some(full) = state.full.as_ref() else {
    return Task::none();
  };
  let Some(scope) = personal_lists::current_scope(&full.personal_lists, &state.kernel) else {
    return Task::none();
  };
  let action = match item {
    Some(item) if !full.personal_lists.watchlist_ids.contains(item_id.trim()) => {
      WatchlistWriteAction::Add(Box::new(item))
    }
    _ => WatchlistWriteAction::Remove,
  };
  sync_scope(&mut state.kernel);
  let Ok(admission) = state.kernel.item_actions.begin(
    &item_id,
    match &action {
      WatchlistWriteAction::Add(_) => Action::Watchlist(true),
      WatchlistWriteAction::Remove => Action::Watchlist(false),
    },
  ) else {
    return Task::none();
  };
  prepare(state, &admission);
  let receipt = admission.receipt().clone();
  let executor = state.kernel.item_actions.clone();
  let runtime = state.watchlist.clone();
  let scope_epoch = runtime.scope_epoch(&scope);
  let storage = StoreWatchlist {
    worker: runtime.clone(),
    scope_epoch,
  };
  let removing = matches!(action, WatchlistWriteAction::Remove);
  let write = WatchlistWrite { item_id, action };
  Task::perform(
    async move {
      if removing {
        match runtime.remove_undoable(admission, scope_epoch).await {
          Ok((snapshot, record)) => (
            Ok(WriteOutcome::Watchlist(snapshot)),
            record.map(|record| Box::new(super::undo::Removal::Watchlist(record))),
          ),
          Err(error) => (Err(error), None),
        }
      } else {
        (
          executor
            .run_watchlist(admission, storage, write)
            .await
            .map(WriteOutcome::Watchlist),
          None,
        )
      }
    },
    move |(result, removed)| {
      AppMessage::ItemActions(Message::Settled {
        receipt,
        origin: Origin::Other,
        result,
        removed,
      })
    },
  )
}

fn prepare(state: &mut State, admission: &Admission) {
  if let Some(full) = state.full.as_mut() {
    personal_lists::prepare_mutation(&mut full.personal_lists);
    if !matches!(admission.action(), Action::Watchlist(_)) {
      detail::prepare_mutation(&mut full.detail, &mut state.kernel, admission.item_id());
    }
  }
}

fn settle(
  state: &mut State,
  receipt: Receipt,
  origin: Origin,
  result: Result<WriteOutcome, ItemActionError>,
  removed: Option<Box<super::undo::Removal>>,
  focus: Option<super::undo::FocusContext>,
) -> Task<AppMessage> {
  sync_scope(&mut state.kernel);
  // Delivery acknowledgment: the queued completion is applied at most once
  // and only while it is still the newest settled write for its item under
  // the current binding, so reordered or stale deliveries cannot overwrite
  // newer accepted state.
  if !state.kernel.item_actions.acknowledge(&receipt) {
    return Task::none();
  }
  super::undo::item_settled(
    state,
    &receipt,
    origin,
    &result,
    removed.map(|removed| *removed),
  );
  if result.is_ok() && matches!(receipt.action(), Action::Played(_)) {
    super::list_playback::invalidate(state);
  }
  let outcome = match result {
    Ok(outcome) => outcome,
    Err(_) if matches!(origin, Origin::Undo(_)) => return Task::none(),
    Err(error) => return report_failure(state, receipt.item_id(), receipt.action(), origin, error),
  };
  let Some(full) = state.full.as_mut() else {
    // Writes outlive Full-mode presentation. Restored surfaces load their own data.
    return Task::none();
  };
  let task = match outcome {
    WriteOutcome::Server(update) => {
      super::shell::apply_user_data_update(&mut state.shell, &update);
      let refresh_detail = !matches!(
        origin,
        Origin::Detail { generation } if generation == detail::view_generation(&full.detail)
      );
      collections::apply_confirmed(&mut full.collections, &update);
      Task::batch([
        browse::apply_user_data_update(&mut full.browse, &mut state.kernel, &update),
        detail::apply_confirmed(&mut full.detail, &mut state.kernel, &update, refresh_detail),
        personal_lists::apply_user_data_update(
          &mut full.personal_lists,
          &mut state.kernel,
          &state.watchlist,
          &update,
          receipt.action(),
        ),
      ])
    }
    WriteOutcome::Watchlist(snapshot) => personal_lists::apply_watchlist_snapshot(
      &mut full.personal_lists,
      &mut state.kernel,
      &state.watchlist,
      receipt.scope(),
      snapshot.revision,
      snapshot.records,
    ),
  };
  Task::batch([task, super::undo::restore_captured_focus(state, focus)])
}

fn report_failure(
  state: &mut State,
  item_id: &str,
  action: Action,
  origin: Origin,
  failure: ItemActionError,
) -> Task<AppMessage> {
  if let ItemActionError::Failed(jellypilot_sdk::SdkError::Request(error)) = &failure {
    tracing::warn!(error = %jellypilot_core::diagnostics::sanitize_message(error), "Item action failed");
  }
  if failure.is_silent() {
    return Task::none();
  }
  if let Origin::Detail { generation } = origin {
    if state
      .full
      .as_mut()
      .is_some_and(|full| detail::mutation_failed(&mut full.detail, item_id, generation))
    {
      return Task::none();
    }
    return state
      .kernel
      .show_toast(NoticeLevel::Error, UiText::new(detail::USER_DATA_FAILURE));
  }
  let key = match (action, &failure) {
    (Action::Favorite(_), ItemActionError::NotConfirmed) => "lists-favorite-not-updated",
    (Action::Favorite(_), _) => "lists-favorite-update-error",
    (Action::Played(_), _) => detail::USER_DATA_FAILURE,
    (Action::Watchlist(_), _) => "lists-watchlist-update-error",
  };
  let error = UiText::new(key);
  if let Some(full) = state.full.as_mut() {
    personal_lists::mutation_failed(&mut full.personal_lists, error.clone());
  }
  state.kernel.show_toast(NoticeLevel::Error, error)
}

fn item_name(state: &State, item_id: &str) -> String {
  if let Some(full) = &state.full {
    if let Some(entry) = full
      .personal_lists
      .favorites
      .entries
      .iter()
      .chain(&full.personal_lists.watchlist.entries)
      .chain(&full.personal_lists.history.entries)
      .find(|entry| entry.id == item_id)
    {
      return entry.name.clone();
    }
    if let Some(item) = full.detail.items.get(item_id) {
      return item.name.clone();
    }
    if let jellypilot_core::browse_model::LibraryBrowseView::Ready { visible_items, .. } =
      &full.browse.view
    {
      if let Some(item) = visible_items
        .iter()
        .filter_map(|slot| slot.item.as_ref())
        .find(|item| item.id == item_id)
      {
        return item.name.clone();
      }
    }
  }
  item_id.to_owned()
}
