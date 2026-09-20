//! Library Browser execution shared by native adapters and SDK sessions.
//!
//! `Browser` keeps the existing core result-set policy and owns physical page
//! cancellation. Adapters supply viewport demand and drive returned requests;
//! they never assemble pages or track request handles. `BrowseSession` drives
//! that same owner on the SDK runtime and publishes revisioned snapshots.

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use jellypilot_core::browse::fetch_browse_page;
use jellypilot_core::browse_model::{
    BrowseDeliveryToken, BrowseEffect, BrowseModel, BrowsePageRequest, BrowsePageSettlement,
    BrowsePreferences, BrowseSource, LibraryBrowseView,
};
use jellypilot_core::request_gate::RequestGate;
use jellypilot_core::{LibraryBrowseCoreError, LibraryBrowseFailure};
use jellypilot_media_server::{JellyfinClient, VideoLibraryShortcut, VideoUserDataUpdate};
use tokio::runtime::Handle;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::{OperationToken, ProfileScopeRef, Sdk, SdkError, SdkInner};

/// Work to adapt to a frontend executor. Apply the viewport reset before
/// polling requests, otherwise old geometry may advance the new query.
#[derive(Default)]
pub struct BrowseWork {
    pub reset_viewport: bool,
    pub requests: Vec<PageRequest>,
    pub changed: bool,
}

impl BrowseWork {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.changed
    }
}

/// One physical request, cancelled by its owning Browser when obsolete.
pub struct PageRequest {
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
pub struct Browser {
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

    pub fn reset(&mut self) {
        self.cancel_requests();
        self.model.reset();
        self.client = None;
    }

    pub fn suspend(&mut self) {
        let effects = self.model.suspend();
        drop(self.execute(effects, true));
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
                        cancel.cancel();
                    }
                }
                BrowseEffect::RequestPage(request) => {
                    let cancel = CancellationToken::new();
                    self.requests.insert(request.token, cancel.clone());
                    work.requests.push(PageRequest {
                        request,
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

/// Immutable query for an SDK-owned browser session.
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
    pub view: LibraryBrowseView,
    pub refreshing: bool,
    pub refresh_failure: Option<LibraryBrowseFailure>,
}

struct SessionState {
    browser: Browser,
    revision: u64,
    failure: Option<SdkError>,
}

/// Application-owned execution of one query under one Profile Scope.
/// Closing or dropping it cancels page work; suspending retains usable data.
pub struct BrowseSession {
    state: Mutex<SessionState>,
    token: Arc<OperationToken>,
    handle: Handle,
    changed: watch::Sender<()>,
    closed: AtomicBool,
}

impl BrowseSession {
    pub fn snapshot(&self) -> Result<BrowseSnapshot, SdkError> {
        self.check_active()?;
        let snapshot = {
            let state = self.state.lock().map_err(|_| SdkError::Closed)?;
            if let Some(error) = &state.failure {
                return Err(error.clone());
            }
            BrowseSnapshot {
                revision: state.revision,
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

    pub fn refresh(self: &Arc<Self>) -> Result<(), SdkError> {
        self.change(Browser::refresh)
    }

    pub fn retry(self: &Arc<Self>) -> Result<(), SdkError> {
        self.change(Browser::retry)
    }

    pub fn suspend(self: &Arc<Self>) -> Result<(), SdkError> {
        self.change(|browser| {
            browser.suspend();
            Ok(BrowseWork {
                changed: true,
                ..BrowseWork::default()
            })
        })
    }

    pub fn resume(self: &Arc<Self>) -> Result<(), SdkError> {
        self.change(Browser::resume)
    }

    pub fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.token.cancel();
        if let Ok(mut state) = self.state.lock() {
            state.browser.reset();
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
        self.check_active()?;
        let (work, changed) = {
            let mut state = self.state.lock().map_err(|_| SdkError::Closed)?;
            if self.closed.load(Ordering::Acquire) || self.token.is_cancelled() {
                return Err(SdkError::Cancelled);
            }
            let work = operation(&mut state.browser).map_err(core_error)?;
            let cleared_failure = state.failure.take().is_some();
            let changed = work.changed || cleared_failure;
            if changed {
                state.revision = state.revision.wrapping_add(1);
            }
            (work, changed)
        };
        if changed {
            // UniFFI can resume and poll a snapshot waiter inline here.
            // Never wake foreign continuations while holding session state.
            self.changed.send_replace(());
        }
        self.dispatch(work);
        Ok(())
    }

    fn dispatch(self: &Arc<Self>, work: BrowseWork) {
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

impl Sdk {
    /// Opens a query with its own scope-bound cancellation lifetime. The
    /// session drives paging; callers provide ranges and consume snapshots.
    pub fn open_browser(&self, query: BrowseQuery) -> Result<Arc<BrowseSession>, SdkError> {
        let token = self.new_operation_token()?;
        let mut state = self.inner.state.lock().map_err(|_| SdkError::Closed)?;
        if state.closed {
            return Err(SdkError::Closed);
        }
        if state.epoch != token.epoch {
            return Err(SdkError::Stale);
        }
        let client = state
            .active
            .as_ref()
            .ok_or(SdkError::NoActiveProfile)?
            .client
            .clone();
        // This result-set owner has one immutable binding. Core delivery IDs
        // are process-unique; the SDK token fences the real profile lifetime.
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
        let mut browser = Browser::default();
        let work = browser
            .configure(Some(client), source, preferences)
            .map_err(core_error)?;
        let (changed, _) = watch::channel(());
        let session = Arc::new(BrowseSession {
            state: Mutex::new(SessionState {
                browser,
                revision: 1,
                failure: None,
            }),
            token,
            handle: self.inner.handle.clone(),
            changed,
            closed: AtomicBool::new(false),
        });
        state.browsers.retain(|browser| browser.strong_count() > 0);
        state.browsers.push(Arc::downgrade(&session));
        drop(state);
        session.dispatch(work);
        Ok(session)
    }
}

impl SdkInner {
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
