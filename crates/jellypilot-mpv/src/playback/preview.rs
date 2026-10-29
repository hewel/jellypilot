//! Admission to an already-resolved source, never a new playback negotiation.

use std::fmt;
use std::sync::{Arc, Mutex, Weak};

use super::tools::PlaybackFileToken;
use super::{AuthenticatedUrl, LoadEventBoundary, PlaybackController};

type Retirement = Box<dyn Fn() + Send>;

#[derive(Default)]
struct Lease {
  generation: u64,
  retirement: Option<Retirement>,
}

pub(super) struct PreviewMedia {
  pub(super) url: AuthenticatedUrl,
  lease: Mutex<Lease>,
}

impl PreviewMedia {
  pub(super) fn new(url: AuthenticatedUrl) -> Arc<Self> {
    Arc::new(Self {
      url,
      lease: Mutex::default(),
    })
  }

  fn retire(&self) {
    let retired;
    {
      let mut lease = self
        .lease
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      lease.generation = lease
        .generation
        .checked_add(1)
        .expect("preview lease generation exhausted");
      retired = lease.retirement.take();
      if let Some(retire) = &retired {
        retire();
      }
    }
    // The callback may retain a decoder owner. Its final Drop must not join a
    // native worker while either the admission or product binding lock is held.
    drop(retired);
  }
}

impl Drop for PreviewMedia {
  fn drop(&mut self) {
    let lease = self
      .lease
      .get_mut()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(retire) = lease.retirement.take() {
      retire();
    }
  }
}

/// Opaque file-and-lease identity, containing no source URL or credentials.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaybackPreviewIdentity {
  file: PlaybackFileToken,
  generation: u64,
}

/// Revocable access to the current file's resolved authenticated playback source.
/// Cloning this capability neither retains the source nor negotiates playback.
/// Retired capabilities cannot bind or admit work, even during an outgoing report.
#[derive(Clone)]
pub struct PlaybackPreviewSource {
  identity: PlaybackPreviewIdentity,
  item_id: String,
  media_source_id: String,
  seekable: bool,
  media: Weak<PreviewMedia>,
}

impl PlaybackPreviewSource {
  pub const fn identity(&self) -> PlaybackPreviewIdentity {
    self.identity
  }
  pub const fn file(&self) -> PlaybackFileToken {
    self.identity.file
  }
  pub fn item_id(&self) -> &str {
    &self.item_id
  }
  pub fn media_source_id(&self) -> &str {
    &self.media_source_id
  }
  pub const fn seekable(&self) -> bool {
    self.seekable
  }

  pub fn is_current(&self) -> bool {
    self.admit(|| ()).is_some()
  }

  /// Registers synchronous mailbox retirement and binds the borrowed URL under
  /// the same admission lock. Rebinding retires the previous binding first.
  /// The retirement callback must compare this capability's identity before
  /// clearing a shared decoder, so an old source cannot cancel its replacement.
  /// Callbacks must only perform bounded mailbox operations, never native work,
  /// await, or reenter this capability. Never log, persist or hash the URL.
  pub fn bind<T>(
    &self,
    retire: impl Fn() + Send + 'static,
    bind: impl FnOnce(&str) -> T,
  ) -> Option<T> {
    let media = self.media.upgrade()?;
    let retired;
    let result;
    {
      let mut lease = media.lease.lock().ok()?;
      if lease.generation != self.identity.generation {
        return None;
      }
      retired = lease.retirement.take();
      if let Some(retire) = &retired {
        retire();
      }
      // Install before admission, including unwinding from the consumer callback.
      lease.retirement = Some(Box::new(retire));
      result = bind(media.url.as_str());
    }
    drop(retired);
    Some(result)
  }

  /// Serializes every new mailbox request with source revocation. The callback
  /// must be bounded, must not perform native work or reenter this capability.
  /// Targets use the main mpv normalized timeline, not original transport PTS.
  /// Already-admitted native calls are not interrupted by revocation.
  pub fn admit<T>(&self, accept: impl FnOnce() -> T) -> Option<T> {
    let media = self.media.upgrade()?;
    let lease = media.lease.lock().ok()?;
    if lease.generation != self.identity.generation {
      return None;
    }
    let result = accept();
    drop(lease);
    Some(result)
  }
}

impl fmt::Debug for PlaybackPreviewSource {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("PlaybackPreviewSource")
      .field("identity", &self.identity)
      .field("seekable", &self.seekable)
      .finish_non_exhaustive()
  }
}

impl PlaybackController {
  /// Captures a settled file's source without network requests or transport mutations.
  pub fn seek_preview_source(&self) -> Option<PlaybackPreviewSource> {
    if !self.active_transport_matches_mpv
      || self.unloading
      || self.load_event_boundary != LoadEventBoundary::Settled
    {
      return None;
    }
    let active = self.active.as_ref()?;
    let generation = active.preview_media.lease.lock().ok()?.generation;
    Some(PlaybackPreviewSource {
      identity: PlaybackPreviewIdentity {
        file: self.tools.file?,
        generation,
      },
      item_id: active.now_playing.item_id.clone(),
      media_source_id: active.media_source_id.clone(),
      seekable: self.tools.seekable,
      media: Arc::downgrade(&active.preview_media),
    })
  }

  pub(super) fn retire_preview_source(&self) {
    if let Some(active) = &self.active {
      active.preview_media.retire();
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
  };

  fn fixture() -> (Arc<PreviewMedia>, PlaybackPreviewSource) {
    let media = PreviewMedia::new(AuthenticatedUrl::new(
      "https://fixture.invalid/video?api_key=private".into(),
    ));
    let source = PlaybackPreviewSource {
      identity: PlaybackPreviewIdentity {
        file: PlaybackFileToken::default(),
        generation: 0,
      },
      item_id: "item".into(),
      media_source_id: "source".into(),
      seekable: true,
      media: Arc::downgrade(&media),
    };
    (media, source)
  }

  #[test]
  fn revocation_serializes_admission_and_clears_pending_work_before_returning() {
    let (media, source) = fixture();
    let pending = Arc::new(AtomicBool::new(false));
    let revoked_pending = pending.clone();
    source
      .bind(
        move || revoked_pending.store(false, Ordering::SeqCst),
        |_| (),
      )
      .unwrap();
    let (entered, admission_entered) = mpsc::channel();
    let (release, proceed) = mpsc::channel();
    std::thread::scope(|scope| {
      let source = &source;
      let pending = &pending;
      let admission = scope.spawn(move || {
        source.admit(|| {
          entered.send(()).unwrap();
          proceed.recv().unwrap();
          pending.store(true, Ordering::SeqCst);
        })
      });
      admission_entered.recv().unwrap();
      // This deterministic observation catches a check-then-unlock admission:
      // retirement must not pass a callback that has not finished queuing work.
      assert!(matches!(
        media.lease.try_lock(),
        Err(std::sync::TryLockError::WouldBlock)
      ));
      let retirement = scope.spawn(|| media.retire());
      release.send(()).unwrap();
      assert_eq!(admission.join().unwrap(), Some(()));
      retirement.join().unwrap();
    });
    assert!(!pending.load(Ordering::SeqCst));
    assert!(source
      .admit(|| pending.store(true, Ordering::SeqCst))
      .is_none());
    assert!(source
      .bind(|| {}, |_| pending.store(true, Ordering::SeqCst))
      .is_none());
    assert!(!pending.load(Ordering::SeqCst));
  }

  #[test]
  fn source_drop_retires_its_binding_without_retaining_the_credential() {
    let (media, source) = fixture();
    let pending = Arc::new(AtomicBool::new(true));
    let cancelled = pending.clone();
    source
      .bind(move || cancelled.store(false, Ordering::SeqCst), |_| ())
      .unwrap();
    drop(media);
    assert!(!pending.load(Ordering::SeqCst));
    assert!(source.admit(|| ()).is_none());
  }

  #[test]
  fn retiring_a_binding_drops_its_owner_after_releasing_admission() {
    struct Owner {
      media: Weak<PreviewMedia>,
      dropped: Arc<AtomicBool>,
    }
    impl Owner {
      fn retire(&self) {
        assert!(self.media.upgrade().is_some());
      }
    }
    impl Drop for Owner {
      fn drop(&mut self) {
        let media = self.media.upgrade().unwrap();
        assert!(
          media.lease.try_lock().is_ok(),
          "owner Drop could join under admission lock"
        );
        self.dropped.store(true, Ordering::SeqCst);
      }
    }
    let (media, source) = fixture();
    let dropped = Arc::new(AtomicBool::new(false));
    let owner = Owner {
      media: Arc::downgrade(&media),
      dropped: dropped.clone(),
    };
    source.bind(move || owner.retire(), |_| ()).unwrap();
    media.retire();
    assert!(dropped.load(Ordering::SeqCst));
  }
}
