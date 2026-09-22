@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class, androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package io.github.hewel.jellypilot.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsDraggedAsState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.hewel.jellypilot.R
import io.github.hewel.jellypilot.player.PlayerSnapshot
import io.github.hewel.jellypilot.player.PlayerStatus
import kotlin.math.ceil

@Composable
internal fun PlayerFullControls(
  snapshot: PlayerSnapshot,
  playback: PlaybackUi?,
  ready: Boolean,
  back: () -> Unit,
  playPause: () -> Unit,
  seek: (Double) -> Unit,
  volume: (Int) -> Unit,
  autoSkip: (Boolean) -> Unit,
  dragging: (Boolean) -> Unit,
  bottomHeight: (Dp) -> Unit,
  phone: Boolean = false,
  more: @Composable () -> Unit = {},
  panels: @Composable () -> Unit,
) {
  if (phone) {
    PhonePlayerFullControls(snapshot, playback, ready, back, playPause, seek, dragging, bottomHeight, more, panels)
    return
  }
  val colors = PilotPlayerTokens
  Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.displayCutout)) {
    Row(
      Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 16.dp),
      horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically,
    ) {
      IconButton(onClick = back, modifier = Modifier.size(colors.touchTarget)) {
        Box(Modifier.size(44.dp).background(colors.back, CircleShape), contentAlignment = Alignment.Center) {
          PilotIcon(R.drawable.ic_chevron_left, stringResource(R.string.close_player), Modifier.size(14.dp), colors.foreground)
        }
      }
      Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(playback?.title ?: stringResource(R.string.player), style = MaterialTheme.typography.titleSmall,
          maxLines = 1, overflow = TextOverflow.Ellipsis, color = colors.foreground)
        playback?.episodeLabel?.let {
          Text(it, style = MaterialTheme.typography.labelMedium, color = colors.secondary,
            maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
      }
      if (playback?.autoSkipAvailable == true) PlayerSessionToggle(playback.autoSkipEnabled, autoSkip)
    }
    val density = LocalDensity.current
    BoxWithConstraints(
      Modifier.align(Alignment.BottomCenter).fillMaxWidth().padding(horizontal = 24.dp)
        .onSizeChanged { with(density) { bottomHeight(it.height.toDp()) } },
    ) {
      val wide = maxWidth >= 640.dp
      Column(Modifier.fillMaxWidth().padding(bottom = 12.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.CenterVertically) {
          Text(playback?.episodeLabel ?: playback?.title.orEmpty(), Modifier.weight(1f),
            style = MaterialTheme.typography.labelMedium, color = colors.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
          snapshot.durationSeconds?.takeIf { it.isFinite() && it > 0 }?.let { duration ->
            Text(stringResource(R.string.remaining_minutes, ceil((duration - snapshot.positionSeconds).coerceAtLeast(0.0) / 60).toInt()),
              style = MaterialTheme.typography.labelMedium, color = colors.metadata)
          }
        }
        Spacer(Modifier.height(10.dp))
        PlayerTimeline(snapshot, ready, dragging, seek)
        Spacer(Modifier.height(14.dp))
        if (wide) {
          Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween) {
            PlayerVolume(snapshot, ready, volume, dragging, Modifier.width(204.dp))
            PlayerTransport(snapshot, ready, playPause, seek)
            Row(Modifier.width(204.dp), horizontalArrangement = Arrangement.spacedBy(4.dp, Alignment.End), verticalAlignment = Alignment.CenterVertically) { panels() }
          }
        } else {
          Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center) {
            PlayerTransport(snapshot, ready, playPause, seek)
          }
          Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            PlayerVolume(snapshot, ready, volume, dragging, Modifier.weight(1f))
            Row(verticalAlignment = Alignment.CenterVertically) { panels() }
          }
        }
      }
    }
  }
}

@Composable
private fun PhonePlayerFullControls(
  snapshot: PlayerSnapshot, playback: PlaybackUi?, ready: Boolean, back: () -> Unit,
  playPause: () -> Unit, seek: (Double) -> Unit, dragging: (Boolean) -> Unit,
  bottomHeight: (Dp) -> Unit, settings: @Composable () -> Unit,
  panels: @Composable () -> Unit,
) {
  val density = LocalDensity.current
  BoxWithConstraints(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.displayCutout)) {
    val reflow = maxWidth < 560.dp || maxHeight < 320.dp || density.fontScale > 1.3f
    val top: @Composable () -> Unit = {
      Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically) {
        IconButton(onClick = back, modifier = Modifier.size(48.dp)) {
          PilotIcon(R.drawable.ic_chevron_left, stringResource(R.string.close_player), tint = PilotPlayerTokens.foreground)
        }
        settings()
      }
    }
    val bottom: @Composable () -> Unit = {
      Column(Modifier.fillMaxWidth().padding(bottom = 20.dp)) {
        val identity: @Composable (Modifier) -> Unit = { modifier ->
          Column(modifier, verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(playback?.title ?: stringResource(R.string.player),
              style = MaterialTheme.typography.titleMedium, color = PilotPlayerTokens.foreground,
              maxLines = 1, overflow = TextOverflow.Ellipsis)
            playback?.episodeLabel?.let {
              Text(it, style = MaterialTheme.typography.labelMedium, color = PilotPlayerTokens.secondary,
                maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
          }
        }
        if (reflow) {
          identity(Modifier.fillMaxWidth())
          FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) { panels() }
        } else Row(Modifier.fillMaxWidth().heightIn(min = 48.dp), horizontalArrangement = Arrangement.spacedBy(16.dp),
          verticalAlignment = Alignment.CenterVertically) {
          identity(Modifier.weight(1f))
          Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) { panels() }
        }
        PlayerTimeline(snapshot, ready, dragging, seek, phone = true)
      }
    }
    if (reflow) Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 24.dp, vertical = 16.dp),
      horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(16.dp)) {
      top()
      PhoneTransport(snapshot, ready, playPause, seek)
      bottom()
    } else {
      Box(Modifier.align(Alignment.TopCenter).padding(horizontal = 24.dp, vertical = 16.dp)) { top() }
      Box(Modifier.align(Alignment.Center).offset(y = (-12).dp)) { PhoneTransport(snapshot, ready, playPause, seek) }
      Box(Modifier.align(Alignment.BottomCenter).fillMaxWidth().padding(horizontal = 24.dp)
        .onSizeChanged { with(density) { bottomHeight(it.height.toDp()) } }) { bottom() }
    }
  }
}

@Composable
private fun PhoneTransport(snapshot: PlayerSnapshot, ready: Boolean, playPause: () -> Unit, seek: (Double) -> Unit) {
  val enabled = ready && snapshot.status != PlayerStatus.IDLE && snapshot.status != PlayerStatus.LOADING
  val tint = if (enabled) PilotPlayerTokens.foreground else PilotPlayerTokens.disabled
  Row(horizontalArrangement = Arrangement.spacedBy(40.dp), verticalAlignment = Alignment.CenterVertically) {
    IconButton(onClick = { seek((snapshot.positionSeconds - 10).coerceAtLeast(0.0)) }, enabled = enabled,
      modifier = Modifier.size(56.dp)) {
      PilotIcon(R.drawable.ic_player_rewind_ten, stringResource(R.string.rewind_ten), Modifier.size(32.dp), tint)
    }
    IconButton(onClick = playPause, enabled = enabled, modifier = Modifier.size(72.dp)) {
      PilotIcon(if (snapshot.paused) R.drawable.ic_player_play else R.drawable.ic_player_pause,
        stringResource(if (snapshot.paused) R.string.play else R.string.pause), Modifier.size(40.dp), tint)
    }
    IconButton(onClick = { seek(snapshot.durationSeconds?.let { (snapshot.positionSeconds + 10).coerceAtMost(it) }
      ?: snapshot.positionSeconds + 10) }, enabled = enabled, modifier = Modifier.size(56.dp)) {
      PilotIcon(R.drawable.ic_player_forward_ten, stringResource(R.string.forward_ten), Modifier.size(32.dp), tint)
    }
  }
}

@Composable
internal fun PlayerSessionToggle(checked: Boolean, change: (Boolean) -> Unit) {
  val colors = PilotPlayerTokens
  Row(
    Modifier.clip(CircleShape).background(colors.chip)
      .toggleable(value = checked, role = Role.Switch, onValueChange = change)
      .heightIn(min = colors.touchTarget).padding(horizontal = 12.dp, vertical = 8.dp),
    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
  ) {
    Text(stringResource(R.string.session_auto_skip), color = colors.foreground, style = MaterialTheme.typography.labelMedium)
    Box(Modifier.size(38.dp, 22.dp).background(if (checked) LocalPilotColors.current.action else colors.disabled, CircleShape).padding(2.dp)) {
      Box(Modifier.align(if (checked) Alignment.CenterEnd else Alignment.CenterStart).size(18.dp).background(colors.foreground, CircleShape))
    }
  }
}

@Composable
private fun PlayerTransport(snapshot: PlayerSnapshot, ready: Boolean, playPause: () -> Unit, seek: (Double) -> Unit) {
  val enabled = ready && snapshot.status != PlayerStatus.IDLE && snapshot.status != PlayerStatus.LOADING
  Row(horizontalArrangement = Arrangement.spacedBy(20.dp), verticalAlignment = Alignment.CenterVertically) {
    IconButton(onClick = { seek((snapshot.positionSeconds - 10).coerceAtLeast(0.0)) }, enabled = enabled, modifier = Modifier.size(48.dp)) {
      PilotIcon(R.drawable.ic_player_rewind_ten, stringResource(R.string.rewind_ten), tint = if (enabled) PilotPlayerTokens.foreground else PilotPlayerTokens.disabled)
    }
    FilledIconButton(onClick = playPause, enabled = enabled, modifier = Modifier.size(56.dp),
      colors = IconButtonDefaults.filledIconButtonColors(containerColor = PilotPlayerTokens.play, contentColor = PilotPlayerTokens.foreground)) {
      PilotIcon(if (snapshot.paused) R.drawable.ic_player_play else R.drawable.ic_player_pause,
        stringResource(if (snapshot.paused) R.string.play else R.string.pause), Modifier.size(22.dp))
    }
    IconButton(onClick = { seek(snapshot.durationSeconds?.let { (snapshot.positionSeconds + 10).coerceAtMost(it) } ?: (snapshot.positionSeconds + 10)) },
      enabled = enabled, modifier = Modifier.size(48.dp)) {
      PilotIcon(R.drawable.ic_player_forward_ten, stringResource(R.string.forward_ten), tint = if (enabled) PilotPlayerTokens.foreground else PilotPlayerTokens.disabled)
    }
  }
}

@Composable
private fun PlayerVolume(snapshot: PlayerSnapshot, ready: Boolean, change: (Int) -> Unit, dragging: (Boolean) -> Unit, modifier: Modifier = Modifier) {
  Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
    Box(Modifier.size(48.dp), contentAlignment = Alignment.Center) {
      PilotIcon(when {
        snapshot.muted || snapshot.volumePercent == 0 -> R.drawable.ic_volume_muted
        snapshot.volumePercent <= 33 -> R.drawable.ic_volume
        snapshot.volumePercent <= 66 -> R.drawable.ic_volume_medium
        else -> R.drawable.ic_volume_loud
      }, tint = PilotPlayerTokens.secondary)
    }
    PlayerSlider(snapshot.volumePercent.toFloat(), { change(it.toInt()); dragging(true) },
      { dragging(false) }, 0f..100f, ready, stringResource(R.string.volume),
      Modifier.widthIn(max = 96.dp).weight(1f, fill = false), showThumb = false)
  }
}

@Composable
internal fun PlayerTimeline(
  snapshot: PlayerSnapshot,
  ready: Boolean,
  dragging: (Boolean) -> Unit,
  seek: (Double) -> Unit,
  phone: Boolean = false,
) {
  var position by remember(snapshot.generation) { mutableStateOf<Float?>(null) }
  val duration = snapshot.durationSeconds?.takeIf { it > 0 && it.isFinite() }
  val current = position?.toDouble() ?: snapshot.positionSeconds
  DisposableEffect(Unit) { onDispose { dragging(false) } }
  PlayerSlider(
    value = duration?.let { (position ?: snapshot.positionSeconds.toFloat()).coerceIn(0f, it.toFloat()) } ?: 0f,
    change = { position = it; dragging(true) },
    finished = { position?.let { seek(it.toDouble()) }; position = null; dragging(false) },
    range = 0f..(duration?.toFloat() ?: 1f), enabled = ready && duration != null && snapshot.status != PlayerStatus.LOADING,
    label = stringResource(R.string.seek), modifier = Modifier.fillMaxWidth(),
    interacting = dragging,
    activeColor = if (phone) PilotPlayerTokens.foreground else null,
    inactiveColor = if (phone) PilotPlayerTokens.phoneRail else null,
    thumbSize = if (phone) 12.dp else 14.dp,
  )
  TimelineTimes(current, duration, phone)
}

@Composable
private fun TimelineTimes(current: Double, duration: Double?, phone: Boolean) {
  Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
    val timeStyle = if (phone) MaterialTheme.typography.labelMedium.copy(fontSize = 12.sp, lineHeight = 20.sp, fontFeatureSettings = "tnum")
      else MaterialTheme.typography.labelMedium.copy(fontFamily = FontFamily.Monospace)
    Text(playbackClock(current), style = timeStyle, color = PilotPlayerTokens.timecode)
    Text(duration?.let { "−" + playbackClock((it - current).coerceAtLeast(0.0)) } ?: "—", style = timeStyle, color = PilotPlayerTokens.timecode)
  }
}

// Keep Material's touch/keyboard/accessibility behavior; replace only the visual track and thumb.
@Composable
internal fun PlayerSlider(
  value: Float,
  change: (Float) -> Unit,
  finished: () -> Unit,
  range: ClosedFloatingPointRange<Float>,
  enabled: Boolean,
  label: String,
  modifier: Modifier = Modifier,
  showThumb: Boolean = true,
  interacting: (Boolean) -> Unit = {},
  activeColor: Color? = null,
  inactiveColor: Color? = null,
  thumbSize: Dp = 14.dp,
) {
  val interactionSource = remember { MutableInteractionSource() }
  var touching by remember { mutableStateOf(false) }
  val dragged by interactionSource.collectIsDraggedAsState()
  val currentInteracting by rememberUpdatedState(interacting)
  LaunchedEffect(touching, dragged) { currentInteracting(touching || dragged) }
  DisposableEffect(Unit) { onDispose { currentInteracting(false) } }
  val active = activeColor ?: if (showThumb) LocalPilotColors.current.action else PilotPlayerTokens.foreground
  Slider(value = value, onValueChange = change, onValueChangeFinished = finished, valueRange = range, enabled = enabled,
    interactionSource = interactionSource,
    modifier = modifier.heightIn(min = PilotPlayerTokens.touchTarget)
      .pointerInput(enabled) {
        // Material3 1.4 omits PressInteraction before a drag (b/308501482).
        // Observe contact without consuming or replacing the Slider's seek input.
        try {
          awaitPointerEventScope {
            while (true) touching = awaitPointerEvent(PointerEventPass.Initial).changes.any { enabled && it.pressed }
          }
        } finally { touching = false }
      }
      .semantics { contentDescription = label },
    thumb = {
      Box(Modifier.size(if (showThumb) thumbSize else 0.dp).alpha(if (enabled) 1f else 0.3f)
        .background(PilotPlayerTokens.foreground, CircleShape))
    },
    track = { state ->
      val fraction = ((state.value - range.start) / (range.endInclusive - range.start)).coerceIn(0f, 1f)
      Box(Modifier.fillMaxWidth().height(PilotPlayerTokens.trackHeight).clip(CircleShape).background(inactiveColor ?: PilotPlayerTokens.rail)) {
        Box(Modifier.fillMaxHeight().fillMaxWidth(fraction).background(active.copy(alpha = if (enabled) 1f else 0.3f)))
      }
    },
  )
}
