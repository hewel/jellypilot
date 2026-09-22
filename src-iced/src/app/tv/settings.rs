//! Living-room settings presentation over the shared preference and account reducers.

use iced::widget::{
  button, column, container, mouse_area, opaque, row, rule, scrollable, space, stack, text, Column,
};
use iced::{Alignment, Element, Fill, Task};
use jellypilot_core::config::{IntroMode, LocalPreference, PlaybackBackend, ThemeMode, UiMode};
use jellypilot_core::locale::{LanguagePreference, UiLanguage};
use jellypilot_core::request_gate::SessionToken;
use jellypilot_core::tv_navigation::Input;
use jellypilot_ui::fonts::HEADING_FONT;
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::tv as style;
use jellypilot_ui::widgets::tv_focus::focus;

use crate::app::accounts;
use crate::app::message::{Message as AppMessage, SettingsMessage};
use crate::app::state::State;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Category {
  Account,
  #[default]
  Playback,
  Audio,
  Appearance,
  Remote,
  Storage,
  About,
}

impl Category {
  const ALL: [Self; 7] = [
    Self::Account,
    Self::Playback,
    Self::Audio,
    Self::Appearance,
    Self::Remote,
    Self::Storage,
    Self::About,
  ];
  const fn key(self) -> &'static str {
    match self {
      Self::Account => "tv-settings-account",
      Self::Playback => "tv-settings-playback",
      Self::Audio => "tv-settings-audio",
      Self::Appearance => "tv-settings-appearance",
      Self::Remote => "tv-settings-remote",
      Self::Storage => "tv-settings-storage",
      Self::About => "tv-settings-about",
    }
  }
  const fn icon(self) -> Icon {
    match self {
      Self::Account => Icon::User,
      Self::Playback => Icon::Play,
      Self::Audio => Icon::Subtitles,
      Self::Appearance => Icon::Settings,
      Self::Remote => Icon::Keyboard,
      Self::Storage => Icon::Database,
      Self::About => Icon::Info,
    }
  }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Focus {
  Category(usize),
  #[default]
  FirstRow,
  Row(usize),
}
impl Focus {
  fn row(self) -> Option<usize> {
    match self {
      Self::FirstRow => Some(0),
      Self::Row(index) => Some(index),
      Self::Category(_) => None,
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Setting {
  Intro,
  Sync,
  Decoder,
  Cache,
  Passthrough,
  Subtitle,
  Language,
  Theme,
  Backend,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Change {
  Intro(IntroMode),
  AutoNext(bool),
  Sync(u64),
  Mpv {
    name: &'static str,
    value: &'static str,
  },
  OriginalAudio(bool),
  SeasonVolume(bool),
  ImageCache(bool),
  AutoLogin(bool),
  ReducedMotion(bool),
  Language(LanguagePreference),
  Theme(ThemeMode),
  Backend(PlaybackBackend),
  Subtitles(Vec<String>),
}

impl Change {
  fn message(&self) -> AppMessage {
    let message = match self {
      Self::Intro(mode) => {
        SettingsMessage::LocalPreferenceSelected(LocalPreference::IntroMode(*mode))
      }
      Self::AutoNext(enabled) => SettingsMessage::AutoNextEpisodeChanged(*enabled),
      Self::Sync(seconds) => SettingsMessage::ProgressSyncSecondsSelected(*seconds),
      Self::Mpv { name, value } => SettingsMessage::MpvOptionSelected { name, value },
      Self::OriginalAudio(enabled) => {
        SettingsMessage::LocalPreferenceSelected(LocalPreference::PreferOriginalAudio(*enabled))
      }
      Self::SeasonVolume(enabled) => {
        SettingsMessage::LocalPreferenceSelected(LocalPreference::RememberSeasonVolume(*enabled))
      }
      Self::ImageCache(enabled) => {
        SettingsMessage::LocalPreferenceSelected(LocalPreference::ImageCache(*enabled))
      }
      Self::AutoLogin(enabled) => {
        SettingsMessage::LocalPreferenceSelected(LocalPreference::AutoLogin(*enabled))
      }
      Self::ReducedMotion(enabled) => {
        SettingsMessage::LocalPreferenceSelected(LocalPreference::ReducedMotion(*enabled))
      }
      Self::Language(preference) => return AppMessage::UiLanguageSelected(*preference),
      Self::Theme(mode) => {
        SettingsMessage::LocalPreferenceSelected(LocalPreference::ThemeMode(*mode))
      }
      Self::Backend(backend) => {
        SettingsMessage::LocalPreferenceSelected(LocalPreference::PlaybackBackend(*backend))
      }
      Self::Subtitles(languages) => SettingsMessage::LocalPreferenceSelected(
        LocalPreference::SubtitleLanguages(languages.clone()),
      ),
    };
    AppMessage::Settings(message)
  }

  fn saved(&self, state: &State) -> bool {
    let saved = state.kernel.settings.snapshot();
    match self {
      Self::Intro(value) => saved.intro_mode() == *value,
      Self::AutoNext(value) => saved.auto_next_episode() == *value,
      Self::Sync(value) => saved.progress_sync_seconds() == *value,
      Self::Mpv { name, value } => mpv_option(state, name).as_deref() == Some(value),
      Self::OriginalAudio(value) => saved.prefer_original_audio() == *value,
      Self::SeasonVolume(value) => saved.remember_season_volume() == *value,
      Self::ImageCache(value) => saved.image_cache_enabled() == *value,
      Self::AutoLogin(value) => saved.auto_login() == *value,
      Self::ReducedMotion(value) => saved.reduced_motion() == *value,
      Self::Language(value) => saved.ui_language() == *value,
      Self::Theme(value) => saved.theme_mode() == *value,
      Self::Backend(value) => saved.playback_backend() == *value,
      Self::Subtitles(value) => saved.subtitle_languages() == value,
    }
  }
}

#[derive(Clone)]
pub enum Action {
  Choose(Setting),
  Change(Change),
  Category(Category),
  Account(accounts::Message),
  Export,
  CaptureLogs(bool),
  Desktop,
  Close,
  Unavailable,
}

#[derive(Clone)]
pub enum Message {
  Close,
  Category(Category),
  Focus(Focus),
  Activate {
    row: usize,
    action: Action,
  },
  Choice(usize),
  Retry,
  Apply {
    generation: u64,
    session: SessionToken,
  },
}

struct Choice {
  label: String,
  detail: String,
  change: Change,
}
struct Selector {
  kind: Setting,
  title: String,
  choices: Vec<Choice>,
  selected: Option<usize>,
  cursor: usize,
}
#[derive(Default)]
enum Save {
  #[default]
  Idle,
  Pending(Change),
  Failed {
    change: Change,
    error: String,
  },
}

#[derive(Default)]
pub struct Surface {
  pub open: bool,
  pub category: Category,
  pub focus: Focus,
  selector: Option<Selector>,
  save: Save,
  generation: u64,
  saved: bool,
}

pub fn open(state: &mut State) -> Task<AppMessage> {
  open_category(state, Category::Playback)
}

pub fn open_category(state: &mut State, category: Category) -> Task<AppMessage> {
  state.tv.settings.open = true;
  state.tv.settings.category = category;
  state.tv.settings.focus = Focus::FirstRow;
  state.tv.settings.selector = None;
  state.tv.settings.save = Save::Idle;
  state.tv.settings.saved = false;
  state.tv.settings.generation = state.tv.settings.generation.wrapping_add(1);
  reveal(state)
}

pub fn close(state: &mut State) {
  state.tv.settings.open = false;
  state.tv.settings.selector = None;
  state.tv.settings.save = Save::Idle;
  state.tv.settings.generation = state.tv.settings.generation.wrapping_add(1);
}

fn message(value: Message) -> AppMessage {
  AppMessage::Tv(super::Message::Settings(value))
}

pub fn update(state: &mut State, event: Message) -> Task<AppMessage> {
  if !state.tv.settings.open
    || !state.tv_mode()
    || state.shell.quit_requested
    || accounts::content_mutations_blocked(&state.kernel)
    || accounts::blocking_modal(&state.accounts)
  {
    return Task::none();
  }
  if matches!(state.tv.settings.save, Save::Pending(_)) && !matches!(event, Message::Apply { .. }) {
    return Task::none();
  }
  match event {
    Message::Close => {
      if state.tv.settings.selector.take().is_some()
        || matches!(state.tv.settings.save, Save::Failed { .. })
      {
        state.tv.settings.save = Save::Idle;
      } else {
        close(state);
      }
      Task::none()
    }
    Message::Category(category)
      if state.tv.settings.selector.is_none() && matches!(state.tv.settings.save, Save::Idle) =>
    {
      state.tv.settings.category = category;
      state.tv.settings.focus = Focus::FirstRow;
      state.tv.settings.saved = false;
      reveal(state)
    }
    Message::Focus(focused)
      if state.tv.settings.selector.is_none() && matches!(state.tv.settings.save, Save::Idle) =>
    {
      state.tv.settings.focus = focused;
      Task::none()
    }
    Message::Activate { row, action }
      if state.tv.settings.selector.is_none() && matches!(state.tv.settings.save, Save::Idle) =>
    {
      state.tv.settings.focus = Focus::Row(row);
      match action {
        Action::Choose(setting) => {
          state.tv.settings.selector = Some(selector(state, setting));
          Task::none()
        }
        Action::Change(change) => begin_save(state, change),
        Action::Category(category) => update(state, Message::Category(category)),
        Action::Account(event) => {
          crate::app::update::route_message(state, AppMessage::Account(event))
        }
        Action::Export => crate::app::update::route_message(
          state,
          AppMessage::Settings(SettingsMessage::ExportLogs),
        ),
        Action::CaptureLogs(enabled) => crate::app::update::route_message(
          state,
          AppMessage::Settings(SettingsMessage::PlayerLogCaptureChanged(enabled)),
        ),
        Action::Close => {
          close(state);
          Task::none()
        }
        Action::Desktop => {
          crate::app::update::route_message(state, AppMessage::UiModeSelected(UiMode::Desktop))
        }
        Action::Unavailable => Task::none(),
      }
    }
    Message::Choice(index) if matches!(state.tv.settings.save, Save::Idle) => {
      let change = state.tv.settings.selector.as_mut().and_then(|selector| {
        if index >= selector.choices.len() {
          return None;
        }
        selector.cursor = index;
        Some(selector.choices[index].change.clone())
      });
      change.map_or_else(Task::none, |change| begin_save(state, change))
    }
    Message::Retry => {
      let Save::Failed { change, .. } = &state.tv.settings.save else {
        return Task::none();
      };
      begin_save(state, change.clone())
    }
    Message::Apply {
      generation,
      session,
    } => {
      if generation != state.tv.settings.generation
        || session != state.kernel.request_gate.current_session()
      {
        return Task::none();
      }
      let Save::Pending(change) = &state.tv.settings.save else {
        return Task::none();
      };
      let change = change.clone();
      if change.saved(state) {
        finish_save(state, &change);
        return Task::none();
      }
      state.settings.view.error = None;
      let task = crate::app::update::route_message(state, change.message());
      finish_save(state, &change);
      task.chain(selector_scroll(state))
    }
    _ => Task::none(),
  }
}

fn begin_save(state: &mut State, change: Change) -> Task<AppMessage> {
  state.tv.settings.generation = state.tv.settings.generation.wrapping_add(1);
  state.tv.settings.save = Save::Pending(change);
  state.tv.settings.saved = false;
  Task::done(message(Message::Apply {
    generation: state.tv.settings.generation,
    session: state.kernel.request_gate.current_session(),
  }))
}

fn finish_save(state: &mut State, change: &Change) {
  if change.saved(state) {
    state.tv.settings.save = Save::Idle;
    state.tv.settings.selector = None;
    state.tv.settings.saved = true;
  } else {
    let error = state.settings.view.error.as_ref().map_or_else(
      || state.t("settings-save-error"),
      |error| state.kernel.locale.message(error),
    );
    state.tv.settings.save = Save::Failed {
      change: change.clone(),
      error,
    };
  }
}

pub fn input(state: &mut State, input: Input) -> Task<AppMessage> {
  if input == Input::Back {
    return update(state, Message::Close);
  }
  if !matches!(state.tv.settings.save, Save::Idle) {
    return if input == Input::Confirm {
      update(state, Message::Retry)
    } else {
      Task::none()
    };
  }
  if let Some(selector) = state.tv.settings.selector.as_mut() {
    match input {
      Input::Up => selector.cursor = selector.cursor.saturating_sub(1),
      Input::Down => {
        selector.cursor = (selector.cursor + 1).min(selector.choices.len().saturating_sub(1))
      }
      Input::Confirm => {
        let cursor = selector.cursor;
        return update(state, Message::Choice(cursor));
      }
      _ => {}
    }
    return selector_scroll(state);
  }
  let count = rows(state).len();
  match state.tv.settings.focus {
    Focus::Category(index) => match input {
      Input::Up => state.tv.settings.focus = Focus::Category(index.saturating_sub(1)),
      Input::Down => {
        state.tv.settings.focus = Focus::Category((index + 1).min(Category::ALL.len() - 1))
      }
      Input::Confirm => return update(state, Message::Category(Category::ALL[index])),
      Input::Right => state.tv.settings.focus = Focus::FirstRow,
      Input::Left => {
        close(state);
      }
      _ => {}
    },
    focused => {
      let index = focused.row().unwrap_or(0);
      match input {
        Input::Up => state.tv.settings.focus = Focus::Row(index.saturating_sub(1)),
        Input::Down => {
          state.tv.settings.focus = Focus::Row((index + 1).min(count.saturating_sub(1)))
        }
        Input::Left => {
          state.tv.settings.focus = Focus::Category(
            Category::ALL
              .iter()
              .position(|category| *category == state.tv.settings.category)
              .unwrap_or(0),
          )
        }
        Input::Confirm => {
          if let Some(row) = rows(state).into_iter().nth(index) {
            return update(
              state,
              Message::Activate {
                row: index,
                action: row.action,
              },
            );
          }
        }
        _ => {}
      }
    }
  }
  reveal(state)
}

pub fn reconcile(state: &mut State) -> Task<AppMessage> {
  if state.tv.settings.open {
    if let Some(index) = state.tv.settings.focus.row() {
      state.tv.settings.focus = Focus::Row(index.min(rows(state).len().saturating_sub(1)));
    }
  }
  Task::none()
}

fn reveal(state: &State) -> Task<AppMessage> {
  let index = state.tv.settings.focus.row().unwrap_or(0);
  let scale = style::scale(state.shell.window_size.width);
  // The standard eight rows fit at reference size; longer profile lists scroll by row.
  iced::widget::operation::scroll_to(
    "tv-settings-detail",
    iced::widget::operation::AbsoluteOffset {
      x: 0.0,
      y: index.saturating_sub(6) as f32 * 88.0 * scale,
    },
  )
}

struct SettingRow {
  label: String,
  value: String,
  help: String,
  action: Action,
  toggle: Option<bool>,
  group: Option<&'static str>,
}
impl SettingRow {
  fn new(state: &State, label: &str, value: String, help: &str, action: Action) -> Self {
    Self {
      label: state.t(label),
      value,
      help: if help.is_empty() {
        String::new()
      } else {
        state.t(help)
      },
      action,
      toggle: None,
      group: None,
    }
  }
  fn switch(mut self, value: bool) -> Self {
    self.toggle = Some(value);
    self
  }
  fn group(mut self, key: &'static str) -> Self {
    self.group = Some(key);
    self
  }
}

fn on_off(state: &State, value: bool) -> String {
  state.t(if value {
    "tv-settings-on"
  } else {
    "tv-settings-off"
  })
}
fn mpv_option(state: &State, name: &str) -> Option<String> {
  state
    .kernel
    .settings
    .snapshot()
    .mpv_args()
    .iter()
    .rev()
    .find_map(|argument| {
      let (key, value) = argument.trim_start_matches('-').split_once('=')?;
      (key == name).then(|| value.to_owned())
    })
}
fn decoder_label(state: &State) -> String {
  state.t(match mpv_option(state, "hwdec").as_deref() {
    Some("no") => "tv-settings-software",
    Some(_) => "tv-settings-hardware",
    None => "tv-settings-player-default",
  })
}
fn cache_label(state: &State) -> String {
  mpv_option(state, "demuxer-max-bytes").unwrap_or_else(|| "256MiB".to_owned())
}
fn passthrough_supported(state: &State) -> bool {
  crate::embedded::options().is_none()
    && state.kernel.settings.snapshot().playback_backend() == PlaybackBackend::External
}
fn language_label(state: &State, language: LanguagePreference) -> String {
  match language {
    LanguagePreference::System => state.t("settings-system"),
    LanguagePreference::Fixed(UiLanguage::English) => "English".to_owned(),
    LanguagePreference::Fixed(UiLanguage::SimplifiedChinese) => "简体中文".to_owned(),
  }
}
fn subtitle_label(state: &State, code: &str) -> String {
  let key = match code {
    "eng" => "english",
    "spa" => "spanish",
    "fra" => "french",
    "deu" => "german",
    "ita" => "italian",
    "por" => "portuguese",
    "jpn" => "japanese",
    "zho" => "chinese",
    _ => return code.to_owned(),
  };
  state.t(&format!("settings-subtitle-{key}"))
}

fn rows(state: &State) -> Vec<SettingRow> {
  let settings = state.kernel.settings.snapshot();
  let choice = |label, value, help, setting| {
    SettingRow::new(state, label, value, help, Action::Choose(setting))
  };
  let toggle = |label, value, help, change| {
    SettingRow::new(
      state,
      label,
      on_off(state, value),
      help,
      Action::Change(change),
    )
    .switch(value)
  };
  match state.tv.settings.category {
    Category::Playback => vec![
      choice(
        "tv-settings-intro",
        state.t(if settings.intro_mode() == IntroMode::Automatic {
          "settings-intro-automatic"
        } else {
          "settings-intro-manual"
        }),
        "tv-settings-intro-help",
        Setting::Intro,
      )
      .group("tv-settings-daily"),
      toggle(
        "tv-settings-auto-next",
        settings.auto_next_episode(),
        "tv-settings-auto-next-help",
        Change::AutoNext(!settings.auto_next_episode()),
      ),
      choice(
        "tv-settings-sync",
        state.format(
          "tv-settings-seconds",
          &[("seconds", settings.progress_sync_seconds().into())],
        ),
        "tv-settings-sync-help",
        Setting::Sync,
      ),
      choice(
        "tv-settings-decoder",
        decoder_label(state),
        "tv-settings-decoder-help",
        Setting::Decoder,
      )
      .group("tv-settings-advanced"),
      SettingRow::new(
        state,
        "tv-settings-enhancement",
        state.t("tv-settings-unavailable"),
        "tv-settings-enhancement-help",
        Action::Unavailable,
      ),
      SettingRow::new(
        state,
        "tv-settings-refresh",
        state.t("tv-settings-unavailable"),
        "tv-settings-refresh-help",
        Action::Unavailable,
      ),
      SettingRow::new(
        state,
        "tv-settings-passthrough",
        state.t("tv-settings-audio"),
        "",
        Action::Category(Category::Audio),
      )
      .group("tv-settings-related"),
      SettingRow::new(
        state,
        "tv-settings-preload",
        state.t("tv-settings-storage"),
        "",
        Action::Category(Category::Storage),
      ),
    ],
    Category::Audio => {
      let mut rows = vec![
        toggle(
          "settings-original-audio",
          settings.prefer_original_audio(),
          "tv-settings-original-audio-help",
          Change::OriginalAudio(!settings.prefer_original_audio()),
        ),
        toggle(
          "settings-season-volume",
          settings.remember_season_volume(),
          "tv-settings-season-volume-help",
          Change::SeasonVolume(!settings.remember_season_volume()),
        ),
        SettingRow::new(
          state,
          "tv-settings-passthrough",
          if passthrough_supported(state) {
            on_off(
              state,
              mpv_option(state, "audio-spdif").is_some_and(|value| !value.is_empty()),
            )
          } else {
            state.t("tv-settings-unavailable")
          },
          if passthrough_supported(state) {
            "tv-settings-passthrough-help"
          } else {
            "tv-settings-passthrough-unavailable"
          },
          if passthrough_supported(state) {
            Action::Choose(Setting::Passthrough)
          } else {
            Action::Unavailable
          },
        ),
        choice(
          "tv-settings-subtitle-priority",
          settings
            .subtitle_languages()
            .iter()
            .map(|code| subtitle_label(state, code))
            .collect::<Vec<_>>()
            .join(" · "),
          "settings-subtitles-help",
          Setting::Subtitle,
        )
        .group("settings-subtitles"),
      ];
      for (index, language) in settings.subtitle_languages().iter().enumerate() {
        let mut reordered = settings.subtitle_languages().to_vec();
        reordered.remove(index);
        rows.push(SettingRow {
          label: state.format(
            "tv-settings-remove-language",
            &[("language", subtitle_label(state, language).into())],
          ),
          value: String::new(),
          help: state.t("settings-subtitles-help"),
          action: Action::Change(Change::Subtitles(reordered)),
          toggle: None,
          group: None,
        });
      }
      rows
    }
    Category::Appearance => vec![
      choice(
        "tv-settings-language",
        language_label(state, settings.ui_language()),
        "",
        Setting::Language,
      ),
      toggle(
        "settings-reduce-motion",
        settings.reduced_motion(),
        "tv-settings-motion-help",
        Change::ReducedMotion(!settings.reduced_motion()),
      ),
      choice(
        "tv-settings-desktop-theme",
        state.t(match settings.theme_mode() {
          ThemeMode::System => "settings-system",
          ThemeMode::Dark => "settings-dark",
          ThemeMode::Light => "settings-light",
        }),
        "tv-settings-theme-help",
        Setting::Theme,
      ),
      choice(
        "settings-player-backend",
        state.t(
          if settings.playback_backend() == PlaybackBackend::Embedded {
            "tv-settings-embedded"
          } else {
            "settings-player-external"
          },
        ),
        "tv-settings-backend-help",
        Setting::Backend,
      ),
      SettingRow::new(
        state,
        "tv-settings-exit",
        String::new(),
        "",
        Action::Desktop,
      ),
    ],
    Category::Remote => vec![SettingRow::new(
      state,
      "tv-settings-back",
      String::new(),
      "",
      Action::Close,
    )],
    Category::Storage => vec![
      toggle(
        "settings-image-cache",
        settings.image_cache_enabled(),
        "settings-image-cache-help",
        Change::ImageCache(!settings.image_cache_enabled()),
      ),
      choice(
        "tv-settings-preload",
        cache_label(state),
        "tv-settings-cache-help",
        Setting::Cache,
      ),
    ],
    Category::About => vec![
      SettingRow::new(
        state,
        "settings-player-log-capture",
        on_off(state, jellypilot_core::player_logs::global().enabled()),
        "tv-settings-capture-help",
        Action::CaptureLogs(!jellypilot_core::player_logs::global().enabled()),
      )
      .switch(jellypilot_core::player_logs::global().enabled()),
      SettingRow::new(
        state,
        "settings-export-logs",
        String::new(),
        "tv-settings-export-help",
        Action::Export,
      ),
    ],
    Category::Account => {
      let account = accounts::view(state);
      let mut rows = vec![toggle(
        "account-auto-login",
        settings.auto_login(),
        "account-auto-login-description",
        Change::AutoLogin(!settings.auto_login()),
      )];
      if let Some(current) = account.current {
        rows.push(SettingRow::new(
          state,
          "account-copy-server-address",
          match account.copy_status {
            accounts::CopyStatus::Idle => current.server_url.to_owned(),
            accounts::CopyStatus::Copied => state.t("account-address-copied"),
            accounts::CopyStatus::Failed => state.t("account-copy-failed"),
          },
          "",
          Action::Account(accounts::Message::CopyServerAddress),
        ));
      }
      for profile in account.profiles {
        rows.push(SettingRow {
          label: profile.user_name.clone(),
          value: if account.active_key == Some(&profile.key) {
            state.t("account-connected")
          } else {
            profile
              .server_name
              .clone()
              .unwrap_or_else(|| profile.server_url.clone())
          },
          help: profile.server_url.clone(),
          action: Action::Account(accounts::Message::SwitchProfile(profile.key.clone())),
          toggle: None,
          group: None,
        });
        rows.push(SettingRow {
          label: state.format(
            "tv-settings-signout-profile",
            &[("name", profile.user_name.clone().into())],
          ),
          value: String::new(),
          help: state.t("tv-settings-signout-help"),
          action: Action::Account(accounts::Message::AskSignOut(profile.key.clone())),
          toggle: None,
          group: None,
        });
      }
      rows.push(SettingRow::new(
        state,
        "account-add",
        String::new(),
        "",
        Action::Account(accounts::Message::AddAccount),
      ));
      rows.push(SettingRow::new(
        state,
        "account-disconnect",
        String::new(),
        "",
        Action::Account(accounts::Message::Disconnect),
      ));
      if account.can_retry_handoff_cleanup {
        rows.push(SettingRow::new(
          state,
          "tv-settings-retry-cleanup",
          String::new(),
          "",
          Action::Account(accounts::Message::RetryHandoffCleanup),
        ));
      }
      if account.can_retry_watchlist_cleanup {
        rows.push(SettingRow::new(
          state,
          "tv-settings-retry-list-cleanup",
          String::new(),
          "",
          Action::Account(accounts::Message::RetryWatchlistCleanup),
        ));
      }
      rows
    }
  }
}

fn selector(state: &State, setting: Setting) -> Selector {
  let saved = state.kernel.settings.snapshot();
  let entry = |label: String, detail: &str, change: Change| Choice {
    label,
    detail: if detail.is_empty() {
      String::new()
    } else {
      state.t(detail)
    },
    change,
  };
  let (key, choices) = match setting {
    Setting::Intro => (
      "tv-settings-intro",
      vec![
        entry(
          state.t("settings-intro-automatic"),
          "",
          Change::Intro(IntroMode::Automatic),
        ),
        entry(
          state.t("settings-intro-manual"),
          "",
          Change::Intro(IntroMode::Manual),
        ),
      ],
    ),
    Setting::Sync => (
      "tv-settings-sync",
      [3, 5, 10, 30]
        .into_iter()
        .map(|seconds| {
          entry(
            state.format("tv-settings-seconds", &[("seconds", seconds.into())]),
            "",
            Change::Sync(seconds),
          )
        })
        .collect(),
    ),
    Setting::Decoder => (
      "tv-settings-decoder",
      vec![
        entry(
          state.t("tv-settings-hardware"),
          "tv-settings-hardware-help",
          Change::Mpv {
            name: "hwdec",
            value: "auto-safe",
          },
        ),
        entry(
          state.t("tv-settings-software"),
          "tv-settings-software-help",
          Change::Mpv {
            name: "hwdec",
            value: "no",
          },
        ),
      ],
    ),
    Setting::Cache => (
      "tv-settings-preload",
      ["128MiB", "256MiB", "512MiB", "1GiB"]
        .into_iter()
        .map(|value| {
          entry(
            value.to_owned(),
            "tv-settings-cache-help",
            Change::Mpv {
              name: "demuxer-max-bytes",
              value,
            },
          )
        })
        .collect(),
    ),
    Setting::Passthrough => (
      "tv-settings-passthrough",
      vec![
        entry(
          state.t("tv-settings-off"),
          "",
          Change::Mpv {
            name: "audio-spdif",
            value: "",
          },
        ),
        entry(
          state.t("tv-settings-on"),
          "tv-settings-passthrough-help",
          Change::Mpv {
            name: "audio-spdif",
            value: "ac3,dts,eac3,truehd,dts-hd",
          },
        ),
      ],
    ),
    Setting::Language => (
      "tv-settings-language",
      [
        LanguagePreference::System,
        LanguagePreference::Fixed(UiLanguage::English),
        LanguagePreference::Fixed(UiLanguage::SimplifiedChinese),
      ]
      .into_iter()
      .map(|value| entry(language_label(state, value), "", Change::Language(value)))
      .collect(),
    ),
    Setting::Theme => (
      "tv-settings-desktop-theme",
      [
        ("settings-system", ThemeMode::System),
        ("settings-dark", ThemeMode::Dark),
        ("settings-light", ThemeMode::Light),
      ]
      .into_iter()
      .map(|(key, value)| entry(state.t(key), "tv-settings-theme-help", Change::Theme(value)))
      .collect(),
    ),
    Setting::Backend => {
      let mut choices = vec![entry(
        state.t("settings-player-external"),
        "tv-settings-backend-help",
        Change::Backend(PlaybackBackend::External),
      )];
      if cfg!(target_os = "linux") {
        choices.push(entry(
          state.t("tv-settings-embedded"),
          "tv-settings-backend-help",
          Change::Backend(PlaybackBackend::Embedded),
        ));
      }
      ("settings-player-backend", choices)
    }
    Setting::Subtitle => (
      "tv-settings-subtitle-priority",
      jellypilot_core::settings::SUBTITLE_LANGUAGE_OPTIONS
        .into_iter()
        .map(|language| {
          let mut languages = saved.subtitle_languages().to_vec();
          languages.retain(|existing| existing != language);
          languages.insert(0, language.to_owned());
          entry(
            subtitle_label(state, language),
            "tv-settings-subtitle-first-help",
            Change::Subtitles(languages),
          )
        })
        .collect(),
    ),
  };
  let selected = choices
    .iter()
    .position(|choice| choice.change.saved(state))
    .or_else(|| match setting {
      Setting::Decoder if mpv_option(state, "hwdec").is_some() => Some(usize::from(
        mpv_option(state, "hwdec").as_deref() == Some("no"),
      )),
      Setting::Cache if mpv_option(state, "demuxer-max-bytes").is_none() => Some(1),
      Setting::Passthrough if mpv_option(state, "audio-spdif").is_none() => Some(0),
      _ => None,
    });
  Selector {
    kind: setting,
    title: state.t(key),
    choices,
    selected,
    cursor: selected.unwrap_or(0),
  }
}

pub fn view(state: &State) -> Element<'_, AppMessage> {
  let scale = style::scale(state.shell.window_size.width);
  let values = rows(state);
  let modal = state.tv.settings.selector.is_some() || !matches!(state.tv.settings.save, Save::Idle);
  let help = state
    .tv
    .settings
    .focus
    .row()
    .and_then(|index| values.get(index))
    .map(|row| row.help.clone())
    .unwrap_or_default();
  let mut categories = Column::new().spacing(8.0 * scale).width(Fill);
  for (index, category) in Category::ALL.into_iter().enumerate() {
    let focused = !modal && state.tv.settings.focus == Focus::Category(index);
    let label = state.t(category.key());
    categories = categories.push(focus(focused, move |progress| {
      button(
        row![
          icon_with_color(
            category.icon(),
            IconSize::Custom(28.0 * scale),
            style::foreground(
              style::PALETTE,
              progress,
              category == state.tv.settings.category
            )
          ),
          text(label.clone())
            .size(style::BODY * scale)
            .width(Fill)
            .color(style::foreground(
              style::PALETTE,
              progress,
              category == state.tv.settings.category
            ))
        ]
        .spacing(14.0 * scale)
        .align_y(Alignment::Center),
      )
      .width(Fill)
      .height(80.0 * scale)
      .padding([0.0, 16.0 * scale])
      .style(style::button_progress(
        style::PALETTE,
        progress,
        category == state.tv.settings.category,
      ))
      .on_press(message(Message::Category(category)))
      .into()
    }));
  }
  let categories = column![
    categories,
    space::vertical().height(Fill),
    text(help)
      .size(style::META * scale)
      .color(style::PALETTE.text.metadata)
      .width(Fill)
      .height(112.0 * scale)
  ]
  .spacing(24.0 * scale)
  .width(360.0 * scale)
  .height(Fill);
  let mut detail = Column::new().width(Fill);
  if state.tv.settings.category == Category::Remote {
    for (label, value) in [
      ("tv-settings-directions", "tv-settings-navigation"),
      ("tv-settings-confirm", "tv-settings-activate"),
      ("tv-settings-back", "tv-settings-previous"),
      ("tv-settings-playpause", "tv-settings-space"),
    ] {
      detail = detail.push(
        container(
          row![
            text(state.t(label)).size(style::BODY * scale).width(Fill),
            text(state.t(value)).size(style::META * scale)
          ]
          .spacing(24.0 * scale),
        )
        .padding([20.0 * scale, 24.0 * scale]),
      );
    }
    detail = detail.push(
      container(
        text(state.t("tv-settings-remote-help"))
          .size(style::META * scale)
          .color(style::PALETTE.text.metadata),
      )
      .padding(24.0 * scale),
    );
  }
  if state.tv.settings.category == Category::About {
    detail = detail.push(
      container(
        text(state.format(
          "settings-version",
          &[("version", env!("CARGO_PKG_VERSION").into())],
        ))
        .size(style::BODY * scale),
      )
      .padding([20.0 * scale, 24.0 * scale]),
    );
  }
  for (index, item) in values.into_iter().enumerate() {
    if let Some(group) = item.group {
      if group == "tv-settings-related" {
        detail = detail.push(space::vertical().height(24.0 * scale));
      } else {
        detail = detail.push(
          container(
            text(state.t(group))
              .size(style::SECTION * scale)
              .font(HEADING_FONT)
              .color(style::PALETTE.text.body),
          )
          .padding(iced::Padding {
            top: if index == 0 { 0.0 } else { 24.0 * scale },
            bottom: 12.0 * scale,
            ..Default::default()
          }),
        );
      }
    }
    detail = detail.push(setting_row(state, index, item, !modal, scale));
  }
  if state.tv.settings.category == Category::About {
    if let Some(saved) = &state.settings.view.saved {
      detail = detail.push(text(state.kernel.locale.message(saved)).size(style::META * scale));
    }
    if let Some(error) = &state.settings.view.error {
      detail = detail.push(
        text(state.kernel.locale.message(error))
          .size(style::META * scale)
          .color(style::PALETTE.colors.error),
      );
    }
  }
  if state.tv.settings.category == Category::Account {
    if let Some(error) = accounts::view(state).error {
      detail = detail.push(
        text(state.kernel.locale.message(error))
          .size(style::META * scale)
          .color(style::PALETTE.colors.error),
      );
    }
  }
  let content = column![
    row![
      text(state.t("tv-settings-title"))
        .size(style::TITLE * scale)
        .font(HEADING_FONT)
        .color(style::PALETTE.text.heading)
        .width(Fill),
      super::view::clock(scale)
    ]
    .spacing(40.0 * scale)
    .align_y(Alignment::Center),
    row![
      categories,
      scrollable(detail)
        .id("tv-settings-detail")
        .width(Fill)
        .height(Fill)
    ]
    .spacing(40.0 * scale)
    .height(Fill),
    row![
      text(if modal {
        String::new()
      } else {
        focus_hint(state)
      })
      .size(style::META * scale)
      .line_height(iced::Pixels(28.0 * scale))
      .color(style::PALETTE.text.metadata)
      .width(Fill)
      .height(28.0 * scale),
      text(if modal {
        String::new()
      } else {
        state.t(if state.tv.settings.saved {
          "settings-saved"
        } else {
          "tv-settings-autosave"
        })
      })
      .size(style::META * scale)
      .color(style::PALETTE.text.metadata)
    ]
    .spacing(24.0 * scale),
  ]
  .spacing(28.0 * scale)
  .width(Fill)
  .height(Fill);
  if modal {
    stack![
      content,
      opaque(
        container(selector_view(state, scale))
          .width(Fill)
          .height(Fill)
          .align_x(Alignment::End)
      )
    ]
    .into()
  } else {
    content.into()
  }
}

fn setting_row(
  state: &State,
  index: usize,
  item: SettingRow,
  active: bool,
  scale: f32,
) -> Element<'_, AppMessage> {
  let focused = active && state.tv.settings.focus.row() == Some(index);
  focus(focused, move |progress| {
    let foreground = style::foreground(style::PALETTE, progress, false);
    let trailing: Element<'_, AppMessage> = if let Some(enabled) = item.toggle {
      row![
        text(item.value.clone())
          .size(style::META * scale)
          .color(style::secondary_foreground(style::PALETTE, progress)),
        icon_with_color(
          if enabled {
            Icon::CircleCheck
          } else {
            Icon::Circle
          },
          IconSize::Custom(28.0 * scale),
          if enabled {
            style::PALETTE.colors.secondary
          } else {
            style::secondary_foreground(style::PALETTE, progress)
          }
        )
      ]
      .spacing(12.0 * scale)
      .align_y(Alignment::Center)
      .into()
    } else {
      let mut content = row![text(item.value.clone())
        .size(style::META * scale)
        .color(style::secondary_foreground(style::PALETTE, progress))]
      .spacing(12.0 * scale)
      .align_y(Alignment::Center);
      if !matches!(item.action, Action::Unavailable) {
        content = content.push(icon_with_color(
          Icon::ChevronRight,
          IconSize::Custom(24.0 * scale),
          foreground,
        ));
      }
      content.into()
    };
    let control = button(
      row![
        text(item.label.clone())
          .size(style::BODY * scale)
          .color(foreground)
          .width(Fill),
        container(trailing).width(iced::Length::Shrink.max(360.0 * scale))
      ]
      .spacing(16.0 * scale)
      .align_y(Alignment::Center),
    )
    .width(Fill)
    .height(
      if state.tv.settings.category == Category::Playback && index >= 6 {
        64.0 * scale
      } else {
        80.0 * scale
      },
    )
    .padding([0.0, 24.0 * scale])
    .style(style::button_progress(style::PALETTE, progress, false))
    .on_press(message(Message::Activate {
      row: index,
      action: item.action.clone(),
    }));
    mouse_area(control)
      .on_enter(message(Message::Focus(Focus::Row(index))))
      .into()
  })
  .into()
}

fn focus_hint(state: &State) -> String {
  let target = match state.tv.settings.focus {
    Focus::Category(index) => {
      return Category::ALL
        .get(index)
        .map_or_else(String::new, |category| {
          state.format(
            "tv-settings-hint-open",
            &[("target", state.t(category.key()).into())],
          )
        })
    }
    focused => rows(state).into_iter().nth(focused.row().unwrap_or(0)),
  };
  let Some(target) = target else {
    return String::new();
  };
  let key = match &target.action {
    Action::Choose(_) => "tv-settings-hint-choose",
    Action::Change(_) | Action::CaptureLogs(_) if target.toggle.is_some() => {
      if target.toggle == Some(true) {
        "tv-settings-hint-turn-off"
      } else {
        "tv-settings-hint-turn-on"
      }
    }
    Action::Account(accounts::Message::SwitchProfile(_)) => "tv-settings-hint-switch",
    Action::Unavailable | Action::Close => return state.t("tv-settings-hint-back"),
    _ => "tv-settings-hint-action",
  };
  state.format(key, &[("target", target.label.into())])
}

fn selector_hint(state: &State) -> String {
  match &state.tv.settings.save {
    Save::Pending(_) => state.t("tv-settings-saving"),
    Save::Failed { .. } => state.t("tv-settings-hint-retry"),
    Save::Idle => state
      .tv
      .settings
      .selector
      .as_ref()
      .and_then(|selector| selector.choices.get(selector.cursor))
      .map_or_else(String::new, |choice| {
        state.format(
          "tv-settings-hint-apply",
          &[("target", choice.label.clone().into())],
        )
      }),
  }
}

fn selector_viewport_height(state: &State, scale: f32) -> f32 {
  // Keep 40px padding, the 45px hint rail, and 24px separation outside scrolling.
  (state.shell.window_size.height - 2.0 * style::SAFE_Y * scale - 149.0 * scale).max(0.0)
}

fn selector_scroll(state: &State) -> Task<AppMessage> {
  let scale = style::scale(state.shell.window_size.width);
  let y = if matches!(state.tv.settings.save, Save::Failed { .. }) {
    f32::MAX
  } else if let Some(selector) = &state.tv.settings.selector {
    let header = if selector.selected.is_some() {
      124.0
    } else {
      72.0
    };
    (header * scale + (selector.cursor as f32 * 128.0 + 104.0) * scale
      - selector_viewport_height(state, scale))
    .max(0.0)
  } else {
    return Task::none();
  };
  iced::widget::operation::scroll_to(
    "tv-settings-selector",
    iced::widget::operation::AbsoluteOffset { x: 0.0, y },
  )
}

fn selector_view(state: &State, scale: f32) -> Element<'_, AppMessage> {
  let surface = &state.tv.settings;
  let mut content = Column::new().spacing(24.0 * scale).width(Fill);
  if let Some(selector) = &surface.selector {
    content = content.push(
      text(selector.title.clone())
        .size(style::TITLE * scale)
        .line_height(iced::Pixels(48.0 * scale))
        .font(HEADING_FONT),
    );
    if let Some(saved) = selector
      .selected
      .and_then(|index| selector.choices.get(index))
    {
      content = content.push(
        text(state.format(
          "tv-settings-saved-value",
          &[("value", saved.label.clone().into())],
        ))
        .size(style::META * scale)
        .line_height(iced::Pixels(28.0 * scale))
        .color(style::PALETTE.text.metadata),
      );
    }
    for (index, choice) in selector.choices.iter().enumerate() {
      let selected = Some(index) == selector.selected;
      let focused = index == selector.cursor && matches!(surface.save, Save::Idle);
      let failed = matches!(&surface.save, Save::Failed { change, .. } if change == &choice.change);
      let detail = if failed {
        state.t("tv-settings-not-applied")
      } else {
        choice.detail.clone()
      };
      content = content.push(focus(focused, move |progress| {
        let mut labels = column![text(choice.label.clone())
          .size(style::BODY * scale)
          .line_height(iced::Pixels(32.0 * scale))
          .color(style::foreground(style::PALETTE, progress, selected))]
        .spacing(4.0 * scale);
        if !detail.is_empty() {
          labels = labels.push(
            text(detail.clone())
              .size(style::META * scale)
              .line_height(iced::Pixels(28.0 * scale))
              .color(style::secondary_foreground(style::PALETTE, progress)),
          );
        }
        container(
          button(
            row![
              icon_with_color(
                if selected {
                  Icon::CircleDot
                } else {
                  Icon::Circle
                },
                IconSize::Custom(28.0 * scale),
                if selected {
                  style::PALETTE.colors.secondary
                } else {
                  style::secondary_foreground(style::PALETTE, progress)
                }
              ),
              labels.width(Fill)
            ]
            .spacing(20.0 * scale)
            .align_y(Alignment::Center),
          )
          .width(Fill)
          .height(104.0 * scale)
          .padding([20.0 * scale, 24.0 * scale])
          .style(style::button_progress(style::PALETTE, progress, selected))
          .on_press(message(Message::Choice(index))),
        )
        .id(format!("tv-settings-choice-{index}"))
        .into()
      }));
    }
    if selector.kind == Setting::Decoder && matches!(surface.save, Save::Idle) {
      content = content.push(
        text(state.t("tv-settings-decoder-impact"))
          .size(style::META * scale)
          .line_height(iced::Pixels(32.0 * scale))
          .color(style::PALETTE.text.metadata),
      );
    }
  }
  match &surface.save {
    Save::Pending(_) => {
      content = content.push(
        text(state.t("tv-settings-saving"))
          .size(style::BODY * scale)
          .line_height(iced::Pixels(32.0 * scale)),
      )
    }
    Save::Failed { error, .. } => {
      let mut explanation = column![
        text(state.t("tv-settings-save-failed"))
          .size(style::BODY * scale)
          .line_height(iced::Pixels(32.0 * scale))
          .color(style::PALETTE.colors.error),
        text(state.t("tv-settings-save-retained"))
          .size(style::META * scale)
          .line_height(iced::Pixels(32.0 * scale)),
      ]
      .spacing(8.0 * scale);
      if error != &state.t("settings-save-error") {
        explanation = explanation.push(
          text(error.clone())
            .size(style::META * scale)
            .line_height(iced::Pixels(32.0 * scale)),
        );
      }
      content = content.push(explanation).push(
        container(
          button(
            text(state.t("tv-settings-retry"))
              .size(style::BODY * scale)
              .line_height(iced::Pixels(32.0 * scale)),
          )
          .width(220.0 * scale)
          .height(64.0 * scale)
          .style(style::button(style::PALETTE, true, false))
          .on_press(message(Message::Retry)),
        )
        .id("tv-settings-retry"),
      );
    }
    Save::Idle => {}
  }
  let hints = column![
    rule::horizontal(scale).style(jellypilot_ui::widgets::library::divider),
    container(
      row![
        text(selector_hint(state))
          .size(style::META * scale)
          .line_height(iced::Pixels(28.0 * scale)),
        mouse_area(
          text(state.t("tv-settings-cancel"))
            .size(style::META * scale)
            .line_height(iced::Pixels(28.0 * scale))
        )
        .on_press(message(Message::Close)),
      ]
      .spacing(32.0 * scale)
      .align_y(Alignment::Center)
    )
    .padding(iced::Padding {
      top: 16.0 * scale,
      ..Default::default()
    }),
  ];
  let panel = column![
    scrollable(content)
      .id("tv-settings-selector")
      .height(iced::Length::Shrink.max(selector_viewport_height(state, scale))),
    hints,
  ]
  .spacing(24.0 * scale);
  container(panel)
    .id("tv-settings-selector-panel")
    .padding(40.0 * scale)
    .width(768.0 * scale)
    .height(iced::Length::Shrink)
    .style(style::panel)
    .into()
}

#[cfg(test)]
mod tests {
  use super::*;
  use jellypilot_core::config::SettingsStore;

  fn state(name: &str) -> State {
    let mut state = crate::app::update::tests::test_state();
    state.shell.ui_mode = UiMode::Tv;
    state.kernel.connection = jellypilot_auth::login::ConnectionPhase::Connected;
    state.kernel.settings = SettingsStore::for_test(std::env::temp_dir().join(format!(
      "jellypilot-tv-settings-{name}-{}.json",
      std::process::id()
    )));
    let _ = open(&mut state);
    state
  }

  fn apply_pending(state: &mut State) {
    let _ = update(
      state,
      Message::Apply {
        generation: state.tv.settings.generation,
        session: state.kernel.request_gate.current_session(),
      },
    );
  }

  #[test]
  fn decoder_focus_and_cancel_do_not_mutate_the_saved_preference() {
    let mut state = state("decoder-cancel");
    state
      .kernel
      .settings
      .set_mpv_option("hwdec", "auto-safe")
      .unwrap();
    let _ = update(
      &mut state,
      Message::Activate {
        row: 3,
        action: Action::Choose(Setting::Decoder),
      },
    );
    let _ = input(&mut state, Input::Down);
    let selector = state.tv.settings.selector.as_ref().unwrap();
    assert_eq!((selector.selected, selector.cursor), (Some(0), 1));
    let _ = input(&mut state, Input::Back);
    assert_eq!(mpv_option(&state, "hwdec").as_deref(), Some("auto-safe"));
    assert!(state.tv.settings.selector.is_none());
    assert_eq!(state.tv.settings.focus, Focus::Row(3));
  }

  #[test]
  fn failed_decoder_save_retains_selection_and_retry_applies_the_same_change() {
    let mut state = state("decoder-retry");
    let directory = std::env::temp_dir().join(format!(
      "jellypilot-tv-settings-retry-{}",
      std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.json");
    state.kernel.settings = SettingsStore::for_test(path.clone());
    state
      .kernel
      .settings
      .set_mpv_option("hwdec", "auto-safe")
      .unwrap();
    let saved = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    let _ = update(
      &mut state,
      Message::Activate {
        row: 3,
        action: Action::Choose(Setting::Decoder),
      },
    );
    let _ = input(&mut state, Input::Down);
    let _ = input(&mut state, Input::Confirm);
    assert!(matches!(state.tv.settings.save, Save::Pending(_)));
    let generation = state.tv.settings.generation;
    let _ = input(&mut state, Input::Confirm);
    assert_eq!(
      state.tv.settings.generation, generation,
      "pending OK cannot queue another write"
    );
    apply_pending(&mut state);
    assert!(matches!(state.tv.settings.save, Save::Failed { .. }));
    assert_eq!(mpv_option(&state, "hwdec").as_deref(), Some("auto-safe"));
    assert_eq!(
      state.tv.settings.selector.as_ref().unwrap().selected,
      Some(0)
    );
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, saved).unwrap();
    let _ = input(&mut state, Input::Confirm);
    apply_pending(&mut state);
    assert_eq!(mpv_option(&state, "hwdec").as_deref(), Some("no"));
    assert!(state.tv.settings.selector.is_none());
    assert!(state.tv.settings.saved);
    std::fs::remove_dir_all(directory).unwrap();
  }

  #[test]
  fn closing_a_page_fences_queued_changes_and_returns_to_its_source_focus() {
    let mut state = state("stale-save");
    state.tv.focus = super::super::Focus::HeroDetail;
    let change = Change::AutoNext(false);
    let _ = begin_save(&mut state, change);
    let generation = state.tv.settings.generation;
    let session = state.kernel.request_gate.current_session();
    close(&mut state);
    let _ = open(&mut state);
    let _ = update(
      &mut state,
      Message::Apply {
        generation,
        session,
      },
    );
    assert!(state.kernel.settings.snapshot().auto_next_episode());
    assert_eq!(state.tv.focus, super::super::Focus::HeroDetail);
  }

  #[test]
  fn columns_bound_navigation_and_back_closes_innermost_layer_first() {
    let mut state = state("columns");
    let _ = input(&mut state, Input::Up);
    assert_eq!(state.tv.settings.focus, Focus::Row(0));
    let _ = input(&mut state, Input::Left);
    assert_eq!(state.tv.settings.focus, Focus::Category(1));
    let _ = input(&mut state, Input::Right);
    let _ = input(&mut state, Input::Confirm);
    assert!(state.tv.settings.selector.is_some());
    let _ = input(&mut state, Input::Back);
    assert!(state.tv.settings.open);
    let _ = input(&mut state, Input::Back);
    assert!(!state.tv.settings.open);
  }

  #[tokio::test]
  async fn decoder_panel_fits_content_and_keeps_retry_reachable_in_short_windows() {
    use iced::advanced::{renderer, renderer::Headless, widget};
    use iced::futures::StreamExt;
    use iced::{Rectangle, Size, Vector};
    use iced_runtime::user_interface::{Cache, UserInterface};

    #[derive(Default)]
    struct Bounds {
      panel: Option<Rectangle>,
      option: Option<Rectangle>,
      retry: Option<Rectangle>,
      scroll: Option<(Rectangle, Rectangle, Vector)>,
    }

    impl widget::Operation for Bounds {
      fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
        visit(self);
      }

      fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle) {
        if id == Some(&widget::Id::new("tv-settings-selector-panel")) {
          self.panel = Some(bounds);
        } else if id == Some(&widget::Id::new("tv-settings-choice-1")) {
          self.option = Some(bounds);
        } else if id == Some(&widget::Id::new("tv-settings-retry")) {
          self.retry = Some(bounds);
        }
      }

      fn scrollable(
        &mut self,
        id: Option<&widget::Id>,
        bounds: Rectangle,
        content: Rectangle,
        translation: Vector,
        _state: &mut dyn widget::operation::Scrollable,
      ) {
        if id == Some(&widget::Id::new("tv-settings-selector")) {
          self.scroll = Some((bounds, content, translation));
        }
      }
    }

    let mut renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .expect("headless layout renderer");
    let mut state = state("decoder-layout");
    state.kernel.locale = crate::i18n::Localizer::new(UiLanguage::SimplifiedChinese);
    state
      .kernel
      .settings
      .set_mpv_option("hwdec", "auto-safe")
      .unwrap();
    let _ = update(
      &mut state,
      Message::Activate {
        row: 3,
        action: Action::Choose(Setting::Decoder),
      },
    );
    let _ = input(&mut state, Input::Down);
    for height in [1080.0, 540.0] {
      state.shell.window_size = Size::new(1920.0, height);
      for failed in [false, true] {
        state.tv.settings.save = if failed {
          Save::Failed {
            change: Change::Mpv {
              name: "hwdec",
              value: "no",
            },
            error: state.t("settings-save-error"),
          }
        } else {
          Save::Idle
        };
        let mut ui = UserInterface::build(
          super::super::view::view(&state),
          state.shell.window_size,
          Cache::default(),
          &mut renderer,
        );
        if let Some(mut stream) = iced_runtime::task::into_stream(selector_scroll(&state)) {
          while let Some(action) = stream.next().await {
            if let iced_runtime::Action::Widget(mut operation) = action {
              ui.operate(&renderer, operation.as_mut());
            }
          }
        }
        let mut bounds = Bounds::default();
        ui.operate(&renderer, &mut bounds);
        let panel = bounds.panel.expect("decoder panel");
        let (viewport, content, translation) = bounds.scroll.expect("decoder options viewport");
        assert_eq!((panel.x, panel.y, panel.width), (1056.0, 60.0, 768.0));
        assert_eq!(bounds.option.expect("software option").height, 104.0);
        assert!(panel.y + panel.height <= height - 60.0);
        if height == 1080.0 {
          assert_eq!(panel.height, if failed { 689.0 } else { 561.0 });
          assert_eq!(viewport.height, content.height);
        } else {
          assert_eq!(panel.height, 420.0);
          assert!(content.height > viewport.height);
        }
        let focused = if failed {
          bounds.retry.expect("retry")
        } else {
          bounds.option.unwrap()
        };
        let focused_y = focused.y - translation.y;
        assert!(focused_y >= viewport.y);
        assert!(focused_y + focused.height <= viewport.y + viewport.height);
      }
    }
  }
}
