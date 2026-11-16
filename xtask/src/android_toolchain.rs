use std::cmp::Ordering;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};

pub const NDK_VERSION: &str = "28.2.13676358";
pub const BUILD_TOOLS_VERSION: &str = "36.0.0";
pub const MIN_API: u32 = 26;
pub const COMPILE_SDK: u32 = 37;
pub const RUST_TARGET: &str = "aarch64-linux-android";
pub const ABI: &str = "arm64-v8a";

#[derive(Debug)]
pub struct Toolchain {
  pub sdk_root: PathBuf,
  pub ndk_root: PathBuf,
  pub llvm_bin_dir: PathBuf,
  pub sysroot: PathBuf,
  pub host_tag: &'static str,
  pub clang_triple: String,
  pub cmake_bin_dir: PathBuf,
}

pub fn android_root() -> PathBuf {
  crate::root().join("android")
}

pub fn artifact_root() -> PathBuf {
  crate::root().join("target/android/mpv").join(ABI)
}

pub fn bindings_dir() -> PathBuf {
  android_root().join("core-bridge/build/generated/uniffi")
}

pub fn jni_libs_dir() -> PathBuf {
  android_root().join("core-bridge/build/jniLibs").join(ABI)
}

pub fn sdk_root() -> Result<PathBuf> {
  if let Some(root) = env::var_os("ANDROID_HOME").or_else(|| env::var_os("ANDROID_SDK_ROOT")) {
    return Ok(PathBuf::from(root));
  }
  let home = env::var_os("HOME")
    .or_else(|| env::var_os("USERPROFILE"))
    .context("Set ANDROID_HOME or ANDROID_SDK_ROOT; no home directory is available")?;
  Ok(PathBuf::from(home).join("Android/Sdk"))
}

pub fn require_directory(path: &Path, hint: &str) -> Result<()> {
  ensure!(
    path.is_dir(),
    "Directory missing at {}. {hint}",
    path.display()
  );
  Ok(())
}

pub fn require_file(path: &Path, hint: &str) -> Result<()> {
  ensure!(path.is_file(), "File missing at {}. {hint}", path.display());
  Ok(())
}

pub fn executable(directory: &Path, name: &str) -> PathBuf {
  directory.join(if cfg!(windows) {
    format!("{name}.exe")
  } else {
    name.to_owned()
  })
}

fn executable_on_path(name: &str) -> Option<PathBuf> {
  env::var_os("PATH").and_then(|path| {
    env::split_paths(&path)
      .map(|directory| executable(&directory, name))
      .find(|path| path.is_file())
  })
}

// Numeric ordering must prefer 3.31 over 3.9, including SDK preview suffixes.
fn version_cmp(left: &str, right: &str) -> Ordering {
  fn tokens(value: &str) -> impl Iterator<Item = u64> + '_ {
    value
      .split(|c: char| !c.is_ascii_digit())
      .filter_map(|part| part.parse::<u64>().ok())
  }
  tokens(left)
    .cmp(tokens(right))
    .then_with(|| left.cmp(right))
}

fn resolve_cmake(sdk: &Path) -> Result<PathBuf> {
  let mut versions = fs::read_dir(sdk.join("cmake"))
    .into_iter()
    .flatten()
    .filter_map(|entry| entry.ok())
    .filter(|entry| entry.path().is_dir())
    .map(|entry| entry.file_name().to_string_lossy().into_owned())
    .collect::<Vec<_>>();
  versions.sort_by(|left, right| version_cmp(right, left));
  for version in versions {
    let bin = sdk.join("cmake").join(version).join("bin");
    if executable(&bin, "cmake").is_file() && executable(&bin, "ninja").is_file() {
      return Ok(bin);
    }
  }
  if let Some(cmake) = executable_on_path("cmake") {
    if let Some(bin) = cmake.parent() {
      if executable(bin, "ninja").is_file() {
        return Ok(bin.to_path_buf());
      }
    }
  }
  bail!("No usable cmake+ninja found. Install with sdkmanager \"cmake;3.22.1\" or put cmake and ninja in the same directory on PATH.")
}

fn ndk_revision(properties: &str) -> Option<&str> {
  properties.lines().find_map(|line| {
    let (key, value) = line.split_once('=')?;
    (key.trim() == "Pkg.Revision").then(|| value.trim())
  })
}

fn host_tag(prebuilt: &Path) -> Result<&'static str> {
  match (env::consts::OS, env::consts::ARCH) {
    ("linux", "x86_64") => Ok("linux-x86_64"),
    ("macos", "aarch64") => {
      // NDK releases may ship an x86_64 directory containing universal macOS binaries.
      Ok(if prebuilt.join("darwin-arm64").is_dir() {
        "darwin-arm64"
      } else {
        "darwin-x86_64"
      })
    }
    ("macos", "x86_64") => Ok("darwin-x86_64"),
    ("windows", "x86_64") => Ok("windows-x86_64"),
    (os, arch) => bail!("Unsupported Android NDK host platform {os}/{arch}."),
  }
}

pub fn resolve() -> Result<Toolchain> {
  let sdk_root = sdk_root()?;
  require_directory(
    &sdk_root,
    "Set ANDROID_HOME or ANDROID_SDK_ROOT to an installed SDK, or install one at ~/Android/Sdk.",
  )?;
  let ndk_root = env::var_os("ANDROID_NDK_HOME")
    .or_else(|| env::var_os("ANDROID_NDK_ROOT"))
    .map_or_else(|| sdk_root.join("ndk").join(NDK_VERSION), PathBuf::from);
  require_directory(
    &ndk_root,
    &format!("Install with sdkmanager \"ndk;{NDK_VERSION}\" or set ANDROID_NDK_HOME."),
  )?;
  let properties = fs::read_to_string(ndk_root.join("source.properties")).unwrap_or_default();
  ensure!(ndk_revision(&properties) == Some(NDK_VERSION),
    "Expected NDK {NDK_VERSION} at {}, found {}. Select the pinned installation with ANDROID_NDK_HOME.",
    ndk_root.display(), ndk_revision(&properties).unwrap_or("no Pkg.Revision"));
  let prebuilt = ndk_root.join("toolchains/llvm/prebuilt");
  let host_tag = host_tag(&prebuilt)?;
  let llvm_bin_dir = prebuilt.join(host_tag).join("bin");
  let sysroot = prebuilt.join(host_tag).join("sysroot");
  for path in [&llvm_bin_dir, &sysroot] {
    require_directory(
      path,
      &format!("The NDK is incomplete; reinstall \"ndk;{NDK_VERSION}\"."),
    )?;
  }
  let cmake_bin_dir = resolve_cmake(&sdk_root)?;
  Ok(Toolchain {
    sdk_root,
    ndk_root,
    llvm_bin_dir,
    sysroot,
    host_tag,
    clang_triple: format!("{RUST_TARGET}{MIN_API}"),
    cmake_bin_dir,
  })
}

pub fn platform(sdk: &Path) -> Result<PathBuf> {
  let prefix = format!("android-{COMPILE_SDK}");
  let platform = fs::read_dir(sdk.join("platforms"))
    .into_iter()
    .flatten()
    .filter_map(|entry| entry.ok())
    .find(|entry| {
      let name = entry.file_name();
      let name = name.to_string_lossy();
      entry.path().is_dir()
        && (name == prefix
          || name
            .strip_prefix(&prefix)
            .is_some_and(|suffix| suffix.starts_with('.')))
    })
    .map(|entry| entry.path());
  platform.with_context(|| {
    format!(
      "No platforms/{prefix}* under {}. Install with sdkmanager \"platforms;{prefix}.0\".",
      sdk.display()
    )
  })
}

pub fn gradlew() -> PathBuf {
  android_root().join(if cfg!(windows) {
    "gradlew.bat"
  } else {
    "gradlew"
  })
}

pub fn java() -> Result<OsString> {
  if let Some(home) = env::var_os("JAVA_HOME") {
    let java = executable(&PathBuf::from(home).join("bin"), "java");
    require_file(&java, "JAVA_HOME must point to an installed JDK.")?;
    return Ok(java.into_os_string());
  }
  Ok(OsString::from("java"))
}

pub fn resolve_build() -> Result<Toolchain> {
  let toolchain = resolve()?;
  platform(&toolchain.sdk_root)?;
  require_directory(
    &toolchain
      .sdk_root
      .join("build-tools")
      .join(BUILD_TOOLS_VERSION),
    &format!("Install with sdkmanager \"build-tools;{BUILD_TOOLS_VERSION}\"."),
  )?;
  require_file(
    &gradlew(),
    "The pinned Gradle wrapper must be checked in under android/.",
  )?;
  crate::process::capture(crate::process::command(java()?).arg("-version"))
    .context("Install a JDK on PATH or set JAVA_HOME.")?;
  Ok(toolchain)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn ndk_revision_ignores_other_properties_and_whitespace() {
    assert_eq!(
      ndk_revision("Pkg.Desc = Android NDK\n Pkg.Revision = 28.2.13676358\r\n"),
      Some(NDK_VERSION)
    );
  }

  #[test]
  fn sdk_versions_are_sorted_numerically() {
    let mut versions = ["3.9.1", "3.31.0", "3.22.1"];
    versions.sort_by(|left, right| version_cmp(right, left));
    assert_eq!(versions, ["3.31.0", "3.22.1", "3.9.1"]);
  }

  #[test]
  fn platform_rejects_next_major_prefix_collision() -> Result<()> {
    let sdk = tempfile::tempdir()?;
    fs::create_dir_all(sdk.path().join("platforms/android-370"))?;
    assert!(platform(sdk.path()).is_err());
    fs::create_dir_all(sdk.path().join("platforms/android-37.0"))?;
    assert_eq!(
      platform(sdk.path())?,
      sdk.path().join("platforms/android-37.0")
    );
    Ok(())
  }
}
