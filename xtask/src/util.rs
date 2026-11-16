use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};

pub fn sha256_file(path: &Path) -> Result<String> {
  let mut file = File::open(path).with_context(|| format!("Opening {}", path.display()))?;
  let mut digest = Sha256::new();
  let mut buffer = [0_u8; 64 * 1024];
  loop {
    let count = file.read(&mut buffer)?;
    if count == 0 {
      break;
    }
    digest.update(&buffer[..count]);
  }
  Ok(format!("{:x}", digest.finalize()))
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
  let file = File::open(path).with_context(|| format!("Opening {}", path.display()))?;
  serde_json::from_reader(BufReader::new(file))
    .with_context(|| format!("Parsing {}", path.display()))
}

pub fn write_json<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
  if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
    fs::create_dir_all(parent)?;
  }
  let mut file =
    BufWriter::new(File::create(path).with_context(|| format!("Writing {}", path.display()))?);
  serde_json::to_writer_pretty(&mut file, value)?;
  file.write_all(b"\n")?;
  file.flush()?;
  Ok(())
}

pub fn now_iso() -> String {
  chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
