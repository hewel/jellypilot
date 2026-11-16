use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Subcommand;

use crate::process;

#[path = "iced_source.rs"]
mod source;

const MANIFEST: &str = "src-iced/Cargo.toml";
const LAUNCHER: &str = "jellypilot-launcher";

#[derive(Debug, Subcommand)]
pub enum Command {
  /// Materialize the pinned fork without modifying the optional source checkout.
  Prepare {
    #[arg(long)]
    source: Option<PathBuf>,
  },
  Build {
    #[arg(long)]
    release: bool,
  },
  Run {
    #[arg(long)]
    smoke: bool,
    #[arg(long)]
    release: bool,
    #[arg(long)]
    embedded: bool,
    #[arg(long)]
    tv: bool,
  },
  Hot,
  Danmaku {
    #[arg(long)]
    smoke: bool,
  },
  Regress(crate::regression::Args),
}

pub fn prepare(source: Option<&Path>) -> Result<()> {
  source::prepare(source)
}

fn launcher_command(action: &str, release: bool) -> std::process::Command {
  let mut command = process::cargo();
  command.args([action, "--manifest-path", MANIFEST, "--package", LAUNCHER]);
  if release {
    command.arg("--release");
  }
  command
}

pub fn build(release: bool) -> Result<()> {
  prepare(None)?;
  let mut command = launcher_command("build", release);
  command.arg("--locked");
  process::run(&mut command)?;
  Ok(())
}

pub fn run(command: Command) -> Result<()> {
  match command {
    Command::Prepare { source } => prepare(source.as_deref()),
    Command::Build { release } => build(release),
    Command::Regress(args) => crate::regression::run(args),
    Command::Run {
      smoke,
      release,
      embedded,
      tv,
    } => {
      let environment = if embedded {
        crate::mpv::embedded_environment()?
      } else {
        crate::mpv::embedded_paths()
      };
      prepare(None)?;
      let mut command = launcher_command("run", release);
      command.envs(environment);
      if smoke || embedded || tv {
        command.arg("--");
      }
      if smoke {
        command.arg("--smoke-test");
      }
      if embedded {
        command.arg("--embedded");
      }
      if tv {
        command.arg("--tv");
      }
      let result = process::run(&mut command);
      if smoke {
        if let Err(error) = &result {
          eprintln!("\n=== iced smoke [FAILED] ===");
          eprintln!("command: {command:?}\nexit: {error:#}");
          eprintln!("hint: Cargo compile output precedes app logs; app tracing writes to stderr via JELLYPILOT_LOG (default warn). Re-run with JELLYPILOT_LOG=debug cargo xtask iced run --smoke");
        }
      }
      result.map(|_| ())
    }
    Command::Hot => {
      prepare(None)?;
      let mut command = launcher_command("hot", false);
      command
        .args(["--features", "dev"])
        .envs(crate::mpv::embedded_paths());
      process::run(&mut command)?;
      Ok(())
    }
    Command::Danmaku { smoke } => {
      prepare(None)?;
      let mut command = process::cargo();
      command.args([
        "run",
        "--manifest-path",
        MANIFEST,
        "--package",
        "jellypilot-ui",
        "--example",
        "danmaku",
      ]);
      if smoke {
        command.args(["--", "--smoke"]);
      }
      process::run(&mut command)?;
      Ok(())
    }
  }
}
