@file:OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package io.github.hewel.jellypilot.ui

import android.content.Context
import android.view.accessibility.AccessibilityManager
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.hewel.jellypilot.AppViewModel
import io.github.hewel.jellypilot.R

@Composable
internal fun PilotApp(model: AppViewModel, playerContent: @Composable () -> Unit) {
  val state by model.state.collectAsStateWithLifecycle()
  val detailNavigationEpoch = model.detailNavigationEpoch
  val routeState = rememberSaveableStateHolder()
  val scopeKey = state.activeProfileKey ?: state.profiles.firstOrNull { it.active }?.key ?: state.activeName.orEmpty()
  var sourceDestination by rememberSaveable(scopeKey) { mutableStateOf(Destination.Home) }
  SideEffect { if (state.destination != Destination.Search) sourceDestination = state.destination }
  val homeScroll = key(scopeKey) { rememberLazyListState() }
  // Source library and pushed search never share a scroll anchor.
  val libraryGrid = rememberLazyGridState()
  val searchGrid = rememberLazyGridState()
  var libraryGeneration by rememberSaveable { mutableLongStateOf(-1L) }
  var searchGeneration by rememberSaveable { mutableLongStateOf(-1L) }
  LaunchedEffect(state.destination, state.browser.generation) {
    if (state.detail == null && !state.showPlayer) {
      if (state.destination == Destination.Library && state.browser.generation != libraryGeneration) {
        if (libraryGeneration != -1L) libraryGrid.scrollToItem(0)
        libraryGeneration = state.browser.generation
      } else if (state.destination == Destination.Search && state.browser.generation != searchGeneration) {
        if (searchGeneration != -1L) searchGrid.scrollToItem(0)
        searchGeneration = state.browser.generation
      }
    }
  }
  val imeVisible = WindowInsets.isImeVisible
  BackHandler(state.remoteController != null || state.showSignIn || (state.detail != null && !imeVisible) || state.showPlayer || (state.detail == null && state.destination == Destination.Search) ||
    (state.detail == null && state.destination == Destination.Account && state.accountPage != AccountPage.Overview)) {
    if (state.showSignIn) model.loginBack() else model.back()
  }
  val dark = when (state.preferences.theme) {
    ThemePreference.System -> isSystemInDarkTheme()
    ThemePreference.Dark -> true
    ThemePreference.Light -> false
  }
  @Composable fun BrowsePage(page: AppUiState) {
    BoxWithConstraints(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal))) {
      val rail = maxWidth >= 600.dp
      val pushed = page.detail != null || page.destination == Destination.Search ||
        (page.destination == Destination.Account && page.accountPage != AccountPage.Overview)
      val immersive = !rail && (page.detail != null || (page.destination == Destination.Home && page.activeName != null))
      Row(Modifier.fillMaxSize()) {
        if (rail) Navigation(if (page.destination == Destination.Search) sourceDestination else page.destination, true, model::navigate)
        Column(Modifier.weight(1f)) {
          if (!immersive) Spacer(Modifier.windowInsetsTopHeight(WindowInsets.statusBars))
          page.error?.takeIf { !page.showSignIn }?.let { error ->
            Surface(color = MaterialTheme.colorScheme.errorContainer) {
              Row(Modifier.fillMaxWidth().padding(start = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(error, Modifier.weight(1f), color = MaterialTheme.colorScheme.onErrorContainer, style = MaterialTheme.typography.bodyMedium)
                IconButton(onClick = model::dismissError) { PilotIcon(R.drawable.ic_x, stringResource(R.string.close)) }
              }
            }
          }
          Box(Modifier.weight(1f)) {
            val routeKey = page.detail?.let { "detail:${it.id}" }
              ?: if (page.destination == Destination.Account) "${page.destination}:${page.accountPage}" else page.destination.name
            routeState.SaveableStateProvider("$scopeKey:$routeKey") {
              when {
                page.detail != null -> DetailScreen(page, model, tablet = rail)
                page.destination == Destination.Account -> AccountScreen(page, model)
                page.activeName == null -> EmptyState(stringResource(R.string.offline_home), stringResource(R.string.not_connected), stringResource(R.string.sign_in), Modifier.align(Alignment.Center), R.drawable.ic_server, model::addAccount)
                page.destination == Destination.Home -> HomeScreen(page, model, homeScroll, tablet = rail)
                page.destination == Destination.Lists -> ListsScreen(page, model)
                page.destination == Destination.Library -> LibraryScreen(page, model, libraryGrid)
                page.destination == Destination.Search -> SearchScreen(page, model, searchGrid)
              }
            }
            page.notice?.let { notice ->
              Snackbar(Modifier.align(Alignment.BottomCenter).padding(16.dp), dismissAction = { IconButton(onClick = model::dismissNotice) { PilotIcon(R.drawable.ic_x, stringResource(R.string.close)) } }) { Text(notice) }
            }
            val browseVisible = page.detail == null && (page.destination == Destination.Library || page.destination == Destination.Search)
            if (page.busy || (browseVisible && page.browser.refreshing)) LinearProgressIndicator(Modifier.fillMaxWidth().align(Alignment.TopCenter))
          }
          if (!rail && !pushed) Navigation(if (page.destination == Destination.Search) sourceDestination else page.destination, false, model::navigate)
          else Spacer(Modifier.windowInsetsBottomHeight(WindowInsets.navigationBars))
        }
      }
    }
  }
  PilotTheme(dark = dark) {
    Surface(color = MaterialTheme.colorScheme.background, modifier = Modifier.fillMaxSize()) {
      if (state.showSignIn) ServerConnectionScreen(
        state, model::loginBack, model::changeLoginServer, model::changeLoginUsername,
        model::changeLoginPassword, model::changeLoginRemember, model::changeLoginProvider,
        model::connectLoginServer, model::submitLogin, model::submitQuickConnect, model::continueLoginManually,
      )
      else if (state.remoteController != null) RemoteControlScreen(
        state.remoteController!!, model::back, model::refreshRemoteController, model::selectRemoteTarget,
        model::sendRemoteCommand, model::playRemoteItem,
      )
      else if (state.showPlayer) playerContent()
      else DetailPredictiveBack(
        current = state,
        pageKey = { page -> "$scopeKey:${page.detail?.let { "detail:${it.id}" } ?: "${page.destination}:${page.accountPage}"}" },
        route = "$scopeKey:${state.destination}:${state.detail?.id}:$detailNavigationEpoch",
        enabled = state.detail != null && !imeVisible,
        reducedMotion = state.preferences.reducedMotion,
        previous = model::detailBackPreview,
        canCommit = {
          val current = model.state.value
          model.detailNavigationEpoch == detailNavigationEpoch && current.detail != null &&
            !current.showPlayer && !current.showSignIn && current.remoteController == null
        },
        onBack = model::back,
      ) { page -> BrowsePage(page) }
    }
  }
}

@Composable
private fun Navigation(selected: Destination, rail: Boolean, navigate: (Destination) -> Unit) {
  val entries = listOf(Destination.Home, Destination.Lists, Destination.Library, Destination.Account)
  @Composable fun entry(destination: Destination, modifier: Modifier) {
    val active = selected == destination
    Column(
      modifier.selectable(active, role = Role.Tab, onClick = { navigate(destination) }).padding(vertical = 8.dp),
      horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp, Alignment.CenterVertically),
    ) {
      val tint = if (active) MaterialTheme.colorScheme.secondary else LocalPilotColors.current.insetMetadata
      Box(Modifier.width(if (rail) 56.dp else 48.dp).height(if (rail) 32.dp else 28.dp).clip(if (rail) androidx.compose.foundation.shape.CircleShape else MaterialTheme.shapes.small).background(if (active) MaterialTheme.colorScheme.secondaryContainer else androidx.compose.ui.graphics.Color.Transparent), contentAlignment = Alignment.Center) {
        PilotIcon(destination.icon, tint = tint)
      }
      Text(stringResource(destination.title), color = tint, style = MaterialTheme.typography.labelSmall)
    }
  }
  Surface(color = LocalPilotColors.current.sidebar) {
    if (rail) Column(Modifier.width(80.dp).fillMaxHeight().statusBarsPadding().navigationBarsPadding().padding(top = 16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
      entries.forEach { entry(it, Modifier.fillMaxWidth().heightIn(min = 64.dp)) }
    } else Row(Modifier.fillMaxWidth().navigationBarsPadding()) {
      entries.forEach { entry(it, Modifier.weight(1f).heightIn(min = 64.dp)) }
    }
  }
}

/** Time-limited controls remain available while a screen reader is exploring them. */
@Composable
internal fun rememberTouchExploration(): Boolean {
  val context = LocalContext.current
  val manager = remember(context) { context.getSystemService(Context.ACCESSIBILITY_SERVICE) as AccessibilityManager }
  var enabled by remember(manager) { mutableStateOf(manager.isTouchExplorationEnabled) }
  DisposableEffect(manager) {
    val listener = AccessibilityManager.TouchExplorationStateChangeListener { enabled = it }
    manager.addTouchExplorationStateChangeListener(listener)
    onDispose { manager.removeTouchExplorationStateChangeListener(listener) }
  }
  return enabled
}
