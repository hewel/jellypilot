//! Linux MPRIS adapter. The bus never owns playback: commands cross the UI
//! admission boundary with the account and replacement identity they observed.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iced::futures::{SinkExt, Stream};
use iced::{Subscription, Task};
use jellypilot_core::request_gate::SessionToken;
use jellypilot_mpv::playback_session::{AdjacentAvailability, AdjacentDirection, PlaybackIntent};
use jellypilot_mpv::statistics::PlaybackControls;
use tokio::sync::{mpsc, oneshot, watch};
use zbus::fdo;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use super::message::{Message as AppMessage, PlaybackMessage, WindowMessage};
use super::state::State;

const NAME: &str = "org.mpris.MediaPlayer2.JellyPilot";
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub(crate) struct Scope {
  session: SessionToken,
  replacement: u64,
  item: String,
  admission: jellypilot_sdk::PlaybackAdmission,
}

impl PartialEq for Scope {
  fn eq(&self, other: &Self) -> bool {
    self.session == other.session
      && self.replacement == other.replacement
      && self.item == other.item
      // A failed account transition can revoke playback admission without
      // replacing the profile or RequestGate token. Its recovery still needs
      // a fresh track identity and commands minted under the fresh admission.
      && self.admission.is_current() == other.admission.is_current()
  }
}

impl Scope {
  pub(super) fn is_current(&self) -> bool {
    self.admission.is_current()
  }
  fn capture(state: &State) -> Option<Self> {
    Self::from_playback(&state.playback, &state.kernel)
  }

  pub(super) fn from_playback(
    playback: &super::playback::Surface,
    kernel: &super::kernel::Kernel,
  ) -> Option<Self> {
    Some(Self {
      session: kernel.request_gate.current_session(),
      replacement: playback.view.lifecycle.replacement_generation,
      item: playback.view.now_playing.as_ref()?.item.item_id.clone(),
      admission: kernel.sdk.playback_admission().ok()?,
    })
  }
}

#[derive(Clone, Debug)]
pub(crate) enum Command {
  Raise,
  Play,
  Pause,
  PlayPause,
  Stop,
  Next,
  Previous,
  Seek(i64),
  SetPosition(OwnedObjectPath, i64),
  Volume(f64),
  Rate(f64),
}

#[derive(Clone)]
pub(crate) struct Request {
  scope: Option<Scope>,
  track: Option<OwnedObjectPath>,
  command: Command,
  reply: Arc<Mutex<Option<oneshot::Sender<fdo::Result<()>>>>>,
}

#[derive(Clone)]
pub(crate) enum Message {
  Request(Request),
  SeekSettled {
    scope: Scope,
    message: Box<PlaybackMessage>,
  },
  Controls {
    scope: Scope,
    sample: Option<PlaybackControls>,
  },
}

#[derive(Clone, PartialEq)]
struct Snapshot {
  scope: Option<Scope>,
  track: Option<OwnedObjectPath>,
  title: String,
  duration: Option<i64>,
  position: i64,
  paused: bool,
  volume: f64,
  rate: f64,
  controls_known: bool,
  available: bool,
  can_seek: bool,
  next: bool,
  previous: bool,
  sampled_at: Instant,
  seek_serial: u64,
}

impl Default for Snapshot {
  fn default() -> Self {
    Self {
      scope: None,
      track: None,
      title: String::new(),
      duration: None,
      position: 0,
      paused: true,
      volume: 1.0,
      rate: 1.0,
      controls_known: false,
      available: false,
      can_seek: false,
      next: false,
      previous: false,
      sampled_at: Instant::now(),
      seek_serial: 0,
    }
  }
}

impl Snapshot {
  fn minimum_rate(&self) -> f64 {
    if self.controls_known {
      self.rate.min(0.25)
    } else {
      1.0
    }
  }

  fn maximum_rate(&self) -> f64 {
    if self.controls_known {
      self.rate.max(4.0)
    } else {
      1.0
    }
  }

  fn status(&self) -> &'static str {
    if self.track.is_none() {
      "Stopped"
    } else if self.paused {
      "Paused"
    } else {
      "Playing"
    }
  }

  fn position(&self) -> i64 {
    let elapsed = if self.paused || self.track.is_none() || !self.controls_known {
      0
    } else {
      micros(self.sampled_at.elapsed().as_secs_f64() * self.rate)
    };
    let position = self.position.saturating_add(elapsed);
    self.duration.map_or(position, |end| position.min(end))
  }

  fn metadata(&self) -> HashMap<&'static str, OwnedValue> {
    let mut values = HashMap::new();
    if let Some(track) = &self.track {
      values.insert(
        "mpris:trackid",
        OwnedValue::from(zbus::zvariant::ObjectPath::from(track.clone())),
      );
      values.insert(
        "xesam:title",
        OwnedValue::from(zbus::zvariant::Str::from(self.title.clone())),
      );
      if let Some(duration) = self.duration {
        values.insert("mpris:length", OwnedValue::from(duration));
      }
    }
    // Server URLs and artwork URLs can contain credentials. Only descriptive
    // text and locally minted object paths cross this public session-bus seam.
    values
  }
}

#[derive(Clone)]
struct Channel {
  snapshot: watch::Sender<Snapshot>,
}

impl Hash for Channel {
  fn hash<H: Hasher>(&self, state: &mut H) {
    // One stable subscription for the lifetime of the app, independent of
    // track changes. Rebuilding it would release/reacquire the bus name.
    NAME.hash(state);
  }
}

pub(crate) struct Surface {
  channel: Channel,
  scope: Option<Scope>,
  track_serial: u64,
  controls: Option<PlaybackControls>,
  sampling: Option<Scope>,
  seek_serial: u64,
  instance: u128,
}

impl Default for Surface {
  fn default() -> Self {
    Self {
      channel: Channel {
        snapshot: watch::channel(Snapshot::default()).0,
      },
      scope: None,
      track_serial: 0,
      controls: None,
      sampling: None,
      seek_serial: 0,
      instance: std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos(),
    }
  }
}

pub(super) fn should_sample(message: &AppMessage) -> bool {
  matches!(
    message,
    AppMessage::Playback(PlaybackMessage::ControllerSettled { .. })
      | AppMessage::SystemMedia(Message::SeekSettled { .. })
  )
}

pub(super) fn sync(state: &mut State, sample_controls: bool) -> Task<AppMessage> {
  let scope = Scope::capture(state);
  let changed = scope != state.system_media.scope;
  if changed {
    state.system_media.scope = scope.clone();
    state.system_media.track_serial = state.system_media.track_serial.wrapping_add(1);
    state.system_media.controls = None;
  }
  let mut snapshot = project(state);
  let previous = state.system_media.channel.snapshot.borrow();
  if snapshot.position == previous.position
    && snapshot.paused == previous.paused
    && snapshot.rate == previous.rate
    && snapshot.track == previous.track
  {
    snapshot.sampled_at = previous.sampled_at;
  }
  drop(previous);
  state
    .system_media
    .channel
    .snapshot
    .send_if_modified(|current| {
      if *current == snapshot {
        false
      } else {
        *current = snapshot;
        true
      }
    });
  if (changed || sample_controls) && state.system_media.sampling.is_none() {
    if let Some((scope, controller)) = scope.zip(state.playback.controller.clone()) {
      state.system_media.sampling = Some(scope.clone());
      return Task::perform(
        async move {
          let reader = controller.lock().await.statistics_reader();
          let sample = match reader {
            Some(reader) => reader.controls().await.ok(),
            None => None,
          };
          Message::Controls { scope, sample }
        },
        AppMessage::SystemMedia,
      );
    }
  }
  Task::none()
}

fn project(state: &State) -> Snapshot {
  let mut snapshot = Snapshot::default();
  let allowed = !state.shell.quit_requested && !state.kernel.sdk.content_mutations_blocked();
  if !allowed {
    return snapshot;
  }
  let Some(playing) = &state.playback.view.now_playing else {
    return snapshot;
  };
  snapshot.scope = Scope::capture(state);
  snapshot.track = OwnedObjectPath::try_from(format!(
    "/io/github/hewel/JellyPilot/track/{}_{}/{}",
    std::process::id(),
    state.system_media.instance,
    state.system_media.track_serial,
  ))
  .ok();
  snapshot.title.clone_from(&playing.item.title);
  snapshot.duration = playing
    .duration_seconds
    .filter(|value| value.is_finite() && *value > 0.0)
    .map(micros);
  snapshot.position = micros(playing.position_seconds);
  snapshot.seek_serial = state.system_media.seek_serial;
  snapshot.paused = playing.paused;
  snapshot.volume = if playing.muted {
    0.0
  } else {
    playing.volume / 100.0
  };
  snapshot.available =
    state.playback.view.engine_available && !state.playback.view.lifecycle.replacing;
  if let Some(controls) = &state.system_media.controls {
    snapshot.controls_known = controls.speed.is_some();
    snapshot.rate = controls.speed.unwrap_or(1.0);
    snapshot.can_seek = snapshot.available
      && controls.seekable == Some(true)
      && controls.partially_seekable == Some(false)
      && controls.duration_seconds.is_some();
    snapshot.duration = controls.duration_seconds.map(micros).or(snapshot.duration);
  }
  // The native adjacent command starts playback. MPRIS requires Next/Previous
  // to preserve Paused, so expose it only while already Playing.
  snapshot.next = snapshot.available
    && !playing.paused
    && matches!(
      state.playback.view.adjacent.next,
      AdjacentAvailability::Available { .. }
    );
  snapshot.previous = snapshot.available
    && !playing.paused
    && matches!(
      state.playback.view.adjacent.previous,
      AdjacentAvailability::Available { .. }
    );
  snapshot
}

fn micros(seconds: f64) -> i64 {
  if seconds.is_finite() {
    (seconds.max(0.0) * 1_000_000.0) as i64
  } else {
    0
  }
}

pub(super) fn update(state: &mut State, message: Message) -> Task<AppMessage> {
  match message {
    message @ Message::SeekSettled { .. } => {
      super::update::update(state, AppMessage::SystemMedia(message))
    }
    Message::Controls { scope, sample } => {
      if state.system_media.sampling.as_ref() == Some(&scope) {
        state.system_media.sampling = None;
      }
      if scope.is_current() && Scope::capture(state).as_ref() == Some(&scope) {
        state.system_media.controls = sample;
      }
      Task::none()
    }
    Message::Request(request) => {
      let Some(reply) = request.reply.lock().ok().and_then(|mut slot| slot.take()) else {
        return Task::none();
      };
      // A caller which timed out while the UI was busy must not cause a late
      // play/seek after it already received failure.
      if reply.is_closed() {
        return Task::none();
      }
      let result = route(state, &request);
      match result {
        Ok(task) => {
          let _ = reply.send(Ok(()));
          task
        }
        Err(error) => {
          let _ = reply.send(Err(error));
          Task::none()
        }
      }
    }
  }
}

/// Only the router can confirm that the playback reducer applied this receipt;
/// a replay or detached cleanup must never advertise another successful seek.
pub(super) fn seek_applied(state: &mut State, scope: Scope) {
  if scope.is_current() && Scope::capture(state).as_ref() == Some(&scope) {
    state.system_media.seek_serial = state.system_media.seek_serial.wrapping_add(1);
  }
}

fn unavailable() -> fdo::Error {
  fdo::Error::Failed("Playback is no longer available for this request".to_owned())
}

fn unsupported() -> fdo::Error {
  fdo::Error::NotSupported("This operation is not supported".to_owned())
}

fn route(state: &mut State, request: &Request) -> fdo::Result<Task<AppMessage>> {
  if state.shell.quit_requested {
    return Err(unavailable());
  }
  if matches!(request.command, Command::Raise) {
    return Ok(super::update::update(
      state,
      AppMessage::Window(WindowMessage::ShowRequested(None)),
    ));
  }
  let snapshot = project(state);
  if !snapshot.available
    || !request.scope.as_ref().is_some_and(Scope::is_current)
    || request.scope != snapshot.scope
    || request.track != snapshot.track
  {
    return Err(unavailable());
  }
  let intent = match request.command {
    Command::Play => PlaybackIntent::SetPaused(false),
    Command::Pause => PlaybackIntent::SetPaused(true),
    Command::PlayPause => PlaybackIntent::SetPaused(!snapshot.paused),
    Command::Stop => PlaybackIntent::Stop,
    Command::Next if snapshot.next => PlaybackIntent::PlayAdjacent(AdjacentDirection::Next),
    Command::Previous if snapshot.previous => {
      PlaybackIntent::PlayAdjacent(AdjacentDirection::Previous)
    }
    Command::Seek(offset) if snapshot.can_seek => {
      let position = snapshot.position().saturating_add(offset).max(0);
      if snapshot
        .duration
        .is_some_and(|duration| position > duration)
      {
        if snapshot.next {
          PlaybackIntent::PlayAdjacent(AdjacentDirection::Next)
        } else {
          return Ok(Task::none());
        }
      } else {
        PlaybackIntent::Seek(position as f64 / 1_000_000.0)
      }
    }
    Command::SetPosition(ref track, position) if snapshot.can_seek => {
      // MPRIS specifies that stale TrackIds and out-of-range positions are ignored.
      if Some(track) != snapshot.track.as_ref()
        || position < 0
        || snapshot.duration.is_some_and(|end| position > end)
      {
        return Ok(Task::none());
      }
      PlaybackIntent::Seek(position as f64 / 1_000_000.0)
    }
    Command::Volume(volume) if volume.is_finite() => {
      PlaybackIntent::SetVolume(volume.clamp(0.0, 1.0) * 100.0)
    }
    Command::Rate(0.0) => PlaybackIntent::SetPaused(true),
    Command::Rate(rate) if rate == snapshot.rate => return Ok(Task::none()),
    Command::Rate(rate) if snapshot.controls_known && rate.is_finite() && rate > 0.0 => {
      // The external player's native controls may select a wider rate than
      // this app's speed adapter. Preserve the observed rate and use MPRIS's
      // permitted best-fit rule for new positive requests.
      PlaybackIntent::SetSpeed(rate.clamp(0.25, 4.0))
    }
    _ => return Err(unsupported()),
  };
  let scope = snapshot.scope.ok_or_else(unavailable)?;
  let visible = super::embedded_player::active(state);
  Ok(super::playback::update_system_media(
    &mut state.playback,
    &mut state.kernel,
    state.shell.quit_requested,
    scope,
    intent,
    visible,
  ))
}

pub(super) fn subscription(surface: &Surface) -> Subscription<AppMessage> {
  Subscription::run_with(surface.channel.clone(), stream)
}

fn stream(channel: &Channel) -> impl Stream<Item = AppMessage> {
  let snapshot = channel.snapshot.subscribe();
  iced::stream::channel(8, async move |mut output| {
    let (commands, mut receiver) = mpsc::channel(16);
    let result = connect(snapshot.clone(), commands).await;
    if let Ok(connection) = result {
      let changes = publish_changes(&connection, snapshot);
      tokio::pin!(changes);
      loop {
        tokio::select! {
          _ = connection.closed() => break,
          _ = &mut changes => break,
          request = receiver.recv() => {
            let Some(request) = request else { break; };
            if output.send(AppMessage::SystemMedia(Message::Request(request))).await.is_err() { break; }
          }
        }
      }
      let _ = connection.clone().close().await;
    }
    // No session bus, name conflict, and bus loss are optional-integration
    // failures. Keep this subscription inert until process exit; no retry loop.
    std::future::pending::<()>().await;
  })
}

async fn connect(
  snapshot: watch::Receiver<Snapshot>,
  commands: mpsc::Sender<Request>,
) -> zbus::Result<zbus::Connection> {
  let endpoint = Endpoint { snapshot, commands };
  let connection = zbus::connection::Builder::session()?
    .serve_at(PATH, Root(endpoint.clone()))?
    .serve_at(PATH, Player(endpoint))?
    .build()
    .await?;
  // Request explicitly: this zbus release's builder accepts InQueue as
  // success despite documenting DoNotQueue. A second instance must neither
  // queue for nor replace the existing JellyPilot player's ownership.
  connection
    .request_name_with_flags(NAME, fdo::RequestNameFlags::DoNotQueue.into())
    .await?;
  Ok(connection)
}

#[derive(Clone)]
struct Endpoint {
  snapshot: watch::Receiver<Snapshot>,
  commands: mpsc::Sender<Request>,
}

impl Endpoint {
  async fn dispatch(&self, command: Command) -> fdo::Result<()> {
    let (sender, receiver) = oneshot::channel();
    let snapshot = self.snapshot.borrow().clone();
    let request = Request {
      scope: snapshot.scope,
      track: snapshot.track,
      command,
      reply: Arc::new(Mutex::new(Some(sender))),
    };
    self.commands.try_send(request).map_err(|_| unavailable())?;
    tokio::time::timeout(COMMAND_TIMEOUT, receiver)
      .await
      .map_err(|_| unavailable())?
      .map_err(|_| unavailable())?
  }
}

struct Root(Endpoint);

#[zbus::interface(name = "org.mpris.MediaPlayer2", spawn = false)]
impl Root {
  async fn raise(&self) -> fdo::Result<()> {
    self.0.dispatch(Command::Raise).await
  }
  fn quit(&self) -> fdo::Result<()> {
    Err(unsupported())
  }
  #[zbus(property(emits_changed_signal = "const"))]
  fn can_quit(&self) -> bool {
    false
  }
  #[zbus(property(emits_changed_signal = "const"))]
  fn can_raise(&self) -> bool {
    true
  }
  #[zbus(property(emits_changed_signal = "const"))]
  fn has_track_list(&self) -> bool {
    false
  }
  #[zbus(property(emits_changed_signal = "const"))]
  fn identity(&self) -> &str {
    "JellyPilot"
  }
  #[zbus(property(emits_changed_signal = "const"))]
  fn supported_uri_schemes(&self) -> Vec<String> {
    Vec::new()
  }
  #[zbus(property(emits_changed_signal = "const"))]
  fn supported_mime_types(&self) -> Vec<String> {
    Vec::new()
  }
}

struct Player(Endpoint);

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player", spawn = false)]
impl Player {
  async fn next(&self) -> fdo::Result<()> {
    self.0.dispatch(Command::Next).await
  }
  async fn previous(&self) -> fdo::Result<()> {
    self.0.dispatch(Command::Previous).await
  }
  async fn pause(&self) -> fdo::Result<()> {
    self.0.dispatch(Command::Pause).await
  }
  async fn play_pause(&self) -> fdo::Result<()> {
    self.0.dispatch(Command::PlayPause).await
  }
  async fn stop(&self) -> fdo::Result<()> {
    self.0.dispatch(Command::Stop).await
  }
  async fn play(&self) -> fdo::Result<()> {
    self.0.dispatch(Command::Play).await
  }
  async fn seek(&self, offset: i64) -> fdo::Result<()> {
    self.0.dispatch(Command::Seek(offset)).await
  }
  async fn set_position(&self, track_id: OwnedObjectPath, position: i64) -> fdo::Result<()> {
    self
      .0
      .dispatch(Command::SetPosition(track_id, position))
      .await
  }
  fn open_uri(&self, _uri: &str) -> fdo::Result<()> {
    Err(unsupported())
  }
  #[zbus(property)]
  fn playback_status(&self) -> String {
    self.0.snapshot.borrow().status().to_owned()
  }
  #[zbus(property)]
  fn metadata(&self) -> HashMap<&'static str, OwnedValue> {
    self.0.snapshot.borrow().metadata()
  }
  #[zbus(property)]
  fn volume(&self) -> f64 {
    self.0.snapshot.borrow().volume
  }
  #[zbus(property)]
  async fn set_volume(&self, value: f64) -> fdo::Result<()> {
    self.0.dispatch(Command::Volume(value)).await
  }
  #[zbus(property)]
  fn rate(&self) -> f64 {
    self.0.snapshot.borrow().rate
  }
  #[zbus(property)]
  async fn set_rate(&self, value: f64) -> fdo::Result<()> {
    self.0.dispatch(Command::Rate(value)).await
  }
  #[zbus(property(emits_changed_signal = "false"))]
  fn position(&self) -> i64 {
    self.0.snapshot.borrow().position()
  }
  #[zbus(property)]
  fn minimum_rate(&self) -> f64 {
    self.0.snapshot.borrow().minimum_rate()
  }
  #[zbus(property)]
  fn maximum_rate(&self) -> f64 {
    self.0.snapshot.borrow().maximum_rate()
  }
  #[zbus(property)]
  fn can_go_next(&self) -> bool {
    self.0.snapshot.borrow().next
  }
  #[zbus(property)]
  fn can_go_previous(&self) -> bool {
    self.0.snapshot.borrow().previous
  }
  #[zbus(property)]
  fn can_play(&self) -> bool {
    self.0.snapshot.borrow().available
  }
  #[zbus(property)]
  fn can_pause(&self) -> bool {
    self.0.snapshot.borrow().available
  }
  #[zbus(property)]
  fn can_seek(&self) -> bool {
    self.0.snapshot.borrow().can_seek
  }
  #[zbus(property(emits_changed_signal = "const"))]
  fn can_control(&self) -> bool {
    true
  }
  #[zbus(signal)]
  async fn seeked(
    emitter: &zbus::object_server::SignalEmitter<'_>,
    position: i64,
  ) -> zbus::Result<()>;
}

async fn publish_changes(
  connection: &zbus::Connection,
  mut snapshot: watch::Receiver<Snapshot>,
) -> zbus::Result<()> {
  let emitter = zbus::object_server::SignalEmitter::new(connection, PATH)?;
  let mut previous = snapshot.borrow_and_update().clone();
  while snapshot.changed().await.is_ok() {
    let current = snapshot.borrow_and_update().clone();
    let mut changes: HashMap<&str, Value<'_>> = HashMap::new();
    if current.status() != previous.status() {
      changes.insert("PlaybackStatus", Value::from(current.status()));
    }
    if current.track != previous.track
      || current.title != previous.title
      || current.duration != previous.duration
    {
      changes.insert("Metadata", Value::from(current.metadata()));
    }
    if current.volume != previous.volume {
      changes.insert("Volume", Value::from(current.volume));
    }
    if current.rate != previous.rate {
      changes.insert("Rate", Value::from(current.rate));
    }
    if current.minimum_rate() != previous.minimum_rate() {
      changes.insert("MinimumRate", Value::from(current.minimum_rate()));
    }
    if current.maximum_rate() != previous.maximum_rate() {
      changes.insert("MaximumRate", Value::from(current.maximum_rate()));
    }
    if current.next != previous.next {
      changes.insert("CanGoNext", Value::from(current.next));
    }
    if current.previous != previous.previous {
      changes.insert("CanGoPrevious", Value::from(current.previous));
    }
    if current.can_seek != previous.can_seek {
      changes.insert("CanSeek", Value::from(current.can_seek));
    }
    if current.available != previous.available {
      changes.insert("CanPlay", Value::from(current.available));
      changes.insert("CanPause", Value::from(current.available));
    }
    if !changes.is_empty() {
      fdo::Properties::properties_changed(
        &emitter,
        PLAYER.try_into()?,
        changes,
        std::borrow::Cow::Borrowed(&[]),
      )
      .await?;
    }
    if current.track.is_some()
      && current.track == previous.track
      && (current.seek_serial != previous.seek_serial
        || (current.paused && previous.paused && current.position != previous.position)
        || current.position.abs_diff(previous.position()) > 2_000_000)
    {
      Player::seeked(&emitter, current.position).await?;
    }
    previous = current;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use iced::futures::StreamExt;
  use std::sync::atomic::Ordering;

  fn playing() -> State {
    let mut state = super::super::update::tests::active_intro_prompt_state();
    state.system_media.controls = Some(PlaybackControls {
      speed: Some(1.5),
      duration_seconds: Some(1800.0),
      seekable: Some(true),
      partially_seekable: Some(false),
      ..Default::default()
    });
    state.system_media.scope = Scope::capture(&state);
    drop(sync(&mut state, false));
    state
  }

  fn request(state: &State, command: Command) -> (Request, oneshot::Receiver<fdo::Result<()>>) {
    let snapshot = project(state);
    let (sender, receiver) = oneshot::channel();
    (
      Request {
        scope: snapshot.scope,
        track: snapshot.track,
        command,
        reply: Arc::new(Mutex::new(Some(sender))),
      },
      receiver,
    )
  }

  #[tokio::test]
  async fn old_account_and_replacement_commands_are_rejected_before_playback() {
    let mut state = playing();
    let (old, response) = request(&state, Command::Stop);
    state.kernel.request_gate.begin_login();
    let task = update(&mut state, Message::Request(old));
    assert!(iced_runtime::task::into_stream(task).is_none());
    assert!(matches!(
      response.await.unwrap(),
      Err(fdo::Error::Failed(_))
    ));

    let (old, response) = request(&state, Command::Pause);
    state.playback.view.lifecycle.replacement_generation += 1;
    let task = update(&mut state, Message::Request(old));
    assert!(iced_runtime::task::into_stream(task).is_none());
    assert!(matches!(
      response.await.unwrap(),
      Err(fdo::Error::Failed(_))
    ));
  }

  #[tokio::test]
  async fn background_play_requires_window_and_cannot_survive_replacement() {
    let mut state = playing();
    state
      .playback
      .presentation_hold
      .store(true, Ordering::Release);
    let (command, response) = request(&state, Command::Play);
    let task = update(&mut state, Message::Request(command));
    response.await.unwrap().unwrap();
    let mut actions = iced_runtime::task::into_stream(task).unwrap();
    assert!(matches!(
      actions.next().await,
      Some(iced_runtime::Action::Output(AppMessage::Window(
        WindowMessage::ShowForPlayback
      )))
    ));
    state.playback.view.lifecycle.replacement_generation += 1;
    assert!(
      super::super::playback::window_opened(&mut state.playback, &mut state.kernel, false)
        .is_none()
    );
  }

  #[tokio::test]
  async fn timed_out_commands_and_old_track_ids_have_no_effect() {
    let mut state = playing();
    let (command, response) = request(&state, Command::Stop);
    drop(response);
    assert!(
      iced_runtime::task::into_stream(update(&mut state, Message::Request(command))).is_none()
    );
    let (command, response) = request(
      &state,
      Command::SetPosition(
        OwnedObjectPath::try_from("/retired/track").unwrap(),
        5_000_000,
      ),
    );
    assert!(
      iced_runtime::task::into_stream(update(&mut state, Message::Request(command))).is_none()
    );
    response.await.unwrap().unwrap();
    assert_eq!(
      state
        .playback
        .view
        .now_playing
        .as_ref()
        .unwrap()
        .position_seconds,
      10.0
    );
  }

  fn settle_short_seek(state: &mut State, position: f64, success: bool) -> Message {
    use jellypilot_mpv::playback::{PlaybackError, PlaybackOutcome, PlaybackSnapshot};
    use jellypilot_mpv::playback_session::{ControllerSettlement, PlaybackEffect, PlaybackInput};
    let scope = Scope::capture(state).unwrap();
    let effects = state
      .playback
      .session
      .handle(
        PlaybackInput::Intent(Box::new(PlaybackIntent::Seek(position))),
        Instant::now(),
      )
      .effects;
    let [PlaybackEffect::Controller(id, _)] = effects.as_slice() else {
      panic!("seek must dispatch through the playback session");
    };
    let playing = state.playback.view.now_playing.as_ref().unwrap();
    let settlement = ControllerSettlement::Controlled(if success {
      Ok(PlaybackOutcome {
        snapshot: PlaybackSnapshot {
          tools: Default::default(),
          now_playing: Some(playing.item.clone()),
          transport: jellypilot_mpv::PlayerState {
            connected: true,
            paused: playing.paused,
            muted: false,
            time_pos: position,
            duration: 1800.0,
            volume: 75.0,
          },
        },
        warnings: Vec::new(),
      })
    } else {
      Err(PlaybackError::MpvControlFailed)
    });
    let message = Message::SeekSettled {
      scope,
      message: Box::new(PlaybackMessage::ControllerSettled {
        id: *id,
        settlement: Box::new(settlement),
        started: None,
      }),
    };
    drop(update(state, message.clone()));
    drop(sync(state, false));
    message
  }

  #[test]
  fn only_successful_current_seek_receipts_advance_the_signal_revision() {
    let mut state = playing();
    settle_short_seek(&mut state, 10.1, false);
    assert_eq!(state.system_media.seek_serial, 0);
    let receipt = settle_short_seek(&mut state, 10.1, true);
    assert_eq!(state.system_media.seek_serial, 1);
    assert_eq!(
      state.system_media.channel.snapshot.borrow().position,
      10_100_000
    );
    drop(update(&mut state, receipt));
    drop(sync(&mut state, false));
    assert_eq!(
      state.system_media.seek_serial, 1,
      "the reducer ignored a replayed receipt"
    );
  }

  #[test]
  fn only_applied_restart_receipts_emit_seeked_and_other_tools_never_do() {
    use jellypilot_mpv::playback::tools::{PlaybackFileToken, PlaybackToolAction};
    use jellypilot_mpv::playback::{
      PlaybackOutcome, PlaybackRefreshOutcome, PlaybackRefreshState, PlaybackSnapshot,
    };
    use jellypilot_mpv::playback_session::{
      ControllerSettlement, PlaybackEffect, PlaybackEvent, PlaybackInput,
    };
    let mut state = playing();
    let playing = state.playback.view.now_playing.as_ref().unwrap();
    let mut snapshot = PlaybackSnapshot {
      tools: Default::default(),
      now_playing: Some(playing.item.clone()),
      transport: jellypilot_mpv::PlayerState {
        connected: true,
        paused: true,
        muted: false,
        time_pos: 10.0,
        duration: 1800.0,
        volume: 75.0,
      },
    };
    let file = PlaybackFileToken::default();
    snapshot.tools.file = Some(file);
    let effects = state
      .playback
      .session
      .handle(
        PlaybackInput::Intent(Box::new(PlaybackIntent::Tick)),
        Instant::now(),
      )
      .effects;
    let [PlaybackEffect::Controller(id, _)] = effects.as_slice() else {
      panic!("refresh");
    };
    drop(state.playback.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::ControllerSettled {
        id: *id,
        settlement: ControllerSettlement::Refreshed {
          outcome: PlaybackRefreshOutcome {
            snapshot: snapshot.clone(),
            state: PlaybackRefreshState::Active,
            warnings: Vec::new(),
          },
          client_messages: Vec::new(),
        },
      })),
      Instant::now(),
    ));
    state.playback.view = state.playback.session.view();
    for action in [
      PlaybackToolAction::MarkA(10.0),
      PlaybackToolAction::Refresh,
      PlaybackToolAction::RestartFromA,
    ] {
      state.playback.session.set_playback_admitted(true);
      let scope = Scope::capture(&state).unwrap();
      let effects = state
        .playback
        .session
        .handle(
          PlaybackInput::Intent(Box::new(PlaybackIntent::PlaybackTool { file, action })),
          Instant::now(),
        )
        .effects;
      let [PlaybackEffect::Controller(id, _)] = effects.as_slice() else {
        panic!("tool must dispatch");
      };
      snapshot.transport.time_pos = 10.1;
      let message = Message::SeekSettled {
        scope,
        message: Box::new(PlaybackMessage::ControllerSettled {
          id: *id,
          settlement: Box::new(ControllerSettlement::ToolApplied {
            tools: snapshot.tools.clone(),
            seeked: (action == PlaybackToolAction::RestartFromA).then(|| {
              Box::new(PlaybackOutcome {
                snapshot: snapshot.clone(),
                warnings: Vec::new(),
              })
            }),
          }),
          started: None,
        }),
      };
      drop(update(&mut state, message.clone()));
      drop(sync(&mut state, false));
      let expected = u64::from(action == PlaybackToolAction::RestartFromA);
      assert_eq!(state.system_media.seek_serial, expected);
      drop(update(&mut state, message));
      drop(sync(&mut state, false));
      assert_eq!(
        state.system_media.seek_serial, expected,
        "duplicate success cannot emit another Seeked"
      );
    }
    assert_eq!(
      state.system_media.channel.snapshot.borrow().position,
      10_100_000
    );
    assert!(state.playback.view.now_playing.as_ref().unwrap().paused);
  }

  #[tokio::test]
  async fn handoff_revokes_commands_even_before_ui_session_token_advances() {
    let mut state = playing();
    state
      .kernel
      .sdk
      .adopt_test_session(jellypilot_media_server::SavedSession {
        provider: jellypilot_media_server::MediaServerProvider::Jellyfin,
        server_url: "https://server.example.test".to_owned(),
        user_id: "user".to_owned(),
        user_name: "User".to_owned(),
        access_token: "secret".to_owned(),
        server_name: None,
        device_id: None,
      });
    let (command, response) = request(&state, Command::Play);
    let old_track = project(&state).track;
    let sdk = Arc::clone(&state.kernel.sdk);
    let disconnect = sdk.disconnect();
    tokio::pin!(disconnect);
    let hook = tokio::select! {
      _ = &mut disconnect => panic!("disconnect must wait for teardown"),
      hook = async { state.kernel.sdk_handoff.receiver.lock().await.recv().await } => hook.unwrap(),
    };
    assert!(state.kernel.sdk.content_mutations_blocked());
    assert!(project(&state).metadata().is_empty());
    assert!(
      iced_runtime::task::into_stream(update(&mut state, Message::Request(command))).is_none()
    );
    assert!(matches!(
      response.await.unwrap(),
      Err(fdo::Error::Failed(_))
    ));
    drop(hook);
    assert!(disconnect.await.is_err());
    assert!(!state.kernel.sdk.content_mutations_blocked());
    drop(sync(&mut state, false));
    let recovered = state.system_media.channel.snapshot.borrow();
    assert_ne!(recovered.track, old_track);
    assert!(recovered.scope.as_ref().unwrap().is_current());
  }

  #[test]
  fn mpris_protocol_in_private_bus() {
    const CHILD: &str = "JELLYPILOT_MPRIS_PROTOCOL_CHILD";
    if std::env::var_os(CHILD).is_some() {
      tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
          tokio::time::timeout(Duration::from_secs(15), protocol())
            .await
            .unwrap()
        });
      return;
    }
    // The child creates no windows, and cannot touch the user's actual session
    // bus or an already-running player's well-known name.
    let status = std::process::Command::new("dbus-run-session")
      .arg("--")
      .arg(std::env::current_exe().unwrap())
      .args([
        "--exact",
        "app::system_media::tests::mpris_protocol_in_private_bus",
        "--nocapture",
      ])
      .env(CHILD, "1")
      .status()
      .expect("dbus-run-session is required for MPRIS protocol verification");
    assert!(status.success(), "private MPRIS protocol process failed");
  }

  async fn protocol() {
    let mut state = playing();
    let snapshot = state.system_media.channel.snapshot.subscribe();
    let (sender, mut commands) = mpsc::channel(16);
    let service = connect(snapshot.clone(), sender.clone()).await.unwrap();
    assert!(
      connect(snapshot.clone(), sender).await.is_err(),
      "never replace another instance's name"
    );
    let client = zbus::Connection::session().await.unwrap();
    let player: zbus::Proxy<'_> = zbus::proxy::Builder::new(&client)
      .destination(NAME)
      .unwrap()
      .path(PATH)
      .unwrap()
      .interface(PLAYER)
      .unwrap()
      .cache_properties(zbus::proxy::CacheProperties::No)
      .build()
      .await
      .unwrap();
    let root = zbus::Proxy::new(&client, NAME, PATH, "org.mpris.MediaPlayer2")
      .await
      .unwrap();
    assert_eq!(
      root.get_property::<String>("Identity").await.unwrap(),
      "JellyPilot"
    );
    assert!(!root.get_property::<bool>("HasTrackList").await.unwrap());
    assert_eq!(
      player
        .get_property::<String>("PlaybackStatus")
        .await
        .unwrap(),
      "Playing"
    );
    assert_eq!(player.get_property::<f64>("Rate").await.unwrap(), 1.5);
    assert!(player.get_property::<bool>("CanSeek").await.unwrap());
    let metadata: HashMap<String, OwnedValue> = player.get_property("Metadata").await.unwrap();
    assert_eq!(metadata.len(), 3);
    assert_eq!(<&str>::try_from(&metadata["xesam:title"]).unwrap(), "Pilot");
    assert_eq!(
      i64::try_from(&metadata["mpris:length"]).unwrap(),
      1_800_000_000
    );
    let track = OwnedObjectPath::try_from(metadata["mpris:trackid"].try_clone().unwrap()).unwrap();

    let stale_position = (
      OwnedObjectPath::try_from("/old/track").unwrap(),
      5_000_000_i64,
    );
    let call = player.call::<_, _, ()>("SetPosition", &stale_position);
    let dispatch = async {
      let request = commands.recv().await.unwrap();
      assert!(
        iced_runtime::task::into_stream(update(&mut state, Message::Request(request))).is_none()
      );
    };
    let (result, ()) = tokio::join!(call, dispatch);
    result.unwrap();

    let call = player.call::<_, _, ()>("Seek", &(1_000_000_i64,));
    let dispatch = async {
      let request = commands.recv().await.unwrap();
      assert!(matches!(request.command, Command::Seek(1_000_000)));
      assert_eq!(request.track, Some(track));
      let _ = request
        .reply
        .lock()
        .unwrap()
        .take()
        .unwrap()
        .send(Err(unsupported()));
    };
    let (result, ()) = tokio::join!(call, dispatch);
    assert!(
      matches!(result, Err(zbus::Error::MethodError(name, _, _)) if name.as_str() == "org.freedesktop.DBus.Error.NotSupported")
    );
    assert!(player
      .call::<_, _, ()>("OpenUri", &("https://secret.example/token",))
      .await
      .is_err());
    assert!(player.get_property::<bool>("Shuffle").await.is_err());

    // External MPV controls may select rates outside the app's menu. The
    // property remains the real value and stays inside its advertised bounds.
    for rate in [0.1, 8.0] {
      state.system_media.controls.as_mut().unwrap().speed = Some(rate);
      drop(sync(&mut state, false));
      assert_eq!(player.get_property::<f64>("Rate").await.unwrap(), rate);
      assert!(player.get_property::<f64>("MinimumRate").await.unwrap() <= rate);
      assert!(player.get_property::<f64>("MaximumRate").await.unwrap() >= rate);
    }
    state.playback.view.now_playing.as_mut().unwrap().paused = true;
    state.playback.view.adjacent.next = AdjacentAvailability::Available {
      title: "Next".to_owned(),
    };
    drop(sync(&mut state, false));
    assert!(!player.get_property::<bool>("CanGoNext").await.unwrap());
    let call = player.call::<_, _, ()>("Next", &());
    let dispatch = async {
      let request = commands.recv().await.unwrap();
      assert!(
        iced_runtime::task::into_stream(update(&mut state, Message::Request(request))).is_none()
      );
    };
    let (result, ()) = tokio::join!(call, dispatch);
    assert!(result.is_err());

    let mut signals = player.receive_signal("Seeked").await.unwrap();
    let publisher = publish_changes(&service, snapshot);
    let exercise = async {
      tokio::task::yield_now().await;
      state.system_media.channel.snapshot.send_modify(|snapshot| {
        snapshot.position += 1_000_000;
        snapshot.sampled_at = Instant::now();
      });
      let signal = signals.next().await.unwrap();
      assert_eq!(signal.body().deserialize::<i64>().unwrap(), 11_000_000);

      // A successful seek of only a tenth of a second while Playing must
      // still notify clients; they cannot infer it from a Position change.
      state.playback.view.now_playing.as_mut().unwrap().paused = false;
      settle_short_seek(&mut state, 10.1, true);
      let signal = signals.next().await.unwrap();
      assert_eq!(signal.body().deserialize::<i64>().unwrap(), 10_100_000);
    };
    tokio::select! {
      result = publisher => panic!("publisher stopped early: {result:?}"),
      () = exercise => {},
    }
    drop(player);
    drop(root);
    service.close().await.unwrap();
    let bus = zbus::fdo::DBusProxy::new(&client).await.unwrap();
    assert!(!bus.name_has_owner(NAME.try_into().unwrap()).await.unwrap());
  }
}
