use std::{
  collections::BTreeMap,
  fs,
  path::{Path, PathBuf},
  process::Output,
};

use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};

use crate::{process, util};

pub const LIBRARY: &str = if cfg!(windows) {
  "lib/jellypilot/libmpv-2.dll"
} else {
  "lib/jellypilot/libmpv.so"
};
pub const BASELINE: &str = "share/jellypilot/mpv-baseline.conf";
pub const NOTICES: [&str; 3] = [
  "share/jellypilot/licenses/mpv/Copyright",
  "share/jellypilot/licenses/mpv/LICENSE.GPL",
  "share/jellypilot/licenses/mpv/LICENSE.LGPL",
];

#[derive(clap::Subcommand)]
pub enum Command {
  Build {
    #[arg(long)]
    source: Option<PathBuf>,
  },
  LinuxRuntime {
    #[arg(long)]
    source: Option<PathBuf>,
  },
}

pub fn run(command: Command) -> Result<()> {
  match command {
    Command::Build { source } => build(source.as_deref(), &BTreeMap::new()),
    Command::LinuxRuntime { source } => crate::linux_mpv_runtime::build(source.as_deref()),
  }
}

pub fn artifact_root() -> PathBuf {
  crate::root().join("target/embedded-mpv")
}

pub fn source_pin() -> Result<Value> {
  serde_json::from_str(include_str!("../../tools/embedded-mpv/source.json"))
    .context("Invalid pinned mpv source manifest")
}

pub fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
  value[key]
    .as_str()
    .with_context(|| format!("Missing source manifest string {key}"))
}

pub fn strings(value: &Value, key: &str) -> Result<Vec<String>> {
  value[key]
    .as_array()
    .with_context(|| format!("Missing source manifest array {key}"))?
    .iter()
    .map(|item| {
      item
        .as_str()
        .map(str::to_owned)
        .with_context(|| format!("Invalid {key} entry"))
    })
    .collect()
}

pub fn execute(
  program: &str,
  args: &[&str],
  env: &BTreeMap<String, String>,
  capture: bool,
) -> Result<Output> {
  let mut cmd = process::command(program);
  cmd.args(args).envs(env);
  if capture {
    process::capture(&mut cmd)
  } else {
    process::run(&mut cmd)
  }
}

pub fn text(output: &Output) -> String {
  String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn verify_source(checkout: &Path, revision: &str, submodules: bool) -> Result<()> {
  let mut head = process::command("git");
  let output = process::capture(head.arg("-C").arg(checkout).args(["rev-parse", "HEAD"]))?;
  ensure!(
    text(&output).trim() == revision,
    "Checkout {} HEAD must be exactly {revision}; found {}. Explicit checkouts are never changed.",
    checkout.display(),
    text(&output).trim()
  );
  let mut status = process::command("git");
  status
    .arg("-C")
    .arg(checkout)
    .args(["status", "--porcelain", "--untracked-files=no"]);
  if submodules {
    status.arg("--ignore-submodules=none");
  }
  ensure!(text(&process::capture(&mut status)?).trim().is_empty(), "Checkout {} has tracked changes (including staged changes). Supply a clean pinned checkout; untracked build files are allowed.", checkout.display());
  Ok(())
}

pub fn build(source: Option<&Path>, environment: &BTreeMap<String, String>) -> Result<()> {
  ensure!(
    cfg!(target_os = "linux") || cfg!(windows),
    "Embedded mpv currently requires Linux or Windows with Vulkan."
  );
  let root = artifact_root();
  fs::create_dir_all(&root)?;
  // Keep build logs and provenance available after a failed build.
  let work = tempfile::Builder::new()
    .prefix("build-")
    .tempdir_in(&root)?
    .keep();
  let pin = source_pin()?;
  let revision = string(&pin, "revision")?;
  let remote = string(&pin, "remote")?;
  let checkout = source.map_or_else(|| work.join("source"), |path| crate::root().join(path));
  if source.is_none() {
    let retrieve = || -> Result<()> {
      process::run(
        process::command("git")
          .args(["clone", "--no-checkout", remote])
          .arg(&checkout),
      )?;
      process::run(process::command("git").arg("-C").arg(&checkout).args([
        "fetch",
        "--depth=1",
        "origin",
        revision,
      ]))?;
      process::run(
        process::command("git")
          .arg("-C")
          .arg(&checkout)
          .args(["checkout", "--detach", revision]),
      )?;
      Ok(())
    };
    retrieve().with_context(|| format!("Cannot retrieve pinned mpv {revision} from {remote}. If unpublished, use cargo xtask mpv build --source <clean-pinned-checkout>. No push is performed."))?;
  }
  verify_source(&checkout, revision, false)?;
  let mut tools = BTreeMap::new();
  for executable in [
    "git",
    "meson",
    "ninja",
    "pkg-config",
    "cc",
    "c++",
    if cfg!(windows) { "python" } else { "python3" },
  ] {
    let output =
      execute(executable, &["--version"], &BTreeMap::new(), true).with_context(|| {
        format!("Install {executable} and ensure it is on PATH (Meson >=1.3 required).")
      })?;
    let stdout = text(&output);
    tools.insert(
      executable,
      if stdout.trim().is_empty() {
        String::from_utf8_lossy(&output.stderr).trim().to_owned()
      } else {
        stdout.trim().to_owned()
      },
    );
  }
  let mut packages = BTreeMap::new();
  for requirement in strings(&pin, "requiredPackages")? {
    let output = execute("pkg-config", &["--modversion", &requirement], environment, true)
      .with_context(|| format!("Install development packages for {requirement}. Vulkan requires both loader and development headers; use PKG_CONFIG_PATH for a documented SDK. No temporary header fallback is used."))?;
    packages.insert(requirement, text(&output).trim().to_owned());
  }
  let build = work.join("meson");
  let mut options = strings(&pin, "mesonOptions")?;
  if cfg!(windows) {
    options.extend(strings(&pin, "windowsMesonOptions")?);
  }
  process::run(process::command("meson").arg("setup").arg(&build).arg(&checkout).args(&options).envs(environment))
    .with_context(|| format!("Meson configuration failed. See {}. Install missing development headers/libraries, including Vulkan headers; dependency downloads are disabled.", build.join("meson-logs/meson-log.txt").display()))?;
  ensure!(fs::read_to_string(build.join("config.h"))?.lines().any(|line| line == "#define HAVE_VULKAN 1"), "Meson configured mpv without Vulkan despite -Dvulkan=enabled. Install both Vulkan development headers and loader; inspect meson-logs/meson-log.txt.");
  process::run(
    process::command("meson")
      .args(["compile", "-C"])
      .arg(&build)
      .arg("mpv")
      .envs(environment),
  )?;
  verify_source(&checkout, revision, false)?;
  let baseline = if cfg!(windows) {
    fs::read(crate::root().join("tools/embedded-mpv/baseline-windows.conf"))?
  } else {
    process::capture(
      process::command("git")
        .arg("-C")
        .arg(&checkout)
        .args(["show", &format!("{revision}:{}", string(&pin, "baseline")?)]),
    )?
    .stdout
  };
  let library = root.join(LIBRARY);
  fs::create_dir_all(library.parent().context("Missing library parent")?)?;
  fs::create_dir_all(
    root
      .join(BASELINE)
      .parent()
      .context("Missing baseline parent")?,
  )?;
  fs::copy(
    build.join(library.file_name().context("Missing library name")?),
    &library,
  )?;
  fs::write(root.join(BASELINE), baseline)?;
  let mut artifacts = BTreeMap::new();
  for relative in [LIBRARY, BASELINE] {
    artifacts.insert(
      relative,
      json!({"sha256": util::sha256_file(&root.join(relative))?}),
    );
  }
  for relative in NOTICES {
    let name = Path::new(relative)
      .file_name()
      .context("Missing notice name")?
      .to_string_lossy();
    let content = process::capture(
      process::command("git")
        .arg("-C")
        .arg(&checkout)
        .args(["show", &format!("{revision}:{name}")]),
    )?
    .stdout;
    let destination = root.join(relative);
    fs::create_dir_all(destination.parent().context("Missing notice parent")?)?;
    fs::write(&destination, content)?;
    artifacts.insert(
      relative,
      json!({"sha256": util::sha256_file(&destination)?}),
    );
  }
  let mut introspection = BTreeMap::<&str, Value>::new();
  for name in ["dependencies", "compilers", "buildoptions", "machines"] {
    introspection.insert(
      name,
      util::read_json(&build.join(format!("meson-info/intro-{name}.json")))?,
    );
  }
  let reproducibility = string(&pin, "reproducibility")?.to_owned();
  util::write_json(
    &root.join("manifest.json"),
    &json!({"schemaVersion": 1, "source": pin, "platform": if cfg!(windows) { "win32" } else { "linux" }, "arch": if cfg!(target_arch = "x86_64") { "x64" } else { std::env::consts::ARCH }, "tools": tools, "packages": packages, "meson": introspection, "artifacts": artifacts}),
  )?;
  println!(
    "Embedded mpv staged at {}. {reproducibility}",
    root.display()
  );
  Ok(())
}

pub fn embedded_paths() -> BTreeMap<String, String> {
  [
    ("JELLYPILOT_LIBMPV", LIBRARY),
    ("JELLYPILOT_MPV_BASELINE", BASELINE),
  ]
  .into_iter()
  .map(|(name, relative)| {
    (
      name.to_owned(),
      std::env::var(name).unwrap_or_else(|_| {
        artifact_root()
          .join(relative)
          .to_string_lossy()
          .into_owned()
      }),
    )
  })
  .collect()
}

fn validate_paths(paths: &BTreeMap<String, String>) -> Result<()> {
  for (name, value) in paths {
    let path = Path::new(value);
    ensure!(
      path.is_absolute(),
      "{name} must be an explicit absolute file path: {value}"
    );
    let metadata = fs::metadata(path).with_context(|| format!("{name} file unavailable: {value}. Run cargo xtask mpv build --source <clean-pinned-checkout> or supply an absolute override. No system libmpv fallback is permitted."))?;
    ensure!(
      metadata.is_file(),
      "{name} must point to a regular file: {value}"
    );
  }
  Ok(())
}

pub fn embedded_environment() -> Result<BTreeMap<String, String>> {
  let paths = embedded_paths();
  validate_paths(&paths)?;
  Ok(paths)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn explicit_selection_rejects_relative_missing_and_directory_paths() {
    let temporary = tempfile::tempdir().unwrap();
    for path in [
      PathBuf::from("relative.so"),
      temporary.path().join("missing.so"),
      temporary.path().to_owned(),
    ] {
      assert!(validate_paths(&BTreeMap::from([(
        "JELLYPILOT_LIBMPV".to_owned(),
        path.to_string_lossy().into_owned()
      )]))
      .is_err());
    }
    let file = temporary.path().join("library.so");
    fs::write(&file, "library").unwrap();
    assert!(validate_paths(&BTreeMap::from([(
      "JELLYPILOT_LIBMPV".to_owned(),
      file.to_string_lossy().into_owned()
    )]))
    .is_ok());
  }
}

#[cfg(test)]
mod source_tests {
  use super::*;

  fn git(checkout: &Path, args: &[&str]) -> String {
    let output =
      process::capture(process::command("git").arg("-C").arg(checkout).args(args)).unwrap();
    text(&output).trim().to_owned()
  }

  #[test]
  fn explicit_source_verification_allows_build_files_but_never_changes_wrong_or_dirty_checkout() {
    let directory = tempfile::tempdir().unwrap();
    let checkout = directory.path();
    git(checkout, &["init", "--quiet"]);
    fs::write(checkout.join("tracked"), "original").unwrap();
    git(checkout, &["add", "tracked"]);
    git(
      checkout,
      &[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "core.hooksPath=nonexistent-fixture-hooks",
        "commit",
        "--quiet",
        "-m",
        "fixture",
      ],
    );
    let revision = git(checkout, &["rev-parse", "HEAD"]);
    fs::write(checkout.join("untracked-build-file"), "build output").unwrap();
    assert!(verify_source(checkout, &revision, false).is_ok());
    assert!(verify_source(checkout, "wrong-revision", false).is_err());
    assert_eq!(git(checkout, &["rev-parse", "HEAD"]), revision);
    fs::write(checkout.join("tracked"), "user edit").unwrap();
    assert!(verify_source(checkout, &revision, false).is_err());
    git(checkout, &["add", "tracked"]);
    assert!(verify_source(checkout, &revision, false).is_err());
    assert_eq!(
      fs::read_to_string(checkout.join("tracked")).unwrap(),
      "user edit"
    );
  }
}
