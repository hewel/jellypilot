use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, ensure, Context, Result};
use clap::{Subcommand, ValueEnum};
use serde_json::Value;

use crate::{process, util};

const GENERATOR_VERSION: &str = "7.22.0";
// SHA-256 of the versioned Maven Central artifact (not an npm wrapper).
const GENERATOR_SHA256: &str = "3f1e6ce5c6ad4f15242c6170ab43aad4bad771622617eeece4a7d4f72ffaf329";
const GENERATOR_URL: &str = "https://repo.maven.apache.org/maven2/org/openapitools/openapi-generator-cli/7.22.0/openapi-generator-cli-7.22.0.jar";
const GLOBAL_PROPERTY: &str = "apiTests=false,modelTests=false,apiDocs=false,modelDocs=false";

#[derive(Debug, Subcommand)]
pub enum Command {
  /// Regenerate a checked-in client using the pinned JAR (requires Java).
  Generate { provider: Provider },
  /// Download a provider spec and regenerate its client transactionally (requires Java).
  Update { provider: Provider },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Provider {
  Jellyfin,
  Emby,
}

impl Provider {
  fn name(self) -> &'static str {
    match self {
      Self::Jellyfin => "jellyfin",
      Self::Emby => "emby",
    }
  }

  fn spec_name(self) -> &'static str {
    match self {
      Self::Jellyfin => "jellyfin-openapi-stable.json",
      Self::Emby => "emby-openapi-4.9.3.0.json",
    }
  }

  fn spec_url(self) -> &'static str {
    match self {
      Self::Jellyfin => "https://api.jellyfin.org/openapi/jellyfin-openapi-stable.json",
      Self::Emby => "https://raw.githubusercontent.com/MediaBrowser/Emby.SDK/4.9.3.0/Resources/OpenApi/openapi_v3.json",
    }
  }

  fn user_agent(self) -> &'static str {
    match self {
      Self::Jellyfin => "jmsr-openapi-snapshot",
      Self::Emby => "jellypilot-emby-openapi-snapshot",
    }
  }
}

fn is_link(metadata: &fs::Metadata) -> bool {
  #[cfg(windows)]
  {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
  }
  #[cfg(not(windows))]
  {
    metadata.file_type().is_symlink()
  }
}

fn metadata(path: &Path) -> Result<Option<fs::Metadata>> {
  match fs::symlink_metadata(path) {
    Ok(metadata) => {
      ensure!(
        !is_link(&metadata),
        "Refusing symlink or junction {}",
        path.display()
      );
      Ok(Some(metadata))
    }
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
    Err(error) => Err(error).with_context(|| format!("Inspecting {}", path.display())),
  }
}

fn directory(path: &Path) -> Result<()> {
  if let Some(metadata) = metadata(path)? {
    ensure!(metadata.is_dir(), "Expected directory {}", path.display());
  } else {
    fs::create_dir(path).with_context(|| format!("Creating {}", path.display()))?;
  }
  Ok(())
}

fn http_client() -> Result<reqwest::blocking::Client> {
  reqwest::blocking::Client::builder()
    .https_only(true)
    .timeout(Duration::from_secs(300))
    .build()
    .context("Creating HTTPS download client")
}

fn verify_jar(path: &Path) -> Result<()> {
  ensure!(
    util::sha256_file(path)? == GENERATOR_SHA256,
    "OpenAPI Generator {GENERATOR_VERSION} SHA-256 mismatch at {}",
    path.display()
  );
  Ok(())
}

fn generator_jar(client: &reqwest::blocking::Client) -> Result<PathBuf> {
  let root = crate::root();
  let cache = root.join("target/openapi-generator");
  directory(&root.join("target"))?;
  directory(&cache)?;
  let jar = cache.join(format!("openapi-generator-cli-{GENERATOR_VERSION}.jar"));
  if let Some(metadata) = metadata(&jar)? {
    ensure!(
      metadata.is_file(),
      "Expected regular generator JAR {}",
      jar.display()
    );
    // Never execute even a previously cached JAR without verifying its bytes.
    verify_jar(&jar)?;
    return Ok(jar);
  }
  let mut temporary = tempfile::NamedTempFile::new_in(&cache)?;
  client
    .get(GENERATOR_URL)
    .send()
    .context("Downloading pinned OpenAPI Generator JAR")?
    .error_for_status()
    .context("Downloading pinned OpenAPI Generator JAR")?
    .copy_to(temporary.as_file_mut())?;
  temporary.as_file().sync_all()?;
  verify_jar(temporary.path())?;
  temporary
    .persist(&jar)
    .map_err(|error| error.error)
    .with_context(|| format!("Publishing verified generator JAR {}", jar.display()))?;
  Ok(jar)
}

fn patch_emby(spec: &mut Value) -> Result<()> {
  let image_path = spec
    .get_mut("paths")
    .and_then(|paths| paths.get_mut("/Users/{Id}/Images/{Type}/{Index}"))
    .context("Emby OpenAPI image path patch target was not found")?;
  let index_parameter = image_path
    .get("delete")
    .and_then(|delete| delete.get("parameters"))
    .and_then(Value::as_array)
    .and_then(|parameters| {
      parameters.iter().find(|parameter| {
        parameter.get("name").and_then(Value::as_str) == Some("Index")
          && parameter.get("in").and_then(Value::as_str) == Some("path")
      })
    })
    .context("Emby OpenAPI image path patch target was not found")?
    .clone();
  let post_parameters = image_path
    .get_mut("post")
    .and_then(|post| post.get_mut("parameters"))
    .and_then(Value::as_array_mut)
    .context("Emby OpenAPI image path patch target was not found")?;
  ensure!(
    !post_parameters.iter().any(|parameter| {
      parameter.get("name").and_then(Value::as_str) == Some("Index")
        && parameter.get("in").and_then(Value::as_str) == Some("path")
    }),
    "Emby OpenAPI image path patch already applied upstream"
  );
  post_parameters.push(index_parameter);
  Ok(())
}

struct Replacement {
  staged: PathBuf,
  target: PathBuf,
}

// Each original remains in the same-filesystem transaction directory until all
// replacements succeed. A failed publication restores every original; failed
// recovery retains the backups rather than deleting the only surviving copy.
fn publish(work: tempfile::TempDir, replacements: &[Replacement]) -> Result<()> {
  publish_unless_cancelled(work, replacements, process::interrupted)
}

fn publish_unless_cancelled(
  work: tempfile::TempDir,
  replacements: &[Replacement],
  mut cancelled: impl FnMut() -> bool,
) -> Result<()> {
  ensure!(
    !cancelled(),
    "API publication cancelled; existing clients and snapshots are unchanged"
  );
  let mut states = Vec::new();
  for (index, replacement) in replacements.iter().enumerate() {
    let backup = work.path().join(format!("backup-{index}"));
    let mut backed_up = false;
    let mut installed = false;
    let result = (|| -> Result<()> {
      if metadata(&replacement.target)?.is_some() {
        fs::rename(&replacement.target, &backup)?;
        backed_up = true;
      }
      ensure!(!cancelled(), "API publication cancelled before commit");
      fs::rename(&replacement.staged, &replacement.target)?;
      installed = true;
      // The final successful poll after installing the last replacement is
      // the commit point. Until then cancellation rolls back all originals;
      // after it backups may be discarded and publication is committed.
      ensure!(!cancelled(), "API publication cancelled before commit");
      Ok(())
    })();
    states.push((replacement, backup, backed_up, installed));
    if let Err(error) = result {
      let mut recovery_errors = Vec::new();
      for (replacement, backup, backed_up, installed) in states.into_iter().rev() {
        if installed {
          if let Err(error) = fs::rename(&replacement.target, &replacement.staged) {
            recovery_errors.push(format!("{}: {error}", replacement.target.display()));
            continue;
          }
        }
        if backed_up {
          if let Err(error) = fs::rename(&backup, &replacement.target) {
            recovery_errors.push(format!("{}: {error}", replacement.target.display()));
          }
        }
      }
      if !recovery_errors.is_empty() {
        let retained = work.keep();
        bail!(
          "API publication failed: {error:#}; recovery failed: {}. Original backups retained at {}",
          recovery_errors.join("; "),
          retained.display()
        );
      }
      return Err(error).context("API publication failed; original clients/spec restored");
    }
  }
  Ok(())
}

fn generate(provider: Provider, update: bool) -> Result<()> {
  let root = crate::root();
  // Only explicit api subcommands construct a downloader or invoke Java.
  let client = http_client()?;
  let jar = generator_jar(&client)?;
  let parent = root.join("crates/media-server-api");
  for path in [root.join("crates"), parent.clone(), parent.join("openapi")] {
    let info =
      metadata(&path)?.with_context(|| format!("Missing API directory {}", path.display()))?;
    ensure!(info.is_dir(), "Expected API directory {}", path.display());
  }
  let output = parent.join(provider.name());
  if let Some(metadata) = metadata(&output)? {
    ensure!(
      metadata.is_dir(),
      "Expected generated client directory {}",
      output.display()
    );
  }
  let snapshot = parent.join("openapi").join(provider.spec_name());
  let config = parent
    .join("openapi")
    .join(format!("{}-api-generator.json", provider.name()));
  let work = tempfile::Builder::new()
    .prefix(".openapi-")
    .tempdir_in(&parent)?;
  let staged_spec = work.path().join("snapshot.json");
  if update {
    let mut response = client
      .get(provider.spec_url())
      .header(reqwest::header::USER_AGENT, provider.user_agent())
      .send()
      .with_context(|| format!("Fetching {} OpenAPI spec", provider.name()))?
      .error_for_status()?;
    response.copy_to(&mut File::create(&staged_spec)?)?;
  } else {
    fs::copy(&snapshot, &staged_spec).context("Copying checked-in OpenAPI snapshot")?;
  }
  let mut spec: Value = util::read_json(&staged_spec)?;
  ensure!(
    spec.get("openapi").and_then(Value::as_str).is_some()
      && spec.get("paths").and_then(Value::as_object).is_some(),
    "Downloaded snapshot is not an OpenAPI document"
  );
  let generator_spec = if matches!(provider, Provider::Emby) {
    patch_emby(&mut spec)?;
    let patched = work.path().join("emby-generator.json");
    util::write_json(&patched, &spec)?;
    patched
  } else {
    staged_spec.clone()
  };
  let staged_client = work.path().join("client");
  process::run(process::command("java")
    .arg("-jar").arg(jar).arg("generate")
    .arg("-i").arg(generator_spec).args(["-g", "rust"])
    .arg("-o").arg(&staged_client).arg("-c").arg(config)
    .args(["--global-property", GLOBAL_PROPERTY]))
    .context("OpenAPI generation failed; existing clients and snapshots are unchanged (Java is required only for api commands)")?;
  for path in [
    staged_client.join("Cargo.toml"),
    staged_client.join("src/lib.rs"),
  ] {
    let info =
      metadata(&path)?.with_context(|| format!("Generator did not produce {}", path.display()))?;
    ensure!(
      info.is_file() && info.len() > 0,
      "Generator produced an incomplete client at {}",
      path.display()
    );
  }
  let mut replacements = vec![Replacement {
    staged: staged_client,
    target: output,
  }];
  if update {
    replacements.push(Replacement {
      staged: staged_spec,
      target: snapshot,
    });
  }
  publish(work, &replacements)
}

pub fn run(command: Command) -> Result<()> {
  match command {
    Command::Generate { provider } => generate(provider, false),
    Command::Update { provider } => generate(provider, true),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn emby_patch_only_adds_the_missing_image_index_to_post() {
    let index =
      json!({"name": "Index", "in": "path", "required": true, "schema": {"type": "integer"}});
    let mut spec = json!({"paths": {"/Users/{Id}/Images/{Type}/{Index}": {
      "delete": {"parameters": [index.clone()]},
      "post": {"parameters": [{"name": "Type", "in": "path"}]}
    }}});
    patch_emby(&mut spec).unwrap();
    let path = &spec["paths"]["/Users/{Id}/Images/{Type}/{Index}"];
    assert_eq!(path["post"]["parameters"][1], index);
    assert_eq!(path["delete"]["parameters"].as_array().unwrap().len(), 1);
    assert!(patch_emby(&mut spec).is_err());
  }

  #[test]
  fn emby_patch_rejects_changed_upstream_shape() {
    assert!(patch_emby(&mut json!({"paths": {}})).is_err());
  }

  #[test]
  fn failed_second_publication_restores_client_and_spec() {
    let parent = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir_in(parent.path()).unwrap();
    let output = parent.path().join("client");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("old.rs"), "original client").unwrap();
    let snapshot = parent.path().join("snapshot.json");
    fs::write(&snapshot, "original spec").unwrap();
    let staged = work.path().join("client");
    fs::create_dir(&staged).unwrap();
    fs::write(staged.join("new.rs"), "new client").unwrap();
    let missing_spec = work.path().join("missing.json");
    let result = publish(
      work,
      &[
        Replacement {
          staged,
          target: output.clone(),
        },
        Replacement {
          staged: missing_spec,
          target: snapshot.clone(),
        },
      ],
    );
    assert!(result.is_err());
    assert_eq!(
      fs::read_to_string(output.join("old.rs")).unwrap(),
      "original client"
    );
    assert!(!output.join("new.rs").exists());
    assert_eq!(fs::read_to_string(snapshot).unwrap(), "original spec");
  }

  #[test]
  fn successful_publication_replaces_the_complete_client() {
    let parent = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir_in(parent.path()).unwrap();
    let output = parent.path().join("client");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("stale.rs"), "old").unwrap();
    let staged = work.path().join("client");
    fs::create_dir(&staged).unwrap();
    fs::write(staged.join("current.rs"), "new").unwrap();
    publish(
      work,
      &[Replacement {
        staged,
        target: output.clone(),
      }],
    )
    .unwrap();
    assert!(!output.join("stale.rs").exists());
    assert_eq!(
      fs::read_to_string(output.join("current.rs")).unwrap(),
      "new"
    );
  }

  #[test]
  fn cancellation_before_publication_preserves_existing_client() {
    let parent = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir_in(parent.path()).unwrap();
    let output = parent.path().join("client");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("old.rs"), "original client").unwrap();
    let staged = work.path().join("client");
    fs::create_dir(&staged).unwrap();
    fs::write(staged.join("new.rs"), "new client").unwrap();
    let result = publish_unless_cancelled(
      work,
      &[Replacement {
        staged,
        target: output.clone(),
      }],
      || true,
    );
    assert!(result.is_err());
    assert_eq!(
      fs::read_to_string(output.join("old.rs")).unwrap(),
      "original client"
    );
    assert!(!output.join("new.rs").exists());
  }

  #[test]
  fn cancellation_during_publication_restores_every_original_before_commit() {
    enum Checkpoint {
      ClientBackup,
      ClientInstalled,
      SpecInstalled,
    }
    for checkpoint in [
      Checkpoint::ClientBackup,
      Checkpoint::ClientInstalled,
      Checkpoint::SpecInstalled,
    ] {
      let parent = tempfile::tempdir().unwrap();
      let work = tempfile::tempdir_in(parent.path()).unwrap();
      let output = parent.path().join("client");
      fs::create_dir(&output).unwrap();
      fs::write(output.join("old.rs"), "original client").unwrap();
      let snapshot = parent.path().join("snapshot.json");
      fs::write(&snapshot, "original spec").unwrap();
      let staged_client = work.path().join("client");
      fs::create_dir(&staged_client).unwrap();
      fs::write(staged_client.join("new.rs"), "new client").unwrap();
      let staged_spec = work.path().join("snapshot.json");
      fs::write(&staged_spec, "new spec").unwrap();
      let backup = work.path().join("backup-0");
      let mut observed_cancellation = false;
      let result = publish_unless_cancelled(
        work,
        &[
          Replacement {
            staged: staged_client,
            target: output.clone(),
          },
          Replacement {
            staged: staged_spec,
            target: snapshot.clone(),
          },
        ],
        || {
          let cancelled = match checkpoint {
            Checkpoint::ClientBackup => backup.exists() && !output.exists(),
            Checkpoint::ClientInstalled => output.join("new.rs").exists(),
            Checkpoint::SpecInstalled => {
              fs::read_to_string(&snapshot).ok().as_deref() == Some("new spec")
            }
          };
          observed_cancellation |= cancelled;
          cancelled
        },
      );
      assert!(observed_cancellation);
      assert!(result.is_err());
      assert_eq!(
        fs::read_to_string(output.join("old.rs")).unwrap(),
        "original client"
      );
      assert!(!output.join("new.rs").exists());
      assert_eq!(fs::read_to_string(&snapshot).unwrap(), "original spec");
    }
  }

  #[test]
  fn refuses_to_execute_corrupted_cached_generator() {
    let temporary = tempfile::NamedTempFile::new().unwrap();
    fs::write(temporary.path(), "not the pinned generator").unwrap();
    assert!(verify_jar(temporary.path()).is_err());
  }
}
