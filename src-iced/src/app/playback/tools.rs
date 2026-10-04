//! File-scoped routing for observed playback tools; the session owns execution.

use iced::Task;
use jellypilot_core::request_gate::SessionToken;
use jellypilot_mpv::playback::tools::{PlaybackFileToken, PlaybackToolAction};
use jellypilot_mpv::playback_session::PlaybackIntent;

use super::{Kernel, PlaybackUpdate, Surface};
use crate::app::state::State;

#[derive(Default)]
pub struct ToolsSurface {
  pub open: bool,
  file: Option<PlaybackFileToken>,
  pub options_open: bool,
  pub presentation: u64,
}

#[derive(Clone)]
pub enum Message {
  OpenLoop,
  Close,
  OptionsToggled,
  Execute {
    session: SessionToken,
    file: PlaybackFileToken,
    action: PlaybackToolAction,
  },
  PanelExecute {
    session: SessionToken,
    file: PlaybackFileToken,
    presentation: u64,
    action: PlaybackToolAction,
  },
}

pub(super) fn bind(surface: &mut Surface) {
  surface.tools.presentation = surface.tools.presentation.wrapping_add(1);
  surface.tools.file = surface.view.tools.file;
}

pub(crate) fn close(surface: &mut Surface) {
  surface.tools.presentation = surface.tools.presentation.wrapping_add(1);
  surface.tools.open = false;
  surface.tools.options_open = false;
  surface.subtitle_menu_open = false;
  surface.tools.file = None;
}

pub(crate) fn is_open(state: &State) -> bool {
  state.playback.tools.open
    || state.playback.subtitle_menu_open
    || state.playback.tools.options_open
}

pub(crate) fn reconcile(state: &mut State) {
  if !is_open(state) {
    return;
  }
  let surface = &mut state.playback;
  if state.shell.window_id.is_none()
    || !state.shell.images_visible
    || state.shell.pending_close.is_some()
    || state.shell.quit_requested
    || state.shell.settings_open
    || super::super::accounts::blocking_modal(&state.accounts)
    || state.kernel.sdk.content_mutations_blocked()
    || surface.view.now_playing.is_none()
    || surface.view.lifecycle.replacing
    || surface
      .tools
      .file
      .is_some_and(|file| Some(file) != surface.view.tools.file)
  {
    close(surface);
  } else if surface.tools.file.is_none() && surface.view.tools.file.is_some() {
    bind(surface);
  }
}

pub(super) fn update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit: bool,
  message: Message,
) -> PlaybackUpdate {
  match message {
    Message::Close => {
      let origin = if surface.subtitle_menu_open {
        "playback-subtitles-trigger"
      } else if crate::embedded::enabled() {
        "playback-options-trigger"
      } else {
        super::super::shell::SETTINGS_TRIGGER_ID
      };
      close(surface);
      return PlaybackUpdate::without_transition(iced::widget::operation::focus(
        iced::widget::Id::new(origin),
      ));
    }
    Message::OptionsToggled => {
      if !quit && surface.view.now_playing.is_some() && !kernel.sdk.content_mutations_blocked() {
        surface.tools.options_open = !surface.tools.options_open;
        bind(surface);
      }
    }
    Message::OpenLoop => {
      if !quit && surface.view.now_playing.is_some() && !kernel.sdk.content_mutations_blocked() {
        surface.tools.open = true;
        surface.tools.options_open = false;
        bind(surface);
        surface.subtitle_menu_open = false;
        surface.audio_menu_open = false;
        surface.queue_menu_open = false;
        surface.viewing_queue.open = false;
      }
    }
    Message::Execute {
      session,
      file,
      action,
    } => {
      if !kernel.request_gate.is_current_session(session)
        || Some(file) != surface.view.tools.file
        || quit
        || kernel.client.is_none()
        || kernel.sdk.content_mutations_blocked()
        || surface.view.now_playing.is_none()
        || surface.view.lifecycle.replacing
        || surface.view.tools.busy
      {
        return PlaybackUpdate::without_transition(Task::none());
      }
      return super::apply_local_playback_intent(
        surface,
        kernel,
        quit,
        PlaybackIntent::PlaybackTool { file, action },
      );
    }
    Message::PanelExecute {
      session,
      file,
      presentation,
      action,
    } => {
      if presentation != surface.tools.presentation
        || !(surface.tools.open || surface.tools.options_open || surface.subtitle_menu_open)
      {
        return PlaybackUpdate::without_transition(Task::none());
      }
      return update(
        surface,
        kernel,
        quit,
        Message::Execute {
          session,
          file,
          action,
        },
      );
    }
  }
  PlaybackUpdate::without_transition(Task::none())
}

#[cfg(test)]
mod tests {
  use super::*;
  use jellypilot_mpv::playback::tools::{AbLoopView, ToolState};

  #[test]
  fn retiring_a_panel_preserves_observed_loop_and_cannot_follow_a_reloaded_file() {
    for retire in 0..4 {
      let mut state = crate::app::update::tests::active_intro_prompt_state();
      state.shell.window_id = Some(iced::window::Id::unique());
      state.shell.images_visible = true;
      state.playback.view.tools.file = Some(PlaybackFileToken::default());
      state.playback.view.tools.ab_loop = ToolState::Ready(AbLoopView {
        a_seconds: Some(5.0),
        b_seconds: Some(20.0),
        enabled: true,
        editable: true,
      });
      state.playback.tools.open = true;
      bind(&mut state.playback);
      let loop_config = state.playback.view.tools.ab_loop.clone();
      match retire {
        0 => close(&mut state.playback),
        1 => state.playback.view.tools.file = Some(PlaybackFileToken::default()),
        2 => state.shell.images_visible = false,
        _ => state.playback.view.lifecycle.replacing = true,
      }
      reconcile(&mut state);
      assert!(!is_open(&state));
      assert_eq!(state.playback.view.tools.ab_loop, loop_config);
    }
  }
}
