use std::collections::VecDeque;
use std::time::{Duration, Instant};

use jellypilot_media_server::MediaItem;
use jellypilot_session::{
  evaluate_intro_skip, evaluate_manual_skip, IntroSkipAction, IntroSkipKind, IntroSkipMode,
  IntroSkipRange,
};

use crate::playback::{
  NowPlayingItem, Playable, PlaybackEndReason, PlaybackError, PlaybackOutcome,
  PlaybackRefreshOutcome, PlaybackRefreshState, PlaybackStartPosition, PlaybackStopOutcome,
  PlaybackWarning, TrackInfo, TrackSelectionOutcome,
};

const INTRO_PROMPT_DURATION: Duration = Duration::from_secs(3);
const INTRO_PROMPT_DURATION_MS: i64 = 3_000;
const INTRO_CONFIRMATION_DURATION_MS: i64 = 1_500;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectId {
  epoch: u64,
  sequence: u64,
}

#[derive(Clone)]
pub enum PlaybackInput {
  Intent(PlaybackIntent),
  Event(PlaybackEvent),
}

#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub enum PlaybackIntent {
  Start {
    item: Playable,
    position: PlaybackStartPosition,
    intro: IntroAvailability,
  },
  TogglePaused,
  SetPaused(bool),
  Seek(f64),
  SetVolume(f64),
  SetMuted(bool),
  SelectAudioTrack(i64),
  SelectSubtitleTrack(Option<i64>),
  Stop,
  PlayAdjacent(AdjacentDirection),
  #[cfg_attr(not(test), allow(dead_code))]
  SkipIntro,
  Tick,
  Disconnect,
  Quit,
  SetIntroMode(IntroSkipMode),
}

#[derive(Clone, Copy)]
pub struct IntroAvailability {
  pub mode: IntroSkipMode,
  pub skipper_available: bool,
}

#[derive(Clone)]
pub enum PlaybackEvent {
  EngineAvailability(bool),
  ControllerSettled {
    id: EffectId,
    settlement: ControllerSettlement,
  },
  IntroRangesSettled {
    id: EffectId,
    result: Result<Vec<IntroSkipRange>, ()>,
  },
  AdjacentSettled {
    id: EffectId,
    direction: AdjacentDirection,
    result: Result<Option<MediaItem>, ()>,
  },
  TracksSettled {
    id: EffectId,
    result: Result<Vec<TrackInfo>, PlaybackError>,
  },
}

#[derive(Clone)]
pub enum ControllerSettlement {
  Started(Result<PlaybackOutcome, PlaybackError>),
  Controlled(Result<PlaybackOutcome, PlaybackError>),
  Stopped(Result<PlaybackStopOutcome, PlaybackError>),
  Refreshed {
    outcome: PlaybackRefreshOutcome,
    client_messages: Vec<String>,
  },
  TrackSelected(Result<TrackSelectionOutcome, PlaybackError>),
  OsdShown(Result<(), PlaybackError>),
  Shutdown(Vec<PlaybackWarning>),
}

#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub enum PlaybackEffect {
  Controller(EffectId, ControllerCommand),
  FetchIntroRanges(EffectId, String),
  LookupAdjacent(EffectId, AdjacentDirection),
}

#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub enum ControllerCommand {
  Start {
    item: Playable,
    position: PlaybackStartPosition,
  },
  SetPaused(bool),
  Seek(f64),
  SetVolume(f64),
  SetMuted(bool),
  SelectAudioTrack(i64),
  SelectSubtitleTrack(Option<i64>),
  ShowText {
    text: String,
    duration_ms: i64,
  },
  Stop,
  Refresh,
  Shutdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdjacentDirection {
  Previous,
  Next,
}

impl AdjacentDirection {
  const fn index(self) -> usize {
    match self {
      Self::Previous => 0,
      Self::Next => 1,
    }
  }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdjacentAvailability {
  Idle,
  Loading,
  Available { title: String },
  Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdjacentView {
  pub previous: AdjacentAvailability,
  pub next: AdjacentAvailability,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TracksView {
  Loading,
  Ready {
    tracks: Vec<TrackInfo>,
    audio: Option<i64>,
    subtitle: Option<i64>,
  },
  Unavailable,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NowPlayingView {
  pub item: NowPlayingItem,
  pub paused: bool,
  pub position_seconds: f64,
  pub duration_seconds: Option<f64>,
  pub volume: f64,
  pub muted: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntroPromptView {
  pub kind: IntroSkipKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaybackNotice {
  Finished,
  Stopped,
  Failed(PlaybackError),
  Warnings(Vec<PlaybackWarning>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionView {
  pub now_playing: Option<NowPlayingView>,
  pub tracks: TracksView,
  pub adjacent: AdjacentView,
  pub intro_prompt: Option<IntroPromptView>,
  pub notice: Option<PlaybackNotice>,
  pub engine_available: bool,
  pub busy: bool,
  pub can_start_login: bool,
  pub quit_may_proceed: bool,
}

pub struct PlaybackSession {
  snapshot: Option<crate::playback::PlaybackSnapshot>,
  tracks: TracksView,
  adjacent: AdjacentState,
  intro: IntroState,
  notice: Option<PlaybackNotice>,
  engine_available: bool,
  desired_paused: Option<bool>,
  desired_muted: Option<bool>,
  epoch: u64,
  sequence: u64,
  in_flight: Option<InFlight>,
  detached: Option<DetachedController>,
  pending_intro: Option<EffectId>,
  pending_adjacent: [Option<EffectId>; 2],
  pending: VecDeque<ControllerRequest>,
  cleanup_pending: bool,
  quitting: bool,
}

impl Default for PlaybackSession {
  fn default() -> Self {
    Self {
      snapshot: None,
      tracks: TracksView::Unavailable,
      adjacent: AdjacentState::default(),
      intro: IntroState::default(),
      notice: None,
      engine_available: false,
      desired_paused: None,
      desired_muted: None,
      epoch: 0,
      sequence: 0,
      in_flight: None,
      detached: None,
      pending_intro: None,
      pending_adjacent: [None, None],
      pending: VecDeque::new(),
      cleanup_pending: false,
      quitting: false,
    }
  }
}

impl PlaybackSession {
  pub fn handle(&mut self, input: PlaybackInput, now: Instant) -> Vec<PlaybackEffect> {
    match input {
      PlaybackInput::Intent(intent) => self.handle_intent(intent, now),
      PlaybackInput::Event(event) => self.handle_event(event, now),
    }
  }

  pub fn view(&self) -> SessionView {
    let now_playing = self.snapshot.as_ref().and_then(|snapshot| {
      snapshot.now_playing.as_ref().map(|item| {
        let duration_seconds = (snapshot.transport.duration.is_finite()
          && snapshot.transport.duration > 0.0)
          .then_some(snapshot.transport.duration)
          .or(item.runtime_seconds);
        NowPlayingView {
          item: item.clone(),
          paused: self.desired_paused.unwrap_or(snapshot.transport.paused),
          position_seconds: snapshot.transport.time_pos,
          duration_seconds,
          volume: snapshot.transport.volume,
          muted: self.desired_muted.unwrap_or(snapshot.transport.muted),
        }
      })
    });
    SessionView {
      now_playing,
      tracks: self.tracks.clone(),
      adjacent: self.adjacent.view(),
      intro_prompt: self
        .intro
        .active_prompt
        .as_ref()
        .map(|prompt| IntroPromptView { kind: prompt.kind }),
      notice: self.notice.clone(),
      engine_available: self.engine_available,
      busy: self.controller_busy(),
      can_start_login: !self.cleanup_pending,
      quit_may_proceed: self.quitting && !self.controller_busy() && !self.cleanup_pending,
    }
  }

  fn handle_intent(&mut self, intent: PlaybackIntent, now: Instant) -> Vec<PlaybackEffect> {
    match intent {
      PlaybackIntent::Start {
        item,
        position,
        intro,
      } => {
        if !self.engine_available || self.quitting {
          return Vec::new();
        }
        self.enqueue(ControllerRequest::start(item, position, intro))
      }
      PlaybackIntent::TogglePaused => {
        let Some(paused) = self.current_paused() else {
          return Vec::new();
        };
        self.desired_paused = Some(!paused);
        self.enqueue(ControllerRequest::controlled(
          RequestKind::Paused,
          ControllerCommand::SetPaused(!paused),
        ))
      }
      PlaybackIntent::SetPaused(paused) => {
        if self.snapshot.is_none() {
          return Vec::new();
        }
        self.desired_paused = Some(paused);
        self.enqueue(ControllerRequest::controlled(
          RequestKind::Paused,
          ControllerCommand::SetPaused(paused),
        ))
      }
      PlaybackIntent::Seek(position) => {
        if self.snapshot.is_none() {
          return Vec::new();
        }
        self.enqueue(ControllerRequest::controlled(
          RequestKind::Seek,
          ControllerCommand::Seek(position),
        ))
      }
      PlaybackIntent::SetVolume(volume) => {
        if self.snapshot.is_none() {
          return Vec::new();
        }
        self.enqueue(ControllerRequest::controlled(
          RequestKind::Volume,
          ControllerCommand::SetVolume(volume),
        ))
      }
      PlaybackIntent::SetMuted(muted) => {
        if self.snapshot.is_none() {
          return Vec::new();
        }
        self.desired_muted = Some(muted);
        self.enqueue(ControllerRequest::controlled(
          RequestKind::Muted,
          ControllerCommand::SetMuted(muted),
        ))
      }
      PlaybackIntent::SelectAudioTrack(id) => self.select_track(true, Some(id)),
      PlaybackIntent::SelectSubtitleTrack(id) => self.select_track(false, id),
      PlaybackIntent::Stop => self.enqueue(ControllerRequest::stop()),
      PlaybackIntent::PlayAdjacent(direction) => self.play_adjacent(direction),
      PlaybackIntent::SkipIntro => self.apply_intro_action(now, true),
      PlaybackIntent::Tick => {
        self.expire_intro_prompt(now);
        if self.snapshot.is_none() || self.quitting {
          Vec::new()
        } else {
          self.enqueue(ControllerRequest::refresh())
        }
      }
      PlaybackIntent::Disconnect => self.begin_teardown(false),
      PlaybackIntent::Quit => self.begin_teardown(true),
      PlaybackIntent::SetIntroMode(mode) => {
        if mode == IntroSkipMode::Off {
          self.intro.disable();
          self.pending_intro = None;
        } else {
          self.intro.mode = mode;
        }
        Vec::new()
      }
    }
  }

  fn handle_event(&mut self, event: PlaybackEvent, now: Instant) -> Vec<PlaybackEffect> {
    match event {
      PlaybackEvent::EngineAvailability(available) => {
        self.engine_available = available;
        Vec::new()
      }
      PlaybackEvent::ControllerSettled { id, settlement } => {
        self.settle_controller(id, settlement, now)
      }
      PlaybackEvent::IntroRangesSettled { id, result } => {
        if id.epoch != self.epoch || self.pending_intro != Some(id) {
          return Vec::new();
        }
        self.pending_intro = None;
        self.intro.ranges = result.unwrap_or_default();
        Vec::new()
      }
      PlaybackEvent::AdjacentSettled {
        id,
        direction,
        result,
      } => {
        let index = direction.index();
        if id.epoch != self.epoch || self.pending_adjacent[index] != Some(id) {
          return Vec::new();
        }
        self.pending_adjacent[index] = None;
        self.adjacent.set(direction, result);
        Vec::new()
      }
      PlaybackEvent::TracksSettled { id, result } => {
        if id.epoch != self.epoch || self.snapshot.is_none() {
          return Vec::new();
        }
        self.tracks = match result {
          Ok(tracks) => ready_tracks(tracks),
          Err(_) => TracksView::Unavailable,
        };
        Vec::new()
      }
    }
  }

  fn enqueue(&mut self, request: ControllerRequest) -> Vec<PlaybackEffect> {
    if self.controller_busy() {
      if request.kind == RequestKind::Refresh {
        return Vec::new();
      }
      self.queue_request(request);
      return Vec::new();
    }
    self.dispatch(request)
  }

  fn queue_request(&mut self, request: ControllerRequest) {
    if matches!(request.kind, RequestKind::Start | RequestKind::Stop) {
      self.pending.clear();
    } else {
      self.pending.retain(|pending| pending.kind != request.kind);
    }
    self.pending.push_back(request);
  }

  fn dispatch(&mut self, request: ControllerRequest) -> Vec<PlaybackEffect> {
    if request.kind == RequestKind::Start {
      self.bump_epoch();
      self.invalidate_auxiliary();
    }
    if matches!(
      request.kind,
      RequestKind::AudioTrack | RequestKind::SubtitleTrack
    ) {
      self.tracks = TracksView::Loading;
    }
    match &request.command {
      ControllerCommand::SetPaused(paused) => self.desired_paused = Some(*paused),
      ControllerCommand::SetMuted(muted) => self.desired_muted = Some(*muted),
      _ => {}
    }
    let id = self.next_effect_id();
    let ControllerRequest {
      command, operation, ..
    } = request;
    self.in_flight = Some(InFlight { id, operation });
    vec![PlaybackEffect::Controller(id, command)]
  }

  fn dispatch_next(&mut self) -> Vec<PlaybackEffect> {
    self
      .pending
      .pop_front()
      .map_or_else(Vec::new, |request| self.dispatch(request))
  }

  fn settle_controller(
    &mut self,
    id: EffectId,
    settlement: ControllerSettlement,
    now: Instant,
  ) -> Vec<PlaybackEffect> {
    if let Some(detached) = self.detached {
      if detached.id == id {
        self.detached = None;
        if detached.was_shutdown {
          self.cleanup_pending = false;
          if let ControllerSettlement::Shutdown(warnings) = settlement {
            self.set_warning_notice(warnings);
          }
          return Vec::new();
        }
        return self.dispatch_shutdown();
      }
    }
    if id.epoch != self.epoch || self.in_flight.as_ref().map(|effect| effect.id) != Some(id) {
      return Vec::new();
    }
    let Some(in_flight) = self.in_flight.take() else {
      return Vec::new();
    };
    let mut effects = self.apply_settlement(in_flight.operation, settlement, now);
    if self.in_flight.is_none() && !self.cleanup_pending && !self.quitting {
      effects.extend(self.dispatch_next());
    }
    effects
  }

  fn apply_settlement(
    &mut self,
    operation: ControllerOperation,
    settlement: ControllerSettlement,
    now: Instant,
  ) -> Vec<PlaybackEffect> {
    match (operation, settlement) {
      (ControllerOperation::Start { intro }, ControllerSettlement::Started(result)) => {
        self.finish_start(result, intro)
      }
      (ControllerOperation::Controlled, ControllerSettlement::Controlled(result)) => {
        self.finish_control(result);
        Vec::new()
      }
      (ControllerOperation::Stop, ControllerSettlement::Stopped(result)) => {
        self.finish_stop(result);
        Vec::new()
      }
      (
        ControllerOperation::Refresh,
        ControllerSettlement::Refreshed {
          outcome,
          client_messages,
        },
      ) => self.finish_refresh(outcome, &client_messages, now),
      (ControllerOperation::TrackSelection, ControllerSettlement::TrackSelected(result)) => {
        self.finish_track_selection(result);
        Vec::new()
      }
      (
        ControllerOperation::Prompt { range_index, kind },
        ControllerSettlement::OsdShown(result),
      ) => {
        match result {
          Ok(()) if self.intro.range_is_promptable(range_index) => {
            self.intro.active_prompt = Some(ActiveIntroPrompt {
              range_index,
              kind,
              expires_at: now + INTRO_PROMPT_DURATION,
            });
          }
          Ok(()) => {}
          Err(error) => self.notice = Some(PlaybackNotice::Failed(error)),
        }
        Vec::new()
      }
      (ControllerOperation::Osd, ControllerSettlement::OsdShown(result)) => {
        if let Err(error) = result {
          self.notice = Some(PlaybackNotice::Failed(error));
        }
        Vec::new()
      }
      (ControllerOperation::Shutdown, ControllerSettlement::Shutdown(warnings)) => {
        self.cleanup_pending = false;
        self.set_warning_notice(warnings);
        Vec::new()
      }
      _ => Vec::new(),
    }
  }

  fn finish_start(
    &mut self,
    result: Result<PlaybackOutcome, PlaybackError>,
    intro: IntroAvailability,
  ) -> Vec<PlaybackEffect> {
    match result {
      Ok(outcome) => {
        let PlaybackOutcome { snapshot, warnings } = outcome;
        self.snapshot = Some(snapshot);
        self.sync_desired_transport();
        self.tracks = TracksView::Unavailable;
        self.adjacent = AdjacentState::default();
        self.intro = IntroState {
          mode: intro.mode,
          skipper_available: intro.skipper_available,
          ..IntroState::default()
        };
        self.set_warning_notice(warnings);
        self.start_auxiliary(intro)
      }
      Err(error) => {
        if clears_snapshot_on_start_failure(error) {
          self.clear_playback_context();
        } else {
          self.sync_desired_transport();
        }
        self.notice = Some(PlaybackNotice::Failed(error));
        Vec::new()
      }
    }
  }

  fn finish_control(&mut self, result: Result<PlaybackOutcome, PlaybackError>) {
    match result {
      Ok(outcome) => {
        self.snapshot = Some(outcome.snapshot);
        self.sync_desired_transport();
        self.set_warning_notice(outcome.warnings);
      }
      Err(error) => {
        self.sync_desired_transport();
        self.notice = Some(PlaybackNotice::Failed(error));
      }
    }
  }

  fn finish_stop(&mut self, result: Result<PlaybackStopOutcome, PlaybackError>) {
    match result {
      Ok(_outcome) => {
        self.clear_playback_context();
        self.notice = Some(PlaybackNotice::Stopped);
      }
      Err(error) => {
        self.sync_desired_transport();
        self.notice = Some(PlaybackNotice::Failed(error));
      }
    }
  }

  fn finish_refresh(
    &mut self,
    outcome: PlaybackRefreshOutcome,
    client_messages: &[String],
    now: Instant,
  ) -> Vec<PlaybackEffect> {
    let PlaybackRefreshOutcome {
      snapshot,
      state,
      warnings,
    } = outcome;
    match state {
      PlaybackRefreshState::Active => {
        self.snapshot = Some(snapshot);
        self.sync_desired_transport();
        self.set_warning_notice(warnings);
        if let Some(direction) = adjacent_direction_from_client_messages(client_messages) {
          return self.play_adjacent(direction);
        }
        self.apply_intro_action(now, manual_intro_skip_requested(client_messages))
      }
      PlaybackRefreshState::Idle => {
        self.clear_playback_context();
        self.set_warning_notice(warnings);
        Vec::new()
      }
      PlaybackRefreshState::Ended(PlaybackEndReason::EndOfFile) => {
        self.clear_playback_context();
        self.notice = Some(PlaybackNotice::Finished);
        Vec::new()
      }
      PlaybackRefreshState::Ended(PlaybackEndReason::Error | PlaybackEndReason::Disconnected) => {
        self.clear_playback_context();
        self.notice = Some(PlaybackNotice::Failed(PlaybackError::MpvControlFailed));
        Vec::new()
      }
    }
  }

  fn finish_track_selection(&mut self, result: Result<TrackSelectionOutcome, PlaybackError>) {
    match result {
      Ok(outcome) => {
        self.tracks = ready_tracks(outcome.tracks);
        self.set_warning_notice(outcome.warnings);
      }
      Err(error) => {
        self.tracks = TracksView::Unavailable;
        self.notice = Some(PlaybackNotice::Failed(error));
      }
    }
  }

  fn start_auxiliary(&mut self, availability: IntroAvailability) -> Vec<PlaybackEffect> {
    let Some(item) = self
      .snapshot
      .as_ref()
      .and_then(|snapshot| snapshot.now_playing.as_ref())
    else {
      return Vec::new();
    };
    let item_id = item.item_id.clone();
    let is_episode = item.item_type == "Episode";
    let fetch_intro = should_fetch_intro_ranges(availability, &item.item_type);
    let mut effects = Vec::with_capacity(3);
    if is_episode {
      self.adjacent.previous = AdjacentSlot::Loading;
      self.adjacent.next = AdjacentSlot::Loading;
      for direction in [AdjacentDirection::Previous, AdjacentDirection::Next] {
        let id = self.next_effect_id();
        self.pending_adjacent[direction.index()] = Some(id);
        effects.push(PlaybackEffect::LookupAdjacent(id, direction));
      }
    }
    if fetch_intro {
      let id = self.next_effect_id();
      self.pending_intro = Some(id);
      effects.push(PlaybackEffect::FetchIntroRanges(id, item_id));
    }
    effects
  }

  fn select_track(&mut self, audio: bool, id: Option<i64>) -> Vec<PlaybackEffect> {
    let TracksView::Ready { tracks, .. } = &self.tracks else {
      return Vec::new();
    };
    let expected_type = if audio { "audio" } else { "sub" };
    if let Some(id) = id {
      if !tracks
        .iter()
        .any(|track| track.track_type == expected_type && track.id == id)
      {
        return Vec::new();
      }
    } else if audio {
      return Vec::new();
    }
    let (kind, command) = if audio {
      (
        RequestKind::AudioTrack,
        ControllerCommand::SelectAudioTrack(id.unwrap_or_default()),
      )
    } else {
      (
        RequestKind::SubtitleTrack,
        ControllerCommand::SelectSubtitleTrack(id),
      )
    };
    self.enqueue(ControllerRequest {
      kind,
      command,
      operation: ControllerOperation::TrackSelection,
    })
  }

  fn play_adjacent(&mut self, direction: AdjacentDirection) -> Vec<PlaybackEffect> {
    let Some(item) = self.adjacent.item(direction).cloned() else {
      return Vec::new();
    };
    self.enqueue(ControllerRequest::start(
      Playable::Media(item),
      PlaybackStartPosition::Beginning,
      IntroAvailability {
        mode: self.intro.mode,
        skipper_available: self.intro.skipper_available,
      },
    ))
  }

  fn apply_intro_action(&mut self, now: Instant, manual_requested: bool) -> Vec<PlaybackEffect> {
    let Some(position) = self
      .snapshot
      .as_ref()
      .map(|snapshot| snapshot.transport.time_pos)
    else {
      return Vec::new();
    };
    let active_prompt_range = self.active_intro_prompt_range(now);
    let Some(action) = evaluate_intro_ui_action(
      position,
      &mut self.intro.ranges,
      self.intro.mode,
      manual_requested,
      active_prompt_range,
    ) else {
      return Vec::new();
    };
    match action {
      IntroUiAction::Seek { target, .. } => self.enqueue(ControllerRequest::controlled(
        RequestKind::Seek,
        ControllerCommand::Seek(target),
      )),
      IntroUiAction::Prompt { range_index, kind } => self.enqueue(ControllerRequest::prompt(
        range_index,
        kind,
        "Skip available — use the JellyPilot skip-intro shortcut".to_owned(),
      )),
      IntroUiAction::ManualSkip {
        range_index,
        seek_target,
        ..
      } => {
        if self
          .intro
          .active_prompt
          .as_ref()
          .is_some_and(|prompt| prompt.range_index == range_index)
        {
          self.intro.active_prompt = None;
        }
        let effects = self.enqueue(ControllerRequest::controlled(
          RequestKind::Seek,
          ControllerCommand::Seek(seek_target),
        ));
        self.queue_request(ControllerRequest::osd(
          "Skipped segment".to_owned(),
          INTRO_CONFIRMATION_DURATION_MS,
        ));
        effects
      }
    }
  }

  fn active_intro_prompt_range(&mut self, now: Instant) -> Option<usize> {
    self.expire_intro_prompt(now);
    self
      .intro
      .active_prompt
      .as_ref()
      .map(|prompt| prompt.range_index)
  }

  fn expire_intro_prompt(&mut self, now: Instant) {
    if self
      .intro
      .active_prompt
      .as_ref()
      .is_some_and(|prompt| now >= prompt.expires_at)
    {
      self.intro.active_prompt = None;
    }
  }

  fn begin_teardown(&mut self, quitting: bool) -> Vec<PlaybackEffect> {
    if quitting {
      self.quitting = true;
    }
    let controller_owned = self.snapshot.is_some() || self.engine_available;
    self.bump_epoch();
    self.pending.clear();
    self.invalidate_auxiliary();
    if !quitting {
      self.clear_playback_context();
      self.notice = None;
    }
    if let Some(in_flight) = self.in_flight.take() {
      self.cleanup_pending = true;
      self.detached = Some(DetachedController {
        id: in_flight.id,
        was_shutdown: matches!(in_flight.operation, ControllerOperation::Shutdown),
      });
      Vec::new()
    } else if self.detached.is_some() {
      self.cleanup_pending = true;
      Vec::new()
    } else if controller_owned {
      self.dispatch_shutdown()
    } else {
      self.cleanup_pending = false;
      Vec::new()
    }
  }

  fn dispatch_shutdown(&mut self) -> Vec<PlaybackEffect> {
    let id = self.next_effect_id();
    self.in_flight = Some(InFlight {
      id,
      operation: ControllerOperation::Shutdown,
    });
    self.cleanup_pending = true;
    vec![PlaybackEffect::Controller(id, ControllerCommand::Shutdown)]
  }

  fn controller_busy(&self) -> bool {
    self.in_flight.is_some() || self.detached.is_some()
  }

  fn current_paused(&self) -> Option<bool> {
    self.desired_paused.or_else(|| {
      self
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.transport.paused)
    })
  }

  fn sync_desired_transport(&mut self) {
    if let Some(snapshot) = self.snapshot.as_ref() {
      self.desired_paused = Some(snapshot.transport.paused);
      self.desired_muted = Some(snapshot.transport.muted);
    } else {
      self.desired_paused = None;
      self.desired_muted = None;
    }
  }

  fn clear_playback_context(&mut self) {
    self.snapshot = None;
    self.desired_paused = None;
    self.desired_muted = None;
    self.tracks = TracksView::Unavailable;
    self.adjacent = AdjacentState::default();
    self.intro = IntroState::default();
    self.invalidate_auxiliary();
  }

  fn invalidate_auxiliary(&mut self) {
    self.pending_intro = None;
    self.pending_adjacent = [None, None];
  }

  fn set_warning_notice(&mut self, warnings: Vec<PlaybackWarning>) {
    self.notice = (!warnings.is_empty()).then_some(PlaybackNotice::Warnings(warnings));
  }

  fn bump_epoch(&mut self) {
    self.epoch = self.epoch.wrapping_add(1);
    self.sequence = 0;
  }

  fn next_effect_id(&mut self) -> EffectId {
    self.sequence = self.sequence.wrapping_add(1);
    EffectId {
      epoch: self.epoch,
      sequence: self.sequence,
    }
  }
}

#[derive(Clone, Copy)]
struct DetachedController {
  id: EffectId,
  was_shutdown: bool,
}

struct InFlight {
  id: EffectId,
  operation: ControllerOperation,
}

enum ControllerOperation {
  Start {
    intro: IntroAvailability,
  },
  Controlled,
  Stop,
  Refresh,
  TrackSelection,
  Prompt {
    range_index: usize,
    kind: IntroSkipKind,
  },
  Osd,
  Shutdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequestKind {
  Start,
  Paused,
  Seek,
  Volume,
  Muted,
  AudioTrack,
  SubtitleTrack,
  ShowText,
  Stop,
  Refresh,
}

struct ControllerRequest {
  kind: RequestKind,
  command: ControllerCommand,
  operation: ControllerOperation,
}

impl ControllerRequest {
  fn start(item: Playable, position: PlaybackStartPosition, intro: IntroAvailability) -> Self {
    Self {
      kind: RequestKind::Start,
      command: ControllerCommand::Start { item, position },
      operation: ControllerOperation::Start { intro },
    }
  }

  fn controlled(kind: RequestKind, command: ControllerCommand) -> Self {
    Self {
      kind,
      command,
      operation: ControllerOperation::Controlled,
    }
  }

  fn stop() -> Self {
    Self {
      kind: RequestKind::Stop,
      command: ControllerCommand::Stop,
      operation: ControllerOperation::Stop,
    }
  }

  fn refresh() -> Self {
    Self {
      kind: RequestKind::Refresh,
      command: ControllerCommand::Refresh,
      operation: ControllerOperation::Refresh,
    }
  }

  fn prompt(range_index: usize, kind: IntroSkipKind, text: String) -> Self {
    Self {
      kind: RequestKind::ShowText,
      command: ControllerCommand::ShowText {
        text,
        duration_ms: INTRO_PROMPT_DURATION_MS,
      },
      operation: ControllerOperation::Prompt { range_index, kind },
    }
  }

  fn osd(text: String, duration_ms: i64) -> Self {
    Self {
      kind: RequestKind::ShowText,
      command: ControllerCommand::ShowText { text, duration_ms },
      operation: ControllerOperation::Osd,
    }
  }
}

#[derive(Default)]
struct AdjacentState {
  previous: AdjacentSlot,
  next: AdjacentSlot,
}

impl AdjacentState {
  fn slot(&self, direction: AdjacentDirection) -> &AdjacentSlot {
    match direction {
      AdjacentDirection::Previous => &self.previous,
      AdjacentDirection::Next => &self.next,
    }
  }

  fn set(&mut self, direction: AdjacentDirection, result: Result<Option<MediaItem>, ()>) {
    let slot = match result {
      Ok(Some(item)) => AdjacentSlot::Available(item),
      Ok(None) | Err(()) => AdjacentSlot::Unavailable,
    };
    match direction {
      AdjacentDirection::Previous => self.previous = slot,
      AdjacentDirection::Next => self.next = slot,
    }
  }

  fn item(&self, direction: AdjacentDirection) -> Option<&MediaItem> {
    match self.slot(direction) {
      AdjacentSlot::Available(item) => Some(item),
      AdjacentSlot::Idle | AdjacentSlot::Loading | AdjacentSlot::Unavailable => None,
    }
  }

  fn view(&self) -> AdjacentView {
    AdjacentView {
      previous: self.previous.view(),
      next: self.next.view(),
    }
  }
}

#[derive(Default)]
#[allow(clippy::large_enum_variant)]
enum AdjacentSlot {
  #[default]
  Idle,
  Loading,
  Available(MediaItem),
  Unavailable,
}

impl AdjacentSlot {
  fn view(&self) -> AdjacentAvailability {
    match self {
      Self::Idle => AdjacentAvailability::Idle,
      Self::Loading => AdjacentAvailability::Loading,
      Self::Available(item) => AdjacentAvailability::Available {
        title: item.name.clone(),
      },
      Self::Unavailable => AdjacentAvailability::Unavailable,
    }
  }
}

struct IntroState {
  mode: IntroSkipMode,
  skipper_available: bool,
  ranges: Vec<IntroSkipRange>,
  active_prompt: Option<ActiveIntroPrompt>,
}

impl Default for IntroState {
  fn default() -> Self {
    Self {
      mode: IntroSkipMode::Off,
      skipper_available: false,
      ranges: Vec::new(),
      active_prompt: None,
    }
  }
}

impl IntroState {
  fn range_is_promptable(&self, range_index: usize) -> bool {
    self.mode == IntroSkipMode::Manual
      && self
        .ranges
        .get(range_index)
        .is_some_and(|range| range.notified && !range.skipped)
  }

  fn disable(&mut self) {
    self.mode = IntroSkipMode::Off;
    self.ranges.clear();
    self.active_prompt = None;
  }
}

struct ActiveIntroPrompt {
  range_index: usize,
  kind: IntroSkipKind,
  expires_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum IntroUiAction {
  Seek {
    range_index: usize,
    target: f64,
  },
  Prompt {
    range_index: usize,
    kind: IntroSkipKind,
  },
  ManualSkip {
    range_index: usize,
    kind: IntroSkipKind,
    seek_target: f64,
  },
}

fn evaluate_intro_ui_action(
  position_seconds: f64,
  ranges: &mut [IntroSkipRange],
  mode: IntroSkipMode,
  manual_requested: bool,
  active_prompt_range: Option<usize>,
) -> Option<IntroUiAction> {
  let range_index = ranges.iter().position(|range| {
    !range.skipped
      && position_seconds.is_finite()
      && position_seconds >= range.start_seconds
      && position_seconds < range.end_seconds
  })?;
  let range = std::slice::from_mut(&mut ranges[range_index]);
  if manual_requested {
    if mode != IntroSkipMode::Manual || active_prompt_range != Some(range_index) {
      return None;
    }
    return evaluate_manual_skip(position_seconds, range).map(|decision| {
      IntroUiAction::ManualSkip {
        range_index,
        kind: decision.kind,
        seek_target: decision.seek_target,
      }
    });
  }
  evaluate_intro_skip(position_seconds, range, mode).map(|action| match action {
    IntroSkipAction::Seek(target) => IntroUiAction::Seek {
      range_index,
      target,
    },
    IntroSkipAction::ShowPrompt(kind) => IntroUiAction::Prompt { range_index, kind },
  })
}

fn should_fetch_intro_ranges(availability: IntroAvailability, item_type: &str) -> bool {
  availability.mode != IntroSkipMode::Off
    && availability.skipper_available
    && item_type == "Episode"
}

fn manual_intro_skip_requested(messages: &[String]) -> bool {
  messages
    .iter()
    .any(|message| message == "jellypilot-skip-intro")
}

fn adjacent_direction_from_client_messages(messages: &[String]) -> Option<AdjacentDirection> {
  messages.iter().find_map(|message| match message.as_str() {
    "jellypilot-next" => Some(AdjacentDirection::Next),
    "jellypilot-prev" => Some(AdjacentDirection::Previous),
    _ => None,
  })
}

const fn clears_snapshot_on_start_failure(error: PlaybackError) -> bool {
  matches!(
    error,
    PlaybackError::MpvStartFailed | PlaybackError::MpvLoadFailed
  )
}

fn ready_tracks(tracks: Vec<TrackInfo>) -> TracksView {
  let audio = tracks
    .iter()
    .find(|track| track.track_type == "audio" && track.selected)
    .map(|track| track.id);
  let subtitle = tracks
    .iter()
    .find(|track| track.track_type == "sub" && track.selected)
    .map(|track| track.id);
  TracksView::Ready {
    tracks,
    audio,
    subtitle,
  }
}

#[cfg(test)]
mod tests {
  use crate::PlayerState;

  use super::*;
  use crate::playback::PlaybackSnapshot;

  fn instant() -> Instant {
    Instant::now()
  }

  fn media_item(id: &str, name: &str) -> MediaItem {
    MediaItem {
      id: id.to_owned(),
      name: name.to_owned(),
      item_type: "Episode".to_owned(),
      series_id: Some("series-1".to_owned()),
      series_name: Some("Series".to_owned()),
      season_name: Some("Season 1".to_owned()),
      index_number: Some(2),
      parent_index_number: Some(1),
      run_time_ticks: Some(1_500 * 10_000_000),
      overview: None,
      series_primary_image_tag: None,
    }
  }

  fn snapshot(item_id: &str, item_type: &str, position: f64) -> PlaybackSnapshot {
    PlaybackSnapshot {
      now_playing: Some(NowPlayingItem {
        item_id: item_id.to_owned(),
        title: "Pilot".to_owned(),
        item_type: item_type.to_owned(),
        runtime_seconds: Some(1_500.0),
        start_position_seconds: 0.0,
        play_method: "DirectPlay".to_owned(),
      }),
      transport: PlayerState {
        connected: true,
        paused: false,
        muted: false,
        time_pos: position,
        duration: 1_500.0,
        volume: 75.0,
      },
    }
  }

  fn intro_availability(mode: IntroSkipMode) -> IntroAvailability {
    IntroAvailability {
      mode,
      skipper_available: true,
    }
  }

  fn intro_range() -> IntroSkipRange {
    IntroSkipRange {
      kind: IntroSkipKind::Introduction,
      start_seconds: 10.0,
      end_seconds: 30.0,
      notified: false,
      skipped: false,
    }
  }

  fn controller_effect(effects: Vec<PlaybackEffect>) -> (EffectId, ControllerCommand) {
    assert_eq!(effects.len(), 1);
    match effects.into_iter().next() {
      Some(PlaybackEffect::Controller(id, command)) => (id, command),
      Some(PlaybackEffect::FetchIntroRanges(_, _) | PlaybackEffect::LookupAdjacent(_, _))
      | None => {
        panic!("expected one controller effect")
      }
    }
  }

  fn start_command(session: &mut PlaybackSession, now: Instant, mode: IntroSkipMode) -> EffectId {
    session.handle(
      PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
      now,
    );
    let (id, command) = controller_effect(session.handle(
      PlaybackInput::Intent(PlaybackIntent::Start {
        item: Playable::Media(media_item("episode-1", "Pilot")),
        position: PlaybackStartPosition::Beginning,
        intro: intro_availability(mode),
      }),
      now,
    ));
    assert!(matches!(command, ControllerCommand::Start { .. }));
    id
  }

  fn settle_start(
    session: &mut PlaybackSession,
    id: EffectId,
    now: Instant,
    item_type: &str,
  ) -> Vec<PlaybackEffect> {
    session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id,
        settlement: ControllerSettlement::Started(Ok(PlaybackOutcome {
          snapshot: snapshot("episode-1", item_type, 0.0),
          warnings: Vec::new(),
        })),
      }),
      now,
    )
  }

  fn start_session(mode: IntroSkipMode) -> (PlaybackSession, Instant, Vec<PlaybackEffect>) {
    let now = instant();
    let mut session = PlaybackSession::default();
    let id = start_command(&mut session, now, mode);
    let effects = settle_start(&mut session, id, now, "Episode");
    (session, now, effects)
  }

  fn intro_fetch_id(effects: &[PlaybackEffect]) -> EffectId {
    effects
      .iter()
      .find_map(|effect| match effect {
        PlaybackEffect::FetchIntroRanges(id, _) => Some(*id),
        PlaybackEffect::Controller(_, _) | PlaybackEffect::LookupAdjacent(_, _) => None,
      })
      .expect("start should fetch intro ranges")
  }

  fn adjacent_id(effects: &[PlaybackEffect], expected: AdjacentDirection) -> EffectId {
    effects
      .iter()
      .find_map(|effect| match effect {
        PlaybackEffect::LookupAdjacent(id, direction) if *direction == expected => Some(*id),
        PlaybackEffect::Controller(_, _)
        | PlaybackEffect::FetchIntroRanges(_, _)
        | PlaybackEffect::LookupAdjacent(_, _) => None,
      })
      .expect("start should look up adjacent episodes")
  }

  fn settle_intro_ranges(session: &mut PlaybackSession, id: EffectId, now: Instant) {
    let effects = session.handle(
      PlaybackInput::Event(PlaybackEvent::IntroRangesSettled {
        id,
        result: Ok(vec![intro_range()]),
      }),
      now,
    );
    assert!(effects.is_empty());
  }

  fn refresh_at(
    session: &mut PlaybackSession,
    now: Instant,
    position: f64,
    messages: Vec<String>,
  ) -> Vec<PlaybackEffect> {
    let (id, command) =
      controller_effect(session.handle(PlaybackInput::Intent(PlaybackIntent::Tick), now));
    assert!(matches!(command, ControllerCommand::Refresh));
    session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id,
        settlement: ControllerSettlement::Refreshed {
          outcome: PlaybackRefreshOutcome {
            snapshot: snapshot("episode-1", "Episode", position),
            state: PlaybackRefreshState::Active,
            warnings: Vec::new(),
          },
          client_messages: messages,
        },
      }),
      now,
    )
  }

  #[test]
  fn queue_coalesces_each_controller_request_kind() {
    let (mut session, now, _) = start_session(IntroSkipMode::Off);
    let (busy_id, _) =
      controller_effect(session.handle(PlaybackInput::Intent(PlaybackIntent::Seek(1.0)), now));

    session.handle(PlaybackInput::Intent(PlaybackIntent::SetVolume(10.0)), now);
    session.handle(PlaybackInput::Intent(PlaybackIntent::Seek(2.0)), now);
    session.handle(PlaybackInput::Intent(PlaybackIntent::SetVolume(20.0)), now);

    assert_eq!(
      session
        .pending
        .iter()
        .map(|request| request.kind)
        .collect::<Vec<_>>(),
      vec![RequestKind::Seek, RequestKind::Volume]
    );
    assert_eq!(
      session.in_flight.as_ref().map(|effect| effect.id),
      Some(busy_id)
    );
  }

  #[test]
  fn seek_queued_behind_refresh_reaches_the_controller_after_refresh_settles() {
    let (mut session, now, _) = start_session(IntroSkipMode::Off);
    let (refresh_id, command) =
      controller_effect(session.handle(PlaybackInput::Intent(PlaybackIntent::Tick), now));
    assert!(matches!(command, ControllerCommand::Refresh));

    let queued = session.handle(PlaybackInput::Intent(PlaybackIntent::Seek(120.0)), now);
    assert!(queued.is_empty());

    let effects = session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: refresh_id,
        settlement: ControllerSettlement::Refreshed {
          outcome: PlaybackRefreshOutcome {
            snapshot: snapshot("episode-1", "Episode", 10.0),
            state: PlaybackRefreshState::Active,
            warnings: Vec::new(),
          },
          client_messages: Vec::new(),
        },
      }),
      now,
    );
    let (_, command) = controller_effect(effects);

    assert!(matches!(
      command,
      ControllerCommand::Seek(position) if position == 120.0
    ));
  }

  #[test]
  fn volume_queued_behind_refresh_reaches_the_controller_after_refresh_settles() {
    let (mut session, now, _) = start_session(IntroSkipMode::Off);
    let (refresh_id, _) =
      controller_effect(session.handle(PlaybackInput::Intent(PlaybackIntent::Tick), now));

    let queued = session.handle(PlaybackInput::Intent(PlaybackIntent::SetVolume(42.0)), now);
    assert!(queued.is_empty());

    let effects = session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: refresh_id,
        settlement: ControllerSettlement::Refreshed {
          outcome: PlaybackRefreshOutcome {
            snapshot: snapshot("episode-1", "Episode", 10.0),
            state: PlaybackRefreshState::Active,
            warnings: Vec::new(),
          },
          client_messages: Vec::new(),
        },
      }),
      now,
    );
    let (_, command) = controller_effect(effects);

    assert!(matches!(
      command,
      ControllerCommand::SetVolume(volume) if volume == 42.0
    ));
  }

  #[test]
  fn start_while_busy_is_queued_until_the_controller_settles() {
    let (mut session, now, _) = start_session(IntroSkipMode::Off);
    let (busy_id, _) =
      controller_effect(session.handle(PlaybackInput::Intent(PlaybackIntent::Seek(1.0)), now));

    let queued = session.handle(
      PlaybackInput::Intent(PlaybackIntent::Start {
        item: Playable::Media(media_item("episode-2", "Second")),
        position: PlaybackStartPosition::Beginning,
        intro: intro_availability(IntroSkipMode::Off),
      }),
      now,
    );
    assert!(queued.is_empty());

    let effects = session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: busy_id,
        settlement: ControllerSettlement::Controlled(Ok(PlaybackOutcome {
          snapshot: snapshot("episode-1", "Episode", 1.0),
          warnings: Vec::new(),
        })),
      }),
      now,
    );
    let (_, command) = controller_effect(effects);
    assert!(matches!(command, ControllerCommand::Start { .. }));
  }

  #[test]
  fn stop_flushes_every_queued_request() {
    let (mut session, now, _) = start_session(IntroSkipMode::Off);
    let _ = session.handle(PlaybackInput::Intent(PlaybackIntent::Seek(1.0)), now);
    let _ = session.handle(PlaybackInput::Intent(PlaybackIntent::SetVolume(20.0)), now);
    let _ = session.handle(PlaybackInput::Intent(PlaybackIntent::SetMuted(true)), now);

    let effects = session.handle(PlaybackInput::Intent(PlaybackIntent::Stop), now);

    assert!(effects.is_empty());
    assert_eq!(session.pending.len(), 1);
    assert_eq!(
      session.pending.front().map(|request| request.kind),
      Some(RequestKind::Stop)
    );
  }

  #[test]
  fn stale_controller_settlement_is_dropped_without_changing_busy_state() {
    let now = instant();
    let mut session = PlaybackSession::default();
    let id = start_command(&mut session, now, IntroSkipMode::Off);
    let stale = EffectId {
      epoch: id.epoch,
      sequence: id.sequence.wrapping_add(1),
    };

    let effects = settle_start(&mut session, stale, now, "Episode");

    assert!(effects.is_empty());
    assert!(session.view().busy);
    assert!(session.view().now_playing.is_none());
  }

  #[test]
  fn eof_refresh_clears_playback_and_reports_finished() {
    let (mut session, now, _) = start_session(IntroSkipMode::Off);
    let (id, _) =
      controller_effect(session.handle(PlaybackInput::Intent(PlaybackIntent::Tick), now));

    let effects = session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id,
        settlement: ControllerSettlement::Refreshed {
          outcome: PlaybackRefreshOutcome {
            snapshot: snapshot("episode-1", "Episode", 1_500.0),
            state: PlaybackRefreshState::Ended(PlaybackEndReason::EndOfFile),
            warnings: Vec::new(),
          },
          client_messages: Vec::new(),
        },
      }),
      now,
    );

    assert!(effects.is_empty());
    assert!(session.view().now_playing.is_none());
    assert_eq!(session.view().notice, Some(PlaybackNotice::Finished));
  }

  #[test]
  fn adjacent_shortcut_preempts_intro_skip_on_the_same_refresh() {
    let (mut session, now, auxiliary) = start_session(IntroSkipMode::Automatic);
    settle_intro_ranges(&mut session, intro_fetch_id(&auxiliary), now);
    let next_id = adjacent_id(&auxiliary, AdjacentDirection::Next);
    session.handle(
      PlaybackInput::Event(PlaybackEvent::AdjacentSettled {
        id: next_id,
        direction: AdjacentDirection::Next,
        result: Ok(Some(media_item("episode-2", "Second"))),
      }),
      now,
    );

    let effects = refresh_at(
      &mut session,
      now,
      10.0,
      vec![
        "jellypilot-next".to_owned(),
        "jellypilot-skip-intro".to_owned(),
      ],
    );

    let (_, command) = controller_effect(effects);
    assert!(matches!(command, ControllerCommand::Start { .. }));
    assert!(!session.intro.ranges[0].skipped);
  }

  #[test]
  fn automatic_skip_fires_once_at_the_exact_start_boundary() {
    let (mut session, now, auxiliary) = start_session(IntroSkipMode::Automatic);
    settle_intro_ranges(&mut session, intro_fetch_id(&auxiliary), now);

    let effects = refresh_at(&mut session, now, 10.0, Vec::new());

    let (_, command) = controller_effect(effects);
    assert!(matches!(command, ControllerCommand::Seek(target) if target == 30.0));
    assert!(session.intro.ranges[0].skipped);
  }

  #[test]
  fn seeking_back_does_not_skip_an_automatic_range_twice() {
    let (mut session, now, auxiliary) = start_session(IntroSkipMode::Automatic);
    settle_intro_ranges(&mut session, intro_fetch_id(&auxiliary), now);
    let (seek_id, _) = controller_effect(refresh_at(&mut session, now, 10.0, Vec::new()));
    session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: seek_id,
        settlement: ControllerSettlement::Controlled(Ok(PlaybackOutcome {
          snapshot: snapshot("episode-1", "Episode", 30.0),
          warnings: Vec::new(),
        })),
      }),
      now,
    );

    let effects = refresh_at(&mut session, now, 10.0, Vec::new());

    assert!(effects.is_empty());
  }

  #[test]
  fn manual_skip_requires_a_live_prompt() {
    let (mut session, now, auxiliary) = start_session(IntroSkipMode::Manual);
    settle_intro_ranges(&mut session, intro_fetch_id(&auxiliary), now);
    let effects = refresh_at(&mut session, now, 10.0, Vec::new());
    let (prompt_id, command) = controller_effect(effects);
    assert!(matches!(command, ControllerCommand::ShowText { .. }));
    assert!(session
      .handle(PlaybackInput::Intent(PlaybackIntent::SkipIntro), now)
      .is_empty());
    session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: prompt_id,
        settlement: ControllerSettlement::OsdShown(Ok(())),
      }),
      now,
    );

    let effects = session.handle(PlaybackInput::Intent(PlaybackIntent::SkipIntro), now);

    let (_, command) = controller_effect(effects);
    assert!(matches!(command, ControllerCommand::Seek(target) if target == 30.0));
  }

  #[test]
  fn tick_expires_the_intro_prompt_at_its_deadline() {
    let (mut session, now, auxiliary) = start_session(IntroSkipMode::Manual);
    settle_intro_ranges(&mut session, intro_fetch_id(&auxiliary), now);
    let (prompt_id, _) = controller_effect(refresh_at(&mut session, now, 10.0, Vec::new()));
    session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: prompt_id,
        settlement: ControllerSettlement::OsdShown(Ok(())),
      }),
      now,
    );

    session.handle(
      PlaybackInput::Intent(PlaybackIntent::Tick),
      now + INTRO_PROMPT_DURATION,
    );

    assert!(session.view().intro_prompt.is_none());
  }

  #[test]
  fn set_intro_mode_off_purges_live_ranges_and_prompt() {
    let (mut session, now, auxiliary) = start_session(IntroSkipMode::Manual);
    settle_intro_ranges(&mut session, intro_fetch_id(&auxiliary), now);
    let (prompt_id, _) = controller_effect(refresh_at(&mut session, now, 10.0, Vec::new()));
    session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: prompt_id,
        settlement: ControllerSettlement::OsdShown(Ok(())),
      }),
      now,
    );
    assert!(session.view().intro_prompt.is_some());

    let effects = session.handle(
      PlaybackInput::Intent(PlaybackIntent::SetIntroMode(IntroSkipMode::Off)),
      now,
    );

    assert!(effects.is_empty());
    assert!(session.view().intro_prompt.is_none());
    assert!(session.intro.ranges.is_empty());
    assert_eq!(session.intro.mode, IntroSkipMode::Off);
    assert!(session.pending_intro.is_none());
  }

  #[test]
  fn set_intro_mode_updates_live_tick_evaluation() {
    let (mut session, now, auxiliary) = start_session(IntroSkipMode::Automatic);
    settle_intro_ranges(&mut session, intro_fetch_id(&auxiliary), now);

    session.handle(
      PlaybackInput::Intent(PlaybackIntent::SetIntroMode(IntroSkipMode::Manual)),
      now,
    );
    let effects = refresh_at(&mut session, now, 10.0, Vec::new());

    let (_, command) = controller_effect(effects);
    assert!(matches!(command, ControllerCommand::ShowText { .. }));
    assert!(!session.intro.ranges[0].skipped);
  }
  #[test]
  fn intro_mode_off_never_fetches_ranges() {
    let (_, _, auxiliary) = start_session(IntroSkipMode::Off);

    assert!(!auxiliary
      .iter()
      .any(|effect| matches!(effect, PlaybackEffect::FetchIntroRanges(_, _))));
  }

  #[test]
  fn intro_fetch_requires_skipper_capability() {
    let now = instant();
    let mut session = PlaybackSession::default();
    session.handle(
      PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
      now,
    );
    let (id, _) = controller_effect(session.handle(
      PlaybackInput::Intent(PlaybackIntent::Start {
        item: Playable::Media(media_item("episode-1", "Pilot")),
        position: PlaybackStartPosition::Beginning,
        intro: IntroAvailability {
          mode: IntroSkipMode::Automatic,
          skipper_available: false,
        },
      }),
      now,
    ));

    let auxiliary = settle_start(&mut session, id, now, "Episode");

    assert!(!auxiliary
      .iter()
      .any(|effect| matches!(effect, PlaybackEffect::FetchIntroRanges(_, _))));
  }

  #[test]
  fn intro_fetch_requires_an_episode() {
    let now = instant();
    let mut session = PlaybackSession::default();
    let id = start_command(&mut session, now, IntroSkipMode::Automatic);

    let auxiliary = settle_start(&mut session, id, now, "Movie");

    assert!(auxiliary.is_empty());
  }

  #[test]
  fn disconnect_wipes_session_state_and_bumps_epoch() {
    let (mut session, now, _) = start_session(IntroSkipMode::Off);
    let old_epoch = session.epoch;

    session.handle(PlaybackInput::Intent(PlaybackIntent::Disconnect), now);

    assert!(session.view().now_playing.is_none());
    assert_eq!(session.epoch, old_epoch.wrapping_add(1));
    assert_eq!(
      session.view().adjacent,
      AdjacentView {
        previous: AdjacentAvailability::Idle,
        next: AdjacentAvailability::Idle,
      }
    );
  }

  #[test]
  fn quit_holds_until_the_in_flight_controller_is_shut_down() {
    let now = instant();
    let mut session = PlaybackSession::default();
    let start_id = start_command(&mut session, now, IntroSkipMode::Off);
    session.handle(PlaybackInput::Intent(PlaybackIntent::Quit), now);
    assert!(!session.view().quit_may_proceed);

    let effects = settle_start(&mut session, start_id, now, "Episode");
    let (shutdown_id, command) = controller_effect(effects);
    assert!(matches!(command, ControllerCommand::Shutdown));
    assert!(!session.view().quit_may_proceed);

    session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: shutdown_id,
        settlement: ControllerSettlement::Shutdown(Vec::new()),
      }),
      now,
    );
    assert!(session.view().quit_may_proceed);
  }

  #[test]
  fn quit_while_idle_may_proceed_immediately() {
    let now = instant();
    let mut session = PlaybackSession::default();

    let effects = session.handle(PlaybackInput::Intent(PlaybackIntent::Quit), now);

    assert!(effects.is_empty());
    assert!(session.view().quit_may_proceed);
  }
  #[test]
  fn disconnect_with_owned_engine_shuts_down_the_idle_controller() {
    let now = instant();
    let mut session = PlaybackSession::default();
    session.handle(
      PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
      now,
    );

    let effects = session.handle(PlaybackInput::Intent(PlaybackIntent::Disconnect), now);

    let (shutdown_id, command) = controller_effect(effects);
    assert!(matches!(command, ControllerCommand::Shutdown));
    assert!(!session.view().can_start_login);

    session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: shutdown_id,
        settlement: ControllerSettlement::Shutdown(Vec::new()),
      }),
      now,
    );
    assert!(session.view().can_start_login);
  }

  #[test]
  fn stale_tracks_settlement_is_dropped() {
    let (mut session, now, _) = start_session(IntroSkipMode::Off);
    assert!(!matches!(session.view().tracks, TracksView::Ready { .. }));

    let stale = EffectId {
      epoch: session.epoch + 1,
      sequence: 0,
    };
    session.handle(
      PlaybackInput::Event(PlaybackEvent::TracksSettled {
        id: stale,
        result: Ok(Vec::new()),
      }),
      now,
    );
    assert!(!matches!(session.view().tracks, TracksView::Ready { .. }));

    let current = EffectId {
      epoch: session.epoch,
      sequence: 0,
    };
    session.handle(
      PlaybackInput::Event(PlaybackEvent::TracksSettled {
        id: current,
        result: Ok(Vec::new()),
      }),
      now,
    );
    assert!(matches!(session.view().tracks, TracksView::Ready { .. }));
  }

  #[test]
  fn login_is_blocked_while_disconnect_cleanup_is_pending() {
    let now = instant();
    let mut session = PlaybackSession::default();
    let start_id = start_command(&mut session, now, IntroSkipMode::Off);

    session.handle(PlaybackInput::Intent(PlaybackIntent::Disconnect), now);

    assert!(!session.view().can_start_login);
    let effects = settle_start(&mut session, start_id, now, "Episode");
    let (shutdown_id, _) = controller_effect(effects);
    session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: shutdown_id,
        settlement: ControllerSettlement::Shutdown(Vec::new()),
      }),
      now,
    );
    assert!(session.view().can_start_login);
  }

  #[test]
  fn start_is_a_defensive_no_op_when_the_engine_is_unavailable() {
    let now = instant();
    let mut session = PlaybackSession::default();

    let effects = session.handle(
      PlaybackInput::Intent(PlaybackIntent::Start {
        item: Playable::Media(media_item("episode-1", "Pilot")),
        position: PlaybackStartPosition::Beginning,
        intro: intro_availability(IntroSkipMode::Automatic),
      }),
      now,
    );

    assert!(effects.is_empty());
    assert_eq!(session.epoch, 0);
    assert!(session.view().notice.is_none());
  }

  #[test]
  fn adjacent_settlement_exposes_only_availability_and_title() {
    let (mut session, now, auxiliary) = start_session(IntroSkipMode::Off);
    let id = adjacent_id(&auxiliary, AdjacentDirection::Previous);

    session.handle(
      PlaybackInput::Event(PlaybackEvent::AdjacentSettled {
        id,
        direction: AdjacentDirection::Previous,
        result: Ok(Some(media_item("episode-0", "Previous"))),
      }),
      now,
    );

    assert_eq!(
      session.view().adjacent.previous,
      AdjacentAvailability::Available {
        title: "Previous".to_owned(),
      }
    );
  }

  #[test]
  fn play_adjacent_starts_the_stored_media_without_a_stop_effect() {
    let (mut session, now, auxiliary) = start_session(IntroSkipMode::Off);
    let id = adjacent_id(&auxiliary, AdjacentDirection::Next);
    session.handle(
      PlaybackInput::Event(PlaybackEvent::AdjacentSettled {
        id,
        direction: AdjacentDirection::Next,
        result: Ok(Some(media_item("episode-2", "Second"))),
      }),
      now,
    );

    let effects = session.handle(
      PlaybackInput::Intent(PlaybackIntent::PlayAdjacent(AdjacentDirection::Next)),
      now,
    );

    let (_, command) = controller_effect(effects);
    assert!(matches!(
      command,
      ControllerCommand::Start {
        item: Playable::Media(MediaItem { id, .. }),
        ..
      } if id == "episode-2"
    ));
  }

  #[test]
  fn stale_auxiliary_settlement_does_not_replace_adjacent_state() {
    let (mut session, now, auxiliary) = start_session(IntroSkipMode::Off);
    let id = adjacent_id(&auxiliary, AdjacentDirection::Next);
    session.handle(PlaybackInput::Intent(PlaybackIntent::Disconnect), now);

    session.handle(
      PlaybackInput::Event(PlaybackEvent::AdjacentSettled {
        id,
        direction: AdjacentDirection::Next,
        result: Ok(Some(media_item("episode-2", "Second"))),
      }),
      now,
    );

    assert_eq!(session.view().adjacent.next, AdjacentAvailability::Idle);
  }

  #[test]
  fn adjacent_client_message_mapping_uses_the_first_known_direction() {
    assert_eq!(
      adjacent_direction_from_client_messages(&[
        "unrelated".to_owned(),
        "jellypilot-next".to_owned(),
        "jellypilot-prev".to_owned(),
      ]),
      Some(AdjacentDirection::Next)
    );
  }

  #[test]
  fn intro_client_message_mapping_ignores_unrelated_messages() {
    assert!(!manual_intro_skip_requested(&[
      "jellypilot-next".to_owned(),
      "unrelated".to_owned(),
    ]));
    assert!(manual_intro_skip_requested(&[
      "unrelated".to_owned(),
      "jellypilot-skip-intro".to_owned(),
    ]));
  }

  #[test]
  fn only_mpv_start_and_load_failures_clear_the_previous_snapshot() {
    assert!(clears_snapshot_on_start_failure(
      PlaybackError::MpvStartFailed
    ));
    assert!(clears_snapshot_on_start_failure(
      PlaybackError::MpvLoadFailed
    ));
    assert!(!clears_snapshot_on_start_failure(
      PlaybackError::PlaybackInfoUnavailable
    ));
  }

  #[test]
  fn non_transport_start_failure_preserves_the_previous_snapshot() {
    let (mut session, now, _) = start_session(IntroSkipMode::Off);
    let id = start_command(&mut session, now, IntroSkipMode::Off);

    session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id,
        settlement: ControllerSettlement::Started(Err(PlaybackError::PlaybackInfoUnavailable)),
      }),
      now,
    );

    assert!(session.view().now_playing.is_some());
  }

  #[test]
  fn mpv_load_start_failure_clears_the_previous_snapshot() {
    let (mut session, now, _) = start_session(IntroSkipMode::Off);
    let id = start_command(&mut session, now, IntroSkipMode::Off);

    session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id,
        settlement: ControllerSettlement::Started(Err(PlaybackError::MpvLoadFailed)),
      }),
      now,
    );

    assert!(session.view().now_playing.is_none());
  }

  #[test]
  fn paused_and_muted_views_are_optimistic_then_reconciled() {
    let (mut session, now, _) = start_session(IntroSkipMode::Off);
    let (id, _) = controller_effect(
      session.handle(PlaybackInput::Intent(PlaybackIntent::SetPaused(true)), now),
    );
    session.handle(PlaybackInput::Intent(PlaybackIntent::SetMuted(true)), now);
    let view = session.view().now_playing.expect("active playback");
    assert!(view.paused);
    assert!(view.muted);

    session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id,
        settlement: ControllerSettlement::Controlled(Err(PlaybackError::MpvControlFailed)),
      }),
      now,
    );

    let view = session.view().now_playing.expect("preserved playback");
    assert!(!view.paused);
    assert!(view.muted);
  }

  #[test]
  fn ready_tracks_projects_selected_audio_and_subtitle_ids() {
    let tracks = vec![
      TrackInfo {
        id: 3,
        track_type: "audio".to_owned(),
        title: Some("English".to_owned()),
        language: Some("eng".to_owned()),
        selected: true,
      },
      TrackInfo {
        id: 8,
        track_type: "sub".to_owned(),
        title: Some("Spanish".to_owned()),
        language: Some("spa".to_owned()),
        selected: true,
      },
    ];

    let view = ready_tracks(tracks);

    assert!(matches!(
      view,
      TracksView::Ready {
        audio: Some(3),
        subtitle: Some(8),
        ..
      }
    ));
  }

  #[test]
  fn unavailable_adjacent_result_cannot_start_playback() {
    let (mut session, now, auxiliary) = start_session(IntroSkipMode::Off);
    let id = adjacent_id(&auxiliary, AdjacentDirection::Next);
    session.handle(
      PlaybackInput::Event(PlaybackEvent::AdjacentSettled {
        id,
        direction: AdjacentDirection::Next,
        result: Ok(None),
      }),
      now,
    );

    let effects = session.handle(
      PlaybackInput::Intent(PlaybackIntent::PlayAdjacent(AdjacentDirection::Next)),
      now,
    );

    assert!(effects.is_empty());
    assert_eq!(
      session.view().adjacent.next,
      AdjacentAvailability::Unavailable
    );
  }

  #[test]
  fn prompt_osd_failure_never_creates_a_live_prompt() {
    let (mut session, now, auxiliary) = start_session(IntroSkipMode::Manual);
    settle_intro_ranges(&mut session, intro_fetch_id(&auxiliary), now);
    let (prompt_id, _) = controller_effect(refresh_at(&mut session, now, 10.0, Vec::new()));

    session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: prompt_id,
        settlement: ControllerSettlement::OsdShown(Err(PlaybackError::MpvControlFailed)),
      }),
      now,
    );

    assert!(session.view().intro_prompt.is_none());
    assert_eq!(
      session.view().notice,
      Some(PlaybackNotice::Failed(PlaybackError::MpvControlFailed))
    );
  }
}
