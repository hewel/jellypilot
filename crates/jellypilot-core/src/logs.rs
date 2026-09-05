//! Support-log capture and export (native builds): the iced binary tees its
//! tracing writer into a bounded in-memory buffer, and the Settings
//! Diagnostics section exports that buffer together with the sanitized
//! diagnostic events into a timestamped file under the config directory.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use crate::config::CONFIG_DIRECTORY;
use crate::diagnostics::{
    current_timestamp_seconds, format_diagnostic_time, format_file_timestamp, DiagnosticRow,
};

/// Default capture budget: generous enough for a debug session's tail while
/// bounding memory on long-running instances.
const DEFAULT_CAPACITY: usize = 8 * 1024 * 1024;

/// Bounded in-memory capture of the app's tracing output. Append-only;
/// once the byte budget is exceeded the oldest complete lines are evicted.
pub struct LogBuffer {
    inner: Mutex<Vec<u8>>,
    capacity: usize,
}

impl LogBuffer {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(Vec::new()),
            capacity,
        }
    }

    /// Appends raw writer output. Lock poisoning is ignored: logging must
    /// never take the app down.
    pub fn append(&self, bytes: &[u8]) {
        let Ok(mut buffer) = self.inner.lock() else {
            return;
        };
        buffer.extend_from_slice(bytes);
        if buffer.len() > self.capacity {
            let excess = buffer.len() - self.capacity;
            buffer.drain(..excess);
            let boundary = buffer
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(0, |index| index + 1);
            buffer.drain(..boundary);
        }
    }

    /// Returns a copy of the buffered bytes, oldest first.
    #[must_use]
    pub fn snapshot(&self) -> Vec<u8> {
        self.inner
            .lock()
            .map(|buffer| buffer.clone())
            .unwrap_or_default()
    }
}

static GLOBAL_BUFFER: LazyLock<LogBuffer> = LazyLock::new(|| LogBuffer::new(DEFAULT_CAPACITY));

/// Process-wide capture filled by the binary's tracing tee.
#[must_use]
pub fn global() -> &'static LogBuffer {
    &GLOBAL_BUFFER
}

/// Current Unix time in seconds; shared by the export header and file name.
#[must_use]
pub fn now_seconds() -> u64 {
    current_timestamp_seconds()
}

/// Builds the support document: app/runtime header, the sanitized diagnostic
/// events, then the buffered log output (lossily decoded).
#[must_use]
pub fn build_support_document<'a>(
    app_version: &str,
    exported_at_seconds: u64,
    diagnostics: impl Iterator<Item = DiagnosticRow<'a>>,
    log_bytes: &[u8],
) -> String {
    let mut document = format!(
        "JellyPilot {app_version} ({} {}) log export at {}\n\n== Diagnostics ==\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
        format_diagnostic_time(exported_at_seconds),
    );
    let mut count = 0_u32;
    for row in diagnostics {
        count += 1;
        document.push_str(&format!(
            "{} {} [{}] {}\n",
            format_diagnostic_time(row.timestamp_seconds),
            row.level.label(),
            row.category.label(),
            row.message,
        ));
    }
    if count == 0 {
        document.push_str("(no diagnostic events)\n");
    }
    document.push_str("\n== Log output ==\n");
    document.push_str(&String::from_utf8_lossy(log_bytes));
    document
}

/// Writes `document` to a fresh timestamped file under
/// `<config>/jellypilot/logs/` and returns its path.
pub fn write_support_document(document: &str, exported_at_seconds: u64) -> io::Result<PathBuf> {
    write_to(&export_directory(), document, exported_at_seconds)
}

fn export_directory() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(CONFIG_DIRECTORY)
        .join("logs")
}

fn write_to(directory: &Path, document: &str, exported_at_seconds: u64) -> io::Result<PathBuf> {
    fs::create_dir_all(directory)?;
    let path = directory.join(format!(
        "jellypilot-logs-{}.log",
        format_file_timestamp(exported_at_seconds)
    ));
    fs::write(&path, document)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_keeps_appended_bytes_in_order() {
        let buffer = LogBuffer::new(1024);
        buffer.append(b"first\n");
        buffer.append(b"second\n");
        assert_eq!(buffer.snapshot(), b"first\nsecond\n");
    }

    #[test]
    fn buffer_evicts_oldest_complete_lines_once_over_budget() {
        let buffer = LogBuffer::new(16);
        buffer.append(b"aaaa\n");
        buffer.append(b"bbbb\n");
        buffer.append(b"cccc\n");
        buffer.append(b"dddd\n");

        assert_eq!(buffer.snapshot(), b"bbbb\ncccc\ndddd\n");
    }

    #[test]
    fn buffer_tolerates_oversized_single_line() {
        let buffer = LogBuffer::new(4);
        buffer.append(b"no-newline-here");

        assert!(buffer.snapshot().len() <= 4);
    }

    #[test]
    fn support_document_contains_header_events_and_log_tail() {
        let mut diagnostics = crate::diagnostics::Diagnostics::default();
        diagnostics.record_at(
            86_401,
            crate::diagnostics::DiagnosticLevel::Warning,
            crate::diagnostics::DiagnosticCategory::Artwork,
            "something failed",
        );

        let document = build_support_document(
            "2.0.0",
            86_401,
            diagnostics.rows(),
            b"raw log line\npartial \xFF tail",
        );

        assert!(document.contains("JellyPilot 2.0.0"));
        assert!(document.contains("1970-01-02 00:00:01 UTC"));
        assert!(document.contains("WARN [Artwork] something failed"));
        assert!(document.contains("raw log line"));
        assert!(document.contains("partial \u{FFFD} tail"));
    }

    #[test]
    fn support_document_marks_empty_diagnostics() {
        let document = build_support_document(
            "2.0.0",
            0,
            crate::diagnostics::Diagnostics::default().rows(),
            b"",
        );
        assert!(document.contains("(no diagnostic events)"));
    }

    #[test]
    fn write_to_creates_timestamped_file_with_document() {
        let directory = std::env::temp_dir().join(format!(
            "jellypilot-logs-test-{}-{}",
            std::process::id(),
            now_seconds()
        ));
        let result = write_to(&directory, "document body", 86_401);

        let path = result.expect("export writes");
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("jellypilot-logs-19700102-000001.log")
        );
        assert_eq!(
            fs::read_to_string(&path).expect("export reads back"),
            "document body"
        );

        fs::remove_dir_all(&directory).expect("cleanup");
    }
}
