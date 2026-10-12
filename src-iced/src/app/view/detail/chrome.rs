use iced::widget::container;
use iced::widget::Widget as _;
use iced::{Element, Fill};
use jellypilot_ui::icons::{Icon, IconSize};
use jellypilot_ui::variants::ButtonVariant;
use jellypilot_ui::widgets::control_button::control_button;

use crate::app::message::{DetailMessage, Message};
use crate::app::state::State;

pub(super) fn detail_back(state: &State) -> Element<'_, Message> {
  let button = control_button(
    Some(Icon::ChevronLeft),
    Some(state.t("detail-back")),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .label_size(12.0)
  .spacing(6.0)
  .padding([8, 14])
  .style(jellypilot_ui::widgets::button::detail_back)
  .on_press_maybe(
    (!state.shell.navigation_stack.is_empty()).then_some(Message::Detail(DetailMessage::Back)),
  );
  container(button).padding(18).width(Fill).boxed()
}
