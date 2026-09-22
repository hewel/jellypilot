//! Complete, bounded library snapshots for metadata the providers cannot query.
//!
//! Countries are production locations, never metadata language or server locale.
//! One active browse snapshot is retained per client. Facet-only scans cannot evict
//! it, and continuation requests never silently rebuild an invalidated snapshot.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::Value;
use tokio::sync::Mutex;

use super::*;

const PAGE_SIZE: i32 = 100;
const MAX_ITEMS: usize = 20_000;
const MAX_PAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_SCAN_BYTES: usize = 32 * 1024 * 1024;
const SCAN_TIMEOUT: Duration = Duration::from_secs(60);
const FRESH_FOR: Duration = Duration::from_secs(60);

#[derive(Default)]
pub(super) struct Cache {
  revision: AtomicU64,
  catalog: Mutex<Option<Catalog>>,
}

#[derive(Clone, Eq, PartialEq)]
struct Scope {
  provider: MediaServerProvider,
  server_url: String,
  user_id: String,
  authentication_epoch: u64,
}

impl Scope {
  fn capture(client: &JellyfinClient) -> Result<Self, JellyfinError> {
    let state = client.state.read();
    if state.access_token.is_none() {
      return Err(JellyfinError::NotConnected);
    }
    Ok(Self {
      provider: state.provider,
      server_url: state
        .server_url
        .clone()
        .ok_or(JellyfinError::NotConnected)?,
      user_id: state.user_id.clone().ok_or(JellyfinError::NotConnected)?,
      authentication_epoch: state.authentication_epoch,
    })
  }

  fn matches(&self, state: &ClientState) -> bool {
    self.provider == state.provider
      && state.server_url.as_deref() == Some(self.server_url.as_str())
      && state.user_id.as_deref() == Some(self.user_id.as_str())
      && self.authentication_epoch == state.authentication_epoch
      && state.access_token.is_some()
  }
}

#[derive(Eq, PartialEq)]
struct Key {
  scope: Scope,
  revision: u64,
  library_id: String,
  kind: VideoLibraryKind,
  sort: VideoLibrarySort,
  direction: VideoLibrarySortDirection,
  played: VideoLibraryPlayedFilter,
  favorites: bool,
}

impl Key {
  fn new(
    client: &JellyfinClient,
    request: &VideoLibraryPageRequest,
  ) -> Result<Self, JellyfinError> {
    if request.library_id.trim().is_empty() {
      return Err(failed("Library id is required for video filtering"));
    }
    Ok(Self {
      scope: Scope::capture(client)?,
      revision: client.filter_catalog.revision.load(Ordering::Acquire),
      library_id: request.library_id.clone(),
      kind: request.collection_type,
      sort: request.sort,
      direction: request.sort_direction,
      played: request.played_filter,
      favorites: request.favorites_only,
    })
  }

  fn validate(&self, client: &JellyfinClient) -> Result<(), JellyfinError> {
    if self.revision != client.filter_catalog.revision.load(Ordering::Acquire)
      || !self.scope.matches(&client.state.read())
    {
      return Err(stale());
    }
    Ok(())
  }

  fn same_library(&self, other: &Self) -> bool {
    self.scope == other.scope
      && self.revision == other.revision
      && self.library_id == other.library_id
      && self.kind == other.kind
  }
}

struct Catalog {
  key: Key,
  completed: Instant,
  entries: Vec<Entry>,
}

struct Entry {
  item: VideoLibraryItem,
  countries: Vec<String>,
  genres: Vec<String>,
  qualities: BTreeSet<VideoLibraryQuality>,
}

impl Entry {
  fn matches(&self, filters: &VideoLibraryFilters) -> bool {
    filters
      .quality
      .is_none_or(|quality| self.qualities.contains(&quality))
      && matches_label(&self.countries, filters.country.as_deref())
      && matches_label(&self.genres, filters.genre.as_deref())
  }
}

fn matches_label(values: &[String], selected: Option<&str>) -> bool {
  selected.is_none_or(|selected| {
    let selected = selected.trim().to_lowercase();
    selected.is_empty() || values.iter().any(|value| value.to_lowercase() == selected)
  })
}

impl JellyfinLibrary<'_> {
  /// Returns actual production countries, genres, and video raster classes from
  /// this user's complete library. Series have no quality choices. Scans fail
  /// explicitly above 20,000 items, 32 MiB of metadata, or 60 seconds; partial
  /// catalogs are never returned as complete results.
  pub async fn filter_options(
    &self,
    library_id: String,
    collection_type: VideoLibraryKind,
  ) -> Result<VideoLibraryFilterOptions, JellyfinError> {
    let request = VideoLibraryPageRequest {
      library_id,
      collection_type,
      start_index: 0,
      limit: PAGE_SIZE,
      sort: VideoLibrarySort::Title,
      sort_direction: VideoLibrarySortDirection::Ascending,
      played_filter: VideoLibraryPlayedFilter::All,
      favorites_only: false,
      filters: VideoLibraryFilters::default(),
    };
    let key = Key::new(self.client, &request)?;
    let mut cached = self.client.filter_catalog.catalog.lock().await;
    key.validate(self.client)?;
    if let Some(catalog) = cached.as_ref().filter(|catalog| {
      catalog.key.same_library(&key)
        && catalog.key.played == VideoLibraryPlayedFilter::All
        && !catalog.key.favorites
        && catalog.completed.elapsed() < FRESH_FOR
    }) {
      return Ok(options(catalog));
    }
    let catalog = scan(self.client, key).await?;
    let result = options(&catalog);
    // Opening and cancelling a filter panel must preserve ongoing pagination.
    if cached.is_none() {
      *cached = Some(catalog);
    }
    Ok(result)
  }

  /// Invalidates filter snapshots after explicit refresh or a confirmed user-data
  /// mutation. In-flight scans are fenced; existing continuation pages fail with
  /// a stale-snapshot error and must restart from index zero.
  pub fn invalidate_filter_catalog(&self) {
    self
      .client
      .filter_catalog
      .revision
      .fetch_add(1, Ordering::AcqRel);
  }
}

pub(super) async fn browse(
  client: &JellyfinClient,
  request: VideoLibraryPageRequest,
) -> Result<VideoLibraryPage, JellyfinError> {
  let key = Key::new(client, &request)?;
  let start_index = request.start_index.max(0);
  let limit = request.limit.clamp(1, PAGE_SIZE);
  let mut cached = client.filter_catalog.catalog.lock().await;
  key.validate(client)?;
  let matching = cached.as_ref().is_some_and(|catalog| catalog.key == key);
  if start_index > 0 && !matching {
    return Err(stale());
  }
  // Sparse page stores may request index zero again while retaining later pages.
  // Only explicit invalidation or a new query may replace this active snapshot.
  if !matching {
    *cached = Some(scan(client, key).await?);
  }
  let catalog = cached.as_ref().ok_or_else(stale)?;
  catalog.key.validate(client)?;
  let matches = catalog
    .entries
    .iter()
    .filter(|entry| entry.matches(&request.filters));
  let total_record_count = matches.clone().count() as i32;
  let items = matches
    .skip(start_index as usize)
    .take(limit as usize)
    .map(|entry| entry.item.clone())
    .collect::<Vec<_>>();
  Ok(VideoLibraryPage {
    library_id: request.library_id,
    collection_type: request.collection_type,
    start_index,
    limit,
    total_record_count,
    has_more: start_index.saturating_add(items.len() as i32) < total_record_count,
    items,
  })
}

fn options(catalog: &Catalog) -> VideoLibraryFilterOptions {
  let mut qualities = BTreeSet::new();
  let mut countries = BTreeMap::new();
  let mut genres = BTreeMap::new();
  for entry in &catalog.entries {
    qualities.extend(entry.qualities.iter().copied());
    for value in &entry.countries {
      countries
        .entry(value.to_lowercase())
        .or_insert_with(|| value.clone());
    }
    for value in &entry.genres {
      genres
        .entry(value.to_lowercase())
        .or_insert_with(|| value.clone());
    }
  }
  VideoLibraryFilterOptions {
    qualities: qualities.into_iter().collect(),
    countries: countries.into_values().collect(),
    genres: genres.into_values().collect(),
  }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Page {
  items: Vec<Value>,
  total_record_count: usize,
  start_index: Option<usize>,
}

async fn scan(client: &JellyfinClient, key: Key) -> Result<Catalog, JellyfinError> {
  tokio::time::timeout(SCAN_TIMEOUT, scan_complete(client, key))
    .await
    .map_err(|_| failed("Library filter scan exceeded 60 seconds; retry to refresh the library"))?
}

async fn scan_complete(client: &JellyfinClient, key: Key) -> Result<Catalog, JellyfinError> {
  let mut entries = Vec::new();
  let mut ids = HashSet::new();
  let mut expected_total = None;
  let mut bytes = 0;
  loop {
    key.validate(client)?;
    let (page, size) = fetch_page(client, &key, entries.len()).await?;
    bytes += size;
    if bytes > MAX_SCAN_BYTES || page.total_record_count > MAX_ITEMS {
      return Err(failed(
        "Library filter scan exceeds the 20,000-item or 32 MiB metadata limit",
      ));
    }
    if page.start_index.is_some_and(|start| start != entries.len())
      || expected_total.is_some_and(|total| total != page.total_record_count)
      || page.items.len() > PAGE_SIZE as usize
      || entries.len() + page.items.len() > page.total_record_count
      || (page.items.is_empty() && entries.len() < page.total_record_count)
    {
      return Err(failed(
        "Library changed or returned an incomplete filter scan; refresh and retry",
      ));
    }
    expected_total = Some(page.total_record_count);
    for raw in page.items {
      let entry = map_entry(&key, raw)?;
      if !ids.insert(entry.item.id.clone()) {
        return Err(failed(
          "Library returned duplicate items during filter scan; refresh and retry",
        ));
      }
      entries.push(entry);
    }
    if entries.len() == page.total_record_count {
      key.validate(client)?;
      return Ok(Catalog {
        key,
        completed: Instant::now(),
        entries,
      });
    }
  }
}

async fn fetch_page(
  client: &JellyfinClient,
  key: &Key,
  start: usize,
) -> Result<(Page, usize), JellyfinError> {
  let scope = &key.scope;
  let authorization = {
    let state = client.state.read();
    if !scope.matches(&state) {
      return Err(stale());
    }
    JellyfinClient::auth_header_from_parts(
      &state.device_name,
      &state.device_id,
      state.access_token.as_deref(),
    )
  };
  let (path, auth_name) = match scope.provider {
    MediaServerProvider::Jellyfin => ("/Items".to_string(), "Authorization"),
    MediaServerProvider::Emby => (
      format!("/Users/{}/Items", scope.user_id),
      "X-Emby-Authorization",
    ),
  };
  let mut query = emby_browse_items_query(EmbyBrowseItemsQuery {
    library_id: Some(key.library_id.clone()),
    collection_type: key.kind,
    search_term: None,
    start_index: start as i32,
    limit: PAGE_SIZE,
    sort: key.sort,
    sort_direction: key.direction,
    played_filter: key.played,
    favorites_only: key.favorites,
  });
  for (name, value) in &mut query {
    if *name == "Fields" {
      value.push_str(",ProductionLocations,Genres,MediaSources,MediaStreams,Width,Height");
    }
  }
  if scope.provider == MediaServerProvider::Jellyfin {
    for (name, value) in &mut query {
      if *name == "Fields" {
        value.push_str(",RecursiveItemCount");
      } else if *name == "EnableImageTypes" {
        *value = "Primary,Logo,Backdrop,Thumb".to_string();
      }
    }
    query.push(("UserId", scope.user_id.clone()));
    query.push(("CollapseBoxSetItems", "false".to_string()));
  }
  let mut response = client
    .authenticated_http
    .get(format!("{}{path}", scope.server_url))
    .header(
      header::USER_AGENT,
      JellyfinClient::request_user_agent_for(scope.provider),
    )
    .header(auth_name, authorization)
    .query(&query)
    .send()
    .await?;
  if !response.status().is_success() {
    return Err(failed(&format!(
      "Library filter scan returned HTTP {}",
      response.status()
    )));
  }
  if response
    .content_length()
    .is_some_and(|length| length > MAX_PAGE_BYTES as u64)
  {
    return Err(failed("Library filter metadata page exceeds 4 MiB"));
  }
  let mut body = Vec::new();
  while let Some(chunk) = response.chunk().await? {
    if body.len() + chunk.len() > MAX_PAGE_BYTES {
      return Err(failed("Library filter metadata page exceeds 4 MiB"));
    }
    body.extend_from_slice(&chunk);
  }
  key.validate(client)?;
  let page = serde_json::from_slice(&body)
    .map_err(|_| failed("Library returned malformed or incomplete filter metadata"))?;
  Ok((page, body.len()))
}

fn map_entry(key: &Key, raw: Value) -> Result<Entry, JellyfinError> {
  let expected_type = match key.kind {
    VideoLibraryKind::Movies => "Movie",
    VideoLibraryKind::TvShows => "Series",
  };
  if raw.get("Type").and_then(Value::as_str) != Some(expected_type) {
    return Err(failed(
      "Library filter scan returned an unexpected item type",
    ));
  }
  let countries = labels(&raw, "ProductionLocations");
  let genres = labels(&raw, "Genres");
  let qualities = if key.kind == VideoLibraryKind::Movies {
    qualities(&raw, key.scope.provider)
  } else {
    BTreeSet::new()
  };
  let item = match key.scope.provider {
    MediaServerProvider::Jellyfin => {
      let dto = serde_json::from_value(raw)
        .map_err(|_| failed("Library returned invalid Jellyfin filter metadata"))?;
      map_video_library_item(&key.scope.server_url, dto)
    }
    MediaServerProvider::Emby => {
      let dto = serde_json::from_value(raw)
        .map_err(|_| failed("Library returned invalid Emby filter metadata"))?;
      map_emby_video_library_item(&key.scope.server_url, dto)
    }
  }
  .filter(|item| !item.id.is_empty())
  .ok_or_else(|| failed("Library filter scan returned an item without an identity"))?;
  Ok(Entry {
    item,
    countries,
    genres,
    qualities,
  })
}

fn labels(raw: &Value, key: &str) -> Vec<String> {
  raw
    .get(key)
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .map(str::to_owned)
    .collect()
}

fn qualities(raw: &Value, provider: MediaServerProvider) -> BTreeSet<VideoLibraryQuality> {
  let mut result = BTreeSet::new();
  let sources = raw
    .get("MediaSources")
    .and_then(Value::as_array)
    .filter(|sources| !sources.is_empty());
  if let Some(sources) = sources {
    for source in sources {
      stream_qualities(source, provider, &mut result);
    }
  } else {
    stream_qualities(raw, provider, &mut result);
  }
  result
}

fn stream_qualities(
  raw: &Value,
  provider: MediaServerProvider,
  result: &mut BTreeSet<VideoLibraryQuality>,
) {
  for stream in raw
    .get("MediaStreams")
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
  {
    if stream.get("Type").and_then(Value::as_str) != Some("Video") {
      continue;
    }
    let width = stream
      .get("Width")
      .and_then(Value::as_u64)
      .filter(|width| *width > 0);
    let dimension = width.or_else(|| stream.get("Height").and_then(Value::as_u64));
    let (hd, full_hd, uhd) = if width.is_some() {
      (1280, 1920, 3840)
    } else {
      (720, 1080, 2160)
    };
    let quality = match dimension {
      Some(value) if value >= uhd => VideoLibraryQuality::Uhd,
      Some(value) if value >= full_hd => VideoLibraryQuality::FullHd,
      Some(value) if value >= hd => VideoLibraryQuality::Hd,
      _ => continue,
    };
    result.insert(quality);
    let dolby_vision = match provider {
      MediaServerProvider::Jellyfin => matches!(
        stream.get("VideoRangeType").and_then(Value::as_str),
        Some(
          "DOVI"
            | "DOVIWithHDR10"
            | "DOVIWithHLG"
            | "DOVIWithSDR"
            | "DOVIWithEL"
            | "DOVIWithHDR10Plus"
            | "DOVIWithELHDR10Plus"
        )
      ),
      MediaServerProvider::Emby => {
        stream.get("ExtendedVideoType").and_then(Value::as_str) == Some("DolbyVision")
      }
    };
    if quality == VideoLibraryQuality::Uhd && dolby_vision {
      result.insert(VideoLibraryQuality::DolbyVision);
    }
  }
}

fn failed(message: &str) -> JellyfinError {
  JellyfinError::HttpError(message.to_string())
}

fn stale() -> JellyfinError {
  failed("Library filter snapshot is stale; refresh from the first page")
}

#[cfg(test)]
mod tests {
  use std::collections::VecDeque;
  use std::sync::Arc;

  use serde_json::json;
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
  use tokio::net::TcpListener;

  use super::*;

  struct Server {
    url: String,
    requests: Arc<parking_lot::Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
  }

  impl Drop for Server {
    fn drop(&mut self) {
      self.task.abort();
    }
  }

  async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut raw = Vec::new();
    loop {
      let mut chunk = [0; 4096];
      let read = socket.read(&mut chunk).await.expect("read request");
      assert!(read > 0, "request closed before complete headers");
      raw.extend_from_slice(&chunk[..read]);
      if raw.windows(4).any(|window| window == b"\r\n\r\n") {
        return String::from_utf8(raw).expect("HTTP request");
      }
    }
  }

  async fn server(responses: Vec<Value>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind server");
    let url = format!("http://{}", listener.local_addr().expect("server address"));
    let requests = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let captured = requests.clone();
    let mut responses = VecDeque::from(responses);
    let task = tokio::spawn(async move {
      loop {
        let (mut socket, _) = listener.accept().await.expect("accept request");
        let raw = read_request(&mut socket).await;
        captured.lock().push(raw);
        let next = responses.pop_front();
        let status = if next.is_some() {
          "200 OK"
        } else {
          "500 Unexpected Request"
        };
        let body = next.unwrap_or(Value::Null).to_string();
        let response = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        socket
          .write_all(response.as_bytes())
          .await
          .expect("write response");
      }
    });
    Server {
      url,
      requests,
      task,
    }
  }

  fn client(server: &Server, provider: MediaServerProvider) -> JellyfinClient {
    let client = JellyfinClient::with_storage_dir(
      std::env::temp_dir().join(format!("filter-test-{}", Uuid::new_v4())),
    );
    {
      let mut state = client.state.write();
      state.provider = provider;
      state.server_url = Some(server.url.clone());
      state.user_id = Some("00000000000000000000000000000001".to_string());
      state.replace_access_token(Some("test-token".to_string()));
    }
    client
  }

  fn request() -> VideoLibraryPageRequest {
    VideoLibraryPageRequest {
      library_id: "00000000000000000000000000000002".to_string(),
      collection_type: VideoLibraryKind::Movies,
      start_index: 0,
      limit: 2,
      sort: VideoLibrarySort::Title,
      sort_direction: VideoLibrarySortDirection::Ascending,
      played_filter: VideoLibraryPlayedFilter::All,
      favorites_only: false,
      filters: VideoLibraryFilters {
        country: Some("Japan".to_string()),
        ..VideoLibraryFilters::default()
      },
    }
  }

  fn movie(index: u128, country: &str) -> Value {
    json!({
      "Id": Uuid::from_u128(index + 100).to_string(),
      "Type": "Movie", "Name": format!("Movie {index}"),
      "ProductionLocations": [country], "Genres": ["Drama"],
      "MediaSources": [{"Id": "version", "MediaStreams": [{"Type": "Video", "Width": 3840, "Height": 1600}]}]
    })
  }

  fn page(items: Vec<Value>, start: usize, total: usize) -> Value {
    json!({"Items":items,"StartIndex":start,"TotalRecordCount":total})
  }

  fn query(request: &str) -> BTreeMap<String, String> {
    let target = request.split_whitespace().nth(1).expect("request target");
    url::Url::parse(&format!("http://localhost{target}"))
      .expect("query URL")
      .query_pairs()
      .into_owned()
      .collect()
  }

  #[tokio::test]
  async fn both_providers_filter_the_complete_library_before_counting_and_paging() {
    for provider in [MediaServerProvider::Jellyfin, MediaServerProvider::Emby] {
      let items = (0..205)
        .map(|index| movie(index, if index < 100 { "France" } else { "Japan" }))
        .collect::<Vec<_>>();
      let responses = vec![
        page(items[..100].to_vec(), 0, 205),
        page(items[100..200].to_vec(), 100, 205),
        page(items[200..].to_vec(), 200, 205),
      ];
      let server = server(responses).await;
      let client = client(&server, provider);
      let mut request = request();
      request.sort = VideoLibrarySort::RecentlyAdded;
      request.sort_direction = VideoLibrarySortDirection::Descending;
      request.played_filter = VideoLibraryPlayedFilter::Unplayed;
      request.favorites_only = true;
      request.filters.genre = Some("drama".to_string());
      request.filters.quality = Some(VideoLibraryQuality::Uhd);
      let first = client
        .library()
        .browse_video(request.clone())
        .await
        .expect("first filtered page");
      assert_eq!(first.total_record_count, 105);
      assert_eq!(
        first
          .items
          .iter()
          .map(|item| item.name.as_str())
          .collect::<Vec<_>>(),
        vec!["Movie 100", "Movie 101"]
      );
      assert!(first.has_more);
      // TTL is intentionally ignored for every page in an active browse query.
      client
        .filter_catalog
        .catalog
        .lock()
        .await
        .as_mut()
        .expect("catalog")
        .completed = Instant::now() - FRESH_FOR - Duration::from_secs(1);
      request.start_index = 2;
      let second = client
        .library()
        .browse_video(request.clone())
        .await
        .expect("next filtered page");
      assert_eq!(second.total_record_count, 105);
      assert_eq!(
        second
          .items
          .iter()
          .map(|item| item.name.as_str())
          .collect::<Vec<_>>(),
        vec!["Movie 102", "Movie 103"]
      );
      request.start_index = 0;
      let reloaded = client
        .library()
        .browse_video(request)
        .await
        .expect("evicted first page reload");
      assert_eq!(reloaded.total_record_count, first.total_record_count);
      assert_eq!(reloaded.items[0].id, first.items[0].id);
      let requests = server.requests.lock();
      assert_eq!(
        requests.len(),
        3,
        "continuation must reuse the same complete snapshot"
      );
      for (index, raw) in requests.iter().enumerate() {
        let query = query(raw);
        assert_eq!(query["StartIndex"], (index * 100).to_string());
        assert_eq!(query["Limit"], "100");
        assert_eq!(query["IsPlayed"], "false");
        assert_eq!(query["IsFavorite"], "true");
        assert_eq!(query["SortBy"], "DateCreated");
        assert_eq!(query["SortOrder"], "Descending");
        assert_eq!(query["ParentId"], "00000000000000000000000000000002");
        assert!(query["Fields"].contains("ProductionLocations,Genres,MediaSources,MediaStreams"));
        match provider {
          MediaServerProvider::Jellyfin => {
            assert!(raw.starts_with("GET /Items?"));
            assert_eq!(query["UserId"], "00000000000000000000000000000001");
          }
          MediaServerProvider::Emby => {
            assert!(raw.starts_with("GET /Users/00000000000000000000000000000001/Items?"))
          }
        }
      }
    }
  }

  #[tokio::test]
  async fn facets_and_dolby_vision_use_actual_same_version_metadata_for_both_providers() {
    for provider in [MediaServerProvider::Jellyfin, MediaServerProvider::Emby] {
      let mut separate_versions = movie(0, " Japan ");
      separate_versions["MediaSources"] = json!([
        {"Id":"4k-sdr","MediaStreams":[{"Type":"Video","Width":3840,"Height":1600}]},
        {"Id":"1080-dv","MediaStreams":[{"Type":"Video","Width":1920,"Height":800,"VideoRangeType":"DOVI","ExtendedVideoType":"DolbyVision"}]}
      ]);
      let mut dolby_vision = movie(1, "France");
      dolby_vision["Genres"] = json!(["Action", "Drama"]);
      dolby_vision["MediaSources"][0]["MediaStreams"][0]["VideoRangeType"] = json!("DOVIWithHDR10");
      dolby_vision["MediaSources"][0]["MediaStreams"][0]["ExtendedVideoType"] =
        json!("DolbyVision");
      let mut unknown = movie(2, "japan");
      unknown["MediaSources"] = json!([]);
      unknown["MediaStreams"] = json!([{"Type":"Video","Width":3840,"Height":2160,"VideoRangeType":"DOVIInvalid","ExtendedVideoType":"None"}]);
      let mut hd = movie(3, "France");
      hd["MediaSources"][0]["MediaStreams"] = json!([{"Type":"Video","Height":720}]);
      let server = server(vec![page(
        vec![separate_versions, dolby_vision, unknown, hd],
        0,
        4,
      )])
      .await;
      let client = client(&server, provider);
      let mut request = request();
      let facets = client
        .library()
        .filter_options(request.library_id.clone(), VideoLibraryKind::Movies)
        .await
        .expect("complete facets");
      assert_eq!(facets.countries, vec!["France", "Japan"]);
      assert_eq!(facets.genres, vec!["Action", "Drama"]);
      assert_eq!(
        facets.qualities,
        vec![
          VideoLibraryQuality::Hd,
          VideoLibraryQuality::FullHd,
          VideoLibraryQuality::Uhd,
          VideoLibraryQuality::DolbyVision
        ]
      );
      request.filters = VideoLibraryFilters {
        quality: Some(VideoLibraryQuality::DolbyVision),
        ..VideoLibraryFilters::default()
      };
      let filtered = client
        .library()
        .browse_video(request)
        .await
        .expect("DV page");
      assert_eq!(
        filtered.total_record_count, 1,
        "DV cannot combine the 4k SDR and 1080 DV versions"
      );
      assert_eq!(filtered.items[0].name, "Movie 1");
      assert_eq!(server.requests.lock().len(), 1);
    }
  }

  #[tokio::test]
  async fn series_never_claim_episode_quality_but_keep_country_and_genre_facets() {
    for provider in [MediaServerProvider::Jellyfin, MediaServerProvider::Emby] {
      let mut series = movie(0, "Japan");
      series["Type"] = json!("Series");
      let server = server(vec![page(vec![series], 0, 1)]).await;
      let client = client(&server, provider);
      let options = client
        .library()
        .filter_options(request().library_id, VideoLibraryKind::TvShows)
        .await
        .expect("series facets");
      assert!(options.qualities.is_empty());
      assert_eq!(options.countries, vec!["Japan"]);
      assert_eq!(options.genres, vec!["Drama"]);
    }
  }

  #[tokio::test]
  async fn facet_scan_preserves_an_active_restricted_browse_snapshot() {
    let items = vec![movie(0, "Japan"), movie(1, "Japan"), movie(2, "Japan")];
    let server = server(vec![
      page(items, 0, 3),
      page(vec![movie(3, "France")], 0, 1),
    ])
    .await;
    let client = client(&server, MediaServerProvider::Jellyfin);
    let mut request = request();
    request.played_filter = VideoLibraryPlayedFilter::Unplayed;
    let first = client
      .library()
      .browse_video(request.clone())
      .await
      .expect("restricted page");
    assert_eq!(first.total_record_count, 3);
    let facets = client
      .library()
      .filter_options(request.library_id.clone(), request.collection_type)
      .await
      .expect("whole-library facets");
    assert_eq!(facets.countries, vec!["France"]);
    request.start_index = 2;
    let next = client
      .library()
      .browse_video(request)
      .await
      .expect("unchanged continuation after panel cancellation");
    assert_eq!(next.items[0].name, "Movie 2");
    assert_eq!(next.total_record_count, 3);
    assert_eq!(server.requests.lock().len(), 2);
    assert!(!query(&server.requests.lock()[1]).contains_key("IsPlayed"));
  }

  #[tokio::test]
  async fn incomplete_duplicate_and_over_limit_scans_never_publish_partial_results() {
    let cases = vec![
      vec![page(vec![movie(0, "Japan")], 0, 2), page(vec![], 1, 2)],
      vec![page(vec![movie(0, "Japan")], 0, 2)],
      vec![
        page(vec![movie(0, "Japan")], 0, 2),
        page(vec![movie(1, "Japan")], 1, 3),
      ],
      vec![page(vec![movie(0, "Japan"), movie(0, "Japan")], 0, 2)],
      vec![page(vec![], 0, MAX_ITEMS + 1)],
      vec![json!({"Items":[movie(0,"Japan")],"StartIndex":0})],
      vec![page(vec![movie(0, "Japan")], 10, 1)],
    ];
    for responses in cases {
      let server = server(responses).await;
      let client = client(&server, MediaServerProvider::Jellyfin);
      assert!(client.library().browse_video(request()).await.is_err());
      assert!(client.filter_catalog.catalog.lock().await.is_none());
    }
  }

  #[tokio::test]
  async fn invalidation_and_session_changes_reject_continuation_without_rescanning() {
    let responses = vec![
      page(
        vec![movie(0, "Japan"), movie(1, "Japan"), movie(2, "Japan")],
        0,
        3,
      ),
      page(vec![movie(3, "Japan")], 0, 1),
      page(vec![movie(4, "Japan")], 0, 1),
    ];
    let server = server(responses).await;
    let client = client(&server, MediaServerProvider::Jellyfin);
    client
      .library()
      .browse_video(request())
      .await
      .expect("first snapshot");
    client.library().invalidate_filter_catalog();
    let mut next = request();
    next.start_index = 2;
    assert!(client
      .library()
      .browse_video(next.clone())
      .await
      .expect_err("stale continuation")
      .to_string()
      .contains("stale"));
    assert_eq!(server.requests.lock().len(), 1);
    assert_eq!(
      client
        .library()
        .browse_video(request())
        .await
        .expect("refreshed snapshot")
        .items[0]
        .name,
      "Movie 3"
    );
    {
      let mut state = client.state.write();
      // Even reauthentication as the same server/user has a distinct epoch.
      state.replace_access_token(Some("new-session-token".to_string()));
    }
    assert!(client.library().browse_video(next).await.is_err());
    assert_eq!(server.requests.lock().len(), 2);
    assert_eq!(
      client
        .library()
        .browse_video(request())
        .await
        .expect("new session snapshot")
        .items[0]
        .name,
      "Movie 4"
    );
    assert!(server.requests.lock()[2].contains("new-session-token"));
  }

  #[tokio::test]
  async fn confirmed_user_data_mutations_invalidate_shared_filter_snapshots() {
    for provider in [MediaServerProvider::Jellyfin, MediaServerProvider::Emby] {
      let server = server(vec![
        page(vec![movie(0, "Japan"), movie(1, "Japan")], 0, 2),
        json!({"Key":"item","IsFavorite":true,"Played":false}),
        page(vec![movie(0, "Japan"), movie(1, "Japan")], 0, 2),
        Value::Null,
      ])
      .await;
      let client = client(&server, provider);
      let first = client
        .library()
        .browse_video(request())
        .await
        .expect("initial page");
      client
        .library()
        .update_user_data(VideoUserDataUpdateRequest {
          item_id: first.items[0].id.clone(),
          action: VideoUserDataAction::Favorite,
        })
        .await
        .expect("confirmed favorite mutation");
      let mut continuation = request();
      continuation.start_index = 1;
      assert!(client
        .library()
        .browse_video(continuation.clone())
        .await
        .is_err());
      assert_eq!(server.requests.lock().len(), 2);
      client
        .library()
        .browse_video(request())
        .await
        .expect("new snapshot after favorite");
      client
        .playback()
        .report_playback_stop(&PlaybackStopInfo {
          item_id: first.items[0].id.clone(),
          media_source_id: None,
          play_session_id: None,
          position_ticks: Some(1_000_000),
        })
        .await
        .expect("confirmed playback stop");
      assert!(client.library().browse_video(continuation).await.is_err());
      assert_eq!(server.requests.lock().len(), 4);
    }
  }

  #[tokio::test]
  async fn cancelled_scan_releases_the_cache_without_publishing_partial_metadata() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("address"));
    let (arrived_tx, arrived_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    let server = Server {
      url,
      requests: Arc::default(),
      task: tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let _request = read_request(&mut socket).await;
        arrived_tx.send(()).expect("notify request");
        let _ = release_rx.await;
      }),
    };
    let client = client(&server, MediaServerProvider::Jellyfin);
    let scanning = client.clone();
    let task = tokio::spawn(async move { scanning.library().browse_video(request()).await });
    arrived_rx.await.expect("scan started");
    task.abort();
    assert!(task.await.expect_err("cancelled task").is_cancelled());
    assert!(client
      .filter_catalog
      .catalog
      .try_lock()
      .expect("released cache")
      .is_none());
    drop(release_tx);
  }

  #[tokio::test]
  async fn session_change_during_http_scan_cannot_publish_old_metadata() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("address"));
    let (arrived_tx, arrived_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let server = Server {
      url,
      requests: Arc::default(),
      task: tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let _request = read_request(&mut socket).await;
        arrived_tx.send(()).expect("notify request");
        release_rx.await.expect("release response");
        let body = page(vec![movie(0, "Japan")], 0, 1).to_string();
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.expect("response");
      }),
    };
    let client = client(&server, MediaServerProvider::Jellyfin);
    let scanning = client.clone();
    let task = tokio::spawn(async move { scanning.library().browse_video(request()).await });
    arrived_rx.await.expect("scan started");
    {
      let mut state = client.state.write();
      state.user_id = Some("00000000000000000000000000000003".to_string());
      state.replace_access_token(Some("other-account".to_string()));
    }
    release_tx.send(()).expect("release");
    assert!(task
      .await
      .expect("scan task")
      .expect_err("stale account scan")
      .to_string()
      .contains("stale"));
    assert!(client.filter_catalog.catalog.lock().await.is_none());
  }
}
