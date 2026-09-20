package io.github.hewel.jellypilot.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.github.hewel.jellypilot.AppViewModel
import io.github.hewel.jellypilot.R
import kotlinx.coroutines.flow.distinctUntilChanged

/**
 * Sparse browse grid over the SDK session window. Cells are absolute display
 * indexes; slots the SDK has not delivered render as fixed-height
 * placeholders, and viewport demand is reported back through
 * [AppViewModel.setBrowserDisplayRange].
 */
@Composable
internal fun BrowserGrid(browser: BrowserUi, model: AppViewModel, gridState: LazyGridState) {
  val count = if (browser.isVirtual) browser.totalCount.coerceAtMost(Int.MAX_VALUE.toUInt()).toInt() else browser.slots.size
  // Report the visible window expanded by one viewport on each side, rounded
  // outward to complete rows; the SDK owns page scheduling from this demand.
  LaunchedEffect(gridState, count) {
    snapshotFlow {
      val visible = gridState.layoutInfo.visibleItemsInfo
      if (visible.isEmpty() || count == 0) return@snapshotFlow null
      val first = visible.minOf { it.index }.toLong()
      if (first >= count) return@snapshotFlow null
      val last = visible.maxOf { it.index }.toLong().coerceAtMost(count.toLong() - 1)
      val rowTop = visible.minOf { it.offset.y }
      val columns = visible.count { it.offset.y == rowTop }.coerceAtLeast(1).toLong()
      val span = visible.size.toLong()
      val start = ((first - span).coerceAtLeast(0) / columns) * columns
      val end = (((last + span + columns) / columns) * columns).coerceAtMost(count.toLong())
      start to end
    }.distinctUntilChanged().collect { range ->
      if (range != null) model.setBrowserDisplayRange(browser.generation, range.first.toUInt(), range.second.toUInt())
    }
  }
  when {
    browser.status == BrowseUiStatus.Failed && !browser.hasContent -> {
      Column(Modifier.fillMaxSize().padding(24.dp), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
        Text(browser.error ?: stringResource(R.string.sdk_request_failed), color = LocalPilotColors.current.body)
        if (browser.retryable) {
          Spacer(Modifier.height(16.dp))
          FilledTonalButton(onClick = model::retryBrowser, enabled = !browser.retryBusy) { Text(stringResource(R.string.retry)) }
        }
      }
    }
    browser.status == BrowseUiStatus.Loading && !browser.hasContent -> {
      Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
    }
    count == 0 -> {
      Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Text(stringResource(R.string.no_results), color = LocalPilotColors.current.metadata) }
    }
    else -> Column(Modifier.fillMaxSize()) {
      if (browser.loadingMore && !browser.refreshing) LinearProgressIndicator(Modifier.fillMaxWidth())
      val retainedError = browser.error ?: browser.refreshError
      // A virtual list's end can be thousands of rows away. Paging failures
      // must remain actionable even when every visible slot is a placeholder.
      if (retainedError != null) {
        Surface(color = MaterialTheme.colorScheme.errorContainer, modifier = Modifier.fillMaxWidth()) {
          Row(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(retainedError, Modifier.weight(1f), color = MaterialTheme.colorScheme.onErrorContainer, style = MaterialTheme.typography.labelSmall)
            if (browser.retryable) {
              TextButton(onClick = model::retryBrowser, enabled = !browser.retryBusy) { Text(stringResource(R.string.retry)) }
            }
          }
        }
      }
      LazyVerticalGrid(
        state = gridState,
        columns = GridCells.Adaptive(if (LocalDensity.current.fontScale > 1.3f) 144.dp else 109.dp), modifier = Modifier.fillMaxWidth().weight(1f),
        contentPadding = PaddingValues(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(16.dp),
      ) {
        items(count, key = { it }) { index ->
          val slot = index.toUInt().let { absolute ->
            if (absolute >= browser.visibleStart && absolute < browser.loadedEnd) {
              browser.slots.getOrNull((absolute - browser.visibleStart).toInt())
            } else null
          }
          if (slot != null) Poster(slot) { model.showDetail(slot.id) } else PosterPlaceholder()
        }
      }
    }
  }
}

