use anyhow::{bail, Context, Result};
use clap::Subcommand;

use crate::process::{self, capture, command, run_with, RunOptions};

#[derive(Subcommand)]
pub enum Command {
  /// Activate .githooks locally; refuses to replace unrelated custom hooks.
  Install,
}

pub fn run(task: Command) -> Result<()> {
  match task {
    Command::Install => install(),
  }
}

fn install() -> Result<()> {
  capture(command("git").args(["rev-parse", "--show-toplevel"]))
    .context("Hook installation requires a Git checkout")?;
  let setting = run_with(
    command("git").args(["config", "--get", "core.hooksPath"]),
    RunOptions {
      capture: true,
      allow_failure: true,
      ..RunOptions::default()
    },
  )?;
  if !setting.status.success() && setting.status.code() != Some(1) {
    bail!(
      "Cannot read core.hooksPath: {}",
      String::from_utf8_lossy(&setting.stderr)
    );
  }
  let previous = String::from_utf8(setting.stdout)?;
  let previous = previous.trim();
  if !previous.is_empty() && !matches!(previous, ".githooks" | ".husky/_" | ".husky") {
    bail!("Refusing to replace custom core.hooksPath {previous:?}; configure .githooks explicitly after preserving your hooks.");
  }
  if previous.is_empty() {
    let directory = capture(command("git").args(["rev-parse", "--git-path", "hooks"]))?;
    let directory = String::from_utf8(directory.stdout)?;
    ensure_no_active_hooks(&crate::root().join(directory.trim()))?;
  }
  for name in ["pre-commit", "pre-push"] {
    let path = crate::root().join(".githooks").join(name);
    let metadata =
      std::fs::metadata(&path).with_context(|| format!("Reading hook {}", path.display()))?;
    if !metadata.is_file() {
      bail!("Hook {} is not a regular file", path.display());
    }
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      let mut permissions = metadata.permissions();
      permissions.set_mode(permissions.mode() | 0o111);
      std::fs::set_permissions(&path, permissions)?;
    }
  }
  process::run(command("git").args(["config", "--local", "core.hooksPath", ".githooks"]))?;
  println!("Installed repository hooks. Hooks check files without modifying the worktree.");
  Ok(())
}

fn ensure_no_active_hooks(directory: &std::path::Path) -> Result<()> {
  let entries = match std::fs::read_dir(directory) {
    Ok(entries) => entries,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
    Err(error) => return Err(error).context("Inspecting existing Git hooks"),
  };
  for entry in entries {
    let path = entry?.path();
    if path
      .extension()
      .is_some_and(|extension| extension == "sample")
    {
      continue;
    }
    let metadata = std::fs::metadata(&path)?;
    if !metadata.is_file() {
      continue;
    }
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      if metadata.permissions().mode() & 0o111 == 0 {
        continue;
      }
    }
    bail!(
      "Refusing to disable existing hook {}; preserve or migrate it before installing .githooks.",
      path.display()
    );
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn refuses_existing_active_hooks_without_modifying_them() {
    let directory = tempfile::tempdir().unwrap();
    let hook = directory.path().join("pre-commit");
    std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert!(ensure_no_active_hooks(directory.path()).is_err());
    assert_eq!(
      std::fs::read_to_string(hook).unwrap(),
      "#!/bin/sh\nexit 1\n"
    );
  }

  #[test]
  fn git_sample_hooks_do_not_prevent_installation() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("pre-commit.sample"), "#!/bin/sh\n").unwrap();
    assert!(ensure_no_active_hooks(directory.path()).is_ok());
  }
}
