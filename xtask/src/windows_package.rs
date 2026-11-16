use std::{
  cmp::Ordering,
  fs,
  path::{Path, PathBuf},
};

use anyhow::{ensure, Context, Result};
use regex::Regex;
use serde_json::{json, Value};

use crate::{mpv, process, util, windows_dependencies};

#[derive(clap::Subcommand)]
pub enum Command {
  Windows {
    #[arg(long)]
    runtime_dir: PathBuf,
  },
}

pub fn run(command: Command) -> Result<()> {
  match command {
    Command::Windows { runtime_dir } => package(&runtime_dir),
  }
}

fn validate_manifest(root: &Path, baseline: &Path, manifest: &Value, revision: &str) -> Result<()> {
  ensure!(
    manifest["schemaVersion"] == 1
      && manifest["platform"] == "win32"
      && manifest["arch"] == "x64"
      && manifest["source"]["revision"] == revision,
    "Rebuild Windows mpv: manifest platform, architecture or pinned revision is invalid."
  );
  let artifacts = manifest["artifacts"]
    .as_object()
    .context("Rebuild Windows mpv: invalid artifact records")?;
  ensure!(
    artifacts
      .values()
      .all(|record| record["sha256"].is_string()),
    "Rebuild Windows mpv: invalid artifact hash record"
  );
  for relative in ["lib/jellypilot/libmpv-2.dll", mpv::BASELINE]
    .into_iter()
    .chain(mpv::NOTICES)
  {
    ensure!(
      manifest["artifacts"][relative]["sha256"].as_str()
        == Some(util::sha256_file(&root.join(relative))?.as_str()),
      "Staged {relative} does not match the mpv manifest. Run cargo xtask mpv build."
    );
    if relative == mpv::BASELINE {
      ensure!(
        fs::read(root.join(relative))? == fs::read(baseline)?,
        "Windows baseline changed; rebuild mpv before packaging."
      );
    }
  }
  Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
  fs::create_dir(destination).with_context(|| {
    format!(
      "Cannot create fresh license destination {}",
      destination.display()
    )
  })?;
  for entry in fs::read_dir(source)
    .with_context(|| format!("Cannot read required MSYS2 licenses {}", source.display()))?
  {
    let entry = entry?;
    let target = destination.join(entry.file_name());
    let kind = entry.file_type()?;
    if kind.is_dir() {
      copy_tree(&entry.path(), &target)?;
    } else if kind.is_file() {
      fs::copy(entry.path(), target)?;
    } else if kind.is_symlink() {
      // MSYS2 license trees can contain links to shared package notices.
      if entry.path().is_dir() {
        copy_tree(&fs::canonicalize(entry.path())?, &target)?;
      } else {
        fs::copy(entry.path(), target)?;
      }
    } else {
      anyhow::bail!("Unsupported license entry {}", entry.path().display());
    }
  }
  Ok(())
}

// vswhere can return several installed toolsets; compare numeric components, not strings.
fn natural_compare(left: &str, right: &str) -> Ordering {
  let mut a = left.as_bytes();
  let mut b = right.as_bytes();
  loop {
    match (a.first(), b.first()) {
      (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
        let a_end = a
          .iter()
          .position(|byte| !byte.is_ascii_digit())
          .unwrap_or(a.len());
        let b_end = b
          .iter()
          .position(|byte| !byte.is_ascii_digit())
          .unwrap_or(b.len());
        let x = &a[..a_end];
        let y = &b[..b_end];
        let x = &x[x.iter().position(|&byte| byte != b'0').unwrap_or(x.len())..];
        let y = &y[y.iter().position(|&byte| byte != b'0').unwrap_or(y.len())..];
        let order = x.len().cmp(&y.len()).then_with(|| x.cmp(y));
        if order != Ordering::Equal {
          return order;
        }
        a = &a[a_end..];
        b = &b[b_end..];
      }
      (Some(x), Some(y)) => {
        let order = x.to_ascii_lowercase().cmp(&y.to_ascii_lowercase());
        if order != Ordering::Equal {
          return order;
        }
        a = &a[1..];
        b = &b[1..];
      }
      (None, None) => return Ordering::Equal,
      (None, Some(_)) => return Ordering::Less,
      (Some(_), None) => return Ordering::Greater,
    }
  }
}

fn package_config(
  base: Value,
  work: &Path,
  launcher: &windows_dependencies::DependencyManifest,
) -> Result<Value> {
  let Value::Object(mut config) = base else {
    anyhow::bail!("Packager.toml must contain a configuration table");
  };
  // Hashes are recorded after building, so the shared hook must not rebuild the launcher.
  config.insert("before-packaging-command".to_owned(), Value::Null);
  let mut resources = vec![
    json!({"src": work.join("lib/jellypilot"), "target": "lib/jellypilot"}),
    json!({"src": work.join("share/jellypilot"), "target": "share/jellypilot"}),
  ];
  resources.extend(
    launcher
      .bundled
      .iter()
      .filter(|item| item.name != "jellypilot.exe")
      .map(|item| json!({"src": work.join("launcher").join(&item.name), "target": item.name})),
  );
  config.insert("resources".to_owned(), resources.into());
  config.insert(
    "out-dir".to_owned(),
    json!(crate::root().join("target/release/bundle")),
  );
  Ok(Value::Object(config))
}

fn package(runtime_directory: &Path) -> Result<()> {
  let system_root = std::env::var_os("SystemRoot").or_else(|| std::env::var_os("SYSTEMROOT"));
  ensure!(
    cfg!(windows) && cfg!(target_arch = "x86_64") && system_root.is_some(),
    "Windows packaging requires native x64 Windows."
  );
  let system = PathBuf::from(system_root.context("Missing Windows SystemRoot")?).join("System32");
  let tool_root = crate::root().join("target/tools/cargo-packager");
  let local_packager = tool_root.join("bin/cargo-packager.exe");
  let mut packager = local_packager.clone();
  let mut ready = false;
  let expected_version = Regex::new(r"\b0\.11\.8\b")?;
  for candidate in [PathBuf::from("cargo-packager"), local_packager] {
    if let Ok(output) = process::capture(process::command(&candidate).arg("--version")) {
      if expected_version.is_match(&mpv::text(&output)) {
        packager = candidate;
        ready = true;
        break;
      }
    }
  }
  if !ready {
    process::run(
      process::cargo()
        .args([
          "install",
          "cargo-packager",
          "--version",
          "0.11.8",
          "--locked",
          "--root",
        ])
        .arg(&tool_root),
    )?;
  }
  let artifact_root = mpv::artifact_root();
  let manifest_text = fs::read_to_string(artifact_root.join("manifest.json"))?;
  let manifest: Value = serde_json::from_str(&manifest_text)?;
  validate_manifest(
    &artifact_root,
    &crate::root().join("tools/embedded-mpv/baseline-windows.conf"),
    &manifest,
    mpv::string(&mpv::source_pin()?, "revision")?,
  )?;
  let parent = crate::root().join("target/windows-package");
  fs::create_dir_all(&parent)?;
  let work = tempfile::Builder::new()
    .prefix("build-")
    .tempdir_in(parent)?
    .keep();
  let runtime = crate::root().join(runtime_directory);
  let payload = work.join("lib/jellypilot");
  fs::create_dir_all(&payload)?;
  let dependencies = windows_dependencies::stage(
    &artifact_root.join("lib/jellypilot/libmpv-2.dll"),
    &runtime,
    &payload,
    &system,
  )?;
  let vswhere = PathBuf::from(
    std::env::var_os("ProgramFiles(x86)").unwrap_or_else(|| "C:/Program Files (x86)".into()),
  )
  .join("Microsoft Visual Studio/Installer/vswhere.exe");
  let redist = process::capture(process::command(vswhere).args([
    "-latest",
    "-products",
    "*",
    "-requires",
    "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
    "-find",
    "VC/Redist/MSVC/**/x64/Microsoft.VC*.CRT/vcruntime140.dll",
  ]))?;
  let redist_text = mpv::text(&redist);
  let vc_runtime = redist_text
    .lines()
    .map(str::trim)
    .filter(|line| !line.is_empty())
    .max_by(|a, b| natural_compare(a, b))
    .context("Install the Visual Studio x64 C++ redistributable build files.")?;
  crate::iced::prepare(None)?;
  crate::iced::build(true)?;
  let launcher_dir = work.join("launcher");
  fs::create_dir(&launcher_dir)?;
  let launcher = windows_dependencies::stage_with(
    &crate::root().join("target/release/jellypilot.exe"),
    Path::new(vc_runtime)
      .parent()
      .context("VC runtime has no parent")?,
    &launcher_dir,
    &system,
    |file| windows_dependencies::read_pe_dump(&runtime, file),
  )?;
  let share = work.join("share/jellypilot");
  fs::create_dir_all(&share)?;
  for relative in [mpv::BASELINE].into_iter().chain(mpv::NOTICES) {
    let target = work.join(relative);
    fs::create_dir_all(target.parent().context("Missing resource parent")?)?;
    fs::copy(artifact_root.join(relative), target)?;
  }
  copy_tree(
    &runtime.join("../share/licenses"),
    &share.join("licenses/msys2"),
  )?;
  fs::write(share.join("mpv-manifest.json"), manifest_text)?;
  util::write_json(
    &share.join("windows-runtime.json"),
    &json!({"schemaVersion": 1, "bundled": dependencies.bundled, "systemDependencies": dependencies.system_dependencies, "launcher": launcher}),
  )?;
  let base: toml::Value =
    toml::from_str(&fs::read_to_string(crate::root().join("Packager.toml"))?)?;
  let config = package_config(serde_json::to_value(base)?, &work, &launcher)?;
  util::write_json(&work.join("Packager.json"), &config)?;
  // Inline JSON avoids cargo-packager 0.11.8 changing cwd and breaking root-relative assets.
  process::run(process::command(packager).args([
    "--config",
    &serde_json::to_string(&config)?,
    "--formats",
    "nsis",
  ]))?;
  println!(
    "Windows NSIS package built with embedded resources from {}.",
    work.display()
  );
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn manifest_hashes_and_current_baseline_are_required() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let mut manifest = json!({"schemaVersion": 1, "platform": "win32", "arch": "x64", "source": {"revision": "pinned"}, "artifacts": {}});
    for relative in ["lib/jellypilot/libmpv-2.dll", mpv::BASELINE]
      .into_iter()
      .chain(mpv::NOTICES)
    {
      fs::create_dir_all(root.join(relative).parent().unwrap()).unwrap();
      fs::write(root.join(relative), relative).unwrap();
      manifest["artifacts"][relative] =
        json!({"sha256": util::sha256_file(&root.join(relative)).unwrap()});
    }
    let baseline = root.join("current-baseline.conf");
    fs::copy(root.join(mpv::BASELINE), &baseline).unwrap();
    assert!(validate_manifest(root, &baseline, &manifest, "pinned").is_ok());
    fs::write(&baseline, "changed").unwrap();
    assert!(validate_manifest(root, &baseline, &manifest, "pinned").is_err());
    fs::copy(root.join(mpv::BASELINE), &baseline).unwrap();
    fs::write(root.join("lib/jellypilot/libmpv-2.dll"), "tampered").unwrap();
    assert!(validate_manifest(root, &baseline, &manifest, "pinned").is_err());
  }

  #[test]
  fn package_config_preserves_platform_settings_and_disables_second_build() {
    let base = json!({"name": "test", "before-packaging-command": "build", "nsis": {"installMode": "currentUser"}, "icons": ["icon.ico"]});
    let launcher = windows_dependencies::DependencyManifest {
      bundled: vec![
        windows_dependencies::BundledLibrary {
          name: "jellypilot.exe".into(),
          sha256: "a".into(),
        },
        windows_dependencies::BundledLibrary {
          name: "vcruntime140.dll".into(),
          sha256: "b".into(),
        },
      ],
      system_dependencies: vec![],
    };
    let config = package_config(base, Path::new("payload"), &launcher).unwrap();
    assert!(config["before-packaging-command"].is_null());
    assert_eq!(config["nsis"]["installMode"], "currentUser");
    assert_eq!(config["resources"].as_array().unwrap().len(), 3);
    assert_eq!(config["resources"][2]["target"], "vcruntime140.dll");
  }

  #[test]
  fn redistributable_selection_compares_numeric_toolset_versions() {
    assert_eq!(
      natural_compare("MSVC/14.9/x64", "MSVC/14.10/x64"),
      Ordering::Less
    );
  }
}
