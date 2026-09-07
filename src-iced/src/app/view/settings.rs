use iced::widget::{column, container, row, scrollable, space, text, text_input, Column};
use iced::{Alignment, Element, Fill, Length};
use jellypilot_core::config::{AppMode, IntroMode, ShortcutKind, ThemeMode};
use jellypilot_core::diagnostics::{format_diagnostic_time, DiagnosticCategory, DiagnosticLevel};
use jellypilot_core::locale::{LanguagePreference, UiLanguage};
use jellypilot_core::settings::SUBTITLE_LANGUAGE_OPTIONS;
use jellypilot_ui::fonts::{FONT_ATTRIBUTIONS, FONT_LICENSES, HEADING_FONT};
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::layout::SizeClass;
use jellypilot_ui::overlay::{popover, tooltip, PopoverOptions, TooltipOptions};
use jellypilot_ui::tokens::{ThemePalette, TOKENS};
use jellypilot_ui::variants::{BadgeVariant, ButtonVariant, FieldVariant, SurfaceVariant};
use jellypilot_ui::widgets::control_button::control_button;

use super::account;

use crate::app::message::{Message, SettingsMessage};
use crate::app::shell::SETTINGS_INITIAL_FOCUS_ID;
use crate::app::state::{diagnostic_matches, SettingsSection, State};
use crate::i18n::Localizer;

pub fn view(state: &State) -> Element<'_, Message> {
  let active = state.settings.view.active_section;
  let selected = selected_section(state, active);
  let feedback = feedback(state);
  let class = SizeClass::from_width(state.shell.window_size.width);
  let show_two_columns = state.app_mode() == AppMode::Full && class != SizeClass::Compact;

  if show_two_columns {
    let navigation = settings_navigation(active, state.kernel.locale);
    let content = scrollable(
      column![feedback, selected]
        .spacing(TOKENS.spacing.s4)
        .padding([TOKENS.spacing.s2, TOKENS.spacing.s6])
        .width(Fill),
    )
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable);
    return row![
      container(navigation)
        .width(Length::Fixed(208.0))
        .height(Fill)
        .padding([TOKENS.spacing.s2, TOKENS.spacing.s3])
        .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Block)),
      content,
    ]
    .spacing(TOKENS.spacing.s4)
    .width(Fill)
    .height(Fill)
    .into();
  }

  let mut content = column![
    feedback,
    settings_navigation(active, state.kernel.locale),
    selected,
  ]
  .spacing(TOKENS.spacing.s4)
  .width(Fill);
  if state.app_mode() == AppMode::ControlOnly {
    content = column![back_to_now_playing(state.kernel.locale), content]
      .spacing(TOKENS.spacing.s2)
      .width(Fill);
  }
  scrollable(container(content).padding([TOKENS.spacing.s2, TOKENS.spacing.s6]))
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable)
    .into()
}

fn selected_section<'a>(state: &'a State, section: SettingsSection) -> Element<'a, Message> {
  match section {
    SettingsSection::Account => connection_section(state),
    SettingsSection::Mpv => mpv_section(state),
    SettingsSection::Playback => playback_section(state),
    SettingsSection::Subtitles => subtitles_section(state),
    SettingsSection::Shortcuts => shortcuts_section(state),
    SettingsSection::Appearance => interface_section(state),
    SettingsSection::Storage => cache_section(state),
    SettingsSection::Diagnostics => column![diagnostics_section(state), about_section(state)]
      .spacing(TOKENS.spacing.s4)
      .width(Fill)
      .into(),
  }
}

fn settings_navigation(active: SettingsSection, locale: Localizer) -> Column<'static, Message> {
  let mut navigation = Column::new().spacing(TOKENS.spacing.s1_5).width(Fill);
  for section in SettingsSection::ALL {
    let variant = if section == active {
      ButtonVariant::Secondary
    } else {
      ButtonVariant::Text
    };
    let button = control_button(
      Some(settings_icon(section)),
      Some(section.label(locale)),
      variant,
    )
    .icon_size(IconSize::Sm)
    .label_size(13.0)
    .spacing(TOKENS.spacing.s2)
    .padding([7, 10])
    .width(Fill)
    .label_fill(true)
    .on_press(Message::Settings(SettingsMessage::SectionSelected(section)));
    navigation = navigation.push(if section == active {
      button.id(SETTINGS_INITIAL_FOCUS_ID)
    } else {
      button
    });
  }
  navigation
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

fn back_to_now_playing<'a>(locale: Localizer) -> Element<'a, Message> {
  control_button(
    Some(Icon::ChevronLeft),
    Some(locale.text("settings-now-playing")),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press(Message::Settings(SettingsMessage::Close))
  .into()
}

fn feedback(state: &State) -> Element<'_, Message> {
  if let Some(error) = &state.settings.view.error {
    return text(state.kernel.locale.message(error))
      .size(13)
      .color(state.palette().colors.error)
      .into();
  }
  if let Some(saved) = &state.settings.view.saved {
    return badge(state.kernel.locale.message(saved), BadgeVariant::Success);
  }
  space::vertical().height(0).into()
}

fn connection_section(state: &State) -> Element<'_, Message> {
  account::management(state)
}

fn mpv_section(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let path = text_input(
    &state.t("settings-mpv-path-placeholder"),
    &state.settings.view.mpv_path_input,
  )
  .on_input(|value| Message::Settings(SettingsMessage::MpvPathChanged(value)))
  .on_submit(Message::Settings(SettingsMessage::SaveMpvPath))
  .padding([7, 10])
  .width(Fill)
  .style(|theme, status| jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled));
  let args = text_input(
    &state.t("settings-mpv-args-placeholder"),
    &state.settings.view.mpv_args_input,
  )
  .on_input(|value| Message::Settings(SettingsMessage::MpvArgsChanged(value)))
  .on_submit(Message::Settings(SettingsMessage::SaveMpvArgs))
  .padding([7, 10])
  .width(Fill)
  .style(|theme, status| jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled));
  section(
    palette,
    Icon::Cpu,
    state.t("settings-mpv"),
    column![
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
    .spacing(TOKENS.spacing.s4),
  )
}

fn playback_section(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let target = text_input(
    "JellyPilot",
    &state.settings.view.playback_target_name_input,
  )
  .on_input(|value| Message::Settings(SettingsMessage::PlaybackTargetNameChanged(value)))
  .on_submit(Message::Settings(SettingsMessage::SavePlaybackTargetName))
  .padding([7, 10])
  .width(Fill)
  .style(|theme, status| jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled));
  let mode = state.kernel.settings.snapshot().intro_mode();
  let trigger = control_button(
    None,
    Some(state.format(
      "settings-intro-selected",
      &[("mode", intro_mode_label(state.kernel.locale, mode).into())],
    )),
    ButtonVariant::Tonal,
  )
  .padding([6, 12])
  .on_press(Message::Settings(SettingsMessage::IntroMenuToggled));
  let menu = column![
    intro_option(
      state.t("settings-intro-automatic"),
      IntroMode::Automatic,
      mode
    ),
    intro_option(state.t("settings-intro-manual"), IntroMode::Manual, mode),
    intro_option(state.t("settings-off"), IntroMode::Off, mode),
  ]
  .spacing(TOKENS.spacing.s1)
  .width(Fill);
  let intro = popover(
    trigger,
    menu,
    state.settings.view.intro_menu_open,
    PopoverOptions {
      width: Some(220.0),
      ..PopoverOptions::default()
    },
    Message::Settings(SettingsMessage::IntroMenuDismissed),
  );
  section(
    palette,
    Icon::Sliders,
    state.t("settings-playback"),
    column![
      labeled_field(
        palette,
        state.kernel.locale,
        state.t("settings-target-name"),
        state.t("settings-target-name-help"),
        target.into(),
        SettingsMessage::SavePlaybackTargetName,
      ),
      column![
        text(state.t("settings-intro"))
          .size(14)
          .color(palette.text.secondary),
        text(state.t("settings-intro-help"))
          .size(12)
          .color(palette.text.body),
        intro,
      ]
      .spacing(TOKENS.spacing.s2),
    ]
    .spacing(TOKENS.spacing.s4),
  )
}

fn subtitles_section(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let trigger = control_button(
    Some(Icon::Subtitles),
    Some(state.t("settings-add-language")),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 12])
  .on_press(Message::Settings(SettingsMessage::SubtitleMenuToggled));
  let mut menu = Column::new().spacing(TOKENS.spacing.s1).width(Fill);
  for language in SUBTITLE_LANGUAGE_OPTIONS {
    menu = menu.push(
      control_button(
        None,
        Some(subtitle_language_label(state.kernel.locale, language)),
        ButtonVariant::Text,
      )
      .padding([6, 10])
      .width(Fill)
      .label_fill(true)
      .on_press(Message::Settings(SettingsMessage::SubtitleLanguageAdded(
        language.to_owned(),
      ))),
    );
  }
  let add = popover(
    trigger,
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
        .size(12)
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
        .size(13)
        .width(Fill),
        compact_button(
          Icon::ArrowUp,
          state.t("settings-move-up"),
          index > 0,
          SettingsMessage::SubtitleLanguageMoved { index, offset: -1 },
        ),
        compact_button(
          Icon::ArrowDown,
          state.t("settings-move-down"),
          index + 1 < languages.len(),
          SettingsMessage::SubtitleLanguageMoved { index, offset: 1 },
        ),
        compact_button(
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
  section(
    palette,
    Icon::Subtitles,
    state.t("settings-subtitles"),
    column![
      text(state.t("settings-subtitles-help"))
        .size(12)
        .color(palette.text.metadata),
      rows,
      add,
    ]
    .spacing(TOKENS.spacing.s3),
  )
}

fn shortcuts_section(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  section(
    palette,
    Icon::Keyboard,
    state.t("settings-shortcuts"),
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
      text(state.t("settings-shortcuts-help"))
        .size(12)
        .color(palette.text.metadata),
    ]
    .spacing(TOKENS.spacing.s2),
  )
}

fn shortcut_row<'a>(state: &'a State, label: String, kind: ShortcutKind) -> Element<'a, Message> {
  let binding = match kind {
    ShortcutKind::Next => state.kernel.settings.snapshot().key_next_episode(),
    ShortcutKind::Previous => state.kernel.settings.snapshot().key_previous_episode(),
    ShortcutKind::IntroSkip => state.kernel.settings.snapshot().key_intro_skip(),
  };
  let capturing = state.settings.view.shortcut_capture == Some(kind);
  let variant = if capturing {
    ButtonVariant::TonalActive
  } else {
    ButtonVariant::Tonal
  };
  row![
    row![
      icon_with_color(
        Icon::Keyboard,
        IconSize::Sm,
        state.palette().colors.onSurfaceVariant
      ),
      text(label).size(13),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center)
    .width(Fill),
    control_button(
      Some(Icon::Keyboard),
      Some(if capturing {
        state.t("settings-press-key")
      } else {
        binding.to_owned()
      }),
      variant,
    )
    .icon_size(IconSize::Xs)
    .spacing(TOKENS.spacing.s1_5)
    .padding([5, 10])
    .on_press(Message::Settings(SettingsMessage::BeginShortcutCapture(
      kind
    ))),
  ]
  .align_y(Alignment::Center)
  .spacing(TOKENS.spacing.s3)
  .into()
}

fn interface_section(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let theme_mode = state.kernel.settings.snapshot().theme_mode();
  let app_mode = state.kernel.settings.snapshot().app_mode();
  let reduced_motion = state.kernel.settings.snapshot().reduced_motion();
  section(
    palette,
    Icon::Settings,
    state.t("settings-interface"),
    column![
      language_row(state),
      appearance_row(palette, state.kernel.locale, theme_mode),
      app_mode_row(palette, state.kernel.locale, app_mode),
      toggle_row(
        palette,
        state.kernel.locale,
        state.t("settings-reduce-motion"),
        state.t("settings-reduce-motion-help"),
        reduced_motion,
        SettingsMessage::ReducedMotionToggled,
      ),
    ]
    .spacing(TOKENS.spacing.s4),
  )
}
fn appearance_row<'a>(
  palette: &ThemePalette,
  locale: Localizer,
  selected: ThemeMode,
) -> Element<'a, Message> {
  row![
    column![
      text(locale.text("settings-appearance"))
        .size(14)
        .color(palette.text.secondary),
      text(locale.text("settings-appearance-help"))
        .size(12)
        .color(palette.text.body),
    ]
    .spacing(TOKENS.spacing.s1)
    .width(Fill),
    row![
      theme_mode_option(locale.text("settings-system"), ThemeMode::System, selected),
      theme_mode_option(locale.text("settings-dark"), ThemeMode::Dark, selected),
      theme_mode_option(locale.text("settings-light"), ThemeMode::Light, selected),
    ]
    .spacing(TOKENS.spacing.s2),
  ]
  .spacing(TOKENS.spacing.s3)
  .align_y(Alignment::Center)
  .into()
}

fn theme_mode_option(
  label: String,
  value: ThemeMode,
  selected: ThemeMode,
) -> Element<'static, Message> {
  let variant = if value == selected {
    ButtonVariant::TonalActive
  } else {
    ButtonVariant::Tonal
  };
  control_button(None, Some(label), variant)
    .padding([5, 10])
    .on_press(Message::Settings(SettingsMessage::ThemeModeSelected(value)))
    .into()
}

fn app_mode_row<'a>(
  palette: &ThemePalette,
  locale: Localizer,
  selected: AppMode,
) -> Element<'a, Message> {
  row![
    column![
      text(locale.text("settings-app-mode"))
        .size(14)
        .color(palette.text.secondary),
      text(locale.text("settings-app-mode-help"))
        .size(12)
        .color(palette.text.body),
    ]
    .spacing(TOKENS.spacing.s1)
    .width(Fill),
    row![
      app_mode_option(
        locale.text("settings-app-mode-full"),
        AppMode::Full,
        selected
      ),
      app_mode_option(
        locale.text("settings-app-mode-control-only"),
        AppMode::ControlOnly,
        selected
      ),
    ]
    .spacing(TOKENS.spacing.s2),
  ]
  .spacing(TOKENS.spacing.s3)
  .align_y(Alignment::Center)
  .into()
}

fn app_mode_option(label: String, value: AppMode, selected: AppMode) -> Element<'static, Message> {
  let variant = if value == selected {
    ButtonVariant::TonalActive
  } else {
    ButtonVariant::Tonal
  };
  control_button(None, Some(label), variant)
    .padding([5, 10])
    .on_press(Message::Settings(SettingsMessage::AppModeSelected(value)))
    .into()
}

fn cache_section(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let cache_enabled = state.kernel.settings.snapshot().image_cache_enabled();
  let start_minimized = state.kernel.settings.snapshot().start_minimized();
  section(
    palette,
    Icon::Database,
    state.t("settings-cache"),
    column![
      toggle_row(
        palette,
        state.kernel.locale,
        state.t("settings-image-cache"),
        state.t("settings-image-cache-help"),
        cache_enabled,
        SettingsMessage::ImageCacheToggled,
      ),
      toggle_row(
        palette,
        state.kernel.locale,
        state.t("settings-start-minimized"),
        state.t("settings-start-minimized-help"),
        start_minimized,
        SettingsMessage::StartMinimizedToggled,
      ),
    ]
    .spacing(TOKENS.spacing.s4),
  )
}

fn diagnostics_section(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let level_trigger = control_button(
    Some(Icon::Filter),
    Some(state.format(
      "settings-level-filter",
      &[(
        "level",
        diagnostic_level_label(state.kernel.locale, state.settings.view.diagnostic_level).into(),
      )],
    )),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press(Message::Settings(
    SettingsMessage::DiagnosticLevelMenuToggled,
  ));
  let level_menu = column![
    diagnostic_level_option(state.t("settings-all"), None),
    diagnostic_level_option(state.t("settings-level-info"), Some(DiagnosticLevel::Info)),
    diagnostic_level_option(
      state.t("settings-level-warning"),
      Some(DiagnosticLevel::Warning)
    ),
    diagnostic_level_option(
      state.t("settings-level-error"),
      Some(DiagnosticLevel::Error)
    ),
  ]
  .spacing(TOKENS.spacing.s1)
  .width(Fill);
  let level_filter = popover(
    level_trigger,
    level_menu,
    state.settings.view.diagnostic_level_menu_open,
    PopoverOptions {
      width: Some(180.0),
      ..PopoverOptions::default()
    },
    Message::Settings(SettingsMessage::DiagnosticLevelMenuDismissed),
  );
  let category_trigger = control_button(
    Some(Icon::Sliders),
    Some(
      state.format(
        "settings-category-filter",
        &[(
          "category",
          diagnostic_category_label(state.kernel.locale, state.settings.view.diagnostic_category)
            .into(),
        )],
      ),
    ),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press(Message::Settings(
    SettingsMessage::DiagnosticCategoryMenuToggled,
  ));
  let category_menu = column![
    diagnostic_category_option(state.t("settings-all"), None),
    diagnostic_category_option(
      state.t("settings-category-connection"),
      Some(DiagnosticCategory::Connection)
    ),
    diagnostic_category_option(
      state.t("settings-category-auth"),
      Some(DiagnosticCategory::Auth)
    ),
    diagnostic_category_option(
      state.t("settings-playback"),
      Some(DiagnosticCategory::Playback)
    ),
    diagnostic_category_option(
      state.t("settings-category-remote-control"),
      Some(DiagnosticCategory::RemoteControl)
    ),
    diagnostic_category_option(
      state.t("settings-category-artwork"),
      Some(DiagnosticCategory::Artwork)
    ),
    diagnostic_category_option(
      state.t("settings-category-config"),
      Some(DiagnosticCategory::Config)
    ),
  ]
  .spacing(TOKENS.spacing.s1)
  .width(Fill);
  let category_filter = popover(
    category_trigger,
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
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press(Message::Settings(SettingsMessage::ExportLogs));

  let mut events = Column::new().spacing(TOKENS.spacing.s2).width(Fill);
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
    events = events.push(
      container(
        column![
          row![
            row![
              icon_with_color(level_icon, IconSize::Xs, level_color),
              badge(
                diagnostic_level_label(state.kernel.locale, Some(diagnostic.level)),
                diagnostic_badge(diagnostic.level)
              ),
            ]
            .spacing(TOKENS.spacing.s1)
            .align_y(Alignment::Center),
            text(diagnostic_category_label(
              state.kernel.locale,
              Some(diagnostic.category)
            ))
            .size(12)
            .color(palette.text.metadata),
            space::horizontal(),
            text(format_diagnostic_time(diagnostic.timestamp_seconds))
              .size(11)
              .color(palette.text.metadata),
          ]
          .spacing(TOKENS.spacing.s2)
          .align_y(Alignment::Center),
          text(diagnostic.message)
            .size(12)
            .color(palette.text.secondary),
        ]
        .spacing(TOKENS.spacing.s2),
      )
      .padding(TOKENS.spacing.s3)
      .width(Fill)
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas)),
    );
  }
  if count == 0 {
    events = events.push(
      text(state.t("settings-diagnostics-empty"))
        .size(12)
        .color(palette.text.metadata),
    );
  }
  section(
    palette,
    Icon::Activity,
    state.t("settings-diagnostics"),
    column![
      row![
        level_filter,
        category_filter,
        space::horizontal().width(Fill),
        export_button,
      ]
      .spacing(TOKENS.spacing.s2),
      text(state.format("settings-event-count", &[("count", count.into())]))
        .size(11)
        .color(palette.text.metadata),
      events,
    ]
    .spacing(TOKENS.spacing.s3),
  )
}

fn about_section(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let expanded = state.settings.view.font_licenses_expanded;
  let mut content = column![
    text("JellyPilot").size(15).color(palette.text.heading),
    text(state.format(
      "settings-version",
      &[("version", env!("CARGO_PKG_VERSION").into())]
    ))
    .size(12)
    .color(palette.text.muted),
    text(FONT_ATTRIBUTIONS).size(12).color(palette.text.body),
    control_button(
      None,
      Some(state.t(if expanded {
        "settings-hide-font-licenses"
      } else {
        "settings-show-font-licenses"
      })),
      ButtonVariant::Tonal,
    )
    .padding([6, 10])
    .on_press(Message::Settings(SettingsMessage::FontLicensesToggled)),
  ]
  .spacing(TOKENS.spacing.s3);
  if expanded {
    content = content.push(
      text(FONT_LICENSES)
        .size(12)
        .color(palette.text.body)
        .width(Fill),
    );
  }
  section(palette, Icon::Info, state.t("settings-about"), content)
}

fn section<'a>(
  palette: &ThemePalette,
  icon: Icon,
  title: String,
  content: Column<'a, Message>,
) -> Element<'a, Message> {
  container(
    column![
      row![
        icon_with_color(icon, IconSize::Md, palette.colors.primary),
        text(title)
          .font(HEADING_FONT)
          .size(18)
          .color(palette.text.heading),
      ]
      .spacing(TOKENS.spacing.s2)
      .align_y(Alignment::Center),
      content,
    ]
    .spacing(TOKENS.spacing.s3),
  )
  .padding(TOKENS.spacing.s4)
  .width(Fill)
  .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
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
    text(label).size(14).color(palette.text.secondary),
    text(help).size(12).color(palette.text.body),
    row![
      field,
      control_button(
        Some(Icon::Check),
        Some(locale.text("settings-save")),
        ButtonVariant::Primary,
      )
      .icon_size(IconSize::Sm)
      .spacing(TOKENS.spacing.s1_5)
      .padding([6, 12])
      .on_press(Message::Settings(save)),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center),
  ]
  .spacing(TOKENS.spacing.s2)
  .into()
}

fn toggle_row<'a>(
  palette: &ThemePalette,
  locale: Localizer,
  label: String,
  help: String,
  enabled: bool,
  message: SettingsMessage,
) -> Element<'a, Message> {
  row![
    column![
      text(label).size(14).color(palette.text.secondary),
      text(help).size(12).color(palette.text.body),
    ]
    .spacing(TOKENS.spacing.s1)
    .width(Fill),
    control_button(
      None,
      Some(locale.text(if enabled {
        "settings-on"
      } else {
        "settings-off"
      })),
      if enabled {
        ButtonVariant::TonalActive
      } else {
        ButtonVariant::Tonal
      },
    )
    .padding([5, 10])
    .on_press(Message::Settings(message)),
  ]
  .spacing(TOKENS.spacing.s3)
  .align_y(Alignment::Center)
  .into()
}

fn compact_button<'a>(
  icon: Icon,
  label: String,
  enabled: bool,
  message: SettingsMessage,
) -> Element<'a, Message> {
  let trigger = control_button(Some(icon), None, ButtonVariant::Tonal)
    .icon_size(IconSize::Xs)
    .padding([5, 8])
    .on_press_maybe(enabled.then_some(Message::Settings(message)));
  tooltip(trigger, label, TooltipOptions::default())
}

fn intro_option(label: String, value: IntroMode, selected: IntroMode) -> Element<'static, Message> {
  let variant = if value == selected {
    ButtonVariant::Secondary
  } else {
    ButtonVariant::Text
  };
  control_button(None, Some(label), variant)
    .padding([6, 10])
    .width(Fill)
    .label_fill(true)
    .on_press(Message::Settings(SettingsMessage::IntroModeSelected(value)))
    .into()
}

fn diagnostic_level_option(
  label: String,
  level: Option<DiagnosticLevel>,
) -> Element<'static, Message> {
  control_button(None, Some(label), ButtonVariant::Text)
    .padding([6, 10])
    .width(Fill)
    .label_fill(true)
    .on_press(Message::Settings(SettingsMessage::DiagnosticLevelSelected(
      level,
    )))
    .into()
}

fn diagnostic_category_option(
  label: String,
  category: Option<DiagnosticCategory>,
) -> Element<'static, Message> {
  control_button(None, Some(label), ButtonVariant::Text)
    .padding([6, 10])
    .width(Fill)
    .label_fill(true)
    .on_press(Message::Settings(
      SettingsMessage::DiagnosticCategorySelected(category),
    ))
    .into()
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

fn badge<'a, Message: 'a>(label: String, variant: BadgeVariant) -> Element<'a, Message> {
  container(text(label).size(12))
    .padding([3, 8])
    .style(move |theme| jellypilot_ui::theme::badge_variant(theme, variant))
    .into()
}

const fn diagnostic_badge(level: DiagnosticLevel) -> BadgeVariant {
  match level {
    DiagnosticLevel::Info => BadgeVariant::Neutral,
    DiagnosticLevel::Warning => BadgeVariant::Warning,
    DiagnosticLevel::Error => BadgeVariant::Neutral,
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
    Some(DiagnosticCategory::RemoteControl) => "settings-category-remote-control",
    Some(DiagnosticCategory::Artwork) => "settings-category-artwork",
    Some(DiagnosticCategory::Config) => "settings-category-config",
  })
}

fn language_row(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let selected = state.kernel.settings.snapshot().ui_language();
  let mut options = row![].spacing(TOKENS.spacing.s2);
  for (id, value) in [
    ("language-system", LanguagePreference::System),
    (
      "language-english",
      LanguagePreference::Fixed(UiLanguage::English),
    ),
    (
      "language-chinese",
      LanguagePreference::Fixed(UiLanguage::SimplifiedChinese),
    ),
  ] {
    options = options.push(
      control_button(
        None,
        Some(state.t(id)),
        if selected == value {
          ButtonVariant::TonalActive
        } else {
          ButtonVariant::Tonal
        },
      )
      .padding([5, 10])
      .on_press(Message::UiLanguageSelected(value)),
    );
  }
  column![
    text(state.t("language-label"))
      .size(14)
      .color(palette.text.secondary),
    text(state.t("language-description"))
      .size(12)
      .color(palette.text.body),
    options.wrap(),
  ]
  .spacing(TOKENS.spacing.s2)
  .into()
}
