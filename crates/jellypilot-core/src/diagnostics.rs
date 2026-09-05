use std::collections::hash_map::DefaultHasher;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_DIAGNOSTIC_EVENTS: usize = 200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

impl DiagnosticLevel {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warning => "WARN",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticCategory {
    Connection,
    Auth,
    Playback,
    RemoteControl,
    Artwork,
    Config,
}

impl DiagnosticCategory {
    pub const fn label(self) -> &'static str {
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

struct DiagnosticEvent {
    id: u64,
    timestamp_seconds: u64,
    level: DiagnosticLevel,
    category: DiagnosticCategory,
    message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticChange {
    Added { id: u64, dropped_id: Option<u64> },
    Updated { id: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticRow<'a> {
    pub id: u64,
    pub timestamp_seconds: u64,
    pub level: DiagnosticLevel,
    pub category: DiagnosticCategory,
    pub message: &'a str,
}

impl DiagnosticEvent {
    fn row(&self) -> DiagnosticRow<'_> {
        DiagnosticRow {
            id: self.id,
            timestamp_seconds: self.timestamp_seconds,
            level: self.level,
            category: self.category,
            message: &self.message,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticsViewState {
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
pub struct Diagnostics {
    events: VecDeque<DiagnosticEvent>,
    next_id: u64,
    coalesced: Option<CoalescedRecord>,
}

impl Diagnostics {
    pub fn record(
        &mut self,
        level: DiagnosticLevel,
        category: DiagnosticCategory,
        message: impl AsRef<str>,
    ) -> DiagnosticChange {
        self.record_with_timestamp(current_timestamp_seconds(), level, category, message)
    }

    #[cfg(test)]
    pub(crate) fn record_at(
        &mut self,
        timestamp_seconds: u64,
        level: DiagnosticLevel,
        category: DiagnosticCategory,
        message: impl AsRef<str>,
    ) -> DiagnosticChange {
        self.record_with_timestamp(timestamp_seconds, level, category, message)
    }

    fn record_with_timestamp(
        &mut self,
        timestamp_seconds: u64,
        level: DiagnosticLevel,
        category: DiagnosticCategory,
        message: impl AsRef<str>,
    ) -> DiagnosticChange {
        self.record_sanitized(
            timestamp_seconds,
            level,
            category,
            sanitize_message(message.as_ref()),
        )
    }

    fn record_sanitized(
        &mut self,
        timestamp_seconds: u64,
        level: DiagnosticLevel,
        category: DiagnosticCategory,
        message: String,
    ) -> DiagnosticChange {
        self.next_id = self.next_id.wrapping_add(1);
        let event = DiagnosticEvent {
            id: self.next_id,
            timestamp_seconds,
            level,
            category,
            message,
        };
        let dropped_id = if self.events.len() == MAX_DIAGNOSTIC_EVENTS {
            self.events.pop_front().map(|event| event.id)
        } else {
            None
        };
        let id = event.id;
        self.events.push_back(event);
        DiagnosticChange::Added { id, dropped_id }
    }

    pub fn record_coalesced(
        &mut self,
        key: &str,
        level: DiagnosticLevel,
        category: DiagnosticCategory,
        message: &str,
    ) -> DiagnosticChange {
        self.record_coalesced_with_timestamp(
            current_timestamp_seconds(),
            key,
            level,
            category,
            message,
        )
    }

    #[cfg(test)]
    fn record_coalesced_at(
        &mut self,
        timestamp_seconds: u64,
        key: &str,
        level: DiagnosticLevel,
        category: DiagnosticCategory,
        message: &str,
    ) -> DiagnosticChange {
        self.record_coalesced_with_timestamp(timestamp_seconds, key, level, category, message)
    }

    fn record_coalesced_with_timestamp(
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
                return DiagnosticChange::Updated { id: event.id };
            }
        }

        let base_message = sanitize_message(message);
        let change =
            self.record_sanitized(timestamp_seconds, level, category, base_message.clone());
        if let DiagnosticChange::Added { id, .. } = change {
            self.coalesced = Some(CoalescedRecord {
                key: key.to_owned(),
                event_id: id,
                count: 1,
                base_message,
            });
        }
        change
    }

    pub fn reset_coalescing(&mut self) {
        self.coalesced = None;
    }

    pub fn rows(&self) -> impl ExactSizeIterator<Item = DiagnosticRow<'_>> {
        self.events.iter().map(DiagnosticEvent::row)
    }

    pub fn row(&self, id: u64) -> Option<DiagnosticRow<'_>> {
        self.events
            .iter()
            .find(|event| event.id == id)
            .map(DiagnosticEvent::row)
    }

    pub fn export_text(&self, mut format_timestamp: impl FnMut(u64) -> String) -> String {
        let mut text = String::new();
        for row in self.rows() {
            if !text.is_empty() {
                text.push('\n');
            }
            write!(
                text,
                "[{}] {} [{}] {}",
                format_timestamp(row.timestamp_seconds),
                row.level.label(),
                row.category.label(),
                row.message
            )
            .expect("writing to a String cannot fail");
        }
        text
    }

    pub fn clear(&mut self) {
        self.events.clear();
        self.coalesced = None;
    }

    pub fn view_state(&self) -> DiagnosticsViewState {
        if self.events.is_empty() {
            DiagnosticsViewState::Empty
        } else {
            DiagnosticsViewState::Events {
                count: self.events.len(),
            }
        }
    }
}

/// Stable coalescing key for `Diagnostics::record_coalesced`: the caller's
/// prefix plus a hash of the full message, so repeated identical events update
/// one row while differing messages open new ones.
#[must_use]
pub fn coalescing_key(prefix: &str, message: &str) -> String {
    let mut hasher = DefaultHasher::new();
    message.hash(&mut hasher);
    format!("{prefix}-{:x}", hasher.finish())
}

pub(crate) fn current_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn sanitize_message(message: &str) -> String {
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

/// Formats a Unix timestamp as a stable UTC date and time without platform APIs.
#[must_use]
pub fn format_diagnostic_time(timestamp_seconds: u64) -> String {
    let days = (timestamp_seconds / 86_400) as i64;
    let seconds_of_day = timestamp_seconds % 86_400;
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    let (year, month, day) = civil_date_from_unix_days(days);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}

/// Formats a Unix timestamp as `yyyymmdd-hhmmss` (UTC) for use in file names.
#[must_use]
pub fn format_file_timestamp(timestamp_seconds: u64) -> String {
    let days = (timestamp_seconds / 86_400) as i64;
    let seconds_of_day = timestamp_seconds % 86_400;
    let (year, month, day) = civil_date_from_unix_days(days);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        seconds_of_day / 3_600,
        (seconds_of_day % 3_600) / 60,
        seconds_of_day % 60,
    )
}

fn civil_date_from_unix_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_keeps_latest_two_hundred_rows_in_order() {
        let mut diagnostics = Diagnostics::default();
        for index in 0..=MAX_DIAGNOSTIC_EVENTS {
            diagnostics.record_at(
                index as u64,
                DiagnosticLevel::Info,
                DiagnosticCategory::Connection,
                format!("event {index}"),
            );
        }

        let rows = diagnostics.rows().collect::<Vec<_>>();
        assert_eq!(rows.len(), MAX_DIAGNOSTIC_EVENTS);
        assert_eq!(rows.first().map(|row| row.message), Some("event 1"));
        assert_eq!(rows.last().map(|row| row.message), Some("event 200"));
    }

    #[test]
    fn rows_and_export_text_expose_only_sanitized_event_data() {
        let mut diagnostics = Diagnostics::default();
        diagnostics.record_at(
            1,
            DiagnosticLevel::Error,
            DiagnosticCategory::Playback,
            "GET https://media.example/video?api_key=secret&token=other Password bearer TOP-SECRET",
        );

        let row = diagnostics.rows().next().expect("event recorded");
        assert_eq!(row.id, 1);
        assert_eq!(row.timestamp_seconds, 1);
        assert_eq!(row.level, DiagnosticLevel::Error);
        assert_eq!(row.category, DiagnosticCategory::Playback);
        assert_eq!(
      row.message,
      "GET https://media.example/video?api_key=[REDACTED]&token=[REDACTED] Password bearer [REDACTED]"
    );
        assert_eq!(
      diagnostics.export_text(|timestamp| format!("{timestamp} UTC")),
      "[1 UTC] ERROR [Playback] GET https://media.example/video?api_key=[REDACTED]&token=[REDACTED] Password bearer [REDACTED]"
    );
        assert_eq!(
            sanitize_message("Authorization:Bearer another-secret"),
            "Authorization:Bearer [REDACTED]"
        );
    }

    #[test]
    fn coalesced_events_update_one_row_until_reset() {
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

        let rows = diagnostics.rows().collect::<Vec<_>>();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].message,
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

        assert_eq!(diagnostics.rows().len(), 2);
    }

    #[test]
    fn coalescing_key_is_stable_per_message_and_scoped_by_prefix() {
        assert_eq!(
            coalescing_key("playback", "boom"),
            coalescing_key("playback", "boom")
        );
        assert_ne!(
            coalescing_key("playback", "boom"),
            coalescing_key("playback", "bam")
        );
        assert_ne!(
            coalescing_key("playback", "boom"),
            coalescing_key("remote", "boom")
        );
        assert!(coalescing_key("playback", "boom").starts_with("playback-"));
    }

    #[test]
    fn clear_removes_every_row() {
        let mut diagnostics = Diagnostics::default();
        diagnostics.record_at(
            1,
            DiagnosticLevel::Warning,
            DiagnosticCategory::Config,
            "Configuration could not be saved.",
        );

        diagnostics.clear();

        assert_eq!(diagnostics.rows().len(), 0);
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
    #[test]
    fn diagnostic_timestamp_includes_date_and_explicit_utc_zone() {
        assert_eq!(format_diagnostic_time(0), "1970-01-01 00:00:00 UTC");
    }
}
