//! TMDb lookups supplementing original-language data that released Jellyfin servers
//! do not expose: `BaseItemDto.original_language` is only populated on master builds,
//! so the value comes from TMDb when the server leaves it empty.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::JellyfinError;

/// Production TMDb API origin; tests substitute a local server.
pub(crate) const TMDB_API_BASE: &str = "https://api.themoviedb.org";

/// TMDb endpoint family for an item: `/3/movie/{id}` or `/3/tv/{id}`.
/// TMDb endpoint family for an item: `/3/movie/{id}` or `/3/tv/{id}`.
#[derive(Debug, Clone, Copy, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TmdbMediaKind {
  Movie,
  Series,
}

impl TmdbMediaKind {
  fn path_segment(self) -> &'static str {
    match self {
      Self::Movie => "movie",
      Self::Series => "tv",
    }
  }
}

const CACHE_VERSION: u32 = 1;
const CACHE_FILE: &str = "tmdb-languages.json";
const CONFIG_DIRECTORY: &str = "jellypilot";

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct TmdbLanguageRecord {
  kind: TmdbMediaKind,
  tmdb_id: String,
  language: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct StoredTmdbLanguages {
  version: u32,
  entries: Vec<TmdbLanguageRecord>,
}

/// Global, server-agnostic on-disk cache of TMDb original languages. A TMDb id
/// identifies one work regardless of which Jellyfin or Emby server hosts the
/// file, so the cache is shared across servers and profiles. Only successful
/// answers are persisted; lookup failures stay in the session-level memory.
#[derive(Debug)]
pub(crate) struct TmdbLanguageCache {
  path: PathBuf,
  entries: HashMap<(TmdbMediaKind, String), String>,
  loaded: bool,
}

impl Default for TmdbLanguageCache {
  fn default() -> Self {
    let path = dirs::config_dir()
      .unwrap_or_else(std::env::temp_dir)
      .join(CONFIG_DIRECTORY)
      .join(CACHE_FILE);
    Self::with_path(path)
  }
}

impl TmdbLanguageCache {
  pub(crate) fn with_path(path: PathBuf) -> Self {
    Self {
      path,
      entries: HashMap::new(),
      loaded: false,
    }
  }

  /// Lazily reads the cache file on first use; malformed or unsupported files are
  /// abandoned (never overwritten until a fresh answer arrives) and treated as empty.
  fn ensure_loaded(&mut self) {
    if self.loaded {
      return;
    }
    self.loaded = true;
    match read_cache(&self.path) {
      Ok(entries) => self.entries = entries,
      Err(error) => log::warn!("TMDb language cache ignored: {error}"),
    }
  }

  pub(crate) fn get(&mut self, kind: TmdbMediaKind, tmdb_id: &str) -> Option<String> {
    self.ensure_loaded();
    self.entries.get(&(kind, tmdb_id.to_owned())).cloned()
  }

  /// Persist a confirmed original language atomically (temp file + rename).
  pub(crate) fn insert(&mut self, kind: TmdbMediaKind, tmdb_id: &str, language: &str) {
    self.ensure_loaded();
    let key = (kind, tmdb_id.to_owned());
    if self.entries.get(&key).map(String::as_str) == Some(language) {
      return;
    }
    self.entries.insert(key, language.to_owned());
    let stored = StoredTmdbLanguages {
      version: CACHE_VERSION,
      entries: self
        .entries
        .iter()
        .map(|((kind, tmdb_id), language)| TmdbLanguageRecord {
          kind: *kind,
          tmdb_id: tmdb_id.clone(),
          language: language.clone(),
        })
        .collect(),
    };
    if let Err(error) = save_cache(&self.path, &stored) {
      log::warn!("could not persist TMDb language cache: {error}");
    }
  }
}

fn read_cache(path: &Path) -> Result<HashMap<(TmdbMediaKind, String), String>, String> {
  let contents = match std::fs::read_to_string(path) {
    Ok(contents) => contents,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
      return Ok(HashMap::new());
    }
    Err(error) => return Err(format!("could not read {}: {error}", path.display())),
  };
  let stored: StoredTmdbLanguages = serde_json::from_str(&contents)
    .map_err(|error| format!("could not parse {}: {error}", path.display()))?;
  if stored.version != CACHE_VERSION {
    return Err(format!(
      "unsupported TMDb language cache version {}",
      stored.version
    ));
  }
  let mut entries = HashMap::with_capacity(stored.entries.len());
  for record in stored.entries {
    if record.tmdb_id.trim().is_empty() || record.language.trim().is_empty() {
      return Err("TMDb language cache holds an invalid record".to_owned());
    }
    if entries
      .insert((record.kind, record.tmdb_id), record.language)
      .is_some()
    {
      return Err("TMDb language cache holds a duplicate record".to_owned());
    }
  }
  Ok(entries)
}

fn save_cache(path: &Path, stored: &StoredTmdbLanguages) -> Result<(), std::io::Error> {
  let contents = serde_json::to_string_pretty(stored)?;
  if std::fs::read_to_string(path).ok().as_deref() == Some(contents.as_str()) {
    return Ok(());
  }
  if let Some(directory) = path.parent() {
    std::fs::create_dir_all(directory)?;
  }
  let temporary = path.with_extension("json.tmp");
  if let Err(error) = std::fs::write(&temporary, contents) {
    let _ = std::fs::remove_file(&temporary);
    return Err(error);
  }
  if let Err(error) = std::fs::rename(&temporary, path) {
    let _ = std::fs::remove_file(&temporary);
    return Err(error);
  }
  Ok(())
}

#[derive(Debug, Deserialize)]
struct TmdbTitle {
  original_language: Option<String>,
}

/// Fetch the ISO 639-1 original language for a TMDb movie or series.
///
/// Missing entries and entries without an original language yield `Ok(None)`; a
/// rejected API key is an error so the caller can surface it in diagnostics.
pub(crate) async fn original_language(
  http: &reqwest::Client,
  base_url: &str,
  api_key: &str,
  kind: TmdbMediaKind,
  tmdb_id: &str,
) -> Result<Option<String>, JellyfinError> {
  let url = format!(
    "{}/3/{}/{}?api_key={}",
    base_url.trim_end_matches('/'),
    kind.path_segment(),
    tmdb_id.trim(),
    api_key
  );
  let response = http.get(&url).send().await.map_err(|error| {
    // The request URL carries the API key; keep it out of logs and diagnostics.
    JellyfinError::HttpError(format!(
      "TMDb original language request failed: {}",
      error.without_url()
    ))
  })?;
  match response.status() {
    reqwest::StatusCode::OK => {
      let title = response.json::<TmdbTitle>().await.map_err(|error| {
        JellyfinError::HttpError(format!(
          "TMDb original language response malformed: {error}"
        ))
      })?;
      Ok(
        title
          .original_language
          .map(|language| language.trim().to_owned())
          .filter(|language| !language.is_empty()),
      )
    }
    reqwest::StatusCode::NOT_FOUND => Ok(None),
    reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => Err(
      JellyfinError::HttpError("TMDb rejected the configured API key".to_string()),
    ),
    status => Err(JellyfinError::HttpError(format!(
      "TMDb original language lookup failed with status {status}"
    ))),
  }
}
