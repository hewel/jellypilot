use std::{
  collections::{BTreeMap, BTreeSet, VecDeque},
  fs,
  path::{Path, PathBuf},
};

use anyhow::{bail, ensure, Context, Result};
use regex::Regex;
use serde::Serialize;

use crate::{process, util};

#[derive(Debug, Serialize)]
pub struct BundledLibrary {
  pub name: String,
  pub sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyManifest {
  pub bundled: Vec<BundledLibrary>,
  pub system_dependencies: Vec<String>,
}

pub fn parse_imports(library: &Path, output: &str) -> Result<Vec<String>> {
  ensure!(
    Regex::new(r"(?m)\bfile format pei-x86-64\s*$")?.is_match(output),
    "{}: expected a PE x86-64 DLL.",
    library.display()
  );
  let name_pattern = Regex::new(r"(?i)^[a-z\d_.+ -]+\.dll$")?;
  let mut imports = Vec::new();
  let mut seen = BTreeSet::new();
  for line in output.lines() {
    let Some(name) = line.trim_start().strip_prefix("DLL Name:") else {
      continue;
    };
    let name = name.trim();
    ensure!(
      name_pattern.is_match(name),
      "{}: invalid imported DLL name {name:?}.",
      library.display()
    );
    let name = name.to_ascii_lowercase();
    if seen.insert(name.clone()) {
      imports.push(name);
    }
  }
  Ok(imports)
}

fn dll_index(directory: &Path) -> Result<BTreeMap<String, PathBuf>> {
  let mut index = BTreeMap::new();
  for entry in
    fs::read_dir(directory).with_context(|| format!("Cannot list {}", directory.display()))?
  {
    let entry = entry?;
    let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
    if !entry.file_type()?.is_file() || !name.ends_with(".dll") {
      continue;
    }
    ensure!(
      index.insert(name.clone(), entry.path()).is_none(),
      "Ambiguous case-insensitive DLL name {name} in {}.",
      directory.display()
    );
  }
  Ok(index)
}

// FFmpeg's export tables exceed bounded process capture; retain only inspection records.
const PE_IMPORT_FILTER: &str = "import subprocess, sys\nwith subprocess.Popen([sys.argv[1], '-p', sys.argv[2]], stdout=subprocess.PIPE, text=True, encoding='utf-8', errors='replace') as child:\n    for line in child.stdout:\n        if 'file format ' in line or line.lstrip().startswith('DLL Name:'):\n            sys.stdout.write(line)\n    sys.exit(child.wait())\n";

pub fn read_pe_dump(runtime: &Path, library: &Path) -> Result<String> {
  let output = process::capture(
    process::command(runtime.join("python.exe"))
      .args(["-c", PE_IMPORT_FILTER])
      .arg(runtime.join("objdump.exe"))
      .arg(library)
      .env("LC_ALL", "C"),
  )
  .with_context(|| format!("Cannot inspect {}", library.display()))?;
  Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn stage(
  library: &Path,
  runtime: &Path,
  destination: &Path,
  system: &Path,
) -> Result<DependencyManifest> {
  stage_with(library, runtime, destination, system, |file| {
    read_pe_dump(runtime, file)
  })
}

// Resolve and validate the entire graph before copying anything into fresh caller-owned staging.
pub fn stage_with(
  library: &Path,
  runtime: &Path,
  destination: &Path,
  system: &Path,
  mut read_dump: impl FnMut(&Path) -> Result<String>,
) -> Result<DependencyManifest> {
  let runtime_dlls = dll_index(runtime)?;
  let system_dlls = dll_index(system)?;
  let main_name = library
    .file_name()
    .context("Library has no filename")?
    .to_string_lossy()
    .to_ascii_lowercase();
  let mut resolved = BTreeMap::from([(main_name, library.to_owned())]);
  let mut queue = VecDeque::from([library.to_owned()]);
  let mut system_dependencies = BTreeSet::new();
  while let Some(importer) = queue.pop_front() {
    for name in parse_imports(&importer, &read_dump(&importer)?)? {
      if name.starts_with("msys-") || name.starts_with("cyg") {
        bail!(
          "{} imports {name}; MSYS/Cygwin runtimes cannot be bundled.",
          importer.display()
        );
      }
      if name.starts_with("api-ms-win-")
        || name.starts_with("ext-ms-win-")
        || (system_dlls.contains_key(&name) && !runtime_dlls.contains_key(&name))
      {
        system_dependencies.insert(name);
        continue;
      }
      if resolved.contains_key(&name) {
        continue;
      }
      let source = runtime_dlls.get(&name).with_context(|| {
        format!(
          "{} imports {name}, missing from {}.",
          importer.display(),
          runtime.display()
        )
      })?;
      resolved.insert(name, source.clone());
      queue.push_back(source.clone());
    }
  }
  let mut sources: Vec<_> = resolved.into_values().collect();
  sources.sort();
  let mut bundled = Vec::new();
  for source in sources {
    let name = source
      .file_name()
      .context("Dependency has no filename")?
      .to_string_lossy()
      .into_owned();
    let target = destination.join(&name);
    fs::copy(&source, &target).with_context(|| format!("Cannot stage {}", source.display()))?;
    bundled.push(BundledLibrary {
      name,
      sha256: util::sha256_file(&target)?,
    });
  }
  Ok(DependencyManifest {
    bundled,
    system_dependencies: system_dependencies.into_iter().collect(),
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn dump(imports: &[&str], format: &str) -> String {
    format!(
      "fixture: file format {format}\n{}",
      imports
        .iter()
        .map(|name| format!("\tDLL Name: {name}\n"))
        .collect::<String>()
    )
  }

  struct Fixture {
    _root: tempfile::TempDir,
    runtime: PathBuf,
    system: PathBuf,
    destination: PathBuf,
    library: PathBuf,
  }
  impl Fixture {
    fn new() -> Self {
      let root = tempfile::tempdir().unwrap();
      let runtime = root.path().join("runtime");
      let system = root.path().join("system");
      let destination = root.path().join("destination");
      for directory in [&runtime, &system, &destination] {
        fs::create_dir(directory).unwrap();
      }
      let library = root.path().join("libmpv-2.dll");
      Self {
        _root: root,
        runtime,
        system,
        destination,
        library,
      }
    }
    fn dll(&self, path: &Path, imports: &[&str], format: &str) {
      fs::write(path, format!("binary {}", path.display())).unwrap();
      fs::write(path.with_extension("dump"), dump(imports, format)).unwrap();
    }
    fn stage(&self) -> Result<DependencyManifest> {
      stage_with(
        &self.library,
        &self.runtime,
        &self.destination,
        &self.system,
        |path| Ok(fs::read_to_string(path.with_extension("dump"))?),
      )
    }
  }

  #[test]
  fn transitive_closure_handles_cycles_and_prefers_explicit_runtime_over_driver_dlls() {
    let fixture = Fixture::new();
    fixture.dll(
      &fixture.library,
      &[
        "Codec.DLL",
        "KERNEL32.dll",
        "vulkan-1.dll",
        "api-ms-win-test.dll",
      ],
      "pei-x86-64",
    );
    fixture.dll(
      &fixture.runtime.join("codec.dll"),
      &["HELPER.dll", "kernel32.DLL"],
      "pei-x86-64",
    );
    fixture.dll(
      &fixture.runtime.join("Helper.DLL"),
      &["CODEC.dll", "LIBMPV-2.DLL", "ext-ms-win-test.dll"],
      "pei-x86-64",
    );
    fixture.dll(
      &fixture.runtime.join("vulkan-1.dll"),
      &["kernel32.dll"],
      "pei-x86-64",
    );
    fixture.dll(&fixture.runtime.join("unrelated.dll"), &[], "pei-x86-64");
    fs::write(fixture.system.join("Kernel32.dll"), "system").unwrap();
    fs::write(fixture.system.join("vulkan-1.dll"), "driver").unwrap();
    let result = fixture.stage().unwrap();
    assert_eq!(
      result.system_dependencies,
      ["api-ms-win-test.dll", "ext-ms-win-test.dll", "kernel32.dll"]
    );
    assert_eq!(result.bundled.len(), 4);
    for item in result.bundled {
      assert_eq!(
        item.sha256,
        util::sha256_file(&fixture.destination.join(item.name)).unwrap()
      );
    }
    assert!(!fixture.destination.join("unrelated.dll").exists());
    assert_eq!(
      fs::read(fixture.destination.join("vulkan-1.dll")).unwrap(),
      fs::read(fixture.runtime.join("vulkan-1.dll")).unwrap()
    );
  }

  #[test]
  fn invalid_transitive_graph_never_produces_partial_bundle() {
    for (imports, format) in [
      (&["missing.dll"][..], "pei-x86-64"),
      (&[][..], "pei-i386"),
      (&["msys-2.0.dll"][..], "pei-x86-64"),
      (&["cygwin1.dll"][..], "pei-x86-64"),
    ] {
      let fixture = Fixture::new();
      fixture.dll(&fixture.library, &["codec.dll"], "pei-x86-64");
      fixture.dll(&fixture.runtime.join("codec.dll"), imports, format);
      fs::write(
        fixture.system.join("msys-2.0.dll"),
        "not a system dependency",
      )
      .unwrap();
      fs::write(fixture.destination.join("stale.dll"), "stale").unwrap();
      assert!(fixture.stage().is_err());
      assert_eq!(fs::read_dir(&fixture.destination).unwrap().count(), 1);
    }
  }

  #[test]
  fn imports_reject_malformed_names_and_non_native_images() {
    for output in [
      dump(&["../escape.dll"], "pei-x86-64"),
      dump(&[], "pei-i386"),
      "DLL Name: codec.dll\n".to_owned(),
    ] {
      assert!(parse_imports(Path::new("fixture.dll"), &output).is_err());
    }
    assert_eq!(
      parse_imports(
        Path::new("fixture.dll"),
        &dump(&["KERNEL32.dll", "D3D11.dll", "kernel32.DLL"], "pei-x86-64").replace('\n', "\r\n")
      )
      .unwrap(),
      ["kernel32.dll", "d3d11.dll"]
    );
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn case_ambiguous_runtime_index_is_rejected() {
    let fixture = Fixture::new();
    fs::write(fixture.runtime.join("CODEC.dll"), "one").unwrap();
    fs::write(fixture.runtime.join("codec.dll"), "two").unwrap();
    assert!(dll_index(&fixture.runtime).is_err());
  }
}
