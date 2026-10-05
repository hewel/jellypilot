@file:OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class, androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package io.github.hewel.jellypilot.ui

import androidx.compose.foundation.BorderStroke
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
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.hewel.jellypilot.remotePlayItem
import io.github.hewel.jellypilot.AppViewModel
import io.github.hewel.jellypilot.R

@Composable
internal fun HomeScreen(state: AppUiState, model: AppViewModel, scroll: LazyListState, tablet: Boolean = false) {
  if (tablet) {
    TabletHomeScreen(state, model, scroll)
    return
  }
  val featured = state.featured.ifEmpty { state.items.take(5) }
  val rows = state.homeRows.ifEmpty {
    if (state.items.isEmpty()) emptyList() else listOf(HomeRowUi("latest", stringResource(R.string.latest_media), state.items))
  }
  LazyColumn(state = scroll, contentPadding = PaddingValues(bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
    if (featured.isNotEmpty()) item(key = "featured") {
      val pager = rememberPagerState(pageCount = { featured.size })
      Box {
        HorizontalPager(pager, key = { "${featured[it].id}:${featured[it].playTargetId}" }) { page ->
          ImmersiveHero(featured[page], false, model, dots = {
            if (featured.size > 1) Row(horizontalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.padding(top = 16.dp)) {
              repeat(featured.size) { index ->
                Box(Modifier.width(if (index == pager.currentPage) 16.dp else 6.dp).height(6.dp).clip(CircleShape).background(Color.White.copy(alpha = if (index == pager.currentPage) 1f else 0.35f)))
              }
            }
          })
        }
        IconButton(onClick = { model.navigate(Destination.Search) }, modifier = Modifier.align(Alignment.TopEnd).statusBarsPadding().padding(12.dp)) {
          PilotIcon(R.drawable.ic_search, stringResource(R.string.search), tint = Color.White)
        }
      }
    } else item {
      if (state.busy) Box(Modifier.fillMaxWidth().height(360.dp), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
      else EmptyState(stringResource(R.string.no_results), action = stringResource(R.string.refresh), onAction = model::refresh)
    }
    state.recovery?.let { recovery -> item(key = "recovery") {
      Surface(Modifier.padding(horizontal = 16.dp), shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.surfaceContainer) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
          Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
              Text(stringResource(R.string.restore_playback), style = MaterialTheme.typography.titleMedium)
              Text(stringResource(R.string.restore_hint, recovery.title, playbackClock(recovery.positionSeconds)), color = LocalPilotColors.current.insetMetadata, style = MaterialTheme.typography.bodyMedium)
            }
            IconButton(onClick = model::clearLocalRecovery) { PilotIcon(R.drawable.ic_x, stringResource(R.string.discard_recovery)) }
          }
          Text(stringResource(R.string.restore_explanation), style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.insetMetadata)
          FilledTonalButton(onClick = model::restoreLocalPlayback) { Text(stringResource(R.string.restore_playback)) }
        }
      }
    } }
    rows.forEach { row -> item(key = "row-${row.id}") {
      Column {
        SectionHeading(row.title, if (row.libraryId != null) stringResource(R.string.view_all) else null) { row.libraryId?.let(model::openHomeLibrary) }
        LazyRow(contentPadding = PaddingValues(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
          items(row.items, key = { it.id }) { media ->
            if (row.landscape) ContinueCard(media, model) else Poster(media, Modifier.width(112.dp)) { model.showDetail(media.id) }
          }
        }
      }
    } }
    if (state.libraries.isNotEmpty()) item(key = "libraries") {
      SectionHeading(stringResource(R.string.library))
      LazyRow(contentPadding = PaddingValues(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        items(state.libraries, key = { it.id }) { library ->
          FilledTonalButton(onClick = { model.navigate(Destination.Library); model.selectLibrary(library.id) }) {
            PilotIcon(R.drawable.ic_folder)
            Spacer(Modifier.width(8.dp))
            Text(library.title)
          }
        }
      }
    }
  }
}

@Composable
private fun ContinueCard(item: MediaUi, model: AppViewModel) {
  Column(Modifier.width(170.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
    Box(Modifier.clip(MaterialTheme.shapes.medium).clickable { model.showDetail(item.id) }) {
      Artwork(item.backdrop ?: item.artwork, item.title, Modifier.fillMaxWidth().aspectRatio(16f / 9f))
      FilledIconButton(
        onClick = { model.playItem(item.playTargetId ?: item.id, item.played) },
        modifier = Modifier.align(Alignment.Center),
        colors = IconButtonDefaults.filledIconButtonColors(containerColor = LocalPilotColors.current.playerScrim, contentColor = Color.White),
      ) { PilotIcon(R.drawable.ic_player_play, playbackLabel(item)) }
      if (item.progress > 0f) LinearProgressIndicator(progress = { item.progress }, modifier = Modifier.fillMaxWidth().height(4.dp).align(Alignment.BottomCenter), drawStopIndicator = {})
    }
    Text(item.title, maxLines = if (LocalDensity.current.fontScale > 1.3f) Int.MAX_VALUE else 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.labelLarge)
    Text(mediaCaption(item), maxLines = 1, overflow = TextOverflow.Ellipsis, color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall)
  }
}

@Composable
internal fun ImmersiveHero(item: MediaUi, detail: Boolean, model: AppViewModel, onAudio: (() -> Unit)? = null, onSubtitles: (() -> Unit)? = null, itemActionsEnabled: Boolean = true, dots: @Composable () -> Unit = {}) {
  val background = MaterialTheme.colorScheme.background
  val fontScale = LocalDensity.current.fontScale
  BoxWithConstraints(Modifier.fillMaxWidth()) {
    val wide = maxWidth >= 600.dp
    val minimum = if (wide) 400.dp else if (detail && maxWidth < 360.dp) 492.dp else if (detail) 440.dp else 480.dp
    Box(Modifier.fillMaxWidth().heightIn(min = minimum)) {
      Artwork(item.backdrop ?: item.artwork, null, Modifier.matchParentSize().clickable { model.showDetail(item.actionItemId) }, rounded = false)
      Box(Modifier.matchParentSize().background(Brush.verticalGradient(listOf(Color.Black.copy(alpha = 0.45f), Color.Transparent, Color.Black.copy(alpha = 0.8f), Color.Black.copy(alpha = 0.85f)))))
      Box(Modifier.align(Alignment.BottomCenter).fillMaxWidth().height(24.dp).background(Brush.verticalGradient(listOf(Color.Black.copy(alpha = 0.85f), background))))
      Column(
        Modifier.align(Alignment.BottomStart).fillMaxWidth().padding(horizontal = if (wide) 32.dp else 16.dp)
          .padding(top = if (wide) 104.dp else 144.dp, bottom = 24.dp),
        verticalArrangement = Arrangement.Bottom,
      ) {
        Spacer(Modifier.height(if (fontScale > 1.3f) 16.dp else if (detail) 28.dp else 48.dp))
        if (item.logo != null) MediaLogo(item.logo, item.title, Modifier.fillMaxWidth().widthIn(max = 680.dp),
          width = if (wide) 320.dp else if (detail && item.itemType.equals("Movie", true)) 240.dp else 208.dp,
          height = if (detail && item.itemType.equals("Movie", true)) 74.dp else 67.dp, maxTitleLines = if (detail) Int.MAX_VALUE else 2)
        else Text(item.title, color = Color.White, style = if (wide) MaterialTheme.typography.displayMedium else MaterialTheme.typography.headlineLarge,
          maxLines = if (detail) Int.MAX_VALUE else 2, overflow = TextOverflow.Ellipsis,
          modifier = Modifier.widthIn(max = 680.dp).semantics { contentDescription = item.title })
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 8.dp)) {
          item.quality.filterNot { detail && it == item.audioLabel }.forEach { quality ->
            Box(Modifier.heightIn(min = if (detail) 48.dp else 24.dp), contentAlignment = Alignment.Center) {
              if (quality == "4K" || quality == "4K UHD") Surface(shape = MaterialTheme.shapes.extraSmall, color = Color.Transparent, contentColor = Color.White, border = BorderStroke(1.dp, Color.White.copy(alpha = 0.65f))) {
                Text(quality, style = MaterialTheme.typography.labelSmall, modifier = Modifier.padding(horizontal = 6.dp, vertical = 4.dp))
              } else Text(quality, color = Color.White, style = MaterialTheme.typography.labelSmall, modifier = Modifier.padding(vertical = 4.dp))
            }
          }
          if (detail && item.playable && onAudio != null) TextButton(onClick = onAudio, modifier = Modifier.heightIn(min = 48.dp), contentPadding = PaddingValues(horizontal = 4.dp), colors = ButtonDefaults.textButtonColors(contentColor = Color.White)) {
            PilotIcon(R.drawable.ic_headphones)
            Spacer(Modifier.width(6.dp))
            Text(item.audioLabel ?: stringResource(R.string.audio_tracks), style = MaterialTheme.typography.labelSmall)
          }
          if (detail && item.playable && onSubtitles != null) TextButton(onClick = onSubtitles, modifier = Modifier.heightIn(min = 48.dp), contentPadding = PaddingValues(horizontal = 4.dp), colors = ButtonDefaults.textButtonColors(contentColor = Color.White)) {
            PilotIcon(R.drawable.ic_subtitles)
            Spacer(Modifier.width(6.dp))
            Text(item.subtitleLabel ?: stringResource(R.string.subtitle_tracks), style = MaterialTheme.typography.labelSmall)
          }
        }
        val detailMetadata = listOfNotNull(item.metadata.takeIf { it.isNotBlank() }, item.runtimeMinutes?.let { stringResource(R.string.runtime_minutes, it) }, item.rating?.let { "$it / 10" }).joinToString(" · ")
        Text(if (detail) detailMetadata else mediaCaption(item), color = Color.White.copy(alpha = 0.85f), style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(top = 8.dp, bottom = 16.dp))
        Row(Modifier.widthIn(max = if (wide) 392.dp else 480.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
          PlayAction(item, Modifier.weight(1f)) { model.playItem(item.playTargetId ?: item.id, item.played) }
          IconButton(onClick = { model.setFavorite(item.actionItemId, !item.favorite) }, enabled = itemActionsEnabled && !item.updating) {
            PilotIcon(if (item.favorite) R.drawable.ic_heart_filled else R.drawable.ic_heart, stringResource(if (item.favorite) R.string.unfavorite else R.string.favorite), tint = if (item.favorite) LocalPilotColors.current.favorite else Color.White)
          }
          IconButton(onClick = { model.setWatchlist(item.actionItemId, !item.inWatchlist) }, enabled = itemActionsEnabled && !item.updating) {
            PilotIcon(if (item.inWatchlist) R.drawable.ic_bookmark_filled else R.drawable.ic_bookmark, stringResource(if (item.inWatchlist) R.string.remove_watchlist else R.string.add_watchlist), tint = Color.White)
          }
        }
        dots()
      }
    }
  }
}

@Composable
internal fun PhoneDetailScreen(state: AppUiState, model: AppViewModel, itemActionsEnabled: Boolean = true) {
  val detail = state.detail ?: return
  val tracks = state.detailTracks?.takeIf { it.targetId == detail.playTargetId }
  val preferencesLabel = stringResource(R.string.playback_preferences_default)
  val hero = detail.copy(
    audioLabel = tracks?.let { value -> value.audio.firstOrNull { it.index == value.selectedAudio }?.label } ?: detail.audioLabel ?: preferencesLabel,
    subtitleLabel = if (tracks?.selectedSubtitle == -1) stringResource(R.string.subtitles_off)
      else tracks?.let { value -> value.subtitles.firstOrNull { it.index == value.selectedSubtitle }?.label } ?: detail.subtitleLabel ?: preferencesLabel,
  )
  var expanded by rememberSaveable(detail.id) { mutableStateOf(false) }
  var audioSheet by rememberSaveable(detail.id) { mutableStateOf<Boolean?>(null) }
  LazyColumn(contentPadding = PaddingValues(bottom = 32.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
    item(key = "hero") {
      Box {
        ImmersiveHero(hero, true, model, onAudio = { audioSheet = true; model.loadDetailTracks(detail.playTargetId) }, onSubtitles = { audioSheet = false; model.loadDetailTracks(detail.playTargetId) }, itemActionsEnabled = itemActionsEnabled)
        IconButton(onClick = model::back, modifier = Modifier.statusBarsPadding().padding(12.dp)) { PilotIcon(R.drawable.ic_chevron_left, stringResource(R.string.back), tint = Color.White) }
      }
    }
    item(key = "status") {
      Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(stringResource(if (detail.played) R.string.watch_status_watched else R.string.watch_status_unwatched), Modifier.weight(1f), color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall)
        TextButton(onClick = { model.setPlayed(detail.id, !detail.played) }, enabled = itemActionsEnabled && !detail.updating) { Text(stringResource(if (detail.played) R.string.mark_unplayed else R.string.mark_played)) }
      }
    }
    item {
      OutlinedButton(onClick = { model.playOnAnotherDevice(detail) }, enabled = itemActionsEnabled && state.remotePlayItem(detail) != null && !detail.updating,
        modifier = Modifier.padding(horizontal = 16.dp).heightIn(min = 48.dp)) { Text(stringResource(R.string.play_on_another_device)) }
    }
    if (detail.playable && detail.resumeSeconds > 0 && !detail.played) item { TextButton(onClick = { model.playItem(detail.playTargetId ?: detail.id, true) }, enabled = !detail.updating, modifier = Modifier.padding(horizontal = 16.dp)) { Text(stringResource(R.string.play_from_start)) } }
    if (detail.genres.isNotEmpty()) item { Text(detail.genres.joinToString(" · "), Modifier.padding(horizontal = 16.dp), color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall) }
    if (detail.overview.isNotBlank()) item(key = "overview") {
      Column(Modifier.padding(horizontal = 16.dp)) {
        Text(stringResource(R.string.overview), style = MaterialTheme.typography.sectionHeading, modifier = Modifier.padding(bottom = 8.dp))
        Text(detail.overview, color = LocalPilotColors.current.body, style = MaterialTheme.typography.bodyMedium, maxLines = if (expanded) Int.MAX_VALUE else 2, overflow = TextOverflow.Ellipsis)
        TextButton(onClick = { expanded = !expanded }, contentPadding = PaddingValues(0.dp)) { Text(stringResource(if (expanded) R.string.collapse else R.string.expand)) }
      }
    }
    if (detail.seasons.isNotEmpty()) item(key = "seasons") {
      LazyRow(contentPadding = PaddingValues(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        items(detail.seasons, key = { it.id }) { season -> FilterChip(state.selectedSeasonId == season.id, { model.selectSeason(season.id) }, label = { Text(season.title) }) }
      }
    }
    if (state.detailItems.isNotEmpty()) {
      item { SectionHeading(stringResource(R.string.episodes)) }
      items(state.detailItems, key = { it.id }) { child -> EpisodeRow(child, model) }
    }
    if (state.episodesHaveMore) item { TextButton(onClick = model::loadMoreEpisodes, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.load_more)) } }
    if (detail.cast.isNotEmpty()) item(key = "cast") {
      SectionHeading(stringResource(R.string.cast))
      LazyRow(contentPadding = PaddingValues(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        items(detail.cast, key = { it.id }) { cast ->
          Column(Modifier.width(104.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Artwork(cast.artwork, null, Modifier.size(72.dp).clip(CircleShape))
            Text(cast.name, minLines = 2, maxLines = 2, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.labelLarge)
            Text(cast.role, minLines = 2, maxLines = 2, overflow = TextOverflow.Ellipsis, color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall)
          }
        }
      }
    }
    if (detail.related.isNotEmpty()) item(key = "related") {
      SectionHeading(stringResource(R.string.related))
      LazyRow(contentPadding = PaddingValues(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        items(detail.related, key = { it.id }) { related -> Poster(related, Modifier.width(112.dp)) { model.showDetail(related.id) } }
      }
    }
  }
  audioSheet?.takeUnless { LocalBackPreview.current }?.let { audio ->
    DetailTrackSheet(audio, tracks, { audioSheet = null }, { model.loadDetailTracks(detail.playTargetId) },
      if (audio) model::selectDetailAudio else model::selectDetailSubtitle)
  }
}

@Composable
internal fun EpisodeRow(item: MediaUi, model: AppViewModel) {
  BoxWithConstraints {
  val narrow = maxWidth < 360.dp || LocalDensity.current.fontScale > 1.3f
  Row(Modifier.fillMaxWidth().clickable { model.showDetail(item.id) }.padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
    Box(Modifier.width(if (narrow) 88.dp else 120.dp)) {
      Artwork(item.backdrop ?: item.artwork, null, Modifier.fillMaxWidth().aspectRatio(16f / 10f))
      if (item.progress > 0f) LinearProgressIndicator(progress = { item.progress }, modifier = Modifier.fillMaxWidth().height(4.dp).align(Alignment.BottomCenter), drawStopIndicator = {})
    }
    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
      item.episodeCode?.let { Text(it, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.secondary) }
      Text(item.title, style = MaterialTheme.typography.labelLarge)
      Text(mediaCaption(item.copy(episodeCode = null)), style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.metadata)
      if (!narrow && item.overview.isNotBlank()) Text(item.overview, maxLines = 2, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.metadata)
    }
    IconButton(onClick = { model.playItem(item.id, item.played) }, enabled = item.playable) { PilotIcon(R.drawable.ic_player_play, playbackLabel(item)) }
  }
  }
}
