//! Shared item-write execution and accepted-change propagation; surfaces own projections.

use std::sync::Arc;

use iced::Task;
use jellypilot_core::item_actions::{Action, Failure, Outcome, Receipt};
use jellypilot_media_server::{VideoLibraryItem, VideoUserDataAction, VideoUserDataUpdateRequest};

use super::kernel::Kernel;
use super::message::Message as AppMessage;
use super::state::{Destination, NoticeLevel, State, UserDataActionKind};
use super::{accounts, browse, collections, detail, personal_lists};
use crate::i18n::UiText;

#[derive(Clone, Copy)]
pub(crate) enum Origin {
  Detail { generation: u64 },
  Other,
}

#[derive(Clone)]
pub(crate) enum Message {
  Detail(UserDataActionKind),
  DetailWatchlist,
  WatchlistToggle(Box<VideoLibraryItem>),
  WatchlistRemove(String),
  Settled {
    receipt: Receipt,
    origin: Origin,
    result: Result<Outcome, String>,
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
    } => settle(state, receipt, origin, result),
  }
}

fn blocked(state: &mut State, origin: Origin) -> Option<Task<AppMessage>> {
  if state.shell.quit_requested {
    return Some(Task::none());
  }
  if accounts::content_mutations_blocked(&state.accounts) {
    let key = match origin {
      Origin::Detail { .. } => "shell-account-change-item",
      Origin::Other => "shell-account-change-lists",
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
  let action = match request_action {
    VideoUserDataAction::Favorite => Action::Favorite(true),
    VideoUserDataAction::Unfavorite => Action::Favorite(false),
    VideoUserDataAction::MarkPlayed => Action::Played(true),
    VideoUserDataAction::MarkUnplayed => Action::Played(false),
  };
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
      Failure::Request("No active media-server profile".to_owned()),
    );
  };
  state
    .kernel
    .item_actions
    .set_scope(state.kernel.request_gate.current_session(), Some(scope));
  let Some(receipt) = state.kernel.item_actions.begin(&item_id, action) else {
    return Task::none();
  };
  prepare(state, &receipt);
  let request = VideoUserDataUpdateRequest {
    item_id: receipt.item_id().to_owned(),
    action: request_action,
  };
  Task::perform(
    async move {
      client
        .library()
        .update_user_data(request)
        .await
        .map(Outcome::Server)
        .map_err(|error| error.to_string())
    },
    move |result| {
      AppMessage::ItemActions(Message::Settled {
        receipt,
        origin,
        result,
      })
    },
  )
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
  let should_add = item.is_some() && !full.personal_lists.watchlist_ids.contains(item_id.trim());
  sync_scope(&mut state.kernel);
  let Some(receipt) = state
    .kernel
    .item_actions
    .begin(&item_id, Action::Watchlist(should_add))
  else {
    return Task::none();
  };
  prepare(state, &receipt);
  let worker = state.watchlist.clone();
  let scope_epoch = worker.scope_epoch(&scope);
  Task::perform(
    async move {
      let snapshot = match item {
        Some(item) => {
          worker
            .set_membership(scope, item, should_add, scope_epoch)
            .await
        }
        None => worker.remove_item(scope, item_id, scope_epoch).await,
      };
      snapshot.map(|(revision, records)| Outcome::Watchlist { revision, records })
    },
    move |result| {
      AppMessage::ItemActions(Message::Settled {
        receipt,
        origin: Origin::Other,
        result,
      })
    },
  )
}

fn prepare(state: &mut State, receipt: &Receipt) {
  if let Some(full) = state.full.as_mut() {
    personal_lists::prepare_mutation(&mut full.personal_lists);
    if !matches!(receipt.action(), Action::Watchlist(_)) {
      detail::prepare_mutation(&mut full.detail, &mut state.kernel, receipt.item_id());
    }
  }
}

fn settle(
  state: &mut State,
  receipt: Receipt,
  origin: Origin,
  result: Result<Outcome, String>,
) -> Task<AppMessage> {
  sync_scope(&mut state.kernel);
  let Some(result) = state.kernel.item_actions.settle(&receipt, result) else {
    return Task::none();
  };
  let outcome = match result {
    Ok(outcome) => outcome,
    Err(error) => return report_failure(state, receipt.item_id(), receipt.action(), origin, error),
  };
  let Some(full) = state.full.as_mut() else {
    // Writes outlive Full-mode presentation. Restored surfaces load their own data.
    return Task::none();
  };
  match outcome {
    Outcome::Server(update) => {
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
    Outcome::Watchlist { revision, records } => personal_lists::apply_watchlist_snapshot(
      &mut full.personal_lists,
      &mut state.kernel,
      &state.watchlist,
      receipt.scope(),
      revision,
      records,
    ),
  }
}

fn report_failure(
  state: &mut State,
  item_id: &str,
  action: Action,
  origin: Origin,
  failure: Failure,
) -> Task<AppMessage> {
  if let Failure::Request(error) = &failure {
    tracing::warn!(error = %jellypilot_core::diagnostics::sanitize_message(error), "Item action failed");
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
  let key = match (action, failure) {
    (Action::Favorite(_), Failure::NotConfirmed) => "lists-favorite-not-updated",
    (Action::Favorite(_), Failure::Request(_)) => "lists-favorite-update-error",
    (Action::Played(_), _) => detail::USER_DATA_FAILURE,
    (Action::Watchlist(_), _) => "lists-watchlist-update-error",
  };
  let error = UiText::new(key);
  if let Some(full) = state.full.as_mut() {
    personal_lists::mutation_failed(&mut full.personal_lists, error.clone());
  }
  state.kernel.show_toast(NoticeLevel::Error, error)
}
