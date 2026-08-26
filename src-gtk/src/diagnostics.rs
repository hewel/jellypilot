use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const MAX_DIAGNOSTIC_EVENTS: usize = 200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiagnosticLevel {
  Info,
  Warning,
  Error,
}

impl DiagnosticLevel {
  pub(crate) const fn label(self) -> &'static str {
    match self {
      Self::Info => "INFO",
      Self::Warning => "WARN",
      Self::Error => "ERROR",
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiagnosticCategory {
  Connection,
  Auth,
  Playback,
  RemoteControl,
  Artwork,
  Config,
}

impl DiagnosticCategory {
  pub(crate) const fn label(self) -> &'static str {
    match self {
      Self::Connection => "Connection",
      Self::Auth => "Auth",
      Self::Playback => "Playback",
      Self::RemoteControl => "Remote Control",
      Self::Artwork => "Artwork",
      Self::Config => "Config",
    }
  }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DiagnosticEvent {
  pub(crate) id: u64,
  pub(crate) timestamp_seconds: u64,
  pub(crate) level: DiagnosticLevel,
  pub(crate) category: DiagnosticCategory,
  pub(crate) message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DiagnosticChange {
  Added {
    event: DiagnosticEvent,
    dropped_id: Option<u64>,
  },
  Updated(DiagnosticEvent),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiagnosticsViewState {
  Empty,
  Events { count: usize },
}

struct CoalescedRecord {
  key: String,
  event_id: u64,
  count: u64,
  base_message: String,
}

#[derive(Default)]
pub(crate) struct Diagnostics {
  events: VecDeque<DiagnosticEvent>,
  next_id: u64,
  coalesced: Option<CoalescedRecord>,
}

impl Diagnostics {
  pub(crate) fn record(
    &mut self,
    level: DiagnosticLevel,
    category: DiagnosticCategory,
    message: impl AsRef<str>,
  ) -> DiagnosticChange {
    self.record_at(current_timestamp_seconds(), level, category, message)
  }

  pub(crate) fn record_at(
    &mut self,
    timestamp_seconds: u64,
    level: DiagnosticLevel,
    category: DiagnosticCategory,
    message: impl AsRef<str>,
  ) -> DiagnosticChange {
    self.next_id = self.next_id.wrapping_add(1);
    let event = DiagnosticEvent {
      id: self.next_id,
      timestamp_seconds,
      level,
      category,
      message: sanitize_message(message.as_ref()),
    };
    let dropped_id = if self.events.len() == MAX_DIAGNOSTIC_EVENTS {
      self.events.pop_front().map(|event| event.id)
    } else {
      None
    };
    self.events.push_back(event.clone());
    DiagnosticChange::Added { event, dropped_id }
  }

  pub(crate) fn record_coalesced(
    &mut self,
    key: &str,
    level: DiagnosticLevel,
    category: DiagnosticCategory,
    message: &str,
  ) -> DiagnosticChange {
    self.record_coalesced_at(current_timestamp_seconds(), key, level, category, message)
  }

  pub(crate) fn record_coalesced_at(
    &mut self,
    timestamp_seconds: u64,
    key: &str,
    level: DiagnosticLevel,
    category: DiagnosticCategory,
    message: &str,
  ) -> DiagnosticChange {
    if let Some(coalesced) = self
      .coalesced
      .as_mut()
      .filter(|coalesced| coalesced.key == key)
    {
      if let Some(event) = self
        .events
        .iter_mut()
        .find(|event| event.id == coalesced.event_id)
      {
        coalesced.count = coalesced.count.saturating_add(1);
        event.message = format!("{} (×{})", coalesced.base_message, coalesced.count);
        return DiagnosticChange::Updated(event.clone());
      }
    }

    let base_message = sanitize_message(message);
    let change = self.record_at(timestamp_seconds, level, category, &base_message);
    if let DiagnosticChange::Added { event, .. } = &change {
      self.coalesced = Some(CoalescedRecord {
        key: key.to_owned(),
        event_id: event.id,
        count: 1,
        base_message,
      });
    }
    change
  }

  pub(crate) fn reset_coalescing(&mut self) {
    self.coalesced = None;
  }

  pub(crate) fn events(&self) -> impl ExactSizeIterator<Item = &DiagnosticEvent> {
    self.events.iter()
  }

  pub(crate) fn clear(&mut self) {
    self.events.clear();
    self.coalesced = None;
  }

  pub(crate) fn view_state(&self) -> DiagnosticsViewState {
    if self.events.is_empty() {
      DiagnosticsViewState::Empty
    } else {
      DiagnosticsViewState::Events {
        count: self.events.len(),
      }
    }
  }
}

fn current_timestamp_seconds() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_secs()
}

pub(crate) fn sanitize_message(message: &str) -> String {
  let lowercase = message.to_ascii_lowercase();
  let lowercase_bytes = lowercase.as_bytes();
  let mut sanitized = String::with_capacity(message.len());
  let mut index = 0;

  while index < message.len() {
    if bearer_token_starts_at(lowercase_bytes, index) {
      let prefix_end = index + "bearer ".len();
      sanitized.push_str(&message[index..prefix_end]);
      sanitized.push_str("[REDACTED]");
      index = secret_value_end(message, prefix_end);
      continue;
    }

    if matches!(lowercase_bytes[index], b'?' | b'&') {
      if let Some(value_start) = sensitive_query_value_start(lowercase_bytes, index) {
        sanitized.push_str(&message[index..value_start]);
        sanitized.push_str("[REDACTED]");
        index = secret_value_end(message, value_start);
        continue;
      }
    }

    let character = message[index..]
      .chars()
      .next()
      .expect("index remains on a character boundary");
    sanitized.push(character);
    index += character.len_utf8();
  }

  sanitized
}

fn bearer_token_starts_at(message: &[u8], index: usize) -> bool {
  message[index..].starts_with(b"bearer ")
}

fn sensitive_query_value_start(message: &[u8], index: usize) -> Option<usize> {
  let key_start = index + 1;
  let equals = message[key_start..].iter().position(|byte| *byte == b'=')? + key_start;
  if message[key_start..equals]
    .iter()
    .any(|byte| matches!(*byte, b'&' | b'?' | b'#') || byte.is_ascii_whitespace())
  {
    return None;
  }
  let key = &message[key_start..equals];
  let sensitive = [
    b"api_key".as_slice(),
    b"access_token".as_slice(),
    b"token".as_slice(),
    b"password".as_slice(),
    b"auth".as_slice(),
    b"authorization".as_slice(),
  ];
  sensitive.contains(&key).then_some(equals + 1)
}

fn secret_value_end(message: &str, start: usize) -> usize {
  message.as_bytes()[start..]
    .iter()
    .position(|byte| matches!(*byte, b'&' | b'#') || byte.is_ascii_whitespace())
    .map_or(message.len(), |offset| start + offset)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn buffer_keeps_latest_two_hundred_events_in_order() {
    let mut diagnostics = Diagnostics::default();
    for index in 0..=MAX_DIAGNOSTIC_EVENTS {
      diagnostics.record_at(
        index as u64,
        DiagnosticLevel::Info,
        DiagnosticCategory::Connection,
        format!("event {index}"),
      );
    }

    let events = diagnostics.events().collect::<Vec<_>>();
    assert_eq!(events.len(), MAX_DIAGNOSTIC_EVENTS);
    assert_eq!(
      events.first().map(|event| event.message.as_str()),
      Some("event 1")
    );
    assert_eq!(
      events.last().map(|event| event.message.as_str()),
      Some("event 200")
    );
  }

  #[test]
  fn record_redacts_query_secrets_and_bearer_tokens_again() {
    let mut diagnostics = Diagnostics::default();
    diagnostics.record_at(
      1,
      DiagnosticLevel::Error,
      DiagnosticCategory::Playback,
      "GET https://media.example/video?api_key=secret&token=other Password bearer TOP-SECRET",
    );

    let message = &diagnostics.events().next().expect("event recorded").message;
    assert_eq!(
      message,
      "GET https://media.example/video?api_key=[REDACTED]&token=[REDACTED] Password bearer [REDACTED]"
    );
    assert!(!message.contains("secret"));
    assert!(!message.contains("other"));
    assert!(!message.contains("TOP-SECRET"));
    assert_eq!(
      sanitize_message("Authorization:Bearer another-secret"),
      "Authorization:Bearer [REDACTED]"
    );
  }

  #[test]
  fn coalesced_events_update_one_entry_until_reset() {
    let mut diagnostics = Diagnostics::default();
    for timestamp in 1..=3 {
      diagnostics.record_coalesced_at(
        timestamp,
        "artwork-failure",
        DiagnosticLevel::Warning,
        DiagnosticCategory::Artwork,
        "Artwork could not be loaded or decoded.",
      );
    }

    let events = diagnostics.events().collect::<Vec<_>>();
    assert_eq!(events.len(), 1);
    assert_eq!(
      events[0].message,
      "Artwork could not be loaded or decoded. (×3)"
    );

    diagnostics.reset_coalescing();
    diagnostics.record_coalesced_at(
      4,
      "artwork-failure",
      DiagnosticLevel::Warning,
      DiagnosticCategory::Artwork,
      "Artwork could not be loaded or decoded.",
    );

    assert_eq!(diagnostics.events().len(), 2);
  }

  #[test]
  fn clear_removes_every_event() {
    let mut diagnostics = Diagnostics::default();
    diagnostics.record_at(
      1,
      DiagnosticLevel::Warning,
      DiagnosticCategory::Config,
      "Configuration could not be saved.",
    );

    diagnostics.clear();

    assert_eq!(diagnostics.events().len(), 0);
  }

  #[test]
  fn view_state_maps_empty_and_populated_buffers() {
    let mut diagnostics = Diagnostics::default();
    assert_eq!(diagnostics.view_state(), DiagnosticsViewState::Empty);

    diagnostics.record_at(
      1,
      DiagnosticLevel::Info,
      DiagnosticCategory::RemoteControl,
      "Remote control connected.",
    );

    assert_eq!(
      diagnostics.view_state(),
      DiagnosticsViewState::Events { count: 1 }
    );
  }
}
