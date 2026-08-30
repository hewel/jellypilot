use std::sync::Arc;

use iced::Task;
use jellypilot_auth::login::ConnectionPhase;
use jellypilot_auth::{AuthStore, SavedProfileKey};
use jellypilot_core::artwork_binder::ArtworkBinder;
use jellypilot_core::config::SettingsStore;
use jellypilot_core::diagnostics::Diagnostics;
use jellypilot_core::request_gate::RequestGate;
use jellypilot_media_server::artwork::ArtworkAdapter;
use jellypilot_media_server::JellyfinClient;
use jellypilot_mpv::playback_session::IntroAvailability;

use super::message::Message;
use super::state::{
  intro_skip_mode, ArtworkHandleRetention, ConnectedIdentity, NoticeLevel, ToastNotice,
};
use crate::tray::Tray;

/// Cross-surface machinery shared by every surface module (ADR 0029): server
/// auth/connection, request gating, diagnostics, user notifications, tray, and
/// the artwork pipeline that ADR 0028's streaming loader drives for all views.
pub struct Kernel {
  /// Persisted application configuration; read and mutated by several
  /// surfaces (settings edits, login prefill, playback options, filters).
  pub settings: SettingsStore,
  pub auth_store: AuthStore,
  pub client: Option<Arc<JellyfinClient>>,
  pub connection: ConnectionPhase,
  pub connected_identity: Option<ConnectedIdentity>,
  pub active_profile: Option<SavedProfileKey>,
  pub request_gate: RequestGate,
  pub diagnostics: Diagnostics,
  pub notice: Option<String>,
  pub active_toast: Option<ToastNotice>,
  pub next_toast_id: u64,
  pub tray: Option<Tray>,
  pub artwork_adapter: Arc<ArtworkAdapter>,
  pub artwork_binder: ArtworkBinder,
  pub artwork_handles: ArtworkHandleRetention,
}

impl Kernel {
  /// Shows a toast notification and mirrors it into the persistent notice
  /// line; the toast auto-dismisses after five seconds.
  pub fn show_toast(&mut self, level: NoticeLevel, message: impl Into<String>) -> Task<Message> {
    self.next_toast_id = self.next_toast_id.wrapping_add(1);
    let id = self.next_toast_id;
    let message = message.into();
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
