use std::sync::Arc;

use jellypilot_auth::login::ConnectionPhase;
use jellypilot_auth::{AuthStore, SavedProfileKey};
use jellypilot_core::artwork_binder::ArtworkBinder;
use jellypilot_core::diagnostics::Diagnostics;
use jellypilot_core::request_gate::RequestGate;
use jellypilot_media_server::artwork::ArtworkAdapter;
use jellypilot_media_server::JellyfinClient;

use super::state::{ArtworkHandleRetention, ConnectedIdentity, ToastNotice};
use crate::tray::Tray;

/// Cross-surface machinery shared by every surface module (ADR 0029): server
/// auth/connection, request gating, diagnostics, user notifications, tray, and
/// the artwork pipeline that ADR 0028's streaming loader drives for all views.
pub struct Kernel {
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
