use super::{JellyfinClient, JellyfinError, JellyfinLogin, MediaServerProvider, ServerIdentity};

const PUBLIC_INFO_PATH: &str = "/System/Info/Public";
const MAX_PUBLIC_INFO_BYTES: usize = 64 * 1024;

impl JellyfinLogin<'_> {
  /// Resolves public server identity without using or changing authentication.
  ///
  /// Probes the supplied base and its conventional Emby suffix. A missing
  /// product declaration remains ambiguous; custom names and version numbers
  /// are not reliable provider identities. Dropping the future cancels I/O.
  pub async fn probe_server(&self, server_url: &str) -> Result<ServerIdentity, JellyfinError> {
    let candidates = JellyfinClient::emby_api_base_candidates(server_url)?;
    let mut failure = None;
    let mut restricted = false;
    for candidate in candidates {
      match self.client.probe_server_at_base(&candidate).await {
        Ok(Some(identity)) => return Ok(identity),
        Ok(None) => {}
        Err(JellyfinError::ServerInfoRestricted) => restricted = true,
        Err(error) => failure = Some(error),
      }
    }
    Err(failure.unwrap_or_else(|| {
      if restricted {
        JellyfinError::ServerInfoRestricted
      } else {
        JellyfinError::HttpError("Server discovery failed".to_owned())
      }
    }))
  }
}

impl JellyfinClient {
  async fn probe_server_at_base(
    &self,
    base: &str,
  ) -> Result<Option<ServerIdentity>, JellyfinError> {
    // Reuse public discovery's token-free headers, timeout and same-origin
    // redirects. Keep the final response URL so redirects resolve the API base.
    let configuration = self.public_emby_openapi_configuration(base)?;
    let mut response = configuration
      .client
      .get(format!("{base}{PUBLIC_INFO_PATH}"))
      .header(reqwest::header::ACCEPT, "application/json")
      .header(reqwest::header::USER_AGENT, Self::emby_chrome_user_agent())
      .send()
      .await
      .map_err(|error| Self::redacted_request_error("Server discovery", error))?;
    match response.status() {
      reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
        return Err(JellyfinError::ServerInfoRestricted);
      }
      reqwest::StatusCode::NOT_FOUND => return Ok(None),
      _ => {}
    }
    if !response.status().is_success() {
      return Err(Self::redacted_response_error(
        "Server discovery",
        response.status(),
        false,
      ));
    }
    let mut resolved = response.url().clone();
    let path = resolved.path();
    let suffix_start = path.len().saturating_sub(PUBLIC_INFO_PATH.len());
    if !path
      .get(suffix_start..)
      .is_some_and(|suffix| suffix.eq_ignore_ascii_case(PUBLIC_INFO_PATH))
    {
      return Err(JellyfinError::HttpError(
        "Server discovery returned an unexpected endpoint".to_owned(),
      ));
    }
    let resolved_path = path[..suffix_start].to_owned();
    resolved.set_path(&resolved_path);
    let server_url = Self::normalize_server_url(resolved.as_str()).map_err(|_| {
      JellyfinError::HttpError("Server discovery returned an invalid API base".to_owned())
    })?;
    let mut body = Vec::new();
    while let Some(chunk) = response
      .chunk()
      .await
      .map_err(|error| Self::redacted_request_error("Server discovery", error))?
    {
      if body.len().saturating_add(chunk.len()) > MAX_PUBLIC_INFO_BYTES {
        return Err(JellyfinError::HttpError(
          "Server discovery response is too large".to_owned(),
        ));
      }
      body.extend_from_slice(&chunk);
    }
    let public: jellyfin_api::models::PublicSystemInfo =
      serde_json::from_slice(&body).map_err(|_| {
        JellyfinError::HttpError("Server discovery returned malformed JSON".to_owned())
      })?;
    let provider = match public.product_name.as_ref().and_then(Option::as_deref) {
      Some(product) if product.eq_ignore_ascii_case("Jellyfin Server") => {
        Some(MediaServerProvider::Jellyfin)
      }
      Some(product) if product.eq_ignore_ascii_case("Emby Server") => {
        Some(MediaServerProvider::Emby)
      }
      _ => None,
    };
    let info = Self::server_info_from_openapi(public)?;
    if [&info.server_name, &info.version, &info.id]
      .iter()
      .any(|field| field.trim().is_empty())
    {
      return Err(JellyfinError::HttpError(
        "Server discovery returned incomplete public information".to_owned(),
      ));
    }
    Ok(Some(ServerIdentity {
      server_url,
      server_name: info.server_name,
      provider,
    }))
  }
}
