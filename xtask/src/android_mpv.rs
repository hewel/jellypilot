use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

use anyhow::{bail, ensure, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::android_toolchain::{self, Toolchain, ABI, MIN_API, NDK_VERSION, RUST_TARGET};
use crate::{process, util};

const POLLUTED_ENV: &[&str] = &[
  "CPATH",
  "LIBRARY_PATH",
  "C_INCLUDE_PATH",
  "CPLUS_INCLUDE_PATH",
  "CFLAGS",
  "CXXFLAGS",
  "CPPFLAGS",
  "PKG_CONFIG_PATH",
];
const HOST_TOOLS: &[&str] = &[
  "git",
  "tar",
  "make",
  "meson",
  "ninja",
  "pkg-config",
  "python3",
  "gperf",
  "env",
  "patch",
];
const SYSTEM_LIBS: &[&str] = &[
  "libaaudio.so",
  "libandroid.so",
  "libc.so",
  "libcamera2ndk.so",
  "libdl.so",
  "libEGL.so",
  "libGLESv1_CM.so",
  "libGLESv2.so",
  "libGLESv3.so",
  "libjnigraphics.so",
  "liblog.so",
  "libm.so",
  "libmediandk.so",
  "libOpenSLES.so",
  "libvulkan.so",
  "libz.so",
];

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Source {
  Tarball {
    url: String,
    sha256: String,
  },
  Git {
    remote: String,
    commit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    submodules: Option<bool>,
  },
}

#[derive(Debug, Deserialize, Serialize)]
struct Patch {
  file: String,
  sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Dependency {
  name: String,
  version: String,
  source: Source,
  license_files: Vec<String>,
  meson_options: Option<Vec<String>>,
  configure_options: Option<Vec<String>>,
  patch: Option<Patch>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Mpv {
  source: Source,
  license_files: Vec<String>,
  meson_options: Vec<String>,
  patch: Option<Patch>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pin {
  schema_version: u32,
  abi: String,
  api_level: u32,
  ndk_version: String,
  cpu_family: String,
  cpu: String,
  deps: Vec<Dependency>,
  mpv: Mpv,
  reproducibility: String,
}

fn relative_path(path: &str) -> Result<&Path> {
  let path = Path::new(path);
  ensure!(
    !path.as_os_str().is_empty()
      && path
        .components()
        .all(|component| matches!(component, Component::Normal(_))),
    "Pin path must stay inside its owned directory: {}",
    path.display()
  );
  Ok(path)
}

fn valid_hex(value: &str, length: usize) -> bool {
  value.len() == length
    && value
      .bytes()
      .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

impl Pin {
  fn validate(&self) -> Result<()> {
    ensure!(
      self.schema_version == 1,
      "Unsupported Android mpv pin schema {}",
      self.schema_version
    );
    ensure!(
      self.abi == ABI && self.api_level == MIN_API && self.ndk_version == NDK_VERSION,
      "Android mpv pins must match {ABI}, API {MIN_API}, NDK {NDK_VERSION}"
    );
    ensure!(
      matches!(self.mpv.source, Source::Git { .. }),
      "mpv must use a pinned git source"
    );
    let mut names = BTreeSet::new();
    for dep in &self.deps {
      ensure!(
        relative_path(&dep.name)?.components().count() == 1
          && dep.name != "mpv"
          && names.insert(&dep.name),
        "Invalid or duplicate Android dependency name {}",
        dep.name
      );
      validate_source(&dep.source)?;
      validate_material(&dep.license_files, dep.patch.as_ref())?;
    }
    validate_source(&self.mpv.source)?;
    validate_material(&self.mpv.license_files, self.mpv.patch.as_ref())?;
    Ok(())
  }
}

fn validate_source(source: &Source) -> Result<()> {
  match source {
    Source::Tarball { url, sha256 } => {
      let url = reqwest::Url::parse(url)?;
      ensure!(
        url.scheme() == "https" && valid_hex(sha256, 64),
        "Tarball requires HTTPS and a lowercase SHA256 digest"
      );
    }
    Source::Git { commit, remote, .. } => {
      ensure!(
        remote.starts_with("https://") && valid_hex(commit, 40),
        "Git source requires HTTPS and an immutable full commit hash"
      );
    }
  }
  Ok(())
}

fn validate_material(licenses: &[String], patch: Option<&Patch>) -> Result<()> {
  ensure!(
    !licenses.is_empty(),
    "Every native source must declare its license material"
  );
  for file in licenses {
    relative_path(file)?;
  }
  if let Some(patch) = patch {
    relative_path(&patch.file)?;
    ensure!(
      valid_hex(&patch.sha256, 64),
      "Patch requires a lowercase SHA256 digest"
    );
  }
  Ok(())
}

struct Build {
  toolchain: Toolchain,
  pin: Pin,
  sources: PathBuf,
  cache: PathBuf,
  build_root: PathBuf,
  prefix: PathBuf,
  jobs: String,
}

impl Build {
  fn command(
    &self,
    program: impl AsRef<std::ffi::OsStr>,
    cross: bool,
    unset_cc: bool,
  ) -> Result<Command> {
    let mut command = process::command(program);
    for name in POLLUTED_ENV {
      command.env_remove(name);
    }
    let mut paths = vec![self.toolchain.llvm_bin_dir.clone()];
    if let Some(path) = std::env::var_os("PATH") {
      paths.extend(std::env::split_paths(&path));
    }
    command.env("PATH", std::env::join_paths(paths)?);
    if cross {
      command
        .env("CC", format!("{}-clang", self.toolchain.clang_triple))
        .env("CXX", format!("{}-clang++", self.toolchain.clang_triple))
        .env("AR", "llvm-ar")
        .env("RANLIB", "llvm-ranlib")
        .env("LDFLAGS", "-Wl,-O1,--icf=safe -Wl,-z,max-page-size=16384")
        .env("PKG_CONFIG_SYSROOT_DIR", &self.prefix)
        .env("PKG_CONFIG_LIBDIR", self.prefix.join("lib/pkgconfig"));
    }
    if unset_cc {
      command.env_remove("CC").env_remove("CXX");
    }
    Ok(command)
  }

  fn host_tools(&self) -> Result<BTreeMap<String, String>> {
    let mut tools = BTreeMap::new();
    for &name in HOST_TOOLS {
      let output = process::capture(self.command(name, false, false)?.arg("--version"))
        .with_context(|| format!("Missing Android native host prerequisite: {name}"))?;
      tools.insert(name.to_owned(), first_line(&output.stdout));
    }
    let clang = format!("{}-clang", self.toolchain.clang_triple);
    let output = process::capture(self.command(&clang, false, false)?.arg("--version"))?;
    tools.insert(clang, first_line(&output.stdout));
    let output = process::capture(
      self
        .command(self.toolchain.cmake_bin_dir.join("cmake"), false, false)?
        .arg("--version"),
    )?;
    tools.insert("cmake".into(), first_line(&output.stdout));
    Ok(tools)
  }

  fn git(&self, directory: &Path, args: &[&str], allow_failure: bool) -> Result<Output> {
    process::run_with(
      self
        .command("git", false, false)?
        .arg("-C")
        .arg(directory)
        .args(args),
      process::RunOptions {
        capture: true,
        allow_failure,
        ..Default::default()
      },
    )
  }

  fn fetch_git(&self, name: &str, source: &Source) -> Result<PathBuf> {
    let Source::Git {
      remote,
      commit,
      tag,
      submodules,
      ..
    } = source
    else {
      bail!("{name} requires a git source");
    };
    let directory = self.sources.join(name);
    // A separate git directory or worktree uses a .git file, not a directory.
    // Existing metadata must reach Git's integrity checks without deleting the checkout.
    match fs::symlink_metadata(directory.join(".git")) {
      Ok(_) => {}
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
        reset_directory(&directory)?;
        self.git(&directory, &["init"], false)?;
      }
      Err(error) => return Err(error).context("Inspecting Android source checkout metadata"),
    }
    let object = format!("{commit}^{{commit}}");
    if !self
      .git(&directory, &["cat-file", "-e", &object], true)?
      .status
      .success()
    {
      let shallow = self.git(&directory, &["fetch", "--depth=1", remote, commit], true)?;
      if !shallow.status.success() {
        if let Some(tag) = tag {
          self.git(
            &directory,
            &["fetch", "--depth=1", remote, "tag", tag],
            true,
          )?;
        }
      }
      if !self
        .git(&directory, &["cat-file", "-e", &object], true)?
        .status
        .success()
      {
        self
          .git(&directory, &["fetch", remote, commit], false)
          .with_context(|| {
            format!("Cannot retrieve pinned {name} commit {commit} from {remote}")
          })?;
      }
    }
    self.git(&directory, &["checkout", "--detach", commit], false)?;
    self.git(&directory, &["clean", "-fdx"], false)?;
    if *submodules == Some(true) {
      self.git(
        &directory,
        &[
          "submodule",
          "update",
          "--init",
          "--recursive",
          "--recommend-shallow",
        ],
        false,
      )?;
    }
    self.verify_git(name, &directory, source, None)?;
    Ok(directory)
  }

  // Write the complete diff to disk: the common process runner intentionally retains bounded tails.
  fn diff_hash(&self, directory: &Path) -> Result<(String, u64)> {
    let file = tempfile::NamedTempFile::new()?;
    process::capture(
      self
        .command("git", false, false)?
        .arg("-C")
        .arg(directory)
        .args([
          "diff",
          "--no-ext-diff",
          "--no-textconv",
          "--binary",
          "--ignore-submodules=untracked",
          "HEAD",
        ])
        .arg(format!("--output={}", file.path().display())),
    )?;
    Ok((
      util::sha256_file(file.path())?,
      file.as_file().metadata()?.len(),
    ))
  }

  fn verify_git(
    &self,
    name: &str,
    directory: &Path,
    source: &Source,
    expected_diff: Option<&str>,
  ) -> Result<()> {
    let Source::Git {
      commit, submodules, ..
    } = source
    else {
      return Ok(());
    };
    let head = self.git(directory, &["rev-parse", "HEAD"], false)?;
    ensure!(
      String::from_utf8_lossy(&head.stdout).trim() == commit,
      "{name} checkout HEAD must be exactly {commit}"
    );
    let (hash, size) = self.diff_hash(directory)?;
    ensure!(
      expected_diff.map_or(size == 0, |expected| hash == expected),
      "{name} checkout has unexpected tracked changes"
    );
    if *submodules == Some(true) {
      let status = self.git(directory, &["submodule", "status", "--recursive"], false)?;
      ensure!(
        String::from_utf8_lossy(&status.stdout)
          .lines()
          .all(|line| line.starts_with(' ')),
        "{name} submodules do not match their pinned gitlinks"
      );
    }
    Ok(())
  }

  fn fetch_tarball(&self, name: &str, url: &str, digest: &str) -> Result<PathBuf> {
    let parsed = reqwest::Url::parse(url)?;
    let filename = parsed
      .path_segments()
      .and_then(|mut segments| segments.next_back())
      .filter(|name| !name.is_empty())
      .context("Archive URL must end in a filename")?;
    relative_path(filename)?;
    let file = self.cache.join(filename);
    if file.is_file() {
      if util::sha256_file(&file)? == digest {
        return Ok(file);
      }
      eprintln!("Cached {name} archive failed SHA256 verification; downloading again.");
      fs::remove_file(&file)?;
    }
    let mut partial = tempfile::NamedTempFile::new_in(&self.cache)?;
    reqwest::blocking::Client::builder()
      .timeout(Duration::from_secs(300))
      .build()?
      .get(url)
      .send()?
      .error_for_status()?
      .copy_to(partial.as_file_mut())?;
    verify_digest(partial.path(), digest)
      .with_context(|| format!("Refusing unverified {name} source from {url}"))?;
    partial.persist(&file).map_err(|error| error.error)?;
    Ok(file)
  }

  fn prepare_source(&self, dep: &Dependency) -> Result<PathBuf> {
    match &dep.source {
      Source::Git { .. } => self.fetch_git(&dep.name, &dep.source),
      Source::Tarball { url, sha256 } => {
        let archive = self.fetch_tarball(&dep.name, url, sha256)?;
        let directory = self.sources.join(&dep.name);
        reset_directory(&directory)?;
        process::run(
          self
            .command("tar", false, false)?
            .arg("-xf")
            .arg(archive)
            .arg("-C")
            .arg(&directory)
            .arg("--strip-components=1"),
        )?;
        Ok(directory)
      }
    }
  }

  fn patch_source(&self, name: &str, source: &Path, patch: Option<&Patch>) -> Result<PathBuf> {
    let Some(patch) = patch else {
      return Ok(source.to_path_buf());
    };
    let file = crate::root().join(relative_path(&patch.file)?);
    verify_digest(&file, &patch.sha256).context("Source patch digest mismatch")?;
    let directory = self.build_root.join(format!("patched-{name}"));
    reset_directory(&directory)?;
    copy_tree(source, &directory, true)?;
    process::run(
      self
        .command("patch", false, false)?
        .current_dir(&directory)
        .args(["--batch", "--forward", "-p1", "--input"])
        .arg(&file),
    )?;
    Ok(directory)
  }

  fn meson(&self, name: &str, source: &Path, options: &[String]) -> Result<()> {
    let directory = source.join("_build");
    remove_directory(&directory)?;
    process::run(
      self
        .command("meson", true, true)?
        .args(["setup"])
        .arg(&directory)
        .arg(source)
        .arg("--cross-file")
        .arg(self.prefix.join("crossfile.txt"))
        .args(options),
    )
    .with_context(|| format!("Configuring {name}"))?;
    process::run(
      self
        .command("ninja", true, true)?
        .arg("-C")
        .arg(&directory)
        .arg(format!("-j{}", self.jobs)),
    )?;
    process::run(
      self
        .command("ninja", true, true)?
        .arg("-C")
        .arg(directory)
        .arg("install")
        .env("DESTDIR", &self.prefix),
    )?;
    Ok(())
  }

  fn configure(&self, dep: &Dependency, source: &Path, extra: &[String]) -> Result<()> {
    let directory = source.join("_build");
    reset_directory(&directory)?;
    process::run(
      self
        .command(source.join("configure"), true, false)?
        .current_dir(&directory)
        .args(dep.configure_options.as_deref().unwrap_or_default())
        .args(extra),
    )?;
    process::run(
      self
        .command("make", true, false)?
        .current_dir(&directory)
        .arg(format!("-j{}", self.jobs)),
    )?;
    process::run(
      self
        .command("make", true, false)?
        .current_dir(directory)
        .arg("install")
        .arg(format!("DESTDIR={}", self.prefix.display())),
    )?;
    Ok(())
  }

  fn mbedtls(&self, source: &Path) -> Result<()> {
    // The archive is re-extracted each run; mbedtls requires an in-source build.
    for args in [
      vec!["scripts/config.py", "set", "MBEDTLS_AESNI_C"],
      vec![
        "scripts/config.py",
        "set",
        "MBEDTLS_PLATFORM_DEV_RANDOM",
        "\"/dev/urandom\"",
      ],
    ] {
      process::run(
        self
          .command("python3", true, false)?
          .current_dir(source)
          .args(args),
      )?;
    }
    process::run(
      self
        .command("make", true, false)?
        .current_dir(source)
        .arg(format!("-j{}", self.jobs))
        .arg("no_test"),
    )?;
    process::run(
      self
        .command("make", true, false)?
        .current_dir(source)
        .arg("install")
        .arg(format!("DESTDIR={}", self.prefix.display())),
    )?;
    Ok(())
  }

  fn glslang(&self, source: &Path) -> Result<()> {
    let directory = source.join("_build");
    remove_directory(&directory)?;
    let cmake = self.toolchain.cmake_bin_dir.join("cmake");
    process::run(
      self
        .command(&cmake, true, true)?
        .arg("-S")
        .arg(source)
        .arg("-B")
        .arg(&directory)
        .args(["-G", "Ninja"])
        .arg(format!(
          "-DCMAKE_MAKE_PROGRAM={}",
          self.toolchain.cmake_bin_dir.join("ninja").display()
        ))
        .arg(format!(
          "-DCMAKE_TOOLCHAIN_FILE={}",
          self
            .toolchain
            .ndk_root
            .join("build/cmake/android.toolchain.cmake")
            .display()
        ))
        .arg(format!("-DANDROID_ABI={}", self.pin.abi))
        .arg(format!("-DANDROID_PLATFORM=android-{}", self.pin.api_level))
        .arg(format!("-DCMAKE_INSTALL_PREFIX={}", self.prefix.display()))
        .args([
          "-DANDROID_STL=c++_shared",
          "-DCMAKE_BUILD_TYPE=Release",
          "-DCMAKE_INSTALL_LIBDIR=lib",
          "-DCMAKE_POSITION_INDEPENDENT_CODE=ON",
          "-DBUILD_SHARED_LIBS=OFF",
          "-DBUILD_EXTERNAL=OFF",
          "-DENABLE_OPT=OFF",
          "-DENABLE_HLSL=OFF",
          "-DENABLE_GLSLANG_BINARIES=OFF",
          "-DGLSLANG_TESTS=OFF",
          "-DGLSLANG_ENABLE_INSTALL=ON",
        ]),
    )?;
    process::run(
      self
        .command(&cmake, true, true)?
        .arg("--build")
        .arg(&directory)
        .arg("--parallel")
        .arg(&self.jobs),
    )?;
    process::run(
      self
        .command(&cmake, true, true)?
        .arg("--install")
        .arg(directory),
    )?;
    Ok(())
  }

  fn vulkan_headers(&self, source: &Path) -> Result<()> {
    let include = source.join("3rdparty/Vulkan-Headers/include");
    let header = fs::read_to_string(include.join("vulkan/vulkan_core.h"))?;
    let patch = Regex::new(r"(?m)^#define VK_HEADER_VERSION (\d+)$")?
      .captures(&header)
      .context("Pinned Vulkan header version is missing")?;
    let api = Regex::new(r"(?m)^#define VK_HEADER_VERSION_COMPLETE VK_MAKE_API_VERSION\(\d+, (\d+), (\d+), VK_HEADER_VERSION\)$")?
      .captures(&header).context("Pinned Vulkan API version is missing")?;
    copy_tree(&include, &self.prefix.join("include"), false)?;
    fs::create_dir_all(self.prefix.join("lib/pkgconfig"))?;
    fs::write(self.prefix.join("lib/pkgconfig/vulkan.pc"), format!(
      "Name: Vulkan\nDescription: Android system Vulkan loader with pinned Vulkan-Headers\nVersion: {}.{}.{}\nCflags: -I{}/include\nLibs: -lvulkan\n",
      &api[1], &api[2], &patch[1], self.prefix.display()))?;
    Ok(())
  }

  fn crossfile(&self) -> Result<()> {
    let prefix = meson_string(&self.prefix.to_string_lossy());
    fs::write(self.prefix.join("crossfile.txt"), format!(
      "[built-in options]\nbuildtype = 'release'\ndefault_library = 'static'\nwrap_mode = 'nodownload'\nprefix = '/usr/local'\nc_args = ['-I{prefix}/include']\ncpp_args = ['-I{prefix}/include']\nc_link_args = ['-L{prefix}/lib', '-Wl,-z,max-page-size=16384']\ncpp_link_args = ['-L{prefix}/lib', '-Wl,-z,max-page-size=16384']\n[binaries]\nc = '{}-clang'\ncpp = '{}-clang++'\nar = 'llvm-ar'\nnm = 'llvm-nm'\nstrip = 'llvm-strip'\npkgconfig = 'pkg-config'\npkg-config = 'pkg-config'\n[host_machine]\nsystem = 'android'\ncpu_family = '{}'\ncpu = 'aarch64'\nendian = 'little'\n[properties]\nneeds_exe_wrapper = true\n",
      self.toolchain.clang_triple, self.toolchain.clang_triple, meson_string(&self.pin.cpu_family)))?;
    Ok(())
  }

  fn readelf(&self, flag: &str, path: &Path) -> Result<String> {
    let output = process::capture(
      self
        .command(
          self.toolchain.llvm_bin_dir.join("llvm-readelf"),
          false,
          false,
        )?
        .arg(flag)
        .arg(path),
    )?;
    Ok(String::from_utf8(output.stdout)?)
  }

  fn needed(&self, directory: &Path, staged: &BTreeSet<String>) -> Result<BTreeSet<String>> {
    let mut needed = BTreeSet::new();
    for name in staged {
      needed.extend(elf_fields(
        &self.readelf("-dW", &directory.join(name))?,
        "NEEDED",
      )?);
    }
    Ok(needed)
  }

  fn stage(&self, sources: &BTreeMap<String, PathBuf>) -> Result<Vec<String>> {
    let root = android_toolchain::artifact_root();
    reset_directory(&root)?;
    let libs = root.join("lib");
    let notices = root.join("notices");
    fs::create_dir_all(&libs)?;
    fs::create_dir_all(&notices)?;
    let mut staged = BTreeSet::new();
    for entry in fs::read_dir(self.prefix.join("lib"))? {
      let entry = entry?;
      let name = entry.file_name().to_string_lossy().into_owned();
      if !(name.ends_with(".so") || name.contains(".so.")) {
        continue;
      }
      let real = fs::canonicalize(entry.path())?;
      let mut names = elf_fields(&self.readelf("-dW", &real)?, "SONAME")?;
      let basename = real
        .file_name()
        .context("Shared library filename is missing")?
        .to_string_lossy();
      let (base, _) = basename
        .split_once(".so")
        .context("Invalid shared library filename")?;
      names.push(format!("{base}.so"));
      for name in names {
        ensure!(
          relative_path(&name)?.components().count() == 1,
          "Invalid ELF SONAME {name}"
        );
        fs::copy(&real, libs.join(&name))?;
        staged.insert(name);
      }
    }
    ensure!(
      staged.contains("libmpv.so"),
      "Native pipeline produced no staged libmpv.so"
    );
    copy_tree(&self.prefix.join("include"), &root.join("include"), false)?;
    let mut needed = self.needed(&libs, &staged)?;
    if needed.contains("libc++_shared.so") && !staged.contains("libc++_shared.so") {
      fs::copy(
        self
          .toolchain
          .sysroot
          .join("usr/lib")
          .join(RUST_TARGET)
          .join("libc++_shared.so"),
        libs.join("libc++_shared.so"),
      )?;
      staged.insert("libc++_shared.so".into());
      needed = self.needed(&libs, &staged)?;
    }
    validate_dependencies(&needed, &staged)?;
    for name in &staged {
      validate_alignment(name, &self.readelf("-lW", &libs.join(name))?)?;
    }
    for (name, files, patch) in self
      .pin
      .deps
      .iter()
      .map(|dep| (dep.name.as_str(), &dep.license_files, dep.patch.as_ref()))
      .chain(std::iter::once((
        "mpv",
        &self.pin.mpv.license_files,
        self.pin.mpv.patch.as_ref(),
      )))
    {
      let source = sources
        .get(name)
        .with_context(|| format!("Missing source directory for {name} license material"))?;
      for license in files {
        let relative = relative_path(license)?;
        let file = source.join(relative);
        android_toolchain::require_file(
          &file,
          &format!("Pinned license file {license} is missing from {name} source."),
        )?;
        let destination = notices
          .join(name)
          .join(relative.file_name().context("Missing license filename")?);
        copy_file(&file, &destination)?;
      }
      if let Some(patch) = patch {
        let relative = relative_path(&patch.file)?;
        let source = crate::root().join(relative);
        verify_digest(&source, &patch.sha256)?;
        copy_file(
          &source,
          &root
            .join("source-patches")
            .join(name)
            .join(relative.file_name().context("Missing patch filename")?),
        )?;
      }
    }
    for notice in ["NOTICE", "NOTICE.toolchain"] {
      let source = self.toolchain.ndk_root.join(notice);
      if source.is_file() {
        copy_file(&source, &notices.join("android-ndk").join(notice))?;
      }
    }
    Ok(staged.into_iter().collect())
  }

  fn manifest(&self, tools: &BTreeMap<String, String>, libraries: &[String]) -> Result<()> {
    let root = android_toolchain::artifact_root();
    let mut artifacts = BTreeMap::new();
    collect_artifacts(&root, &root, &mut artifacts)?;
    let sources = self
      .pin
      .deps
      .iter()
      .map(|dep| {
        (
          dep.name.as_str(),
          json!({
            "version": dep.version, "source": dep.source, "mesonOptions": dep.meson_options,
            "configureOptions": dep.configure_options, "patch": dep.patch,
          }),
        )
      })
      .collect::<BTreeMap<_, _>>();
    util::write_json(
      &root.join("manifest.json"),
      &json!({
        "schemaVersion": 1, "abi": self.pin.abi, "apiLevel": self.pin.api_level,
        "ndk": { "version": self.pin.ndk_version, "root": self.toolchain.ndk_root, "hostTag": self.toolchain.host_tag },
        "toolchain": { "clangTriple": self.toolchain.clang_triple, "llvmBinDir": self.toolchain.llvm_bin_dir, "sysroot": self.toolchain.sysroot },
        "tools": tools, "sources": sources,
        "mpv": { "source": self.pin.mpv.source, "mesonOptions": self.pin.mpv.meson_options, "patch": self.pin.mpv.patch },
        "libraries": libraries, "artifacts": artifacts, "reproducibility": self.pin.reproducibility,
      }),
    )
  }
}

fn first_line(output: &[u8]) -> String {
  String::from_utf8_lossy(output)
    .trim()
    .lines()
    .next()
    .unwrap_or_default()
    .to_owned()
}

fn meson_string(value: &str) -> String {
  value.replace('\\', "\\\\").replace('\'', "\\'")
}

fn verify_digest(file: &Path, expected: &str) -> Result<()> {
  let digest = util::sha256_file(file)?;
  ensure!(
    digest == expected,
    "SHA256 mismatch for {}: found {digest}, expected {expected}",
    file.display()
  );
  Ok(())
}

fn remove_directory(path: &Path) -> Result<()> {
  match fs::remove_dir_all(path) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error.into()),
  }
}

fn reset_directory(path: &Path) -> Result<()> {
  remove_directory(path)?;
  fs::create_dir_all(path)?;
  Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<()> {
  fs::create_dir_all(destination.parent().context("Destination has no parent")?)?;
  fs::copy(source, destination)
    .with_context(|| format!("Copying {} to {}", source.display(), destination.display()))?;
  Ok(())
}

fn copy_tree(source: &Path, destination: &Path, exclude_build: bool) -> Result<()> {
  fs::create_dir_all(destination)?;
  for entry in fs::read_dir(source)? {
    let entry = entry?;
    if exclude_build && entry.file_name() == "_build" {
      continue;
    }
    let target = destination.join(entry.file_name());
    let kind = entry.file_type()?;
    if kind.is_symlink() {
      create_symlink(&fs::read_link(entry.path())?, &target)?;
    } else if kind.is_dir() {
      copy_tree(&entry.path(), &target, exclude_build)?;
    } else if kind.is_file() {
      fs::copy(entry.path(), target)?;
    }
  }
  Ok(())
}

fn create_symlink(source: &Path, destination: &Path) -> Result<()> {
  #[cfg(unix)]
  {
    std::os::unix::fs::symlink(source, destination)?;
    Ok(())
  }
  #[cfg(not(unix))]
  {
    let _ = (source, destination);
    bail!("Android native source staging requires Unix symbolic links")
  }
}

fn elf_fields(output: &str, tag: &str) -> Result<Vec<String>> {
  let marker = format!("({tag})");
  output
    .lines()
    .filter(|line| line.contains(&marker))
    .map(|line| {
      let (_, field) = line
        .split_once('[')
        .context("Malformed ELF dynamic field")?;
      let (value, _) = field
        .split_once(']')
        .context("Malformed ELF dynamic field")?;
      ensure!(!value.is_empty(), "Empty ELF dynamic field");
      Ok(value.to_owned())
    })
    .collect()
}

fn validate_dependencies(needed: &BTreeSet<String>, staged: &BTreeSet<String>) -> Result<()> {
  let missing = needed
    .iter()
    .filter(|name| !staged.contains(*name) && !SYSTEM_LIBS.contains(&name.as_str()))
    .map(String::as_str)
    .collect::<Vec<_>>();
  ensure!(missing.is_empty(), "Staged libraries require unpackaged dependencies: {}. Extend the pipeline or system-library allowlist deliberately.", missing.join(", "));
  Ok(())
}

fn validate_alignment(name: &str, headers: &str) -> Result<()> {
  let mut segments = 0;
  for line in headers.lines() {
    let mut fields = line.split_whitespace();
    if fields.next() != Some("LOAD") {
      continue;
    }
    segments += 1;
    let field = fields
      .next_back()
      .context("ELF LOAD segment alignment is missing")?;
    let alignment = u64::from_str_radix(field.trim_start_matches("0x"), 16)
      .with_context(|| format!("Invalid {name} LOAD segment alignment {field}"))?;
    ensure!(
      alignment >= 0x4000,
      "{name} has a LOAD segment aligned to {field} (< 0x4000)"
    );
  }
  ensure!(segments > 0, "{name} has no ELF LOAD segments");
  Ok(())
}

fn collect_artifacts(
  root: &Path,
  directory: &Path,
  artifacts: &mut BTreeMap<String, serde_json::Value>,
) -> Result<()> {
  for entry in fs::read_dir(directory)? {
    let entry = entry?;
    if entry.file_type()?.is_dir() {
      collect_artifacts(root, &entry.path(), artifacts)?;
    } else if entry.file_type()?.is_file() {
      artifacts.insert(
        entry
          .path()
          .strip_prefix(root)?
          .to_string_lossy()
          .replace('\\', "/"),
        json!({ "sha256": util::sha256_file(&entry.path())? }),
      );
    }
  }
  Ok(())
}

pub fn build() -> Result<()> {
  ensure!(
    cfg!(target_os = "linux"),
    "The Android libmpv pipeline requires Linux and a Linux NDK toolchain."
  );
  let pin: Pin = util::read_json(&crate::root().join("tools/android/mpv-deps.json"))?;
  pin.validate()?;
  let toolchain = android_toolchain::resolve()?;
  ensure!(
    toolchain.clang_triple == format!("{RUST_TARGET}{}", pin.api_level),
    "Android toolchain does not match pinned API level"
  );
  let root = crate::root().join("target/android/mpv");
  let build = Build {
    toolchain,
    pin,
    sources: root.join("sources"),
    cache: root.join("cache"),
    build_root: root.join("build"),
    prefix: root.join("build/prefix"),
    jobs: std::thread::available_parallelism()
      .map_or(1, usize::from)
      .to_string(),
  };
  let tools = build.host_tools()?;
  fs::create_dir_all(&build.sources)?;
  fs::create_dir_all(&build.cache)?;
  reset_directory(&build.prefix)?;
  // DESTDIR=/prefix installs under /usr/local; these links flatten that layout.
  create_symlink(Path::new("."), &build.prefix.join("usr"))?;
  create_symlink(Path::new("."), &build.prefix.join("local"))?;
  build.crossfile()?;
  let mut sources = BTreeMap::new();
  for dep in &build.pin.deps {
    let pristine = build.prepare_source(dep)?;
    let source = build.patch_source(&dep.name, &pristine, dep.patch.as_ref())?;
    let expected = if dep.patch.is_some() && matches!(dep.source, Source::Git { .. }) {
      Some(build.diff_hash(&source)?.0)
    } else {
      None
    };
    sources.insert(dep.name.clone(), source.clone());
    match dep.name.as_str() {
      "mbedtls" => build.mbedtls(&source)?,
      "ffmpeg" => build.configure(
        dep,
        &source,
        &[
          format!("--cross-prefix={RUST_TARGET}-"),
          format!("--cc={}-clang", build.toolchain.clang_triple),
          format!("--arch={}", build.pin.cpu_family),
          format!("--cpu={}", build.pin.cpu),
          format!("--extra-cflags=-I{}/include", build.prefix.display()),
          format!(
            "--extra-ldflags=-L{}/lib -Wl,-z,max-page-size=16384",
            build.prefix.display()
          ),
        ],
      )?,
      "glslang" => build.glslang(&source)?,
      "libplacebo" => {
        build.vulkan_headers(&source)?;
        let mut options = dep
          .meson_options
          .clone()
          .context("libplacebo requires Meson options")?;
        options.push(format!("-Dvulkan-sdk={}", build.prefix.display()));
        build.meson(&dep.name, &source, &options)?;
      }
      _ => match &dep.meson_options {
        Some(options) => build.meson(&dep.name, &source, options)?,
        None => build.configure(dep, &source, &[])?,
      },
    }
    build.verify_git(&dep.name, &source, &dep.source, expected.as_deref())?;
  }
  // Meson omits libplacebo's C++ runtime in static pkg-config metadata (meson#11300).
  let pc = build.prefix.join("lib/pkgconfig/libplacebo.pc");
  let content = fs::read_to_string(&pc)?;
  if !content.contains("-lc++") {
    let expression = Regex::new(r"(?m)^Libs:(.*)$")?;
    ensure!(
      expression.is_match(&content),
      "{} has no Libs: line",
      pc.display()
    );
    fs::write(
      &pc,
      expression.replace(&content, "Libs:$1 -lc++").as_bytes(),
    )?;
  }
  let pristine = build.fetch_git("mpv", &build.pin.mpv.source)?;
  let mpv = build.patch_source("mpv", &pristine, build.pin.mpv.patch.as_ref())?;
  let expected = if build.pin.mpv.patch.is_some() {
    Some(build.diff_hash(&mpv)?.0)
  } else {
    None
  };
  build.verify_git("mpv", &mpv, &build.pin.mpv.source, expected.as_deref())?;
  build.meson("mpv", &mpv, &build.pin.mpv.meson_options)?;
  build.verify_git("mpv", &mpv, &build.pin.mpv.source, expected.as_deref())?;
  sources.insert("mpv".into(), mpv);
  ensure!(
    !build.prefix.join("lib/libmpv.a").exists(),
    "Meson produced libmpv.a instead of libmpv.so; check the mpv options"
  );
  android_toolchain::require_file(
    &build.prefix.join("lib/libmpv.so"),
    "mpv did not install lib/libmpv.so into the prefix.",
  )?;
  let libraries = build.stage(&sources)?;
  build.manifest(&tools, &libraries)?;
  println!(
    "Android libmpv staged at {}: {}. {}",
    android_toolchain::artifact_root().display(),
    libraries.join(", "),
    build.pin.reproducibility
  );
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn gitfile_checkout_preserves_tracked_edits_and_rejects_dirty_sources() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let sources = workspace.path().join("sources");
    fs::create_dir(&sources)?;
    let checkout = sources.join("dependency");
    let metadata = workspace.path().join("git-metadata");
    process::capture(
      process::command("git")
        .args(["init", "--separate-git-dir"])
        .arg(&metadata)
        .arg(&checkout),
    )?;
    fs::write(checkout.join("tracked.txt"), "pinned content\n")?;
    process::capture(
      process::command("git")
        .arg("-C")
        .arg(&checkout)
        .args(["add", "tracked.txt"]),
    )?;
    process::capture(process::command("git").arg("-C").arg(&checkout).args([
      "-c",
      "user.name=xtask test",
      "-c",
      "user.email=xtask@example.invalid",
      "-c",
      "commit.gpgsign=false",
      "commit",
      "-m",
      "pinned source",
    ]))?;
    let head = process::capture(
      process::command("git")
        .arg("-C")
        .arg(&checkout)
        .args(["rev-parse", "HEAD"]),
    )?;
    let source = Source::Git {
      remote: "https://example.invalid/never-fetch.git".into(),
      commit: String::from_utf8(head.stdout)?.trim().into(),
      tag: None,
      note: None,
      submodules: None,
    };
    let unused = workspace.path().join("unused-toolchain");
    let build = Build {
      toolchain: Toolchain {
        sdk_root: unused.clone(),
        ndk_root: unused.clone(),
        llvm_bin_dir: unused.clone(),
        sysroot: unused.clone(),
        host_tag: "linux-x86_64",
        clang_triple: format!("{RUST_TARGET}{MIN_API}"),
        cmake_bin_dir: unused,
      },
      pin: serde_json::from_str(include_str!("../../tools/android/mpv-deps.json"))?,
      sources,
      cache: workspace.path().join("cache"),
      build_root: workspace.path().join("build"),
      prefix: workspace.path().join("prefix"),
      jobs: "1".into(),
    };
    assert_eq!(build.fetch_git("dependency", &source)?, checkout);
    fs::write(checkout.join("tracked.txt"), "user changes must survive\n")?;
    let result = build.fetch_git("dependency", &source);
    assert!(result.is_err(), "Dirty source checkout was accepted");
    assert_eq!(
      fs::read_to_string(checkout.join("tracked.txt"))?,
      "user changes must survive\n"
    );
    assert!(checkout.join(".git").is_file());
    Ok(())
  }

  #[test]
  fn source_hash_mismatch_is_rejected_before_use() -> Result<()> {
    let file = tempfile::NamedTempFile::new()?;
    fs::write(file.path(), b"untrusted archive")?;
    assert!(verify_digest(file.path(), &"0".repeat(64)).is_err());
    let digest = util::sha256_file(file.path())?;
    verify_digest(file.path(), &digest)?;
    Ok(())
  }

  #[test]
  fn pinned_paths_cannot_escape_owned_source_directories() {
    for value in ["../outside", "/tmp/outside", "tools/../../outside", ""] {
      assert!(relative_path(value).is_err(), "Accepted {value}");
    }
  }

  #[test]
  fn missing_cpp_runtime_is_not_treated_as_android_system_library() {
    let needed = BTreeSet::from(["libc.so".into(), "libc++_shared.so".into()]);
    assert!(validate_dependencies(&needed, &BTreeSet::new()).is_err());
    assert!(validate_dependencies(&needed, &BTreeSet::from(["libc++_shared.so".into()])).is_ok());
  }

  #[test]
  fn dependency_closure_rejects_unstaged_transitive_libraries() {
    let needed = BTreeSet::from(["libavcodec.so.63".into(), "libm.so".into()]);
    assert!(validate_dependencies(&needed, &BTreeSet::from(["libmpv.so".into()])).is_err());
  }

  #[test]
  fn every_load_segment_must_support_sixteen_kilobyte_pages() {
    assert!(
      validate_alignment("libmpv.so", " LOAD 0 0 0 R E 0x4000\n LOAD 0 0 0 RW 0x1000").is_err()
    );
    assert!(validate_alignment(
      "libmpv.so",
      " LOAD 0 0 0 R E 0x4000\n LOAD 0 0 0 RW 0x10000"
    )
    .is_ok());
  }

  #[test]
  fn malformed_or_absent_elf_headers_fail_closed() {
    assert!(validate_alignment("bad.so", "not an ELF").is_err());
    assert!(validate_alignment("bad.so", "LOAD 0 0 invalid").is_err());
    assert!(elf_fields("0x01 (NEEDED) missing brackets", "NEEDED").is_err());
  }

  #[test]
  fn soname_and_needed_are_parsed_separately() -> Result<()> {
    let output = " 0x01 (NEEDED) Shared library: [libc++_shared.so]\n 0x0e (SONAME) Library soname: [libmpv.so]";
    assert_eq!(elf_fields(output, "SONAME")?, ["libmpv.so"]);
    assert_eq!(elf_fields(output, "NEEDED")?, ["libc++_shared.so"]);
    Ok(())
  }

  #[test]
  fn source_copy_excludes_build_artifacts_but_preserves_file_permissions() -> Result<()> {
    let source = tempfile::tempdir()?;
    let destination = tempfile::tempdir()?;
    fs::create_dir(source.path().join("_build"))?;
    fs::write(source.path().join("_build/stale"), "old")?;
    fs::write(source.path().join("configure"), "#!/bin/sh\n")?;
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      fs::set_permissions(
        source.path().join("configure"),
        fs::Permissions::from_mode(0o755),
      )?;
    }
    copy_tree(source.path(), destination.path(), true)?;
    assert!(!destination.path().join("_build").exists());
    assert_eq!(
      fs::read(destination.path().join("configure"))?,
      b"#!/bin/sh\n"
    );
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      assert_eq!(
        fs::metadata(destination.path().join("configure"))?
          .permissions()
          .mode()
          & 0o777,
        0o755
      );
    }
    Ok(())
  }
}
