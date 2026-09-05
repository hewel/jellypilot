//! Top-level message router (ADR 0029): per-surface messages delegate to the
//! surface modules; navigation lives in the shell surface, and cross-surface
//! follow-ups (login connection transitions, settings playback snapshots,
//! window resize/close effects, tray window actions) are hoisted here.

use iced::Task;
use jellypilot_auth::login::ConnectionPhase;
use jellypilot_core::config::{AppMode, IntroMode, Settings};
use jellypilot_core::diagnostics::{DiagnosticCategory, DiagnosticLevel};
use jellypilot_mpv::playback_session::{PlaybackInput, PlaybackIntent};

use crate::tray::TrayAction;

use super::accounts;
use super::browse;
use super::detail;
use super::home;
use super::login;
use super::message::{
  BrowseMessage, DetailMessage, HomeMessage, Message, SettingsMessage, WindowMessage,
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
    tasks.push(home::start_load(
      &mut full.home,
      &mut state.kernel,
      state.playback.view.now_playing.is_none(),
    ));
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
  let previous_error = accounts::view(state).error.map(str::to_owned);
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
    .map(str::to_owned)
    .filter(|error| Some(error) != previous_error.as_ref())
  {
    state
      .kernel
      .diagnostics
      .record(DiagnosticLevel::Error, DiagnosticCategory::Auth, &error);
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
  Task::batch(tasks)
}

pub fn update(state: &mut State, message: Message) -> Task<Message> {
  match message {
    Message::Account(message) => update_account(state, message),
    Message::Shell(message) => {
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
          "Wait for the account change to finish before changing your lists.".to_owned(),
        );
      }
      let Some(full) = state.full.as_mut() else {
        return Task::none();
      };
      let task = super::personal_lists::update(
        &mut full.personal_lists,
        &mut state.kernel,
        &state.watchlist,
        message,
      );
      state.retain_artwork_handles();
      task
    }
    // Theme re-resolves every frame from state, so recording the OS mode is
    // the whole update; the next frame picks up the new effective theme.
    Message::SystemThemeDiscovered(mode) | Message::SystemThemeChanged(mode) => {
      state.system_theme = mode;
      Task::none()
    }
    Message::Window(message) => {
      // Cross-surface follow-ups hoisted out of the shell surface (ADR 0029):
      // a close without an available tray runs the playback quit handshake,
      // and a resize re-syncs the browse scroll window, after which handles
      // are retained here because retention reads every surface's slot set.
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
        state.retain_artwork_handles();
      }
      Task::batch(tasks)
    }
    Message::Login(message) => {
      let was_connected = state.kernel.connection == ConnectionPhase::Connected;
      let previous_error = state.login.flow.error.clone();
      let login_task = login::update(
        &mut state.login,
        &mut state.kernel,
        state.playback.view.can_start_login,
        message,
      );
      let is_connected = state.kernel.connection == ConnectionPhase::Connected;
      if state.login.flow.error != previous_error {
        if let Some(error) = &state.login.flow.error {
          state
            .kernel
            .diagnostics
            .record(DiagnosticLevel::Error, DiagnosticCategory::Auth, error);
        }
      }
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
      let reprepared_artwork = matches!(message, HomeMessage::Loaded { .. });
      let task = home::update(
        &mut full.home,
        &mut state.kernel,
        state.playback.view.now_playing.is_none(),
        state.shell.window_size.width,
        message,
      );
      if reprepared_artwork {
        state.retain_artwork_handles();
      }
      task
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
      let reprepared_artwork = matches!(
        message,
        BrowseMessage::PageSettled(_) | BrowseMessage::Scrolled(_)
      );
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
        state.playback.view.now_playing.is_none(),
        state.shell.window_size,
        message,
      );
      if reprepared_artwork {
        state.retain_artwork_handles();
      }
      if let Some(notice) = state
        .kernel
        .notice
        .clone()
        .filter(|notice| Some(notice.as_str()) != previous_notice.as_deref())
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
          "Wait for the account change to finish before changing your lists.".to_owned(),
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
          "Wait for the account change to finish before changing this item.".to_owned(),
        );
      }
      let Some(full) = state.full.as_mut() else {
        return Task::none();
      };
      let reprepared_artwork = matches!(
        message,
        DetailMessage::Loaded { .. }
          | DetailMessage::SeasonLoaded { .. }
          | DetailMessage::NeighborsLoaded { .. }
          | DetailMessage::SimilarLoaded { .. }
      );
      let detail_item_id = match &state.shell.destination {
        Destination::Detail(item_id) => Some(item_id.as_str()),
        _ => None,
      };
      let task = detail::update(&mut full.detail, &mut state.kernel, detail_item_id, message);
      if reprepared_artwork {
        state.retain_artwork_handles();
      }
      task
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
      let task = playback::update(
        &mut state.playback,
        &mut state.kernel,
        state.shell.quit_requested,
        message,
      );
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
    Message::ArtworkStreamCompleted(summary) => {
      if let Some(message) = summary.diagnostic_message() {
        state.kernel.diagnostics.record(
          DiagnosticLevel::Info,
          DiagnosticCategory::Artwork,
          message,
        );
      }
      Task::none()
    }
  }
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
  use jellypilot_media_server::artwork::ArtworkLoadSummary;
  use jellypilot_media_server::{JellyfinClient, MediaServerProvider, VideoLibraryItem};
  use jellypilot_mpv::playback::{
    Playable, PlaybackOutcome, PlaybackRefreshOutcome, PlaybackRefreshState, PlaybackSnapshot,
  };
  use jellypilot_mpv::playback_session::{
    ControllerCommand, ControllerSettlement, IntroAvailability, PlaybackEffect, PlaybackEvent,
  };
  use jellypilot_session::IntroSkipMode;

  use super::*;
  use crate::app::kernel::Kernel;
  use crate::app::state::{ArtworkCellState, LoginState, RemoteSessionHandle};

  fn test_state() -> State {
    let settings = SettingsStore::default();
    let mut request_gate = jellypilot_core::request_gate::RequestGate::default();
    let playback = playback::Surface::new(&mut request_gate);
    let settings_view = crate::app::state::SettingsState::from_settings(settings.snapshot());
    let login_flow = LoginState::from_settings(settings.snapshot());
    State {
      system_theme: iced::theme::Mode::None,
      kernel: Kernel {
        settings,
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
        artwork_binder: Default::default(),
        artwork_handles: Default::default(),
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
      }),
      playback,
      shell: shell::Surface::new(false),
      watchlist: super::super::personal_lists::Runtime::default(),
      accounts: super::super::accounts::Surface::new(),
    }
  }

  #[tokio::test]
  async fn account_settings_entry_keeps_its_focus_origin_when_reopened() {
    use iced::advanced::{clipboard, renderer::Headless, widget};
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
    let mut renderer = iced::Renderer::new(Font::DEFAULT, 14.0.into(), Some("tiny-skia"))
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
    ui.update(
      &[key(keyboard::key::Named::Tab)],
      mouse::Cursor::Unavailable,
      &mut renderer,
      &mut clipboard::Null,
      &mut messages,
    );
    ui.operate(
      &renderer,
      &mut widget::operation::focusable::focus::<()>(widget::Id::new("account-settings")),
    );
    ui.update(
      &[key(keyboard::key::Named::Enter)],
      mouse::Cursor::Unavailable,
      &mut renderer,
      &mut clipboard::Null,
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
    ui.update(
      &[key(keyboard::key::Named::Enter)],
      mouse::Cursor::Unavailable,
      &mut renderer,
      &mut clipboard::Null,
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
    ui.update(
      &[key(keyboard::key::Named::Enter)],
      mouse::Cursor::Unavailable,
      &mut renderer,
      &mut clipboard::Null,
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
        result: Ok(vec![jellypilot_session::IntroSkipRange {
          kind: jellypilot_session::IntroSkipKind::Introduction,
          start_seconds: 10.0,
          end_seconds: 30.0,
          notified: false,
          skipped: false,
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
  fn failure_produces_toast_that_clears_on_timeout_message() {
    let mut state = test_state();
    assert!(state.kernel.active_toast.is_none());

    let task = state
      .kernel
      .show_toast(NoticeLevel::Error, "Playback failed: decoder error");
    assert!(task.units() > 0);
    assert!(state.kernel.active_toast.is_some());
    let toast = state.kernel.active_toast.as_ref().unwrap();
    assert_eq!(toast.id, 1);
    assert_eq!(toast.message, "Playback failed: decoder error");
    assert_eq!(toast.level, NoticeLevel::Error);

    // Dismissing with matching ID clears the toast
    drop(update(&mut state, Message::DismissNotice(1)));
    assert!(state.kernel.active_toast.is_none());
  }

  #[test]
  fn newer_notice_replaces_older_and_older_timeout_does_not_dismiss_newer() {
    let mut state = test_state();

    drop(state.kernel.show_toast(NoticeLevel::Warning, "Warning 1"));
    assert_eq!(state.kernel.active_toast.as_ref().unwrap().id, 1);
    assert_eq!(
      state.kernel.active_toast.as_ref().unwrap().message,
      "Warning 1"
    );

    drop(state.kernel.show_toast(NoticeLevel::Error, "Error 2"));
    assert_eq!(state.kernel.active_toast.as_ref().unwrap().id, 2);
    assert_eq!(
      state.kernel.active_toast.as_ref().unwrap().message,
      "Error 2"
    );

    // Timeout from older notice (id: 1) arrives -> does NOT clear newer notice (id: 2)
    drop(update(&mut state, Message::DismissNotice(1)));
    assert!(state.kernel.active_toast.is_some());
    assert_eq!(state.kernel.active_toast.as_ref().unwrap().id, 2);

    // Timeout or manual dismiss for newer notice (id: 2) -> clears it
    drop(update(&mut state, Message::DismissNotice(2)));
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
        jellypilot_session::IntroSkipMode::Automatic,
      ),
      (
        jellypilot_core::config::IntroMode::Manual,
        jellypilot_session::IntroSkipMode::Manual,
      ),
      (
        jellypilot_core::config::IntroMode::Off,
        jellypilot_session::IntroSkipMode::Off,
      ),
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
  fn artwork_stream_completion_records_one_sanitized_aggregate_event() {
    let mut state = test_state();
    let summary = ArtworkLoadSummary {
      raster_loads: 1,
      memory_loads: 2,
      disk_loads: 1,
      network_loads: 3,
      failed_loads: 1,
      total_duration_millis: 120,
      total_bytes: 4096,
    };

    drop(update(&mut state, Message::ArtworkStreamCompleted(summary)));

    let artwork_events = state
      .kernel
      .diagnostics
      .rows()
      .filter(|row| row.category == DiagnosticCategory::Artwork)
      .count();
    assert_eq!(artwork_events, 1);

    // An empty summary records nothing.
    drop(update(
      &mut state,
      Message::ArtworkStreamCompleted(ArtworkLoadSummary::default()),
    ));
    let artwork_events = state
      .kernel
      .diagnostics
      .rows()
      .filter(|row| row.category == DiagnosticCategory::Artwork)
      .count();
    assert_eq!(artwork_events, 1);
  }

  #[test]
  fn browse_re_navigation_rebuilds_from_the_raster_cache() {
    let mut state = test_state();
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    let library = Destination::Library {
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

    drop(shell::navigate(&mut state, library.clone()));

    let mut item = episode("browse-item-1", 1);
    item.artwork_image_id = Some("browse-art-1".to_owned());
    state.full.as_mut().unwrap().browse.view = LibraryBrowseView::Ready {
      visible_items: vec![jellypilot_core::browse_model::LibraryItemSlot {
        item: Some(item.clone()),
      }],
      visible_start: 0,
      mode: jellypilot_core::LibraryBrowseMode::Normal,
      total_record_count: 1,
      is_fetching_more: false,
      load_more_failure: None,
      retry_busy: false,
    };

    // Mirrors the router: a re-prepared pipeline is followed by cross-surface
    // handle retention (ADR 0029).
    drop(browse::prepare_artwork(
      &mut state.full.as_mut().unwrap().browse,
      &mut state.kernel,
      state.shell.window_size.width,
    ));
    state.retain_artwork_handles();
    let slot = state
      .full
      .as_mut()
      .unwrap()
      .browse
      .artwork
      .get("browse-item-1")
      .unwrap()
      .slot;
    let session = state.kernel.request_gate.current_session();
    // A real load stores the raster in the adapter cache; seed it beside the
    // synthesized completion so re-navigation can rebuild from it.
    state.kernel.artwork_adapter.seed_raster_for_test(
      "browse-art-1",
      jellypilot_media_server::artwork::ArtworkSizeClass::Card,
      jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(1, 1, vec![1, 2, 3, 4]),
    );
    drop(browse::update(
      &mut state.full.as_mut().unwrap().browse,
      &mut state.kernel,
      None,
      false,
      state.playback.view.now_playing.is_none(),
      state.shell.window_size,
      BrowseMessage::ArtworkLoaded {
        session,
        slot,
        image_id: "browse-art-1".to_owned(),
        result: Ok(
          jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(
            1,
            1,
            vec![1, 2, 3, 4],
          ),
        ),
      },
    ));
    assert_eq!(
      state
        .full
        .as_ref()
        .unwrap()
        .browse
        .artwork
        .get("browse-item-1")
        .map(|cell| cell.state),
      Some(ArtworkCellState::Ready)
    );
    assert!(state
      .kernel
      .artwork_handles
      .get(slot, "browse-art-1")
      .is_some());

    // Navigate away to Home
    drop(shell::navigate(&mut state, Destination::Home));

    // Return to Browse
    drop(shell::navigate(&mut state, library));
    state.full.as_mut().unwrap().browse.view = LibraryBrowseView::Ready {
      visible_items: vec![jellypilot_core::browse_model::LibraryItemSlot { item: Some(item) }],
      visible_start: 0,
      mode: jellypilot_core::LibraryBrowseMode::Normal,
      total_record_count: 1,
      is_fetching_more: false,
      load_more_failure: None,
      retry_busy: false,
    };
    drop(browse::prepare_artwork(
      &mut state.full.as_mut().unwrap().browse,
      &mut state.kernel,
      state.shell.window_size.width,
    ));
    state.retain_artwork_handles();

    let browse_cell = state
      .full
      .as_ref()
      .unwrap()
      .browse
      .artwork
      .get("browse-item-1")
      .expect("browse cell exists");
    assert_eq!(browse_cell.state, ArtworkCellState::Ready);
    // The handle is rebuilt synchronously from the raster cache; there is no
    // cross-navigation handle identity to preserve.
    assert!(state
      .kernel
      .artwork_handles
      .get(browse_cell.slot, "browse-art-1")
      .is_some());
  }

  #[test]
  fn reselecting_active_played_filter_keeps_loaded_artwork() {
    let mut state = test_state();
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
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
    // Seed the raster cache before settlement: the settlement's own artwork
    // preparation then settles the poster synchronously from cache.
    state.kernel.artwork_adapter.seed_raster_for_test(
      "browse-art-1",
      jellypilot_media_server::artwork::ArtworkSizeClass::Card,
      jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(1, 1, vec![1, 2, 3, 4]),
    );
    let mut item = episode("browse-item-1", 1);
    item.artwork_image_id = Some("browse-art-1".to_owned());
    drop(browse::update(
      &mut state.full.as_mut().unwrap().browse,
      &mut state.kernel,
      None,
      false,
      state.playback.view.now_playing.is_none(),
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

    state.retain_artwork_handles();
    assert_eq!(
      state
        .full
        .as_ref()
        .unwrap()
        .browse
        .artwork
        .get("browse-item-1")
        .map(|cell| cell.state),
      Some(ArtworkCellState::Ready)
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
    assert_eq!(cell.state, ArtworkCellState::Ready);
    assert!(state
      .kernel
      .artwork_handles
      .get(cell.slot, "browse-art-1")
      .is_some());
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
    state.full.as_mut().unwrap().browse.view = LibraryBrowseView::Loading;
    let (_task, handle) = Task::<Message>::none().abortable();
    state.full.as_mut().unwrap().browse.page_tasks.insert(
      jellypilot_core::LibraryBrowseLoadToken {
        generation: 1,
        sequence: 1,
      },
      handle,
    );

    drop(shell::apply_app_mode(
      &mut state,
      jellypilot_core::config::AppMode::ControlOnly,
    ));
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
}
