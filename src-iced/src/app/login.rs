//! Login surface (ADR 0029): provider/server/credential form state, Quick
//! Connect, password authentication, and saved-profile restore.
//!
//! Authentication itself is owned by the shared [`jellypilot_sdk::Sdk`]:
//! password, Quick Connect, and saved-profile validation all produce an SDK
//! [`ProfileCandidate`], and [`Sdk::activate_candidate`] performs the single
//! committed adoption. This module keeps only form state, presentation, and
//! the message plumbing between the SDK and the UI loop.

use std::sync::Arc;

use iced::futures::SinkExt;
use iced::Task;
use jellypilot_auth::login::{can_start_login, provider_key, validate_server_url, ConnectionPhase};
use jellypilot_auth::SavedProfileKey;
use jellypilot_core::config::{LoginPrefill, Settings};
use jellypilot_media_server::MediaServerProvider;
use jellypilot_sdk::{
  ProfileCandidate, QuickConnectListener, QuickConnectOutcome, QuickConnectSession, Sdk, SdkError,
};
use zeroize::Zeroizing;

use super::accounts;
use super::kernel::Kernel;
use super::message::{
  LoginMessage, Message, PasswordSubmission, ProtectedCandidate, ProtectedOutcome,
  QuickConnectEvent,
};
use super::state::{LoginMethod, LoginState, QuickConnectState};
use crate::i18n::UiText;
use jellypilot_core::diagnostics::{DiagnosticCategory, DiagnosticLevel};

/// Maps an SDK failure to the closest localized login error.
pub(crate) fn sdk_error_text(error: &SdkError) -> UiText {
  UiText::new(match error {
    SdkError::ProfileNotFound => "login-profile-missing",
    SdkError::Storage(_) => "login-storage-unavailable",
    _ => "login-request-failed",
  })
}

fn invalid_server_text(provider: MediaServerProvider) -> UiText {
  UiText::new("login-invalid-server").arg(
    "provider",
    match provider {
      MediaServerProvider::Jellyfin => "Jellyfin",
      MediaServerProvider::Emby => "Emby",
    },
  )
}

/// Login surface slice: the credential form flow plus the live SDK Quick
/// Connect session and its event generation.
pub struct Surface {
  pub flow: LoginState,
  pub password_task: Option<iced::task::Handle>,
  /// Password and restore completions retain their request identity even after
  /// their task has queued a message and can no longer be cancelled.
  pub request_seq: u64,
  /// Activation cannot be replaced once its committed SDK task is admitted.
  pub activation_pending: bool,
  pub quick_connect_session: Option<Arc<QuickConnectSession>>,
  /// Monotonic session counter; events carry the sequence they belong to so
  /// a late terminal outcome can never settle a newer session.
  pub qc_seq: u64,
}

/// Login form used while another media-server session remains active.
///
/// A successful request yields a validated SDK candidate; adopting it remains
/// the account coordinator's responsibility. Cancellation drops in-flight
/// futures and cancels the SDK Quick Connect session; late events are fenced
/// by the surface instance and per-method request sequences.
pub struct CandidateSurface {
  pub flow: LoginState,
  pub password_busy: bool,
  instance: u64,
  quick_connect_session: Option<Arc<QuickConnectSession>>,
  qc_seq: u64,
  password_task: Option<iced::task::Handle>,
  password_seq: u64,
}

impl CandidateSurface {
  pub fn new(settings: &Settings, instance: u64) -> Self {
    let mut flow = LoginState::from_settings(settings);
    flow.profiles_loading = false;
    flow.auto_login_attempted = true;
    Self {
      flow,
      password_busy: false,
      instance,
      quick_connect_session: None,
      qc_seq: 0,
      password_task: None,
      password_seq: 0,
    }
  }

  pub fn busy(&self) -> bool {
    self.password_busy
      || self.quick_connect_session.is_some()
      || matches!(
        self.flow.quick_connect,
        QuickConnectState::Requesting
          | QuickConnectState::Waiting(_)
          | QuickConnectState::Approving
      )
  }

  pub fn cancel(&mut self) {
    self.qc_seq = self.qc_seq.wrapping_add(1);
    self.password_seq = self.password_seq.wrapping_add(1);
    if let Some(session) = self.quick_connect_session.take() {
      session.cancel();
    }
    if let Some(handle) = self.password_task.take() {
      handle.abort();
    }
    self.password_busy = false;
    self.flow.password.clear();
    self.flow.reset_quick_connect();
  }
}

#[derive(Clone)]
pub enum CandidateMessage {
  ProviderSelected(MediaServerProvider),
  MethodSelected(LoginMethod),
  ServerUrlChanged(String),
  UsernameChanged(String),
  PasswordChanged(String),
  RememberToggled,
  QuickConnectSubmitted,
  QuickConnectCancelled,
  PasswordSubmitted,
  QuickConnectEvent {
    instance: u64,
    session: u64,
    event: QuickConnectEvent,
  },
  PasswordFinished {
    instance: u64,
    request: u64,
    result: Result<ProtectedCandidate, SdkError>,
    submission: PasswordSubmission,
  },
}

pub struct CandidateCompletion {
  pub candidate: ProfileCandidate,
  pub submission: Option<PasswordSubmission>,
}

pub struct CandidateUpdate {
  pub task: Task<CandidateMessage>,
  pub completion: Option<CandidateCompletion>,
}

impl CandidateUpdate {
  fn none() -> Self {
    Self {
      task: Task::none(),
      completion: None,
    }
  }
}

/// Forwards SDK Quick Connect callbacks into the UI event stream.
struct UiQuickConnectListener {
  sender: tokio::sync::mpsc::UnboundedSender<QuickConnectEvent>,
}

impl QuickConnectListener for UiQuickConnectListener {
  fn on_code(&self, code: String) {
    let _ = self.sender.send(QuickConnectEvent::Code(code));
  }

  fn on_approving(&self) {
    let _ = self.sender.send(QuickConnectEvent::Approving);
  }

  fn on_completed(&self, outcome: QuickConnectOutcome) {
    let _ = self
      .sender
      .send(QuickConnectEvent::Completed(ProtectedOutcome::new(outcome)));
  }
}

/// Starts an SDK Quick Connect session and returns its handle plus the task
/// draining listener events into `map`ped messages. The drain ends when the
/// SDK drops the listener after the terminal outcome.
fn start_sdk_quick_connect(
  sdk: &Sdk,
  provider: MediaServerProvider,
  server_url: String,
  map: impl Fn(QuickConnectEvent) -> CandidateMessage + Send + 'static,
) -> Result<(Arc<QuickConnectSession>, Task<CandidateMessage>), SdkError> {
  let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
  let listener: Arc<dyn QuickConnectListener> = Arc::new(UiQuickConnectListener { sender });
  let session = sdk.start_quick_connect(provider, server_url, listener)?;
  let stream = iced::stream::channel(8, async move |mut output| {
    while let Some(event) = receiver.recv().await {
      if output.send(map(event)).await.is_err() {
        break;
      }
    }
  });
  Ok((session, Task::run(stream, std::convert::identity)))
}

/// Reduces one isolated add-account authentication event.
///
/// The returned completion owns the validated candidate but does not replace
/// the live [`Kernel`] client. Callers may discard it safely on cancellation.
pub fn update_candidate(
  surface: &mut CandidateSurface,
  sdk: &Arc<Sdk>,
  message: CandidateMessage,
) -> CandidateUpdate {
  match message {
    CandidateMessage::ProviderSelected(provider) => {
      if !surface.busy() {
        surface.flow.select_provider(provider);
        surface.flow.error = None;
      }
      CandidateUpdate::none()
    }
    CandidateMessage::MethodSelected(method) => {
      if !surface.busy() && surface.flow.provider == MediaServerProvider::Jellyfin {
        surface.flow.method = method;
        surface.flow.error = None;
      }
      CandidateUpdate::none()
    }
    CandidateMessage::ServerUrlChanged(value) => {
      // Keep the exact address fixed while a Quick Connect code is live.
      if !surface.busy() {
        surface.flow.server_url = value;
        surface.flow.error = None;
      }
      CandidateUpdate::none()
    }
    CandidateMessage::UsernameChanged(value) => {
      if !surface.busy() {
        surface.flow.username = value;
        surface.flow.error = None;
      }
      CandidateUpdate::none()
    }
    CandidateMessage::PasswordChanged(value) => {
      if !surface.busy() {
        surface.flow.password = Zeroizing::new(value);
        surface.flow.error = None;
      }
      CandidateUpdate::none()
    }
    CandidateMessage::RememberToggled => {
      if !surface.busy() {
        surface.flow.remember = !surface.flow.remember;
      }
      CandidateUpdate::none()
    }
    CandidateMessage::QuickConnectSubmitted => start_candidate_quick_connect(surface, sdk),
    CandidateMessage::QuickConnectCancelled => {
      surface.cancel();
      surface.flow.error = None;
      CandidateUpdate::none()
    }
    CandidateMessage::PasswordSubmitted => start_candidate_password_login(surface, sdk),
    CandidateMessage::QuickConnectEvent {
      instance,
      session,
      event,
    } => {
      if instance != surface.instance || session != surface.qc_seq {
        return CandidateUpdate::none();
      }
      handle_candidate_qc_event(surface, event)
    }
    CandidateMessage::PasswordFinished {
      instance,
      request,
      result,
      submission,
    } => {
      if instance != surface.instance
        || request != surface.password_seq
        || surface.password_task.take().is_none()
      {
        return CandidateUpdate::none();
      }
      surface.password_busy = false;
      finish_candidate_authentication(surface, result, Some(submission))
    }
  }
}

fn start_candidate_quick_connect(
  surface: &mut CandidateSurface,
  sdk: &Arc<Sdk>,
) -> CandidateUpdate {
  if surface.busy() {
    return CandidateUpdate::none();
  }
  if surface.flow.provider != MediaServerProvider::Jellyfin {
    surface.flow.method = LoginMethod::Password;
    return CandidateUpdate::none();
  }
  let server_url = match validate_server_url(&surface.flow.server_url, surface.flow.provider) {
    Ok(server_url) => server_url,
    Err(_) => {
      surface.flow.error = Some(invalid_server_text(surface.flow.provider));
      return CandidateUpdate::none();
    }
  };
  surface.flow.server_url = server_url.clone();
  if let Some(session) = surface.quick_connect_session.take() {
    session.cancel();
  }
  surface.qc_seq = surface.qc_seq.wrapping_add(1);
  let qc_seq = surface.qc_seq;
  let instance = surface.instance;
  match start_sdk_quick_connect(sdk, surface.flow.provider, server_url, move |event| {
    CandidateMessage::QuickConnectEvent {
      instance,
      session: qc_seq,
      event,
    }
  }) {
    Ok((session, task)) => {
      surface.quick_connect_session = Some(session);
      surface.flow.quick_connect = QuickConnectState::Requesting;
      surface.flow.error = None;
      CandidateUpdate {
        task,
        completion: None,
      }
    }
    Err(error) => {
      surface.flow.error = Some(sdk_error_text(&error));
      CandidateUpdate::none()
    }
  }
}

fn start_candidate_password_login(
  surface: &mut CandidateSurface,
  sdk: &Arc<Sdk>,
) -> CandidateUpdate {
  if surface.busy() {
    return CandidateUpdate::none();
  }
  let server_url = match validate_server_url(&surface.flow.server_url, surface.flow.provider) {
    Ok(server_url) => server_url,
    Err(_) => {
      surface.flow.error = Some(invalid_server_text(surface.flow.provider));
      return CandidateUpdate::none();
    }
  };
  surface.flow.server_url = server_url.clone();
  let username = surface.flow.username.trim().to_owned();
  if username.is_empty() {
    surface.flow.error = Some(UiText::new("login-username-required"));
    return CandidateUpdate::none();
  }

  let instance = surface.instance;
  surface.password_seq = surface.password_seq.wrapping_add(1);
  let request = surface.password_seq;
  surface.password_busy = true;
  surface.flow.error = None;
  let submission = candidate_password_submission(surface, server_url.clone(), username.clone());
  let password = std::mem::take(&mut *surface.flow.password);
  let sdk = Arc::clone(sdk);
  let provider = surface.flow.provider;
  let task = Task::perform(
    async move {
      sdk
        .password_login(provider, server_url, username, password)
        .await
    },
    move |result| CandidateMessage::PasswordFinished {
      instance,
      request,
      result: result.map(ProtectedCandidate::new),
      submission,
    },
  );
  let (task, handle) = task.abortable();
  surface.password_task = Some(handle);
  CandidateUpdate {
    task,
    completion: None,
  }
}

fn candidate_password_submission(
  surface: &CandidateSurface,
  server_url: String,
  username: String,
) -> PasswordSubmission {
  PasswordSubmission {
    remember: surface.flow.remember,
    prefill: LoginPrefill::new(server_url, username),
    provider: surface.flow.provider,
  }
}

fn handle_candidate_qc_event(
  surface: &mut CandidateSurface,
  event: QuickConnectEvent,
) -> CandidateUpdate {
  match event {
    QuickConnectEvent::Code(code) => {
      surface.flow.quick_connect = QuickConnectState::Waiting(code);
      CandidateUpdate::none()
    }
    QuickConnectEvent::Approving => {
      surface.flow.quick_connect = QuickConnectState::Approving;
      CandidateUpdate::none()
    }
    QuickConnectEvent::Completed(outcome) => {
      surface.qc_seq = surface.qc_seq.wrapping_add(1);
      surface.quick_connect_session = None;
      match outcome.take() {
        Some(QuickConnectOutcome::Success(candidate)) => {
          finish_candidate_authentication(surface, Ok(ProtectedCandidate::new(*candidate)), None)
        }
        Some(QuickConnectOutcome::Failed(error)) => {
          finish_candidate_authentication(surface, Err(error), None)
        }
        Some(QuickConnectOutcome::Cancelled) | None => {
          surface.flow.reset_quick_connect();
          CandidateUpdate::none()
        }
      }
    }
  }
}

fn finish_candidate_authentication(
  surface: &mut CandidateSurface,
  result: Result<ProtectedCandidate, SdkError>,
  submission: Option<PasswordSubmission>,
) -> CandidateUpdate {
  match result.and_then(|candidate| candidate.take().ok_or(SdkError::Cancelled)) {
    Ok(candidate) => {
      surface.flow.password.clear();
      surface.flow.reset_quick_connect();
      surface.flow.error = None;
      CandidateUpdate {
        task: Task::none(),
        completion: Some(CandidateCompletion {
          candidate,
          submission,
        }),
      }
    }
    Err(error) => {
      surface.flow.error = Some(sdk_error_text(&error));
      surface.flow.quick_connect = QuickConnectState::Failed;
      CandidateUpdate::none()
    }
  }
}

/// `can_start_login` is the playback surface's readiness fact
/// (`playback_view.can_start_login`), hoisted by the top-level router so this
/// module never reads playback state.
pub fn update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  can_start_login: bool,
  message: LoginMessage,
) -> Task<Message> {
  if !can_start_login
    && matches!(
      &message,
      LoginMessage::QuickConnectSubmitted
        | LoginMessage::PasswordSubmitted
        | LoginMessage::RestoreProfile(_)
    )
  {
    kernel.diagnostics.record(
      DiagnosticLevel::Error,
      DiagnosticCategory::Auth,
      "Finishing external playback shutdown. Try again in a moment.",
    );
  }
  update_login(surface, kernel, can_start_login, message).map(Message::Login)
}

fn update_login(
  surface: &mut Surface,
  kernel: &mut Kernel,
  can_login: bool,
  message: LoginMessage,
) -> Task<LoginMessage> {
  match message {
    LoginMessage::ProviderSelected(provider) => {
      interrupt_quick_connect(surface, kernel);
      surface.flow.select_provider(provider);
      surface.flow.error = None;
      Task::none()
    }
    LoginMessage::MethodSelected(method) => {
      if surface.flow.provider == MediaServerProvider::Jellyfin {
        if method == LoginMethod::Password {
          interrupt_quick_connect(surface, kernel);
        }
        surface.flow.method = method;
        surface.flow.error = None;
      }
      Task::none()
    }
    LoginMessage::ServerUrlChanged(value) => {
      surface.flow.server_url = value;
      surface.flow.error = None;
      Task::none()
    }
    LoginMessage::UsernameChanged(value) => {
      surface.flow.username = value;
      surface.flow.error = None;
      Task::none()
    }
    LoginMessage::PasswordChanged(value) => {
      surface.flow.password = Zeroizing::new(value);
      surface.flow.error = None;
      Task::none()
    }
    LoginMessage::RememberToggled => {
      surface.flow.remember = !surface.flow.remember;
      Task::none()
    }
    LoginMessage::QuickConnectSubmitted => {
      if playback_allows_login(surface, can_login) {
        start_quick_connect(surface, kernel)
      } else {
        Task::none()
      }
    }
    LoginMessage::QuickConnectCancelled => {
      interrupt_quick_connect(surface, kernel);
      surface.flow.error = None;
      Task::none()
    }
    LoginMessage::PasswordSubmitted => {
      if playback_allows_login(surface, can_login) {
        start_password_login(surface, kernel)
      } else {
        Task::none()
      }
    }
    LoginMessage::ProfilesLoaded { revision, result } => {
      if revision != surface.flow.profiles_revision {
        return Task::none();
      }
      surface.flow.profiles_loading = false;
      match result {
        Ok(snapshot) => {
          let selected = snapshot.last_successfully_activated().cloned();
          surface.flow.profiles = snapshot.into_profiles();
          if should_auto_login(&mut surface.flow, kernel, selected.is_some()) {
            if let Some(selected) = selected {
              return start_restore(surface, kernel, selected);
            }
          }
        }
        Err(error) => {
          should_auto_login(&mut surface.flow, kernel, false);
          kernel.diagnostics.record(
            DiagnosticLevel::Error,
            DiagnosticCategory::Auth,
            error.to_string(),
          );
          surface.flow.error = Some(sdk_error_text(&error));
        }
      }
      Task::none()
    }
    LoginMessage::QuickConnectEvent { session, event } => {
      if session != surface.qc_seq {
        return Task::none();
      }
      handle_qc_event(surface, kernel, event)
    }
    LoginMessage::PasswordFinished {
      request,
      result,
      submission,
    } => {
      if request != surface.request_seq || surface.password_task.take().is_none() {
        return Task::none();
      }
      match result {
        Ok(candidate) => {
          persist_password_submission(kernel, submission);
          begin_activation(surface, kernel, candidate)
        }
        Err(error) => {
          fail_password_login(surface, kernel, &error);
          Task::none()
        }
      }
    }
    LoginMessage::RestoreProfile(key) => {
      if playback_allows_login(surface, can_login) {
        start_restore(surface, kernel, key)
      } else {
        Task::none()
      }
    }
    LoginMessage::RestoreFinished {
      request,
      key,
      result,
    } => {
      // Only the tracked restore may settle; a stale completion for another
      // key must not clear the in-flight busy marker.
      if request != surface.request_seq || surface.flow.busy_profile.as_ref() != Some(&key) {
        return Task::none();
      }
      surface.request_seq = surface.request_seq.wrapping_add(1);
      match result {
        Ok(candidate) => begin_activation(surface, kernel, candidate),
        Err(error) => {
          surface.flow.busy_profile = None;
          fail_restore(surface, kernel, &error);
          Task::none()
        }
      }
    }
    LoginMessage::ActivationFinished { result } => finish_activation(surface, kernel, result),
  }
}

fn should_auto_login(flow: &mut LoginState, kernel: &Kernel, has_profiles: bool) -> bool {
  if flow.auto_login_attempted {
    return false;
  }
  flow.auto_login_attempted = true;
  kernel.connection == ConnectionPhase::SignedOut
    && has_profiles
    && kernel.settings.snapshot().auto_login()
}

fn playback_allows_login(surface: &mut Surface, can_login: bool) -> bool {
  if can_login {
    true
  } else {
    surface.flow.error = Some(UiText::new("login-playback-cleanup"));
    false
  }
}

pub fn load_saved_profiles(flow: &LoginState, kernel: &Kernel) -> Task<LoginMessage> {
  let sdk = Arc::clone(&kernel.sdk);
  let revision = flow.profiles_revision;
  Task::perform(async move { sdk.saved_profiles().await }, move |result| {
    LoginMessage::ProfilesLoaded { revision, result }
  })
}

/// Admission shared by form controls and the reducer. Authentication keeps its
/// request identity until settlement; another saved profile must wait for it.
pub fn can_start_authentication(surface: &Surface, kernel: &Kernel) -> bool {
  can_start_login(kernel.connection)
    && !surface.activation_pending
    && surface.password_task.is_none()
    && surface.flow.busy_profile.is_none()
    && surface.quick_connect_session.is_none()
    && !kernel.sdk.content_mutations_blocked()
    && kernel.sdk.active_profile().is_none()
}

fn start_quick_connect(surface: &mut Surface, kernel: &mut Kernel) -> Task<LoginMessage> {
  if !can_start_authentication(surface, kernel) {
    return Task::none();
  }
  if surface.flow.provider != MediaServerProvider::Jellyfin {
    surface.flow.method = LoginMethod::Password;
    return Task::none();
  }
  let server_url = match validate_server_url(&surface.flow.server_url, surface.flow.provider) {
    Ok(server_url) => server_url,
    Err(error) => {
      kernel
        .diagnostics
        .record(DiagnosticLevel::Error, DiagnosticCategory::Auth, &error);
      surface.flow.error = Some(invalid_server_text(surface.flow.provider));
      return Task::none();
    }
  };
  surface.flow.server_url = server_url.clone();

  if let Some(session) = surface.quick_connect_session.take() {
    session.cancel();
  }
  surface.qc_seq = surface.qc_seq.wrapping_add(1);
  let qc_seq = surface.qc_seq;
  match start_sdk_quick_connect(
    &kernel.sdk,
    surface.flow.provider,
    server_url,
    move |event| CandidateMessage::QuickConnectEvent {
      instance: 0,
      session: qc_seq,
      event,
    },
  ) {
    Ok((session, task)) => {
      surface.quick_connect_session = Some(session);
      kernel.connection = ConnectionPhase::Connecting;
      surface.flow.quick_connect = QuickConnectState::Requesting;
      surface.flow.error = None;
      task.map(|message| match message {
        CandidateMessage::QuickConnectEvent { session, event, .. } => {
          LoginMessage::QuickConnectEvent { session, event }
        }
        _ => unreachable!("quick connect drain only emits QuickConnectEvent"),
      })
    }
    Err(error) => {
      kernel.diagnostics.record(
        DiagnosticLevel::Error,
        DiagnosticCategory::Auth,
        "Could not start or activate this sign-in. Try again.",
      );
      surface.flow.error = Some(sdk_error_text(&error));
      Task::none()
    }
  }
}

fn start_password_login(surface: &mut Surface, kernel: &mut Kernel) -> Task<LoginMessage> {
  if !can_start_authentication(surface, kernel) {
    return Task::none();
  }
  let server_url = match validate_server_url(&surface.flow.server_url, surface.flow.provider) {
    Ok(server_url) => server_url,
    Err(error) => {
      kernel
        .diagnostics
        .record(DiagnosticLevel::Error, DiagnosticCategory::Auth, &error);
      surface.flow.error = Some(invalid_server_text(surface.flow.provider));
      return Task::none();
    }
  };
  surface.flow.server_url = server_url.clone();
  let username = surface.flow.username.trim().to_owned();
  if username.is_empty() {
    kernel.diagnostics.record(
      DiagnosticLevel::Error,
      DiagnosticCategory::Auth,
      "Enter your username before signing in.",
    );
    surface.flow.error = Some(UiText::new("login-username-required"));
    return Task::none();
  }

  interrupt_quick_connect(surface, kernel);
  invalidate_pending_authentication(surface);
  let request = surface.request_seq;
  kernel.connection = ConnectionPhase::Connecting;
  surface.flow.error = None;
  let submission = password_submission(surface, server_url.clone(), username.clone());
  let password = std::mem::take(&mut *surface.flow.password);
  let sdk = Arc::clone(&kernel.sdk);
  let provider = surface.flow.provider;
  let task = Task::perform(
    async move {
      sdk
        .password_login(provider, server_url, username, password)
        .await
    },
    move |result| LoginMessage::PasswordFinished {
      request,
      result: result.map(ProtectedCandidate::new),
      submission,
    },
  );
  let (task, handle) = task.abortable();
  surface.password_task = Some(handle);
  task
}

fn password_submission(
  surface: &Surface,
  server_url: String,
  username: String,
) -> PasswordSubmission {
  PasswordSubmission {
    remember: surface.flow.remember,
    prefill: LoginPrefill::new(server_url, username),
    provider: surface.flow.provider,
  }
}

fn handle_qc_event(
  surface: &mut Surface,
  kernel: &mut Kernel,
  event: QuickConnectEvent,
) -> Task<LoginMessage> {
  match event {
    QuickConnectEvent::Code(code) => {
      surface.flow.quick_connect = QuickConnectState::Waiting(code);
      Task::none()
    }
    QuickConnectEvent::Approving => {
      surface.flow.quick_connect = QuickConnectState::Approving;
      Task::none()
    }
    QuickConnectEvent::Completed(outcome) => {
      surface.qc_seq = surface.qc_seq.wrapping_add(1);
      surface.quick_connect_session = None;
      match outcome.take() {
        Some(QuickConnectOutcome::Success(candidate)) => {
          begin_activation(surface, kernel, ProtectedCandidate::new(*candidate))
        }
        Some(QuickConnectOutcome::Failed(error)) => {
          kernel.connection = ConnectionPhase::Failed;
          kernel.diagnostics.record(
            DiagnosticLevel::Error,
            DiagnosticCategory::Auth,
            "Quick Connect failed. Try signing in again.",
          );
          surface.flow.error = Some(sdk_error_text(&error));
          surface.flow.quick_connect = QuickConnectState::Failed;
          Task::none()
        }
        Some(QuickConnectOutcome::Cancelled) | None => {
          surface.flow.reset_quick_connect();
          if kernel.connection == ConnectionPhase::Connecting {
            kernel.connection = ConnectionPhase::SignedOut;
          }
          Task::none()
        }
      }
    }
  }
}

/// Hands a validated candidate to the SDK for the single committed adoption.
///
/// The SDK owns teardown ordering, the client swap, session persistence, and
/// startup-restore recording; this surface only projects the outcome.
fn begin_activation(
  surface: &mut Surface,
  kernel: &mut Kernel,
  candidate: ProtectedCandidate,
) -> Task<LoginMessage> {
  if surface.activation_pending {
    return Task::none();
  }
  let Some(candidate) = candidate.take() else {
    return Task::none();
  };
  surface.activation_pending = true;
  kernel.connection = ConnectionPhase::Connecting;
  surface.flow.password.clear();
  surface.flow.reset_quick_connect();
  surface.flow.error = None;
  let sdk = Arc::clone(&kernel.sdk);
  Task::perform(
    async move { sdk.activate_candidate(candidate, true).await },
    move |result| LoginMessage::ActivationFinished { result },
  )
}

fn finish_activation(
  surface: &mut Surface,
  kernel: &mut Kernel,
  result: Result<jellypilot_sdk::ActivationOutcome, SdkError>,
) -> Task<LoginMessage> {
  if !std::mem::take(&mut surface.activation_pending) {
    return Task::none();
  }
  match result {
    Ok(outcome) => {
      accounts::sync_activated(kernel, &outcome.profile);
      surface.flow.busy_profile = None;
      if let Some(warning) = outcome.persistence_warning {
        kernel.diagnostics.record(
          DiagnosticLevel::Error,
          DiagnosticCategory::Auth,
          format!("Connected, but the login could not be persisted: {warning}."),
        );
        kernel.notice = Some(UiText::new("account-session-save-failed"));
      }
      // The SDK persisted the session itself; refresh the saved-profile list
      // so the new/updated entry appears without a restart.
      surface.flow.profiles_revision = surface.flow.profiles_revision.wrapping_add(1);
      surface.flow.profiles_loading = true;
      load_saved_profiles(&surface.flow, kernel)
    }
    Err(SdkError::HandoffAborted) => {
      // Reversible teardown was abandoned; no profile was adopted.
      surface.flow.busy_profile = None;
      if kernel.client.is_none() {
        kernel.connection = ConnectionPhase::SignedOut;
      }
      Task::none()
    }
    Err(error) => {
      surface.flow.busy_profile = None;
      kernel.connection = ConnectionPhase::Failed;
      kernel.diagnostics.record(
        DiagnosticLevel::Error,
        DiagnosticCategory::Auth,
        "Could not start or activate this sign-in. Try again.",
      );
      surface.flow.error = Some(sdk_error_text(&error));
      Task::none()
    }
  }
}

pub(crate) fn persist_password_submission(kernel: &mut Kernel, submission: PasswordSubmission) {
  let settings_result = if submission.remember {
    kernel.settings.set_login_prefill(
      submission.prefill,
      provider_key(submission.provider).to_owned(),
    )
  } else {
    kernel.settings.clear_login_prefill()
  };
  if let Err(error) = settings_result {
    kernel.diagnostics.record(
      DiagnosticLevel::Error,
      DiagnosticCategory::Auth,
      format!("Could not update remembered sign-in: {error}"),
    );
    kernel.notice = Some(UiText::new("login-prefill-save-failed"));
  }
}

fn start_restore(
  surface: &mut Surface,
  kernel: &mut Kernel,
  key: SavedProfileKey,
) -> Task<LoginMessage> {
  if !can_start_authentication(surface, kernel) {
    return Task::none();
  }
  interrupt_quick_connect(surface, kernel);
  invalidate_pending_authentication(surface);
  let request = surface.request_seq;
  kernel.connection = ConnectionPhase::Connecting;
  surface.flow.busy_profile = Some(key.clone());
  surface.flow.error = None;
  let sdk = Arc::clone(&kernel.sdk);
  let request_key = key.as_str().to_owned();
  Task::perform(
    async move { sdk.restore_saved_profile(request_key).await },
    move |result| LoginMessage::RestoreFinished {
      request,
      key,
      result: result.map(ProtectedCandidate::new),
    },
  )
}

fn invalidate_pending_authentication(surface: &mut Surface) {
  surface.request_seq = surface.request_seq.wrapping_add(1);
  if let Some(handle) = surface.password_task.take() {
    handle.abort();
  }
  surface.flow.busy_profile = None;
}

fn interrupt_quick_connect(surface: &mut Surface, kernel: &mut Kernel) {
  surface.qc_seq = surface.qc_seq.wrapping_add(1);
  if surface.quick_connect_session.is_some()
    || !matches!(surface.flow.quick_connect, QuickConnectState::Idle)
  {
    if let Some(session) = surface.quick_connect_session.take() {
      session.cancel();
    }
    if kernel.connection == ConnectionPhase::Connecting {
      kernel.connection = ConnectionPhase::SignedOut;
    }
    surface.flow.reset_quick_connect();
  }
}

fn fail_password_login(surface: &mut Surface, kernel: &mut Kernel, _error: &SdkError) {
  kernel.connection = ConnectionPhase::Failed;
  kernel.diagnostics.record(
    DiagnosticLevel::Error,
    DiagnosticCategory::Auth,
    "Sign-in failed. Check your server, username, and password, then try again.",
  );
  surface.flow.error = Some(UiText::new("login-password-failed"));
}

fn fail_restore(surface: &mut Surface, kernel: &mut Kernel, _error: &SdkError) {
  kernel.connection = ConnectionPhase::Failed;
  kernel.diagnostics.record(
    DiagnosticLevel::Error,
    DiagnosticCategory::Auth,
    "Could not restore this saved sign-in. Sign in again to refresh it.",
  );
  surface.flow.error = Some(UiText::new("login-restore-failed"));
}

#[cfg(test)]
mod tests {
  use std::fs;
  use std::path::PathBuf;
  use std::sync::atomic::{AtomicU64, Ordering};
  use std::time::Duration;

  use iced::futures::{executor::block_on, StreamExt};
  use jellypilot_auth::login::ValidatedProfileCandidate;
  use jellypilot_auth::SensitiveSavedSession;
  use jellypilot_core::config::SettingsStore;
  use jellypilot_core::diagnostics::Diagnostics;
  use jellypilot_core::request_gate::RequestGate;
  use jellypilot_media_server::{JellyfinClient, SavedSession};

  use super::*;

  struct TestSettingsFile(PathBuf);

  impl Drop for TestSettingsFile {
    fn drop(&mut self) {
      let _ = fs::remove_file(&self.0);
    }
  }

  fn test_fixture() -> (Surface, Kernel, TestSettingsFile) {
    static NEXT_SETTINGS: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
      "jellypilot-login-test-{}-{}.json",
      std::process::id(),
      NEXT_SETTINGS.fetch_add(1, Ordering::Relaxed),
    ));
    let _ = fs::remove_file(&path);
    let settings = SettingsStore::for_test(path.clone());
    let surface = Surface {
      flow: LoginState::from_settings(settings.snapshot()),
      password_task: None,
      request_seq: 0,
      activation_pending: false,
      quick_connect_session: None,
      qc_seq: 0,
    };
    let auth_store = super::super::kernel::test_auth_store();
    let (sdk, sdk_handoff) = super::super::kernel::test_account_runtime(&auth_store);
    let kernel = Kernel {
      item_actions: Default::default(),
      locale: crate::i18n::Localizer::default(),
      settings,
      diagnostics: Diagnostics::default(),
      auth_store,
      sdk,
      sdk_handoff,
      request_gate: RequestGate::default(),
      client: None,
      connection: ConnectionPhase::SignedOut,
      connected_identity: None,
      active_profile: None,
      notice: None,
      active_toast: None,
      undo: crate::app::undo::Runtime::default(),
      next_toast_id: 0,
      tray: None,
      artwork_adapter: Arc::new(jellypilot_media_server::artwork::ArtworkAdapter::new()),
      avatar_adapter: Arc::new(jellypilot_media_server::artwork::ArtworkAdapter::new()),
      profile_avatars: Default::default(),
    };
    (surface, kernel, TestSettingsFile(path))
  }

  fn saved_session(name: &str) -> SavedSession {
    SavedSession {
      provider: MediaServerProvider::Jellyfin,
      server_url: format!("https://{name}.example.test"),
      access_token: format!("{name}-token"),
      user_id: format!("{name}-user-id"),
      user_name: name.to_owned(),
      server_name: Some(name.to_owned()),
      device_id: Some("login-test-device".to_owned()),
    }
  }

  fn profile_key(name: &str) -> SavedProfileKey {
    let session = saved_session(name);
    SavedProfileKey::for_identity(session.provider, &session.server_url, &session.user_id)
  }

  fn candidate(name: &str) -> ProtectedCandidate {
    let client = Arc::new(JellyfinClient::new());
    client.login().adopt_validated_session(&saved_session(name));
    ProtectedCandidate::new(ProfileCandidate::new(
      ValidatedProfileCandidate::from_authenticated_client(client)
        .unwrap_or_else(|_| panic!("authenticated candidate")),
    ))
  }

  fn prepare_password(flow: &mut LoginState) {
    flow.server_url = "https://media.example.test".to_owned();
    flow.username = "ada".to_owned();
    flow.password = Zeroizing::new("secret".to_owned());
  }

  fn submission() -> PasswordSubmission {
    PasswordSubmission {
      remember: false,
      prefill: LoginPrefill::new("https://media.example.test".to_owned(), "ada".to_owned()),
      provider: MediaServerProvider::Jellyfin,
    }
  }

  async fn login_output(task: Task<LoginMessage>) -> LoginMessage {
    let mut stream = iced_runtime::task::into_stream(task).expect("login emitted work");
    tokio::time::timeout(Duration::from_secs(5), async {
      loop {
        match stream.next().await {
          Some(iced_runtime::Action::Output(message)) => return message,
          Some(_) => {}
          None => panic!("login task finished without an output"),
        }
      }
    })
    .await
    .expect("login task settled")
  }

  #[test]
  fn startup_restore_uses_saved_activation_once_and_honors_the_boot_gate() {
    for (enabled, has_selected, already_attempted, expected_restore) in [
      (true, true, false, true),
      (false, true, false, false),
      (true, false, false, false),
      (true, true, true, false),
    ] {
      let (mut surface, mut kernel, _settings_file) = test_fixture();
      kernel.settings.set_auto_login(enabled).unwrap();
      surface.flow.auto_login_attempted = already_attempted;
      let (key, _) = block_on(kernel.auth_store.save_session(
        SensitiveSavedSession::from_saved_session(saved_session("startup")),
      ))
      .unwrap();
      if has_selected {
        block_on(kernel.auth_store.record_successful_activation(key.clone())).unwrap();
      }
      for _ in 0..2 {
        let snapshot = block_on(kernel.sdk.saved_profiles()).unwrap();
        drop(update_login(
          &mut surface,
          &mut kernel,
          true,
          LoginMessage::ProfilesLoaded {
            revision: 0,
            result: Ok(snapshot),
          },
        ));
      }
      assert_eq!(
        surface.flow.busy_profile.as_ref() == Some(&key),
        expected_restore
      );
      assert_eq!(surface.request_seq, u64::from(expected_restore));
    }
  }

  #[test]
  fn invalid_server_is_rejected_before_authentication_starts() {
    let (mut surface, mut kernel, _settings_file) = test_fixture();
    surface.flow.server_url = "not a server".to_owned();
    for message in [
      LoginMessage::QuickConnectSubmitted,
      LoginMessage::PasswordSubmitted,
    ] {
      let task = update_login(&mut surface, &mut kernel, true, message);
      assert_eq!(task.units(), 0);
      assert!(surface.flow.error.is_some());
      assert_eq!(kernel.connection, ConnectionPhase::SignedOut);
      assert!(surface.password_task.is_none());
      assert!(surface.quick_connect_session.is_none());
      assert!(kernel.sdk.active_profile().is_none());
    }
  }

  #[test]
  fn candidate_authentication_does_not_adopt_or_replace_the_active_profile() {
    let (_, mut kernel, _settings_file) = test_fixture();
    kernel.sdk.adopt_test_session(saved_session("active"));
    let active = kernel.sdk.active_profile().unwrap();
    accounts::sync_activated(&mut kernel, &active);
    let active_request = kernel.request_gate.current_session();
    let mut surface = CandidateSurface::new(kernel.settings.snapshot(), 1);
    prepare_password(&mut surface.flow);
    drop(update_candidate(
      &mut surface,
      &kernel.sdk,
      CandidateMessage::PasswordSubmitted,
    ));
    let request = surface.password_seq;
    let result = update_candidate(
      &mut surface,
      &kernel.sdk,
      CandidateMessage::PasswordFinished {
        instance: 1,
        request,
        result: Ok(candidate("candidate")),
        submission: submission(),
      },
    );

    assert_eq!(
      result.completion.unwrap().candidate.key(),
      profile_key("candidate").as_str()
    );
    assert_eq!(kernel.sdk.active_profile().unwrap().key, active.key);
    assert_eq!(kernel.request_gate.current_session(), active_request);
    assert_eq!(
      kernel.connected_identity.as_ref().unwrap().user_name,
      "active"
    );
  }

  #[test]
  fn candidate_fields_remain_locked_while_a_quick_connect_code_is_live() {
    let (_, kernel, _settings_file) = test_fixture();
    let mut surface = CandidateSurface::new(kernel.settings.snapshot(), 1);
    prepare_password(&mut surface.flow);
    surface.flow.quick_connect = QuickConnectState::Waiting("ABC123".to_owned());
    for message in [
      CandidateMessage::ServerUrlChanged("https://other.example.test".to_owned()),
      CandidateMessage::UsernameChanged("other".to_owned()),
      CandidateMessage::ProviderSelected(MediaServerProvider::Emby),
    ] {
      drop(update_candidate(&mut surface, &kernel.sdk, message));
    }
    assert_eq!(surface.flow.server_url, "https://media.example.test");
    assert_eq!(surface.flow.username, "ada");
    assert_eq!(surface.flow.provider, MediaServerProvider::Jellyfin);
  }

  #[test]
  fn candidate_cancellation_fences_queued_quick_connect_success() {
    let (_, kernel, _settings_file) = test_fixture();
    let mut surface = CandidateSurface::new(kernel.settings.snapshot(), 1);
    surface.flow.quick_connect = QuickConnectState::Waiting("ABC123".to_owned());
    let session = surface.qc_seq;
    let late = candidate("late");
    let outcome =
      ProtectedOutcome::new(QuickConnectOutcome::Success(Box::new(late.take().unwrap())));
    surface.cancel();
    for event in [
      QuickConnectEvent::Code("LATE12".to_owned()),
      QuickConnectEvent::Completed(outcome),
    ] {
      let result = update_candidate(
        &mut surface,
        &kernel.sdk,
        CandidateMessage::QuickConnectEvent {
          instance: 1,
          session,
          event,
        },
      );
      assert!(result.completion.is_none());
    }
    assert_eq!(surface.flow.quick_connect, QuickConnectState::Idle);
    assert!(!surface.busy());
    assert!(kernel.sdk.active_profile().is_none());
  }

  #[test]
  fn candidate_password_retry_rejects_completion_queued_before_cancel() {
    let (_, kernel, _settings_file) = test_fixture();
    let mut surface = CandidateSurface::new(kernel.settings.snapshot(), 1);
    prepare_password(&mut surface.flow);
    drop(update_candidate(
      &mut surface,
      &kernel.sdk,
      CandidateMessage::PasswordSubmitted,
    ));
    let old_request = surface.password_seq;
    surface.cancel();
    prepare_password(&mut surface.flow);
    drop(update_candidate(
      &mut surface,
      &kernel.sdk,
      CandidateMessage::PasswordSubmitted,
    ));
    let new_request = surface.password_seq;
    let stale = update_candidate(
      &mut surface,
      &kernel.sdk,
      CandidateMessage::PasswordFinished {
        instance: 1,
        request: old_request,
        result: Ok(candidate("old")),
        submission: submission(),
      },
    );
    assert!(stale.completion.is_none());
    assert!(surface.password_busy);
    assert!(surface.password_task.is_some());
    let fresh = update_candidate(
      &mut surface,
      &kernel.sdk,
      CandidateMessage::PasswordFinished {
        instance: 1,
        request: new_request,
        result: Ok(candidate("fresh")),
        submission: submission(),
      },
    );
    assert_eq!(
      fresh.completion.unwrap().candidate.key(),
      profile_key("fresh").as_str()
    );
  }

  #[test]
  fn reopened_candidate_rejects_a_prior_instances_password_completion() {
    let (_, kernel, _settings_file) = test_fixture();
    let mut surface = CandidateSurface::new(kernel.settings.snapshot(), 2);
    prepare_password(&mut surface.flow);
    drop(update_candidate(
      &mut surface,
      &kernel.sdk,
      CandidateMessage::PasswordSubmitted,
    ));
    let request = surface.password_seq;
    let result = update_candidate(
      &mut surface,
      &kernel.sdk,
      CandidateMessage::PasswordFinished {
        instance: 1,
        request,
        result: Ok(candidate("old")),
        submission: submission(),
      },
    );
    assert!(result.completion.is_none());
    assert!(surface.password_busy);
    assert!(surface.password_task.is_some());
    surface.cancel();
  }

  #[test]
  fn quick_connect_cancel_and_retry_rejects_old_progress_and_completion() {
    let (mut surface, mut kernel, _settings_file) = test_fixture();
    surface.flow.server_url = "http://127.0.0.1:1".to_owned();
    drop(update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::QuickConnectSubmitted,
    ));
    let stale_session = surface.qc_seq;
    drop(update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::QuickConnectCancelled,
    ));
    assert_eq!(surface.flow.quick_connect, QuickConnectState::Idle);
    drop(update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::QuickConnectSubmitted,
    ));
    for event in [
      QuickConnectEvent::Code("OLD123".to_owned()),
      QuickConnectEvent::Completed(ProtectedOutcome::new(QuickConnectOutcome::Failed(
        SdkError::Authentication("old failure".to_owned()),
      ))),
    ] {
      drop(update_login(
        &mut surface,
        &mut kernel,
        true,
        LoginMessage::QuickConnectEvent {
          session: stale_session,
          event,
        },
      ));
    }
    assert!(surface.quick_connect_session.is_some());
    assert_eq!(kernel.connection, ConnectionPhase::Connecting);
    assert_eq!(surface.flow.quick_connect, QuickConnectState::Requesting);
    interrupt_quick_connect(&mut surface, &mut kernel);
  }

  #[test]
  fn selecting_emby_cancels_quick_connect_and_fences_its_queued_code() {
    let (mut surface, mut kernel, _settings_file) = test_fixture();
    surface.flow.quick_connect = QuickConnectState::Waiting("ABC123".to_owned());
    kernel.connection = ConnectionPhase::Connecting;
    let session = surface.qc_seq;
    drop(update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::ProviderSelected(MediaServerProvider::Emby),
    ));
    drop(update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::QuickConnectEvent {
        session,
        event: QuickConnectEvent::Code("LATE12".to_owned()),
      },
    ));
    assert_eq!(surface.flow.method, LoginMethod::Password);
    assert_eq!(surface.flow.quick_connect, QuickConnectState::Idle);
    assert_eq!(kernel.connection, ConnectionPhase::SignedOut);
  }

  #[test]
  fn restore_cannot_replace_password_even_when_its_success_is_already_queued() {
    let (mut surface, mut kernel, _settings_file) = test_fixture();
    prepare_password(&mut surface.flow);
    drop(update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::PasswordSubmitted,
    ));
    let request = surface.request_seq;
    let key = profile_key("restore");
    let restore = update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::RestoreProfile(key),
    );
    assert_eq!(restore.units(), 0);
    assert_eq!(surface.request_seq, request);
    assert!(surface.password_task.is_some());
    let task = update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::PasswordFinished {
        request,
        result: Ok(candidate("password")),
        submission: submission(),
      },
    );
    assert_eq!(task.units(), 1);
    assert!(surface.activation_pending);
    assert!(surface.flow.busy_profile.is_none());
    assert_eq!(kernel.connection, ConnectionPhase::Connecting);
    assert!(kernel.sdk.active_profile().is_none());
  }

  #[test]
  fn password_retry_ignores_a_queued_completion_from_the_settled_request() {
    let (mut surface, mut kernel, _settings_file) = test_fixture();
    prepare_password(&mut surface.flow);
    drop(update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::PasswordSubmitted,
    ));
    let old_request = surface.request_seq;
    drop(update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::PasswordFinished {
        request: old_request,
        result: Err(SdkError::Authentication("first failure".to_owned())),
        submission: submission(),
      },
    ));
    prepare_password(&mut surface.flow);
    drop(update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::PasswordSubmitted,
    ));
    let current_request = surface.request_seq;
    let stale = update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::PasswordFinished {
        request: old_request,
        result: Ok(candidate("old")),
        submission: submission(),
      },
    );
    assert_eq!(stale.units(), 0);
    assert_eq!(surface.request_seq, current_request);
    assert!(surface.password_task.is_some());
    assert!(!surface.activation_pending);
    assert_eq!(kernel.connection, ConnectionPhase::Connecting);
  }

  #[tokio::test]
  async fn running_sdk_restore_rejects_competing_profile_and_can_still_activate() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_url = format!("http://{}", listener.local_addr().unwrap());
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
      for (gate, target, body) in [
        (
          Some((entered_tx, release_rx)),
          "/Users/Me",
          r#"{"Id":"00000000000000000000000000000001","Name":"Ada"}"#,
        ),
        (
          None,
          "/System/Info/Public",
          r#"{"ServerName":"Login fixture","Version":"10.10.0","Id":"server-1"}"#,
        ),
      ] {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
          let received = socket.read(&mut buffer).await.unwrap();
          assert_ne!(received, 0, "the authentication request reached the server");
          request.extend_from_slice(&buffer[..received]);
        }
        assert!(String::from_utf8_lossy(&request).starts_with(&format!("GET {target} ")));
        if let Some((entered, release)) = gate {
          entered.send(()).unwrap();
          release.await.unwrap();
        }
        let response = format!(
          "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
          body.len(),
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        socket.shutdown().await.unwrap();
      }
    });

    let (mut surface, mut kernel, _settings_file) = test_fixture();
    let mut session = saved_session("first");
    session.server_url = server_url;
    session.user_id = "00000000000000000000000000000001".to_owned();
    let (first_key, _) = kernel
      .auth_store
      .save_session(SensitiveSavedSession::from_saved_session(session))
      .await
      .unwrap();
    let task = update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::RestoreProfile(first_key.clone()),
    );
    let request = surface.request_seq;
    let mut stream = iced_runtime::task::into_stream(task).expect("restore task");
    tokio::time::timeout(Duration::from_secs(5), async {
      tokio::select! {
        _ = stream.next() => panic!("restore settled before the held server response"),
        result = entered_rx => result.expect("SDK holds account admission while validating A"),
      }
    })
    .await
    .unwrap();

    let competing = update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::RestoreProfile(profile_key("second")),
    );
    assert_eq!(competing.units(), 0);
    assert_eq!(surface.request_seq, request);
    assert_eq!(surface.flow.busy_profile.as_ref(), Some(&first_key));
    assert!(!can_start_authentication(&surface, &kernel));

    release_tx.send(()).unwrap();
    let completion = tokio::time::timeout(Duration::from_secs(5), stream.next())
      .await
      .unwrap();
    let Some(iced_runtime::Action::Output(completion)) = completion else {
      panic!("restore emits its candidate completion");
    };
    let activation = update_login(&mut surface, &mut kernel, true, completion);
    assert!(surface.activation_pending);
    let activated = login_output(activation).await;
    drop(update_login(&mut surface, &mut kernel, true, activated));
    assert_eq!(kernel.active_profile, Some(first_key.clone()));
    assert_eq!(kernel.sdk.active_profile().unwrap().key, first_key.as_str());
    assert!(surface.flow.error.is_none());
    server.await.unwrap();
  }

  #[tokio::test]
  async fn sdk_sign_out_handoff_blocks_standalone_authentication_starts() {
    let (mut surface, mut kernel, _settings_file) = test_fixture();
    prepare_password(&mut surface.flow);
    let session = saved_session("active");
    let (key, _) = kernel
      .auth_store
      .save_session(SensitiveSavedSession::from_saved_session(session.clone()))
      .await
      .unwrap();
    kernel.sdk.adopt_test_session(session);
    let sdk = Arc::clone(&kernel.sdk);
    let sign_out = tokio::spawn(async move { sdk.sign_out(key.as_str().to_owned(), false).await });
    let hook = tokio::time::timeout(Duration::from_secs(5), async {
      kernel
        .sdk_handoff
        .receiver
        .lock()
        .await
        .recv()
        .await
        .unwrap()
    })
    .await
    .unwrap();
    assert!(kernel.sdk.content_mutations_blocked());
    // The native connection projection still says SignedOut; admission must
    // also consult the SDK while its committed transaction is pending.
    assert_eq!(kernel.connection, ConnectionPhase::SignedOut);
    for message in [
      LoginMessage::PasswordSubmitted,
      LoginMessage::QuickConnectSubmitted,
      LoginMessage::RestoreProfile(profile_key("other")),
    ] {
      let task = update_login(&mut surface, &mut kernel, true, message);
      assert_eq!(task.units(), 0);
      assert!(surface.flow.busy_profile.is_none());
      assert!(surface.password_task.is_none());
      assert!(surface.quick_connect_session.is_none());
    }
    hook.resolve(true);
    tokio::time::timeout(Duration::from_secs(5), sign_out)
      .await
      .unwrap()
      .unwrap()
      .unwrap();
  }

  #[test]
  fn stale_restore_does_not_settle_a_new_request_even_for_the_same_profile() {
    for same_profile in [false, true] {
      let (mut surface, mut kernel, _settings_file) = test_fixture();
      let first_key = profile_key("first");
      let second_key = profile_key(if same_profile { "first" } else { "second" });
      drop(update_login(
        &mut surface,
        &mut kernel,
        true,
        LoginMessage::RestoreProfile(first_key.clone()),
      ));
      let request = surface.request_seq;
      drop(update_login(
        &mut surface,
        &mut kernel,
        true,
        LoginMessage::RestoreFinished {
          request,
          key: first_key.clone(),
          result: Err(SdkError::Authentication("first failure".to_owned())),
        },
      ));
      drop(update_login(
        &mut surface,
        &mut kernel,
        true,
        LoginMessage::RestoreProfile(second_key.clone()),
      ));
      drop(update_login(
        &mut surface,
        &mut kernel,
        true,
        LoginMessage::RestoreFinished {
          request,
          key: first_key,
          result: Err(SdkError::Authentication("stale failure".to_owned())),
        },
      ));
      assert_eq!(surface.flow.busy_profile.as_ref(), Some(&second_key));
      assert_eq!(kernel.connection, ConnectionPhase::Connecting);
      assert!(surface.flow.error.is_none());
    }
  }

  #[test]
  fn restore_waits_for_explicit_quick_connect_cancellation_and_fences_late_success() {
    let (mut surface, mut kernel, _settings_file) = test_fixture();
    surface.flow.server_url = "http://127.0.0.1:1".to_owned();
    drop(update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::QuickConnectSubmitted,
    ));
    let session = surface.qc_seq;
    let key = profile_key("restore");
    let blocked = update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::RestoreProfile(key.clone()),
    );
    assert_eq!(blocked.units(), 0);
    assert!(surface.quick_connect_session.is_some());
    drop(update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::QuickConnectCancelled,
    ));
    drop(update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::RestoreProfile(key.clone()),
    ));
    let stale = candidate("stale").take().unwrap();
    let task = update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::QuickConnectEvent {
        session,
        event: QuickConnectEvent::Completed(ProtectedOutcome::new(QuickConnectOutcome::Success(
          Box::new(stale),
        ))),
      },
    );
    assert_eq!(task.units(), 0);
    assert!(surface.quick_connect_session.is_none());
    assert_eq!(surface.flow.quick_connect, QuickConnectState::Idle);
    assert_eq!(surface.flow.busy_profile.as_ref(), Some(&key));
  }

  #[test]
  fn login_submissions_and_stale_quick_connect_cancel_do_not_replace_inflight_password() {
    let (mut surface, mut kernel, _settings_file) = test_fixture();
    prepare_password(&mut surface.flow);
    drop(update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::PasswordSubmitted,
    ));
    let request = surface.request_seq;
    surface.flow.password = Zeroizing::new("new secret".to_owned());
    for message in [
      LoginMessage::QuickConnectCancelled,
      LoginMessage::QuickConnectSubmitted,
      LoginMessage::PasswordSubmitted,
    ] {
      drop(update_login(&mut surface, &mut kernel, true, message));
    }
    assert_eq!(surface.request_seq, request);
    assert_eq!(surface.flow.password.as_str(), "new secret");
    assert!(surface.password_task.is_some());
    assert_eq!(kernel.connection, ConnectionPhase::Connecting);
  }

  #[tokio::test]
  async fn sdk_activation_refreshes_profiles_and_fences_earlier_snapshots() {
    let (mut surface, mut kernel, _settings_file) = test_fixture();
    surface.flow.auto_login_attempted = true;
    let stale_snapshot = kernel.sdk.saved_profiles().await.unwrap();
    let task = begin_activation(&mut surface, &mut kernel, candidate("new"));
    // A queued restore must not replace an activation before its lazy task
    // starts polling and the SDK raises its own handoff admission gate.
    let request = surface.request_seq;
    drop(update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::RestoreProfile(profile_key("old")),
    ));
    assert_eq!(surface.request_seq, request);
    assert!(surface.flow.busy_profile.is_none());
    let completion = login_output(task).await;
    let refresh = update_login(&mut surface, &mut kernel, true, completion);
    assert_eq!(kernel.active_profile, Some(profile_key("new")));
    assert_eq!(
      kernel.sdk.active_profile().unwrap().key,
      profile_key("new").as_str()
    );
    drop(update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::ProfilesLoaded {
        revision: 0,
        result: Ok(stale_snapshot),
      },
    ));
    assert!(surface.flow.profiles_loading);
    let refreshed = login_output(refresh).await;
    drop(update_login(&mut surface, &mut kernel, true, refreshed));
    assert_eq!(surface.flow.profiles.len(), 1);
    assert_eq!(surface.flow.profiles[0].key, profile_key("new"));
    assert!(!surface.flow.profiles_loading);
    assert!(surface.flow.error.is_none());
  }

  #[test]
  fn password_completion_persists_submitted_prefill_after_form_edits() {
    let (mut surface, mut kernel, _settings_file) = test_fixture();
    prepare_password(&mut surface.flow);
    surface.flow.remember = true;
    drop(update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::PasswordSubmitted,
    ));
    let request = surface.request_seq;
    let submitted = password_submission(
      &surface,
      "https://submitted.example.test".to_owned(),
      "submitted-user".to_owned(),
    );
    surface.flow.server_url = "https://edited.example.test".to_owned();
    surface.flow.username = "edited-user".to_owned();
    surface.flow.remember = false;
    surface.flow.provider = MediaServerProvider::Emby;
    drop(update_login(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::PasswordFinished {
        request,
        result: Ok(candidate("submitted")),
        submission: submitted,
      },
    ));
    let persisted = kernel.settings.snapshot();
    assert!(persisted.remembers_login_prefill());
    assert_eq!(
      persisted.login_prefill().server_url(),
      "https://submitted.example.test"
    );
    assert_eq!(persisted.login_prefill().username(), "submitted-user");
    assert_eq!(persisted.login_provider(), "jellyfin");
  }

  #[test]
  fn authentication_failures_keep_server_secrets_out_of_localized_ui_and_diagnostics() {
    for method in ["password", "restore", "quick-connect"] {
      let (mut surface, mut kernel, _settings_file) = test_fixture();
      let error = SdkError::Authentication("response echoed credential-sequence-9382".to_owned());
      let message = match method {
        "password" => {
          prepare_password(&mut surface.flow);
          drop(update_login(
            &mut surface,
            &mut kernel,
            true,
            LoginMessage::PasswordSubmitted,
          ));
          LoginMessage::PasswordFinished {
            request: surface.request_seq,
            result: Err(error),
            submission: submission(),
          }
        }
        "restore" => {
          let key = profile_key("restore-error");
          drop(update_login(
            &mut surface,
            &mut kernel,
            true,
            LoginMessage::RestoreProfile(key.clone()),
          ));
          LoginMessage::RestoreFinished {
            request: surface.request_seq,
            key,
            result: Err(error),
          }
        }
        _ => {
          kernel.connection = ConnectionPhase::Connecting;
          surface.flow.quick_connect = QuickConnectState::Approving;
          LoginMessage::QuickConnectEvent {
            session: surface.qc_seq,
            event: QuickConnectEvent::Completed(ProtectedOutcome::new(
              QuickConnectOutcome::Failed(error),
            )),
          }
        }
      };
      drop(update_login(&mut surface, &mut kernel, true, message));
      for language in [
        jellypilot_core::locale::UiLanguage::English,
        jellypilot_core::locale::UiLanguage::SimplifiedChinese,
      ] {
        let locale = crate::i18n::Localizer::new(language);
        let rendered = locale.message(surface.flow.error.as_ref().expect("authentication failure"));
        assert!(!rendered.contains("credential-sequence-9382"));
      }
      assert!(kernel.diagnostics.rows().len() > 0);
      assert!(kernel
        .diagnostics
        .rows()
        .all(|row| !row.message.contains("credential-sequence-9382")));
    }
  }

  #[test]
  fn playback_cleanup_blocks_every_login_entry_point() {
    let (mut surface, mut kernel, _settings_file) = test_fixture();
    prepare_password(&mut surface.flow);
    for message in [
      LoginMessage::PasswordSubmitted,
      LoginMessage::QuickConnectSubmitted,
      LoginMessage::RestoreProfile(profile_key("restore")),
    ] {
      let task = update(&mut surface, &mut kernel, false, message);
      assert_eq!(task.units(), 0);
      assert_eq!(kernel.connection, ConnectionPhase::SignedOut);
      assert!(surface.password_task.is_none());
      assert!(surface.quick_connect_session.is_none());
      assert!(surface.flow.error.is_some());
    }
  }
}
