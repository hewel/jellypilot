pub(crate) mod account;
mod activation_guard;
pub(crate) mod browse;
mod detail;
pub(crate) mod home;
pub(crate) mod image_observer;
mod image_transition;
mod login;
mod modal_dismiss;
pub(crate) mod motion;
mod personal_lists;
pub(crate) mod player;
mod player_info;
pub(crate) mod scroll_memory;
mod settings;
pub(crate) mod shell;

use crate::i18n::Localizer;
use iced::widget::{button, container, row, stack, text};
use iced::{Alignment, Color, Element, Fill, Length};
use jellypilot_auth::login::ConnectionPhase;
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::tokens::{ThemePalette, TOKENS};
use jellypilot_ui::variants::ButtonVariant;
use jellypilot_ui::widgets::control_button::control_button_content;
use jellypilot_ui::widgets::inert::{concealed, inert};

use super::message::Message;
use super::state::{NoticeLevel, State, ToastNotice};

pub fn view(state: &State) -> Element<'_, Message> {
  let base = if state.kernel.connection == ConnectionPhase::Connected {
    shell::view(state)
  } else {
    login::view(state)
  };
  // Preserve the browser's layout and widget state while native video fills the window.
  let base = container(base)
    .width(if state.shell.player_fullscreen {
      Length::Fixed(state.shell.window_size.width)
    } else {
      Fill
    })
    .height(if state.shell.player_fullscreen {
      Length::Fixed(state.shell.window_size.height)
    } else {
      Fill
    });
  // Keep this ancestor stable: adding or removing it would reset descendant
  // input focus, cursor positions, and scroll state during iced reconciliation.
  let layers = if state.shell.player_fullscreen {
    stack![concealed(base), player::embedded(state)]
  } else {
    let base: Element<'_, Message> = if account::modal_open(state) {
      inert(base)
    } else {
      base.into()
    };
    stack![base, account::modal_layer(state)]
  }
  .width(Fill)
  .height(Fill);
  let toast = state
    .kernel
    .active_toast
    .as_ref()
    .or(state.motion.toast.as_ref());
  let toast_content: Element<'_, Message> = match toast {
    Some(toast) => container(toast_view(state.palette(), state.kernel.locale, toast))
      .width(Fill)
      .padding(iced::Padding {
        top: TOKENS.spacing.s2,
        right: TOKENS.spacing.s3,
        bottom: 0.0,
        left: TOKENS.spacing.s3,
      })
      .align_x(Alignment::End)
      .into(),
    None => iced::widget::Space::new().into(),
  };
  let layers = layers.push(jellypilot_ui::widgets::motion::reveal(
    toast_content,
    state.kernel.active_toast.is_some(),
    state.shell.images_visible && !state.kernel.settings.snapshot().reduced_motion(),
    TOKENS.durations.ms200,
  ));
  player::guard_activation(layers, state)
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
  let dismiss_button = control_button_content(
    move |_| icon_with_color(Icon::Close, IconSize::Xs, text_color).into(),
    ButtonVariant::Text,
  )
  .padding([3, 5])
  .min_height(0.0)
  .on_press(Message::DismissNotice(close_id))
  .style(|_theme, _variant, status| {
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
