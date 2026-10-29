//! Demand-owned atlas previews with a separate, already-resolved stream decoder.

use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::sync::{
  atomic::{AtomicU64, Ordering},
  Arc,
};
use std::time::Duration;

use crate::preview::Runtime;
use iced::futures::SinkExt;
use iced::widget::image::Handle;
use iced::Subscription;
use jellypilot_core::request_gate::SessionToken;
use jellypilot_core::seek_preview::{AtlasLayout, PreviewTile};
use jellypilot_media_server::{JellyfinClient, JellyfinError, TrickplayManifest};
use jellypilot_mpv::playback::{tools::PlaybackFileToken, PlaybackPreviewSource};

use super::{AppMessage, Message};
use crate::app::state::{PlaybackControllerHandle, State};

#[derive(Clone, PartialEq, Eq)]
struct Context {
  session: SessionToken,
  generation: u64,
  file: PlaybackFileToken,
  seekable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum RequestKind {
  Manifest,
  Atlas(u32),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Request {
  token: u64,
  kind: RequestKind,
}

impl Request {
  fn new(kind: RequestKind) -> Self {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    Self {
      token: NEXT.fetch_add(1, Ordering::Relaxed),
      kind,
    }
  }
}

#[derive(Default)]
enum Metadata {
  #[default]
  Unrequested,
  Ready(TrickplayManifest, AtlasLayout),
  Missing,
  Blocked,
}

#[derive(Clone)]
enum Asset<T> {
  Ready(T),
  Unavailable,
  /// Authentication/session failures must not trigger another authenticated read.
  Blocked,
}

impl<T> Asset<T> {
  fn from_result(result: Result<T, JellyfinError>) -> Self {
    match result {
      Ok(value) => Self::Ready(value),
      Err(
        JellyfinError::AuthFailed(_) | JellyfinError::NotConnected | JellyfinError::SessionNotFound,
      ) => Self::Blocked,
      Err(_) => Self::Unavailable,
    }
  }
}

#[derive(Default)]
pub(super) struct Surface {
  pub(super) interaction_generation: u64,
  context: Option<Context>,
  source: Option<PlaybackPreviewSource>,
  metadata: Metadata,
  request: Option<Request>,
  target: Option<PreviewTile>,
  // Atlas tiles share one RGBA buffer; pointer movement never copies pixels.
  atlas: Option<(u32, Handle)>,
  failed: VecDeque<u32>,
  decoder_loaded: bool,
  decode_position: Option<f64>,
  decode_target: Option<f64>,
  decoded: Option<iced_mpv_preview::Frame>,
  decode_failed: bool,
}

pub(crate) struct ImagePreview<'a> {
  pub image: &'a Handle,
  pub crop: Option<PreviewTile>,
}

#[derive(Clone)]
pub struct Completion {
  request: Request,
  result: Loaded,
}

#[derive(Clone)]
enum Loaded {
  Manifest(
    Option<PlaybackPreviewSource>,
    Asset<Option<TrickplayManifest>>,
  ),
  Atlas(Asset<Handle>),
}

impl Surface {
  fn cancel_decode(&mut self, decoder: Option<&Runtime>, retire: bool) {
    if let (Some(decoder), Some(source)) = (decoder, self.source.as_ref()) {
      if retire && self.decoder_loaded {
        decoder.retire(source.identity());
      } else if self.decode_position.is_some() {
        decoder.cancel(source.identity());
      }
    }
    if retire {
      self.decoder_loaded = false;
    }
    self.decode_position = None;
    self.decode_target = None;
    self.decoded = None;
  }

  fn reconcile(
    &mut self,
    context: Option<Context>,
    seconds: Option<f64>,
    decoder: Option<&Runtime>,
  ) {
    if self.context != context {
      self.cancel_decode(decoder, true);
      let interaction_generation = self.interaction_generation.wrapping_add(1);
      *self = Self {
        context,
        interaction_generation,
        ..Self::default()
      };
    }
    self.target = None;
    let Some(seconds) = seconds
      .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
      .filter(|_| {
        self
          .context
          .as_ref()
          .is_some_and(|context| context.seekable)
      })
    else {
      self.request = None;
      self.cancel_decode(decoder, false);
      return;
    };
    if self
      .source
      .as_ref()
      .is_some_and(|source| !source.is_current())
    {
      self.metadata = Metadata::Blocked;
      self.atlas = None;
    }
    let kind = match &self.metadata {
      Metadata::Unrequested => Some(RequestKind::Manifest),
      Metadata::Missing | Metadata::Blocked => None,
      Metadata::Ready(_, layout) => {
        self.target = layout.tile_at(seconds);
        self.target.and_then(|tile| {
          (!self
            .atlas
            .as_ref()
            .is_some_and(|(index, _)| *index == tile.atlas_index)
            && !self.failed.contains(&tile.atlas_index))
          .then_some(RequestKind::Atlas(tile.atlas_index))
        })
      }
    };
    if self.request.map(|request| request.kind) != kind {
      self.request = kind.map(Request::new);
    }
    if !self.needs_decoder() {
      self.cancel_decode(decoder, matches!(self.metadata, Metadata::Blocked));
      return;
    }
    if self.decode_failed {
      return;
    }
    let (Some(decoder), Some(source)) = (decoder, self.source.as_ref()) else {
      return;
    };
    if !source.seekable() || !source.is_current() {
      self.cancel_decode(Some(decoder), true);
      return;
    }
    if !self.decoder_loaded {
      if !decoder.set_source(source) {
        return;
      }
      self.decoder_loaded = true;
    }
    if self.decode_position != Some(seconds) {
      let target = decoder.request(source, seconds);
      if target != self.decode_target {
        self.decoded = None;
      }
      self.decode_target = target;
      self.decode_position = Some(seconds);
    }
  }

  fn needs_decoder(&self) -> bool {
    matches!(self.metadata, Metadata::Missing)
      || (matches!(self.metadata, Metadata::Ready(..))
        && self
          .target
          .is_some_and(|tile| self.failed.contains(&tile.atlas_index)))
  }

  fn settle(&mut self, session: SessionToken, generation: u64, completion: Completion) {
    if self.request != Some(completion.request)
      || !self
        .context
        .as_ref()
        .is_some_and(|context| context.session == session && context.generation == generation)
    {
      return;
    }
    self.request = None;
    match (completion.request.kind, completion.result) {
      (RequestKind::Manifest, Loaded::Manifest(source, manifest)) => {
        let current = source.as_ref().is_some_and(|source| {
          source.is_current()
            && self
              .context
              .as_ref()
              .is_some_and(|context| source.file() == context.file)
        });
        self.source = source;
        self.metadata = if !current {
          Metadata::Blocked
        } else {
          match manifest {
            Asset::Ready(Some(manifest)) => {
              let geometry = manifest.geometry();
              AtlasLayout::new(
                geometry.width,
                geometry.height,
                geometry.columns,
                geometry.rows,
                geometry.thumbnail_count,
                geometry.interval_ms,
              )
              .map(|layout| Metadata::Ready(manifest, layout))
              .unwrap_or(Metadata::Missing)
            }
            Asset::Ready(None) | Asset::Unavailable => Metadata::Missing,
            Asset::Blocked => Metadata::Blocked,
          }
        };
      }
      (RequestKind::Atlas(index), Loaded::Atlas(Asset::Ready(handle))) => {
        self.atlas = Some((index, handle))
      }
      (RequestKind::Atlas(index), Loaded::Atlas(Asset::Unavailable)) => {
        if self.failed.len() == 8 {
          self.failed.pop_front();
        }
        self.failed.push_back(index);
      }
      (RequestKind::Atlas(_), Loaded::Atlas(Asset::Blocked)) => {
        self.metadata = Metadata::Blocked;
        self.atlas = None;
      }
      _ => {}
    }
  }

  fn decoded_ready(&mut self, outcome: iced_mpv_preview::Outcome) {
    if self.decode_target != Some(outcome.requested)
      || !self.needs_decoder()
      || !self
        .source
        .as_ref()
        .is_some_and(PlaybackPreviewSource::is_current)
    {
      return;
    }
    match outcome.result {
      Ok(frame)
        if frame.requested == outcome.requested
          && frame.actual.is_finite()
          && frame.actual >= 0.0 =>
      {
        self.decoded = Some(frame)
      }
      _ => {
        self.decoded = None;
        self.decode_failed = true;
      }
    }
  }

  pub(super) fn frame(&self) -> Option<ImagePreview<'_>> {
    if self
      .source
      .as_ref()
      .is_some_and(|source| !source.is_current())
    {
      return None;
    }
    if let Some(tile) = self.target {
      if let Some((index, image)) = &self.atlas {
        if *index == tile.atlas_index {
          return Some(ImagePreview {
            image,
            crop: Some(tile),
          });
        }
      }
    }
    let frame = self.decoded.as_ref()?;
    (self.needs_decoder() && self.decode_target == Some(frame.requested)).then_some(ImagePreview {
      image: &frame.image,
      crop: None,
    })
  }
}

pub(super) fn reconcile(state: &mut State) {
  let enabled = !state.tv_mode()
    && super::active(state)
    && super::controls_visible(state)
    && !super::input_blocked(state)
    && !state.playback.view.lifecycle.replacing
    && state.shell.window_id.is_some()
    && state.shell.pending_close.is_none();
  let context = state
    .playback
    .view
    .now_playing
    .as_ref()
    .filter(|_| enabled)
    .and_then(|_| {
      Some(Context {
        session: state.kernel.request_gate.current_session(),
        generation: state.playback.view.lifecycle.replacement_generation,
        file: state.playback.view.tools.file?,
        seekable: state.playback.view.tools.seekable,
      })
    });
  if state.shell.embedded_player.seek_preview.context != context {
    let _ = crate::app::playback::adjust(
      &mut state.playback,
      jellypilot_core::now_playing_adjustments::Input::Cancel(
        jellypilot_core::now_playing_adjustments::Control::Seek,
      ),
    );
    state.shell.embedded_player.seek_hover = None;
  }
  // Transient command work cancels demand, not the media generation or keyboard focus.
  let seconds = state
    .playback
    .view
    .now_playing
    .as_ref()
    .filter(|_| !state.playback.view.busy || super::is_seek_dragging(state))
    .and_then(|playing| {
      playing
        .duration_seconds
        .filter(|duration| duration.is_finite() && *duration > 0.0)
        .and_then(|duration| {
          state
            .shell
            .embedded_player
            .seek_hover
            .map(|seconds| seconds.clamp(0.0, duration))
        })
    });
  state
    .shell
    .embedded_player
    .seek_preview
    .reconcile(context, seconds, state.preview.as_ref());
}

pub(super) fn settle(state: &mut State, completion: Completion) {
  reconcile(state);
  state.shell.embedded_player.seek_preview.settle(
    state.kernel.request_gate.current_session(),
    state.playback.view.lifecycle.replacement_generation,
    completion,
  );
}

pub(super) fn decoded_ready(state: &mut State) {
  reconcile(state);
  if let Some(outcome) = state.preview.as_ref().and_then(Runtime::take) {
    state
      .shell
      .embedded_player
      .seek_preview
      .decoded_ready(outcome);
  }
}

#[derive(Clone)]
struct Demand {
  request: Request,
  context: Context,
  manifest: Option<(TrickplayManifest, AtlasLayout)>,
  client: Arc<JellyfinClient>,
  controller: PlaybackControllerHandle,
}

impl Hash for Demand {
  fn hash<H: Hasher>(&self, hasher: &mut H) {
    self.request.hash(hasher);
    Arc::as_ptr(&self.client).hash(hasher);
    Arc::as_ptr(&self.controller).hash(hasher);
  }
}

pub(super) fn subscription(state: &State) -> Subscription<AppMessage> {
  let surface = &state.shell.embedded_player.seek_preview;
  let (Some(request), Some(context), Some(client), Some(controller)) = (
    surface.request,
    surface.context.as_ref(),
    state.kernel.client.as_ref(),
    state.playback.controller.as_ref(),
  ) else {
    return Subscription::none();
  };
  let manifest = match &surface.metadata {
    Metadata::Ready(manifest, layout) => Some((manifest.clone(), *layout)),
    _ => None,
  };
  Subscription::run_with(
    Demand {
      request,
      context: context.clone(),
      manifest,
      client: Arc::clone(client),
      controller: Arc::clone(controller),
    },
    stream,
  )
}

fn stream(demand: &Demand) -> impl iced::futures::Stream<Item = AppMessage> {
  let demand = demand.clone();
  iced::stream::channel(1, async move |mut output| {
    // Retiring this subscription cancels HTTP and debounce; native cancellation is separate.
    tokio::time::sleep(Duration::from_millis(120)).await;
    let result = load(&demand).await;
    let _ = output
      .send(AppMessage::EmbeddedPlayer(Message::SeekPreviewLoaded(
        Completion {
          request: demand.request,
          result,
        },
      )))
      .await;
    iced::futures::future::pending::<()>().await;
  })
}

async fn load(demand: &Demand) -> Loaded {
  match demand.request.kind {
    RequestKind::Manifest => {
      let source = tokio::time::timeout(Duration::from_secs(6), async {
        demand
          .controller
          .lock()
          .await
          .seek_preview_source()
          .filter(|source| source.file() == demand.context.file)
      })
      .await
      .ok()
      .flatten();
      let Some(source) = source else {
        return Loaded::Manifest(None, Asset::Blocked);
      };
      let result = tokio::time::timeout(
        Duration::from_secs(6),
        demand
          .client
          .playback()
          .trickplay_manifest(source.item_id(), Some(source.media_source_id())),
      )
      .await;
      let manifest = result.map_or(Asset::Unavailable, Asset::from_result);
      Loaded::Manifest(Some(source), manifest)
    }
    RequestKind::Atlas(index) => {
      let Some((manifest, layout)) = &demand.manifest else {
        return Loaded::Atlas(Asset::Unavailable);
      };
      let result = tokio::time::timeout(
        Duration::from_secs(6),
        demand.client.playback().trickplay_atlas(manifest, index),
      )
      .await;
      let atlas = match result.map_or(Asset::Unavailable, Asset::from_result) {
        Asset::Ready(atlas)
          if atlas.width == layout.atlas_size().0 && atlas.height <= layout.atlas_size().1 =>
        {
          Asset::Ready(Handle::from_rgba(atlas.width, atlas.height, atlas.pixels))
        }
        Asset::Blocked => Asset::Blocked,
        _ => Asset::Unavailable,
      };
      Loaded::Atlas(atlas)
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use jellypilot_core::request_gate::RequestGate;

  fn context(gate: &RequestGate, generation: u64) -> Context {
    Context {
      session: gate.current_session(),
      generation,
      file: PlaybackFileToken::default(),
      seekable: true,
    }
  }

  #[test]
  fn leaving_hover_cancels_demand_and_late_failure_does_not_disable_retry() {
    let gate = RequestGate::default();
    let context = context(&gate, 1);
    let mut surface = Surface::default();
    surface.reconcile(Some(context.clone()), Some(5.0), None);
    let retired = surface.request.unwrap();
    surface.reconcile(Some(context.clone()), None, None);
    surface.settle(
      gate.current_session(),
      1,
      Completion {
        request: retired,
        result: Loaded::Manifest(None, Asset::Blocked),
      },
    );
    surface.reconcile(Some(context), Some(6.0), None);
    assert!(matches!(surface.metadata, Metadata::Unrequested));
    assert!(surface.request.is_some_and(|request| request != retired));
  }

  #[test]
  fn replaced_media_or_profile_cannot_accept_retired_results() {
    let mut gate = RequestGate::default();
    let mut surface = Surface::default();
    surface.reconcile(Some(context(&gate, 1)), Some(0.0), None);
    let retired = surface.request.unwrap();
    for generation in [2, 1] {
      if generation == 1 {
        gate.disconnect();
      }
      surface.settle(
        gate.current_session(),
        generation,
        Completion {
          request: retired,
          result: Loaded::Manifest(None, Asset::Blocked),
        },
      );
      assert!(matches!(surface.metadata, Metadata::Unrequested));
    }
  }

  #[test]
  fn same_item_reload_and_nonseekable_stream_retire_preview_work() {
    let gate = RequestGate::default();
    let mut surface = Surface::default();
    surface.reconcile(Some(context(&gate, 1)), Some(3.0), None);
    let retired = surface.request.unwrap();
    let mut next = context(&gate, 1);
    next.seekable = false;
    surface.reconcile(Some(next), Some(3.0), None);
    surface.settle(
      gate.current_session(),
      1,
      Completion {
        request: retired,
        result: Loaded::Atlas(Asset::Ready(Handle::from_rgba(1, 1, vec![0, 0, 0, 255]))),
      },
    );
    assert!(surface.request.is_none());
    assert!(surface.frame().is_none());
    assert!(!surface.needs_decoder());
  }

  #[test]
  fn stale_sheet_cannot_replace_current_pixels_and_wrong_sheet_is_never_shown() {
    let gate = RequestGate::default();
    let mut surface = Surface {
      context: Some(context(&gate, 1)),
      ..Surface::default()
    };
    let retired = Request::new(RequestKind::Atlas(0));
    let current = Request::new(RequestKind::Atlas(1));
    surface.request = Some(current);
    surface.target = AtlasLayout::new(1, 1, 1, 1, 2, 1000).unwrap().tile_at(1.0);
    let handle = Handle::from_rgba(1, 1, vec![1, 2, 3, 255]);
    surface.settle(
      gate.current_session(),
      1,
      Completion {
        request: retired,
        result: Loaded::Atlas(Asset::Ready(handle.clone())),
      },
    );
    assert!(surface.atlas.is_none());
    surface.atlas = Some((0, handle.clone()));
    assert!(surface.frame().is_none());
    surface.settle(
      gate.current_session(),
      1,
      Completion {
        request: current,
        result: Loaded::Atlas(Asset::Ready(handle)),
      },
    );
    assert_eq!(
      surface
        .frame()
        .and_then(|frame| frame.crop)
        .map(|tile| tile.atlas_index),
      Some(1)
    );
  }

  #[test]
  fn reopened_presentation_cannot_reuse_previous_window_timeline_generation() {
    let gate = RequestGate::default();
    let context = context(&gate, 1);
    let now = std::time::Instant::now();
    let mut presentation = super::super::Surface::default();
    super::super::reconcile_surface(&mut presentation, true, false, true, now);
    presentation
      .seek_preview
      .reconcile(Some(context.clone()), Some(5.0), None);
    let retired = presentation.seek_preview.interaction_generation;

    super::super::reconcile_surface(&mut presentation, false, false, true, now);
    presentation.seek_preview.reconcile(None, None, None);
    super::super::reconcile_surface(&mut presentation, true, false, true, now);
    presentation
      .seek_preview
      .reconcile(Some(context), Some(5.0), None);

    assert_ne!(
      presentation.seek_preview.interaction_generation, retired,
      "a queued timeline action from the previous window must remain stale"
    );
  }
}
