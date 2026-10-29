//! Shared CPU preview ownership. The runner retains a clone across the entire iced runtime.

use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use iced::futures::{channel::mpsc, SinkExt, StreamExt};
use iced::Subscription;
use iced_mpv_preview::{Outcome, Preview};
use jellypilot_mpv::playback::{PlaybackPreviewIdentity, PlaybackPreviewSource};

use crate::app::{embedded_player, Message};

struct Binding {
  decoder: Arc<Preview>,
  source: Mutex<Option<PlaybackPreviewIdentity>>,
}

impl Binding {
  fn retire(&self, identity: PlaybackPreviewIdentity) {
    let mut source = self
      .source
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    if *source == Some(identity) {
      self.decoder.set_media(None);
      *source = None;
    }
  }
}

#[derive(Clone)]
pub(crate) struct Runtime {
  binding: Arc<Binding>,
  notifications: Arc<Mutex<Option<mpsc::Receiver<()>>>>,
}

impl Runtime {
  pub fn start() -> Option<Self> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
      let library = crate::embedded::options()?.libmpv.to_str()?.to_owned();
      match Preview::new(library) {
        Ok((decoder, notifications)) => Some(Self {
          binding: Arc::new(Binding {
            decoder: Arc::new(decoder),
            source: Mutex::new(None),
          }),
          notifications: Arc::new(Mutex::new(Some(notifications))),
        }),
        Err(_) => {
          tracing::warn!(
            "Independent thumbnail decoder is unavailable; time previews remain available"
          );
          None
        }
      }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
      None
    }
  }

  pub fn set_source(&self, source: &PlaybackPreviewSource) -> bool {
    let identity = source.identity();
    let binding = self.binding.clone();
    // Always acquire source admission before product binding before the decoder
    // mailbox. Retirement uses the same order and never invokes native APIs.
    // The callback retains its owner until the source gate has been released.
    source
      .bind(
        move || binding.retire(identity),
        |url| {
          let mut current = self
            .binding
            .source
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
          self.binding.decoder.set_media(Some(url));
          *current = Some(identity);
        },
      )
      .is_some()
  }

  pub fn request(&self, source: &PlaybackPreviewSource, seconds: f64) -> Option<f64> {
    source
      .admit(|| {
        let current = self.binding.source.lock().ok()?;
        if *current != Some(source.identity()) {
          return None;
        }
        self.binding.decoder.request(seconds)
      })
      .flatten()
  }

  pub fn retire(&self, identity: PlaybackPreviewIdentity) {
    self.binding.retire(identity);
  }

  pub fn cancel(&self, identity: PlaybackPreviewIdentity) {
    let current = self
      .binding
      .source
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    if *current == Some(identity) {
      self.binding.decoder.cancel();
    }
  }

  pub fn take(&self) -> Option<Outcome> {
    let current = self.binding.source.lock().ok()?;
    current.as_ref()?;
    self.binding.decoder.take()
  }

  pub fn subscription(&self) -> Subscription<Message> {
    Subscription::run_with(self.clone(), notifications)
  }
}

impl Hash for Runtime {
  fn hash<H: Hasher>(&self, state: &mut H) {
    Arc::as_ptr(&self.binding).hash(state);
  }
}

fn notifications(runtime: &Runtime) -> impl iced::futures::Stream<Item = Message> {
  let runtime = runtime.clone();
  iced::stream::channel(1, async move |mut output| {
    let receiver = runtime
      .notifications
      .lock()
      .ok()
      .and_then(|mut slot| slot.take());
    let Some(mut receiver) = receiver else {
      return;
    };
    while receiver.next().await.is_some() {
      if output
        .send(Message::EmbeddedPlayer(
          embedded_player::Message::DecodedPreviewReady,
        ))
        .await
        .is_err()
      {
        break;
      }
    }
  })
}
