use std::{
  collections::{BTreeMap, BTreeSet, VecDeque},
  fs,
  path::{Path, PathBuf},
  process::Output,
};

use anyhow::{ensure, Context, Result};
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};

use crate::{mpv, process, util};

const LIBRARY_RELATIVE: &str = "lib/jellypilot";
const NOTICE_RELATIVE: &str = "share/jellypilot/licenses/linux-runtime";
const ORIGIN: &str = "$ORIGIN";

#[derive(Debug, PartialEq)]
struct ElfDynamic {
  needed: Vec<String>,
  soname: Option<String>,
  runpaths: Vec<String>,
}

fn parse_elf(output: &str) -> Result<ElfDynamic> {
  let captures = |expression: &str| -> Result<Vec<String>> {
    Ok(
      Regex::new(expression)?
        .captures_iter(output)
        .map(|capture| capture[1].to_owned())
        .collect(),
    )
  };
  Ok(ElfDynamic {
    needed: captures(r"\(NEEDED\)\s+Shared library: \[(.*?)\]")?,
    soname: captures(r"\(SONAME\)\s+Library soname: \[(.*?)\]")?
      .into_iter()
      .next(),
    runpaths: captures(r"\((?:RPATH|RUNPATH)\)\s+Library (?:rpath|runpath): \[(.*?)\]")?,
  })
}

fn parse_ldconfig(output: &str) -> Result<BTreeMap<String, PathBuf>> {
  let entries = Regex::new(r"(?m)^\s*(\S+)\s+\(([^)]*)\)\s+=>\s+(/.*)$")?;
  let architecture = Regex::new(r"x86[-_]64")?;
  let optimized = Regex::new(r"(?:glibc-hwcaps|/x86-64-v[2-4]/)")?;
  let mut libraries = BTreeMap::new();
  for entry in entries.captures_iter(output) {
    if architecture.is_match(&entry[2]) && !optimized.is_match(&entry[3]) {
      libraries
        .entry(entry[1].to_owned())
        .or_insert_with(|| PathBuf::from(entry[3].trim()));
    }
  }
  Ok(libraries)
}

fn parse_loader_paths(output: &str) -> Result<BTreeMap<String, PathBuf>> {
  let mut libraries = BTreeMap::new();
  for line in output.lines() {
    let Some((name, resolved)) = line.trim().split_once(" => ") else {
      continue;
    };
    let path = Path::new(resolved.split_whitespace().next().unwrap_or(""));
    ensure!(
      valid_soname(name) && path.is_absolute(),
      "Unresolved or invalid loader dependency: {line}"
    );
    libraries.insert(name.to_owned(), path.to_owned());
  }
  Ok(libraries)
}

fn valid_soname(name: &str) -> bool {
  // A filename-only NEEDED is mandatory: no build-prefix or traversal dependencies.
  let Some((base, versions)) = name.rsplit_once(".so") else {
    return false;
  };
  !base.is_empty()
    && base
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || b"_.+-".contains(&byte))
    && (versions.is_empty()
      || versions.strip_prefix('.').is_some_and(|rest| {
        rest.split('.').all(|part| {
          !part.is_empty()
            && part
              .bytes()
              .all(|byte| byte.is_ascii_alphanumeric() || b"_+-".contains(&byte))
        })
      }))
}

fn system_reason(name: &str) -> Option<&'static str> {
  match name {
    "ld-linux-x86-64.so.2"
    | "libanl.so.1"
    | "libBrokenLocale.so.1"
    | "libc.so.6"
    | "libdl.so.2"
    | "libm.so.6"
    | "libmvec.so.1"
    | "libnsl.so.1"
    | "libnss_compat.so.2"
    | "libnss_dns.so.2"
    | "libnss_files.so.2"
    | "libnss_hesiod.so.2"
    | "libpthread.so.0"
    | "libresolv.so.2"
    | "librt.so.1"
    | "libthread_db.so.1"
    | "libutil.so.1" => Some("glibc"),
    // The host loader must retain its normal dynamic ICD/driver discovery policy.
    "libvulkan.so.1" => Some("vulkan-loader"),
    _ => None,
  }
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum PackageManager {
  Dpkg,
  Pacman,
}
impl PackageManager {
  fn executable(self) -> &'static str {
    match self {
      Self::Dpkg => "/usr/bin/dpkg-query",
      Self::Pacman => "/usr/bin/pacman",
    }
  }
  fn name(self) -> &'static str {
    match self {
      Self::Dpkg => "dpkg",
      Self::Pacman => "pacman",
    }
  }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackageOrigin {
  manager: PackageManager,
  package_name: String,
  version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NormalizedDependency {
  name: String,
  original_path: String,
  origin: PackageOrigin,
}

struct RuntimeLibrary {
  name: String,
  origin: Option<PackageOrigin>,
  notices: Vec<String>,
}

fn run(
  program: &str,
  args: &[&str],
  environment: &BTreeMap<String, String>,
  capture: bool,
  allow_failure: bool,
) -> Result<Output> {
  process::run_with(
    process::command(program)
      .args(args)
      .envs(environment)
      .env("LC_ALL", "C"),
    process::RunOptions {
      capture,
      allow_failure,
      ..Default::default()
    },
  )
  .with_context(|| format!("Linux runtime command {program} {}", args.join(" ")))
}

fn captured(program: &str, args: &[&str]) -> Result<String> {
  Ok(mpv::text(&run(
    program,
    args,
    &BTreeMap::new(),
    true,
    false,
  )?))
}

fn read_elf(library: &Path) -> Result<ElfDynamic> {
  let output = process::capture(
    process::command("readelf")
      .args(["--dynamic", "--wide"])
      .arg(library)
      .env("LC_ALL", "C"),
  )?;
  parse_elf(&mpv::text(&output))
}

fn list_files(root: &Path) -> Result<Vec<PathBuf>> {
  fn visit(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root)? {
      let entry = entry?;
      let kind = entry.file_type()?;
      if kind.is_dir() {
        visit(&entry.path(), files)?;
      } else if kind.is_file() {
        files.push(entry.path());
      } else {
        anyhow::bail!(
          "Unexpected non-regular runtime payload entry: {}",
          entry.path().display()
        );
      }
    }
    Ok(())
  }
  let mut files = Vec::new();
  visit(root, &mut files)?;
  files.sort();
  Ok(files)
}

fn detect_package_manager() -> Result<PackageManager> {
  if Path::new("/usr/bin/dpkg-query").is_file() {
    return Ok(PackageManager::Dpkg);
  }
  if Path::new("/usr/bin/pacman").is_file() {
    return Ok(PackageManager::Pacman);
  }
  anyhow::bail!("Bundled system-library origins require dpkg-query or pacman.")
}

fn owning_package(manager: PackageManager, library: &Path) -> Result<PackageOrigin> {
  let canonical = fs::canonicalize(library)?;
  let mut name = None;
  let pacman_name = Regex::new(r"(?i)^[a-z0-9][a-z0-9@._+-]*$")?;
  let dpkg_name = Regex::new(r"(?i)^([a-z0-9][a-z0-9+.-]+)(?::[a-z0-9]+)?:\s+/")?;
  for candidate in BTreeSet::from([library.to_owned(), canonical]) {
    let candidate = candidate.to_string_lossy();
    let args = match manager {
      PackageManager::Dpkg => vec!["--search", candidate.as_ref()],
      PackageManager::Pacman => vec!["-Qoq", candidate.as_ref()],
    };
    let output = run(manager.executable(), &args, &BTreeMap::new(), true, true)?;
    if !output.status.success() {
      continue;
    }
    let text = mpv::text(&output);
    name = match manager {
      PackageManager::Pacman => pacman_name
        .is_match(text.trim())
        .then(|| text.trim().to_owned()),
      PackageManager::Dpkg => text.lines().find_map(|line| {
        dpkg_name
          .captures(line)
          .map(|capture| capture[1].to_owned())
      }),
    };
    if name.is_some() {
      break;
    }
  }
  let package_name = name.with_context(|| {
    format!(
      "Cannot identify distribution package and license origin for {}.",
      library.display()
    )
  })?;
  let args = match manager {
    PackageManager::Dpkg => vec!["--show", "--showformat=${Version}", &package_name],
    PackageManager::Pacman => vec!["-Q", &package_name],
  };
  let text = captured(manager.executable(), &args)?;
  let version = match manager {
    PackageManager::Dpkg => text.trim(),
    PackageManager::Pacman => text.trim().strip_prefix(&package_name).unwrap_or("").trim(),
  };
  ensure!(
    !version.is_empty(),
    "No version recorded for {package_name}."
  );
  Ok(PackageOrigin {
    manager,
    version: version.to_owned(),
    package_name,
  })
}

fn parse_pacman_licenses(metadata: &str) -> Vec<String> {
  let mut expression = String::new();
  let mut found = false;
  for line in metadata.lines() {
    if !found {
      if let Some((key, value)) = line.split_once(':') {
        if key.trim_end() == "Licenses" {
          expression.push_str(value);
          found = true;
        }
      }
    } else if line.starts_with([' ', '\t']) {
      expression.push(' ');
      expression.push_str(line);
    } else {
      break;
    }
  }
  let mut seen = BTreeSet::new();
  expression
    .split(['(', ')', ' ', '\t', '\r', '\n'])
    .filter(|token| {
      !token.is_empty()
        && token.as_bytes()[0].is_ascii_alphanumeric()
        && token
          .bytes()
          .all(|byte| byte.is_ascii_alphanumeric() || b".+-".contains(&byte))
        && !["AND", "OR", "WITH", "None"].contains(token)
    })
    .filter(|token| seen.insert((*token).to_owned()))
    .map(str::to_owned)
    .collect()
}

fn copy_package_notice_files(
  origin: &PackageOrigin,
  candidates: &[PathBuf],
  prefix: &Path,
  notice_root: &Path,
) -> Result<Vec<String>> {
  let mut sources: BTreeSet<_> = candidates.iter().cloned().collect();
  if matches!(origin.manager, PackageManager::Dpkg) {
    sources.insert(prefix.join(&origin.package_name).join("copyright"));
    sources.insert(prefix.join(&origin.package_name).join("copyright.gz"));
  }
  let mut notices = Vec::new();
  for source in sources {
    // metadata/copy follow doc-directory symlinks that dpkg does not expand in --listfiles.
    if !source.is_file() {
      continue;
    }
    let relative_source = source
      .strip_prefix(prefix)
      .context("Package notice outside license prefix")?;
    ensure!(
      !relative_source
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir)),
      "Package notice escapes license prefix"
    );
    let relative = Path::new(origin.manager.name())
      .join(&origin.package_name)
      .join(relative_source);
    let target = notice_root.join(&relative);
    fs::create_dir_all(target.parent().context("Notice has no parent")?)?;
    fs::copy(&source, &target)?;
    notices.push(relative.to_string_lossy().replace('\\', "/"));
  }
  ensure!(
    !notices.is_empty(),
    "No {} license notice found for {}.",
    origin.manager.name(),
    origin.package_name
  );
  Ok(notices)
}

fn copy_package_notices(origin: &PackageOrigin, notice_root: &Path) -> Result<Vec<String>> {
  let args = match origin.manager {
    PackageManager::Dpkg => vec!["--listfiles", &origin.package_name],
    PackageManager::Pacman => vec!["-Ql", &origin.package_name],
  };
  let text = captured(origin.manager.executable(), &args)?;
  let prefix = Path::new(match origin.manager {
    PackageManager::Dpkg => "/usr/share/doc/",
    PackageManager::Pacman => "/usr/share/licenses/",
  });
  let mut candidates = Vec::new();
  for line in text.lines() {
    let file = match origin.manager {
      PackageManager::Dpkg => line.trim(),
      PackageManager::Pacman => line
        .find(char::is_whitespace)
        .map_or("", |position| line[position..].trim()),
    };
    let path = Path::new(file);
    if !path.starts_with(prefix) || file.ends_with('/') {
      continue;
    }
    if matches!(origin.manager, PackageManager::Pacman)
      || file.ends_with("/copyright")
      || file.ends_with("/copyright.gz")
    {
      candidates.push(path.to_owned());
    }
  }
  if matches!(origin.manager, PackageManager::Pacman) {
    let metadata = captured(origin.manager.executable(), &["-Qi", &origin.package_name])?;
    for id in parse_pacman_licenses(&metadata) {
      candidates.push(prefix.join("spdx").join(format!("{id}.txt")));
      candidates.push(prefix.join("spdx/exceptions").join(format!("{id}.txt")));
    }
  }
  copy_package_notice_files(origin, &candidates, prefix, notice_root)
}

fn copy_source_notices(checkout: &Path, notice_root: &Path) -> Result<Vec<String>> {
  let mut notices = Vec::new();
  for source in [
    "LICENSE",
    "3rdparty/Vulkan-Headers/LICENSE.md",
    "3rdparty/Vulkan-Headers/LICENSES/Apache-2.0.txt",
    "3rdparty/Vulkan-Headers/LICENSES/MIT.txt",
    "3rdparty/fast_float/LICENSE-APACHE",
    "3rdparty/fast_float/LICENSE-BOOST",
    "3rdparty/fast_float/LICENSE-MIT",
  ] {
    let relative = format!("libplacebo/{source}");
    let destination = notice_root.join(&relative);
    fs::create_dir_all(
      destination
        .parent()
        .context("Source notice has no parent")?,
    )?;
    fs::copy(checkout.join(source), destination)?;
    notices.push(relative);
  }
  Ok(notices)
}

fn stage_closure(
  prefix: &Path,
  checkout: &Path,
  payload: &Path,
  notices: &Path,
  manager: PackageManager,
  soname: &str,
  bundle_mpv: bool,
) -> Result<(Vec<RuntimeLibrary>, BTreeMap<String, &'static str>)> {
  let mut system_libraries = parse_ldconfig(&captured("ldconfig", &["-p"])?)?;
  let private_library = prefix.join("lib").join(soname);
  ensure!(
    read_elf(&private_library)?.soname.as_deref() == Some(soname),
    "Expected libplacebo SONAME {soname}."
  );
  fs::copy(&private_library, payload.join(soname))?;
  let mut libraries = BTreeMap::from([(
    soname.to_owned(),
    RuntimeLibrary {
      name: soname.to_owned(),
      origin: None,
      notices: copy_source_notices(checkout, notices)?,
    },
  )]);
  let mut mpv_notices = Vec::new();
  fs::create_dir_all(notices.join("mpv"))?;
  for relative in mpv::NOTICES {
    let name = Path::new(relative)
      .file_name()
      .context("Missing mpv notice filename")?;
    let notice = Path::new("mpv").join(name);
    fs::copy(mpv::artifact_root().join(relative), notices.join(&notice))?;
    mpv_notices.push(notice.to_string_lossy().into_owned());
  }
  libraries.insert(
    "libmpv.so".to_owned(),
    RuntimeLibrary {
      name: "libmpv.so".to_owned(),
      origin: None,
      notices: mpv_notices,
    },
  );
  let mut retained = BTreeMap::new();
  let mut notice_cache = BTreeMap::<String, Vec<String>>::new();
  // Debian/Arch retain their distro mpv dependencies. AppImage additionally
  // stages the complete mpv closure so its FFmpeg ABI and notices travel with it.
  let mut queue = VecDeque::from([private_library]);
  if bundle_mpv {
    queue.push_back(payload.join("libmpv.so"));
    // PulseAudio and Samba keep private SONAMEs outside ldconfig's index.
    // Resolve the actual native closure before replacing package RUNPATHs.
    let loader = process::capture(
      process::command("ldd")
        .arg(payload.join("libmpv.so"))
        .env("LD_LIBRARY_PATH", prefix.join("lib"))
        .env("LC_ALL", "C"),
    )?;
    system_libraries.extend(parse_loader_paths(&mpv::text(&loader))?);
  }
  while let Some(importer) = queue.pop_front() {
    for name in read_elf(&importer)?.needed {
      ensure!(
        valid_soname(&name),
        "{} has invalid NEEDED entry {name}.",
        importer.display()
      );
      if let Some(reason) = system_reason(&name) {
        retained.insert(name, reason);
        continue;
      }
      if libraries.contains_key(&name) {
        continue;
      }
      ensure!(
        !prefix.join("lib").join(&name).is_file(),
        "Unexpected additional private library {name}; stage its pinned provenance explicitly."
      );
      let source = system_libraries
        .get(&name)
        .with_context(|| format!("{} requires unavailable {name}.", importer.display()))?;
      let origin = owning_package(manager, source)?;
      let cache_key = format!("{}:{}", origin.manager.name(), origin.package_name);
      let package_notices = if let Some(value) = notice_cache.get(&cache_key) {
        value.clone()
      } else {
        let value = copy_package_notices(&origin, notices)?;
        notice_cache.insert(cache_key, value.clone());
        value
      };
      fs::copy(source, payload.join(&name))?;
      libraries.insert(
        name.clone(),
        RuntimeLibrary {
          name,
          origin: Some(origin),
          notices: package_notices,
        },
      );
      queue.push_back(source.clone());
    }
  }
  Ok((libraries.into_values().collect(), retained))
}

fn update_manifest(
  root: &Path,
  pin: &Value,
  libraries: &[RuntimeLibrary],
  retained: &BTreeMap<String, &'static str>,
  tools: &BTreeMap<String, String>,
  normalized: &[NormalizedDependency],
) -> Result<()> {
  let manifest_path = root.join("manifest.json");
  let mut manifest: Value = util::read_json(&manifest_path)?;
  ensure!(manifest.is_object(), "Invalid mpv manifest object");
  let artifacts = manifest["artifacts"]
    .as_object_mut()
    .context("Invalid mpv manifest artifacts")?;
  ensure!(
    artifacts.values().all(|value| value["sha256"].is_string()),
    "Invalid artifact hash record"
  );
  artifacts.retain(|key, _| {
    !key.starts_with(&format!("{LIBRARY_RELATIVE}/"))
      && !key.starts_with(&format!("{NOTICE_RELATIVE}/"))
  });
  let mut hashes = BTreeMap::new();
  for file in list_files(&root.join(LIBRARY_RELATIVE))?
    .into_iter()
    .chain(list_files(&root.join(NOTICE_RELATIVE))?)
  {
    let relative = file
      .strip_prefix(root)?
      .to_string_lossy()
      .replace('\\', "/");
    let hash = util::sha256_file(&file)?;
    artifacts.insert(relative.clone(), json!({"sha256": hash}));
    hashes.insert(relative, hash);
  }
  let mut records = Vec::new();
  for library in libraries {
    let relative = format!("{LIBRARY_RELATIVE}/{}", library.name);
    let hash = hashes
      .get(&relative)
      .with_context(|| format!("Missing patched artifact hash for {relative}"))?;
    let origin = if let Some(origin) = &library.origin {
      json!({"type": "distribution-package", "manager": origin.manager, "packageName": origin.package_name, "version": origin.version})
    } else {
      json!({"type": "pinned-source"})
    };
    records.push(json!({"name": library.name, "sha256": hash, "origin": origin, "notices": library.notices.iter().map(|notice| format!("{NOTICE_RELATIVE}/{notice}")).collect::<Vec<_>>()}));
  }
  records.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
  manifest["linuxRuntime"] = json!({"schemaVersion": 1, "libplacebo": pin, "runpath": ORIGIN, "tools": tools, "libraries": records, "normalizedSystemDependencies": normalized, "retainedSystemLibraries": retained.iter().map(|(name, reason)| json!({"name": name, "reason": reason})).collect::<Vec<_>>(), "loaderCheck": {"directDependency": pin["soname"], "resolvedFrom": ORIGIN}});
  util::write_json(&manifest_path, &manifest)
}

pub fn build(source: Option<&Path>) -> Result<()> {
  ensure!(
    cfg!(target_os = "linux") && cfg!(target_arch = "x86_64"),
    "The private Linux runtime requires native x86_64 Linux."
  );
  let root = mpv::artifact_root();
  fs::create_dir_all(&root)?;
  // $ORIGIN resolution is canonical; crate::root() may contain an xtask/.. component.
  let root = fs::canonicalize(root)?;
  let work = tempfile::Builder::new()
    .prefix("linux-runtime-")
    .tempdir_in(&root)?
    .keep();
  let pin: Value =
    serde_json::from_str(include_str!("../../packaging/linux/libplacebo-source.json"))?;
  let revision = mpv::string(&pin, "revision")?;
  let soname = mpv::string(&pin, "soname")?;
  let mut tools = BTreeMap::new();
  for executable in [
    "git",
    "meson",
    "ninja",
    "pkg-config",
    "cc",
    "c++",
    "python3",
    "readelf",
    "patchelf",
    "ldconfig",
    "ldd",
  ] {
    let output = run(executable, &["--version"], &BTreeMap::new(), true, false)?;
    let stdout = mpv::text(&output);
    tools.insert(
      executable.to_owned(),
      if stdout.trim().is_empty() {
        String::from_utf8_lossy(&output.stderr).trim().to_owned()
      } else {
        stdout.trim().lines().next().unwrap_or("").to_owned()
      },
    );
  }
  let manager = detect_package_manager()?;
  let checkout = work.join("libplacebo-source");
  process::run(
    process::command("git")
      .args([
        "clone",
        "--filter=blob:none",
        "--no-checkout",
        mpv::string(&pin, "remote")?,
      ])
      .arg(&checkout)
      .env("LC_ALL", "C"),
  )?;
  process::run(
    process::command("git")
      .arg("-C")
      .arg(&checkout)
      .args(["fetch", "--depth=1", "origin", revision])
      .env("LC_ALL", "C"),
  )?;
  process::run(
    process::command("git")
      .arg("-C")
      .arg(&checkout)
      .args(["checkout", "--detach", revision])
      .env("LC_ALL", "C"),
  )?;
  process::run(
    process::command("git")
      .arg("-C")
      .arg(&checkout)
      .args(["submodule", "update", "--init", "--recursive"])
      .env("LC_ALL", "C"),
  )?;
  mpv::verify_source(&checkout, revision, true)?;
  let build = work.join("libplacebo-build");
  let prefix = work.join("prefix");
  process::run(
    process::command("meson")
      .arg("setup")
      .arg(&build)
      .arg(&checkout)
      .arg(format!("--prefix={}", prefix.display()))
      .arg("--libdir=lib")
      .args(mpv::strings(&pin, "mesonOptions")?)
      .env("LC_ALL", "C"),
  )?;
  process::run(
    process::command("meson")
      .args(["compile", "-C"])
      .arg(&build)
      .env("LC_ALL", "C"),
  )?;
  process::run(
    process::command("meson")
      .args(["install", "-C"])
      .arg(&build)
      .env("LC_ALL", "C"),
  )?;
  mpv::verify_source(&checkout, revision, true)?;
  let mut pkg_path = prefix.join("lib/pkgconfig").to_string_lossy().into_owned();
  if let Ok(existing) = std::env::var("PKG_CONFIG_PATH") {
    if !existing.is_empty() {
      pkg_path.push(':');
      pkg_path.push_str(&existing);
    }
  }
  let environment = BTreeMap::from([
    ("PKG_CONFIG_PATH".to_owned(), pkg_path),
    (
      "LD_LIBRARY_PATH".to_owned(),
      prefix.join("lib").to_string_lossy().into_owned(),
    ),
  ]);
  let version = mpv::text(&run(
    "pkg-config",
    &["--modversion", "libplacebo"],
    &environment,
    true,
    false,
  )?);
  ensure!(
    version.trim() == mpv::string(&pin, "version")?,
    "Expected libplacebo {}; pkg-config selected {}.",
    pin["version"],
    version.trim()
  );
  mpv::build(source, &environment)?;
  let payload = work.join("payload");
  let notice_root = work.join("notices");
  fs::create_dir(&payload)?;
  fs::create_dir(&notice_root)?;
  fs::copy(
    root.join(LIBRARY_RELATIVE).join("libmpv.so"),
    payload.join("libmpv.so"),
  )?;
  let bundle_mpv = std::env::var("JELLYPILOT_APPIMAGE_BUILD").as_deref() == Ok("1");
  let (libraries, retained) = stage_closure(
    &prefix,
    &checkout,
    &payload,
    &notice_root,
    manager,
    soname,
    bundle_mpv,
  )?;
  let mut normalized = Vec::new();
  let system_path = Regex::new(r"^(?:/usr)?/lib(?:64)?/")?;
  for library in list_files(&payload)? {
    for dependency in read_elf(&library)?.needed {
      if valid_soname(&dependency) {
        continue;
      }
      let name = Path::new(&dependency)
        .file_name()
        .context("Absolute dependency has no filename")?
        .to_string_lossy()
        .into_owned();
      ensure!(
        system_path.is_match(&dependency) && valid_soname(&name),
        "Unsupported absolute dependency: {dependency}."
      );
      let origin = owning_package(manager, Path::new(&dependency))?;
      process::run(
        process::command("patchelf")
          .args(["--replace-needed", &dependency, &name])
          .arg(&library)
          .env("LC_ALL", "C"),
      )?;
      normalized.push(NormalizedDependency {
        name,
        original_path: dependency,
        origin,
      });
    }
    process::run(
      process::command("patchelf")
        .args(["--set-rpath", ORIGIN])
        .arg(&library)
        .env("LC_ALL", "C"),
    )?;
  }
  let library_directory = root.join(LIBRARY_RELATIVE);
  let notice_directory = root.join(NOTICE_RELATIVE);
  if library_directory.exists() {
    fs::remove_dir_all(&library_directory)?;
  }
  fs::rename(&payload, &library_directory)?;
  if notice_directory.exists() {
    fs::remove_dir_all(&notice_directory)?;
  }
  fs::create_dir_all(
    notice_directory
      .parent()
      .context("Missing license parent")?,
  )?;
  fs::rename(&notice_root, &notice_directory)?;
  for library in list_files(&library_directory)? {
    validate_dynamic(&library, &read_elf(&library)?)?;
  }
  // Unset caller overrides so loader verification proves the staged $ORIGIN paths.
  let loader = process::capture(
    process::command("ldd")
      .arg(library_directory.join("libmpv.so"))
      .env_remove("LD_LIBRARY_PATH")
      .env("LC_ALL", "C"),
  )?;
  let loader_stdout = mpv::text(&loader);
  let loader_stderr = String::from_utf8_lossy(&loader.stderr);
  validate_loader(&loader_stdout, &loader_stderr, soname, &library_directory)?;
  if bundle_mpv {
    for library in &libraries {
      if library.name != "libmpv.so" {
        validate_loader(
          &loader_stdout,
          &loader_stderr,
          &library.name,
          &library_directory,
        )?;
      }
    }
  }
  update_manifest(&root, &pin, &libraries, &retained, &tools, &normalized)?;
  println!(
    "Linux mpv runtime staged with private libplacebo {} at {}.",
    pin["version"],
    library_directory.display()
  );
  Ok(())
}

fn validate_dynamic(library: &Path, dynamic: &ElfDynamic) -> Result<()> {
  ensure!(
    dynamic.runpaths == [ORIGIN],
    "{} must have only {ORIGIN}; found {:?}.",
    library.display(),
    dynamic.runpaths
  );
  for name in &dynamic.needed {
    ensure!(
      valid_soname(name),
      "{} has invalid NEEDED {name}.",
      library.display()
    );
  }
  Ok(())
}

fn validate_loader(stdout: &str, stderr: &str, soname: &str, directory: &Path) -> Result<()> {
  ensure!(
    !stdout.contains("not found") && !stderr.contains("not found"),
    "Loader has unresolved dependencies:\n{stdout}\n{stderr}"
  );
  let expected = format!("{soname} => {}", directory.join(soname).display());
  ensure!(
    stdout.contains(&expected),
    "Expected {expected}; found:\n{stdout}"
  );
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn elf_metadata_preserves_all_dependencies_and_lookup_paths() {
    assert_eq!(parse_elf("(NEEDED) Shared library: [libplacebo.so.387]\n(NEEDED) Shared library: [libvulkan.so.1]\n(RPATH) Library rpath: [/tmp/private/lib]\n(RUNPATH) Library runpath: [$ORIGIN]\n(SONAME) Library soname: [libmpv.so]").unwrap(), ElfDynamic { needed: vec!["libplacebo.so.387".into(), "libvulkan.so.1".into()], soname: Some("libmpv.so".into()), runpaths: vec!["/tmp/private/lib".into(), "$ORIGIN".into()] });
  }

  #[test]
  fn loader_index_uses_first_generic_native_entry_not_hwcaps_or_foreign_architecture() {
    let index = parse_ldconfig(" libcodec.so.1 (libc6,x86-64) => /opt/first/libcodec.so.1\n libcodec.so.1 (libc6,x86-64) => /opt/duplicate/libcodec.so.1\n libforeign.so (libc6) => /lib/i386/libforeign.so\n libshaderc.so.1 (libc6,x86-64) => /usr/lib/glibc-hwcaps/x86-64-v3/libshaderc.so.1\n libshaderc.so.1 (libc6,x86-64) => /usr/lib/libshaderc.so.1\n").unwrap();
    assert_eq!(
      index,
      BTreeMap::from([
        (
          "libcodec.so.1".to_owned(),
          PathBuf::from("/opt/first/libcodec.so.1")
        ),
        (
          "libshaderc.so.1".to_owned(),
          PathBuf::from("/usr/lib/libshaderc.so.1")
        )
      ])
    );
  }

  #[test]
  fn loader_paths_include_private_package_libraries_and_reject_unresolved_dependencies() {
    let output = "linux-vdso.so.1 (0x1234)\n\
      libpulsecommon-15.99.so => /usr/lib/x86_64-linux-gnu/pulseaudio/libpulsecommon-15.99.so (0x2345)\n\
      /lib64/ld-linux-x86-64.so.2 (0x3456)\n";
    assert_eq!(
      parse_loader_paths(output).unwrap(),
      BTreeMap::from([(
        "libpulsecommon-15.99.so".to_owned(),
        PathBuf::from("/usr/lib/x86_64-linux-gnu/pulseaudio/libpulsecommon-15.99.so")
      )])
    );
    assert!(parse_loader_paths("libcodec.so.1 => not found").is_err());
    assert!(parse_loader_paths("../libcodec.so.1 => /usr/lib/libcodec.so.1").is_err());
  }

  #[test]
  fn wrapped_spdx_expressions_collect_texts_and_exceptions_without_path_injection() {
    assert_eq!(parse_pacman_licenses("Name : libgcc\nLicenses : (GPL-3.0-or-later WITH GCC-exception-3.1)\n                  AND GFDL-1.3-or-later OR GPL-3.0-or-later\nGroups : None\n"), ["GPL-3.0-or-later", "GCC-exception-3.1", "GFDL-1.3-or-later"]);
    assert!(parse_pacman_licenses("Licenses : ../../private\n").is_empty());
  }

  #[cfg(unix)]
  #[test]
  fn dpkg_notices_follow_unlisted_doc_symlinks_and_keep_compressed_bytes() {
    for filename in ["copyright", "copyright.gz"] {
      let root = tempfile::tempdir().unwrap();
      let prefix = root.path().join("doc");
      let notice_root = root.path().join("notices");
      fs::create_dir_all(prefix.join("gcc-base")).unwrap();
      let bytes = b"\x1f\x8bcompressed-or-plain-notice";
      fs::write(prefix.join("gcc-base").join(filename), bytes).unwrap();
      std::os::unix::fs::symlink("gcc-base", prefix.join("libstdc++6")).unwrap();
      let origin = PackageOrigin {
        manager: PackageManager::Dpkg,
        package_name: "libstdc++6".into(),
        version: "16".into(),
      };
      let notices = copy_package_notice_files(&origin, &[], &prefix, &notice_root).unwrap();
      assert_eq!(notices, [format!("dpkg/libstdc++6/libstdc++6/{filename}")]);
      assert_eq!(fs::read(notice_root.join(&notices[0])).unwrap(), bytes);
    }
  }

  #[test]
  fn notice_collection_deduplicates_and_rejects_missing_or_outside_sources() {
    let root = tempfile::tempdir().unwrap();
    let prefix = root.path().join("doc");
    let destination = root.path().join("notices");
    let origin = PackageOrigin {
      manager: PackageManager::Dpkg,
      package_name: "package".into(),
      version: "1".into(),
    };
    fs::create_dir_all(prefix.join("package/additional")).unwrap();
    assert!(copy_package_notice_files(&origin, &[], &prefix, &destination).is_err());
    let standard = prefix.join("package/copyright");
    let extra = prefix.join("package/additional/copyright");
    fs::write(&standard, "main").unwrap();
    fs::write(&extra, "extra").unwrap();
    let notices = copy_package_notice_files(
      &origin,
      &[standard.clone(), standard, extra],
      &prefix,
      &destination,
    )
    .unwrap();
    assert_eq!(notices.len(), 2);
    let outside = root.path().join("outside");
    fs::write(&outside, "outside").unwrap();
    assert!(copy_package_notice_files(&origin, &[outside], &prefix, &destination).is_err());
  }

  #[test]
  fn runtime_verification_rejects_build_prefixes_extra_paths_and_unresolved_loads() {
    let path = Path::new("libmpv.so");
    for (needed, runpaths) in [
      (vec!["/tmp/prefix/libcodec.so".into()], vec![ORIGIN.into()]),
      (
        vec!["libcodec.so".into()],
        vec![ORIGIN.into(), "/tmp".into()],
      ),
    ] {
      assert!(validate_dynamic(
        path,
        &ElfDynamic {
          needed,
          runpaths,
          soname: None
        }
      )
      .is_err());
    }
    let directory = Path::new("/staged/lib");
    assert!(validate_loader(
      "libplacebo.so.387 => /usr/lib/libplacebo.so.387",
      "",
      "libplacebo.so.387",
      directory
    )
    .is_err());
    assert!(validate_loader(
      "libplacebo.so.387 => /staged/lib/libplacebo.so.387\nlibcodec.so => not found",
      "",
      "libplacebo.so.387",
      directory
    )
    .is_err());
    assert!(validate_loader(
      "libplacebo.so.387 => /staged/lib/libplacebo.so.387",
      "",
      "libplacebo.so.387",
      directory
    )
    .is_ok());
  }
}

#[cfg(test)]
mod manifest_tests {
  use super::*;

  #[test]
  fn manifest_rehashes_patched_payload_and_removes_stale_closure_records() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    fs::create_dir_all(root.join(LIBRARY_RELATIVE)).unwrap();
    fs::create_dir_all(root.join(NOTICE_RELATIVE).join("libplacebo")).unwrap();
    fs::write(root.join(LIBRARY_RELATIVE).join("libmpv.so"), "patched mpv").unwrap();
    fs::write(
      root.join(LIBRARY_RELATIVE).join("libplacebo.so.387"),
      "patched placebo",
    )
    .unwrap();
    fs::write(
      root.join(NOTICE_RELATIVE).join("libplacebo/LICENSE"),
      "license",
    )
    .unwrap();
    util::write_json(
      &root.join("manifest.json"),
      &json!({
        "schemaVersion": 1,
        "source": {"revision": "mpv-pinned"},
        "artifacts": {
          "lib/jellypilot/libmpv.so": {"sha256": "pre-patch"},
          "lib/jellypilot/stale.so": {"sha256": "stale"},
          "share/jellypilot/licenses/linux-runtime/old": {"sha256": "stale"},
          "share/jellypilot/mpv-baseline.conf": {"sha256": "baseline-preserved"}
        }
      }),
    )
    .unwrap();
    let pin = json!({"soname": "libplacebo.so.387", "revision": "placebo-pinned"});
    let libraries = [RuntimeLibrary {
      name: "libplacebo.so.387".into(),
      origin: None,
      notices: vec!["libplacebo/LICENSE".into()],
    }];
    update_manifest(
      root,
      &pin,
      &libraries,
      &BTreeMap::from([("libvulkan.so.1".into(), "vulkan-loader")]),
      &BTreeMap::new(),
      &[],
    )
    .unwrap();
    let manifest: Value = util::read_json(&root.join("manifest.json")).unwrap();
    assert_eq!(
      manifest["artifacts"]["lib/jellypilot/libmpv.so"]["sha256"],
      util::sha256_file(&root.join(LIBRARY_RELATIVE).join("libmpv.so")).unwrap()
    );
    assert!(manifest["artifacts"]
      .get("lib/jellypilot/stale.so")
      .is_none());
    assert!(manifest["artifacts"]
      .get("share/jellypilot/licenses/linux-runtime/old")
      .is_none());
    assert_eq!(
      manifest["artifacts"]["share/jellypilot/mpv-baseline.conf"]["sha256"],
      "baseline-preserved"
    );
    assert_eq!(
      manifest["linuxRuntime"]["libraries"][0]["sha256"],
      util::sha256_file(&root.join(LIBRARY_RELATIVE).join("libplacebo.so.387")).unwrap()
    );
    assert_eq!(
      manifest["linuxRuntime"]["libraries"][0]["notices"][0],
      format!("{NOTICE_RELATIVE}/libplacebo/LICENSE")
    );
  }
}

#[cfg(all(test, unix))]
mod broken_notice_tests {
  use super::*;

  #[test]
  fn broken_package_documentation_link_does_not_collect_unrelated_licenses() {
    let temporary = tempfile::tempdir().unwrap();
    let prefix = temporary.path().join("doc");
    fs::create_dir_all(prefix.join("unrelated")).unwrap();
    fs::write(prefix.join("unrelated/copyright"), "wrong package").unwrap();
    std::os::unix::fs::symlink("missing", prefix.join("package")).unwrap();
    let destination = temporary.path().join("notices");
    let origin = PackageOrigin {
      manager: PackageManager::Dpkg,
      package_name: "package".into(),
      version: "1".into(),
    };
    assert!(copy_package_notice_files(&origin, &[], &prefix, &destination).is_err());
    assert!(!destination.exists());
  }
}
