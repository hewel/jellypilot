// glibc's per-thread malloc arenas pin dozens of ~64 MiB reservations across
// Tokio/wgpu worker threads and never return freed pages, so the process shows
// high and never-shrinking memory. mimalloc reclaims aggressively instead.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Tees tracing output into the process-wide support-log buffer (Settings →
/// Diagnostics exports it) while preserving the existing stderr stream.
struct LogTee;

impl<'a> tracing_subscriber::fmt::writer::MakeWriter<'a> for LogTee {
  type Writer = LogTeeWriter;

  fn make_writer(&'a self) -> Self::Writer {
    LogTeeWriter
  }
}

struct LogTeeWriter;

impl std::io::Write for LogTeeWriter {
  fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
    let _ = std::io::Write::write(&mut std::io::stderr(), buffer);
    jellypilot_core::logs::global().append(buffer);
    Ok(buffer.len())
  }

  fn flush(&mut self) -> std::io::Result<()> {
    Ok(())
  }
}

fn main() -> iced::Result {
  let filter = std::env::var("JELLYPILOT_LOG").unwrap_or_else(|_| "warn".to_owned());
  tracing_subscriber::fmt()
    .with_env_filter(filter)
    .with_writer(LogTee)
    .init();
  if std::env::args().any(|argument| argument == "--smoke-test") {
    jellypilot_iced::run_smoke()
  } else {
    jellypilot_iced::run()
  }
}
