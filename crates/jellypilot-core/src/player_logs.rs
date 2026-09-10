//! Opt-in, bounded player diagnostics. Credentials are removed before capture;
//! recording never writes a file or forwards mpv's output to application tracing.

use std::collections::VecDeque;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;

use crate::diagnostics::sanitize_message;

const DEFAULT_CAPACITY: usize = 8 * 1024 * 1024;
const MAX_MESSAGE_BYTES: usize = 16 * 1024;

#[derive(Default)]
struct Capture {
    enabled: bool,
    lines: VecDeque<String>,
    bytes: usize,
    discarded: u64,
    oversized: u64,
}

impl Capture {
    fn append(&mut self, line: String, capacity: usize) {
        if line.len() > capacity {
            self.discarded = self.discarded.saturating_add(1);
            return;
        }
        while self.bytes + line.len() > capacity {
            let Some(oldest) = self.lines.pop_front() else {
                break;
            };
            self.bytes -= oldest.len();
            self.discarded = self.discarded.saturating_add(1);
        }
        self.bytes += line.len();
        self.lines.push_back(line);
    }
}

/// In-memory mpv log tail, disabled by default. The supplied byte capacity is
/// capped at 8 MiB; older complete records are discarded when the budget fills.
pub struct PlayerLogs {
    capture: Mutex<Capture>,
    started: Instant,
    capacity: usize,
}

impl PlayerLogs {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capture: Mutex::new(Capture::default()),
            started: Instant::now(),
            capacity: capacity.min(DEFAULT_CAPACITY),
        }
    }

    #[must_use]
    pub fn enabled(&self) -> bool {
        self.capture.lock().is_ok_and(|capture| capture.enabled)
    }

    /// Enables or stops capture without erasing already captured evidence.
    pub fn set_enabled(&self, enabled: bool) {
        let Ok(mut capture) = self.capture.lock() else {
            return;
        };
        if capture.enabled == enabled {
            return;
        }
        capture.enabled = enabled;
        let state = if enabled { "enabled" } else { "disabled" };
        capture.append(
            format!(
                "+{}ms [player capture {state}]\n",
                self.started.elapsed().as_millis()
            ),
            self.capacity,
        );
    }

    /// Captures a sanitized mpv event with its IPC connection, module prefix,
    /// original severity, and milliseconds since this capture buffer was created.
    /// Oversized messages are omitted rather than copied or partially redacted.
    pub fn record(&self, connection: u64, prefix: &str, level: &str, text: &str) {
        let Ok(mut capture) = self.capture.lock() else {
            return;
        };
        if !capture.enabled {
            return;
        }

        let message = if text.len() > MAX_MESSAGE_BYTES {
            capture.oversized = capture.oversized.saturating_add(1);
            "[oversized mpv message omitted]".to_owned()
        } else if contains_sensitive_context(text) {
            "[URL, header, or indented mpv message omitted]".to_owned()
        } else {
            sanitize_message(text)
        };
        let prefix = safe_label(prefix);
        let level = safe_label(level);
        let millis = self.started.elapsed().as_millis();
        for line in message.lines() {
            capture.append(
                format!("+{millis}ms connection={connection} [{prefix}] [{level}] {line}\n"),
                self.capacity,
            );
        }
    }

    /// Returns the sanitized tail, oldest first, with capture state and explicit
    /// loss markers. These small headers are outside the retained-record budget.
    #[must_use]
    pub fn snapshot(&self) -> String {
        let Ok(capture) = self.capture.lock() else {
            return "[player capture unavailable]\n".to_owned();
        };
        let state = if capture.enabled {
            "enabled"
        } else {
            "disabled"
        };
        let mut snapshot =
            format!("[player capture {state}; timestamps relative to capture buffer creation]\n");
        if capture.discarded > 0 || capture.oversized > 0 {
            snapshot.push_str(&format!(
                "[player log truncated: {} records discarded; {} oversized messages omitted]\n",
                capture.discarded, capture.oversized
            ));
        }
        for line in &capture.lines {
            snapshot.push_str(line);
        }
        snapshot
    }
}

fn safe_label(label: &str) -> &str {
    if label.len() <= 96
        && label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'.'))
    {
        label
    } else {
        "invalid-label"
    }
}

fn contains_sensitive_context(text: &str) -> bool {
    let lowercase = text.to_ascii_lowercase();
    [
        "://",
        "http:",
        "https:",
        "http-header",
        "http-proxy",
        "authorization",
        "cookie",
        "password",
        "passwd",
        "credential",
        "token",
        "api_key",
        "apikey",
        "signature",
        "x-amz-",
        "x-goog-",
        "awsaccesskeyid",
        "key-pair-id",
    ]
    .iter()
    .any(|pattern| lowercase.contains(pattern))
        || lowercase.contains('?')
        // mpv splits multiline output into separate events. An indented event
        // may be a folded credential header even when its label was omitted
        // earlier or another module logged in between. Do not retain its bytes.
        || lowercase.lines().any(|line| line.starts_with([' ', '\t']))
        || lowercase.lines().any(|line| {
            // HTTP header names are case-insensitive tokens; conservatively omit
            // arbitrary headers too, including custom authentication schemes.
            line.trim_start().split_once(':').is_some_and(|(name, _)| {
                !name.is_empty()
                    && !matches!(name, "ao" | "vo")
                    && name.len() <= 64
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            })
        })
}

static GLOBAL: LazyLock<Arc<PlayerLogs>> =
    LazyLock::new(|| Arc::new(PlayerLogs::new(DEFAULT_CAPACITY)));

/// Process-wide capture shared by IPC connections and support export.
#[must_use]
pub fn global() -> &'static Arc<PlayerLogs> {
    &GLOBAL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_is_opt_in_and_disabling_preserves_existing_evidence() {
        let logs = PlayerLogs::new(4096);
        logs.record(1, "ad", "debug", "before capture");
        logs.set_enabled(true);
        logs.record(2, "demux", "v", "refresh seek to 1870.0");
        logs.set_enabled(false);
        logs.record(2, "ad", "debug", "after capture");
        let snapshot = logs.snapshot();

        assert!(!logs.enabled());
        assert!(!snapshot.contains("before capture"));
        assert!(!snapshot.contains("after capture"));
        assert!(snapshot.contains("ms connection=2 [demux] [v] refresh seek to 1870.0"));
        assert!(snapshot.contains("[player capture enabled]"));
        assert!(snapshot.contains("[player capture disabled]"));
    }

    #[test]
    fn secrets_are_removed_before_entering_the_buffer() {
        let logs = PlayerLogs::new(16 * 1024);
        logs.set_enabled(true);
        for message in [
            "Setting option 'http-header-fields' = 'X-Emby-Authorization: MediaBrowser Token=\"emby-secret\"'",
            "X-Emby-Authorization: MediaBrowser Client=\"JellyPilot\",\n Token=\"folded-secret\"",
            "Authorization: Basic basic-secret",
            "X-Private-Authentication: custom-secret",
            "Cookie: session=cookie-secret",
            "Opening https://user:pass@example.invalid/video?X-Amz-Signature=signed-secret",
            "GET /video?custom-signature=query-secret HTTP/1.1",
            "credential Bearer bearer-secret expires soon",
        ] {
            logs.record(1, "ffmpeg", "debug", message);
        }
        logs.record(1, "ipc", "info", "Bearer standalone-secret accepted");
        logs.record(1, "ao/pipewire", "v", "starting audio playback");
        logs.record(
            1,
            "cplayer",
            "info",
            "AO: [pipewire] 48000Hz 5.1(side) 6ch floatp",
        );
        logs.record(1, "cplayer", "info", "VO: [gpu-next] 3840x2160 vaapi[p010]");
        let capture = logs.capture.lock().expect("capture lock");
        let retained = capture.lines.iter().cloned().collect::<String>();

        assert!(
            !retained.contains("-secret"),
            "secrets must never reach retained memory"
        );
        assert!(retained.contains("Bearer [REDACTED] accepted"));
        assert!(retained.contains("[ao/pipewire] [v] starting audio playback"));
        assert!(retained.contains("AO: [pipewire] 48000Hz 5.1(side) 6ch floatp"));
        assert!(retained.contains("VO: [gpu-next] 3840x2160 vaapi[p010]"));
    }

    #[test]
    fn overflowing_capture_keeps_recent_whole_records_and_reports_loss() {
        let logs = PlayerLogs::new(256);
        logs.set_enabled(true);
        for index in 0..30 {
            logs.record(1, "ad", "v", &format!("audio frame {index}"));
        }
        let snapshot = logs.snapshot();

        assert!(snapshot.contains("player log truncated"));
        assert!(!snapshot.contains("audio frame 0\n"));
        assert!(snapshot.contains("audio frame 29\n"));
        assert!(logs.capture.lock().expect("capture lock").bytes <= 256);
    }

    #[test]
    fn separate_header_continuations_never_enter_capture_even_with_interleaved_events() {
        let logs = PlayerLogs::new(4096);
        logs.set_enabled(true);
        logs.record(1, "ffmpeg", "v", "X-Private-Authentication: first-secret");
        logs.record(2, "demux", "v", "refresh seek to 1870.0");
        logs.record(1, "ffmpeg", "v", "  continuation-secret");
        logs.record(1, "ffmpeg", "v", "\tsecond-secret");
        logs.record(1, "cplayer", "v", "delaying audio start 22.6");
        logs.record(1, "cplayer", "v", "starting audio playback");
        let capture = logs.capture.lock().expect("capture lock");
        let retained = capture.lines.iter().cloned().collect::<String>();

        assert!(!retained.contains("-secret"));
        assert!(retained.contains("refresh seek to 1870.0"));
        assert!(retained.contains("delaying audio start 22.6"));
        assert!(retained.contains("starting audio playback"));
    }

    #[test]
    fn oversized_unicode_message_is_omitted_without_losing_following_records() {
        let logs = PlayerLogs::new(1024);
        logs.set_enabled(true);
        logs.record(1, "ffmpeg", "debug", &"secret字".repeat(MAX_MESSAGE_BYTES));
        logs.record(1, "ad", "v", "delaying audio start 22.6");
        let snapshot = logs.snapshot();

        assert!(snapshot.contains("1 oversized messages omitted"));
        assert!(!snapshot.contains("secret字"));
        assert!(snapshot.contains("delaying audio start 22.6"));
        assert!(logs.capture.lock().expect("capture lock").bytes <= 1024);
    }
}
