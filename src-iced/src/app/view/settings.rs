use iced::widget::{button, column, container, row, scrollable, space, text, text_input, Column};
use iced::{Alignment, Element, Fill};
use jellypilot_auth::login::ConnectionPhase;
use jellypilot_core::config::{IntroMode, ShortcutKind};
use jellypilot_core::diagnostics::{format_diagnostic_time, DiagnosticCategory, DiagnosticLevel};
use jellypilot_core::settings::SUBTITLE_LANGUAGE_OPTIONS;
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::overlay::{popover, PopoverOptions};
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::{BadgeVariant, ButtonVariant, FieldVariant, SurfaceVariant};

use crate::app::message::{Message, SettingsMessage};
use crate::app::state::{diagnostic_matches, State};

pub fn view(state: &State) -> Element<'_, Message> {
  let header = column![
    text("Settings")
      .font(SPACE_GROTESK_FONT)
      .size(34)
      .color(TOKENS.colors.onSurface),
    text("Changes are written to disk when Saved appears.")
      .size(13)
      .color(TOKENS.colors.onSurfaceVariant),
  ]
  .spacing(TOKENS.spacing.s1);

  let content = column![
    header,
    feedback(state),
    connection_section(state),
    mpv_section(state),
    playback_section(state),
    subtitles_section(state),
    shortcuts_section(state),
    cache_section(state),
    diagnostics_section(state),
    about_section(),
  ]
  .spacing(TOKENS.spacing.s4)
  .width(Fill);

  scrollable(container(content).padding([TOKENS.spacing.s5, TOKENS.spacing.s8]))
    .width(Fill)
    .height(Fill)
    .into()
}

fn feedback(state: &State) -> Element<'_, Message> {
  if let Some(error) = state.settings_view.error {
    return text(error).size(13).color(TOKENS.colors.error).into();
  }
  if let Some(saved) = state.settings_view.saved {
    return badge(saved, BadgeVariant::Success);
  }
  space::vertical().height(0).into()
}

fn connection_section(state: &State) -> Element<'_, Message> {
  let status = match state.connection {
    ConnectionPhase::Connected => badge("Connected", BadgeVariant::Success),
    ConnectionPhase::Connecting => badge("Working", BadgeVariant::Warning),
    ConnectionPhase::SignedOut | ConnectionPhase::Failed => {
      badge("Disconnected", BadgeVariant::Neutral)
    }
  };
  let identity: Element<'_, Message> = state.connected_identity.as_ref().map_or_else(
    || {
      text("No active connection")
        .size(13)
        .color(TOKENS.colors.onSurfaceVariant)
        .into()
    },
    |identity| {
      column![
        text(&identity.user_name)
          .size(15)
          .color(TOKENS.colors.onSurface),
        text(&identity.server)
          .size(12)
          .color(TOKENS.colors.onSurfaceVariant),
      ]
      .spacing(TOKENS.spacing.s1)
      .into()
    },
  );
  let connected = state.connection == ConnectionPhase::Connected;
  let disconnect = action_button("Disconnect", connected, SettingsMessage::Disconnect);
  let sign_out = action_button("Sign Out", connected, SettingsMessage::SignOut);
  section(
    "Connection",
    column![
      row![identity, space::horizontal(), status]
        .align_y(Alignment::Center)
        .width(Fill),
      text("Disconnect keeps saved profiles. Sign Out securely removes the active saved profile.")
        .size(12)
        .color(TOKENS.colors.onSurfaceVariant),
      row![disconnect, sign_out].spacing(TOKENS.spacing.s2),
    ]
    .spacing(TOKENS.spacing.s3),
  )
}

fn mpv_section(state: &State) -> Element<'_, Message> {
  let path = text_input("Auto-detect from PATH", &state.settings_view.mpv_path_input)
    .on_input(|value| Message::Settings(SettingsMessage::MpvPathChanged(value)))
    .on_submit(Message::Settings(SettingsMessage::SaveMpvPath))
    .padding([10, 12])
    .width(Fill)
    .style(|theme, status| {
      jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
    });
  let args = text_input(
    "Additional MPV arguments",
    &state.settings_view.mpv_args_input,
  )
  .on_input(|value| Message::Settings(SettingsMessage::MpvArgsChanged(value)))
  .on_submit(Message::Settings(SettingsMessage::SaveMpvArgs))
  .padding([10, 12])
  .width(Fill)
  .style(|theme, status| jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled));
  section(
    "MPV",
    column![
      labeled_field(
        "Executable path",
        "Leave empty to discover MPV from PATH. Applies to future MPV process starts.",
        path.into(),
        SettingsMessage::SaveMpvPath,
      ),
      labeled_field(
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
  let target = text_input(
    "JellyPilot",
    &state.settings_view.playback_target_name_input,
  )
  .on_input(|value| Message::Settings(SettingsMessage::PlaybackTargetNameChanged(value)))
  .on_submit(Message::Settings(SettingsMessage::SavePlaybackTargetName))
  .padding([10, 12])
  .width(Fill)
  .style(|theme, status| jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled));
  let mode = state.settings.snapshot().intro_mode();
  let trigger = button(text(format!("Intro Skipper: {}", intro_mode_label(mode))))
    .padding([9, 13])
    .on_press(Message::Settings(SettingsMessage::IntroMenuToggled))
    .style(|theme, status| {
      jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
    });
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
    state.settings_view.intro_menu_open,
    PopoverOptions {
      width: Some(220.0),
      ..PopoverOptions::default()
    },
    Message::Settings(SettingsMessage::IntroMenuDismissed),
  );
  section(
    "Playback",
    column![
      labeled_field(
        "Playback Target name",
        "Saved names are re-registered with the connected server immediately.",
        target.into(),
        SettingsMessage::SavePlaybackTargetName,
      ),
      column![
        text("Intro Skipper")
          .size(14)
          .color(TOKENS.colors.onSurface),
        text("Automatic skips detected intros; Manual shows the skip action; Off disables it.")
          .size(12)
          .color(TOKENS.colors.onSurfaceVariant),
        intro,
      ]
      .spacing(TOKENS.spacing.s2),
    ]
    .spacing(TOKENS.spacing.s4),
  )
}

fn subtitles_section(state: &State) -> Element<'_, Message> {
  let trigger = button(text("Add language"))
    .padding([9, 13])
    .on_press(Message::Settings(SettingsMessage::SubtitleMenuToggled))
    .style(|theme, status| {
      jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
    });
  let mut menu = Column::new().spacing(TOKENS.spacing.s1).width(Fill);
  for language in SUBTITLE_LANGUAGE_OPTIONS {
    menu = menu.push(
      button(text(subtitle_language_label(language)).width(Fill))
        .padding([8, 10])
        .width(Fill)
        .on_press(Message::Settings(SettingsMessage::SubtitleLanguageAdded(
          language.to_owned(),
        )))
        .style(|theme, status| {
          jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Text)
        }),
    );
  }
  let add = popover(
    trigger,
    menu,
    state.settings_view.subtitle_menu_open,
    PopoverOptions {
      width: Some(220.0),
      ..PopoverOptions::default()
    },
    Message::Settings(SettingsMessage::SubtitleMenuDismissed),
  );
  let languages = state.settings.snapshot().subtitle_languages();
  let mut rows = Column::new().spacing(TOKENS.spacing.s2).width(Fill);
  if languages.is_empty() {
    rows = rows.push(
      text("No preferred subtitle languages.")
        .size(12)
        .color(TOKENS.colors.onSurfaceVariant),
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
          "↑",
          index > 0,
          SettingsMessage::SubtitleLanguageMoved { index, offset: -1 }
        ),
        compact_button(
          "↓",
          index + 1 < languages.len(),
          SettingsMessage::SubtitleLanguageMoved { index, offset: 1 },
        ),
        compact_button(
          "Remove",
          true,
          SettingsMessage::SubtitleLanguageRemoved(index)
        ),
      ]
      .spacing(TOKENS.spacing.s2)
      .align_y(Alignment::Center),
    );
  }
  section(
    "Subtitles",
    column![
      text("Languages are tried from top to bottom on future playback starts.")
        .size(12)
        .color(TOKENS.colors.onSurfaceVariant),
      rows,
      add,
    ]
    .spacing(TOKENS.spacing.s3),
  )
}

fn shortcuts_section(state: &State) -> Element<'_, Message> {
  section(
    "Shortcuts",
    column![
      shortcut_row(state, "Next episode", ShortcutKind::Next),
      shortcut_row(state, "Previous episode", ShortcutKind::Previous),
      shortcut_row(state, "Skip intro", ShortcutKind::IntroSkip),
      text("Shortcut subscriptions read the current persisted bindings.")
        .size(12)
        .color(TOKENS.colors.onSurfaceVariant),
    ]
    .spacing(TOKENS.spacing.s2),
  )
}

fn shortcut_row<'a>(state: &'a State, label: &'a str, kind: ShortcutKind) -> Element<'a, Message> {
  let binding = match kind {
    ShortcutKind::Next => state.settings.snapshot().key_next_episode(),
    ShortcutKind::Previous => state.settings.snapshot().key_previous_episode(),
    ShortcutKind::IntroSkip => state.settings.snapshot().key_intro_skip(),
  };
  let capturing = state.settings_view.shortcut_capture == Some(kind);
  row![
    text(label).size(13).width(Fill),
    button(text(if capturing { "Press a key…" } else { binding }))
      .padding([8, 11])
      .on_press(Message::Settings(SettingsMessage::BeginShortcutCapture(
        kind
      )))
      .style(move |theme, status| {
        jellypilot_ui::theme::button_variant(
          theme,
          status,
          if capturing {
            ButtonVariant::Secondary
          } else {
            ButtonVariant::Outlined
          },
        )
      }),
  ]
  .align_y(Alignment::Center)
  .spacing(TOKENS.spacing.s3)
  .into()
}

fn cache_section(state: &State) -> Element<'_, Message> {
  let cache_enabled = state.settings.snapshot().image_cache_enabled();
  let start_minimized = state.settings.snapshot().start_minimized();
  section(
    "Cache",
    column![
      toggle_row(
        "Image disk cache",
        "Caches encoded artwork on disk and applies immediately.",
        cache_enabled,
        SettingsMessage::ImageCacheToggled,
      ),
      toggle_row(
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
  let level_trigger = button(text(format!(
    "Level: {}",
    state
      .settings_view
      .diagnostic_level
      .map_or("All", DiagnosticLevel::label)
  )))
  .padding([8, 11])
  .on_press(Message::Settings(
    SettingsMessage::DiagnosticLevelMenuToggled,
  ))
  .style(|theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
  });
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
    state.settings_view.diagnostic_level_menu_open,
    PopoverOptions {
      width: Some(180.0),
      ..PopoverOptions::default()
    },
    Message::Settings(SettingsMessage::DiagnosticLevelMenuDismissed),
  );
  let category_trigger = button(text(format!(
    "Category: {}",
    state
      .settings_view
      .diagnostic_category
      .map_or("All", DiagnosticCategory::label)
  )))
  .padding([8, 11])
  .on_press(Message::Settings(
    SettingsMessage::DiagnosticCategoryMenuToggled,
  ))
  .style(|theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
  });
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
    state.settings_view.diagnostic_category_menu_open,
    PopoverOptions {
      width: Some(210.0),
      ..PopoverOptions::default()
    },
    Message::Settings(SettingsMessage::DiagnosticCategoryMenuDismissed),
  );

  let mut events = Column::new().spacing(TOKENS.spacing.s2).width(Fill);
  let mut count = 0_usize;
  for diagnostic in state.diagnostics.rows().filter(|diagnostic| {
    diagnostic_matches(
      state.settings_view.diagnostic_level,
      state.settings_view.diagnostic_category,
      diagnostic.level,
      diagnostic.category,
    )
  }) {
    count = count.saturating_add(1);
    events = events.push(
      container(
        column![
          row![
            badge(diagnostic.level.label(), diagnostic_badge(diagnostic.level)),
            text(diagnostic.category.label())
              .size(12)
              .color(TOKENS.colors.onSurfaceVariant),
            space::horizontal(),
            text(format_diagnostic_time(diagnostic.timestamp_seconds))
              .size(11)
              .color(TOKENS.colors.onSurfaceVariant),
          ]
          .spacing(TOKENS.spacing.s2)
          .align_y(Alignment::Center),
          text(diagnostic.message)
            .size(12)
            .color(TOKENS.colors.onSurface),
        ]
        .spacing(TOKENS.spacing.s2),
      )
      .padding(TOKENS.spacing.s3)
      .width(Fill)
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Filled)),
    );
  }
  if count == 0 {
    events = events.push(
      text("No diagnostic events match these filters.")
        .size(12)
        .color(TOKENS.colors.onSurfaceVariant),
    );
  }
  section(
    "Diagnostics",
    column![
      row![level_filter, category_filter].spacing(TOKENS.spacing.s2),
      text(format!("Showing {count} of at most 200 retained events."))
        .size(11)
        .color(TOKENS.colors.onSurfaceVariant),
      events,
    ]
    .spacing(TOKENS.spacing.s3),
  )
}

fn about_section<'a>() -> Element<'a, Message> {
  section(
    "About",
    column![
      text("JellyPilot").size(15).color(TOKENS.colors.onSurface),
      text(format!("Version {}", env!("CARGO_PKG_VERSION")))
        .size(12)
        .color(TOKENS.colors.onSurfaceVariant),
    ]
    .spacing(TOKENS.spacing.s1),
  )
}

fn section<'a>(title: &'a str, content: Column<'a, Message>) -> Element<'a, Message> {
  container(
    column![
      text(title)
        .font(SPACE_GROTESK_FONT)
        .size(22)
        .color(TOKENS.colors.onSurface),
      content,
    ]
    .spacing(TOKENS.spacing.s3),
  )
  .padding(TOKENS.spacing.s5)
  .width(Fill)
  .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Elevated))
  .into()
}

fn labeled_field<'a>(
  label: &'a str,
  help: &'a str,
  field: Element<'a, Message>,
  save: SettingsMessage,
) -> Element<'a, Message> {
  column![
    text(label).size(14).color(TOKENS.colors.onSurface),
    text(help).size(12).color(TOKENS.colors.onSurfaceVariant),
    row![
      field,
      button(text("Save"))
        .padding([10, 14])
        .on_press(Message::Settings(save))
        .style(|theme, status| {
          jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Primary)
        }),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center),
  ]
  .spacing(TOKENS.spacing.s2)
  .into()
}

fn toggle_row<'a>(
  label: &'a str,
  help: &'a str,
  enabled: bool,
  message: SettingsMessage,
) -> Element<'a, Message> {
  row![
    column![
      text(label).size(14).color(TOKENS.colors.onSurface),
      text(help).size(12).color(TOKENS.colors.onSurfaceVariant),
    ]
    .spacing(TOKENS.spacing.s1)
    .width(Fill),
    button(text(if enabled { "On" } else { "Off" }))
      .padding([8, 12])
      .on_press(Message::Settings(message))
      .style(move |theme, status| {
        jellypilot_ui::theme::button_variant(
          theme,
          status,
          if enabled {
            ButtonVariant::Secondary
          } else {
            ButtonVariant::Outlined
          },
        )
      }),
  ]
  .spacing(TOKENS.spacing.s3)
  .align_y(Alignment::Center)
  .into()
}

fn action_button<'a>(
  label: &'a str,
  enabled: bool,
  message: SettingsMessage,
) -> Element<'a, Message> {
  let button = button(text(label)).padding([9, 13]).style(|theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
  });
  if enabled {
    button.on_press(Message::Settings(message)).into()
  } else {
    button.into()
  }
}

fn compact_button<'a>(
  label: &'a str,
  enabled: bool,
  message: SettingsMessage,
) -> Element<'a, Message> {
  let button = button(text(label).size(12))
    .padding([6, 9])
    .style(|theme, status| {
      jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Text)
    });
  if enabled {
    button.on_press(Message::Settings(message)).into()
  } else {
    button.into()
  }
}

fn intro_option(
  label: &'static str,
  value: IntroMode,
  selected: IntroMode,
) -> Element<'static, Message> {
  button(text(label).width(Fill))
    .padding([8, 10])
    .width(Fill)
    .on_press(Message::Settings(SettingsMessage::IntroModeSelected(value)))
    .style(move |theme, status| {
      jellypilot_ui::theme::button_variant(
        theme,
        status,
        if value == selected {
          ButtonVariant::Secondary
        } else {
          ButtonVariant::Text
        },
      )
    })
    .into()
}

fn diagnostic_level_option(
  label: &'static str,
  level: Option<DiagnosticLevel>,
) -> Element<'static, Message> {
  button(text(label).width(Fill))
    .padding([8, 10])
    .width(Fill)
    .on_press(Message::Settings(SettingsMessage::DiagnosticLevelSelected(
      level,
    )))
    .style(|theme, status| jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Text))
    .into()
}

fn diagnostic_category_option(
  label: &'static str,
  category: Option<DiagnosticCategory>,
) -> Element<'static, Message> {
  button(text(label).width(Fill))
    .padding([8, 10])
    .width(Fill)
    .on_press(Message::Settings(
      SettingsMessage::DiagnosticCategorySelected(category),
    ))
    .style(|theme, status| jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Text))
    .into()
}

fn badge<'a>(label: &'a str, variant: BadgeVariant) -> Element<'a, Message> {
  container(text(label).size(11))
    .padding([4, 8])
    .style(move |theme| jellypilot_ui::widgets::badge::style(theme, variant))
    .into()
}

const fn intro_mode_label(mode: IntroMode) -> &'static str {
  match mode {
    IntroMode::Automatic => "Automatic",
    IntroMode::Manual => "Manual",
    IntroMode::Off => "Off",
  }
}

fn subtitle_language_label(code: &str) -> &'static str {
  match code {
    "eng" => "English (eng)",
    "spa" => "Spanish (spa)",
    "fra" => "French (fra)",
    "deu" => "German (deu)",
    "ita" => "Italian (ita)",
    "por" => "Portuguese (por)",
    "jpn" => "Japanese (jpn)",
    "zho" => "Chinese (zho)",
    _ => "Custom language",
  }
}

const fn diagnostic_badge(level: DiagnosticLevel) -> BadgeVariant {
  match level {
    DiagnosticLevel::Info => BadgeVariant::Neutral,
    DiagnosticLevel::Warning => BadgeVariant::Warning,
    DiagnosticLevel::Error => BadgeVariant::Warning,
  }
}
