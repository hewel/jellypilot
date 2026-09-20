@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class, androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package io.github.hewel.jellypilot.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import io.github.hewel.jellypilot.AppViewModel
import io.github.hewel.jellypilot.R
import kotlinx.coroutines.delay

@Composable
internal fun LibraryScreen(state: AppUiState, model: AppViewModel, grid: LazyGridState) {
  var sortOpen by remember { mutableStateOf(false) }
  Column(Modifier.fillMaxSize()) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
      Column(Modifier.weight(1f)) {
        Text(state.libraries.firstOrNull { it.id == state.libraryId }?.title ?: stringResource(R.string.library), style = MaterialTheme.typography.headlineSmall)
        Text(pluralStringResource(if (state.libraryPlayed != PlayedFilter.All || state.libraryFavorites) R.plurals.filtered_count else R.plurals.items_count, state.browser.totalCount.coerceAtMost(Int.MAX_VALUE.toUInt()).toInt(), state.browser.totalCount.coerceAtMost(Int.MAX_VALUE.toUInt()).toInt()), color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall)
      }
      Box {
        TextButton(onClick = { sortOpen = true }) {
          PilotIcon(R.drawable.ic_sort_descending)
          Spacer(Modifier.width(6.dp))
          Text(stringResource(state.librarySort.title))
        }
        DropdownMenu(sortOpen, { sortOpen = false }) {
          LibrarySort.entries.forEach { sort -> DropdownMenuItem(text = { Text(stringResource(sort.title)) }, onClick = { model.setLibrarySort(sort); sortOpen = false }, trailingIcon = { if (sort == state.librarySort) PilotIcon(R.drawable.ic_check) }) }
        }
      }
      IconButton(onClick = { model.navigate(Destination.Search) }) { PilotIcon(R.drawable.ic_search, stringResource(R.string.search)) }
    }
    if (state.libraries.size > 1) LazyRow(contentPadding = PaddingValues(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
      items(state.libraries, key = { it.id }) { library -> FilterChip(state.libraryId == library.id, { model.selectLibrary(library.id) }, label = { Text(library.title) }) }
    }
    val enlarged = LocalDensity.current.fontScale > 1.3f
    FlowRow(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
      Row(Modifier.then(if (enlarged) Modifier.fillMaxWidth() else Modifier).clip(MaterialTheme.shapes.medium).background(MaterialTheme.colorScheme.surfaceContainer)) {
        PlayedFilter.entries.forEach { filter ->
          Box(Modifier.then(if (enlarged) Modifier.weight(1f) else Modifier).heightIn(min = 48.dp).selectable(state.libraryPlayed == filter, role = Role.RadioButton, onClick = { model.setLibraryFilters(filter, state.libraryFavorites) }).padding(4.dp), contentAlignment = Alignment.Center) {
            Box(Modifier.clip(MaterialTheme.shapes.small).background(if (state.libraryPlayed == filter) MaterialTheme.colorScheme.primaryContainer else androidx.compose.ui.graphics.Color.Transparent).padding(horizontal = 12.dp, vertical = 8.dp)) {
              Text(stringResource(filter.title), style = MaterialTheme.typography.labelMedium, color = if (state.libraryPlayed == filter) MaterialTheme.colorScheme.onPrimaryContainer else LocalPilotColors.current.body)
            }
          }
        }
      }
      Row(Modifier.heightIn(min = 48.dp).toggleable(state.libraryFavorites, role = Role.Switch, onValueChange = { model.setLibraryFilters(state.libraryPlayed, it) }), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(stringResource(R.string.only_favorites), style = MaterialTheme.typography.labelMedium)
        Box(Modifier.width(28.dp).height(16.dp).clip(androidx.compose.foundation.shape.CircleShape).background(if (state.libraryFavorites) LocalPilotColors.current.action else MaterialTheme.colorScheme.outline)) {
          Box(Modifier.align(if (state.libraryFavorites) Alignment.CenterEnd else Alignment.CenterStart).padding(2.dp).size(12.dp).clip(androidx.compose.foundation.shape.CircleShape).background(androidx.compose.ui.graphics.Color.White))
        }
      }
    }
    BrowserGrid(state.browser, model, grid)
  }
}

@Composable
internal fun SearchScreen(state: AppUiState, model: AppViewModel, grid: LazyGridState) {
  val searchFocus = remember { FocusRequester() }
  var focusedOnce by rememberSaveable { mutableStateOf(false) }
  LaunchedEffect(Unit) { if (!focusedOnce) { searchFocus.requestFocus(); focusedOnce = true } }
  Column(Modifier.fillMaxSize().imePadding()) {
    Row(Modifier.fillMaxWidth().padding(end = 16.dp), verticalAlignment = Alignment.CenterVertically) {
      IconButton(onClick = model::back) { PilotIcon(R.drawable.ic_chevron_left, stringResource(R.string.back)) }
      OutlinedTextField(
        state.searchQuery, model::search, Modifier.weight(1f).focusRequester(searchFocus), singleLine = true,
        textStyle = MaterialTheme.typography.bodyLarge,
        placeholder = { Text(stringResource(R.string.query_hint)) },
        leadingIcon = { PilotIcon(R.drawable.ic_search) },
        trailingIcon = { if (state.searchQuery.isNotEmpty()) IconButton(onClick = { model.search("") }) { PilotIcon(R.drawable.ic_x, stringResource(R.string.clear_query)) } },
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search), keyboardActions = KeyboardActions(onSearch = { model.search(state.searchQuery) }),
      )
    }
    Text(pluralStringResource(R.plurals.items_count, state.browser.totalCount.coerceAtMost(Int.MAX_VALUE.toUInt()).toInt(), state.browser.totalCount.coerceAtMost(Int.MAX_VALUE.toUInt()).toInt()), Modifier.padding(16.dp), color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall)
    when {
      state.searchQuery.isBlank() -> EmptyState(stringResource(R.string.search_empty), stringResource(R.string.search_empty_hint), icon = R.drawable.ic_search)
      state.browser.status == BrowseUiStatus.Empty -> EmptyState(stringResource(R.string.search_no_results, state.searchQuery), stringResource(R.string.search_no_results_hint), stringResource(R.string.clear_query), icon = R.drawable.ic_search, onAction = { model.search("") })
      else -> BrowserGrid(state.browser, model, grid)
    }
  }
}

internal data class PersonalListActions(
  val selectList: (PersonalListKind) -> Unit,
  val openItem: (String) -> Unit,
  val browseLibrary: () -> Unit,
  val remove: (List<String>) -> Unit,
  val undo: () -> Unit,
  val dismissUndo: () -> Unit,
  val loadMore: () -> Unit,
)

@Composable
internal fun ListsScreen(state: AppUiState, model: AppViewModel) = PersonalLists(state, PersonalListActions(
  model::selectList, model::showDetail, { model.navigate(Destination.Library) }, model::removeListItems,
  model::undoListRemoval, model::dismissListUndo, model::loadMoreList,
))

@Composable
internal fun PersonalLists(state: AppUiState, actions: PersonalListActions) {
  var managing by remember(state.selectedList) { mutableStateOf(false) }
  var selected by remember(state.selectedList) { mutableStateOf(emptyList<String>()) }
  val watchlistGrid = rememberLazyGridState()
  val favoritesGrid = rememberLazyGridState()
  val grid = if (state.selectedList == PersonalListKind.Watchlist) watchlistGrid else favoritesGrid
  val focusers = remember { mutableMapOf<String, FocusRequester>() }
  var nextFocus by remember { mutableStateOf<String?>(null) }
  var removedAnchor by remember { mutableStateOf<String?>(null) }
  var restorePending by remember { mutableStateOf(false) }
  var focusActive by remember { mutableStateOf(false) }
  val accessibilityFocus = rememberTouchExploration()
  val emptyFocus = remember { FocusRequester() }
  BackHandler(managing) { managing = false; selected = emptyList() }
  LaunchedEffect(state.listUndo?.id) {
    if (state.listUndo != null) {
      managing = false
      selected = emptyList()
      if (focusActive || accessibilityFocus) {
        withFrameNanos { }
        if (state.listItems.isEmpty()) runCatching { emptyFocus.requestFocus() }
        else nextFocus?.let { runCatching { focusers[it]?.requestFocus() } }
      }
    }
  }
  LaunchedEffect(state.listUndo?.id, state.listItems, restorePending) {
    if (restorePending && state.listUndo == null) {
      val restoredIndex = state.listItems.indexOfFirst { it.id == removedAnchor }
      if (restoredIndex >= 0 && (focusActive || accessibilityFocus)) {
        grid.scrollToItem(restoredIndex)
        withFrameNanos { }
        runCatching { removedAnchor?.let { focusers[it]?.requestFocus() } }
      }
      restorePending = false
    }
  }
  Column(Modifier.fillMaxSize().onFocusChanged { focusActive = it.hasFocus }) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
      Text(stringResource(if (managing) { if (state.selectedList == PersonalListKind.Watchlist) R.string.manage_watchlist else R.string.manage_favorites } else R.string.personal_lists), Modifier.weight(1f), style = MaterialTheme.typography.headlineSmall)
      TextButton(onClick = { managing = !managing; selected = emptyList() }, enabled = state.listItems.isNotEmpty() && !state.listBusy) { Text(stringResource(if (managing) R.string.done else R.string.manage)) }
    }
    Row(Modifier.padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
      PersonalListKind.entries.forEach { kind ->
        FilterChip(state.selectedList == kind, { managing = false; selected = emptyList(); actions.selectList(kind) }, label = {
          Text("${stringResource(kind.title)}  ${if (kind == PersonalListKind.Watchlist) state.listCount else state.favoriteCount}")
        })
      }
    }
    if (state.listBusy) LinearProgressIndicator(Modifier.fillMaxWidth())
    Box(Modifier.weight(1f)) {
      if (state.listItems.isEmpty() && !state.listBusy) EmptyState(
        stringResource(if (state.selectedList == PersonalListKind.Watchlist) R.string.empty_watchlist else R.string.empty_favorites),
        stringResource(if (state.selectedList == PersonalListKind.Watchlist) R.string.empty_watchlist_hint else R.string.empty_favorites_hint),
        stringResource(R.string.browse_library), modifier = Modifier.align(Alignment.Center).focusRequester(emptyFocus),
        icon = if (state.selectedList == PersonalListKind.Watchlist) R.drawable.ic_bookmark else R.drawable.ic_heart,
        onAction = { actions.browseLibrary() },
      ) else LazyVerticalGrid(
        columns = GridCells.Adaptive(if (LocalDensity.current.fontScale > 1.3f) 144.dp else 109.dp), state = grid,
        contentPadding = PaddingValues(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(16.dp),
      ) {
        items(state.listItems, key = { it.id }) { item ->
          val focus = remember(item.id) { focusers.getOrPut(item.id) { FocusRequester() } }
          Poster(item, Modifier.focusRequester(focus), if (managing) item.id in selected else null, enabled = !managing || !state.listBusy) {
            if (managing) selected = if (item.id in selected) selected - item.id else selected + item.id
            else actions.openItem(item.id)
          }
        }
      }
    }
    if (state.listHasMore) TextButton(onClick = actions.loadMore, enabled = !state.listBusy, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.load_more)) }
    if (managing) Surface(color = MaterialTheme.colorScheme.surfaceContainer) {
      Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(stringResource(R.string.selected_count, selected.size), Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
        Button(onClick = {
          removedAnchor = selected.firstOrNull()
          val first = state.listItems.indexOfFirst { it.id in selected }
          nextFocus = state.listItems.drop(first.coerceAtLeast(0)).firstOrNull { it.id !in selected }?.id
            ?: state.listItems.take(first.coerceAtLeast(0)).lastOrNull { it.id !in selected }?.id
          actions.remove(selected)
        }, enabled = selected.isNotEmpty() && !state.listBusy) { Text(stringResource(if (state.selectedList == PersonalListKind.Watchlist) R.string.remove_selected else R.string.unfavorite_selected)) }
      }
    }
    state.listUndo?.let { undo -> CollectionUndo(undo, pluralStringResource(R.plurals.removed_count, undo.count, undo.count), { restorePending = true; actions.undo() }, { restorePending = false; actions.dismissUndo() }) }
  }
}

@Composable
internal fun CollectionUndo(undo: ListUndoUi, message: String, onUndo: () -> Unit, dismiss: () -> Unit) {
  var focused by remember { mutableStateOf(false) }
  val touchExploration = rememberTouchExploration()
  var remaining by remember(undo.id) { mutableLongStateOf(8_000L) }
  LaunchedEffect(undo.id, focused, touchExploration, undo.busy, undo.error) {
    if (!focused && !touchExploration && !undo.busy && undo.error == null) {
      while (remaining > 0) { delay(250); remaining -= 250 }
      dismiss()
    }
  }
  Snackbar(
    modifier = Modifier.padding(12.dp).onFocusChanged { focused = it.hasFocus }.semantics { liveRegion = LiveRegionMode.Polite },
    action = { TextButton(onClick = onUndo, enabled = !undo.busy) { Text(stringResource(R.string.undo)) } },
    dismissAction = { IconButton(onClick = dismiss) { PilotIcon(R.drawable.ic_x, stringResource(R.string.close)) } },
  ) { Text(undo.error ?: message) }
}
