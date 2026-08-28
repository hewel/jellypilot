use std::sync::{Arc, Mutex};

use iced::window;
use jellypilot_auth::login::{LoginError, LoginEvent};
use jellypilot_auth::{
  AuthStorageError, SavedProfileKey, SavedProfileSummary, SensitiveSavedSession,
};
use jellypilot_core::artwork_binder::ArtworkSlot;
use jellypilot_core::config::LoginPrefill;
use jellypilot_core::request_gate::{HomeToken, SessionToken};
use jellypilot_media_server::artwork::{ArtworkBytes, ArtworkError};
use jellypilot_media_server::home::HomeDataResult;
use jellypilot_media_server::{JellyfinClient, MediaServerProvider};

use zeroize::Zeroize;
impl std::fmt::Debug for Message {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Window(message) => formatter.debug_tuple("Window").field(message).finish(),
      Self::Login(_) => formatter.write_str("Login([redacted])"),
      Self::Home(_) => formatter.write_str("Home"),
    }
  }
}

#[derive(Clone)]
pub enum Message {
  Window(WindowMessage),
  Login(LoginMessage),
  Home(HomeMessage),
}

#[derive(Clone, Copy, Debug)]
pub enum WindowMessage {
  CloseRequested(window::Id),
  FrameRendered,
}

#[derive(Clone)]
pub enum HomeMessage {
  Navigate(super::state::Destination),
  Retry,
  Loaded {
    token: HomeToken,
    result: HomeDataResult,
  },
  ArtworkLoaded {
    session: SessionToken,
    slot: ArtworkSlot,
    image_id: String,
    result: Result<ArtworkBytes, ArtworkError>,
  },
}

pub type SensitiveSessionPayload = SensitiveSavedSession;

struct ProtectedPayloadOwner<T: Zeroize>(Option<T>);

impl<T: Zeroize> Drop for ProtectedPayloadOwner<T> {
  fn drop(&mut self) {
    if let Some(payload) = &mut self.0 {
      payload.zeroize();
    }
  }
}

struct ProtectedPayload<T: Zeroize>(Arc<Mutex<ProtectedPayloadOwner<T>>>);

impl<T: Zeroize> Clone for ProtectedPayload<T> {
  fn clone(&self) -> Self {
    Self(Arc::clone(&self.0))
  }
}

impl<T: Zeroize> ProtectedPayload<T> {
  fn new(payload: T) -> Self {
    Self(Arc::new(Mutex::new(ProtectedPayloadOwner(Some(payload)))))
  }

  fn take(&self) -> Option<T> {
    self
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .0
      .take()
  }
}

#[derive(Clone)]
pub struct ProtectedSavedSession(ProtectedPayload<SensitiveSessionPayload>);

impl ProtectedSavedSession {
  pub fn new(session: SensitiveSessionPayload) -> Self {
    Self(ProtectedPayload::new(session))
  }

  pub fn take(&self) -> Option<SensitiveSessionPayload> {
    self.0.take()
  }
}

#[derive(Clone)]
pub struct PasswordSubmission {
  pub remember: bool,
  pub prefill: LoginPrefill,
  pub provider: MediaServerProvider,
}

#[derive(Clone)]
pub enum LoginMessage {
  ProviderSelected(MediaServerProvider),
  MethodSelected(super::state::LoginMethod),
  ServerUrlChanged(String),
  UsernameChanged(String),
  PasswordChanged(String),
  RememberToggled,
  QuickConnectSubmitted,
  QuickConnectCancelled,
  PasswordSubmitted,
  ProfilesLoaded {
    revision: u64,
    result: Result<Vec<SavedProfileSummary>, AuthStorageError>,
  },
  WorkflowEvent(LoginEvent),
  PasswordFinished {
    session: SessionToken,
    client: Arc<JellyfinClient>,
    result: Result<ProtectedSavedSession, LoginError>,
    submission: PasswordSubmission,
  },
  SavedSessionStored {
    session: SessionToken,
    result: Result<(SavedProfileKey, Vec<SavedProfileSummary>), AuthStorageError>,
  },
  RestoreProfile(SavedProfileKey),
  RestoreFinished {
    session: SessionToken,
    key: SavedProfileKey,
    result: Result<ProtectedSavedSession, LoginError>,
  },
  AskForgetProfile(SavedProfileKey),
  CancelForgetProfile,
  ConfirmForgetProfile(SavedProfileKey),
  ForgetFinished {
    session: SessionToken,
    key: SavedProfileKey,
    sign_out: bool,
    result: Result<Vec<SavedProfileSummary>, AuthStorageError>,
  },
}

#[cfg(test)]
mod tests {
  use std::sync::atomic::{AtomicBool, Ordering};

  use super::*;

  struct ZeroizeWitness(Arc<AtomicBool>);

  impl Zeroize for ZeroizeWitness {
    fn zeroize(&mut self) {
      self.0.store(true, Ordering::Relaxed);
    }
  }

  #[test]
  fn protected_payload_zeroizes_when_the_last_message_copy_is_dropped() {
    let zeroized = Arc::new(AtomicBool::new(false));
    let payload = ProtectedPayload::new(ZeroizeWitness(Arc::clone(&zeroized)));
    let cloned = payload.clone();

    drop(payload);
    assert!(!zeroized.load(Ordering::Relaxed));
    drop(cloned);

    assert!(zeroized.load(Ordering::Relaxed));
  }
}
