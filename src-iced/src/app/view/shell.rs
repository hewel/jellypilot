use crate::app::message::{BrowseMessage, HomeMessage, Message};
use crate::app::state::{Destination, State};
use iced::widget::{button, column, container, row, space, stack, text, text_input, Column};
use iced::{Alignment, Element, Fill, Length};
use jellypilot_core::LoadState;
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::icons::{icon_for_variant, icon_with_color, Icon, IconSize};
use jellypilot_ui::overlay::{tooltip, TooltipOptions};
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::{ButtonVariant, FieldVariant, SurfaceVariant};

use super::{browse, detail, home, player, settings};

const SIDEBAR_WIDTH: f32 = 248.0;

pub fn view(state: &State) -> Element<'_, Message> {
  let sidebar = sidebar(state).width(Length::Fixed(SIDEBAR_WIDTH));
  let content: Element<'_, Message> = match &state.destination {
    Destination::Home => home::view(state),
    Destination::Library { .. } | Destination::Search(_) => browse::view(state),
    Destination::Detail(_) => detail::view(state),
    Destination::Settings => settings::view(state),
  };
  let content = stack![content].width(Fill).height(Fill);
  let mut content_column = Column::new()
    .spacing(TOKENS.spacing.s2)
    .width(Fill)
    .height(Fill);
  if let Some(notice) = visible_notice(state) {
    content_column = content_column.push(
      container(
        row![
          icon_with_color(Icon::Warning, IconSize::Sm, TOKENS.colors.warning),
          text(notice).size(13).color(TOKENS.colors.warning),
        ]
        .spacing(TOKENS.spacing.s2)
        .align_y(Alignment::Center),
      )
      .padding([8, 12])
      .width(Fill)
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Elevated)),
    );
  }
  content_column = content_column.push(content);
  let body = row![sidebar, content_column]
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
fn visible_notice(state: &State) -> Option<&str> {
  state.notice.as_deref().or(state.playback_notice.as_deref())
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
    .padding([10, 12])
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
  .padding([9, 11])
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
  .padding([10, 12])
  .width(Fill)
  .on_press(Message::Home(HomeMessage::Navigate(destination)))
  .style(move |theme, status| jellypilot_ui::theme::button_variant(theme, status, variant))
  .into()
}
fn shortcut_skeleton<'a>() -> Element<'a, Message> {
  container(space::horizontal())
    .height(38)
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
  fn general_notice_takes_precedence_over_stale_playback_notice() {
    let mut state = State::boot(false);
    state.playback_notice = Some("Playback stopped.".to_owned());
    state.notice = Some("Library refresh failed.".to_owned());

    assert_eq!(visible_notice(&state), Some("Library refresh failed."));
  }
}
