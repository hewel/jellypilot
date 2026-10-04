//! TV queue presentation; all mutations use the shared session-owned queue route.

use iced::widget::{
  button, column, container, mouse_area, opaque, row, scrollable, space, stack, text, Column,
};
use iced::{Alignment, Element, Fill, Length, Task};
use jellypilot_core::request_gate::SessionToken;
use jellypilot_core::tv_navigation::Input;
use jellypilot_core::viewing_queue::{QueueEntryId, QueueMove};
use jellypilot_mpv::playback::PlaybackStartPosition;
use jellypilot_ui::fonts::HEADING_FONT;
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::tv as style;
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::tv_focus::focus;

use super::{AppMessage, PlaybackMessage, State};
use crate::app::playback::viewing_queue::{self, Action};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Focus {
  Entry(QueueEntryId, EntryAction),
  Clear,
  #[default]
  Close,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryAction {
  Play,
  Up,
  Down,
  Remove,
}

const ACTIONS: [EntryAction; 4] = [
  EntryAction::Play,
  EntryAction::Up,
  EntryAction::Down,
  EntryAction::Remove,
];
const ROW_HEIGHT: f32 = 104.0;
const ROW_GAP: f32 = 12.0;

#[derive(Clone, Debug)]
pub enum Message {
  Close,
  Focus(Focus),
  Activate {
    session: SessionToken,
    revision: u64,
    generation: u64,
    presentation: Option<std::time::Instant>,
    focus: Focus,
  },
}

#[derive(Default)]
pub(super) struct Surface {
  pub(super) open: bool,
  session: Option<SessionToken>,
  focus: Focus,
  index: usize,
}

fn message(event: Message) -> AppMessage {
  super::message(super::Message::Upcoming(event))
}

fn route(state: &mut State, event: viewing_queue::Message) -> Task<AppMessage> {
  crate::app::update::route_message(
    state,
    AppMessage::Playback(PlaybackMessage::ViewingQueue(event)),
  )
}

pub(crate) fn is_open(state: &State) -> bool {
  state.tv_mode() && state.playback.viewing_queue.open
}

fn admitted(state: &State) -> bool {
  state.tv_mode()
    && state.shell.images_visible
    && state.shell.window_id.is_some()
    && !state.shell.quit_requested
    && state.shell.pending_close.is_none()
    && state.kernel.connection == jellypilot_auth::login::ConnectionPhase::Connected
    && !crate::app::accounts::content_mutations_blocked(&state.kernel)
    && !crate::app::accounts::blocking_modal(&state.accounts)
    && !super::super::account::modal_open(state)
    && !state.shell.settings_open
    && !state.shell.account_popover_open
    && !state.tv.settings.open
    && !state.tv.search.open
    && !super::super::filters::is_open(state)
}

pub(crate) fn open(state: &mut State) -> Task<AppMessage> {
  if !admitted(state) {
    return Task::none();
  }
  route(state, viewing_queue::Message::Open)
}

fn activation(state: &State, focus: Focus) -> Message {
  Message::Activate {
    session: state.kernel.request_gate.current_session(),
    revision: state.playback.view.upcoming.revision,
    generation: state.playback.view.lifecycle.replacement_generation,
    presentation: state.tv.player.presentation,
    focus,
  }
}

fn enabled(state: &State, focus: Focus) -> bool {
  if focus == Focus::Close {
    return true;
  }
  if state.playback.view.upcoming.pending.is_some()
    || state.playback.view.busy
    || state.playback.view.lifecycle.replacing
  {
    return false;
  }
  let entries = &state.playback.view.upcoming.entries;
  match focus {
    Focus::Entry(id, action) => {
      entries
        .iter()
        .position(|entry| entry.id == id)
        .is_some_and(|index| match action {
          EntryAction::Up => index > 0,
          EntryAction::Down => index + 1 < entries.len(),
          EntryAction::Play => state.playback.view.engine_available,
          EntryAction::Remove => true,
        })
    }
    Focus::Clear => !entries.is_empty(),
    Focus::Close => true,
  }
}

pub(crate) fn update(state: &mut State, event: Message) -> Task<AppMessage> {
  if !is_open(state) || !admitted(state) {
    return Task::none();
  }
  match event {
    Message::Close => route(state, viewing_queue::Message::Close),
    Message::Focus(focus) => {
      if valid_focus(state, focus) {
        state.tv.player.upcoming.focus = focus;
        remember_index(state);
      }
      Task::none()
    }
    Message::Activate {
      session,
      revision,
      generation,
      presentation,
      focus,
    } => {
      if !state.kernel.request_gate.is_current_session(session)
        || state.playback.view.upcoming.revision != revision
        || state.playback.view.lifecycle.replacement_generation != generation
        || state.tv.player.presentation != presentation
        || !enabled(state, focus)
      {
        return Task::none();
      }
      if focus == Focus::Close {
        return route(state, viewing_queue::Message::Close);
      }
      state.tv.player.upcoming.focus = focus;
      remember_index(state);
      let action = match focus {
        Focus::Entry(id, EntryAction::Play) => Action::PlayNow(id),
        Focus::Entry(id, EntryAction::Up) => Action::Move(id, QueueMove::Up),
        Focus::Entry(id, EntryAction::Down) => Action::Move(id, QueueMove::Down),
        Focus::Entry(id, EntryAction::Remove) => Action::Remove(id),
        Focus::Clear => Action::Clear,
        Focus::Close => return Task::none(),
      };
      route(
        state,
        viewing_queue::Message::Edit {
          session,
          revision,
          action,
        },
      )
    }
  }
}

fn valid_focus(state: &State, focus: Focus) -> bool {
  match focus {
    Focus::Entry(id, _) => state
      .playback
      .view
      .upcoming
      .entries
      .iter()
      .any(|entry| entry.id == id),
    Focus::Clear | Focus::Close => true,
  }
}

fn remember_index(state: &mut State) {
  if let Focus::Entry(id, _) = state.tv.player.upcoming.focus {
    if let Some(index) = state
      .playback
      .view
      .upcoming
      .entries
      .iter()
      .position(|entry| entry.id == id)
    {
      state.tv.player.upcoming.index = index;
    }
  }
}

fn reveal(state: &State) -> Task<AppMessage> {
  let scale = style::scale(state.shell.window_size.width);
  let current_height = if state.playback.view.now_playing.is_some() {
    96.0
  } else {
    0.0
  };
  let offset = match state.tv.player.upcoming.focus {
    Focus::Entry(_, _) => {
      state.tv.player.upcoming.index as f32 * (ROW_HEIGHT + ROW_GAP) + current_height
    }
    Focus::Clear => {
      state.playback.view.upcoming.entries.len() as f32 * (ROW_HEIGHT + ROW_GAP)
        + current_height
        + 12.0
    }
    Focus::Close => return Task::none(),
  };
  iced::widget::operation::scroll_to(
    "tv-upcoming-entries",
    iced::widget::operation::AbsoluteOffset {
      x: 0.0,
      y: offset * scale,
    },
  )
}

pub(crate) fn reconcile(state: &mut State) -> Task<AppMessage> {
  let open = is_open(state);
  let session = state.kernel.request_gate.current_session();
  let queue = &mut state.tv.player.upcoming;
  if !open {
    queue.open = false;
    return Task::none();
  }
  let previous = queue.focus;
  let previous_index = queue.index;
  let entries = &state.playback.view.upcoming.entries;
  if !queue.open || queue.session != Some(session) {
    queue.focus = entries.first().map_or(Focus::Close, |entry| {
      Focus::Entry(entry.id, EntryAction::Play)
    });
    queue.index = 0;
  } else if let Focus::Entry(id, action) = queue.focus {
    if let Some(index) = entries.iter().position(|entry| entry.id == id) {
      queue.index = index;
    } else {
      queue.index = queue.index.min(entries.len().saturating_sub(1));
      queue.focus = entries
        .get(queue.index)
        .map_or(Focus::Close, |entry| Focus::Entry(entry.id, action));
    }
  } else if queue.focus == Focus::Clear && entries.is_empty() {
    queue.focus = Focus::Close;
  }
  queue.open = true;
  queue.session = Some(session);
  if queue.focus != previous || queue.index != previous_index {
    reveal(state)
  } else {
    Task::none()
  }
}

pub(crate) fn input(state: &mut State, input: Input) -> Task<AppMessage> {
  if !is_open(state) || !admitted(state) {
    return Task::none();
  }
  if input == Input::Back {
    return update(state, Message::Close);
  }
  if input == Input::Confirm {
    return update(state, activation(state, state.tv.player.upcoming.focus));
  }
  let entries = &state.playback.view.upcoming.entries;
  let current = state.tv.player.upcoming.focus;
  let next = match (current, input) {
    (Focus::Close, Input::Down) => entries.first().map_or(Focus::Close, |entry| {
      Focus::Entry(entry.id, EntryAction::Play)
    }),
    (Focus::Clear, Input::Up) => entries.last().map_or(Focus::Close, |entry| {
      Focus::Entry(entry.id, EntryAction::Play)
    }),
    (Focus::Clear, Input::Down) => Focus::Close,
    (Focus::Entry(_, action), Input::Up) => state
      .tv
      .player
      .upcoming
      .index
      .checked_sub(1)
      .and_then(|index| entries.get(index))
      .map_or(Focus::Close, |entry| Focus::Entry(entry.id, action)),
    (Focus::Entry(_, action), Input::Down) => entries
      .get(state.tv.player.upcoming.index + 1)
      .map_or(Focus::Clear, |entry| Focus::Entry(entry.id, action)),
    (Focus::Entry(id, action), Input::Left | Input::Right) => {
      let index = ACTIONS
        .iter()
        .position(|candidate| candidate == &action)
        .unwrap_or(0);
      let next = if input == Input::Left {
        index.saturating_sub(1)
      } else {
        (index + 1).min(ACTIONS.len() - 1)
      };
      Focus::Entry(id, ACTIONS[next])
    }
    _ => current,
  };
  state.tv.player.upcoming.focus = next;
  remember_index(state);
  reveal(state)
}

fn action_key(focus: Focus) -> &'static str {
  match focus {
    Focus::Entry(_, EntryAction::Play) => "viewing-queue-play",
    Focus::Entry(_, EntryAction::Up) => "viewing-queue-up",
    Focus::Entry(_, EntryAction::Down) => "viewing-queue-down",
    Focus::Entry(_, EntryAction::Remove) => "viewing-queue-remove",
    Focus::Clear => "viewing-queue-clear",
    Focus::Close => "viewing-queue-close",
  }
}

fn control(
  state: &State,
  target: Focus,
  icon: Icon,
  label: bool,
  scale: f32,
) -> Element<'_, AppMessage> {
  let event = message(if target == Focus::Close {
    Message::Close
  } else {
    activation(state, target)
  });
  let enabled = enabled(state, target);
  let label = label.then(|| state.t(action_key(target)));
  let control: Element<'_, AppMessage> = mouse_area(focus(
    state.tv.player.upcoming.focus == target,
    move |progress| {
      let mut content = row![icon_with_color(
        icon,
        IconSize::Custom(28.0 * scale),
        style::foreground(style::PALETTE, progress, false)
      )]
      .spacing(12.0 * scale)
      .align_y(Alignment::Center);
      if let Some(label) = &label {
        content = content.push(
          text(label.clone())
            .size(style::BODY * scale)
            .font(HEADING_FONT),
        );
      }
      button(
        container(content)
          .padding(16.0 * scale)
          .width(Length::Fit.min(style::CONTROL * scale))
          .height(Length::Fit.min(style::CONTROL * scale))
          .align_x(Alignment::Center)
          .align_y(Alignment::Center),
      )
      .padding(0)
      .style(move |theme, status| {
        // A pending start disables activation without erasing the remote's location.
        let status = if status == button::Status::Disabled && progress > 0.0 {
          button::Status::Active
        } else {
          status
        };
        style::button_progress(style::PALETTE, progress, false)(theme, status)
      })
      .on_press_maybe(enabled.then_some(event.clone()))
      .into()
    },
  ))
  .on_enter(message(Message::Focus(target)))
  .into();
  if target == Focus::Close {
    container(control).id("tv-upcoming-close").into()
  } else if target == Focus::Clear {
    container(control).id("tv-upcoming-clear").into()
  } else if let Focus::Entry(id, _) = target {
    container(control)
      .id(entry_control_id(id, action_key(target)))
      .into()
  } else {
    control
  }
}

fn entry_control_id(id: QueueEntryId, action: &str) -> iced::widget::Id {
  iced::widget::Id::from(format!("tv-upcoming-{}-{action}", id.0))
}

pub(crate) fn view(state: &State) -> Element<'_, AppMessage> {
  let scale = style::scale(state.shell.window_size.width);
  let compact = state.shell.window_size.height < (360.0 + 2.0 * style::SAFE_Y) * scale;
  let mut content = column![row![
    text(state.t("viewing-queue-title"))
      .font(HEADING_FONT)
      .size(style::TITLE * scale),
    space().width(Fill),
    control(state, Focus::Close, Icon::Close, false, scale),
  ]
  .align_y(Alignment::Center)]
  .spacing(if compact { 12.0 } else { 24.0 } * scale);
  let mut body = Column::new().spacing(24.0 * scale);
  if let Some(playing) = &state.playback.view.now_playing {
    body = body.push(
      column![
        text(state.t("viewing-queue-current"))
          .size(style::META * scale)
          .color(style::PALETTE.text.metadata),
        ellipsis_text(&playing.item.title)
          .size(style::BODY * scale)
          .font(HEADING_FONT)
      ]
      .spacing(8.0 * scale)
      .height(72.0 * scale),
    );
  }
  let mut entries = Vec::new();
  for entry in state.playback.view.upcoming.entries.iter() {
    let identity = column![
      ellipsis_text(&entry.title)
        .font(HEADING_FONT)
        .size(style::BODY * scale),
      text(state.t(if entry.position == PlaybackStartPosition::Resume {
        "viewing-queue-resume"
      } else {
        "viewing-queue-beginning"
      }))
      .size(style::META * scale)
      .color(style::PALETTE.text.metadata)
    ]
    .spacing(8.0 * scale)
    .width(Fill);
    let mut actions = row![].spacing(8.0 * scale);
    for (action, icon) in [
      (EntryAction::Play, Icon::Play),
      (EntryAction::Up, Icon::ArrowUp),
      (EntryAction::Down, Icon::ArrowDown),
      (EntryAction::Remove, Icon::Trash),
    ] {
      actions = actions.push(control(
        state,
        Focus::Entry(entry.id, action),
        icon,
        false,
        scale,
      ));
    }
    entries.push((
      entry.id,
      container(
        row![identity, actions]
          .spacing(16.0 * scale)
          .align_y(Alignment::Center),
      )
      .height(ROW_HEIGHT * scale)
      .align_y(Alignment::Center)
      .into(),
    ));
  }
  if state.playback.view.upcoming.entries.is_empty() {
    body = body.push(text(state.t("viewing-queue-empty")).size(style::BODY * scale));
  } else {
    body = body.push(crate::app::view::viewing_queue::queue_rows(
      state,
      entries,
      ROW_GAP * scale,
    ));
    body = body.push(crate::app::view::viewing_queue::queue_guard(
      state,
      control(state, Focus::Clear, Icon::Trash, true, scale),
    ));
  }
  if state.playback.view.upcoming.pending.is_some() {
    body = body.push(text(state.t("viewing-queue-starting")).size(style::META * scale));
  }
  if let Some(error) = state
    .playback
    .viewing_queue
    .error
    .as_ref()
    .or(state.playback.notice.as_ref())
  {
    body = body.push(
      text(state.kernel.locale.message(error))
        .size(style::META * scale)
        .color(style::PALETTE.colors.error),
    );
  }
  content = content.push(
    container(scrollable(body).id("tv-upcoming-entries").height(Fill))
      .id("tv-upcoming-body")
      .height(Fill),
  );
  if !compact {
    content = content.push(
      text(state.t(action_key(state.tv.player.upcoming.focus)))
        .size(style::META * scale)
        .color(style::PALETTE.text.body),
    );
  }
  let panel = container(content)
    .id("tv-upcoming-panel")
    .padding(if compact { 16.0 } else { 32.0 } * scale)
    .width((800.0 * scale).min(state.shell.window_size.width - 2.0 * style::SAFE_X * scale))
    .height(Fill)
    .style(style::panel);
  let scrim = mouse_area(
    container(space())
      .width(Fill)
      .height(Fill)
      .style(style::scrim),
  )
  .on_press(message(Message::Close));
  stack![
    scrim,
    container(opaque(panel))
      .width(Fill)
      .height(Fill)
      .align_x(Alignment::End)
      .padding([style::SAFE_Y * scale, style::SAFE_X * scale])
  ]
  .into()
}

#[cfg(test)]
mod tests {
  use super::*;
  use iced::Size;
  use jellypilot_core::config::UiMode;
  use jellypilot_mpv::playback_session::UpcomingQueueEntry;

  fn state() -> State {
    let mut state = crate::app::update::tests::test_state();
    state.shell.ui_mode = UiMode::Tv;
    state.shell.window_id = Some(iced::window::Id::unique());
    state.shell.images_visible = true;
    state.kernel.connection = jellypilot_auth::login::ConnectionPhase::Connected;
    state.playback.view.engine_available = true;
    state.playback.viewing_queue.open = true;
    replace_entries(&mut state, &[1, 2, 3]);
    drop(reconcile(&mut state));
    state
  }

  fn replace_entries(state: &mut State, ids: &[u64]) {
    state.playback.view.upcoming.revision += 1;
    state.playback.view.upcoming.entries = ids
      .iter()
      .map(|id| UpcomingQueueEntry {
        id: QueueEntryId(*id),
        item_id: format!("item-{id}"),
        title: format!("Movie {id}"),
        item_type: "Movie".into(),
        position: PlaybackStartPosition::Beginning,
        artwork_image_id: None,
      })
      .collect();
  }

  #[tokio::test]
  async fn focus_follows_identity_then_surviving_neighbor_across_queue_changes() {
    let mut state = state();
    drop(input(&mut state, Input::Down));
    drop(input(&mut state, Input::Right));
    assert_eq!(
      state.tv.player.upcoming.focus,
      Focus::Entry(QueueEntryId(2), EntryAction::Up)
    );
    replace_entries(&mut state, &[2, 1, 3]);
    drop(reconcile(&mut state));
    assert_eq!(state.tv.player.upcoming.index, 0);
    assert!(!enabled(&state, state.tv.player.upcoming.focus));
    replace_entries(&mut state, &[1, 3]);
    drop(reconcile(&mut state));
    assert_eq!(
      state.tv.player.upcoming.focus,
      Focus::Entry(QueueEntryId(1), EntryAction::Up)
    );
    replace_entries(&mut state, &[]);
    drop(reconcile(&mut state));
    assert_eq!(state.tv.player.upcoming.focus, Focus::Close);
  }

  #[tokio::test]
  async fn loading_keeps_focus_and_stopped_queue_remains_navigable() {
    let mut state = state();
    assert!(!super::super::active(&state));
    drop(input(&mut state, Input::Down));
    let focus = state.tv.player.upcoming.focus;
    state.playback.view.upcoming.pending = Some(QueueEntryId(2));
    drop(reconcile(&mut state));
    assert_eq!(state.tv.player.upcoming.focus, focus);
    assert!(!enabled(&state, focus));
    assert!(!enabled(&state, Focus::Clear));
    let event = activation(&state, focus);
    assert!(update(&mut state, event).units() == 0);
    drop(input(&mut state, Input::Back));
    assert!(!state.playback.viewing_queue.open);
  }

  #[tokio::test]
  async fn retired_queue_activation_cannot_apply_after_reorder_or_presentation_change() {
    let mut state = state();
    let fixture = crate::app::test_support::BrowseFixture::new();
    state.kernel.client = Some(fixture.client());
    for id in ["one", "two"] {
      let revision = state.playback.session.view().upcoming.revision;
      state
        .playback
        .session
        .queue_insert(
          revision,
          crate::app::tv::view::tests::item(id).into(),
          PlaybackStartPosition::Beginning,
          jellypilot_core::viewing_queue::QueuePlacement::Last,
        )
        .expect("queued movie");
    }
    state.playback.view = state.playback.session.view();
    let id = state.playback.view.upcoming.entries[0].id;
    let target = Focus::Entry(id, EntryAction::Remove);
    let stale_revision = activation(&state, target);
    state
      .playback
      .session
      .queue_move(state.playback.view.upcoming.revision, id, QueueMove::Down)
      .expect("reorder");
    state.playback.view = state.playback.session.view();
    drop(update(&mut state, stale_revision));
    assert_eq!(state.playback.session.view().upcoming.entries.len(), 2);
    let stale_presentation = activation(&state, target);
    state.tv.player.presentation = Some(std::time::Instant::now());
    drop(update(&mut state, stale_presentation));
    assert_eq!(state.playback.session.view().upcoming.entries.len(), 2);
    let stale_generation = activation(&state, target);
    state.playback.view.lifecycle.replacement_generation += 1;
    drop(update(&mut state, stale_generation));
    assert_eq!(state.playback.session.view().upcoming.entries.len(), 2);
    let stale_session = activation(&state, target);
    state.kernel.request_gate.disconnect();
    drop(update(&mut state, stale_session));
    assert_eq!(state.playback.session.view().upcoming.entries.len(), 2);
    let current = activation(&state, target);
    drop(update(&mut state, current));
    assert!(state
      .playback
      .session
      .view()
      .upcoming
      .entries
      .iter()
      .all(|entry| entry.id != id));
  }

  #[tokio::test]
  async fn queue_close_stays_reachable_with_long_error_in_short_viewports() {
    use iced::advanced::{layout, renderer, renderer::Headless, widget, Layout};
    use jellypilot_core::locale::UiLanguage;

    #[derive(Default)]
    struct Bounds {
      panel: Option<iced::Rectangle>,
      close: Option<iced::Rectangle>,
      body: Option<iced::Rectangle>,
    }
    impl widget::Operation for Bounds {
      fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
        visit(self);
      }
      fn container(&mut self, id: Option<&widget::Id>, bounds: iced::Rectangle) {
        if id == Some(&widget::Id::new("tv-upcoming-panel")) {
          self.panel = Some(bounds);
        }
        if id == Some(&widget::Id::new("tv-upcoming-close")) {
          self.close = Some(bounds);
        }
        if id == Some(&widget::Id::new("tv-upcoming-body")) {
          self.body = Some(bounds);
        }
      }
    }
    let renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .expect("headless layout renderer");
    for language in [UiLanguage::English, UiLanguage::SimplifiedChinese] {
      for viewport in [
        Size::new(1920.0, 1080.0),
        Size::new(1280.0, 720.0),
        Size::new(1920.0, 320.0),
      ] {
        let mut state = state();
        state.shell.window_size = viewport;
        state.kernel.locale = crate::i18n::Localizer::new(language);
        state.playback.viewing_queue.error = Some(crate::i18n::UiText::new("viewing-queue-full"));
        let mut page = view(&state);
        let mut tree = widget::Tree::new(&page);
        tree.diff(page.as_widget_mut());
        let node = page.as_widget_mut().layout(
          &mut tree,
          &renderer,
          &layout::Limits::new(Size::ZERO, viewport),
        );
        let mut bounds = Bounds::default();
        page
          .as_widget_mut()
          .operate(&mut tree, Layout::new(&node), &renderer, &mut bounds);
        let panel = bounds.panel.expect("queue panel");
        let close = bounds.close.expect("header close");
        let scale = style::scale(viewport.width);
        assert!(panel.height <= viewport.height - 2.0 * style::SAFE_Y * scale + 0.1);
        assert!(panel.contains(close.center()));
        assert!(close.height >= style::CONTROL * scale - 0.1);
        assert!(close.width >= style::CONTROL * scale - 0.1);
        assert!(bounds.body.expect("scrolling body").height >= style::CONTROL * scale - 0.1);
      }
    }
  }

  #[tokio::test]
  async fn pointer_press_cannot_transfer_to_another_entry_after_removal_or_reorder() {
    use crate::app::view::viewing_queue::tests::Bounds;
    use iced::advanced::{renderer, renderer::Headless, shell};
    use iced_runtime::user_interface::{Cache, UserInterface};

    let mut renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .expect("headless layout renderer");
    for target in [
      Focus::Entry(QueueEntryId(1), EntryAction::Play),
      Focus::Entry(QueueEntryId(1), EntryAction::Remove),
      Focus::Clear,
    ] {
      for replacement in [&[2, 3][..], &[2, 1, 3][..], &[1, 3][..]] {
        let mut state = state();
        state.shell.window_size = Size::new(1920.0, 1080.0);
        let mut ui = UserInterface::build(
          view(&state),
          state.shell.window_size,
          Cache::new(),
          &mut renderer,
        );
        let mut bounds = Bounds::default();
        ui.operate(&renderer, &mut bounds);
        let pressed = bounds.get(if target == Focus::Clear {
          iced::widget::Id::new("tv-upcoming-clear")
        } else {
          entry_control_id(QueueEntryId(1), action_key(target))
        });
        let mut bus = shell::Bus::new();
        ui.update(
          &iced::window::Headless,
          &shell::Waker::noop(),
          &[iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
            iced::mouse::Button::Left,
          ))],
          iced::mouse::Cursor::Available(pressed.center()),
          &mut renderer,
          &mut bus,
        );
        assert!(bus.drain().all(|(event, _)| {
          !matches!(
            event,
            AppMessage::Tv(crate::app::tv::Message::Player(
              super::super::Message::Upcoming(Message::Activate { .. })
            ))
          )
        }));
        let cache = ui.into_cache();
        replace_entries(&mut state, replacement);
        drop(reconcile(&mut state));
        let mut ui =
          UserInterface::build(view(&state), state.shell.window_size, cache, &mut renderer);
        let mut moved = Bounds::default();
        ui.operate(&renderer, &mut moved);
        if target != Focus::Clear {
          assert!(moved
            .get(entry_control_id(
              QueueEntryId(replacement[0]),
              action_key(target)
            ))
            .contains(pressed.center()));
        } else if replacement.len() == 3 {
          assert!(moved.get("tv-upcoming-clear").contains(pressed.center()));
        }
        ui.update(
          &iced::window::Headless,
          &shell::Waker::noop(),
          &[iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
            iced::mouse::Button::Left,
          ))],
          iced::mouse::Cursor::Available(pressed.center()),
          &mut renderer,
          &mut bus,
        );
        assert!(bus.drain().all(|(event, _)| {
          !matches!(
            event,
            AppMessage::Tv(crate::app::tv::Message::Player(
              super::super::Message::Upcoming(Message::Activate { .. })
            ))
          )
        }));
      }
    }
  }
}
