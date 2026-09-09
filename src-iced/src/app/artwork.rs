//! Native Library Image collection: core owns correlation; this module owns
//! demand controls and display handles. Pages only select images and observe them.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use iced::{task, widget::image, Task};
use jellypilot_core::image_lifecycle::{ImageChange, ImageLifecycle, ImageOutcome, ImageToken};
pub use jellypilot_core::image_lifecycle::{ImagePriority, ImageSpec, ImageStatus};
use jellypilot_core::request_gate::SessionToken;
use jellypilot_media_server::artwork::{
  ArtworkAdapter, ArtworkDemand, ArtworkDemandControl, ArtworkError, ArtworkLoadObservation,
  ArtworkLoadSettlement, ArtworkLoadSummary, ArtworkRaster, LoadLane,
};
use jellypilot_media_server::JellyfinClient;

use super::message::Message;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ArtworkSurface {
  Home,
  Browse,
  Detail,
  PersonalLists,
}
static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);

fn next_epoch() -> u64 {
  NEXT_EPOCH
    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
      value.checked_add(1)
    })
    .unwrap_or_else(|_| std::process::abort())
}

#[derive(Clone)]
pub struct ImageCompletion {
  pub session: SessionToken,
  pub token: ImageToken,
  pub result: Result<ArtworkRaster, ArtworkError>,
  pub observation: ArtworkLoadObservation,
}

pub struct ImageCell {
  pub state: ImageStatus,
  #[cfg(test)]
  pub image_id: String,
  priority: ImagePriority,
  handles: Option<ArtworkHandles>,
}

impl ImageCell {
  pub fn handle(&self) -> Option<&iced::widget::image::Handle> {
    self.handles.as_ref().map(|handles| &handles.main)
  }

  pub fn dims(&self) -> Option<(u32, u32)> {
    self
      .handles
      .as_ref()
      .map(|handles| (handles.main_width, handles.main_height))
  }

  pub fn logo_shadow(&self) -> Option<&iced::widget::image::Handle> {
    self
      .handles
      .as_ref()
      .and_then(|handles| handles.logo_shadow.as_ref())
  }
}

struct PendingImage {
  control: ArtworkDemandControl,
  task: task::Handle,
}

impl Drop for PendingImage {
  fn drop(&mut self) {
    self.task.abort();
  }
}

/// One page's live image positions, not a cache of previously visited pages.
pub struct ImageCollection {
  epoch: u64,
  session: Option<SessionToken>,
  lifecycle: ImageLifecycle,
  cells: HashMap<String, ImageCell>,
  pending: HashMap<ImageToken, PendingImage>,
  summary: ArtworkLoadSummary,
}

impl Default for ImageCollection {
  fn default() -> Self {
    Self {
      epoch: next_epoch(),
      session: None,
      lifecycle: ImageLifecycle::default(),
      cells: HashMap::new(),
      pending: HashMap::new(),
      summary: ArtworkLoadSummary::default(),
    }
  }
}

impl ImageCollection {
  pub const fn epoch(&self) -> u64 {
    self.epoch
  }

  pub fn get(&self, key: &str) -> Option<&ImageCell> {
    self.cells.get(key)
  }

  #[cfg(test)]
  pub fn is_empty(&self) -> bool {
    self.cells.is_empty()
  }

  pub fn has_loading(&self) -> bool {
    self
      .cells
      .values()
      .any(|cell| cell.priority == ImagePriority::Visible && cell.state == ImageStatus::Loading)
  }

  pub fn clear(&mut self) {
    self.epoch = next_epoch();
    self.session = None;
    self.lifecycle = ImageLifecycle::default();
    self.pending.clear();
    self.cells.clear();
  }
  /// Data changes prune obsolete selections. Geometry alone admits new demand.
  pub fn retain(&mut self, specs: &[ImageSpec]) {
    for change in self.lifecycle.retain(specs) {
      if let ImageChange::Remove { token, key } = change {
        self.pending.remove(&token);
        self.cells.remove(&key);
      }
    }
  }

  pub fn observe(
    &mut self,
    session: SessionToken,
    spec: ImageSpec,
    priority: Option<ImagePriority>,
    client: Arc<JellyfinClient>,
    adapter: Arc<ArtworkAdapter>,
    make_message: impl Fn(ImageCompletion) -> Message + Send + Sync + 'static,
  ) -> Task<Message> {
    if self.session.is_some_and(|current| current != session) {
      self.clear();
    }
    self.session = Some(session);

    // Prefetch completion retains pixels only in the adapter's budgeted cache.
    // When it becomes visible, re-enter through the authorized cache fast path;
    // that cache may have evicted the raster in the meantime.
    if priority == Some(ImagePriority::Visible)
      && self
        .cells
        .get(&spec.key)
        .is_some_and(|cell| cell.state == ImageStatus::Ready && cell.handles.is_none())
    {
      for change in self.lifecycle.observe(spec.clone(), None) {
        if let ImageChange::Remove { token, key } = change {
          self.pending.remove(&token);
          self.cells.remove(&key);
        }
      }
    }

    for change in self.lifecycle.observe(spec, priority) {
      match change {
        ImageChange::Remove { token, key } => {
          self.pending.remove(&token);
          self.cells.remove(&key);
        }
        ImageChange::Priority { token, priority } => {
          if let Some(key) = self.lifecycle.key(token) {
            if let Some(cell) = self.cells.get_mut(key) {
              cell.priority = priority;
              if priority == ImagePriority::Prefetch {
                cell.handles = None;
              }
            }
          }
          if let Some(pending) = self.pending.get(&token) {
            pending.control.set_lane(load_lane(priority));
          }
        }
        ImageChange::Load {
          token,
          spec,
          priority,
        } => {
          self.cells.insert(
            spec.key,
            ImageCell {
              state: ImageStatus::Loading,
              #[cfg(test)]
              image_id: spec.image_id.clone(),
              priority,
              handles: None,
            },
          );
          match adapter.demand(
            Arc::clone(&client),
            spec.image_id,
            spec.size_class,
            spec.derived,
            adapter.ticket(),
            load_lane(priority),
          ) {
            ArtworkDemand::Ready((result, observation)) => {
              self.settle(
                session,
                ImageCompletion {
                  session,
                  token,
                  result,
                  observation,
                },
              );
            }
            ArtworkDemand::Pending { control, receiver } => {
              let (task, handle) = Task::perform(
                async move {
                  let (result, observation) = receiver.await.unwrap_or_else(|_| {
                    (
                      Err(ArtworkError::Cancelled),
                      ArtworkLoadObservation {
                        settlement: ArtworkLoadSettlement::Cancelled,
                        duration: Duration::ZERO,
                        bytes: 0,
                      },
                    )
                  });
                  ImageCompletion {
                    session,
                    token,
                    result,
                    observation,
                  }
                },
                make_message,
              )
              .abortable();
              self.pending.insert(
                token,
                PendingImage {
                  control,
                  task: handle,
                },
              );
              return task;
            }
          }
        }
      }
    }
    Task::none()
  }

  pub fn settle(&mut self, current_session: SessionToken, completion: ImageCompletion) {
    if self.session != Some(current_session) || completion.session != current_session {
      return;
    }
    let outcome = match &completion.result {
      Ok(_) => ImageOutcome::Ready,
      Err(ArtworkError::Cancelled) => ImageOutcome::Cancelled,
      Err(_) => ImageOutcome::Failed,
    };
    let Some(key) = self.lifecycle.settle(completion.token, outcome) else {
      return;
    };
    self.pending.remove(&completion.token);
    self.summary.record(&completion.observation);
    match completion.result {
      Ok(raster) => {
        if let Some(cell) = self.cells.get_mut(&key) {
          cell.state = ImageStatus::Ready;
          if cell.priority == ImagePriority::Visible {
            cell.handles = Some(ArtworkHandles::from_raster(raster));
          }
        }
      }
      Err(ArtworkError::Cancelled) => {
        self.cells.remove(&key);
      }
      Err(_) => {
        if let Some(cell) = self.cells.get_mut(&key) {
          cell.state = ImageStatus::Failed;
        }
      }
    }
  }

  pub fn take_summary(&mut self) -> ArtworkLoadSummary {
    std::mem::take(&mut self.summary)
  }
}

fn load_lane(priority: ImagePriority) -> LoadLane {
  match priority {
    ImagePriority::Visible => LoadLane::Visible,
    ImagePriority::Prefetch => LoadLane::Offscreen,
  }
}
pub struct ArtworkHandles {
  main: image::Handle,
  main_width: u32,
  main_height: u32,
  logo_shadow: Option<image::Handle>,
}
impl ArtworkHandles {
  pub fn from_raster(raster: ArtworkRaster) -> Self {
    let (width, height, pixels, logo_shadow) = raster.into_parts();
    Self {
      main: image::Handle::from_rgba(width, height, pixels),
      main_width: width,
      main_height: height,
      logo_shadow: logo_shadow.map(|shadow| {
        let (width, height, pixels, ..) = shadow.into_parts();
        image::Handle::from_rgba(width, height, pixels)
      }),
    }
  }
}

/// Batches sanitized counters, never image work: no delay is applied to loading.
#[derive(Default)]
pub struct ImageDiagnostics {
  summary: ArtworkLoadSummary,
  scheduled: bool,
}

impl ImageDiagnostics {
  pub fn record(&mut self, summary: ArtworkLoadSummary) {
    self.summary.raster_loads = self
      .summary
      .raster_loads
      .saturating_add(summary.raster_loads);
    self.summary.memory_loads = self
      .summary
      .memory_loads
      .saturating_add(summary.memory_loads);
    self.summary.disk_loads = self.summary.disk_loads.saturating_add(summary.disk_loads);
    self.summary.network_loads = self
      .summary
      .network_loads
      .saturating_add(summary.network_loads);
    self.summary.failed_loads = self
      .summary
      .failed_loads
      .saturating_add(summary.failed_loads);
    self.summary.total_duration_millis = self
      .summary
      .total_duration_millis
      .saturating_add(summary.total_duration_millis);
    self.summary.total_bytes = self.summary.total_bytes.saturating_add(summary.total_bytes);
  }

  pub fn schedule(&mut self) -> Task<Message> {
    if self.scheduled || self.summary == ArtworkLoadSummary::default() {
      return Task::none();
    }
    self.scheduled = true;
    Task::perform(
      async { tokio::time::sleep(Duration::from_millis(250)).await },
      |()| Message::ArtworkSummaryReady,
    )
  }

  pub fn drain(&mut self) -> ArtworkLoadSummary {
    self.scheduled = false;
    std::mem::take(&mut self.summary)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use jellypilot_core::request_gate::RequestGate;
  use jellypilot_media_server::{
    artwork::ArtworkSizeClass, image_id_for_url, ImageRefKind, MediaServerProvider, SavedSession,
  };

  fn fixture() -> (Arc<JellyfinClient>, Arc<ArtworkAdapter>, ImageSpec) {
    let server_url = "https://images.example.com";
    let client = Arc::new(JellyfinClient::new());
    client.login().adopt_validated_session(&SavedSession {
      provider: MediaServerProvider::Jellyfin,
      server_url: server_url.into(),
      access_token: "token".into(),
      user_id: "user".into(),
      user_name: "user".into(),
      server_name: None,
      device_id: None,
    });
    let image_id = image_id_for_url(
      MediaServerProvider::Jellyfin,
      server_url,
      format!("{server_url}/Items/one/Images/Primary"),
      ImageRefKind::Artwork,
    )
    .expect("signed image");
    let adapter = Arc::new(ArtworkAdapter::new());
    adapter.seed_raster_for_test(
      &image_id,
      ArtworkSizeClass::Card,
      ArtworkRaster::from_raw_for_test(1, 1, vec![1, 2, 3, 255]),
    );
    (
      client,
      adapter,
      ImageSpec {
        key: "card".into(),
        image_id,
        size_class: ArtworkSizeClass::Card,
        derived: Default::default(),
      },
    )
  }

  fn observe(
    collection: &mut ImageCollection,
    client: &Arc<JellyfinClient>,
    adapter: &Arc<ArtworkAdapter>,
    spec: &ImageSpec,
    priority: ImagePriority,
  ) {
    drop(collection.observe(
      RequestGate::default().current_session(),
      spec.clone(),
      Some(priority),
      Arc::clone(client),
      Arc::clone(adapter),
      |completion| {
        Message::Home(super::super::message::HomeMessage::ArtworkLoaded(
          completion,
        ))
      },
    ));
  }

  #[test]
  fn offscreen_credit_does_not_release_a_visible_shared_portrait() {
    let (client, adapter, image) = fixture();
    let mut member = jellypilot_media_server::VideoCastMember {
      name: "Actor".to_owned(),
      role: Some("First role".to_owned()),
      image_id: Some(image.image_id),
    };
    let visible = super::super::detail::cast_image_spec(0, &member).unwrap();
    member.role = Some("Second role".to_owned());
    let offscreen = super::super::detail::cast_image_spec(1, &member).unwrap();
    let mut images = ImageCollection::default();
    observe(
      &mut images,
      &client,
      &adapter,
      &visible,
      ImagePriority::Visible,
    );
    let shown = images.get(&visible.key).unwrap().handle().unwrap().id();

    observe(
      &mut images,
      &client,
      &adapter,
      &offscreen,
      ImagePriority::Prefetch,
    );
    images.retain(&[visible.clone(), offscreen.clone()]);

    assert_eq!(
      images.get(&visible.key).unwrap().handle().unwrap().id(),
      shown
    );
    assert!(images.get(&offscreen.key).unwrap().handle().is_none());
  }

  #[test]
  fn prefetch_drops_display_handles_and_promotion_uses_current_raster_cache() {
    let (client, adapter, spec) = fixture();
    let mut images = ImageCollection::default();
    observe(
      &mut images,
      &client,
      &adapter,
      &spec,
      ImagePriority::Prefetch,
    );
    let prefetched = images.get(&spec.key).expect("demand remains ready");
    assert_eq!(prefetched.state, ImageStatus::Ready);
    assert!(prefetched.handle().is_none());
    observe(
      &mut images,
      &client,
      &adapter,
      &spec,
      ImagePriority::Visible,
    );
    let shown = images.get(&spec.key).unwrap().handle().unwrap().id();
    images.retain(std::slice::from_ref(&spec));
    observe(
      &mut images,
      &client,
      &adapter,
      &spec,
      ImagePriority::Visible,
    );
    assert_eq!(images.get(&spec.key).unwrap().handle().unwrap().id(), shown);

    observe(
      &mut images,
      &client,
      &adapter,
      &spec,
      ImagePriority::Prefetch,
    );
    assert!(images.get(&spec.key).unwrap().handle().is_none());
    adapter.clear_caches();
    adapter.seed_raster_for_test(
      &spec.image_id,
      spec.size_class,
      ArtworkRaster::from_raw_for_test(2, 1, vec![1, 2, 3, 255, 4, 5, 6, 255]),
    );
    observe(
      &mut images,
      &client,
      &adapter,
      &spec,
      ImagePriority::Visible,
    );
    assert_eq!(images.get(&spec.key).unwrap().dims(), Some((2, 1)));
  }

  #[test]
  fn promoting_prefetch_revalidates_authorization_before_showing_cached_pixels() {
    let (client, adapter, spec) = fixture();
    let mut images = ImageCollection::default();
    observe(
      &mut images,
      &client,
      &adapter,
      &spec,
      ImagePriority::Prefetch,
    );
    client.login().adopt_validated_session(&SavedSession {
      provider: MediaServerProvider::Jellyfin,
      server_url: "https://other.example.com".into(),
      access_token: "other-token".into(),
      user_id: "other".into(),
      user_name: "other".into(),
      server_name: None,
      device_id: None,
    });
    observe(
      &mut images,
      &client,
      &adapter,
      &spec,
      ImagePriority::Visible,
    );
    let rejected = images.get(&spec.key).unwrap();
    assert_eq!(rejected.state, ImageStatus::Failed);
    assert!(rejected.handle().is_none());
  }
}
