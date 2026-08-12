fn main() {
  if std::env::args().any(|argument| argument == "--smoke-test") {
    jellypilot_gtk::run_smoke();
  } else {
    jellypilot_gtk::run();
  }
}
