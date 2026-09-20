use iced::widget::{column, container, keyed, row, text};
use iced::{Alignment, Element, Fill, Length};
use jellypilot_ui::icons::{Icon, IconSize};
use jellypilot_ui::overlay::{tooltip, TooltipOptions};
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};
use jellypilot_ui::widgets::control_button::control_button;
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::notice_interaction::notice_interaction;

use crate::app::message::Message;
use crate::app::state::State;
use crate::app::{accounts, undo};

pub(super) fn view(state: &State) -> Element<'_, Message> {
  let children = state.kernel.undo.queue.visible().filter_map(|id| {
    let notice = state.kernel.undo.notices.get(&id)?;
    let message = state.kernel.locale.message(&notice.removal.message());
    let mut copy = column![ellipsis_text(message.clone())
      .size(14)
      .color(state.palette().text.heading)]
    .spacing(4);
    if notice.failed {
      copy = copy.push(
        text(state.t("lists-undo-failed"))
          .size(12)
          .color(state.palette().colors.error),
      );
    }
    let blocked = notice.pending || accounts::content_mutations_blocked(&state.kernel);
    let restore = control_button(
      None,
      Some(state.t(if notice.pending {
        "common-loading"
      } else if notice.failed {
        "lists-retry-undo"
      } else {
        "lists-undo"
      })),
      ButtonVariant::Tonal,
    )
    .min_height(40.0)
    .id(iced::widget::Id::from(format!("undo-restore-{id}")))
    .on_press_maybe((!blocked).then_some(Message::Undo(undo::Message::Restore(id))));
    let dismiss = tooltip(
      control_button(Some(Icon::Close), None, ButtonVariant::Tonal)
        .icon_size(IconSize::Sm)
        .width(Length::Fixed(40.0))
        .min_height(40.0)
        .id(iced::widget::Id::from(format!("undo-dismiss-{id}")))
        .on_press_maybe((!notice.pending).then_some(Message::Undo(undo::Message::Dismiss(id)))),
      state.t("common-dismiss"),
      TooltipOptions::default(),
    );
    let content = container(
      row![
        tooltip(copy.width(Fill), message, TooltipOptions::default()),
        restore,
        dismiss
      ]
      .spacing(8)
      .align_y(Alignment::Center),
    )
    .padding(12)
    .width(Fill)
    .height(Length::Shrink.min(72.0))
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Floating));
    Some((
      id,
      notice_interaction(content, move |interaction| {
        Message::Undo(undo::Message::Interaction(id, interaction))
      }),
    ))
  });
  let stack = keyed::Column::with_children(children)
    .spacing(8)
    .width(Length::Fill.max(560.0));
  container(stack)
    .width(Fill)
    .height(Fill)
    .align_x(Alignment::End)
    .align_y(Alignment::End)
    .padding(16)
    .into()
}
