//! Shared Library Image streaming executor (ADR 0028): a surface's prepare
//! pass submits its planned loads here, each image settles into one message
//! as it completes, and the stream ends with a single sanitized aggregate for
//! the diagnostics event. Surface-agnostic by construction — it reads only
//! kernel machinery (adapter, client, session) plus the caller's load plan.

use std::sync::{Arc, Mutex};

use iced::futures::stream::{self, StreamExt};
use iced::Task;
use jellypilot_core::artwork_loader::{plan_artwork_loads, PlannedArtworkLoad};
use jellypilot_media_server::artwork::{ArtworkLoadSummary, LoadLane};
use jellypilot_media_server::JellyfinClient;

use super::message::{ArtworkLoadCompletion, Message};

enum ArtworkStreamEvent {
  Loaded(ArtworkLoadCompletion),
  Completed(ArtworkLoadSummary),
}

/// Streams a surface's Library Image loads visible-first, emitting one message
/// per image as it settles and `Message::ArtworkStreamCompleted` with this
/// stream's own sanitized aggregate at the end. `summary` seeds the aggregate
/// with synchronous cache hits from the prepare pass.
pub(crate) fn stream_artwork_loads<F>(
  adapter: Arc<jellypilot_media_server::artwork::ArtworkAdapter>,
  client: Arc<JellyfinClient>,
  session: jellypilot_core::request_gate::SessionToken,
  loads: Vec<PlannedArtworkLoad>,
  summary: ArtworkLoadSummary,
  make_message: F,
) -> Task<Message>
where
  F: Fn(jellypilot_core::request_gate::SessionToken, ArtworkLoadCompletion) -> Message
    + Send
    + Sync
    + 'static,
{
  if loads.is_empty() && summary == ArtworkLoadSummary::default() {
    return Task::none();
  }
  let summary = Arc::new(Mutex::new(summary));
  let planned = plan_artwork_loads(loads);
  let concurrency = planned.len();
  let completions = stream::iter(planned).map({
    let summary = Arc::clone(&summary);
    move |load| {
      let adapter = Arc::clone(&adapter);
      let client = Arc::clone(&client);
      let summary = Arc::clone(&summary);
      async move {
        let lane = if load.visible {
          LoadLane::Visible
        } else {
          LoadLane::Offscreen
        };
        let image_id = load.image_id;
        let (result, observation) = adapter
          .load_with_derived(&client, &image_id, load.size_class, load.derived, lane)
          .await;
        if let Ok(mut summary) = summary.lock() {
          summary.record(&observation);
        }
        ArtworkLoadCompletion {
          slot: load.slot,
          image_id,
          result,
        }
      }
    }
  });
  let events = completions
    // max(1): an all-cache-hit prepare yields an empty plan, and
    // buffer_unordered(0) would never emit the chained completion event.
    .buffer_unordered(concurrency.max(1))
    .map(ArtworkStreamEvent::Loaded)
    .chain(stream::once(async move {
      let summary = summary.lock().map(|summary| *summary).unwrap_or_default();
      ArtworkStreamEvent::Completed(summary)
    }));
  Task::run(events, move |event| match event {
    ArtworkStreamEvent::Loaded(completion) => make_message(session, completion),
    ArtworkStreamEvent::Completed(summary) => Message::ArtworkStreamCompleted(summary),
  })
}
