use std::sync::Arc;

use jellypilot_core::viewing_queue::{QueueEditError, QueueEntryId, QueueMove, QueuePlacement};

use super::{
  AdjacentAvailability, AdjacentView, ControllerOperation, ControllerRequest, IntroAvailability,
  Playable, PlaybackSelection, PlaybackSession, PlaybackStartPosition, PlaybackStep,
};

pub(super) struct QueuedPlayback {
  pub playable: Playable,
  pub position: PlaybackStartPosition,
}

/// Small presentation metadata. Rich playback payloads remain session-owned.
#[derive(Clone, Debug, PartialEq)]
pub struct UpcomingQueueEntry {
  pub id: QueueEntryId,
  pub item_id: String,
  pub title: String,
  pub item_type: String,
  pub position: PlaybackStartPosition,
  pub artwork_image_id: Option<String>,
}

/// The entries allocation is shared between periodic transport snapshots and
/// replaced only when the queue changes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpcomingQueueView {
  pub revision: u64,
  pub entries: Arc<[UpcomingQueueEntry]>,
  pub pending: Option<QueueEntryId>,
}

fn identity(playable: &Playable) -> (&str, &str) {
  match playable {
    Playable::Library(item) => (&item.name, &item.item_type),
    Playable::Detail(item) => (&item.name, &item.item_type),
    Playable::Media(item) => (&item.name, &item.item_type),
  }
}

impl PlaybackSession {
  fn queue_editable(&self, revision: u64) -> Result<(), QueueEditError> {
    if revision != self.upcoming.revision() {
      return Err(QueueEditError::Stale);
    }
    if self
      .in_flight
      .as_ref()
      .is_some_and(|flight| matches!(flight.operation, ControllerOperation::Start { .. }))
      || self
        .pending
        .iter()
        .any(|pending| matches!(pending.operation, ControllerOperation::Start { .. }))
    {
      return Err(QueueEditError::Busy);
    }
    Ok(())
  }

  /// Adds or repositions a real movie/episode without changing current playback.
  pub fn queue_insert(
    &mut self,
    revision: u64,
    item: Playable,
    position: PlaybackStartPosition,
    placement: QueuePlacement,
  ) -> Result<QueueEntryId, QueueEditError> {
    self.queue_editable(revision)?;
    if !matches!(identity(&item).1, "Movie" | "Episode")
      || matches!(&item, Playable::Detail(detail) if !detail.can_play)
      || !matches!(
        position,
        PlaybackStartPosition::Beginning | PlaybackStartPosition::Resume
      )
    {
      return Err(QueueEditError::InvalidItem);
    }
    let result = self.upcoming.insert(
      revision,
      item.item_id().to_owned(),
      QueuedPlayback {
        playable: item,
        position,
      },
      placement,
    );
    self.sync_upcoming_view();
    result
  }

  pub fn queue_move(
    &mut self,
    revision: u64,
    id: QueueEntryId,
    direction: QueueMove,
  ) -> Result<(), QueueEditError> {
    self.queue_editable(revision)?;
    let result = self.upcoming.move_entry(revision, id, direction);
    self.sync_upcoming_view();
    result
  }

  pub fn queue_remove(&mut self, revision: u64, id: QueueEntryId) -> Result<(), QueueEditError> {
    self.queue_editable(revision)?;
    let result = self.upcoming.remove(revision, id);
    self.sync_upcoming_view();
    result
  }

  pub fn queue_clear(&mut self, revision: u64) -> Result<(), QueueEditError> {
    self.queue_editable(revision)?;
    let result = self.upcoming.clear(revision);
    self.sync_upcoming_view();
    result
  }

  /// Borrows the payload to resolve existing per-series playback preferences.
  pub fn queued_playable(&self, id: QueueEntryId) -> Option<&Playable> {
    self
      .upcoming
      .entries()
      .iter()
      .find(|entry| entry.id == id)
      .map(|entry| &entry.value.playable)
  }

  pub(super) fn sync_upcoming_view(&mut self) {
    if self.upcoming_view.revision == self.upcoming.revision() {
      return;
    }
    self.upcoming_view = UpcomingQueueView {
      revision: self.upcoming.revision(),
      entries: self
        .upcoming
        .entries()
        .iter()
        .map(|entry| {
          let (title, item_type) = identity(&entry.value.playable);
          UpcomingQueueEntry {
            id: entry.id,
            item_id: entry.media_id.clone(),
            title: title.to_owned(),
            item_type: item_type.to_owned(),
            position: entry.value.position,
            artwork_image_id: entry.value.playable.image_id().map(str::to_owned),
          }
        })
        .collect(),
      pending: self.upcoming.pending(),
    };
  }

  pub(super) fn effective_adjacent(&self) -> AdjacentView {
    let mut adjacent = self.adjacent.view();
    if let Some(entry) = self.upcoming.entries().first() {
      adjacent.next = AdjacentAvailability::Available {
        title: identity(&entry.value.playable).0.to_owned(),
      };
    }
    adjacent
  }

  pub(super) fn play_queued(
    &mut self,
    revision: u64,
    id: QueueEntryId,
    intro: IntroAvailability,
    continue_playback: bool,
  ) -> PlaybackStep {
    if !self.engine_available
      || self.quitting
      || !self.playback_admitted
      || self
        .in_flight
        .as_ref()
        .is_some_and(|flight| matches!(flight.operation, ControllerOperation::Start { .. }))
    {
      return PlaybackStep::ignored();
    }
    let Ok(claim) = self.upcoming.claim(revision, id) else {
      return PlaybackStep::ignored();
    };
    let Some(entry) = self.upcoming.claimed(claim) else {
      return PlaybackStep::ignored();
    };
    let mut request = ControllerRequest::start(
      entry.playable.clone(),
      entry.position,
      intro,
      PlaybackSelection::default(),
      continue_playback,
    );
    if let ControllerOperation::Start { queue_claim, .. } = &mut request.operation {
      *queue_claim = Some(claim);
    }
    self.enqueue(request)
  }
}
