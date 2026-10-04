//! Current-file playback tools. MPV owns subtitle rendering and loop execution;
//! this adapter serializes mutations and publishes confirmed property reads.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use super::{PlaybackController, PlaybackError, PlaybackOutcome, TrackInfo};
use crate::{MpvClient, PropertyValue};

static NEXT_FILE_TOKEN: AtomicU64 = AtomicU64::new(1);
const READ_TIMEOUT: Duration = Duration::from_millis(500);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(3);

/// A unique load identity, including repeated loads of the same media item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaybackFileToken(u64);

impl Default for PlaybackFileToken {
  fn default() -> Self {
    Self(NEXT_FILE_TOKEN.fetch_add(1, Ordering::Relaxed))
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubtitleRole {
  Primary,
  Secondary,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum ToolState<T> {
  Loading,
  #[default]
  Unavailable,
  Ready(T),
  Failed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SecondarySubtitleView {
  pub selected: Option<i64>,
  /// A known text primary is selected; candidates must also differ from it.
  pub eligible: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AbLoopView {
  pub a_seconds: Option<f64>,
  pub b_seconds: Option<f64>,
  /// Observed configuration, not proof that a decoder completed a loop.
  pub enabled: bool,
  pub editable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackToolError {
  Unavailable,
  StaleFile,
  InvalidTrack,
  InvalidRange,
  CommandFailed,
  ReadFailed,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlaybackToolsView {
  pub file: Option<PlaybackFileToken>,
  pub tracks: Vec<TrackInfo>,
  pub secondary: ToolState<SecondarySubtitleView>,
  pub ab_loop: ToolState<AbLoopView>,
  pub busy: bool,
  pub error: Option<PlaybackToolError>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlaybackToolAction {
  SelectPrimarySubtitle(Option<i64>),
  SelectSecondarySubtitle(Option<i64>),
  MarkA(f64),
  MarkB(f64),
  SetLoopEnabled(bool),
  RestartFromA,
  ClearLoop,
  Refresh,
}

impl TrackInfo {
  pub fn is_text_subtitle(&self) -> bool {
    self.track_type == "sub"
      && self.codec.as_deref().is_some_and(|codec| {
        matches!(
          codec.to_ascii_lowercase().as_str(),
          "ass" | "ssa" | "subrip" | "srt" | "text" | "webvtt" | "webvtt-webm" | "mov_text"
        )
      })
  }
}

fn number(value: &PropertyValue) -> Option<f64> {
  match value {
    PropertyValue::Number(value) if value.is_finite() => Some(*value),
    _ => None,
  }
}

fn point(value: &PropertyValue) -> Result<Option<f64>, PlaybackToolError> {
  match value {
    PropertyValue::String(value) if value == "no" => Ok(None),
    PropertyValue::Number(value) if value.is_finite() && *value >= 0.0 => Ok(Some(*value)),
    _ => Err(PlaybackToolError::Unavailable),
  }
}

fn subtitle_id(value: &PropertyValue) -> Result<Option<i64>, PlaybackToolError> {
  match value {
    PropertyValue::Bool(false) => Ok(None),
    PropertyValue::Number(value)
      if value.is_finite() && *value >= 0.0 && *value <= 8190.0 && value.fract() == 0.0 =>
    {
      Ok(Some(*value as i64))
    }
    _ => Err(PlaybackToolError::Unavailable),
  }
}

async fn property(mpv: &MpvClient, name: &str) -> Result<PropertyValue, PlaybackToolError> {
  mpv.get_property(name).await.map_err(|error| match error {
    crate::MpvError::CommandFailed => PlaybackToolError::Unavailable,
    _ => PlaybackToolError::ReadFailed,
  })
}

async fn playing_entry(mpv: &MpvClient) -> Result<i64, PlaybackToolError> {
  let PropertyValue::Json(json) = property(mpv, "playlist").await? else {
    return Err(PlaybackToolError::Unavailable);
  };
  let entries: Vec<serde_json::Value> =
    serde_json::from_str(&json).map_err(|_| PlaybackToolError::ReadFailed)?;
  entries
    .iter()
    .find(|entry| entry.get("playing").and_then(serde_json::Value::as_bool) == Some(true))
    .and_then(|entry| entry.get("id"))
    .and_then(serde_json::Value::as_i64)
    .ok_or(PlaybackToolError::StaleFile)
}

impl PlaybackController {
  pub(super) fn retire_tools(&mut self) {
    self.tools = PlaybackToolsView::default();
    self.tools_playlist_entry = None;
  }

  /// Runs only after file-loaded, before the existing guarded initial resume.
  /// Unsupported properties never turn a successful media load into failure.
  pub(super) async fn initialize_tools(&mut self) {
    self.tools = PlaybackToolsView {
      file: Some(PlaybackFileToken::default()),
      ..Default::default()
    };
    self.tools_playlist_entry = None;
    let initialized = tokio::time::timeout(READ_TIMEOUT, async {
      self.tools_playlist_entry = playing_entry(&self.mpv).await.ok();
      let secondary = self
        .mpv
        .set_property_string("file-local-options/secondary-sid", "no")
        .await;
      let mut loop_ok = true;
      for (name, value) in [
        ("ab-loop-count", "0"),
        ("ab-loop-b", "no"),
        ("ab-loop-a", "no"),
      ] {
        if self
          .mpv
          .set_property_string(&format!("file-local-options/{name}"), value)
          .await
          .is_err()
        {
          loop_ok = false;
        }
      }
      (secondary.is_ok(), loop_ok)
    })
    .await;
    if !matches!(initialized, Ok((true, true))) {
      self.tools.error = Some(PlaybackToolError::CommandFailed);
    }
  }

  pub(super) async fn refresh_tools(&mut self) {
    if self.tools.file.is_none() {
      return;
    }
    let result = tokio::time::timeout(READ_TIMEOUT, self.read_tools()).await;
    match result {
      Ok(Ok(mut view)) => {
        view.error = self.tools.error;
        self.tools = view;
      }
      Ok(Err(PlaybackToolError::StaleFile)) => self.retire_tools(),
      _ => {
        self.tools.secondary = ToolState::Failed;
        self.tools.ab_loop = ToolState::Failed;
        self.tools.error = Some(PlaybackToolError::ReadFailed);
      }
    }
  }

  async fn read_tools(&self) -> Result<PlaybackToolsView, PlaybackToolError> {
    let before = playing_entry(&self.mpv).await?;
    if self.tools_playlist_entry != Some(before) {
      return Err(PlaybackToolError::StaleFile);
    }
    let (tracks, sid, secondary, a, b, count, duration, seekable, partial) = tokio::join!(
      self.tracks(),
      property(&self.mpv, "sid"),
      property(&self.mpv, "secondary-sid"),
      property(&self.mpv, "ab-loop-a"),
      property(&self.mpv, "ab-loop-b"),
      property(&self.mpv, "ab-loop-count"),
      property(&self.mpv, "duration"),
      property(&self.mpv, "seekable"),
      property(&self.mpv, "partially-seekable"),
    );
    if playing_entry(&self.mpv).await? != before {
      return Err(PlaybackToolError::StaleFile);
    }
    let mut view = PlaybackToolsView {
      file: self.tools.file,
      ..Default::default()
    };
    if let Ok(mut tracks) = tracks {
      let primary = sid.as_ref().ok().and_then(|value| subtitle_id(value).ok());
      let secondary_id = secondary
        .as_ref()
        .ok()
        .and_then(|value| subtitle_id(value).ok());
      for track in &mut tracks {
        if track.track_type == "sub" {
          if let Some(primary) = primary {
            track.subtitle_role = if Some(track.id) == primary {
              Some(SubtitleRole::Primary)
            } else if secondary_id.flatten() == Some(track.id) {
              Some(SubtitleRole::Secondary)
            } else {
              None
            };
            track.selected = track.subtitle_role.is_some();
          }
        }
      }
      if let Some(selected) = secondary_id {
        view.secondary = ToolState::Ready(SecondarySubtitleView {
          selected,
          eligible: tracks.iter().any(|track| {
            track.subtitle_role == Some(SubtitleRole::Primary) && track.is_text_subtitle()
          }),
        });
      } else if secondary.is_err_and(|error| error != PlaybackToolError::Unavailable) {
        view.secondary = ToolState::Failed;
      }
      view.tracks = tracks;
    } else {
      view.secondary = ToolState::Failed;
    }
    if [&a, &b, &count]
      .iter()
      .any(|result| matches!(result, Err(error) if *error != PlaybackToolError::Unavailable))
    {
      view.ab_loop = ToolState::Failed;
    }
    if let (Ok(a), Ok(b), Ok(count)) = (a, b, count) {
      let count_enabled = match &count {
        PropertyValue::String(value) if value == "inf" => Some(true),
        PropertyValue::Number(value) if value.is_finite() && *value >= 0.0 => Some(*value > 0.0),
        _ => None,
      };
      if let (Ok(a_seconds), Ok(b_seconds), Some(count_enabled)) =
        (point(&a), point(&b), count_enabled)
      {
        view.ab_loop = ToolState::Ready(AbLoopView {
          a_seconds,
          b_seconds,
          enabled: count_enabled && a_seconds.zip(b_seconds).is_some_and(|(a, b)| a < b),
          editable: duration
            .as_ref()
            .ok()
            .and_then(number)
            .is_some_and(|value| value > 0.0)
            && matches!(seekable, Ok(PropertyValue::Bool(true)))
            && matches!(partial, Ok(PropertyValue::Bool(false))),
        });
      }
    }
    Ok(view)
  }

  pub(super) async fn check_tool_file(
    &self,
    file: PlaybackFileToken,
  ) -> Result<(), PlaybackToolError> {
    if self.tools.file != Some(file)
      || !self.active_transport_matches_mpv
      || self.unloading
      || self.active.is_none()
      || self.load_event_boundary != super::LoadEventBoundary::Settled
    {
      return Err(PlaybackToolError::StaleFile);
    }
    if let Some(expected) = self.tools_playlist_entry {
      if playing_entry(&self.mpv).await? != expected {
        return Err(PlaybackToolError::StaleFile);
      }
    }
    Ok(())
  }

  async fn set_tool_property(
    &self,
    file: PlaybackFileToken,
    name: &str,
    value: &str,
  ) -> Result<(), PlaybackToolError> {
    self.check_tool_file(file).await?;
    self
      .mpv
      .set_property_string(&format!("file-local-options/{name}"), value)
      .await
      .map_err(|_| PlaybackToolError::CommandFailed)
  }

  pub(super) async fn execute_tool(
    &mut self,
    file: PlaybackFileToken,
    action: PlaybackToolAction,
  ) -> (PlaybackToolsView, Option<PlaybackOutcome>) {
    if self.tools.file != Some(file) {
      return (
        PlaybackToolsView {
          error: Some(PlaybackToolError::StaleFile),
          ..Default::default()
        },
        None,
      );
    }
    self.tools.error = None;
    let result = tokio::time::timeout(MUTATION_TIMEOUT, self.mutate_tool(file, action))
      .await
      .unwrap_or(Err(PlaybackToolError::CommandFailed));
    self.refresh_tools().await;
    let seeked = if self.tools.file == Some(file) {
      match result {
        Ok(seeked) => seeked.map(|mut outcome| {
          outcome.snapshot.tools = self.tools.clone();
          outcome
        }),
        Err(error) => {
          self.tools.error = Some(error);
          None
        }
      }
    } else {
      None
    };
    (self.tools.clone(), seeked)
  }

  async fn mutate_tool(
    &mut self,
    file: PlaybackFileToken,
    action: PlaybackToolAction,
  ) -> Result<Option<PlaybackOutcome>, PlaybackToolError> {
    self.check_tool_file(file).await?;
    if action == PlaybackToolAction::Refresh {
      return Ok(None);
    }
    if let PlaybackToolAction::SelectPrimarySubtitle(id) = action {
      let _confirmed = self
        .select_subtitle_track(id)
        .await
        .map_err(|error| match error {
          PlaybackError::TrackUnavailable => PlaybackToolError::InvalidTrack,
          _ => PlaybackToolError::CommandFailed,
        })?;
      self.check_tool_file(file).await?;
      return Ok(None);
    }
    let observed = self.read_tools().await?;
    self.check_tool_file(file).await?;
    if let PlaybackToolAction::SelectSecondarySubtitle(id) = action {
      let ToolState::Ready(secondary) = &observed.secondary else {
        return Err(PlaybackToolError::Unavailable);
      };
      if let Some(id) = id {
        if !secondary.eligible
          || !observed.tracks.iter().any(|track| {
            track.id == id
              && track.is_text_subtitle()
              && track.subtitle_role != Some(SubtitleRole::Primary)
          })
        {
          return Err(PlaybackToolError::InvalidTrack);
        }
      }
      self
        .set_tool_property(
          file,
          "secondary-sid",
          &id.map_or_else(|| "no".into(), |id| id.to_string()),
        )
        .await?;
      self.check_tool_file(file).await?;
      if subtitle_id(&property(&self.mpv, "secondary-sid").await?)? != id {
        return Err(PlaybackToolError::CommandFailed);
      }
      return Ok(None);
    }
    let ToolState::Ready(loop_view) = observed.ab_loop else {
      return Err(PlaybackToolError::Unavailable);
    };
    let requires_seeking = !matches!(
      action,
      PlaybackToolAction::ClearLoop | PlaybackToolAction::SetLoopEnabled(false)
    );
    if requires_seeking && !loop_view.editable {
      return Err(PlaybackToolError::Unavailable);
    }
    let duration = if requires_seeking {
      number(&property(&self.mpv, "duration").await?)
        .filter(|value| *value > 0.0)
        .ok_or(PlaybackToolError::Unavailable)?
    } else {
      0.0
    };
    let valid_point = |value: f64| value.is_finite() && value >= 0.0 && value <= duration;
    let mut seeked = None;
    match action {
      PlaybackToolAction::MarkA(a) => {
        if !valid_point(a) {
          return Err(PlaybackToolError::InvalidRange);
        }
        self.set_tool_property(file, "ab-loop-count", "0").await?;
        self.set_tool_property(file, "ab-loop-b", "no").await?;
        self
          .set_tool_property(file, "ab-loop-a", &a.to_string())
          .await?;
      }
      PlaybackToolAction::MarkB(b) => {
        let a = loop_view.a_seconds.ok_or(PlaybackToolError::InvalidRange)?;
        if !valid_point(a) || !valid_point(b) || a >= b {
          return Err(PlaybackToolError::InvalidRange);
        }
        self.install_loop(file, a, b).await?;
      }
      PlaybackToolAction::SetLoopEnabled(enabled) => {
        if enabled {
          let (a, b) = loop_view
            .a_seconds
            .zip(loop_view.b_seconds)
            .ok_or(PlaybackToolError::InvalidRange)?;
          if !valid_point(a) || !valid_point(b) || a >= b {
            return Err(PlaybackToolError::InvalidRange);
          }
          self.install_loop(file, a, b).await?;
        } else {
          self.set_tool_property(file, "ab-loop-count", "0").await?;
        }
      }
      PlaybackToolAction::RestartFromA => {
        let a = loop_view
          .a_seconds
          .filter(|value| valid_point(*value))
          .ok_or(PlaybackToolError::InvalidRange)?;
        self.check_tool_file(file).await?;
        seeked = Some(
          self
            .seek(a)
            .await
            .map_err(|_| PlaybackToolError::CommandFailed)?,
        );
      }
      PlaybackToolAction::ClearLoop => {
        for (name, value) in [
          ("ab-loop-count", "0"),
          ("ab-loop-b", "no"),
          ("ab-loop-a", "no"),
        ] {
          self.set_tool_property(file, name, value).await?;
        }
      }
      _ => return Err(PlaybackToolError::Unavailable),
    }
    self.check_tool_file(file).await?;
    let verified = self.read_tools().await?;
    let ToolState::Ready(actual) = verified.ab_loop else {
      return Err(PlaybackToolError::ReadFailed);
    };
    let confirmed = match action {
      PlaybackToolAction::MarkA(a) => {
        actual.a_seconds == Some(a) && actual.b_seconds.is_none() && !actual.enabled
      }
      PlaybackToolAction::MarkB(b) => {
        actual.a_seconds == loop_view.a_seconds && actual.b_seconds == Some(b) && actual.enabled
      }
      PlaybackToolAction::SetLoopEnabled(enabled) => {
        actual.a_seconds == loop_view.a_seconds
          && actual.b_seconds == loop_view.b_seconds
          && actual.enabled == enabled
      }
      PlaybackToolAction::ClearLoop => {
        actual.a_seconds.is_none() && actual.b_seconds.is_none() && !actual.enabled
      }
      PlaybackToolAction::RestartFromA => true,
      _ => false,
    };
    if confirmed {
      Ok(seeked)
    } else {
      Err(PlaybackToolError::CommandFailed)
    }
  }

  async fn install_loop(
    &self,
    file: PlaybackFileToken,
    a: f64,
    b: f64,
  ) -> Result<(), PlaybackToolError> {
    // B must transition through no: this pinned MPV does not reliably re-arm
    // clipping when only count changes from zero to infinity.
    for (name, value) in [
      ("ab-loop-count", "0".into()),
      ("ab-loop-b", "no".into()),
      ("ab-loop-count", "inf".into()),
      ("ab-loop-a", a.to_string()),
      ("ab-loop-b", b.to_string()),
    ] {
      self.set_tool_property(file, name, &value).await?;
    }
    Ok(())
  }
}
