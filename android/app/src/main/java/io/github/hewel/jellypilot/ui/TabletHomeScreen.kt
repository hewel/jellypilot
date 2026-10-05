@file:OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class, androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package io.github.hewel.jellypilot.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.hewel.jellypilot.AppViewModel
import io.github.hewel.jellypilot.R

@Composable
internal fun TabletHomeScreen(state: AppUiState, model: AppViewModel, scroll: LazyListState) {
  BoxWithConstraints(Modifier.fillMaxSize()) {
    val layout = TabletShelfLayout.forWidth(maxWidth.value, LocalDensity.current.fontScale)
    val featured = state.featured.ifEmpty { state.items.take(5) }
    val rows = state.homeRows.ifEmpty {
      if (state.items.isEmpty()) emptyList() else listOf(HomeRowUi("latest", stringResource(R.string.latest_media), state.items))
    }
    val shelfWidth = maxWidth - 48.dp
    LazyColumn(state = scroll, contentPadding = PaddingValues(vertical = 24.dp), verticalArrangement = Arrangement.spacedBy(28.dp)) {
      item(key = "featured") {
        if (featured.isNotEmpty()) {
          val pager = rememberPagerState(pageCount = { featured.size })
          HorizontalPager(pager, contentPadding = PaddingValues(horizontal = 24.dp), pageSpacing = 24.dp,
            key = { "${featured[it].id}:${featured[it].playTargetId}" }) { page ->
            TabletFeaturedHero(featured[page], pager.currentPage, featured.size,
              search = { model.navigate(Destination.Search) }, open = { model.showDetail(featured[page].actionItemId) },
              play = { model.playItem(featured[page].playTargetId ?: featured[page].id, featured[page].played) },
              watchlist = { model.setWatchlist(featured[page].actionItemId, !featured[page].inWatchlist) })
          }
        } else if (state.busy) Box(Modifier.fillMaxWidth().height(400.dp), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        else EmptyState(stringResource(R.string.no_results), action = stringResource(R.string.refresh), onAction = model::refresh)
      }
      state.recovery?.let { recovery -> item(key = "recovery") {
        Surface(Modifier.padding(horizontal = 24.dp), color = MaterialTheme.colorScheme.surfaceContainer, shape = MaterialTheme.shapes.medium) {
          Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
              Column(Modifier.weight(1f)) {
                Text(stringResource(R.string.restore_playback), style = MaterialTheme.typography.titleMedium)
                Text(stringResource(R.string.restore_hint, recovery.title, playbackClock(recovery.positionSeconds)), style = MaterialTheme.typography.bodyMedium)
              }
              IconButton(onClick = model::clearLocalRecovery) { PilotIcon(R.drawable.ic_x, stringResource(R.string.discard_recovery)) }
            }
            Text(stringResource(R.string.restore_explanation), style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.insetMetadata)
            FilledTonalButton(onClick = model::restoreLocalPlayback) { Text(stringResource(R.string.restore_playback)) }
          }
        }
      } }
      rows.forEach { row -> item(key = "row-${row.id}") {
        val columns = if (row.landscape) layout.episodeColumns else layout.posterColumns
        val cardWidth = (shelfWidth - 16.dp * (columns - 1)) / columns
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
          Row(Modifier.fillMaxWidth().padding(horizontal = 24.dp).heightIn(min = 48.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(row.title, Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
            row.libraryId?.let { id -> TextButton(onClick = { model.openHomeLibrary(id) }) { Text(stringResource(R.string.view_all)) } }
          }
          LazyRow(contentPadding = PaddingValues(horizontal = 24.dp), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            items(row.items, key = { it.id }) { item ->
              if (row.landscape) TabletEpisodeCard(item, cardWidth, { model.showDetail(item.id) }, { model.playItem(item.playTargetId ?: item.id, item.played) })
              else Poster(item, Modifier.width(cardWidth)) { model.showDetail(item.id) }
            }
          }
        }
      } }
      if (state.libraries.isNotEmpty()) item(key = "libraries") {
        Column(Modifier.padding(horizontal = 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
          Text(stringResource(R.string.library), style = MaterialTheme.typography.titleLarge)
          FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            state.libraries.forEach { library -> FilledTonalButton(onClick = { model.openHomeLibrary(library.id) }) {
              PilotIcon(R.drawable.ic_folder)
              Spacer(Modifier.width(8.dp))
              Text(library.title)
            } }
          }
        }
      }
    }
  }
}

@Composable
internal fun TabletFeaturedHero(item: MediaUi, page: Int, pages: Int, search: () -> Unit, open: () -> Unit, play: () -> Unit, watchlist: () -> Unit) {
  Box(Modifier.fillMaxWidth().heightIn(min = 400.dp).clip(MaterialTheme.shapes.large).testTag("tablet-featured")) {
    Artwork(item.backdrop ?: item.artwork, null, Modifier.matchParentSize().clickable(onClick = open), rounded = false)
    Box(Modifier.matchParentSize().background(Brush.verticalGradient(listOf(Color.Transparent, LocalPilotColors.current.playerScrim))))
    Column(Modifier.align(Alignment.BottomStart).padding(horizontal = 24.dp, vertical = 20.dp).widthIn(max = 480.dp).padding(top = 112.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
      if (item.logo != null) MediaLogo(item.logo, item.title, width = 180.dp, height = 58.dp, maxTitleLines = 2)
      else Text(item.title, color = Color.White, style = MaterialTheme.typography.headlineLarge, maxLines = if (LocalDensity.current.fontScale > 1.3f) Int.MAX_VALUE else 2, overflow = TextOverflow.Ellipsis)
      Text(mediaCaption(item), color = Color.White, style = MaterialTheme.typography.bodyMedium)
      FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        PlayAction(item, Modifier.widthIn(max = 280.dp).fillMaxWidth(), progressEdge = true, play = play)
        FilledTonalIconButton(onClick = watchlist, enabled = !item.updating, colors = IconButtonDefaults.filledTonalIconButtonColors(containerColor = LocalPilotColors.current.playerScrim, contentColor = Color.White)) {
          PilotIcon(if (item.inWatchlist) R.drawable.ic_bookmark_filled else R.drawable.ic_bookmark, stringResource(if (item.inWatchlist) R.string.remove_watchlist else R.string.add_watchlist))
        }
      }
      if (pages > 1) Row(Modifier.padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        repeat(pages) { index -> Box(Modifier.width(if (index == page) 16.dp else 6.dp).height(6.dp).clip(CircleShape).background(Color.White.copy(alpha = if (index == page) 1f else 0.35f))) }
      }
    }
    if (item.quality.isNotEmpty()) Surface(Modifier.align(Alignment.TopStart).padding(20.dp).padding(end = 56.dp), color = LocalPilotColors.current.playerScrim, shape = CircleShape) {
      Text(item.quality.joinToString(" · "), Modifier.padding(horizontal = 12.dp, vertical = 6.dp), color = Color.White, style = MaterialTheme.typography.labelSmall)
    }
    FilledIconButton(onClick = search, modifier = Modifier.align(Alignment.TopEnd).padding(16.dp), colors = IconButtonDefaults.filledIconButtonColors(containerColor = LocalPilotColors.current.playerScrim, contentColor = Color.White)) {
      PilotIcon(R.drawable.ic_search, stringResource(R.string.search))
    }
  }
}

@Composable
private fun TabletEpisodeCard(item: MediaUi, width: Dp, open: () -> Unit, play: () -> Unit) {
  Column(Modifier.width(width), verticalArrangement = Arrangement.spacedBy(6.dp)) {
    Box(Modifier.clip(MaterialTheme.shapes.medium).clickable(onClick = open)) {
      Artwork(item.backdrop ?: item.artwork, item.title, Modifier.fillMaxWidth().aspectRatio(16f / 9f))
      FilledIconButton(onClick = play, modifier = Modifier.align(Alignment.Center), colors = IconButtonDefaults.filledIconButtonColors(containerColor = LocalPilotColors.current.playerScrim, contentColor = Color.White)) {
        PilotIcon(R.drawable.ic_player_play, playbackLabel(item))
      }
      if (item.progress > 0f) LinearProgressIndicator(progress = { item.progress }, modifier = Modifier.fillMaxWidth().height(4.dp).align(Alignment.BottomCenter), drawStopIndicator = {})
    }
    Text(item.title, maxLines = if (LocalDensity.current.fontScale > 1.3f) Int.MAX_VALUE else 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.labelLarge)
    Text(mediaCaption(item), style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.metadata)
  }
}
