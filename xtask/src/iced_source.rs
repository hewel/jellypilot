use std::fs::{self, Metadata};
use std::path::{Component, Path, PathBuf};

use anyhow::{ensure, Context, Result};
use serde::Deserialize;

use crate::{process, util};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pin {
  remote: String,
  revision: String,
}

impl Pin {
  fn validate(&self) -> Result<()> {
    ensure!(
      self.revision.len() == 40
        && self
          .revision
          .bytes()
          .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
      "iced revision must be a full lowercase commit SHA"
    );
    let remote = regex::Regex::new(r"^https://[A-Za-z0-9.-]+/[A-Za-z0-9._/-]+\.git$")?;
    ensure!(
      remote.is_match(&self.remote),
      "iced remote must be a pinned HTTPS Git URL"
    );
    Ok(())
  }
}

fn status(path: &Path) -> Result<Option<Metadata>> {
  match fs::symlink_metadata(path) {
    Ok(metadata) => Ok(Some(metadata)),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
    Err(error) => Err(error).with_context(|| format!("Inspecting {}", path.display())),
  }
}

fn is_link(metadata: &Metadata) -> bool {
  #[cfg(windows)]
  {
    use std::os::windows::fs::MetadataExt;
    // Junctions and other reparse points must never be followed as owned dirs.
    metadata.file_attributes() & 0x400 != 0
  }
  #[cfg(not(windows))]
  {
    metadata.file_type().is_symlink()
  }
}

fn ensure_directory(path: &Path) -> Result<()> {
  if let Some(metadata) = status(path)? {
    ensure!(
      metadata.is_dir() && !is_link(&metadata),
      "Refusing non-directory or symlink preparation root {}",
      path.display()
    );
  } else {
    fs::create_dir(path).with_context(|| format!("Creating {}", path.display()))?;
  }
  Ok(())
}

fn git_output(checkout: &Path, arguments: &[&str]) -> Result<String> {
  let output = process::capture(
    process::command("git")
      .arg("-C")
      .arg(checkout)
      .args(arguments),
  )?;
  Ok(
    String::from_utf8(output.stdout)
      .context("Git returned non-UTF-8 output")?
      .trim()
      .to_owned(),
  )
}

fn verify_checkout(checkout: &Path, revision: &str) -> Result<()> {
  for directory in [checkout.to_path_buf(), checkout.join(".git")] {
    let metadata = status(&directory)?
      .with_context(|| format!("Missing owned iced directory {}", directory.display()))?;
    ensure!(
      metadata.is_dir() && !is_link(&metadata),
      "Refusing non-owned iced checkout at {}",
      directory.display()
    );
  }
  ensure!(
    git_output(checkout, &["rev-parse", "HEAD"])? == revision
      && git_output(checkout, &["status", "--porcelain", "--untracked-files=no"])?.is_empty(),
    "Prepared iced source at {} must have HEAD {revision} and no tracked edits. Preserve local work and move this checkout aside before preparing again.",
    checkout.display()
  );
  Ok(())
}

fn owned_link_destination(vendor: &Path, destination: &Path) -> Result<PathBuf> {
  let destination = if destination.is_absolute() {
    destination.to_path_buf()
  } else {
    vendor.join(destination)
  };
  let mut normalized = PathBuf::new();
  for component in destination.components() {
    match component {
      Component::CurDir => {}
      Component::ParentDir => {
        normalized.pop();
      }
      component => normalized.push(component.as_os_str()),
    }
  }
  let name = normalized
    .file_name()
    .and_then(|name| name.to_str())
    .unwrap_or_default();
  let digest = name.strip_prefix("iced-").unwrap_or_default();
  ensure!(
    normalized.parent() == Some(vendor)
      && (digest.len() == 40 || digest.len() == 64)
      && digest
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
    "Refusing iced link outside the owned vendor checkout namespace: {}",
    destination.display()
  );
  Ok(normalized)
}

fn verify_link(vendor: &Path, link: &Path) -> Result<Option<PathBuf>> {
  let Some(metadata) = status(link)? else {
    return Ok(None);
  };
  ensure!(
    is_link(&metadata),
    "Refusing to replace non-symlink {}. Move it aside without deleting its contents.",
    link.display()
  );
  let target = fs::read_link(link)?;
  let destination = owned_link_destination(vendor, dunce::simplified(&target))?;
  if let Some(metadata) = status(&destination)? {
    ensure!(
      metadata.is_dir() && !is_link(&metadata),
      "Refusing indirect or non-directory iced link target {}",
      destination.display()
    );
  }
  Ok(Some(destination))
}

fn create_link(checkout: &Path, staged: &Path) -> Result<()> {
  #[cfg(unix)]
  {
    std::os::unix::fs::symlink(checkout, staged)?;
  }
  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    // Junctions do not require Developer Mode or symlink creation privileges.
    // Reject cmd metacharacters rather than interpolate untrusted path syntax.
    for path in [checkout, staged] {
      let text = path.to_str().context("Junction paths must be Unicode")?;
      ensure!(
        !text.contains(['"', '%', '\r', '\n']),
        "Unsupported character in junction path {}",
        path.display()
      );
    }
    let mut command = process::command("cmd");
    command.args(["/d", "/v:off", "/c"]).raw_arg(format!(
      "mklink /J \"{}\" \"{}\"",
      staged.display(),
      checkout.display()
    ));
    process::run(&mut command)?;
  }
  #[cfg(not(any(unix, windows)))]
  anyhow::bail!("iced source links are unsupported on this platform");
  Ok(())
}

pub fn prepare(source: Option<&Path>) -> Result<()> {
  // Git for Windows and cmd's mklink reject verbatim paths on ordinary drive checkouts.
  let root = dunce::canonicalize(crate::root()).context("Resolving repository root")?;
  prepare_in(&root, source)
}

fn prepare_in(root: &Path, source: Option<&Path>) -> Result<()> {
  let pin: Pin = util::read_json(&root.join("tools/embedded-mpv/iced-source.json"))?;
  pin.validate()?;
  let vendor = root.join("target/vendor");
  for directory in [root.join("target"), vendor.clone()] {
    ensure_directory(&directory)?;
  }
  let checkout = vendor.join(format!("iced-{}", pin.revision));
  let link = vendor.join("iced");
  verify_link(&vendor, &link)?;
  if status(&checkout)?.is_none() {
    let explicit = source
      .map(|source| dunce::canonicalize(root.join(source)))
      .transpose()
      .context("Resolving explicit iced source")?;
    let origin = if let Some(source) = &explicit {
      ensure!(git_output(source, &["rev-parse", "HEAD"])? == pin.revision,
        "Explicit iced source HEAD must be {}. Only committed contents are fetched; the source checkout is never modified.", pin.revision);
      source.as_os_str()
    } else {
      std::ffi::OsStr::new(&pin.remote)
    };
    let work = tempfile::Builder::new()
      .prefix(".iced-")
      .tempdir_in(&vendor)?;
    let staged = work.path().join("checkout");
    process::run(process::command("git").arg("init").arg(&staged))?;
    process::run(process::command("git").arg("-C").arg(&staged).args(["fetch", "--depth=1", "--no-tags"]).arg(origin).arg(&pin.revision))
      .with_context(|| format!("Cannot fetch pinned iced revision {}. No substitute revision or source-tree edits are used.", pin.revision))?;
    process::run(process::command("git").arg("-C").arg(&staged).args([
      "checkout",
      "--detach",
      &pin.revision,
    ]))?;
    verify_checkout(&staged, &pin.revision)?;
    if status(&checkout)?.is_some() {
      // A concurrent preparation may have published this revision first.
      verify_checkout(&checkout, &pin.revision)?;
    } else {
      fs::rename(&staged, &checkout).context("Publishing pinned iced checkout")?;
    }
  }
  verify_checkout(&checkout, &pin.revision)?;
  if verify_link(&vendor, &link)?.as_deref() == Some(checkout.as_path()) {
    return Ok(());
  }
  let work = tempfile::Builder::new()
    .prefix(".iced-link-")
    .tempdir_in(&vendor)?;
  let staged_link = work.path().join("iced");
  create_link(&checkout, &staged_link)?;
  verify_link(&vendor, &link)?;
  // Publish only a link, never replace or remove a source directory. If the
  // platform cannot atomically replace its old junction, preserve it and fail.
  fs::rename(&staged_link, &link).with_context(|| format!(
    "Atomically publishing iced source link {} (existing source preserved). If Windows refuses junction replacement, move only this owned stable junction aside, preserve all checkout directories, then rerun cargo xtask iced prepare.",
    link.display()
  ))?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn rejects_unpinned_or_non_https_sources() {
    for (remote, revision) in [
      ("https://github.com/hewel/iced.git", "main"),
      (
        "https://github.com/hewel/iced.git",
        "ABCDEF0123456789012345678901234567890123",
      ),
      (
        "file:///tmp/iced.git",
        "7219acf75118cf11b2d6866309d154d56f483dff",
      ),
      (
        "https://host/iced.git?branch=main",
        "7219acf75118cf11b2d6866309d154d56f483dff",
      ),
    ] {
      assert!(Pin {
        remote: remote.into(),
        revision: revision.into()
      }
      .validate()
      .is_err());
    }
  }

  #[test]
  fn refuses_directory_and_foreign_link_takeovers() {
    let temporary = tempfile::tempdir().unwrap();
    let vendor = temporary.path();
    let link = vendor.join("iced");
    fs::create_dir(&link).unwrap();
    fs::write(link.join("user-source"), "preserve").unwrap();
    assert!(verify_link(vendor, &link).is_err());
    assert_eq!(
      fs::read_to_string(link.join("user-source")).unwrap(),
      "preserve"
    );
    assert!(owned_link_destination(vendor, Path::new("../user-source")).is_err());
    assert!(owned_link_destination(vendor, &vendor.join("iced-main")).is_err());
  }

  #[cfg(unix)]
  #[test]
  fn rejects_owned_names_that_redirect_to_user_sources() {
    let temporary = tempfile::tempdir().unwrap();
    let vendor = temporary.path();
    let source = vendor.join("user-source");
    fs::create_dir(&source).unwrap();
    let checkout = vendor.join(format!("iced-{}", "a".repeat(40)));
    std::os::unix::fs::symlink(&source, &checkout).unwrap();
    let link = vendor.join("iced");
    std::os::unix::fs::symlink(&checkout, &link).unwrap();
    assert!(verify_link(vendor, &link).is_err());
    assert!(ensure_directory(&checkout).is_err());
  }

  #[test]
  fn rejects_unknown_pin_fields() {
    assert!(serde_json::from_value::<Pin>(serde_json::json!({
      "remote": "https://github.com/hewel/iced.git",
      "revision": "7219acf75118cf11b2d6866309d154d56f483dff",
      "branch": "main"
    }))
    .is_err());
  }

  #[test]
  fn preparation_fetches_only_committed_contents_and_never_resets_user_source() {
    let temporary = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(temporary.path()).unwrap();
    let source = root.join("user-source");
    process::capture(process::command("git").arg("init").arg(&source)).unwrap();
    fs::write(source.join("tracked"), "committed").unwrap();
    process::capture(
      process::command("git")
        .arg("-C")
        .arg(&source)
        .args(["add", "tracked"]),
    )
    .unwrap();
    process::capture(process::command("git").arg("-C").arg(&source).args([
      "-c",
      "user.name=xtask-test",
      "-c",
      "user.email=xtask@example.invalid",
      "-c",
      "commit.gpgsign=false",
      "commit",
      "-m",
      "fixture",
    ]))
    .unwrap();
    let revision = git_output(&source, &["rev-parse", "HEAD"]).unwrap();
    fs::write(source.join("tracked"), "user edit").unwrap();
    fs::write(source.join("untracked"), "user work").unwrap();
    fs::create_dir_all(root.join("tools/embedded-mpv")).unwrap();
    util::write_json(
      &root.join("tools/embedded-mpv/iced-source.json"),
      &serde_json::json!({
        "remote": "https://github.com/hewel/iced.git",
        "revision": revision
      }),
    )
    .unwrap();
    prepare_in(&root, Some(&source)).unwrap();
    assert_eq!(
      fs::read_to_string(source.join("tracked")).unwrap(),
      "user edit"
    );
    assert_eq!(
      fs::read_to_string(source.join("untracked")).unwrap(),
      "user work"
    );
    let checkout = root.join("target/vendor/iced");
    assert_eq!(
      fs::read_to_string(checkout.join("tracked")).unwrap(),
      "committed"
    );
    assert!(!checkout.join("untracked").exists());
    // A developer edit in the managed checkout is rejected, not reset.
    fs::write(checkout.join("tracked"), "managed edit").unwrap();
    assert!(prepare_in(&root, Some(&source)).is_err());
    assert_eq!(
      fs::read_to_string(checkout.join("tracked")).unwrap(),
      "managed edit"
    );
  }
}
