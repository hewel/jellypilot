//! Login surface (ADR 0029): provider/server/credential form state, Quick
//! Connect, password authentication, and saved-profile restore.

use std::sync::{Arc, Mutex};

use iced::Task;
use jellypilot_auth::login::{
  can_start_login, provider_key, quick_connect_workflow, validate_server_url, ConnectionPhase,
  LoginError, LoginEvent, ValidatedProfileCandidate, QUICK_CONNECT_POLL_INTERVAL,
  QUICK_CONNECT_TIMEOUT,
};
use jellypilot_auth::AuthStore;
use jellypilot_core::config::{LoginPrefill, Settings};
use jellypilot_core::request_gate::{RequestGate, SessionToken};
use jellypilot_media_server::{Credentials, JellyfinClient, MediaServerProvider};
use zeroize::{Zeroize, Zeroizing};

use super::kernel::Kernel;
use super::message::{
  LoginMessage, Message, PasswordSubmission, ProtectedSavedSession, SensitiveSessionPayload,
};
use super::state::{ConnectedIdentity, LoginMethod, LoginState, QuickConnectState};

/// Login surface slice: the credential form flow plus the abort handle for an
/// in-flight Quick Connect stream.
pub struct Surface {
  pub flow: LoginState,
  pub quick_connect_task: Option<iced::task::Handle>,
}

/// Login form used while another media-server session remains active.
///
/// Its request gate and cancellation handle are intentionally independent of
/// [`Kernel`]. A successful request yields a validated candidate; adopting it
/// remains the account coordinator's responsibility.
pub struct CandidateSurface {
  pub flow: LoginState,
  pub password_busy: bool,
  instance: u64,
  request_gate: RequestGate,
  quick_connect_task: Option<iced::task::Handle>,
  password_task: Option<iced::task::Handle>,
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
      request_gate: RequestGate::default(),
      quick_connect_task: None,
      password_task: None,
    }
  }

  pub fn busy(&self) -> bool {
    self.password_busy
      || self.quick_connect_task.is_some()
      || matches!(
        self.flow.quick_connect,
        QuickConnectState::Requesting
          | QuickConnectState::Waiting(_)
          | QuickConnectState::Approving
      )
  }

  pub fn cancel(&mut self) {
    cancel_candidate_quick_connect(self);
    if let Some(handle) = self.password_task.take() {
      handle.abort();
    }
    self.request_gate.disconnect();
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
  WorkflowEvent {
    instance: u64,
    event: LoginEvent,
  },
  PasswordFinished {
    instance: u64,
    session: SessionToken,
    client: Arc<JellyfinClient>,
    result: Result<(), LoginError>,
    submission: PasswordSubmission,
  },
}

pub struct CandidateCompletion {
  pub candidate: ValidatedProfileCandidate,
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

/// Reduces one isolated add-account authentication event.
///
/// The returned completion owns the validated candidate but does not replace
/// the live [`Kernel`] client. Callers may discard it safely on cancellation.
pub fn update_candidate(
  surface: &mut CandidateSurface,
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
    CandidateMessage::QuickConnectSubmitted => start_candidate_quick_connect(surface),
    CandidateMessage::QuickConnectCancelled => {
      surface.cancel();
      surface.flow.error = None;
      CandidateUpdate::none()
    }
    CandidateMessage::PasswordSubmitted => start_candidate_password_login(surface),
    CandidateMessage::WorkflowEvent { instance, event } => {
      if instance != surface.instance {
        return CandidateUpdate::none();
      }
      handle_candidate_workflow_event(surface, event)
    }
    CandidateMessage::PasswordFinished {
      instance,
      session,
      client,
      result,
      submission,
    } => {
      if instance != surface.instance || !surface.request_gate.finish_login(session) {
        return CandidateUpdate::none();
      }
      surface.password_task = None;
      surface.password_busy = false;
      finish_candidate_authentication(surface, client, result, Some(submission))
    }
  }
}

fn start_candidate_quick_connect(surface: &mut CandidateSurface) -> CandidateUpdate {
  if surface.busy() {
    return CandidateUpdate::none();
  }
  if surface.flow.provider != MediaServerProvider::Jellyfin {
    surface.flow.method = LoginMethod::Password;
    return CandidateUpdate::none();
  }
  let server_url = match validate_server_url(&surface.flow.server_url, surface.flow.provider) {
    Ok(server_url) => server_url,
    Err(error) => {
      surface.flow.error = Some(error);
      return CandidateUpdate::none();
    }
  };
  surface.flow.server_url = server_url.clone();
  cancel_candidate_quick_connect(surface);
  let session = surface.request_gate.begin_login();
  surface.flow.quick_connect = QuickConnectState::Requesting;
  surface.flow.error = None;
  let client = Arc::new(JellyfinClient::new());
  let instance = surface.instance;
  let stream = iced::stream::channel(16, async move |sender| {
    let sender = Arc::new(Mutex::new(sender));
    quick_connect_workflow(
      client,
      server_url,
      session,
      move |event| {
        sender
          .lock()
          .is_ok_and(|mut sender| sender.try_send(event).is_ok())
      },
      QUICK_CONNECT_POLL_INTERVAL,
      QUICK_CONNECT_TIMEOUT,
    )
    .await;
  });
  let (task, handle) = Task::run(stream, move |event| CandidateMessage::WorkflowEvent {
    instance,
    event,
  })
  .abortable();
  surface.quick_connect_task = Some(handle);
  CandidateUpdate {
    task,
    completion: None,
  }
}

fn start_candidate_password_login(surface: &mut CandidateSurface) -> CandidateUpdate {
  if surface.busy() {
    return CandidateUpdate::none();
  }
  let server_url = match validate_server_url(&surface.flow.server_url, surface.flow.provider) {
    Ok(server_url) => server_url,
    Err(error) => {
      surface.flow.error = Some(error);
      return CandidateUpdate::none();
    }
  };
  surface.flow.server_url = server_url.clone();
  let username = surface.flow.username.trim().to_owned();
  if username.is_empty() {
    surface.flow.error = Some("Enter your username before signing in.".to_owned());
    return CandidateUpdate::none();
  }

  let session = surface.request_gate.begin_login();
  let instance = surface.instance;
  surface.password_busy = true;
  surface.flow.error = None;
  let client = Arc::new(JellyfinClient::new());
  let command_client = Arc::clone(&client);
  let submission = candidate_password_submission(surface, server_url.clone(), username.clone());
  let credentials = AuthStore::protect_credentials(Credentials {
    provider: surface.flow.provider,
    server_url,
    username,
    password: std::mem::take(&mut *surface.flow.password),
  });
  let task = Task::perform(
    async move {
      let result = async {
        let mut response = command_client
          .login()
          .authenticate(&credentials)
          .await
          .map_err(|_| LoginError::Request("Password authentication failed.".to_owned()))?;
        response.access_token.zeroize();
        Ok(())
      }
      .await;
      (client, result)
    },
    move |(client, result)| CandidateMessage::PasswordFinished {
      instance,
      session,
      client,
      result,
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

fn handle_candidate_workflow_event(
  surface: &mut CandidateSurface,
  event: LoginEvent,
) -> CandidateUpdate {
  match event {
    LoginEvent::QuickConnectCode { session, code } => {
      if surface.request_gate.is_current_login(session) {
        surface.flow.quick_connect = QuickConnectState::Waiting(code);
      }
      CandidateUpdate::none()
    }
    LoginEvent::QuickConnectApproving { session } => {
      if surface.request_gate.is_current_login(session) {
        surface.flow.quick_connect = QuickConnectState::Approving;
      }
      CandidateUpdate::none()
    }
    LoginEvent::Login {
      session,
      client,
      result,
    } => {
      if !surface.request_gate.finish_login(session) {
        return CandidateUpdate::none();
      }
      surface.quick_connect_task = None;
      finish_candidate_authentication(surface, client, result, None)
    }
    LoginEvent::SavedProfiles(_) | LoginEvent::SavedSessionStored { .. } => CandidateUpdate::none(),
  }
}

fn finish_candidate_authentication(
  surface: &mut CandidateSurface,
  client: Arc<JellyfinClient>,
  result: Result<(), LoginError>,
  submission: Option<PasswordSubmission>,
) -> CandidateUpdate {
  match result.and_then(|()| ValidatedProfileCandidate::from_authenticated_client(client)) {
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
      surface.flow.error = Some(error.to_string());
      surface.flow.quick_connect = QuickConnectState::Failed;
      CandidateUpdate::none()
    }
  }
}

fn cancel_candidate_quick_connect(surface: &mut CandidateSurface) {
  if let Some(handle) = surface.quick_connect_task.take() {
    handle.abort();
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
      cancel_quick_connect(surface);
      kernel.connection = ConnectionPhase::SignedOut;
      surface.flow.reset_quick_connect();
      surface.flow.error = None;
      kernel.request_gate.disconnect();
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
      surface.flow.profiles_loading = false;
      if revision != surface.flow.profiles_revision {
        return Task::none();
      }
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
          surface.flow.error = Some(LoginError::AuthStorage(error).to_string());
        }
      }
      Task::none()
    }
    LoginMessage::WorkflowEvent(event) => handle_workflow_event(surface, kernel, can_login, event),
    LoginMessage::PasswordFinished {
      session,
      client,
      result,
      submission,
    } => {
      if !kernel.request_gate.finish_login(session) {
        return Task::none();
      }
      match result {
        Ok(saved_session) => {
          let Some(saved_session) = saved_session.take() else {
            return Task::none();
          };
          complete_authentication(
            surface,
            kernel,
            session,
            client,
            saved_session,
            Some(submission),
          )
        }
        Err(error) => {
          fail_password_login(surface, kernel, &error);
          Task::none()
        }
      }
    }
    LoginMessage::SavedSessionStored { session, result } => {
      let current = kernel.request_gate.is_current_session(session);
      match result {
        Ok((key, profiles)) => {
          surface.flow.profiles_revision = surface.flow.profiles_revision.wrapping_add(1);
          surface.flow.profiles = profiles;
          if current {
            kernel.active_profile = Some(key.clone());
            return record_successful_activation(kernel, session, key);
          }
        }
        Err(error) if current => {
          kernel.notice = Some(LoginError::AuthStorage(error).to_string());
        }
        Err(_) => {}
      }
      Task::none()
    }
    LoginMessage::ActivationRecorded {
      session,
      key,
      result,
    } => {
      if kernel.request_gate.is_current_session(session)
        && kernel.active_profile.as_ref() == Some(&key)
      {
        if let Err(error) = result {
          kernel.notice = Some(format!(
            "Connected, but the startup account selection could not be saved: {error}."
          ));
        }
      }
      Task::none()
    }
    LoginMessage::RestoreProfile(key) => {
      if playback_allows_login(surface, can_login) {
        start_restore(surface, kernel, key)
      } else {
        Task::none()
      }
    }
    LoginMessage::RestoreFinished {
      session,
      key,
      result,
    } => {
      if !kernel.request_gate.finish_login(session) {
        return Task::none();
      }
      if surface.flow.busy_profile.as_ref() == Some(&key) {
        surface.flow.busy_profile = None;
      }
      match result {
        Ok(saved_session) => {
          let Some(saved_session) = saved_session.take() else {
            return Task::none();
          };
          let client = Arc::new(JellyfinClient::new());
          client.login().adopt_validated_session(&saved_session);
          kernel.connection = ConnectionPhase::Connected;
          kernel.connected_identity = Some(ConnectedIdentity::from_session(&saved_session));
          kernel.client = Some(client);
          kernel.active_profile = Some(key.clone());
          surface.flow.error = None;
          return record_successful_activation(kernel, session, key);
        }
        Err(error) => fail_restore(surface, kernel, &error),
      }
      Task::none()
    }
  }
}

fn record_successful_activation(
  kernel: &Kernel,
  session: SessionToken,
  key: jellypilot_auth::SavedProfileKey,
) -> Task<LoginMessage> {
  let store = kernel.auth_store.clone();
  let completion_key = key.clone();
  Task::perform(
    async move { store.record_successful_activation(key).await },
    move |result| LoginMessage::ActivationRecorded {
      session,
      key: completion_key,
      result,
    },
  )
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
    surface.flow.error =
      Some("Finishing external playback shutdown. Try again in a moment.".to_owned());
    false
  }
}

pub fn load_saved_profiles(surface: &Surface, kernel: &Kernel) -> Task<LoginMessage> {
  let store = kernel.auth_store.clone();
  let revision = surface.flow.profiles_revision;
  Task::perform(
    async move { store.load_profiles_snapshot().await },
    move |result| LoginMessage::ProfilesLoaded { revision, result },
  )
}

fn start_quick_connect(surface: &mut Surface, kernel: &mut Kernel) -> Task<LoginMessage> {
  if !can_start_login(kernel.connection) {
    return Task::none();
  }
  if surface.flow.provider != MediaServerProvider::Jellyfin {
    surface.flow.method = LoginMethod::Password;
    return Task::none();
  }
  let server_url = match validate_server_url(&surface.flow.server_url, surface.flow.provider) {
    Ok(server_url) => server_url,
    Err(error) => {
      surface.flow.error = Some(error);
      return Task::none();
    }
  };
  surface.flow.server_url = server_url.clone();

  cancel_quick_connect(surface);
  let session = kernel.request_gate.begin_login();
  kernel.connection = ConnectionPhase::Connecting;
  surface.flow.quick_connect = QuickConnectState::Requesting;
  surface.flow.error = None;
  let client = Arc::new(JellyfinClient::new());
  let stream = iced::stream::channel(16, async move |sender| {
    let sender = Arc::new(Mutex::new(sender));
    quick_connect_workflow(
      client,
      server_url,
      session,
      move |event| {
        sender
          .lock()
          .is_ok_and(|mut sender| sender.try_send(event).is_ok())
      },
      QUICK_CONNECT_POLL_INTERVAL,
      QUICK_CONNECT_TIMEOUT,
    )
    .await;
  });
  let (task, handle) = Task::run(stream, LoginMessage::WorkflowEvent).abortable();
  surface.quick_connect_task = Some(handle);
  task
}

fn start_password_login(surface: &mut Surface, kernel: &mut Kernel) -> Task<LoginMessage> {
  if !can_start_login(kernel.connection) {
    return Task::none();
  }
  let server_url = match validate_server_url(&surface.flow.server_url, surface.flow.provider) {
    Ok(server_url) => server_url,
    Err(error) => {
      surface.flow.error = Some(error);
      return Task::none();
    }
  };
  surface.flow.server_url = server_url.clone();
  let username = surface.flow.username.trim().to_owned();
  if username.is_empty() {
    surface.flow.error = Some("Enter your username before signing in.".to_owned());
    return Task::none();
  }

  cancel_quick_connect(surface);
  let session = kernel.request_gate.begin_login();
  kernel.connection = ConnectionPhase::Connecting;
  surface.flow.error = None;
  let client = Arc::new(JellyfinClient::new());
  let command_client = Arc::clone(&client);
  let submission = password_submission(surface, server_url.clone(), username.clone());
  let credentials = AuthStore::protect_credentials(Credentials {
    provider: surface.flow.provider,
    server_url,
    username,
    password: std::mem::take(&mut *surface.flow.password),
  });
  Task::perform(
    async move {
      let result = async {
        let mut response = command_client
          .login()
          .authenticate(&credentials)
          .await
          .map_err(|_| LoginError::Request("Password authentication failed.".to_owned()))?;
        response.access_token.zeroize();
        jellypilot_auth::SensitiveSavedSession::from_client(&command_client)
          .map(ProtectedSavedSession::new)
          .ok_or_else(|| LoginError::Request("Password authentication failed.".to_owned()))
      }
      .await;
      (client, result)
    },
    move |(client, result)| LoginMessage::PasswordFinished {
      session,
      client,
      result,
      submission,
    },
  )
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

fn handle_workflow_event(
  surface: &mut Surface,
  kernel: &mut Kernel,
  can_login: bool,
  event: LoginEvent,
) -> Task<LoginMessage> {
  match event {
    LoginEvent::QuickConnectCode { session, code } => {
      if kernel.request_gate.is_current_login(session) {
        surface.flow.quick_connect = QuickConnectState::Waiting(code);
      }
      Task::none()
    }
    LoginEvent::QuickConnectApproving { session } => {
      if kernel.request_gate.is_current_login(session) {
        surface.flow.quick_connect = QuickConnectState::Approving;
      }
      Task::none()
    }
    LoginEvent::Login {
      session,
      client,
      result,
    } => {
      if !kernel.request_gate.finish_login(session) {
        return Task::none();
      }
      surface.quick_connect_task = None;
      match result {
        Ok(()) => match jellypilot_auth::SensitiveSavedSession::from_client(&client) {
          Some(saved_session) => {
            complete_authentication(surface, kernel, session, client, saved_session, None)
          }
          None => {
            fail_login(
              surface,
              kernel,
              LoginError::Request("Quick Connect returned no session.".to_owned()),
            );
            Task::none()
          }
        },
        Err(error) => {
          fail_login(surface, kernel, error);
          surface.flow.quick_connect = QuickConnectState::Failed;
          Task::none()
        }
      }
    }
    LoginEvent::SavedProfiles(result) => update_login(
      surface,
      kernel,
      can_login,
      LoginMessage::ProfilesLoaded {
        revision: surface.flow.profiles_revision,
        result,
      },
    ),
    LoginEvent::SavedSessionStored { session, result } => update_login(
      surface,
      kernel,
      can_login,
      LoginMessage::SavedSessionStored { session, result },
    ),
  }
}

fn complete_authentication(
  surface: &mut Surface,
  kernel: &mut Kernel,
  session: jellypilot_core::request_gate::SessionToken,
  client: Arc<JellyfinClient>,
  saved_session: SensitiveSessionPayload,
  submission: Option<PasswordSubmission>,
) -> Task<LoginMessage> {
  let identity = ConnectedIdentity::from_session(&saved_session);
  let active_profile = jellypilot_auth::SavedProfileKey::for_session(&saved_session);
  if let Some(submission) = submission {
    persist_password_submission(kernel, submission);
  }

  kernel.connection = ConnectionPhase::Connected;
  kernel.connected_identity = Some(identity);
  kernel.client = Some(client);
  kernel.active_profile = Some(active_profile);
  surface.flow.password.clear();
  surface.flow.error = None;
  surface.flow.reset_quick_connect();
  let store = kernel.auth_store.clone();

  Task::perform(
    async move { store.save_session(saved_session).await },
    move |result| LoginMessage::SavedSessionStored { session, result },
  )
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
    kernel.notice = Some(format!("Could not update remembered sign-in: {error}"));
  }
}

fn start_restore(
  surface: &mut Surface,
  kernel: &mut Kernel,
  key: jellypilot_auth::SavedProfileKey,
) -> Task<LoginMessage> {
  interrupt_quick_connect(surface, kernel);
  let session = kernel.request_gate.begin_login();
  kernel.connection = ConnectionPhase::Connecting;
  surface.flow.busy_profile = Some(key.clone());
  surface.flow.error = None;
  let store = kernel.auth_store.clone();
  Task::perform(
    async move {
      let result = async {
        let sensitive = store.load_session(key.clone()).await?;
        let candidate = JellyfinClient::for_saved_profile(&sensitive);
        candidate
          .login()
          .restore_session(&sensitive)
          .await
          .map_err(|_| LoginError::Request("Saved sign-in validation failed.".to_owned()))?;
        jellypilot_auth::SensitiveSavedSession::from_client(&candidate)
          .map(ProtectedSavedSession::new)
          .ok_or_else(|| LoginError::Request("Saved sign-in validation failed.".to_owned()))
      }
      .await;
      (key, result)
    },
    move |(key, result)| LoginMessage::RestoreFinished {
      session,
      key,
      result,
    },
  )
}

fn cancel_quick_connect(surface: &mut Surface) {
  if let Some(handle) = surface.quick_connect_task.take() {
    handle.abort();
  }
}

fn interrupt_quick_connect(surface: &mut Surface, kernel: &mut Kernel) {
  if surface.quick_connect_task.is_some()
    || !matches!(surface.flow.quick_connect, QuickConnectState::Idle)
  {
    cancel_quick_connect(surface);
    kernel.request_gate.disconnect();
    kernel.connection = ConnectionPhase::SignedOut;
    surface.flow.reset_quick_connect();
  }
}

fn fail_login(surface: &mut Surface, kernel: &mut Kernel, error: LoginError) {
  kernel.connection = ConnectionPhase::Failed;
  surface.flow.error = Some(error.to_string());
}

fn fail_password_login(surface: &mut Surface, kernel: &mut Kernel, _error: &LoginError) {
  kernel.connection = ConnectionPhase::Failed;
  surface.flow.error =
    Some("Sign-in failed. Check your server, username, and password, then try again.".to_owned());
}

fn fail_restore(surface: &mut Surface, kernel: &mut Kernel, _error: &LoginError) {
  kernel.connection = ConnectionPhase::Failed;
  surface.flow.error =
    Some("Could not restore this saved sign-in. Sign in again to refresh it.".to_owned());
}

#[cfg(test)]
mod tests {
  use std::fs;
  use std::path::PathBuf;

  use jellypilot_auth::{AuthStorageError, SavedProfileKey};
  use jellypilot_core::config::SettingsStore;
  use jellypilot_core::diagnostics::Diagnostics;
  use jellypilot_core::request_gate::RequestGate;

  use super::*;
  use crate::app::state::ArtworkHandleRetention;

  struct TestSettingsFile(PathBuf);

  impl Drop for TestSettingsFile {
    fn drop(&mut self) {
      let _ = fs::remove_file(&self.0);
    }
  }

  fn isolated_settings(name: &str) -> (SettingsStore, TestSettingsFile) {
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-settings-{}-{name}.json",
      std::process::id()
    ));
    let _ = fs::remove_file(&path);
    (
      SettingsStore::for_test(path.clone()),
      TestSettingsFile(path),
    )
  }

  fn profile_key(name: &str) -> SavedProfileKey {
    let server_url = format!("https://{name}.example.test");
    let user_id = format!("{name}-user-id");
    SavedProfileKey::for_identity(MediaServerProvider::Jellyfin, &server_url, &user_id)
  }

  fn test_fixture() -> (Surface, Kernel) {
    let settings = SettingsStore::default();
    let surface = Surface {
      flow: LoginState::from_settings(settings.snapshot()),
      quick_connect_task: None,
    };
    let kernel = Kernel {
      settings,
      diagnostics: Diagnostics::default(),
      auth_store: AuthStore::default(),
      request_gate: RequestGate::default(),
      client: None,
      connection: ConnectionPhase::SignedOut,
      connected_identity: None,
      active_profile: None,
      notice: None,
      active_toast: None,
      next_toast_id: 0,
      tray: None,
      artwork_adapter: Arc::new(jellypilot_media_server::artwork::ArtworkAdapter::new()),
      artwork_binder: Default::default(),
      artwork_handles: ArtworkHandleRetention::default(),
    };
    (surface, kernel)
  }

  fn auto_login_gate_results(auto_login: bool, has_profiles: bool) -> (bool, bool) {
    let (mut surface, mut kernel) = test_fixture();
    let (settings, _settings_file) = isolated_settings("auto-login-gate");
    kernel.settings = settings;
    if !auto_login {
      kernel.settings.set_auto_login(false).unwrap();
    }
    let first = should_auto_login(&mut surface.flow, &kernel, has_profiles);
    let second = should_auto_login(&mut surface.flow, &kernel, true);
    (first, second)
  }

  #[test]
  fn auto_login_boot_gate_fires_once_with_profiles_when_enabled() {
    assert_eq!(auto_login_gate_results(true, true), (true, false));
  }

  #[test]
  fn auto_login_boot_gate_does_not_fire_when_disabled_or_profiles_are_empty() {
    assert_eq!(auto_login_gate_results(false, true), (false, false));
    assert_eq!(auto_login_gate_results(true, false), (false, false));
  }

  #[test]
  fn auto_login_boot_gate_does_not_fire_in_smoke_mode() {
    let (mut surface, kernel) = test_fixture();
    surface.flow.auto_login_attempted = true;

    assert!(!should_auto_login(&mut surface.flow, &kernel, true));
  }

  #[test]
  fn invalid_server_url_is_rejected_before_a_login_token_is_created() {
    let (mut surface, mut kernel) = test_fixture();
    surface.flow.server_url = "not a server".to_owned();
    let session_before = kernel.request_gate.current_session();

    drop(update(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::QuickConnectSubmitted,
    ));

    assert_eq!(kernel.request_gate.current_session(), session_before);
    assert_eq!(
      surface.flow.error.as_deref(),
      Some("Enter a valid Jellyfin server URL.")
    );
  }

  #[test]
  fn candidate_authentication_uses_a_private_request_generation() {
    let (_, kernel) = test_fixture();
    let kernel_session = kernel.request_gate.current_session();
    let mut candidate = CandidateSurface::new(kernel.settings.snapshot(), 1);
    candidate.flow.server_url = "https://candidate.example.test".to_owned();
    let candidate_session = candidate.request_gate.current_session();

    drop(update_candidate(
      &mut candidate,
      CandidateMessage::QuickConnectSubmitted,
    ));

    assert_eq!(kernel.request_gate.current_session(), kernel_session);
    assert_ne!(candidate.request_gate.current_session(), candidate_session);
    assert_eq!(candidate.flow.quick_connect, QuickConnectState::Requesting);
    candidate.cancel();
  }

  #[test]
  fn candidate_server_address_is_locked_while_quick_connect_is_busy() {
    let (_, kernel) = test_fixture();
    let mut candidate = CandidateSurface::new(kernel.settings.snapshot(), 1);
    candidate.flow.server_url = "https://candidate.example.test".to_owned();
    candidate.flow.quick_connect = QuickConnectState::Waiting("ABC123".to_owned());

    drop(update_candidate(
      &mut candidate,
      CandidateMessage::ServerUrlChanged("https://other.example.test".to_owned()),
    ));

    assert_eq!(candidate.flow.server_url, "https://candidate.example.test");
  }

  #[test]
  fn cancelling_candidate_quick_connect_ignores_late_events() {
    let (_, kernel) = test_fixture();
    let mut candidate = CandidateSurface::new(kernel.settings.snapshot(), 1);
    candidate.flow.server_url = "https://candidate.example.test".to_owned();
    drop(update_candidate(
      &mut candidate,
      CandidateMessage::QuickConnectSubmitted,
    ));
    let cancelled_session = candidate.request_gate.current_session();

    drop(update_candidate(
      &mut candidate,
      CandidateMessage::QuickConnectCancelled,
    ));
    drop(update_candidate(
      &mut candidate,
      CandidateMessage::WorkflowEvent {
        instance: 1,
        event: LoginEvent::QuickConnectCode {
          session: cancelled_session,
          code: "LATE12".to_owned(),
        },
      },
    ));

    assert_eq!(candidate.flow.quick_connect, QuickConnectState::Idle);
    assert!(!candidate.busy());
  }

  #[test]
  fn reopened_candidate_rejects_a_prior_instances_password_completion() {
    let (_, kernel) = test_fixture();
    let mut prior = CandidateSurface::new(kernel.settings.snapshot(), 1);
    prior.flow.server_url = "https://prior.example.test".to_owned();
    prior.flow.username = "prior-user".to_owned();
    prior.flow.password = Zeroizing::new("prior-password".to_owned());
    drop(update_candidate(
      &mut prior,
      CandidateMessage::PasswordSubmitted,
    ));
    let prior_session = prior.request_gate.current_session();
    prior.cancel();

    let mut reopened = CandidateSurface::new(kernel.settings.snapshot(), 2);
    reopened.flow.server_url = "https://reopened.example.test".to_owned();
    reopened.flow.username = "reopened-user".to_owned();
    reopened.flow.password = Zeroizing::new("reopened-password".to_owned());
    drop(update_candidate(
      &mut reopened,
      CandidateMessage::PasswordSubmitted,
    ));
    assert_eq!(reopened.request_gate.current_session(), prior_session);

    let stale_session = jellypilot_media_server::SavedSession {
      provider: MediaServerProvider::Jellyfin,
      server_url: "https://prior.example.test".to_owned(),
      access_token: "prior-token".to_owned(),
      user_id: "prior-user-id".to_owned(),
      user_name: "prior-user".to_owned(),
      server_name: Some("Prior server".to_owned()),
      device_id: Some("prior-device".to_owned()),
    };
    let stale_client = Arc::new(JellyfinClient::new());
    stale_client.login().adopt_validated_session(&stale_session);
    let update = update_candidate(
      &mut reopened,
      CandidateMessage::PasswordFinished {
        instance: 1,
        session: prior_session,
        client: stale_client,
        result: Ok(()),
        submission: PasswordSubmission {
          remember: false,
          prefill: LoginPrefill::new(
            "https://prior.example.test".to_owned(),
            "prior-user".to_owned(),
          ),
          provider: MediaServerProvider::Jellyfin,
        },
      },
    );

    assert!(update.completion.is_none());
    assert!(reopened.password_busy);
    assert_eq!(reopened.flow.username, "reopened-user");
    reopened.cancel();
  }

  #[test]
  fn quick_connect_cancel_and_retry_reset_display_state_and_replace_request() {
    let (mut surface, mut kernel) = test_fixture();
    surface.flow.server_url = "https://media.example.test".to_owned();
    drop(update(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::QuickConnectSubmitted,
    ));
    let first_session = kernel.request_gate.current_session();
    surface.flow.quick_connect = QuickConnectState::Waiting("ABC123".to_owned());

    drop(update(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::QuickConnectCancelled,
    ));
    assert_eq!(surface.flow.quick_connect, QuickConnectState::Idle);

    drop(update(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::QuickConnectSubmitted,
    ));
    assert_eq!(surface.flow.quick_connect, QuickConnectState::Requesting);
    assert_ne!(kernel.request_gate.current_session(), first_session);
  }

  #[test]
  fn remembered_prefill_can_be_applied_and_cleared_without_display_state() {
    let (mut surface, _) = test_fixture();
    surface.flow.apply_prefill(Some(LoginPrefill::new(
      "https://media.example.test".to_owned(),
      "ada".to_owned(),
    )));
    assert_eq!(surface.flow.username, "ada");

    surface.flow.apply_prefill(None);
    assert!(surface.flow.server_url.is_empty());
    assert!(surface.flow.username.is_empty());
    assert!(!surface.flow.remember);
  }

  #[test]
  fn selecting_emby_forces_password_and_hides_quick_connect_state() {
    let (mut surface, mut kernel) = test_fixture();
    surface.flow.method = LoginMethod::QuickConnect;
    surface.flow.quick_connect = QuickConnectState::Waiting("ABC123".to_owned());

    drop(update(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::ProviderSelected(MediaServerProvider::Emby),
    ));

    assert_eq!(surface.flow.method, LoginMethod::Password);
    assert_eq!(surface.flow.quick_connect, QuickConnectState::Idle);
  }

  #[test]
  fn stale_quick_connect_completion_does_not_clear_retry_abort_handle() {
    let (mut surface, mut kernel) = test_fixture();
    surface.flow.server_url = "https://media.example.test".to_owned();
    drop(update(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::QuickConnectSubmitted,
    ));
    let stale_session = kernel.request_gate.current_session();
    drop(update(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::QuickConnectCancelled,
    ));
    drop(update(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::QuickConnectSubmitted,
    ));

    drop(handle_workflow_event(
      &mut surface,
      &mut kernel,
      true,
      LoginEvent::Login {
        session: stale_session,
        client: Arc::new(JellyfinClient::new()),
        result: Err(LoginError::Request("stale failure".to_owned())),
      },
    ));

    assert!(surface.quick_connect_task.is_some());
    assert!(kernel.connection == ConnectionPhase::Connecting);
    assert_eq!(surface.flow.quick_connect, QuickConnectState::Requesting);
  }

  #[test]
  fn stale_profile_load_is_rejected_after_session_storage_completes() {
    let (mut surface, mut kernel) = test_fixture();
    let session = kernel.request_gate.current_session();
    let key = profile_key("new");

    drop(update(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::SavedSessionStored {
        session,
        result: Ok((key.clone(), Vec::new())),
      },
    ));
    drop(update(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::ProfilesLoaded {
        revision: 0,
        result: Err(AuthStorageError::Corrupt),
      },
    ));

    assert_eq!(surface.flow.profiles_revision, 1);
    assert_eq!(kernel.active_profile.as_ref(), Some(&key));
    assert!(surface.flow.error.is_none());
    assert!(!surface.flow.profiles_loading);
  }

  #[test]
  fn stale_restore_completion_does_not_clear_new_restore_busy_key() {
    let (mut surface, mut kernel) = test_fixture();
    let first_key = profile_key("first");
    let second_key = profile_key("second");
    drop(start_restore(&mut surface, &mut kernel, first_key.clone()));
    let first_session = kernel.request_gate.current_session();
    drop(start_restore(&mut surface, &mut kernel, second_key.clone()));
    let second_session = kernel.request_gate.current_session();

    drop(update(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::RestoreFinished {
        session: first_session,
        key: first_key,
        result: Err(LoginError::Request("stale failure".to_owned())),
      },
    ));

    assert_eq!(kernel.request_gate.current_session(), second_session);
    assert_eq!(surface.flow.busy_profile.as_ref(), Some(&second_key));
    assert!(kernel.connection == ConnectionPhase::Connecting);
    assert!(surface.flow.error.is_none());
  }

  #[test]
  fn starting_restore_fully_interrupts_quick_connect_state() {
    let (mut surface, mut kernel) = test_fixture();
    surface.flow.server_url = "https://media.example.test".to_owned();
    drop(start_quick_connect(&mut surface, &mut kernel));
    surface.flow.quick_connect = QuickConnectState::Waiting("ABC123".to_owned());
    let quick_connect_session = kernel.request_gate.current_session();
    let key = profile_key("restore");

    drop(start_restore(&mut surface, &mut kernel, key.clone()));

    assert_ne!(kernel.request_gate.current_session(), quick_connect_session);
    assert!(surface.quick_connect_task.is_none());
    assert_eq!(surface.flow.quick_connect, QuickConnectState::Idle);
    assert_eq!(surface.flow.busy_profile.as_ref(), Some(&key));
  }

  #[test]
  fn login_submit_handlers_reject_requests_while_connecting() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.connection = ConnectionPhase::Connecting;
    surface.flow.server_url = "https://media.example.test".to_owned();
    surface.flow.username = "ada".to_owned();
    surface.flow.password = Zeroizing::new("secret".to_owned());
    let session = kernel.request_gate.current_session();

    drop(update(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::QuickConnectSubmitted,
    ));
    drop(update(
      &mut surface,
      &mut kernel,
      true,
      LoginMessage::PasswordSubmitted,
    ));

    assert_eq!(kernel.request_gate.current_session(), session);
    assert_eq!(surface.flow.password.as_str(), "secret");
    assert_eq!(surface.flow.quick_connect, QuickConnectState::Idle);
  }

  #[test]
  fn password_completion_persists_submitted_snapshot_after_form_edits() {
    let (mut surface, mut kernel) = test_fixture();
    let (settings, _settings_file) = isolated_settings("password-snapshot");
    kernel.settings = settings;
    surface.flow.remember = true;
    surface.flow.provider = MediaServerProvider::Jellyfin;
    let submission = password_submission(
      &surface,
      "https://submitted.example.test".to_owned(),
      "submitted-user".to_owned(),
    );

    surface.flow.server_url = "https://edited.example.test".to_owned();
    surface.flow.username = "edited-user".to_owned();
    surface.flow.remember = false;
    surface.flow.provider = MediaServerProvider::Emby;

    persist_password_submission(&mut kernel, submission);

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
  fn password_and_restore_failures_use_fixed_user_messages() {
    let (mut password_surface, mut password_kernel) = test_fixture();
    let password_session = password_kernel.request_gate.begin_login();
    password_kernel.connection = ConnectionPhase::Connecting;
    let submission = password_submission(
      &password_surface,
      "https://media.example.test".to_owned(),
      "ada".to_owned(),
    );
    drop(update(
      &mut password_surface,
      &mut password_kernel,
      true,
      LoginMessage::PasswordFinished {
        session: password_session,
        client: Arc::new(JellyfinClient::new()),
        result: Err(LoginError::Request(
          "response included password=secret".to_owned(),
        )),
        submission,
      },
    ));

    let (mut restore_surface, mut restore_kernel) = test_fixture();
    let key = profile_key("restore-error");
    let restore_session = restore_kernel.request_gate.begin_login();
    restore_kernel.connection = ConnectionPhase::Connecting;
    restore_surface.flow.busy_profile = Some(key.clone());
    drop(update(
      &mut restore_surface,
      &mut restore_kernel,
      true,
      LoginMessage::RestoreFinished {
        session: restore_session,
        key,
        result: Err(LoginError::Request(
          "response included access_token=secret".to_owned(),
        )),
      },
    ));

    assert_eq!(
      password_surface.flow.error.as_deref(),
      Some("Sign-in failed. Check your server, username, and password, then try again.")
    );
    assert_eq!(
      restore_surface.flow.error.as_deref(),
      Some("Could not restore this saved sign-in. Sign in again to refresh it.")
    );
  }

  #[test]
  fn login_is_gated_by_playback_cleanup_with_fixed_copy() {
    let (mut surface, _) = test_fixture();

    assert!(!playback_allows_login(&mut surface, false));
    assert_eq!(
      surface.flow.error.as_deref(),
      Some("Finishing external playback shutdown. Try again in a moment.")
    );
  }
}
