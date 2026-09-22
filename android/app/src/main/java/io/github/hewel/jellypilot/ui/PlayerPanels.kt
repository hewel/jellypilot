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
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Switch
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.key
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
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
import io.github.hewel.jellypilot.player.PlayerStatus
import io.github.hewel.jellypilot.player.TrackKind
import java.text.NumberFormat

internal enum class PlayerPanel { Audio, Subtitles, Queue, Video, More, Speed, Picture }

internal data class PlayerPanelActions(
  val selectTrack: (TrackKind, Int) -> Unit,
  val selectEpisode: (MediaUi) -> Unit,
  val previousEpisode: () -> Unit,
  val nextEpisode: () -> Unit,
  val loadMoreEpisodes: () -> Unit,
  val setVolume: (Int) -> Unit,
  val close: () -> Unit,
  val open: (PlayerPanel) -> Unit = {},
  val back: (() -> Unit)? = null,
  val setAutoSkip: (Boolean) -> Unit = {},
  val setSpeed: (Double) -> Unit = {},
  val setPictureBrightness: (Int) -> Unit = {},
)

@Composable
internal fun PlayerPanelContent(
  panel: PlayerPanel,
  snapshot: PlayerSnapshot,
  playback: PlaybackUi?,
  model: AppViewModel,
  close: () -> Unit,
  open: (PlayerPanel) -> Unit = {},
  back: (() -> Unit)? = null,
  restoreRow: PlayerPanel? = null,
  phone: Boolean = false,
) {
  PlayerPanelContent(panel, snapshot, playback, PlayerPanelActions(
    selectTrack = model::selectPlaybackTrack,
    selectEpisode = { model.playItem(it.id, it.played) },
    previousEpisode = { model.previousEpisode() },
    nextEpisode = { model.nextEpisode() },
    loadMoreEpisodes = { model.loadMorePlaybackEpisodes() },
    setVolume = model::setPlaybackVolume,
    close = close,
    open = open,
    back = back,
    setAutoSkip = model::setSessionAutoSkip,
    setSpeed = model.player::speed,
    setPictureBrightness = model.player::pictureBrightness,
  ), restoreRow = restoreRow, phone = phone)
}

/** The same content is hosted in a portrait sheet or a landscape side panel. */
@Composable
internal fun PlayerPanelContent(
  panel: PlayerPanel,
  snapshot: PlayerSnapshot,
  playback: PlaybackUi?,
  actions: PlayerPanelActions,
  restoreRow: PlayerPanel? = null,
  phone: Boolean = false,
) {
  val displayLocale = ULocale.forLocale(LocalConfiguration.current.locales[0])
  val trackKind = when (panel) {
    PlayerPanel.Audio -> TrackKind.AUDIO
    PlayerPanel.Subtitles -> TrackKind.SUBTITLE
    else -> null
  }
  val tracks = snapshot.tracks.filter { it.kind == trackKind }
  val settingsReady = snapshot.status == PlayerStatus.READY || snapshot.status == PlayerStatus.BUFFERING
  val title = stringResource(when (panel) {
    PlayerPanel.Audio -> R.string.audio_tracks
    PlayerPanel.Subtitles -> R.string.subtitle_tracks
    PlayerPanel.Queue -> R.string.queue
    PlayerPanel.Video -> if (phone) R.string.player_video_details else R.string.video_options
    PlayerPanel.More -> R.string.player_playback_settings
    PlayerPanel.Speed -> R.string.player_playback_speed
    PlayerPanel.Picture -> R.string.player_picture
  })
  val count = when (panel) {
    PlayerPanel.Audio -> pluralStringResource(R.plurals.player_audio_track_count, tracks.size, tracks.size)
    PlayerPanel.Subtitles -> pluralStringResource(R.plurals.player_subtitle_track_count, tracks.size, tracks.size)
    PlayerPanel.Queue -> playback?.queue?.size?.let { pluralStringResource(R.plurals.items_count, it, it) }
    PlayerPanel.Video, PlayerPanel.More, PlayerPanel.Speed, PlayerPanel.Picture -> null
  }
  val listState = key(panel) { rememberLazyListState() }
  val rowFocus = remember { PlayerPanel.entries.associateWith { FocusRequester() } }
  LaunchedEffect(panel, restoreRow) {
    if (panel == PlayerPanel.More && restoreRow != null) {
      val skipRow = if (playback?.autoSkipAvailable == true) 1 else 0
      val index = when (restoreRow) {
        PlayerPanel.Speed -> 1
        PlayerPanel.Picture -> 2
        PlayerPanel.Audio -> 3 + skipRow
        PlayerPanel.Video -> 4 + skipRow
        else -> 0
      }
      listState.scrollToItem(index)
      withFrameNanos { }
      rowFocus[restoreRow]?.requestFocus()
    }
  }
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
      actions.back?.let { back ->
        Box(Modifier.size(48.dp).clip(RoundedCornerShape(12.dp)).clickable(role = Role.Button, onClick = back),
          contentAlignment = Alignment.Center) {
          PilotIcon(R.drawable.ic_chevron_left, stringResource(R.string.back))
        }
      }
      Column(Modifier.weight(1f)) {
        Text(title, Modifier.semantics { heading() },
          style = if (phone) MaterialTheme.typography.titleMedium else MaterialTheme.typography.titleLarge.copy(fontSize = 20.sp, lineHeight = 28.sp, fontWeight = FontWeight.SemiBold))
        count?.let {
          Text(it, style = MaterialTheme.typography.bodySmall.copy(fontSize = 12.sp, lineHeight = 16.sp),
            color = LocalPilotColors.current.metadata)
        }
      }
      Box(
        Modifier.size(48.dp).clip(RoundedCornerShape(12.dp)).clickable(role = Role.Button, onClick = actions.close),
        contentAlignment = Alignment.Center,
      ) {
        Box(Modifier.size(44.dp).then(if (phone) Modifier else Modifier.background(MaterialTheme.colorScheme.surfaceContainerHigh, RoundedCornerShape(12.dp))),
          contentAlignment = Alignment.Center) {
          PilotIcon(R.drawable.ic_x, stringResource(R.string.close))
        }
      }
    }
    LazyColumn(
      Modifier.weight(1f).fillMaxWidth().then(if (trackKind != null || panel == PlayerPanel.Speed) Modifier.selectableGroup() else Modifier),
      state = listState,
      verticalArrangement = Arrangement.spacedBy(if (phone) 4.dp else 8.dp),
    ) {
      if (trackKind != null) {
        if (trackKind == TrackKind.SUBTITLE) item(key = "subtitles-off") {
          PlayerTrackRow(
            title = stringResource(R.string.subtitles_off),
            metadata = stringResource(R.string.player_subtitles_off_description),
            selected = tracks.none { it.isSelected },
            minimumHeight = if (phone) 56.dp else 52.dp,
            phone = phone,
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
            metadata = listOfNotNull(language.takeIf { namedTitle != null }, track.codec,
              if (phone && track.isDefault) stringResource(R.string.player_track_default) else null)
              .filter { it.isNotBlank() }.joinToString(" · ").ifBlank { null },
            selected = track.isSelected,
            minimumHeight = if (phone) 56.dp else if (trackKind == TrackKind.AUDIO) 64.dp else 52.dp,
            phone = phone,
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
            minimumHeight = if (phone) 56.dp else 64.dp,
            phone = phone,
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
      } else if (panel == PlayerPanel.More) {
        item(key = "volume") { PlayerPanelVolume(snapshot, actions.setVolume, phone = true) }
        item(key = "speed") {
          PlayerSettingsRow(stringResource(R.string.player_playback_speed),
            playerRateLabel(snapshot.speed),
            Modifier.focusRequester(rowFocus.getValue(PlayerPanel.Speed)), inlineDetail = true,
            open = { actions.open(PlayerPanel.Speed) })
        }
        item(key = "picture") {
          PlayerSettingsRow(stringResource(R.string.player_picture), null,
            Modifier.focusRequester(rowFocus.getValue(PlayerPanel.Picture)),
            open = { actions.open(PlayerPanel.Picture) })
        }
        if (playback?.autoSkipAvailable == true) item(key = "auto-skip") {
          Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp))
            .toggleable(playback.autoSkipEnabled, role = Role.Switch, onValueChange = actions.setAutoSkip)
            .heightIn(min = 56.dp).padding(vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Column(Modifier.weight(1f)) {
              Text(stringResource(R.string.player_auto_skip_full), style = MaterialTheme.typography.bodyMedium)
              Text(stringResource(R.string.player_session_only), style = MaterialTheme.typography.bodySmall,
                color = LocalPilotColors.current.metadata)
            }
            Switch(checked = playback.autoSkipEnabled, onCheckedChange = null)
          }
        }
        item(key = "audio") {
          val audio = snapshot.tracks.firstOrNull { it.kind == TrackKind.AUDIO && it.isSelected }
          val label = audio?.let { track ->
            track.title?.takeIf { it.isNotBlank() } ?: track.language?.let {
              ULocale.createCanonical(it.replace('-', '_')).getDisplayName(displayLocale)
            } ?: stringResource(R.string.player_track_number, track.mpvId)
          }
          PlayerSettingsRow(stringResource(R.string.audio_tracks),
            listOfNotNull(label, audio?.codec).joinToString(" · ").ifBlank { null },
            Modifier.focusRequester(rowFocus.getValue(PlayerPanel.Audio)),
            enabled = snapshot.tracks.any { it.kind == TrackKind.AUDIO },
            open = { actions.open(PlayerPanel.Audio) })
        }
        item(key = "video") {
          PlayerSettingsRow(stringResource(R.string.player_video_details), null,
            Modifier.focusRequester(rowFocus.getValue(PlayerPanel.Video)),
            open = { actions.open(PlayerPanel.Video) })
        }
      } else if (panel == PlayerPanel.Speed) {
        items(listOf(0.75, 1.0, 1.5, 2.0), key = { it }) { rate ->
          PlayerTrackRow(playerRateLabel(rate), null,
            selected = snapshot.speed == rate, minimumHeight = 48.dp, phone = phone, enabled = settingsReady) { actions.setSpeed(rate) }
        }
      } else if (panel == PlayerPanel.Picture) {
        item(key = "brightness") {
          val label = stringResource(R.string.player_picture_brightness)
          Column {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
              Text(label, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
              Text(stringResource(R.string.player_volume_percent, snapshot.pictureBrightnessPercent),
                style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.metadata)
            }
            PlayerSlider(snapshot.pictureBrightnessPercent.coerceIn(20, 100).toFloat(),
              { actions.setPictureBrightness(it.toInt()) }, finished = {}, range = 20f..100f,
              enabled = settingsReady && snapshot.pictureBrightnessAvailable, label = label, modifier = Modifier.fillMaxWidth(),
              activeColor = PilotPlayerTokens.foreground, thumbSize = 12.dp)
            Text(stringResource(if (snapshot.pictureBrightnessAvailable) R.string.player_picture_only else R.string.player_picture_unavailable),
              style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.metadata)
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
        if (!phone) item(key = "volume") { PlayerPanelVolume(snapshot, actions.setVolume) }
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
private fun playerRateLabel(speed: Double): String {
  val locale = LocalConfiguration.current.locales[0]
  val number = remember(speed, locale) {
    NumberFormat.getNumberInstance(locale).apply { maximumFractionDigits = 2 }.format(speed)
  }
  return stringResource(R.string.player_speed_value, number)
}

@Composable
private fun PlayerTrackRow(
  title: String,
  metadata: String?,
  selected: Boolean,
  minimumHeight: Dp,
  phone: Boolean = false,
  enabled: Boolean = true,
  select: () -> Unit,
) {
  Row(
    Modifier.fillMaxWidth().clip(RoundedCornerShape(if (phone) 8.dp else 12.dp))
      .background(if (selected) { if (phone) PilotPlayerTokens.phonePanelSelected else MaterialTheme.colorScheme.primaryContainer } else MaterialTheme.colorScheme.surfaceContainer)
      .selectable(selected = selected, enabled = enabled, role = Role.RadioButton, onClick = select)
      .heightIn(min = minimumHeight).padding(horizontal = 12.dp, vertical = 8.dp),
    horizontalArrangement = Arrangement.spacedBy(12.dp),
    verticalAlignment = Alignment.CenterVertically,
  ) {
    Column(Modifier.weight(1f)) {
      Text(title, style = MaterialTheme.typography.bodyLarge.copy(fontSize = 16.sp, lineHeight = 24.sp, fontWeight = FontWeight.Medium),
        color = if (enabled) MaterialTheme.colorScheme.onSurface else LocalPilotColors.current.metadata)
      metadata?.let {
        Text(it, style = MaterialTheme.typography.bodySmall.copy(fontSize = 12.sp, lineHeight = 16.sp),
          color = LocalPilotColors.current.metadata)
      }
    }
    if (selected) Icon(painterResource(R.drawable.ic_check), contentDescription = null,
      modifier = Modifier.size(24.dp), tint = if (phone) PilotPlayerTokens.foreground else MaterialTheme.colorScheme.secondary)
    else Spacer(Modifier.size(24.dp))
  }
}

@Composable
private fun PlayerPanelVolume(snapshot: PlayerSnapshot, change: (Int) -> Unit, phone: Boolean = false) {
  val label = stringResource(R.string.volume)
  Column {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically,
      horizontalArrangement = Arrangement.spacedBy(12.dp)) {
      Text(label, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
      Text(stringResource(R.string.player_volume_percent, snapshot.volumePercent),
        style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.metadata)
    }
    if (phone) PlayerSlider(snapshot.volumePercent.coerceIn(0, 100).toFloat(), { change(it.toInt()) },
      finished = {}, range = 0f..100f, enabled = true, label = label, modifier = Modifier.fillMaxWidth(),
      activeColor = PilotPlayerTokens.foreground, thumbSize = 12.dp)
    else Slider(snapshot.volumePercent.coerceIn(0, 100).toFloat(), { change(it.toInt()) },
      modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).semantics { contentDescription = label },
      valueRange = 0f..100f)
  }
}

@Composable
private fun PlayerSettingsRow(title: String, detail: String?, modifier: Modifier = Modifier,
  enabled: Boolean = true, inlineDetail: Boolean = false, open: () -> Unit) {
  Row(modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).clickable(enabled = enabled, role = Role.Button, onClick = open)
    .heightIn(min = 52.dp).padding(vertical = 4.dp),
    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
    Column(Modifier.weight(1f)) {
      Text(title, style = MaterialTheme.typography.bodyMedium,
        color = if (enabled) MaterialTheme.colorScheme.onSurface else LocalPilotColors.current.metadata)
      if (!inlineDetail) detail?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.metadata) }
    }
    if (inlineDetail) detail?.let { Text(it, style = MaterialTheme.typography.bodyMedium, color = LocalPilotColors.current.metadata) }
    PilotIcon(R.drawable.ic_chevron_right, tint = LocalPilotColors.current.metadata)
  }
}
