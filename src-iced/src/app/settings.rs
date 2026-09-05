//! Settings surface (ADR 0029): MPV/playback-target/intro/subtitle preference
//! edits, shortcut capture, diagnostic filters, and inline save feedback.

use iced::Task;
use jellypilot_core::config::{SettingsMutationError, ThemeMode};
use jellypilot_core::diagnostics::{DiagnosticCategory, DiagnosticLevel};
use jellypilot_ui::theme::ThemeMode as UiThemeMode;

use super::kernel::Kernel;
use super::message::{Message, SettingsMessage};
use super::state::SettingsState;

const SETTINGS_SAVE_ERROR: &str = "Could not save settings.";
const INVALID_LOGIN_PREFILL_ERROR: &str = "Server and username are required.";
const INVALID_PROVIDER_ERROR: &str = "The selected provider is invalid.";
const INVALID_SUBTITLE_LANGUAGE_ERROR: &str = "Choose a valid subtitle language.";
const DUPLICATE_SUBTITLE_LANGUAGE_ERROR: &str = "That subtitle language is already in the list.";
const EMPTY_SHORTCUT_ERROR: &str = "Press a non-modifier key for this shortcut.";
const SHORTCUT_COLLISION_ERROR: &str = "That shortcut is already assigned.";
const PLAYBACK_CONFIG_APPLY_ERROR: &str = "Settings were saved, but MPV could not be reconfigured.";
const LOG_EXPORT_ERROR: &str = "Could not export logs.";

/// Settings surface slice: the editable form state behind the Settings page.
pub struct Surface {
  pub view: SettingsState,
}

/// Cross-surface follow-ups are hoisted to the top-level router (ADR 0029):
/// mutations that change playback-relevant settings reconfigure playback,
/// re-feed the intro mode, or refinalize the remote target there, and
/// Disconnect/SignOut write the login and remote-session state there. This
/// entry point only ever mutates the settings slice and the kernel.
pub fn update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  effective_theme_mode: UiThemeMode,
  message: SettingsMessage,
) -> Task<Message> {
  update_settings(surface, kernel, effective_theme_mode, message)
}

fn update_settings(
  surface: &mut Surface,
  kernel: &mut Kernel,
  effective_theme_mode: UiThemeMode,
  message: SettingsMessage,
) -> Task<Message> {
  match message {
    SettingsMessage::MpvPathChanged(value) => {
      surface.view.mpv_path_input = value;
      clear_settings_feedback(surface);
      Task::none()
    }
    SettingsMessage::SaveMpvPath => {
      let value = surface.view.mpv_path_input.clone();
      let result = kernel.settings.set_mpv_path(value);
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::MpvArgsChanged(value) => {
      surface.view.mpv_args_input = value;
      clear_settings_feedback(surface);
      Task::none()
    }
    SettingsMessage::SaveMpvArgs => {
      let value = surface.view.mpv_args_input.clone();
      let result = kernel.settings.set_mpv_args(&value);
      if finish_settings_mutation(surface, kernel, result) {
        surface.view.mpv_args_input = kernel.settings.snapshot().mpv_args().join(" ");
      }
      Task::none()
    }
    SettingsMessage::PlaybackTargetNameChanged(value) => {
      surface.view.playback_target_name_input = value;
      clear_settings_feedback(surface);
      Task::none()
    }
    SettingsMessage::SavePlaybackTargetName => {
      let value = surface.view.playback_target_name_input.clone();
      let result = kernel.settings.set_playback_target_name(value);
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::IntroMenuToggled => {
      surface.view.intro_menu_open = !surface.view.intro_menu_open;
      Task::none()
    }
    SettingsMessage::IntroMenuDismissed => {
      surface.view.intro_menu_open = false;
      Task::none()
    }
    SettingsMessage::IntroModeSelected(mode) => {
      surface.view.intro_menu_open = false;
      let result = kernel.settings.set_intro_mode(mode);
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::ThemeModeSelected(mode) => {
      let result = kernel.settings.set_theme_mode(mode);
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::ThemeTogglePressed => {
      let mode = match effective_theme_mode {
        UiThemeMode::Dark => ThemeMode::Light,
        UiThemeMode::Light => ThemeMode::Dark,
      };
      let result = kernel.settings.set_theme_mode(mode);
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::AppModeSelected(mode) => {
      let result = kernel.settings.set_app_mode(mode);
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::SubtitleMenuToggled => {
      surface.view.subtitle_menu_open = !surface.view.subtitle_menu_open;
      Task::none()
    }
    SettingsMessage::SubtitleMenuDismissed => {
      surface.view.subtitle_menu_open = false;
      Task::none()
    }
    SettingsMessage::SubtitleLanguageAdded(language) => {
      surface.view.subtitle_menu_open = false;
      let result = kernel.settings.add_subtitle_language(language);
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::SubtitleLanguageMoved { index, offset } => {
      let result = kernel.settings.move_subtitle_language(index, offset);
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::SubtitleLanguageRemoved(index) => {
      let result = kernel.settings.remove_subtitle_language(index);
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::BeginShortcutCapture(kind) => {
      surface.view.shortcut_capture = Some(kind);
      clear_settings_feedback(surface);
      Task::none()
    }
    SettingsMessage::ShortcutCaptured(binding) => {
      let Some(kind) = surface.view.shortcut_capture.take() else {
        return Task::none();
      };
      let result = kernel.settings.set_shortcut(kind, binding);
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::CancelShortcutCapture => {
      surface.view.shortcut_capture = None;
      Task::none()
    }
    SettingsMessage::ImageCacheToggled => {
      let enabled = !kernel.settings.snapshot().image_cache_enabled();
      let result = kernel.settings.set_image_cache_enabled(enabled);
      if finish_settings_mutation(surface, kernel, result) {
        kernel.artwork_adapter.set_disk_cache_enabled(enabled);
      }
      Task::none()
    }
    SettingsMessage::AutoLoginToggled => {
      let enabled = !kernel.settings.snapshot().auto_login();
      let result = kernel.settings.set_auto_login(enabled);
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::StartMinimizedToggled => {
      let enabled = !kernel.settings.snapshot().start_minimized();
      let result = kernel.settings.set_start_minimized(enabled);
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::ReducedMotionToggled => {
      let enabled = !kernel.settings.snapshot().reduced_motion();
      let result = kernel.settings.set_reduced_motion(enabled);
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::DiagnosticLevelMenuToggled => {
      surface.view.diagnostic_level_menu_open = !surface.view.diagnostic_level_menu_open;
      Task::none()
    }
    SettingsMessage::DiagnosticLevelMenuDismissed => {
      surface.view.diagnostic_level_menu_open = false;
      Task::none()
    }
    SettingsMessage::DiagnosticLevelSelected(level) => {
      surface.view.diagnostic_level = level;
      surface.view.diagnostic_level_menu_open = false;
      Task::none()
    }
    SettingsMessage::DiagnosticCategoryMenuToggled => {
      surface.view.diagnostic_category_menu_open = !surface.view.diagnostic_category_menu_open;
      Task::none()
    }
    SettingsMessage::DiagnosticCategoryMenuDismissed => {
      surface.view.diagnostic_category_menu_open = false;
      Task::none()
    }
    SettingsMessage::DiagnosticCategorySelected(category) => {
      surface.view.diagnostic_category = category;
      surface.view.diagnostic_category_menu_open = false;
      Task::none()
    }
    SettingsMessage::ExportLogs => {
      clear_settings_feedback(surface);
      let exported_at = jellypilot_core::logs::now_seconds();
      let document = jellypilot_core::logs::build_support_document(
        env!("CARGO_PKG_VERSION"),
        exported_at,
        kernel.diagnostics.rows(),
        &jellypilot_core::logs::global().snapshot(),
      );
      Task::perform(
        async move {
          jellypilot_core::logs::write_support_document(&document, exported_at)
            .map(|path| path.display().to_string())
            .map_err(|_| LOG_EXPORT_ERROR.to_owned())
        },
        |result| Message::Settings(SettingsMessage::LogsExported(result)),
      )
    }
    SettingsMessage::LogsExported(result) => {
      match result {
        Ok(path) => {
          surface.view.saved = Some("Logs exported");
          kernel.diagnostics.record(
            DiagnosticLevel::Info,
            DiagnosticCategory::Config,
            format!("Logs exported to {path}."),
          );
        }
        Err(message) => {
          surface.view.error = Some(LOG_EXPORT_ERROR);
          kernel
            .diagnostics
            .record(DiagnosticLevel::Error, DiagnosticCategory::Config, message);
        }
      }
      Task::none()
    }
    // Handled entirely by the top-level router: Disconnect and SignOut write
    // the remote session state (and SignOut drives the login surface's forget flow).
    // Open and Close drive the shell's Settings Modal lifecycle.
    SettingsMessage::Open
    | SettingsMessage::Close
    | SettingsMessage::Disconnect
    | SettingsMessage::SignOut => Task::none(),
    SettingsMessage::PlaybackConfigApplied(result) => {
      if result.is_err() {
        surface.view.error = Some(PLAYBACK_CONFIG_APPLY_ERROR);
        kernel.diagnostics.record(
          DiagnosticLevel::Error,
          DiagnosticCategory::Config,
          PLAYBACK_CONFIG_APPLY_ERROR,
        );
      }
      Task::none()
    }
  }
}

fn clear_settings_feedback(surface: &mut Surface) {
  surface.view.error = None;
  surface.view.saved = None;
}

fn finish_settings_mutation(
  surface: &mut Surface,
  kernel: &mut Kernel,
  result: Result<bool, SettingsMutationError>,
) -> bool {
  match result {
    Ok(changed) => {
      surface.view.error = None;
      surface.view.saved = Some("Saved");
      if changed {
        kernel.diagnostics.record(
          DiagnosticLevel::Info,
          DiagnosticCategory::Config,
          "Settings updated.",
        );
      }
      changed
    }
    Err(error) => {
      surface.view.saved = None;
      surface.view.error = Some(settings_mutation_error(&error));
      kernel.diagnostics.record(
        DiagnosticLevel::Error,
        DiagnosticCategory::Config,
        error.to_string(),
      );
      false
    }
  }
}

fn settings_mutation_error(error: &SettingsMutationError) -> &'static str {
  match error {
    SettingsMutationError::Config(_) => SETTINGS_SAVE_ERROR,
    SettingsMutationError::InvalidLoginPrefill => INVALID_LOGIN_PREFILL_ERROR,
    SettingsMutationError::InvalidProvider => INVALID_PROVIDER_ERROR,
    SettingsMutationError::InvalidSubtitleLanguage => INVALID_SUBTITLE_LANGUAGE_ERROR,
    SettingsMutationError::DuplicateSubtitleLanguage => DUPLICATE_SUBTITLE_LANGUAGE_ERROR,
    SettingsMutationError::EmptyShortcut => EMPTY_SHORTCUT_ERROR,
    SettingsMutationError::ShortcutCollision => SHORTCUT_COLLISION_ERROR,
  }
}

#[cfg(test)]
mod tests {
  use std::fs;
  use std::sync::Arc;

  use jellypilot_auth::login::ConnectionPhase;
  use jellypilot_auth::AuthStore;
  use jellypilot_core::config::SettingsStore;
  use jellypilot_core::diagnostics::Diagnostics;
  use jellypilot_core::request_gate::RequestGate;

  use super::*;
  use crate::app::state::ArtworkHandleRetention;

  fn test_fixture() -> (Surface, Kernel) {
    let settings = SettingsStore::default();
    let surface = Surface {
      view: SettingsState::from_settings(settings.snapshot()),
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

  #[test]
  fn settings_mutation_errors_use_fixed_inline_copy() {
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-settings-error-{}.json",
      std::process::id()
    ));
    let _ = fs::remove_file(&path);
    let (mut surface, mut kernel) = test_fixture();
    kernel.settings = SettingsStore::for_test(path);
    surface.view = SettingsState::from_settings(kernel.settings.snapshot());
    surface.view.shortcut_capture = Some(jellypilot_core::config::ShortcutKind::Next);

    drop(update(
      &mut surface,
      &mut kernel,
      UiThemeMode::Dark,
      SettingsMessage::ShortcutCaptured("Shift+<".to_owned()),
    ));

    assert_eq!(
      surface.view.error,
      Some("That shortcut is already assigned.")
    );
  }

  #[test]
  fn logs_exported_reports_path_via_badge_and_diagnostics() {
    let (mut surface, mut kernel) = test_fixture();

    drop(update(
      &mut surface,
      &mut kernel,
      UiThemeMode::Dark,
      SettingsMessage::LogsExported(Ok("/tmp/jellypilot-logs-19700102-000001.log".to_owned())),
    ));

    assert_eq!(surface.view.saved, Some("Logs exported"));
    assert!(surface.view.error.is_none());
    assert!(kernel
      .diagnostics
      .rows()
      .any(|row| row.message.contains("jellypilot-logs-19700102-000001.log")));
  }

  #[test]
  fn logs_exported_failure_shows_fixed_inline_error() {
    let (mut surface, mut kernel) = test_fixture();

    drop(update(
      &mut surface,
      &mut kernel,
      UiThemeMode::Dark,
      SettingsMessage::LogsExported(Err(LOG_EXPORT_ERROR.to_owned())),
    ));

    assert_eq!(surface.view.error, Some(LOG_EXPORT_ERROR));
    assert!(surface.view.saved.is_none());
    assert!(kernel
      .diagnostics
      .rows()
      .any(|row| row.level == DiagnosticLevel::Error));
  }

  #[test]
  fn auto_login_toggle_persists_the_opposite_value() {
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-auto-login-toggle-{}.json",
      std::process::id()
    ));
    let _ = fs::remove_file(&path);
    let (mut surface, mut kernel) = test_fixture();
    kernel.settings = SettingsStore::for_test(path.clone());

    drop(update(
      &mut surface,
      &mut kernel,
      UiThemeMode::Dark,
      SettingsMessage::AutoLoginToggled,
    ));

    assert!(!kernel.settings.snapshot().auto_login());
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn theme_toggle_persists_explicit_opposite_of_effective_mode() {
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-theme-toggle-{}.json",
      std::process::id()
    ));
    let _ = fs::remove_file(&path);
    let (mut surface, mut kernel) = test_fixture();
    kernel.settings = SettingsStore::for_test(path.clone());

    drop(update(
      &mut surface,
      &mut kernel,
      UiThemeMode::Dark,
      SettingsMessage::ThemeTogglePressed,
    ));
    assert_eq!(
      kernel.settings.snapshot().theme_mode(),
      ThemeMode::Light,
      "System with an effective dark theme should switch to explicit light"
    );

    drop(update(
      &mut surface,
      &mut kernel,
      UiThemeMode::Light,
      SettingsMessage::ThemeTogglePressed,
    ));
    assert_eq!(
      kernel.settings.snapshot().theme_mode(),
      ThemeMode::Dark,
      "explicit light should switch to explicit dark"
    );

    drop(update(
      &mut surface,
      &mut kernel,
      UiThemeMode::Dark,
      SettingsMessage::ThemeTogglePressed,
    ));
    assert_eq!(
      kernel.settings.snapshot().theme_mode(),
      ThemeMode::Light,
      "explicit dark should switch to explicit light"
    );
    fs::remove_file(path).unwrap();
  }
}
