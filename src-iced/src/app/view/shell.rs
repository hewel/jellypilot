use super::{browse, detail, home, player, settings};
use crate::app::message::{BrowseMessage, HomeMessage, Message};
use crate::app::state::{Destination, NoticeLevel, State, ToastNotice};
use iced::widget::{button, column, container, row, space, stack, text, text_input, Column};
use iced::{Alignment, Color, Element, Fill, Length};
use jellypilot_core::LoadState;
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::icons::{icon_for_variant, icon_with_color, Icon, IconSize};
use jellypilot_ui::layout::SizeClass;
use jellypilot_ui::overlay::{tooltip, TooltipOptions};
use jellypilot_ui::tokens::{ThemePalette, TOKENS};
use jellypilot_ui::variants::{ButtonVariant, FieldVariant, SurfaceVariant};
use jellypilot_ui::widgets::skeleton::skeleton_block;
pub(crate) const SIDEBAR_WIDTH: f32 = 248.0;
pub(crate) const SIDEBAR_RAIL_WIDTH: f32 = 72.0;
/// Width of the two shell hairlines (sidebar edge and player-bar edge).
pub(crate) const HAIRLINE_WIDTH: f32 = 1.0;

/// Returns the sidebar width corresponding to the given window-width [`SizeClass`].
///
/// Compact windows collapse the sidebar to a 72px icon rail to maximize screen
/// real estate for media content, while Standard and Wide windows use the full 248px panel.
pub(crate) fn sidebar_width(class: SizeClass) -> f32 {
  match class {
    SizeClass::Compact => SIDEBAR_RAIL_WIDTH,
    SizeClass::Standard | SizeClass::Wide => SIDEBAR_WIDTH,
  }
}

pub fn view(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let skeleton_phase = state.shell.skeleton_phase;
  let reduced_motion = state.kernel.settings.snapshot().reduced_motion();
  let class = SizeClass::from_width(state.shell.window_size.width);
  let sidebar = sidebar(state, class, skeleton_phase, reduced_motion)
    .width(Length::Fixed(sidebar_width(class)));
  let content: Element<'_, Message> = match &state.shell.destination {
    Destination::Home => home::view(state),
    Destination::Library { .. } | Destination::Search(_) => browse::view(state),
    Destination::Detail(_) => detail::view(state),
    Destination::Settings => settings::view(state),
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

  container(body)
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
  let title = column![
    text("JellyPilot")
      .font(SPACE_GROTESK_FONT)
      .size(26)
      .color(state.palette().colors.onSurface),
    text("Video Library")
      .size(12)
      .color(state.palette().colors.onSurfaceVariant),
  ]
  .spacing(TOKENS.spacing.s1);
  let search_input = text_input("Search videos", &state.browse.search_input)
    .on_input(|value| Message::Browse(BrowseMessage::SearchInputChanged(value)))
    .on_submit(Message::Browse(BrowseMessage::SearchSubmitted))
    .padding([8, 12])
    .size(14)
    .width(Fill)
    .style(|theme, status| {
      jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
    });
  let search_button = button(icon_for_variant(
    Icon::Search,
    IconSize::Sm,
    ButtonVariant::Tonal,
  ))
  .padding([7, 11])
  .on_press(Message::Browse(BrowseMessage::SearchSubmitted))
  .style(|theme, status| jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Tonal));
  let search_slot = row![
    search_input,
    tooltip(search_button, "Search", TooltipOptions::default()),
  ]
  .spacing(TOKENS.spacing.s1_5)
  .align_y(Alignment::Center);
  let mut destinations = Column::new()
    .spacing(TOKENS.spacing.s1_5)
    .push(destination_button(
      Icon::Home,
      "Home",
      Destination::Home,
      state.shell.destination == Destination::Home,
    ));
  match &state.home.data.shortcuts {
    LoadState::Idle | LoadState::Loading => {
      destinations = destinations
        .push(shortcut_skeleton(skeleton_phase, reduced_motion))
        .push(shortcut_skeleton(skeleton_phase, reduced_motion));
    }
    LoadState::Ready(shortcuts) => {
      for shortcut in shortcuts {
        let destination = Destination::Library {
          library_id: shortcut.id.clone(),
          collection_type: shortcut.collection_type.clone(),
        };
        let active = state.shell.destination == destination;
        destinations = destinations.push(destination_button(
          Icon::for_collection_type(&shortcut.collection_type),
          &shortcut.name,
          destination,
          active,
        ));
      }
    }
    LoadState::Failed(_) => {
      destinations = destinations.push(
        text("Libraries unavailable")
          .size(12)
          .color(state.palette().colors.warning),
      );
    }
  }

  let main = column![title, search_slot, destinations]
    .spacing(TOKENS.spacing.s5)
    .width(Fill);
  let bottom = column![
    connection_summary(state),
    destination_button(
      Icon::Settings,
      "Settings",
      Destination::Settings,
      state.shell.destination == Destination::Settings,
    ),
  ]
  .spacing(TOKENS.spacing.s3);
  let content = column![main, space::vertical(), bottom]
    .spacing(TOKENS.spacing.s4)
    .width(Fill)
    .height(Fill);

  container(content)
    .padding(TOKENS.spacing.s4)
    .height(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Block))
}

fn sidebar_compact(state: &State) -> container::Container<'_, Message> {
  let mut destinations = Column::new()
    .spacing(TOKENS.spacing.s1_5)
    .align_x(Alignment::Center)
    .width(Fill)
    .push(compact_destination_button(
      Icon::Home,
      "Home",
      Destination::Home,
      state.shell.destination == Destination::Home,
    ));
  if let LoadState::Ready(shortcuts) = &state.home.data.shortcuts {
    for shortcut in shortcuts {
      let destination = Destination::Library {
        library_id: shortcut.id.clone(),
        collection_type: shortcut.collection_type.clone(),
      };
      let active = state.shell.destination == destination;
      destinations = destinations.push(compact_destination_button(
        Icon::for_collection_type(&shortcut.collection_type),
        &shortcut.name,
        destination,
        active,
      ));
    }
  }

  let bottom = column![
    compact_connection_status(state),
    compact_destination_button(
      Icon::Settings,
      "Settings",
      Destination::Settings,
      state.shell.destination == Destination::Settings,
    ),
  ]
  .spacing(TOKENS.spacing.s3)
  .align_x(Alignment::Center)
  .width(Fill);

  let content = column![destinations, space::vertical(), bottom]
    .spacing(TOKENS.spacing.s4)
    .width(Fill)
    .height(Fill);

  container(content)
    .padding(TOKENS.spacing.s4)
    .height(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Block))
}

fn destination_button<'a>(
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
  button(
    row![
      icon_for_variant(icon, IconSize::Md, variant),
      text(label).size(14).width(Fill),
    ]
    .spacing(TOKENS.spacing.s2_5)
    .align_y(Alignment::Center),
  )
  .padding([7, 12])
  .width(Fill)
  .on_press(Message::Home(HomeMessage::Navigate(destination)))
  .style(move |theme, status| jellypilot_ui::theme::button_variant(theme, status, variant))
  .into()
}
fn shortcut_skeleton<'a>(skeleton_phase: f32, reduced_motion: bool) -> Element<'a, Message> {
  skeleton_block(Length::Fill, 34.0, skeleton_phase, reduced_motion).into()
}

fn connection_summary(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let Some(identity) = &state.kernel.connected_identity else {
    return space::vertical().into();
  };
  row![
    icon_with_color(Icon::Server, IconSize::Md, palette.colors.onSurfaceVariant),
    column![
      text(&identity.user_name)
        .size(13)
        .color(palette.colors.onSurface),
      text(&identity.server)
        .size(11)
        .color(palette.colors.onSurfaceVariant),
    ]
    .spacing(TOKENS.spacing.s0_5),
  ]
  .spacing(TOKENS.spacing.s2)
  .align_y(Alignment::Center)
  .into()
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
  let btn = button(
    container(icon_for_variant(icon, IconSize::Md, variant))
      .width(Fill)
      .align_x(Alignment::Center),
  )
  .padding([7, 0])
  .width(Fill)
  .on_press(Message::Home(HomeMessage::Navigate(destination)))
  .style(move |theme, status| jellypilot_ui::theme::button_variant(theme, status, variant));

  tooltip(btn, label, TooltipOptions::default())
}

fn compact_connection_status(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let Some(identity) = &state.kernel.connected_identity else {
    return space::vertical().into();
  };
  let summary = format!("{} • {}", identity.user_name, identity.server);
  let dot = container(space::horizontal())
    .width(8.0)
    .height(8.0)
    .style(|_theme| container::Style {
      background: Some(iced::Background::Color(palette.colors.onSurfaceVariant)),
      border: iced::Border {
        radius: TOKENS.radii.full.into(),
        ..iced::Border::default()
      },
      ..container::Style::default()
    });
  let trigger = container(dot)
    .padding(TOKENS.spacing.s2)
    .width(Fill)
    .align_x(Alignment::Center);
  tooltip(trigger, summary, TooltipOptions::default())
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
    assert_eq!(sidebar_width(SizeClass::Standard), 248.0);
    assert_eq!(sidebar_width(SizeClass::Wide), 248.0);
  }

  #[test]
  fn shell_view_renders_in_loading_state() {
    let mut state = State::boot(false);
    state.shell.skeleton_phase = 0.42;
    state.home.data.shortcuts = LoadState::Loading;
    let _element = view(&state);
  }
}
