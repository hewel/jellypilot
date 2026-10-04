//! Profile-owned saved query orchestration. Only the Browser executes a query.

#[cfg(test)]
mod handoff_tests;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use iced::Task;
use jellypilot_core::browse_model::BrowsePreferences;
use jellypilot_core::request_gate::SessionToken;
use jellypilot_media_server::{
  VideoLibraryKind, VideoLibraryPlayedFilter, VideoLibraryQuality, VideoLibrarySort,
  VideoLibrarySortDirection,
};
use jellypilot_sdk::saved_browse::{
  ResolvedSavedBrowse, SavedBrowseDraft, SavedBrowseError, SavedBrowseFilter, SavedBrowseId,
};
use jellypilot_sdk::OperationToken;

use super::message::Message as AppMessage;
use super::state::{Destination, State};
use crate::i18n::UiText;

#[derive(Clone)]
pub struct AppliedFilter {
  pub filter: SavedBrowseFilter,
  pub deleted: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordAction {
  Apply,
  Rename,
  Delete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Condition {
  Sort,
  Direction,
  Played,
  Favorites,
  Quality,
  Country,
  Genre,
}

#[derive(Clone)]
pub enum EditorKind {
  Save(Box<SavedBrowseDraft>),
  Rename(SavedBrowseFilter),
}

#[derive(Clone)]
pub struct Editor {
  pub name: String,
  pub kind: EditorKind,
  pub error: Option<UiText>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingAction {
  Load,
  Save,
  Rename(SavedBrowseId),
  Delete(SavedBrowseId),
  Apply(SavedBrowseId),
}

#[derive(Default)]
pub struct Surface {
  pub records: Vec<SavedBrowseFilter>,
  pub loaded: bool,
  pub pending: Option<PendingAction>,
  pub error: Option<UiText>,
  pub editor: Option<Editor>,
  pub presentation: u64,
  sequence: u64,
  token: Option<Arc<OperationToken>>,
}

impl Surface {
  fn cancel(&mut self) {
    if let Some(token) = self.token.take() {
      token.cancel();
    }
    self.pending = None;
    self.sequence = self.sequence.wrapping_add(1);
  }

  fn retire(&mut self) {
    self.cancel();
    self.presentation = self.presentation.wrapping_add(1);
  }
}

impl Drop for Surface {
  fn drop(&mut self) {
    self.cancel();
  }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Target {
  session: SessionToken,
  presentation: u64,
  query: Option<String>,
}

#[derive(Clone)]
pub enum Action {
  Back,
  Reload,
  SaveCurrent,
  Record {
    id: SavedBrowseId,
    action: RecordAction,
  },
  NameChanged(String),
  Submit,
  CloseEditor,
  RemoveCondition(Condition),
  ClearConditions,
}

#[derive(Clone)]
pub enum Completion {
  Loaded(Vec<SavedBrowseFilter>),
  Written(SavedBrowseFilter),
  Deleted(SavedBrowseId),
  Resolved(Box<ResolvedSavedBrowse>),
}

#[derive(Clone)]
pub enum Message {
  Action {
    target: Target,
    action: Action,
  },
  Completed {
    target: Target,
    sequence: u64,
    result: Result<Completion, SavedBrowseError>,
  },
}

pub fn target(state: &State) -> Target {
  Target {
    session: state.kernel.request_gate.current_session(),
    presentation: state.saved_browse.presentation,
    query: state
      .full
      .as_ref()
      .and_then(|full| full.browse.browser.model().identity().map(str::to_owned)),
  }
}

pub fn message(state: &State, action: Action) -> AppMessage {
  AppMessage::SavedBrowse(Message::Action {
    target: target(state),
    action,
  })
}

fn current(state: &State, receipt: &Target) -> bool {
  *receipt == target(state) && !state.kernel.sdk.content_mutations_blocked()
}

pub fn current_preferences(state: &State) -> Option<BrowsePreferences> {
  matches!(state.shell.destination, Destination::Library { .. })
    .then(|| {
      state
        .full
        .as_ref()
        .map(|full| super::browse::preferences(&full.browse, &state.kernel))
    })
    .flatten()
}

pub fn modified(state: &State) -> bool {
  state
    .full
    .as_ref()
    .and_then(|full| full.browse.saved.as_ref())
    .is_some_and(|saved| {
      current_preferences(state).is_some_and(|preferences| preferences != saved.filter.preferences)
    })
}

/// Complete labels include values unavailable in a presentation's ordinary toolbar.
pub fn conditions(
  state: &State,
  preferences: &BrowsePreferences,
) -> Vec<(Condition, String, bool)> {
  let defaults = BrowsePreferences::default();
  [
    (
      Condition::Sort,
      "saved-filters-sort",
      state.t(match preferences.sort {
        VideoLibrarySort::Title => "browse-sort-title",
        VideoLibrarySort::RecentlyAdded => "browse-sort-recently-added",
        VideoLibrarySort::ReleaseDate => "browse-sort-release-date",
      }),
      preferences.sort != defaults.sort,
    ),
    (
      Condition::Direction,
      "saved-filters-direction",
      state.t(match preferences.sort_direction {
        VideoLibrarySortDirection::Ascending => "browse-ascending",
        VideoLibrarySortDirection::Descending => "browse-descending",
      }),
      preferences.sort_direction != defaults.sort_direction,
    ),
    (
      Condition::Played,
      "saved-filters-played",
      state.t(match preferences.played_filter {
        VideoLibraryPlayedFilter::All => "browse-all",
        VideoLibraryPlayedFilter::Played => "browse-played",
        VideoLibraryPlayedFilter::Unplayed => "browse-unplayed",
      }),
      preferences.played_filter != defaults.played_filter,
    ),
    (
      Condition::Favorites,
      "saved-filters-favorites",
      state.t(if preferences.favorites_only {
        "browse-favorites"
      } else {
        "browse-all"
      }),
      preferences.favorites_only,
    ),
    (
      Condition::Quality,
      "saved-filters-quality",
      preferences.filters.quality.map_or_else(
        || state.t("browse-all"),
        |quality| {
          state.t(match quality {
            VideoLibraryQuality::Hd => "tv-filter-hd",
            VideoLibraryQuality::FullHd => "tv-filter-full-hd",
            VideoLibraryQuality::Uhd => "tv-filter-uhd",
            VideoLibraryQuality::DolbyVision => "tv-filter-dolby-vision",
          })
        },
      ),
      preferences.filters.quality.is_some(),
    ),
    (
      Condition::Country,
      "saved-filters-country",
      preferences
        .filters
        .country
        .clone()
        .unwrap_or_else(|| state.t("browse-all")),
      preferences.filters.country.is_some(),
    ),
    (
      Condition::Genre,
      "saved-filters-genre",
      preferences
        .filters
        .genre
        .clone()
        .unwrap_or_else(|| state.t("browse-all")),
      preferences.filters.genre.is_some(),
    ),
  ]
  .into_iter()
  .map(|(condition, label, value, removable)| {
    (
      condition,
      state.format(
        "saved-filters-condition",
        &[("name", state.t(label).into()), ("value", value.into())],
      ),
      removable,
    )
  })
  .collect()
}

pub fn open(state: &mut State) -> Task<AppMessage> {
  state.saved_browse.editor = None;
  state.saved_browse.retire();
  load(state)
}

pub fn leave(state: &mut State) {
  state.saved_browse.editor = None;
  state.saved_browse.retire();
}

pub fn reset(state: &mut State) {
  state.saved_browse = Surface::default();
}

pub fn reconcile(state: &mut State) {
  // A failed/cancelled account handoff can retain this UI session. Retire its
  // cancelled work now, so an ignored completion cannot leave controls busy.
  if state.saved_browse.pending.is_some()
    && (state.kernel.sdk.content_mutations_blocked()
      || state
        .saved_browse
        .token
        .as_ref()
        .is_some_and(|token| token.is_cancelled()))
  {
    state.saved_browse.retire();
    let error = UiText::new("saved-filters-stale");
    if let Some(editor) = &mut state.saved_browse.editor {
      editor.error = Some(error);
    } else {
      state.saved_browse.error = Some(error);
    }
  }
  if let Some(saved) = state
    .full
    .as_mut()
    .and_then(|full| full.browse.saved.as_mut())
  {
    if state.saved_browse.loaded {
      if let Some(record) = state
        .saved_browse
        .records
        .iter()
        .find(|record| record.id == saved.filter.id)
      {
        saved.filter.name.clone_from(&record.name);
        saved.deleted = false;
      } else {
        saved.deleted = true;
      }
    }
  }
}

fn begin(state: &mut State, action: PendingAction) -> Option<(Target, u64, Arc<OperationToken>)> {
  state.saved_browse.cancel();
  let token = match state.kernel.sdk.new_operation_token() {
    Ok(token) => token,
    Err(_) => {
      state.saved_browse.error = Some(UiText::new("saved-filters-stale"));
      return None;
    }
  };
  state.saved_browse.presentation = state.saved_browse.presentation.wrapping_add(1);
  state.saved_browse.pending = Some(action);
  state.saved_browse.error = None;
  state.saved_browse.token = Some(Arc::clone(&token));
  Some((target(state), state.saved_browse.sequence, token))
}

fn load(state: &mut State) -> Task<AppMessage> {
  let Some((target, sequence, token)) = begin(state, PendingAction::Load) else {
    return Task::none();
  };
  let sdk = Arc::clone(&state.kernel.sdk);
  Task::perform(
    async move { sdk.saved_browse_list(token).await.map(Completion::Loaded) },
    move |result| {
      AppMessage::SavedBrowse(Message::Completed {
        target,
        sequence,
        result,
      })
    },
  )
}

fn error_text(error: SavedBrowseError, pending: PendingAction) -> UiText {
  UiText::new(match error {
    SavedBrowseError::InvalidName => "saved-filters-invalid-name",
    SavedBrowseError::DuplicateName => "saved-filters-duplicate-name",
    SavedBrowseError::LimitReached => "saved-filters-full",
    SavedBrowseError::Missing => "saved-filters-stale",
    SavedBrowseError::LibraryUnavailable => "saved-filters-library-unavailable",
    SavedBrowseError::Sdk(
      jellypilot_sdk::SdkError::Stale
      | jellypilot_sdk::SdkError::Cancelled
      | jellypilot_sdk::SdkError::NoActiveProfile
      | jellypilot_sdk::SdkError::OperationInProgress,
    ) => "saved-filters-stale",
    SavedBrowseError::Sdk(_) => match pending {
      PendingAction::Load => "saved-filters-load-failed",
      PendingAction::Apply(_) => "saved-filters-apply-failed",
      _ => "saved-filters-save-failed",
    },
  })
}

fn open_save(state: &mut State) -> Task<AppMessage> {
  let (
    Some(preferences),
    Some(jellypilot_core::browse_model::BrowseSource::Library { shortcut, .. }),
  ) = (
    current_preferences(state),
    super::shell::browse_source(state),
  )
  else {
    return Task::none();
  };
  let collection_type = match shortcut.collection_type.as_str() {
    "movies" => VideoLibraryKind::Movies,
    "tvshows" => VideoLibraryKind::TvShows,
    _ => return Task::none(),
  };
  state.saved_browse.retire();
  state.saved_browse.error = None;
  state.saved_browse.editor = Some(Editor {
    name: String::new(),
    kind: EditorKind::Save(Box::new(SavedBrowseDraft {
      name: String::new(),
      library_id: shortcut.id,
      library_name: shortcut.name,
      collection_type,
      preferences,
    })),
    error: None,
  });
  iced::widget::operation::focus(iced::widget::Id::new(
    super::view::saved_browse::EDITOR_INPUT_ID,
  ))
}

pub fn update(state: &mut State, message: Message) -> Task<AppMessage> {
  match message {
    Message::Action {
      target: receipt,
      action,
    } => {
      if !current(state, &receipt) {
        return Task::none();
      }
      if matches!(action, Action::CloseEditor) {
        let origin = state
          .saved_browse
          .editor
          .as_ref()
          .map(|editor| match &editor.kind {
            EditorKind::Save(_) => {
              iced::widget::Id::new(super::view::saved_browse::SAVE_CURRENT_ID)
            }
            EditorKind::Rename(record) => {
              super::view::saved_browse::record_id(record.id, RecordAction::Rename)
            }
          });
        state.saved_browse.editor = None;
        state.saved_browse.retire();
        return origin.map_or_else(Task::none, iced::widget::operation::focus);
      }
      if matches!(action, Action::Back) {
        return if state.shell.destination == Destination::SavedBrowse {
          super::shell::navigate_back(state)
        } else {
          Task::none()
        };
      }
      if state.saved_browse.pending.is_some() {
        return Task::none();
      }
      match action {
        Action::Back => Task::none(),
        Action::Reload => load(state),
        Action::SaveCurrent => open_save(state),
        Action::NameChanged(value) => {
          if let Some(editor) = &mut state.saved_browse.editor {
            editor.name = value;
            editor.error = None;
          }
          Task::none()
        }
        Action::Submit => {
          let Some(editor) = state.saved_browse.editor.clone() else {
            return Task::none();
          };
          let pending = match &editor.kind {
            EditorKind::Save(_) => PendingAction::Save,
            EditorKind::Rename(record) => PendingAction::Rename(record.id),
          };
          let Some((target, sequence, token)) = begin(state, pending) else {
            return Task::none();
          };
          let sdk = Arc::clone(&state.kernel.sdk);
          Task::perform(
            async move {
              match editor.kind {
                EditorKind::Save(mut draft) => {
                  draft.name = editor.name;
                  sdk.saved_browse_save(token, *draft).await
                }
                EditorKind::Rename(record) => {
                  sdk.saved_browse_rename(token, record.id, editor.name).await
                }
              }
              .map(Completion::Written)
            },
            move |result| {
              AppMessage::SavedBrowse(Message::Completed {
                target,
                sequence,
                result,
              })
            },
          )
        }
        Action::Record { id, action } => {
          let Some(record) = state
            .saved_browse
            .records
            .iter()
            .find(|record| record.id == id)
            .cloned()
          else {
            return Task::none();
          };
          if action == RecordAction::Rename {
            state.saved_browse.retire();
            state.saved_browse.editor = Some(Editor {
              name: record.name.clone(),
              kind: EditorKind::Rename(record),
              error: None,
            });
            return iced::widget::operation::focus(iced::widget::Id::new(
              super::view::saved_browse::EDITOR_INPUT_ID,
            ));
          }
          let pending = if action == RecordAction::Apply {
            PendingAction::Apply(id)
          } else {
            PendingAction::Delete(id)
          };
          let Some((target, sequence, token)) = begin(state, pending) else {
            return Task::none();
          };
          let sdk = Arc::clone(&state.kernel.sdk);
          Task::perform(
            async move {
              if action == RecordAction::Apply {
                sdk
                  .saved_browse_resolve_for_apply(token, id)
                  .await
                  .map(|resolved| Completion::Resolved(Box::new(resolved)))
              } else {
                sdk
                  .saved_browse_delete(token, id)
                  .await
                  .map(|_| Completion::Deleted(id))
              }
            },
            move |result| {
              AppMessage::SavedBrowse(Message::Completed {
                target,
                sequence,
                result,
              })
            },
          )
        }
        Action::RemoveCondition(condition) => remove(state, Some(condition)),
        Action::ClearConditions => remove(state, None),
        Action::CloseEditor => Task::none(),
      }
    }
    Message::Completed {
      target: receipt,
      sequence,
      result,
    } => {
      if !current(state, &receipt)
        || sequence != state.saved_browse.sequence
        || state
          .saved_browse
          .token
          .as_ref()
          .is_none_or(|token| token.is_cancelled())
      {
        return Task::none();
      }
      let Some(pending) = state.saved_browse.pending.take() else {
        return Task::none();
      };
      state.saved_browse.token = None;
      state.saved_browse.presentation = state.saved_browse.presentation.wrapping_add(1);
      match result {
        Err(error) => {
          let message = error_text(error, pending);
          if let Some(editor) = &mut state.saved_browse.editor {
            editor.error = Some(message);
          } else {
            state.saved_browse.error = Some(message);
          }
          Task::none()
        }
        Ok(Completion::Loaded(records)) => {
          state.saved_browse.records = records;
          state.saved_browse.loaded = true;
          Task::none()
        }
        Ok(Completion::Written(record)) => {
          let id = record.id;
          if let Some(previous) = state
            .saved_browse
            .records
            .iter_mut()
            .find(|previous| previous.id == id)
          {
            *previous = record;
          } else {
            state.saved_browse.records.push(record);
          }
          state.saved_browse.editor = None;
          let focus = if pending == PendingAction::Save {
            iced::widget::Id::new(super::view::saved_browse::SAVE_CURRENT_ID)
          } else {
            super::view::saved_browse::record_id(id, RecordAction::Rename)
          };
          iced::widget::operation::focus(focus)
        }
        Ok(Completion::Deleted(id)) => {
          let index = state
            .saved_browse
            .records
            .iter()
            .position(|record| record.id == id)
            .unwrap_or(0);
          state.saved_browse.records.retain(|record| record.id != id);
          let focus = state
            .saved_browse
            .records
            .get(index.min(state.saved_browse.records.len().saturating_sub(1)))
            .map_or_else(
              || iced::widget::Id::new(super::view::saved_browse::BACK_ID),
              |record| super::view::saved_browse::record_id(record.id, RecordAction::Delete),
            );
          iced::widget::operation::focus(focus)
        }
        Ok(Completion::Resolved(resolved)) => {
          if let Some(record) = state
            .saved_browse
            .records
            .iter_mut()
            .find(|record| record.id == resolved.filter.id)
          {
            *record = resolved.filter.clone();
          }
          super::shell::apply_saved_browse(state, *resolved)
        }
      }
    }
  }
}

fn remove(state: &mut State, condition: Option<Condition>) -> Task<AppMessage> {
  let Some(mut preferences) = current_preferences(state) else {
    return Task::none();
  };
  let defaults = BrowsePreferences::default();
  match condition {
    None => preferences = defaults,
    Some(Condition::Sort) => preferences.sort = defaults.sort,
    Some(Condition::Direction) => preferences.sort_direction = defaults.sort_direction,
    Some(Condition::Played) => preferences.played_filter = defaults.played_filter,
    Some(Condition::Favorites) => preferences.favorites_only = false,
    Some(Condition::Quality) => preferences.filters.quality = None,
    Some(Condition::Country) => preferences.filters.country = None,
    Some(Condition::Genre) => preferences.filters.genre = None,
  }
  let source = super::shell::browse_source(state);
  let Some(full) = state.full.as_mut() else {
    return Task::none();
  };
  super::browse::commit_preferences(&mut full.browse, &mut state.kernel, source, preferences)
}
