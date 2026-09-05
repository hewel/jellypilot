use super::{account, browse, detail, home, personal_lists, player, settings};
use crate::app::message::{BrowseMessage, HomeMessage, Message, SettingsMessage, ShellMessage};
use crate::app::personal_lists::Route;
use crate::app::shell::{SEARCH_INPUT_ID, SEARCH_TRIGGER_ID, SETTINGS_TRIGGER_ID};
use crate::app::state::{Destination, NoticeLevel, State, ToastNotice};
use iced::widget::{
  button, column, container, row, scrollable, space, stack, text, text_input, Column, Id,
};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length};
use jellypilot_core::config::AppMode;
use jellypilot_core::LoadState;
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::layout::SizeClass;
use jellypilot_ui::overlay::{popover, tooltip, PopoverOptions, TooltipOptions};
use jellypilot_ui::tokens::{ThemePalette, TOKENS};
use jellypilot_ui::variants::{ButtonVariant, FieldVariant, SurfaceVariant};
use jellypilot_ui::widgets::control_button::control_button;
use jellypilot_ui::widgets::escape_input::clear_on_escape;
use jellypilot_ui::widgets::inert::inert;
use jellypilot_ui::widgets::skeleton::skeleton_block;
pub(crate) const SIDEBAR_WIDTH: f32 = 240.0;
pub(crate) const SIDEBAR_RAIL_WIDTH: f32 = 72.0;
/// Width of the two shell hairlines (sidebar edge and player-bar edge).
pub(crate) const HAIRLINE_WIDTH: f32 = 1.0;

fn platform_search_hint() -> &'static str {
  if cfg!(target_os = "macos") {
    "⌘K"
  } else {
    "Ctrl K"
  }
}

/// Returns the sidebar width corresponding to the given window-width [`SizeClass`].
///
/// Compact windows collapse the sidebar to a 72px icon rail to maximize screen
/// real estate for media content, while Standard and Wide windows use the full 240px panel.
pub(crate) fn sidebar_width(class: SizeClass) -> f32 {
  match class {
    SizeClass::Compact => SIDEBAR_RAIL_WIDTH,
    SizeClass::Standard | SizeClass::Wide => SIDEBAR_WIDTH,
  }
}

pub fn view(state: &State) -> Element<'_, Message> {
  if state.app_mode() == AppMode::ControlOnly {
    return control_only_view(state);
  }
  let palette = state.palette();
  let skeleton_phase = state.shell.skeleton_phase;
  let reduced_motion = state.kernel.settings.snapshot().reduced_motion();
  let class = SizeClass::from_width(state.shell.window_size.width);
  let sidebar = sidebar(state, class, skeleton_phase, reduced_motion)
    .width(Length::Fixed(sidebar_width(class)));
  let content: Element<'_, Message> = match &state.shell.destination {
    Destination::Home => home::view(state),
    Destination::Library { .. } | Destination::Search(_) => browse::view(state),
    Destination::PersonalLists(_) => personal_lists::view(state),
    Destination::Detail(_) => detail::view(state),
    // Now Playing is the Control-Only root; the router never routes here in
    // Full mode, where the player is a bar.
    Destination::NowPlaying => home::view(state),
  };
  let mut content_stack = stack![content].width(Fill).height(Fill);
  if let Some(toast) = visible_toast(state) {
    content_stack = content_stack.push(
      container(toast_view(palette, toast))
        .width(Fill)
        .padding(iced::Padding {
          top: TOKENS.spacing.s2,
          right: TOKENS.spacing.s3,
          bottom: 0.0,
          left: TOKENS.spacing.s3,
        })
        .align_x(Alignment::End),
    );
  }
  // One of the two shell hairlines: 1px between the sidebar and the content.
  let sidebar_divider = container(space::vertical())
    .width(HAIRLINE_WIDTH)
    .height(Fill)
    .style(move |_| {
      iced::widget::container::Style::default().background(palette.colors.outlineVariant)
    });
  // The sidebar docks full-height so its bottom (Settings, user) never moves
  // when the player bar appears; the bar docks under the content region only.
  let mut right = Column::new().spacing(0.0).push(content_stack);
  if let Some(player_bar) = player::bar(state) {
    // The second shell hairline: 1px above the player bar.
    let player_divider = container(space::horizontal())
      .width(Fill)
      .height(HAIRLINE_WIDTH)
      .style(move |_| {
        iced::widget::container::Style::default().background(palette.colors.outlineVariant)
      });
    right = right.push(player_divider).push(player_bar);
  }
  let body = row![sidebar, sidebar_divider, right]
    .spacing(0.0)
    .width(Fill)
    .height(Fill);

  let base_view = container(body)
    .width(Fill)
    .height(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas));

  if state.shell.settings_open {
    // Keep the shell visible below Settings while removing it from input,
    // overlays, and focus traversal.
    let modal_stack = stack![inert(base_view), settings_modal(state)]
      .width(Fill)
      .height(Fill);
    container(modal_stack)
      .width(Fill)
      .height(Fill)
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
      .into()
  } else {
    base_view.into()
  }
}

/// Control-Only shell: no sidebar, no hairlines, no player bar — the compact
/// full-window Now Playing view, or full-window Settings, with the toast
/// layer on top.
fn control_only_view(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let content: Element<'_, Message> = if state.shell.settings_open {
    settings_modal(state)
  } else {
    player::full(state)
  };
  let mut content_stack = stack![content].width(Fill).height(Fill);
  if let Some(toast) = visible_toast(state) {
    content_stack = content_stack.push(
      container(toast_view(palette, toast))
        .width(Fill)
        .padding(iced::Padding {
          top: TOKENS.spacing.s2,
          right: TOKENS.spacing.s3,
          bottom: 0.0,
          left: TOKENS.spacing.s3,
        })
        .align_x(Alignment::End),
    );
  }
  container(content_stack)
    .width(Fill)
    .height(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
    .into()
}
fn visible_toast(state: &State) -> Option<&ToastNotice> {
  state.kernel.active_toast.as_ref()
}

#[allow(dead_code)]
pub fn visible_notice(state: &State) -> Option<&str> {
  state
    .kernel
    .active_toast
    .as_ref()
    .map(|toast| toast.message.as_str())
}

fn toast_view<'a>(palette: &'static ThemePalette, toast: &'a ToastNotice) -> Element<'a, Message> {
  let colors = palette.colors;
  let (icon, icon_color, text_color, bg_color) = match toast.level {
    NoticeLevel::Error => (
      Icon::Warning,
      colors.error,
      colors.onErrorContainer,
      colors.errorContainer,
    ),
    NoticeLevel::Warning => (
      Icon::Warning,
      colors.warning,
      colors.onWarningContainer,
      colors.warningContainer,
    ),
  };

  let close_id = toast.id;
  let dismiss_button = button(icon_with_color(Icon::Close, IconSize::Xs, text_color))
    .padding([3, 5])
    .on_press(Message::DismissNotice(close_id))
    .style(|_theme, status| {
      let bg = match status {
        button::Status::Hovered => Some(iced::Background::Color(Color::from_rgba(
          1.0, 1.0, 1.0, 0.1,
        ))),
        button::Status::Pressed => Some(iced::Background::Color(Color::from_rgba(
          1.0, 1.0, 1.0, 0.18,
        ))),
        _ => None,
      };
      button::Style {
        background: bg,
        text_color: Color::TRANSPARENT,
        border: iced::Border {
          radius: TOKENS.radii.sm.into(),
          ..iced::Border::default()
        },
        ..button::Style::default()
      }
    });

  let toast_content = row![
    icon_with_color(icon, IconSize::Sm, icon_color),
    text(&toast.message).size(13).color(text_color).width(Fill),
    dismiss_button,
  ]
  .spacing(TOKENS.spacing.s2)
  .align_y(Alignment::Center);

  container(toast_content)
    .max_width(440.0)
    .padding([10, 14])
    .style(move |_theme| container::Style {
      background: Some(iced::Background::Color(bg_color)),
      text_color: Some(text_color),
      border: iced::Border {
        color: Color::TRANSPARENT,
        width: 0.0,
        radius: TOKENS.radii.lg.into(),
      },
      shadow: palette.shadows.raised_high.iced(),
      ..container::Style::default()
    })
    .into()
}

fn sidebar(
  state: &State,
  class: SizeClass,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> container::Container<'_, Message> {
  match class {
    SizeClass::Compact => sidebar_compact(state),
    SizeClass::Standard | SizeClass::Wide => sidebar_full(state, skeleton_phase, reduced_motion),
  }
}

fn sidebar_full(
  state: &State,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> container::Container<'_, Message> {
  let search_draft = &state
    .full
    .as_ref()
    .expect("FullUi required")
    .browse
    .search_input;
  let search_slot = unified_search_field(search_draft, Some(SEARCH_TRIGGER_ID));
  let personal_destination = Destination::PersonalLists(Route::Overview);
  let personal_active = matches!(state.shell.destination, Destination::PersonalLists(_));
  let personal_navigation = Column::new()
    .spacing(TOKENS.spacing.s1_5)
    .push(destination_button(
      Icon::Home,
      "Home",
      Destination::Home,
      state.shell.destination == Destination::Home,
    ))
    .push(destination_button(
      Icon::Heart,
      "Personal Lists",
      personal_destination,
      personal_active,
    ));

  let libraries = match &state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .data
    .shortcuts
  {
    LoadState::Idle | LoadState::Loading => Column::new()
      .spacing(TOKENS.spacing.s1_5)
      .push(shortcut_skeleton(skeleton_phase, reduced_motion))
      .push(shortcut_skeleton(skeleton_phase, reduced_motion)),
    LoadState::Ready(shortcuts) => {
      let mut libraries = Column::new().spacing(TOKENS.spacing.s1_5);
      for shortcut in shortcuts {
        let destination = Destination::Library {
          library_id: shortcut.id.clone(),
          collection_type: shortcut.collection_type.clone(),
        };
        let active = state.shell.destination == destination;
        libraries = libraries.push(destination_button(
          Icon::for_collection_type(&shortcut.collection_type),
          &shortcut.name,
          destination,
          active,
        ));
      }
      libraries
    }
    LoadState::Failed(_) => Column::new().push(
      text("Libraries unavailable")
        .size(12)
        .color(state.palette().colors.warning),
    ),
  };

  let library_count = match &state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .data
    .shortcuts
  {
    LoadState::Ready(shortcuts) => format!("Libraries · {}", shortcuts.len()),
    _ => "Libraries".to_owned(),
  };
  let main = column![
    search_slot,
    personal_navigation,
    text(library_count)
      .size(12)
      .color(state.palette().text.metadata),
  ]
  .spacing(TOKENS.spacing.s4)
  .width(Fill);
  let libraries = scrollable(libraries)
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable);
  let bottom = column![
    account::sidebar_popover(state, false),
    footer_toolbar(state),
  ]
  .spacing(TOKENS.spacing.s3);
  let content = column![main, libraries, bottom]
    .spacing(TOKENS.spacing.s4)
    .width(Fill)
    .height(Fill);

  container(content)
    .padding(TOKENS.spacing.s4)
    .height(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Block))
}

/// One shared, raised field keeps the full sidebar and compact-search popover
/// visually and behaviorally identical. The compact rail owns the trigger ID;
/// the expanded field owns it directly.
fn unified_search_field<'a>(
  search_draft: &'a str,
  trigger_id: Option<&'static str>,
) -> Element<'a, Message> {
  let leading = control_button(Some(Icon::Search), None, ButtonVariant::Text)
    .icon_size(IconSize::Sm)
    .min_height(36.0)
    .padding([7, 8])
    .on_press(Message::Browse(BrowseMessage::SearchSubmitted));
  let leading = match trigger_id {
    Some(id) => leading.id(id),
    None => leading,
  };
  let input = clear_on_escape(
    text_input("Search movies and shows…", search_draft)
      .on_input(|value| Message::Browse(BrowseMessage::SearchInputChanged(value)))
      .on_submit(Message::Browse(BrowseMessage::SearchSubmitted))
      .id(Id::new(SEARCH_INPUT_ID))
      .padding([8, 2])
      .size(14)
      .width(Fill)
      .style(|theme, status| {
        let mut style = jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled);
        style.background = Background::Color(
          jellypilot_ui::tokens::palette(theme)
            .colors
            .surfaceContainerHigh,
        );
        style
      }),
    Message::Shell(ShellMessage::ClearSearch),
  );
  let keycap = container(text(platform_search_hint()).size(11))
    .padding([5, 7])
    .style(|theme| {
      let colors = jellypilot_ui::tokens::palette(theme).colors;
      container::Style {
        background: Some(Background::Color(colors.control)),
        text_color: Some(colors.onControl),
        border: Border {
          radius: TOKENS.radii.md.into(),
          color: Color::TRANSPARENT,
          width: 0.0,
        },
        ..container::Style::default()
      }
    });
  let trailing: Element<'_, Message> = if search_draft.is_empty() {
    keycap.into()
  } else {
    tooltip(
      control_button(Some(Icon::Close), None, ButtonVariant::Text)
        .min_height(32.0)
        .padding([6, 7])
        .on_press(Message::Shell(ShellMessage::ClearSearch)),
      "Clear search",
      TooltipOptions::default(),
    )
  };

  container(row![leading, input, trailing].align_y(Alignment::Center))
    .padding(3)
    .width(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Raised))
    .into()
}

fn sidebar_compact(state: &State) -> container::Container<'_, Message> {
  let search_trigger = tooltip(
    control_button(Some(Icon::Search), None, ButtonVariant::Tonal)
      .id(SEARCH_TRIGGER_ID)
      .min_height(36.0)
      .padding([7, 0])
      .width(Fill)
      .content_centered(true)
      .on_press(Message::Shell(ShellMessage::ToggleCompactSearch)),
    "Search",
    TooltipOptions::default(),
  );
  let search_draft = &state
    .full
    .as_ref()
    .expect("FullUi required")
    .browse
    .search_input;
  let compact_search_content = unified_search_field(search_draft, None);
  let compact_search = popover(
    search_trigger,
    compact_search_content,
    state.shell.compact_search_open,
    PopoverOptions {
      width: Some(288.0),
      ..PopoverOptions::default()
    },
    Message::Shell(ShellMessage::DismissCompactSearch),
  );
  let personal_destination = Destination::PersonalLists(Route::Overview);
  let personal_active = matches!(state.shell.destination, Destination::PersonalLists(_));
  let personal_navigation = Column::new()
    .spacing(TOKENS.spacing.s1_5)
    .align_x(Alignment::Center)
    .width(Fill)
    .push(compact_destination_button(
      Icon::Home,
      "Home",
      Destination::Home,
      state.shell.destination == Destination::Home,
    ));
  let libraries = match &state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .data
    .shortcuts
  {
    LoadState::Idle | LoadState::Loading => Column::new()
      .spacing(TOKENS.spacing.s2)
      .align_x(Alignment::Center)
      .push(shortcut_skeleton(
        state.shell.skeleton_phase,
        state.kernel.settings.snapshot().reduced_motion(),
      ))
      .push(shortcut_skeleton(
        state.shell.skeleton_phase,
        state.kernel.settings.snapshot().reduced_motion(),
      )),
    LoadState::Ready(shortcuts) => {
      let mut libraries = Column::new()
        .spacing(TOKENS.spacing.s1_5)
        .align_x(Alignment::Center);
      for shortcut in shortcuts {
        let destination = Destination::Library {
          library_id: shortcut.id.clone(),
          collection_type: shortcut.collection_type.clone(),
        };
        let active = state.shell.destination == destination;
        libraries = libraries.push(compact_destination_button(
          Icon::for_collection_type(&shortcut.collection_type),
          &shortcut.name,
          destination,
          active,
        ));
      }
      libraries
    }
    LoadState::Failed(_) => Column::new().push(icon_with_color(
      Icon::Warning,
      IconSize::Sm,
      state.palette().colors.warning,
    )),
  };
  let personal_navigation = personal_navigation.push(compact_destination_button(
    Icon::Heart,
    "Personal Lists",
    personal_destination,
    personal_active,
  ));
  let refresh = tooltip(
    control_button(Some(Icon::Refresh), None, ButtonVariant::Tonal)
      .min_height(36.0)
      .padding([7, 0])
      .width(Fill)
      .content_centered(true)
      .on_press_maybe(
        (!state.shell.refresh_busy).then_some(Message::Shell(ShellMessage::RefreshCurrent)),
      ),
    if state.shell.refresh_busy {
      "Refreshing…"
    } else {
      "Refresh"
    },
    TooltipOptions::default(),
  );

  let bottom = column![
    account::sidebar_popover(state, true),
    compact_settings_button(),
    refresh,
    tooltip(
      control_button(Some(Icon::PictureInPicture), None, ButtonVariant::Tonal,)
        .min_height(36.0)
        .padding([7, 12])
        .width(Fill)
        .content_centered(true)
        .on_press(Message::Settings(SettingsMessage::AppModeSelected(
          AppMode::ControlOnly,
        ))),
      "Control mode",
      TooltipOptions::default(),
    ),
  ]
  .spacing(TOKENS.spacing.s3)
  .align_x(Alignment::Center)
  .width(Fill);

  let content = column![
    compact_search,
    personal_navigation,
    scrollable(libraries).height(Fill),
    bottom
  ]
  .spacing(TOKENS.spacing.s4)
  .width(Fill)
  .height(Fill);

  container(content)
    .padding(TOKENS.spacing.s4)
    .height(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Block))
}

fn settings_modal(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let close_button = control_button(Some(Icon::Close), None, ButtonVariant::Tonal)
    .padding([6, 10])
    .on_press(Message::Settings(SettingsMessage::Close));

  let header = row![
    column![
      text("Settings")
        .font(SPACE_GROTESK_FONT)
        .size(28)
        .color(palette.text.heading),
      text("Changes are written to disk when Saved appears.")
        .size(13)
        .color(palette.text.body),
    ]
    .spacing(TOKENS.spacing.s0_5),
    space::horizontal(),
    tooltip(close_button, "Close", TooltipOptions::default()),
  ]
  .width(Fill)
  .align_y(Alignment::Center);

  let modal_content = column![header, settings::view(state),]
    .spacing(TOKENS.spacing.s4)
    .padding([TOKENS.spacing.s4, TOKENS.spacing.s6])
    .width(Fill)
    .height(Fill);

  let narrow = state.app_mode() == AppMode::ControlOnly
    || SizeClass::from_width(state.shell.window_size.width) == SizeClass::Compact;
  if narrow {
    return container(modal_content)
      .width(Fill)
      .height(Fill)
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
      .into();
  }

  container(
    container(modal_content)
      .width(Length::Fixed(896.0))
      .height(Length::Fixed(
        (state.shell.window_size.height - 48.0).clamp(0.0, 620.0),
      ))
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Raised)),
  )
  .width(Fill)
  .height(Fill)
  .padding(24)
  .center_x(Fill)
  .center_y(Fill)
  .into()
}

fn settings_button<'a>() -> Element<'a, Message> {
  control_button(Some(Icon::Settings), None, ButtonVariant::Text)
    .id(SETTINGS_TRIGGER_ID)
    .min_height(40.0)
    .width(Fill)
    .content_centered(true)
    .on_press(Message::Settings(SettingsMessage::Open))
    .into()
}

fn compact_settings_button<'a>() -> Element<'a, Message> {
  // Action, not navigation — neutral Tonal, never the ghost vocabulary.
  let btn = control_button(Some(Icon::Settings), None, ButtonVariant::Tonal)
    .id(SETTINGS_TRIGGER_ID)
    .min_height(36.0)
    .padding([7, 0])
    .width(Fill)
    .content_centered(true)
    .on_press(Message::Settings(SettingsMessage::Open));

  tooltip(btn, "Settings", TooltipOptions::default())
}

/// The full sidebar groups its three global actions into one compact control
/// strip so the account trigger remains the clear visual anchor at the bottom.
fn footer_toolbar(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let divider = || {
    container(space::horizontal())
      .width(1.0)
      .height(24.0)
      .style(move |_| container::Style::default().background(palette.colors.outlineVariant))
  };
  let settings = tooltip(settings_button(), "Settings", TooltipOptions::default());
  let refresh = tooltip(
    control_button(Some(Icon::Refresh), None, ButtonVariant::Text)
      .min_height(40.0)
      .width(Fill)
      .content_centered(true)
      .on_press_maybe(
        (!state.shell.refresh_busy).then_some(Message::Shell(ShellMessage::RefreshCurrent)),
      ),
    if state.shell.refresh_busy {
      "Refreshing…"
    } else {
      "Refresh"
    },
    TooltipOptions::default(),
  );
  let control = tooltip(
    control_button(Some(Icon::PictureInPicture), None, ButtonVariant::Text)
      .min_height(40.0)
      .width(Fill)
      .content_centered(true)
      .on_press(Message::Settings(SettingsMessage::AppModeSelected(
        AppMode::ControlOnly,
      ))),
    "Control mode",
    TooltipOptions::default(),
  );

  container(row![settings, divider(), refresh, divider(), control].align_y(Alignment::Center))
    .padding(3)
    .width(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Raised))
    .into()
}

fn destination_button<'a>(
  icon: Icon,
  label: &'a str,
  destination: Destination,
  active: bool,
) -> Element<'a, Message> {
  let min_height = if matches!(&destination, Destination::Library { .. }) {
    32.0
  } else {
    38.0
  };
  let variant = if active {
    ButtonVariant::Secondary
  } else {
    ButtonVariant::Text
  };
  control_button(Some(icon), Some(label.to_owned()), variant)
    .min_height(min_height)
    .label_size(14.0)
    .spacing(TOKENS.spacing.s2_5)
    .padding([7, 12])
    .width(Fill)
    .label_fill(true)
    .on_press(Message::Home(HomeMessage::Navigate(destination)))
    .into()
}
fn shortcut_skeleton<'a>(skeleton_phase: f32, reduced_motion: bool) -> Element<'a, Message> {
  skeleton_block(Length::Fill, 34.0, skeleton_phase, reduced_motion).into()
}

fn compact_destination_button<'a>(
  icon: Icon,
  label: &'a str,
  destination: Destination,
  active: bool,
) -> Element<'a, Message> {
  let variant = if active {
    ButtonVariant::Secondary
  } else {
    ButtonVariant::Text
  };
  let btn = control_button(Some(icon), None, variant)
    .min_height(if matches!(&destination, Destination::Library { .. }) {
      32.0
    } else {
      38.0
    })
    .padding([7, 0])
    .width(Fill)
    .content_centered(true)
    .on_press(Message::Home(HomeMessage::Navigate(destination)));

  tooltip(btn, label, TooltipOptions::default())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn active_toast_is_rendered_and_cleared_on_dismiss() {
    let mut state = State::boot(false);
    assert_eq!(visible_notice(&state), None);
    assert_eq!(visible_toast(&state), None);

    state.kernel.active_toast = Some(ToastNotice {
      id: 1,
      message: "Playback failed.".to_owned(),
      level: NoticeLevel::Error,
    });
    assert_eq!(visible_notice(&state), Some("Playback failed."));
    assert_eq!(
      visible_toast(&state),
      Some(&ToastNotice {
        id: 1,
        message: "Playback failed.".to_owned(),
        level: NoticeLevel::Error,
      })
    );

    state.dismiss_toast(1);
    assert_eq!(visible_notice(&state), None);
    assert_eq!(visible_toast(&state), None);
  }

  #[test]
  fn newer_toast_replaces_older_and_older_id_does_not_dismiss_newer() {
    let mut state = State::boot(false);
    state.kernel.active_toast = Some(ToastNotice {
      id: 1,
      message: "First notice".to_owned(),
      level: NoticeLevel::Warning,
    });
    state.kernel.active_toast = Some(ToastNotice {
      id: 2,
      message: "Second notice".to_owned(),
      level: NoticeLevel::Error,
    });

    assert_eq!(visible_notice(&state), Some("Second notice"));

    state.dismiss_toast(1);
    assert_eq!(visible_notice(&state), Some("Second notice"));

    state.dismiss_toast(2);
    assert_eq!(visible_notice(&state), None);
  }
  #[test]
  fn sidebar_width_maps_size_classes_to_expected_widths() {
    assert_eq!(sidebar_width(SizeClass::Compact), 72.0);
    assert_eq!(sidebar_width(SizeClass::Standard), 240.0);
    assert_eq!(sidebar_width(SizeClass::Wide), 240.0);
  }

  #[test]
  fn shell_view_renders_in_loading_state() {
    let mut state = State::boot(false);
    state.shell.skeleton_phase = 0.42;
    state.full.as_mut().unwrap().home.data.shortcuts = LoadState::Loading;
    let _element = view(&state);
  }

  #[test]
  fn shell_view_renders_settings_modal_in_full_mode() {
    let mut state = State::boot(false);
    state.shell.settings_open = true;
    let _element = view(&state);
  }

  #[test]
  fn shell_view_renders_settings_modal_in_control_only_mode() {
    let mut state = State::boot(false);
    // This test persists a settings mutation; run it against a scratch file
    // instead of the developer's real config (dirs::config_dir).
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-settings-shell-view-{}.json",
      std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    state.kernel.settings = jellypilot_core::config::SettingsStore::for_test(path);
    state
      .kernel
      .settings
      .set_app_mode(AppMode::ControlOnly)
      .unwrap();
    state.shell.settings_open = true;
    let _element = view(&state);
  }
}
