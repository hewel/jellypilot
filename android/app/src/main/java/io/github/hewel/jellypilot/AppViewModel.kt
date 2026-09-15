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
  private var nextIndex = 0
  private var libraries = emptyList<VideoLibraryShortcut>()

  init {
    viewModelScope.launch {
      refreshIdentity()
      if (sdk.activeProfile() != null) refresh()
    }
  }

  fun setVisible(visible: Boolean) { visibility.setVisible(visible) }
  fun activityFinished() {
    cancelLogin()
    cancelQuery()
    player.stop()
    mutableState.update { it.copy(showPlayer = false, busy = false) }
  }
  fun dismissError() { mutableState.update { it.copy(error = null) } }
  fun addAccount() { mutableState.update { it.copy(showSignIn = true, error = null) } }
  fun openPlayer() { cancelQuery(); mutableState.update { it.copy(showPlayer = true, busy = false) } }
  fun back() {
    if (state.value.showPlayer) {
      player.stop()
      mutableState.update { it.copy(showPlayer = false) }
    } else {
      cancelQuery()
      mutableState.update { it.copy(detail = null, detailItems = emptyList(), busy = false) }
    }
  }

  fun navigate(destination: Destination) {
    if (state.value.destination == destination && state.value.detail == null) return
    cancelQuery()
    mutableState.update { it.copy(destination = destination, detail = null, detailItems = emptyList(), items = emptyList(), busy = false, hasMore = false) }
    if (destination == Destination.Account) viewModelScope.launch { refreshIdentity() } else refresh()
  }

  fun cancelLogin() {
    ++authGeneration
    quickSession?.cancel()
    quickSession?.destroy()
    quickSession = null
    loginJob?.cancel()
    mutableState.update { it.copy(loginBusy = loginJob != null, quickConnectCode = null, showSignIn = false) }
  }

  fun signIn(jellyfin: Boolean, server: String, username: String, password: String, remember: Boolean) {
    accountOperation {
      val candidate = sdk.passwordLogin(if (jellyfin) Provider.JELLYFIN else Provider.EMBY, server, username, password)
      activateCandidate(candidate, remember)
    }
  }

  fun activate(key: String) { accountOperation { activateCandidate(sdk.restoreSavedProfile(key), true) } }
  fun disconnect() { accountOperation { withContext(NonCancellable) { sdk.disconnect(); connectionChanged() } } }
  fun signOut(key: String) {
    accountOperation {
      withContext(NonCancellable) {
        val activeBefore = sdk.activeProfile()?.key
        val outcome = sdk.signOut(key, false)
        if (activeBefore == key) connectionChanged(outcome.remaining) else refreshIdentity(outcome.remaining)
        val warnings = listOfNotNull(
          outcome.teardownError?.let { app.getString(R.string.sdk_signed_out_cleanup_failed) },
          outcome.watchlistError?.let { app.getString(R.string.sdk_watchlist_delete_failed) },
        )
        if (warnings.isNotEmpty()) mutableState.update { it.copy(error = warnings.joinToString("\n")) }
      }
    }
  }

  private suspend fun activateCandidate(candidate: ProfileCandidate, remember: Boolean) = withContext(NonCancellable) {
    var activated = false
    try {
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
        player.setHandoffBlocked(false)
        loginJob = null
        mutableState.update { it.copy(loginBusy = false) }
      }
    }
    loginJob?.start()
  }

  fun quickConnect(server: String, remember: Boolean) {
    if (state.value.loginBusy) return
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
    mutableState.update { it.copy(activeName = active?.userName) }
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
    libraries = emptyList()
    searchText = ""
    nextIndex = 0
    mutableState.update { it.copy(activeName = sdk.activeProfile()?.userName, items = emptyList(), detail = null, detailItems = emptyList(), libraries = emptyList(), libraryId = null, hasMore = false, busy = false, destination = Destination.Home) }
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

  private fun query(debounce: Boolean = false, action: suspend (OperationToken, ProfileScopeRef) -> Unit) {
    cancelQuery()
    if (sdk.activeProfile() == null) return
    val generation = queryGeneration
    mutableState.update { it.copy(busy = true, error = null) }
    queryJob = viewModelScope.launch {
      var token: OperationToken? = null
      try {
        if (debounce) delay(250)
        val operation = sdk.newOperationToken()
        token = operation
        queryToken = operation
        action(operation, operation.scopeRef())
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

  fun refresh() {
    state.value.detail?.let { showDetail(it.id); return }
    when (state.value.destination) {
      Destination.Account -> viewModelScope.launch { refreshIdentity() }
      Destination.Search -> search(searchText)
      Destination.Library -> loadLibrary(false)
      Destination.Home -> query { token, scope ->
        loadLibraries(token)
        val home = sdk.videoHome(token)
        val latest = libraries.firstOrNull()?.let { sdk.libraryLatest(token, it.id) } ?: emptyList()
        currentCoroutineContext().ensureActive()
        mutableState.update { it.copy(items = (home.continueWatching + home.nextUp + latest).distinctBy { item -> item.id }.map { item -> media(item, scope) }, hasMore = false) }
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
    mutableState.update { it.copy(libraryId = id, items = emptyList(), hasMore = false) }
    loadLibrary(false)
  }

  private fun loadLibrary(append: Boolean) = query { token, scope ->
    if (libraries.isEmpty()) loadLibraries(token)
    val library = libraries.firstOrNull { it.id == state.value.libraryId } ?: return@query
    val kind = when (library.collectionType.lowercase()) {
      "movies" -> VideoLibraryKind.MOVIES
      "tvshows" -> VideoLibraryKind.TV_SHOWS
      else -> throw IllegalStateException("Unsupported library kind")
    }
    val page = sdk.browseVideo(token, VideoLibraryPageRequest(library.id, kind, if (append) nextIndex else 0, 48, VideoLibrarySort.RECENTLY_ADDED, VideoLibrarySortDirection.DESCENDING, VideoLibraryPlayedFilter.ALL, false))
    currentCoroutineContext().ensureActive()
    nextIndex = page.startIndex + page.items.size
    val items = page.items.map { media(it, scope) }
    mutableState.update { it.copy(items = (if (append) it.items + items else items).distinctBy { item -> item.id }, hasMore = page.hasMore) }
  }

  fun search(value: String) {
    searchText = value
    searchPage(false)
  }

  private fun searchPage(append: Boolean) = query(debounce = !append) { token, scope ->
    val page = sdk.searchVideo(token, VideoSearchRequest(searchText, if (append) nextIndex else 0, 48))
    currentCoroutineContext().ensureActive()
    nextIndex = page.startIndex + page.items.size
    val items = page.items.map { media(it, scope) }
    mutableState.update { it.copy(items = (if (append) it.items + items else items).distinctBy { item -> item.id }, hasMore = page.hasMore) }
  }

  fun loadMore() {
    if (state.value.busy || !state.value.hasMore) return
    if (state.value.destination == Destination.Search) searchPage(true) else loadLibrary(true)
  }

  fun showDetail(id: String) = query { token, scope ->
    val type = (state.value.items + state.value.detailItems).firstOrNull { it.id == id }?.itemType ?: state.value.detail?.takeIf { it.id == id }?.itemType
    val detail = if (type == "Series") {
      val show = sdk.showDetail(token, id)
      MediaUi(show.id, show.name, "Series", metadata(show.productionYear, "Series"), artwork(show.artworkImageId, scope), show.overview.orEmpty(), show.favorite, show.played)
    } else {
      val item = sdk.itemDetail(token, id)
      MediaUi(item.id, item.name, item.itemType, metadata(item.productionYear, item.itemType), artwork(item.artworkImageId, scope), item.overview.orEmpty(), item.favorite, item.played)
    }
    currentCoroutineContext().ensureActive()
    mutableState.update { it.copy(detail = detail, detailItems = emptyList()) }
    val related = sdk.similarVideo(token, id)
    currentCoroutineContext().ensureActive()
    mutableState.update { it.copy(detailItems = related.map { item -> media(item, scope) }) }
  }

  fun setFavorite(id: String, favorite: Boolean) { updateUserData(id, if (favorite) VideoUserDataAction.FAVORITE else VideoUserDataAction.UNFAVORITE) }
  fun setPlayed(id: String, played: Boolean) { updateUserData(id, if (played) VideoUserDataAction.MARK_PLAYED else VideoUserDataAction.MARK_UNPLAYED) }
  private fun updateUserData(id: String, action: VideoUserDataAction) = query { token, _ ->
    val result = sdk.updateUserData(token, id, action)
    currentCoroutineContext().ensureActive()
    fun reconciled(item: MediaUi) = if (item.id == result.itemId) item.copy(played = result.played, favorite = result.favorite) else item
    mutableState.update { it.copy(items = it.items.map(::reconciled), detail = it.detail?.let(::reconciled), detailItems = it.detailItems.map(::reconciled)) }
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
    visibility.close()
    player.stop()
    super.onCleared()
  }
}
