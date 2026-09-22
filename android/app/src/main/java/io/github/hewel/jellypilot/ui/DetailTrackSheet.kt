@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.hewel.jellypilot.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.hewel.jellypilot.R

/** Provider stream indexes are kept distinct from the active player's mpv track IDs. */
@Composable
internal fun DetailTrackSheet(audio: Boolean, tracks: DetailTracksUi?, close: () -> Unit, retry: () -> Unit, select: (Int?) -> Unit) {
  val list = rememberLazyListState()
  val options = if (audio) tracks?.audio.orEmpty() else tracks?.subtitles.orEmpty()
  val enlarged = LocalDensity.current.fontScale > 1.3f
  ModalBottomSheet(onDismissRequest = close, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
    shape = RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp), containerColor = MaterialTheme.colorScheme.surfaceContainer) {
    BoxWithConstraints(Modifier.fillMaxWidth()) {
    val contentHeight = if (enlarged) maxHeight * 0.85f else minOf(maxHeight * 0.85f, if (audio) 318.dp else 442.dp)
    Column(Modifier.fillMaxWidth().heightIn(max = contentHeight).padding(horizontal = 16.dp)) {
      Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
          Text(stringResource(if (audio) R.string.audio_tracks else R.string.subtitle_tracks), style = MaterialTheme.typography.titleMedium)
          if (tracks != null && !tracks.busy && tracks.error == null) Text(
            pluralStringResource(if (audio) R.plurals.player_audio_track_count else R.plurals.player_subtitle_track_count, options.size, options.size),
            style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.metadata)
        }
        IconButton(onClick = close) { PilotIcon(R.drawable.ic_x, stringResource(R.string.close)) }
      }
      when {
        tracks == null || tracks.busy -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        tracks.error != null -> EmptyState(tracks.error, action = stringResource(R.string.retry), onAction = retry)
        else -> {
          val selection = if (audio) tracks.selectedAudio else tracks.selectedSubtitle
          LazyColumn(Modifier.weight(1f, fill = false).selectableGroup(), state = list, contentPadding = PaddingValues(top = 8.dp, bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            item(key = "default") { DetailTrackRow(stringResource(R.string.playback_preferences_default), selection == null) { select(null) } }
            if (!audio) item(key = "off") {
              DetailTrackRow(stringResource(R.string.subtitles_off), selection == -1,
                minimumHeight = if (enlarged) 64.dp else 56.dp) { select(-1) }
            }
            items(options, key = { it.index }) { track ->
              val metadata = listOfNotNull(
                track.language?.takeIf { it.isNotBlank() && !track.label.contains(it, ignoreCase = true) },
                track.codec?.takeIf { it.isNotBlank() && !track.label.contains(it, ignoreCase = true) }?.uppercase(java.util.Locale.ROOT),
                if (track.isDefault) stringResource(R.string.browse_track_default) else null,
                if (track.isExternal) stringResource(R.string.browse_track_external) else null,
              ).joinToString(" · ")
              DetailTrackRow(track.label.ifBlank { stringResource(R.string.player_track_number, track.index) }, selection == track.index, metadata) { select(track.index) }
            }
            if (options.isEmpty()) item { Text(stringResource(R.string.no_tracks), Modifier.padding(16.dp), color = LocalPilotColors.current.metadata) }
          }
          val remaining by remember(options.size, audio) { derivedStateOf {
            if (!list.canScrollForward) 0 else (options.size + (if (audio) 1 else 2) - (list.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: -1) - 1).coerceAtLeast(1)
          } }
          if (remaining > 0) Text(pluralStringResource(R.plurals.player_tracks_remaining, remaining, remaining),
            Modifier.fillMaxWidth().padding(vertical = 12.dp), style = MaterialTheme.typography.bodySmall,
            color = LocalPilotColors.current.metadata, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
        }
      }
    }
    }
  }
}

@Composable
private fun DetailTrackRow(label: String, selected: Boolean, metadata: String = "", minimumHeight: Dp = 56.dp, select: () -> Unit) {
  TextButton(onClick = select, modifier = Modifier.fillMaxWidth().heightIn(min = minimumHeight).semantics { this.selected = selected; role = Role.RadioButton }, shape = MaterialTheme.shapes.small,
    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp),
    colors = ButtonDefaults.textButtonColors(containerColor = if (selected) LocalPilotColors.current.trackSelection else Color.Transparent)) {
    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
      Text(label, style = MaterialTheme.typography.bodyLarge.copy(lineHeight = 20.sp, fontWeight = FontWeight.Medium), color = MaterialTheme.colorScheme.onSurface)
      if (metadata.isNotBlank()) Text(metadata, style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.insetMetadata)
    }
    Spacer(Modifier.width(12.dp))
    Box(Modifier.width(24.dp), contentAlignment = Alignment.Center) { if (selected) PilotIcon(R.drawable.ic_check, tint = MaterialTheme.colorScheme.onSurface) }
  }
}
