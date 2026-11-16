use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use clap::Subcommand;

use crate::android_toolchain::{
  self as toolchain, BUILD_TOOLS_VERSION, COMPILE_SDK, NDK_VERSION, RUST_TARGET,
};
use crate::process;

const FFI_PACKAGE: &str = "jellypilot-ffi";
const FFI_LIBRARY: &str = "libjellypilot_ffi.so";

#[derive(Debug, Subcommand)]
pub enum Command {
  Doctor,
  Bindings,
  Rust {
    #[arg(long)]
    release: bool,
  },
  Mpv,
  Build {
    #[arg(long)]
    release: bool,
  },
  Check,
}

pub fn run(command: Command) -> Result<()> {
  match command {
    Command::Doctor => doctor(),
    Command::Bindings => bindings(),
    Command::Rust { release } => rust(release),
    Command::Mpv => mpv(),
    Command::Build { release } => build(release),
    Command::Check => check(),
  }
}

fn target_dir() -> PathBuf {
  std::env::var_os("CARGO_TARGET_DIR").map_or_else(
    || crate::root().join("target"),
    |path| crate::root().join(path),
  )
}

fn kotlin_files(directory: &Path) -> Result<usize> {
  let mut count = 0;
  for entry in fs::read_dir(directory)
    .with_context(|| format!("Reading bindings under {}", directory.display()))?
  {
    let entry = entry?;
    if entry.file_type()?.is_dir() {
      count += kotlin_files(&entry.path())?;
    } else if entry.file_type()?.is_file()
      && entry
        .path()
        .extension()
        .is_some_and(|extension| extension == "kt")
    {
      count += 1;
    }
  }
  Ok(count)
}

fn bindings() -> Result<()> {
  process::run(process::cargo().args([
    "build",
    "--locked",
    "--manifest-path",
    "Cargo.toml",
    "--package",
    FFI_PACKAGE,
  ]))?;
  let library = target_dir()
    .join("debug")
    .join(if cfg!(target_os = "macos") {
      "libjellypilot_ffi.dylib"
    } else if cfg!(windows) {
      "jellypilot_ffi.dll"
    } else {
      FFI_LIBRARY
    });
  toolchain::require_file(
    &library,
    "The ffi crate must produce a host cdylib; check crates/jellypilot-ffi/Cargo.toml.",
  )?;
  let directory = toolchain::bindings_dir();
  if directory.exists() {
    fs::remove_dir_all(&directory)?;
  }
  process::run(
    process::cargo()
      .args([
        "run",
        "--locked",
        "--manifest-path",
        "Cargo.toml",
        "--package",
        FFI_PACKAGE,
        "--bin",
        "jellypilot-bindgen",
        "--",
        "generate",
        "--library",
      ])
      .arg(&library)
      .args(["--language", "kotlin", "--out-dir"])
      .arg(&directory)
      .arg("--no-format"),
  )?;
  let generated = kotlin_files(&directory)?;
  ensure!(
    generated > 0,
    "jellypilot-bindgen produced no Kotlin sources under {}.",
    directory.display()
  );
  println!("Generated {generated} Kotlin binding file(s).");
  Ok(())
}

fn rust(release: bool) -> Result<()> {
  let toolchain = toolchain::resolve()?;
  let mut command = process::cargo();
  command.args([
    "build",
    "--locked",
    "--manifest-path",
    "Cargo.toml",
    "--package",
    FFI_PACKAGE,
    "--target",
    RUST_TARGET,
  ]);
  if release {
    command.arg("--release");
  }
  let clang = toolchain::executable(
    &toolchain.llvm_bin_dir,
    &format!("{}-clang", toolchain.clang_triple),
  );
  command
    .env("CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER", &clang)
    .env("CC_aarch64_linux_android", &clang)
    .env(
      "AR_aarch64_linux_android",
      toolchain::executable(&toolchain.llvm_bin_dir, "llvm-ar"),
    )
    .env(
      "RANLIB_aarch64_linux_android",
      toolchain::executable(&toolchain.llvm_bin_dir, "llvm-ranlib"),
    );
  process::run(&mut command)?;
  let built = target_dir()
    .join(RUST_TARGET)
    .join(if release { "release" } else { "debug" })
    .join(FFI_LIBRARY);
  toolchain::require_file(
    &built,
    "The Android cross-build produced no cdylib; check crates/jellypilot-ffi/Cargo.toml.",
  )?;
  let destination = toolchain::jni_libs_dir().join(FFI_LIBRARY);
  fs::create_dir_all(toolchain::jni_libs_dir())?;
  fs::copy(&built, &destination)?;
  println!("Staged {FFI_LIBRARY} at {}.", destination.display());
  Ok(())
}

fn mpv() -> Result<()> {
  crate::android_mpv::build()?;
  toolchain::require_file(&toolchain::artifact_root().join("lib/libmpv.so"),
    "The native pipeline finished without producing libmpv.so; inspect target/android/mpv/arm64-v8a/manifest.json.")
}

fn gradle(toolchain: &toolchain::Toolchain, task: &str) -> Result<()> {
  let wrapper = toolchain::gradlew();
  let mut command = if cfg!(windows) {
    let mut command = process::command("cmd");
    command.args(["/C"]).arg(&wrapper);
    command
  } else {
    process::command(&wrapper)
  };
  command
    .args(["-p", "android", task])
    .env("ANDROID_HOME", &toolchain.sdk_root)
    .env("ANDROID_SDK_ROOT", &toolchain.sdk_root)
    .env("ANDROID_NDK_HOME", &toolchain.ndk_root);
  process::run(&mut command)?;
  Ok(())
}

fn build(release: bool) -> Result<()> {
  let toolchain = toolchain::resolve_build()?;
  bindings()?;
  rust(release)?;
  mpv()?;
  gradle(
    &toolchain,
    if release {
      "assembleRelease"
    } else {
      "assembleDebug"
    },
  )
}

fn check() -> Result<()> {
  let toolchain = toolchain::resolve_build()?;
  bindings()?;
  gradle(&toolchain, "check")
}

struct DoctorCheck {
  label: String,
  required: bool,
  result: Result<String>,
}

fn path_detail(path: &Path, directory: bool, hint: &str) -> Result<String> {
  if directory {
    toolchain::require_directory(path, hint)?;
  } else {
    toolchain::require_file(path, hint)?;
  }
  Ok(path.display().to_string())
}

fn doctor() -> Result<()> {
  let sdk = toolchain::sdk_root()?;
  let mut checks = Vec::new();
  let mut probe = |label: String, required: bool, result: Result<String>| {
    checks.push(DoctorCheck {
      label,
      required,
      result,
    });
  };
  probe(
    "JDK".into(),
    true,
    (|| {
      let output = process::capture(process::command(toolchain::java()?).arg("-version"))
        .context("Install a JDK on PATH or set JAVA_HOME.")?;
      let version = if output.stderr.is_empty() {
        &output.stdout
      } else {
        &output.stderr
      };
      Ok(
        String::from_utf8_lossy(version)
          .lines()
          .next()
          .unwrap_or("java available")
          .trim()
          .to_owned(),
      )
    })(),
  );
  probe(
    "Android SDK".into(),
    true,
    path_detail(
      &sdk,
      true,
      "Set ANDROID_HOME or ANDROID_SDK_ROOT, or install at ~/Android/Sdk.",
    ),
  );
  probe(
    format!("platform android-{COMPILE_SDK}"),
    true,
    toolchain::platform(&sdk).map(|path| path.display().to_string()),
  );
  probe(
    format!("build-tools {BUILD_TOOLS_VERSION}"),
    true,
    path_detail(
      &sdk.join("build-tools").join(BUILD_TOOLS_VERSION),
      true,
      &format!("Install with sdkmanager \"build-tools;{BUILD_TOOLS_VERSION}\"."),
    ),
  );
  probe(
    format!("NDK {NDK_VERSION} + cmake"),
    true,
    toolchain::resolve().map(|toolchain| {
      format!(
        "{} (host {}, cmake {})",
        toolchain.ndk_root.display(),
        toolchain.host_tag,
        toolchain.cmake_bin_dir.display()
      )
    }),
  );
  probe(
    format!("rust target {RUST_TARGET}"),
    true,
    (|| {
      let output =
        process::capture(process::command("rustup").args(["target", "list", "--installed"]))
          .with_context(|| format!("Run rustup target add {RUST_TARGET}."))?;
      ensure!(
        String::from_utf8_lossy(&output.stdout)
          .lines()
          .any(|line| line.trim() == RUST_TARGET),
        "Run rustup target add {RUST_TARGET}."
      );
      Ok(RUST_TARGET.to_owned())
    })(),
  );
  probe(
    FFI_PACKAGE.into(),
    true,
    path_detail(
      &crate::root().join("crates/jellypilot-ffi/Cargo.toml"),
      false,
      "The shared ffi crate is required for bindings and the Rust cross-build.",
    ),
  );
  probe(
    "Gradle wrapper".into(),
    true,
    path_detail(
      &toolchain::gradlew(),
      false,
      "The pinned Gradle wrapper must be checked in under android/.",
    ),
  );
  probe(
    "sdkmanager".into(),
    false,
    (|| {
      for version in ["latest", "23.0"] {
        let path = sdk
          .join("cmdline-tools")
          .join(version)
          .join("bin")
          .join(if cfg!(windows) {
            "sdkmanager.bat"
          } else {
            "sdkmanager"
          });
        if path.is_file() {
          return Ok(path.display().to_string());
        }
      }
      bail!("Optional; install cmdline-tools to manage SDK packages.")
    })(),
  );
  probe(
    format!("staged {FFI_LIBRARY}"),
    false,
    path_detail(
      &toolchain::jni_libs_dir().join(FFI_LIBRARY),
      false,
      "Run cargo xtask android rust.",
    ),
  );
  probe(
    "staged libmpv.so".into(),
    false,
    path_detail(
      &toolchain::artifact_root().join("lib/libmpv.so"),
      false,
      "Run cargo xtask android mpv.",
    ),
  );
  probe(
    "generated Kotlin bindings".into(),
    false,
    (|| {
      let directory = toolchain::bindings_dir();
      let count = kotlin_files(&directory).context("Run cargo xtask android bindings.")?;
      ensure!(count > 0, "Run cargo xtask android bindings.");
      Ok(format!("{count} file(s) under {}", directory.display()))
    })(),
  );
  println!("\n=== android doctor ===");
  let mut missing = Vec::new();
  for check in checks {
    let (mark, detail) = match check.result {
      Ok(detail) => ("ok", detail),
      Err(error) => {
        let detail = format!("{error:#}");
        if check.required {
          missing.push(format!("- {}: {detail}", check.label));
        }
        (if check.required { "MISSING" } else { "absent" }, detail)
      }
    };
    println!("[{mark}] {}: {detail}", check.label);
  }
  ensure!(
    missing.is_empty(),
    "Missing required Android prerequisites:\n{}",
    missing.join("\n")
  );
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn bindings_require_real_kotlin_files_recursively() -> Result<()> {
    let directory = tempfile::tempdir()?;
    fs::create_dir_all(directory.path().join("uniffi/jellypilot_sdk"))?;
    fs::write(
      directory.path().join("uniffi/jellypilot_sdk/bridge.kt"),
      "package uniffi.jellypilot_sdk",
    )?;
    fs::write(directory.path().join("manifest.json"), "{}")?;
    fs::create_dir(directory.path().join("fake.kt"))?;
    assert_eq!(kotlin_files(directory.path())?, 1);
    Ok(())
  }

  #[test]
  fn unavailable_bindings_are_not_reported_as_empty_success() {
    let directory = tempfile::tempdir().unwrap();
    assert!(kotlin_files(&directory.path().join("missing")).is_err());
  }
}
