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

pub(super) fn run(
  factory: crate::EmbeddedEngineFactory,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
  let filter = std::env::var("JELLYPILOT_LOG").unwrap_or_else(|_| "warn".to_owned());
  tracing_subscriber::fmt()
    .with_env_filter(filter)
    .with_writer(LogTee)
    .init();
  let arguments: Vec<_> = std::env::args().collect();
  crate::regression::initialize(&arguments)?;
  let result = crate::run_application(
    arguments.iter().any(|argument| argument == "--smoke-test"),
    factory,
  );
  crate::embedded::cleanup();
  crate::regression::finish(result)
}
