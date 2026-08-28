use iced::widget::{column, container, text};
use iced::{Element, Fill};
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::SurfaceVariant;

use crate::app::message::Message;
use crate::app::state::State;

pub fn view(state: &State) -> Element<'_, Message> {
  let identity = state.connected_identity.as_ref();
  let connection = identity.map_or_else(
    || "Connected session".to_owned(),
    |identity| format!("Connected as {}@{}", identity.user_name, identity.server),
  );
  let mut content = column![
    text("JellyPilot")
      .font(SPACE_GROTESK_FONT)
      .size(44)
      .color(TOKENS.colors.onSurface),
    text(connection)
      .size(18)
      .color(TOKENS.colors.onSurfaceVariant),
    text("Your Home library will appear here in the next application slice.")
      .size(16)
      .color(TOKENS.colors.onSurfaceVariant),
  ]
  .spacing(18);
  if let Some(notice) = &state.notice {
    content = content.push(text(notice).color(TOKENS.colors.warning));
  }

  let card = container(content)
    .padding(40)
    .max_width(720)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Elevated));
  container(card)
    .width(Fill)
    .height(Fill)
    .center_x(Fill)
    .center_y(Fill)
    .padding(40)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Filled))
    .into()
}
