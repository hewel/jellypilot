use iced::widget::{column, container, row, scrollable, space, text, text_input, Column};
use iced::{Alignment, Element, Fill, Length};
use jellypilot_core::config::{AppMode, IntroMode, ShortcutKind, ThemeMode};
use jellypilot_core::diagnostics::{format_diagnostic_time, DiagnosticCategory, DiagnosticLevel};
use jellypilot_core::settings::SUBTITLE_LANGUAGE_OPTIONS;
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
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

pub fn view(state: &State) -> Element<'_, Message> {
  let active = state.settings.view.active_section;
  let selected = selected_section(state, active);
  let feedback = feedback(state);
  let class = SizeClass::from_width(state.shell.window_size.width);
  let show_two_columns = state.app_mode() == AppMode::Full && class != SizeClass::Compact;

  if show_two_columns {
    let navigation = settings_navigation(active);
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

  let mut content = column![feedback, settings_navigation(active), selected,]
    .spacing(TOKENS.spacing.s4)
    .width(Fill);
  if state.app_mode() == AppMode::ControlOnly {
    content = column![back_to_now_playing(), content]
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
    SettingsSection::Diagnostics => {
      column![diagnostics_section(state), about_section(state.palette())]
        .spacing(TOKENS.spacing.s4)
        .width(Fill)
        .into()
    }
  }
}

fn settings_navigation(active: SettingsSection) -> Column<'static, Message> {
  let mut navigation = Column::new().spacing(TOKENS.spacing.s1_5).width(Fill);
  for section in SettingsSection::ALL {
    let variant = if section == active {
      ButtonVariant::Secondary
    } else {
      ButtonVariant::Text
    };
    let button = control_button(
      Some(settings_icon(section)),
      Some(section.label().to_owned()),
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

fn back_to_now_playing<'a>() -> Element<'a, Message> {
  control_button(
    Some(Icon::ChevronLeft),
    Some("Now Playing".to_owned()),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press(Message::Settings(SettingsMessage::Close))
  .into()
}

fn feedback(state: &State) -> Element<'_, Message> {
  if let Some(error) = state.settings.view.error {
    return text(error)
      .size(13)
      .color(state.palette().colors.error)
      .into();
  }
  if let Some(saved) = state.settings.view.saved {
    return badge(saved, BadgeVariant::Success);
  }
  space::vertical().height(0).into()
}

fn connection_section(state: &State) -> Element<'_, Message> {
  account::management(state)
}

fn mpv_section(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let path = text_input("Auto-detect from PATH", &state.settings.view.mpv_path_input)
    .on_input(|value| Message::Settings(SettingsMessage::MpvPathChanged(value)))
    .on_submit(Message::Settings(SettingsMessage::SaveMpvPath))
    .padding([7, 10])
    .width(Fill)
    .style(|theme, status| {
      jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
    });
  let args = text_input(
    "Additional MPV arguments",
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
    "MPV",
    column![
      labeled_field(
        palette,
        "Executable path",
        "Leave empty to discover MPV from PATH. Applies to future MPV process starts.",
        path.into(),
        SettingsMessage::SaveMpvPath,
      ),
      labeled_field(
        palette,
        "Extra arguments",
        "Whitespace-separated arguments. Applies to future playback starts.",
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
    Some(format!("Intro Skipper: {}", intro_mode_label(mode))),
    ButtonVariant::Tonal,
  )
  .padding([6, 12])
  .on_press(Message::Settings(SettingsMessage::IntroMenuToggled));
  let menu = column![
    intro_option("Automatic", IntroMode::Automatic, mode),
    intro_option("Manual", IntroMode::Manual, mode),
    intro_option("Off", IntroMode::Off, mode),
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
    "Playback",
    column![
      labeled_field(
        palette,
        "Playback Target name",
        "Saved names are re-registered with the connected server immediately.",
        target.into(),
        SettingsMessage::SavePlaybackTargetName,
      ),
      column![
        text("Intro Skipper").size(14).color(palette.text.secondary),
        text("Automatic skips detected intros; Manual shows the skip action; Off disables it.")
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
    Some("Add language".to_owned()),
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
        Some(subtitle_language_label(language).to_owned()),
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
      text("No preferred subtitle languages.")
        .size(12)
        .color(palette.text.metadata),
    );
  }
  for (index, language) in languages.iter().enumerate() {
    rows = rows.push(
      row![
        text(format!(
          "{}. {}",
          index + 1,
          subtitle_language_label(language)
        ))
        .size(13)
        .width(Fill),
        compact_button(
          Icon::ArrowUp,
          "Move up",
          index > 0,
          SettingsMessage::SubtitleLanguageMoved { index, offset: -1 },
        ),
        compact_button(
          Icon::ArrowDown,
          "Move down",
          index + 1 < languages.len(),
          SettingsMessage::SubtitleLanguageMoved { index, offset: 1 },
        ),
        compact_button(
          Icon::Trash,
          "Remove",
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
    "Subtitles",
    column![
      text("Languages are tried from top to bottom on future playback starts.")
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
    "Shortcuts",
    column![
      shortcut_row(state, "Next episode", ShortcutKind::Next),
      shortcut_row(state, "Previous episode", ShortcutKind::Previous),
      shortcut_row(state, "Skip intro", ShortcutKind::IntroSkip),
      text("Shortcut subscriptions read the current persisted bindings.")
        .size(12)
        .color(palette.text.metadata),
    ]
    .spacing(TOKENS.spacing.s2),
  )
}

fn shortcut_row<'a>(state: &'a State, label: &'a str, kind: ShortcutKind) -> Element<'a, Message> {
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
        "Press a key…".to_owned()
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
    "Interface",
    column![
      appearance_row(palette, theme_mode),
      app_mode_row(palette, app_mode),
      toggle_row(
        palette,
        "Reduce motion",
        "Shows skeleton loading placeholders without the shimmer animation.",
        reduced_motion,
        SettingsMessage::ReducedMotionToggled,
      ),
    ]
    .spacing(TOKENS.spacing.s4),
  )
}
fn appearance_row<'a>(palette: &ThemePalette, selected: ThemeMode) -> Element<'a, Message> {
  row![
    column![
      text("Appearance").size(14).color(palette.text.secondary),
      text("System follows the OS light/dark setting and switches live.")
        .size(12)
        .color(palette.text.body),
    ]
    .spacing(TOKENS.spacing.s1)
    .width(Fill),
    row![
      theme_mode_option("System", ThemeMode::System, selected),
      theme_mode_option("Dark", ThemeMode::Dark, selected),
      theme_mode_option("Light", ThemeMode::Light, selected),
    ]
    .spacing(TOKENS.spacing.s2),
  ]
  .spacing(TOKENS.spacing.s3)
  .align_y(Alignment::Center)
  .into()
}

fn theme_mode_option(
  label: &'static str,
  value: ThemeMode,
  selected: ThemeMode,
) -> Element<'static, Message> {
  let variant = if value == selected {
    ButtonVariant::TonalActive
  } else {
    ButtonVariant::Tonal
  };
  control_button(None, Some(label.to_owned()), variant)
    .padding([5, 10])
    .on_press(Message::Settings(SettingsMessage::ThemeModeSelected(value)))
    .into()
}

fn app_mode_row<'a>(palette: &ThemePalette, selected: AppMode) -> Element<'a, Message> {
  row![
    column![
      text("App mode").size(14).color(palette.text.secondary),
      text("Control only shows a compact fixed-size player window without the library browser; it switches live.")
        .size(12)
        .color(palette.text.body),
    ]
    .spacing(TOKENS.spacing.s1)
    .width(Fill),
    row![
      app_mode_option("Full", AppMode::Full, selected),
      app_mode_option("Control only", AppMode::ControlOnly, selected),
    ]
    .spacing(TOKENS.spacing.s2),
  ]
  .spacing(TOKENS.spacing.s3)
  .align_y(Alignment::Center)
  .into()
}

fn app_mode_option(
  label: &'static str,
  value: AppMode,
  selected: AppMode,
) -> Element<'static, Message> {
  let variant = if value == selected {
    ButtonVariant::TonalActive
  } else {
    ButtonVariant::Tonal
  };
  control_button(None, Some(label.to_owned()), variant)
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
    "Cache",
    column![
      toggle_row(
        palette,
        "Image disk cache",
        "Caches encoded artwork on disk and applies immediately.",
        cache_enabled,
        SettingsMessage::ImageCacheToggled,
      ),
      toggle_row(
        palette,
        "Start minimized",
        "Starts hidden only when the system tray initializes successfully.",
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
    Some(format!(
      "Level: {}",
      state
        .settings
        .view
        .diagnostic_level
        .map_or("All", DiagnosticLevel::label)
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
    diagnostic_level_option("All", None),
    diagnostic_level_option("Info", Some(DiagnosticLevel::Info)),
    diagnostic_level_option("Warning", Some(DiagnosticLevel::Warning)),
    diagnostic_level_option("Error", Some(DiagnosticLevel::Error)),
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
    Some(format!(
      "Category: {}",
      state
        .settings
        .view
        .diagnostic_category
        .map_or("All", DiagnosticCategory::label)
    )),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press(Message::Settings(
    SettingsMessage::DiagnosticCategoryMenuToggled,
  ));
  let category_menu = column![
    diagnostic_category_option("All", None),
    diagnostic_category_option("Connection", Some(DiagnosticCategory::Connection)),
    diagnostic_category_option("Auth", Some(DiagnosticCategory::Auth)),
    diagnostic_category_option("Playback", Some(DiagnosticCategory::Playback)),
    diagnostic_category_option("Remote Control", Some(DiagnosticCategory::RemoteControl)),
    diagnostic_category_option("Artwork", Some(DiagnosticCategory::Artwork)),
    diagnostic_category_option("Config", Some(DiagnosticCategory::Config)),
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
    Some("Export logs".to_owned()),
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
              badge(diagnostic.level.label(), diagnostic_badge(diagnostic.level)),
            ]
            .spacing(TOKENS.spacing.s1)
            .align_y(Alignment::Center),
            text(diagnostic.category.label())
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
      text("No diagnostic events match these filters.")
        .size(12)
        .color(palette.text.metadata),
    );
  }
  section(
    palette,
    Icon::Activity,
    "Diagnostics",
    column![
      row![
        level_filter,
        category_filter,
        space::horizontal().width(Fill),
        export_button,
      ]
      .spacing(TOKENS.spacing.s2),
      text(format!("Showing {count} of at most 200 retained events."))
        .size(11)
        .color(palette.text.metadata),
      events,
    ]
    .spacing(TOKENS.spacing.s3),
  )
}

fn about_section<'a>(palette: &ThemePalette) -> Element<'a, Message> {
  section(
    palette,
    Icon::Info,
    "About",
    column![
      text("JellyPilot").size(15).color(palette.text.heading),
      text(format!("Version {}", env!("CARGO_PKG_VERSION")))
        .size(12)
        .color(palette.text.muted),
    ]
    .spacing(TOKENS.spacing.s1),
  )
}

fn section<'a>(
  palette: &ThemePalette,
  icon: Icon,
  title: &'a str,
  content: Column<'a, Message>,
) -> Element<'a, Message> {
  container(
    column![
      row![
        icon_with_color(icon, IconSize::Md, palette.colors.primary),
        text(title)
          .font(SPACE_GROTESK_FONT)
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
  label: &'a str,
  help: &'a str,
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
        Some("Save".to_owned()),
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
  label: &'a str,
  help: &'a str,
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
      Some(if enabled { "On" } else { "Off" }.to_owned()),
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
  label: &'a str,
  enabled: bool,
  message: SettingsMessage,
) -> Element<'a, Message> {
  let trigger = control_button(Some(icon), None, ButtonVariant::Tonal)
    .icon_size(IconSize::Xs)
    .padding([5, 8])
    .on_press_maybe(enabled.then_some(Message::Settings(message)));
  tooltip(trigger, label, TooltipOptions::default())
}

fn intro_option(
  label: &'static str,
  value: IntroMode,
  selected: IntroMode,
) -> Element<'static, Message> {
  let variant = if value == selected {
    ButtonVariant::Secondary
  } else {
    ButtonVariant::Text
  };
  control_button(None, Some(label.to_owned()), variant)
    .padding([6, 10])
    .width(Fill)
    .label_fill(true)
    .on_press(Message::Settings(SettingsMessage::IntroModeSelected(value)))
    .into()
}

fn diagnostic_level_option(
  label: &'static str,
  level: Option<DiagnosticLevel>,
) -> Element<'static, Message> {
  control_button(None, Some(label.to_owned()), ButtonVariant::Text)
    .padding([6, 10])
    .width(Fill)
    .label_fill(true)
    .on_press(Message::Settings(SettingsMessage::DiagnosticLevelSelected(
      level,
    )))
    .into()
}

fn diagnostic_category_option(
  label: &'static str,
  category: Option<DiagnosticCategory>,
) -> Element<'static, Message> {
  control_button(None, Some(label.to_owned()), ButtonVariant::Text)
    .padding([6, 10])
    .width(Fill)
    .label_fill(true)
    .on_press(Message::Settings(
      SettingsMessage::DiagnosticCategorySelected(category),
    ))
    .into()
}

const fn intro_mode_label(mode: IntroMode) -> &'static str {
  match mode {
    IntroMode::Automatic => "Automatic",
    IntroMode::Manual => "Manual",
    IntroMode::Off => "Off",
  }
}

fn subtitle_language_label(code: &str) -> &str {
  match code {
    "eng" => "English",
    "spa" => "Spanish",
    "fra" | "fre" => "French",
    "deu" | "ger" => "German",
    "ita" => "Italian",
    "por" => "Portuguese",
    "rus" => "Russian",
    "zho" | "chi" => "Chinese",
    "jpn" => "Japanese",
    "kor" => "Korean",
    "ara" => "Arabic",
    "hin" => "Hindi",
    _ => code,
  }
}

fn badge<'a, Message: 'a>(label: &'a str, variant: BadgeVariant) -> Element<'a, Message> {
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn diagnostic_badge_maps_levels() {
    assert_eq!(
      diagnostic_badge(DiagnosticLevel::Info),
      BadgeVariant::Neutral
    );
    assert_eq!(
      diagnostic_badge(DiagnosticLevel::Warning),
      BadgeVariant::Warning
    );
    assert_eq!(
      diagnostic_badge(DiagnosticLevel::Error),
      BadgeVariant::Neutral
    );
  }

  #[test]
  fn subtitle_language_options_all_have_human_readable_labels() {
    for code in SUBTITLE_LANGUAGE_OPTIONS {
      let label = subtitle_language_label(code);
      assert_ne!(
        label, code,
        "Configured subtitle language option {code:?} must have a human-readable label"
      );
    }
  }
}
