use iced::widget::{
  button, column, container, row, scrollable, space, text, text_input, Column, Row,
};
use iced::{Alignment, Element, Fill, Length};
use jellypilot_core::config::{AppMode, IntroMode, PlaybackBackend, ShortcutKind, ThemeMode};
use jellypilot_core::diagnostics::{format_diagnostic_time, DiagnosticCategory, DiagnosticLevel};
use jellypilot_core::locale::{LanguagePreference, UiLanguage};
use jellypilot_core::settings::SUBTITLE_LANGUAGE_OPTIONS;
use jellypilot_ui::fonts::{FONT_ATTRIBUTIONS, FONT_LICENSES, HEADING_FONT, MONO_FONT};
use jellypilot_ui::icons::{
  icon_for_control_state, icon_with_color, Icon, IconControlState, IconSize,
};
use jellypilot_ui::overlay::{popover, tooltip, PopoverOptions, TooltipOptions};
use jellypilot_ui::tokens::{palette, ThemePalette, TOKENS};
use jellypilot_ui::variants::{BadgeVariant, ButtonVariant, FieldVariant};
use jellypilot_ui::widgets::badge::status_tag;
use jellypilot_ui::widgets::control_button::{control_button, control_button_content};
use jellypilot_ui::widgets::settings as settings_style;
use jellypilot_ui::widgets::switch::switch;

use super::account;

use crate::app::message::{Message, SettingsMessage};
use crate::app::shell::SETTINGS_INITIAL_FOCUS_ID;
use crate::app::state::{diagnostic_matches, SettingsSection, State};
use crate::i18n::Localizer;

pub fn view(state: &State) -> Element<'_, Message> {
  let content = scrollable(content_column(state))
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable);
  row![
    container(navigation(
      state.kernel.locale,
      state.settings.view.active_section
    ))
    .width(Length::Fixed(208.0))
    .height(Fill)
    .style(settings_style::navigation),
    content,
  ]
  .width(Fill)
  .height(Fill)
  .into()
}

fn content_column(state: &State) -> Column<'_, Message> {
  let has_feedback = state.settings.view.error.is_some() || state.settings.view.saved.is_some();
  // Keep the section at the same tree index as feedback appears or disappears.
  // Replacing its tree with the banner's would drop input focus on the first edit.
  column![
    container(feedback(state)).padding(iced::Padding {
      bottom: if has_feedback { TOKENS.spacing.s5 } else { 0.0 },
      ..iced::Padding::ZERO
    }),
    selected_section(state, state.settings.view.active_section),
  ]
  .padding([TOKENS.spacing.s5, TOKENS.spacing.s6])
  .width(Fill)
}

fn selected_section<'a>(state: &'a State, section: SettingsSection) -> Element<'a, Message> {
  match section {
    SettingsSection::Account => account::management(state),
    SettingsSection::Mpv => mpv_section(state),
    SettingsSection::Playback => playback_section(state),
    SettingsSection::Subtitles => subtitles_section(state),
    SettingsSection::Shortcuts => shortcuts_section(state),
    SettingsSection::Appearance => interface_section(state),
    SettingsSection::Storage => storage_section(state),
    SettingsSection::Diagnostics => column![diagnostics_section(state), about_block(state)]
      .spacing(TOKENS.spacing.s5)
      .width(Fill)
      .into(),
  }
}

fn navigation(locale: Localizer, active: SettingsSection) -> Element<'static, Message> {
  container(
    column![
      scrollable(navigation_items(locale, active))
        .height(Fill)
        .style(jellypilot_ui::theme::scrollable),
      navigation_footer(locale),
    ]
    .spacing(TOKENS.spacing.s1_5)
    .padding([TOKENS.spacing.s3, TOKENS.spacing.s2_5])
    .width(Fill)
    .height(Fill),
  )
  .width(Fill)
  .height(Fill)
  .into()
}

fn navigation_items(locale: Localizer, active: SettingsSection) -> Column<'static, Message> {
  let mut items = Column::new().spacing(TOKENS.spacing.s1_5).width(Fill);
  for section in SettingsSection::ALL {
    let selected = section == active;
    let button = control_button(
      Some(settings_icon(section)),
      Some(section.label(locale)),
      if selected {
        ButtonVariant::Secondary
      } else {
        ButtonVariant::Text
      },
    )
    .style(settings_style::navigation_button)
    .icon_size(IconSize::Sm)
    .label_size(TOKENS.font_sizes.s12)
    .spacing(TOKENS.spacing.s2_5)
    .padding([TOKENS.spacing.s2, TOKENS.spacing.s2_5])
    .width(Fill)
    .label_fill(true)
    .on_press(Message::Settings(SettingsMessage::SectionSelected(section)));
    items = items.push(if selected {
      button.id(SETTINGS_INITIAL_FOCUS_ID)
    } else {
      button
    });
  }
  items
}

fn navigation_footer(locale: Localizer) -> Element<'static, Message> {
  let close = control_button_content(
    move |_| {
      row![
        text(locale.text("common-close"))
          .size(TOKENS.font_sizes.s12)
          .font(MONO_FONT)
          .style(|theme| text::Style {
            color: Some(palette(theme).text.metadata),
          }),
        container(
          text("esc")
            .size(TOKENS.font_sizes.s10)
            .font(MONO_FONT)
            .style(|theme| text::Style {
              color: Some(palette(theme).text.metadata),
            }),
        )
        .padding([TOKENS.spacing.s0_5, TOKENS.spacing.s1_5])
        .style(settings_style::keycap),
      ]
      .spacing(TOKENS.spacing.s1_5)
      .align_y(Alignment::Center)
      .into()
    },
    ButtonVariant::Text,
  )
  .padding([TOKENS.spacing.s2, TOKENS.spacing.s3])
  .min_height(40.0)
  .id("settings-close")
  .on_press(Message::Settings(SettingsMessage::Close));
  column![
    text(locale.text("shell-settings-save-hint"))
      .size(TOKENS.font_sizes.s12)
      .width(Fill)
      .style(|theme| text::Style {
        color: Some(palette(theme).text.metadata),
      }),
    container(close).width(Fill).align_x(Alignment::End),
  ]
  .spacing(TOKENS.spacing.s2)
  .padding(TOKENS.spacing.s2)
  .width(Fill)
  .into()
}

const fn settings_icon(section: SettingsSection) -> Icon {
  match section {
    SettingsSection::Account => Icon::User,
    SettingsSection::Mpv => Icon::Cpu,
    SettingsSection::Playback => Icon::Sliders,
    SettingsSection::Subtitles => Icon::Subtitles,
    SettingsSection::Shortcuts => Icon::Keyboard,
    SettingsSection::Appearance => Icon::Settings,
    SettingsSection::Storage => Icon::Database,
    SettingsSection::Diagnostics => Icon::Activity,
  }
}

fn feedback(state: &State) -> Element<'_, Message> {
  if let Some(error) = &state.settings.view.error {
    return text(state.kernel.locale.message(error))
      .size(TOKENS.font_sizes.s13)
      .color(state.palette().colors.error)
      .into();
  }
  if let Some(saved) = &state.settings.view.saved {
    return status_tag(state.kernel.locale.message(saved), BadgeVariant::Success);
  }
  space::vertical().height(0).into()
}

fn mpv_section(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let path = text_input(
    state.t("settings-mpv-path-placeholder"),
    &state.settings.view.mpv_path_input,
  )
  .id("settings-mpv-path")
  .on_input(|value| Message::Settings(SettingsMessage::MpvPathChanged(value)))
  .on_submit(Message::Settings(SettingsMessage::SaveMpvPath))
  .font(MONO_FONT)
  .size(TOKENS.font_sizes.s12)
  .padding([TOKENS.spacing.s2, TOKENS.spacing.s3])
  .width(Fill)
  .style(|theme, status| jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled));
  let args = text_input(
    state.t("settings-mpv-args-placeholder"),
    &state.settings.view.mpv_args_input,
  )
  .on_input(|value| Message::Settings(SettingsMessage::MpvArgsChanged(value)))
  .on_submit(Message::Settings(SettingsMessage::SaveMpvArgs))
  .font(MONO_FONT)
  .size(TOKENS.font_sizes.s12)
  .padding([TOKENS.spacing.s2, TOKENS.spacing.s3])
  .width(Fill)
  .style(|theme, status| jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled));
  let backend = state.kernel.settings.snapshot().playback_backend();
  column![
    column![
      column![
        text(state.t("settings-player-backend"))
          .font(HEADING_FONT)
          .size(TOKENS.font_sizes.s14)
          .color(palette.text.secondary),
        text(state.t("settings-player-backend-help"))
          .size(TOKENS.font_sizes.s12)
          .color(palette.text.body),
      ]
      .spacing(TOKENS.spacing.s0_5),
      segmented_row(row![
        segmented_option(
          state.t("settings-player-external"),
          backend == PlaybackBackend::External,
          SettingsMessage::PlaybackBackendSelected(PlaybackBackend::External),
        ),
        segmented_option(
          state.t("settings-player-embedded"),
          backend == PlaybackBackend::Embedded,
          SettingsMessage::PlaybackBackendSelected(PlaybackBackend::Embedded),
        ),
      ]),
    ]
    .spacing(TOKENS.spacing.s2),
    labeled_field(
      palette,
      state.kernel.locale,
      state.t("settings-mpv-path"),
      state.t("settings-mpv-path-help"),
      path.into(),
      SettingsMessage::SaveMpvPath,
    ),
    labeled_field(
      palette,
      state.kernel.locale,
      state.t("settings-mpv-args"),
      state.t("settings-mpv-args-help"),
      args.into(),
      SettingsMessage::SaveMpvArgs,
    ),
  ]
  .spacing(TOKENS.spacing.s5)
  .width(Fill)
  .into()
}

fn playback_section(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let target = text_input(
    "JellyPilot",
    &state.settings.view.playback_target_name_input,
  )
  .on_input(|value| Message::Settings(SettingsMessage::PlaybackTargetNameChanged(value)))
  .on_submit(Message::Settings(SettingsMessage::SavePlaybackTargetName))
  .font(MONO_FONT)
  .size(TOKENS.font_sizes.s12)
  .padding([TOKENS.spacing.s2, TOKENS.spacing.s3])
  .width(Fill)
  .style(|theme, status| jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled));
  let tmdb_key = text_input("", &state.settings.view.tmdb_api_key_input)
    .on_input(|value| Message::Settings(SettingsMessage::TmdbApiKeyChanged(value)))
    .on_submit(Message::Settings(SettingsMessage::SaveTmdbApiKey))
    .font(MONO_FONT)
    .size(TOKENS.font_sizes.s12)
    .padding([TOKENS.spacing.s2, TOKENS.spacing.s3])
    .width(Fill)
    .style(|theme, status| {
      jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
    });
  let mode = state.kernel.settings.snapshot().intro_mode();
  let menu = column![
    menu_option(
      state.t("settings-intro-automatic"),
      mode == IntroMode::Automatic,
      SettingsMessage::IntroModeSelected(IntroMode::Automatic),
    ),
    menu_option(
      state.t("settings-intro-manual"),
      mode == IntroMode::Manual,
      SettingsMessage::IntroModeSelected(IntroMode::Manual),
    ),
    menu_option(
      state.t("settings-off"),
      mode == IntroMode::Off,
      SettingsMessage::IntroModeSelected(IntroMode::Off),
    ),
  ]
  .spacing(TOKENS.spacing.s1)
  .width(Fill);
  let intro = popover(
    select_trigger(
      intro_mode_label(state.kernel.locale, mode),
      [TOKENS.spacing.s2, TOKENS.spacing.s3],
      TOKENS.spacing.s2,
      Length::Fit,
    )
    .on_press(Message::Settings(SettingsMessage::IntroMenuToggled)),
    menu,
    state.settings.view.intro_menu_open,
    PopoverOptions {
      width: Some(220.0),
      ..PopoverOptions::default()
    },
    Message::Settings(SettingsMessage::IntroMenuDismissed),
  );
  let snapshot = state.kernel.settings.snapshot();
  column![
    labeled_field(
      palette,
      state.kernel.locale,
      state.t("settings-target-name"),
      state.t("settings-target-name-help"),
      target.into(),
      SettingsMessage::SavePlaybackTargetName,
    ),
    choice_row(
      palette,
      state.t("settings-intro"),
      Some(state.t("settings-intro-help")),
      intro,
    ),
    labeled_field(
      palette,
      state.kernel.locale,
      state.t("settings-tmdb-api-key"),
      state.t("settings-tmdb-api-key-help"),
      tmdb_key.into(),
      SettingsMessage::SaveTmdbApiKey,
    ),
    toggle_row(
      palette,
      state.t("settings-season-volume"),
      Some(state.t("settings-season-volume-help")),
      snapshot.remember_season_volume(),
      "settings-season-volume",
      SettingsMessage::RememberSeasonVolumeChanged(!snapshot.remember_season_volume()),
    ),
    toggle_row(
      palette,
      state.t("settings-original-audio"),
      Some(state.t("settings-original-audio-help")),
      snapshot.prefer_original_audio(),
      "settings-original-audio",
      SettingsMessage::PreferOriginalAudioChanged(!snapshot.prefer_original_audio()),
    ),
  ]
  .spacing(TOKENS.spacing.s5)
  .width(Fill)
  .into()
}

fn subtitles_section(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let mut menu = Column::new().spacing(TOKENS.spacing.s1).width(Fill);
  for language in SUBTITLE_LANGUAGE_OPTIONS {
    menu = menu.push(
      control_button(
        None,
        Some(subtitle_language_label(state.kernel.locale, language)),
        ButtonVariant::Text,
      )
      .padding([TOKENS.spacing.s1_5, TOKENS.spacing.s2_5])
      .width(Fill)
      .label_fill(true)
      .label_size(TOKENS.font_sizes.s12)
      .on_press(Message::Settings(SettingsMessage::SubtitleLanguageAdded(
        language.to_owned(),
      ))),
    );
  }
  let add = popover(
    control_button(
      Some(Icon::Subtitles),
      Some(state.t("settings-add-language")),
      ButtonVariant::Tonal,
    )
    .icon_size(IconSize::Xs)
    .label_size(TOKENS.font_sizes.s12)
    .spacing(TOKENS.spacing.s1_5)
    .padding([TOKENS.spacing.s2, TOKENS.spacing.s3_5])
    .radius(TOKENS.radii.lg)
    .on_press(Message::Settings(SettingsMessage::SubtitleMenuToggled)),
    menu,
    state.settings.view.subtitle_menu_open,
    PopoverOptions {
      width: Some(220.0),
      ..PopoverOptions::default()
    },
    Message::Settings(SettingsMessage::SubtitleMenuDismissed),
  );
  let languages = state.kernel.settings.snapshot().subtitle_languages();
  let mut rows = Column::new().spacing(TOKENS.spacing.s2).width(Fill);
  if languages.is_empty() {
    rows = rows.push(
      text(state.t("settings-subtitle-empty"))
        .size(TOKENS.font_sizes.s12)
        .color(palette.text.metadata),
    );
  }
  for (index, language) in languages.iter().enumerate() {
    rows = rows.push(
      row![
        text(state.format(
          "settings-subtitle-ranked",
          &[
            ("index", (index + 1).into()),
            (
              "language",
              subtitle_language_label(state.kernel.locale, language).into()
            ),
          ]
        ))
        .size(TOKENS.font_sizes.s12)
        .color(palette.text.secondary)
        .width(Fill),
        compact_icon_button(
          Icon::ArrowUp,
          state.t("settings-move-up"),
          index > 0,
          SettingsMessage::SubtitleLanguageMoved { index, offset: -1 },
        ),
        compact_icon_button(
          Icon::ArrowDown,
          state.t("settings-move-down"),
          index + 1 < languages.len(),
          SettingsMessage::SubtitleLanguageMoved { index, offset: 1 },
        ),
        compact_icon_button(
          Icon::Trash,
          state.t("settings-remove"),
          true,
          SettingsMessage::SubtitleLanguageRemoved(index),
        ),
      ]
      .spacing(TOKENS.spacing.s2)
      .align_y(Alignment::Center),
    );
  }
  column![
    text(state.t("settings-subtitles-help"))
      .size(TOKENS.font_sizes.s12)
      .color(palette.text.metadata),
    rows,
    row![add],
  ]
  .spacing(TOKENS.spacing.s5)
  .width(Fill)
  .into()
}

fn shortcuts_section(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  column![
    column![
      shortcut_row(state, state.t("settings-next-episode"), ShortcutKind::Next),
      shortcut_row(
        state,
        state.t("settings-previous-episode"),
        ShortcutKind::Previous
      ),
      shortcut_row(
        state,
        state.t("settings-skip-intro"),
        ShortcutKind::IntroSkip
      ),
    ]
    .spacing(TOKENS.spacing.s2),
    text(state.t("settings-shortcuts-help"))
      .size(TOKENS.font_sizes.s12)
      .color(palette.text.metadata),
  ]
  .spacing(TOKENS.spacing.s5)
  .width(Fill)
  .into()
}

fn shortcut_row<'a>(state: &'a State, label: String, kind: ShortcutKind) -> Element<'a, Message> {
  let binding = match kind {
    ShortcutKind::Next => state.kernel.settings.snapshot().key_next_episode(),
    ShortcutKind::Previous => state.kernel.settings.snapshot().key_previous_episode(),
    ShortcutKind::IntroSkip => state.kernel.settings.snapshot().key_intro_skip(),
  };
  let capturing = state.settings.view.shortcut_capture == Some(kind);
  let caption = if capturing {
    state.t("settings-press-key")
  } else {
    binding.to_owned()
  };
  let variant = if capturing {
    ButtonVariant::Secondary
  } else {
    ButtonVariant::Tonal
  };
  let keycap = control_button_content(
    move |state| {
      let status = match state {
        IconControlState::Rest => button::Status::Active,
        IconControlState::Hovered => button::Status::Hovered,
        IconControlState::Disabled => button::Status::Disabled,
      };
      text(caption.clone())
        .size(TOKENS.font_sizes.s12)
        .font(MONO_FONT)
        .style(move |theme| text::Style {
          color: Some(settings_style::navigation_button(theme, variant, status).text_color),
        })
        .into()
    },
    variant,
  )
  .style(settings_style::navigation_button)
  .padding([TOKENS.spacing.s2, TOKENS.spacing.s3_5])
  .radius(TOKENS.radii.lg)
  .on_press(Message::Settings(SettingsMessage::BeginShortcutCapture(
    kind,
  )));
  row![
    row![
      icon_with_color(
        Icon::Keyboard,
        IconSize::Xs,
        state.palette().colors.onSurfaceVariant
      ),
      text(label)
        .size(TOKENS.font_sizes.s12)
        .color(state.palette().text.secondary),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center)
    .width(Fill),
    keycap,
  ]
  .align_y(Alignment::Center)
  .spacing(TOKENS.spacing.s2_5)
  .wrap()
  .into()
}

fn interface_section(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let theme_mode = state.kernel.settings.snapshot().theme_mode();
  let app_mode = state.kernel.settings.snapshot().app_mode();
  let reduced_motion = state.kernel.settings.snapshot().reduced_motion();
  column![
    language_row(state),
    choice_row(
      palette,
      state.t("settings-appearance"),
      Some(state.t("settings-appearance-help")),
      segmented_row(row![
        segmented_option(
          state.t("settings-system"),
          theme_mode == ThemeMode::System,
          SettingsMessage::ThemeModeSelected(ThemeMode::System),
        ),
        segmented_option(
          state.t("settings-dark"),
          theme_mode == ThemeMode::Dark,
          SettingsMessage::ThemeModeSelected(ThemeMode::Dark),
        ),
        segmented_option(
          state.t("settings-light"),
          theme_mode == ThemeMode::Light,
          SettingsMessage::ThemeModeSelected(ThemeMode::Light),
        ),
      ]),
    ),
    choice_row(
      palette,
      state.t("settings-app-mode"),
      Some(state.t("settings-app-mode-help")),
      segmented_row(row![
        segmented_option(
          state.t("settings-app-mode-full"),
          app_mode == AppMode::Full,
          SettingsMessage::AppModeSelected(AppMode::Full),
        ),
        segmented_option(
          state.t("settings-app-mode-control-only"),
          app_mode == AppMode::ControlOnly,
          SettingsMessage::AppModeSelected(AppMode::ControlOnly),
        ),
      ]),
    ),
    toggle_row(
      palette,
      state.t("settings-reduce-motion"),
      Some(state.t("settings-reduce-motion-help")),
      reduced_motion,
      "settings-reduce-motion",
      SettingsMessage::ReducedMotionToggled,
    ),
  ]
  .spacing(TOKENS.spacing.s5)
  .width(Fill)
  .into()
}

fn language_row(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let selected = state.kernel.settings.snapshot().ui_language();
  let options = [
    ("language-system", LanguagePreference::System),
    (
      "language-english",
      LanguagePreference::Fixed(UiLanguage::English),
    ),
    (
      "language-chinese",
      LanguagePreference::Fixed(UiLanguage::SimplifiedChinese),
    ),
  ];
  let selected_label = options
    .iter()
    .find(|(_, value)| *value == selected)
    .map_or_else(|| state.t("language-system"), |(id, _)| state.t(id));
  let mut menu = Column::new().spacing(TOKENS.spacing.s1).width(Fill);
  for (id, value) in options {
    menu = menu.push(menu_option(
      state.t(id),
      value == selected,
      SettingsMessage::UiLanguageSelected(value),
    ));
  }
  let picker = popover(
    select_trigger(
      selected_label,
      [TOKENS.spacing.s2, TOKENS.spacing.s3],
      TOKENS.spacing.s2,
      Length::Fit,
    )
    .on_press(Message::Settings(SettingsMessage::LanguageMenuToggled)),
    menu,
    state.settings.view.language_menu_open,
    PopoverOptions {
      width: Some(220.0),
      ..PopoverOptions::default()
    },
    Message::Settings(SettingsMessage::LanguageMenuDismissed),
  );
  choice_row(
    palette,
    state.t("language-label"),
    Some(state.t("language-description")),
    picker,
  )
}

fn storage_section(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let cache_enabled = state.kernel.settings.snapshot().image_cache_enabled();
  let start_minimized = state.kernel.settings.snapshot().start_minimized();
  column![
    toggle_row(
      palette,
      state.t("settings-image-cache"),
      Some(state.t("settings-image-cache-help")),
      cache_enabled,
      "settings-image-cache",
      SettingsMessage::ImageCacheToggled,
    ),
    toggle_row(
      palette,
      state.t("settings-start-minimized"),
      Some(state.t("settings-start-minimized-help")),
      start_minimized,
      "settings-start-minimized",
      SettingsMessage::StartMinimizedToggled,
    ),
  ]
  .spacing(TOKENS.spacing.s5)
  .width(Fill)
  .into()
}

fn diagnostics_section(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let player_log_capture = jellypilot_core::player_logs::global().enabled();
  let level_menu = column![
    menu_option(
      state.t("settings-all"),
      state.settings.view.diagnostic_level.is_none(),
      SettingsMessage::DiagnosticLevelSelected(None),
    ),
    menu_option(
      state.t("settings-level-info"),
      state.settings.view.diagnostic_level == Some(DiagnosticLevel::Info),
      SettingsMessage::DiagnosticLevelSelected(Some(DiagnosticLevel::Info)),
    ),
    menu_option(
      state.t("settings-level-warning"),
      state.settings.view.diagnostic_level == Some(DiagnosticLevel::Warning),
      SettingsMessage::DiagnosticLevelSelected(Some(DiagnosticLevel::Warning)),
    ),
    menu_option(
      state.t("settings-level-error"),
      state.settings.view.diagnostic_level == Some(DiagnosticLevel::Error),
      SettingsMessage::DiagnosticLevelSelected(Some(DiagnosticLevel::Error)),
    ),
  ]
  .spacing(TOKENS.spacing.s1)
  .width(Fill);
  let level_filter = popover(
    filter_trigger(
      Icon::Filter,
      state.format(
        "settings-level-filter",
        &[(
          "level",
          diagnostic_level_label(state.kernel.locale, state.settings.view.diagnostic_level).into(),
        )],
      ),
    )
    .on_press(Message::Settings(
      SettingsMessage::DiagnosticLevelMenuToggled,
    )),
    level_menu,
    state.settings.view.diagnostic_level_menu_open,
    PopoverOptions {
      width: Some(180.0),
      ..PopoverOptions::default()
    },
    Message::Settings(SettingsMessage::DiagnosticLevelMenuDismissed),
  );
  let category_menu = column![
    menu_option(
      state.t("settings-all"),
      state.settings.view.diagnostic_category.is_none(),
      SettingsMessage::DiagnosticCategorySelected(None),
    ),
    menu_option(
      state.t("settings-category-connection"),
      state.settings.view.diagnostic_category == Some(DiagnosticCategory::Connection),
      SettingsMessage::DiagnosticCategorySelected(Some(DiagnosticCategory::Connection)),
    ),
    menu_option(
      state.t("settings-category-auth"),
      state.settings.view.diagnostic_category == Some(DiagnosticCategory::Auth),
      SettingsMessage::DiagnosticCategorySelected(Some(DiagnosticCategory::Auth)),
    ),
    menu_option(
      state.t("settings-playback"),
      state.settings.view.diagnostic_category == Some(DiagnosticCategory::Playback),
      SettingsMessage::DiagnosticCategorySelected(Some(DiagnosticCategory::Playback)),
    ),
    menu_option(
      state.t("settings-category-player"),
      state.settings.view.diagnostic_category == Some(DiagnosticCategory::Player),
      SettingsMessage::DiagnosticCategorySelected(Some(DiagnosticCategory::Player)),
    ),
    menu_option(
      state.t("settings-category-remote-control"),
      state.settings.view.diagnostic_category == Some(DiagnosticCategory::RemoteControl),
      SettingsMessage::DiagnosticCategorySelected(Some(DiagnosticCategory::RemoteControl)),
    ),
    menu_option(
      state.t("settings-category-artwork"),
      state.settings.view.diagnostic_category == Some(DiagnosticCategory::Artwork),
      SettingsMessage::DiagnosticCategorySelected(Some(DiagnosticCategory::Artwork)),
    ),
    menu_option(
      state.t("settings-category-config"),
      state.settings.view.diagnostic_category == Some(DiagnosticCategory::Config),
      SettingsMessage::DiagnosticCategorySelected(Some(DiagnosticCategory::Config)),
    ),
  ]
  .spacing(TOKENS.spacing.s1)
  .width(Fill);
  let category_filter = popover(
    filter_trigger(
      Icon::Sliders,
      state.format(
        "settings-category-filter",
        &[(
          "category",
          diagnostic_category_label(state.kernel.locale, state.settings.view.diagnostic_category)
            .into(),
        )],
      ),
    )
    .on_press(Message::Settings(
      SettingsMessage::DiagnosticCategoryMenuToggled,
    )),
    category_menu,
    state.settings.view.diagnostic_category_menu_open,
    PopoverOptions {
      width: Some(210.0),
      ..PopoverOptions::default()
    },
    Message::Settings(SettingsMessage::DiagnosticCategoryMenuDismissed),
  );
  let export_button = control_button(
    Some(Icon::Download),
    Some(state.t("settings-export-logs")),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Xs)
  .label_size(TOKENS.font_sizes.s12)
  .spacing(TOKENS.spacing.s1_5)
  .padding([TOKENS.spacing.s2, TOKENS.spacing.s3_5])
  .radius(TOKENS.radii.lg)
  .on_press(Message::Settings(SettingsMessage::ExportLogs));

  let mut cards = Column::new().spacing(TOKENS.spacing.s5).width(Fill);
  let mut count = 0_usize;
  for diagnostic in state.kernel.diagnostics.rows().filter(|diagnostic| {
    diagnostic_matches(
      state.settings.view.diagnostic_level,
      state.settings.view.diagnostic_category,
      diagnostic.level,
      diagnostic.category,
    )
  }) {
    count = count.saturating_add(1);
    let (level_icon, level_color) = match diagnostic.level {
      DiagnosticLevel::Info => (Icon::Info, palette.colors.primary),
      DiagnosticLevel::Warning => (Icon::Warning, palette.colors.warning),
      DiagnosticLevel::Error => (Icon::Error, palette.colors.error),
    };
    cards = cards.push(
      container(
        column![
          row![
            icon_with_color(level_icon, IconSize::Xs, level_color),
            status_tag(
              diagnostic_level_label(state.kernel.locale, Some(diagnostic.level)),
              diagnostic_badge(diagnostic.level)
            ),
            text(diagnostic_category_label(
              state.kernel.locale,
              Some(diagnostic.category)
            ))
            .size(TOKENS.font_sizes.s12)
            .color(palette.text.metadata),
            space::horizontal(),
            text(format_diagnostic_time(diagnostic.timestamp_seconds))
              .size(TOKENS.font_sizes.s12)
              .color(palette.text.metadata),
          ]
          .spacing(TOKENS.spacing.s2)
          .align_y(Alignment::Center),
          text(diagnostic.message)
            .size(TOKENS.font_sizes.s12)
            .color(palette.text.secondary),
        ]
        .spacing(TOKENS.spacing.s2),
      )
      .padding(TOKENS.spacing.s3)
      .width(Fill)
      .style(settings_style::event_card),
    );
  }
  if count == 0 {
    cards = cards.push(
      text(state.t("settings-diagnostics-empty"))
        .size(TOKENS.font_sizes.s12)
        .color(palette.text.metadata),
    );
  }
  column![
    toggle_row(
      palette,
      state.t("settings-player-log-capture"),
      Some(state.t("settings-player-log-capture-help")),
      player_log_capture,
      "settings-player-log-capture",
      SettingsMessage::PlayerLogCaptureChanged(!player_log_capture),
    ),
    row![
      level_filter,
      category_filter,
      space::horizontal(),
      export_button,
    ]
    .spacing(TOKENS.spacing.s2_5)
    .align_y(Alignment::Center)
    .wrap(),
    text(state.format("settings-event-count", &[("count", count.into())]))
      .size(TOKENS.font_sizes.s12)
      .color(palette.text.metadata),
    cards,
  ]
  .spacing(TOKENS.spacing.s5)
  .width(Fill)
  .into()
}

fn about_block(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let expanded = state.settings.view.font_licenses_expanded;
  let mut content = column![
    text("JellyPilot")
      .font(HEADING_FONT)
      .size(TOKENS.font_sizes.s16)
      .color(palette.text.heading),
    text(state.format(
      "settings-version",
      &[("version", env!("CARGO_PKG_VERSION").into())]
    ))
    .size(TOKENS.font_sizes.s12)
    .color(palette.text.muted),
    text(FONT_ATTRIBUTIONS)
      .size(TOKENS.font_sizes.s12)
      .color(palette.text.body),
    row![control_button(
      None,
      Some(state.t(if expanded {
        "settings-hide-font-licenses"
      } else {
        "settings-show-font-licenses"
      })),
      ButtonVariant::Tonal,
    )
    .label_size(TOKENS.font_sizes.s12)
    .padding([TOKENS.spacing.s2, TOKENS.spacing.s3_5])
    .radius(TOKENS.radii.lg)
    .on_press(Message::Settings(SettingsMessage::FontLicensesToggled)),]
    .padding(iced::Padding {
      top: TOKENS.spacing.s1,
      ..iced::Padding::ZERO
    }),
  ]
  .spacing(TOKENS.spacing.s1_5);
  if expanded {
    content = content.push(
      text(FONT_LICENSES)
        .size(TOKENS.font_sizes.s12)
        .color(palette.text.body)
        .width(Fill),
    );
  }
  column![
    container(space::horizontal())
      .width(Fill)
      .height(Length::Fixed(1.0))
      .style(settings_style::divider),
    content,
  ]
  .spacing(TOKENS.spacing.s4)
  .width(Fill)
  .into()
}

fn labeled_field<'a>(
  palette: &ThemePalette,
  locale: Localizer,
  label: String,
  help: String,
  field: Element<'a, Message>,
  save: SettingsMessage,
) -> Element<'a, Message> {
  column![
    column![
      text(label)
        .font(HEADING_FONT)
        .size(TOKENS.font_sizes.s14)
        .color(palette.text.secondary),
      text(help)
        .size(TOKENS.font_sizes.s12)
        .color(palette.text.body),
    ]
    .spacing(TOKENS.spacing.s0_5),
    row![
      field,
      control_button(
        Some(Icon::Check),
        Some(locale.text("settings-save")),
        ButtonVariant::Primary,
      )
      .icon_size(IconSize::Xs)
      .label_size(TOKENS.font_sizes.s12)
      .spacing(TOKENS.spacing.s1_5)
      .padding([TOKENS.spacing.s2, TOKENS.spacing.s3])
      .min_height(32.0)
      .radius(TOKENS.radii.lg)
      .on_press(Message::Settings(save)),
    ]
    .spacing(TOKENS.spacing.s2_5)
    .align_y(Alignment::Center)
    .wrap(),
  ]
  .spacing(TOKENS.spacing.s2)
  .into()
}

fn choice_row<'a>(
  palette: &ThemePalette,
  label: String,
  help: Option<String>,
  choice: Element<'a, Message>,
) -> Element<'a, Message> {
  let mut copy = Column::new().spacing(TOKENS.spacing.s0_5).width(Fill);
  copy = copy.push(
    text(label)
      .font(HEADING_FONT)
      .size(TOKENS.font_sizes.s14)
      .color(palette.text.secondary),
  );
  if let Some(help) = help {
    copy = copy.push(
      text(help)
        .size(TOKENS.font_sizes.s12)
        .color(palette.text.body),
    );
  }
  row![copy, choice]
    .spacing(TOKENS.spacing.s2_5)
    .align_y(Alignment::Center)
    .wrap()
    .into()
}

fn toggle_row<'a>(
  palette: &ThemePalette,
  label: String,
  help: Option<String>,
  enabled: bool,
  id: &'static str,
  message: SettingsMessage,
) -> Element<'a, Message> {
  choice_row(
    palette,
    label,
    help,
    switch(enabled)
      .id(id)
      .on_press(Message::Settings(message))
      .into(),
  )
}

fn segmented_row<'a>(options: Row<'a, Message>) -> Element<'a, Message> {
  container(options.spacing(0).align_y(Alignment::Center))
    .padding(TOKENS.spacing.s0_5)
    .style(settings_style::segmented_group)
    .into()
}

fn segmented_option(
  label: String,
  selected: bool,
  message: SettingsMessage,
) -> Element<'static, Message> {
  control_button(
    None,
    Some(label),
    if selected {
      ButtonVariant::Secondary
    } else {
      ButtonVariant::Text
    },
  )
  .style(settings_style::segmented_button)
  .label_size(TOKENS.font_sizes.s12)
  .padding([TOKENS.spacing.s1_5, TOKENS.spacing.s3])
  .on_press(Message::Settings(message))
  .into()
}

fn select_trigger<'a>(
  label: String,
  padding: [f32; 2],
  gap: f32,
  width: Length,
) -> jellypilot_ui::widgets::control_button::ControlButton<'a, Message> {
  control_button_content(
    move |state| {
      let status = match state {
        IconControlState::Rest => button::Status::Active,
        IconControlState::Hovered => button::Status::Hovered,
        IconControlState::Disabled => button::Status::Disabled,
      };
      row![
        text(label.clone())
          .size(TOKENS.font_sizes.s12)
          .style(move |theme| text::Style {
            color: Some(
              jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Tonal).text_color,
            ),
          }),
        icon_for_control_state(Icon::ChevronDown, IconSize::Xs, ButtonVariant::Tonal, state,),
      ]
      .spacing(gap)
      .align_y(Alignment::Center)
      .into()
    },
    ButtonVariant::Tonal,
  )
  .padding(padding)
  .radius(TOKENS.radii.lg)
  .width(width)
}

fn filter_trigger<'a>(
  icon: Icon,
  label: String,
) -> jellypilot_ui::widgets::control_button::ControlButton<'a, Message> {
  control_button_content(
    move |state| {
      let status = match state {
        IconControlState::Rest => button::Status::Active,
        IconControlState::Hovered => button::Status::Hovered,
        IconControlState::Disabled => button::Status::Disabled,
      };
      row![
        icon_for_control_state(icon, IconSize::Xs, ButtonVariant::Tonal, state),
        text(label.clone())
          .size(TOKENS.font_sizes.s12)
          .style(move |theme| text::Style {
            color: Some(
              jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Tonal).text_color,
            ),
          }),
        icon_for_control_state(Icon::ChevronDown, IconSize::Xs, ButtonVariant::Tonal, state,),
      ]
      .spacing(TOKENS.spacing.s1_5)
      .align_y(Alignment::Center)
      .into()
    },
    ButtonVariant::Tonal,
  )
  .padding([TOKENS.spacing.s2, TOKENS.spacing.s3_5])
  .radius(TOKENS.radii.lg)
}

fn menu_option(
  label: String,
  selected: bool,
  message: SettingsMessage,
) -> Element<'static, Message> {
  control_button(
    None,
    Some(label),
    if selected {
      ButtonVariant::Secondary
    } else {
      ButtonVariant::Text
    },
  )
  .style(settings_style::navigation_button)
  .label_size(TOKENS.font_sizes.s12)
  .padding([TOKENS.spacing.s1_5, TOKENS.spacing.s2_5])
  .width(Fill)
  .label_fill(true)
  .on_press(Message::Settings(message))
  .into()
}

fn compact_icon_button<'a>(
  icon: Icon,
  label: String,
  enabled: bool,
  message: SettingsMessage,
) -> Element<'a, Message> {
  let trigger = control_button(Some(icon), None, ButtonVariant::Tonal)
    .icon_size(IconSize::Xs)
    .padding(TOKENS.spacing.s1_5)
    .content_centered(true)
    .on_press_maybe(enabled.then_some(Message::Settings(message)));
  tooltip(trigger, label, TooltipOptions::default())
}

fn intro_mode_label(locale: Localizer, mode: IntroMode) -> String {
  locale.text(match mode {
    IntroMode::Automatic => "settings-intro-automatic",
    IntroMode::Manual => "settings-intro-manual",
    IntroMode::Off => "settings-off",
  })
}

fn subtitle_language_label(locale: Localizer, code: &str) -> String {
  let id = match code {
    "eng" => "settings-subtitle-english",
    "spa" => "settings-subtitle-spanish",
    "fra" | "fre" => "settings-subtitle-french",
    "deu" | "ger" => "settings-subtitle-german",
    "ita" => "settings-subtitle-italian",
    "por" => "settings-subtitle-portuguese",
    "rus" => "settings-subtitle-russian",
    "zho" | "chi" => "settings-subtitle-chinese",
    "jpn" => "settings-subtitle-japanese",
    "kor" => "settings-subtitle-korean",
    "ara" => "settings-subtitle-arabic",
    "hin" => "settings-subtitle-hindi",
    _ => return code.to_owned(),
  };
  locale.text(id)
}

const fn diagnostic_badge(level: DiagnosticLevel) -> BadgeVariant {
  match level {
    DiagnosticLevel::Info => BadgeVariant::Neutral,
    DiagnosticLevel::Warning => BadgeVariant::Warning,
    DiagnosticLevel::Error => BadgeVariant::Error,
  }
}

fn diagnostic_level_label(locale: Localizer, level: Option<DiagnosticLevel>) -> String {
  locale.text(match level {
    None => "settings-all",
    Some(DiagnosticLevel::Info) => "settings-level-info",
    Some(DiagnosticLevel::Warning) => "settings-level-warning",
    Some(DiagnosticLevel::Error) => "settings-level-error",
  })
}

fn diagnostic_category_label(locale: Localizer, category: Option<DiagnosticCategory>) -> String {
  locale.text(match category {
    None => "settings-all",
    Some(DiagnosticCategory::Connection) => "settings-category-connection",
    Some(DiagnosticCategory::Auth) => "settings-category-auth",
    Some(DiagnosticCategory::Playback) => "settings-playback",
    Some(DiagnosticCategory::Player) => "settings-category-player",
    Some(DiagnosticCategory::RemoteControl) => "settings-category-remote-control",
    Some(DiagnosticCategory::Artwork) => "settings-category-artwork",
    Some(DiagnosticCategory::Config) => "settings-category-config",
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::i18n::UiText;
  use iced::advanced::{renderer::Headless, widget::operation::focusable};
  use iced::keyboard::{key, Event as KeyEvent, Key, Location, Modifiers};
  use iced_runtime::user_interface::{Cache, UserInterface};

  #[tokio::test]
  async fn feedback_changes_preserve_the_focused_field_and_enter_save() {
    let mut state = State::boot(true);
    state.shell.window_size = iced::Size::new(480.0, 760.0);
    state.settings.view.active_section = SettingsSection::Mpv;
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    let bounds = state.shell.window_size;
    let mut ui = UserInterface::build(view(&state), bounds, Cache::new(), &mut renderer);
    ui.operate(&renderer, &mut focusable::focus("settings-mpv-path".into()));
    let mut cache = ui.into_cache();
    for saved in [true, false] {
      state.settings.view.saved = saved.then(|| UiText::new("settings-saved"));
      let mut ui = UserInterface::build(view(&state), bounds, cache, &mut renderer);
      let enter = iced::Event::Keyboard(KeyEvent::KeyPressed {
        key: Key::Named(key::Named::Enter),
        modified_key: Key::Named(key::Named::Enter),
        physical_key: key::Physical::Code(key::Code::Enter),
        location: Location::Standard,
        modifiers: Modifiers::NONE,
        text: None,
        repeat: false,
      });
      let mut bus = iced::advanced::shell::Bus::new();
      let _ = ui.update(
        &iced::window::Headless,
        &iced::advanced::shell::Waker::noop(),
        &[enter],
        iced::mouse::Cursor::Unavailable,
        &mut renderer,
        &mut bus,
      );
      assert!(
        bus
          .drain()
          .any(|(message, _)| matches!(message, Message::Settings(SettingsMessage::SaveMpvPath))),
        "Enter must still save after saved feedback becomes {saved}"
      );
      cache = ui.into_cache();
    }
  }

  #[tokio::test]
  async fn translated_wide_footer_keeps_close_reachable_in_short_windows() {
    use iced::advanced::widget::{Id, Operation};
    use iced::{mouse, Event, Rectangle, Size};

    #[derive(Default)]
    struct CloseBounds(Option<Rectangle>);
    impl Operation for CloseBounds {
      fn focusable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _state: &mut dyn focusable::Focusable,
      ) {
        if id == Some(&Id::new("settings-close")) {
          self.0 = Some(bounds);
        }
      }

      fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
      }
    }

    let mut state = State::boot(true);
    state.kernel.settings = jellypilot_core::config::SettingsStore::default();
    state.settings.view.active_section = SettingsSection::Storage;
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    for language in [UiLanguage::English, UiLanguage::SimplifiedChinese] {
      state.kernel.locale = Localizer::new(language);
      for (width, height) in [
        (1080.0, 700.0),
        (852.0, 552.0),
        (432.0, 712.0),
        (432.0, 240.0),
      ] {
        state.shell.window_size = Size::new(width + 48.0, height + 48.0);
        let mut ui = UserInterface::build(
          view(&state),
          Size::new(width, height),
          Cache::new(),
          &mut renderer,
        );
        let mut close = CloseBounds::default();
        ui.operate(&renderer, &mut close);
        let bounds = close.0.expect("visible Close action");
        assert!(bounds.width >= 64.0 && bounds.height >= 40.0, "{bounds:?}");
        assert!(
          bounds.y >= 0.0 && bounds.y + bounds.height <= height,
          "{bounds:?}"
        );
        let mut bus = iced::advanced::shell::Bus::new();
        let _ = ui.update(
          &iced::window::Headless,
          &iced::advanced::shell::Waker::noop(),
          &[
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
          ],
          mouse::Cursor::Available(bounds.center()),
          &mut renderer,
          &mut bus,
        );
        assert!(bus
          .drain()
          .any(|(message, _)| matches!(message, Message::Settings(SettingsMessage::Close))));
      }
    }
  }

  #[tokio::test]
  async fn settings_controls_fit_the_available_width_without_horizontal_scrolling() {
    use iced::advanced::{layout, widget};
    use iced::Size;

    fn assert_fits(node: &layout::Node, parent_x: f32, width: f32) {
      let bounds = node.bounds();
      let x = parent_x + bounds.x;
      assert!(
        x >= -0.5 && x + bounds.width <= width + 0.5,
        "content at x={x} with width={} extends beyond viewport {width}",
        bounds.width
      );
      for child in node.children() {
        assert_fits(child, x, width);
      }
    }

    let mut state = State::boot(true);
    state.kernel.settings = jellypilot_core::config::SettingsStore::default();
    state.kernel.connection = jellypilot_auth::login::ConnectionPhase::Connected;
    state.kernel.connected_identity = Some(crate::app::state::ConnectedIdentity {
      user_name: "Current user".to_owned(),
      provider: jellypilot_media_server::MediaServerProvider::Jellyfin,
      server_url: "https://media.example.test".to_owned(),
      server_name: Some("Media server".to_owned()),
    });
    let renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    for language in [UiLanguage::English, UiLanguage::SimplifiedChinese] {
      state.kernel.locale = Localizer::new(language);
      for width in [1020.0, 852.0, 432.0] {
        for section in SettingsSection::ALL {
          state.settings.view.active_section = section;
          state.shell.window_size = Size::new(width + 48.0, 760.0);
          let mut element = view(&state);
          let mut tree = widget::Tree::new(&element);
          tree.diff(element.as_widget_mut());
          let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(width, 712.0)),
          );
          assert_fits(&node, 0.0, width);
        }
      }
    }
  }
}
