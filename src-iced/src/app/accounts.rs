//! Saved-account management and the single-active-connection handoff.
//!
//! Candidate authentication is isolated from [`Kernel`]. This reducer owns
//! the candidate until playback and remote teardown have both settled, then
//! swaps the active client synchronously and tells the top-level router to
//! reset the account-bound presentation surfaces.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use iced::Task;
use jellypilot_auth::login::{
  validate_saved_profile, ConnectionPhase, LoginError, ValidatedProfileCandidate,
};
use jellypilot_auth::{AuthStorageError, SavedProfileKey};
use jellypilot_core::watchlist::ProfileScope;
use jellypilot_media_server::MediaServerProvider;
use jellypilot_session::RemoteControlState;

use super::kernel::Kernel;
use super::login::{self, CandidateMessage, CandidateSurface, CandidateUpdate};
use super::message::PasswordSubmission;
use super::personal_lists;
use super::state::{ConnectedIdentity, LoginState, State};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CopyStatus {
  Idle,
  Copied,
  Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfirmationKind {
  SwitchAccount,
  ConnectAndSwitch,
  Disconnect,
  SignOut,
}

pub struct ConfirmationView<'a> {
  pub kind: ConfirmationKind,
  pub account: Option<&'a str>,
  pub delete_watchlist: bool,
  pub active_profile: bool,
}

pub struct CurrentAccountView<'a> {
  pub provider: MediaServerProvider,
  pub user_name: &'a str,
  pub server_name: Option<&'a str>,
  pub server_url: &'a str,
}

pub struct AccountView<'a> {
  pub current: Option<CurrentAccountView<'a>>,
  pub profiles: &'a [jellypilot_auth::SavedProfileSummary],
  pub active_key: Option<&'a SavedProfileKey>,
  pub busy_key: Option<&'a SavedProfileKey>,
  pub loading: bool,
  pub management_open: bool,
  pub confirmation: Option<ConfirmationView<'a>>,
  pub error: Option<&'a str>,
  pub auto_login: bool,
  pub remote_control: RemoteControlState,
  pub copy_status: CopyStatus,
  pub add_account: Option<&'a CandidateSurface>,
  pub handoff_blocking: bool,
  pub can_retry_handoff_cleanup: bool,
  pub can_retry_watchlist_cleanup: bool,
}

pub struct RuntimeFacts {
  pub playback_active: bool,
  pub quit_requested: bool,
}

pub struct Update {
  pub task: Task<Message>,
  pub effect: Option<Effect>,
}

impl Update {
  fn none() -> Self {
    Self {
      task: Task::none(),
      effect: None,
    }
  }

  fn task(task: Task<Message>) -> Self {
    Self { task, effect: None }
  }

  fn effect(effect: Effect) -> Self {
    Self {
      task: Task::none(),
      effect: Some(effect),
    }
  }

  fn task_and_effect(task: Task<Message>, effect: Effect) -> Self {
    Self {
      task,
      effect: Some(effect),
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Effect {
  BeginHandoff { generation: u64 },
  Activated,
  Disconnected,
}

#[derive(Clone)]
pub enum Message {
  CopyServerAddress,
  ClipboardVerified {
    generation: u64,
    matched: bool,
  },
  AddAccount,
  CloseAddAccount,
  AddLogin(CandidateMessage),
  SwitchProfile(SavedProfileKey),
  Disconnect,
  AskSignOut(SavedProfileKey),
  ToggleManagement,
  Confirm,
  CancelConfirmation,
  ToggleDeleteWatchlist,
  RetryHandoffCleanup,
  RetryWatchlistCleanup,
  DismissError,
  CandidateValidated {
    generation: u64,
    key: SavedProfileKey,
    result: Result<ProtectedCandidate, LoginError>,
  },
  CredentialsRemoved {
    generation: u64,
    result: Result<Vec<jellypilot_auth::SavedProfileSummary>, AuthStorageError>,
  },
  WatchlistRemoved {
    generation: u64,
    result: Result<usize, String>,
  },
  RemoteHandoffSettled {
    generation: u64,
  },
  PlaybackHandoffSettled {
    generation: u64,
    result: Result<(), String>,
  },
  SessionStored {
    candidate_key: SavedProfileKey,
    result: Result<(SavedProfileKey, Vec<jellypilot_auth::SavedProfileSummary>), AuthStorageError>,
  },
  ActivationRecorded {
    candidate_key: SavedProfileKey,
    result: Result<(), AuthStorageError>,
  },
}

pub(crate) struct ProtectedCandidate(Arc<Mutex<Option<ValidatedProfileCandidate>>>);

impl ProtectedCandidate {
  fn new(candidate: ValidatedProfileCandidate) -> Self {
    Self(Arc::new(Mutex::new(Some(candidate))))
  }

  fn take(&self) -> Option<ValidatedProfileCandidate> {
    self
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .take()
  }
}

impl Clone for ProtectedCandidate {
  fn clone(&self) -> Self {
    Self(Arc::clone(&self.0))
  }
}

impl std::fmt::Debug for ProtectedCandidate {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.write_str("ProtectedCandidate([redacted])")
  }
}

enum PendingConfirmation {
  SwitchBeforeValidation {
    key: SavedProfileKey,
    account: String,
  },
  NewAuthentication {
    method: NewAuthenticationMethod,
  },
  CandidateHandoff {
    generation: u64,
    kind: ConfirmationKind,
    account: String,
  },
  Disconnect,
  SignOut {
    key: SavedProfileKey,
    scope: ProfileScope,
    account: String,
    active: bool,
    delete_watchlist: bool,
  },
}

#[derive(Clone, Copy)]
enum NewAuthenticationMethod {
  QuickConnect,
  Password,
}

enum Operation {
  Idle,
  AuthenticatingNew {
    generation: u64,
    playback_confirmed: bool,
  },
  ValidatingSaved {
    generation: u64,
    key: SavedProfileKey,
    playback_confirmed: bool,
  },
  CandidateReady {
    generation: u64,
    handoff: HandoffKind,
  },
  RemovingCredentials {
    generation: u64,
    scope: ProfileScope,
    active: bool,
    delete_watchlist: bool,
  },
  Handoff {
    generation: u64,
    kind: HandoffKind,
    remote_done: bool,
    playback_result: Option<Result<(), String>>,
  },
}

impl Operation {
  const fn generation(&self) -> Option<u64> {
    match self {
      Self::Idle => None,
      Self::AuthenticatingNew { generation, .. }
      | Self::ValidatingSaved { generation, .. }
      | Self::CandidateReady { generation, .. }
      | Self::RemovingCredentials { generation, .. }
      | Self::Handoff { generation, .. } => Some(*generation),
    }
  }

  const fn is_handoff(&self) -> bool {
    matches!(self, Self::Handoff { .. })
  }
}

enum HandoffKind {
  Activate {
    candidate: Box<ValidatedProfileCandidate>,
    save_session: bool,
    submission: Option<PasswordSubmission>,
  },
  Disconnect,
  SignOut,
}

pub struct Surface {
  pub management_open: bool,
  pub error: Option<String>,
  pub copy_status: CopyStatus,
  pub add_account: Option<CandidateSurface>,
  confirmation: Option<PendingConfirmation>,
  operation: Operation,
  next_generation: u64,
  next_candidate_instance: u64,
  next_copy_generation: u64,
  copy_generation: Option<u64>,
  busy_profile: Option<SavedProfileKey>,
  current_candidate_key: Option<SavedProfileKey>,
  next_watchlist_generation: u64,
  watchlist_in_flight: HashMap<u64, ProfileScope>,
  failed_watchlist_cleanup: Vec<ProfileScope>,
}

impl Surface {
  pub fn new() -> Self {
    Self {
      management_open: false,
      error: None,
      copy_status: CopyStatus::Idle,
      add_account: None,
      confirmation: None,
      operation: Operation::Idle,
      next_generation: 0,
      next_candidate_instance: 0,
      next_copy_generation: 0,
      copy_generation: None,
      busy_profile: None,
      current_candidate_key: None,
      next_watchlist_generation: 0,
      watchlist_in_flight: HashMap::new(),
      failed_watchlist_cleanup: Vec::new(),
    }
  }

  fn begin_operation(&mut self) -> u64 {
    self.next_generation = self.next_generation.wrapping_add(1);
    self.next_generation
  }

  fn operation_busy(&self) -> bool {
    !matches!(self.operation, Operation::Idle)
  }

  fn action_blocked(&self) -> bool {
    self.operation_busy() || self.confirmation.is_some()
  }
}

impl Default for Surface {
  fn default() -> Self {
    Self::new()
  }
}

pub fn view(state: &State) -> AccountView<'_> {
  let surface = &state.accounts;
  AccountView {
    current: state
      .kernel
      .connected_identity
      .as_ref()
      .map(current_account_view),
    profiles: &state.login.flow.profiles,
    active_key: state.kernel.active_profile.as_ref(),
    busy_key: surface.busy_profile.as_ref(),
    loading: state.login.flow.profiles_loading,
    management_open: surface.management_open,
    confirmation: confirmation_view(surface.confirmation.as_ref()),
    error: surface.error.as_deref(),
    auto_login: state.kernel.settings.snapshot().auto_login(),
    remote_control: state.playback.remote_control_state,
    copy_status: surface.copy_status,
    add_account: surface.add_account.as_ref(),
    handoff_blocking: surface.operation.is_handoff(),
    can_retry_handoff_cleanup: can_retry_handoff_cleanup(surface),
    can_retry_watchlist_cleanup: !surface.failed_watchlist_cleanup.is_empty(),
  }
}

fn can_retry_handoff_cleanup(surface: &Surface) -> bool {
  matches!(
    surface.operation,
    Operation::Handoff {
      playback_result: Some(Err(_)),
      ..
    }
  )
}

fn current_account_view(identity: &ConnectedIdentity) -> CurrentAccountView<'_> {
  CurrentAccountView {
    provider: identity.provider,
    user_name: &identity.user_name,
    server_name: identity.server_name.as_deref(),
    server_url: &identity.server_url,
  }
}

fn confirmation_view(confirmation: Option<&PendingConfirmation>) -> Option<ConfirmationView<'_>> {
  confirmation.map(|confirmation| match confirmation {
    PendingConfirmation::SwitchBeforeValidation { account, .. } => ConfirmationView {
      kind: ConfirmationKind::SwitchAccount,
      account: Some(account),
      delete_watchlist: false,
      active_profile: false,
    },
    PendingConfirmation::NewAuthentication { .. } => ConfirmationView {
      kind: ConfirmationKind::ConnectAndSwitch,
      account: None,
      delete_watchlist: false,
      active_profile: false,
    },
    PendingConfirmation::CandidateHandoff { kind, account, .. } => ConfirmationView {
      kind: *kind,
      account: Some(account),
      delete_watchlist: false,
      active_profile: false,
    },
    PendingConfirmation::Disconnect => ConfirmationView {
      kind: ConfirmationKind::Disconnect,
      account: None,
      delete_watchlist: false,
      active_profile: true,
    },
    PendingConfirmation::SignOut {
      account,
      active,
      delete_watchlist,
      ..
    } => ConfirmationView {
      kind: ConfirmationKind::SignOut,
      account: Some(account),
      delete_watchlist: *delete_watchlist,
      active_profile: *active,
    },
  })
}

pub fn handoff_generation(surface: &Surface) -> Option<u64> {
  match surface.operation {
    Operation::Handoff { generation, .. } => Some(generation),
    _ => None,
  }
}

/// Whether account teardown must reject new mutations against the old scope.
///
/// Candidate validation is intentionally excluded because the current account
/// remains fully active until handoff starts. Inactive-account removal is also
/// excluded because it cannot affect the current content scope.
pub fn content_mutations_blocked(surface: &Surface) -> bool {
  matches!(
    surface.operation,
    Operation::RemovingCredentials { active: true, .. } | Operation::Handoff { .. }
  )
}

pub fn blocking_modal(surface: &Surface) -> bool {
  surface.confirmation.is_some() || surface.add_account.is_some()
}

/// Closes transient popover presentation without cancelling authentication or
/// an irreversible handoff already in progress.
pub fn hide(surface: &mut Surface) {
  surface.management_open = false;
  surface.copy_status = CopyStatus::Idle;
  surface.copy_generation = None;
}

pub fn update(
  surface: &mut Surface,
  login_flow: &mut LoginState,
  kernel: &mut Kernel,
  watchlist: &personal_lists::Runtime,
  facts: RuntimeFacts,
  message: Message,
) -> Update {
  match message {
    Message::CopyServerAddress => copy_server_address(surface, kernel),
    Message::ClipboardVerified {
      generation,
      matched,
    } => finish_clipboard_verification(surface, generation, matched),
    Message::AddAccount => {
      if !surface.action_blocked() && surface.add_account.is_none() {
        surface.next_candidate_instance = surface.next_candidate_instance.wrapping_add(1);
        surface.add_account = Some(CandidateSurface::new(
          kernel.settings.snapshot(),
          surface.next_candidate_instance,
        ));
        surface.error = None;
      }
      Update::none()
    }
    Message::CloseAddAccount => close_add_account(surface),
    Message::AddLogin(message) => update_add_login(
      surface,
      kernel,
      facts.playback_active,
      facts.quit_requested,
      message,
    ),
    Message::SwitchProfile(key) => {
      request_switch(surface, login_flow, kernel, facts.playback_active, key)
    }
    Message::Disconnect => request_disconnect(surface, facts.playback_active),
    Message::AskSignOut(key) => request_sign_out(surface, login_flow, kernel, key),
    Message::ToggleManagement => {
      surface.management_open = !surface.management_open;
      Update::none()
    }
    Message::Confirm => confirm(surface, kernel, watchlist),
    Message::CancelConfirmation => cancel_confirmation(surface),
    Message::ToggleDeleteWatchlist => {
      if let Some(PendingConfirmation::SignOut {
        delete_watchlist, ..
      }) = &mut surface.confirmation
      {
        *delete_watchlist = !*delete_watchlist;
      }
      Update::none()
    }
    Message::RetryHandoffCleanup => retry_handoff(surface),
    Message::RetryWatchlistCleanup => retry_watchlist_cleanup(surface, watchlist),
    Message::DismissError => {
      surface.error = None;
      Update::none()
    }
    Message::CandidateValidated {
      generation,
      key,
      result,
    } => finish_saved_validation(
      surface,
      facts.playback_active,
      facts.quit_requested,
      generation,
      key,
      result,
    ),
    Message::CredentialsRemoved { generation, result } => finish_credentials_removal(
      surface,
      login_flow,
      watchlist,
      generation,
      result,
      facts.quit_requested,
    ),
    Message::WatchlistRemoved { generation, result } => {
      finish_watchlist_removal(surface, generation, result)
    }
    Message::RemoteHandoffSettled { generation } => settle_remote_handoff(
      surface,
      login_flow,
      kernel,
      generation,
      facts.quit_requested,
    ),
    Message::PlaybackHandoffSettled { generation, result } => settle_playback_handoff(
      surface,
      login_flow,
      kernel,
      generation,
      result,
      facts.quit_requested,
    ),
    Message::SessionStored {
      candidate_key,
      result,
    } => finish_session_storage(surface, login_flow, kernel, candidate_key, result),
    Message::ActivationRecorded {
      candidate_key,
      result,
    } => finish_activation_record(surface, kernel, candidate_key, result),
  }
}

fn copy_server_address(surface: &mut Surface, kernel: &Kernel) -> Update {
  let Some(identity) = &kernel.connected_identity else {
    surface.error = Some("No connected server address is available to copy.".to_owned());
    return Update::none();
  };
  surface.next_copy_generation = surface.next_copy_generation.wrapping_add(1);
  let generation = surface.next_copy_generation;
  surface.copy_generation = Some(generation);
  surface.copy_status = CopyStatus::Idle;
  let contents = identity.server_url.clone();
  let expected = contents.clone();
  let verification = iced::clipboard::read().map(move |actual| Message::ClipboardVerified {
    generation,
    matched: actual.as_deref() == Some(expected.as_str()),
  });
  Update::task(iced::clipboard::write(contents).chain(verification))
}

fn finish_clipboard_verification(surface: &mut Surface, generation: u64, matched: bool) -> Update {
  if surface.copy_generation != Some(generation) {
    return Update::none();
  }
  surface.copy_generation = None;
  surface.copy_status = if matched {
    CopyStatus::Copied
  } else {
    CopyStatus::Failed
  };
  Update::none()
}

fn close_add_account(surface: &mut Surface) -> Update {
  if surface.operation.is_handoff() {
    surface.add_account = None;
    return Update::none();
  }
  if surface.add_account.is_some()
    && matches!(
      surface.operation,
      Operation::AuthenticatingNew { .. } | Operation::CandidateReady { .. }
    )
  {
    if let Some(add_account) = &mut surface.add_account {
      add_account.cancel();
    }
    surface.next_generation = surface.next_generation.wrapping_add(1);
    surface.operation = Operation::Idle;
    surface.confirmation = None;
  }
  surface.add_account = None;
  Update::none()
}

fn update_add_login(
  surface: &mut Surface,
  kernel: &mut Kernel,
  playback_active: bool,
  quit_requested: bool,
  message: CandidateMessage,
) -> Update {
  if quit_requested {
    if let Some(add_account) = &mut surface.add_account {
      add_account.cancel();
    }
    surface.add_account = None;
    surface.confirmation = None;
    surface.operation = Operation::Idle;
    surface.busy_profile = None;
    return Update::none();
  }
  if matches!(message, CandidateMessage::QuickConnectCancelled) {
    if !matches!(
      surface.operation,
      Operation::Idle | Operation::AuthenticatingNew { .. }
    ) {
      return Update::none();
    }
    if let Some(add_account) = &mut surface.add_account {
      let CandidateUpdate { task, .. } = login::update_candidate(add_account, message);
      surface.next_generation = surface.next_generation.wrapping_add(1);
      surface.operation = Operation::Idle;
      surface.confirmation = None;
      return Update::task(task.map(Message::AddLogin));
    }
    return Update::none();
  }

  let method = match message {
    CandidateMessage::QuickConnectSubmitted => Some(NewAuthenticationMethod::QuickConnect),
    CandidateMessage::PasswordSubmitted => Some(NewAuthenticationMethod::Password),
    _ => None,
  };
  if let Some(method) = method {
    if surface.action_blocked() {
      return Update::none();
    }
    if playback_active {
      surface.confirmation = Some(PendingConfirmation::NewAuthentication { method });
      return Update::none();
    }
    return start_new_authentication(surface, kernel, method, false);
  }

  let active_authentication = match surface.operation {
    Operation::AuthenticatingNew {
      generation,
      playback_confirmed,
    } => Some((generation, playback_confirmed)),
    Operation::Idle => None,
    _ => return Update::none(),
  };
  let Some(add_account) = &mut surface.add_account else {
    return Update::none();
  };
  let CandidateUpdate { task, completion } = login::update_candidate(add_account, message);
  if let (Some((generation, _)), Some(completion)) = (active_authentication, completion) {
    let follow_up = finish_new_authentication(surface, playback_active, generation, completion);
    return Update {
      task: Task::batch([task.map(Message::AddLogin), follow_up.task]),
      effect: follow_up.effect,
    };
  }
  if active_authentication.is_some() && !add_account.busy() {
    surface.operation = Operation::Idle;
  }
  Update::task(task.map(Message::AddLogin))
}

fn start_new_authentication(
  surface: &mut Surface,
  _kernel: &Kernel,
  method: NewAuthenticationMethod,
  playback_confirmed: bool,
) -> Update {
  let generation = surface.begin_operation();
  surface.operation = Operation::AuthenticatingNew {
    generation,
    playback_confirmed,
  };
  surface.error = None;
  let message = match method {
    NewAuthenticationMethod::QuickConnect => CandidateMessage::QuickConnectSubmitted,
    NewAuthenticationMethod::Password => CandidateMessage::PasswordSubmitted,
  };
  let Some(add_account) = &mut surface.add_account else {
    surface.operation = Operation::Idle;
    return Update::none();
  };
  let CandidateUpdate { task, completion } = login::update_candidate(add_account, message);
  if let Some(completion) = completion {
    return finish_new_authentication(surface, false, generation, completion);
  }
  // Synchronous input validation leaves the form idle and reports its own
  // error; do not leave the account coordinator permanently busy.
  if !add_account.busy() {
    surface.operation = Operation::Idle;
  }
  Update::task(task.map(Message::AddLogin))
}

fn finish_new_authentication(
  surface: &mut Surface,
  playback_active: bool,
  generation: u64,
  completion: login::CandidateCompletion,
) -> Update {
  let Operation::AuthenticatingNew {
    generation: active_generation,
    playback_confirmed,
  } = surface.operation
  else {
    return Update::none();
  };
  if generation != active_generation {
    return Update::none();
  }
  let account = completion.candidate.account_title();
  let handoff = HandoffKind::Activate {
    candidate: Box::new(completion.candidate),
    save_session: true,
    submission: completion.submission,
  };
  if playback_active && !playback_confirmed {
    surface.operation = Operation::CandidateReady {
      generation,
      handoff,
    };
    surface.confirmation = Some(PendingConfirmation::CandidateHandoff {
      generation,
      kind: ConfirmationKind::ConnectAndSwitch,
      account,
    });
    return Update::none();
  }
  begin_handoff(surface, generation, handoff)
}

fn request_switch(
  surface: &mut Surface,
  login_flow: &LoginState,
  kernel: &Kernel,
  playback_active: bool,
  key: SavedProfileKey,
) -> Update {
  if surface.action_blocked() || active_account_key(surface, kernel) == Some(&key) {
    return Update::none();
  }
  let Some(profile) = login_flow
    .profiles
    .iter()
    .find(|profile| profile.key() == &key)
  else {
    surface.error = Some("This saved sign-in is no longer available.".to_owned());
    return Update::none();
  };
  surface.error = None;
  if playback_active {
    surface.confirmation = Some(PendingConfirmation::SwitchBeforeValidation {
      key,
      account: profile.title(),
    });
    return Update::none();
  }
  start_saved_validation(surface, kernel, key, false)
}

fn start_saved_validation(
  surface: &mut Surface,
  kernel: &Kernel,
  key: SavedProfileKey,
  playback_confirmed: bool,
) -> Update {
  let generation = surface.begin_operation();
  surface.busy_profile = Some(key.clone());
  surface.operation = Operation::ValidatingSaved {
    generation,
    key: key.clone(),
    playback_confirmed,
  };
  surface.error = None;
  let store = kernel.auth_store.clone();
  let completion_key = key.clone();
  Update::task(Task::perform(
    async move {
      validate_saved_profile(store, key)
        .await
        .map(ProtectedCandidate::new)
    },
    move |result| Message::CandidateValidated {
      generation,
      key: completion_key,
      result,
    },
  ))
}

fn finish_saved_validation(
  surface: &mut Surface,
  playback_active: bool,
  quit_requested: bool,
  generation: u64,
  key: SavedProfileKey,
  result: Result<ProtectedCandidate, LoginError>,
) -> Update {
  let Operation::ValidatingSaved {
    generation: active_generation,
    key: active_key,
    playback_confirmed,
  } = &surface.operation
  else {
    return Update::none();
  };
  if *active_generation != generation || active_key != &key {
    return Update::none();
  }
  if quit_requested {
    surface.operation = Operation::Idle;
    surface.busy_profile = None;
    return Update::none();
  }
  let playback_confirmed = *playback_confirmed;
  let candidate = match result.and_then(|candidate| {
    candidate.take().ok_or_else(|| {
      LoginError::Request("This saved sign-in result is no longer available.".to_owned())
    })
  }) {
    Ok(candidate) => candidate,
    Err(error) => {
      surface.operation = Operation::Idle;
      surface.busy_profile = None;
      surface.error = Some(error.to_string());
      return Update::none();
    }
  };
  let account = candidate.account_title();
  let handoff = HandoffKind::Activate {
    candidate: Box::new(candidate),
    save_session: false,
    submission: None,
  };
  if playback_active && !playback_confirmed {
    surface.operation = Operation::CandidateReady {
      generation,
      handoff,
    };
    surface.confirmation = Some(PendingConfirmation::CandidateHandoff {
      generation,
      kind: ConfirmationKind::SwitchAccount,
      account,
    });
    return Update::none();
  }
  begin_handoff(surface, generation, handoff)
}

fn request_disconnect(surface: &mut Surface, playback_active: bool) -> Update {
  if surface.action_blocked() {
    return Update::none();
  }
  if playback_active {
    surface.confirmation = Some(PendingConfirmation::Disconnect);
    return Update::none();
  }
  let generation = surface.begin_operation();
  begin_handoff(surface, generation, HandoffKind::Disconnect)
}

fn request_sign_out(
  surface: &mut Surface,
  login_flow: &LoginState,
  kernel: &Kernel,
  key: SavedProfileKey,
) -> Update {
  if surface.action_blocked() {
    return Update::none();
  }
  let Some(profile) = login_flow
    .profiles
    .iter()
    .find(|profile| profile.key() == &key)
  else {
    surface.error = Some("This saved sign-in is no longer available.".to_owned());
    return Update::none();
  };
  let scope = profile.scope().clone();
  let account = profile.title();
  request_sign_out_for_profile(surface, kernel, key, scope, account)
}

fn request_sign_out_for_profile(
  surface: &mut Surface,
  kernel: &Kernel,
  key: SavedProfileKey,
  scope: ProfileScope,
  account: String,
) -> Update {
  surface.confirmation = Some(PendingConfirmation::SignOut {
    scope,
    account,
    active: active_account_key(surface, kernel) == Some(&key),
    key,
    delete_watchlist: false,
  });
  surface.error = None;
  Update::none()
}

fn active_account_key<'a>(surface: &'a Surface, kernel: &'a Kernel) -> Option<&'a SavedProfileKey> {
  surface
    .current_candidate_key
    .as_ref()
    .or(kernel.active_profile.as_ref())
}

fn confirm(surface: &mut Surface, kernel: &Kernel, watchlist: &personal_lists::Runtime) -> Update {
  let Some(confirmation) = surface.confirmation.take() else {
    return Update::none();
  };
  match confirmation {
    PendingConfirmation::SwitchBeforeValidation { key, .. } => {
      start_saved_validation(surface, kernel, key, true)
    }
    PendingConfirmation::NewAuthentication { method } => {
      start_new_authentication(surface, kernel, method, true)
    }
    PendingConfirmation::CandidateHandoff { generation, .. } => {
      let operation = std::mem::replace(&mut surface.operation, Operation::Idle);
      match operation {
        Operation::CandidateReady {
          generation: active_generation,
          handoff,
        } if active_generation == generation => begin_handoff(surface, generation, handoff),
        other => {
          surface.operation = other;
          Update::none()
        }
      }
    }
    PendingConfirmation::Disconnect => {
      let generation = surface.begin_operation();
      begin_handoff(surface, generation, HandoffKind::Disconnect)
    }
    PendingConfirmation::SignOut {
      key,
      scope,
      active,
      delete_watchlist,
      ..
    } => start_credentials_removal(
      surface,
      kernel,
      watchlist,
      key,
      scope,
      active,
      delete_watchlist,
    ),
  }
}

fn cancel_confirmation(surface: &mut Surface) -> Update {
  if let Some(PendingConfirmation::CandidateHandoff { generation, .. }) =
    surface.confirmation.take()
  {
    if surface.operation.generation() == Some(generation) {
      surface.operation = Operation::Idle;
      surface.busy_profile = None;
    }
  } else {
    surface.confirmation = None;
  }
  Update::none()
}

fn start_credentials_removal(
  surface: &mut Surface,
  kernel: &Kernel,
  _watchlist: &personal_lists::Runtime,
  key: SavedProfileKey,
  scope: ProfileScope,
  active: bool,
  delete_watchlist: bool,
) -> Update {
  let generation = surface.begin_operation();
  surface.busy_profile = Some(key.clone());
  surface.operation = Operation::RemovingCredentials {
    generation,
    scope,
    active,
    delete_watchlist,
  };
  let store = kernel.auth_store.clone();
  Update::task(Task::perform(
    async move { store.remove_profile(key).await },
    move |result| Message::CredentialsRemoved { generation, result },
  ))
}

fn finish_credentials_removal(
  surface: &mut Surface,
  login_flow: &mut LoginState,
  watchlist: &personal_lists::Runtime,
  generation: u64,
  result: Result<Vec<jellypilot_auth::SavedProfileSummary>, AuthStorageError>,
  quit_requested: bool,
) -> Update {
  if surface.operation.generation() != Some(generation) {
    return Update::none();
  }
  let operation = std::mem::replace(&mut surface.operation, Operation::Idle);
  let Operation::RemovingCredentials {
    scope,
    active,
    delete_watchlist,
    ..
  } = operation
  else {
    return Update::none();
  };
  match result {
    Err(error) => {
      surface.busy_profile = None;
      surface.error = Some(LoginError::AuthStorage(error).to_string());
      Update::none()
    }
    Ok(profiles) => {
      login_flow.profiles_revision = login_flow.profiles_revision.wrapping_add(1);
      login_flow.profiles = profiles;
      let cleanup = delete_watchlist.then(|| start_watchlist_cleanup(surface, watchlist, scope));
      if active && !quit_requested {
        let handoff = begin_handoff(surface, generation, HandoffKind::SignOut);
        if let Some(cleanup) = cleanup {
          Update::task_and_effect(cleanup, handoff.effect.expect("handoff effect"))
        } else {
          handoff
        }
      } else {
        surface.busy_profile = None;
        cleanup.map_or_else(Update::none, Update::task)
      }
    }
  }
}

fn start_watchlist_cleanup(
  surface: &mut Surface,
  watchlist: &personal_lists::Runtime,
  scope: ProfileScope,
) -> Task<Message> {
  surface.next_watchlist_generation = surface.next_watchlist_generation.wrapping_add(1);
  let generation = surface.next_watchlist_generation;
  surface
    .watchlist_in_flight
    .insert(generation, scope.clone());
  let runtime = watchlist.clone();
  Task::perform(
    async move { runtime.remove_scope(scope).await },
    move |result| Message::WatchlistRemoved { generation, result },
  )
}

fn finish_watchlist_removal(
  surface: &mut Surface,
  generation: u64,
  result: Result<usize, String>,
) -> Update {
  let Some(scope) = surface.watchlist_in_flight.remove(&generation) else {
    return Update::none();
  };
  match result {
    Ok(_) => {}
    Err(error) => {
      surface.failed_watchlist_cleanup.push(scope);
      surface.error = Some(format!(
        "The saved login was removed, but its Watchlist remains on this device: {error}"
      ));
    }
  }
  Update::none()
}

fn retry_watchlist_cleanup(surface: &mut Surface, watchlist: &personal_lists::Runtime) -> Update {
  let Some(scope) = surface.failed_watchlist_cleanup.pop() else {
    return Update::none();
  };
  surface.error = None;
  Update::task(start_watchlist_cleanup(surface, watchlist, scope))
}

fn begin_handoff(surface: &mut Surface, generation: u64, kind: HandoffKind) -> Update {
  surface.copy_generation = None;
  surface.copy_status = CopyStatus::Idle;
  surface.operation = Operation::Handoff {
    generation,
    kind,
    remote_done: false,
    playback_result: None,
  };
  surface.confirmation = None;
  surface.add_account = None;
  Update::effect(Effect::BeginHandoff { generation })
}

fn retry_handoff(surface: &mut Surface) -> Update {
  let Operation::Handoff {
    generation,
    playback_result,
    ..
  } = &mut surface.operation
  else {
    return Update::none();
  };
  if !matches!(playback_result, Some(Err(_))) {
    return Update::none();
  }
  *playback_result = None;
  surface.error = None;
  Update::effect(Effect::BeginHandoff {
    generation: *generation,
  })
}

fn settle_remote_handoff(
  surface: &mut Surface,
  login_flow: &mut LoginState,
  kernel: &mut Kernel,
  generation: u64,
  quit_requested: bool,
) -> Update {
  let Operation::Handoff {
    generation: active_generation,
    remote_done,
    ..
  } = &mut surface.operation
  else {
    return Update::none();
  };
  if *active_generation != generation {
    return Update::none();
  }
  *remote_done = true;
  finish_handoff_if_ready(surface, login_flow, kernel, quit_requested)
}

fn settle_playback_handoff(
  surface: &mut Surface,
  login_flow: &mut LoginState,
  kernel: &mut Kernel,
  generation: u64,
  result: Result<(), String>,
  quit_requested: bool,
) -> Update {
  let Operation::Handoff {
    generation: active_generation,
    kind,
    playback_result,
    ..
  } = &mut surface.operation
  else {
    return Update::none();
  };
  if *active_generation != generation {
    return Update::none();
  }
  if let Err(error) = &result {
    surface.error = Some(match kind {
      HandoffKind::Activate { .. } => format!(
        "External playback cleanup failed. The account was not changed: {error}"
      ),
      HandoffKind::Disconnect => {
        format!("External playback cleanup failed. The account remains connected: {error}")
      }
      HandoffKind::SignOut => format!(
        "The saved login was removed, but external playback cleanup failed. The current session remains connected until cleanup is retried: {error}"
      ),
    });
  }
  *playback_result = Some(result);
  finish_handoff_if_ready(surface, login_flow, kernel, quit_requested)
}

fn finish_handoff_if_ready(
  surface: &mut Surface,
  login_flow: &mut LoginState,
  kernel: &mut Kernel,
  quit_requested: bool,
) -> Update {
  let ready = matches!(
    &surface.operation,
    Operation::Handoff {
      remote_done: true,
      playback_result: Some(Ok(())),
      ..
    }
  );
  if !ready {
    return Update::none();
  }
  let operation = std::mem::replace(&mut surface.operation, Operation::Idle);
  let Operation::Handoff {
    generation, kind, ..
  } = operation
  else {
    unreachable!("handoff readiness was checked above")
  };
  if quit_requested {
    drop(kind);
    disconnect_active_client(surface, kernel);
    surface.busy_profile = None;
    return Update::effect(Effect::Disconnected);
  }
  match kind {
    HandoffKind::Activate {
      candidate,
      save_session,
      submission,
    } => activate_candidate(
      surface,
      login_flow,
      kernel,
      generation,
      *candidate,
      save_session,
      submission,
    ),
    HandoffKind::Disconnect | HandoffKind::SignOut => {
      disconnect_active_client(surface, kernel);
      surface.busy_profile = None;
      surface.error = None;
      Update::effect(Effect::Disconnected)
    }
  }
}

fn activate_candidate(
  surface: &mut Surface,
  _login_flow: &mut LoginState,
  kernel: &mut Kernel,
  _generation: u64,
  candidate: ValidatedProfileCandidate,
  save_session: bool,
  submission: Option<PasswordSubmission>,
) -> Update {
  let (key, _scope, client, saved_session) = candidate.into_parts();
  if let Some(old_client) = kernel.client.take() {
    old_client.login().disconnect();
  }
  kernel.request_gate.disconnect();
  let session = kernel.request_gate.begin_login();
  let _ = kernel.request_gate.finish_login(session);
  kernel.connected_identity = Some(ConnectedIdentity::from_session(&saved_session));
  kernel.connection = ConnectionPhase::Connected;
  kernel.client = Some(client);
  kernel.active_profile = Some(key.clone());
  surface.current_candidate_key = Some(key.clone());
  surface.busy_profile = None;
  surface.error = None;
  if let Some(submission) = submission {
    login::persist_password_submission(kernel, submission);
  }

  let store = kernel.auth_store.clone();
  let task = if save_session {
    let candidate_key = key;
    Task::perform(
      async move { store.save_session(saved_session).await },
      move |result| Message::SessionStored {
        candidate_key,
        result,
      },
    )
  } else {
    record_activation_task(store, key)
  };
  Update::task_and_effect(task, Effect::Activated)
}

fn disconnect_active_client(surface: &mut Surface, kernel: &mut Kernel) {
  if let Some(client) = kernel.client.take() {
    client.login().disconnect();
  }
  kernel.request_gate.disconnect();
  kernel.connection = ConnectionPhase::SignedOut;
  kernel.connected_identity = None;
  kernel.active_profile = None;
  surface.current_candidate_key = None;
}

fn finish_session_storage(
  surface: &mut Surface,
  login_flow: &mut LoginState,
  kernel: &mut Kernel,
  candidate_key: SavedProfileKey,
  result: Result<(SavedProfileKey, Vec<jellypilot_auth::SavedProfileSummary>), AuthStorageError>,
) -> Update {
  match result {
    Ok((stored_key, profiles)) => {
      login_flow.profiles_revision = login_flow.profiles_revision.wrapping_add(1);
      login_flow.profiles = profiles;
      if surface.current_candidate_key.as_ref() == Some(&candidate_key)
        && candidate_key == stored_key
        && kernel.connection == ConnectionPhase::Connected
      {
        kernel.active_profile = Some(stored_key.clone());
        return Update::task(record_activation_task(
          kernel.auth_store.clone(),
          stored_key,
        ));
      }
    }
    Err(error) if surface.current_candidate_key.as_ref() == Some(&candidate_key) => {
      surface.error = Some(format!(
        "Connected for this session, but the login could not be saved: {error}."
      ));
    }
    Err(_) => {}
  }
  Update::none()
}

fn record_activation_task(
  store: jellypilot_auth::AuthStore,
  candidate_key: SavedProfileKey,
) -> Task<Message> {
  let key = candidate_key.clone();
  Task::perform(
    async move { store.record_successful_activation(key).await },
    move |result| Message::ActivationRecorded {
      candidate_key,
      result,
    },
  )
}

fn finish_activation_record(
  surface: &mut Surface,
  kernel: &Kernel,
  candidate_key: SavedProfileKey,
  result: Result<(), AuthStorageError>,
) -> Update {
  if surface.current_candidate_key.as_ref() == Some(&candidate_key)
    && kernel.active_profile.as_ref() == Some(&candidate_key)
  {
    if let Err(error) = result {
      surface.error = Some(format!(
        "Connected, but the startup account selection could not be saved: {error}."
      ));
    }
  }
  Update::none()
}

#[cfg(test)]
mod tests {
  use std::fs;
  use std::path::PathBuf;

  use jellypilot_media_server::SavedSession;

  use super::*;

  struct TestSettingsFile(PathBuf);

  impl Drop for TestSettingsFile {
    fn drop(&mut self) {
      let _ = fs::remove_file(&self.0);
    }
  }

  fn saved_session(name: &str) -> SavedSession {
    SavedSession {
      provider: MediaServerProvider::Jellyfin,
      server_url: format!("https://{name}.example.test"),
      access_token: format!("{name}-token"),
      user_id: format!("{name}-user-id"),
      user_name: name.to_owned(),
      server_name: Some(format!("{name} server")),
      device_id: Some(format!("{name}-device")),
    }
  }

  fn validated_candidate(name: &str) -> ValidatedProfileCandidate {
    let client = Arc::new(jellypilot_media_server::JellyfinClient::new());
    client.login().adopt_validated_session(&saved_session(name));
    ValidatedProfileCandidate::from_authenticated_client(client)
      .unwrap_or_else(|_| panic!("complete authenticated client should become a candidate"))
  }

  fn connect_kernel(
    kernel: &mut Kernel,
    name: &str,
  ) -> Arc<jellypilot_media_server::JellyfinClient> {
    let session = saved_session(name);
    let client = Arc::new(jellypilot_media_server::JellyfinClient::new());
    client.login().adopt_validated_session(&session);
    kernel.connection = ConnectionPhase::Connected;
    kernel.connected_identity = Some(ConnectedIdentity {
      user_name: session.user_name.clone(),
      provider: session.provider,
      server_url: session.server_url.clone(),
      server_name: session.server_name.clone(),
    });
    kernel.active_profile = Some(SavedProfileKey::for_session(&session));
    kernel.client = Some(Arc::clone(&client));
    client
  }

  #[test]
  fn cancelling_candidate_confirmation_drops_the_candidate_operation() {
    let mut surface = Surface::new();
    surface.operation = Operation::CandidateReady {
      generation: 7,
      handoff: HandoffKind::Activate {
        candidate: Box::new(validated_candidate("candidate")),
        save_session: false,
        submission: None,
      },
    };
    surface.confirmation = Some(PendingConfirmation::CandidateHandoff {
      generation: 7,
      kind: ConfirmationKind::SwitchAccount,
      account: "Ada@Media".to_owned(),
    });

    let _ = cancel_confirmation(&mut surface);

    assert!(matches!(surface.operation, Operation::Idle));
    assert!(surface.confirmation.is_none());
  }

  #[test]
  fn stale_handoff_settlements_do_not_advance_the_current_operation() {
    let mut surface = Surface::new();
    surface.operation = Operation::Handoff {
      generation: 9,
      kind: HandoffKind::Disconnect,
      remote_done: false,
      playback_result: None,
    };

    let (mut kernel, mut login_flow, _settings) = test_kernel();
    let _ = settle_remote_handoff(&mut surface, &mut login_flow, &mut kernel, 8, false);
    let _ = settle_playback_handoff(&mut surface, &mut login_flow, &mut kernel, 8, Ok(()), false);

    assert!(matches!(
      surface.operation,
      Operation::Handoff {
        remote_done: false,
        playback_result: None,
        ..
      }
    ));
  }

  #[test]
  fn content_mutations_are_blocked_only_during_active_account_teardown() {
    let scope = ProfileScope::new(
      MediaServerProvider::Jellyfin,
      "https://inactive.example.test",
      "inactive-user",
    )
    .expect("test account scope should be valid");
    let mut surface = Surface::new();

    surface.operation = Operation::ValidatingSaved {
      generation: 1,
      key: SavedProfileKey::for_scope(&scope),
      playback_confirmed: false,
    };
    assert!(!content_mutations_blocked(&surface));

    surface.operation = Operation::RemovingCredentials {
      generation: 2,
      scope: scope.clone(),
      active: false,
      delete_watchlist: true,
    };
    assert!(!content_mutations_blocked(&surface));

    surface.operation = Operation::RemovingCredentials {
      generation: 3,
      scope,
      active: true,
      delete_watchlist: true,
    };
    assert!(content_mutations_blocked(&surface));

    surface.operation = Operation::Handoff {
      generation: 3,
      kind: HandoffKind::Disconnect,
      remote_done: false,
      playback_result: None,
    };
    assert!(content_mutations_blocked(&surface));
  }

  #[test]
  fn cleanup_failure_retains_the_handoff_for_retry() {
    let mut surface = Surface::new();
    surface.operation = Operation::Handoff {
      generation: 3,
      kind: HandoffKind::Disconnect,
      remote_done: true,
      playback_result: None,
    };
    let (mut kernel, mut login_flow, _settings) = test_kernel();
    let old_client = connect_kernel(&mut kernel, "current");

    let settlement = settle_playback_handoff(
      &mut surface,
      &mut login_flow,
      &mut kernel,
      3,
      Err("MPV cleanup failed".to_owned()),
      false,
    );

    assert!(settlement.effect.is_none());
    assert_eq!(handoff_generation(&surface), Some(3));
    assert!(kernel
      .client
      .as_ref()
      .is_some_and(|client| Arc::ptr_eq(client, &old_client)));
    assert_eq!(kernel.connection, ConnectionPhase::Connected);
    assert!(surface
      .error
      .as_deref()
      .is_some_and(|error| error.contains("remains connected")));
    assert!(can_retry_handoff_cleanup(&surface));

    let handoff_error = surface.error.clone();
    let scope = ProfileScope::new(
      MediaServerProvider::Jellyfin,
      "https://cleanup.example.test",
      "cleanup-user",
    )
    .expect("cleanup scope should be valid");
    surface.watchlist_in_flight.insert(11, scope);
    let _ = finish_watchlist_removal(
      &mut surface,
      11,
      Err("Watchlist storage unavailable".to_owned()),
    );
    assert_ne!(surface.error, handoff_error);
    assert!(surface
      .error
      .as_deref()
      .is_some_and(|error| error.contains("Watchlist remains")));
    assert!(can_retry_handoff_cleanup(&surface));

    let runtime = personal_lists::Runtime::default();
    let _ = update(
      &mut surface,
      &mut login_flow,
      &mut kernel,
      &runtime,
      RuntimeFacts {
        playback_active: false,
        quit_requested: false,
      },
      Message::DismissError,
    );
    assert!(surface.error.is_none());
    assert!(can_retry_handoff_cleanup(&surface));

    let _ = retry_watchlist_cleanup(&mut surface, &runtime);
    assert!(surface.error.is_none());
    assert!(can_retry_handoff_cleanup(&surface));

    let retry = retry_handoff(&mut surface);
    assert_eq!(retry.effect, Some(Effect::BeginHandoff { generation: 3 }));
  }

  #[test]
  fn active_sign_out_cleanup_failure_reports_that_credentials_were_removed() {
    let mut surface = Surface::new();
    surface.operation = Operation::Handoff {
      generation: 4,
      kind: HandoffKind::SignOut,
      remote_done: true,
      playback_result: None,
    };
    let (mut kernel, mut login_flow, _settings) = test_kernel();
    connect_kernel(&mut kernel, "current");

    let update = settle_playback_handoff(
      &mut surface,
      &mut login_flow,
      &mut kernel,
      4,
      Err("MPV process cleanup could not be confirmed".to_owned()),
      false,
    );

    assert!(update.effect.is_none());
    assert_eq!(handoff_generation(&surface), Some(4));
    let error = surface
      .error
      .as_deref()
      .expect("active sign out cleanup failure should be visible");
    assert!(error.contains("saved login was removed"));
    assert!(error.contains("current session remains connected"));
    assert!(!error.contains("account was not changed"));
  }

  #[test]
  fn candidate_is_adopted_only_after_remote_and_playback_settle() {
    let mut surface = Surface::new();
    let candidate = validated_candidate("candidate");
    let candidate_key = candidate.key().clone();
    let candidate_client = Arc::clone(candidate.client());
    surface.operation = Operation::Handoff {
      generation: 5,
      kind: HandoffKind::Activate {
        candidate: Box::new(candidate),
        save_session: false,
        submission: None,
      },
      remote_done: false,
      playback_result: None,
    };
    let (mut kernel, mut login_flow, _settings) = test_kernel();
    let old_client = connect_kernel(&mut kernel, "current");

    let playback =
      settle_playback_handoff(&mut surface, &mut login_flow, &mut kernel, 5, Ok(()), false);

    assert!(playback.effect.is_none());
    assert!(kernel
      .client
      .as_ref()
      .is_some_and(|client| Arc::ptr_eq(client, &old_client)));

    let remote = settle_remote_handoff(&mut surface, &mut login_flow, &mut kernel, 5, false);

    assert_eq!(remote.effect, Some(Effect::Activated));
    assert_eq!(kernel.active_profile.as_ref(), Some(&candidate_key));
    assert!(kernel
      .client
      .as_ref()
      .is_some_and(|client| Arc::ptr_eq(client, &candidate_client)));
  }

  #[test]
  fn saved_identity_stays_active_when_reauthentication_cannot_be_persisted() {
    let mut surface = Surface::new();
    let candidate = validated_candidate("saved");
    let candidate_key = candidate.key().clone();
    let candidate_scope = candidate.scope().clone();
    let (mut kernel, mut login_flow, _settings) = test_kernel();
    connect_kernel(&mut kernel, "current");

    let activation = activate_candidate(
      &mut surface,
      &mut login_flow,
      &mut kernel,
      4,
      candidate,
      true,
      None,
    );
    assert_eq!(activation.effect, Some(Effect::Activated));
    assert_eq!(kernel.active_profile.as_ref(), Some(&candidate_key));

    let _ = finish_session_storage(
      &mut surface,
      &mut login_flow,
      &mut kernel,
      candidate_key.clone(),
      Err(AuthStorageError::WriteFailed),
    );
    let _ = request_sign_out_for_profile(
      &mut surface,
      &kernel,
      candidate_key,
      candidate_scope,
      "saved@saved server".to_owned(),
    );
    assert!(matches!(
      surface.confirmation.as_ref(),
      Some(PendingConfirmation::SignOut { active: true, .. })
    ));

    let runtime = personal_lists::Runtime::default();
    let removal = confirm(&mut surface, &kernel, &runtime);
    assert!(removal.effect.is_none());
    let generation = surface
      .operation
      .generation()
      .expect("credential removal should own an operation generation");
    let completion = finish_credentials_removal(
      &mut surface,
      &mut login_flow,
      &runtime,
      generation,
      Ok(Vec::new()),
      false,
    );
    assert_eq!(completion.effect, Some(Effect::BeginHandoff { generation }));
  }

  #[test]
  fn quit_discards_late_candidate_and_active_credential_completions() {
    let candidate = validated_candidate("candidate");
    let key = candidate.key().clone();
    let scope = candidate.scope().clone();
    let (mut kernel, mut login_flow, _settings) = test_kernel();
    let old_client = connect_kernel(&mut kernel, "current");
    let mut surface = Surface::new();
    surface.operation = Operation::ValidatingSaved {
      generation: 8,
      key: key.clone(),
      playback_confirmed: false,
    };

    let validation = finish_saved_validation(
      &mut surface,
      false,
      true,
      8,
      key,
      Ok(ProtectedCandidate::new(candidate)),
    );

    assert!(validation.effect.is_none());
    assert!(matches!(surface.operation, Operation::Idle));
    assert!(kernel
      .client
      .as_ref()
      .is_some_and(|client| Arc::ptr_eq(client, &old_client)));

    surface.operation = Operation::RemovingCredentials {
      generation: 9,
      scope,
      active: true,
      delete_watchlist: false,
    };
    let removal = finish_credentials_removal(
      &mut surface,
      &mut login_flow,
      &personal_lists::Runtime::default(),
      9,
      Ok(Vec::new()),
      true,
    );

    assert!(removal.effect.is_none());
    assert!(matches!(surface.operation, Operation::Idle));
    assert!(kernel
      .client
      .as_ref()
      .is_some_and(|client| Arc::ptr_eq(client, &old_client)));
  }

  #[test]
  fn quit_after_handoff_cleanup_does_not_adopt_the_candidate() {
    let mut surface = Surface::new();
    let candidate = validated_candidate("candidate");
    let candidate_key = candidate.key().clone();
    surface.operation = Operation::Handoff {
      generation: 6,
      kind: HandoffKind::Activate {
        candidate: Box::new(candidate),
        save_session: false,
        submission: None,
      },
      remote_done: true,
      playback_result: None,
    };
    let (mut kernel, mut login_flow, _settings) = test_kernel();
    connect_kernel(&mut kernel, "current");

    let update =
      settle_playback_handoff(&mut surface, &mut login_flow, &mut kernel, 6, Ok(()), true);

    assert_eq!(update.effect, Some(Effect::Disconnected));
    assert_eq!(kernel.connection, ConnectionPhase::SignedOut);
    assert_ne!(kernel.active_profile.as_ref(), Some(&candidate_key));
    assert!(kernel.client.is_none());
  }

  #[test]
  fn clipboard_status_changes_only_for_the_latest_verified_copy() {
    let mut surface = Surface::new();
    surface.copy_generation = Some(2);

    let _ = finish_clipboard_verification(&mut surface, 1, true);
    assert_eq!(surface.copy_status, CopyStatus::Idle);
    assert_eq!(surface.copy_generation, Some(2));

    let _ = finish_clipboard_verification(&mut surface, 2, false);
    assert_eq!(surface.copy_status, CopyStatus::Failed);
    assert!(surface.copy_generation.is_none());

    surface.copy_generation = Some(3);
    let _ = finish_clipboard_verification(&mut surface, 3, true);
    assert_eq!(surface.copy_status, CopyStatus::Copied);
  }

  fn test_kernel() -> (Kernel, LoginState, TestSettingsFile) {
    use jellypilot_core::artwork_binder::ArtworkBinder;
    use jellypilot_core::config::SettingsStore;
    use jellypilot_core::diagnostics::Diagnostics;
    use jellypilot_core::request_gate::RequestGate;
    use jellypilot_media_server::artwork::ArtworkAdapter;

    let path = std::env::temp_dir().join(format!(
      "jellypilot-accounts-test-{}.json",
      std::process::id()
    ));
    let settings = SettingsStore::for_test(path.clone());
    let login = LoginState::from_settings(settings.snapshot());
    let kernel = Kernel {
      settings,
      auth_store: jellypilot_auth::AuthStore::default(),
      client: None,
      connection: ConnectionPhase::SignedOut,
      connected_identity: None,
      active_profile: None,
      request_gate: RequestGate::default(),
      diagnostics: Diagnostics::default(),
      notice: None,
      active_toast: None,
      next_toast_id: 0,
      tray: None,
      artwork_adapter: Arc::new(ArtworkAdapter::new()),
      artwork_binder: ArtworkBinder::default(),
      artwork_handles: super::super::state::ArtworkHandleRetention::default(),
    };
    (kernel, login, TestSettingsFile(path))
  }
}
