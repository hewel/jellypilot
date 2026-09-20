package io.github.hewel.jellypilot.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.github.hewel.jellypilot.AppViewModel
import io.github.hewel.jellypilot.R

@Composable
internal fun HistoryScreen(state: AppUiState, model: AppViewModel) {
  val scroll = rememberLazyListState()
  val focusers = remember { mutableMapOf<String, FocusRequester>() }
  var removedId by remember { mutableStateOf<String?>(null) }
  var nextId by remember { mutableStateOf<String?>(null) }
  var restoring by remember { mutableStateOf(false) }
  var focused by remember { mutableStateOf(false) }
  val touchExploration = rememberTouchExploration()
  LaunchedEffect(state.historyUndo?.id, state.historyItems, restoring) {
    if (focused || touchExploration) {
      val target = if (restoring && state.historyUndo == null) removedId else if (state.historyUndo != null) nextId else null
      val index = state.historyItems.indexOfFirst { it.id == target }
      if (index >= 0) {
        scroll.scrollToItem(index)
        withFrameNanos { }
        target?.let { runCatching { focusers[it]?.requestFocus() } }
      }
    }
    if (restoring && state.historyUndo == null) restoring = false
  }
  Column(Modifier.fillMaxSize().onFocusChanged { focused = it.hasFocus }) {
    if (state.historyBusy) LinearProgressIndicator(Modifier.fillMaxWidth())
    LazyColumn(Modifier.weight(1f), state = scroll, contentPadding = PaddingValues(bottom = 24.dp)) {
      if (state.historyItems.isEmpty() && !state.busy) item {
        EmptyState(stringResource(R.string.empty_history), stringResource(R.string.empty_history_hint), stringResource(R.string.browse_library), onAction = { model.navigate(Destination.Library) })
      }
      items(state.historyItems, key = { it.id }) { item ->
        val focus = remember(item.id) { focusers.getOrPut(item.id) { FocusRequester() } }
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
          Box(Modifier.weight(1f)) { EpisodeRow(item, model) }
          IconButton(onClick = {
            removedId = item.id
            val index = state.historyItems.indexOfFirst { it.id == item.id }
            nextId = state.historyItems.getOrNull(index + 1)?.id ?: state.historyItems.getOrNull(index - 1)?.id
            model.removeHistoryItem(item.id)
          }, enabled = !state.historyBusy, modifier = Modifier.focusRequester(focus).padding(end = 8.dp)) { PilotIcon(R.drawable.ic_trash, stringResource(R.string.remove_history)) }
        }
      }
      if (state.historyHasMore) item { TextButton(onClick = model::loadMoreHistory, enabled = !state.historyBusy, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.load_more)) } }
    }
    state.historyUndo?.let { undo ->
      CollectionUndo(undo, stringResource(R.string.history_removed), { restoring = true; model.undoHistoryRemoval() }, { restoring = false; model.dismissHistoryUndo() })
    }
  }
}
