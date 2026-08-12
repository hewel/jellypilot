//! Native GTK frontend for JellyPilot.
//!
//! GTK and Relm4 stay in [`shell`]. The [`library_browse`] module is a
//! framework-independent adapter around `jellypilot-core`, so its behavior can
//! be tested without initializing GTK.

#[cfg(target_os = "linux")]
pub mod library_browse;
#[cfg(target_os = "linux")]
mod shell;

/// Starts the native GTK application and blocks until its last window closes.
#[cfg(target_os = "linux")]
pub fn run() {
  shell::run(false);
}

/// Starts the GTK application and closes it after the first window is realized.
///
/// This is a Linux-native startup smoke gate, not a feature-level UI test.
#[cfg(target_os = "linux")]
pub fn run_smoke() {
  shell::run(true);
}

/// Reports that the GTK frontend is not supported by this operating system.
#[cfg(not(target_os = "linux"))]
pub fn run() {
  eprintln!("The JellyPilot GTK frontend is available on Linux only.");
}

/// Reports that the GTK startup smoke gate is not supported by this operating system.
#[cfg(not(target_os = "linux"))]
pub fn run_smoke() {
  eprintln!("The JellyPilot GTK frontend smoke gate is available on Linux only.");
}
