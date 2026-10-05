package io.github.hewel.jellypilot.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.unit.dp
import io.github.hewel.jellypilot.R
import io.github.hewel.jellypilot.ffi.PlaybackStartPosition
import io.github.hewel.jellypilot.ffi.RemoteControlCommand
import io.github.hewel.jellypilot.ffi.RemoteControlSnapshot
import io.github.hewel.jellypilot.ffi.RemoteControlTarget
import io.github.hewel.jellypilot.ffi.RemoteControlTargetKey
import io.github.hewel.jellypilot.ffi.RemoteControllerStatus
import kotlin.math.roundToInt

internal data class RemoteControllerUiState(
  val snapshot: RemoteControlSnapshot? = null,
  val loading: Boolean = false,
  val failed: Boolean = false,
  val commandFailed: Boolean = false,
  val accepted: Boolean = false,
  val pendingPlay: RemotePlayItem? = null,
  val serverName: String = "",
)

internal data class RemotePlayItem(val itemId: String, val title: String, val position: PlaybackStartPosition)

@Composable
internal fun RemoteControlScreen(
  state: RemoteControllerUiState,
  onBack: () -> Unit,
  onRefresh: () -> Unit,
  onSelect: (RemoteControlTargetKey) -> Unit,
  onCommand: (ULong, RemoteControlTargetKey, RemoteControlCommand) -> Unit,
  onPlay: (ULong, RemoteControlTargetKey) -> Unit,
) {
  val snapshot = state.snapshot
  val loading = state.loading || snapshot?.status == RemoteControllerStatus.LOADING
  val failed = state.failed || snapshot?.status == RemoteControllerStatus.FAILED || snapshot?.error != null
  val ready = !loading && !failed && snapshot?.status == RemoteControllerStatus.READY
  val pending = snapshot?.commandPending == true
  val refreshing = loading || snapshot?.refreshing == true
  val selected = snapshot?.targets?.firstOrNull { it.key == snapshot.selected }
  Box(Modifier.fillMaxSize().safeDrawingPadding(), contentAlignment = Alignment.TopCenter) {
    Column(Modifier.widthIn(max = 640.dp).fillMaxSize()) {
      Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        IconButton(onClick = onBack, modifier = Modifier.size(48.dp)) {
          PilotIcon(R.drawable.ic_chevron_left, stringResource(R.string.back))
        }
        Column(Modifier.weight(1f).padding(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
          Text(selected?.deviceName ?: stringResource(R.string.remote_control), style = MaterialTheme.typography.sectionHeading)
          if (state.serverName.isNotBlank()) Text(state.serverName, style = MaterialTheme.typography.bodyMedium,
            color = LocalPilotColors.current.metadata)
        }
        TextButton(onClick = onRefresh, enabled = !refreshing && !pending, modifier = Modifier.heightIn(min = 48.dp)) {
          Text(stringResource(R.string.refresh))
        }
      }
      Column(Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(24.dp)) {
        when {
          loading -> Text(stringResource(R.string.remote_loading), Modifier.semantics { liveRegion = LiveRegionMode.Polite },
            color = LocalPilotColors.current.metadata)
          failed -> Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.remote_load_failed), color = MaterialTheme.colorScheme.error,
              modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
            FilledTonalButton(onClick = onRefresh, enabled = !refreshing && !pending, modifier = Modifier.heightIn(min = 48.dp)) {
              Text(stringResource(R.string.retry))
            }
          }
          !ready -> Text(stringResource(R.string.remote_inactive), color = LocalPilotColors.current.metadata)
          snapshot.targets.isEmpty() -> Text(stringResource(R.string.remote_empty), color = LocalPilotColors.current.metadata)
        }
        if (!snapshot?.targets.isNullOrEmpty()) {
          Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(stringResource(R.string.remote_devices), Modifier.semantics { heading() }, style = MaterialTheme.typography.titleMedium)
            AccountGroup {
              Column(Modifier.selectableGroup()) {
                snapshot.targets.forEachIndexed { index, target ->
                  if (index > 0) AccountDivider()
                  RemoteTargetRow(target, selected?.key == target.key, ready && !pending) { onSelect(target.key) }
                }
              }
            }
          }
        }
        state.pendingPlay?.let { item ->
          Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(item.title, style = MaterialTheme.typography.titleLarge)
            Text(when (val position = item.position) {
              PlaybackStartPosition.Beginning -> stringResource(R.string.play_from_start)
              PlaybackStartPosition.Resume -> stringResource(R.string.remote_resume_saved)
              is PlaybackStartPosition.At -> stringResource(R.string.remote_start_at, playbackClock(position.seconds))
            }, color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodyMedium)
            if (selected == null) {
              Text(stringResource(R.string.remote_choose_play_target), color = LocalPilotColors.current.metadata)
            } else {
              Button(onClick = { onPlay(snapshot.generation, selected.key) },
                enabled = ready && !pending && selected.capabilities.canPlayNow,
                modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) {
                Text(stringResource(R.string.remote_play_on, selected.deviceName))
              }
              if (!selected.capabilities.canPlayNow) Text(stringResource(R.string.remote_play_unsupported),
                color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodyMedium)
            }
          }
        }
        if (pending || state.commandFailed || state.accepted) {
          val message = when {
            pending -> R.string.remote_command_pending
            state.commandFailed -> R.string.remote_command_failed
            else -> R.string.remote_command_accepted
          }
          Text(stringResource(message), Modifier.semantics { liveRegion = LiveRegionMode.Polite },
            color = if (state.commandFailed && !pending) MaterialTheme.colorScheme.error else LocalPilotColors.current.metadata,
            style = MaterialTheme.typography.bodyMedium)
        }
        if (ready && selected != null) {
          RemoteTargetControls(snapshot.generation, selected, !pending, onCommand)
        } else if (ready && selected == null && snapshot.targets.isNotEmpty() && state.pendingPlay == null) {
          Text(stringResource(R.string.remote_choose_target), color = LocalPilotColors.current.metadata)
        }
      }
    }
  }
}

@Composable
private fun RemoteTargetRow(target: RemoteControlTarget, selected: Boolean, enabled: Boolean, onSelect: () -> Unit) {
  Row(Modifier.fillMaxWidth().background(if (selected) MaterialTheme.colorScheme.primaryContainer else LocalPilotColors.current.accountSurface)
    .selectable(selected, enabled = enabled, role = Role.RadioButton, onClick = onSelect)
    .heightIn(min = 64.dp).padding(horizontal = 16.dp, vertical = 12.dp),
    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
      Text(target.deviceName, style = MaterialTheme.typography.titleMedium,
        color = if (selected) MaterialTheme.colorScheme.onPrimaryContainer else LocalPilotColors.current.body)
      val identity = listOfNotNull(target.clientName.takeIf { it.isNotBlank() }, target.userName?.takeIf { it.isNotBlank() })
      if (identity.isNotEmpty()) Text(identity.joinToString(" · "), style = MaterialTheme.typography.bodyMedium,
        color = if (selected) MaterialTheme.colorScheme.onPrimaryContainer else LocalPilotColors.current.insetMetadata)
    }
    RadioButton(selected = selected, onClick = null, enabled = enabled)
  }
}

@Composable
private fun RemoteTargetControls(
  generation: ULong,
  target: RemoteControlTarget,
  enabled: Boolean,
  onCommand: (ULong, RemoteControlTargetKey, RemoteControlCommand) -> Unit,
) {
  val playing = target.nowPlaying
  val capabilities = target.capabilities
  val origin = RemoteCommandOrigin(generation, target.key, playing?.itemId)
  Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
    Text(stringResource(R.string.remote_now_playing_on, target.deviceName), Modifier.semantics { heading() },
      style = MaterialTheme.typography.titleMedium)
    if (playing == null) {
      Text(stringResource(R.string.remote_no_media), color = LocalPilotColors.current.metadata)
    } else {
      Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(playing.title, style = MaterialTheme.typography.titleLarge)
        Text(stringResource(when (playing.paused) {
          true -> R.string.remote_paused
          false -> R.string.remote_playing
          null -> R.string.remote_state_unknown
        }), color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodyMedium)
      }
      Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (playing.paused == false && capabilities.canPause) {
          FilledTonalButton(onClick = { onCommand(generation, target.key, RemoteControlCommand.Pause) }, enabled = enabled,
            modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(stringResource(R.string.pause)) }
        }
        if (playing.paused == true && capabilities.canResume) {
          FilledTonalButton(onClick = { onCommand(generation, target.key, RemoteControlCommand.Resume) }, enabled = enabled,
            modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(stringResource(R.string.remote_resume)) }
        }
      }
      val position = playing.positionSeconds?.takeIf { it.isFinite() && it >= 0.0 }
      val duration = playing.durationSeconds?.takeIf { it.isFinite() && it > 0.0 && it.toFloat().isFinite() }
      if (position != null && duration != null) {
        RemoteControlSlider(origin, position.coerceAtMost(duration).toFloat(), 0f..duration.toFloat(),
          enabled && capabilities.canSeek, stringResource(R.string.seek),
          valueLabel = { stringResource(R.string.remote_position_of_duration, playbackClock(it.toDouble()), playbackClock(duration)) },
          onCommit = { source, value -> onCommand(source.generation, source.key, RemoteControlCommand.Seek(value.toDouble())) })
      } else {
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
          Text(stringResource(R.string.seek), style = MaterialTheme.typography.titleSmall)
          Text(stringResource(R.string.remote_position_of_duration, position?.let(::playbackClock) ?: "—",
            duration?.let(::playbackClock) ?: "—"), color = LocalPilotColors.current.metadata,
            style = MaterialTheme.typography.bodyMedium)
        }
      }
    }
    if (capabilities.canStop) {
      OutlinedButton(onClick = { onCommand(generation, target.key, RemoteControlCommand.Stop) }, enabled = enabled,
        modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(stringResource(R.string.stop)) }
    }
    if (capabilities.canSetVolume) {
      val volume = target.volume?.takeIf { it <= 100u }
      if (volume != null) {
        RemoteControlSlider(origin, volume.toFloat(), 0f..100f, enabled, stringResource(R.string.volume),
          valueLabel = { stringResource(R.string.remote_volume_percent, it.roundToInt()) },
          onCommit = { source, value -> onCommand(source.generation, source.key, RemoteControlCommand.SetVolume(value.roundToInt().toUInt())) })
      } else {
        Text(stringResource(R.string.remote_volume_unavailable), color = LocalPilotColors.current.metadata,
          style = MaterialTheme.typography.bodyMedium)
        Text("—", style = MaterialTheme.typography.bodyMedium)
      }
    }
  }
}

private data class RemoteCommandOrigin(val generation: ULong, val key: RemoteControlTargetKey, val itemId: String?)
private data class RemoteSliderDrag(val origin: RemoteCommandOrigin, val value: Float, val changed: Boolean = false)

@Composable
private fun RemoteControlSlider(
  origin: RemoteCommandOrigin,
  value: Float,
  range: ClosedFloatingPointRange<Float>,
  enabled: Boolean,
  label: String,
  valueLabel: @Composable (Float) -> String,
  onCommit: (RemoteCommandOrigin, Float) -> Unit,
) {
  // Polling changes the confirmed value, never an in-progress preview or its command identity.
  var drag by remember { mutableStateOf<RemoteSliderDrag?>(null) }
  val currentOrigin by rememberUpdatedState(origin)
  val currentValue by rememberUpdatedState(value)
  val currentEnabled by rememberUpdatedState(enabled)
  val preview = drag?.takeIf { it.origin == origin }?.value ?: value
  Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
    Text(label, style = MaterialTheme.typography.titleSmall)
    Text(valueLabel(preview), style = MaterialTheme.typography.bodyMedium, color = LocalPilotColors.current.metadata)
    Slider(value = preview.coerceIn(range), valueRange = range, enabled = enabled,
      onValueChange = { next ->
        if (enabled) drag = drag?.copy(value = next, changed = true) ?: RemoteSliderDrag(origin, next, changed = true)
      },
      onValueChangeFinished = {
        val finished = drag
        drag = null
        if (enabled && finished != null && finished.changed && finished.origin == origin) {
          onCommit(finished.origin, finished.value.coerceIn(range))
        }
      },
      modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)
        .pointerInput(Unit) {
          // Capture contact before Material's first value change, including a stationary hold.
          var touching = false
          try {
            awaitPointerEventScope {
              while (true) {
                val pressed = awaitPointerEvent(PointerEventPass.Initial).changes.any { it.pressed }
                if (pressed && !touching && currentEnabled) drag = RemoteSliderDrag(currentOrigin, currentValue)
                touching = pressed
              }
            }
          } finally { drag = null }
        }
        .semantics { contentDescription = label })
  }
}
