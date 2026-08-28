use std::sync::Arc;

use jellypilot_media_server::{Credentials, JellyfinClient, MediaServerProvider};
use relm4::adw::prelude::*;
use relm4::tokio::sync::watch;
use relm4::{adw, gtk, Sender};

use crate::shell::AppMessage;
use jellypilot_auth::login::{
  provider_for, quick_connect_available, should_disconnect_after_forget, ConnectionPhase,
  LoginEffect, LoginError, LoginEvent,
};
use jellypilot_auth::{AuthStorageError, AuthStore, SavedProfileKey, SavedProfileSummary};
use jellypilot_core::config::{self, LoginPrefill};
use jellypilot_core::diagnostics::{DiagnosticCategory, DiagnosticLevel};
use jellypilot_core::request_gate::{RequestGate, SessionToken};
use jellypilot_core::LoadState;

pub(crate) struct LoginPage {
  root: gtk::ScrolledWindow,
  sender: Sender<AppMessage>,
  pending_prefill: Option<PendingLoginPrefill>,
  quick_connect_phase: QuickConnectPhase,
  quick_connect_cancellation: watch::Sender<u64>,
  profile_operation_busy: bool,
  provider: adw::ComboRow,
  server_url: adw::EntryRow,
  username: adw::EntryRow,
  password: adw::PasswordEntryRow,
  remember_prefill: gtk::Switch,
  login_method_switcher: gtk::StackSwitcher,
  quick_connect_code: gtk::Label,
  quick_connect_status: gtk::Label,
  quick_connect_spinner: gtk::Spinner,
  quick_connect_button: gtk::Button,
  quick_connect_cancel_button: gtk::Button,
  saved_profiles: gtk::ListBox,
  saved_profiles_status: gtk::Label,
  login_status: gtk::Label,
  login_button: gtk::Button,
}

struct PendingLoginPrefill {
  prefill: LoginPrefill,
  provider: String,
  remember: bool,
}

pub(crate) struct LoginContext<'a> {
  pub gate: &'a mut RequestGate,
  pub saved_profiles: &'a mut LoadState<Vec<SavedProfileSummary>>,
  pub active_saved_profile: &'a mut Option<SavedProfileKey>,
  pub connection: ConnectionPhase,
  pub can_start_login: bool,
}

#[derive(Debug)]
pub(crate) enum Message {
  LoadSavedProfiles,
  LoginRequested,
  QuickConnectRequested,
  CancelQuickConnect,
  RestoreSavedProfile(SavedProfileKey),
  ForgetSavedProfile(SavedProfileKey),
  ForgetCurrentProfile,
  ConfirmForgetSavedProfile {
    key: SavedProfileKey,
    sign_out: bool,
  },
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum QuickConnectPhase {
  #[default]
  Idle,
  Requesting,
  Waiting,
  Approving,
  Failed,
}

impl QuickConnectPhase {
  const fn is_active(self) -> bool {
    matches!(self, Self::Requesting | Self::Waiting | Self::Approving)
  }
}

impl LoginPage {
  pub(crate) fn build(sender: &Sender<AppMessage>, settings: &config::Settings) -> Self {
    let prefill = settings.login_prefill();
    let provider = adw::ComboRow::new();
    provider.set_title("Server type");
    provider.set_model(Some(&gtk::StringList::new(&["Jellyfin", "Emby"])));
    provider.set_selected(if settings.login_provider().eq_ignore_ascii_case("emby") {
      1
    } else {
      0
    });
    let server_url = adw::EntryRow::new();
    server_url.set_title("Server URL");
    server_url.set_input_purpose(gtk::InputPurpose::Url);
    server_url.set_text(prefill.server_url());
    let username = adw::EntryRow::new();
    username.set_title("Username");
    username.set_input_purpose(gtk::InputPurpose::Name);
    username.set_text(prefill.username());
    let password = adw::PasswordEntryRow::new();
    password.set_title("Password");
    let remember_prefill = gtk::Switch::new();
    remember_prefill.set_active(settings.remembers_login_prefill());
    let saved_profiles = gtk::ListBox::new();
    saved_profiles.set_selection_mode(gtk::SelectionMode::None);
    saved_profiles.add_css_class("boxed-list");
    let saved_profiles_status = dim_label("Loading saved sign-ins…");
    saved_profiles_status.set_wrap(true);
    saved_profiles_status.set_accessible_role(gtk::AccessibleRole::Status);
    let login_status = dim_label("");
    login_status.set_wrap(true);
    login_status.set_visible(false);
    login_status.set_accessible_role(gtk::AccessibleRole::Status);
    let login_button = gtk::Button::with_label("Sign in");
    login_button.add_css_class("suggested-action");
    login_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Login(Message::LoginRequested))
    });
    password.connect_entry_activated({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Login(Message::LoginRequested))
    });
    let login_method_stack = gtk::Stack::builder()
      .transition_type(gtk::StackTransitionType::SlideLeftRight)
      .build();
    let login_method_switcher = gtk::StackSwitcher::new();
    login_method_switcher.set_stack(Some(&login_method_stack));
    login_method_switcher.set_halign(gtk::Align::Center);
    let quick_connect_code = gtk::Label::new(None);
    quick_connect_code.add_css_class("title-1");
    quick_connect_code.add_css_class("monospace");
    quick_connect_code.set_selectable(true);
    quick_connect_code.set_visible(false);
    let quick_connect_status = dim_label("");
    quick_connect_status.set_wrap(true);
    quick_connect_status.set_justify(gtk::Justification::Center);
    quick_connect_status.set_accessible_role(gtk::AccessibleRole::Status);
    let quick_connect_spinner = gtk::Spinner::new();
    quick_connect_spinner.set_visible(false);
    let quick_connect_button = gtk::Button::with_label("Request Quick Connect code");
    quick_connect_button.add_css_class("suggested-action");
    quick_connect_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Login(Message::QuickConnectRequested))
    });
    let quick_connect_cancel_button = gtk::Button::with_label("Cancel request");
    quick_connect_cancel_button.set_visible(false);
    quick_connect_cancel_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Login(Message::CancelQuickConnect))
    });
    let root = login_page(LoginPageWidgets {
      remember_prefill: &remember_prefill,
      provider: &provider,
      server_url: &server_url,
      username: &username,
      password: &password,
      method_stack: &login_method_stack,
      method_switcher: &login_method_switcher,
      quick_connect_code: &quick_connect_code,
      quick_connect_status: &quick_connect_status,
      quick_connect_spinner: &quick_connect_spinner,
      quick_connect: &quick_connect_button,
      cancel_quick_connect: &quick_connect_cancel_button,
      saved_profiles: &saved_profiles,
      saved_profiles_status: &saved_profiles_status,
      status: &login_status,
      sign_in: &login_button,
    });
    provider.connect_selected_notify({
      let sender = sender.clone();
      let method_stack = login_method_stack.clone();
      let method_switcher = login_method_switcher.clone();
      let quick_connect_button = quick_connect_button.clone();
      move |provider| {
        let available = quick_connect_available(provider_for(provider.selected()));
        method_switcher.set_visible(available);
        quick_connect_button.set_sensitive(available);
        if !available {
          method_stack.set_visible_child_name("password");
          sender.emit(AppMessage::Login(Message::CancelQuickConnect));
        }
      }
    });
    if !quick_connect_available(provider_for(provider.selected())) {
      login_method_switcher.set_visible(false);
      login_method_stack.set_visible_child_name("password");
      quick_connect_button.set_sensitive(false);
    }
    let (quick_connect_cancellation, _) = watch::channel(0);
    Self {
      root,
      sender: sender.clone(),
      pending_prefill: None,
      quick_connect_phase: QuickConnectPhase::Idle,
      quick_connect_cancellation,
      profile_operation_busy: false,
      provider,
      server_url,
      username,
      password,
      remember_prefill,
      login_method_switcher,
      quick_connect_code,
      quick_connect_status,
      quick_connect_spinner,
      quick_connect_button,
      quick_connect_cancel_button,
      saved_profiles,
      saved_profiles_status,
      login_status,
      login_button,
    }
  }

  pub(crate) fn root(&self) -> &gtk::ScrolledWindow {
    &self.root
  }

  pub(crate) fn handle(&mut self, message: Message, cx: &mut LoginContext<'_>) -> Vec<LoginEffect> {
    match message {
      Message::LoadSavedProfiles => self.load_saved_profiles(cx),
      Message::LoginRequested => self.start_login(cx),
      Message::QuickConnectRequested => self.start_quick_connect(cx),
      Message::CancelQuickConnect => self.cancel_quick_connect(cx),
      Message::RestoreSavedProfile(key) => self.start_saved_login(key, cx),
      Message::ForgetSavedProfile(key) => {
        self.confirm_forget_saved_profile(key, false, cx);
        Vec::new()
      }
      Message::ForgetCurrentProfile => {
        if let Some(key) = cx.active_saved_profile.clone() {
          self.confirm_forget_saved_profile(key, true, cx);
        }
        Vec::new()
      }
      Message::ConfirmForgetSavedProfile { key, sign_out } => {
        self.forget_saved_profile(key, sign_out, cx)
      }
    }
  }

  pub(crate) fn handle_event(
    &mut self,
    event: LoginEvent,
    cx: &mut LoginContext<'_>,
  ) -> Vec<LoginEffect> {
    match event {
      LoginEvent::SavedProfiles(result) => self.finish_saved_profiles(result, cx),
      LoginEvent::SavedSessionStored { session, result } => {
        self.finish_saved_session(session, result, cx)
      }
      LoginEvent::Login {
        session,
        client,
        result,
      } => self.finish_login(session, client, result, cx),
      LoginEvent::QuickConnectCode { session, code } => {
        self.finish_quick_connect_code(session, code, cx)
      }
      LoginEvent::QuickConnectApproving { session } => {
        self.finish_quick_connect_approving(session, cx)
      }
      LoginEvent::ForgotProfile {
        session,
        key,
        sign_out,
        result,
      } => self.finish_forgot_profile(session, key, sign_out, result, cx),
    }
  }

  pub(crate) fn reset_flow(&mut self) {
    self.cancel_inflight_quick_connect();
    self.quick_connect_phase = QuickConnectPhase::Idle;
    self.clear_quick_connect_surface();
    self.render_quick_connect_controls();
  }

  pub(crate) fn is_profile_busy(&self) -> bool {
    self.profile_operation_busy
  }

  pub(crate) fn set_profile_busy(&mut self, busy: bool) {
    self.profile_operation_busy = busy;
    self.saved_profiles.set_sensitive(!busy);
  }

  pub(crate) fn set_controls_sensitive(&self, sensitive: bool) {
    let sensitive = sensitive && !self.profile_operation_busy;
    self.provider.set_sensitive(sensitive);
    self.server_url.set_sensitive(sensitive);
    self.remember_prefill.set_sensitive(sensitive);
    self.username.set_sensitive(sensitive);
    self.password.set_sensitive(sensitive);
    self.login_button.set_sensitive(sensitive);
    self.saved_profiles.set_sensitive(sensitive);
    self.login_method_switcher.set_sensitive(sensitive);
    self
      .quick_connect_button
      .set_sensitive(sensitive && quick_connect_available(provider_for(self.provider.selected())));
  }

  pub(crate) fn render_saved_profiles(&self, profiles: &LoadState<Vec<SavedProfileSummary>>) {
    clear_list_box(&self.saved_profiles);
    match profiles {
      LoadState::Idle | LoadState::Loading => {
        self
          .saved_profiles_status
          .set_label("Loading saved sign-ins…");
        self.saved_profiles_status.set_visible(true);
      }
      LoadState::Failed(message) => {
        self.saved_profiles_status.set_label(message);
        self.saved_profiles_status.set_visible(true);
      }
      LoadState::Ready(profiles) if profiles.is_empty() => {
        self
          .saved_profiles_status
          .set_label("No saved sign-ins yet.");
        self.saved_profiles_status.set_visible(true);
      }
      LoadState::Ready(profiles) => {
        self.saved_profiles_status.set_visible(false);
        for profile in profiles {
          self
            .saved_profiles
            .append(&saved_profile_row(profile, &self.sender));
        }
      }
    }
  }

  pub(crate) fn set_status(&self, message: &str) {
    self.login_status.set_label(message);
    self.login_status.set_visible(true);
  }

  pub(crate) fn apply_prefill_warning(&self, warning: Option<&str>) {
    if let Some(warning) = warning {
      self.login_status.set_label(warning);
      self.login_status.set_visible(true);
    } else {
      self.login_status.set_label("");
      self.login_status.set_visible(false);
    }
  }

  pub(crate) fn server_url_text(&self) -> String {
    self.server_url.text().to_string()
  }

  pub(crate) fn username_text(&self) -> String {
    self.username.text().to_string()
  }

  fn load_saved_profiles(&mut self, cx: &mut LoginContext<'_>) -> Vec<LoginEffect> {
    *cx.saved_profiles = LoadState::Loading;
    self.render_saved_profiles(cx.saved_profiles);
    vec![LoginEffect::LoadSavedProfiles]
  }

  fn start_login(&mut self, cx: &mut LoginContext<'_>) -> Vec<LoginEffect> {
    if self.profile_operation_busy || !cx.can_start_login {
      return Vec::new();
    }
    let server_url = self.server_url.text().trim().to_owned();
    let username = self.username.text().trim().to_owned();
    let password = self.password.text().to_string();
    if server_url.is_empty() || username.is_empty() {
      self.set_status("Enter a server URL and username to continue.");
      return vec![
        diagnostic(
          DiagnosticLevel::Warning,
          DiagnosticCategory::Auth,
          "Password sign-in was rejected because the server URL or username is empty.",
        ),
        LoginEffect::InvalidInput,
      ];
    }
    let provider = if self.provider.selected() == 1 {
      "emby".to_owned()
    } else {
      "jellyfin".to_owned()
    };
    self.pending_prefill = Some(PendingLoginPrefill {
      prefill: LoginPrefill::new(server_url.clone(), username.clone()),
      provider,
      remember: self.remember_prefill.is_active(),
    });

    self.cancel_inflight_quick_connect();
    self.quick_connect_phase = QuickConnectPhase::Idle;
    self.clear_quick_connect_surface();
    self.password.set_text("");
    let session = self.begin_login(cx, "Connecting and loading your libraries…");
    vec![
      diagnostic(
        DiagnosticLevel::Info,
        DiagnosticCategory::Connection,
        "Connecting to the selected media server.",
      ),
      LoginEffect::AuthStarted,
      LoginEffect::RunPasswordAuth {
        session,
        credentials: AuthStore::protect_credentials(Credentials {
          provider: provider_for(self.provider.selected()),
          server_url,
          username,
          password,
        }),
      },
    ]
  }

  fn start_quick_connect(&mut self, cx: &mut LoginContext<'_>) -> Vec<LoginEffect> {
    if self.profile_operation_busy || !cx.can_start_login {
      return Vec::new();
    }
    if !quick_connect_available(provider_for(self.provider.selected())) {
      self.quick_connect_phase = QuickConnectPhase::Failed;
      self
        .quick_connect_status
        .set_label("Quick Connect is available only for Jellyfin. Sign in with a password.");
      self.render_quick_connect_controls();
      return vec![diagnostic(
        DiagnosticLevel::Warning,
        DiagnosticCategory::Auth,
        "Quick Connect was rejected because the selected server type does not support it.",
      )];
    }
    let server_url = self.server_url.text().trim().to_owned();
    if server_url.is_empty() {
      self.quick_connect_phase = QuickConnectPhase::Failed;
      self
        .quick_connect_status
        .set_label("Enter a Jellyfin server URL to request a code.");
      self.render_quick_connect_controls();
      return vec![diagnostic(
        DiagnosticLevel::Warning,
        DiagnosticCategory::Auth,
        "Quick Connect was rejected because the server URL is empty.",
      )];
    }
    self.pending_prefill = None;
    self.cancel_inflight_quick_connect();
    let session = self.begin_login(cx, "Requesting a Quick Connect code…");
    self.quick_connect_phase = QuickConnectPhase::Requesting;
    self.login_status.set_label("");
    self.login_status.set_visible(false);
    self.quick_connect_code.set_label("");
    self.quick_connect_code.set_visible(false);
    self.quick_connect_status.set_label("Requesting a code…");
    self.quick_connect_spinner.start();
    self.quick_connect_spinner.set_visible(true);
    self.render_quick_connect_controls();
    let cancellation = self.quick_connect_cancellation.subscribe();
    vec![
      diagnostic(
        DiagnosticLevel::Info,
        DiagnosticCategory::Auth,
        "Quick Connect request started.",
      ),
      LoginEffect::AuthStarted,
      LoginEffect::RunQuickConnect {
        session,
        server_url,
        cancellation,
      },
    ]
  }

  fn cancel_quick_connect(&mut self, cx: &mut LoginContext<'_>) -> Vec<LoginEffect> {
    if !self.quick_connect_phase.is_active() {
      return Vec::new();
    }
    self.cancel_inflight_quick_connect();
    cx.gate.disconnect();
    self.quick_connect_phase = QuickConnectPhase::Idle;
    self.set_controls_sensitive(true);
    self.login_status.set_label("");
    self.login_status.set_visible(false);
    self.clear_quick_connect_surface();
    self.render_quick_connect_controls();
    vec![
      diagnostic(
        DiagnosticLevel::Info,
        DiagnosticCategory::Auth,
        "Quick Connect request was cancelled.",
      ),
      LoginEffect::Cancelled,
    ]
  }

  fn start_saved_login(
    &mut self,
    key: SavedProfileKey,
    cx: &mut LoginContext<'_>,
  ) -> Vec<LoginEffect> {
    if self.profile_operation_busy || !cx.can_start_login {
      return Vec::new();
    }
    let Some(profile) = saved_profile_summaries(cx.saved_profiles)
      .iter()
      .find(|profile| profile.key() == &key)
      .cloned()
    else {
      self.set_status("That saved sign-in is no longer available.");
      return vec![diagnostic(
        DiagnosticLevel::Warning,
        DiagnosticCategory::Auth,
        "Saved profile restore was rejected because the profile is no longer available.",
      )];
    };
    self.cancel_inflight_quick_connect();
    self.quick_connect_phase = QuickConnectPhase::Idle;
    self.clear_quick_connect_surface();
    self.provider.set_selected(match profile.provider() {
      MediaServerProvider::Jellyfin => 0,
      MediaServerProvider::Emby => 1,
    });
    self.server_url.set_text(profile.server_url());
    self.username.set_text(profile.user_name());
    self.password.set_text("");
    self.pending_prefill = None;
    let session = self.begin_login(cx, "Restoring the saved sign-in…");
    vec![
      diagnostic(
        DiagnosticLevel::Info,
        DiagnosticCategory::Auth,
        "Saved profile restore started.",
      ),
      LoginEffect::AuthStarted,
      LoginEffect::RunRestore { session, key },
    ]
  }

  fn forget_saved_profile(
    &mut self,
    key: SavedProfileKey,
    sign_out: bool,
    cx: &mut LoginContext<'_>,
  ) -> Vec<LoginEffect> {
    if self.profile_operation_busy {
      return Vec::new();
    }
    self.set_profile_busy(true);
    self
      .saved_profiles_status
      .set_label("Forgetting saved sign-in…");
    self.saved_profiles_status.set_visible(true);
    let session = cx.gate.current_session();
    vec![
      diagnostic(
        DiagnosticLevel::Info,
        DiagnosticCategory::Auth,
        "Saved profile removal started.",
      ),
      LoginEffect::ProfileBusyChanged,
      LoginEffect::RunForget {
        session,
        key,
        sign_out,
      },
    ]
  }

  #[allow(deprecated)]
  fn confirm_forget_saved_profile(
    &self,
    key: SavedProfileKey,
    sign_out: bool,
    cx: &LoginContext<'_>,
  ) {
    if self.profile_operation_busy {
      return;
    }
    let Some(profile) = saved_profile_summaries(cx.saved_profiles)
      .iter()
      .find(|profile| profile.key() == &key)
    else {
      return;
    };
    let title = if sign_out {
      "Sign out and forget this profile?"
    } else {
      "Forget this saved sign-in?"
    };
    let dialog = gtk::MessageDialog::builder()
      .modal(true)
      .message_type(gtk::MessageType::Question)
      .text(title)
      .secondary_text(profile.forget_confirmation())
      .build();
    if let Some(window) = relm4::main_adw_application().active_window() {
      dialog.set_transient_for(Some(&window));
    }
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    let forget_button = dialog.add_button("Forget", gtk::ResponseType::Accept);
    forget_button.add_css_class("destructive-action");
    dialog.set_default_response(gtk::ResponseType::Cancel);
    dialog.connect_response({
      let sender = self.sender.clone();
      move |dialog, response| {
        dialog.close();
        if response == gtk::ResponseType::Accept {
          sender.emit(AppMessage::Login(Message::ConfirmForgetSavedProfile {
            key: key.clone(),
            sign_out,
          }));
        }
      }
    });
    dialog.present();
  }

  fn begin_login(&mut self, cx: &mut LoginContext<'_>, status: &str) -> SessionToken {
    let session = cx.gate.begin_login();
    self.set_controls_sensitive(false);
    self.set_status(status);
    session
  }

  fn finish_saved_profiles(
    &mut self,
    result: Result<Vec<SavedProfileSummary>, AuthStorageError>,
    cx: &mut LoginContext<'_>,
  ) -> Vec<LoginEffect> {
    let mut effects = Vec::new();
    *cx.saved_profiles = match result {
      Ok(profiles) => LoadState::Ready(profiles),
      Err(error) => {
        effects.push(diagnostic(
          DiagnosticLevel::Warning,
          DiagnosticCategory::Auth,
          "Saved profiles could not be loaded from Secret Service.",
        ));
        LoadState::Failed(format!("Saved sign-ins unavailable: {error}."))
      }
    };
    self.render_saved_profiles(cx.saved_profiles);
    effects
  }

  fn finish_saved_session(
    &mut self,
    session: SessionToken,
    result: Result<(SavedProfileKey, Vec<SavedProfileSummary>), AuthStorageError>,
    cx: &mut LoginContext<'_>,
  ) -> Vec<LoginEffect> {
    self.set_profile_busy(false);
    let is_current =
      cx.gate.is_current_session(session) && matches!(cx.connection, ConnectionPhase::Connected);
    let mut effects = vec![LoginEffect::ProfileBusyChanged];
    match result {
      Ok((key, profiles)) => {
        if is_current {
          *cx.active_saved_profile = Some(key);
        }
        *cx.saved_profiles = LoadState::Ready(profiles);
        effects.push(diagnostic(
          DiagnosticLevel::Info,
          DiagnosticCategory::Auth,
          "The connected session was stored in Secret Service.",
        ));
      }
      Err(_) => {
        if is_current {
          *cx.active_saved_profile = None;
        }
        effects.push(diagnostic(
          DiagnosticLevel::Warning,
          DiagnosticCategory::Auth,
          "The connected session could not be stored in Secret Service.",
        ));
      }
    }
    self.render_saved_profiles(cx.saved_profiles);
    effects
  }

  fn finish_login(
    &mut self,
    session: SessionToken,
    client: Arc<JellyfinClient>,
    result: Result<(), LoginError>,
    cx: &mut LoginContext<'_>,
  ) -> Vec<LoginEffect> {
    if !matches!(cx.connection, ConnectionPhase::Connecting) || !cx.gate.finish_login(session) {
      return Vec::new();
    }

    let was_quick_connect = self.quick_connect_phase.is_active();
    self.quick_connect_phase = if result.is_ok() {
      QuickConnectPhase::Idle
    } else if self.quick_connect_phase.is_active() {
      QuickConnectPhase::Failed
    } else {
      self.quick_connect_phase
    };
    self.quick_connect_spinner.stop();
    self.quick_connect_spinner.set_visible(false);
    if matches!(self.quick_connect_phase, QuickConnectPhase::Idle) {
      self.clear_quick_connect_surface();
    }
    self.set_controls_sensitive(true);
    self.render_quick_connect_controls();
    let pending_prefill = self.pending_prefill.take();
    match result {
      Ok(()) => {
        let mut effects = vec![
          diagnostic(
            DiagnosticLevel::Info,
            DiagnosticCategory::Connection,
            "Media server connection established.",
          ),
          diagnostic(
            DiagnosticLevel::Info,
            DiagnosticCategory::Auth,
            if was_quick_connect {
              "Quick Connect approval completed successfully."
            } else {
              "Authentication completed successfully."
            },
          ),
        ];
        if let Some(pending) = pending_prefill {
          effects.push(LoginEffect::PersistPrefill {
            prefill: pending.prefill,
            provider: pending.provider,
            remember: pending.remember,
          });
        } else {
          self.apply_prefill_warning(None);
        }
        let stored_session = client.login().get_saved_session();
        if let Some(session) = stored_session.as_ref() {
          self.server_url.set_text(&session.server_url);
          self.username.set_text(&session.user_name);
        }
        *cx.active_saved_profile = None;
        effects.push(LoginEffect::Authenticated {
          client,
          stored_session,
        });
        effects
      }
      Err(error) => {
        let message = error.to_string();
        if was_quick_connect {
          self.quick_connect_status.set_label(&message);
          self.login_status.set_label("");
          self.login_status.set_visible(false);
        } else {
          self.set_status(&message);
        }
        self.render_saved_profiles(cx.saved_profiles);
        vec![
          diagnostic(
            DiagnosticLevel::Error,
            DiagnosticCategory::Auth,
            if was_quick_connect {
              "Quick Connect failed or expired before authentication completed."
            } else {
              "Authentication failed."
            },
          ),
          diagnostic(
            DiagnosticLevel::Error,
            DiagnosticCategory::Connection,
            "Media server connection failed.",
          ),
          LoginEffect::AuthFailed { message },
        ]
      }
    }
  }

  fn finish_quick_connect_code(
    &mut self,
    session: SessionToken,
    code: String,
    cx: &mut LoginContext<'_>,
  ) -> Vec<LoginEffect> {
    if !cx.gate.is_current_login(session) {
      return Vec::new();
    }
    self.quick_connect_phase = QuickConnectPhase::Waiting;
    self.quick_connect_code.set_label(&code);
    self
      .quick_connect_code
      .update_property(&[gtk::accessible::Property::Label(&format!(
        "Quick Connect code: {code}"
      ))]);
    self.quick_connect_code.set_visible(true);
    self
      .quick_connect_status
      .set_label("Waiting for approval in another signed-in Jellyfin client…");
    self.quick_connect_spinner.start();
    self.quick_connect_spinner.set_visible(true);
    self.render_quick_connect_controls();
    vec![diagnostic(
      DiagnosticLevel::Info,
      DiagnosticCategory::Auth,
      "Quick Connect code received; waiting for approval.",
    )]
  }

  fn finish_quick_connect_approving(
    &mut self,
    session: SessionToken,
    cx: &mut LoginContext<'_>,
  ) -> Vec<LoginEffect> {
    if !cx.gate.is_current_login(session) {
      return Vec::new();
    }
    self.quick_connect_phase = QuickConnectPhase::Approving;
    self.quick_connect_status.set_label("Approved. Signing in…");
    self.render_quick_connect_controls();
    vec![diagnostic(
      DiagnosticLevel::Info,
      DiagnosticCategory::Auth,
      "Quick Connect was approved; authentication is finishing.",
    )]
  }

  fn finish_forgot_profile(
    &mut self,
    session: SessionToken,
    key: SavedProfileKey,
    sign_out: bool,
    result: Result<Vec<SavedProfileSummary>, AuthStorageError>,
    cx: &mut LoginContext<'_>,
  ) -> Vec<LoginEffect> {
    let disconnect_current_session = should_disconnect_after_forget(
      sign_out,
      session,
      cx.gate.current_session(),
      cx.connection,
      cx.active_saved_profile.as_ref() == Some(&key),
    );
    self.set_profile_busy(false);
    let mut effects = vec![LoginEffect::ProfileBusyChanged];
    match result {
      Ok(profiles) => {
        if cx.active_saved_profile.as_ref() == Some(&key) {
          *cx.active_saved_profile = None;
        }
        *cx.saved_profiles = LoadState::Ready(profiles);
        self
          .saved_profiles_status
          .set_label("Saved sign-in forgotten.");
        self.saved_profiles_status.set_visible(true);
        self.render_saved_profiles(cx.saved_profiles);
        if disconnect_current_session {
          effects.push(LoginEffect::Disconnect);
        }
        effects.push(diagnostic(
          DiagnosticLevel::Info,
          DiagnosticCategory::Auth,
          "Saved profile removal completed.",
        ));
      }
      Err(error) => {
        self
          .saved_profiles_status
          .set_label(&format!("Saved sign-in could not be forgotten: {error}."));
        self.saved_profiles_status.set_visible(true);
        effects.push(diagnostic(
          DiagnosticLevel::Warning,
          DiagnosticCategory::Auth,
          "Saved profile removal failed in Secret Service.",
        ));
      }
    }
    effects
  }

  fn cancel_inflight_quick_connect(&self) {
    let next = (*self.quick_connect_cancellation.borrow()).wrapping_add(1);
    let _ = self.quick_connect_cancellation.send_replace(next);
  }

  fn clear_quick_connect_surface(&self) {
    self.quick_connect_code.set_label("");
    self.quick_connect_code.set_visible(false);
    self.quick_connect_status.set_label("");
    self.quick_connect_spinner.stop();
    self.quick_connect_spinner.set_visible(false);
  }

  fn render_quick_connect_controls(&self) {
    let provider_supported = quick_connect_available(provider_for(self.provider.selected()));
    let failed = matches!(self.quick_connect_phase, QuickConnectPhase::Failed);
    self.login_method_switcher.set_visible(provider_supported);
    self.quick_connect_button.set_visible(matches!(
      self.quick_connect_phase,
      QuickConnectPhase::Idle | QuickConnectPhase::Failed
    ));
    self.quick_connect_button.set_label(
      if matches!(self.quick_connect_phase, QuickConnectPhase::Failed) {
        "Request a new code"
      } else {
        "Request Quick Connect code"
      },
    );
    self
      .quick_connect_cancel_button
      .set_visible(self.quick_connect_phase.is_active());
    self.quick_connect_code.set_visible(matches!(
      self.quick_connect_phase,
      QuickConnectPhase::Waiting | QuickConnectPhase::Approving
    ));
    self.quick_connect_status.set_accessible_role(if failed {
      gtk::AccessibleRole::Alert
    } else {
      gtk::AccessibleRole::Status
    });
    if failed {
      self.quick_connect_status.add_css_class("error");
    } else {
      self.quick_connect_status.remove_css_class("error");
    }
  }
}

fn diagnostic(
  level: DiagnosticLevel,
  category: DiagnosticCategory,
  message: impl Into<String>,
) -> LoginEffect {
  LoginEffect::Diagnostic(level, category, message.into())
}

fn saved_profile_summaries(
  saved_profiles: &LoadState<Vec<SavedProfileSummary>>,
) -> &[SavedProfileSummary] {
  match saved_profiles {
    LoadState::Ready(profiles) => profiles,
    LoadState::Idle | LoadState::Loading | LoadState::Failed(_) => &[],
  }
}

struct LoginPageWidgets<'a> {
  remember_prefill: &'a gtk::Switch,
  provider: &'a adw::ComboRow,
  server_url: &'a adw::EntryRow,
  username: &'a adw::EntryRow,
  password: &'a adw::PasswordEntryRow,
  method_stack: &'a gtk::Stack,
  method_switcher: &'a gtk::StackSwitcher,
  quick_connect_code: &'a gtk::Label,
  quick_connect_status: &'a gtk::Label,
  quick_connect_spinner: &'a gtk::Spinner,
  quick_connect: &'a gtk::Button,
  cancel_quick_connect: &'a gtk::Button,
  saved_profiles: &'a gtk::ListBox,
  saved_profiles_status: &'a gtk::Label,
  status: &'a gtk::Label,
  sign_in: &'a gtk::Button,
}

fn login_page(widgets: LoginPageWidgets<'_>) -> gtk::ScrolledWindow {
  let LoginPageWidgets {
    remember_prefill,
    provider,
    server_url,
    username,
    password,
    method_stack,
    method_switcher,
    quick_connect_code,
    quick_connect_status,
    quick_connect_spinner,
    quick_connect,
    cancel_quick_connect,
    saved_profiles,
    saved_profiles_status,
    status,
    sign_in,
  } = widgets;
  let page = gtk::Box::new(gtk::Orientation::Vertical, 24);
  page.set_halign(gtk::Align::Center);
  page.set_margin_top(32);
  page.set_margin_bottom(32);
  let branding = gtk::Box::new(gtk::Orientation::Vertical, 8);
  branding.set_halign(gtk::Align::Center);
  let icon = gtk::Image::from_icon_name("video-x-generic-symbolic");
  icon.set_pixel_size(48);
  branding.append(&icon);
  let title = gtk::Label::new(Some("JellyPilot"));
  title.add_css_class("title-1");
  branding.append(&title);
  let subtitle = dim_label("Connect to your Jellyfin or Emby server.");
  subtitle.set_justify(gtk::Justification::Center);
  subtitle.set_wrap(true);
  branding.append(&subtitle);
  page.append(&branding);

  let server_group = adw::PreferencesGroup::new();
  server_group.set_title("Server");
  server_group.add(provider);
  server_group.add(server_url);
  page.append(&server_group);
  method_switcher.set_halign(gtk::Align::Center);
  page.append(method_switcher);

  let quick_connect_page = gtk::Box::new(gtk::Orientation::Vertical, 14);
  let quick_copy = dim_label(
    "Request a code, then approve it from another client already signed in to this Jellyfin server.",
  );
  quick_copy.set_wrap(true);
  quick_copy.set_justify(gtk::Justification::Center);
  quick_connect_page.append(&quick_copy);
  quick_connect_code.set_halign(gtk::Align::Center);
  quick_connect_page.append(quick_connect_code);
  quick_connect_spinner.set_halign(gtk::Align::Center);
  quick_connect_page.append(quick_connect_spinner);
  quick_connect_status.set_justify(gtk::Justification::Center);
  quick_connect_status.set_wrap(true);
  quick_connect_page.append(quick_connect_status);
  quick_connect.set_halign(gtk::Align::Center);
  quick_connect_page.append(quick_connect);
  cancel_quick_connect.set_halign(gtk::Align::Center);
  quick_connect_page.append(cancel_quick_connect);
  method_stack.add_titled(&quick_connect_page, Some("quick-connect"), "Quick Connect");

  let password_page = gtk::Box::new(gtk::Orientation::Vertical, 14);
  let credentials_group = adw::PreferencesGroup::new();
  credentials_group.add(username);
  credentials_group.add(password);
  let remember_row = adw::ActionRow::new();
  remember_row.set_title("Remember sign-in details");
  remember_row.set_subtitle("Save server, provider, and username on this device.");
  remember_row.add_suffix(remember_prefill);
  remember_row.set_activatable_widget(Some(remember_prefill));
  credentials_group.add(&remember_row);
  password_page.append(&credentials_group);
  let storage_copy = dim_label(
    "Successful sign-ins are saved in Linux Secret Service. JellyPilot never stores your password.",
  );
  storage_copy.set_wrap(true);
  password_page.append(&storage_copy);
  sign_in.set_hexpand(true);
  password_page.append(sign_in);
  method_stack.add_titled(&password_page, Some("password"), "Password");
  method_stack.set_visible_child_name("quick-connect");
  page.append(method_stack);
  status.set_halign(gtk::Align::Center);
  status.set_wrap(true);
  page.append(status);

  let saved_group = adw::PreferencesGroup::new();
  saved_group.set_title("Saved sign-ins");
  let saved_profiles_scroll = gtk::ScrolledWindow::builder()
    .child(saved_profiles)
    .max_content_height(240)
    .propagate_natural_height(true)
    .hscrollbar_policy(gtk::PolicyType::Never)
    .build();
  saved_group.add(&saved_profiles_scroll);
  saved_group.add(saved_profiles_status);
  page.append(&saved_group);

  let clamp = adw::Clamp::new();
  clamp.set_maximum_size(620);
  clamp.set_child(Some(&page));
  gtk::ScrolledWindow::builder()
    .child(&clamp)
    .hscrollbar_policy(gtk::PolicyType::Never)
    .vexpand(true)
    .build()
}

fn saved_profile_row(profile: &SavedProfileSummary, sender: &Sender<AppMessage>) -> adw::ActionRow {
  let action = adw::ActionRow::new();
  action.set_title(&profile.title());
  action.set_subtitle(profile.subtitle());
  action.set_activatable(true);
  action.update_property(&[gtk::accessible::Property::Label(
    &profile.restore_accessibility_label(),
  )]);
  let key = profile.key().clone();
  let sender_clone = sender.clone();
  action.connect_activated(move |_| {
    sender_clone.emit(AppMessage::Login(Message::RestoreSavedProfile(key.clone())));
  });
  let forget = gtk::Button::with_label("Forget");
  forget.add_css_class("destructive-action");
  forget.update_property(&[gtk::accessible::Property::Label(
    &profile.forget_accessibility_label(),
  )]);
  let key = profile.key().clone();
  let sender_clone = sender.clone();
  forget.connect_clicked(move |_| {
    sender_clone.emit(AppMessage::Login(Message::ForgetSavedProfile(key.clone())));
  });
  action.add_suffix(&forget);
  action
}

fn dim_label(text: &str) -> gtk::Label {
  let label = gtk::Label::new(Some(text));
  label.add_css_class("dim-label");
  label.set_xalign(0.0);
  label
}

fn clear_list_box(container: &gtk::ListBox) {
  while let Some(child) = container.first_child() {
    container.remove(&child);
  }
}
