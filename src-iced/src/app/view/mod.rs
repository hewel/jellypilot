mod account;
pub(crate) mod browse;
mod detail;
pub(crate) mod home;
mod login;
mod modal_dismiss;
mod personal_lists;
mod player;
pub(crate) mod scroll_memory;
mod settings;
pub(crate) mod shell;

use crate::i18n::Localizer;
use iced::widget::{button, container, row, stack, text};
use iced::{Alignment, Color, Element, Fill, Length};
use jellypilot_auth::login::ConnectionPhase;
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::tokens::{ThemePalette, TOKENS};
use jellypilot_ui::widgets::inert::inert;

use super::message::Message;
use super::state::{NoticeLevel, State, ToastNotice};

pub fn view(state: &State) -> Element<'_, Message> {
  let base = if state.kernel.connection == ConnectionPhase::Connected {
    shell::view(state)
  } else {
    login::view(state)
  };
  // Keep this ancestor stable: adding or removing it would reset descendant
  // input focus, cursor positions, and scroll state during iced reconciliation.
  let mut layers = if let Some(modal) = account::modal_layer(state) {
    stack![inert(base), modal]
  } else {
    stack![base]
  }
  .width(Fill)
  .height(Fill);
  if let Some(toast) = state.kernel.active_toast.as_ref() {
    layers = layers.push(
      container(toast_view(state.palette(), state.kernel.locale, toast))
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
  layers.into()
}

fn toast_view<'a>(
  palette: &'static ThemePalette,
  locale: Localizer,
  toast: &'a ToastNotice,
) -> Element<'a, Message> {
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
        }
        .smoothing(jellypilot_ui::widgets::container::SURFACE_SMOOTHING),
        ..button::Style::default()
      }
    });

  let toast_content = row![
    icon_with_color(icon, IconSize::Sm, icon_color),
    text(locale.message(&toast.message))
      .size(13)
      .color(text_color)
      .width(Fill),
    dismiss_button,
  ]
  .spacing(TOKENS.spacing.s2)
  .align_y(Alignment::Center);

  container(toast_content)
    .width(Length::Fill.max(440.0))
    .padding([10, 14])
    .style(move |_theme| container::Style {
      background: Some(iced::Background::Color(bg_color)),
      text_color: Some(text_color),
      border: iced::Border {
        smoothing: jellypilot_ui::widgets::container::SURFACE_SMOOTHING,
        color: colors.outlineVariant,
        width: 1.0,
        radius: TOKENS.radii.lg.into(),
      },
      shadow: palette.shadows.raised_high.iced(),
      ..container::Style::default()
    })
    .into()
}
