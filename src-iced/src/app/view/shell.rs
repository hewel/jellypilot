use super::{browse, detail, home, player, settings};
use crate::app::message::{BrowseMessage, HomeMessage, Message};
use crate::app::state::{Destination, NoticeLevel, State, ToastNotice};
use iced::widget::{button, column, container, row, space, stack, text, text_input, Column};
use iced::{Alignment, Color, Element, Fill, Length};
use jellypilot_core::LoadState;
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::icons::{icon_for_variant, icon_with_color, Icon, IconSize};
use jellypilot_ui::overlay::{tooltip, TooltipOptions};
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::{ButtonVariant, FieldVariant, SurfaceVariant};

const SIDEBAR_WIDTH: f32 = 248.0;

pub fn view(state: &State) -> Element<'_, Message> {
  let sidebar = sidebar(state).width(Length::Fixed(SIDEBAR_WIDTH));
  let content: Element<'_, Message> = match &state.destination {
    Destination::Home => home::view(state),
    Destination::Library { .. } | Destination::Search(_) => browse::view(state),
    Destination::Detail(_) => detail::view(state),
    Destination::Settings => settings::view(state),
  };
  let mut content_stack = stack![content].width(Fill).height(Fill);
  if let Some(toast) = visible_toast(state) {
    content_stack = content_stack.push(
      container(toast_view(toast))
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
  let body = row![sidebar, content_stack]
    .spacing(TOKENS.spacing.s4)
    .width(Fill)
    .height(Fill);
  let mut shell = Column::new().spacing(TOKENS.spacing.s3).push(body);
  if let Some(player_bar) = player::bar(state) {
    shell = shell.push(player_bar);
  }
  let shell = shell.padding(TOKENS.spacing.s3).width(Fill).height(Fill);

  container(shell)
    .width(Fill)
    .height(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Filled))
    .into()
}
fn visible_toast(state: &State) -> Option<&ToastNotice> {
  state.active_toast.as_ref()
}

#[allow(dead_code)]
pub fn visible_notice(state: &State) -> Option<&str> {
  state
    .active_toast
    .as_ref()
    .map(|toast| toast.message.as_str())
}

fn toast_view(toast: &ToastNotice) -> Element<'_, Message> {
  let colors = TOKENS.colors;
  let (icon, icon_color, text_color, bg_color, border_color) = match toast.level {
    NoticeLevel::Error => (
      Icon::Warning,
      colors.error,
      colors.onErrorContainer,
      with_alpha(colors.errorContainer, 0.90),
      with_alpha(colors.error, 0.40),
    ),
    NoticeLevel::Warning => (
      Icon::Warning,
      colors.warning,
      colors.onWarningContainer,
      with_alpha(colors.warningContainer, 0.90),
      with_alpha(colors.warning, 0.40),
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
        color: border_color,
        width: 1.0,
        radius: TOKENS.radii.xl.into(),
      },
      shadow: TOKENS.shadows.x2l.iced(),
      ..container::Style::default()
    })
    .into()
}

fn with_alpha(color: Color, alpha: f32) -> Color {
  Color { a: alpha, ..color }
}

fn sidebar(state: &State) -> container::Container<'_, Message> {
  let title = column![
    text("JellyPilot")
      .font(SPACE_GROTESK_FONT)
      .size(26)
      .color(TOKENS.colors.onSurface),
    text("Video Library")
      .size(12)
      .color(TOKENS.colors.onSurfaceVariant),
  ]
  .spacing(TOKENS.spacing.s1);
  let search_input = text_input("Search videos", &state.search_input)
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
    ButtonVariant::Outlined,
  ))
  .padding([7, 11])
  .on_press(Message::Browse(BrowseMessage::SearchSubmitted))
  .style(|theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
  });
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
      state.destination == Destination::Home,
    ));
  match &state.home.shortcuts {
    LoadState::Idle | LoadState::Loading => {
      destinations = destinations
        .push(shortcut_skeleton())
        .push(shortcut_skeleton());
    }
    LoadState::Ready(shortcuts) => {
      for shortcut in shortcuts {
        let destination = Destination::Library {
          library_id: shortcut.id.clone(),
          collection_type: shortcut.collection_type.clone(),
        };
        let active = state.destination == destination;
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
          .color(TOKENS.colors.warning),
      );
    }
  }

  let main = column![title, search_slot, destinations]
    .spacing(TOKENS.spacing.s5)
    .width(Fill);
  let bottom = column![
    destination_button(
      Icon::Settings,
      "Settings",
      Destination::Settings,
      state.destination == Destination::Settings,
    ),
    connection_summary(state),
  ]
  .spacing(TOKENS.spacing.s3);
  let content = column![main, space::vertical(), bottom]
    .spacing(TOKENS.spacing.s4)
    .width(Fill)
    .height(Fill);

  container(content)
    .padding(TOKENS.spacing.s4)
    .height(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Elevated))
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
fn shortcut_skeleton<'a>() -> Element<'a, Message> {
  container(space::horizontal())
    .height(34)
    .width(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Filled))
    .into()
}

fn connection_summary(state: &State) -> Element<'_, Message> {
  let Some(identity) = &state.connected_identity else {
    return space::vertical().into();
  };
  row![
    icon_with_color(Icon::Server, IconSize::Md, TOKENS.colors.onSurfaceVariant),
    column![
      text(&identity.user_name)
        .size(13)
        .color(TOKENS.colors.onSurface),
      text(&identity.server)
        .size(11)
        .color(TOKENS.colors.onSurfaceVariant),
    ]
    .spacing(TOKENS.spacing.s0_5),
  ]
  .spacing(TOKENS.spacing.s2)
  .align_y(Alignment::Center)
  .into()
}
#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn active_toast_is_rendered_and_cleared_on_dismiss() {
    let mut state = State::boot(false);
    assert_eq!(visible_notice(&state), None);
    assert_eq!(visible_toast(&state), None);

    state.active_toast = Some(ToastNotice {
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
    state.active_toast = Some(ToastNotice {
      id: 1,
      message: "First notice".to_owned(),
      level: NoticeLevel::Warning,
    });
    state.active_toast = Some(ToastNotice {
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
}
