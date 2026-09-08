//! Display-free demand and completion correlation for Library Images.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};

use jellypilot_media_server::artwork::{ArtworkSizeClass, DerivedArtwork};

/// The chosen image at one domain location, including its raster variant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageSpec {
    pub key: String,
    pub image_id: String,
    pub size_class: ArtworkSizeClass,
    pub derived: DerivedArtwork,
}

/// Current measured demand for a Library Image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImagePriority {
    Visible,
    Prefetch,
}

/// Process-unique identity of one load attempt, never reused by another lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ImageToken(u64);

impl ImageToken {
    fn fresh() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        // Exhaustion must fail rather than let an old completion match a new attempt.
        // Relaxed ordering suffices: the serial establishes identity, not publication.
        let serial = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .unwrap_or_else(|_| panic!("Library Image token identity exhausted"));
        Self(serial)
    }
}

/// Outcome reported by the concrete image loader.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageOutcome {
    Ready,
    Failed,
    Cancelled,
}

/// Observable state of an actively demanded image location.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageStatus {
    Loading,
    Ready,
    Failed,
}

/// Effects for the native owner to apply in order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImageChange {
    Load {
        token: ImageToken,
        spec: ImageSpec,
        priority: ImagePriority,
    },
    Priority {
        token: ImageToken,
        priority: ImagePriority,
    },
    Remove {
        token: ImageToken,
        key: String,
    },
}

#[derive(Debug)]
struct ImageEntry {
    token: ImageToken,
    spec: ImageSpec,
    priority: ImagePriority,
    status: ImageStatus,
}

/// Tracks only visible/prefetch demand; owns neither runtime work nor raster data.
///
/// Removing demand forgets the location. Failed demand remains failed until its
/// identity changes or it leaves and reenters the demand window.
#[derive(Debug, Default)]
pub struct ImageLifecycle {
    // A tree releases storage as entries leave instead of retaining peak capacity
    // after a large page is replaced by a small one.
    entries: BTreeMap<String, ImageEntry>,
    keys: BTreeMap<u64, String>,
}

impl ImageLifecycle {
    /// Updates exactly one location. Unchanged observations do not restart work.
    /// Replacement emits removal before loading the new identity.
    pub fn observe(
        &mut self,
        spec: ImageSpec,
        priority: Option<ImagePriority>,
    ) -> Vec<ImageChange> {
        let Some(priority) = priority else {
            return self.remove(&spec.key).into_iter().collect();
        };
        if let Some(entry) = self.entries.get_mut(&spec.key) {
            if entry.spec == spec {
                if entry.priority == priority {
                    return Vec::new();
                }
                entry.priority = priority;
                return vec![ImageChange::Priority {
                    token: entry.token,
                    priority,
                }];
            }
        }

        let token = ImageToken::fresh();
        let mut changes = Vec::new();
        if let Some(removed) = self.remove(&spec.key) {
            changes.push(removed);
        }
        self.keys.insert(token.0, spec.key.clone());
        self.entries.insert(
            spec.key.clone(),
            ImageEntry {
                token,
                spec: spec.clone(),
                priority,
                status: ImageStatus::Loading,
            },
        );
        changes.push(ImageChange::Load {
            token,
            spec,
            priority,
        });
        changes
    }

    /// Removes absent or replaced identities without admitting any new demand.
    pub fn retain(&mut self, specs: &[ImageSpec]) -> Vec<ImageChange> {
        let wanted: HashMap<_, _> = specs.iter().map(|spec| (spec.key.as_str(), spec)).collect();
        let mut changes = Vec::new();
        self.entries.retain(|key, entry| {
            if wanted
                .get(key.as_str())
                .is_some_and(|spec| **spec == entry.spec)
            {
                true
            } else {
                self.keys.remove(&entry.token.0);
                changes.push(ImageChange::Remove {
                    token: entry.token,
                    key: key.clone(),
                });
                false
            }
        });
        changes
    }

    /// Revokes all current demand. Future tokens remain distinct across clear/reset.
    pub fn clear(&mut self) -> Vec<ImageChange> {
        self.keys.clear();
        std::mem::take(&mut self.entries)
            .into_iter()
            .map(|(key, entry)| ImageChange::Remove {
                token: entry.token,
                key,
            })
            .collect()
    }

    /// Settles one current loading attempt, returning its location for native state.
    /// Stale and duplicate completions are ignored; cancellation forgets demand.
    pub fn settle(&mut self, token: ImageToken, outcome: ImageOutcome) -> Option<String> {
        let key = self.keys.get(&token.0)?;
        let entry = self.entries.get_mut(key)?;
        if entry.status != ImageStatus::Loading {
            return None;
        }
        let key = key.clone();
        match outcome {
            ImageOutcome::Ready => entry.status = ImageStatus::Ready,
            ImageOutcome::Failed => entry.status = ImageStatus::Failed,
            ImageOutcome::Cancelled => {
                let _ = self.remove(&key);
            }
        }
        Some(key)
    }

    /// Resolves a live attempt's location in logarithmic time without copying it.
    #[must_use]
    pub fn key(&self, token: ImageToken) -> Option<&str> {
        self.keys.get(&token.0).map(String::as_str)
    }

    /// State of an actively demanded location; hidden locations have no state.
    #[must_use]
    pub fn status(&self, key: &str) -> Option<ImageStatus> {
        self.entries.get(key).map(|entry| entry.status)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn has_loading(&self) -> bool {
        self.entries
            .values()
            .any(|entry| entry.status == ImageStatus::Loading)
    }

    fn remove(&mut self, key: &str) -> Option<ImageChange> {
        let (key, entry) = self.entries.remove_entry(key)?;
        self.keys.remove(&entry.token.0);
        Some(ImageChange::Remove {
            token: entry.token,
            key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(key: &str, image_id: &str) -> ImageSpec {
        ImageSpec {
            key: key.into(),
            image_id: image_id.into(),
            size_class: ArtworkSizeClass::Card,
            derived: DerivedArtwork::default(),
        }
    }

    fn load(lifecycle: &mut ImageLifecycle, spec: ImageSpec) -> ImageToken {
        let changes = lifecycle.observe(spec, Some(ImagePriority::Visible));
        let [ImageChange::Load { token, .. }] = changes.as_slice() else {
            panic!("expected one new load, got {changes:?}");
        };
        *token
    }

    #[test]
    fn same_image_reentry_rejects_the_previous_attempt() {
        let mut lifecycle = ImageLifecycle::default();
        let image = spec("card", "poster");
        let old = load(&mut lifecycle, image.clone());
        assert_eq!(
            lifecycle.observe(image.clone(), None),
            vec![ImageChange::Remove {
                token: old,
                key: "card".into(),
            }]
        );
        let current = load(&mut lifecycle, image);
        assert_eq!(lifecycle.settle(old, ImageOutcome::Ready), None);
        assert_eq!(lifecycle.status("card"), Some(ImageStatus::Loading));
        assert_eq!(
            lifecycle.settle(current, ImageOutcome::Ready),
            Some("card".into())
        );
    }

    #[test]
    fn clear_and_recreated_lifecycles_never_accept_old_tokens() {
        let image = spec("card", "poster");
        let mut lifecycle = ImageLifecycle::default();
        let old = load(&mut lifecycle, image.clone());
        lifecycle.clear();
        let after_clear = load(&mut lifecycle, image.clone());
        assert_eq!(lifecycle.settle(old, ImageOutcome::Ready), None);
        let mut recreated = ImageLifecycle::default();
        let current = load(&mut recreated, image);
        assert_eq!(recreated.settle(old, ImageOutcome::Ready), None);
        assert_eq!(recreated.settle(after_clear, ImageOutcome::Ready), None);
        assert_eq!(
            recreated.settle(current, ImageOutcome::Ready),
            Some("card".into())
        );
    }

    #[test]
    fn scrolling_retains_pending_overlap_without_admitting_hidden_candidates() {
        let mut lifecycle = ImageLifecycle::default();
        let leaving = spec("first", "a");
        let overlap = spec("second", "b");
        let arriving = spec("third", "c");
        let first = load(&mut lifecycle, leaving);
        let second = load(&mut lifecycle, overlap.clone());
        assert_eq!(
            lifecycle.retain(&[overlap.clone(), arriving]),
            vec![ImageChange::Remove {
                token: first,
                key: "first".into(),
            }]
        );
        assert!(lifecycle
            .observe(overlap, Some(ImagePriority::Visible))
            .is_empty());
        assert_eq!(lifecycle.status("third"), None);
        assert_eq!(
            lifecycle.settle(second, ImageOutcome::Ready),
            Some("second".into())
        );
    }

    #[test]
    fn priority_changes_keep_the_pending_attempt_and_emit_only_changes() {
        let mut lifecycle = ImageLifecycle::default();
        let image = spec("card", "poster");
        let token = load(&mut lifecycle, image.clone());
        for priority in [ImagePriority::Prefetch, ImagePriority::Visible] {
            assert_eq!(
                lifecycle.observe(image.clone(), Some(priority)),
                vec![ImageChange::Priority { token, priority }]
            );
            assert!(lifecycle.observe(image.clone(), Some(priority)).is_empty());
        }
        assert_eq!(
            lifecycle.settle(token, ImageOutcome::Ready),
            Some("card".into())
        );
    }

    #[test]
    fn failed_demand_does_not_retry_until_removed() {
        let mut lifecycle = ImageLifecycle::default();
        let image = spec("card", "poster");
        let token = load(&mut lifecycle, image.clone());
        lifecycle.settle(token, ImageOutcome::Failed);
        assert!(lifecycle
            .observe(image.clone(), Some(ImagePriority::Visible))
            .is_empty());
        assert!(lifecycle.retain(std::slice::from_ref(&image)).is_empty());
        lifecycle.observe(image.clone(), Some(ImagePriority::Prefetch));
        assert_eq!(lifecycle.status("card"), Some(ImageStatus::Failed));
        assert_eq!(lifecycle.settle(token, ImageOutcome::Ready), None);
        lifecycle.observe(image.clone(), None);
        let retry = load(&mut lifecycle, image);
        assert_eq!(
            lifecycle.settle(retry, ImageOutcome::Ready),
            Some("card".into())
        );
    }

    #[test]
    fn cancellation_forgets_demand_and_allows_a_fresh_attempt() {
        let mut lifecycle = ImageLifecycle::default();
        let image = spec("card", "poster");
        let cancelled = load(&mut lifecycle, image.clone());
        assert_eq!(
            lifecycle.settle(cancelled, ImageOutcome::Cancelled),
            Some("card".into())
        );
        assert!(lifecycle.is_empty());
        let retry = load(&mut lifecycle, image);
        assert_eq!(lifecycle.settle(cancelled, ImageOutcome::Ready), None);
        assert_eq!(
            lifecycle.settle(retry, ImageOutcome::Ready),
            Some("card".into())
        );
    }

    #[test]
    fn same_image_at_distinct_locations_has_independent_demand() {
        let mut lifecycle = ImageLifecycle::default();
        let season = spec("season/episode", "episode");
        let next = spec("next-up/episode", "episode");
        let season_token = load(&mut lifecycle, season.clone());
        let next_token = load(&mut lifecycle, next);
        lifecycle.observe(season, None);
        assert_eq!(lifecycle.settle(season_token, ImageOutcome::Ready), None);
        assert_eq!(
            lifecycle.settle(next_token, ImageOutcome::Ready),
            Some("next-up/episode".into())
        );
        assert_eq!(lifecycle.len(), 1);
    }

    #[test]
    fn changed_raster_identity_removes_old_demand_before_reloading() {
        let mut lifecycle = ImageLifecycle::default();
        let mut image = spec("logo", "image");
        let old = load(&mut lifecycle, image.clone());
        image.derived.logo_shadow = true;
        let changes = lifecycle.observe(image.clone(), Some(ImagePriority::Visible));
        assert!(
            matches!(changes.as_slice(), [ImageChange::Remove { token, .. }, ImageChange::Load { spec, .. }]
      if *token == old && *spec == image)
        );
        assert_eq!(lifecycle.settle(old, ImageOutcome::Ready), None);
        image.size_class = ArtworkSizeClass::Hero;
        assert!(matches!(
            lifecycle.retain(&[image]).as_slice(),
            [ImageChange::Remove { .. }]
        ));
        assert!(lifecycle.is_empty());
    }

    #[test]
    fn scrolling_churn_retains_only_the_current_demand_window() {
        let mut lifecycle = ImageLifecycle::default();
        for start in 0..1_000 {
            let window: Vec<_> = (start..start + 3)
                .map(|index| spec(&index.to_string(), "poster"))
                .collect();
            lifecycle.retain(&window);
            for image in window {
                lifecycle.observe(image, Some(ImagePriority::Prefetch));
            }
            assert_eq!(lifecycle.len(), 3);
            if start > 0 {
                assert_eq!(lifecycle.status(&(start - 1).to_string()), None);
            }
        }
        assert_eq!(lifecycle.clear().len(), 3);
        assert!(lifecycle.is_empty());
        assert!(!lifecycle.has_loading());
    }

    #[test]
    fn token_location_queries_forget_every_revoked_attempt() {
        let mut lifecycle = ImageLifecycle::default();
        let image = spec("card", "poster");
        let first = load(&mut lifecycle, image.clone());
        assert_eq!(lifecycle.key(first), Some("card"));
        lifecycle.observe(image.clone(), None);
        assert_eq!(lifecycle.key(first), None);

        let cancelled = load(&mut lifecycle, image.clone());
        lifecycle.settle(cancelled, ImageOutcome::Cancelled);
        assert_eq!(lifecycle.key(cancelled), None);

        let replaced = load(&mut lifecycle, image.clone());
        lifecycle.retain(&[spec("card", "different")]);
        assert_eq!(lifecycle.key(replaced), None);

        let cleared = load(&mut lifecycle, image);
        lifecycle.settle(cleared, ImageOutcome::Ready);
        assert_eq!(lifecycle.key(cleared), Some("card"));
        lifecycle.clear();
        assert_eq!(lifecycle.key(cleared), None);
    }
}
