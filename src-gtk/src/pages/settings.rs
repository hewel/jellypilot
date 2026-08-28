use std::cell::Cell;
use std::rc::Rc;

use jellypilot_mpv::{find_mpv, write_input_conf};
use relm4::adw::prelude::*;
use relm4::{adw, gtk, Sender};

use crate::pages::login;
use crate::shell::AppMessage;
use jellypilot_auth::login::ConnectionPhase;
use jellypilot_auth::{SavedProfileKey, SavedProfileSummary};
use jellypilot_core::config::{self, Settings, SettingsMutationError, SettingsStore, ShortcutKind};
use jellypilot_core::diagnostics::{DiagnosticCategory, DiagnosticLevel};
use jellypilot_core::settings::{
  config_intro_mode, format_byte_count, intro_mode_selection, SUBTITLE_LANGUAGE_OPTIONS,
};
use jellypilot_core::LoadState;
use jellypilot_media_server::ArtworkCacheStats;

pub(crate) struct SettingsPage {
  root: adw::PreferencesDialog,
  sender: Sender<AppMessage>,
  config_status: gtk::Label,
  server_url: gtk::Label,
  user: gtk::Label,
  remote_status: gtk::Label,
  disconnect: gtk::Button,
  reconnect: gtk::Button,
  refresh_status: gtk::Button,
  saved_profile: gtk::Label,
  storage_status: gtk::Label,
  forget: gtk::Button,
  mpv_path: adw::EntryRow,
  mpv_status: gtk::Label,
  subtitle_languages: gtk::Box,
  subtitle_preset: gtk::DropDown,
  subtitle_custom: adw::EntryRow,
  image_cache: adw::SwitchRow,
  image_cache_syncing: Rc<Cell<bool>>,
  image_cache_stats: gtk::Label,
  image_cache_clear: gtk::Button,
  intro_skip_group: adw::PreferencesGroup,
  intro_skip_mode: adw::ComboRow,
  intro_skip_status: gtk::Label,
}

pub(crate) struct SettingsContext {
  pub intro_mode: config::IntroMode,
  pub image_cache_clearing: bool,
  pub connected: bool,
}

pub(crate) struct ConnectionView<'a> {
  pub connected: bool,
  pub server_url: &'a str,
  pub user: &'a str,
  pub remote_status: &'a str,
  pub reconnect_sensitive: bool,
  pub refresh_sensitive: bool,
}

#[derive(Debug)]
pub(crate) enum Message {
  SetIntroMode(u32),
  ReconnectRemoteControl,
  RefreshConnectionStatus,
  DetectMpv,
  SetMpvPath(String),
  SetMpvArgs(String),
  SetPlaybackTargetName(String),
  AddSubtitlePreset,
  AddSubtitleCustom,
  MoveSubtitleLanguage { index: usize, offset: i32 },
  RemoveSubtitleLanguage(usize),
  ClearSubtitleLanguages,
  SetNextEpisodeKey(String),
  SetPreviousEpisodeKey(String),
  SetIntroSkipKey(String),
  SetImageCacheEnabled(bool),
  RefreshImageCacheStats,
  ConfirmClearImageCache,
  ClearImageCache,
}

pub(crate) enum SettingsEvent {
  ImageCacheStats(Result<ArtworkCacheStats, ()>),
  ImageCacheCleared(Result<ArtworkCacheStats, ()>),
  ConnectionStatus(Result<(), ()>),
}

pub(crate) enum SettingsEffect {
  ReconfigurePlayback,
  IntroModeChanged(config::IntroMode),
  ReconnectRemoteControl,
  RefreshConnectionStatus,
  SetImageCacheEnabled(bool),
  RefreshImageCacheStats,
  ClearImageCache,
  Diagnostic(DiagnosticLevel, DiagnosticCategory, String),
}

impl SettingsPage {
  pub(crate) fn build(
    sender: &Sender<AppMessage>,
    diagnostics: &adw::PreferencesPage,
    settings: &Settings,
  ) -> Self {
    let saved_profile = dim_label("");
    saved_profile.set_wrap(true);
    let storage_status = dim_label("");
    storage_status.set_wrap(true);
    storage_status.set_visible(false);
    storage_status.set_accessible_role(gtk::AccessibleRole::Status);
    let disconnect = gtk::Button::with_label("Disconnect");
    disconnect.add_css_class("destructive-action");
    disconnect.set_halign(gtk::Align::Start);
    disconnect.connect_clicked({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Disconnect)
    });
    let intro_skip_group = adw::PreferencesGroup::new();
    intro_skip_group.set_title("Intro Skip");
    intro_skip_group.set_description(Some(
      "Automatic skips detected ranges. Manual shows an MPV prompt. Off does not fetch or apply ranges.",
    ));
    intro_skip_group.set_visible(false);
    let intro_skip_mode = adw::ComboRow::new();
    intro_skip_mode.set_title("Mode");
    intro_skip_mode.set_subtitle("Changes apply when playback next (re)starts in MPV.");
    intro_skip_mode.set_model(Some(&gtk::StringList::new(&["Automatic", "Manual", "Off"])));
    intro_skip_mode.set_selected(intro_mode_selection(settings.intro_mode()));
    intro_skip_mode.connect_selected_notify({
      let sender = sender.clone();
      move |row| sender.emit(AppMessage::Settings(Message::SetIntroMode(row.selected())))
    });
    intro_skip_group.add(&intro_skip_mode);
    let intro_skip_status = dim_label("");
    intro_skip_status.set_wrap(true);
    intro_skip_status.set_visible(false);
    let config_status = dim_label("");
    config_status.set_wrap(true);
    config_status.set_visible(false);
    config_status.set_accessible_role(gtk::AccessibleRole::Status);
    let server_url = dim_label("Not connected");
    server_url.set_selectable(true);
    server_url.set_wrap(true);
    let user = dim_label("No authenticated user");
    user.set_selectable(true);
    let remote_status = dim_label("Remote Control unavailable");
    let reconnect = gtk::Button::with_label("Reconnect remote control");
    reconnect.set_sensitive(false);
    reconnect.connect_clicked({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Settings(Message::ReconnectRemoteControl))
    });
    let refresh_status = gtk::Button::with_label("Refresh status");
    refresh_status.set_sensitive(false);
    refresh_status.connect_clicked({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Settings(Message::RefreshConnectionStatus))
    });
    let mpv_path = adw::EntryRow::new();
    mpv_path.set_title("MPV path");
    mpv_path.set_text(settings.mpv_path().unwrap_or(""));
    mpv_path.connect_changed({
      let sender = sender.clone();
      move |entry| {
        sender.emit(AppMessage::Settings(Message::SetMpvPath(
          entry.text().to_string(),
        )))
      }
    });
    let detect_mpv = gtk::Button::with_label("Detect");
    detect_mpv.connect_clicked({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Settings(Message::DetectMpv))
    });
    let mpv_status = dim_label("");
    mpv_status.set_wrap(true);
    mpv_status.set_visible(false);
    mpv_status.set_accessible_role(gtk::AccessibleRole::Status);
    let mpv_args = adw::EntryRow::new();
    mpv_args.set_title("Advanced MPV arguments");
    mpv_args.set_text(&settings.mpv_args().join(" "));
    mpv_args.connect_changed({
      let sender = sender.clone();
      move |entry| {
        sender.emit(AppMessage::Settings(Message::SetMpvArgs(
          entry.text().to_string(),
        )))
      }
    });
    let target_name = adw::EntryRow::new();
    target_name.set_title("Playback Target name");
    target_name.set_text(settings.playback_target_name().unwrap_or(""));
    target_name.connect_changed({
      let sender = sender.clone();
      move |entry| {
        sender.emit(AppMessage::Settings(Message::SetPlaybackTargetName(
          entry.text().to_string(),
        )))
      }
    });
    let subtitle_languages = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let subtitle_preset = gtk::DropDown::from_strings(&SUBTITLE_LANGUAGE_OPTIONS);
    let subtitle_preset_add = gtk::Button::with_label("Add selected");
    subtitle_preset_add.connect_clicked({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Settings(Message::AddSubtitlePreset))
    });
    let subtitle_custom = adw::EntryRow::new();
    subtitle_custom.set_title("Custom language code");
    subtitle_custom.connect_entry_activated({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Settings(Message::AddSubtitleCustom))
    });
    let subtitle_custom_add = gtk::Button::with_label("Add custom");
    subtitle_custom_add.connect_clicked({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Settings(Message::AddSubtitleCustom))
    });
    let subtitle_clear = gtk::Button::with_label("Clear all");
    subtitle_clear.add_css_class("destructive-action");
    subtitle_clear.connect_clicked({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Settings(Message::ClearSubtitleLanguages))
    });
    let key_next = adw::EntryRow::new();
    key_next.set_title("Next episode");
    key_next.set_text(settings.key_next_episode());
    key_next.connect_changed({
      let sender = sender.clone();
      move |entry| {
        sender.emit(AppMessage::Settings(Message::SetNextEpisodeKey(
          entry.text().to_string(),
        )))
      }
    });
    let key_previous = adw::EntryRow::new();
    key_previous.set_title("Previous episode");
    key_previous.set_text(settings.key_previous_episode());
    key_previous.connect_changed({
      let sender = sender.clone();
      move |entry| {
        sender.emit(AppMessage::Settings(Message::SetPreviousEpisodeKey(
          entry.text().to_string(),
        )))
      }
    });
    let key_intro = adw::EntryRow::new();
    key_intro.set_title("Skip intro");
    key_intro.set_text(settings.key_intro_skip());
    key_intro.connect_changed({
      let sender = sender.clone();
      move |entry| {
        sender.emit(AppMessage::Settings(Message::SetIntroSkipKey(
          entry.text().to_string(),
        )))
      }
    });
    let image_cache_syncing = Rc::new(Cell::new(false));
    let image_cache = adw::SwitchRow::new();
    image_cache.set_title("Disk Library Image Cache");
    image_cache.set_subtitle(
      "Stores original server image bytes for faster repeat browsing; never used as offline truth.",
    );
    image_cache.set_active(settings.image_cache_enabled());
    image_cache.connect_active_notify({
      let sender = sender.clone();
      let syncing = Rc::clone(&image_cache_syncing);
      move |row| {
        if !syncing.get() {
          sender.emit(AppMessage::Settings(Message::SetImageCacheEnabled(
            row.is_active(),
          )));
        }
      }
    });
    let image_cache_stats = dim_label("Cache statistics have not been computed.");
    image_cache_stats.set_wrap(true);
    image_cache_stats.set_accessible_role(gtk::AccessibleRole::Status);
    let image_cache_clear = gtk::Button::with_label("Clear Library Image Cache");
    image_cache_clear.add_css_class("destructive-action");
    image_cache_clear.connect_clicked({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Settings(Message::ConfirmClearImageCache))
    });
    intro_skip_status.set_accessible_role(gtk::AccessibleRole::Status);
    intro_skip_group.add(&intro_skip_status);
    let forget = gtk::Button::with_label("Sign out and forget");
    forget.add_css_class("destructive-action");
    forget.set_sensitive(false);
    forget.connect_clicked({
      let sender = sender.clone();
      move |_| {
        // The model resolves the current key at message time so stale widgets never retain tokens.
        sender.emit(AppMessage::Login(login::Message::ForgetCurrentProfile))
      }
    });
    let root = settings_page(
      SettingsPageWidgets {
        config_status: &config_status,
        server_url: &server_url,
        user: &user,
        remote_status: &remote_status,
        disconnect: &disconnect,
        reconnect: &reconnect,
        refresh_status: &refresh_status,
        saved_profile: &saved_profile,
        storage_status: &storage_status,
        forget_saved_profile: &forget,
        mpv_path: &mpv_path,
        detect_mpv: &detect_mpv,
        mpv_status: &mpv_status,
        mpv_args: &mpv_args,
        target_name: &target_name,
        subtitle_languages: &subtitle_languages,
        subtitle_preset: &subtitle_preset,
        subtitle_preset_add: &subtitle_preset_add,
        subtitle_custom: &subtitle_custom,
        subtitle_custom_add: &subtitle_custom_add,
        subtitle_clear: &subtitle_clear,
        key_next: &key_next,
        key_previous: &key_previous,
        key_intro: &key_intro,
        image_cache: &image_cache,
        image_cache_stats: &image_cache_stats,
        image_cache_clear: &image_cache_clear,
        intro_skip: &intro_skip_group,
      },
      diagnostics,
    );
    let application = relm4::main_adw_application();
    let preferences_action = gtk::gio::SimpleAction::new("preferences", None);
    preferences_action.connect_activate({
      let preferences = root.clone();
      let sender = sender.clone();
      move |_, _| {
        sender.emit(AppMessage::Diagnostics(
          super::diagnostics::Message::Refresh,
        ));
        sender.emit(AppMessage::Settings(Message::RefreshImageCacheStats));
        let parent = relm4::main_adw_application().active_window();
        preferences.present(parent.as_ref());
      }
    });
    application.add_action(&preferences_action);
    let page = Self {
      root,
      sender: sender.clone(),
      config_status,
      server_url,
      user,
      remote_status,
      disconnect,
      reconnect,
      refresh_status,
      saved_profile,
      storage_status,
      forget,
      mpv_path,
      mpv_status,
      subtitle_languages,
      subtitle_preset,
      subtitle_custom,
      image_cache,
      image_cache_syncing,
      image_cache_stats,
      image_cache_clear,
      intro_skip_group,
      intro_skip_mode,
      intro_skip_status,
    };
    page.render_subtitle_settings(settings);
    page
  }

  pub(crate) fn root(&self) -> &adw::PreferencesDialog {
    &self.root
  }

  pub(crate) fn close(&self) {
    self.root().close();
  }

  pub(crate) fn set_intro_skip_visible(&self, visible: bool) {
    self.intro_skip_group.set_visible(visible);
  }

  pub(crate) fn set_disconnect_sensitive(&self, sensitive: bool) {
    self.disconnect.set_sensitive(sensitive);
  }

  pub(crate) fn set_forget_sensitive(&self, sensitive: bool) {
    self.forget.set_sensitive(sensitive);
  }

  pub(crate) fn set_config_status(&self, message: &str) {
    self.config_status.set_label(message);
    self.config_status.set_visible(true);
  }

  pub(crate) fn set_storage_status(&self, message: &str, visible: bool) {
    self.storage_status.set_label(message);
    self.storage_status.set_visible(visible);
  }

  pub(crate) fn handle(
    &mut self,
    message: Message,
    cx: &SettingsContext,
    settings: &mut SettingsStore,
  ) -> Vec<SettingsEffect> {
    match message {
      Message::SetIntroMode(selected) => self.set_intro_mode(selected, cx, settings),
      Message::ReconnectRemoteControl => vec![SettingsEffect::ReconnectRemoteControl],
      Message::RefreshConnectionStatus => {
        if !cx.connected {
          return Vec::new();
        }
        self.set_config_status("Refreshing connection status…");
        vec![SettingsEffect::RefreshConnectionStatus]
      }
      Message::DetectMpv => self.detect_mpv(),
      Message::SetMpvPath(path) => self.update_mpv_path(path, settings),
      Message::SetMpvArgs(args) => self.update_mpv_args(args, settings),
      Message::SetPlaybackTargetName(name) => self.update_playback_target_name(name, settings),
      Message::AddSubtitlePreset => self.add_subtitle_preset(settings),
      Message::AddSubtitleCustom => self.add_custom_subtitle(settings),
      Message::MoveSubtitleLanguage { index, offset } => {
        self.move_subtitle_language(index, offset, settings)
      }
      Message::RemoveSubtitleLanguage(index) => self.remove_subtitle_language(index, settings),
      Message::ClearSubtitleLanguages => self.clear_subtitle_languages(settings),
      Message::SetNextEpisodeKey(key) => self.update_shortcut(ShortcutKind::Next, key, settings),
      Message::SetPreviousEpisodeKey(key) => {
        self.update_shortcut(ShortcutKind::Previous, key, settings)
      }
      Message::SetIntroSkipKey(key) => self.update_shortcut(ShortcutKind::IntroSkip, key, settings),
      Message::SetImageCacheEnabled(enabled) => self.set_image_cache_enabled(enabled, settings),
      Message::RefreshImageCacheStats => {
        if cx.image_cache_clearing {
          return Vec::new();
        }
        self
          .image_cache_stats
          .set_label("Computing cache statistics…");
        self.image_cache_clear.set_sensitive(false);
        vec![SettingsEffect::RefreshImageCacheStats]
      }
      Message::ConfirmClearImageCache => self.confirm_clear_image_cache(cx),
      Message::ClearImageCache => {
        if cx.image_cache_clearing {
          return Vec::new();
        }
        self
          .image_cache_stats
          .set_label("Clearing Library Image Cache…");
        self.image_cache_clear.set_sensitive(false);
        vec![SettingsEffect::ClearImageCache]
      }
    }
  }

  pub(crate) fn handle_event(&self, event: SettingsEvent, cx: &SettingsContext) {
    match event {
      SettingsEvent::ConnectionStatus(result) => {
        self.set_config_status(match result {
          Ok(()) => "Connection status refreshed.",
          Err(()) => "Connection status refresh failed.",
        });
      }
      SettingsEvent::ImageCacheStats(result) => match result {
        Ok(stats) => self.render_image_cache_stats(stats, cx.image_cache_clearing),
        Err(()) => {
          self
            .image_cache_stats
            .set_label("Cache statistics are unavailable.");
          self.image_cache_clear.set_sensitive(false);
        }
      },
      SettingsEvent::ImageCacheCleared(result) => match result {
        Ok(stats) => self.render_image_cache_stats(stats, cx.image_cache_clearing),
        Err(()) => {
          self
            .image_cache_stats
            .set_label("Library Image Cache could not be cleared.");
          self.image_cache_clear.set_sensitive(true);
        }
      },
    }
  }

  pub(crate) fn render_connection(&self, view: &ConnectionView<'_>) {
    let server_url = if view.connected {
      match view.server_url.trim() {
        "" => "Connected server URL unavailable",
        server_url => server_url,
      }
    } else {
      "Not connected"
    };
    self
      .server_url
      .set_label(&format!("Server URL: {server_url}"));
    let user = if view.connected {
      match view.user.trim() {
        "" => "Authenticated user unavailable",
        user => user,
      }
    } else {
      "No authenticated user"
    };
    self.user.set_label(&format!("User: {user}"));
    self.remote_status.set_label(view.remote_status);
    self.reconnect.set_sensitive(view.reconnect_sensitive);
    self.refresh_status.set_sensitive(view.refresh_sensitive);
  }

  pub(crate) fn render_profiles(
    &self,
    saved_profiles: &LoadState<Vec<SavedProfileSummary>>,
    active: &Option<SavedProfileKey>,
    connection: ConnectionPhase,
    busy: bool,
  ) {
    self.storage_status.set_visible(false);
    if !matches!(connection, ConnectionPhase::Connected) {
      self
        .saved_profile
        .set_label("No active session. Sign in to manage this device's saved profile.");
      self.disconnect.set_sensitive(false);
      self.forget.set_sensitive(false);
      return;
    }

    let summaries = match saved_profiles {
      LoadState::Ready(profiles) => profiles.as_slice(),
      LoadState::Idle | LoadState::Loading | LoadState::Failed(_) => &[],
    };
    let profile = active
      .as_ref()
      .and_then(|key| summaries.iter().find(|profile| &profile.key == key));
    if let Some(profile) = profile {
      self.saved_profile.set_label(&format!(
        "Signed in as {} on {}. The session token is stored in Linux Secret Service; the password is not saved.",
        profile.user_name,
        profile
          .server_name
          .as_deref()
          .unwrap_or(profile.server_url.as_str())
      ));
      self.forget.set_sensitive(!busy);
      self
        .forget
        .update_property(&[gtk::accessible::Property::Label(&format!(
          "Sign out and forget saved sign-in for {} on {}",
          profile.user_name,
          profile
            .server_name
            .as_deref()
            .unwrap_or(profile.server_url.as_str())
        ))]);
      self.storage_status.set_visible(true);
    } else {
      self
        .saved_profile
        .set_label("This active session is not saved. Passwords are never stored by the GTK app.");
      self.forget.set_sensitive(false);
      if matches!(saved_profiles, LoadState::Failed(_)) {
        self
          .storage_status
          .set_label("Linux Secret Service is unavailable or locked.");
        self.storage_status.set_visible(true);
      }
    }
  }

  fn set_intro_mode(
    &self,
    selected: u32,
    cx: &SettingsContext,
    settings: &mut SettingsStore,
  ) -> Vec<SettingsEffect> {
    let mode = config_intro_mode(selected);
    match settings.set_intro_mode(mode) {
      Ok(changed) => {
        self.intro_skip_status.set_label("");
        self.intro_skip_status.set_visible(false);
        if changed {
          vec![
            SettingsEffect::IntroModeChanged(mode),
            diagnostic(
              DiagnosticLevel::Info,
              DiagnosticCategory::Config,
              "Intro Skip preference was saved.",
            ),
          ]
        } else {
          Vec::new()
        }
      }
      Err(_) => {
        self
          .intro_skip_status
          .set_label("The Intro Skip preference could not be saved.");
        self.intro_skip_status.set_visible(true);
        self
          .intro_skip_mode
          .set_selected(intro_mode_selection(cx.intro_mode));
        vec![diagnostic(
          DiagnosticLevel::Warning,
          DiagnosticCategory::Config,
          "The Intro Skip preference could not be saved.",
        )]
      }
    }
  }

  fn detect_mpv(&self) -> Vec<SettingsEffect> {
    match find_mpv() {
      Some(path) => {
        self.mpv_path.set_text(&path.to_string_lossy());
        self
          .mpv_status
          .set_label("MPV detected. The path applies on the next MPV start.");
        self.mpv_status.set_visible(true);
        Vec::new()
      }
      None => {
        self
          .mpv_status
          .set_label("MPV was not found in PATH or a standard install location.");
        self.mpv_status.set_visible(true);
        vec![diagnostic(
          DiagnosticLevel::Warning,
          DiagnosticCategory::Playback,
          "MPV detection from Settings did not find an executable.",
        )]
      }
    }
  }

  fn update_mpv_path(&self, path: String, settings: &mut SettingsStore) -> Vec<SettingsEffect> {
    let result = settings.set_mpv_path(path);
    self.save_and_reconfigure(result)
  }

  fn update_mpv_args(&self, args: String, settings: &mut SettingsStore) -> Vec<SettingsEffect> {
    let result = settings.set_mpv_args(&args);
    self.save_and_reconfigure(result)
  }

  fn update_playback_target_name(
    &self,
    name: String,
    settings: &mut SettingsStore,
  ) -> Vec<SettingsEffect> {
    match self.save_application_config(settings.set_playback_target_name(name)) {
      Ok(_) => Vec::new(),
      Err(effect) => vec![effect],
    }
  }

  fn add_subtitle_preset(&self, settings: &mut SettingsStore) -> Vec<SettingsEffect> {
    let selected = self.subtitle_preset.selected() as usize;
    let Some(language) = SUBTITLE_LANGUAGE_OPTIONS.get(selected) else {
      return Vec::new();
    };
    self.add_subtitle_language((*language).to_owned(), settings)
  }

  fn add_custom_subtitle(&self, settings: &mut SettingsStore) -> Vec<SettingsEffect> {
    let language = self.subtitle_custom.text().to_string();
    let effects = self.add_subtitle_language(language, settings);
    if effects
      .iter()
      .any(|effect| matches!(effect, SettingsEffect::ReconfigurePlayback))
    {
      self.subtitle_custom.set_text("");
    }
    effects
  }

  fn add_subtitle_language(
    &self,
    language: String,
    settings: &mut SettingsStore,
  ) -> Vec<SettingsEffect> {
    let result = settings.add_subtitle_language(language);
    match result {
      Err(SettingsMutationError::InvalidSubtitleLanguage) => {
        vec![self.show_failure("Enter a language code using letters, numbers, '-' or '_'.")]
      }
      Err(SettingsMutationError::DuplicateSubtitleLanguage) => {
        vec![self.show_failure("That subtitle language is already in the priority list.")]
      }
      result => self.finish_subtitle_mutation(result, settings.snapshot()),
    }
  }

  fn move_subtitle_language(
    &self,
    index: usize,
    offset: i32,
    settings: &mut SettingsStore,
  ) -> Vec<SettingsEffect> {
    let result = settings.move_subtitle_language(index, offset);
    self.finish_subtitle_mutation(result, settings.snapshot())
  }

  fn remove_subtitle_language(
    &self,
    index: usize,
    settings: &mut SettingsStore,
  ) -> Vec<SettingsEffect> {
    let result = settings.remove_subtitle_language(index);
    self.finish_subtitle_mutation(result, settings.snapshot())
  }

  fn clear_subtitle_languages(&self, settings: &mut SettingsStore) -> Vec<SettingsEffect> {
    let result = settings.clear_subtitle_languages();
    self.finish_subtitle_mutation(result, settings.snapshot())
  }

  fn finish_subtitle_mutation(
    &self,
    result: Result<bool, SettingsMutationError>,
    settings: &Settings,
  ) -> Vec<SettingsEffect> {
    let effects = self.save_and_reconfigure(result);
    if effects
      .iter()
      .any(|effect| matches!(effect, SettingsEffect::ReconfigurePlayback))
    {
      self.render_subtitle_settings(settings);
    }
    effects
  }

  fn render_subtitle_settings(&self, settings: &Settings) {
    clear_box(&self.subtitle_languages);
    let languages = settings.subtitle_languages();
    if languages.is_empty() {
      self
        .subtitle_languages
        .append(&dim_label("No subtitle language priority configured."));
      return;
    }
    let last = languages.len().saturating_sub(1);
    for (index, language) in languages.iter().enumerate() {
      let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
      let label = gtk::Label::new(Some(language));
      label.add_css_class("monospace");
      label.set_hexpand(true);
      label.set_xalign(0.0);
      row.append(&label);
      let up = gtk::Button::from_icon_name("go-up-symbolic");
      up.set_tooltip_text(Some("Move up"));
      up.set_sensitive(index > 0);
      up.connect_clicked({
        let sender = self.sender.clone();
        move |_| {
          sender.emit(AppMessage::Settings(Message::MoveSubtitleLanguage {
            index,
            offset: -1,
          }))
        }
      });
      row.append(&up);
      let down = gtk::Button::from_icon_name("go-down-symbolic");
      down.set_tooltip_text(Some("Move down"));
      down.set_sensitive(index < last);
      down.connect_clicked({
        let sender = self.sender.clone();
        move |_| {
          sender.emit(AppMessage::Settings(Message::MoveSubtitleLanguage {
            index,
            offset: 1,
          }))
        }
      });
      row.append(&down);
      let remove = gtk::Button::from_icon_name("edit-delete-symbolic");
      remove.set_tooltip_text(Some("Remove"));
      remove.connect_clicked({
        let sender = self.sender.clone();
        move |_| sender.emit(AppMessage::Settings(Message::RemoveSubtitleLanguage(index)))
      });
      row.append(&remove);
      self.subtitle_languages.append(&row);
    }
  }

  fn update_shortcut(
    &self,
    kind: ShortcutKind,
    key: String,
    settings: &mut SettingsStore,
  ) -> Vec<SettingsEffect> {
    match settings.set_shortcut(kind, key) {
      Err(SettingsMutationError::EmptyShortcut) => {
        vec![self.show_failure("MPV shortcut keys cannot be empty.")]
      }
      Err(SettingsMutationError::ShortcutCollision) => {
        vec![
          self.show_failure("That MPV shortcut is already assigned to another JellyPilot action.")
        ]
      }
      result => match self.save_application_config(result) {
        Ok(true) => self.write_shortcut_config(settings.snapshot()),
        Ok(false) => Vec::new(),
        Err(effect) => vec![effect],
      },
    }
  }

  fn write_shortcut_config(&self, settings: &Settings) -> Vec<SettingsEffect> {
    if write_input_conf(
      settings.key_next_episode(),
      settings.key_previous_episode(),
      settings.key_intro_skip(),
    )
    .is_some()
    {
      self.set_config_status("Saved. Shortcut changes apply when MPV (re)starts.");
      Vec::new()
    } else {
      vec![self.show_failure("Settings were saved, but the MPV shortcut file could not be written.")]
    }
  }

  fn set_image_cache_enabled(
    &self,
    enabled: bool,
    settings: &mut SettingsStore,
  ) -> Vec<SettingsEffect> {
    let previous = settings.snapshot().image_cache_enabled();
    match self.save_application_config(settings.set_image_cache_enabled(enabled)) {
      Ok(true) => {
        self.set_config_status(if enabled {
          "Saved. Disk Library Image Cache enabled."
        } else {
          "Saved. Disk Library Image Cache disabled; memory caching remains active."
        });
        vec![SettingsEffect::SetImageCacheEnabled(enabled)]
      }
      Ok(false) => Vec::new(),
      Err(effect) => {
        self.image_cache_syncing.set(true);
        self.image_cache.set_active(previous);
        self.image_cache_syncing.set(false);
        vec![effect]
      }
    }
  }

  fn confirm_clear_image_cache(&self, cx: &SettingsContext) -> Vec<SettingsEffect> {
    if cx.image_cache_clearing {
      return Vec::new();
    }
    let Some(parent) = relm4::main_adw_application().active_window() else {
      return vec![self.show_failure("Library Image Cache confirmation could not be shown.")];
    };
    let dialog = adw::AlertDialog::new(
      Some("Clear Library Image Cache?"),
      Some(
        "This removes best-effort original image copies. Artwork will be fetched again as needed.",
      ),
    );
    dialog.add_responses(&[("cancel", "Cancel"), ("clear", "Clear cache")]);
    dialog.set_close_response("cancel");
    dialog.set_response_appearance("clear", adw::ResponseAppearance::Destructive);
    let sender = self.sender.clone();
    gtk::glib::spawn_future_local(async move {
      if dialog.choose_future(&parent).await.as_str() == "clear" {
        sender.emit(AppMessage::Settings(Message::ClearImageCache));
      }
    });
    Vec::new()
  }

  fn render_image_cache_stats(&self, stats: ArtworkCacheStats, clearing: bool) {
    self.image_cache_stats.set_label(&format!(
      "{} across {} cached image{}",
      format_byte_count(stats.bytes),
      stats.entries,
      if stats.entries == 1 { "" } else { "s" }
    ));
    self
      .image_cache_clear
      .set_sensitive(!clearing && stats.entries > 0);
  }

  fn save_and_reconfigure(
    &self,
    result: Result<bool, SettingsMutationError>,
  ) -> Vec<SettingsEffect> {
    match self.save_application_config(result) {
      Ok(true) => vec![SettingsEffect::ReconfigurePlayback],
      Ok(false) => Vec::new(),
      Err(effect) => vec![effect],
    }
  }

  fn save_application_config(
    &self,
    result: Result<bool, SettingsMutationError>,
  ) -> Result<bool, SettingsEffect> {
    match result {
      Ok(changed) => {
        if changed {
          self.set_config_status("Saved");
        }
        Ok(changed)
      }
      Err(_) => Err(self.show_failure("Settings could not be saved.")),
    }
  }

  fn show_failure(&self, message: &str) -> SettingsEffect {
    self.set_config_status(message);
    diagnostic(
      DiagnosticLevel::Warning,
      DiagnosticCategory::Config,
      message,
    )
  }
}

fn diagnostic(
  level: DiagnosticLevel,
  category: DiagnosticCategory,
  message: impl Into<String>,
) -> SettingsEffect {
  SettingsEffect::Diagnostic(level, category, message.into())
}

struct SettingsPageWidgets<'a> {
  config_status: &'a gtk::Label,
  server_url: &'a gtk::Label,
  user: &'a gtk::Label,
  remote_status: &'a gtk::Label,
  disconnect: &'a gtk::Button,
  reconnect: &'a gtk::Button,
  refresh_status: &'a gtk::Button,
  saved_profile: &'a gtk::Label,
  storage_status: &'a gtk::Label,
  forget_saved_profile: &'a gtk::Button,
  mpv_path: &'a adw::EntryRow,
  detect_mpv: &'a gtk::Button,
  mpv_status: &'a gtk::Label,
  mpv_args: &'a adw::EntryRow,
  target_name: &'a adw::EntryRow,
  subtitle_languages: &'a gtk::Box,
  subtitle_preset: &'a gtk::DropDown,
  subtitle_preset_add: &'a gtk::Button,
  subtitle_custom: &'a adw::EntryRow,
  subtitle_custom_add: &'a gtk::Button,
  subtitle_clear: &'a gtk::Button,
  key_next: &'a adw::EntryRow,
  key_previous: &'a adw::EntryRow,
  key_intro: &'a adw::EntryRow,
  image_cache: &'a adw::SwitchRow,
  image_cache_stats: &'a gtk::Label,
  image_cache_clear: &'a gtk::Button,
  intro_skip: &'a adw::PreferencesGroup,
}

fn settings_page(
  widgets: SettingsPageWidgets<'_>,
  diagnostics: &adw::PreferencesPage,
) -> adw::PreferencesDialog {
  let dialog = adw::PreferencesDialog::new();
  dialog.set_title("Preferences");
  let page = adw::PreferencesPage::new();
  page.set_title("JellyPilot");
  let SettingsPageWidgets {
    config_status,
    server_url,
    user,
    remote_status,
    disconnect,
    reconnect,
    refresh_status,
    saved_profile,
    storage_status,
    forget_saved_profile,
    mpv_path,
    detect_mpv,
    mpv_status,
    mpv_args,
    target_name,
    subtitle_languages,
    subtitle_preset,
    subtitle_preset_add,
    subtitle_custom,
    subtitle_custom_add,
    subtitle_clear,
    key_next,
    key_previous,
    key_intro,
    image_cache,
    image_cache_stats,
    image_cache_clear,
    intro_skip,
  } = widgets;

  let status_group = adw::PreferencesGroup::new();
  status_group.add(config_status);
  page.add(&status_group);

  let connection_group = adw::PreferencesGroup::new();
  connection_group.set_title("Connection");
  connection_group.set_description(Some(
    "Live authenticated-session and Remote Control status.",
  ));
  connection_group.add(server_url);
  connection_group.add(user);
  connection_group.add(remote_status);
  let connection_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  connection_actions.append(disconnect);
  connection_actions.append(reconnect);
  connection_actions.append(refresh_status);
  connection_group.add(&connection_actions);
  page.add(&connection_group);

  let player_group = adw::PreferencesGroup::new();
  player_group.set_title("Player");
  player_group.set_description(Some(
    "MPV path, advanced arguments, and subtitle priorities apply on the next MPV start. Playback Target name applies to newly established sessions.",
  ));
  player_group.add(mpv_path);
  let detect_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  detect_row.append(detect_mpv);
  detect_row.append(mpv_status);
  player_group.add(&detect_row);
  player_group.add(mpv_args);
  player_group.add(target_name);
  page.add(&player_group);

  let subtitles_group = adw::PreferencesGroup::new();
  subtitles_group.set_title("Subtitles");
  subtitles_group.set_description(Some(
    "Ordered MPV subtitle-language priority for newly started playback.",
  ));
  subtitles_group.add(subtitle_languages);
  let preset_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  preset_row.append(subtitle_preset);
  preset_row.append(subtitle_preset_add);
  subtitles_group.add(&preset_row);
  subtitles_group.add(subtitle_custom);
  let subtitle_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  subtitle_actions.append(subtitle_custom_add);
  subtitle_actions.append(subtitle_clear);
  subtitles_group.add(&subtitle_actions);
  page.add(&subtitles_group);

  let shortcuts_group = adw::PreferencesGroup::new();
  shortcuts_group.set_title("Shortcuts");
  shortcuts_group.set_description(Some(
    "JellyPilot MPV bindings are saved immediately and apply when MPV (re)starts.",
  ));
  shortcuts_group.add(key_next);
  shortcuts_group.add(key_previous);
  shortcuts_group.add(key_intro);
  page.add(&shortcuts_group);

  let library_group = adw::PreferencesGroup::new();
  library_group.set_title("Library");
  library_group.set_description(Some(
    "The disk cache is best-effort acceleration for original Library Image bytes, not an offline artwork source. Capacity is bounded to 512 MiB.",
  ));
  library_group.add(image_cache);
  library_group.add(image_cache_stats);
  image_cache_clear.set_halign(gtk::Align::Start);
  library_group.add(image_cache_clear);
  page.add(&library_group);

  page.add(intro_skip);

  let session_group = adw::PreferencesGroup::new();
  session_group.set_title("Session");
  session_group.set_description(Some(
    "Saved sign-ins remain available until they are forgotten.",
  ));
  session_group.add(saved_profile);
  session_group.add(storage_status);
  forget_saved_profile.set_halign(gtk::Align::Start);
  session_group.add(forget_saved_profile);
  page.add(&session_group);

  dialog.add(&page);
  dialog.add(diagnostics);
  dialog
}

fn dim_label(text: &str) -> gtk::Label {
  let label = gtk::Label::new(Some(text));
  label.add_css_class("dim-label");
  label.set_xalign(0.0);
  label
}

fn clear_box(container: &gtk::Box) {
  while let Some(child) = container.first_child() {
    container.remove(&child);
  }
}
