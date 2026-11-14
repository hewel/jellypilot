//! Video-only companion window; playback remains owned by the shared session.

use iced::widget::{column, container, mouse_area, row, space, stack, text, themer, Widget as _};
use iced::{Alignment, Element, Fill, Length};
use jellypilot_mpv::player::format_duration;
use jellypilot_ui::fonts::MONO_FONT;
use jellypilot_ui::icons::{Icon, IconSize};
use jellypilot_ui::overlay::{focus_tooltip, TooltipOptions};
use jellypilot_ui::tokens::{DARK_PALETTE, TOKENS};
use jellypilot_ui::variants::ButtonVariant;
use jellypilot_ui::widgets::control_button::control_button;
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::embedded_player as cinema;
use jellypilot_ui::widgets::notice_interaction::notice_interaction;

use crate::app::message::Message;
use crate::app::pip::{self, Message as PipMessage};
use crate::app::state::State;

pub(crate) fn view(state: &State) -> Element<'_, Message> {
  composition(
    state,
    pip::controls_visible(state),
    pip::controls_enabled(state),
  )
}

fn composition(state: &State, visible: bool, enabled: bool) -> Element<'_, Message> {
  let playing = state.playback.view.now_playing.as_ref();
  let title = playing.map_or_else(
    || state.t("player-pip"),
    |playing| playing.item.title.clone(),
  );
  let paused = playing.is_none_or(|playing| playing.paused);
  let header = row![
    container(
      ellipsis_text(title)
        .size(14)
        .color(DARK_PALETTE.text.heading)
    )
    .width(Fill),
    action(
      state,
      "pip-return",
      Icon::ArrowsMaximize,
      "player-pip-return",
      PipMessage::Return,
      enabled
    ),
    action(
      state,
      "pip-close",
      Icon::Close,
      "player-pip-close",
      PipMessage::Close,
      enabled
    ),
  ]
  .spacing(TOKENS.spacing.s1)
  .align_y(Alignment::Center);
  let transport = action(
    state,
    "pip-play-pause",
    if paused { Icon::Play } else { Icon::Pause },
    if paused {
      "common-play"
    } else {
      "common-pause"
    },
    PipMessage::TogglePaused,
    enabled
      && playing.is_some()
      && !state.playback.view.busy
      && !state.playback.view.lifecycle.replacing,
  );
  let mut footer = column![container(transport).center_x(Fill)].spacing(TOKENS.spacing.s1);
  if let Some(playing) = playing {
    footer = footer.push(
      container(
        text(format_duration(playing.position_seconds))
          .font(MONO_FONT)
          .size(12),
      )
      .center_x(Fill)
      .boxed(),
    );
  }
  let controls = column![header, space::vertical(), footer].padding(12);
  let controls = notice_interaction(controls, |interaction| {
    Message::Pip(PipMessage::FocusChanged(interaction.focused))
  });
  let mut chrome = stack![
    container(space())
      .width(Fill)
      .height(68)
      .style(|_| cinema::scrim(true)),
    container(
      container(space())
        .width(Fill)
        .height(96)
        .style(cinema::minimal_scrim),
    )
    .width(Fill)
    .height(Fill)
    .align_y(Alignment::End),
    controls,
  ]
  .width(Fill)
  .height(Fill);
  if let Some(notice) = pip::notice(state) {
    chrome = chrome.push(
      container(
        container(text(state.kernel.locale.message(notice)).size(13))
          .padding(12)
          .style(cinema::prompt),
      )
      .padding([56, 12])
      .center(Fill)
      .boxed(),
    );
  }
  let mut layers = stack![
    mouse_area(crate::embedded::view()).on_press(Message::Pip(PipMessage::Drag)),
    jellypilot_ui::widgets::motion::reveal(chrome, visible, false, TOKENS.durations.none),
  ]
  .width(Fill)
  .height(Fill);
  for (direction, horizontal, vertical, width, height, cursor) in [
    (
      iced::window::Direction::North,
      Alignment::Center,
      Alignment::Start,
      Length::Fill,
      Length::Fixed(6.0),
      iced::mouse::Interaction::ResizingVertically,
    ),
    (
      iced::window::Direction::South,
      Alignment::Center,
      Alignment::End,
      Length::Fill,
      Length::Fixed(6.0),
      iced::mouse::Interaction::ResizingVertically,
    ),
    (
      iced::window::Direction::West,
      Alignment::Start,
      Alignment::Center,
      Length::Fixed(6.0),
      Length::Fill,
      iced::mouse::Interaction::ResizingHorizontally,
    ),
    (
      iced::window::Direction::East,
      Alignment::End,
      Alignment::Center,
      Length::Fixed(6.0),
      Length::Fill,
      iced::mouse::Interaction::ResizingHorizontally,
    ),
    (
      iced::window::Direction::NorthWest,
      Alignment::Start,
      Alignment::Start,
      Length::Fixed(12.0),
      Length::Fixed(12.0),
      iced::mouse::Interaction::ResizingDiagonallyDown,
    ),
    (
      iced::window::Direction::NorthEast,
      Alignment::End,
      Alignment::Start,
      Length::Fixed(12.0),
      Length::Fixed(12.0),
      iced::mouse::Interaction::ResizingDiagonallyUp,
    ),
    (
      iced::window::Direction::SouthWest,
      Alignment::Start,
      Alignment::End,
      Length::Fixed(12.0),
      Length::Fixed(12.0),
      iced::mouse::Interaction::ResizingDiagonallyUp,
    ),
    (
      iced::window::Direction::SouthEast,
      Alignment::End,
      Alignment::End,
      Length::Fixed(12.0),
      Length::Fixed(12.0),
      iced::mouse::Interaction::ResizingDiagonallyDown,
    ),
  ] {
    layers = layers.push(
      container(
        mouse_area(space().width(width).height(height))
          .on_press(Message::Pip(PipMessage::Resize(direction)))
          .interaction(cursor),
      )
      .width(Fill)
      .height(Fill)
      .align_x(horizontal)
      .align_y(vertical)
      .boxed(),
    );
  }
  container(jellypilot_ui::widgets::focus_scope::focus_scope(
    themer(
      Some(cinema::theme()),
      mouse_area(layers)
        .on_move(|_| Message::Pip(PipMessage::PointerMoved))
        .on_exit(Message::Pip(PipMessage::PointerLeft)),
    ),
    pip::focus_visibility(state),
  ))
  .id("pip-root")
  .width(Fill)
  .height(Fill)
  .boxed()
}

fn action<'a>(
  state: &State,
  id: &'static str,
  icon: Icon,
  label: &'static str,
  message: PipMessage,
  enabled: bool,
) -> Element<'a, Message> {
  focus_tooltip(
    control_button(Some(icon), None, ButtonVariant::Tonal)
      .id(id)
      .icon_size(IconSize::Custom(20.0))
      .style(cinema::top_control)
      .padding(0)
      .width(Length::Fixed(40.0))
      .min_height(40.0)
      .content_centered(true)
      .on_press_maybe(enabled.then_some(Message::Pip(message))),
    state.t(label),
    TooltipOptions::default(),
  )
}

pub(crate) fn placeholder(state: &State) -> Element<'_, Message> {
  let title = state
    .playback
    .view
    .now_playing
    .as_ref()
    .map(|playing| playing.item.title.as_str())
    .unwrap_or("");
  let mut content = column![
    ellipsis_text(title).size(20),
    text(state.t("player-pip-active")).size(14),
    control_button(
      Some(Icon::PictureInPicture),
      Some(state.t("player-pip-return")),
      ButtonVariant::Primary
    )
    .on_press_maybe(
      pip::controls_enabled(state).then(|| pip::bind_current(state, PipMessage::Return))
    ),
  ]
  .spacing(TOKENS.spacing.s4)
  .align_x(Alignment::Center);
  if state.app_mode() == jellypilot_core::config::AppMode::Full {
    content = content.push(
      control_button(
        Some(Icon::ChevronLeft),
        Some(state.t("player-back")),
        ButtonVariant::Text,
      )
      .on_press(Message::Home(crate::app::message::HomeMessage::Navigate(
        crate::app::state::Destination::Home,
      )))
      .boxed(),
    );
  }
  container(container(content).width(Length::Fit.max(480.0)))
    .padding(24)
    .center(Fill)
    .boxed()
}

#[cfg(test)]
mod tests {
  use super::*;
  use iced::advanced::{renderer::Headless, widget};
  use iced::{mouse, Event, Point, Rectangle, Size};
  use iced_runtime::user_interface::{Cache, UserInterface};

  #[derive(Default)]
  struct Targets(Vec<(widget::Id, Rectangle)>);

  impl widget::Operation for Targets {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
      visit(self);
    }

    fn focusable(
      &mut self,
      id: Option<&widget::Id>,
      bounds: Rectangle,
      _: &mut dyn widget::operation::Focusable,
    ) {
      if let Some(id) = id {
        self.0.push((id.clone(), bounds));
      }
    }
  }

  #[tokio::test]
  async fn small_window_controls_do_not_drag_or_resize_and_hidden_controls_release_hit_targets() {
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    let mut state = State::boot(false);
    for language in [
      jellypilot_core::locale::UiLanguage::English,
      jellypilot_core::locale::UiLanguage::SimplifiedChinese,
    ] {
      state.kernel.locale = crate::i18n::Localizer::new(language);
      for size in [
        Size::new(280.0, 158.0),
        Size::new(480.0, 270.0),
        Size::new(280.0, 480.0),
      ] {
        let mut ui = UserInterface::build(
          composition(&state, true, true),
          size,
          Cache::new(),
          &mut renderer,
        );
        let mut targets = Targets::default();
        ui.operate(&renderer, &mut targets);
        let target = |id| {
          targets
            .0
            .iter()
            .find(|(key, _)| *key == widget::Id::new(id))
            .expect("window action")
            .1
        };
        let close = target("pip-close");
        let back = target("pip-return");
        for bounds in [close, back] {
          assert!(
            bounds.x >= 0.0
              && bounds.y >= 0.0
              && bounds.x + bounds.width <= size.width
              && bounds.y + bounds.height <= size.height
          );
        }
        let mut click = |point: Point| {
          let mut bus = iced::advanced::shell::Bus::new();
          let _ = ui.update(
            &iced::window::Headless,
            &iced::advanced::shell::Waker::noop(),
            &[
              Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
              Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ],
            mouse::Cursor::Available(point),
            &mut renderer,
            &mut bus,
          );
          bus
            .drain()
            .filter_map(|(message, _)| match message {
              Message::Pip(action)
                if !matches!(
                  action,
                  PipMessage::FocusChanged(_) | PipMessage::PointerMoved | PipMessage::PointerLeft
                ) =>
              {
                Some(action)
              }
              _ => None,
            })
            .collect::<Vec<_>>()
        };
        assert!(matches!(
          click(close.center()).as_slice(),
          [PipMessage::Close]
        ));
        assert!(matches!(
          click(back.center()).as_slice(),
          [PipMessage::Return]
        ));
        assert!(matches!(
          click(Point::new(size.width / 2.0, size.height / 2.0)).as_slice(),
          [PipMessage::Drag]
        ));
        assert!(matches!(
          click(Point::new(2.0, 2.0)).as_slice(),
          [PipMessage::Resize(iced::window::Direction::NorthWest)]
        ));
        assert!(matches!(
          click(Point::new(size.width - 2.0, size.height - 2.0)).as_slice(),
          [PipMessage::Resize(iced::window::Direction::SouthEast)]
        ));
        let cache = ui.into_cache();
        let mut hidden =
          UserInterface::build(composition(&state, false, true), size, cache, &mut renderer);
        let mut bus = iced::advanced::shell::Bus::new();
        let _ = hidden.update(
          &iced::window::Headless,
          &iced::advanced::shell::Waker::noop(),
          &[Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
          ))],
          mouse::Cursor::Available(close.center()),
          &mut renderer,
          &mut bus,
        );
        let messages: Vec<_> = bus.drain().map(|(message, _)| message).collect();
        assert!(messages
          .iter()
          .any(|message| matches!(message, Message::Pip(PipMessage::Drag))));
        assert!(!messages.iter().any(|message| matches!(
          message,
          Message::Pip(PipMessage::Close | PipMessage::Return)
        )));
      }
    }
  }
}
