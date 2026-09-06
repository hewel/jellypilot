//! Saved-profile user photos through a dedicated Library Image adapter (ADR
//! 0028): each load authenticates with the profile's own stored session, so
//! rows for accounts that are not the active connection still fetch. Loads are
//! keyed by profile identity; the signed image reference stays an internal
//! detail of the load task.

use std::sync::Arc;

use iced::widget::image;
use iced::Task;
use jellypilot_auth::{SavedProfileKey, SavedProfileSummary};
use jellypilot_media_server::artwork::{ArtworkError, ArtworkRaster, ArtworkSizeClass, LoadLane};
use jellypilot_media_server::{user_image_id, JellyfinClient};

use super::kernel::Kernel;
use super::message::Message;

pub(crate) type AvatarLoadOutcome = Option<Result<ArtworkRaster, ArtworkError>>;

/// Fires one Library Image load per saved profile whose photo is neither
/// cached nor in flight, and prunes photos of profiles no longer saved.
/// Idempotent: safe to fire on any profile-set or activation change.
pub(crate) fn refresh(kernel: &mut Kernel, profiles: &[SavedProfileSummary]) -> Task<Message> {
  let keys: Vec<SavedProfileKey> = profiles
    .iter()
    .map(|profile| profile.key().clone())
    .collect();
  kernel.profile_avatars.retain(&keys);

  let mut tasks = Vec::new();
  for profile in profiles {
    let key = profile.key().clone();
    if !kernel.profile_avatars.begin_loading(key.clone()) {
      continue;
    }
    let store = kernel.auth_store.clone();
    // The dedicated adapter keeps avatar loads alive through surface
    // navigation, which cancels previous-generation work on the main one.
    let adapter = Arc::clone(&kernel.avatar_adapter);
    tasks.push(Task::perform(
      async move {
        let outcome = {
          let key = key.clone();
          async move {
            let session = store.load_session(key).await.ok()?;
            let image_id =
              user_image_id(session.provider, &session.server_url, &session.user_id).ok()?;
            let client = JellyfinClient::new();
            client.login().adopt_validated_session(&session);
            let (result, _observation) = adapter
              .load(
                &client,
                &image_id,
                ArtworkSizeClass::Avatar,
                LoadLane::Offscreen,
              )
              .await;
            Some(result)
          }
        };
        (key, outcome.await)
      },
      |(key, outcome)| Message::ProfileAvatarLoaded { key, outcome },
    ));
  }
  Task::batch(tasks)
}

/// Applies a settled load: successful rasters become cached photos; every
/// other outcome clears the in-flight mark so a later refresh may retry,
/// leaving the initial-tile fallback in place.
pub(crate) fn settle(kernel: &mut Kernel, key: SavedProfileKey, outcome: AvatarLoadOutcome) {
  match outcome {
    Some(Ok(raster)) => {
      let (width, height, pixels, ..) = raster.into_parts();
      kernel
        .profile_avatars
        .insert(key, image::Handle::from_rgba(width, height, pixels));
    }
    Some(Err(error)) => {
      tracing::debug!(?error, "profile avatar load failed");
      kernel.profile_avatars.finish_loading(&key);
    }
    None => {
      tracing::debug!("profile avatar load skipped: session or image reference unavailable");
      kernel.profile_avatars.finish_loading(&key);
    }
  }
}
