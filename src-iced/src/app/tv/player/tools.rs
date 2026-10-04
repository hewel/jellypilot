//! TV navigation and presentation of the session's observed current-file tools.

use super::*;
use iced::widget::{column, scrollable};
use jellypilot_mpv::playback::tools::{
  PlaybackFileToken, PlaybackToolAction, SubtitleRole, ToolState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Action {
  Open(Panel),
  Back,
  Primary(Option<i64>),
  Secondary(Option<i64>),
  MarkA,
  MarkB,
  ToggleLoop,
  Restart,
  Clear,
  Refresh,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Scope {
  session: SessionToken,
  generation: u64,
  presentation: Option<Instant>,
  panel_presentation: Option<Instant>,
  file: Option<PlaybackFileToken>,
  panel: Option<Panel>,
  revision: u64,
}

#[derive(Clone, Debug)]
pub struct Message {
  scope: Scope,
  action: Action,
  command: Option<PlaybackToolAction>,
}

fn scope(state: &State) -> Scope {
  let panel = state.tv.player.remote.panel().map(|panel| panel.kind);
  Scope {
    session: state.kernel.request_gate.current_session(),
    generation: state.playback.view.lifecycle.replacement_generation,
    presentation: state.tv.player.presentation,
    panel_presentation: state.tv.player.panel_presentation,
    file: state.playback.view.tools.file,
    panel,
    revision: panel.map_or(0, |panel| choices_revision(state, panel)),
  }
}

pub(super) fn is_panel(panel: Panel) -> bool {
  matches!(
    panel,
    Panel::Subtitles | Panel::PrimarySubtitles | Panel::SecondarySubtitles | Panel::Loop
  )
}

pub(super) fn hash_choices(state: &State, hash: &mut impl Hasher) {
  let tools = &state.playback.view.tools;
  tools.busy.hash(hash);
  std::mem::discriminant(&tools.error).hash(hash);
  std::mem::discriminant(&state.playback.view.tracks).hash(hash);
  if let TracksView::Ready {
    tracks, subtitle, ..
  } = &state.playback.view.tracks
  {
    subtitle.hash(hash);
    for track in tracks {
      (
        &track.id,
        &track.track_type,
        &track.title,
        &track.language,
        &track.codec,
      )
        .hash(hash);
      track.selected.hash(hash);
      std::mem::discriminant(&track.subtitle_role).hash(hash);
      if let Some(role) = track.subtitle_role {
        std::mem::discriminant(&role).hash(hash);
      }
    }
  }
  for track in &tools.tracks {
    (
      &track.id,
      &track.track_type,
      &track.title,
      &track.language,
      &track.codec,
    )
      .hash(hash);
    if let Some(role) = track.subtitle_role {
      std::mem::discriminant(&role).hash(hash);
    }
  }
  std::mem::discriminant(&tools.secondary).hash(hash);
  if let ToolState::Ready(secondary) = &tools.secondary {
    secondary.selected.hash(hash);
    secondary.eligible.hash(hash);
  }
  std::mem::discriminant(&tools.ab_loop).hash(hash);
  if let ToolState::Ready(ab) = &tools.ab_loop {
    ab.a_seconds.map(f64::to_bits).hash(hash);
    ab.b_seconds.map(f64::to_bits).hash(hash);
    ab.enabled.hash(hash);
    ab.editable.hash(hash);
  }
}

fn choice(label: String, action: Action) -> Choice {
  Choice {
    label,
    action: ChoiceAction::Tool(action),
  }
}

pub(super) fn choices(state: &State, panel: Panel) -> Vec<Choice> {
  if panel == Panel::Subtitles {
    return vec![
      choice(
        state.t("subtitle-primary"),
        Action::Open(Panel::PrimarySubtitles),
      ),
      choice(
        state.t("subtitle-secondary"),
        Action::Open(Panel::SecondarySubtitles),
      ),
    ];
  }
  let mut rows = vec![choice(state.t("common-back"), Action::Back)];
  match panel {
    Panel::PrimarySubtitles => {
      if let TracksView::Ready { tracks, .. } = &state.playback.view.tracks {
        rows.push(choice(
          state.t("tv-player-subtitles-off"),
          Action::Primary(None),
        ));
        rows.extend(
          tracks
            .iter()
            .filter(|track| track.track_type == "sub")
            .map(|track| choice(track_label(state, track), Action::Primary(Some(track.id)))),
        );
      }
    }
    Panel::SecondarySubtitles => {
      if let ToolState::Ready(secondary) = &state.playback.view.tools.secondary {
        rows.push(choice(
          state.t("tv-player-subtitles-off"),
          Action::Secondary(None),
        ));
        if secondary.eligible {
          rows.extend(
            state
              .playback
              .view
              .tools
              .tracks
              .iter()
              .filter(|track| {
                track.is_text_subtitle() && track.subtitle_role != Some(SubtitleRole::Primary)
              })
              .map(|track| choice(track_label(state, track), Action::Secondary(Some(track.id)))),
          );
        }
      }
    }
    Panel::Loop => {
      rows.extend([
        choice(state.t("ab-loop-mark-a"), Action::MarkA),
        choice(state.t("ab-loop-mark-b"), Action::MarkB),
        choice(state.t("ab-loop-enable"), Action::ToggleLoop),
        choice(state.t("ab-loop-restart"), Action::Restart),
        choice(state.t("ab-loop-clear"), Action::Clear),
      ]);
    }
    _ => {}
  }
  rows.push(choice(state.t("playback-tools-refresh"), Action::Refresh));
  rows
}

fn position(state: &State) -> Option<f64> {
  let playing = state.playback.view.now_playing.as_ref()?;
  let duration = playing
    .duration_seconds
    .filter(|value| value.is_finite() && *value > 0.0)?;
  let position = playing.position_seconds;
  (position.is_finite() && position >= 0.0 && position <= duration).then_some(position)
}

pub(super) fn enabled(state: &State, action: Action) -> bool {
  if matches!(action, Action::Open(_) | Action::Back) {
    return true;
  }
  let tools = &state.playback.view.tools;
  if observation(state).busy || tools.busy {
    return false;
  }
  match action {
    Action::Primary(id) => matches!(&state.playback.view.tracks, TracksView::Ready {tracks, ..}
      if id.is_none_or(|id| tracks.iter().any(|track| track.track_type == "sub" && track.id == id))),
    Action::Secondary(id) => {
      tools.file.is_some()
        && matches!(&tools.secondary, ToolState::Ready(secondary)
      if id.is_none() || (secondary.eligible && tools.tracks.iter().any(|track| Some(track.id) == id
        && track.is_text_subtitle() && track.subtitle_role != Some(SubtitleRole::Primary))))
    }
    Action::Refresh => tools.file.is_some(),
    Action::MarkA | Action::MarkB | Action::ToggleLoop | Action::Restart | Action::Clear => {
      if tools.file.is_none() {
        return false;
      }
      let ToolState::Ready(ab) = &tools.ab_loop else {
        return false;
      };
      match action {
        Action::MarkA => ab.editable && position(state).is_some(),
        Action::MarkB => {
          ab.editable
            && ab
              .a_seconds
              .zip(position(state))
              .is_some_and(|(a, position)| position > a)
        }
        Action::ToggleLoop => {
          ab.enabled || (ab.editable && ab.a_seconds.zip(ab.b_seconds).is_some_and(|(a, b)| b > a))
        }
        Action::Restart => ab.editable && ab.a_seconds.is_some(),
        Action::Clear => ab.a_seconds.is_some() || ab.b_seconds.is_some(),
        _ => false,
      }
    }
    Action::Open(_) | Action::Back => true,
  }
}

pub(super) fn selected(state: &State, action: Action) -> bool {
  match action {
    Action::Primary(id) => {
      matches!(&state.playback.view.tracks, TracksView::Ready {subtitle, ..} if *subtitle == id)
    }
    Action::Secondary(id) => {
      matches!(&state.playback.view.tools.secondary, ToolState::Ready(secondary) if secondary.selected == id)
    }
    Action::ToggleLoop => {
      matches!(&state.playback.view.tools.ab_loop, ToolState::Ready(ab) if ab.enabled)
    }
    _ => false,
  }
}

pub(super) fn event(state: &State, action: Action) -> Message {
  let command = match action {
    Action::Primary(id) => Some(PlaybackToolAction::SelectPrimarySubtitle(id)),
    Action::Secondary(id) => Some(PlaybackToolAction::SelectSecondarySubtitle(id)),
    Action::MarkA => position(state).map(PlaybackToolAction::MarkA),
    Action::MarkB => position(state).map(PlaybackToolAction::MarkB),
    Action::ToggleLoop => Some(PlaybackToolAction::SetLoopEnabled(!selected(
      state,
      Action::ToggleLoop,
    ))),
    Action::Restart => Some(PlaybackToolAction::RestartFromA),
    Action::Clear => Some(PlaybackToolAction::ClearLoop),
    Action::Refresh => Some(PlaybackToolAction::Refresh),
    Action::Open(_) | Action::Back => None,
  };
  Message {
    scope: scope(state),
    action,
    command,
  }
}

pub(super) fn update(state: &mut State, event: Message) -> Task<AppMessage> {
  if !input_admitted(state) || event.scope != scope(state) || !enabled(state, event.action) {
    return Task::none();
  }
  let Some(index) = state.tv.player.choices.iter().position(
    |choice| matches!(choice.action, ChoiceAction::Tool(action) if action == event.action),
  ) else {
    return Task::none();
  };
  state.tv.player.remote.focus_choice(index);
  match event.action {
    Action::Open(panel) => {
      let choices = panel_choices(state, panel);
      let selected = choices
        .iter()
        .position(|choice| choice_selected(state, &choice.action))
        .unwrap_or(0);
      state
        .tv
        .player
        .remote
        .open_subpanel(panel, selected, choices.len(), Instant::now());
      state.tv.player.choices_revision = choices_revision(state, panel);
      state.tv.player.choices_panel = Some(panel);
      state.tv.player.panel_presentation = Some(Instant::now());
      state.tv.player.choices = choices;
      reveal(state)
    }
    Action::Back => {
      let previous = state.tv.player.remote.panel().map(|panel| panel.kind);
      state.tv.player.remote.back_panel(Instant::now());
      let sync = synchronize_panel(state, previous);
      Task::batch([sync, reveal(state)])
    }
    Action::Primary(id) if event.scope.file.is_none() => command(
      state,
      AppMessage::Playback(PlaybackMessage::SubtitleTrackSelected(id)),
    ),
    _ => {
      let Some((file, action)) = event.scope.file.zip(event.command) else {
        return Task::none();
      };
      command(
        state,
        AppMessage::Playback(PlaybackMessage::Tools(playback::tools::Message::Execute {
          session: event.scope.session,
          file,
          action,
        })),
      )
    }
  }
}

pub(super) fn reveal(state: &State) -> Task<AppMessage> {
  let Some(panel) = state.tv.player.remote.panel() else {
    return Task::none();
  };
  if !is_panel(panel.kind) && panel.kind != Panel::Settings {
    return Task::none();
  }
  if state
    .tv
    .player
    .choices
    .get(panel.focused)
    .is_none_or(|choice| matches!(choice.action, ChoiceAction::Close))
  {
    return Task::none();
  }
  let offset = if panel.kind == Panel::Settings {
    match panel.focused {
      0..=4 => 0.0,
      5 => 144.0,
      _ => 256.0,
    }
  } else {
    panel.focused as f32 * 88.0
  };
  iced::widget::operation::scroll_to(
    "tv-player-tools-body",
    iced::widget::operation::AbsoluteOffset {
      x: 0.0,
      y: offset * style::scale(state.shell.window_size.width),
    },
  )
}

fn role_value(state: &State, secondary: bool) -> String {
  let id = if secondary {
    match &state.playback.view.tools.secondary {
      ToolState::Ready(view) => view.selected,
      ToolState::Loading => return state.t("common-loading"),
      ToolState::Failed => return state.t("playback-tools-failed"),
      ToolState::Unavailable => return state.t("playback-tools-unavailable"),
    }
  } else {
    match &state.playback.view.tracks {
      TracksView::Ready { subtitle, .. } => *subtitle,
      TracksView::Loading => return state.t("common-loading"),
      TracksView::Unavailable => return state.t("playback-tools-unavailable"),
    }
  };
  let Some(id) = id else {
    return state.t("tv-player-subtitles-off");
  };
  if let TracksView::Ready { tracks, .. } = &state.playback.view.tracks {
    if let Some(track) = tracks.iter().find(|track| track.id == id) {
      return track_label(state, track);
    }
  }
  state.t("playback-tools-unavailable")
}

fn label(state: &State, choice: &Choice, action: Action) -> String {
  match action {
    Action::Open(Panel::PrimarySubtitles) => {
      format!("{} · {}", choice.label, role_value(state, false))
    }
    Action::Open(Panel::SecondarySubtitles) => {
      format!("{} · {}", choice.label, role_value(state, true))
    }
    Action::MarkA | Action::MarkB => {
      let point = match &state.playback.view.tools.ab_loop {
        ToolState::Ready(ab) => {
          if action == Action::MarkA {
            ab.a_seconds
          } else {
            ab.b_seconds
          }
        }
        _ => None,
      };
      format!(
        "{} · {}",
        choice.label,
        point.map(format_duration).unwrap_or_else(|| "—".into())
      )
    }
    Action::ToggleLoop => state.t(if selected(state, action) {
      "ab-loop-disable"
    } else {
      "ab-loop-enable"
    }),
    _ => choice.label.clone(),
  }
}

fn status(state: &State, panel: Panel) -> Vec<String> {
  let tools = &state.playback.view.tools;
  let mut keys = Vec::new();
  if tools.busy {
    keys.push("common-loading");
  }
  if tools.error.is_some() {
    keys.push("playback-tools-failed");
  }
  match panel {
    Panel::Subtitles => keys.push("subtitle-secondary-help"),
    Panel::PrimarySubtitles => match state.playback.view.tracks {
      TracksView::Loading => keys.push("common-loading"),
      TracksView::Unavailable => keys.push("playback-tools-unavailable"),
      _ => {}
    },
    Panel::SecondarySubtitles => {
      match &tools.secondary {
        ToolState::Loading => keys.push("common-loading"),
        ToolState::Unavailable => keys.push("playback-tools-unavailable"),
        ToolState::Failed => keys.push("playback-tools-failed"),
        ToolState::Ready(secondary) if !secondary.eligible => {
          keys.push("subtitle-secondary-requires-primary")
        }
        ToolState::Ready(_) => {
          if !tools.tracks.iter().any(|track| {
            track.is_text_subtitle() && track.subtitle_role != Some(SubtitleRole::Primary)
          }) {
            keys.push("subtitle-secondary-empty");
          }
        }
      }
      keys.push("subtitle-secondary-help");
    }
    Panel::Loop => {
      match &tools.ab_loop {
        ToolState::Loading => keys.push("common-loading"),
        ToolState::Unavailable => keys.push("ab-loop-unavailable"),
        ToolState::Failed => keys.push("playback-tools-failed"),
        ToolState::Ready(ab) => {
          if !ab.editable {
            keys.push("ab-loop-unavailable");
          }
          if ab.enabled {
            keys.push("ab-loop-configured");
          } else if ab.a_seconds.is_some() && ab.b_seconds.is_some() {
            keys.push("ab-loop-disabled");
          }
          if !enabled(state, Action::MarkB) {
            keys.push("ab-loop-mark-b-help");
          }
        }
      }
      keys.push("ab-loop-help");
    }
    _ => {}
  }
  keys.dedup();
  keys.into_iter().map(|key| state.t(key)).collect()
}

fn control<'a>(
  state: &'a State,
  index: usize,
  choice: &'a Choice,
  action: Action,
  scale: f32,
) -> Element<'a, AppMessage> {
  let focused = state
    .tv
    .player
    .remote
    .panel()
    .is_some_and(|panel| panel.focused == index);
  let selected = selected(state, action);
  let enabled = enabled(state, action);
  let event = event(state, action);
  let label = label(state, choice, action);
  container(focus(focused, move |progress| {
    let color = if enabled || focused {
      style::foreground(style::PALETTE, progress, selected)
    } else {
      style::PALETTE.text.muted
    };
    button(
      row![
        icon_with_color(
          if selected {
            Icon::CircleDot
          } else {
            Icon::Circle
          },
          IconSize::Custom(28.0 * scale),
          color
        ),
        ellipsis_text(label.clone())
          .size(style::BODY * scale)
          .color(color),
      ]
      .spacing(20.0 * scale)
      .align_y(Alignment::Center),
    )
    .width(Fill)
    .height(style::CONTROL * scale)
    .padding([12.0 * scale, 20.0 * scale])
    .style(move |theme, status| {
      chrome::choice(progress, selected, false)(
        theme,
        if status == button::Status::Disabled && progress > 0.0 {
          button::Status::Active
        } else {
          status
        },
      )
    })
    .on_press_maybe(enabled.then(|| message(super::Message::Tools(event.clone()))))
    .into()
  }))
  .id(iced::widget::Id::from(format!("tv-player-tool-{action:?}")))
  .into()
}

pub(super) fn view(
  state: &State,
  panel: Panel,
  bounds: Size,
  scale: f32,
) -> Element<'_, AppMessage> {
  let title = match panel {
    Panel::Subtitles => "player-subtitles",
    Panel::PrimarySubtitles => "subtitle-primary",
    Panel::SecondarySubtitles => "subtitle-secondary",
    Panel::Loop => "ab-loop-title",
    _ => "tv-player-settings",
  };
  let mut body = Column::new().spacing(24.0 * scale);
  for (index, choice) in state.tv.player.choices.iter().enumerate() {
    if let ChoiceAction::Tool(action) = choice.action {
      body = body.push(control(state, index, choice, action, scale));
    }
  }
  for hint in status(state, panel) {
    body = body.push(
      text(hint)
        .size(style::META * scale)
        .line_height(iced::Pixels(28.0 * scale))
        .color(style::PALETTE.text.body),
    );
  }
  let body = crate::app::view::playback_tools::guard_scope(scope(state), body);
  frame(state, state.t(title), body, bounds, scale)
}

pub(super) fn settings_view(state: &State, bounds: Size, scale: f32) -> Element<'_, AppMessage> {
  let focused = state
    .tv
    .player
    .remote
    .panel()
    .map_or(0, |panel| panel.focused);
  let mut body = column![text(state.t("tv-player-speed"))
    .size(style::BODY * scale)
    .line_height(iced::Pixels(32.0 * scale))]
  .spacing(24.0 * scale);
  let speeds = state
    .tv
    .player
    .choices
    .iter()
    .take(5)
    .enumerate()
    .fold(row![], |row, (index, choice)| {
      row.push(choice_button(state, index, choice, focused, scale))
    });
  body = body.push(speeds.spacing(12.0 * scale));
  for (index, choice) in state.tv.player.choices.iter().enumerate().skip(5) {
    match choice.action {
      ChoiceAction::Tool(action) => body = body.push(control(state, index, choice, action, scale)),
      ChoiceAction::Close => {}
      _ => body = body.push(choice_button(state, index, choice, focused, scale)),
    }
  }
  body = body.push(
    text(
      state.t(if choice_enabled(state, &ChoiceAction::SessionSkip) {
        "tv-player-session-skip-hint"
      } else {
        "tv-player-session-skip-unavailable"
      }),
    )
    .size(style::META * scale)
    .line_height(iced::Pixels(28.0 * scale))
    .color(style::PALETTE.text.body),
  );
  frame(
    state,
    state.t("tv-player-settings"),
    body.into(),
    bounds,
    scale,
  )
}

fn frame<'a>(
  state: &'a State,
  title: String,
  body: Element<'a, AppMessage>,
  bounds: Size,
  scale: f32,
) -> Element<'a, AppMessage> {
  let focused = state
    .tv
    .player
    .remote
    .panel()
    .map_or(0, |panel| panel.focused);
  let close = state
    .tv
    .player
    .choices
    .iter()
    .position(|choice| matches!(choice.action, ChoiceAction::Close))
    .unwrap_or(0);
  let compact = bounds.height < (360.0 + 2.0 * style::SAFE_Y) * scale;
  let padding = if compact { 16.0 } else { 40.0 } * scale;
  let gap = if compact { 12.0 } else { 24.0 } * scale;
  let available = (bounds.height - 2.0 * style::SAFE_Y * scale).max(1.0);
  let body_height = (available - padding * 2.0 - style::CONTROL * scale - gap).max(1.0);
  let header = row![
    ellipsis_text(title)
      .font(HEADING_FONT)
      .size(style::TITLE * scale),
    space().width(Fill),
    panel_close_button(close, focused, scale),
  ]
  .spacing(16.0 * scale)
  .align_y(Alignment::Center);
  let content = column![
    header,
    container(
      scrollable(body)
        .id("tv-player-tools-body")
        .height(Length::Fit.max(body_height))
    )
    .id("tv-player-tools-viewport")
  ]
  .spacing(gap);
  container(
    container(content)
      .id("tv-player-panel")
      .padding(padding)
      .width((744.0 * scale).min((bounds.width - 2.0 * style::SAFE_X * scale).max(1.0)))
      .height(Length::Fit.max(available))
      .style(chrome::panel),
  )
  .width(Fill)
  .height(Fill)
  .align_x(Alignment::End)
  .padding([style::SAFE_Y * scale, style::SAFE_X * scale])
  .into()
}

#[cfg(test)]
mod tests {
  use super::*;
  use jellypilot_mpv::playback::tools::{AbLoopView, SecondarySubtitleView};

  fn state() -> State {
    let mut state = super::super::tests::playing_state();
    let track = |id, codec: Option<&str>, role: Option<SubtitleRole>| TrackInfo {
      id,
      track_type: "sub".into(),
      title: Some(format!("Subtitle {id}")),
      language: None,
      selected: role.is_some(),
      provider_index: None,
      codec: codec.map(str::to_owned),
      subtitle_role: role,
    };
    let tracks = vec![
      track(1, Some("ass"), Some(SubtitleRole::Primary)),
      track(2, Some("subrip"), Some(SubtitleRole::Secondary)),
      track(3, Some("hdmv_pgs_subtitle"), None),
      track(4, None, None),
      track(5, Some("webvtt"), None),
    ];
    state.playback.view.tracks = TracksView::Ready {
      tracks: tracks.clone(),
      audio: None,
      subtitle: Some(1),
    };
    state.playback.view.tools.file = Some(PlaybackFileToken::default());
    state.playback.view.tools.tracks = tracks;
    state.playback.view.tools.secondary = ToolState::Ready(SecondarySubtitleView {
      selected: Some(2),
      eligible: true,
    });
    state.playback.view.tools.ab_loop = ToolState::Ready(AbLoopView {
      a_seconds: None,
      b_seconds: None,
      enabled: false,
      editable: true,
    });
    state
  }

  fn open(state: &mut State, root: Panel, child: Panel) {
    drop(open_panel(state, root));
    let event = event(state, Action::Open(child));
    drop(update(state, event));
  }

  fn focused_action(state: &State) -> &ChoiceAction {
    &state.tv.player.choices[state.tv.player.remote.panel().unwrap().focused].action
  }

  #[test]
  fn subtitle_roles_use_real_text_candidates_and_keep_parent_focus_while_busy() {
    let mut state = state();
    open(&mut state, Panel::Subtitles, Panel::SecondarySubtitles);
    let candidates: Vec<_> = state
      .tv
      .player
      .choices
      .iter()
      .filter_map(|choice| {
        if let ChoiceAction::Tool(Action::Secondary(id)) = choice.action {
          Some(id)
        } else {
          None
        }
      })
      .collect();
    assert_eq!(candidates, vec![None, Some(2), Some(5)]);
    assert!(matches!(
      focused_action(&state),
      ChoiceAction::Tool(Action::Secondary(Some(2)))
    ));
    state.playback.view.tools.busy = true;
    drop(reconcile(&mut state));
    assert!(matches!(
      focused_action(&state),
      ChoiceAction::Tool(Action::Secondary(Some(2)))
    ));
    assert!(!enabled(&state, Action::Secondary(Some(5))));
    drop(super::super::input(&mut state, Input::PlayPause));
    drop(super::super::input(&mut state, Input::Back));
    assert!(matches!(
      focused_action(&state),
      ChoiceAction::Tool(Action::Open(Panel::SecondarySubtitles))
    ));
    assert!(state.playback.view.now_playing.as_ref().unwrap().paused);
    drop(super::super::update(
      &mut state,
      super::super::Message::ClosePanel,
    ));
    assert!(state.tv.player.remote.panel().is_none());
  }

  #[test]
  fn arriving_and_removed_subtitles_do_not_turn_unknown_into_off_or_retarget_focus() {
    let mut state = state();
    state.playback.view.tools.secondary = ToolState::Loading;
    open(&mut state, Panel::Subtitles, Panel::SecondarySubtitles);
    assert!(!state
      .tv
      .player
      .choices
      .iter()
      .any(|choice| matches!(choice.action, ChoiceAction::Tool(Action::Secondary(_)))));
    assert!(!selected(&state, Action::Secondary(None)));
    assert_eq!(role_value(&state, true), state.t("common-loading"));
    state.playback.view.tools.secondary = ToolState::Failed;
    assert_eq!(role_value(&state, true), state.t("playback-tools-failed"));
    state.playback.view.tools.secondary = ToolState::Ready(SecondarySubtitleView {
      selected: Some(2),
      eligible: true,
    });
    drop(reconcile(&mut state));
    let index = state
      .tv
      .player
      .choices
      .iter()
      .position(|choice| {
        matches!(
          choice.action,
          ChoiceAction::Tool(Action::Secondary(Some(2)))
        )
      })
      .unwrap();
    state.tv.player.remote.focus_choice(index);
    state
      .playback
      .view
      .tools
      .tracks
      .retain(|track| track.id != 2);
    state.playback.view.tools.secondary = ToolState::Ready(SecondarySubtitleView {
      selected: None,
      eligible: true,
    });
    drop(reconcile(&mut state));
    assert!(matches!(focused_action(&state), ChoiceAction::Close));
    assert!(!enabled(&state, Action::Secondary(Some(2))));
  }

  #[tokio::test]
  async fn marks_capture_displayed_position_and_reject_retired_media_or_presentation() {
    let mut state = state();
    open(&mut state, Panel::Settings, Panel::Loop);
    let mark = event(&state, Action::MarkA);
    state
      .playback
      .view
      .now_playing
      .as_mut()
      .unwrap()
      .position_seconds = 30.0;
    let dispatched = super::super::tests::dispatched(update(&mut state, mark)).await;
    let super::super::Message::Dispatch { command, .. } = dispatched else {
      panic!("TV dispatch");
    };
    assert!(matches!(
      *command,
      AppMessage::Playback(PlaybackMessage::Tools(playback::tools::Message::Execute {
        action: PlaybackToolAction::MarkA(15.0),
        ..
      }))
    ));
    assert!(state.tv.player.remote.panel().is_some());
    for boundary in 0..4 {
      let old = event(&state, Action::MarkA);
      match boundary {
        0 => state.playback.view.tools.file = Some(PlaybackFileToken::default()),
        1 => state.tv.player.presentation = Some(Instant::now()),
        2 => state.playback.view.lifecycle.replacement_generation += 1,
        _ => state.kernel.request_gate.disconnect(),
      }
      assert_eq!(update(&mut state, old).units(), 0);
    }
    state.playback.view.tools.ab_loop = ToolState::Unavailable;
    assert!(!enabled(&state, Action::MarkA));
    state.playback.view.tools.ab_loop = ToolState::Ready(AbLoopView {
      a_seconds: Some(40.0),
      b_seconds: None,
      enabled: false,
      editable: true,
    });
    assert!(!enabled(&state, Action::MarkB));
    assert!(!enabled(&state, Action::ToggleLoop));
    assert!(enabled(&state, Action::Restart));
    state.playback.view.tools.ab_loop = ToolState::Ready(AbLoopView {
      a_seconds: Some(10.0),
      b_seconds: Some(20.0),
      enabled: true,
      editable: false,
    });
    assert!(!enabled(&state, Action::MarkA));
    assert!(!enabled(&state, Action::MarkB));
    assert!(!enabled(&state, Action::Restart));
    assert!(
      enabled(&state, Action::ToggleLoop),
      "disabling remains possible after seekability becomes unknown"
    );
    assert!(enabled(&state, Action::Clear));
    state.playback.view.tools.busy = true;
    assert!(!enabled(&state, Action::Restart));
    drop(super::super::input(&mut state, Input::Back));
    assert!(matches!(
      focused_action(&state),
      ChoiceAction::Tool(Action::Open(Panel::Loop))
    ));
  }

  #[tokio::test]
  async fn tools_close_and_body_stay_reachable_in_short_viewports() {
    use iced::advanced::{layout, renderer, renderer::Headless, widget, Layout};
    use jellypilot_core::locale::UiLanguage;
    let renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .expect("headless renderer");
    for language in [UiLanguage::English, UiLanguage::SimplifiedChinese] {
      for size in [
        Size::new(1920.0, 1080.0),
        Size::new(1280.0, 720.0),
        Size::new(1920.0, 320.0),
      ] {
        for panel in [
          Panel::PrimarySubtitles,
          Panel::SecondarySubtitles,
          Panel::Loop,
        ] {
          let mut state = state();
          state.kernel.locale = crate::i18n::Localizer::new(language);
          let parent = if panel == Panel::Loop {
            Panel::Settings
          } else {
            Panel::Subtitles
          };
          open(&mut state, parent, panel);
          state.playback.view.tools.busy = true;
          let scale = style::scale(size.width);
          let mut page = view(&state, panel, size, scale);
          let mut tree = widget::Tree::new(&page);
          tree.diff(page.as_widget_mut());
          let node = page.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, size),
          );
          let mut bounds = crate::app::view::viewing_queue::tests::Bounds::default();
          page
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut bounds);
          let close = bounds.get("tv-player-panel-close");
          let body = bounds.get("tv-player-tools-viewport");
          assert!(close.height >= style::CONTROL * scale - 0.1);
          assert!(close.y >= style::SAFE_Y * scale);
          assert!(close.y + close.height <= size.height - style::SAFE_Y * scale);
          assert!(
            body.height >= style::CONTROL * scale - 0.1,
            "{size:?}: body {body:?}"
          );
          assert!(body.y + body.height <= size.height - style::SAFE_Y * scale);
        }
      }
    }
  }
  #[tokio::test]
  async fn pointer_press_cannot_select_a_replacement_track_or_file() {
    use iced::advanced::{renderer, renderer::Headless, shell};
    use iced::mouse::{Button, Cursor, Event as MouseEvent};
    use iced_runtime::user_interface::{Cache, UserInterface};
    let mut renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .expect("headless renderer");
    let size = Size::new(1920.0, 1080.0);
    for boundary in 0..3 {
      let mut state = state();
      open(&mut state, Panel::Subtitles, Panel::SecondarySubtitles);
      let mut ui = UserInterface::build(
        view(&state, Panel::SecondarySubtitles, size, 1.0),
        size,
        Cache::new(),
        &mut renderer,
      );
      let mut bounds = crate::app::view::viewing_queue::tests::Bounds::default();
      ui.operate(&renderer, &mut bounds);
      let target = bounds.get("tv-player-tool-Secondary(Some(2))").center();
      let mut bus = shell::Bus::new();
      let _ = ui.update(
        &iced::window::Headless,
        &shell::Waker::noop(),
        &[iced::Event::Mouse(MouseEvent::ButtonPressed(Button::Left))],
        Cursor::Available(target),
        &mut renderer,
        &mut bus,
      );
      let cache = ui.into_cache();
      match boundary {
        0 => state.playback.view.tools.tracks.swap(1, 4),
        1 => state.playback.view.tools.file = Some(PlaybackFileToken::default()),
        _ => {
          drop(super::super::update(
            &mut state,
            super::super::Message::ClosePanel,
          ));
          open(&mut state, Panel::Subtitles, Panel::SecondarySubtitles);
        }
      }
      drop(reconcile(&mut state));
      let mut ui = UserInterface::build(
        view(&state, Panel::SecondarySubtitles, size, 1.0),
        size,
        cache,
        &mut renderer,
      );
      let _ = ui.update(
        &iced::window::Headless,
        &shell::Waker::noop(),
        &[iced::Event::Mouse(MouseEvent::ButtonReleased(Button::Left))],
        Cursor::Available(target),
        &mut renderer,
        &mut bus,
      );
      assert!(
        bus.into_iter().all(|message| !matches!(
          message,
          AppMessage::Tv(super::super::super::Message::Player(
            super::super::Message::Tools(_)
          ))
        )),
        "retired pointer gesture crossed boundary {boundary}"
      );
    }
  }
}
