use std::collections::BTreeMap;

use jellypilot_core::{
  LibraryBrowseAction, LibraryBrowseCommand, LibraryBrowseCore, LibraryBrowseCoreError,
  LibraryBrowseLoadToken, LibraryBrowseMode, LibraryBrowsePageOutcome, LibraryBrowseStatus,
};

/// One display position and its payload, if the corresponding page is loaded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryItemSlot<T> {
  /// Stable position in the current result ordering.
  pub display_index: u32,
  /// Item payload, or `None` while the native transport is loading its page.
  pub item: Option<T>,
}

/// Inputs accepted by the native Library Browser adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryBrowseInput<T> {
  /// Configures the complete identity of the active browse query.
  Configure { source_id: String },
  /// Requests the next sequential page when one is available.
  LoadNext,
  /// Reports the item positions currently represented by a virtual GTK view.
  WindowChanged { display_indexes: Vec<u32> },
  /// Retries retained retryable failures.
  Retry,
  /// Returns one completed page load to the portable reducer.
  PageLoaded {
    token: LibraryBrowseLoadToken,
    start_index: u32,
    limit: u32,
    total_record_count: u32,
    has_more: bool,
    items: Vec<T>,
  },
  /// Returns one failed page load to the portable reducer.
  PageFailed {
    token: LibraryBrowseLoadToken,
    message: String,
    retryable: bool,
  },
}

/// Side effects the GTK shell must execute in the emitted order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryBrowseEffect {
  /// Moves the native viewport back to its initial position.
  ResetViewport,
  /// Requests a page from the environment-specific media-server adapter.
  RequestPage {
    token: LibraryBrowseLoadToken,
    start_index: u32,
    limit: u32,
  },
  /// Cancels a page request that is no longer relevant.
  CancelPage { token: LibraryBrowseLoadToken },
}

/// GTK-friendly projection of the portable Library Browser state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryBrowseView<T> {
  /// No source has been configured.
  Inactive,
  /// Page zero is currently loading.
  Loading,
  /// The configured source has no items.
  Empty,
  /// Page zero failed.
  Failed {
    message: String,
    retryable: bool,
    retry_busy: bool,
  },
  /// Loaded items and continuation metadata.
  Ready {
    visible_items: Vec<LibraryItemSlot<T>>,
    mode: LibraryBrowseMode,
    total_record_count: u32,
    is_fetching_more: bool,
    can_load_next: bool,
    load_more_failure: Option<String>,
    retry_busy: bool,
  },
}

/// Native adapter around the framework-independent browse reducer.
#[derive(Clone, Debug)]
pub struct NativeLibraryBrowse<T> {
  core: LibraryBrowseCore,
  pages: BTreeMap<u32, Vec<T>>,
  pending: BTreeMap<LibraryBrowseLoadToken, PendingPage>,
}

impl<T> Default for NativeLibraryBrowse<T> {
  fn default() -> Self {
    Self {
      core: LibraryBrowseCore::default(),
      pages: BTreeMap::new(),
      pending: BTreeMap::new(),
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingPage {
  start_index: u32,
  limit: u32,
}

impl<T: Clone> NativeLibraryBrowse<T> {
  /// Applies an input and returns ordered effects for the GTK shell.
  pub fn handle(
    &mut self,
    input: LibraryBrowseInput<T>,
  ) -> Result<Vec<LibraryBrowseEffect>, LibraryBrowseCoreError> {
    let action = match input {
      LibraryBrowseInput::Configure { source_id } => LibraryBrowseAction::Configure {
        source_id,
        enabled: true,
      },
      LibraryBrowseInput::LoadNext => LibraryBrowseAction::LoadNext,
      LibraryBrowseInput::WindowChanged { display_indexes } => {
        LibraryBrowseAction::WindowChanged { display_indexes }
      }
      LibraryBrowseInput::Retry => LibraryBrowseAction::Retry,
      LibraryBrowseInput::PageLoaded {
        token,
        start_index,
        limit,
        total_record_count,
        has_more,
        items,
      } => {
        let pending = self.pending.get(&token).copied();
        let item_count = u32::try_from(items.len()).unwrap_or(u32::MAX);
        let action = LibraryBrowseAction::PageSettled {
          token,
          outcome: LibraryBrowsePageOutcome::Loaded {
            start_index,
            limit,
            total_record_count,
            item_count,
            has_more,
          },
        };
        let update = self.core.dispatch(action)?;
        self.pending.remove(&token);
        let page_is_valid = !update.commands.iter().any(|command| {
          matches!(
            command,
            LibraryBrowseCommand::ReleasePages { page_starts }
              if page_starts.contains(&start_index)
          )
        });
        if page_is_valid && pending == Some(PendingPage { start_index, limit }) {
          self.pages.insert(start_index, items);
        }
        return Ok(self.apply_commands(update.commands));
      }
      LibraryBrowseInput::PageFailed {
        token,
        message,
        retryable,
      } => {
        let action = LibraryBrowseAction::PageSettled {
          token,
          outcome: LibraryBrowsePageOutcome::Failed {
            failure: jellypilot_core::LibraryBrowseFailure { message, retryable },
          },
        };
        let update = self.core.dispatch(action)?;
        self.pending.remove(&token);
        return Ok(self.apply_commands(update.commands));
      }
    };

    let update = self.core.dispatch(action)?;
    Ok(self.apply_commands(update.commands))
  }

  /// Returns a GTK-friendly immutable projection of the current state.
  #[must_use]
  pub fn view(&self) -> LibraryBrowseView<T> {
    let snapshot = self.core.snapshot();
    match snapshot.status {
      LibraryBrowseStatus::Inactive => LibraryBrowseView::Inactive,
      LibraryBrowseStatus::Loading => LibraryBrowseView::Loading,
      LibraryBrowseStatus::Empty { .. } => LibraryBrowseView::Empty,
      LibraryBrowseStatus::InitialFailure {
        failure,
        retry_busy,
      } => LibraryBrowseView::Failed {
        message: failure.message,
        retryable: failure.retryable,
        retry_busy,
      },
      LibraryBrowseStatus::Ready {
        mode,
        total_record_count,
        is_fetching_more,
        can_load_next,
        load_more_failure,
        retry_busy,
        ..
      } => LibraryBrowseView::Ready {
        visible_items: snapshot
          .slots
          .iter()
          .map(|slot| {
            let item = usize::try_from(slot.index_within_page)
              .ok()
              .and_then(|index| self.pages.get(&slot.page_start)?.get(index).cloned());
            LibraryItemSlot {
              display_index: slot.display_index,
              item,
            }
          })
          .collect(),
        mode,
        total_record_count,
        is_fetching_more,
        can_load_next,
        load_more_failure: load_more_failure.map(|failure| failure.message),
        retry_busy,
      },
    }
  }

  #[cfg(test)]
  pub(crate) fn retained_page_count(&self) -> usize {
    self.pages.len()
  }

  #[cfg(test)]
  pub(crate) fn retained_item_count(&self) -> usize {
    self.pages.values().map(Vec::len).sum()
  }

  fn apply_commands(&mut self, commands: Vec<LibraryBrowseCommand>) -> Vec<LibraryBrowseEffect> {
    commands
      .into_iter()
      .filter_map(|command| match command {
        LibraryBrowseCommand::ResetViewport => Some(LibraryBrowseEffect::ResetViewport),
        LibraryBrowseCommand::LoadPage {
          token,
          start_index,
          limit,
          ..
        } => {
          self
            .pending
            .insert(token, PendingPage { start_index, limit });
          Some(LibraryBrowseEffect::RequestPage {
            token,
            start_index,
            limit,
          })
        }
        LibraryBrowseCommand::CancelLoad { token } => {
          self.pending.remove(&token);
          Some(LibraryBrowseEffect::CancelPage { token })
        }
        LibraryBrowseCommand::ReleasePages { page_starts } => {
          for page_start in page_starts {
            self.pages.remove(&page_start);
          }
          None
        }
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[derive(Clone, Debug, Eq, PartialEq)]
  struct LibraryItem {
    id: String,
    title: String,
  }

  fn requested_page(effects: &[LibraryBrowseEffect]) -> (LibraryBrowseLoadToken, u32, u32) {
    effects
      .iter()
      .find_map(|effect| match effect {
        LibraryBrowseEffect::RequestPage {
          token,
          start_index,
          limit,
        } => Some((*token, *start_index, *limit)),
        LibraryBrowseEffect::ResetViewport | LibraryBrowseEffect::CancelPage { .. } => None,
      })
      .expect("a page request should be emitted")
  }

  fn items(start_index: u32, count: u32) -> Vec<LibraryItem> {
    (start_index..start_index + count)
      .map(|index| LibraryItem {
        id: format!("item-{index}"),
        title: format!("Library item {index}"),
      })
      .collect()
  }

  #[test]
  fn configure_requests_bootstrap_page_through_portable_core() {
    let mut browse = NativeLibraryBrowse::<LibraryItem>::default();

    let effects = browse
      .handle(LibraryBrowseInput::Configure {
        source_id: "gtk-walking-slice".to_owned(),
      })
      .expect("source should configure");

    assert!(matches!(
      effects.first(),
      Some(LibraryBrowseEffect::ResetViewport)
    ));
    assert_eq!(requested_page(&effects).1, 0);
    assert_eq!(browse.view(), LibraryBrowseView::Loading);
  }

  #[test]
  fn loaded_page_is_projected_with_caller_owned_payloads() {
    let mut browse = NativeLibraryBrowse::<LibraryItem>::default();
    let effects = browse
      .handle(LibraryBrowseInput::Configure {
        source_id: "gtk-walking-slice".to_owned(),
      })
      .expect("source should configure");
    let (token, start_index, limit) = requested_page(&effects);

    browse
      .handle(LibraryBrowseInput::PageLoaded {
        token,
        start_index,
        limit,
        total_record_count: 30,
        has_more: true,
        items: items(0, 24),
      })
      .expect("page should settle");

    assert!(matches!(
      browse.view(),
      LibraryBrowseView::Ready {
        ref visible_items,
        total_record_count: 30,
        can_load_next: true,
        ..
      } if visible_items.len() == 24
        && visible_items.iter().all(|slot| slot.item.is_some())
    ));
  }

  #[test]
  fn load_next_appends_the_second_page() {
    let mut browse = NativeLibraryBrowse::default();
    let bootstrap = browse
      .handle(LibraryBrowseInput::Configure {
        source_id: "gtk-walking-slice".to_owned(),
      })
      .expect("source should configure");
    let (token, start_index, limit) = requested_page(&bootstrap);
    browse
      .handle(LibraryBrowseInput::PageLoaded {
        token,
        start_index,
        limit,
        total_record_count: 30,
        has_more: true,
        items: items(0, 24),
      })
      .expect("bootstrap should settle");

    let next = browse
      .handle(LibraryBrowseInput::LoadNext)
      .expect("load next should dispatch");
    let (token, start_index, limit) = requested_page(&next);
    assert_eq!(start_index, 24);
    browse
      .handle(LibraryBrowseInput::PageLoaded {
        token,
        start_index,
        limit,
        total_record_count: 30,
        has_more: false,
        items: items(24, 6),
      })
      .expect("second page should settle");

    assert!(matches!(
      browse.view(),
      LibraryBrowseView::Ready {
        ref visible_items,
        can_load_next: false,
        ..
      } if visible_items.len() == 30
        && visible_items.iter().all(|slot| slot.item.is_some())
    ));
  }

  #[test]
  fn virtual_window_loads_and_projects_requested_slots() {
    let mut browse = NativeLibraryBrowse::default();
    let bootstrap = browse
      .handle(LibraryBrowseInput::Configure {
        source_id: "gtk-walking-slice".to_owned(),
      })
      .expect("source should configure");
    let (token, start_index, limit) = requested_page(&bootstrap);
    browse
      .handle(LibraryBrowseInput::PageLoaded {
        token,
        start_index,
        limit,
        total_record_count: 240,
        has_more: true,
        items: items(0, 24),
      })
      .expect("bootstrap should settle");

    let window = browse
      .handle(LibraryBrowseInput::WindowChanged {
        display_indexes: vec![48, 49],
      })
      .expect("virtual window should dispatch");
    assert!(matches!(
      browse.view(),
      LibraryBrowseView::Ready {
        ref visible_items,
        mode: LibraryBrowseMode::Virtual,
        ..
      } if visible_items.len() == 2
        && visible_items.iter().all(|slot| slot.item.is_none())
    ));
    let (token, start_index, limit) = requested_page(&window);
    assert_eq!(start_index, 48);
    browse
      .handle(LibraryBrowseInput::PageLoaded {
        token,
        start_index,
        limit,
        total_record_count: 240,
        has_more: true,
        items: items(48, 24),
      })
      .expect("visible page should settle");

    assert!(matches!(
      browse.view(),
      LibraryBrowseView::Ready {
        ref visible_items,
        mode: LibraryBrowseMode::Virtual,
        ..
      } if visible_items
        .iter()
        .filter_map(|slot| slot.item.as_ref().map(|item| item.id.as_str()))
        .eq(["item-48", "item-49"])
        && visible_items
          .iter()
          .map(|slot| slot.display_index)
          .eq([48, 49])
    ));
  }

  #[test]
  fn failed_bootstrap_can_be_retried() {
    let mut browse = NativeLibraryBrowse::<LibraryItem>::default();
    let bootstrap = browse
      .handle(LibraryBrowseInput::Configure {
        source_id: "gtk-walking-slice".to_owned(),
      })
      .expect("source should configure");
    let (token, _, _) = requested_page(&bootstrap);
    browse
      .handle(LibraryBrowseInput::PageFailed {
        token,
        message: "temporary source failure".to_owned(),
        retryable: true,
      })
      .expect("failure should settle");

    assert!(matches!(
      browse.view(),
      LibraryBrowseView::Failed {
        retryable: true,
        retry_busy: false,
        ..
      }
    ));
    let retry = browse
      .handle(LibraryBrowseInput::Retry)
      .expect("retry should dispatch");
    assert_eq!(requested_page(&retry).1, 0);
  }

  #[test]
  fn stale_completion_does_not_replace_active_source() {
    let mut browse = NativeLibraryBrowse::default();
    let first = browse
      .handle(LibraryBrowseInput::Configure {
        source_id: "movies".to_owned(),
      })
      .expect("first source should configure");
    let (stale_token, stale_start, stale_limit) = requested_page(&first);
    browse
      .handle(LibraryBrowseInput::Configure {
        source_id: "shows".to_owned(),
      })
      .expect("second source should configure");

    browse
      .handle(LibraryBrowseInput::PageLoaded {
        token: stale_token,
        start_index: stale_start,
        limit: stale_limit,
        total_record_count: 1,
        has_more: false,
        items: items(0, 1),
      })
      .expect("stale completion should be ignored");

    assert_eq!(browse.view(), LibraryBrowseView::Loading);
    assert!(browse.pages.is_empty());
  }

  #[test]
  fn malformed_page_metadata_releases_unusable_payload() {
    let mut browse = NativeLibraryBrowse::default();
    let bootstrap = browse
      .handle(LibraryBrowseInput::Configure {
        source_id: "movies".to_owned(),
      })
      .expect("source should configure");
    let (token, start_index, limit) = requested_page(&bootstrap);

    browse
      .handle(LibraryBrowseInput::PageLoaded {
        token,
        start_index,
        limit: limit - 1,
        total_record_count: 1,
        has_more: false,
        items: items(0, 1),
      })
      .expect("malformed completion should become a retained failure");

    assert!(browse.pages.is_empty());
    assert!(matches!(
      browse.view(),
      LibraryBrowseView::Failed {
        retryable: false,
        ..
      }
    ));
  }
}
