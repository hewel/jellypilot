//! File-scoped subtitle roles and A/B repeat controls above the player surface.

use std::collections::HashMap;

use iced::advanced::{layout, mouse, overlay, renderer, widget, Layout, Shell, Widget};
use iced::widget::{column, container, row, scrollable, space, text, Column};
use iced::{Alignment, Element, Event, Fill, Length, Rectangle, Size, Theme, Vector};
use jellypilot_core::request_gate::SessionToken;
use jellypilot_mpv::playback::tools::{PlaybackFileToken, PlaybackToolAction, ToolState};
use jellypilot_mpv::playback::TrackInfo;
use jellypilot_mpv::playback_session::TracksView;
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::overlay::{popover, Placement, PopoverOptions};
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::ButtonVariant;
use jellypilot_ui::widgets::control_button::{control_button, control_button_content};

use crate::app::message::{Message, PlaybackMessage, SettingsMessage};
use crate::app::playback::tools::Message as ToolsMessage;
use crate::app::state::State;

fn close() -> Message {
  Message::Playback(PlaybackMessage::Tools(ToolsMessage::Close))
}

fn execute(state: &State, action: PlaybackToolAction) -> Option<Message> {
  Some(Message::Playback(PlaybackMessage::Tools(
    ToolsMessage::PanelExecute {
      session: state.kernel.request_gate.current_session(),
      file: state.playback.view.tools.file?,
      presentation: state.playback.tools.presentation,
      action,
    },
  )))
}

fn available(state: &State) -> bool {
  let view = &state.playback.view;
  view.tools.file.is_some()
    && view.engine_available
    && !view.busy
    && !view.tools.busy
    && !view.lifecycle.replacing
    && !state.kernel.sdk.content_mutations_blocked()
}

pub(super) fn loop_entry(state: &State) -> Element<'_, Message> {
  control_button(None, Some(state.t("ab-loop-title")), ButtonVariant::Text)
    .id("playback-tools-entry")
    .min_height(40.0)
    .width(Fill)
    .on_press(Message::Playback(PlaybackMessage::Tools(
      ToolsMessage::OpenLoop,
    )))
    .into()
}

pub(super) fn layer(state: &State) -> Element<'_, Message> {
  let looping = state.playback.tools.open;
  let options = state.playback.tools.options_open;
  if state.tv_mode() || (!looping && !options && !state.playback.subtitle_menu_open) {
    return space().into();
  }
  let width = (state.shell.window_size.width - 32.0).clamp(
    0.0,
    if looping {
      320.0
    } else if options {
      260.0
    } else {
      240.0
    },
  );
  let content = if looping {
    loop_content(state)
  } else if options {
    panel(
      state,
      "player-options",
      column![
        loop_entry(state),
        control_button(
          Some(Icon::Settings),
          Some(state.t("common-settings")),
          ButtonVariant::Text
        )
        .id("playback-tools-app-settings")
        .min_height(40.0)
        .width(Fill)
        .on_press(Message::Settings(SettingsMessage::Open)),
      ]
      .spacing(TOKENS.spacing.s1)
      .into(),
      true,
    )
  } else {
    subtitle_content(state, crate::app::embedded_player::active(state))
  };
  container(popover(
    space().width(0).height(0),
    content,
    true,
    PopoverOptions {
      placement: if looping || options {
        Placement::Below
      } else {
        Placement::Above
      },
      alignment: jellypilot_ui::overlay::Alignment::End,
      width: Some(width),
      consume_outside_press: true,
      ..PopoverOptions::default()
    },
    close(),
  ))
  .width(Fill)
  .height(Fill)
  .align_x(Alignment::End)
  .align_y(if looping || options {
    Alignment::Start
  } else {
    Alignment::End
  })
  .padding(TOKENS.spacing.s4)
  .into()
}

fn panel<'a>(
  state: &State,
  title: &str,
  body: Element<'a, Message>,
  subtitles: bool,
) -> Element<'a, Message> {
  let heading = row![
    text(state.t(title)).size(TOKENS.font_sizes.s16).width(Fill),
    control_button(
      Some(Icon::Close),
      Some(state.t("common-close")),
      ButtonVariant::Text
    )
    .id("playback-tools-close")
    .min_height(40.0)
    .on_press(close()),
  ]
  .spacing(TOKENS.spacing.s2)
  .align_y(Alignment::Center);
  // The floating surface contributes padding; the fixed heading stays outside scrolling.
  let height =
    ((state.shell.window_size.height - 64.0).clamp(0.0, 640.0) - 2.0 * TOKENS.spacing.s3).max(0.0);
  let body = scrollable(body)
    .id("playback-tools-scroll")
    .style(jellypilot_ui::theme::scrollable)
    .height(if subtitles {
      Length::Fit.max(280.0)
    } else {
      Length::Fill
    });
  guard(
    state,
    container(
      column![heading, body]
        .spacing(TOKENS.spacing.s2)
        .height(if subtitles {
          Length::Fit.max(height)
        } else {
          Length::Fixed(height)
        })
        .width(Fill),
    )
    .id("playback-tools-content"),
  )
}

fn hint<'a>(state: &State, key: &str) -> Element<'a, Message> {
  text(state.t(key))
    .size(TOKENS.font_sizes.s13)
    .color(state.palette().text.metadata)
    .into()
}

fn action<'a>(
  state: &State,
  key: &'static str,
  action: PlaybackToolAction,
  enabled: bool,
) -> Element<'a, Message> {
  guard_scope(
    (scope(state), key),
    control_button(None, Some(state.t(key)), ButtonVariant::Tonal)
      .id(key)
      .min_height(40.0)
      .padding([TOKENS.spacing.s2, TOKENS.spacing.s2_5])
      .on_press_maybe(enabled.then(|| execute(state, action)).flatten()),
  )
}

fn feedback<'a>(state: &State, mut body: Column<'a, Message>, failed: bool) -> Column<'a, Message> {
  if state.playback.view.tools.busy {
    body = body.push(hint(state, "common-loading"));
  }
  if failed || state.playback.view.tools.error.is_some() {
    body = body.push(
      text(state.t("playback-tools-failed"))
        .size(TOKENS.font_sizes.s13)
        .color(state.palette().colors.error),
    );
  }
  body.push(action(
    state,
    "playback-tools-refresh",
    PlaybackToolAction::Refresh,
    available(state),
  ))
}

fn loop_content(state: &State) -> Element<'_, Message> {
  let tools = &state.playback.view.tools;
  let mut body = Column::new().width(Fill).spacing(TOKENS.spacing.s3);
  let failed = matches!(tools.ab_loop, ToolState::Failed);
  match &tools.ab_loop {
    ToolState::Loading => body = body.push(hint(state, "common-loading")),
    ToolState::Unavailable => body = body.push(hint(state, "ab-loop-unavailable")),
    ToolState::Failed => {}
    ToolState::Ready(looping) => {
      let duration = state
        .playback
        .view
        .now_playing
        .as_ref()
        .and_then(|playing| playing.duration_seconds)
        .filter(|duration| duration.is_finite() && *duration > 0.0);
      let position = state
        .playback
        .view
        .now_playing
        .as_ref()
        .map(|playing| playing.position_seconds)
        .filter(|position| position.is_finite() && *position >= 0.0)
        .filter(|position| duration.is_some_and(|duration| *position <= duration));
      let can_clear = available(state);
      let enabled = can_clear && tools.error.is_none() && looping.editable && duration.is_some();
      let point = |label, seconds: Option<f64>| {
        column![
          text(state.t(label))
            .size(TOKENS.font_sizes.s13)
            .color(state.palette().text.metadata),
          text(seconds.map_or_else(
            || "—".to_owned(),
            |seconds| state.kernel.locale.duration(seconds)
          ))
          .size(TOKENS.font_sizes.s16)
        ]
        .spacing(TOKENS.spacing.s1)
        .width(Fill)
      };
      body = body.push(
        row![
          point("ab-loop-a", looping.a_seconds),
          point("ab-loop-b", looping.b_seconds)
        ]
        .spacing(TOKENS.spacing.s3),
      );
      let mark_a = position.map(PlaybackToolAction::MarkA);
      let mark_b = position
        .filter(|position| {
          looping
            .a_seconds
            .is_some_and(|a| a.is_finite() && a >= 0.0 && *position > a)
        })
        .map(PlaybackToolAction::MarkB);
      let mark = |key: &'static str, command: Option<PlaybackToolAction>| {
        guard_scope(
          (scope(state), key),
          control_button(None, Some(state.t(key)), ButtonVariant::Tonal)
            .id(key)
            .min_height(40.0)
            .padding([TOKENS.spacing.s2, TOKENS.spacing.s2_5])
            .on_press_maybe(
              command
                .filter(|_| enabled)
                .and_then(|command| execute(state, command)),
            ),
        )
      };
      body = body.push(
        row![
          mark("ab-loop-mark-a", mark_a),
          mark("ab-loop-mark-b", mark_b)
        ]
        .spacing(TOKENS.spacing.s2)
        .wrap(),
      );
      let interval = looping
        .a_seconds
        .zip(looping.b_seconds)
        .is_some_and(|(a, b)| {
          a.is_finite()
            && b.is_finite()
            && a >= 0.0
            && a < b
            && duration.is_some_and(|duration| b <= duration)
        });
      body = body.push(hint(
        state,
        if looping.enabled {
          "ab-loop-configured"
        } else {
          "ab-loop-disabled"
        },
      ));
      body = body.push(
        row![
          action(
            state,
            if looping.enabled {
              "ab-loop-disable"
            } else {
              "ab-loop-enable"
            },
            PlaybackToolAction::SetLoopEnabled(!looping.enabled),
            if looping.enabled {
              can_clear
            } else {
              enabled && interval
            }
          ),
          action(
            state,
            "ab-loop-restart",
            PlaybackToolAction::RestartFromA,
            enabled
              && looping.a_seconds.is_some_and(|a| a.is_finite()
                && a >= 0.0
                && duration.is_some_and(|duration| a <= duration))
          ),
          action(
            state,
            "ab-loop-clear",
            PlaybackToolAction::ClearLoop,
            can_clear && (looping.a_seconds.is_some() || looping.b_seconds.is_some())
          ),
        ]
        .spacing(TOKENS.spacing.s2)
        .wrap(),
      );
      if !looping.editable || duration.is_none() {
        body = body.push(hint(state, "ab-loop-unavailable"));
      } else if !interval {
        body = body.push(hint(state, "ab-loop-mark-b-help"));
      }
      body = body.push(hint(state, "ab-loop-help"));
    }
  }
  panel(
    state,
    "ab-loop-title",
    feedback(state, body, failed).into(),
    false,
  )
}

pub(super) fn subtitle_content(state: &State, embedded: bool) -> Element<'_, Message> {
  let tools = &state.playback.view.tools;
  let enabled = available(state) && tools.error.is_none();
  let mut body = Column::new().width(Fill).spacing(TOKENS.spacing.s3);
  body = body.push(text(state.t("subtitle-primary")).size(TOKENS.font_sizes.s14));
  match &state.playback.view.tracks {
    TracksView::Loading => body = body.push(hint(state, "player-loading-subtitles")),
    TracksView::Unavailable => body = body.push(hint(state, "player-unavailable-subtitles")),
    TracksView::Ready {
      tracks, subtitle, ..
    } => {
      let mut rows = vec![(
        "primary-off".to_owned(),
        choice(
          state,
          "primary-off",
          state.t("common-off"),
          subtitle.is_none(),
          enabled,
          PlaybackToolAction::SelectPrimarySubtitle(None),
          embedded,
        ),
      )];
      for track in tracks.iter().filter(|track| track.track_type == "sub") {
        let id = format!("primary-{}", track.id);
        rows.push((
          id.clone(),
          choice(
            state,
            &id,
            track_label(state, track),
            *subtitle == Some(track.id),
            enabled,
            PlaybackToolAction::SelectPrimarySubtitle(Some(track.id)),
            embedded,
          ),
        ));
      }
      body = body.push(stable_rows(state, rows));
    }
  }
  body = body
    .push(text(state.t("subtitle-secondary")).size(TOKENS.font_sizes.s14))
    .push(hint(state, "subtitle-secondary-help"));
  match &tools.secondary {
    ToolState::Loading => body = body.push(hint(state, "common-loading")),
    ToolState::Unavailable => body = body.push(hint(state, "playback-tools-unavailable")),
    ToolState::Failed => {}
    ToolState::Ready(secondary) => {
      if let TracksView::Ready {
        tracks, subtitle, ..
      } = &state.playback.view.tracks
      {
        let primary_is_text = tracks
          .iter()
          .any(|track| Some(track.id) == *subtitle && track.is_text_subtitle());
        let eligible: Vec<_> = tracks
          .iter()
          .filter(|track| track.is_text_subtitle() && Some(track.id) != *subtitle)
          .collect();
        let mut rows = vec![(
          "secondary-off".to_owned(),
          choice(
            state,
            "secondary-off",
            state.t("common-off"),
            secondary.selected.is_none(),
            enabled,
            PlaybackToolAction::SelectSecondarySubtitle(None),
            embedded,
          ),
        )];
        if primary_is_text && secondary.eligible {
          for track in &eligible {
            let id = format!("secondary-{}", track.id);
            rows.push((
              id.clone(),
              choice(
                state,
                &id,
                track_label(state, track),
                secondary.selected == Some(track.id),
                enabled,
                PlaybackToolAction::SelectSecondarySubtitle(Some(track.id)),
                embedded,
              ),
            ));
          }
        }
        body = body.push(stable_rows(state, rows));
        if !primary_is_text || !secondary.eligible {
          body = body.push(hint(state, "subtitle-secondary-requires-primary"));
        } else if eligible.is_empty() {
          body = body.push(hint(state, "subtitle-secondary-empty"));
        }
        if secondary
          .selected
          .is_some_and(|selected| !eligible.iter().any(|track| track.id == selected))
        {
          body = body.push(hint(state, "playback-tools-unavailable"));
        }
      } else {
        body = body.push(hint(state, "player-unavailable-subtitles"));
      }
    }
  }
  panel(
    state,
    "player-subtitles",
    feedback(state, body, matches!(tools.secondary, ToolState::Failed)).into(),
    true,
  )
}

fn track_label(state: &State, track: &TrackInfo) -> String {
  match (track.title.as_deref(), track.language.as_deref()) {
    (Some(title), Some(language)) => format!("{title} · {language}"),
    (Some(title), None) => title.to_owned(),
    (None, Some(language)) => language.to_owned(),
    (None, None) => state
      .kernel
      .locale
      .format("player-track", &[("number", track.id.into())]),
  }
}

fn choice<'a>(
  state: &State,
  id: &str,
  label: String,
  selected: bool,
  enabled: bool,
  command: PlaybackToolAction,
  embedded: bool,
) -> Element<'a, Message> {
  let accent = state.palette().colors.secondary;
  control_button_content(
    move |_| {
      let marker: Element<'_, Message> = if selected {
        icon_with_color(Icon::Check, IconSize::Xs, accent).into()
      } else {
        space().width(14).into()
      };
      row![
        text(label.clone()).size(TOKENS.font_sizes.s13).width(Fill),
        marker
      ]
      .spacing(TOKENS.spacing.s2)
      .align_y(Alignment::Center)
      .into()
    },
    if embedded && selected {
      ButtonVariant::PillActive
    } else {
      ButtonVariant::Text
    },
  )
  .id(format!("playback-tools-{id}"))
  .width(Fill)
  .min_height(40.0)
  .padding([TOKENS.spacing.s2, TOKENS.spacing.s2_5])
  .on_press_maybe(enabled.then(|| execute(state, command)).flatten())
  .into()
}

type Scope = (SessionToken, Option<PlaybackFileToken>, u64);

fn scope(state: &State) -> Scope {
  (
    state.kernel.request_gate.current_session(),
    state.playback.view.tools.file,
    state.playback.tools.presentation,
  )
}

pub(super) fn guard<'a>(
  state: &State,
  content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
  guard_scope(scope(state), content)
}

pub(crate) fn guard_scope<'a, S: Clone + Eq + 'static>(
  scope: S,
  content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
  Element::new(Guard {
    scope,
    keys: Vec::new(),
    content: content.into(),
  })
}

fn stable_rows<'a>(
  state: &State,
  entries: Vec<(String, Element<'a, Message>)>,
) -> Element<'a, Message> {
  let (keys, children): (Vec<String>, Vec<Element<'a, Message>>) = entries.into_iter().unzip();
  Element::new(Guard {
    scope: scope(state),
    keys,
    content: Column::with_children(children)
      .width(Fill)
      .spacing(TOKENS.spacing.s1)
      .into(),
  })
}

struct Guard<'a, S> {
  scope: S,
  keys: Vec<String>,
  content: Element<'a, Message>,
}
struct GuardState<S> {
  keys: Vec<String>,
  pressed: Option<(S, Vec<String>)>,
}

impl<S: Clone + Eq + 'static> Widget<Message, Theme, iced::Renderer> for Guard<'_, S> {
  fn tag(&self) -> widget::tree::Tag {
    widget::tree::Tag::of::<GuardState<S>>()
  }
  fn state(&self) -> widget::tree::State {
    widget::tree::State::new(GuardState::<S> {
      keys: self.keys.clone(),
      pressed: None,
    })
  }
  fn diff(&mut self, tree: &mut widget::Tree) {
    if tree.children.is_empty() {
      tree.children.push(widget::Tree::new(&self.content));
    }
    let state = tree.state.downcast_mut::<GuardState<S>>();
    if state.keys != self.keys {
      // Preserve keyboard focus by track identity, including equal-length replacements/reorders.
      let mut previous: HashMap<_, _> = state
        .keys
        .iter()
        .cloned()
        .zip(std::mem::take(&mut tree.children[0].children))
        .collect();
      tree.children[0].children = self
        .keys
        .iter()
        .map(|key| previous.remove(key).unwrap_or_else(widget::Tree::empty))
        .collect();
      state.keys.clone_from(&self.keys);
    }
    self.content.as_widget_mut().diff(&mut tree.children[0]);
  }
  fn size(&self) -> Size<Length> {
    self.content.as_widget().size()
  }
  fn layout(
    &mut self,
    tree: &mut widget::Tree,
    renderer: &iced::Renderer,
    limits: &layout::Limits,
  ) -> layout::Node {
    self
      .content
      .as_widget_mut()
      .layout(&mut tree.children[0], renderer, limits)
  }
  fn operate(
    &mut self,
    tree: &mut widget::Tree,
    layout: Layout<'_>,
    renderer: &iced::Renderer,
    operation: &mut dyn widget::Operation,
  ) {
    self
      .content
      .as_widget_mut()
      .operate(&mut tree.children[0], layout, renderer, operation);
  }
  fn update(
    &mut self,
    tree: &mut widget::Tree,
    event: &Event,
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    renderer: &iced::Renderer,
    shell: &mut Shell<'_, Message>,
    viewport: &Rectangle,
  ) {
    let state = tree.state.downcast_mut::<GuardState<S>>();
    if matches!(
      event,
      Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        | Event::Touch(iced::touch::Event::FingerPressed { .. })
    ) {
      state.pressed = Some((self.scope.clone(), self.keys.clone()));
    }
    let retired = matches!(
      event,
      Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
        | Event::Touch(iced::touch::Event::FingerLifted { .. })
    ) && state
      .pressed
      .take()
      .is_some_and(|(scope, keys)| scope != self.scope || keys != self.keys);
    // A release with no hit clears the child's pressed state while retaining keyboard focus.
    self.content.as_widget_mut().update(
      &mut tree.children[0],
      event,
      layout,
      if retired {
        mouse::Cursor::Unavailable
      } else {
        cursor
      },
      renderer,
      shell,
      viewport,
    );
  }
  fn draw(
    &self,
    tree: &widget::Tree,
    renderer: &mut iced::Renderer,
    theme: &Theme,
    style: &renderer::Style,
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    viewport: &Rectangle,
  ) {
    self.content.as_widget().draw(
      &tree.children[0],
      renderer,
      theme,
      style,
      layout,
      cursor,
      viewport,
    );
  }
  fn mouse_interaction(
    &self,
    tree: &widget::Tree,
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    viewport: &Rectangle,
    renderer: &iced::Renderer,
  ) -> mouse::Interaction {
    self.content.as_widget().mouse_interaction(
      &tree.children[0],
      layout,
      cursor,
      viewport,
      renderer,
    )
  }
  fn overlay<'a>(
    &'a mut self,
    tree: &'a mut widget::Tree,
    layout: Layout<'a>,
    renderer: &iced::Renderer,
    viewport: &Rectangle,
    translation: Vector,
  ) -> Vec<overlay::Element<'a, Message, Theme, iced::Renderer>> {
    self.content.as_widget_mut().overlay(
      &mut tree.children[0],
      layout,
      renderer,
      viewport,
      translation,
    )
  }
}

#[cfg(test)]
mod tests {
  use super::super::viewing_queue::tests::{click, Bounds};
  use super::*;
  use iced::advanced::{renderer::Headless, widget::operation::focusable};
  use iced_runtime::user_interface::{Cache, UserInterface};
  use jellypilot_mpv::playback::tools::{
    AbLoopView, PlaybackToolsView, SecondarySubtitleView, SubtitleRole,
  };
  use jellypilot_mpv::playback::NowPlayingItem;
  use jellypilot_mpv::playback_session::NowPlayingView;

  fn track(id: i64, codec: Option<&str>, role: Option<SubtitleRole>) -> TrackInfo {
    TrackInfo {
      id,
      track_type: "sub".into(),
      title: Some(format!("Track {id}")),
      language: None,
      selected: role.is_some(),
      codec: codec.map(str::to_owned),
      subtitle_role: role,
      provider_index: None,
    }
  }

  fn state() -> State {
    let mut state = State::boot(false);
    state.shell.window_size = Size::new(600.0, 1000.0);
    state.playback.view.engine_available = true;
    state.playback.tools.open = true;
    state.playback.view.now_playing = Some(NowPlayingView {
      item: NowPlayingItem {
        item_id: "movie".into(),
        title: "A movie".into(),
        item_type: "Movie".into(),
        series_id: None,
        runtime_seconds: Some(300.0),
        start_position_seconds: 0.0,
        play_method: "DirectPlay".into(),
        original_language: None,
      },
      paused: true,
      position_seconds: 120.0,
      duration_seconds: Some(300.0),
      volume: 50.0,
      muted: false,
    });
    let tracks = vec![
      track(1, Some("ass"), Some(SubtitleRole::Primary)),
      track(2, Some("subrip"), None),
      track(3, Some("webvtt"), None),
    ];
    state.playback.view.tracks = TracksView::Ready {
      tracks: tracks.clone(),
      audio: None,
      subtitle: Some(1),
    };
    state.playback.view.tools = PlaybackToolsView {
      file: Some(PlaybackFileToken::default()),
      tracks,
      secondary: ToolState::Ready(SecondarySubtitleView {
        selected: None,
        eligible: true,
      }),
      ab_loop: ToolState::Ready(AbLoopView {
        a_seconds: Some(100.0),
        b_seconds: None,
        enabled: false,
        editable: true,
      }),
      busy: false,
      error: None,
    };
    state
  }

  async fn renderer() -> iced::Renderer {
    iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .unwrap()
  }

  fn events(
    ui: &mut UserInterface<'_, Message, Theme, iced::Renderer>,
    renderer: &mut iced::Renderer,
    events: &[Event],
    cursor: mouse::Cursor,
  ) -> Vec<Message> {
    let mut bus = iced::advanced::shell::Bus::new();
    ui.update(
      &iced::window::Headless,
      &iced::advanced::shell::Waker::noop(),
      events,
      cursor,
      renderer,
      &mut bus,
    );
    bus.drain().map(|(message, _)| message).collect()
  }

  fn commands(messages: Vec<Message>) -> Vec<PlaybackToolAction> {
    messages
      .into_iter()
      .filter_map(|message| match message {
        Message::Playback(PlaybackMessage::Tools(ToolsMessage::PanelExecute {
          action, ..
        })) => Some(action),
        _ => None,
      })
      .collect()
  }

  #[tokio::test]
  async fn short_narrow_panels_keep_close_and_scroll_within_their_surface() {
    let mut renderer = renderer().await;
    for language in [
      jellypilot_core::locale::UiLanguage::English,
      jellypilot_core::locale::UiLanguage::SimplifiedChinese,
    ] {
      for viewport in [
        Size::new(360.0, 320.0),
        Size::new(400.0, 480.0),
        Size::new(1280.0, 720.0),
      ] {
        for subtitles in [false, true] {
          let mut state = state();
          state.shell.window_size = viewport;
          state.kernel.locale = crate::i18n::Localizer::new(language);
          state.playback.tools.open = !subtitles;
          state.playback.subtitle_menu_open = subtitles;
          if let TracksView::Ready { tracks, .. } = &mut state.playback.view.tracks {
            for id in 4..20 {
              let mut track = track(id, Some("ass"), None);
              track.title = Some(format!("A long subtitle title that must wrap without clipping its selection or touch target {id}"));
              tracks.push(track);
            }
          }
          let mut ui = UserInterface::build(layer(&state), viewport, Cache::new(), &mut renderer);
          let mut bounds = Bounds::default();
          ui.operate(&renderer, &mut bounds);
          let content = bounds.get("playback-tools-content");
          let close = bounds.get("playback-tools-close");
          assert!(
            content.height + 2.0 * TOKENS.spacing.s3 <= (viewport.height - 64.0).min(640.0) + 0.1
          );
          assert!(content.width + 2.0 * TOKENS.spacing.s3 <= if subtitles { 240.1 } else { 320.1 });
          assert!(close.height >= 40.0 && close.width >= 40.0);
          assert!(close.y >= 16.0 && close.y + close.height <= viewport.height - 16.0);
          for (_, control) in &bounds.0 {
            assert!(
              control.x >= 15.9 && control.x + control.width <= viewport.width - 15.9,
              "horizontal overflow {control:?} at {viewport:?}"
            );
          }
          assert!(matches!(
            click(&mut ui, &mut renderer, close).as_slice(),
            [Message::Playback(PlaybackMessage::Tools(
              ToolsMessage::Close
            ))]
          ));
        }
      }
    }
  }

  #[tokio::test]
  async fn mark_actions_capture_real_position_and_disable_unknown_reversed_or_busy_intervals() {
    let mut renderer = renderer().await;
    let mut state = state();
    for (position, duration, busy, expected_a, expected_b) in [
      (120.0, Some(300.0), false, true, true),
      (90.0, Some(300.0), false, true, false),
      (301.0, Some(300.0), false, false, false),
      (120.0, None, false, false, false),
      (120.0, Some(300.0), true, false, false),
    ] {
      let current = state.playback.view.now_playing.as_mut().unwrap();
      current.position_seconds = position;
      current.duration_seconds = duration;
      state.playback.view.tools.busy = busy;
      let mut ui = UserInterface::build(
        loop_content(&state),
        Size::new(296.0, 900.0),
        Cache::new(),
        &mut renderer,
      );
      let mut bounds = Bounds::default();
      ui.operate(&renderer, &mut bounds);
      let a = commands(click(&mut ui, &mut renderer, bounds.get("ab-loop-mark-a")));
      let b = commands(click(&mut ui, &mut renderer, bounds.get("ab-loop-mark-b")));
      assert_eq!(
        a,
        if expected_a {
          vec![PlaybackToolAction::MarkA(position)]
        } else {
          Vec::new()
        }
      );
      assert_eq!(
        b,
        if expected_b {
          vec![PlaybackToolAction::MarkB(position)]
        } else {
          Vec::new()
        }
      );
    }
    // Losing seekability or duration cannot trap a previously configured loop.
    for (loop_enabled, busy) in [(true, false), (false, false), (true, true)] {
      state
        .playback
        .view
        .now_playing
        .as_mut()
        .unwrap()
        .duration_seconds = None;
      state.playback.view.tools.busy = busy;
      state.playback.view.tools.ab_loop = ToolState::Ready(AbLoopView {
        a_seconds: Some(100.0),
        b_seconds: Some(200.0),
        enabled: loop_enabled,
        editable: false,
      });
      let mut ui = UserInterface::build(
        loop_content(&state),
        Size::new(296.0, 900.0),
        Cache::new(),
        &mut renderer,
      );
      let mut bounds = Bounds::default();
      ui.operate(&renderer, &mut bounds);
      for id in ["ab-loop-mark-a", "ab-loop-mark-b", "ab-loop-restart"] {
        assert!(commands(click(&mut ui, &mut renderer, bounds.get(id))).is_empty());
      }
      let repeat = commands(click(
        &mut ui,
        &mut renderer,
        bounds.get(if loop_enabled {
          "ab-loop-disable"
        } else {
          "ab-loop-enable"
        }),
      ));
      assert_eq!(
        repeat,
        if loop_enabled && !busy {
          vec![PlaybackToolAction::SetLoopEnabled(false)]
        } else {
          Vec::new()
        }
      );
      let clear = commands(click(&mut ui, &mut renderer, bounds.get("ab-loop-clear")));
      assert_eq!(
        clear,
        if busy {
          Vec::new()
        } else {
          vec![PlaybackToolAction::ClearLoop]
        }
      );
    }
  }

  #[tokio::test]
  async fn subtitle_candidates_use_real_ids_and_unknown_secondary_state_has_no_off_selection() {
    let mut renderer = renderer().await;
    let mut state = state();
    let mut tracks = vec![
      track(1, Some("ass"), Some(SubtitleRole::Primary)),
      track(22, Some("srt"), None),
      track(33, Some("hdmv_pgs_subtitle"), None),
      track(44, None, None),
    ];
    state.playback.view.tracks = TracksView::Ready {
      tracks: tracks.clone(),
      audio: None,
      subtitle: Some(1),
    };
    // Inspect the content independently of scroll clipping, then exercise the actual row button.
    let row = choice(
      &state,
      "secondary-22",
      track_label(&state, &tracks[1]),
      false,
      available(&state),
      PlaybackToolAction::SelectSecondarySubtitle(Some(22)),
      false,
    );
    let mut ui = UserInterface::build(
      guard(&state, row),
      Size::new(216.0, 100.0),
      Cache::new(),
      &mut renderer,
    );
    let mut bounds = Bounds::default();
    ui.operate(&renderer, &mut bounds);
    assert!(
      matches!(click(&mut ui, &mut renderer, bounds.get("playback-tools-secondary-22")).as_slice(),
      [Message::Playback(PlaybackMessage::Tools(ToolsMessage::PanelExecute { session, file,
        action: PlaybackToolAction::SelectSecondarySubtitle(Some(22)), .. }))]
        if *session == state.kernel.request_gate.current_session() && Some(*file) == state.playback.view.tools.file)
    );
    drop(ui);
    for secondary in [
      ToolState::Ready(SecondarySubtitleView {
        selected: Some(99),
        eligible: true,
      }),
      ToolState::Loading,
      ToolState::Unavailable,
      ToolState::Failed,
    ] {
      let ready = matches!(secondary, ToolState::Ready(_));
      state.playback.view.tools.secondary = secondary;
      let mut ui = UserInterface::build(
        subtitle_content(&state, false),
        Size::new(216.0, 900.0),
        Cache::new(),
        &mut renderer,
      );
      let mut bounds = Bounds::default();
      ui.operate(&renderer, &mut bounds);
      assert_eq!(
        bounds
          .0
          .iter()
          .any(|(id, _)| *id == widget::Id::new("playback-tools-secondary-off")),
        ready
      );
      for id in [1, 33, 44, 99] {
        assert!(!bounds
          .0
          .iter()
          .any(|(candidate, _)| *candidate
            == widget::Id::from(format!("playback-tools-secondary-{id}"))));
      }
    }
    tracks[0].codec = Some("hdmv_pgs_subtitle".into());
    tracks.truncate(2);
    state.playback.view.tracks = TracksView::Ready {
      tracks,
      audio: None,
      subtitle: Some(1),
    };
    state.playback.view.tools.secondary = ToolState::Ready(SecondarySubtitleView {
      selected: Some(22),
      eligible: false,
    });
    let mut ui = UserInterface::build(
      subtitle_content(&state, false),
      Size::new(216.0, 900.0),
      Cache::new(),
      &mut renderer,
    );
    let mut bounds = Bounds::default();
    ui.operate(&renderer, &mut bounds);
    assert!(!bounds
      .0
      .iter()
      .any(|(id, _)| *id == widget::Id::new("playback-tools-secondary-22")));
    drop(ui);
    for busy in [false, true] {
      state.playback.view.tools.busy = busy;
      let mut ui = UserInterface::build(
        subtitle_content(&state, false),
        Size::new(216.0, 900.0),
        Cache::new(),
        &mut renderer,
      );
      ui.operate(
        &renderer,
        &mut widget::operation::scrollable::scroll_to::<()>(
          widget::Id::new("playback-tools-scroll"),
          widget::operation::scrollable::AbsoluteOffset {
            x: None,
            y: Some(1000.0),
          },
        ),
      );
      ui.operate(
        &renderer,
        &mut focusable::focus::<()>(widget::Id::new("playback-tools-secondary-off")),
      );
      let messages = commands(events(
        &mut ui,
        &mut renderer,
        &[Event::Keyboard(iced::keyboard::Event::KeyPressed {
          key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
          modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
          physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Enter),
          location: iced::keyboard::Location::Standard,
          modifiers: iced::keyboard::Modifiers::NONE,
          text: None,
          repeat: false,
        })],
        mouse::Cursor::Unavailable,
      ));
      if busy {
        assert!(messages.is_empty());
      } else {
        assert_eq!(
          messages,
          vec![PlaybackToolAction::SelectSecondarySubtitle(None)]
        );
      }
    }
  }

  #[tokio::test]
  async fn mouse_and_touch_release_cannot_cross_file_or_account_identity() {
    let mut renderer = renderer().await;
    for touch in [false, true] {
      for retirement in 0..3 {
        let mut state = state();
        let mut ui = UserInterface::build(
          action(
            &state,
            "playback-tools-refresh",
            PlaybackToolAction::Refresh,
            true,
          ),
          Size::new(300.0, 100.0),
          Cache::new(),
          &mut renderer,
        );
        let mut bounds = Bounds::default();
        ui.operate(&renderer, &mut bounds);
        let point = bounds.get("playback-tools-refresh").center();
        let press = if touch {
          Event::Touch(iced::touch::Event::FingerPressed {
            id: iced::touch::Finger(1),
            position: point,
          })
        } else {
          Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        };
        assert!(events(
          &mut ui,
          &mut renderer,
          &[press],
          mouse::Cursor::Available(point)
        )
        .is_empty());
        let cache = ui.into_cache();
        match retirement {
          0 => state.playback.view.tools.file = Some(PlaybackFileToken::default()),
          1 => {
            state.kernel.request_gate.disconnect();
          }
          _ => {
            // No intermediate widget build: reusing the old cache must still retire the press.
            for message in [ToolsMessage::Close, ToolsMessage::OpenLoop] {
              drop(crate::app::playback::update(
                &mut state.playback,
                &mut state.kernel,
                false,
                PlaybackMessage::Tools(message),
              ));
            }
          }
        }
        let mut ui = UserInterface::build(
          action(
            &state,
            "playback-tools-refresh",
            PlaybackToolAction::Refresh,
            true,
          ),
          Size::new(300.0, 100.0),
          cache,
          &mut renderer,
        );
        let release = if touch {
          Event::Touch(iced::touch::Event::FingerLifted {
            id: iced::touch::Finger(1),
            position: point,
          })
        } else {
          Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
        };
        assert!(events(
          &mut ui,
          &mut renderer,
          &[release],
          mouse::Cursor::Available(point)
        )
        .is_empty());
        assert_eq!(
          commands(click(
            &mut ui,
            &mut renderer,
            bounds.get("playback-tools-refresh")
          )),
          vec![PlaybackToolAction::Refresh]
        );
      }
    }
  }

  fn primary_rows(state: &State, ids: &[i64]) -> Element<'static, Message> {
    stable_rows(
      state,
      ids
        .iter()
        .map(|id| {
          let key = format!("primary-{id}");
          let row = choice(
            state,
            &key,
            format!("Track {id}"),
            false,
            available(state),
            PlaybackToolAction::SelectPrimarySubtitle(Some(*id)),
            false,
          );
          (key, row)
        })
        .collect(),
    )
  }

  #[tokio::test]
  async fn same_file_track_replacement_cancels_old_release_and_stable_ids_retain_focus() {
    let mut renderer = renderer().await;
    let state = state();
    let mut ui = UserInterface::build(
      primary_rows(&state, &[1, 2, 3]),
      Size::new(216.0, 300.0),
      Cache::new(),
      &mut renderer,
    );
    let mut bounds = Bounds::default();
    ui.operate(&renderer, &mut bounds);
    let point = bounds.get("playback-tools-primary-2").center();
    assert!(events(
      &mut ui,
      &mut renderer,
      &[Event::Mouse(mouse::Event::ButtonPressed(
        mouse::Button::Left
      ))],
      mouse::Cursor::Available(point)
    )
    .is_empty());
    let mut ui = UserInterface::build(
      primary_rows(&state, &[1, 3, 4]),
      Size::new(216.0, 300.0),
      ui.into_cache(),
      &mut renderer,
    );
    assert!(events(
      &mut ui,
      &mut renderer,
      &[Event::Mouse(mouse::Event::ButtonReleased(
        mouse::Button::Left
      ))],
      mouse::Cursor::Available(point)
    )
    .is_empty());
    ui.operate(
      &renderer,
      &mut focusable::focus::<()>(widget::Id::new("playback-tools-primary-3")),
    );
    let mut ui = UserInterface::build(
      primary_rows(&state, &[4, 1, 3]),
      Size::new(216.0, 300.0),
      ui.into_cache(),
      &mut renderer,
    );
    let mut bounds = Bounds::default();
    ui.operate(&renderer, &mut bounds);
    assert_eq!(bounds.1, Some(widget::Id::new("playback-tools-primary-3")));
  }

  #[tokio::test]
  async fn pending_tools_disable_actions_without_discarding_keyboard_focus() {
    let mut renderer = renderer().await;
    let mut state = state();
    let mut ui = UserInterface::build(
      primary_rows(&state, &[1, 2]),
      Size::new(216.0, 300.0),
      Cache::new(),
      &mut renderer,
    );
    ui.operate(
      &renderer,
      &mut focusable::focus::<()>(widget::Id::new("playback-tools-primary-2")),
    );
    let cache = ui.into_cache();
    state.playback.view.tools.busy = true;
    let mut ui = UserInterface::build(
      primary_rows(&state, &[1, 2]),
      Size::new(216.0, 300.0),
      cache,
      &mut renderer,
    );
    let mut bounds = Bounds::default();
    ui.operate(&renderer, &mut bounds);
    assert_eq!(bounds.1, Some(widget::Id::new("playback-tools-primary-2")));
    assert!(click(
      &mut ui,
      &mut renderer,
      bounds.get("playback-tools-primary-2")
    )
    .is_empty());
  }
}
