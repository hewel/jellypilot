//! Presentation and scope checks for the session-owned temporary viewing queue.

use iced::Task;
use jellypilot_core::request_gate::SessionToken;
use jellypilot_core::viewing_queue::{QueueEditError, QueueEntryId, QueueMove, QueuePlacement};
use jellypilot_mpv::playback::{Playable, PlaybackStartPosition};
use jellypilot_mpv::playback_session::PlaybackIntent;

use super::{Kernel, PendingPlay, PlaybackUpdate, Surface};
use crate::app::message::{Message as AppMessage, WindowMessage};
use crate::i18n::UiText;

#[derive(Default)]
pub struct QueueSurface {
  pub open: bool,
  pub error: Option<UiText>,
}

#[derive(Clone)]
pub enum Message {
  Open,
  Close,
  Edit {
    session: SessionToken,
    revision: u64,
    action: Action,
  },
}

#[derive(Clone)]
pub enum Action {
  Insert {
    item: Box<Playable>,
    position: PlaybackStartPosition,
    placement: QueuePlacement,
  },
  PlayNow(QueueEntryId),
  Move(QueueEntryId, QueueMove),
  Remove(QueueEntryId),
  Clear,
}

pub(super) fn update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit: bool,
  message: Message,
) -> PlaybackUpdate {
  match message {
    Message::Close => surface.viewing_queue.open = false,
    Message::Open => {
      if kernel.client.is_some() && !quit {
        surface.viewing_queue.open = true;
        surface.audio_menu_open = false;
        surface.subtitle_menu_open = false;
        surface.queue_menu_open = false;
      }
    }
    Message::Edit {
      session,
      revision,
      action,
    } => {
      if !kernel.request_gate.is_current_session(session)
        || quit
        || kernel.client.is_none()
        || kernel.sdk.content_mutations_blocked()
      {
        return PlaybackUpdate::without_transition(Task::none());
      }
      if let Action::PlayNow(id) = action {
        if surface.view.upcoming.revision != revision {
          surface.viewing_queue.error = Some(UiText::new("viewing-queue-stale"));
          return PlaybackUpdate::without_transition(Task::none());
        }
        if surface.view.upcoming.pending.is_some() {
          return PlaybackUpdate::without_transition(Task::none());
        }
        let Some(item) = surface.session.queued_playable(id) else {
          surface.viewing_queue.error = Some(UiText::new("viewing-queue-stale"));
          return PlaybackUpdate::without_transition(Task::none());
        };
        let intro = super::start_intro_availability(kernel, item);
        surface.viewing_queue.error = None;
        if crate::embedded::enabled()
          && surface
            .presentation_hold
            .load(std::sync::atomic::Ordering::Acquire)
        {
          let task = if super::defer_play(
            surface,
            PendingPlay::Queued {
              session,
              revision,
              id,
            },
          ) {
            Task::done(AppMessage::Window(WindowMessage::ShowForPlayback))
          } else {
            Task::none()
          };
          return PlaybackUpdate::without_transition(task);
        }
        return super::apply_local_playback_intent(
          surface,
          kernel,
          quit,
          PlaybackIntent::PlayQueued {
            revision,
            id,
            intro,
          },
        );
      }
      let inserted = matches!(&action, Action::Insert { .. });
      let result = match action {
        Action::Insert {
          item,
          position,
          placement,
        } => surface
          .session
          .queue_insert(revision, *item, position, placement)
          .map(|_| ()),
        Action::Move(id, direction) => surface.session.queue_move(revision, id, direction),
        Action::Remove(id) => surface.session.queue_remove(revision, id),
        Action::Clear => surface.session.queue_clear(revision),
        Action::PlayNow(_) => return PlaybackUpdate::without_transition(Task::none()),
      };
      surface.viewing_queue.error = result.as_ref().err().map(|error| {
        UiText::new(match error {
          QueueEditError::Stale | QueueEditError::Missing => "viewing-queue-stale",
          QueueEditError::Busy => "viewing-queue-busy",
          QueueEditError::Full => "viewing-queue-full",
          QueueEditError::InvalidItem => "viewing-queue-invalid",
        })
      });
      if inserted {
        surface.viewing_queue.open = true;
      }
      super::sync_playback_projection(surface, kernel, quit);
    }
  }
  PlaybackUpdate::without_transition(Task::none())
}
