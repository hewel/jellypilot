package io.github.hewel.jellypilot.ui

import android.icu.util.ULocale
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.hewel.jellypilot.AppViewModel
import io.github.hewel.jellypilot.R
import io.github.hewel.jellypilot.player.PlayerHost
import io.github.hewel.jellypilot.player.PlayerSnapshot
import io.github.hewel.jellypilot.player.TrackKind

internal enum class PlayerPanel { Audio, Subtitles, Queue, Video }

internal data class PlayerPanelActions(
  val selectTrack: (TrackKind, Int) -> Unit,
  val selectEpisode: (MediaUi) -> Unit,
  val previousEpisode: () -> Unit,
  val nextEpisode: () -> Unit,
  val loadMoreEpisodes: () -> Unit,
  val setVolume: (Int) -> Unit,
  val close: () -> Unit,
)

@Composable
internal fun PlayerPanelContent(
  panel: PlayerPanel,
  snapshot: PlayerSnapshot,
  playback: PlaybackUi?,
  model: AppViewModel,
  close: () -> Unit,
) {
  PlayerPanelContent(panel, snapshot, playback, PlayerPanelActions(
    selectTrack = model::selectPlaybackTrack,
    selectEpisode = { model.playItem(it.id, it.played) },
    previousEpisode = { model.previousEpisode() },
    nextEpisode = { model.nextEpisode() },
    loadMoreEpisodes = { model.loadMorePlaybackEpisodes() },
    setVolume = model::setPlaybackVolume,
    close = close,
  ))
}

/** The same content is hosted in a portrait sheet or a landscape side panel. */
@Composable
internal fun PlayerPanelContent(
  panel: PlayerPanel,
  snapshot: PlayerSnapshot,
  playback: PlaybackUi?,
  actions: PlayerPanelActions,
) {
  val displayLocale = ULocale.forLocale(LocalConfiguration.current.locales[0])
  val trackKind = when (panel) {
    PlayerPanel.Audio -> TrackKind.AUDIO
    PlayerPanel.Subtitles -> TrackKind.SUBTITLE
    else -> null
  }
  val tracks = snapshot.tracks.filter { it.kind == trackKind }
  val title = stringResource(when (panel) {
    PlayerPanel.Audio -> R.string.audio_tracks
    PlayerPanel.Subtitles -> R.string.subtitle_tracks
    PlayerPanel.Queue -> R.string.queue
    PlayerPanel.Video -> R.string.video_options
  })
  val count = when (panel) {
    PlayerPanel.Audio -> pluralStringResource(R.plurals.player_audio_track_count, tracks.size, tracks.size)
    PlayerPanel.Subtitles -> pluralStringResource(R.plurals.player_subtitle_track_count, tracks.size, tracks.size)
    PlayerPanel.Queue -> playback?.queue?.size?.let { pluralStringResource(R.plurals.items_count, it, it) }
    PlayerPanel.Video -> null
  }
  val listState = rememberLazyListState()
  val remainingTracks by remember(listState) {
    derivedStateOf {
      val layout = listState.layoutInfo
      if (!listState.canScrollForward || layout.visibleItemsInfo.isEmpty()) 0
      else {
        val fullyVisibleEnd = layout.visibleItemsInfo.lastOrNull {
          it.offset + it.size <= layout.viewportEndOffset
        }?.index ?: (layout.visibleItemsInfo.first().index - 1)
        (layout.totalItemsCount - fullyVisibleEnd - 1).coerceAtLeast(0)
      }
    }
  }
  Column(
    Modifier.fillMaxSize().background(MaterialTheme.colorScheme.surfaceContainer).padding(16.dp),
    verticalArrangement = Arrangement.spacedBy(8.dp),
  ) {
    Row(
      Modifier.fillMaxWidth(),
      horizontalArrangement = Arrangement.spacedBy(12.dp),
      verticalAlignment = Alignment.CenterVertically,
    ) {
      Column(Modifier.weight(1f)) {
        Text(title, Modifier.semantics { heading() },
          style = MaterialTheme.typography.titleLarge.copy(fontSize = 20.sp, lineHeight = 28.sp, fontWeight = FontWeight.SemiBold))
        count?.let {
          Text(it, style = MaterialTheme.typography.bodySmall.copy(fontSize = 12.sp, lineHeight = 16.sp),
            color = LocalPilotColors.current.metadata)
        }
      }
      Box(
        Modifier.size(48.dp).clip(RoundedCornerShape(12.dp)).clickable(role = Role.Button, onClick = actions.close),
        contentAlignment = Alignment.Center,
      ) {
        Box(Modifier.size(44.dp).background(MaterialTheme.colorScheme.surfaceContainerHigh, RoundedCornerShape(12.dp)),
          contentAlignment = Alignment.Center) {
          PilotIcon(R.drawable.ic_x, stringResource(R.string.close))
        }
      }
    }
    LazyColumn(
      Modifier.weight(1f).fillMaxWidth().then(if (trackKind != null) Modifier.selectableGroup() else Modifier),
      state = listState,
      verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
      if (trackKind != null) {
        if (trackKind == TrackKind.SUBTITLE) item(key = "subtitles-off") {
          PlayerTrackRow(
            title = stringResource(R.string.subtitles_off),
            metadata = stringResource(R.string.player_subtitles_off_description),
            selected = tracks.none { it.isSelected },
            minimumHeight = 52.dp,
            select = { actions.selectTrack(TrackKind.SUBTITLE, PlayerHost.TRACK_ID_NONE) },
          )
        }
        items(tracks, key = { "${it.kind}:${it.mpvId}" }) { track ->
          val namedTitle = track.title?.trim()?.takeIf { it.isNotEmpty() }
          val language = remember(track.language, displayLocale) {
            track.language?.trim()?.takeIf { it.isNotEmpty() }?.let {
              ULocale.createCanonical(it.replace('-', '_')).getDisplayName(displayLocale).ifBlank { it }
            }
          }
          PlayerTrackRow(
            title = namedTitle ?: language ?: stringResource(R.string.player_track_number, track.mpvId),
            metadata = listOfNotNull(language.takeIf { namedTitle != null }, track.codec)
              .filter { it.isNotBlank() }.joinToString(" · ").ifBlank { null },
            selected = track.isSelected,
            minimumHeight = if (trackKind == TrackKind.AUDIO) 64.dp else 52.dp,
            select = { actions.selectTrack(trackKind, track.mpvId) },
          )
        }
      } else if (panel == PlayerPanel.Queue) {
        item(key = "episode-navigation") {
          Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            TextButton(onClick = actions.previousEpisode, enabled = playback?.canPrevious == true,
              modifier = Modifier.weight(1f).heightIn(min = 48.dp)) {
              Text(stringResource(R.string.previous_episode))
            }
            TextButton(onClick = actions.nextEpisode, enabled = playback?.canNext == true,
              modifier = Modifier.weight(1f).heightIn(min = 48.dp)) {
              Text(stringResource(R.string.next_episode))
            }
          }
        }
        items(playback?.queue.orEmpty(), key = { it.id }) { episode ->
          PlayerTrackRow(
            title = episode.title,
            metadata = listOfNotNull(episode.episodeCode, episode.metadata.takeIf { it.isNotBlank() }).joinToString(" · ").ifBlank { null },
            selected = playback?.currentItemId == episode.id,
            minimumHeight = 64.dp,
          ) {
            actions.selectEpisode(episode)
            actions.close()
          }
        }
        if (playback?.queueHasMore == true) item(key = "more-episodes") {
          TextButton(onClick = actions.loadMoreEpisodes, enabled = !playback.queueLoading,
            modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) {
            Text(stringResource(R.string.load_more))
          }
        }
      } else {
        item(key = "resolution") {
          Text(stringResource(R.string.video_resolution, snapshot.videoWidth, snapshot.videoHeight),
            Modifier.padding(vertical = 8.dp), style = MaterialTheme.typography.bodyLarge)
        }
        item(key = "video-hint") {
          Text(stringResource(R.string.video_controls_hint), style = MaterialTheme.typography.bodySmall,
            color = LocalPilotColors.current.metadata)
        }
        item(key = "volume") {
          val volumeLabel = stringResource(R.string.volume)
          Column(Modifier.padding(top = 16.dp)) {
            Text(volumeLabel, style = MaterialTheme.typography.bodyLarge)
            Slider(snapshot.volumePercent.coerceIn(0, 100).toFloat(), { actions.setVolume(it.toInt()) },
              modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).semantics { contentDescription = volumeLabel },
              valueRange = 0f..100f)
          }
        }
      }
    }
    if (trackKind != null && remainingTracks > 0) {
      Text(pluralStringResource(R.plurals.player_tracks_remaining, remainingTracks, remainingTracks),
        style = MaterialTheme.typography.bodySmall.copy(fontSize = 12.sp, lineHeight = 16.sp),
        color = LocalPilotColors.current.metadata)
    }
  }
}

@Composable
private fun PlayerTrackRow(
  title: String,
  metadata: String?,
  selected: Boolean,
  minimumHeight: Dp,
  select: () -> Unit,
) {
  Row(
    Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp))
      .background(if (selected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainer)
      .selectable(selected = selected, role = Role.RadioButton, onClick = select)
      .heightIn(min = minimumHeight).padding(horizontal = 12.dp, vertical = 8.dp),
    horizontalArrangement = Arrangement.spacedBy(12.dp),
    verticalAlignment = Alignment.CenterVertically,
  ) {
    Column(Modifier.weight(1f)) {
      Text(title, style = MaterialTheme.typography.bodyLarge.copy(fontSize = 16.sp, lineHeight = 24.sp, fontWeight = FontWeight.Medium))
      metadata?.let {
        Text(it, style = MaterialTheme.typography.bodySmall.copy(fontSize = 12.sp, lineHeight = 16.sp),
          color = LocalPilotColors.current.metadata)
      }
    }
    if (selected) Icon(painterResource(R.drawable.ic_check), contentDescription = null,
      modifier = Modifier.size(24.dp), tint = MaterialTheme.colorScheme.secondary)
    else Spacer(Modifier.size(24.dp))
  }
}
