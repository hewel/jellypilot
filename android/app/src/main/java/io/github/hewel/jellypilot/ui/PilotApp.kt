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
  val routeState = rememberSaveableStateHolder()
  val scopeKey = state.activeProfileKey ?: state.profiles.firstOrNull { it.active }?.key ?: state.activeName.orEmpty()
  var sourceDestination by rememberSaveable(scopeKey) { mutableStateOf(Destination.Home) }
  SideEffect { if (state.destination != Destination.Search) sourceDestination = state.destination }
  val navigationDestination = if (state.destination == Destination.Search) sourceDestination else state.destination
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
  BackHandler(state.detail != null || state.showPlayer || state.destination == Destination.Search || state.accountPage != AccountPage.Overview) { model.back() }
  val dark = when (state.preferences.theme) {
    ThemePreference.System -> isSystemInDarkTheme()
    ThemePreference.Dark -> true
    ThemePreference.Light -> false
  }
  PilotTheme(dark = dark) {
    Surface(color = MaterialTheme.colorScheme.background, modifier = Modifier.fillMaxSize()) {
      if (state.showPlayer) playerContent()
      else BoxWithConstraints(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal))) {
        val rail = maxWidth >= 600.dp
        val pushed = state.detail != null || state.destination == Destination.Search || state.accountPage != AccountPage.Overview
        val immersive = state.detail != null || (state.destination == Destination.Home && state.activeName != null)
        Row(Modifier.fillMaxSize()) {
          if (rail) Navigation(navigationDestination, true, model::navigate)
          Column(Modifier.weight(1f)) {
            if (!immersive) Spacer(Modifier.windowInsetsTopHeight(WindowInsets.statusBars))
            state.error?.takeIf { !state.showSignIn }?.let { error ->
              Surface(color = MaterialTheme.colorScheme.errorContainer) {
                Row(Modifier.fillMaxWidth().padding(start = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                  Text(error, Modifier.weight(1f), color = MaterialTheme.colorScheme.onErrorContainer, style = MaterialTheme.typography.bodyMedium)
                  IconButton(onClick = model::dismissError) { PilotIcon(R.drawable.ic_x, stringResource(R.string.close)) }
                }
              }
            }
            Box(Modifier.weight(1f)) {
              val routeKey = state.detail?.let { "detail:${it.id}" } ?: "${state.destination}:${state.accountPage}"
              routeState.SaveableStateProvider("$scopeKey:$routeKey") {
              when {
                state.detail != null -> DetailScreen(state, model)
                state.destination == Destination.Account -> AccountScreen(state, model)
                state.activeName == null -> EmptyState(stringResource(R.string.offline_home), stringResource(R.string.not_connected), stringResource(R.string.sign_in), Modifier.align(Alignment.Center), R.drawable.ic_server, model::addAccount)
                state.destination == Destination.Home -> HomeScreen(state, model, homeScroll)
                state.destination == Destination.Lists -> ListsScreen(state, model)
                state.destination == Destination.Library -> LibraryScreen(state, model, libraryGrid)
                state.destination == Destination.Search -> SearchScreen(state, model, searchGrid)
              }
              }
              state.notice?.let { notice ->
                Snackbar(Modifier.align(Alignment.BottomCenter).padding(16.dp), dismissAction = { IconButton(onClick = model::dismissNotice) { PilotIcon(R.drawable.ic_x, stringResource(R.string.close)) } }) { Text(notice) }
              }
              val browseVisible = state.detail == null && (state.destination == Destination.Library || state.destination == Destination.Search)
              if (state.busy || (browseVisible && state.browser.refreshing)) LinearProgressIndicator(Modifier.fillMaxWidth().align(Alignment.TopCenter))
            }
            if (!rail && !pushed) Navigation(navigationDestination, false, model::navigate)
            else Spacer(Modifier.windowInsetsBottomHeight(WindowInsets.navigationBars))
          }
        }
      }
      if (state.showSignIn) SignInSheet(state, model::cancelLogin, model::signIn, model::quickConnect)
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
      Box(Modifier.width(48.dp).height(28.dp).clip(MaterialTheme.shapes.small).background(if (active) MaterialTheme.colorScheme.secondaryContainer else androidx.compose.ui.graphics.Color.Transparent), contentAlignment = Alignment.Center) {
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
