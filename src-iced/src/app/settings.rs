//! Settings surface (ADR 0029): MPV/playback-target/intro/subtitle preference
//! edits, shortcut capture, diagnostic filters, and inline save feedback.

use iced::Task;
use jellypilot_core::config::{LocalPreference, PlaybackBackend, Settings, SettingsMutationError};
use jellypilot_core::diagnostics::{DiagnosticCategory, DiagnosticLevel};
use jellypilot_core::rendering::RenderBackend;

use super::kernel::Kernel;
use super::message::{Message, SettingsMessage};
use super::state::SettingsState;

use crate::i18n::{Localizer, UiText};
const PLAYBACK_CONFIG_APPLY_ERROR: &str = "Settings were saved, but MPV could not be reconfigured.";
const LOG_EXPORT_ERROR: &str = "Could not export logs.";

/// Settings surface slice: the editable form state behind the Settings page.
pub struct Surface {
  pub view: SettingsState,
}

/// Cross-surface follow-ups are hoisted to the top-level router (ADR 0029):
/// mutations that change playback-relevant settings reconfigure playback,
/// re-feed the intro mode, or refinalize the remote target there. This entry
/// point only ever mutates the settings slice and the kernel.
pub fn update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  message: SettingsMessage,
) -> Task<Message> {
  update_settings(surface, kernel, message)
}

fn update_settings(
  surface: &mut Surface,
  kernel: &mut Kernel,
  message: SettingsMessage,
) -> Task<Message> {
  match message {
    SettingsMessage::PlaybackBackendSelected(backend) => {
      if reject_renderer_combination(
        surface,
        kernel.settings.snapshot(),
        &LocalPreference::PlaybackBackend(backend),
      ) {
        return Task::none();
      }
      let result = kernel.settings.set_playback_backend(backend);
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::HdrOutputSelected(output) => {
      let result = kernel.settings.set_hdr_output(output);
      if result.is_ok() {
        crate::embedded::set_hdr_output(output);
      }
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::HdrContentChanged(hdr) => {
      crate::embedded::set_hdr_content(hdr);
      Task::none()
    }
    SettingsMessage::HdrStatusChanged(status) => {
      if surface.view.hdr_state != status {
        surface.view.hdr_state = status;
        let (level, message) = match status {
          crate::embedded::HdrState::Active => {
            (DiagnosticLevel::Info, "HDR10 PQ presentation active.")
          }
          crate::embedded::HdrState::Sdr => (DiagnosticLevel::Info, "SDR presentation active."),
          crate::embedded::HdrState::Unavailable => (
            DiagnosticLevel::Warning,
            "HDR10 presentation unavailable; using SDR.",
          ),
        };
        kernel
          .diagnostics
          .record(level, DiagnosticCategory::Player, message);
      }
      Task::none()
    }
    SettingsMessage::FontLicensesToggled => {
      surface.view.font_licenses_expanded = !surface.view.font_licenses_expanded;
      Task::none()
    }
    SettingsMessage::SectionSelected(section) => {
      surface.view.active_section = section;
      surface.view.shortcut_capture = None;
      surface.view.intro_menu_open = false;
      surface.view.subtitle_menu_open = false;
      surface.view.diagnostic_level_menu_open = false;
      surface.view.diagnostic_category_menu_open = false;
      surface.view.language_menu_open = false;
      surface.view.renderer_menu_open = false;
      Task::none()
    }
    SettingsMessage::LanguageMenuToggled => {
      surface.view.language_menu_open = !surface.view.language_menu_open;
      surface.view.renderer_menu_open = false;
      Task::none()
    }
    SettingsMessage::LanguageMenuDismissed => {
      surface.view.language_menu_open = false;
      Task::none()
    }
    SettingsMessage::RendererMenuToggled => {
      surface.view.renderer_menu_open = !surface.view.renderer_menu_open;
      surface.view.language_menu_open = false;
      Task::none()
    }
    SettingsMessage::RendererMenuDismissed => {
      surface.view.renderer_menu_open = false;
      Task::none()
    }
    SettingsMessage::UiLanguageSelected(preference) => {
      surface.view.language_menu_open = false;
      Task::done(Message::UiLanguageSelected(preference))
    }
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
    SettingsMessage::LocalPreferenceSelected(preference) => {
      if reject_renderer_combination(surface, kernel.settings.snapshot(), &preference) {
        return Task::none();
      }
      let renderer = matches!(preference, LocalPreference::RenderBackend(_));
      if renderer {
        surface.view.renderer_menu_open = false;
      }
      let refresh_cache = matches!(
        preference,
        jellypilot_core::config::LocalPreference::ImageCache(_)
      );
      let hdr = match &preference {
        jellypilot_core::config::LocalPreference::HdrOutput(output) => Some(*output),
        _ => None,
      };
      let result = kernel.settings.set_local_preference(preference);
      if finish_settings_mutation(surface, kernel, result) {
        if renderer {
          surface.view.saved = Some(UiText::new("settings-renderer-saved"));
        }
        if refresh_cache {
          kernel
            .artwork_adapter
            .set_disk_cache_enabled(kernel.settings.snapshot().image_cache_enabled());
        }
        if let Some(output) = hdr {
          crate::embedded::set_hdr_output(output);
        }
      }
      Task::none()
    }
    SettingsMessage::MpvOptionSelected { name, value } => {
      let result = kernel.settings.set_mpv_option(name, value);
      if finish_settings_mutation(surface, kernel, result) {
        surface.view.mpv_args_input = kernel.settings.snapshot().mpv_args().join(" ");
      }
      Task::none()
    }
    SettingsMessage::AutoNextEpisodeChanged(enabled) => {
      let result = kernel.settings.set_auto_next_episode(enabled);
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::ProgressSyncSecondsSelected(seconds) => {
      let result = kernel.settings.set_progress_sync_seconds(seconds);
      finish_settings_mutation(surface, kernel, result);
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
    SettingsMessage::TmdbApiKeyChanged(value) => {
      surface.view.tmdb_api_key_input = value;
      clear_settings_feedback(surface);
      Task::none()
    }
    SettingsMessage::SaveTmdbApiKey => {
      let value = surface.view.tmdb_api_key_input.clone();
      let result = kernel.settings.set_tmdb_api_key(value);
      if finish_settings_mutation(surface, kernel, result) {
        if let Some(client) = kernel.client.as_ref() {
          client.set_tmdb_api_key(kernel.settings.snapshot().tmdb_api_key().map(str::to_owned));
        }
      }
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
    SettingsMessage::RememberSeasonVolumeChanged(enabled) => {
      let result = kernel.settings.set_remember_season_volume(enabled);
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::PreferOriginalAudioChanged(enabled) => {
      let result = kernel.settings.set_prefer_original_audio(enabled);
      finish_settings_mutation(surface, kernel, result);
      Task::none()
    }
    SettingsMessage::ThemeModeSelected(mode) => {
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
    SettingsMessage::PlayerLogCaptureChanged(enabled) => {
      let player_logs = jellypilot_core::player_logs::global();
      if player_logs.enabled() != enabled {
        player_logs.set_enabled(enabled);
        kernel.diagnostics.record(
          DiagnosticLevel::Info,
          DiagnosticCategory::Player,
          if enabled {
            "Player log capture enabled."
          } else {
            "Player log capture disabled."
          },
        );
      }
      Task::none()
    }
    SettingsMessage::ExportLogs => {
      clear_settings_feedback(surface);
      let exported_at = jellypilot_core::logs::now_seconds();
      let mut document = jellypilot_core::logs::build_support_document(
        env!("CARGO_PKG_VERSION"),
        exported_at,
        kernel.diagnostics.rows(),
        &jellypilot_core::logs::global().snapshot(),
        &jellypilot_core::player_logs::global().snapshot(),
      );
      document.push_str("\n== Interface renderer ==\n");
      document.push_str(&format!(
        "Persisted: {}\n",
        kernel.settings.snapshot().render_backend().as_str()
      ));
      if let Some(selection) = crate::rendering::selection() {
        document.push_str(&format!(
          "Requested at startup: {}\nSelection source: {}\n",
          selection.requested.as_str(),
          selection.source,
        ));
      }
      document.push_str(&format!(
        "Observed actual: {}\n",
        crate::rendering::actual_backend()
          .as_deref()
          .unwrap_or("unknown")
      ));
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
          surface.view.saved = Some(UiText::new("settings-logs-exported"));
          kernel.diagnostics.record(
            DiagnosticLevel::Info,
            DiagnosticCategory::Config,
            format!("Logs exported to {path}."),
          );
        }
        Err(message) => {
          surface.view.error = Some(UiText::new("settings-log-export-error"));
          kernel
            .diagnostics
            .record(DiagnosticLevel::Error, DiagnosticCategory::Config, message);
        }
      }
      Task::none()
    }
    // Modal lifecycle belongs to the top-level router.
    SettingsMessage::Open | SettingsMessage::OpenAccounts | SettingsMessage::Close => Task::none(),
    SettingsMessage::PlaybackConfigApplied(result) => {
      if result.is_err() {
        surface.view.error = Some(UiText::new("settings-playback-config-apply-error"));
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

pub(crate) fn renderer_requires_vulkan(settings: &Settings) -> bool {
  crate::embedded::enabled() || settings.playback_backend() == PlaybackBackend::Embedded
}

pub(crate) fn renderer_label(locale: Localizer, backend: RenderBackend) -> String {
  locale.text(match backend {
    RenderBackend::Auto => "settings-renderer-auto",
    RenderBackend::Dx12 => "settings-renderer-dx12",
    RenderBackend::Vulkan => "settings-renderer-vulkan",
    RenderBackend::Metal => "settings-renderer-metal",
    RenderBackend::OpenGl => "settings-renderer-open-gl",
    RenderBackend::Software => "settings-renderer-software",
  })
}

pub(crate) fn actual_renderer_label(locale: Localizer) -> String {
  crate::rendering::actual_backend()
    .as_deref()
    .and_then(RenderBackend::parse)
    .map_or_else(
      || locale.text("settings-renderer-unknown"),
      |backend| renderer_label(locale, backend),
    )
}

pub(crate) fn renderer_preference_error(
  settings: &Settings,
  preference: &LocalPreference,
) -> Option<&'static str> {
  match preference {
    LocalPreference::RenderBackend(backend)
      if !backend.supported(std::env::consts::OS, renderer_requires_vulkan(settings)) =>
    {
      Some(if backend.supported(std::env::consts::OS, false) {
        "settings-renderer-embedded-required"
      } else {
        "settings-renderer-unsupported"
      })
    }
    LocalPreference::PlaybackBackend(PlaybackBackend::Embedded)
      if !settings
        .render_backend()
        .supported(std::env::consts::OS, true) =>
    {
      Some("settings-renderer-playback-incompatible")
    }
    _ => None,
  }
}

fn reject_renderer_combination(
  surface: &mut Surface,
  settings: &Settings,
  preference: &LocalPreference,
) -> bool {
  if let Some(error) = renderer_preference_error(settings, preference) {
    surface.view.saved = None;
    surface.view.error = Some(UiText::new(error));
    true
  } else {
    false
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
      surface.view.saved = Some(UiText::new("settings-saved"));
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

fn settings_mutation_error(error: &SettingsMutationError) -> UiText {
  UiText::new(match error {
    SettingsMutationError::Config(_) => "settings-save-error",
    SettingsMutationError::InvalidLoginPrefill => "settings-invalid-login-prefill-error",
    SettingsMutationError::InvalidProvider => "settings-invalid-provider-error",
    SettingsMutationError::InvalidSubtitleLanguage => "settings-invalid-subtitle-language-error",
    SettingsMutationError::DuplicateSubtitleLanguage => {
      "settings-duplicate-subtitle-language-error"
    }
    SettingsMutationError::EmptyShortcut => "settings-empty-shortcut-error",
    SettingsMutationError::ShortcutCollision => "settings-shortcut-collision-error",
    SettingsMutationError::IncompatibleRenderer => "settings-renderer-playback-incompatible",
  })
}

#[cfg(test)]
mod tests {
  use std::fs;
  use std::sync::Arc;

  use jellypilot_auth::login::ConnectionPhase;
  use jellypilot_core::config::SettingsStore;
  use jellypilot_core::diagnostics::Diagnostics;
  use jellypilot_core::request_gate::RequestGate;

  use super::*;

  fn test_fixture() -> (Surface, Kernel) {
    let settings = SettingsStore::default();
    let surface = Surface {
      view: SettingsState::from_settings(settings.snapshot()),
    };
    let auth_store = crate::app::kernel::test_auth_store();
    let (sdk, sdk_handoff) = crate::app::kernel::test_account_runtime(&auth_store);
    let kernel = Kernel {
      item_actions: Default::default(),
      settings,
      locale: crate::i18n::Localizer::new(jellypilot_core::locale::UiLanguage::English),
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
      profile_avatars: Default::default(),
      avatar_adapter: Arc::new(jellypilot_media_server::artwork::ArtworkAdapter::new()),
    };
    (surface, kernel)
  }

  #[test]
  fn logs_exported_reports_path_via_badge_and_diagnostics() {
    let (mut surface, mut kernel) = test_fixture();

    drop(update(
      &mut surface,
      &mut kernel,
      SettingsMessage::LogsExported(Ok("/tmp/jellypilot-logs-19700102-000001.log".to_owned())),
    ));

    assert!(surface.view.error.is_none());
    assert!(kernel
      .diagnostics
      .rows()
      .any(|row| row.message.contains("jellypilot-logs-19700102-000001.log")));
  }

  #[test]
  fn logs_exported_failure_records_diagnostic_error() {
    let (mut surface, mut kernel) = test_fixture();

    drop(update(
      &mut surface,
      &mut kernel,
      SettingsMessage::LogsExported(Err(LOG_EXPORT_ERROR.to_owned())),
    ));

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
      SettingsMessage::AutoLoginToggled,
    ));

    assert!(!kernel.settings.snapshot().auto_login());
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn embedded_renderer_rejection_preserves_every_saved_setting() {
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-renderer-rejection-{}.json",
      std::process::id()
    ));
    let (mut surface, mut kernel) = test_fixture();
    kernel.settings = SettingsStore::for_test(path.clone());
    kernel
      .settings
      .set_playback_backend(PlaybackBackend::Embedded)
      .unwrap();
    kernel
      .settings
      .set_local_preference(LocalPreference::RenderBackend(RenderBackend::Auto))
      .unwrap();
    let before = kernel.settings.snapshot().clone();
    let bytes = fs::read(&path).unwrap();

    drop(update(
      &mut surface,
      &mut kernel,
      SettingsMessage::LocalPreferenceSelected(LocalPreference::RenderBackend(
        RenderBackend::Software,
      )),
    ));

    assert_eq!(kernel.settings.snapshot(), &before);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert!(surface.view.saved.is_none());
    assert_eq!(
      kernel.locale.message(surface.view.error.as_ref().unwrap()),
      kernel.locale.text("settings-renderer-embedded-required"),
    );
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn desktop_and_tv_playback_changes_reject_an_incompatible_saved_renderer() {
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-playback-renderer-rejection-{}.json",
      std::process::id()
    ));
    let (mut surface, mut kernel) = test_fixture();
    kernel.settings = SettingsStore::for_test(path.clone());
    kernel
      .settings
      .set_playback_backend(PlaybackBackend::External)
      .unwrap();
    kernel
      .settings
      .set_local_preference(LocalPreference::RenderBackend(RenderBackend::Software))
      .unwrap();
    let before = kernel.settings.snapshot().clone();
    let bytes = fs::read(&path).unwrap();

    for message in [
      SettingsMessage::PlaybackBackendSelected(PlaybackBackend::Embedded),
      SettingsMessage::LocalPreferenceSelected(LocalPreference::PlaybackBackend(
        PlaybackBackend::Embedded,
      )),
    ] {
      drop(update(&mut surface, &mut kernel, message));
      assert_eq!(kernel.settings.snapshot(), &before);
      assert_eq!(fs::read(&path).unwrap(), bytes);
      assert_eq!(
        kernel.locale.message(surface.view.error.as_ref().unwrap()),
        kernel
          .locale
          .text("settings-renderer-playback-incompatible"),
      );
    }
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn saving_a_renderer_changes_next_launch_without_changing_the_observed_renderer() {
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-renderer-next-launch-{}.json",
      std::process::id()
    ));
    let (mut surface, mut kernel) = test_fixture();
    kernel.settings = SettingsStore::for_test(path.clone());
    kernel
      .settings
      .set_playback_backend(PlaybackBackend::External)
      .unwrap();
    kernel
      .settings
      .set_local_preference(LocalPreference::RenderBackend(RenderBackend::Software))
      .unwrap();
    let actual = crate::rendering::actual_backend();

    drop(update(
      &mut surface,
      &mut kernel,
      SettingsMessage::LocalPreferenceSelected(LocalPreference::RenderBackend(RenderBackend::Auto)),
    ));

    assert_eq!(
      kernel.settings.snapshot().render_backend(),
      RenderBackend::Auto
    );
    assert_eq!(crate::rendering::actual_backend(), actual);
    assert_eq!(
      kernel.locale.message(surface.view.saved.as_ref().unwrap()),
      kernel.locale.text("settings-renderer-saved"),
    );
    fs::remove_file(path).unwrap();
  }
}
