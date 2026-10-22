//! Scoped query ownership, persistence and page execution for native clients.

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use jellypilot_core::browse::fetch_browse_page;
pub use jellypilot_core::browse_model::{
    AppliedSavedBrowse, BrowseCondition, BrowsePreferences, BrowsePresentation, BrowseSource,
};
use jellypilot_core::browse_model::{
    BrowseDeliveryToken, BrowseEffect, BrowseModel, BrowsePageRequest, BrowsePageSettlement,
    BrowseQueryPolicy, LibraryBrowseView,
};
use jellypilot_core::config::{BrowseFilterSettings, SettingsStore};
use jellypilot_core::request_gate::RequestGate;
use jellypilot_core::{LibraryBrowseCoreError, LibraryBrowseFailure};
use jellypilot_media_server::{JellyfinClient, VideoLibraryShortcut, VideoUserDataUpdate};
use tokio::runtime::Handle;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::{OperationToken, ProfileScopeRef, Sdk, SdkError, SdkInner, SdkState};

#[derive(Default)]
pub(super) struct BrowseWork {
    reset_viewport: bool,
    requests: Vec<PageRequest>,
    changed: bool,
    cancellations: Vec<CancellationToken>,
}

/// One physical request, cancelled by its owning Browser when obsolete.
struct PageRequest {
    request: BrowsePageRequest,
    client: Option<Arc<JellyfinClient>>,
    cancel: CancellationToken,
}

impl PageRequest {
    pub async fn run(self) -> BrowsePageSettlement {
        let Self {
            request,
            client,
            cancel,
        } = self;
        let Some(client) = client else {
            return BrowsePageSettlement {
                source_id: request.source_id,
                token: request.token,
                result: Err("media-server-session-unavailable".to_owned()),
            };
        };
        let source_id = request.source_id.clone();
        let token = request.token;
        tokio::select! {
            biased;
            () = cancel.cancelled() => BrowsePageSettlement {
                source_id,
                token,
                result: Err("browse request was cancelled".to_owned()),
            },
            settlement = fetch_browse_page(client, request) => settlement,
        }
    }
}

/// One retained query, its core result sets, and its physical request lifetime.
#[derive(Default)]
struct Browser {
    model: BrowseModel,
    client: Option<Arc<JellyfinClient>>,
    requests: HashMap<BrowseDeliveryToken, CancellationToken>,
}

impl fmt::Debug for Browser {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Browser")
            .field("model", &self.model)
            .field("pending_requests", &self.requests.len())
            .finish_non_exhaustive()
    }
}

impl Browser {
    #[must_use]
    pub fn model(&self) -> &BrowseModel {
        &self.model
    }

    pub fn configure(
        &mut self,
        client: Option<Arc<JellyfinClient>>,
        source: BrowseSource,
        preferences: BrowsePreferences,
    ) -> Result<BrowseWork, LibraryBrowseCoreError> {
        let effects = self.model.configure_with_preferences(source, preferences)?;
        self.client = client;
        let changed = !effects.is_empty();
        Ok(self.execute(effects, changed))
    }

    fn reset(&mut self) -> BrowseWork {
        let work = BrowseWork {
            cancellations: self.requests.drain().map(|(_, cancel)| cancel).collect(),
            ..BrowseWork::default()
        };
        self.model.reset();
        self.client = None;
        work
    }

    fn suspend(&mut self) -> BrowseWork {
        let effects = self.model.suspend();
        self.execute(effects, true)
    }

    pub fn resume(&mut self) -> Result<BrowseWork, LibraryBrowseCoreError> {
        let effects = self.model.resume()?;
        Ok(self.execute(effects, true))
    }

    pub fn refresh(&mut self) -> Result<BrowseWork, LibraryBrowseCoreError> {
        let before = self.model.is_refreshing();
        let effects = self.model.refresh()?;
        let changed = before != self.model.is_refreshing() || !effects.is_empty();
        Ok(self.execute(effects, changed))
    }

    pub fn retry(&mut self) -> Result<BrowseWork, LibraryBrowseCoreError> {
        let effects = self.model.retry()?;
        let changed = !effects.is_empty();
        Ok(self.execute(effects, changed))
    }

    pub fn apply_user_data_update(
        &mut self,
        update: &VideoUserDataUpdate,
    ) -> Result<BrowseWork, LibraryBrowseCoreError> {
        let effects = self.model.apply_user_data_update(update)?;
        Ok(self.execute(effects, true))
    }

    pub fn set_display_range(
        &mut self,
        range: Range<u32>,
    ) -> Result<BrowseWork, LibraryBrowseCoreError> {
        let Some(total) = self.model.total_record_count() else {
            return Ok(BrowseWork::default());
        };
        let before = self.model.peek_display_range();
        let effects = self.model.set_display_range(range, total)?;
        let changed = before != self.model.peek_display_range() || !effects.is_empty();
        Ok(self.execute(effects, changed))
    }

    pub fn settle(
        &mut self,
        settlement: BrowsePageSettlement,
    ) -> Result<BrowseWork, LibraryBrowseCoreError> {
        if !self.model.is_current_settlement(&settlement) {
            return Ok(BrowseWork::default());
        }
        self.requests.remove(&settlement.token);
        let effects = self.model.settle(settlement)?;
        Ok(self.execute(effects, true))
    }

    fn execute(&mut self, effects: Vec<BrowseEffect>, changed: bool) -> BrowseWork {
        let mut work = BrowseWork {
            changed,
            ..BrowseWork::default()
        };
        for effect in effects {
            match effect {
                BrowseEffect::ResetViewport => work.reset_viewport = true,
                BrowseEffect::CancelPage { token } => {
                    if let Some(cancel) = self.requests.remove(&token) {
                        work.cancellations.push(cancel);
                    }
                }
                BrowseEffect::RequestPage(request) => {
                    let cancel = CancellationToken::new();
                    self.requests.insert(request.token, cancel.clone());
                    work.requests.push(PageRequest {
                        request: *request,
                        client: self.client.clone(),
                        cancel,
                    });
                }
            }
        }
        work
    }

    fn cancel_requests(&mut self) {
        for (_, cancel) in self.requests.drain() {
            cancel.cancel();
        }
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        self.cancel_requests();
    }
}

/// Initial ordinary query; subsequent condition changes belong to its session.
#[derive(Clone, Debug)]
pub enum BrowseQuery {
    Library {
        library: VideoLibraryShortcut,
        preferences: BrowsePreferences,
    },
    Search {
        query: String,
    },
}

/// Bounded presentation window; retained page ownership remains in Browser.
#[derive(Clone, Debug)]
pub struct BrowseSnapshot {
    pub revision: u64,
    pub scope: ProfileScopeRef,
    pub identity: String,
    pub source: BrowseSource,
    pub preferences: BrowsePreferences,
    pub saved: Option<AppliedSavedBrowse>,
    pub viewport_revision: u64,
    pub view: LibraryBrowseView,
    pub refreshing: bool,
    pub refresh_failure: Option<LibraryBrowseFailure>,
}

struct SessionState {
    browser: Browser,
    source: BrowseSource,
    policy: BrowseQueryPolicy,
    viewport_revision: u64,
    revision: u64,
    failure: Option<SdkError>,
}

#[derive(Clone, Copy)]
enum DefaultsWrite {
    Never,
    ToolbarChanged,
    ExplicitReset,
}

/// Application-owned execution of one query under one Profile Scope.
/// Closing or dropping it cancels page work; suspending retains usable data.
pub struct BrowseSession {
    id: u64,
    state: Mutex<SessionState>,
    token: Arc<OperationToken>,
    handle: Handle,
    changed: watch::Sender<()>,
    closed: AtomicBool,
}

impl BrowseSession {
    pub(super) fn token_scope(&self) -> &ProfileScopeRef {
        &self.token.scope
    }

    pub(super) fn reconcile_saved(
        &self,
        records: &[crate::saved_browse::SavedBrowseFilter],
    ) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        if !state.policy.reconcile_saved(records) {
            return false;
        }
        state.revision = state.revision.wrapping_add(1);
        true
    }

    pub(super) fn notify_saved_changed(&self) {
        self.changed.send_replace(());
    }

    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Reads the notification fence even when the current snapshot reports a
    /// recoverable failure, without materializing the visible item window.
    pub fn revision(&self) -> Result<u64, SdkError> {
        self.check_active()?;
        let revision = self.state.lock().map_err(|_| SdkError::Closed)?.revision;
        self.check_active()?;
        Ok(revision)
    }

    /// Reads the viewport fence without materializing the visible item window.
    pub fn viewport_revision(&self) -> Result<u64, SdkError> {
        self.check_active()?;
        let revision = self
            .state
            .lock()
            .map_err(|_| SdkError::Closed)?
            .viewport_revision;
        self.check_active()?;
        Ok(revision)
    }

    pub fn set_preferences(
        self: &Arc<Self>,
        preferences: BrowsePreferences,
    ) -> Result<(), SdkError> {
        self.change_policy(DefaultsWrite::ToolbarChanged, |policy| {
            policy.set_preferences(preferences)
        })
    }

    pub fn remove_condition(self: &Arc<Self>, condition: BrowseCondition) -> Result<(), SdkError> {
        self.change_policy(DefaultsWrite::ExplicitReset, |policy| {
            policy.remove_condition(condition)
        })
    }

    pub fn clear_conditions(self: &Arc<Self>) -> Result<(), SdkError> {
        self.change_policy(
            DefaultsWrite::ExplicitReset,
            BrowseQueryPolicy::clear_conditions,
        )
    }

    pub fn set_presentation(
        self: &Arc<Self>,
        presentation: BrowsePresentation,
    ) -> Result<(), SdkError> {
        self.change_policy(DefaultsWrite::Never, |policy| {
            policy.set_presentation(presentation)
        })
    }

    fn change_policy(
        self: &Arc<Self>,
        persistence: DefaultsWrite,
        operation: impl FnOnce(&mut BrowseQueryPolicy),
    ) -> Result<(), SdkError> {
        let inner = self.token.inner.upgrade().ok_or(SdkError::Closed)?;
        let (work, changed) = {
            // Account activation and the physical settings write share this
            // lock. Never let a queued old-profile edit reach a new scope.
            let sdk_state = inner.state.lock().map_err(|_| SdkError::Closed)?;
            inner.check_browse_scope(&sdk_state, &self.token)?;
            let mut state = self.state.lock().map_err(|_| SdkError::Closed)?;
            if self.closed.load(Ordering::Acquire) {
                return Err(SdkError::Closed);
            }
            inner.check_browse_scope(&sdk_state, &self.token)?;
            let mut policy = state.policy.clone();
            operation(&mut policy);
            let filters = ordinary_filters(policy.preferences());
            let invalidate_catalog = matches!(persistence, DefaultsWrite::ToolbarChanged)
                && policy.preferences().filters != state.policy.preferences().filters;
            let persist = match persistence {
                DefaultsWrite::Never => false,
                DefaultsWrite::ToolbarChanged => {
                    filters != ordinary_filters(state.policy.preferences())
                }
                DefaultsWrite::ExplicitReset => true,
            };
            if persist
                && policy.persists_defaults()
                && matches!(state.source, BrowseSource::Library { .. })
            {
                let _preferences = inner
                    .preferences_gate
                    .lock()
                    .map_err(|_| SdkError::Closed)?;
                let mut store = SettingsStore::load_in_dir(inner.config.storage_dir.clone())
                    .map_err(|error| SdkError::Storage(error.to_string()))?;
                inner.check_browse_scope(&sdk_state, &self.token)?;
                store
                    .set_browse_filters(filters)
                    .map_err(|error| SdkError::Storage(error.to_string()))?;
            }
            if policy.preferences() == state.policy.preferences() {
                state.policy = policy;
                return Ok(());
            }
            if invalidate_catalog {
                if let Some(client) = &state.browser.client {
                    client.library().invalidate_filter_catalog();
                }
            }
            let source = state.source.clone();
            let client = state.browser.client.clone();
            let work = state
                .browser
                .configure(client, source, policy.preferences().clone())
                .map_err(core_error)?;
            state.policy = policy;
            let changed = Self::commit_work(&mut state, &work);
            (work, changed)
        };
        self.publish(work, changed);
        Ok(())
    }
    pub fn snapshot(&self) -> Result<BrowseSnapshot, SdkError> {
        self.check_active()?;
        let snapshot = {
            let state = self.state.lock().map_err(|_| SdkError::Closed)?;
            if let Some(error) = &state.failure {
                return Err(error.clone());
            }
            BrowseSnapshot {
                revision: state.revision,
                source: state.source.clone(),
                preferences: state.policy.preferences().clone(),
                saved: state.policy.applied_saved().cloned(),
                viewport_revision: state.viewport_revision,
                scope: self.token.scope.clone(),
                identity: state
                    .browser
                    .model()
                    .identity()
                    .unwrap_or_default()
                    .to_owned(),
                view: state.browser.model().view(),
                refreshing: state.browser.model().is_refreshing(),
                refresh_failure: state.browser.model().refresh_failure().cloned(),
            }
        };
        self.check_active()?;
        Ok(snapshot)
    }

    /// Waits for a revision different from the last consumed one. Slow
    /// consumers receive the latest snapshot, not an unbounded event queue.
    pub async fn next_snapshot(&self, after_revision: u64) -> Result<BrowseSnapshot, SdkError> {
        let mut changed = self.changed.subscribe();
        loop {
            self.check_active()?;
            // Notifications may arrive out of order after writers unlock.
            // Only committed state decides whether a new snapshot exists.
            let revision = self.state.lock().map_err(|_| SdkError::Closed)?.revision;
            if revision != after_revision {
                return self.snapshot();
            }
            tokio::select! {
                biased;
                () = self.token.cancel.cancelled() => {
                    self.check_active()?;
                    return Err(SdkError::Cancelled);
                },
                result = changed.changed() => result.map_err(|_| SdkError::Closed)?,
            }
        }
    }

    pub fn set_display_range(self: &Arc<Self>, start: u32, end: u32) -> Result<(), SdkError> {
        self.change(|browser| browser.set_display_range(start..end))
    }

    /// Ignores geometry captured before a query reset.
    pub fn set_display_range_for_revision(
        self: &Arc<Self>,
        viewport_revision: u64,
        start: u32,
        end: u32,
    ) -> Result<(), SdkError> {
        self.change_state(|state| {
            if state.viewport_revision != viewport_revision {
                return Ok(BrowseWork::default());
            }
            state.browser.set_display_range(start..end)
        })
    }

    pub fn refresh(self: &Arc<Self>) -> Result<(), SdkError> {
        self.change(|browser| {
            if !browser.model.is_refreshing() {
                if let Some(client) = &browser.client {
                    client.library().invalidate_filter_catalog();
                }
            }
            browser.refresh()
        })
    }

    pub fn retry(self: &Arc<Self>) -> Result<(), SdkError> {
        self.change(Browser::retry)
    }

    pub fn suspend(self: &Arc<Self>) -> Result<(), SdkError> {
        self.change(|browser| Ok(browser.suspend()))
    }

    pub fn resume(self: &Arc<Self>) -> Result<(), SdkError> {
        self.change_state(|state| {
            if state.policy.preferences().filters.is_empty() {
                state.browser.resume()
            } else {
                if let Some(client) = &state.browser.client {
                    client.library().invalidate_filter_catalog();
                }
                let effects = state.browser.model.resume_with_refresh()?;
                Ok(state.browser.execute(effects, true))
            }
        })
    }

    pub fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.token.cancel();
        let work = self
            .state
            .lock()
            .ok()
            .map(|mut state| state.browser.reset());
        if let Some(work) = work {
            for cancel in work.cancellations {
                cancel.cancel();
            }
        }
    }

    fn check_active(&self) -> Result<(), SdkError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(SdkError::Closed);
        }
        let inner = self.token.inner.upgrade().ok_or(SdkError::Closed)?;
        let state = inner.state.lock().map_err(|_| SdkError::Closed)?;
        if state.closed {
            return Err(SdkError::Closed);
        }
        if state.epoch != self.token.epoch {
            return Err(SdkError::Stale);
        }
        if self.token.is_cancelled() {
            return Err(SdkError::Cancelled);
        }
        Ok(())
    }

    fn change(
        self: &Arc<Self>,
        operation: impl FnOnce(&mut Browser) -> Result<BrowseWork, LibraryBrowseCoreError>,
    ) -> Result<(), SdkError> {
        self.change_state(|state| operation(&mut state.browser))
    }

    fn change_state(
        self: &Arc<Self>,
        operation: impl FnOnce(&mut SessionState) -> Result<BrowseWork, LibraryBrowseCoreError>,
    ) -> Result<(), SdkError> {
        self.check_active()?;
        let (work, changed) = {
            let mut state = self.state.lock().map_err(|_| SdkError::Closed)?;
            if self.closed.load(Ordering::Acquire) || self.token.is_cancelled() {
                return Err(SdkError::Cancelled);
            }
            let work = operation(&mut state).map_err(core_error)?;
            let changed = Self::commit_work(&mut state, &work);
            (work, changed)
        };
        self.publish(work, changed);
        Ok(())
    }

    fn commit_work(state: &mut SessionState, work: &BrowseWork) -> bool {
        let cleared_failure = state.failure.take().is_some();
        let changed = work.changed || cleared_failure;
        if work.reset_viewport {
            state.viewport_revision = state.viewport_revision.wrapping_add(1);
        }
        if changed {
            state.revision = state.revision.wrapping_add(1);
        }
        changed
    }

    fn publish(self: &Arc<Self>, work: BrowseWork, changed: bool) {
        if changed {
            self.changed.send_replace(());
        }
        self.dispatch(work);
    }

    pub(super) fn dispatch(self: &Arc<Self>, work: BrowseWork) {
        for cancel in work.cancellations {
            cancel.cancel();
        }
        for request in work.requests {
            let session = Arc::downgrade(self);
            let cancel = Arc::clone(&self.token.cancel);
            // Browser and scope tokens own cancellation; the task must not
            // keep its session alive merely by awaiting a server response.
            drop(self.handle.spawn(async move {
                let settlement = tokio::select! {
                    biased;
                    () = cancel.cancelled() => return,
                    settlement = request.run() => settlement,
                };
                let Some(session) = session.upgrade() else {
                    return;
                };
                if let Err(error) = session.change(|browser| browser.settle(settlement)) {
                    session.fail(error);
                }
            }));
        }
    }

    fn fail(&self, error: SdkError) {
        if matches!(
            error,
            SdkError::Closed | SdkError::Cancelled | SdkError::Stale
        ) {
            return;
        }
        if let Ok(mut state) = self.state.lock() {
            state.failure = Some(error);
            state.revision = state.revision.wrapping_add(1);
            drop(state);
            self.changed.send_replace(());
        }
    }
}

impl Drop for BrowseSession {
    fn drop(&mut self) {
        self.token.cancel();
    }
}

fn core_error(error: LibraryBrowseCoreError) -> SdkError {
    SdkError::Request(error.to_string())
}

fn ordinary_filters(preferences: &BrowsePreferences) -> BrowseFilterSettings {
    BrowseFilterSettings::default()
        .with_sort(preferences.sort)
        .with_sort_direction(preferences.sort_direction)
        .with_played_filter(preferences.played_filter)
        .with_favorites_only(preferences.favorites_only)
}

impl Sdk {
    /// Opens an explicitly configured ordinary query. Android preferences are
    /// ephemeral; Desktop/TV mutations persist the ordinary defaults.
    pub fn open_browser(
        &self,
        query: BrowseQuery,
        presentation: BrowsePresentation,
    ) -> Result<Arc<BrowseSession>, SdkError> {
        let token = self.new_operation_token()?;
        let mut state = self.inner.state.lock().map_err(|_| SdkError::Closed)?;
        self.inner.check_browse_scope(&state, &token)?;
        let session_token = RequestGate::default().begin_login();
        let (source, preferences) = match query {
            BrowseQuery::Library {
                library,
                preferences,
            } => (
                BrowseSource::Library {
                    session: session_token,
                    shortcut: library,
                },
                preferences,
            ),
            BrowseQuery::Search { query } => (
                BrowseSource::Search {
                    session: session_token,
                    query,
                },
                BrowsePreferences::default(),
            ),
        };
        let policy = BrowseQueryPolicy::ordinary(preferences, presentation);
        let (session, work) = self
            .inner
            .create_browser(&mut state, &token, source, policy)?;
        drop(state);
        session.dispatch(work);
        Ok(session)
    }

    /// Reads persisted ordinary defaults without exposing settings assembly to clients.
    pub fn open_library_browser(
        &self,
        library: VideoLibraryShortcut,
        presentation: BrowsePresentation,
    ) -> Result<Arc<BrowseSession>, SdkError> {
        let token = self.new_operation_token()?;
        let mut state = self.inner.state.lock().map_err(|_| SdkError::Closed)?;
        self.inner.check_browse_scope(&state, &token)?;
        let preferences = {
            let _guard = self
                .inner
                .preferences_gate
                .lock()
                .map_err(|_| SdkError::Closed)?;
            let store = SettingsStore::load_in_dir(self.inner.config.storage_dir.clone())
                .map_err(|error| SdkError::Storage(error.to_string()))?;
            BrowsePreferences::from(store.snapshot().browse_filters())
        };
        let policy = BrowseQueryPolicy::ordinary(preferences, presentation);
        let source = BrowseSource::Library {
            session: RequestGate::default().begin_login(),
            shortcut: library,
        };
        let (session, work) = self
            .inner
            .create_browser(&mut state, &token, source, policy)?;
        drop(state);
        session.dispatch(work);
        Ok(session)
    }
}

impl SdkInner {
    pub(super) fn create_browser(
        &self,
        state: &mut SdkState,
        token: &Arc<OperationToken>,
        source: BrowseSource,
        policy: BrowseQueryPolicy,
    ) -> Result<(Arc<BrowseSession>, BrowseWork), SdkError> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let client = state
            .active
            .as_ref()
            .ok_or(SdkError::NoActiveProfile)?
            .client
            .clone();
        let mut browser = Browser::default();
        let work = browser
            .configure(Some(client), source.clone(), policy.preferences().clone())
            .map_err(core_error)?;
        let (changed, _) = watch::channel(());
        let session = Arc::new(BrowseSession {
            id,
            state: Mutex::new(SessionState {
                source,
                policy,
                viewport_revision: u64::from(work.reset_viewport),
                browser,
                revision: 1,
                failure: None,
            }),
            token: Arc::clone(token),
            handle: self.handle.clone(),
            changed,
            closed: AtomicBool::new(false),
        });
        state.browsers.retain(|browser| browser.strong_count() > 0);
        state.browsers.push(Arc::downgrade(&session));
        Ok((session, work))
    }
}

impl SdkInner {
    pub(crate) fn check_browse_scope(
        &self,
        state: &SdkState,
        token: &OperationToken,
    ) -> Result<(), SdkError> {
        if state.closed {
            return Err(SdkError::Closed);
        }
        if state.epoch != token.epoch || !std::ptr::eq(self, token.inner.as_ptr()) {
            return Err(SdkError::Stale);
        }
        if token.is_cancelled() {
            return Err(SdkError::Cancelled);
        }
        if state.handoff_in_progress || state.sign_out_cleanup_pending {
            return Err(SdkError::OperationInProgress);
        }
        let active = state.active.as_ref().ok_or(SdkError::NoActiveProfile)?;
        if active.key.as_str() != token.scope.profile_key {
            return Err(SdkError::Stale);
        }
        Ok(())
    }

    pub(crate) fn publish_user_data(
        &self,
        scope: &ProfileScopeRef,
        update: &VideoUserDataUpdate,
    ) -> Result<(), SdkError> {
        let browsers = {
            let mut state = self.state.lock().map_err(|_| SdkError::Closed)?;
            if state.closed {
                return Err(SdkError::Closed);
            }
            if state.epoch != scope.generation {
                return Err(SdkError::Stale);
            }
            state.browsers.retain(|browser| browser.strong_count() > 0);
            state
                .browsers
                .iter()
                .filter_map(std::sync::Weak::upgrade)
                .collect::<Vec<_>>()
        };
        for browser in browsers {
            // Dropping the last handle also drops its OperationToken, which
            // locks SDK state. Never reject/drop upgraded handles under it.
            if browser.token.scope != *scope {
                continue;
            }
            if let Err(error) = browser.change(|browser| browser.apply_user_data_update(update)) {
                browser.fail(error);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/browse_notifications.rs"]
mod notification_tests;
