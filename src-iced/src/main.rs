fn main() -> iced::Result {
  if std::env::args().any(|argument| argument == "--smoke-test") {
    jellypilot_iced::run_smoke()
  } else {
    jellypilot_iced::run()
  }
}
