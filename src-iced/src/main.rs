fn main() -> iced::Result {
  let filter = std::env::var("JELLYPILOT_LOG").unwrap_or_else(|_| "warn".to_owned());
  tracing_subscriber::fmt()
    .with_env_filter(filter)
    .with_writer(std::io::stderr)
    .init();
  if std::env::args().any(|argument| argument == "--smoke-test") {
    jellypilot_iced::run_smoke()
  } else {
    jellypilot_iced::run()
  }
}
