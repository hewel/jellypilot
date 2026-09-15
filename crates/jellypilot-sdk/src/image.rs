//! Library image request resolution for platform image modules.
//!
//! Rust owns image references and access rules; the platform owns fetching,
//! caching, and decoding. [`Sdk::image_target`] resolves a signed image id
//! into an origin URL plus the headers the platform fetcher must send. The
//! authorization value is secret-bearing: it belongs to the image module's
//! request only, never to UI state, logs, or persisted records.

use crate::{ProfileScopeRef, Sdk, SdkError};

/// A resolved image request for the platform image fetcher.
#[derive(Clone)]
pub struct LibraryImageTarget {
    /// Provider-sized origin URL to GET.
    pub url: String,
    /// `Authorization` header value (secret-bearing).
    pub authorization: String,
    /// `User-Agent` header value matching the provider's expectations.
    pub user_agent: String,
    /// `Accept` header value advertising decodable formats.
    pub accept: String,
}

impl std::fmt::Debug for LibraryImageTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LibraryImageTarget")
            .field("url", &self.url)
            .field("authorization", &"[redacted]")
            .field("user_agent", &self.user_agent)
            .field("accept", &self.accept)
            .finish()
    }
}

impl Sdk {
    /// Resolves a signed image id into a fetch target for `scope`'s session.
    ///
    /// `scope` must come from [`crate::OperationToken::scope_ref`]; the
    /// caller captured it when the image work was queued. Scope validation
    /// and the client clone happen under one state lock, so artwork queued
    /// under one account cannot resolve against a later account's
    /// credentials. `max_width` clamps the server-side resize to the
    /// caller's decode-class source width.
    pub fn image_target(
        &self,
        scope: &ProfileScopeRef,
        image_id: String,
        max_width: Option<u32>,
    ) -> Result<LibraryImageTarget, SdkError> {
        let client = {
            let state = self.inner.state.lock().map_err(|_| SdkError::Closed)?;
            if state.closed {
                return Err(SdkError::Closed);
            }
            let active = state.active.as_ref().ok_or(SdkError::NoActiveProfile)?;
            if state.epoch != scope.generation || active.key.as_str() != scope.profile_key {
                return Err(SdkError::Stale);
            }
            std::sync::Arc::clone(&active.client)
        };
        let library = client.library();
        let request = match max_width {
            Some(width) => library.image_request_with_max_width(&image_id, width),
            None => library.image_request(&image_id),
        }
        .map_err(SdkError::from)?;
        let headers = library
            .image_request_headers(&request)
            .map_err(SdkError::from)?;
        Ok(LibraryImageTarget {
            url: request.origin_url().to_owned(),
            authorization: headers.authorization().to_owned(),
            user_agent: headers.user_agent().to_owned(),
            accept: headers.accept().to_owned(),
        })
    }
}
