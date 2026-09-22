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
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import io.github.hewel.jellypilot.player.*

internal class AppViewModel(application: Application, private val sdk: JellypilotSdk) : AndroidViewModel(application) {
  constructor(application: Application) : this(application, (application as JellyPilotApplication).sdk)

  private val app = application as JellyPilotApplication
  private val platformPreferences = AndroidPreferences(app)
  val player = app.player
  private val mutableState = MutableStateFlow(AppUiState())
  val state = mutableState.asStateFlow()
  private val playback = MediaPlaybackCoordinator(
    sdk, player, viewModelScope,
    onPlaybackUi = { value ->
      mutableState.update { it.copy(playbackUi = value, showPlayer = if (value == null) false else it.showPlayer) }
      if (value == null) resumeBrowser()
    },
    onError = { message -> mutableState.update { it.copy(error = message) } },
    onOpenPlayer = ::openPlayer,
    onRecoveryChanged = ::refreshRecovery,
  )
  private val visibility = PlaybackVisibility(application, playback::setEligible)
  private val preferenceWrites = Mutex()
  private var recoveryJob: Job? = null
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
  private var browserSequence = 0L
  private var libraries = emptyList<VideoLibraryShortcut>()
  private var watchlistIds = emptySet<String>()
  private var searchSource = Destination.Home
  private val accountPageHistory = ArrayDeque<AccountPage>()
  private data class DetailPage(val item: MediaUi, val children: List<MediaUi>, val seasonId: String?, val hasMore: Boolean, val tracks: DetailTracksUi?)
  private val details = ArrayDeque<DetailPage>()
  private data class RetainedBrowser(val session: BrowseSession, val generation: Long, val ui: BrowserUi)
  private val retainedBrowsers = mutableMapOf<Destination, RetainedBrowser>()
  private var watchlistEntries = emptyList<WatchlistEntry>()
  private var watchlistRevision = 0L
  private var batchWriteIds = emptySet<String>()
  private data class CollectionUndo(
    val scope: ProfileScopeRef,
    val kind: PersonalListKind,
    val items: List<MediaUi>,
    val order: List<String>,
    val watchlist: WatchlistRemoval?,
  )
  private var collectionUndo: CollectionUndo? = null
  private var undoSequence = 0L
  private var historyOffset = 0
  private data class HistoryUndo(val scope: ProfileScopeRef, val item: MediaUi, val order: List<String>, val receipt: HistoryRemoval)
  private var historyUndo: HistoryUndo? = null
  /** In-flight Favorite/Played writes keyed by item id; each owns its token and job, independent of the browse query slot. */
  private val userDataWrites = mutableMapOf<String, PendingWrite>()
  /** Latest server-confirmed flags per item, used to keep older in-flight reads from republishing pre-write state. */
  private val confirmedUserData = mutableMapOf<String, ConfirmedUserData>()
  private var confirmedRevision = 0L

  private class PendingWrite(val token: OperationToken, val job: Job)
  private class ConfirmedUserData(val revision: Long, val played: Boolean, val favorite: Boolean)

  init {
    app.beforePlaybackHandoff = playback::beforeHandoff
    viewModelScope.launch {
      try {
        val prefill = withContext(Dispatchers.IO) { sdk.loginPrefill() }
        mutableState.update { if (it.showSignIn) it else it.copy(loginServer = prefill.serverUrl, loginUsername = prefill.username, loginJellyfin = prefill.provider == Provider.JELLYFIN, loginRemember = prefill.remember) }
        refreshPreferences()
        val saved = sdk.savedProfiles()
        refreshIdentity(saved.profiles)
        if (sdk.activeProfile() != null) { refresh(); refreshRecovery(); playback.profileChanged() }
        else if (state.value.preferences.startupAutoLogin) saved.lastActivatedKey?.let(::activate)
      } catch (error: Exception) { showError(error) }
    }
  }
  fun setVisible(visible: Boolean) {
    visibility.setVisible(visible)
    if (visible) mutableState.update { it.copy(preferences = it.preferences.copy(language = platformPreferences.language())) }
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
    closeRetainedBrowsers()
    playback.stop()
    mutableState.update { it.copy(showPlayer = false, busy = false) }
  }
  fun dismissError() { mutableState.update { it.copy(error = null) } }
  fun dismissNotice() { mutableState.update { it.copy(notice = null) } }
  fun addAccount() {
    if (state.value.loginBusy || rejectWhileCleanupPending()) return
    mutableState.update { it.copy(showSignIn = true, loginStep = LoginStep.Server, loginIdentity = null, loginPassword = "", loginError = null, loginConnectionLost = false, loginPublicInfoRestricted = false, error = null) }
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
      playback.stop()
      mutableState.update { it.copy(showPlayer = false) }
      resumeBrowser()
    } else if (state.value.detail != null) {
      cancelQuery()
      val previous = details.removeLastOrNull()
      mutableState.update { it.copy(detail = previous?.item, detailItems = previous?.children.orEmpty(), selectedSeasonId = previous?.seasonId, episodesHaveMore = previous?.hasMore ?: false, detailTracks = previous?.tracks, busy = false) }
      resumeBrowser()
    } else if (state.value.destination == Destination.Search) {
      navigate(searchSource)
    } else if (state.value.destination == Destination.Account && state.value.accountPage != AccountPage.Overview) {
      val previous = accountPageHistory.removeLastOrNull() ?: AccountPage.Overview
      mutableState.update { it.copy(accountPage = previous) }
    }
  }

  fun navigate(destination: Destination) {
    if (state.value.destination == destination && state.value.detail == null) return
    if (destination == Destination.Search && state.value.destination != Destination.Search) searchSource = state.value.destination
    cancelQuery()
    searchDebounce?.cancel()
    searchDebounce = null
    retainBrowser()
    details.clear()
    mutableState.update { it.copy(destination = destination, detail = null, detailItems = emptyList(), items = emptyList(), browser = BrowserUi(), busy = false) }
    when (destination) {
      Destination.Account -> viewModelScope.launch { refreshIdentity() }
      Destination.Search -> if (!restoreBrowser(destination)) search(searchText)
      Destination.Library -> if (!restoreBrowser(destination)) openLibraryQuery()
      Destination.Home -> refresh()
      Destination.Lists -> selectList(state.value.selectedList)
    }
  }

  fun cancelLogin() {
    if (state.value.loginCommitting) return
    cancelLoginRequest()
    mutableState.update { it.copy(showSignIn = false, loginStep = LoginStep.Server, loginIdentity = null, loginPassword = "", loginError = null, loginConnectionLost = false, loginPublicInfoRestricted = false) }
  }

  private fun cancelLoginRequest() {
    ++authGeneration
    quickSession?.cancel()
    quickSession?.destroy()
    quickSession = null
    loginJob?.cancel()
    // Keep admission closed until this job's finally block has finished, including
    // a non-cancellable saved-account teardown already past its commit boundary.
    mutableState.update { it.copy(loginBusy = loginJob != null, quickConnectCode = null) }
  }

  fun loginBack() {
    if (state.value.loginCommitting) return
    if (state.value.loginStep == LoginStep.Account) {
      cancelLoginRequest()
      mutableState.update { it.copy(loginStep = LoginStep.Server, loginError = null, loginConnectionLost = false) }
    } else cancelLogin()
  }

  fun changeLoginServer(value: String) {
    if (state.value.loginBusy || state.value.loginServer == value) return
    mutableState.update { it.copy(loginServer = value, loginIdentity = null, loginPassword = "", loginError = null, loginConnectionLost = false, loginPublicInfoRestricted = false) }
  }
  fun changeLoginUsername(value: String) {
    if (!state.value.loginBusy) mutableState.update { it.copy(loginUsername = value, loginError = null) }
  }
  fun changeLoginPassword(value: String) {
    if (!state.value.loginBusy) mutableState.update { it.copy(loginPassword = value, loginError = null) }
  }
  fun changeLoginRemember(value: Boolean) {
    if (!state.value.loginBusy) mutableState.update { it.copy(loginRemember = value) }
  }
  fun changeLoginProvider(jellyfin: Boolean) {
    if (state.value.loginBusy || state.value.loginIdentity?.providerKnown != false) return
    mutableState.update { it.copy(loginJellyfin = jellyfin, loginIdentity = it.loginIdentity?.copy(jellyfin = jellyfin, providerSelected = true), loginError = null) }
  }

  fun continueLoginManually() {
    val input = state.value
    if (input.loginBusy || !input.loginPublicInfoRestricted || input.loginStep != LoginStep.Server) return
    mutableState.update { it.copy(loginStep = LoginStep.Account, loginError = null, loginConnectionLost = false,
      loginIdentity = LoginServerUi(null, it.loginServer.trim(), it.loginJellyfin, providerKnown = false, providerSelected = false)) }
  }

  fun connectLoginServer() {
    val input = state.value
    if (input.loginBusy || input.loginServer.isBlank() || rejectWhileCleanupPending()) return
    val generation = ++authGeneration
    mutableState.update { it.copy(loginBusy = true, loginError = null, loginConnectionLost = false, loginPublicInfoRestricted = false) }
    loginJob = viewModelScope.launch(start = CoroutineStart.LAZY) {
      try {
        val identity = sdk.probeServer(input.loginServer)
        if (generation != authGeneration) return@launch
        mutableState.update { it.copy(loginBusy = false, loginStep = LoginStep.Account,
          loginJellyfin = identity.provider?.let { provider -> provider == Provider.JELLYFIN } ?: it.loginJellyfin,
          loginIdentity = LoginServerUi(identity.serverName, identity.serverUrl, identity.provider?.let { provider -> provider == Provider.JELLYFIN } ?: it.loginJellyfin, identity.provider != null)) }
      } catch (cancelled: CancellationException) { throw cancelled }
      catch (error: Exception) { if (generation == authGeneration) showLoginError(error, connecting = true) }
      finally {
        if (loginJob === currentCoroutineContext().job) {
          loginJob = null
          mutableState.update { it.copy(loginBusy = false) }
        }
      }
    }
    loginJob?.start()
  }

  fun submitLogin() {
    val input = state.value
    val identity = input.loginIdentity ?: return
    if (!identity.providerSelected) return
    signIn(identity.jellyfin, identity.address, input.loginUsername, input.loginPassword, input.loginRemember)
  }

  fun submitQuickConnect() {
    val input = state.value
    val identity = input.loginIdentity ?: return
    if (identity.providerSelected && identity.jellyfin) quickConnect(identity.address, input.loginRemember)
  }

  /** Explains why playback and writes stay blocked until sign-out cleanup is retried. */
  private fun showCleanupPending() {
    mutableState.update { it.copy(error = app.localizedString(R.string.sdk_sign_out_cleanup_pending)) }
  }

  /** New account transitions cannot bypass a pending sign-out cleanup; the SDK also rejects them. */
  private fun rejectWhileCleanupPending(): Boolean {
    if (!sdk.signOutCleanupPending()) return false
    showCleanupPending()
    return true
  }

  fun signIn(jellyfin: Boolean, server: String, username: String, password: String, remember: Boolean) {
    if (state.value.loginBusy || rejectWhileCleanupPending()) return
    mutableState.update { it.copy(loginUsername = username, loginRemember = remember) }
    accountOperation {
      val candidate = sdk.passwordLogin(if (jellyfin) Provider.JELLYFIN else Provider.EMBY, server, username, password)
      try { currentCoroutineContext().ensureActive() }
      catch (cancelled: CancellationException) { candidate.discard(); candidate.destroy(); throw cancelled }
      activateCandidate(candidate, remember)
      withContext(Dispatchers.IO) { sdk.saveLoginPrefill(server, username, if (jellyfin) Provider.JELLYFIN else Provider.EMBY, remember) }
    }
  }

  fun activate(key: String) {
    if (rejectWhileCleanupPending()) return
    accountOperation { activateCandidate(sdk.restoreSavedProfile(key), true) }
  }
  fun disconnect() {
    accountOperation {
      withContext(NonCancellable) {
        playback.blockForHandoff()
        sdk.disconnect()
        connectionChanged()
      }
    }
  }
  /** Explicit retry of a failed sign-out teardown; `disconnect` performs the SDK cleanup retry. */
  fun retryCleanup() = disconnect()
  fun signOut(key: String, deleteWatchlist: Boolean = false) {
    accountOperation {
      withContext(NonCancellable) {
        val activeBefore = sdk.activeProfile()?.key
        // Media-session commands bypass this ViewModel. Block the native
        // player before deletion starts, not only when the teardown hook runs.
        if (activeBefore == key) playback.blockForHandoff()
        val outcome = sdk.signOut(key, deleteWatchlist)
        // A failed teardown keeps the session connected for a cleanup retry:
        // only the saved-profile list changes, never the connection reset.
        if (activeBefore == key && outcome.teardownError == null) connectionChanged(outcome.remaining) else refreshIdentity(outcome.remaining)
        if (outcome.watchlistError != null) mutableState.update { it.copy(watchlistCleanupKeys = (it.watchlistCleanupKeys + key).distinct()) }
        val warnings = listOfNotNull(
          outcome.teardownError?.let { app.localizedString(R.string.sdk_sign_out_cleanup_pending) },
          outcome.watchlistError?.let { app.localizedString(R.string.sdk_watchlist_delete_failed) },
          outcome.recoveryError?.let { app.localizedString(R.string.recovery_cleanup_failed) },
        )
        if (warnings.isNotEmpty()) mutableState.update { it.copy(error = warnings.joinToString("\n")) }
      }
    }
  }

  fun retryWatchlistCleanup(key: String) {
    accountOperation {
      sdk.retryWatchlistCleanup(key)
      mutableState.update { it.copy(watchlistCleanupKeys = it.watchlistCleanupKeys - key) }
    }
  }

  private suspend fun activateCandidate(candidate: ProfileCandidate, remember: Boolean) = withContext(NonCancellable) {
    mutableState.update { it.copy(loginCommitting = true) }
    var activated = false
    try {
      playback.blockForHandoff()
      val outcome = sdk.activateCandidate(candidate, remember)
      activated = true
      connectionChanged()
      mutableState.update { it.copy(showSignIn = false, quickConnectCode = null, loginPassword = "", loginIdentity = null, loginError = null, loginStep = LoginStep.Server, loginConnectionLost = false, loginPublicInfoRestricted = false) }
      if (outcome.persistenceWarning != null) {
        mutableState.update { it.copy(error = app.localizedString(R.string.sdk_activated_persistence_failed)) }
      }
    } finally {
      if (!activated) candidate.discard()
      candidate.destroy()
      mutableState.update { it.copy(loginCommitting = false) }
    }
  }

  private fun accountOperation(action: suspend () -> Unit) {
    if (state.value.loginBusy) return
    val generation = ++authGeneration
    mutableState.update { it.copy(loginBusy = true, error = null, loginError = null) }
    loginJob = viewModelScope.launch(start = CoroutineStart.LAZY) {
      try { action() }
      catch (cancelled: CancellationException) { throw cancelled }
      catch (error: Exception) {
        if (generation == authGeneration) {
          if (state.value.showSignIn) showLoginError(error) else showError(error)
        }
      }
      finally {
        // A failed sign-out teardown leaves the SDK mutation block in place
        // for the cleanup retry; only a settled operation may unblock.
        player.setHandoffBlocked(sdk.contentMutationsBlocked())
        playback.profileChanged()
        if (loginJob === currentCoroutineContext().job) {
          loginJob = null
          val cleanupPending = sdk.signOutCleanupPending()
          mutableState.update { it.copy(loginBusy = false, signOutCleanupPending = cleanupPending) }
        }
      }
    }
    loginJob?.start()
  }

  fun quickConnect(server: String, remember: Boolean) {
    if (state.value.loginBusy || rejectWhileCleanupPending()) return
    val generation = ++authGeneration
    mutableState.update { it.copy(loginBusy = true, error = null, loginError = null) }
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
              is QuickConnectOutcome.Success -> accountOperation {
                activateCandidate(outcome.candidate, remember)
                withContext(Dispatchers.IO) { sdk.saveLoginPrefill(server, sdk.activeProfile()?.userName.orEmpty(), Provider.JELLYFIN, remember) }
              }
              is QuickConnectOutcome.Failed -> showLoginError(outcome.error)
              QuickConnectOutcome.Cancelled -> Unit
            }
          }
        }
      })
    } catch (error: Exception) {
      mutableState.update { it.copy(loginBusy = false) }
      showLoginError(error)
    }
  }

  private fun showLoginError(error: Throwable, connecting: Boolean = false) {
    if (error is SdkException.Cancelled || error is SdkException.Stale) return
    val connectionLost = error is SdkException.Request
    val publicInfoRestricted = connecting && error is SdkException.ServerInfoRestricted
    val resource = when {
      publicInfoRestricted -> R.string.login_public_info_restricted
      connecting && error is SdkException.InvalidInput -> R.string.login_invalid_address
      connecting -> R.string.login_connection_failed
      error is SdkException.Authentication -> R.string.login_credentials_failed
      connectionLost -> R.string.login_connection_lost
      error is SdkException.Storage -> R.string.sdk_storage_failed
      error is SdkException.HandoffAborted -> R.string.sdk_handoff_failed
      error is SdkException.InvalidInput -> R.string.sdk_invalid_input
      else -> R.string.sdk_request_failed
    }
    mutableState.update { it.copy(loginError = app.localizedString(resource), loginConnectionLost = connectionLost,
      loginPublicInfoRestricted = if (connecting) publicInfoRestricted else it.loginPublicInfoRestricted) }
  }

  private suspend fun refreshIdentity(saved: List<SavedProfile>? = null) {
    val active = sdk.activeProfile()
    val cleanupPending = sdk.signOutCleanupPending()
    mutableState.update { it.copy(activeName = active?.userName, activeProfileKey = active?.key, signOutCleanupPending = cleanupPending) }
    try {
      val snapshot = sdk.savedProfiles()
      val profiles = (saved ?: snapshot.profiles).map { profile ->
        val provider = if (profile.provider == Provider.JELLYFIN) "Jellyfin" else "Emby"
        ProfileUi(profile.key, profile.userName, profile.serverName?.takeIf { it.isNotBlank() } ?: provider, provider, profile.key == active?.key, profile.serverUrl)
      }
      mutableState.update { it.copy(profiles = profiles, selectedProfileKey = snapshot.lastActivatedKey) }
    } catch (cancelled: CancellationException) { throw cancelled }
    catch (error: Exception) { showError(error) }
  }

  private suspend fun connectionChanged(saved: List<SavedProfile>? = null) {
    accountPageHistory.clear()
    cancelQuery()
    closeBrowserSession()
    closeRetainedBrowsers()
    cancelUserDataWrites()
    confirmedUserData.clear()
    libraries = emptyList()
    watchlistIds = emptySet()
    watchlistEntries = emptyList()
    ++watchlistRevision
    collectionUndo?.watchlist?.destroy()
    collectionUndo = null
    historyUndo?.receipt?.destroy()
    historyUndo = null
    historyOffset = 0
    batchWriteIds = emptySet()
    details.clear()
    searchText = ""
    mutableState.update { it.copy(activeName = sdk.activeProfile()?.userName, activeProfileKey = sdk.activeProfile()?.key, showPlayer = false, playbackUi = null, listBusy = false, listUndo = null, historyBusy = false, historyUndo = null, detailTracks = null, items = emptyList(), featured = emptyList(), homeRows = emptyList(), listItems = emptyList(), historyItems = emptyList(), listCount = 0, favoriteCount = 0, recovery = null, browser = BrowserUi(), detail = null, detailItems = emptyList(), libraries = emptyList(), libraryId = null, busy = false, destination = Destination.Home, searchQuery = "", accountPage = AccountPage.Overview) }
    refreshIdentity(saved)
    playback.profileChanged()
    refreshRecovery()
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
    val generation = ++browserSequence
    browserGeneration = generation
    browseSession = session
    // A session opened while detail/player is showing starts suspended;
    // back() resumes it when the temporary navigation ends.
    if (state.value.detail != null || state.value.showPlayer) suspendBrowser()
    collectBrowser(session, generation)
  }

  private fun collectBrowser(session: BrowseSession, generation: Long) {
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

  private fun retainBrowser() {
    val session = browseSession ?: return
    val destination = state.value.destination
    if (destination != Destination.Library && destination != Destination.Search) { closeBrowserSession(); return }
    suspendBrowser()
    browseCollector?.cancel()
    browseCollector = null
    retainedBrowsers.put(destination, RetainedBrowser(session, browserGeneration, state.value.browser))?.session?.let {
      it.shutdown(); it.destroy()
    }
    browseSession = null
    browserSuspended = false
  }

  private fun restoreBrowser(destination: Destination): Boolean {
    val retained = retainedBrowsers.remove(destination) ?: return false
    browseSession = retained.session
    browserGeneration = retained.generation
    browserSuspended = true
    mutableState.update { it.copy(browser = retained.ui) }
    resumeBrowser()
    collectBrowser(retained.session, retained.generation)
    return true
  }

  private fun closeRetainedBrowsers() {
    retainedBrowsers.values.forEach { it.session.shutdown(); it.session.destroy() }
    retainedBrowsers.clear()
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
      Destination.Lists -> selectList(state.value.selectedList)
      Destination.Home -> query { token, scope, base ->
        loadLibraries(token)
        refreshWatchlist(token)
        val home = sdk.videoHome(token)
        val presenter = CatalogPresentation(scope, watchlistIds)
        val rows = mutableListOf<HomeRowUi>()
        if (home.continueWatching.isNotEmpty()) rows += HomeRowUi("continue", app.localizedString(R.string.continue_watching), home.continueWatching.map { presented(presenter.library(it), base) }, true)
        if (home.nextUp.isNotEmpty()) rows += HomeRowUi("next", app.localizedString(R.string.next_up), home.nextUp.map { presented(presenter.library(it), base) }, true)
        val latest = mutableListOf<List<VideoLibraryItem>>()
        for (library in libraries) {
          val items = sdk.libraryLatest(token, library.id)
          latest.add(items)
          rows += HomeRowUi(library.id, library.name, items.map { presented(presenter.library(it), base) }, libraryId = library.id)
        }
        val candidates = sdk.homeFeaturedItems(home, latest).take(5)
        val parentIds = candidates.filter { it.itemType == "Episode" }.mapNotNull { it.seriesId }.distinct()
        val parents = if (parentIds.isEmpty()) emptyMap() else sdk.videoItemsByIds(token, parentIds).associateBy { it.id }
        val featured = candidates.map { item ->
          val child = presented(presenter.library(item), base)
          parents[item.seriesId]?.let { parent ->
            presented(presenter.library(parent), base).copy(
              playTargetId = item.id, resumeSeconds = child.resumeSeconds,
              durationSeconds = child.durationSeconds, episodeCode = child.episodeCode,
            )
          } ?: child
        }
        currentCoroutineContext().ensureActive()
        mutableState.update { it.copy(items = rows.flatMap { row -> row.items }.distinctBy { item -> item.id }, homeRows = rows, featured = featured) }
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
          sort = when (state.value.librarySort) { LibrarySort.DateAdded -> VideoLibrarySort.RECENTLY_ADDED; LibrarySort.Title -> VideoLibrarySort.TITLE; LibrarySort.Year -> VideoLibrarySort.RELEASE_DATE },
          sortDirection = if (state.value.librarySort == LibrarySort.Title) VideoLibrarySortDirection.ASCENDING else VideoLibrarySortDirection.DESCENDING,
          playedFilter = when (state.value.libraryPlayed) { PlayedFilter.All -> VideoLibraryPlayedFilter.ALL; PlayedFilter.Played -> VideoLibraryPlayedFilter.PLAYED; PlayedFilter.Unplayed -> VideoLibraryPlayedFilter.UNPLAYED },
          favoritesOnly = state.value.libraryFavorites,
        ),
      ),
    )
  }
  fun setLibraryFilters(played: PlayedFilter, favorites: Boolean) {
    mutableState.update { it.copy(libraryPlayed = played, libraryFavorites = favorites) }
    openLibrarySession()
  }
  fun setLibrarySort(sort: LibrarySort) {
    mutableState.update { it.copy(librarySort = sort) }
    openLibrarySession()
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
    val previous = state.value.detail?.takeIf { it.id != id }?.let { DetailPage(it, state.value.detailItems, state.value.selectedSeasonId, state.value.episodesHaveMore, state.value.detailTracks) }
    query { token, scope, base ->
      var detailShown = false
      try {
        val type = (state.value.browser.slots.asSequence().filterNotNull() + state.value.items + state.value.detailItems + state.value.listItems + state.value.historyItems + state.value.featured)
          .firstOrNull { it.id == id }?.itemType
          ?: state.value.detail?.takeIf { it.id == id }?.itemType
        refreshWatchlist(token)
        val presenter = CatalogPresentation(scope, watchlistIds)
        var episodes = emptyList<MediaUi>()
        var selectedSeason: String? = null
        var more = false
        val detail = if (type == "Series") {
          val show = sdk.showDetail(token, id)
          selectedSeason = show.seasons.firstOrNull { it.seasonNumber == show.nextEpisode?.seasonNumber }?.id ?: show.seasons.firstOrNull()?.id
          val page = sdk.seasonEpisodesPage(token, VideoSeasonEpisodesPageRequest(id, selectedSeason, null, 0, 50))
          episodes = page.episodes.map { presented(presenter.library(it), base) }
          more = page.hasMore
          presenter.show(show)
        } else {
          val item = sdk.itemDetail(token, id)
          presenter.item(item)
        }
        currentCoroutineContext().ensureActive()
        previous?.takeIf { it.item.id != id }?.let(details::addLast)
        mutableState.update { it.copy(detail = presented(detail, base), detailTracks = null, detailItems = episodes, selectedSeasonId = selectedSeason, episodesHaveMore = more) }
        detailShown = true
        val related = sdk.similarVideo(token, id)
        currentCoroutineContext().ensureActive()
        mutableState.update { it.copy(detail = it.detail?.copy(related = related.map { item -> presented(presenter.library(item), base) })) }
      } catch (error: Exception) {
        // The retained browser stays suspended only while a detail is actually
        // shown; a failed or fenced load must hand it back to the grid, but
        // never over a newer suspend intent (player, newer detail, session swap).
        if (!detailShown) resumeBrowserIfCurrent(suspendEpoch)
        throw error
      }
    }
  }

  fun openHomeLibrary(id: String) {
    navigate(Destination.Library)
    selectLibrary(id)
  }

  fun loadDetailTracks() {
    val detail = state.value.detail ?: return
    val target = detail.playTargetId ?: return
    val existing = state.value.detailTracks
    if (existing?.targetId == target && (existing.busy || existing.error == null)) return
    mutableState.update { it.copy(detailTracks = DetailTracksUi(target, busy = true)) }
    query { token, _, _ ->
      try {
        val streams = sdk.itemStreams(token, target)
        currentCoroutineContext().ensureActive()
        if (state.value.detail?.playTargetId != target) return@query
        fun option(value: VideoPlaybackStreamOption) = DetailTrackUi(value.index, value.label, value.language, value.codec, value.isDefault, value.isExternal)
        mutableState.update { it.copy(detailTracks = DetailTracksUi(target, streams.audioStreams.map(::option), streams.subtitleStreams.map(::option))) }
      } catch (cancelled: CancellationException) { throw cancelled }
      catch (error: Exception) {
        if (state.value.detail?.playTargetId == target) mutableState.update { it.copy(detailTracks = DetailTracksUi(target, error = app.localizedString(R.string.sdk_request_failed))) }
      }
    }
  }

  fun selectDetailAudio(index: Int?) {
    val tracks = state.value.detailTracks ?: return
    if (index != null && tracks.audio.none { it.index == index }) return
    mutableState.update { it.copy(detailTracks = tracks.copy(selectedAudio = index)) }
  }
  fun selectDetailSubtitle(index: Int?) {
    val tracks = state.value.detailTracks ?: return
    if (index != null && index != -1 && tracks.subtitles.none { it.index == index }) return
    mutableState.update { it.copy(detailTracks = tracks.copy(selectedSubtitle = index)) }
  }

  fun playItem(id: String, fromBeginning: Boolean = false) {
    if (rejectWhileCleanupPending()) return
    val choice = state.value.detailTracks?.takeIf { it.targetId == id }
    val selection = choice?.let { PlaybackSelection(null, it.selectedAudio, it.selectedSubtitle) }
    playback.play(id, fromBeginning, selection)
  }
  fun playPlayback() = playback.playPlayback()
  fun pausePlayback() = playback.pausePlayback()
  fun seekPlayback(seconds: Double) = playback.seek(seconds)
  fun setPlaybackVolume(volume: Int) = playback.volume(volume)
  fun selectPlaybackTrack(kind: TrackKind, index: Int) = playback.selectTrack(kind, index)
  fun previousEpisode() = playback.previous()
  fun nextEpisode() = playback.next()
  fun setSessionAutoSkip(enabled: Boolean) = playback.sessionAutoSkip(enabled)
  fun skipSegment() = playback.skip()
  fun undoSkip() = playback.undoSkip()
  fun dismissSkipUndo() = playback.dismissSkipUndo()
  fun acknowledgeSkipPrompt(presented: Boolean) = playback.acknowledgeSkipPrompt(presented)
  fun loadMorePlaybackEpisodes() = playback.loadMoreQueue()
  fun restoreLocalPlayback() = playback.restoreRecovery()

  private fun refreshRecovery() {
    recoveryJob?.cancel()
    if (sdk.activeProfile() == null) {
      mutableState.update { it.copy(recovery = null) }
      return
    }
    recoveryJob = viewModelScope.launch {
      var token: OperationToken? = null
      try {
        token = sdk.newOperationToken()
        val recovery = sdk.localPlaybackRecovery(token)
        currentCoroutineContext().ensureActive()
        if (sdk.isScopeActive(token.scopeRef())) mutableState.update { it.copy(recovery = recovery?.let { record -> RecoveryUi(record.title, record.positionSeconds, record.itemId) }) }
      } catch (cancelled: CancellationException) { throw cancelled }
      catch (error: Exception) { showError(error) }
      finally { token?.cancel(); token?.destroy() }
    }
  }

  fun clearLocalRecovery() {
    recoveryJob?.cancel()
    recoveryJob = viewModelScope.launch {
      var token: OperationToken? = null
      try {
        token = sdk.newOperationToken()
        sdk.clearLocalPlaybackRecovery(token)
        currentCoroutineContext().ensureActive()
        if (sdk.isScopeActive(token.scopeRef())) mutableState.update { it.copy(recovery = null) }
      } catch (cancelled: CancellationException) { throw cancelled }
      catch (error: Exception) { showError(error) }
      finally { token?.cancel(); token?.destroy() }
    }
  }

  private suspend fun refreshPreferences() {
    val business = withContext(Dispatchers.IO) { sdk.businessPreferences() }
    val presentation = platformPreferences.presentation()
    mutableState.update { it.copy(preferences = presentation.copy(
      startupAutoLogin = business.autoLogin,
      targetName = business.playbackTargetName ?: "JellyPilot Android",
      autoPlayNext = business.autoPlayNext,
      introMode = when (business.introMode) {
        IntroSkipMode.AUTOMATIC -> IntroPreference.Auto
        IntroSkipMode.MANUAL -> IntroPreference.Manual
        IntroSkipMode.OFF -> IntroPreference.Off
      },
      preferredSubtitleLanguage = business.subtitleLanguages.joinToString(", "),
      preferOriginalAudio = business.preferOriginalAudio,
      rememberSeasonVolume = business.rememberSeasonVolume,
    )) }
  }

  fun updatePreferences(value: PreferencesUi) {
    // Capture the user's changed fields, so queued saves cannot undo a different control's edit.
    val previous = state.value.preferences
    viewModelScope.launch {
      preferenceWrites.withLock {
        try {
          if (value.theme != previous.theme) platformPreferences.setTheme(value.theme)
          if (value.reducedMotion != previous.reducedMotion) platformPreferences.setReducedMotion(value.reducedMotion)
          if (value.language != previous.language) platformPreferences.setLanguage(value.language)
          withContext(Dispatchers.IO) {
            if (value.startupAutoLogin != previous.startupAutoLogin) sdk.setAutoLogin(value.startupAutoLogin)
            if (value.targetName != previous.targetName) sdk.setPlaybackTargetName(value.targetName)
            if (value.autoPlayNext != previous.autoPlayNext) sdk.setAutoPlayNext(value.autoPlayNext)
            if (value.preferOriginalAudio != previous.preferOriginalAudio) sdk.setPreferOriginalAudio(value.preferOriginalAudio)
            if (value.rememberSeasonVolume != previous.rememberSeasonVolume) sdk.setRememberSeasonVolume(value.rememberSeasonVolume)
            if (value.introMode != previous.introMode) sdk.setIntroMode(when (value.introMode) {
              IntroPreference.Auto -> IntroSkipMode.AUTOMATIC
              IntroPreference.Manual -> IntroSkipMode.MANUAL
              IntroPreference.Off -> IntroSkipMode.OFF
            })
            if (value.preferredSubtitleLanguage != previous.preferredSubtitleLanguage) {
              sdk.setSubtitleLanguages(value.preferredSubtitleLanguage.split(',').map(String::trim).filter(String::isNotBlank))
            }
          }
          if (value.targetName != previous.targetName) playback.profileChanged()
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) { showError(error) }
        finally {
          try { refreshPreferences() }
          catch (cancelled: CancellationException) { throw cancelled }
          catch (error: Exception) { showError(error) }
        }
      }
    }
  }

  fun clearImageCache() {
    viewModelScope.launch {
      try {
        clearArtworkCache(app)
        mutableState.update { it.copy(notice = app.localizedString(R.string.cache_cleared)) }
      } catch (cancelled: CancellationException) { throw cancelled }
      catch (error: Exception) { showError(error) }
    }
  }

  fun exportDiagnostics() {
    viewModelScope.launch {
      try {
        val current = player.snapshot.value
        exportAndroidDiagnostics(app, AndroidDiagnosticState(
          signedIn = sdk.activeProfile() != null, savedProfiles = state.value.profiles.size,
          playbackStatus = current.status, playerReady = player.ready.value,
          recoveryAvailable = state.value.recovery != null, playbackError = player.error.value != null,
          reportingError = playback.reportingError,
        ))
      } catch (cancelled: CancellationException) { throw cancelled }
      catch (error: Exception) { showError(error) }
    }
  }

  fun selectSeason(id: String) {
    val current = state.value
    val detail = current.detail ?: return
    if (detail.itemType != "Series" || current.selectedSeasonId == id) return
    mutableState.update { it.copy(selectedSeasonId = id, detailItems = emptyList(), episodesHaveMore = true) }
    // A new season replaces the pending query even while its previous page is loading.
    loadEpisodes(detail.id, id, 0)
  }

  fun loadMoreEpisodes() {
    val current = state.value
    val detail = current.detail ?: return
    if (detail.itemType != "Series" || current.busy || !current.episodesHaveMore) return
    loadEpisodes(detail.id, current.selectedSeasonId, current.detailItems.size)
  }

  private fun loadEpisodes(seriesId: String, season: String?, start: Int) {
    query { token, scope, base ->
      val page = sdk.seasonEpisodesPage(token, VideoSeasonEpisodesPageRequest(seriesId, season, null, start, 50))
      currentCoroutineContext().ensureActive()
      val items = page.episodes.map { presented(media(it, scope), base) }
      mutableState.update {
        if (it.detail?.id != seriesId || it.selectedSeasonId != season) it
        else it.copy(detailItems = (it.detailItems + items).distinctBy { item -> item.id }, episodesHaveMore = page.hasMore)
      }
    }
  }

  private suspend fun refreshWatchlist(token: OperationToken) {
    val revision = watchlistRevision
    val entries = sdk.watchlistItems(token)
    currentCoroutineContext().ensureActive()
    if (revision != watchlistRevision) return
    watchlistIds = entries.map { it.itemId }.toSet()
    watchlistEntries = entries
    mutableState.update { it.copy(listCount = entries.size) }
  }

  fun selectList(kind: PersonalListKind) {
    mutableState.update { it.copy(selectedList = kind, listItems = emptyList(), listHasMore = false) }
    loadListPage(reset = true)
  }

  fun loadMoreList() {
    if (state.value.listHasMore && !state.value.busy) loadListPage(reset = false)
  }

  private fun loadListPage(reset: Boolean) {
    val kind = state.value.selectedList
    val start = if (reset) 0 else state.value.listItems.size
    query { token, scope, base ->
      refreshWatchlist(token)
      val presenter = CatalogPresentation(scope, watchlistIds)
      val page: List<MediaUi>
      val more: Boolean
      if (kind == PersonalListKind.Watchlist) {
        val entries = watchlistEntries.drop(start).take(50)
        val actual = if (entries.isEmpty()) emptyMap() else sdk.videoItemsByIds(token, entries.map { it.itemId }).associateBy { it.id }
        page = entries.map { actual[it.itemId]?.let(presenter::library) ?: presenter.unavailable(it) }
        more = start + entries.size < watchlistEntries.size
      } else {
        val result = sdk.favorites(token, start, 50)
        page = result.items.map(presenter::library)
        more = result.hasMore
        mutableState.update { it.copy(favoriteCount = result.totalRecordCount) }
      }
      currentCoroutineContext().ensureActive()
      mutableState.update {
        it.copy(listItems = ((if (reset) emptyList() else it.listItems) + page.map { item -> presented(item, base) }).distinctBy { item -> item.id }, listHasMore = more)
      }
    }
  }

  fun openAccountPage(page: AccountPage) {
    if (state.value.accountPage == page) return
    if (page == AccountPage.Overview) accountPageHistory.clear()
    else accountPageHistory.addLast(state.value.accountPage)
    mutableState.update { it.copy(accountPage = page) }
    if (page == AccountPage.History) loadHistory(reset = true)
  }
  fun loadMoreHistory() {
    if (state.value.historyHasMore && !state.value.busy) loadHistory(reset = false)
  }
  private fun loadHistory(reset: Boolean) {
    val start = if (reset) 0 else historyOffset
    query { token, scope, base ->
      val page = sdk.watchHistory(token, start, 50)
      currentCoroutineContext().ensureActive()
      historyOffset = page.nextStartIndex
      mutableState.update {
        it.copy(historyItems = ((if (reset) emptyList() else it.historyItems) + page.items.map { item -> presented(media(item, scope), base) }).distinctBy { item -> item.id }, historyHasMore = page.hasMore)
      }
    }
  }

  fun removeHistoryItem(id: String) {
    if (state.value.historyBusy || state.value.historyUndo?.busy == true || sdk.contentMutationsBlocked()) return
    val item = state.value.historyItems.firstOrNull { it.id == id } ?: return
    val order = state.value.historyItems.map { it.id }
    dismissHistoryUndo()
    val token = try { sdk.newOperationToken() } catch (error: Exception) { showError(error); return }
    val scope = token.scopeRef()
    mutableState.update { it.copy(historyBusy = true) }
    viewModelScope.launch {
      var receipt: HistoryRemoval? = null
      try {
        receipt = sdk.hideHistoryItem(token, id)
        currentCoroutineContext().ensureActive()
        if (!sdk.isScopeActive(scope)) return@launch
        historyUndo = HistoryUndo(scope, item, order, receipt)
        receipt = null
        mutableState.update { it.copy(historyItems = it.historyItems.filterNot { item -> item.id == id }, historyUndo = ListUndoUi(++undoSequence, 1)) }
      } catch (cancelled: CancellationException) { throw cancelled }
      catch (error: Exception) { showError(error) }
      finally {
        receipt?.destroy(); token.cancel(); token.destroy()
        if (sdk.isScopeActive(scope)) mutableState.update { it.copy(historyBusy = false) }
      }
    }
  }

  fun undoHistoryRemoval() {
    val undo = historyUndo ?: return
    if (state.value.historyBusy || state.value.historyUndo?.busy == true || !sdk.isScopeActive(undo.scope)) return
    mutableState.update { it.copy(historyUndo = it.historyUndo?.copy(busy = true, error = null)) }
    viewModelScope.launch {
      var restored = false
      try {
        restored = undo.receipt.undo()
        if (!sdk.isScopeActive(undo.scope) || historyUndo !== undo) return@launch
        if (restored) mutableState.update { current -> current.copy(historyItems = (current.historyItems + undo.item).distinctBy { it.id }.sortedBy { undo.order.indexOf(it.id).takeIf { index -> index >= 0 } ?: Int.MAX_VALUE }) }
      } catch (cancelled: CancellationException) { throw cancelled }
      catch (_: Exception) {
        if (sdk.isScopeActive(undo.scope) && historyUndo === undo) mutableState.update { it.copy(historyUndo = it.historyUndo?.copy(error = app.localizedString(R.string.sdk_request_failed))) }
      } finally {
        if (sdk.isScopeActive(undo.scope) && historyUndo === undo) {
          mutableState.update { it.copy(historyUndo = it.historyUndo?.copy(busy = false)) }
          if (restored) dismissHistoryUndo()
        }
      }
    }
  }

  fun dismissHistoryUndo() {
    if (state.value.historyUndo?.busy == true) return
    historyUndo?.receipt?.destroy()
    historyUndo = null
    mutableState.update { it.copy(historyUndo = null) }
  }

  fun setWatchlist(id: String, added: Boolean) {
    if (userDataWrites.containsKey(id) || id in batchWriteIds || sdk.contentMutationsBlocked()) return
    val token = try { sdk.newOperationToken() } catch (error: Exception) { showError(error); return }
    val job = viewModelScope.launch(start = CoroutineStart.LAZY) {
      try {
        val scope = token.scopeRef()
        if (added) {
          val item = sdk.videoItemsByIds(token, listOf(id)).firstOrNull { it.id == id }
            ?: error("Media is unavailable")
          sdk.watchlistAdd(token, item)
        } else sdk.watchlistRemove(token, id)
        currentCoroutineContext().ensureActive()
        if (!sdk.isScopeActive(scope)) return@launch
        ++watchlistRevision
        refreshWatchlist(token)
        projectAll { item -> if (item.id == id) item.copy(inWatchlist = added) else item }
        if (!added && state.value.selectedList == PersonalListKind.Watchlist) mutableState.update { it.copy(listItems = it.listItems.filterNot { item -> item.id == id }) }
      } catch (cancelled: CancellationException) { throw cancelled }
      catch (error: Exception) { showError(error) }
      finally {
        token.cancel(); token.destroy()
        if (userDataWrites[id]?.job === currentCoroutineContext().job) {
          userDataWrites.remove(id); setWritePending(id, false)
        }
      }
    }
    userDataWrites[id] = PendingWrite(token, job)
    setWritePending(id, true)
    job.start()
  }

  fun removeListItems(ids: List<String>) {
    if (state.value.listBusy || state.value.listUndo?.busy == true || ids.isEmpty() || sdk.contentMutationsBlocked()) return
    val selected = state.value.listItems.filter { it.id in ids }
    if (selected.isEmpty() || selected.any { it.updating }) return
    val kind = state.value.selectedList
    val order = state.value.listItems.map { it.id }
    dismissListUndo()
    val token = try { sdk.newOperationToken() } catch (error: Exception) { showError(error); return }
    batchWriteIds = selected.map { it.id }.toSet()
    batchWriteIds.forEach { setWritePending(it, true) }
    mutableState.update { it.copy(listBusy = true) }
    viewModelScope.launch {
      val removed = mutableListOf<MediaUi>()
      var receipt: WatchlistRemoval? = null
      val scope = token.scopeRef()
      try {
        if (kind == PersonalListKind.Watchlist) {
          receipt = sdk.removeWatchlistItems(token, selected.map { it.id })
          if (!sdk.isScopeActive(scope)) return@launch
          val confirmed = receipt.removedEntries().map { it.itemId }.toSet()
          removed += selected.filter { it.id in confirmed }
          ++watchlistRevision
          refreshWatchlist(token)
        } else {
          for (item in selected) {
            val result = sdk.updateUserData(token, item.id, VideoUserDataAction.UNFAVORITE)
            if (!sdk.isScopeActive(scope)) return@launch
            confirmedUserData[item.id] = ConfirmedUserData(++confirmedRevision, result.played, result.favorite)
            projectAll { if (it.id == item.id) it.copy(favorite = result.favorite, played = result.played) else it }
            removed += item
          }
        }
      } catch (cancelled: CancellationException) { throw cancelled }
      catch (error: Exception) { showError(error) }
      finally {
        token.cancel(); token.destroy()
        if (sdk.isScopeActive(scope)) {
          val removedIds = removed.map { it.id }.toSet()
          if (removed.isNotEmpty()) {
            collectionUndo = CollectionUndo(scope, kind, removed, order, receipt)
            mutableState.update { it.copy(
              listItems = if (it.selectedList == kind) it.listItems.filterNot { item -> item.id in removedIds } else it.listItems,
              favoriteCount = if (kind == PersonalListKind.Favorites) (it.favoriteCount - removed.size).coerceAtLeast(0) else it.favoriteCount,
              listUndo = ListUndoUi(++undoSequence, removed.size),
            ) }
            if (kind == PersonalListKind.Watchlist) projectAll { if (it.id in removedIds) it.copy(inWatchlist = false) else it }
          } else receipt?.destroy()
          val pending = batchWriteIds
          batchWriteIds = emptySet()
          pending.forEach { setWritePending(it, false) }
          mutableState.update { it.copy(listBusy = false) }
        } else receipt?.destroy()
      }
    }
  }

  fun undoListRemoval() {
    val undo = collectionUndo ?: return
    if (state.value.listBusy || state.value.listUndo?.busy == true || !sdk.isScopeActive(undo.scope)) return
    batchWriteIds = undo.items.map { it.id }.toSet()
    batchWriteIds.forEach { setWritePending(it, true) }
    mutableState.update { it.copy(listUndo = it.listUndo?.copy(busy = true, error = null)) }
    viewModelScope.launch {
      val restored = mutableListOf<MediaUi>()
      var token: OperationToken? = null
      try {
        if (undo.watchlist != null) {
          if (undo.watchlist.undo()) {
            if (!sdk.isScopeActive(undo.scope)) return@launch
            restored += undo.items
            ++watchlistRevision
          }
        } else {
          token = sdk.newOperationToken()
          for (item in undo.items) {
            val result = sdk.updateUserData(token, item.id, VideoUserDataAction.FAVORITE)
            if (!sdk.isScopeActive(undo.scope)) return@launch
            confirmedUserData[item.id] = ConfirmedUserData(++confirmedRevision, result.played, result.favorite)
            restored += item.copy(favorite = result.favorite, played = result.played)
          }
        }
        if (!sdk.isScopeActive(undo.scope)) return@launch
        if (undo.watchlist != null) {
          token = sdk.newOperationToken()
          refreshWatchlist(token)
        }
      } catch (cancelled: CancellationException) { throw cancelled }
      catch (_: Exception) {
        if (sdk.isScopeActive(undo.scope)) mutableState.update { it.copy(listUndo = it.listUndo?.copy(error = app.localizedString(R.string.sdk_request_failed))) }
      } finally {
        token?.cancel(); token?.destroy()
        if (sdk.isScopeActive(undo.scope) && collectionUndo === undo) {
          val pending = batchWriteIds
          batchWriteIds = emptySet()
          pending.forEach { setWritePending(it, false) }
          val restoredIds = restored.map { it.id }.toSet()
          projectAll { item -> if (item.id !in restoredIds) item else if (undo.kind == PersonalListKind.Watchlist) item.copy(inWatchlist = true) else item.copy(favorite = true) }
          mutableState.update { current ->
            val combined = if (current.selectedList == undo.kind) (current.listItems + restored).distinctBy { it.id }.sortedBy { item -> undo.order.indexOf(item.id).takeIf { it >= 0 } ?: Int.MAX_VALUE } else current.listItems
            current.copy(listItems = combined,
              favoriteCount = if (undo.kind == PersonalListKind.Favorites) current.favoriteCount + restored.size else current.favoriteCount,
              listUndo = current.listUndo?.copy(busy = false))
          }
          val remaining = undo.items.filterNot { it.id in restoredIds }
          if (remaining.isEmpty()) dismissListUndo() else collectionUndo = undo.copy(items = remaining)
        }
      }
    }
  }

  fun dismissListUndo() {
    if (state.value.listUndo?.busy == true) return
    collectionUndo?.watchlist?.destroy()
    collectionUndo = null
    mutableState.update { it.copy(listUndo = null) }
  }

  private fun projectAll(transform: (MediaUi) -> MediaUi) {
    fun projected(item: MediaUi): MediaUi = transform(item).let { it.copy(related = it.related.map(transform)) }
    mutableState.update { state -> state.copy(
      items = state.items.map(::projected), featured = state.featured.map(::projected),
      homeRows = state.homeRows.map { it.copy(items = it.items.map(::projected)) },
      listItems = state.listItems.map(::projected), historyItems = state.historyItems.map(::projected),
      browser = state.browser.copy(slots = state.browser.slots.map { it?.let(::projected) }),
      detail = state.detail?.let(::projected), detailItems = state.detailItems.map(::projected),
    ) }
    details.indices.forEach { index -> details[index] = details[index].let { it.copy(item = projected(it.item), children = it.children.map(::projected)) } }
    retainedBrowsers.replaceAll { _, retained -> retained.copy(ui = retained.ui.copy(slots = retained.ui.slots.map { it?.let(::projected) })) }
  }

  fun setFavorite(id: String, favorite: Boolean) { updateUserData(id, if (favorite) VideoUserDataAction.FAVORITE else VideoUserDataAction.UNFAVORITE) }
  fun setPlayed(id: String, played: Boolean) { updateUserData(id, if (played) VideoUserDataAction.MARK_PLAYED else VideoUserDataAction.MARK_UNPLAYED) }

  /** Runs one Favorite/Played write per item on its own token, independent of the browse query slot. */
  private fun updateUserData(id: String, action: VideoUserDataAction) {
    if (userDataWrites.containsKey(id) || id in batchWriteIds) return
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
        projectAll(::reconciled)
        if (state.value.selectedList == PersonalListKind.Favorites && !result.favorite) {
          mutableState.update { current -> current.copy(listItems = current.listItems.filterNot { it.id == id }) }
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
    projectAll(::marked)
  }

  /** Applies confirmed write results newer than `base` and the live pending flag to a freshly read item. */
  private fun presented(item: MediaUi, base: Long): MediaUi {
    val confirmed = confirmedUserData[item.id]?.takeIf { it.revision > base }
    val overlaid = (confirmed?.let { item.copy(played = it.played, favorite = it.favorite) } ?: item).copy(inWatchlist = item.id in watchlistIds)
    val updating = userDataWrites.containsKey(item.id) || item.id in batchWriteIds
    return if (overlaid.updating == updating) overlaid else overlaid.copy(updating = updating)
  }

  private fun media(item: VideoLibraryItem, scope: ProfileScopeRef) = CatalogPresentation(scope, watchlistIds).library(item)

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
    mutableState.update { it.copy(error = app.localizedString(resource)) }
  }

  override fun onCleared() {
    ++authGeneration
    quickSession?.cancel()
    quickSession?.destroy()
    cancelQuery()
    closeBrowserSession()
    cancelUserDataWrites()
    confirmedUserData.clear()
    closeRetainedBrowsers()
    collectionUndo?.watchlist?.destroy()
    historyUndo?.receipt?.destroy()
    visibility.close()
    app.beforePlaybackHandoff = null
    playback.close()
    super.onCleared()
  }
}
