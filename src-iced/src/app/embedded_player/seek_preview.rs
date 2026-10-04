//! Demand-owned Trickplay loading for the complete desktop timeline.

use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::futures::SinkExt;
use iced::widget::image::Handle;
use iced::Subscription;
use jellypilot_core::request_gate::SessionToken;
use jellypilot_core::seek_preview::{AtlasLayout, PreviewTile};
use jellypilot_media_server::{JellyfinClient, TrickplayManifest};

use super::{AppMessage, Message};
use crate::app::state::{PlaybackControllerHandle, State};

#[derive(Clone, PartialEq, Eq)]
struct Context {
  session: SessionToken,
  generation: u64,
  item_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum RequestKind {
  Manifest,
  Atlas(u32),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Request {
  token: Instant,
  kind: RequestKind,
}

#[derive(Default)]
enum Metadata {
  #[default]
  Unrequested,
  Ready(TrickplayManifest, AtlasLayout),
  Unavailable,
}

#[derive(Default)]
pub(super) struct Surface {
  context: Option<Context>,
  metadata: Metadata,
  request: Option<Request>,
  target: Option<PreviewTile>,
  // One atlas handle owns the shared RGBA buffer. Changing frames within this
  // atlas changes only the source rectangle, with no HTTP or pixel copies.
  atlas: Option<(u32, Handle)>,
  failed: VecDeque<u32>,
}

#[derive(Clone)]
pub struct Completion {
  request: Request,
  result: Loaded,
}

#[derive(Clone)]
enum Loaded {
  Manifest(Option<TrickplayManifest>),
  Atlas(Option<Handle>),
}

impl Surface {
  fn reconcile(&mut self, context: Option<Context>, seconds: Option<f64>) {
    if self.context != context {
      *self = Self {
        context,
        ..Self::default()
      };
    }
    self.target = None;
    let Some(seconds) = seconds
      .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
      .filter(|_| self.context.is_some())
    else {
      self.request = None;
      return;
    };
    let kind = match &self.metadata {
      Metadata::Unrequested => Some(RequestKind::Manifest),
      Metadata::Unavailable => None,
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
      self.request = kind.map(|kind| Request {
        token: Instant::now(),
        kind,
      });
    }
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
      (RequestKind::Manifest, Loaded::Manifest(manifest)) => {
        self.metadata = manifest
          .and_then(|manifest| {
            let geometry = manifest.geometry();
            let layout = AtlasLayout::new(
              geometry.width,
              geometry.height,
              geometry.columns,
              geometry.rows,
              geometry.thumbnail_count,
              geometry.interval_ms,
            )?;
            Some(Metadata::Ready(manifest, layout))
          })
          .unwrap_or(Metadata::Unavailable);
      }
      (RequestKind::Atlas(index), Loaded::Atlas(Some(handle))) => {
        self.atlas = Some((index, handle));
      }
      (RequestKind::Atlas(index), Loaded::Atlas(None)) => {
        // Bound negative caching as well as pixels. Repeated pointer messages
        // must not hammer an unavailable sheet while the text fallback works.
        if self.failed.len() == 8 {
          self.failed.pop_front();
        }
        self.failed.push_back(index);
      }
      _ => {}
    }
  }

  pub(super) fn frame(&self) -> Option<(&Handle, PreviewTile)> {
    let tile = self.target?;
    let (index, handle) = self.atlas.as_ref()?;
    (*index == tile.atlas_index).then_some((handle, tile))
  }
}

pub(super) fn reconcile(state: &mut State) {
  let enabled = !state.tv_mode()
    && super::active(state)
    && super::controls_visible(state)
    && !super::input_blocked(state)
    && !state.playback.view.lifecycle.replacing;
  let playing = state.playback.view.now_playing.as_ref();
  let context = playing.filter(|_| enabled).map(|playing| Context {
    session: state.kernel.request_gate.current_session(),
    generation: state.playback.view.lifecycle.replacement_generation,
    item_id: playing.item.item_id.clone(),
  });
  let seconds = playing.and_then(|playing| {
    playing
      .duration_seconds
      .filter(|duration| duration.is_finite() && *duration > 0.0)
      .and_then(|duration| {
        state
          .playback
          .adjustments
          .view()
          .seek_preview
          .or(state.shell.embedded_player.seek_hover)
          .map(|seconds| seconds.clamp(0.0, duration))
      })
  });
  state
    .shell
    .embedded_player
    .seek_preview
    .reconcile(context, seconds);
}

pub(super) fn settle(state: &mut State, completion: Completion) {
  state.shell.embedded_player.seek_preview.settle(
    state.kernel.request_gate.current_session(),
    state.playback.view.lifecycle.replacement_generation,
    completion,
  );
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
    // A new sheet waits briefly, while movements within the current atlas
    // render immediately. Retiring the subscription cancels HTTP and the wait.
    tokio::time::sleep(Duration::from_millis(120)).await;
    let result = tokio::time::timeout(Duration::from_secs(6), load(&demand)).await;
    let result = result.unwrap_or_else(|_| match demand.request.kind {
      RequestKind::Manifest => Loaded::Manifest(None),
      RequestKind::Atlas(_) => Loaded::Atlas(None),
    });
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
      let identity = {
        let controller = demand.controller.lock().await;
        controller
          .seek_preview_identity()
          .and_then(|(item, source)| (item == demand.context.item_id).then(|| source.to_owned()))
      };
      let Some(source) = identity else {
        return Loaded::Manifest(None);
      };
      Loaded::Manifest(
        demand
          .client
          .playback()
          .trickplay_manifest(&demand.context.item_id, Some(&source))
          .await
          .ok()
          .flatten(),
      )
    }
    RequestKind::Atlas(index) => {
      let Some((manifest, layout)) = &demand.manifest else {
        return Loaded::Atlas(None);
      };
      let atlas = demand
        .client
        .playback()
        .trickplay_atlas(manifest, index)
        .await
        .ok();
      Loaded::Atlas(atlas.and_then(|atlas| {
        (atlas.width == layout.atlas_size().0 && atlas.height <= layout.atlas_size().1)
          .then(|| Handle::from_rgba(atlas.width, atlas.height, atlas.pixels))
      }))
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
      item_id: "item".to_owned(),
    }
  }

  #[test]
  fn leaving_hover_cancels_demand_and_late_failure_does_not_disable_retry() {
    let gate = RequestGate::default();
    let context = context(&gate, 1);
    let mut surface = Surface::default();
    surface.reconcile(Some(context.clone()), Some(5.0));
    let retired = surface.request.unwrap();
    surface.reconcile(Some(context.clone()), None);
    surface.settle(
      gate.current_session(),
      1,
      Completion {
        request: retired,
        result: Loaded::Manifest(None),
      },
    );
    surface.reconcile(Some(context), Some(6.0));
    assert!(matches!(surface.metadata, Metadata::Unrequested));
    assert!(surface.request.is_some_and(|request| request != retired));
  }

  #[test]
  fn replaced_media_or_profile_cannot_accept_retired_results() {
    let mut gate = RequestGate::default();
    let mut surface = Surface::default();
    surface.reconcile(Some(context(&gate, 1)), Some(0.0));
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
          result: Loaded::Manifest(None),
        },
      );
      assert!(matches!(surface.metadata, Metadata::Unrequested));
    }
  }

  #[test]
  fn stale_sheet_cannot_replace_current_pixels_and_wrong_sheet_is_never_shown() {
    let gate = RequestGate::default();
    let mut surface = Surface {
      context: Some(context(&gate, 1)),
      ..Surface::default()
    };
    let retired = Request {
      token: Instant::now(),
      kind: RequestKind::Atlas(0),
    };
    let current = Request {
      token: Instant::now(),
      kind: RequestKind::Atlas(1),
    };
    surface.request = Some(current);
    surface.target = AtlasLayout::new(1, 1, 1, 1, 2, 1000).unwrap().tile_at(1.0);
    let handle = Handle::from_rgba(1, 1, vec![1, 2, 3, 255]);
    surface.settle(
      gate.current_session(),
      1,
      Completion {
        request: retired,
        result: Loaded::Atlas(Some(handle.clone())),
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
        result: Loaded::Atlas(Some(handle)),
      },
    );
    assert_eq!(surface.frame().map(|(_, tile)| tile.atlas_index), Some(1));
  }
}
