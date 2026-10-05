@file:OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package io.github.hewel.jellypilot.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.hewel.jellypilot.AppViewModel
import io.github.hewel.jellypilot.R
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive

internal data class TabletDetailActions(
  val back: () -> Unit,
  val selectSeason: (String) -> Unit,
  val loadMore: () -> Unit,
  val preview: suspend (String) -> MediaUi?,
  val play: (String, Boolean) -> Unit,
  val favorite: (String, Boolean) -> Unit,
  val watchlist: (String, Boolean) -> Unit,
  val watched: (String, Boolean) -> Unit,
  val loadTracks: (String) -> Unit,
  val audio: (Int?) -> Unit,
  val subtitle: (Int?) -> Unit,
  val detail: (String) -> Unit,
  val remotePlay: (MediaUi) -> Unit = {},
)

@Composable
internal fun DetailScreen(state: AppUiState, model: AppViewModel, tablet: Boolean) {
  val actions = remember(model) {
    TabletDetailActions(model::back, model::selectSeason, model::loadMoreEpisodes, model::loadDetailEpisodePreview,
      model::playItem, model::setFavorite, model::setWatchlist, model::setPlayed,
      { model.loadDetailTracks(it) }, model::selectDetailAudio, model::selectDetailSubtitle, model::showDetail, model::playOnAnotherDevice)
  }
  AdaptiveDetailContent(state, tablet, actions) { phoneState, selectionResolved -> PhoneDetailScreen(phoneState, model, selectionResolved) }
}

/** Selection and all three scroll anchors outlive changes between one, two, and compact panes. */
@Composable
internal fun AdaptiveDetailContent(state: AppUiState, tablet: Boolean, actions: TabletDetailActions, phone: @Composable (AppUiState, Boolean) -> Unit) {
  val backPreview = LocalBackPreview.current
  val detail = state.detail ?: return
  val initialSeason by rememberSaveable(detail.id) { mutableStateOf(state.selectedSeasonId) }
  var adaptiveSelection by rememberSaveable(detail.id) { mutableStateOf(tablet) }
  var selectedId by rememberSaveable(detail.id, state.selectedSeasonId) { mutableStateOf<String?>(null) }
  val portraitScroll = rememberLazyListState()
  val previewScroll = rememberLazyListState()
  val episodeScroll = rememberLazyListState()
  var previousTwoPanes by remember { mutableStateOf<Boolean?>(null) }
  val initialTarget = if (initialSeason == state.selectedSeasonId) detail.playTargetId?.takeUnless { it == detail.id }
    ?: state.detailItems.firstOrNull()?.id ?: detail.playTargetId else state.detailItems.firstOrNull()?.id
  val targetId = selectedId ?: initialTarget
  val candidate = state.detailItems.firstOrNull { it.id == targetId }
  var preview by remember(detail.id, targetId) { mutableStateOf<MediaUi?>(null) }
  var previewBusy by remember(detail.id, targetId) { mutableStateOf(false) }
  var previewFailed by remember(detail.id, targetId) { mutableStateOf(false) }
  var retry by remember(detail.id, targetId) { mutableIntStateOf(0) }
  var audioSheet by rememberSaveable(detail.id, targetId) { mutableStateOf<Boolean?>(null) }
  LaunchedEffect(tablet, detail.id, state.selectedSeasonId, initialTarget, backPreview) {
    if (backPreview) return@LaunchedEffect
    if (tablet) {
      adaptiveSelection = true
      if (selectedId == null) selectedId = targetId
    }
  }
  LaunchedEffect(detail.id, targetId, selectedId, retry, backPreview) {
    if (backPreview) return@LaunchedEffect
    if (selectedId != null && targetId != null && targetId != detail.id) {
      previewBusy = true
      previewFailed = false
      try {
        val loaded = actions.preview(targetId)
        currentCoroutineContext().ensureActive()
        preview = loaded
        previewFailed = preview == null
      } catch (cancelled: CancellationException) { throw cancelled }
      catch (_: Exception) { previewFailed = true }
      finally { previewBusy = false }
    }
  }
  // The loaded metadata must not roll back newer favorite/watched/list mutations from the page.
  val retainedPreview = state.detailPreview?.takeIf { it.ownerId == detail.id && it.seasonId == state.selectedSeasonId && it.item.id == targetId }?.item
  val currentFlags = state.detailItems.firstOrNull { it.id == targetId } ?: detail.takeIf { it.id == targetId } ?: retainedPreview
  val selected = ((retainedPreview ?: preview)?.let { loaded -> currentFlags?.let { loaded.copy(favorite = it.favorite, played = it.played, inWatchlist = it.inWatchlist, updating = it.updating) } ?: loaded }
    ?: candidate ?: detail).let {
      it.copy(playable = it.playable && targetId != null && it.id == targetId && !previewBusy && !previewFailed)
    }
  val tracks = state.detailTracks?.takeIf { it.targetId == targetId }
  val selectionResolved = targetId != null && selected.id == targetId
  if (!tablet) {
    val originalPhone = !adaptiveSelection && selectedId == null
    phone(if (originalPhone) state else state.copy(detail = selected.copy(seasons = detail.seasons, related = detail.related), detailItems = state.detailItems), originalPhone || selectionResolved)
    audioSheet?.takeUnless { backPreview }?.let { audio ->
      DetailTrackSheet(audio, tracks, { audioSheet = null }, { targetId?.let(actions.loadTracks) }, if (audio) actions.audio else actions.subtitle)
    }
    return
  }
  val openTracks: (Boolean) -> Unit = { audio ->
    audioSheet = audio
    targetId?.let(actions.loadTracks)
  }
  BoxWithConstraints(Modifier.fillMaxSize()) {
    val twoPanes = detail.itemType == "Series" && tabletDetailHasTwoPanes(maxWidth.value, LocalDensity.current.fontScale)
    LaunchedEffect(twoPanes, backPreview) {
      if (backPreview) return@LaunchedEffect
      if (previousTwoPanes != null && previousTwoPanes != twoPanes) {
        val index = state.detailItems.indexOfFirst { it.id == targetId }
        val scroll = if (twoPanes) episodeScroll else portraitScroll
        if (index >= 0 && scroll.layoutInfo.visibleItemsInfo.none { it.key == "episode-$targetId" }) {
          scroll.scrollToItem(index + if (twoPanes) 1 else 2)
        }
      }
      previousTwoPanes = twoPanes
    }
    Column(Modifier.fillMaxSize().padding(horizontal = 24.dp).testTag(if (twoPanes) "tablet-detail-two-panes" else "tablet-detail-one-pane")) {
      Row(Modifier.fillMaxWidth().heightIn(min = 64.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        IconButton(onClick = actions.back) { PilotIcon(R.drawable.ic_chevron_left, stringResource(R.string.back)) }
        Text(detail.title, Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
        IconButton(onClick = { actions.favorite(detail.actionItemId, !detail.favorite) }, enabled = !detail.updating) {
          PilotIcon(if (detail.favorite) R.drawable.ic_heart_filled else R.drawable.ic_heart, stringResource(if (detail.favorite) R.string.unfavorite else R.string.favorite), tint = if (detail.favorite) LocalPilotColors.current.favorite else LocalContentColor.current)
        }
      }
      val previewContent: @Composable () -> Unit = {
        TabletEpisodePreview(selected, selectionResolved, tracks, previewBusy, previewFailed, { retry++ }, openTracks, actions)
      }
      if (twoPanes) Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(24.dp)) {
        LazyColumn(state = episodeScroll, modifier = Modifier.weight(0.42f).testTag("tablet-episodes-scroll"), contentPadding = PaddingValues(bottom = 32.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
          tabletEpisodes(state, targetId, { selectedId = it }, actions)
        }
        LazyColumn(state = previewScroll, modifier = Modifier.weight(0.58f).testTag("tablet-preview-scroll"), contentPadding = PaddingValues(bottom = 32.dp)) {
          item(key = "preview") { previewContent() }
        }
      } else LazyColumn(state = portraitScroll, modifier = Modifier.weight(1f).testTag("tablet-detail-scroll"), contentPadding = PaddingValues(bottom = 32.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        item(key = "preview") { previewContent() }
        tabletEpisodes(state, targetId, { selectedId = it }, actions)
        if (detail.related.isNotEmpty()) item(key = "related") {
          Column(Modifier.padding(top = 20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(stringResource(R.string.related), style = MaterialTheme.typography.titleLarge)
            androidx.compose.foundation.lazy.LazyRow(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
              items(detail.related, key = { it.id }) { item -> Poster(item, Modifier.width(132.dp)) { actions.detail(item.id) } }
            }
          }
        }
      }
    }
  }
  audioSheet?.takeUnless { backPreview }?.let { audio ->
    DetailTrackSheet(audio, tracks, { audioSheet = null }, { targetId?.let(actions.loadTracks) }, if (audio) actions.audio else actions.subtitle)
  }
}

private fun LazyListScope.tabletEpisodes(state: AppUiState, selectedId: String?, select: (String) -> Unit, actions: TabletDetailActions) {
  if (state.detail?.itemType != "Series") return
  item(key = "episodes-header") {
    Row(Modifier.fillMaxWidth().padding(top = 20.dp).heightIn(min = 48.dp), verticalAlignment = Alignment.CenterVertically) {
      Text(stringResource(R.string.episodes), Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
      if (state.detail.seasons.isNotEmpty()) Box {
        var expanded by remember { mutableStateOf(false) }
        OutlinedButton(onClick = { expanded = true }) {
          Text(state.detail.seasons.firstOrNull { it.id == state.selectedSeasonId }?.title.orEmpty())
          Spacer(Modifier.width(8.dp))
          PilotIcon(R.drawable.ic_chevron_down)
        }
        DropdownMenu(expanded, { expanded = false }) {
          state.detail.seasons.forEach { season -> DropdownMenuItem(text = { Text(season.title) }, onClick = { expanded = false; actions.selectSeason(season.id) }) }
        }
      }
    }
  }
  if (state.detailItems.isEmpty()) item(key = "episodes-empty") {
    if (state.busy) Box(Modifier.fillMaxWidth().padding(24.dp), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
    else Text(stringResource(R.string.season_empty), Modifier.padding(vertical = 16.dp), color = LocalPilotColors.current.metadata)
  }
  items(state.detailItems, key = { "episode-${it.id}" }) { item ->
    TabletEpisodeRow(item, selectedId == item.id, { select(item.id) }, { actions.play(item.id, item.played) })
  }
  if (state.episodesHaveMore) item(key = "episodes-more") {
    TextButton(onClick = actions.loadMore, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.load_more)) }
  }
}

@Composable
private fun TabletEpisodePreview(item: MediaUi, selectionResolved: Boolean, tracks: DetailTracksUi?, busy: Boolean, failed: Boolean, retry: () -> Unit, openTracks: (Boolean) -> Unit, actions: TabletDetailActions) {
  Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
    Box(Modifier.fillMaxWidth().aspectRatio(16f / 9f).clip(MaterialTheme.shapes.medium).testTag("tablet-static-preview")) {
      Artwork(item.backdrop ?: item.artwork, null, Modifier.matchParentSize(), rounded = false)
      if (item.progress > 0f) {
        Box(Modifier.matchParentSize().background(Brush.verticalGradient(listOf(Color.Transparent, LocalPilotColors.current.playerScrim))))
        Text(mediaCaption(item), Modifier.align(Alignment.BottomStart).padding(16.dp), color = Color.White, style = MaterialTheme.typography.bodySmall)
      }
      if (busy) CircularProgressIndicator(Modifier.align(Alignment.Center), color = Color.White)
    }
    Text(listOfNotNull(item.episodeCode, item.title).joinToString(" · "), style = MaterialTheme.typography.headlineMedium)
    Text(listOfNotNull(item.metadata.takeIf { it.isNotBlank() }, item.runtimeMinutes?.let { stringResource(R.string.runtime_minutes, it) }, item.rating?.let { "$it / 10" }).joinToString(" · "), color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall)
    if (failed) Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
      Text(stringResource(R.string.sdk_request_failed), Modifier.weight(1f), color = MaterialTheme.colorScheme.error)
      TextButton(onClick = retry) { Text(stringResource(R.string.retry)) }
    }
    FlowRow(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
      PlayAction(item) { actions.play(item.playTargetId ?: item.id, item.played) }
      OutlinedButton(onClick = { actions.remotePlay(item) }, enabled = selectionResolved && item.playable && item.itemType in setOf("Movie", "Episode") && !item.updating,
        modifier = Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.play_on_another_device)) }
      FilledTonalButton(onClick = { actions.watchlist(item.actionItemId, !item.inWatchlist) }, enabled = selectionResolved && !item.updating) {
        PilotIcon(if (item.inWatchlist) R.drawable.ic_bookmark_filled else R.drawable.ic_bookmark)
        Spacer(Modifier.width(6.dp))
        Text(stringResource(if (item.inWatchlist) R.string.remove_watchlist else R.string.add_watchlist))
      }
      OutlinedButton(onClick = { actions.watched(item.id, !item.played) }, enabled = selectionResolved && !item.updating) { Text(stringResource(if (item.played) R.string.mark_unplayed else R.string.mark_played)) }
    }
    if (item.playable && item.resumeSeconds > 0 && !item.played) TextButton(onClick = { actions.play(item.playTargetId ?: item.id, true) }, enabled = !item.updating) { Text(stringResource(R.string.play_from_start)) }
    if (item.playable) FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
      item.quality.forEach { Text(it, Modifier.padding(vertical = 14.dp), style = MaterialTheme.typography.labelSmall) }
      TextButton(onClick = { openTracks(true) }) {
        PilotIcon(R.drawable.ic_headphones)
        Spacer(Modifier.width(6.dp))
        Text(tracks?.audio?.firstOrNull { it.index == tracks.selectedAudio }?.label ?: item.audioLabel ?: stringResource(R.string.audio_tracks))
      }
      TextButton(onClick = { openTracks(false) }) {
        PilotIcon(R.drawable.ic_subtitles)
        Spacer(Modifier.width(6.dp))
        Text(if (tracks?.selectedSubtitle == -1) stringResource(R.string.subtitles_off) else tracks?.subtitles?.firstOrNull { it.index == tracks.selectedSubtitle }?.label ?: item.subtitleLabel ?: stringResource(R.string.subtitle_tracks))
      }
    }
    if (item.overview.isNotBlank()) Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
      Text(stringResource(R.string.overview), style = MaterialTheme.typography.titleMedium)
      Text(item.overview, style = MaterialTheme.typography.bodyMedium, color = LocalPilotColors.current.body)
    }
    if (item.cast.isNotEmpty()) Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
      Text(stringResource(R.string.cast), style = MaterialTheme.typography.titleMedium)
      FlowRow(horizontalArrangement = Arrangement.spacedBy(24.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        item.cast.forEach { person -> Text(listOf(person.role, person.name).filter { it.isNotBlank() }.joinToString(" · "), style = MaterialTheme.typography.bodySmall) }
      }
    }
  }
}

@Composable
internal fun TabletEpisodeRow(item: MediaUi, selected: Boolean, select: () -> Unit, play: () -> Unit) {
  val enlarged = LocalDensity.current.fontScale > 1.3f
  Surface(color = if (selected) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface, shape = MaterialTheme.shapes.medium) {
    Row(Modifier.fillMaxWidth().selectable(selected, role = Role.RadioButton, onClick = select).heightIn(min = 88.dp).padding(horizontal = 10.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
      Box(Modifier.width(if (enlarged) 88.dp else 120.dp)) {
        Artwork(item.backdrop ?: item.artwork, null, Modifier.fillMaxWidth().aspectRatio(16f / 9f))
        if (item.progress > 0f) LinearProgressIndicator(progress = { item.progress }, modifier = Modifier.fillMaxWidth().height(3.dp).align(Alignment.BottomCenter), drawStopIndicator = {})
        if (item.played) PilotIcon(R.drawable.ic_circle_check, stringResource(R.string.watched), Modifier.align(Alignment.TopEnd).padding(4.dp), tint = MaterialTheme.colorScheme.tertiary)
      }
      Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(listOfNotNull(item.episodeCode, item.title).joinToString(" "), style = MaterialTheme.typography.titleMedium, maxLines = if (enlarged) Int.MAX_VALUE else 2, overflow = TextOverflow.Ellipsis)
        Text(listOfNotNull(item.runtimeMinutes?.let { stringResource(R.string.runtime_minutes, it) },
          if (item.progress > 0f && !item.played) mediaCaption(item.copy(episodeCode = null)) else stringResource(if (item.played) R.string.watch_status_watched else R.string.watch_status_unwatched)).joinToString(" · "),
          style = MaterialTheme.typography.bodySmall, color = if (selected) MaterialTheme.colorScheme.secondary else LocalPilotColors.current.metadata)
      }
      FilledIconButton(onClick = play, enabled = item.playable, colors = IconButtonDefaults.filledIconButtonColors(containerColor = if (selected) LocalPilotColors.current.action else MaterialTheme.colorScheme.surfaceContainerHigh, contentColor = if (selected) Color.White else MaterialTheme.colorScheme.onSurface)) {
        PilotIcon(R.drawable.ic_player_play, playbackLabel(item))
      }
    }
  }
}
