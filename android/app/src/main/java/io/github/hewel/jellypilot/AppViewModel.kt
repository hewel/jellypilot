package io.github.hewel.jellypilot

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.hewel.jellypilot.ffi.*
import io.github.hewel.jellypilot.ui.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.job

internal class AppViewModel(application: Application) : AndroidViewModel(application) {
  private val app = application as JellyPilotApplication
  private val sdk = app.sdk
  val player = app.player
  private val visibility = PlaybackVisibility(application, player::setEligible)
  private val mutableState = MutableStateFlow(AppUiState())
  val state = mutableState.asStateFlow()
  private var queryJob: Job? = null
  private var loginJob: Job? = null
  private var queryToken: OperationToken? = null
  private var quickSession: QuickConnectSession? = null
  private var authGeneration = 0L
  private var queryGeneration = 0L
  private var searchText = ""
  private var searchDebounce: Job? = null
  /** The single SDK-owned browse session for the current Library/Search query. */
  private var browseSession: BrowseSession? = null
  private var browseCollector: Job? = null
  /** Set only while the retained session is suspended for detail/player navigation. */
  private var browserSuspended = false
  /** Incremented on every suspend intent; lets a failed detail load resume only its own suspension. */
  private var browserSuspendEpoch = 0L
  /** Monotonic session generation; published into BrowserUi so the grid keys its viewport per session, not per scope-relative identity. */
  private var browserGeneration = 0L
  private var libraries = emptyList<VideoLibraryShortcut>()
  /** In-flight Favorite/Played writes keyed by item id; each owns its token and job, independent of the browse query slot. */
  private val userDataWrites = mutableMapOf<String, PendingWrite>()
  /** Latest server-confirmed flags per item, used to keep older in-flight reads from republishing pre-write state. */
  private val confirmedUserData = mutableMapOf<String, ConfirmedUserData>()
  private var confirmedRevision = 0L

  private class PendingWrite(val token: OperationToken, val job: Job)
  private class ConfirmedUserData(val revision: Long, val played: Boolean, val favorite: Boolean)

  init {
    viewModelScope.launch {
      refreshIdentity()
      if (sdk.activeProfile() != null) refresh()
    }
  }
  fun setVisible(visible: Boolean) {
    visibility.setVisible(visible)
    // This ViewModel is Application-owned: activityFinished() closed the
    // session, so reopening must restore the intended browse query.
    if (visible && browseSession == null && sdk.activeProfile() != null) {
      when (state.value.destination) {
        Destination.Search -> search(searchText)
        Destination.Library -> openLibraryQuery()
        else -> Unit
      }
    }
  }
  fun activityFinished() {
    cancelLogin()
    cancelQuery()
    closeBrowserSession()
    player.stop()
    mutableState.update { it.copy(showPlayer = false, busy = false) }
  }
  fun dismissError() { mutableState.update { it.copy(error = null) } }
  fun addAccount() {
    if (rejectWhileCleanupPending()) return
    mutableState.update { it.copy(showSignIn = true, error = null) }
  }
  fun openPlayer() {
    if (sdk.contentMutationsBlocked()) {
      if (sdk.signOutCleanupPending()) showCleanupPending()
      return
    }
    suspendBrowser()
    cancelQuery()
    mutableState.update { it.copy(showPlayer = true, busy = false) }
  }
  fun back() {
    if (state.value.showPlayer) {
      player.stop()
      mutableState.update { it.copy(showPlayer = false) }
      resumeBrowser()
    } else {
      cancelQuery()
      mutableState.update { it.copy(detail = null, detailItems = emptyList(), busy = false) }
      resumeBrowser()
    }
  }

  fun navigate(destination: Destination) {
    if (state.value.destination == destination && state.value.detail == null) return
    cancelQuery()
    closeBrowserSession()
    mutableState.update { it.copy(destination = destination, detail = null, detailItems = emptyList(), items = emptyList(), browser = BrowserUi(), busy = false) }
    when (destination) {
      Destination.Account -> viewModelScope.launch { refreshIdentity() }
      Destination.Search -> search(searchText)
      Destination.Library -> openLibraryQuery()
      Destination.Home -> refresh()
    }
  }

  fun cancelLogin() {
    ++authGeneration
    quickSession?.cancel()
    quickSession?.destroy()
    quickSession = null
    loginJob?.cancel()
    mutableState.update { it.copy(loginBusy = loginJob != null, quickConnectCode = null, showSignIn = false) }
  }

  /** Explains why playback and writes stay blocked until sign-out cleanup is retried. */
  private fun showCleanupPending() {
    mutableState.update { it.copy(error = app.getString(R.string.sdk_sign_out_cleanup_pending)) }
  }

  /** New account transitions cannot bypass a pending sign-out cleanup; the SDK also rejects them. */
  private fun rejectWhileCleanupPending(): Boolean {
    if (!sdk.signOutCleanupPending()) return false
    showCleanupPending()
    return true
  }

  fun signIn(jellyfin: Boolean, server: String, username: String, password: String, remember: Boolean) {
    if (rejectWhileCleanupPending()) return
    accountOperation {
      val candidate = sdk.passwordLogin(if (jellyfin) Provider.JELLYFIN else Provider.EMBY, server, username, password)
      activateCandidate(candidate, remember)
    }
  }

  fun activate(key: String) {
    if (rejectWhileCleanupPending()) return
    accountOperation { activateCandidate(sdk.restoreSavedProfile(key), true) }
  }
  fun disconnect() {
    accountOperation {
      withContext(NonCancellable) {
        player.setHandoffBlocked(true)
        sdk.disconnect()
        connectionChanged()
      }
    }
  }
  /** Explicit retry of a failed sign-out teardown; `disconnect` performs the SDK cleanup retry. */
  fun retryCleanup() = disconnect()
  fun signOut(key: String) {
    accountOperation {
      withContext(NonCancellable) {
        val activeBefore = sdk.activeProfile()?.key
        // Media-session commands bypass this ViewModel. Block the native
        // player before deletion starts, not only when the teardown hook runs.
        if (activeBefore == key) player.setHandoffBlocked(true)
        val outcome = sdk.signOut(key, false)
        // A failed teardown keeps the session connected for a cleanup retry:
        // only the saved-profile list changes, never the connection reset.
        if (activeBefore == key && outcome.teardownError == null) connectionChanged(outcome.remaining) else refreshIdentity(outcome.remaining)
        val warnings = listOfNotNull(
          outcome.teardownError?.let { app.getString(R.string.sdk_sign_out_cleanup_pending) },
          outcome.watchlistError?.let { app.getString(R.string.sdk_watchlist_delete_failed) },
        )
        if (warnings.isNotEmpty()) mutableState.update { it.copy(error = warnings.joinToString("\n")) }
      }
    }
  }

  private suspend fun activateCandidate(candidate: ProfileCandidate, remember: Boolean) = withContext(NonCancellable) {
    var activated = false
    try {
      player.setHandoffBlocked(true)
      val outcome = sdk.activateCandidate(candidate, remember)
      activated = true
      connectionChanged()
      mutableState.update { it.copy(showSignIn = false, quickConnectCode = null) }
      if (outcome.persistenceWarning != null) {
        mutableState.update { it.copy(error = app.getString(R.string.sdk_activated_persistence_failed)) }
      }
    } finally {
      if (!activated) candidate.discard()
      candidate.destroy()
    }
  }

  private fun accountOperation(action: suspend () -> Unit) {
    if (state.value.loginBusy) return
    ++authGeneration
    mutableState.update { it.copy(loginBusy = true, error = null) }
    loginJob = viewModelScope.launch(start = CoroutineStart.LAZY) {
      try { action() }
      catch (cancelled: CancellationException) { throw cancelled }
      catch (error: Exception) { showError(error) }
      finally {
        // A failed sign-out teardown leaves the SDK mutation block in place
        // for the cleanup retry; only a settled operation may unblock.
        player.setHandoffBlocked(sdk.contentMutationsBlocked())
        loginJob = null
        val cleanupPending = sdk.signOutCleanupPending()
        mutableState.update { it.copy(loginBusy = false, signOutCleanupPending = cleanupPending) }
      }
    }
    loginJob?.start()
  }

  fun quickConnect(server: String, remember: Boolean) {
    if (state.value.loginBusy || rejectWhileCleanupPending()) return
    val generation = ++authGeneration
    mutableState.update { it.copy(loginBusy = true, error = null) }
    try {
      quickSession = sdk.startQuickConnect(server, object : QuickConnectListener {
        override fun onCode(code: String) {
          viewModelScope.launch { if (authGeneration == generation) mutableState.update { it.copy(quickConnectCode = code) } }
        }
        override fun onApproving() {
          viewModelScope.launch { if (authGeneration == generation) mutableState.update { it.copy(quickConnectCode = null) } }
        }
        override fun onCompleted(outcome: QuickConnectOutcome) {
          viewModelScope.launch {
            if (authGeneration != generation) {
              if (outcome is QuickConnectOutcome.Success) { outcome.candidate.discard(); outcome.candidate.destroy() }
              return@launch
            }
            quickSession?.destroy()
            quickSession = null
            mutableState.update { it.copy(loginBusy = false, quickConnectCode = null) }
            when (outcome) {
              is QuickConnectOutcome.Success -> accountOperation { activateCandidate(outcome.candidate, remember) }
              is QuickConnectOutcome.Failed -> showError(outcome.error)
              QuickConnectOutcome.Cancelled -> Unit
            }
          }
        }
      })
    } catch (error: Exception) {
      mutableState.update { it.copy(loginBusy = false) }
      showError(error)
    }
  }

  private suspend fun refreshIdentity(saved: List<SavedProfile>? = null) {
    val active = sdk.activeProfile()
    val cleanupPending = sdk.signOutCleanupPending()
    mutableState.update { it.copy(activeName = active?.userName, signOutCleanupPending = cleanupPending) }
    try {
      val profiles = (saved ?: sdk.savedProfiles().profiles).map { profile ->
        ProfileUi(profile.key, profile.title, profile.serverUrl, if (profile.provider == Provider.JELLYFIN) "Jellyfin" else "Emby", profile.key == active?.key)
      }
      mutableState.update { it.copy(profiles = profiles) }
    } catch (cancelled: CancellationException) { throw cancelled }
    catch (error: Exception) { showError(error) }
  }

  private suspend fun connectionChanged(saved: List<SavedProfile>? = null) {
    cancelQuery()
    closeBrowserSession()
    cancelUserDataWrites()
    confirmedUserData.clear()
    libraries = emptyList()
    searchText = ""
    mutableState.update { it.copy(activeName = sdk.activeProfile()?.userName, items = emptyList(), browser = BrowserUi(), detail = null, detailItems = emptyList(), libraries = emptyList(), libraryId = null, busy = false, destination = Destination.Home, searchQuery = "") }
    refreshIdentity(saved)
    if (sdk.activeProfile() != null) refresh()
  }

  private fun cancelQuery() {
    ++queryGeneration
    queryToken?.cancel()
    queryJob?.cancel()
    queryToken = null
    queryJob = null
  }

  private fun query(action: suspend (OperationToken, ProfileScopeRef, Long) -> Unit) {
    cancelQuery()
    if (sdk.activeProfile() == null) return
    val generation = queryGeneration
    // Reads issued now may carry pre-write flags for items whose writes
    // confirm later; `base` lets publish sites keep the confirmed values.
    val base = confirmedRevision
    mutableState.update { it.copy(busy = true, error = null) }
    queryJob = viewModelScope.launch {
      var token: OperationToken? = null
      try {
        val operation = sdk.newOperationToken()
        token = operation
        queryToken = operation
        action(operation, operation.scopeRef(), base)
      } catch (cancelled: CancellationException) { throw cancelled }
      catch (error: Exception) { showError(error) }
      finally {
        token?.cancel()
        token?.destroy()
        if (queryGeneration == generation) {
          queryToken = null
          mutableState.update { it.copy(busy = false) }
        }
      }
    }
  }

  /**
   * Replaces the browse session with one immutable SDK query. The old session
   * is closed before the new one exists, so a stale query can never publish
   * into the new projection.
   */
  private fun openBrowserSession(query: BrowseQuery) {
    closeBrowserSession()
    if (sdk.activeProfile() == null) return
    val session = try {
      sdk.openBrowser(query)
    } catch (error: Exception) {
      showError(error)
      return
    }
    val generation = ++browserGeneration
    browseSession = session
    // A session opened while detail/player is showing starts suspended;
    // back() resumes it when the temporary navigation ends.
    if (state.value.detail != null || state.value.showPlayer) suspendBrowser()
    browseCollector = viewModelScope.launch {
      // `base` is captured before each await so a write confirming while the
      // snapshot is in flight still overlays its confirmed flags on publish.
      var base = confirmedRevision
      var snapshot = try {
        session.snapshot()
      } catch (error: Exception) {
        if (browseSession === session) showError(error)
        return@launch
      }
      while (true) {
        publish(session, snapshot, generation, base)
        base = confirmedRevision
        snapshot = try {
          session.nextSnapshot(snapshot.revision)
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) {
          if (browseSession === session) showError(error)
          return@launch
        }
      }
    }
  }

  /** Publishes one session snapshot if the session and its profile scope are still current. */
  private fun publish(session: BrowseSession, snapshot: BrowseSnapshot, generation: Long, base: Long) {
    if (browseSession !== session || browserGeneration != generation || !sdk.isScopeActive(snapshot.scope)) return
    val scope = snapshot.scope
    mutableState.update {
      it.copy(
        browser = BrowserUi(
          revision = snapshot.revision,
          generation = generation,
          status = when (snapshot.status) {
            BrowseStatus.INACTIVE -> BrowseUiStatus.Inactive
            BrowseStatus.LOADING -> BrowseUiStatus.Loading
            BrowseStatus.EMPTY -> BrowseUiStatus.Empty
            BrowseStatus.READY -> BrowseUiStatus.Ready
            BrowseStatus.FAILED -> BrowseUiStatus.Failed
          },
          slots = snapshot.items.map { item -> item?.let { presented(media(it, scope), base) } },
          visibleStart = snapshot.visibleStart,
          totalCount = snapshot.totalCount,
          isVirtual = snapshot.isVirtual,
          loadingMore = snapshot.loadingMore,
          error = snapshot.error,
          retryable = snapshot.retryable,
          retryBusy = snapshot.retryBusy,
          refreshing = snapshot.refreshing,
          refreshError = snapshot.refreshError,
        ),
      )
    }
  }

  private fun closeBrowserSession() {
    searchDebounce?.cancel()
    searchDebounce = null
    browseCollector?.cancel()
    browseCollector = null
    val session = browseSession
    browseSession = null
    browserSuspended = false
    if (session != null) {
      try {
        session.shutdown()
      } catch (error: Exception) {
        showError(error)
      }
      session.destroy()
    }
  }


  /** Pauses page work while detail/player navigation retains the result set.
   *  Returns the suspend epoch so a failed detail load can tell whether its
   *  suspension is still the current one. */
  private fun suspendBrowser(): Long {
    val epoch = ++browserSuspendEpoch
    val session = browseSession ?: return epoch
    if (!browserSuspended) {
      try {
        session.`suspend`()
        browserSuspended = true
      } catch (error: Exception) {
        showError(error)
      }
    }
    return epoch
  }

  private fun resumeBrowser() {
    val current = state.value
    if (current.showPlayer || current.detail != null) return
    val session = browseSession ?: return
    if (!browserSuspended) return
    try {
      session.resume()
      browserSuspended = false
    } catch (error: Exception) {
      showError(error)
    }
  }

  /** Resumes only if no newer suspend intent (detail, player, replacement) ran since `epoch`. */
  private fun resumeBrowserIfCurrent(epoch: Long) {
    if (browserSuspendEpoch == epoch) resumeBrowser()
  }

  /** Runs one synchronous session call, surfacing SDK failures uniformly. */
  private fun browserCall(action: (BrowseSession) -> Unit) {
    val session = browseSession ?: return
    try {
      action(session)
    } catch (error: Exception) {
      showError(error)
    }
  }

  /** Viewport demand from the grid; the SDK owns all page scheduling. The
   *  generation fence drops emissions a replaced grid can still send before
   *  Compose removes its effect. */
  fun setBrowserDisplayRange(generation: Long, start: UInt, end: UInt) {
    if (generation != browserGeneration) return
    browserCall { it.setDisplayRange(start, end) }
  }

  /** Retries retained failures without discarding usable results. */
  fun retryBrowser() = browserCall { it.retry() }

  fun refresh() {
    state.value.detail?.let { showDetail(it.id); return }
    when (state.value.destination) {
      Destination.Account -> viewModelScope.launch { refreshIdentity() }
      Destination.Search -> if (browseSession != null) browserCall { it.refresh() } else search(searchText)
      Destination.Library -> if (browseSession != null) browserCall { it.refresh() } else openLibraryQuery()
      Destination.Home -> query { token, scope, base ->
        loadLibraries(token)
        val home = sdk.videoHome(token)
        val latest = libraries.firstOrNull()?.let { sdk.libraryLatest(token, it.id) } ?: emptyList()
        currentCoroutineContext().ensureActive()
        mutableState.update { it.copy(items = (home.continueWatching + home.nextUp + latest).distinctBy { item -> item.id }.map { item -> presented(media(item, scope), base) }) }
      }
    }
  }

  private suspend fun loadLibraries(token: OperationToken) {
    val result = sdk.libraryShortcuts(token)
    currentCoroutineContext().ensureActive()
    libraries = result
    mutableState.update { previous -> previous.copy(libraries = result.map { LibraryUi(it.id, it.name) }, libraryId = previous.libraryId?.takeIf { id -> result.any { it.id == id } } ?: result.firstOrNull()?.id) }
  }

  fun selectLibrary(id: String) {
    if (state.value.libraryId == id) return
    mutableState.update { it.copy(libraryId = id, browser = BrowserUi()) }
    openLibraryQuery()
  }

  /** Loads the shortcut list if needed, then opens the selected library session. */
  private fun openLibraryQuery() = query { token, _, _ ->
    if (libraries.isEmpty()) loadLibraries(token)
    openLibrarySession()
  }

  private fun openLibrarySession() {
    val library = libraries.firstOrNull { it.id == state.value.libraryId } ?: return
    openBrowserSession(
      BrowseQuery.Library(
        library,
        BrowsePreferences(
          sort = VideoLibrarySort.RECENTLY_ADDED,
          sortDirection = VideoLibrarySortDirection.DESCENDING,
          playedFilter = VideoLibraryPlayedFilter.ALL,
          favoritesOnly = false,
        ),
      ),
    )
  }
  fun search(value: String) {
    if (state.value.destination != Destination.Search) return
    searchText = value
    // A new search intent fences obsolete query-slot work (e.g. an in-flight
    // detail load) so it cannot publish over the replacement session.
    cancelQuery()
    // The previous query's session is obsolete immediately; only opening the
    // replacement is debounced so a stale query can never publish again.
    closeBrowserSession()
    mutableState.update { it.copy(searchQuery = value, busy = false) }
    searchDebounce = viewModelScope.launch {
      delay(250)
      // Clear the handle first: openBrowserSession closes the current session,
      // which would otherwise cancel this still-running job.
      searchDebounce = null
      openBrowserSession(BrowseQuery.Search(searchText))
    }
  }

  fun showDetail(id: String) {
    val suspendEpoch = suspendBrowser()
    query { token, scope, base ->
      var detailShown = false
      try {
        val type = (state.value.browser.slots.asSequence().filterNotNull() + state.value.items + state.value.detailItems)
          .firstOrNull { it.id == id }?.itemType
          ?: state.value.detail?.takeIf { it.id == id }?.itemType
        val detail = if (type == "Series") {
          val show = sdk.showDetail(token, id)
          MediaUi(show.id, show.name, "Series", metadata(show.productionYear, "Series"), artwork(show.artworkImageId, scope), show.overview.orEmpty(), show.favorite, show.played)
        } else {
          val item = sdk.itemDetail(token, id)
          MediaUi(item.id, item.name, item.itemType, metadata(item.productionYear, item.itemType), artwork(item.artworkImageId, scope), item.overview.orEmpty(), item.favorite, item.played)
        }
        currentCoroutineContext().ensureActive()
        mutableState.update { it.copy(detail = presented(detail, base), detailItems = emptyList()) }
        detailShown = true
        val related = sdk.similarVideo(token, id)
        currentCoroutineContext().ensureActive()
        mutableState.update { it.copy(detailItems = related.map { item -> presented(media(item, scope), base) }) }
      } catch (error: Exception) {
        // The retained browser stays suspended only while a detail is actually
        // shown; a failed or fenced load must hand it back to the grid, but
        // never over a newer suspend intent (player, newer detail, session swap).
        if (!detailShown) resumeBrowserIfCurrent(suspendEpoch)
        throw error
      }
    }
  }

  fun setFavorite(id: String, favorite: Boolean) { updateUserData(id, if (favorite) VideoUserDataAction.FAVORITE else VideoUserDataAction.UNFAVORITE) }
  fun setPlayed(id: String, played: Boolean) { updateUserData(id, if (played) VideoUserDataAction.MARK_PLAYED else VideoUserDataAction.MARK_UNPLAYED) }

  /** Runs one Favorite/Played write per item on its own token, independent of the browse query slot. */
  private fun updateUserData(id: String, action: VideoUserDataAction) {
    if (userDataWrites.containsKey(id)) return
    if (sdk.contentMutationsBlocked()) {
      if (sdk.signOutCleanupPending()) showCleanupPending()
      return
    }
    if (sdk.activeProfile() == null) return
    val token = try {
      sdk.newOperationToken()
    } catch (error: Exception) {
      showError(error)
      return
    }
    val job = viewModelScope.launch(start = CoroutineStart.LAZY) {
      try {
        val scope = token.scopeRef()
        val result = sdk.updateUserData(token, id, action)
        currentCoroutineContext().ensureActive()
        // The SDK already rejects stale results; this guards a confirmation
        // that was queued on this scope before a profile switch ran.
        if (!sdk.isScopeActive(scope)) return@launch
        confirmedUserData[id] = ConfirmedUserData(++confirmedRevision, result.played, result.favorite)
        fun reconciled(item: MediaUi) = if (item.id == result.itemId) item.copy(played = result.played, favorite = result.favorite, updating = false) else item
        mutableState.update {
          it.copy(
            items = it.items.map(::reconciled),
            browser = it.browser.copy(slots = it.browser.slots.map { slot -> slot?.let(::reconciled) }),
            detail = it.detail?.let(::reconciled),
            detailItems = it.detailItems.map(::reconciled),
          )
        }
      } catch (cancelled: CancellationException) { throw cancelled }
      catch (error: Exception) { showError(error) }
      finally {
        token.cancel()
        token.destroy()
        // Only the job that still owns the pending slot may release it, so a
        // cancelled write cannot clear a newer write admitted for this item.
        if (userDataWrites[id]?.job === currentCoroutineContext().job) {
          userDataWrites.remove(id)
          setWritePending(id, false)
        }
      }
    }
    userDataWrites[id] = PendingWrite(token, job)
    setWritePending(id, true)
    job.start()
  }

  private fun cancelUserDataWrites() {
    val writes = userDataWrites.values.toList()
    userDataWrites.clear()
    for (write in writes) {
      write.token.cancel()
      write.job.cancel()
    }
  }

  private fun setWritePending(id: String, updating: Boolean) {
    fun marked(item: MediaUi) = if (item.id == id) item.copy(updating = updating) else item
    mutableState.update {
      it.copy(
        items = it.items.map(::marked),
        browser = it.browser.copy(slots = it.browser.slots.map { slot -> slot?.let(::marked) }),
        detail = it.detail?.let(::marked),
        detailItems = it.detailItems.map(::marked),
      )
    }
  }

  /** Applies confirmed write results newer than `base` and the live pending flag to a freshly read item. */
  private fun presented(item: MediaUi, base: Long): MediaUi {
    val confirmed = confirmedUserData[item.id]?.takeIf { it.revision > base }
    val overlaid = confirmed?.let { item.copy(played = it.played, favorite = it.favorite) } ?: item
    val updating = userDataWrites.containsKey(item.id)
    return if (overlaid.updating == updating) overlaid else overlaid.copy(updating = updating)
  }

  private fun media(item: VideoLibraryItem, scope: ProfileScopeRef) = MediaUi(item.id, item.name, item.itemType, metadata(item.productionYear, item.itemType), artwork(item.artworkImageId, scope), item.overview.orEmpty(), item.favorite, item.played)
  private fun metadata(year: Int?, type: String) = listOfNotNull(year?.toString(), type).joinToString(" · ")
  private fun artwork(id: String?, scope: ProfileScopeRef): ArtworkUi? = id?.let { ArtworkUi(it, scope) }

  private fun showError(error: Throwable) {
    val resource = when (error) {
      is SdkException.Cancelled, is SdkException.Stale -> return
      is SdkException.Authentication -> R.string.sdk_authentication_failed
      is SdkException.Storage -> R.string.sdk_storage_failed
      is SdkException.InvalidInput -> R.string.sdk_invalid_input
      is SdkException.HandoffAborted -> R.string.sdk_handoff_failed
      is SdkException.NoActiveProfile -> R.string.not_connected
      else -> R.string.sdk_request_failed
    }
    mutableState.update { it.copy(error = app.getString(resource)) }
  }

  override fun onCleared() {
    ++authGeneration
    quickSession?.cancel()
    quickSession?.destroy()
    cancelQuery()
    closeBrowserSession()
    cancelUserDataWrites()
    confirmedUserData.clear()
    visibility.close()
    player.stop()
    super.onCleared()
  }
}
