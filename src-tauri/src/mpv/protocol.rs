//! MPV JSON IPC protocol types.
//!
//! Reference: https://mpv.io/manual/master/#json-ipc

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicI64, Ordering};

/// Global request ID counter for unique command identification.
static REQUEST_ID: AtomicI64 = AtomicI64::new(1);

/// Generate a unique request ID for MPV commands.
pub fn next_request_id() -> i64 {
  REQUEST_ID.fetch_add(1, Ordering::SeqCst)
}

/// Command sent to MPV via IPC.
#[derive(Debug, Clone, Serialize)]
pub struct MpvCommand {
  pub command: Vec<serde_json::Value>,
  pub request_id: i64,
}

impl MpvCommand {
  /// Create a new command with auto-generated request ID.
  pub fn new(args: Vec<serde_json::Value>) -> Self {
    Self {
      command: args,
      request_id: next_request_id(),
    }
  }

  /// Load a file for playback.
  pub fn loadfile(url: &str) -> Self {
    Self::new(vec!["loadfile".into(), url.into()])
  }

  /// Load a file for playback with options.
  /// Options are passed as comma-separated key=value pairs (e.g., "start=10,sid=2,aid=1").
  /// Note: Since mpv 0.38.0, loadfile has a 4-argument form: loadfile <url> <flags> <index> <options>
  /// We pass -1 for index to use the new 4-argument form correctly.
  pub fn loadfile_with_options(url: &str, options: &str) -> Self {
    Self::new(vec![
      "loadfile".into(),
      url.into(),
      "replace".into(),
      (-1_i64).into(), // index: -1 means use default behavior
      options.into(),
    ])
  }

  /// Seek to absolute position in seconds.
  pub fn seek(time: f64) -> Self {
    Self::new(vec!["seek".into(), time.into(), "absolute".into()])
  }

  /// Show text on MPV's on-screen display.
  pub fn show_text(text: &str, duration_ms: i64) -> Self {
    Self::new(vec!["show-text".into(), text.into(), duration_ms.into()])
  }

  /// Set pause state.
  pub fn set_pause(paused: bool) -> Self {
    Self::new(vec!["set_property".into(), "pause".into(), paused.into()])
  }

  /// Set volume (0-100).
  pub fn set_volume(volume: f64) -> Self {
    Self::new(vec!["set_property".into(), "volume".into(), volume.into()])
  }

  /// Set audio track by ID.
  pub fn set_audio_track(id: i64) -> Self {
    Self::new(vec!["set_property".into(), "aid".into(), id.into()])
  }

  /// Set subtitle track by ID.
  pub fn set_subtitle_track(id: i64) -> Self {
    Self::new(vec!["set_property".into(), "sid".into(), id.into()])
  }

  /// Get a property value.
  pub fn get_property(name: &str) -> Self {
    Self::new(vec!["get_property".into(), name.into()])
  }

  /// Quit MPV.
  pub fn quit() -> Self {
    Self::new(vec!["quit".into()])
  }

  /// Cycle (toggle) a property.
  pub fn cycle(property: &str) -> Self {
    Self::new(vec!["cycle".into(), property.into()])
  }

  /// Set a string property.
  pub fn set_property_string(name: &str, value: &str) -> Self {
    Self::new(vec!["set_property".into(), name.into(), value.into()])
  }

  /// Disable a track (set property to "no").
  pub fn disable_track(property: &str) -> Self {
    Self::new(vec!["set_property".into(), property.into(), "no".into()])
  }

  /// Observe a property for changes.
  /// The observer_id is used to identify which observation triggered the event.
  pub fn observe_property(observer_id: i64, property: &str) -> Self {
    Self::new(vec![
      "observe_property".into(),
      observer_id.into(),
      property.into(),
    ])
  }

  /// Stop observing a property.
  #[allow(dead_code)]
  pub fn unobserve_property(observer_id: i64) -> Self {
    Self::new(vec!["unobserve_property".into(), observer_id.into()])
  }

  /// Add an external subtitle file.
  ///
  /// MPV sub-add format: `sub-add <url> [<flags> [<title> [<lang>]]]`
  /// Flags: "select" (select immediately), "auto" (don't select), "cached" (cache only)
  pub fn sub_add(url: &str, flags: Option<&str>) -> Self {
    let mut args: Vec<serde_json::Value> = vec!["sub-add".into(), url.into()];
    if let Some(f) = flags {
      args.push(f.into());
    }
    Self::new(args)
  }
}

/// Response from MPV for a command.
#[derive(Debug, Clone, Deserialize)]
pub struct MpvResponse {
  /// "success" or error message.
  pub error: String,
  /// Response data (command-specific).
  pub data: Option<serde_json::Value>,
  /// Matching request ID.
  pub request_id: i64,
}

impl MpvResponse {
  /// Check if the command succeeded.
  pub fn is_success(&self) -> bool {
    self.error == "success"
  }
}

/// Event sent by MPV (property changes, playback events, etc.).
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // MPV protocol fields - may be used later
pub struct MpvEvent {
  /// Event type (e.g., "property-change", "end-file", "client-message").
  pub event: String,
  /// Observer ID for property-change events.
  pub id: Option<i64>,
  /// Property name for property-change events.
  pub name: Option<String>,
  /// Event data.
  pub data: Option<serde_json::Value>,
  /// Reason for end-file events (e.g., "eof", "stop", "quit", "error").
  pub reason: Option<String>,
  /// Arguments for client-message events (from script-message command).
  pub args: Option<Vec<String>>,
}

/// Typed property values from MPV.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(untagged)]
pub enum PropertyValue {
  Bool(bool),
  Number(f64),
  String(String),
  /// Arrays serialized as JSON string for specta compatibility.
  Json(String),
  Null,
}

impl From<serde_json::Value> for PropertyValue {
  fn from(value: serde_json::Value) -> Self {
    match value {
      serde_json::Value::Bool(b) => PropertyValue::Bool(b),
      serde_json::Value::Number(n) => PropertyValue::Number(n.as_f64().unwrap_or(0.0)),
      serde_json::Value::String(s) => PropertyValue::String(s),
      serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
        PropertyValue::Json(value.to_string())
      }
      serde_json::Value::Null => PropertyValue::Null,
    }
  }
}

/// Message received from MPV IPC (either response or event).
#[derive(Debug, Clone)]
pub enum MpvMessage {
  Response(MpvResponse),
  Event(MpvEvent),
}

impl MpvMessage {
  /// Parse a JSON line from MPV.
  pub fn parse(line: &str) -> Result<Self, serde_json::Error> {
    // Try parsing as response first (has request_id)
    if line.contains("request_id") {
      let response: MpvResponse = serde_json::from_str(line)?;
      Ok(MpvMessage::Response(response))
    } else if line.contains("\"event\"") {
      let event: MpvEvent = serde_json::from_str(line)?;
      Ok(MpvMessage::Event(event))
    } else {
      // Fallback to event
      let event: MpvEvent = serde_json::from_str(line)?;
      Ok(MpvMessage::Event(event))
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_command_serialization() {
    let cmd = MpvCommand::loadfile("http://example.com/video.mp4");
    let json = serde_json::to_string(&cmd).unwrap();
    assert!(json.contains("loadfile"));
    assert!(json.contains("http://example.com/video.mp4"));
  }

  #[test]
  fn test_response_parsing() {
    let json = r#"{"error":"success","data":null,"request_id":1}"#;
    let msg = MpvMessage::parse(json).unwrap();
    match msg {
      MpvMessage::Response(r) => {
        assert!(r.is_success());
        assert_eq!(r.request_id, 1);
      }
      _ => panic!("Expected response"),
    }
  }

  #[test]
  fn test_event_parsing() {
    let json = r#"{"event":"property-change","id":1,"name":"pause","data":false}"#;
    let msg = MpvMessage::parse(json).unwrap();
    match msg {
      MpvMessage::Event(e) => {
        assert_eq!(e.event, "property-change");
        assert_eq!(e.name, Some("pause".to_string()));
      }
      _ => panic!("Expected event"),
    }
  }
}
