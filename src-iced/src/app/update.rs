//! Top-level message router (ADR 0029): per-surface messages delegate to the
//! surface modules; navigation lives in the shell surface, and cross-surface
//! follow-ups (login connection transitions, settings playback snapshots,
//! window resize/close effects, tray window actions) are hoisted here.

use iced::Task;
use jellypilot_auth::login::ConnectionPhase;
use jellypilot_core::config::{AppMode, IntroMode, Settings};
use jellypilot_core::diagnostics::{DiagnosticCategory, DiagnosticLevel};
use jellypilot_core::locale::LanguagePreference;
use jellypilot_mpv::playback_session::{PlaybackInput, PlaybackIntent};

use crate::i18n::{Localizer, UiText};
use crate::tray::TrayAction;

use super::accounts;
use super::avatars;
use super::browse;
use super::detail;
use super::home;
use super::login;
use super::message::{
  BrowseMessage, DetailMessage, HomeMessage, LoginMessage, Message, SettingsMessage, WindowMessage,
};
use super::playback;
use super::settings;
use super::shell;
use super::state::{Destination, NoticeLevel, State};

/// Settings fields whose mutation triggers cross-surface follow-ups (playback
/// reconfiguration, intro-mode input, remote refinalization). The top-level
/// router snapshots them around the settings surface update so it can hoist
/// those follow-up writes (ADR 0029).
struct SettingsPlaybackSnapshot {
  mpv_path: Option<String>,
  mpv_args: Vec<String>,
  subtitle_languages: Vec<String>,
  intro_mode: IntroMode,
  remember_season_volume: bool,
  playback_target_name: Option<String>,
  app_mode: AppMode,
}

impl SettingsPlaybackSnapshot {
  fn capture(settings: &Settings) -> Self {
    Self {
      mpv_path: settings.mpv_path().map(str::to_owned),
      mpv_args: settings.mpv_args().to_vec(),
      subtitle_languages: settings.subtitle_languages().to_vec(),
      intro_mode: settings.intro_mode(),
      remember_season_volume: settings.remember_season_volume(),
      playback_target_name: settings.playback_target_name().map(str::to_owned),
      app_mode: settings.app_mode(),
    }
  }
}

fn activate_connection(state: &mut State) -> Task<Message> {
  let control_only = state.app_mode() == AppMode::ControlOnly;
  state.shell.destination = if control_only {
    Destination::NowPlaying
  } else {
    Destination::Home
  };
  playback::initialize_playback(
    &mut state.playback,
    &mut state.kernel,
    state.shell.quit_requested,
  );
  let mut tasks = vec![playback::start_remote_session(
    &mut state.playback,
    &mut state.kernel,
  )];
  if let Some(full) = state.full.as_mut() {
    tasks.push(home::start_load(&mut full.home, &mut state.kernel));
    tasks.push(super::personal_lists::load_membership(
      &mut full.personal_lists,
      &mut state.kernel,
      &state.watchlist,
    ));
  }
  Task::batch(tasks)
}

fn update_account(state: &mut State, message: accounts::Message) -> Task<Message> {
  let was_modal = accounts::blocking_modal(&state.accounts);
  let closing = matches!(
    &message,
    accounts::Message::CancelConfirmation | accounts::Message::CloseAddAccount
  );
  match &message {
    accounts::Message::SwitchProfile(key) | accounts::Message::AskSignOut(key) => {
      if let Some(index) = state
        .login
        .flow
        .profiles
        .iter()
        .position(|profile| profile.key() == key)
      {
        let action = if matches!(&message, accounts::Message::SwitchProfile(_)) {
          "switch"
        } else {
          "signout"
        };
        state.shell.account_focus_return = shell::profile_action_id(index, action);
      }
    }
    accounts::Message::AddAccount => {
      state.shell.account_focus_return = shell::ACCOUNT_ADD_TRIGGER_ID.to_owned()
    }
    accounts::Message::Disconnect => {
      state.shell.account_focus_return = shell::ACCOUNT_DISCONNECT_TRIGGER_ID.to_owned()
    }
    _ => {}
  }
  if let accounts::Message::RemoteHandoffSettled { generation } = &message {
    playback::finish_account_remote_handoff(&mut state.playback, *generation);
  }
  let previous_error = accounts::view(state).error.cloned();
  let result = accounts::update(
    &mut state.accounts,
    &mut state.login.flow,
    &mut state.kernel,
    &state.watchlist,
    accounts::RuntimeFacts {
      quit_requested: state.shell.quit_requested,
      playback_active: state.playback.view.now_playing.is_some()
        || state.playback.in_flight_command.is_some(),
    },
    message,
  );
  if let Some(diagnostic) = state.accounts.diagnostic.take() {
    state
      .kernel
      .diagnostics
      .record(DiagnosticLevel::Error, DiagnosticCategory::Auth, diagnostic);
  }
  let mut tasks = vec![result.task.map(Message::Account)];
  match result.effect {
    Some(accounts::Effect::BeginHandoff { generation }) => {
      let start = playback::begin_account_handoff(
        &mut state.playback,
        &mut state.kernel,
        state.shell.quit_requested,
        generation,
      );
      tasks.push(start.playback);
      tasks.push(start.remote.map(Message::Account));
      if let Some(result) = start.playback_cleanup {
        tasks.push(Task::done(Message::Account(
          accounts::Message::PlaybackHandoffSettled { generation, result },
        )));
      }
    }
    Some(accounts::Effect::Activated) => {
      shell::reset_connected_content(state);
      tasks.push(activate_connection(state));
    }
    Some(accounts::Effect::Disconnected) => shell::reset_connected_content(state),
    None => {}
  }
  if let Some(error) = accounts::view(state)
    .error
    .cloned()
    .filter(|error| Some(error) != previous_error.as_ref())
  {
    tasks.push(state.kernel.show_toast(NoticeLevel::Error, error));
  }
  if playback::quit_may_exit(&state.playback, state.shell.quit_requested) {
    tasks.push(iced::exit());
  }
  let is_modal = accounts::blocking_modal(&state.accounts);
  if !was_modal && is_modal {
    tasks.push(iced::widget::operation::focus_next());
  } else if closing && was_modal && !is_modal {
    tasks.push(iced::widget::operation::focus(
      state.shell.account_focus_return.clone(),
    ));
  }
  tasks.push(avatars::refresh(
    &mut state.kernel,
    &state.login.flow.profiles,
  ));
  Task::batch(tasks)
}

fn select_ui_language(state: &mut State, preference: LanguagePreference) -> Task<Message> {
  if let Err(error) = state.kernel.settings.set_ui_language(preference) {
    state.kernel.diagnostics.record(
      DiagnosticLevel::Error,
      DiagnosticCategory::Config,
      error.to_string(),
    );
    return state
      .kernel
      .show_toast(NoticeLevel::Error, UiText::new("language-save-failed"));
  }
  // A repeated System choice still samples the current OS preferences. The
  // persisted-field change flag says nothing about the effective language.
  let locale = Localizer::resolve(preference);
  state.kernel.locale = locale;
  if let Some(tray) = &state.kernel.tray {
    tray.sync(&state.playback.view, state.shell.quit_requested, locale);
  }
  Task::none()
}

pub fn update(state: &mut State, message: Message) -> Task<Message> {
  let was_fullscreen = state.shell.player_fullscreen;
  let task = route_message(state, message);
  if was_fullscreen && !state.shell.player_fullscreen {
    playback::cancel_slider_drags(&mut state.playback);
  }
  shell::reconcile_refresh(state);
  let collections_task = super::collections::reconcile(state);
  let fullscreen_task = shell::reconcile_player_fullscreen(state);
  super::embedded_player::reconcile(state);
  if let Some(full) = state.full.as_mut() {
    state
      .image_diagnostics
      .record(full.home.artwork.take_summary());
    state
      .image_diagnostics
      .record(full.browse.artwork.take_summary());
    state
      .image_diagnostics
      .record(full.detail.artwork.take_summary());
    state
      .image_diagnostics
      .record(full.personal_lists.artwork.take_summary());
  }
  state
    .image_diagnostics
    .record(state.playback.artwork.take_summary());
  Task::batch([
    task,
    collections_task,
    fullscreen_task,
    state.image_diagnostics.schedule(),
  ])
}

fn route_message(state: &mut State, message: Message) -> Task<Message> {
  match message {
    Message::EmbeddedPlayer(message) => super::embedded_player::update(state, message),
    Message::Collections(message) => super::collections::update(state, message),
    Message::UiLanguageSelected(preference) => select_ui_language(state, preference),
    Message::Account(message) => update_account(state, message),
    Message::Shell(super::message::ShellMessage::ExitPlayerFullscreen) => {
      shell::exit_player_fullscreen(state)
    }
    Message::Shell(message) => {
      if matches!(message, super::message::ShellMessage::RefreshCurrent) {
        super::collections::invalidate(state);
      }
      let previous_notice = state.kernel.notice.clone();
      let task = shell::update_shell(state, message);
      if let Some(notice) = state
        .kernel
        .notice
        .clone()
        .filter(|notice| Some(notice) != previous_notice.as_ref())
      {
        Task::batch([task, state.kernel.show_toast(NoticeLevel::Error, notice)])
      } else {
        task
      }
    }
    Message::PersonalLists(message) => {
      if accounts::content_mutations_blocked(&state.accounts)
        && matches!(
          &message,
          super::personal_lists::PersonalListsMessage::ToggleWatchlist(_)
            | super::personal_lists::PersonalListsMessage::RemoveWatchlist(_)
            | super::personal_lists::PersonalListsMessage::RemoveFavorite(_)
        )
      {
        return state.kernel.show_toast(
          NoticeLevel::Warning,
          UiText::new("shell-account-change-lists"),
        );
      }
      let Some(full) = state.full.as_mut() else {
        return Task::none();
      };
      if matches!(&message, super::personal_lists::PersonalListsMessage::RemoveFavorite(item)
        if super::collections::busy(full, &item.id))
      {
        return Task::none();
      }
      super::personal_lists::update(
        &mut full.personal_lists,
        &mut state.kernel,
        &state.watchlist,
        message,
      )
    }
    // Theme re-resolves every frame from state, so recording the OS mode is
    // the whole update; the next frame picks up the new effective theme.
    Message::SystemThemeDiscovered(mode) | Message::SystemThemeChanged(mode) => {
      state.system_theme = mode;
      Task::none()
    }
    Message::Window(message) => {
      // Window lifecycle owns cross-surface demand suspension; a resize also
      // re-syncs the sparse metadata window before geometry observes its images.
      let skeletons_active = state.skeletons_active();
      let close_without_tray =
        matches!(message, WindowMessage::CloseRequested(_)) && state.kernel.tray.is_none();
      let window_task = shell::update(
        &mut state.shell,
        &mut state.kernel,
        skeletons_active,
        state.playback.view.now_playing.is_some(),
        message,
      );
      let mut tasks = vec![window_task];
      if !state.shell.images_visible {
        playback::suspend_artwork(&mut state.playback);
        if let Some(full) = state.full.as_mut() {
          full.home.artwork.clear();
          full.browse.artwork.clear();
          full.detail.artwork.clear();
          full.personal_lists.artwork.clear();
        }
      } else if matches!(message, WindowMessage::ShowRequested(_)) {
        tasks.push(playback::resume_artwork(
          &mut state.playback,
          &mut state.kernel,
        ));
      }
      if close_without_tray {
        tasks.push(playback::apply_playback_input(
          &mut state.playback,
          &mut state.kernel,
          state.shell.quit_requested,
          PlaybackInput::Intent(Box::new(PlaybackIntent::Quit)),
        ));
        tasks.push(playback::stop_remote_session_for_quit(
          &mut state.playback,
          &mut state.kernel,
        ));
      }
      if let WindowMessage::Resized(size) = message {
        if let Some(full) = state.full.as_mut() {
          tasks.push(browse::sync_scroll_window(
            &mut full.browse,
            &mut state.kernel,
            size,
          ));
        }
      }
      Task::batch(tasks)
    }
    Message::Login(message) => {
      let was_connected = state.kernel.connection == ConnectionPhase::Connected;
      let profiles_landed = matches!(&message, LoginMessage::ProfilesLoaded { .. });
      let login_task = login::update(
        &mut state.login,
        &mut state.kernel,
        state.playback.view.can_start_login,
        message,
      );
      let avatar_task = if profiles_landed {
        avatars::refresh(&mut state.kernel, &state.login.flow.profiles)
      } else {
        Task::none()
      };
      let login_task = Task::batch([login_task, avatar_task]);
      let is_connected = state.kernel.connection == ConnectionPhase::Connected;
      if !was_connected && is_connected {
        state.kernel.diagnostics.record(
          DiagnosticLevel::Info,
          DiagnosticCategory::Connection,
          "Connected to media server.",
        );
        Task::batch([login_task, activate_connection(state)])
      } else if was_connected && !is_connected {
        state.kernel.diagnostics.record(
          DiagnosticLevel::Info,
          DiagnosticCategory::Connection,
          "Disconnected from media server.",
        );
        Task::batch([login_task, shell::reset_connected_surface(state)])
      } else {
        login_task
      }
    }
    Message::ProfileAvatarLoaded { key, outcome } => {
      avatars::settle(&mut state.kernel, key, outcome);
      Task::none()
    }
    Message::Home(HomeMessage::Navigate(destination)) => {
      // Control-Only mode has no Library Browser; reject its destinations.
      if shell::destination_allowed(state.app_mode(), &destination) {
        shell::navigate(state, destination)
      } else {
        Task::none()
      }
    }
    Message::Home(message) => {
      let Some(full) = state.full.as_mut() else {
        return Task::none();
      };
      home::update(&mut full.home, &mut state.kernel, message)
    }
    Message::Browse(BrowseMessage::SearchSubmitted) => {
      state.shell.compact_search_open = false;
      // Navigation stays at the top-level router; Control-Only has no browse
      // composition, so its messages are ignored.
      let Some(full) = state.full.as_ref() else {
        return Task::none();
      };
      let query = full.browse.search_input.trim().to_owned();
      let destination = (!query.is_empty()).then_some(Destination::Search(query));
      match destination {
        Some(destination) if shell::destination_allowed(state.app_mode(), &destination) => {
          shell::navigate(state, destination)
        }
        _ => Task::none(),
      }
    }
    Message::Browse(message) => {
      if state.full.is_none() {
        return Task::none();
      }
      let previous_notice = state.kernel.notice.clone();
      let source = match &message {
        BrowseMessage::SortChanged(_)
        | BrowseMessage::SortDirectionToggled
        | BrowseMessage::PlayedFilterChanged(_)
        | BrowseMessage::FavoritesToggled => shell::browse_source(state),
        _ => None,
      };
      let full = state.full.as_mut().expect("FullUi checked above");
      let task = browse::update(
        &mut full.browse,
        &mut state.kernel,
        source,
        matches!(state.shell.destination, Destination::Library { .. }),
        state.shell.window_size,
        message,
      );
      if let Some(notice) = state
        .kernel
        .notice
        .clone()
        .filter(|notice| Some(notice) != previous_notice.as_ref())
      {
        let toast_task = state.kernel.show_toast(NoticeLevel::Error, notice);
        Task::batch([task, toast_task])
      } else {
        task
      }
    }
    Message::OpenDetail(item) => {
      let destination = Destination::Detail(item.id.clone());
      if shell::destination_allowed(state.app_mode(), &destination) {
        shell::open_detail(state, item)
      } else {
        Task::none()
      }
    }
    Message::Detail(DetailMessage::Back) if state.full.is_some() => shell::navigate_back(state),
    Message::Detail(DetailMessage::Back) => Task::none(),
    Message::Detail(DetailMessage::WatchlistToggled) => {
      if accounts::content_mutations_blocked(&state.accounts) {
        return state.kernel.show_toast(
          NoticeLevel::Warning,
          UiText::new("shell-account-change-lists"),
        );
      }
      let Some(full) = state.full.as_mut() else {
        return Task::none();
      };
      let Destination::Detail(id) = &state.shell.destination else {
        return Task::none();
      };
      let Some(item) = full.detail.items.get(id).cloned() else {
        return Task::none();
      };
      super::personal_lists::update(
        &mut full.personal_lists,
        &mut state.kernel,
        &state.watchlist,
        super::personal_lists::PersonalListsMessage::ToggleWatchlist(item),
      )
    }
    Message::Detail(message) => {
      if accounts::content_mutations_blocked(&state.accounts)
        && matches!(
          &message,
          DetailMessage::FavoriteToggled | DetailMessage::PlayedToggled
        )
      {
        return state.kernel.show_toast(
          NoticeLevel::Warning,
          UiText::new("shell-account-change-item"),
        );
      }
      let Some(full) = state.full.as_mut() else {
        return Task::none();
      };
      let detail_item_id = match &state.shell.destination {
        Destination::Detail(item_id) => Some(item_id.as_str()),
        _ => None,
      };
      if matches!(
        &message,
        DetailMessage::FavoriteToggled | DetailMessage::PlayedToggled
      ) && detail_item_id.is_some_and(|id| super::collections::busy(full, id))
      {
        return Task::none();
      }
      detail::update(&mut full.detail, &mut state.kernel, detail_item_id, message)
    }
    Message::Settings(message @ (SettingsMessage::Open | SettingsMessage::OpenAccounts)) => {
      if !state.shell.settings_open {
        state.shell.settings_focus_return = if matches!(message, SettingsMessage::OpenAccounts) {
          shell::ACCOUNT_TRIGGER_ID
        } else {
          shell::SETTINGS_TRIGGER_ID
        };
      }
      let prepare = if matches!(message, SettingsMessage::OpenAccounts) {
        settings::update(
          &mut state.settings,
          &mut state.kernel,
          SettingsMessage::SectionSelected(super::state::SettingsSection::Account),
        )
      } else {
        Task::none()
      };
      shell::open_settings(state);
      prepare.chain(iced::widget::operation::focus(
        shell::SETTINGS_INITIAL_FOCUS_ID,
      ))
    }
    Message::Settings(SettingsMessage::Close) => {
      accounts::hide(&mut state.accounts);
      shell::close_settings(state);
      iced::widget::operation::focus(state.shell.settings_focus_return)
    }
    Message::Settings(message) => {
      // Cross-surface writes hoisted out of the settings surface (ADR 0029):
      // mutations that change playback-relevant settings reconfigure playback,
      // re-feed the intro mode, or refinalize the remote target here.
      let settings_before = SettingsPlaybackSnapshot::capture(state.kernel.settings.snapshot());
      let settings_task = settings::update(&mut state.settings, &mut state.kernel, message);
      let settings_after = SettingsPlaybackSnapshot::capture(state.kernel.settings.snapshot());
      let mut tasks = vec![settings_task];
      if settings_after.mpv_path != settings_before.mpv_path
        || settings_after.mpv_args != settings_before.mpv_args
        || settings_after.subtitle_languages != settings_before.subtitle_languages
        || settings_after.remember_season_volume != settings_before.remember_season_volume
      {
        tasks.push(playback::apply_playback_configuration(
          &mut state.playback,
          &mut state.kernel,
          state.shell.quit_requested,
        ));
      }
      if settings_after.intro_mode != settings_before.intro_mode {
        let mode = state.kernel.intro_availability().mode;
        tasks.push(playback::apply_playback_input(
          &mut state.playback,
          &mut state.kernel,
          state.shell.quit_requested,
          PlaybackInput::Intent(Box::new(PlaybackIntent::SetIntroMode(mode))),
        ));
      }
      if settings_after.playback_target_name != settings_before.playback_target_name {
        tasks.push(playback::refinalize_playback_target(
          &mut state.playback,
          &mut state.kernel,
        ));
      }
      if settings_after.app_mode != settings_before.app_mode {
        tasks.push(shell::apply_app_mode(state, settings_after.app_mode));
      }

      Task::batch(tasks)
    }
    Message::Playback(message) => {
      if accounts::handoff_generation(&state.accounts).is_some()
        && matches!(
          &message,
          super::message::PlaybackMessage::Intent(_)
            | super::message::PlaybackMessage::SeekDragStarted
            | super::message::PlaybackMessage::SeekAdjusted(_)
            | super::message::PlaybackMessage::VolumeDragStarted
            | super::message::PlaybackMessage::VolumeAdjusted(_)
            | super::message::PlaybackMessage::SeekChanged(_)
            | super::message::PlaybackMessage::SeekReleased
            | super::message::PlaybackMessage::VolumeChanged(_)
            | super::message::PlaybackMessage::VolumeReleased
            | super::message::PlaybackMessage::AudioTrackSelected(_)
            | super::message::PlaybackMessage::SubtitleTrackSelected(_)
            | super::message::PlaybackMessage::QueueItemSelected(_)
        )
      {
        return Task::none();
      }
      if crate::embedded::enabled()
        && matches!(&message, super::message::PlaybackMessage::Intent(intent)
          if matches!(intent.as_ref(), PlaybackIntent::ToggleFullscreen))
      {
        return shell::toggle_player_fullscreen(state);
      }
      let had_playback = state.playback.view.now_playing.is_some();
      let return_to_source = super::embedded_player::before_playback(state, &message);
      let task = playback::update(
        &mut state.playback,
        &mut state.kernel,
        state.shell.quit_requested,
        message,
      );
      let task = if crate::embedded::enabled()
        && !had_playback
        && state.playback.view.now_playing.is_some()
        && state.app_mode() == AppMode::Full
      {
        Task::batch([task, shell::navigate(state, Destination::NowPlaying)])
      } else {
        task
      };
      let task = if return_to_source {
        Task::batch([task, super::embedded_player::return_to_source(state)])
      } else {
        task
      };
      if let Some((generation, result)) =
        playback::take_account_handoff_settlement(&mut state.playback)
      {
        Task::batch([
          task,
          Task::done(Message::Account(
            accounts::Message::PlaybackHandoffSettled { generation, result },
          )),
        ])
      } else {
        task
      }
    }
    Message::Remote(message) => playback::update_remote(
      &mut state.playback,
      &mut state.kernel,
      state.shell.quit_requested,
      message,
    ),
    // Tray window actions stay at the router (ADR 0029): Show routes through
    // the window surface and Quit owns the shell's quit handshake; transport
    // actions delegate to the playback surface.
    Message::Tray(TrayAction::Show) => {
      iced::window::oldest().map(|id| Message::Window(WindowMessage::ShowRequested(id)))
    }
    Message::Tray(TrayAction::Quit) => {
      if state.shell.quit_requested {
        return Task::none();
      }
      state.shell.quit_requested = true;
      playback::sync_tray(&state.playback, &state.kernel, state.shell.quit_requested);
      Task::batch([
        playback::apply_playback_input(
          &mut state.playback,
          &mut state.kernel,
          state.shell.quit_requested,
          PlaybackInput::Intent(Box::new(PlaybackIntent::Quit)),
        ),
        playback::stop_remote_session_for_quit(&mut state.playback, &mut state.kernel),
      ])
    }
    Message::Tray(action) => {
      if accounts::handoff_generation(&state.accounts).is_some() {
        return Task::none();
      }
      playback::update_tray(
        &mut state.playback,
        &mut state.kernel,
        state.shell.quit_requested,
        action,
      )
    }
    Message::DismissNotice(id) => {
      state.dismiss_toast(id);
      Task::none()
    }
    Message::ArtworkSummaryReady => {
      let summary = state.image_diagnostics.drain();
      if let Some(message) = summary.diagnostic_message() {
        state.kernel.diagnostics.record(
          DiagnosticLevel::Info,
          DiagnosticCategory::Artwork,
          message,
        );
      }
      Task::none()
    }
    Message::ImageObserved {
      surface,
      epoch,
      spec,
      priority,
    } => observe_image(state, surface, epoch, spec, priority),
  }
}

fn observe_image(
  state: &mut State,
  surface: super::artwork::ArtworkSurface,
  epoch: u64,
  spec: super::artwork::ImageSpec,
  priority: Option<super::artwork::ImagePriority>,
) -> Task<Message> {
  use super::artwork::ArtworkSurface;
  if !state.shell.images_visible || !matches!(state.kernel.connection, ConnectionPhase::Connected) {
    return Task::none();
  }
  let Some(client) = state.kernel.client.clone() else {
    return Task::none();
  };
  let collection = match (&state.shell.destination, surface, state.full.as_mut()) {
    (Destination::Home, ArtworkSurface::Home, Some(full)) => &mut full.home.artwork,
    (Destination::Library { .. } | Destination::Search(_), ArtworkSurface::Browse, Some(full)) => {
      &mut full.browse.artwork
    }
    (Destination::Detail(_), ArtworkSurface::Detail, Some(full)) => &mut full.detail.artwork,
    (Destination::PersonalLists(_), ArtworkSurface::PersonalLists, Some(full)) => {
      &mut full.personal_lists.artwork
    }
    _ => return Task::none(),
  };
  if collection.epoch() != epoch {
    return Task::none();
  }
  collection.observe(
    state.kernel.request_gate.current_session(),
    spec,
    priority,
    client,
    std::sync::Arc::clone(&state.kernel.artwork_adapter),
    move |completion| match surface {
      ArtworkSurface::Home => Message::Home(HomeMessage::ArtworkLoaded(completion)),
      ArtworkSurface::Browse => Message::Browse(BrowseMessage::ArtworkLoaded(completion)),
      ArtworkSurface::Detail => Message::Detail(DetailMessage::ArtworkLoaded(completion)),
      ArtworkSurface::PersonalLists => Message::PersonalLists(
        super::personal_lists::PersonalListsMessage::ArtworkLoaded(completion),
      ),
    },
  )
}

#[cfg(test)]
mod tests {
  use crate::app::message::LoginMessage;
  use std::fs;
  use std::path::PathBuf;
  use std::sync::Arc;
  use std::time::Instant;

  use jellypilot_auth::{AuthStorageError, AuthStore, SavedProfileKey};
  use jellypilot_core::browse_model::LibraryBrowseView;
  use jellypilot_core::config::SettingsStore;
  use jellypilot_core::intro_skipper::IntroSkipMode;
  use jellypilot_media_server::{JellyfinClient, MediaServerProvider, VideoLibraryItem};
  use jellypilot_mpv::playback::{
    Playable, PlaybackOutcome, PlaybackRefreshOutcome, PlaybackRefreshState, PlaybackSnapshot,
  };
  use jellypilot_mpv::playback_session::{
    ControllerCommand, ControllerSettlement, IntroAvailability, PlaybackEffect, PlaybackEvent,
  };
  use jellypilot_ui::fonts;

  use super::super::artwork::{ArtworkSurface, ImagePriority, ImageSpec, ImageStatus};
  use super::*;
  use crate::app::kernel::Kernel;
  use crate::app::state::{LoginState, RemoteSessionHandle};

  fn update_ui(
    ui: &mut iced_runtime::user_interface::UserInterface<'_, Message, iced::Theme, iced::Renderer>,
    renderer: &mut iced::Renderer,
    events: &[iced::Event],
    cursor: iced::mouse::Cursor,
    messages: &mut Vec<Message>,
  ) {
    let mut bus = iced::advanced::shell::Bus::new();
    let (_, _) = ui.update(
      &iced::window::Headless,
      &iced::advanced::shell::Waker::noop(),
      events,
      cursor,
      renderer,
      &mut bus,
    );
    messages.extend(
      bus
        .drain()
        .map(|(message, _)| message)
        .filter(|message| !matches!(message, Message::ImageObserved { .. })),
    );
  }

  fn test_state() -> State {
    let settings = SettingsStore::default();
    let mut request_gate = jellypilot_core::request_gate::RequestGate::default();
    let playback = playback::Surface::new(&mut request_gate);
    let settings_view = crate::app::state::SettingsState::from_settings(settings.snapshot());
    let login_flow = LoginState::from_settings(settings.snapshot());
    State {
      system_theme: iced::theme::Mode::None,
      image_diagnostics: Default::default(),
      kernel: Kernel {
        settings,
        locale: Localizer::default(),
        diagnostics: jellypilot_core::diagnostics::Diagnostics::default(),
        auth_store: AuthStore::default(),
        request_gate,
        client: None,
        connection: ConnectionPhase::SignedOut,
        connected_identity: None,
        active_profile: None,
        notice: None,
        active_toast: None,
        next_toast_id: 0,
        tray: None,
        artwork_adapter: Arc::new(jellypilot_media_server::artwork::ArtworkAdapter::new()),
        avatar_adapter: Arc::new(jellypilot_media_server::artwork::ArtworkAdapter::new()),
        profile_avatars: Default::default(),
      },
      login: crate::app::login::Surface {
        flow: login_flow,
        quick_connect_task: None,
      },
      settings: crate::app::settings::Surface {
        view: settings_view,
      },
      instance: None,
      full: Some(crate::app::state::FullUi {
        home: home::Surface::default(),
        detail: detail::Surface::default(),
        browse: browse::Surface::default(),
        personal_lists: super::super::personal_lists::Surface::default(),
        collections: super::super::collections::Surface::default(),
      }),
      playback,
      shell: shell::Surface::new(false),
      watchlist: super::super::personal_lists::Runtime::default(),
      accounts: super::super::accounts::Surface::new(),
    }
  }

  #[tokio::test]
  async fn season_volume_switch_is_keyboard_operable_and_persists() {
    use iced::advanced::{renderer::Headless, widget};
    use iced::{keyboard, mouse, Event, Font, Size};
    use iced_runtime::user_interface::{Cache, UserInterface};

    let (settings, _file) = isolated_settings("season-volume-keyboard");
    let mut state = test_state();
    state.kernel.settings = settings;
    state.kernel.connection = ConnectionPhase::Connected;
    state.shell.settings_open = true;
    state.settings.view.active_section = crate::app::state::SettingsSection::Playback;
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings {
        font: Font::DEFAULT,
        text_size: 14.0.into(),
        line_height: fonts::DEFAULT_LINE_HEIGHT,
        metrics_hinting: false,
      },
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    let mut ui = UserInterface::build(
      crate::app::view(&state, iced::window::Id::unique()),
      Size::new(1200.0, 900.0),
      Cache::new(),
      &mut renderer,
    );
    let key = |named| {
      Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(named),
        modified_key: keyboard::Key::Named(named),
        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Enter),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::NONE,
        text: None,
        repeat: false,
      })
    };
    let mut messages = Vec::new();
    update_ui(
      &mut ui,
      &mut renderer,
      &[key(keyboard::key::Named::Tab)],
      mouse::Cursor::Unavailable,
      &mut messages,
    );
    ui.operate(
      &renderer,
      &mut widget::operation::focusable::focus::<()>(widget::Id::new("settings-season-volume")),
    );
    update_ui(
      &mut ui,
      &mut renderer,
      &[key(keyboard::key::Named::Enter)],
      mouse::Cursor::Unavailable,
      &mut messages,
    );
    drop(ui);
    let message = messages
      .into_iter()
      .find(|message| {
        matches!(
          message,
          Message::Settings(SettingsMessage::RememberSeasonVolumeChanged(_))
        )
      })
      .expect("keyboard activation should toggle season volume memory");
    drop(update(&mut state, message));
    assert!(!state.kernel.settings.snapshot().remember_season_volume());
  }

  fn image_reference(state: &mut State, name: &str) -> String {
    use jellypilot_media_server::{
      image_id_for_url, ImageRefKind, MediaServerProvider, SavedSession,
    };
    let server_url = "https://images.example.com";
    let client = Arc::new(JellyfinClient::new());
    client.login().adopt_validated_session(&SavedSession {
      provider: MediaServerProvider::Jellyfin,
      server_url: server_url.into(),
      access_token: "test-token".into(),
      user_id: "user".into(),
      user_name: "user".into(),
      server_name: None,
      device_id: None,
    });
    state.kernel.client = Some(client);
    state.kernel.connection = ConnectionPhase::Connected;
    image_id_for_url(
      MediaServerProvider::Jellyfin,
      server_url,
      format!("{server_url}/Items/{name}/Images/Primary"),
      ImageRefKind::Artwork,
    )
    .expect("signed test image")
  }

  fn observe_browse_image(state: &mut State, key: &str, image_id: &str) {
    let epoch = state.full.as_ref().unwrap().browse.artwork.epoch();
    drop(update(
      state,
      Message::ImageObserved {
        surface: ArtworkSurface::Browse,
        epoch,
        spec: ImageSpec {
          key: key.into(),
          image_id: image_id.into(),
          size_class: jellypilot_media_server::artwork::ArtworkSizeClass::Card,
          derived: Default::default(),
        },
        priority: Some(ImagePriority::Visible),
      },
    ));
  }

  #[tokio::test]
  async fn account_settings_entry_keeps_its_focus_origin_when_reopened() {
    use iced::advanced::{renderer::Headless, widget};
    use iced::futures::StreamExt;
    use iced::{keyboard, mouse, Event, Font, Size};
    use iced_runtime::user_interface::{Cache, UserInterface};

    type Ui<'a> = UserInterface<'a, Message, iced::Theme, iced::Renderer>;

    async fn apply_focus(task: Task<Message>, ui: &mut Ui<'_>, renderer: &iced::Renderer) {
      if let Some(mut stream) = iced_runtime::task::into_stream(task) {
        while let Some(action) = stream.next().await {
          if let iced_runtime::Action::Widget(mut operation) = action {
            loop {
              ui.operate(renderer, operation.as_mut());
              match operation.finish() {
                widget::operation::Outcome::Chain(next) => operation = next,
                _ => break,
              }
            }
          }
        }
      }
    }

    fn key(key: keyboard::key::Named) -> Event {
      Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(key),
        modified_key: keyboard::Key::Named(key),
        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Enter),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::NONE,
        text: None,
        repeat: false,
      })
    }

    let mut state = test_state();
    state.kernel.connection = ConnectionPhase::Connected;
    state.kernel.connected_identity = Some(crate::app::state::ConnectedIdentity {
      user_name: "Current user".to_owned(),
      provider: jellypilot_media_server::MediaServerProvider::Jellyfin,
      server_url: "https://media.example.test".to_owned(),
      server_name: Some("Media server".to_owned()),
    });
    state.login.flow.profiles_loading = false;
    state.settings.view.active_section = crate::app::state::SettingsSection::Appearance;
    state.shell.account_popover_open = true;
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings {
        font: Font::DEFAULT,
        text_size: 14.0.into(),
        line_height: fonts::DEFAULT_LINE_HEIGHT,
        metrics_hinting: false,
      },
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    let bounds = Size::new(1600.0, 900.0);
    let window = iced::window::Id::unique();
    let mut ui = UserInterface::build(
      crate::app::view(&state, window),
      bounds,
      Cache::new(),
      &mut renderer,
    );
    let mut messages = Vec::new();
    update_ui(
      &mut ui,
      &mut renderer,
      &[key(keyboard::key::Named::Tab)],
      mouse::Cursor::Unavailable,
      &mut messages,
    );
    ui.operate(
      &renderer,
      &mut widget::operation::focusable::focus::<()>(widget::Id::new("account-settings")),
    );
    update_ui(
      &mut ui,
      &mut renderer,
      &[key(keyboard::key::Named::Enter)],
      mouse::Cursor::Unavailable,
      &mut messages,
    );
    let open = messages.pop().expect("Manage accounts keyboard activation");
    let cache = ui.into_cache();
    let task = update(&mut state, open);
    assert!(state.shell.settings_open);
    assert!(!state.shell.account_popover_open);
    assert_eq!(
      state.settings.view.active_section,
      crate::app::state::SettingsSection::Account
    );
    let mut ui = UserInterface::build(
      crate::app::view(&state, window),
      bounds,
      cache,
      &mut renderer,
    );
    apply_focus(task, &mut ui, &renderer).await;
    let cache = ui.into_cache();

    // Ctrl/Cmd+, focuses an already-open modal; it must not replace its origin.
    let task = update(&mut state, Message::Settings(SettingsMessage::Open));
    let mut ui = UserInterface::build(
      crate::app::view(&state, window),
      bounds,
      cache,
      &mut renderer,
    );
    apply_focus(task, &mut ui, &renderer).await;
    let cache = ui.into_cache();
    let task = update(&mut state, Message::Settings(SettingsMessage::Close));
    let mut ui = UserInterface::build(
      crate::app::view(&state, window),
      bounds,
      cache,
      &mut renderer,
    );
    apply_focus(task, &mut ui, &renderer).await;
    messages.clear();
    update_ui(
      &mut ui,
      &mut renderer,
      &[key(keyboard::key::Named::Enter)],
      mouse::Cursor::Unavailable,
      &mut messages,
    );
    assert!(matches!(
      messages.as_slice(),
      [Message::Shell(
        crate::app::message::ShellMessage::ToggleAccountPopover
      )]
    ));
    let cache = ui.into_cache();

    // A subsequent ordinary Settings entry restores its own trigger instead.
    let task = update(&mut state, Message::Settings(SettingsMessage::Open));
    let mut ui = UserInterface::build(
      crate::app::view(&state, window),
      bounds,
      cache,
      &mut renderer,
    );
    apply_focus(task, &mut ui, &renderer).await;
    let cache = ui.into_cache();
    let task = update(&mut state, Message::Settings(SettingsMessage::Close));
    let mut ui = UserInterface::build(
      crate::app::view(&state, window),
      bounds,
      cache,
      &mut renderer,
    );
    apply_focus(task, &mut ui, &renderer).await;
    messages.clear();
    update_ui(
      &mut ui,
      &mut renderer,
      &[key(keyboard::key::Named::Enter)],
      mouse::Cursor::Unavailable,
      &mut messages,
    );
    assert!(matches!(
      messages.as_slice(),
      [Message::Settings(SettingsMessage::Open)]
    ));
  }

  fn episode(id: &str, season_number: i32) -> VideoLibraryItem {
    VideoLibraryItem {
      logo_image_id: None,
      id: id.to_owned(),
      name: "Episode".to_owned(),
      item_type: "Episode".to_owned(),
      production_year: None,
      runtime_seconds: Some(1_800.0),
      played: false,
      favorite: false,
      artwork_image_id: None,
      backdrop_image_id: None,
      series_poster_image_id: None,
      episode_thumb_image_id: None,
      series_thumb_image_id: None,
      series_backdrop_image_id: None,
      season_number: Some(season_number),
      episode_number: Some(1),
      series_id: Some("show-1".to_owned()),
      series_name: Some("Show".to_owned()),
      resume_position_seconds: None,
      played_percentage: None,
      overview: None,
      index_number_end: None,
      season_poster_image_id: None,
      end_year: None,
      series_continuing: false,
      unplayed_item_count: None,
    }
  }

  #[tokio::test]
  async fn hero_selection_and_visible_resume_cards_have_independent_keyboard_actions() {
    use iced::advanced::{renderer::Headless, widget};
    use iced::futures::StreamExt;
    use iced::{keyboard, mouse, Event, Font, Rectangle, Size};
    use iced_runtime::user_interface::{Cache, UserInterface};

    struct FindControl {
      id: widget::Id,
      bounds: Option<Rectangle>,
      rail: Option<(Rectangle, iced::Vector)>,
      page: Option<Rectangle>,
      background: Option<Rectangle>,
      captions: Vec<Rectangle>,
      hero_text: Vec<Rectangle>,
      hero_controls: Vec<Rectangle>,
    }
    impl widget::Operation for FindControl {
      fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
        visit(self);
      }
      fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle) {
        if id == Some(&widget::Id::new("home-backdrop")) {
          self.background = Some(bounds);
        }
      }
      fn focusable(
        &mut self,
        id: Option<&widget::Id>,
        bounds: Rectangle,
        _state: &mut dyn widget::operation::Focusable,
      ) {
        if id == Some(&self.id) {
          self.bounds = Some(bounds);
        }
        if [
          "home-hero-play",
          "home-hero-details",
          "home-hero-rail-prev",
          "home-hero-rail-next",
        ]
        .iter()
        .any(|name| id == Some(&widget::Id::new(name)))
        {
          self.hero_controls.push(bounds);
        }
      }
      fn scrollable(
        &mut self,
        id: Option<&widget::Id>,
        bounds: Rectangle,
        _content: Rectangle,
        translation: iced::Vector,
        _state: &mut dyn widget::operation::Scrollable,
      ) {
        if id == Some(&widget::Id::new("home-hero-rail")) {
          self.rail = Some((bounds, translation));
        } else if id == Some(&widget::Id::new("home-page")) {
          self.page = Some(bounds);
          assert_eq!(translation.y, 0.0, "selection must not scroll the page");
        }
      }
      fn text(&mut self, _id: Option<&widget::Id>, bounds: Rectangle, text: &str) {
        if text.starts_with("A long series") || text == "S1:E1 - Episode A" {
          self.captions.push(bounds);
        }
        if text.starts_with("B long series") || text.contains("Episode B") {
          self.hero_text.push(bounds);
        }
      }
    }
    impl FindControl {
      fn new(id: &'static str) -> Self {
        Self {
          id: widget::Id::new(id),
          bounds: None,
          rail: None,
          page: None,
          background: None,
          captions: Vec::new(),
          hero_text: Vec::new(),
          hero_controls: Vec::new(),
        }
      }

      fn assert_selected_visible(&self) {
        let card = self.bounds.expect("selected rail control");
        let (rail, translation) = self.rail.expect("selection rail");
        let x = card.x - translation.x;
        assert!(
          x >= rail.x && x + card.width <= rail.x + rail.width,
          "selected card is clipped: {card:?}, rail {rail:?}, offset {translation:?}"
        );
        let page = self.page.expect("Home viewport");
        for control in &self.hero_controls {
          assert!(
            control.x >= page.x
              && control.y >= page.y
              && control.x + control.width <= page.x + page.width
              && control.y + control.height <= page.y + page.height,
            "Hero control outside the viewport: {control:?}, page {page:?}"
          );
        }
        for text in self
          .hero_text
          .iter()
          .filter(|text| text.y < rail.y + rail.height)
        {
          for control in self.hero_controls.iter().chain(std::iter::once(&rail)) {
            assert!(
              text.x + text.width <= control.x
                || text.x >= control.x + control.width
                || text.y + text.height <= control.y
                || text.y >= control.y + control.height,
              "Hero identification overlaps a control: {text:?}, control {control:?}"
            );
          }
        }
      }
    }

    async fn apply_widgets(
      task: Task<Message>,
      ui: &mut UserInterface<'_, Message, iced::Theme, iced::Renderer>,
      renderer: &iced::Renderer,
    ) {
      if let Some(mut stream) = iced_runtime::task::into_stream(task) {
        while let Some(action) = stream.next().await {
          if let iced_runtime::Action::Widget(mut operation) = action {
            loop {
              ui.operate(renderer, operation.as_mut());
              match operation.finish() {
                widget::operation::Outcome::Chain(next) => operation = next,
                _ => break,
              }
            }
          }
        }
      }
    }
    fn key(key: keyboard::key::Named) -> Event {
      Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(key),
        modified_key: keyboard::Key::Named(key),
        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Enter),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::NONE,
        text: None,
        repeat: false,
      })
    }
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings {
        font: Font::DEFAULT,
        text_size: 14.0.into(),
        line_height: fonts::DEFAULT_LINE_HEIGHT,
        metrics_hinting: false,
      },
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    for (bounds, with_player, with_prompt) in [
      (Size::new(1024.0, 640.0), false, false),
      (Size::new(1024.0, 640.0), true, false),
      (Size::new(1024.0, 640.0), true, true),
      (Size::new(1760.0, 900.0), false, false),
      (Size::new(1760.0, 900.0), true, false),
      (Size::new(1760.0, 900.0), true, true),
    ] {
      let mut state = test_state();
      state.kernel.connection = ConnectionPhase::Connected;
      let backdrop_id = image_reference(&mut state, "backdrop-b");
      state.shell.window_size = bounds;
      state.playback.view.engine_available = true;
      let mut a = episode("a", 1);
      a.series_id = Some("series-a".to_owned());
      a.resume_position_seconds = Some(120.0);
      a.name = "Episode A".to_owned();
      a.series_name =
        Some("A long series title that must remain identifiable in a compact window".to_owned());
      let mut b = episode("b", 1);
      b.series_id = Some("series-b".to_owned());
      b.series_backdrop_image_id = Some(backdrop_id.clone());
      b.resume_position_seconds = Some(360.0);
      b.name = "Episode B".to_owned();
      b.series_name = Some("B long series title with many words that must not expand the selection card or cover the playback controls".to_owned());
      let mut candidates = vec![a];
      candidates.extend((0..8).map(|index| {
        let mut candidate = episode(&format!("filler-{index}"), 1);
        candidate.series_id = Some(format!("series-{index}"));
        candidate.resume_position_seconds = Some(120.0);
        candidate
      }));
      candidates.push(b);
      state
        .full
        .as_mut()
        .expect("Full mode")
        .home
        .data
        .settle_video_home(Ok(jellypilot_media_server::VideoHome {
          continue_watching: candidates,
          next_up: Vec::new(),
        }));
      if with_player {
        state.playback.view.now_playing = Some(jellypilot_mpv::playback_session::NowPlayingView {
          item: jellypilot_mpv::playback::NowPlayingItem {
            item_id: "another".to_owned(),
            title: "Another movie".to_owned(),
            item_type: "Movie".to_owned(),
            runtime_seconds: Some(2400.0),
            start_position_seconds: 0.0,
            play_method: "DirectPlay".to_owned(),
          },
          paused: false,
          position_seconds: 120.0,
          duration_seconds: Some(2400.0),
          volume: 85.0,
          muted: false,
        });
      }
      if with_prompt {
        state.playback.view.intro_prompt =
          Some(jellypilot_mpv::playback_session::IntroPromptView {
            kind: jellypilot_media_server::IntroSkipKind::Introduction,
          });
      }
      let window = iced::window::Id::unique();
      let mut ui = UserInterface::build(
        crate::app::view(&state, window),
        bounds,
        Cache::new(),
        &mut renderer,
      );
      let mut messages = Vec::new();
      update_ui(
        &mut ui,
        &mut renderer,
        &[key(keyboard::key::Named::Tab)],
        mouse::Cursor::Unavailable,
        &mut messages,
      );
      ui.operate(
        &renderer,
        &mut widget::operation::focusable::focus::<()>(widget::Id::new("home-hero-rail-card-b")),
      );
      update_ui(
        &mut ui,
        &mut renderer,
        &[key(keyboard::key::Named::Enter)],
        mouse::Cursor::Unavailable,
        &mut messages,
      );
      assert!(
        matches!(
          messages.as_slice(),
          [Message::Home(HomeMessage::HeroSelected(id))] if id == "b"
        ),
        "unexpected rail action: {messages:?}, window {bounds:?}, player {with_player}"
      );
      let selected = messages.pop().expect("selection action");
      let cache = ui.into_cache();
      let selection_task = update(&mut state, selected);
      assert_eq!(
        state
          .full
          .as_ref()
          .expect("Full mode")
          .home
          .data
          .featured_item()
          .map(|item| item.id.as_str()),
        Some("b"),
      );
      let mut ui = UserInterface::build(
        crate::app::view(&state, window),
        bounds,
        cache,
        &mut renderer,
      );
      apply_widgets(selection_task, &mut ui, &renderer).await;
      let mut selected_geometry = FindControl::new("home-hero-rail-card-b");
      ui.operate(&renderer, &mut selected_geometry);
      selected_geometry.assert_selected_visible();
      let mut before_artwork = FindControl::new("home-card-play-0-a");
      ui.operate(&renderer, &mut before_artwork);
      let resume_before_artwork = before_artwork.bounds.expect("resume before backdrop");

      // An uncached Backdrop settling must not reset the rail's scroll or focus.
      let cache = ui.into_cache();
      let (image_width, image_height) = if with_prompt {
        (9_u32, 16_u32)
      } else {
        (16, 9)
      };
      state.kernel.artwork_adapter.seed_raster_for_test(
        &backdrop_id,
        jellypilot_media_server::artwork::ArtworkSizeClass::Backdrop,
        jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(
          image_width,
          image_height,
          [120, 180, 210, 255].repeat((image_width * image_height) as usize),
        ),
      );
      let epoch = state.full.as_ref().unwrap().home.artwork.epoch();
      drop(update(
        &mut state,
        Message::ImageObserved {
          surface: ArtworkSurface::Home,
          epoch,
          spec: ImageSpec {
            key: home::ArtworkPlacement::HeroBackdrop.key("b"),
            image_id: backdrop_id,
            size_class: jellypilot_media_server::artwork::ArtworkSizeClass::Backdrop,
            derived: Default::default(),
          },
          priority: Some(ImagePriority::Visible),
        },
      ));
      let mut ui = UserInterface::build(
        crate::app::view(&state, window),
        bounds,
        cache,
        &mut renderer,
      );
      let mut settled_geometry = FindControl::new("home-hero-rail-card-b");
      ui.operate(&renderer, &mut settled_geometry);
      settled_geometry.assert_selected_visible();
      ui.draw(
        &mut renderer,
        &iced::Theme::Dark,
        &iced::advanced::renderer::Style::default(),
        mouse::Cursor::Unavailable,
      );
      update_ui(
        &mut ui,
        &mut renderer,
        &[key(keyboard::key::Named::Enter)],
        mouse::Cursor::Unavailable,
        &mut messages,
      );
      assert!(
        matches!(messages.as_slice(),
          [Message::Home(HomeMessage::HeroSelected(id))] if id == "b"
        ),
        "artwork settlement must preserve selection focus"
      );
      messages.clear();
      let mut query = FindControl::new("home-card-play-0-a");
      ui.operate(&renderer, &mut query);
      let card = query.bounds.expect("independent resume control");
      let page = query.page.expect("Home viewport");
      assert_eq!(
        card, resume_before_artwork,
        "backdrop must not move the resume row"
      );
      let background = query.background.expect("ready backdrop");
      assert_eq!(background.x, page.x, "backdrop must start at the page edge");
      assert_eq!(background.y, page.y, "backdrop must start at the page top");
      assert_eq!(
        background.width, page.width,
        "backdrop must span the full page width"
      );
      assert!(
        (background.height / background.width - image_height as f32 / image_width as f32).abs()
          < 0.0001,
        "backdrop aspect changed: {background:?}, source {image_width}x{image_height}"
      );
      // The fixed Hero can put Continue Watching below the initial viewport.
      // Scroll as a user would before exercising its independent resume target.
      let scroll_y = (card.y + card.height - page.y - page.height + 80.0).max(0.0);
      ui.operate(
        &renderer,
        &mut widget::operation::scrollable::scroll_to::<()>(
          widget::Id::new("home-page"),
          iced::widget::scrollable::AbsoluteOffset {
            x: None,
            y: Some(scroll_y),
          },
        ),
      );
      ui.operate(
        &renderer,
        &mut widget::operation::focusable::focus::<()>(query.id),
      );
      update_ui(
        &mut ui,
        &mut renderer,
        &[key(keyboard::key::Named::Enter)],
        mouse::Cursor::Unavailable,
        &mut messages,
      );
      assert!(matches!(
        messages.as_slice(),
        [Message::Playback(crate::app::message::PlaybackMessage::Intent(intent))]
          if matches!(intent.as_ref(), jellypilot_mpv::playback_session::PlaybackIntent::Start {
            item: Playable::Library(item),
            position: jellypilot_mpv::playback::PlaybackStartPosition::Resume,
            ..
          } if item.id == "a")
      ));
      messages.clear();
      let cursor = mouse::Cursor::Available(iced::Point::new(
        card.center_x(),
        card.y + card.height - 3.0 - scroll_y,
      ));
      update_ui(
        &mut ui,
        &mut renderer,
        &[
          Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
          Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ],
        cursor,
        &mut messages,
      );
      assert!(
        messages.iter().any(|message| matches!(
          message,
          Message::Playback(crate::app::message::PlaybackMessage::Intent(intent))
            if matches!(intent.as_ref(), jellypilot_mpv::playback_session::PlaybackIntent::Start {
              item: Playable::Library(item),
              position: jellypilot_mpv::playback::PlaybackStartPosition::Resume,
              ..
            } if item.id == "a")
        )),
        "the resume target's lower edge must not be clipped by the viewport or player bar"
      );
      assert!(!messages
        .iter()
        .any(|message| matches!(message, Message::Home(HomeMessage::HeroSelected(_)))));
    }
  }

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

  #[test]
  fn language_selection_retranslates_retained_feedback_without_adopting_external_settings() {
    use jellypilot_core::locale::UiLanguage;

    let (mut settings, file) = isolated_settings("language-isolation");
    settings
      .set_ui_language(LanguagePreference::Fixed(UiLanguage::English))
      .unwrap();
    let mut external = SettingsStore::for_test(file.0.clone());
    external.set_app_mode(AppMode::ControlOnly).unwrap();
    external.set_mpv_path("/external/mpv".to_owned()).unwrap();
    external
      .set_ui_language(LanguagePreference::Fixed(UiLanguage::SimplifiedChinese))
      .unwrap();

    let mut state = active_intro_prompt_state();
    state.kernel.settings = settings;
    state.login.flow.quick_connect =
      crate::app::state::QuickConnectState::Waiting("731731".to_owned());
    *state.login.flow.password = "unsaved credential".to_owned();
    drop(state.kernel.show_toast(
      NoticeLevel::Error,
      UiText::new("player-playback-info-unavailable"),
    ));
    let toast = state.kernel.active_toast.clone().unwrap();
    let before_text = state.kernel.locale.message(&toast.message);
    let before_playback = state.playback.session.view();

    drop(update(
      &mut state,
      Message::UiLanguageSelected(LanguagePreference::Fixed(UiLanguage::SimplifiedChinese)),
    ));

    assert_eq!(
      state.kernel.locale,
      Localizer::new(UiLanguage::SimplifiedChinese)
    );
    assert_ne!(state.kernel.locale.message(&toast.message), before_text);
    assert_eq!(state.kernel.active_toast, Some(toast));
    assert_eq!(state.playback.session.view(), before_playback);
    assert_eq!(state.playback.view, before_playback);
    assert_eq!(
      state.login.flow.quick_connect,
      crate::app::state::QuickConnectState::Waiting("731731".to_owned())
    );
    assert_eq!(state.login.flow.password.as_str(), "unsaved credential");
    assert_eq!(state.app_mode(), AppMode::Full);
    assert!(state.full.is_some());
    assert_eq!(state.kernel.settings.snapshot().mpv_path(), None);
    let persisted: Settings = serde_json::from_str(&fs::read_to_string(&file.0).unwrap()).unwrap();
    assert_eq!(persisted.app_mode(), AppMode::ControlOnly);
    assert_eq!(persisted.mpv_path(), Some("/external/mpv"));
  }

  #[test]
  fn repeated_system_selection_resolves_again_even_when_preference_is_unchanged() {
    use jellypilot_core::locale::UiLanguage;

    let (settings, _file) = isolated_settings("language-system");
    let mut state = test_state();
    state.kernel.settings = settings;
    let current = Localizer::resolve(LanguagePreference::System);
    state.kernel.locale = if current == Localizer::default() {
      Localizer::new(UiLanguage::SimplifiedChinese)
    } else {
      Localizer::default()
    };
    drop(update(
      &mut state,
      Message::UiLanguageSelected(LanguagePreference::System),
    ));
    assert_eq!(state.kernel.locale, current);
    assert_eq!(
      state.kernel.settings.snapshot().ui_language(),
      LanguagePreference::System
    );
  }

  #[tokio::test]
  async fn failed_language_save_preserves_the_selected_preference_and_live_text() {
    use iced::advanced::{renderer::Headless, widget};
    use iced::{Font, Rectangle, Size};
    use iced_runtime::user_interface::{Cache, UserInterface};
    use jellypilot_core::locale::UiLanguage;

    #[derive(Default)]
    struct VisibleText(Vec<String>);
    impl widget::Operation for VisibleText {
      fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
        visit(self);
      }
      fn text(&mut self, _id: Option<&widget::Id>, _bounds: Rectangle, text: &str) {
        self.0.push(text.to_owned());
      }
    }

    let (mut settings, file) = isolated_settings("language-failure");
    let previous = LanguagePreference::Fixed(UiLanguage::English);
    settings.set_ui_language(previous).unwrap();
    fs::write(&file.0, "unreadable JSON").unwrap();
    let mut state = test_state();
    state.kernel.settings = settings;
    state.login.flow.error = Some(UiText::new("login-password-failed"));
    drop(update(
      &mut state,
      Message::UiLanguageSelected(LanguagePreference::Fixed(UiLanguage::SimplifiedChinese)),
    ));
    assert_eq!(state.kernel.locale, Localizer::default());
    assert_eq!(state.kernel.settings.snapshot().ui_language(), previous);
    assert!(matches!(
      state.kernel.active_toast,
      Some(crate::app::state::ToastNotice {
        level: NoticeLevel::Error,
        ..
      })
    ));
    assert_eq!(fs::read_to_string(&file.0).unwrap(), "unreadable JSON");

    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings {
        font: Font::DEFAULT,
        text_size: 14.0.into(),
        line_height: fonts::DEFAULT_LINE_HEIGHT,
        metrics_hinting: false,
      },
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    for settings_open in [false, true] {
      state.shell.settings_open = settings_open;
      state.kernel.connection = if settings_open {
        ConnectionPhase::Connected
      } else {
        ConnectionPhase::SignedOut
      };
      let mut ui = UserInterface::build(
        crate::app::view(&state, iced::window::Id::unique()),
        Size::new(1600.0, 900.0),
        Cache::new(),
        &mut renderer,
      );
      let mut visible = VisibleText::default();
      ui.operate(&renderer, &mut visible);
      assert!(
        visible.0.contains(&state.t("language-save-failed")),
        "language save failure must be visible on the active surface"
      );
      if !settings_open {
        assert!(
          visible.0.contains(&state.t("login-password-failed")),
          "language feedback must not replace authentication feedback"
        );
      }
    }
  }

  #[tokio::test]
  async fn toast_appearance_and_dismissal_preserve_login_input_focus() {
    use iced::advanced::{renderer::Headless, widget};
    use iced::{mouse, Event, Font, Rectangle, Size};
    use iced_runtime::user_interface::{Cache, UserInterface};

    #[derive(Default)]
    struct InputProbe {
      input: Option<Rectangle>,
      focused: bool,
    }
    impl widget::Operation for InputProbe {
      fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
        visit(self);
      }
      fn text_input(
        &mut self,
        _id: Option<&widget::Id>,
        bounds: Rectangle,
        _state: &mut dyn widget::operation::TextInput,
      ) {
        if self.input.is_none() {
          self.input = Some(bounds);
        }
      }
      fn focusable(
        &mut self,
        _id: Option<&widget::Id>,
        _bounds: Rectangle,
        state: &mut dyn widget::operation::Focusable,
      ) {
        self.focused |= state.is_focused();
      }
    }
    let mut state = test_state();
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings {
        font: Font::DEFAULT,
        text_size: 14.0.into(),
        line_height: fonts::DEFAULT_LINE_HEIGHT,
        metrics_hinting: false,
      },
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    let bounds = Size::new(1600.0, 900.0);
    let window = iced::window::Id::unique();
    let mut ui = UserInterface::build(
      crate::app::view(&state, window),
      bounds,
      Cache::new(),
      &mut renderer,
    );
    let mut probe = InputProbe::default();
    ui.operate(&renderer, &mut probe);
    update_ui(
      &mut ui,
      &mut renderer,
      &[
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
      ],
      mouse::Cursor::Available(probe.input.expect("login server input").center()),
      &mut Vec::new(),
    );
    let mut probe = InputProbe::default();
    ui.operate(&renderer, &mut probe);
    assert!(
      probe.focused,
      "the input must receive focus before feedback appears"
    );
    let mut cache = ui.into_cache();

    drop(
      state
        .kernel
        .show_toast(NoticeLevel::Error, UiText::new("language-save-failed")),
    );
    for visible in [true, false] {
      if !visible {
        let id = state.kernel.active_toast.as_ref().unwrap().id;
        drop(update(&mut state, Message::DismissNotice(id)));
      }
      let mut ui = UserInterface::build(
        crate::app::view(&state, window),
        bounds,
        cache,
        &mut renderer,
      );
      let mut probe = InputProbe::default();
      ui.operate(&renderer, &mut probe);
      assert!(
        probe.focused,
        "the active input must survive toast visibility={visible}"
      );
      cache = ui.into_cache();
    }
  }

  fn profile_key(name: &str) -> SavedProfileKey {
    let server_url = format!("https://{name}.example.test");
    let user_id = format!("{name}-user-id");
    SavedProfileKey::for_identity(MediaServerProvider::Jellyfin, &server_url, &user_id)
  }

  fn playback_snapshot(position: f64) -> PlaybackSnapshot {
    PlaybackSnapshot {
      now_playing: Some(jellypilot_mpv::playback::NowPlayingItem {
        item_id: "episode-1".to_owned(),
        title: "Pilot".to_owned(),
        item_type: "Episode".to_owned(),
        runtime_seconds: Some(1_800.0),
        start_position_seconds: 0.0,
        play_method: "Transcode".to_owned(),
      }),
      transport: jellypilot_mpv::PlayerState {
        connected: true,
        paused: false,
        muted: false,
        time_pos: position,
        duration: 1_800.0,
        volume: 75.0,
      },
    }
  }

  fn controller_effect(
    effects: Vec<PlaybackEffect>,
  ) -> (
    jellypilot_mpv::playback_session::EffectId,
    ControllerCommand,
  ) {
    let [PlaybackEffect::Controller(id, command)] = effects.as_slice() else {
      panic!("expected one controller effect");
    };
    (*id, command.clone())
  }

  fn active_intro_prompt_state() -> State {
    let mut state = test_state();
    let now = Instant::now();
    state.playback.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::EngineAvailability(true))),
      now,
    );
    let effects = state.playback.session.handle(
      PlaybackInput::Intent(Box::new(PlaybackIntent::Start {
        item: Playable::Library(episode("episode-1", 1)),
        position: jellypilot_mpv::playback::PlaybackStartPosition::Beginning,
        intro: IntroAvailability {
          mode: IntroSkipMode::Manual,
          skipper_available: true,
        },
        selection: Box::default(),
      })),
      now,
    );
    let (start_id, _) = controller_effect(effects);
    let auxiliary = state.playback.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::ControllerSettled {
        id: start_id,
        settlement: ControllerSettlement::Started(Ok(PlaybackOutcome {
          snapshot: playback_snapshot(0.0),
          warnings: Vec::new(),
        })),
      })),
      now,
    );
    let intro_id = auxiliary
      .iter()
      .find_map(|effect| match effect {
        PlaybackEffect::FetchIntroRanges(id, _) => Some(*id),
        PlaybackEffect::Controller(_, _) | PlaybackEffect::LookupAdjacent(_, _) => None,
      })
      .expect("active intro playback should fetch ranges");
    state.playback.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::IntroRangesSettled {
        id: intro_id,
        result: Ok(vec![jellypilot_media_server::IntroSkipRange {
          kind: jellypilot_media_server::IntroSkipKind::Introduction,
          start_seconds: 10.0,
          end_seconds: 30.0,
        }]),
      })),
      now,
    );
    let (refresh_id, _) = controller_effect(
      state
        .playback
        .session
        .handle(PlaybackInput::Intent(Box::new(PlaybackIntent::Tick)), now),
    );
    let effects = state.playback.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::ControllerSettled {
        id: refresh_id,
        settlement: ControllerSettlement::Refreshed {
          outcome: PlaybackRefreshOutcome {
            snapshot: playback_snapshot(10.0),
            state: PlaybackRefreshState::Active,
            warnings: Vec::new(),
          },
          client_messages: Vec::new(),
        },
      })),
      now,
    );
    let (prompt_id, command) = controller_effect(effects);
    assert!(matches!(command, ControllerCommand::ShowText { .. }));
    state.playback.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::ControllerSettled {
        id: prompt_id,
        settlement: ControllerSettlement::OsdShown(Ok(())),
      })),
      now,
    );
    state.playback.view = state.playback.session.view();
    assert!(state.playback.view.intro_prompt.is_some());
    state
  }

  #[test]
  fn newer_notice_replaces_older_and_older_timeout_does_not_dismiss_newer() {
    let mut state = test_state();

    drop(state.kernel.show_toast(
      NoticeLevel::Warning,
      UiText::new("player-remote-connection-lost"),
    ));
    let older = state.kernel.active_toast.as_ref().unwrap().id;
    drop(
      state
        .kernel
        .show_toast(NoticeLevel::Error, UiText::new("player-mpv-control-failed")),
    );
    let newer = state.kernel.active_toast.as_ref().unwrap().id;

    drop(update(&mut state, Message::DismissNotice(older)));
    assert_eq!(
      state.kernel.active_toast.as_ref().map(|toast| toast.id),
      Some(newer)
    );

    drop(update(&mut state, Message::DismissNotice(newer)));
    assert!(state.kernel.active_toast.is_none());
  }

  #[test]
  fn settings_intro_mode_threads_into_playback_availability() {
    let (settings, _file) = isolated_settings("intro-availability");
    let mut state = test_state();
    state.kernel.settings = settings;
    for (configured, expected) in [
      (
        jellypilot_core::config::IntroMode::Automatic,
        IntroSkipMode::Automatic,
      ),
      (
        jellypilot_core::config::IntroMode::Manual,
        IntroSkipMode::Manual,
      ),
      (jellypilot_core::config::IntroMode::Off, IntroSkipMode::Off),
    ] {
      state
        .kernel
        .settings
        .set_intro_mode(configured)
        .expect("isolated settings should save");
      let availability = state.kernel.intro_availability();
      assert_eq!(availability.mode, expected);
      assert!(!availability.skipper_available);
    }
  }

  #[test]
  fn intro_mode_mutation_updates_the_active_playback_session() {
    let (settings, _file) = isolated_settings("live-intro-mode");
    let mut state = active_intro_prompt_state();
    state.kernel.settings = settings;
    state.settings.view =
      crate::app::state::SettingsState::from_settings(state.kernel.settings.snapshot());

    drop(update(
      &mut state,
      Message::Settings(SettingsMessage::IntroModeSelected(
        jellypilot_core::config::IntroMode::Off,
      )),
    ));

    assert_eq!(
      state.kernel.settings.snapshot().intro_mode(),
      jellypilot_core::config::IntroMode::Off
    );
    assert!(state.playback.view.intro_prompt.is_none());
  }

  #[test]
  fn account_disconnect_waits_for_both_cleanup_settlements() {
    let mut state = test_state();
    state.kernel.connection = ConnectionPhase::Connected;
    let client = Arc::new(JellyfinClient::new());
    client
      .login()
      .adopt_validated_session(&jellypilot_media_server::SavedSession {
        provider: MediaServerProvider::Jellyfin,
        server_url: "https://example.test".to_owned(),
        user_id: "user".to_owned(),
        user_name: "User".to_owned(),
        access_token: "test-token".to_owned(),
        server_name: None,
        device_id: None,
      });
    state.kernel.client = Some(client);

    drop(update(
      &mut state,
      Message::Account(accounts::Message::Disconnect),
    ));
    let generation = accounts::handoff_generation(&state.accounts).expect("handoff started");
    assert_eq!(state.kernel.connection, ConnectionPhase::Connected);
    assert!(state.kernel.client.is_some());
    drop(update(
      &mut state,
      Message::PersonalLists(
        super::super::personal_lists::PersonalListsMessage::ToggleWatchlist(episode(
          "late-write",
          1,
        )),
      ),
    ));
    assert!(state
      .full
      .as_ref()
      .unwrap()
      .personal_lists
      .busy_items
      .is_empty());
    assert!(matches!(
      state.kernel.active_toast.as_ref().map(|toast| toast.level),
      Some(NoticeLevel::Warning)
    ));
    drop(update(
      &mut state,
      Message::Account(accounts::Message::PlaybackHandoffSettled {
        generation,
        result: Ok(()),
      }),
    ));
    assert_eq!(state.kernel.connection, ConnectionPhase::Connected);
    drop(update(
      &mut state,
      Message::Account(accounts::Message::RemoteHandoffSettled { generation }),
    ));
    assert!(state.kernel.connection == ConnectionPhase::SignedOut);
    assert!(state.kernel.client.is_none());
  }

  #[test]
  fn close_without_an_available_tray_uses_the_quit_cleanup_handshake() {
    let mut state = test_state();

    drop(update(
      &mut state,
      Message::Window(WindowMessage::CloseRequested(iced::window::Id::unique())),
    ));

    assert!(state.shell.quit_requested);
    assert!(state.playback.view.quit_may_proceed);
  }

  #[test]
  fn target_name_mutation_requests_live_remote_refinalization() {
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-target-name-{}.json",
      std::process::id()
    ));
    let _ = fs::remove_file(&path);
    let mut state = test_state();
    state.kernel.settings = SettingsStore::for_test(path.clone());
    state.settings.view =
      crate::app::state::SettingsState::from_settings(state.kernel.settings.snapshot());
    state.kernel.connection = ConnectionPhase::Connected;
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    state.playback.remote_session = Some(RemoteSessionHandle {
      websocket: Arc::new(jellypilot_session::JellyfinWebSocket::new()),
      lifecycle: Arc::new(tokio::sync::Mutex::new(())),
    });
    state.settings.view.playback_target_name_input = "Bedroom".to_owned();
    state.playback.remote_control_state = jellypilot_session::RemoteControlState::Available;

    let task = update(
      &mut state,
      Message::Settings(SettingsMessage::SavePlaybackTargetName),
    );

    assert_eq!(
      state.kernel.settings.snapshot().playback_target_name(),
      Some("Bedroom")
    );
    assert!(state.kernel.diagnostics.rows().any(|event| {
      event.message == "Playback target name changed; remote registration requested."
    }));
    assert_eq!(task.units(), 1);
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn connecting_target_name_mutation_schedules_no_duplicate_registration() {
    let (settings, _file) = isolated_settings("connecting-target-name");
    let mut state = test_state();
    state.kernel.settings = settings;
    state.settings.view =
      crate::app::state::SettingsState::from_settings(state.kernel.settings.snapshot());
    state.kernel.connection = ConnectionPhase::Connected;
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    state.playback.remote_session = Some(RemoteSessionHandle {
      websocket: Arc::new(jellypilot_session::JellyfinWebSocket::new()),
      lifecycle: Arc::new(tokio::sync::Mutex::new(())),
    });
    state.playback.remote_control_state = jellypilot_session::RemoteControlState::Connecting;
    state.settings.view.playback_target_name_input = "Bedroom".to_owned();

    let task = update(
      &mut state,
      Message::Settings(SettingsMessage::SavePlaybackTargetName),
    );

    assert_eq!(task.units(), 0);
    assert!(!state.kernel.diagnostics.rows().any(|event| {
      event.message == "Playback target name changed; remote registration requested."
    }));
  }

  #[test]
  fn saving_mpv_path_discovers_a_missing_playback_controller() {
    let (settings, _file) = isolated_settings("discover-mpv-path");
    let mut state = test_state();
    state.kernel.settings = settings;
    state.settings.view =
      crate::app::state::SettingsState::from_settings(state.kernel.settings.snapshot());
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    state.settings.view.mpv_path_input = std::env::current_exe()
      .expect("test executable path should resolve")
      .to_string_lossy()
      .into_owned();

    let task = update(&mut state, Message::Settings(SettingsMessage::SaveMpvPath));

    assert_eq!(task.units(), 0);
    assert!(state.playback.controller.is_some());
    assert!(state.playback.view.engine_available);
    assert!(state.playback.notice.is_none());
  }

  #[test]
  fn escape_and_closing_settings_modal_both_clear_shortcut_capture() {
    let mut state = test_state();
    state.shell.settings_open = true;
    state.settings.view.shortcut_capture = Some(jellypilot_core::config::ShortcutKind::Next);

    drop(update(
      &mut state,
      Message::Settings(SettingsMessage::CancelShortcutCapture),
    ));
    assert!(state.settings.view.shortcut_capture.is_none());

    state.settings.view.shortcut_capture = Some(jellypilot_core::config::ShortcutKind::Previous);
    drop(update(
      &mut state,
      Message::Settings(SettingsMessage::Close),
    ));
    assert!(!state.shell.settings_open);
    assert!(state.settings.view.shortcut_capture.is_none());
  }

  #[test]
  fn sign_out_does_not_remove_credentials_when_the_saved_profile_summary_is_missing() {
    let key = profile_key("active");
    let mut disconnect = test_state();
    disconnect.kernel.connection = ConnectionPhase::Connected;
    disconnect.kernel.active_profile = Some(key.clone());

    drop(update(
      &mut disconnect,
      Message::Account(accounts::Message::Disconnect),
    ));

    assert!(disconnect.login.flow.busy_profile.is_none());

    let mut sign_out = test_state();
    sign_out.kernel.connection = ConnectionPhase::Connected;
    sign_out.kernel.active_profile = Some(key.clone());
    drop(update(
      &mut sign_out,
      Message::Account(accounts::Message::AskSignOut(key.clone())),
    ));

    assert!(sign_out.login.flow.busy_profile.is_none());
    assert!(accounts::view(&sign_out).error.is_some());
    assert_eq!(sign_out.kernel.active_profile.as_ref(), Some(&key));
    assert_eq!(sign_out.kernel.connection, ConnectionPhase::Connected);
  }

  #[test]
  fn login_failures_feed_the_sanitized_diagnostics_buffer() {
    let mut state = test_state();
    let revision = state.login.flow.profiles_revision;

    drop(update(
      &mut state,
      Message::Login(LoginMessage::ProfilesLoaded {
        revision,
        result: Err(AuthStorageError::Corrupt),
      }),
    ));

    assert!(state.kernel.diagnostics.rows().any(|event| {
      event.level == DiagnosticLevel::Error && event.category == DiagnosticCategory::Auth
    }));
  }

  #[test]
  fn image_bursts_record_one_event_only_when_the_aggregate_is_flushed() {
    let mut state = test_state();
    let image_id = image_reference(&mut state, "shared");
    state.shell.destination = Destination::Search("images".into());
    state.kernel.artwork_adapter.seed_raster_for_test(
      &image_id,
      jellypilot_media_server::artwork::ArtworkSizeClass::Card,
      jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(1, 1, vec![1, 2, 3, 255]),
    );
    observe_browse_image(&mut state, "one", &image_id);
    observe_browse_image(&mut state, "two", &image_id);
    assert!(!state
      .kernel
      .diagnostics
      .rows()
      .any(|event| event.category == DiagnosticCategory::Artwork));
    drop(update(&mut state, Message::ArtworkSummaryReady));
    drop(update(&mut state, Message::ArtworkSummaryReady));
    assert_eq!(
      state
        .kernel
        .diagnostics
        .rows()
        .filter(|event| event.category == DiagnosticCategory::Artwork)
        .count(),
      1
    );
  }

  #[test]
  fn disappearing_image_revocation_survives_metadata_retention_and_allows_reentry() {
    let mut state = test_state();
    let image_id = image_reference(&mut state, "retained");
    state.shell.destination = Destination::Search("images".into());
    let spec = ImageSpec {
      key: "card".into(),
      image_id: image_id.clone(),
      size_class: jellypilot_media_server::artwork::ArtworkSizeClass::Card,
      derived: Default::default(),
    };
    state.kernel.artwork_adapter.seed_raster_for_test(
      &image_id,
      spec.size_class,
      jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(1, 1, vec![1, 2, 3, 255]),
    );
    observe_browse_image(&mut state, "card", &image_id);
    let images = &mut state.full.as_mut().unwrap().browse.artwork;
    let epoch = images.epoch();
    // A failed refresh may retain its data while replacing all cards with an
    // error surface. Its last rendered marker still owns the final revocation.
    images.retain(std::slice::from_ref(&spec));
    drop(update(
      &mut state,
      Message::ImageObserved {
        surface: ArtworkSurface::Browse,
        epoch,
        spec,
        priority: None,
      },
    ));
    assert!(state
      .full
      .as_ref()
      .unwrap()
      .browse
      .artwork
      .get("card")
      .is_none());
    observe_browse_image(&mut state, "card", &image_id);
    assert!(state
      .full
      .as_ref()
      .unwrap()
      .browse
      .artwork
      .get("card")
      .unwrap()
      .handle()
      .is_some());
  }

  #[tokio::test]
  async fn browse_scrolling_with_cached_images_settles_before_redraw_retry_limit() {
    use iced::advanced::renderer::Headless;
    use iced::{mouse, Event, Font, Point, Size};
    use iced_runtime::user_interface::{Cache, UserInterface};
    use jellypilot_core::browse_model::{BrowseEffect, BrowsePagePayload, BrowsePageSettlement};

    let mut state = test_state();
    let image_id = image_reference(&mut state, "scroll-regression");
    state
      .shell
      .navigate_to(Destination::Search("scroll regression".into()));
    let source = shell::browse_source(&state).unwrap();
    let browse = &mut state.full.as_mut().unwrap().browse;
    let mut effects = browse.data.configure(source).unwrap();
    let mut expanded = false;
    while let Some(effect) = effects.pop() {
      let BrowseEffect::RequestPage(request) = effect else {
        continue;
      };
      let end = (request.start_index + request.limit).min(240);
      effects.extend(
        browse
          .data
          .settle(BrowsePageSettlement {
            source_id: request.source_id,
            token: request.token,
            result: Ok(BrowsePagePayload {
              start_index: request.start_index,
              limit: request.limit,
              total_record_count: 240,
              has_more: end < 240,
              items: (request.start_index..end)
                .map(|index| {
                  let mut item = episode(&format!("scroll-{index}"), 1);
                  item.artwork_image_id = Some(image_id.clone());
                  item
                })
                .collect(),
            }),
          })
          .unwrap(),
      );
      if !expanded {
        expanded = true;
        effects.extend(browse.data.set_display_range(0..240, 240).unwrap());
      }
    }
    browse.view = browse.data.view();
    state.kernel.artwork_adapter.seed_raster_for_test(
      &image_id,
      jellypilot_media_server::artwork::ArtworkSizeClass::Card,
      jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(2, 3, vec![255; 24]),
    );
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings {
        font: Font::DEFAULT,
        text_size: 14.0.into(),
        line_height: fonts::DEFAULT_LINE_HEIGHT,
        metrics_hinting: false,
      },
      Some("tiny-skia"),
    )
    .await
    .unwrap();
    // Fractional card sizes expose demand-boundary crossings where a placeholder
    // that fills the copy area can repeatedly admit and revoke the same image.
    let bounds = Size::new(1349.0, 731.0);
    state.shell.window_size = bounds;
    let window = iced::window::Id::unique();
    let cursor = mouse::Cursor::Available(Point::new(800.0, 450.0));
    let mut cache = Cache::new();
    for frame in 0..80 {
      if frame > 0 {
        let mut ui = UserInterface::build(
          crate::app::view(&state, window),
          bounds,
          cache,
          &mut renderer,
        );
        let mut bus = iced::advanced::shell::Bus::new();
        ui.update(
          &iced::window::Headless,
          &iced::advanced::shell::Waker::noop(),
          &[Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -100.0 },
          })],
          cursor,
          &mut renderer,
          &mut bus,
        );
        cache = ui.into_cache();
        for (message, _) in bus.drain() {
          drop(update(&mut state, message));
        }
      }
      let redraw = Event::Window(iced::window::Event::RedrawRequested(Instant::now()));
      for pass in 0..3 {
        let mut ui = UserInterface::build(
          crate::app::view(&state, window),
          bounds,
          cache,
          &mut renderer,
        );
        let mut bus = iced::advanced::shell::Bus::new();
        let (ui_state, _) = ui.update(
          &iced::window::Headless,
          &iced::advanced::shell::Waker::noop(),
          std::slice::from_ref(&redraw),
          cursor,
          &mut renderer,
          &mut bus,
        );
        cache = ui.into_cache();
        let messages = bus.drain().map(|(message, _)| message).collect::<Vec<_>>();
        let unstable = !messages.is_empty() || ui_state.has_layout_changed();
        assert!(
          !unstable || pass < 2,
          "redraw retry limit at frame {frame}, pass {pass}: {messages:?}",
        );
        for message in messages {
          drop(update(&mut state, message));
        }
        if !unstable {
          break;
        }
      }
    }
  }

  #[test]
  fn back_restores_search_results_viewport_and_cached_artwork() {
    use jellypilot_core::browse_model::{BrowseEffect, BrowsePagePayload, BrowsePageSettlement};
    let mut state = test_state();
    let image_id = image_reference(&mut state, "browse-art-1");
    let destination = Destination::Search("original query".to_owned());
    state.shell.navigate_to(destination.clone());
    let source = shell::browse_source(&state).expect("search source");
    let browse = &mut state.full.as_mut().unwrap().browse;
    let request = browse
      .data
      .configure(source)
      .unwrap()
      .into_iter()
      .find_map(|effect| {
        if let BrowseEffect::RequestPage(request) = effect {
          Some(request)
        } else {
          None
        }
      })
      .unwrap();
    let mut item = episode("browse-item-1", 1);
    item.artwork_image_id = Some(image_id.clone());
    browse
      .data
      .settle(BrowsePageSettlement {
        source_id: request.source_id,
        token: request.token,
        result: Ok(BrowsePagePayload {
          start_index: 0,
          limit: 24,
          total_record_count: 24,
          has_more: false,
          items: (1..=24)
            .map(|index| {
              let mut item = item.clone();
              item.id = format!("browse-item-{index}");
              item
            })
            .collect(),
        }),
      })
      .unwrap();
    browse.view = browse.data.view();
    browse.viewport.offset_y = 500.0;
    browse.search_input = "original query".to_owned();
    state.kernel.artwork_adapter.seed_raster_for_test(
      &image_id,
      jellypilot_media_server::artwork::ArtworkSizeClass::Card,
      jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(1, 1, vec![1, 2, 3, 4]),
    );

    drop(shell::open_detail(&mut state, item));
    // Visiting a second result set must not overwrite the earlier history entry.
    drop(shell::navigate(
      &mut state,
      Destination::Search("second query".to_owned()),
    ));
    drop(shell::navigate_back(&mut state));
    drop(shell::navigate_back(&mut state));
    drop(update(
      &mut state,
      Message::Browse(BrowseMessage::SearchInputChanged("third query".to_owned())),
    ));
    drop(update(
      &mut state,
      Message::Browse(BrowseMessage::SearchSubmitted),
    ));
    drop(shell::navigate_back(&mut state));
    observe_browse_image(&mut state, "browse-item-1", &image_id);

    assert_eq!(state.shell.destination, destination);
    let browse = &state.full.as_ref().unwrap().browse;
    assert_eq!(browse.search_input, "original query");
    assert_eq!(browse.viewport.offset_y, 500.0);
    let LibraryBrowseView::Ready { visible_items, .. } = &browse.view else {
      panic!("back must show cached results without entering Loading");
    };
    assert_eq!(visible_items[0].item.as_ref().unwrap().id, "browse-item-1");
    let cell = browse
      .artwork
      .get("browse-item-1")
      .expect("restored poster");
    assert!(cell.handle().is_some());
  }

  #[test]
  fn reselecting_active_played_filter_keeps_loaded_artwork() {
    let mut state = test_state();
    let image_id = image_reference(&mut state, "browse-art-1");
    state.full.as_mut().unwrap().home.data.shortcuts =
      jellypilot_core::LoadState::Ready(vec![jellypilot_media_server::VideoLibraryShortcut {
        id: "movies".to_owned(),
        name: "Movies".to_owned(),
        collection_type: "movies".to_owned(),
        item_count: Some(1),
        artwork_image_id: None,
      }]);
    state.shell.destination = Destination::Library {
      library_id: "movies".to_owned(),
      collection_type: "movies".to_owned(),
    };

    // Configure and settle the browse model exactly like a completed page
    // load, so the Ready view comes from the model rather than test injection.
    let source = shell::browse_source(&state).expect("library source resolves");
    let preferences = jellypilot_core::browse_model::BrowsePreferences::from(
      state.kernel.settings.snapshot().browse_filters(),
    );
    let request = state
      .full
      .as_mut()
      .unwrap()
      .browse
      .data
      .configure_with_preferences(source, preferences)
      .expect("library configures")
      .into_iter()
      .find_map(|effect| match effect {
        jellypilot_core::browse_model::BrowseEffect::RequestPage(request) => Some(request),
        _ => None,
      })
      .expect("bootstrap page request is emitted");
    // Data settlement retains candidates; geometry admits the cached image.
    state.kernel.artwork_adapter.seed_raster_for_test(
      &image_id,
      jellypilot_media_server::artwork::ArtworkSizeClass::Card,
      jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(1, 1, vec![1, 2, 3, 4]),
    );
    let mut item = episode("browse-item-1", 1);
    item.artwork_image_id = Some(image_id.clone());
    drop(browse::update(
      &mut state.full.as_mut().unwrap().browse,
      &mut state.kernel,
      None,
      false,
      state.shell.window_size,
      BrowseMessage::PageSettled(jellypilot_core::browse_model::BrowsePageSettlement {
        source_id: request.source_id.clone(),
        token: request.token,
        result: Ok(jellypilot_core::browse_model::BrowsePagePayload {
          start_index: 0,
          limit: 24,
          total_record_count: 1,
          has_more: false,
          items: vec![item],
        }),
      }),
    ));
    assert!(matches!(
      state.full.as_ref().unwrap().browse.view,
      LibraryBrowseView::Ready { .. }
    ));

    observe_browse_image(&mut state, "browse-item-1", &image_id);
    assert_eq!(
      state
        .full
        .as_ref()
        .unwrap()
        .browse
        .artwork
        .get("browse-item-1")
        .map(|cell| cell.state),
      Some(ImageStatus::Ready)
    );

    // `All` is the default played filter: this click changes nothing, so the
    // reconfigure emits no page request and no settlement will re-drive
    // artwork preparation.
    drop(update(
      &mut state,
      Message::Browse(BrowseMessage::PlayedFilterChanged(
        jellypilot_media_server::VideoLibraryPlayedFilter::All,
      )),
    ));

    let browse = &state.full.as_ref().unwrap().browse;
    assert!(
      matches!(browse.view, LibraryBrowseView::Ready { .. }),
      "no-op reconfigure must keep the settled grid view"
    );
    let cell = browse
      .artwork
      .get("browse-item-1")
      .expect("reselecting the active filter must not drop artwork cells");
    assert_eq!(cell.state, ImageStatus::Ready);
    assert!(cell.handle().is_some());
  }
  #[test]
  fn search_draft_and_escape_do_not_replace_submitted_results() {
    let mut state = test_state();
    state.shell.destination = Destination::Search("submitted title".to_owned());
    drop(update(
      &mut state,
      Message::Browse(BrowseMessage::SearchInputChanged("new draft".to_owned())),
    ));
    assert_eq!(
      state.shell.destination,
      Destination::Search("submitted title".to_owned())
    );
    state.shell.compact_search_open = true;
    drop(update(
      &mut state,
      Message::Shell(super::super::message::ShellMessage::ClearSearch),
    ));
    assert!(state.full.as_ref().unwrap().browse.search_input.is_empty());
    assert!(!state.shell.compact_search_open);
    assert_eq!(
      state.shell.destination,
      Destination::Search("submitted title".to_owned())
    );
    drop(update(
      &mut state,
      Message::Browse(BrowseMessage::SearchSubmitted),
    ));
    assert_eq!(
      state.shell.destination,
      Destination::Search("submitted title".to_owned())
    );
  }

  #[test]
  fn control_only_mode_rejects_library_browser_navigation() {
    let mut state = test_state();
    let (mut settings, _guard) = isolated_settings("app-mode-guard");
    settings
      .set_app_mode(jellypilot_core::config::AppMode::ControlOnly)
      .unwrap();
    state.kernel.settings = settings;
    state.full = None;
    state.shell.destination = Destination::NowPlaying;

    drop(update(
      &mut state,
      Message::Home(HomeMessage::Navigate(Destination::Library {
        library_id: "movies".to_owned(),
        collection_type: "movies".to_owned(),
      })),
    ));
    assert_eq!(state.shell.destination, Destination::NowPlaying);

    // The browser composition is absent in Control-Only, so stale input is
    // not retained or routed.
    drop(update(
      &mut state,
      Message::Browse(BrowseMessage::SearchSubmitted),
    ));
    assert_eq!(state.shell.destination, Destination::NowPlaying);

    drop(update(
      &mut state,
      Message::OpenDetail(episode("movie-1", 1)),
    ));
    assert_eq!(state.shell.destination, Destination::NowPlaying);

    // Now Playing is the root; Settings modal opens over it.
    drop(update(&mut state, Message::Settings(SettingsMessage::Open)));
    assert!(state.shell.settings_open);
    assert_eq!(state.shell.destination, Destination::NowPlaying);
    drop(update(
      &mut state,
      Message::Settings(SettingsMessage::Close),
    ));
    assert!(!state.shell.settings_open);
    assert_eq!(state.shell.destination, Destination::NowPlaying);
  }

  #[test]
  fn entering_control_only_closes_settings_modal() {
    let mut state = test_state();
    state.shell.settings_open = true;
    drop(shell::apply_app_mode(
      &mut state,
      jellypilot_core::config::AppMode::ControlOnly,
    ));
    assert!(!state.shell.settings_open);
    assert_eq!(state.shell.destination, Destination::NowPlaying);
  }

  #[test]
  fn entering_control_only_aborts_browse_and_resets_library_surfaces() {
    let mut state = test_state();
    state.shell.window_size = iced::Size::new(1440.0, 810.0);
    state.shell.destination = Destination::Library {
      library_id: "movies".to_owned(),
      collection_type: "movies".to_owned(),
    };
    state.full.as_mut().unwrap().home.data.shortcuts =
      jellypilot_core::LoadState::Ready(vec![jellypilot_media_server::VideoLibraryShortcut {
        id: "movies".to_owned(),
        name: "Movies".to_owned(),
        collection_type: "movies".to_owned(),
        item_count: Some(1),
        artwork_image_id: None,
      }]);
    let source = shell::browse_source(&state).unwrap();
    let request = state
      .full
      .as_mut()
      .unwrap()
      .browse
      .data
      .configure(source)
      .unwrap()
      .into_iter()
      .find_map(|effect| match effect {
        jellypilot_core::browse_model::BrowseEffect::RequestPage(request) => Some(request),
        _ => None,
      })
      .unwrap();
    let (_task, handle) =
      Task::perform(std::future::pending::<Message>(), std::convert::identity).abortable();
    let cancellation = handle.clone();
    state
      .full
      .as_mut()
      .unwrap()
      .browse
      .page_tasks
      .insert(request.token, handle);

    drop(shell::apply_app_mode(
      &mut state,
      jellypilot_core::config::AppMode::ControlOnly,
    ));
    assert!(cancellation.is_aborted());
    assert_eq!(state.shell.destination, Destination::NowPlaying);
    assert!(state.shell.navigation_stack.is_empty());
    assert!(state.full.is_none());
  }

  #[test]
  fn entering_full_restores_home_and_consumes_the_stashed_size() {
    let mut state = test_state();
    state.shell.destination = Destination::NowPlaying;
    state.shell.window_id = Some(iced::window::Id::unique());
    state.shell.full_window_size = Some(iced::Size::new(1280.0, 800.0));

    drop(shell::apply_app_mode(
      &mut state,
      jellypilot_core::config::AppMode::Full,
    ));

    assert_eq!(state.shell.destination, Destination::Home);
    assert!(state.shell.navigation_stack.is_empty());
    assert_eq!(state.shell.full_window_size, None);
  }

  #[tokio::test]
  async fn settings_modal_preserves_home_scroll_after_open_and_close() {
    use iced::advanced::{renderer::Headless, widget};
    use iced_runtime::user_interface::{Cache, UserInterface};

    #[derive(Default)]
    struct PageScroll {
      set: Option<f32>,
      observed: Option<f32>,
    }
    impl widget::Operation for PageScroll {
      fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
        visit(self);
      }
      fn scrollable(
        &mut self,
        id: Option<&widget::Id>,
        _bounds: iced::Rectangle,
        _content: iced::Rectangle,
        translation: iced::Vector,
        scroll: &mut dyn widget::operation::Scrollable,
      ) {
        if id == Some(&widget::Id::new("home-page-loading")) {
          self.observed = Some(translation.y);
          if let Some(y) = self.set {
            scroll.scroll_to(widget::operation::scrollable::AbsoluteOffset {
              x: None,
              y: Some(y),
            });
          }
        }
      }
    }

    let mut state = test_state();
    state.kernel.connection = ConnectionPhase::Connected;
    state.full.as_mut().expect("full UI").home.data.begin_load();
    let bounds = iced::Size::new(1400.0, 600.0);
    state.shell.window_size = bounds;
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings {
        font: iced::Font::DEFAULT,
        text_size: 14.0.into(),
        line_height: fonts::DEFAULT_LINE_HEIGHT,
        metrics_hinting: false,
      },
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    let window = iced::window::Id::unique();
    let mut ui = UserInterface::build(
      crate::app::view(&state, window),
      bounds,
      Cache::new(),
      &mut renderer,
    );
    ui.operate(
      &renderer,
      &mut PageScroll {
        set: Some(120.0),
        observed: None,
      },
    );
    let mut before = PageScroll::default();
    ui.operate(&renderer, &mut before);
    assert_eq!(before.observed, Some(120.0));
    let cache = ui.into_cache();
    drop(update(&mut state, Message::Settings(SettingsMessage::Open)));
    let mut ui = UserInterface::build(
      crate::app::view(&state, window),
      bounds,
      cache,
      &mut renderer,
    );
    let mut obscured = PageScroll::default();
    ui.operate(&renderer, &mut obscured);
    assert_eq!(
      obscured.observed, None,
      "background stays excluded from operations"
    );
    let cache = ui.into_cache();
    drop(update(
      &mut state,
      Message::Settings(SettingsMessage::Close),
    ));
    let mut ui = UserInterface::build(
      crate::app::view(&state, window),
      bounds,
      cache,
      &mut renderer,
    );
    let mut after = PageScroll::default();
    ui.operate(&renderer, &mut after);
    assert_eq!(after.observed, before.observed);
    let cache = ui.into_cache();
    state.playback = active_intro_prompt_state().playback;
    state.shell.window_id = Some(window);
    drop(shell::toggle_player_fullscreen(&mut state));
    assert!(state.shell.player_fullscreen);
    let fullscreen_bounds = iced::Size::new(1920.0, 1080.0);
    drop(update(
      &mut state,
      Message::Window(WindowMessage::Resized(fullscreen_bounds)),
    ));
    let ui = UserInterface::build(
      crate::app::view(&state, window),
      fullscreen_bounds,
      cache,
      &mut renderer,
    );
    let cache = ui.into_cache();
    drop(update(
      &mut state,
      Message::Shell(crate::app::message::ShellMessage::ExitPlayerFullscreen),
    ));
    assert!(!state.shell.player_fullscreen);
    assert_eq!(state.shell.window_size, bounds);
    let mut ui = UserInterface::build(
      crate::app::view(&state, window),
      bounds,
      cache,
      &mut renderer,
    );
    let mut restored = PageScroll::default();
    ui.operate(&renderer, &mut restored);
    assert_eq!(
      restored.observed, before.observed,
      "fullscreen must preserve browser scroll"
    );
  }

  #[tokio::test]
  async fn backdrop_click_dismisses_only_the_top_modal_and_never_the_dialog_body() {
    use iced::advanced::renderer::Headless;
    use iced::{mouse, Event, Point, Size};
    use iced_runtime::user_interface::{Cache, UserInterface};

    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings {
        font: iced::Font::DEFAULT,
        text_size: 14.0.into(),
        line_height: fonts::DEFAULT_LINE_HEIGHT,
        metrics_hinting: false,
      },
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    for kind in [
      "settings",
      "add-account",
      "confirmation",
      "compact-settings",
    ] {
      let mut state = test_state();
      state.kernel.connection = ConnectionPhase::Connected;
      let bounds = if kind == "compact-settings" {
        Size::new(600.0, 600.0)
      } else {
        Size::new(1400.0, 900.0)
      };
      state.shell.window_size = bounds;
      state.shell.settings_open = true;
      if kind == "add-account" {
        drop(update(
          &mut state,
          Message::Account(accounts::Message::AddAccount),
        ));
      } else if kind == "confirmation" {
        drop(accounts::update(
          &mut state.accounts,
          &mut state.login.flow,
          &mut state.kernel,
          &state.watchlist,
          accounts::RuntimeFacts {
            quit_requested: false,
            playback_active: true,
          },
          accounts::Message::Disconnect,
        ));
      }
      let mut ui = UserInterface::build(
        crate::app::view(&state, iced::window::Id::unique()),
        bounds,
        Cache::new(),
        &mut renderer,
      );
      let dismisses = |message: &Message| {
        matches!(
          message,
          Message::Settings(SettingsMessage::Close)
            | Message::Account(
              accounts::Message::CloseAddAccount | accounts::Message::CancelConfirmation
            )
        )
      };
      let mut messages = Vec::new();
      update_ui(
        &mut ui,
        &mut renderer,
        &[
          Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
          Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ],
        mouse::Cursor::Available(Point::new(bounds.width / 2.0, bounds.height / 2.0)),
        &mut messages,
      );
      assert!(!messages.iter().any(dismisses), "dialog interior: {kind}");
      messages.clear();
      update_ui(
        &mut ui,
        &mut renderer,
        &[
          Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
          Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
        ],
        mouse::Cursor::Available(Point::new(5.0, 5.0)),
        &mut messages,
      );
      assert!(!messages.iter().any(dismisses), "secondary click: {kind}");
      messages.clear();
      let inside = Point::new(bounds.width / 2.0, bounds.height / 2.0);
      update_ui(
        &mut ui,
        &mut renderer,
        &[
          Event::Touch(iced::touch::Event::FingerPressed {
            id: iced::touch::Finger(1),
            position: inside,
          }),
          Event::Touch(iced::touch::Event::FingerLifted {
            id: iced::touch::Finger(1),
            position: inside,
          }),
        ],
        mouse::Cursor::Available(inside),
        &mut messages,
      );
      assert!(
        !messages.iter().any(dismisses),
        "touch inside dialog: {kind}"
      );
      messages.clear();
      let outside = Point::new(5.0, 5.0);
      update_ui(
        &mut ui,
        &mut renderer,
        &[
          Event::Touch(iced::touch::Event::FingerPressed {
            id: iced::touch::Finger(2),
            position: outside,
          }),
          Event::Touch(iced::touch::Event::FingerLifted {
            id: iced::touch::Finger(2),
            position: outside,
          }),
        ],
        mouse::Cursor::Unavailable,
        &mut messages,
      );
      assert_eq!(
        messages.iter().filter(|message| dismisses(message)).count(),
        usize::from(kind != "compact-settings"),
        "touch backdrop: {kind}"
      );
      messages.clear();
      update_ui(
        &mut ui,
        &mut renderer,
        &[
          Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
          Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ],
        mouse::Cursor::Available(Point::new(5.0, 5.0)),
        &mut messages,
      );
      drop(ui);
      match kind {
        "settings" => assert!(matches!(
          messages.as_slice(),
          [Message::Settings(SettingsMessage::Close)]
        )),
        "add-account" => assert!(matches!(
          messages.as_slice(),
          [Message::Account(accounts::Message::CloseAddAccount)]
        )),
        "confirmation" => assert!(matches!(
          messages.as_slice(),
          [Message::Account(accounts::Message::CancelConfirmation)]
        )),
        _ => assert!(messages.is_empty(), "fullscreen Settings has no backdrop"),
      }
      for message in messages {
        drop(update(&mut state, message));
      }
      assert_eq!(state.shell.settings_open, kind != "settings");
      assert!(!accounts::blocking_modal(&state.accounts));
    }
  }
}
