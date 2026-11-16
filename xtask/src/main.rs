use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod android;
mod android_mpv;
mod android_toolchain;
mod api;
mod hooks;
mod iced;
mod linux_mpv_runtime;
mod monitor;
mod mpv;
mod process;
mod regression;
mod rust;
mod util;
mod windows_dependencies;
mod windows_package;

#[cfg(test)]
mod guardrails;

#[derive(Parser)]
#[command(name = "cargo xtask", about = "JellyPilot build and maintenance tasks")]
struct Cli {
  #[command(subcommand)]
  command: Task,
}

#[derive(Subcommand)]
enum Task {
  /// Check maintained Rust formatting, Clippy and build-tool tests.
  Check,
  /// Test both Cargo workspaces (temporary XDG configuration on Linux).
  Test,
  /// Format maintained Rust packages in both workspaces.
  Fmt {
    #[arg(long)]
    check: bool,
  },
  /// Select maintained packages across the shared and desktop workspaces.
  #[command(subcommand)]
  Rust(rust::Command),
  /// Prepare, build, run or probe the native desktop application.
  #[command(subcommand)]
  Iced(iced::Command),
  /// Build pinned embedded MPV and distribution runtimes.
  #[command(subcommand)]
  Mpv(mpv::Command),
  /// Create platform release packages.
  #[command(subcommand)]
  Package(windows_package::Command),
  /// Build and check the Android client and its native dependencies.
  #[command(subcommand)]
  Android(android::Command),
  /// Maintain API snapshots and generated clients (optional Java toolchain).
  #[command(subcommand)]
  Api(api::Command),
  /// Observe raw Linux resource usage without changing the target process.
  Monitor(monitor::Args),
  /// Explicitly install repository Git hooks.
  #[command(subcommand)]
  Hooks(hooks::Command),
}

pub fn root() -> &'static Path {
  Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}

fn execute(cli: Cli) -> Result<()> {
  std::env::set_current_dir(root()).context("Entering repository root")?;
  match cli.command {
    Task::Check => rust::check_all(),
    Task::Test => rust::test_all(),
    Task::Fmt { check } => rust::format(check),
    Task::Rust(task) => rust::run(task),
    Task::Iced(task) => iced::run(task),
    Task::Mpv(task) => mpv::run(task),
    Task::Package(task) => windows_package::run(task),
    Task::Android(task) => android::run(task),
    Task::Api(task) => api::run(task),
    Task::Monitor(args) => monitor::run(args),
    Task::Hooks(task) => hooks::run(task),
  }
}

fn main() -> ExitCode {
  let cli = Cli::parse();
  let result = process::install_signal_handler().and_then(|()| execute(cli));
  match result {
    Ok(()) => ExitCode::SUCCESS,
    Err(error) => {
      eprintln!("{error:#}");
      if process::interrupted() {
        ExitCode::from(130)
      } else {
        ExitCode::FAILURE
      }
    }
  }
}
