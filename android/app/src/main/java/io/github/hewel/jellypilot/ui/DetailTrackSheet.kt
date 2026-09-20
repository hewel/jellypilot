@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.hewel.jellypilot.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.hewel.jellypilot.R

/** Provider stream indexes are kept distinct from the active player's mpv track IDs. */
@Composable
internal fun DetailTrackSheet(audio: Boolean, tracks: DetailTracksUi?, close: () -> Unit, retry: () -> Unit, select: (Int?) -> Unit) {
  ModalBottomSheet(onDismissRequest = close) {
    Column(Modifier.fillMaxWidth().fillMaxHeight(0.8f).padding(horizontal = 16.dp)) {
      Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(stringResource(if (audio) R.string.audio_tracks else R.string.subtitle_tracks), Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
        IconButton(onClick = close) { PilotIcon(R.drawable.ic_x, stringResource(R.string.close)) }
      }
      when {
        tracks == null || tracks.busy -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        tracks.error != null -> EmptyState(tracks.error, action = stringResource(R.string.retry), onAction = retry)
        else -> {
          val selection = if (audio) tracks.selectedAudio else tracks.selectedSubtitle
          val options = if (audio) tracks.audio else tracks.subtitles
          LazyColumn(Modifier.weight(1f), contentPadding = PaddingValues(bottom = 24.dp)) {
            item(key = "default") { DetailTrackRow(stringResource(R.string.playback_preferences_default), selection == null) { select(null) } }
            if (!audio) item(key = "off") { DetailTrackRow(stringResource(R.string.subtitles_off), selection == -1) { select(-1) } }
            items(options, key = { it.index }) { track ->
              DetailTrackRow(track.label, selection == track.index) { select(track.index) }
            }
            if (options.isEmpty()) item { Text(stringResource(R.string.no_tracks), Modifier.padding(16.dp), color = LocalPilotColors.current.metadata) }
          }
        }
      }
    }
  }
}

@Composable
private fun DetailTrackRow(label: String, selected: Boolean, select: () -> Unit) {
  TextButton(onClick = select, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp).semantics { this.selected = selected; role = Role.RadioButton }, shape = MaterialTheme.shapes.small,
    colors = ButtonDefaults.textButtonColors(containerColor = if (selected) MaterialTheme.colorScheme.primaryContainer else Color.Transparent)) {
    Text(label, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium, color = if (selected) MaterialTheme.colorScheme.onPrimaryContainer else MaterialTheme.colorScheme.onSurface)
    if (selected) PilotIcon(R.drawable.ic_check)
  }
}
