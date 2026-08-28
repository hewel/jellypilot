use iced::widget::{button, column, container, row, space, stack, text, text_input, Column};
use iced::{Element, Fill, Length};
use jellypilot_core::LoadState;
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::{ButtonVariant, FieldVariant, SurfaceVariant};

use crate::app::message::{BrowseMessage, HomeMessage, Message};
use crate::app::state::{Destination, State};

use super::{browse, detail, home};

const SIDEBAR_WIDTH: f32 = 248.0;

pub fn view(state: &State) -> Element<'_, Message> {
  let sidebar = sidebar(state).width(Length::Fixed(SIDEBAR_WIDTH));
  let content: Element<'_, Message> = match &state.destination {
    Destination::Home => home::view(state),
    Destination::Library { .. } | Destination::Search(_) => browse::view(state),
    Destination::Detail(_) => detail::view(state),
  };
  let content = stack![content].width(Fill).height(Fill);
  let shell = row![sidebar, content]
    .spacing(TOKENS.spacing.s4)
    .padding(TOKENS.spacing.s3)
    .width(Fill)
    .height(Fill);

  container(shell)
    .width(Fill)
    .height(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Filled))
    .into()
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
  let search_slot = text_input("Search videos", &state.search_input)
    .on_input(|value| Message::Browse(BrowseMessage::SearchInputChanged(value)))
    .on_submit(Message::Browse(BrowseMessage::SearchSubmitted))
    .padding([10, 12])
    .size(14)
    .width(Fill)
    .style(|theme, status| {
      jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
    });

  let mut destinations = Column::new()
    .spacing(TOKENS.spacing.s1_5)
    .push(destination_button(
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
        destinations = destinations.push(destination_button(&shortcut.name, destination, active));
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
  let bottom = connection_summary(state);
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
  label: &'a str,
  destination: Destination,
  active: bool,
) -> Element<'a, Message> {
  button(text(label).size(14).width(Fill))
    .padding([10, 12])
    .width(Fill)
    .on_press(Message::Home(HomeMessage::Navigate(destination)))
    .style(move |theme, status| {
      jellypilot_ui::theme::button_variant(
        theme,
        status,
        if active {
          ButtonVariant::Secondary
        } else {
          ButtonVariant::Text
        },
      )
    })
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
  column![
    text(&identity.user_name)
      .size(13)
      .color(TOKENS.colors.onSurface),
    text(&identity.server)
      .size(11)
      .color(TOKENS.colors.onSurfaceVariant),
  ]
  .spacing(TOKENS.spacing.s1)
  .into()
}
