use std::sync::{Arc, Mutex, OnceLock};

use iced::Task;
use jellypilot_auth::login::ConnectionPhase;
use jellypilot_auth::{AuthStore, SavedProfileKey};
use jellypilot_core::config::SettingsStore;
use jellypilot_core::diagnostics::Diagnostics;
use jellypilot_core::request_gate::RequestGate;
use jellypilot_media_server::artwork::ArtworkAdapter;
use jellypilot_media_server::JellyfinClient;
use jellypilot_mpv::playback_session::IntroAvailability;
use jellypilot_sdk::{Sdk, SdkConfig, SdkHooks};
use tokio::sync::mpsc;

use super::message::Message;
use super::personal_lists;
use super::state::{
  intro_skip_mode, ConnectedIdentity, NoticeLevel, ProfileAvatarHandles, ToastNotice,
};
use crate::i18n::{Localizer, UiText};
use crate::tray::Tray;

/// Cross-surface machinery shared by every surface module (ADR 0029): server
/// auth/connection, request gating, diagnostics, user notifications, tray, and
/// the shared Library Image byte/raster adapter; pages own their own demands.
pub struct Kernel {
  /// Persisted application configuration; read and mutated by several
  /// surfaces (settings edits, login prefill, playback options, filters).
  pub settings: SettingsStore,
  pub locale: Localizer,
  pub auth_store: AuthStore,
  /// Shared account owner: authentication, adoption, disconnect, sign-out,
  /// and their teardown ordering. `client`, `connection`,
  /// `connected_identity`, `active_profile`, and `request_gate` are
  /// read/presentation projections re-synced from committed SDK outcomes;
  /// native code must never adopt or disconnect a client itself.
  pub sdk: Arc<Sdk>,
  /// Receives the SDK teardown hook's requests on the UI loop.
  pub sdk_handoff: HandoffChannel,
  pub client: Option<Arc<JellyfinClient>>,
  pub connection: ConnectionPhase,
  pub connected_identity: Option<ConnectedIdentity>,
  pub active_profile: Option<SavedProfileKey>,
  pub request_gate: RequestGate,
  pub(crate) item_actions: jellypilot_sdk::item_actions::ItemActions,
  pub diagnostics: Diagnostics,
  pub notice: Option<UiText>,
  pub active_toast: Option<ToastNotice>,
  pub next_toast_id: u64,
  pub tray: Option<Tray>,
  /// Saved-profile photos have an independent authorization and cache lifecycle.
  pub avatar_adapter: Arc<ArtworkAdapter>,
  pub artwork_adapter: Arc<ArtworkAdapter>,
  pub profile_avatars: ProfileAvatarHandles,
}

impl Kernel {
  /// Shows a toast notification and mirrors it into the persistent notice
  /// line; the toast auto-dismisses after five seconds.
  pub fn show_toast(&mut self, level: NoticeLevel, message: UiText) -> Task<Message> {
    self.next_toast_id = self.next_toast_id.wrapping_add(1);
    let id = self.next_toast_id;
    self.active_toast = Some(ToastNotice {
      id,
      message: message.clone(),
      level,
    });
    self.notice = Some(message);
    Task::perform(
      async move {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        id
      },
      Message::DismissNotice,
    )
  }

  /// Intro-skip availability for a playback start: the configured mode plus
  /// whether the connected server supports Intro Skipper.
  pub fn intro_availability(&self) -> IntroAvailability {
    IntroAvailability {
      mode: intro_skip_mode(self.settings.snapshot().intro_mode()),
      skipper_available: self
        .client
        .as_ref()
        .is_some_and(|client| client.supports_intro_skipper()),
    }
  }
}

/// One SDK teardown-hook request delivered to the UI loop.
///
/// The responder is single-use: the account reducer resolves it once the
/// physical playback and remote teardown settle (`true`), once teardown
/// fails under an irreversible Sign Out (`false`), or once a reversible
/// transition is explicitly abandoned (`false`).
#[derive(Clone)]
pub struct HookRequest {
  pub responder: Arc<Mutex<Option<tokio::sync::oneshot::Sender<bool>>>>,
}

impl HookRequest {
  /// Answers the hook exactly once; later resolutions are ignored.
  pub fn resolve(&self, allowed: bool) {
    let responder = self
      .responder
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .take();
    if let Some(responder) = responder {
      let _ = responder.send(allowed);
    }
  }
}

impl std::fmt::Debug for HookRequest {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.write_str("HookRequest")
  }
}

/// Subscription identity for the SDK teardown-hook channel.
///
/// The receiver is shared so the subscription can be rebuilt every state
/// change; hashing the stable receiver pointer keeps the recipe alive.
#[derive(Clone)]
pub struct HandoffChannel {
  pub receiver: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<HookRequest>>>,
}

impl std::hash::Hash for HandoffChannel {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    Arc::as_ptr(&self.receiver).hash(state);
  }
}

/// Desktop teardown adapter: forwards the SDK's profile-transition hook to
/// the UI loop and delegates opted-in Watchlist deletion to the shared
/// serialized runtime so no second store owner races it.
struct DesktopHooks {
  requests: mpsc::UnboundedSender<HookRequest>,
  watchlist: personal_lists::Runtime,
}

#[async_trait::async_trait]
impl SdkHooks for DesktopHooks {
  async fn before_profile_handoff(&self) -> bool {
    let (responder, outcome) = tokio::sync::oneshot::channel();
    let request = HookRequest {
      responder: Arc::new(Mutex::new(Some(responder))),
    };
    if self.requests.send(request).is_err() {
      // The UI loop is gone (process exit): decline so the SDK keeps the
      // previous profile instead of committing a transition nobody observed.
      return false;
    }
    outcome.await.unwrap_or(false)
  }

  async fn remove_watchlist(
    &self,
    scope: &jellypilot_core::watchlist::ProfileScope,
  ) -> Option<Result<(), String>> {
    Some(self.watchlist.remove_scope(scope.clone()).await.map(|_| ()))
  }
}

/// Builds the shared SDK and its desktop teardown adapter for one Kernel.
///
/// The SDK shares the ambient Tokio runtime when one exists (production boot
/// runs inside `runtime.enter`). Synchronous tests have no ambient runtime,
/// so the SDK falls back to one lazily created process-wide runtime rather
/// than a runtime per test.
pub(crate) fn account_runtime(
  auth_store: &AuthStore,
  watchlist: &personal_lists::Runtime,
) -> (Arc<Sdk>, HandoffChannel) {
  let (requests, receiver) = mpsc::unbounded_channel();
  let hooks: Arc<dyn SdkHooks> = Arc::new(DesktopHooks {
    requests,
    watchlist: watchlist.clone(),
  });
  let handle =
    tokio::runtime::Handle::try_current().unwrap_or_else(|_| fallback_runtime_handle().clone());
  let sdk = Arc::new(Sdk::with_handle(
    sdk_config(),
    auth_store.clone(),
    Some(hooks),
    handle,
  ));
  (
    sdk,
    HandoffChannel {
      receiver: Arc::new(tokio::sync::Mutex::new(receiver)),
    },
  )
}

fn sdk_config() -> SdkConfig {
  SdkConfig {
    storage_dir: dirs::config_dir()
      .unwrap_or_else(std::env::temp_dir)
      .join("jellypilot"),
    device_name: "JellyPilot".to_owned(),
  }
}

/// Process-wide runtime used only when no ambient Tokio runtime exists —
/// synchronous tests. Production always enters the iced runtime first.
fn fallback_runtime_handle() -> &'static tokio::runtime::Handle {
  static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
  RUNTIME
    .get_or_init(|| {
      tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .thread_name("jellypilot-iced-sdk")
        .build()
        .expect("fallback SDK runtime")
    })
    .handle()
}

#[cfg(test)]
pub(crate) fn test_auth_store() -> AuthStore {
  use jellypilot_auth::{CredentialError, SecureCredential};

  struct MemoryCredential(Mutex<Option<Vec<u8>>>);

  impl SecureCredential for MemoryCredential {
    fn read(&self) -> Result<Vec<u8>, CredentialError> {
      self
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or(CredentialError::Missing)
    }

    fn write(&self, secret: &[u8]) -> Result<(), CredentialError> {
      *self
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(secret.to_vec());
      Ok(())
    }

    fn delete(&self) -> Result<(), CredentialError> {
      *self
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
      Ok(())
    }
  }

  AuthStore::with_credential(Arc::new(MemoryCredential(Mutex::new(None))))
}

#[cfg(test)]
pub(crate) fn test_account_runtime(auth_store: &AuthStore) -> (Arc<Sdk>, HandoffChannel) {
  test_account_runtime_with_watchlist(auth_store, &personal_lists::Runtime::default())
}

#[cfg(test)]
pub(crate) fn test_account_runtime_with_watchlist(
  auth_store: &AuthStore,
  watchlist: &personal_lists::Runtime,
) -> (Arc<Sdk>, HandoffChannel) {
  let (requests, receiver) = mpsc::unbounded_channel();
  let hooks: Arc<dyn SdkHooks> = Arc::new(DesktopHooks {
    requests,
    watchlist: watchlist.clone(),
  });
  let sdk = Arc::new(Sdk::with_handle(
    SdkConfig {
      storage_dir: std::env::temp_dir().join(format!(
        "jellypilot-iced-sdk-test-{}-{}",
        std::process::id(),
        TEST_STORAGE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
      )),
      device_name: "JellyPilot".to_owned(),
    },
    auth_store.clone(),
    Some(hooks),
    fallback_runtime_handle().clone(),
  ));
  (
    sdk,
    HandoffChannel {
      receiver: Arc::new(tokio::sync::Mutex::new(receiver)),
    },
  )
}

#[cfg(test)]
static TEST_STORAGE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
