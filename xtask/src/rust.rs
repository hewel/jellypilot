use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{bail, Context, Result};
use clap::{Subcommand, ValueEnum};

use crate::process;

#[derive(Debug, Subcommand)]
pub enum Command {
  Fmt {
    #[arg(long)]
    check: bool,
  },
  Check {
    crates: Vec<Crate>,
  },
  Clippy {
    crates: Vec<Crate>,
  },
  /// Run tests with Linux XDG_CONFIG_HOME isolated in a temporary directory.
  Test {
    crates: Vec<Crate>,
  },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Crate {
  Auth,
  Core,
  MediaServer,
  Mpv,
  MpvHost,
  Session,
  Sdk,
  Ffi,
  Iced,
  Xtask,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Workspace {
  Shared,
  Desktop,
}

impl Workspace {
  const ALL: [Self; 2] = [Self::Shared, Self::Desktop];

  fn manifest(self) -> &'static str {
    match self {
      Self::Shared => "Cargo.toml",
      Self::Desktop => "src-iced/Cargo.toml",
    }
  }
}

impl Crate {
  const ALL: [Self; 10] = [
    Self::Auth,
    Self::Core,
    Self::MediaServer,
    Self::Mpv,
    Self::MpvHost,
    Self::Session,
    Self::Sdk,
    Self::Ffi,
    Self::Iced,
    Self::Xtask,
  ];

  fn workspace(self) -> Workspace {
    match self {
      Self::MpvHost | Self::Iced => Workspace::Desktop,
      _ => Workspace::Shared,
    }
  }

  fn packages(self) -> &'static [&'static str] {
    match self {
      Self::Auth => &["jellypilot-auth"],
      Self::Core => &["jellypilot-core"],
      Self::MediaServer => &["jellypilot-media-server"],
      Self::Mpv => &["jellypilot-mpv"],
      Self::MpvHost => &["jellypilot-mpv-host"],
      Self::Session => &["jellypilot-session"],
      Self::Sdk => &["jellypilot-sdk"],
      Self::Ffi => &["jellypilot-ffi"],
      Self::Iced => &[
        "jellypilot-ui",
        "jellypilot-mpv-host",
        "jellypilot-iced",
        "jellypilot-launcher",
      ],
      Self::Xtask => &["xtask"],
    }
  }
}

fn packages(workspace: Workspace, selected: &[Crate]) -> BTreeSet<&'static str> {
  let selected = if selected.is_empty() {
    &Crate::ALL
  } else {
    selected
  };
  selected
    .iter()
    .filter(|selector| selector.workspace() == workspace)
    .flat_map(|selector| selector.packages().iter().copied())
    .collect()
}

fn cargo_command(
  action: &str,
  workspace: Workspace,
  packages: &BTreeSet<&str>,
) -> std::process::Command {
  let mut command = process::cargo();
  command.args([action, "--manifest-path", workspace.manifest()]);
  for package in packages {
    command.args(["--package", package]);
  }
  command
}

pub fn run(command: Command) -> Result<()> {
  match command {
    Command::Fmt { check } => format(check),
    Command::Check { crates } => run_selected("check", &crates),
    Command::Clippy { crates } => run_selected("clippy", &crates),
    Command::Test { crates } => run_selected("test", &crates),
  }
}

pub fn format(check: bool) -> Result<()> {
  crate::iced::prepare(None)?;
  for workspace in Workspace::ALL {
    let mut command = cargo_command("fmt", workspace, &packages(workspace, &[]));
    if check {
      command.args(["--", "--check"]);
    }
    process::run(&mut command)?;
  }
  Ok(())
}

fn isolate_configuration(command: &mut std::process::Command, directory: &Path) {
  // dirs::config_dir honors this on Linux; native Windows/macOS paths are
  // unchanged. Do not change HOME and accidentally redirect toolchain state.
  command.env("XDG_CONFIG_HOME", directory);
}

fn run_selected(action: &str, selected: &[Crate]) -> Result<()> {
  let isolation = (action == "test")
    .then(|| {
      tempfile::Builder::new()
        .prefix("jellypilot-test-config-")
        .tempdir()
    })
    .transpose()
    .context("Creating isolated test configuration")?;
  for workspace in Workspace::ALL {
    let packages = packages(workspace, selected);
    if packages.is_empty() {
      continue;
    }
    if workspace == Workspace::Desktop {
      crate::iced::prepare(None)?;
    }
    let mut command = cargo_command(action, workspace, &packages);
    match action {
      "clippy" => {
        command.args([
          "--all-targets",
          "--all-features",
          "--no-deps",
          "--",
          "-D",
          "warnings",
        ]);
      }
      "test" => {
        if packages.contains("jellypilot-mpv") {
          command.args(["--features", "jellypilot-mpv/test-utils"]);
        }
        if let Some(directory) = &isolation {
          isolate_configuration(&mut command, directory.path());
        }
      }
      "check" => {}
      _ => bail!("Unsupported Rust action: {action}"),
    }
    process::run(&mut command)?;
  }
  Ok(())
}

pub fn check_all() -> Result<()> {
  let mut failed = Vec::new();
  for label in ["rust fmt --check", "rust clippy", "xtask tests"] {
    let result = match label {
      "rust fmt --check" => format(true),
      "rust clippy" => run_selected("clippy", &[]),
      _ => run_selected("test", &[Crate::Xtask]),
    };
    match result {
      Ok(()) => eprintln!("\n=== {label} [OK] ==="),
      Err(error) => {
        eprintln!("\n=== {label} [FAILED] ===\n{error:#}");
        failed.push(label);
      }
    }
    if process::interrupted() {
      bail!("Check interrupted after cleanup");
    }
  }
  if !failed.is_empty() {
    bail!("Check failed: {}", failed.join(", "));
  }
  Ok(())
}

pub fn test_all() -> Result<()> {
  run_selected("test", &[])
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn overlapping_selectors_are_deduplicated_and_partitioned() {
    let selectors = [Crate::Core, Crate::Iced, Crate::MpvHost, Crate::Iced];
    let shared = packages(Workspace::Shared, &selectors);
    let desktop = packages(Workspace::Desktop, &selectors);
    assert_eq!(shared, BTreeSet::from(["jellypilot-core"]));
    assert_eq!(desktop.len(), 4);
    assert!(!desktop.contains("jellypilot-core"));
  }

  #[test]
  fn unqualified_selection_covers_maintained_packages_not_generated_or_vendor() {
    for workspace in Workspace::ALL {
      let selected = packages(workspace, &[]);
      assert!(!selected.is_empty());
      assert!(selected
        .iter()
        .all(|name| name.starts_with("jellypilot-") || *name == "xtask"));
    }
    assert!(packages(Workspace::Shared, &[]).contains("xtask"));
  }
}
