//! Saved-account management and the single-active-connection handoff.
//!
//! The shared [`jellypilot_sdk::Sdk`] owns credential removal, account
//! adoption/disconnection, session persistence, and startup-restore ordering.
//! This reducer keeps presentation: confirmations, the add-account form, the
//! physical playback/remote teardown the SDK requests through its handoff
//! hook, and projection of committed outcomes into [`Kernel`].

use std::sync::Arc;

use iced::Task;
use jellypilot_auth::SavedProfileKey;
use jellypilot_media_server::MediaServerProvider;
use jellypilot_sdk::{ActivationOutcome, SdkError, SignOutOutcome};
use jellypilot_session::RemoteControlState;

use super::kernel::{HookRequest, Kernel};
use super::login::{self, CandidateMessage, CandidateSurface, CandidateUpdate};
use super::message::{PasswordSubmission, ProtectedCandidate};
use super::state::{ConnectedIdentity, LoginState, State};
use crate::i18n::UiText;

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
  pub error: Option<&'a UiText>,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Effect {
  /// The SDK teardown hook needs physical playback and remote teardown for
  /// `generation`; the router drives it and reports the settlements back.
  BeginHandoff { generation: u64 },
  /// The SDK committed a new active profile; reset account-bound surfaces
  /// and start the connection.
  Activated,
  /// The SDK ended the active profile scope; reset account-bound surfaces.
  Disconnected,
  /// SDK-side Watchlist cleanup deleted records for the still-active scope;
  /// the router must reload the membership projection.
  MembershipInvalidated,
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
  /// The SDK teardown hook asked for physical teardown on the UI loop.
  HandoffRequested(HookRequest),
  CandidateValidated {
    generation: u64,
    key: SavedProfileKey,
    result: Result<ProtectedCandidate, SdkError>,
  },
  ActivationFinished {
    generation: u64,
    result: Result<ActivationOutcome, SdkError>,
  },
  DisconnectFinished {
    generation: u64,
    result: Result<(), SdkError>,
  },
  SignOutFinished {
    generation: u64,
    result: Result<SignOutOutcome, SdkError>,
  },
  WatchlistCleanupFinished {
    generation: u64,
    key: SavedProfileKey,
    result: Result<(), SdkError>,
  },
  RemoteHandoffSettled {
    generation: u64,
  },
  PlaybackHandoffSettled {
    generation: u64,
    result: Result<(), String>,
  },
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

/// One SDK account operation this reducer started and still owns.
enum SdkOp {
  Activate {
    save_session: bool,
    submission: Option<PasswordSubmission>,
  },
  Disconnect,
  /// Retries pending Sign Out teardown through `Sdk::disconnect`.
  SignOutRetry,
  SignOut {
    key: SavedProfileKey,
    delete_watchlist: bool,
  },
  WatchlistRetry {
    key: SavedProfileKey,
  },
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
    candidate: ProtectedCandidate,
    save_session: bool,
    submission: Option<PasswordSubmission>,
  },
  Sdk {
    generation: u64,
    kind: SdkOp,
  },
}

impl Operation {
  const fn generation(&self) -> Option<u64> {
    match self {
      Self::Idle => None,
      Self::AuthenticatingNew { generation, .. }
      | Self::ValidatingSaved { generation, .. }
      | Self::CandidateReady { generation, .. }
      | Self::Sdk { generation, .. } => Some(*generation),
    }
  }
}

/// The SDK teardown hook currently being serviced on the UI loop.
///
/// `irreversible` marks Sign Out teardown (credential deletion already
/// committed): a physical failure resolves the hook `false` so the SDK can
/// record the pending cleanup. Reversible teardown (activation, disconnect)
/// stays pending across failures so Retry re-drives the same hook; it only
/// resolves `false` when the transition is explicitly abandoned.
struct PendingHook {
  generation: u64,
  request: HookRequest,
  irreversible: bool,
  remote_done: bool,
  playback_done: bool,
  failed: bool,
}

pub struct Surface {
  pub management_open: bool,
  pub error: Option<UiText>,
  /// Technical event from the current reducer turn, consumed by the router.
  pub diagnostic: Option<String>,
  pub copy_status: CopyStatus,
  pub add_account: Option<CandidateSurface>,
  confirmation: Option<PendingConfirmation>,
  operation: Operation,
  pending_hook: Option<PendingHook>,
  next_generation: u64,
  next_candidate_instance: u64,
  next_copy_generation: u64,
  copy_generation: Option<u64>,
  busy_profile: Option<SavedProfileKey>,
  /// Saved-profile keys whose opted-in Watchlist cleanup still needs a retry.
  failed_watchlist_cleanup: Vec<SavedProfileKey>,
}

impl Surface {
  pub fn new() -> Self {
    Self {
      management_open: false,
      error: None,
      diagnostic: None,
      copy_status: CopyStatus::Idle,
      add_account: None,
      confirmation: None,
      operation: Operation::Idle,
      pending_hook: None,
      next_generation: 0,
      next_candidate_instance: 0,
      next_copy_generation: 0,
      copy_generation: None,
      busy_profile: None,
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
    error: surface.error.as_ref(),
    auto_login: state.kernel.settings.snapshot().auto_login(),
    remote_control: state.playback.remote.view().state,
    copy_status: surface.copy_status,
    add_account: surface.add_account.as_ref(),
    handoff_blocking: surface.pending_hook.is_some(),
    can_retry_handoff_cleanup: can_retry_handoff_cleanup(surface, &state.kernel),
    can_retry_watchlist_cleanup: !surface.failed_watchlist_cleanup.is_empty(),
  }
}

fn can_retry_handoff_cleanup(surface: &Surface, kernel: &Kernel) -> bool {
  surface
    .pending_hook
    .as_ref()
    .is_some_and(|hook| hook.failed)
    || kernel.sdk.sign_out_cleanup_pending()
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

/// Whether the SDK forbids new playback and content writes right now.
///
/// True while the SDK owns an active-profile transition (including teardown
/// the hook is still servicing) and after a failed Sign Out cleanup until
/// the retry succeeds. Candidate validation is excluded: the current account
/// remains fully active until the SDK starts the handoff.
pub fn content_mutations_blocked(kernel: &Kernel) -> bool {
  kernel.sdk.content_mutations_blocked()
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

/// Dismisses every transient account surface when the window that hosted it
/// closes (ADR 0043): the management popover, add-account sheet, and pending
/// confirmation. In-flight authentication or an irreversible handoff keeps
/// running — only the presentation is dropped.
pub fn dismiss_transient(surface: &mut Surface) {
  hide(surface);
  let _ = close_add_account(surface);
  let _ = cancel_confirmation(surface);
}

pub fn update(
  surface: &mut Surface,
  login_flow: &mut LoginState,
  kernel: &mut Kernel,
  facts: RuntimeFacts,
  message: Message,
) -> Update {
  surface.diagnostic = None;
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
    Message::Disconnect => request_disconnect(surface, kernel, facts.playback_active),
    Message::AskSignOut(key) => request_sign_out(surface, login_flow, kernel, key),
    Message::ToggleManagement => {
      surface.management_open = !surface.management_open;
      Update::none()
    }
    Message::Confirm => confirm(surface, kernel),
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
    Message::RetryHandoffCleanup => retry_handoff(surface, kernel),
    Message::RetryWatchlistCleanup => retry_watchlist_cleanup(surface, kernel),
    Message::DismissError => {
      surface.error = None;
      Update::none()
    }
    Message::HandoffRequested(request) => start_hook(surface, kernel, request),
    Message::CandidateValidated {
      generation,
      key,
      result,
    } => finish_saved_validation(
      surface,
      kernel,
      facts.playback_active,
      facts.quit_requested,
      generation,
      key,
      result,
    ),
    Message::ActivationFinished { generation, result } => finish_activation(
      surface,
      login_flow,
      kernel,
      generation,
      result,
      facts.quit_requested,
    ),
    Message::DisconnectFinished { generation, result } => {
      finish_disconnect(surface, kernel, generation, result)
    }
    Message::SignOutFinished { generation, result } => {
      finish_sign_out(surface, login_flow, kernel, generation, result)
    }
    Message::WatchlistCleanupFinished {
      generation,
      key,
      result,
    } => finish_watchlist_cleanup(surface, kernel, generation, key, result),
    Message::RemoteHandoffSettled { generation } => {
      settle_remote_handoff(surface, kernel, facts.quit_requested, generation)
    }
    Message::PlaybackHandoffSettled { generation, result } => {
      settle_playback_handoff(surface, kernel, facts.quit_requested, generation, result)
    }
  }
}

fn copy_server_address(surface: &mut Surface, kernel: &Kernel) -> Update {
  let Some(identity) = &kernel.connected_identity else {
    surface.diagnostic = Some("No connected server address is available to copy.".to_owned());
    surface.error = Some(UiText::new("account-no-copy-address"));
    return Update::none();
  };
  surface.next_copy_generation = surface.next_copy_generation.wrapping_add(1);
  let generation = surface.next_copy_generation;
  surface.copy_generation = Some(generation);
  surface.copy_status = CopyStatus::Idle;
  let contents = identity.server_url.clone();
  let expected = contents.clone();
  Update::task(
    iced::clipboard::write(contents)
      .discard()
      .chain(
        iced::clipboard::read_text().map(move |actual| Message::ClipboardVerified {
          generation,
          matched: actual.as_deref().ok() == Some(&expected),
        }),
      ),
  )
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
  if surface.pending_hook.is_some() {
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
    if !matches!(surface.operation, Operation::Sdk { .. }) {
      surface.operation = Operation::Idle;
    }
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
      let CandidateUpdate { task, .. } = login::update_candidate(add_account, &kernel.sdk, message);
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
  let CandidateUpdate { task, completion } =
    login::update_candidate(add_account, &kernel.sdk, message);
  if let (Some((generation, _)), Some(completion)) = (active_authentication, completion) {
    let follow_up =
      finish_new_authentication(surface, kernel, playback_active, generation, completion);
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
  kernel: &Kernel,
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
  let CandidateUpdate { task, completion } =
    login::update_candidate(add_account, &kernel.sdk, message);
  if let Some(completion) = completion {
    return finish_new_authentication(surface, kernel, false, generation, completion);
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
  kernel: &Kernel,
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
  let account = completion.candidate.account_title().to_owned();
  let candidate = ProtectedCandidate::new(completion.candidate);
  if playback_active && !playback_confirmed {
    surface.operation = Operation::CandidateReady {
      generation,
      candidate,
      save_session: true,
      submission: completion.submission,
    };
    surface.confirmation = Some(PendingConfirmation::CandidateHandoff {
      generation,
      kind: ConfirmationKind::ConnectAndSwitch,
      account,
    });
    return Update::none();
  }
  start_activation(
    surface,
    kernel,
    generation,
    candidate,
    true,
    completion.submission,
  )
}

/// Starts SDK activation for a validated candidate.
///
/// The SDK owns teardown ordering, the client swap, session persistence, and
/// startup-restore recording; this reducer only tracks the operation and
/// projects the committed outcome.
fn start_activation(
  surface: &mut Surface,
  kernel: &Kernel,
  generation: u64,
  candidate: ProtectedCandidate,
  save_session: bool,
  submission: Option<PasswordSubmission>,
) -> Update {
  let Some(candidate) = candidate.take() else {
    surface.operation = Operation::Idle;
    surface.busy_profile = None;
    return Update::none();
  };
  surface.operation = Operation::Sdk {
    generation,
    kind: SdkOp::Activate {
      save_session,
      submission,
    },
  };
  surface.copy_generation = None;
  surface.copy_status = CopyStatus::Idle;
  surface.confirmation = None;
  surface.add_account = None;
  let sdk = Arc::clone(&kernel.sdk);
  Update::task(Task::perform(
    async move { sdk.activate_candidate(candidate, save_session).await },
    move |result| Message::ActivationFinished { generation, result },
  ))
}

fn request_switch(
  surface: &mut Surface,
  login_flow: &LoginState,
  kernel: &Kernel,
  playback_active: bool,
  key: SavedProfileKey,
) -> Update {
  if surface.action_blocked() || kernel.active_profile.as_ref() == Some(&key) {
    return Update::none();
  }
  let Some(profile) = login_flow
    .profiles
    .iter()
    .find(|profile| profile.key() == &key)
  else {
    surface.diagnostic = Some("This saved sign-in is no longer available.".to_owned());
    surface.error = Some(UiText::new("login-profile-missing"));
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
  let sdk = Arc::clone(&kernel.sdk);
  let completion_key = key.clone();
  Update::task(Task::perform(
    async move {
      sdk
        .restore_saved_profile(key.as_str().to_owned())
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
  kernel: &Kernel,
  playback_active: bool,
  quit_requested: bool,
  generation: u64,
  key: SavedProfileKey,
  result: Result<ProtectedCandidate, SdkError>,
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
  let candidate =
    match result.and_then(|candidate| candidate.take().ok_or(SdkError::ProfileNotFound)) {
      Ok(candidate) => candidate,
      Err(error) => {
        surface.operation = Operation::Idle;
        surface.busy_profile = None;
        surface.diagnostic = Some("Could not validate this saved sign-in. Try again.".to_owned());
        surface.error = Some(sdk_error_text(&error));
        return Update::none();
      }
    };
  let account = candidate.account_title().to_owned();
  let candidate = ProtectedCandidate::new(candidate);
  if playback_active && !playback_confirmed {
    surface.operation = Operation::CandidateReady {
      generation,
      candidate,
      save_session: false,
      submission: None,
    };
    surface.confirmation = Some(PendingConfirmation::CandidateHandoff {
      generation,
      kind: ConfirmationKind::SwitchAccount,
      account,
    });
    return Update::none();
  }
  start_activation(surface, kernel, generation, candidate, false, None)
}

fn request_disconnect(surface: &mut Surface, kernel: &Kernel, playback_active: bool) -> Update {
  if surface.action_blocked() {
    return Update::none();
  }
  if playback_active {
    surface.confirmation = Some(PendingConfirmation::Disconnect);
    return Update::none();
  }
  let generation = surface.begin_operation();
  start_disconnect(surface, kernel, generation)
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
    surface.diagnostic = Some("This saved sign-in is no longer available.".to_owned());
    surface.error = Some(UiText::new("login-profile-missing"));
    return Update::none();
  };
  let account = profile.title();
  surface.confirmation = Some(PendingConfirmation::SignOut {
    account,
    active: kernel.active_profile.as_ref() == Some(&key),
    key,
    delete_watchlist: false,
  });
  surface.error = None;
  Update::none()
}

fn confirm(surface: &mut Surface, kernel: &Kernel) -> Update {
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
          candidate,
          save_session,
          submission,
        } if active_generation == generation => start_activation(
          surface,
          kernel,
          generation,
          candidate,
          save_session,
          submission,
        ),
        other => {
          surface.operation = other;
          Update::none()
        }
      }
    }
    PendingConfirmation::Disconnect => {
      let generation = surface.begin_operation();
      start_disconnect(surface, kernel, generation)
    }
    PendingConfirmation::SignOut {
      key,
      delete_watchlist,
      ..
    } => start_sign_out(surface, kernel, key, delete_watchlist),
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

fn start_disconnect(surface: &mut Surface, kernel: &Kernel, generation: u64) -> Update {
  surface.operation = Operation::Sdk {
    generation,
    kind: SdkOp::Disconnect,
  };
  surface.copy_generation = None;
  surface.copy_status = CopyStatus::Idle;
  surface.confirmation = None;
  surface.add_account = None;
  let sdk = Arc::clone(&kernel.sdk);
  Update::task(Task::perform(
    async move { sdk.disconnect().await },
    move |result| Message::DisconnectFinished { generation, result },
  ))
}

fn start_sign_out(
  surface: &mut Surface,
  kernel: &Kernel,
  key: SavedProfileKey,
  delete_watchlist: bool,
) -> Update {
  let generation = surface.begin_operation();
  surface.busy_profile = Some(key.clone());
  surface.operation = Operation::Sdk {
    generation,
    kind: SdkOp::SignOut {
      key: key.clone(),
      delete_watchlist,
    },
  };
  surface.copy_generation = None;
  surface.copy_status = CopyStatus::Idle;
  surface.confirmation = None;
  surface.add_account = None;
  let sdk = Arc::clone(&kernel.sdk);
  Update::task(Task::perform(
    async move {
      sdk
        .sign_out(key.as_str().to_owned(), delete_watchlist)
        .await
    },
    move |result| Message::SignOutFinished { generation, result },
  ))
}

/// Registers the SDK teardown hook and asks the router to drive physical
/// playback and remote teardown for it.
fn start_hook(surface: &mut Surface, kernel: &Kernel, request: HookRequest) -> Update {
  if surface.pending_hook.is_some() || !matches!(surface.operation, Operation::Sdk { .. }) {
    // No SDK operation this reducer owns is waiting on teardown; decline so
    // the SDK never commits a transition nobody is tracking.
    request.resolve(false);
    return Update::none();
  }
  let generation = surface.begin_operation();
  surface.pending_hook = Some(PendingHook {
    generation,
    request,
    irreversible: kernel.sdk.sign_out_cleanup_pending(),
    remote_done: false,
    playback_done: false,
    failed: false,
  });
  Update::effect(Effect::BeginHandoff { generation })
}

fn settle_remote_handoff(
  surface: &mut Surface,
  kernel: &Kernel,
  quit_requested: bool,
  generation: u64,
) -> Update {
  let Some(hook) = &mut surface.pending_hook else {
    return Update::none();
  };
  if hook.generation != generation {
    return Update::none();
  }
  hook.remote_done = true;
  resolve_hook_if_ready(surface, kernel, quit_requested)
}

fn settle_playback_handoff(
  surface: &mut Surface,
  kernel: &Kernel,
  quit_requested: bool,
  generation: u64,
  result: Result<(), String>,
) -> Update {
  let Some(hook) = &mut surface.pending_hook else {
    return Update::none();
  };
  if hook.generation != generation {
    return Update::none();
  }
  hook.playback_done = true;
  match result {
    Ok(()) => resolve_hook_if_ready(surface, kernel, quit_requested),
    Err(error) => {
      if hook.irreversible {
        // Sign Out already deleted the credentials: report the failure so
        // the SDK records pending cleanup and keeps authentication for the
        // retry driven by `Sdk::disconnect`.
        let hook = surface.pending_hook.take().expect("hook checked above");
        surface.error = Some(UiText::new("account-signout-cleanup-failed"));
        surface.diagnostic = Some(format!(
          "The saved login was removed, but external playback cleanup failed. The current session remains connected until cleanup is retried: {error}"
        ));
        hook.request.resolve(false);
      } else {
        hook.failed = true;
        surface.error = Some(UiText::new(match surface.operation {
          Operation::Sdk {
            kind: SdkOp::Activate { .. },
            ..
          } => "account-switch-cleanup-failed",
          _ => "account-disconnect-cleanup-failed",
        }));
        surface.diagnostic = Some(match surface.operation {
          Operation::Sdk {
            kind: SdkOp::Activate { .. },
            ..
          } => format!("External playback cleanup failed. The account was not changed: {error}"),
          _ => format!("External playback cleanup failed. The account remains connected: {error}"),
        });
      }
      Update::none()
    }
  }
}

fn resolve_hook_if_ready(surface: &mut Surface, kernel: &Kernel, quit_requested: bool) -> Update {
  let ready = surface
    .pending_hook
    .as_ref()
    .is_some_and(|hook| hook.remote_done && hook.playback_done && !hook.failed);
  if !ready {
    return Update::none();
  }
  let hook = surface.pending_hook.take().expect("hook readiness checked");
  if quit_requested {
    // Close before resolving: a committed activation must not adopt a new
    // candidate while the process is exiting.
    kernel.sdk.close();
  }
  hook.request.resolve(true);
  Update::none()
}

fn retry_handoff(surface: &mut Surface, kernel: &Kernel) -> Update {
  if let Some(hook) = &mut surface.pending_hook {
    if !hook.failed {
      return Update::none();
    }
    // Re-drive physical teardown for the same pending hook; the SDK keeps
    // waiting on the original responder.
    hook.failed = false;
    hook.remote_done = false;
    hook.playback_done = false;
    surface.error = None;
    return Update::effect(Effect::BeginHandoff {
      generation: hook.generation,
    });
  }
  if kernel.sdk.sign_out_cleanup_pending() {
    if matches!(surface.operation, Operation::Sdk { .. }) {
      return Update::none();
    }
    let generation = surface.begin_operation();
    surface.operation = Operation::Sdk {
      generation,
      kind: SdkOp::SignOutRetry,
    };
    surface.error = None;
    let sdk = Arc::clone(&kernel.sdk);
    return Update::task(Task::perform(
      async move { sdk.disconnect().await },
      move |result| Message::DisconnectFinished { generation, result },
    ));
  }
  Update::none()
}

fn retry_watchlist_cleanup(surface: &mut Surface, kernel: &Kernel) -> Update {
  if surface.operation_busy() {
    return Update::none();
  }
  let Some(key) = surface.failed_watchlist_cleanup.first().cloned() else {
    return Update::none();
  };
  let generation = surface.begin_operation();
  surface.operation = Operation::Sdk {
    generation,
    kind: SdkOp::WatchlistRetry { key: key.clone() },
  };
  surface.error = None;
  let sdk = Arc::clone(&kernel.sdk);
  let cleanup_key = key.as_str().to_owned();
  Update::task(Task::perform(
    async move { sdk.retry_watchlist_cleanup(cleanup_key).await },
    move |result| Message::WatchlistCleanupFinished {
      generation,
      key,
      result,
    },
  ))
}

fn finish_activation(
  surface: &mut Surface,
  login_flow: &mut LoginState,
  kernel: &mut Kernel,
  generation: u64,
  result: Result<ActivationOutcome, SdkError>,
  quit_requested: bool,
) -> Update {
  if !matches!(surface.operation, Operation::Sdk { .. }) {
    return Update::none();
  }
  let Operation::Sdk {
    generation: active_generation,
    kind,
  } = std::mem::replace(&mut surface.operation, Operation::Idle)
  else {
    return Update::none();
  };
  if active_generation != generation {
    surface.operation = Operation::Sdk {
      generation: active_generation,
      kind,
    };
    return Update::none();
  }
  let (save_session, submission) = match kind {
    SdkOp::Activate {
      save_session,
      submission,
    } => (save_session, submission),
    other => {
      surface.operation = Operation::Sdk {
        generation: active_generation,
        kind: other,
      };
      return Update::none();
    }
  };
  match result {
    Ok(outcome) => {
      sync_activated(kernel, &outcome.profile);
      if let Some(submission) = submission {
        login::persist_password_submission(kernel, submission);
      }
      surface.busy_profile = None;
      if let Some(warning) = outcome.persistence_warning {
        surface.diagnostic = Some(format!(
          "Connected, but the login could not be persisted: {warning}."
        ));
        surface.error = Some(UiText::new(if save_session {
          "account-session-save-failed"
        } else {
          "account-activation-save-failed"
        }));
      } else {
        surface.error = None;
      }
      if quit_requested {
        return Update::none();
      }
      // The SDK persisted the session itself; refresh the saved-profile list
      // so the new/updated entry appears without a restart.
      login_flow.profiles_revision = login_flow.profiles_revision.wrapping_add(1);
      login_flow.profiles_loading = true;
      Update::effect(Effect::Activated)
    }
    Err(SdkError::HandoffAborted) => {
      // Reversible teardown was abandoned; the previous profile is intact.
      surface.busy_profile = None;
      Update::none()
    }
    Err(error) => {
      surface.busy_profile = None;
      surface.diagnostic = Some(error.to_string());
      surface.error = Some(sdk_error_text(&error));
      Update::none()
    }
  }
}

fn finish_disconnect(
  surface: &mut Surface,
  kernel: &mut Kernel,
  generation: u64,
  result: Result<(), SdkError>,
) -> Update {
  if !matches!(
    surface.operation,
    Operation::Sdk {
      kind: SdkOp::Disconnect | SdkOp::SignOutRetry,
      ..
    }
  ) {
    return Update::none();
  }
  let Operation::Sdk {
    generation: active_generation,
    kind,
  } = std::mem::replace(&mut surface.operation, Operation::Idle)
  else {
    return Update::none();
  };
  if active_generation != generation {
    surface.operation = Operation::Sdk {
      generation: active_generation,
      kind,
    };
    return Update::none();
  }
  let retry = matches!(kind, SdkOp::SignOutRetry);
  match result {
    Ok(()) => {
      sync_disconnected(kernel);
      surface.busy_profile = None;
      surface.error = None;
      Update::effect(Effect::Disconnected)
    }
    Err(SdkError::HandoffAborted) => {
      if retry {
        // Sign Out teardown failed again; authentication and the write block
        // stay until the next retry succeeds.
        surface.error = Some(UiText::new("account-signout-cleanup-failed"));
      }
      Update::none()
    }
    Err(error) => {
      surface.diagnostic = Some(error.to_string());
      surface.error = Some(sdk_error_text(&error));
      Update::none()
    }
  }
}

fn finish_sign_out(
  surface: &mut Surface,
  login_flow: &mut LoginState,
  kernel: &mut Kernel,
  generation: u64,
  result: Result<SignOutOutcome, SdkError>,
) -> Update {
  if !matches!(surface.operation, Operation::Sdk { .. }) {
    return Update::none();
  }
  let Operation::Sdk {
    generation: active_generation,
    kind,
  } = std::mem::replace(&mut surface.operation, Operation::Idle)
  else {
    return Update::none();
  };
  if active_generation != generation {
    surface.operation = Operation::Sdk {
      generation: active_generation,
      kind,
    };
    return Update::none();
  }
  let (key, delete_watchlist) = match kind {
    SdkOp::SignOut {
      key,
      delete_watchlist,
    } => (key, delete_watchlist),
    other => {
      surface.operation = Operation::Sdk {
        generation: active_generation,
        kind: other,
      };
      return Update::none();
    }
  };
  surface.busy_profile = None;
  match result {
    Ok(outcome) => {
      // The remaining saved profiles are authoritative even when cleanup
      // stays pending.
      login_flow.profiles_revision = login_flow.profiles_revision.wrapping_add(1);
      login_flow.profiles = outcome.remaining;
      login_flow.profiles_loading = false;
      let membership_invalidated = delete_watchlist
        && outcome.watchlist_error.is_none()
        && kernel.active_profile.as_ref() == Some(&key);
      if let Some(error) = outcome.watchlist_error {
        surface.failed_watchlist_cleanup.push(key.clone());
        surface.error = Some(UiText::new("account-watchlist-cleanup-failed"));
        surface.diagnostic = Some(format!(
          "The saved login was removed, but its Watchlist remains on this device: {error}"
        ));
      }
      if outcome.teardown_error.is_some() {
        // Authentication stays for the cleanup retry; the SDK keeps new
        // playback and writes blocked until `Sdk::disconnect` succeeds.
        surface.error = Some(UiText::new("account-signout-cleanup-failed"));
        if surface.diagnostic.is_none() {
          surface.diagnostic = Some(
            "The saved login was removed, but external playback cleanup failed. The current session remains connected until cleanup is retried."
              .to_owned(),
          );
        }
        return if membership_invalidated {
          Update::effect(Effect::MembershipInvalidated)
        } else {
          Update::none()
        };
      }
      // Only project a disconnect when the SDK actually ended the active
      // scope: signing out an inactive saved profile leaves the current
      // connection untouched.
      if kernel.sdk.active_profile().is_none() {
        sync_disconnected(kernel);
        return Update::effect(Effect::Disconnected);
      }
      // The signed-out scope stayed active only while teardown was pending;
      // a successful cleanup of the active scope's Watchlist invalidates the
      // membership projection.
      if membership_invalidated {
        return Update::effect(Effect::MembershipInvalidated);
      }
      Update::none()
    }
    Err(error) => {
      surface.diagnostic = Some(error.to_string());
      surface.error = Some(match &error {
        SdkError::Storage(_) => UiText::new("login-storage-write-failed"),
        other => sdk_error_text(other),
      });
      Update::none()
    }
  }
}

fn finish_watchlist_cleanup(
  surface: &mut Surface,
  kernel: &Kernel,
  generation: u64,
  key: SavedProfileKey,
  result: Result<(), SdkError>,
) -> Update {
  if !matches!(
    &surface.operation,
    Operation::Sdk { kind: SdkOp::WatchlistRetry { key: active_key }, .. } if active_key == &key
  ) {
    return Update::none();
  }
  let Operation::Sdk {
    generation: active_generation,
    kind,
  } = std::mem::replace(&mut surface.operation, Operation::Idle)
  else {
    return Update::none();
  };
  if active_generation != generation {
    surface.operation = Operation::Sdk {
      generation: active_generation,
      kind,
    };
    return Update::none();
  }
  match result {
    Ok(()) => {
      surface
        .failed_watchlist_cleanup
        .retain(|failed| failed != &key);
      // A successful cleanup of the still-active scope's records invalidates
      // the membership projection.
      if kernel.active_profile.as_ref() == Some(&key) {
        return Update::effect(Effect::MembershipInvalidated);
      }
    }
    Err(error) => {
      surface.error = Some(UiText::new("account-watchlist-cleanup-failed"));
      surface.diagnostic = Some(format!(
        "The saved login was removed, but its Watchlist remains on this device: {error}"
      ));
    }
  }
  Update::none()
}

/// Projects a committed SDK activation into the Kernel's read/presentation
/// fields. The SDK owns the client; native code only mirrors it.
pub(crate) fn sync_activated(kernel: &mut Kernel, profile: &jellypilot_sdk::ActiveProfile) {
  kernel.request_gate.disconnect();
  let session = kernel.request_gate.begin_login();
  let _ = kernel.request_gate.finish_login(session);
  kernel.client = kernel.sdk.active_client();
  kernel.connected_identity = Some(ConnectedIdentity::from_profile(profile));
  kernel.connection = jellypilot_auth::login::ConnectionPhase::Connected;
  kernel.active_profile = Some(SavedProfileKey::from_raw(profile.key.clone()));
}

/// Projects a committed SDK scope end into the Kernel's read/presentation
/// fields.
pub(crate) fn sync_disconnected(kernel: &mut Kernel) {
  kernel.client = None;
  kernel.request_gate.disconnect();
  kernel.connection = jellypilot_auth::login::ConnectionPhase::SignedOut;
  kernel.connected_identity = None;
  kernel.active_profile = None;
}

fn sdk_error_text(error: &SdkError) -> UiText {
  login::sdk_error_text(error)
}

#[cfg(test)]
mod tests {
  use std::fs;
  use std::path::PathBuf;
  use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
  use std::sync::Mutex;
  use std::time::Duration;

  use iced::futures::StreamExt;
  use jellypilot_auth::{AuthStore, CredentialError, SecureCredential, SensitiveSavedSession};
  use jellypilot_core::watchlist::{ProfileScope, WatchlistRecord, WatchlistStore};
  use jellypilot_media_server::{SavedSession, VideoLibraryItem};
  use jellypilot_sdk::ProfileCandidate;

  use super::*;

  struct TestSettingsFile(PathBuf);

  impl Drop for TestSettingsFile {
    fn drop(&mut self) {
      let _ = fs::remove_dir_all(&self.0);
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

  fn adopt(kernel: &Kernel, name: &str) -> jellypilot_sdk::ActiveProfile {
    kernel.sdk.adopt_test_session(saved_session(name));
    kernel
      .sdk
      .active_profile()
      .expect("adopted session is active")
  }

  fn connect_kernel(
    kernel: &mut Kernel,
    name: &str,
  ) -> Arc<jellypilot_media_server::JellyfinClient> {
    let profile = adopt(kernel, name);
    sync_activated(kernel, &profile);
    kernel.client.as_ref().expect("client projected").clone()
  }

  fn hook_request() -> (HookRequest, tokio::sync::oneshot::Receiver<bool>) {
    let (responder, outcome) = tokio::sync::oneshot::channel();
    (
      HookRequest {
        responder: Arc::new(Mutex::new(Some(responder))),
      },
      outcome,
    )
  }

  #[test]
  fn cancelling_candidate_confirmation_drops_the_candidate_operation() {
    let mut surface = Surface::new();
    surface.operation = Operation::CandidateReady {
      generation: 7,
      candidate: ProtectedCandidate::new(candidate_for("candidate")),
      save_session: false,
      submission: None,
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
  fn stale_handoff_settlements_do_not_advance_the_pending_hook() {
    let mut surface = Surface::new();
    let (kernel, _login_flow, _settings) = test_kernel();
    let (request, mut outcome) = hook_request();
    surface.pending_hook = Some(PendingHook {
      generation: 9,
      request,
      irreversible: false,
      remote_done: false,
      playback_done: false,
      failed: false,
    });

    let _ = settle_remote_handoff(&mut surface, &kernel, false, 8);
    let _ = settle_playback_handoff(&mut surface, &kernel, false, 8, Ok(()));

    let hook = surface.pending_hook.as_ref().expect("hook still pending");
    assert!(!hook.remote_done);
    assert!(!hook.playback_done);
    assert_eq!(
      outcome.try_recv(),
      Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    );
  }

  #[test]
  fn failed_reversible_teardown_keeps_the_hook_for_retry() {
    let mut surface = Surface::new();
    let (request, mut outcome) = hook_request();
    surface.operation = Operation::Sdk {
      generation: 3,
      kind: SdkOp::Disconnect,
    };
    surface.pending_hook = Some(PendingHook {
      generation: 3,
      request,
      irreversible: false,
      remote_done: true,
      playback_done: false,
      failed: false,
    });
    let (mut kernel, _login_flow, _settings) = test_kernel();
    connect_kernel(&mut kernel, "current");

    let _ = settle_playback_handoff(
      &mut surface,
      &kernel,
      false,
      3,
      Err("MPV cleanup failed".to_owned()),
    );

    let hook = surface.pending_hook.as_ref().expect("hook stays pending");
    assert!(hook.failed);
    assert!(can_retry_handoff_cleanup(&surface, &kernel));
    assert!(surface.error.is_some());

    let retry = retry_handoff(&mut surface, &kernel);
    assert_eq!(retry.effect, Some(Effect::BeginHandoff { generation: 3 }));
    let hook = surface.pending_hook.as_ref().expect("hook still pending");
    assert!(!hook.failed);
    assert!(!hook.remote_done);
    assert!(!hook.playback_done);
    assert_eq!(
      outcome.try_recv(),
      Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    );
    let _ = settle_remote_handoff(&mut surface, &kernel, false, 3);
    assert_eq!(
      outcome.try_recv(),
      Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    );
    let _ = settle_playback_handoff(&mut surface, &kernel, false, 3, Ok(()));
    assert_eq!(outcome.try_recv(), Ok(true));
  }

  #[test]
  fn failed_signout_teardown_resolves_the_hook_and_keeps_retry_pending() {
    let mut surface = Surface::new();
    let (request, mut outcome) = hook_request();
    surface.operation = Operation::Sdk {
      generation: 4,
      kind: SdkOp::SignOut {
        key: SavedProfileKey::from_raw("key".to_owned()),
        delete_watchlist: false,
      },
    };
    surface.pending_hook = Some(PendingHook {
      generation: 4,
      request,
      irreversible: true,
      remote_done: false,
      playback_done: false,
      failed: false,
    });
    let (mut kernel, _login_flow, _settings) = test_kernel();
    connect_kernel(&mut kernel, "current");

    let _ = settle_playback_handoff(
      &mut surface,
      &kernel,
      false,
      4,
      Err("MPV cleanup failed".to_owned()),
    );

    // The hook resolved false so the SDK records pending cleanup; the
    // reducer no longer owns a hook, and the session projection stays.
    assert!(surface.pending_hook.is_none());
    assert_eq!(
      kernel.connection,
      jellypilot_auth::login::ConnectionPhase::Connected
    );
    assert!(kernel.client.is_some());
    assert!(surface.error.is_some());
    assert_eq!(outcome.try_recv(), Ok(false));
  }

  #[test]
  fn hook_requests_without_an_sdk_operation_are_declined() {
    let mut surface = Surface::new();
    let (kernel, _login_flow, _settings) = test_kernel();
    let (request, mut outcome) = hook_request();

    let update = start_hook(&mut surface, &kernel, request);

    assert!(update.effect.is_none());
    assert!(surface.pending_hook.is_none());
    assert_eq!(outcome.try_recv(), Ok(false));
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

  #[tokio::test]
  async fn inactive_signout_preserves_active_connection_scope_and_watchlist() {
    let (mut kernel, mut login_flow, settings) = test_kernel_with_store(
      super::super::kernel::test_auth_store(),
      &[("active", "active-movie"), ("inactive", "inactive-movie")],
    );
    let active_key = save_profile(&kernel, &mut login_flow, "active").await;
    let inactive_key = save_profile(&kernel, &mut login_flow, "inactive").await;
    let client = connect_kernel(&mut kernel, "active");
    let session = kernel.request_gate.current_session();
    let scope = kernel
      .sdk
      .new_operation_token()
      .expect("token")
      .scope_ref()
      .expect("scope");
    let mut surface = Surface::new();

    let task = start_sign_out(&mut surface, &kernel, inactive_key, true).task;
    let completion = task_output(task).await;
    let result = apply_completion(&mut surface, &mut login_flow, &mut kernel, completion);

    assert!(result.effect.is_none());
    assert_eq!(kernel.active_profile.as_ref(), Some(&active_key));
    assert!(Arc::ptr_eq(
      kernel.client.as_ref().expect("active client"),
      &client
    ));
    assert!(client.login().is_connected());
    assert!(kernel.request_gate.is_current_session(session));
    assert!(kernel.sdk.is_scope_active(&scope));
    assert!(!content_mutations_blocked(&kernel));
    assert_eq!(login_flow.profiles.len(), 1);
    assert_eq!(login_flow.profiles[0].key(), &active_key);
    assert!(kernel.sdk_handoff.receiver.lock().await.try_recv().is_err());
    let store = WatchlistStore::for_test(settings.0.join("watchlist.json")).expect("reopen store");
    assert!(store.contains(&profile_scope("active"), "active-movie"));
    assert!(!store.contains(&profile_scope("inactive"), "inactive-movie"));
  }

  #[derive(Default)]
  struct FailingCredential {
    secret: Mutex<Option<Vec<u8>>>,
    fail_mutation: AtomicBool,
  }

  impl SecureCredential for FailingCredential {
    fn read(&self) -> Result<Vec<u8>, CredentialError> {
      self
        .secret
        .lock()
        .expect("credential lock")
        .clone()
        .ok_or(CredentialError::Missing)
    }

    fn write(&self, secret: &[u8]) -> Result<(), CredentialError> {
      if self.fail_mutation.load(Ordering::SeqCst) {
        return Err(CredentialError::WriteFailed);
      }
      *self.secret.lock().expect("credential lock") = Some(secret.to_vec());
      Ok(())
    }

    fn delete(&self) -> Result<(), CredentialError> {
      if self.fail_mutation.load(Ordering::SeqCst) {
        return Err(CredentialError::WriteFailed);
      }
      *self.secret.lock().expect("credential lock") = None;
      Ok(())
    }
  }

  #[tokio::test]
  async fn failed_protected_deletion_preserves_connection_without_starting_teardown() {
    let credential = Arc::new(FailingCredential::default());
    let (mut kernel, mut login_flow, settings) = test_kernel_with_store(
      AuthStore::with_credential(credential.clone()),
      &[("active", "movie")],
    );
    let key = save_profile(&kernel, &mut login_flow, "active").await;
    let client = connect_kernel(&mut kernel, "active");
    let session = kernel.request_gate.current_session();
    let scope = kernel
      .sdk
      .new_operation_token()
      .expect("token")
      .scope_ref()
      .expect("scope");
    credential.fail_mutation.store(true, Ordering::SeqCst);
    let mut surface = Surface::new();

    let task = start_sign_out(&mut surface, &kernel, key.clone(), true).task;
    let completion = task_output(task).await;
    assert!(matches!(
      &completion,
      Message::SignOutFinished {
        result: Err(SdkError::Storage(_)),
        ..
      }
    ));
    let result = apply_completion(&mut surface, &mut login_flow, &mut kernel, completion);

    assert!(result.effect.is_none());
    assert!(surface.error.is_some());
    assert_eq!(kernel.active_profile.as_ref(), Some(&key));
    assert!(Arc::ptr_eq(
      kernel.client.as_ref().expect("active client"),
      &client
    ));
    assert!(client.login().is_connected());
    assert!(kernel.request_gate.is_current_session(session));
    assert!(kernel.sdk.is_scope_active(&scope));
    assert!(!content_mutations_blocked(&kernel));
    assert!(!kernel.sdk.sign_out_cleanup_pending());
    assert_eq!(login_flow.profiles.len(), 1);
    assert_eq!(
      kernel
        .sdk
        .saved_profiles()
        .await
        .expect("saved profiles")
        .profiles()
        .len(),
      1
    );
    assert!(kernel.sdk_handoff.receiver.lock().await.try_recv().is_err());
    let store = WatchlistStore::for_test(settings.0.join("watchlist.json")).expect("reopen store");
    assert!(store.contains(&profile_scope("active"), "movie"));
  }

  #[tokio::test]
  async fn committed_signout_retains_auth_until_retry_receives_both_cleanup_receipts() {
    for delete_watchlist in [false, true] {
      let (mut kernel, mut login_flow, settings) = test_kernel_with_store(
        super::super::kernel::test_auth_store(),
        &[("active", "movie")],
      );
      let key = save_profile(&kernel, &mut login_flow, "active").await;
      let client = connect_kernel(&mut kernel, "active");
      let token = kernel.sdk.new_operation_token().expect("token");
      let scope = token.scope_ref().expect("scope");
      let session = kernel.request_gate.current_session();
      let mut surface = Surface::new();
      let operation = tokio::spawn(task_output(
        start_sign_out(&mut surface, &kernel, key.clone(), delete_watchlist).task,
      ));
      let failed_generation = receive_hook(&mut surface, &kernel).await;
      assert!(kernel
        .sdk
        .saved_profiles()
        .await
        .expect("saved profiles")
        .profiles()
        .is_empty());
      let _ = settle_playback_handoff(
        &mut surface,
        &kernel,
        false,
        failed_generation,
        Err("MPV cleanup failed".to_owned()),
      );

      // The SDK may already have finished, but its queued completion still
      // owns this operation generation. Retry must not supersede it.
      let generation = surface.operation.generation();
      let premature_retry = retry_handoff(&mut surface, &kernel);
      assert!(premature_retry.effect.is_none());
      assert!(iced_runtime::task::into_stream(premature_retry.task).is_none());
      assert_eq!(surface.operation.generation(), generation);
      let completion = operation.await.expect("sign-out task");
      let result = apply_completion(&mut surface, &mut login_flow, &mut kernel, completion);
      assert_eq!(
        result.effect,
        delete_watchlist.then_some(Effect::MembershipInvalidated)
      );
      assert!(login_flow.profiles.is_empty());
      assert!(kernel.sdk.sign_out_cleanup_pending());
      assert!(content_mutations_blocked(&kernel));
      assert!(client.login().is_connected());
      assert!(Arc::ptr_eq(
        kernel.client.as_ref().expect("retained client"),
        &client
      ));
      assert!(kernel.request_gate.is_current_session(session));
      assert!(kernel.sdk.is_scope_active(&scope));
      assert_eq!(
        kernel
          .sdk
          .update_user_data(
            token,
            "movie".to_owned(),
            jellypilot_media_server::VideoUserDataAction::Favorite,
          )
          .await
          .expect_err("write blocked before HTTP"),
        SdkError::OperationInProgress
      );
      let store =
        WatchlistStore::for_test(settings.0.join("watchlist.json")).expect("reopen store");
      assert_eq!(
        store.contains(&profile_scope("active"), "movie"),
        !delete_watchlist
      );

      let retry = tokio::spawn(task_output(retry_handoff(&mut surface, &kernel).task));
      let retry_generation = receive_hook(&mut surface, &kernel).await;
      assert_ne!(retry_generation, failed_generation);
      let _ = settle_remote_handoff(&mut surface, &kernel, false, failed_generation);
      let _ = settle_playback_handoff(&mut surface, &kernel, false, retry_generation, Ok(()));
      let hook = surface
        .pending_hook
        .as_ref()
        .expect("remote receipt still missing");
      assert!(!hook.remote_done);
      assert!(hook.playback_done);
      assert!(!retry.is_finished());
      assert!(client.login().is_connected());
      assert!(content_mutations_blocked(&kernel));
      let _ = settle_remote_handoff(&mut surface, &kernel, false, retry_generation);
      let completion = retry.await.expect("retry task");
      let result = apply_completion(&mut surface, &mut login_flow, &mut kernel, completion);
      assert_eq!(result.effect, Some(Effect::Disconnected));
      assert!(kernel.client.is_none());
      assert!(!client.login().is_connected());
      assert!(kernel.sdk.active_profile().is_none());
      assert!(!kernel.sdk.sign_out_cleanup_pending());
      assert!(!content_mutations_blocked(&kernel));
    }
  }

  #[tokio::test]
  async fn quit_closes_sdk_before_releasing_the_activation_hook() {
    let (mut kernel, _login_flow, _settings) = test_kernel();
    let client = connect_kernel(&mut kernel, "active");
    let mut surface = Surface::new();
    let generation = surface.begin_operation();
    let activation = start_activation(
      &mut surface,
      &kernel,
      generation,
      ProtectedCandidate::new(candidate_for("candidate")),
      false,
      None,
    );
    let operation = tokio::spawn(task_output(activation.task));
    let hook_generation = receive_hook(&mut surface, &kernel).await;
    let _ = settle_remote_handoff(&mut surface, &kernel, true, hook_generation);
    assert!(kernel.sdk.new_operation_token().is_ok());
    let _ = settle_playback_handoff(&mut surface, &kernel, true, hook_generation, Ok(()));
    assert!(matches!(
      kernel.sdk.new_operation_token(),
      Err(SdkError::Closed)
    ));
    let completion = operation.await.expect("activation task");
    assert!(matches!(
      completion,
      Message::ActivationFinished {
        result: Err(SdkError::Closed),
        ..
      }
    ));
    assert!(kernel.sdk.active_profile().is_none());
    assert!(!client.login().is_connected());
  }

  #[tokio::test]
  async fn failed_watchlist_cleanup_invalidates_membership_only_after_successful_retry() {
    let (mut kernel, mut login_flow, settings) = test_kernel_with_store(
      super::super::kernel::test_auth_store(),
      &[("active", "movie")],
    );
    let key = save_profile(&kernel, &mut login_flow, "active").await;
    connect_kernel(&mut kernel, "active");
    let watchlist_path = settings.0.join("watchlist.json");
    // A directory at the destination makes the real atomic save fail on all
    // platforms, without depending on user privileges or permission bits.
    fs::remove_file(&watchlist_path).expect("remove fixture file");
    fs::create_dir(&watchlist_path).expect("block Watchlist destination");
    let mut surface = Surface::new();
    let operation = tokio::spawn(task_output(
      start_sign_out(&mut surface, &kernel, key.clone(), true).task,
    ));
    let generation = receive_hook(&mut surface, &kernel).await;
    let _ = settle_playback_handoff(
      &mut surface,
      &kernel,
      false,
      generation,
      Err("MPV cleanup failed".to_owned()),
    );
    let completion = operation.await.expect("sign-out task");
    assert!(matches!(
      &completion,
      Message::SignOutFinished {
        result: Ok(SignOutOutcome {
          watchlist_error: Some(_),
          ..
        }),
        ..
      }
    ));
    let result = apply_completion(&mut surface, &mut login_flow, &mut kernel, completion);
    assert!(result.effect.is_none());
    assert_eq!(surface.failed_watchlist_cleanup, vec![key.clone()]);
    assert!(kernel.sdk.sign_out_cleanup_pending());

    fs::remove_dir(&watchlist_path).expect("restore Watchlist destination");
    let completion = task_output(retry_watchlist_cleanup(&mut surface, &kernel).task).await;
    let result = apply_completion(&mut surface, &mut login_flow, &mut kernel, completion);
    assert_eq!(result.effect, Some(Effect::MembershipInvalidated));
    assert!(surface.failed_watchlist_cleanup.is_empty());
    assert_eq!(kernel.active_profile.as_ref(), Some(&key));
    assert!(content_mutations_blocked(&kernel));
    let store = WatchlistStore::for_test(watchlist_path).expect("reopen store");
    assert!(!store.contains(&profile_scope("active"), "movie"));
  }

  #[test]
  fn stale_sdk_completions_preserve_a_newer_validation() {
    let (mut kernel, mut login_flow, _settings) = test_kernel();
    let mut surface = Surface::new();
    surface.operation = Operation::ValidatingSaved {
      generation: 9,
      key: SavedProfileKey::for_session(&saved_session("candidate")),
      playback_confirmed: false,
    };
    let _ = finish_sign_out(
      &mut surface,
      &mut login_flow,
      &mut kernel,
      7,
      Err(SdkError::Closed),
    );
    let _ = finish_activation(
      &mut surface,
      &mut login_flow,
      &mut kernel,
      7,
      Err(SdkError::Closed),
      false,
    );
    let _ = finish_disconnect(&mut surface, &mut kernel, 7, Ok(()));
    let _ = finish_watchlist_cleanup(
      &mut surface,
      &kernel,
      7,
      SavedProfileKey::for_session(&saved_session("active")),
      Ok(()),
    );
    assert!(matches!(
      surface.operation,
      Operation::ValidatingSaved { generation: 9, .. }
    ));
  }

  fn candidate_for(name: &str) -> ProfileCandidate {
    let client = Arc::new(jellypilot_media_server::JellyfinClient::new());
    client.login().adopt_validated_session(&saved_session(name));
    ProfileCandidate::new(
      jellypilot_auth::login::ValidatedProfileCandidate::from_authenticated_client(client)
        .unwrap_or_else(|_| panic!("complete authenticated client should become a candidate")),
    )
  }

  fn test_kernel() -> (Kernel, LoginState, TestSettingsFile) {
    test_kernel_with_store(super::super::kernel::test_auth_store(), &[])
  }

  fn test_kernel_with_store(
    auth_store: AuthStore,
    memberships: &[(&str, &str)],
  ) -> (Kernel, LoginState, TestSettingsFile) {
    use jellypilot_core::config::SettingsStore;
    use jellypilot_core::diagnostics::Diagnostics;
    use jellypilot_core::request_gate::RequestGate;
    use jellypilot_media_server::artwork::ArtworkAdapter;

    static SEQ: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
      "jellypilot-accounts-test-{}-{}",
      std::process::id(),
      SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).expect("test directory");
    let settings = SettingsStore::for_test(path.join("settings.json"));
    let login = LoginState::from_settings(settings.snapshot());
    let mut store = WatchlistStore::for_test(path.join("watchlist.json")).expect("Watchlist store");
    for (name, item_id) in memberships {
      let item = VideoLibraryItem {
        id: (*item_id).to_owned(),
        name: (*item_id).to_owned(),
        item_type: "Movie".to_owned(),
        production_year: None,
        premiere_date: None,
        community_rating: None,
        episode_count: None,
        last_played_date: None,
        runtime_seconds: None,
        played: false,
        favorite: false,
        artwork_image_id: None,
        backdrop_image_id: None,
        logo_image_id: None,
        series_poster_image_id: None,
        episode_thumb_image_id: None,
        series_thumb_image_id: None,
        series_backdrop_image_id: None,
        season_poster_image_id: None,
        season_number: None,
        episode_number: None,
        index_number_end: None,
        series_id: None,
        series_name: None,
        end_year: None,
        series_continuing: false,
        unplayed_item_count: None,
        resume_position_seconds: None,
        played_percentage: None,
        overview: None,
      };
      store
        .add(WatchlistRecord::from_item(profile_scope(name), &item, 1).expect("record"))
        .expect("seed membership");
    }
    let watchlist = super::super::personal_lists::Runtime::for_test(store);
    let (sdk, sdk_handoff) =
      super::super::kernel::test_account_runtime_with_watchlist(&auth_store, &watchlist);
    let kernel = Kernel {
      item_actions: Default::default(),
      locale: crate::i18n::Localizer::default(),
      settings,
      auth_store,
      sdk,
      sdk_handoff,
      client: None,
      connection: jellypilot_auth::login::ConnectionPhase::SignedOut,
      connected_identity: None,
      active_profile: None,
      request_gate: RequestGate::default(),
      diagnostics: Diagnostics::default(),
      notice: None,
      active_toast: None,
      next_toast_id: 0,
      tray: None,
      artwork_adapter: Arc::new(ArtworkAdapter::new()),
      avatar_adapter: Arc::new(ArtworkAdapter::new()),
      profile_avatars: Default::default(),
    };
    (kernel, login, TestSettingsFile(path))
  }

  fn profile_scope(name: &str) -> ProfileScope {
    let session = saved_session(name);
    ProfileScope::new(session.provider, session.server_url, session.user_id).expect("profile scope")
  }

  async fn save_profile(
    kernel: &Kernel,
    login_flow: &mut LoginState,
    name: &str,
  ) -> SavedProfileKey {
    let session = saved_session(name);
    let key = SavedProfileKey::for_session(&session);
    login_flow.profiles = kernel
      .auth_store
      .save_session(SensitiveSavedSession::from_saved_session(session))
      .await
      .expect("save session")
      .1;
    key
  }

  async fn task_output(task: Task<Message>) -> Message {
    let mut stream = iced_runtime::task::into_stream(task).expect("account task");
    tokio::time::timeout(Duration::from_secs(5), async move {
      while let Some(action) = stream.next().await {
        if let iced_runtime::Action::Output(message) = action {
          return message;
        }
      }
      panic!("account task completed without an output");
    })
    .await
    .expect("account task completed")
  }

  async fn receive_hook(surface: &mut Surface, kernel: &Kernel) -> u64 {
    let request = tokio::time::timeout(Duration::from_secs(5), async {
      kernel.sdk_handoff.receiver.lock().await.recv().await
    })
    .await
    .expect("handoff requested")
    .expect("handoff channel open");
    match start_hook(surface, kernel, request).effect {
      Some(Effect::BeginHandoff { generation }) => generation,
      _ => panic!("SDK operation must request physical teardown"),
    }
  }

  fn apply_completion(
    surface: &mut Surface,
    login_flow: &mut LoginState,
    kernel: &mut Kernel,
    message: Message,
  ) -> Update {
    update(
      surface,
      login_flow,
      kernel,
      RuntimeFacts {
        playback_active: false,
        quit_requested: false,
      },
      message,
    )
  }
}
