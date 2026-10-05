package io.github.hewel.jellypilot.ui

import android.icu.util.ULocale
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import io.github.hewel.jellypilot.R
import io.github.hewel.jellypilot.player.PlayerSnapshot
import io.github.hewel.jellypilot.player.PlayerStatus
import io.github.hewel.jellypilot.player.SubtitleTimingAvailability
import io.github.hewel.jellypilot.player.SubtitleTimingContext
import io.github.hewel.jellypilot.player.SubtitleTimingState
import io.github.hewel.jellypilot.player.TrackKind
import java.text.NumberFormat
import kotlin.math.abs
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@Composable
internal fun SubtitleTimingEntry(state: SubtitleTimingState, modifier: Modifier = Modifier, open: () -> Unit) {
  val available = state.availability == SubtitleTimingAvailability.AVAILABLE && state.context != null
  val value = subtitleTimingValue(state.offsetTenths)
  val title = stringResource(R.string.subtitle_timing_title)
  Column(Modifier.fillMaxWidth()) {
    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
    Row(modifier.fillMaxWidth().testTag("subtitle-timing-entry")
      .clip(RoundedCornerShape(12.dp)).clickable(enabled = available, role = Role.Button, onClick = open)
      .heightIn(min = 52.dp).padding(vertical = 4.dp),
      verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
      if (available && LocalConfiguration.current.fontScale < 1.5f) {
        Text(title, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
        Text(value, style = MaterialTheme.typography.bodyMedium, color = LocalPilotColors.current.metadata)
      } else {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
          Text(title, style = MaterialTheme.typography.bodyMedium)
          Text(if (available) value else subtitleTimingUnavailable(state.availability),
            style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.metadata)
        }
      }
      if (available) PilotIcon(R.drawable.ic_chevron_right, tint = LocalPilotColors.current.metadata)
    }
  }
}

@Composable
internal fun SubtitleTimingContent(snapshot: PlayerSnapshot, apply: (SubtitleTimingContext, Int) -> Boolean, modifier: Modifier = Modifier) {
  val state = snapshot.subtitleTiming
  val locale = LocalConfiguration.current.locales[0]
  val track = snapshot.tracks.firstOrNull { it.kind == TrackKind.SUBTITLE && it.mpvId == state.context?.trackId }
  val language = remember(track?.language, locale) {
    track?.language?.trim()?.takeIf(String::isNotEmpty)?.let {
      ULocale.createCanonical(it.replace('-', '_')).getDisplayName(ULocale.forLocale(locale))
    }
  }
  val trackName = track?.let {
    listOfNotNull(it.title?.trim()?.takeIf(String::isNotEmpty) ?: language
      ?: stringResource(R.string.player_track_number, it.mpvId), it.codec?.takeIf(String::isNotBlank)).joinToString(" · ")
  }
  val foreground = subtitleTimingForeground()
  val available = state.availability == SubtitleTimingAvailability.AVAILABLE && state.context != null &&
    (snapshot.status == PlayerStatus.READY || snapshot.status == PlayerStatus.BUFFERING)
  // The host reserves synchronously; StateFlow may conflate its pending and settled publications.
  // Keep controls disabled until the settlement sequence changes, including repeated failures.
  var reservedRevision by remember(state.context) { mutableStateOf<Long?>(null) }
  val busy = state.pending || reservedRevision == state.requestRevision
  var heldDirections by remember(state.context) { mutableStateOf(emptySet<Int>()) }
  val value = subtitleTimingValue(state.offsetTenths)
  val description = when {
    state.offsetTenths > 0 -> stringResource(R.string.subtitle_timing_delayed, subtitleTimingNumber(state.offsetTenths))
    state.offsetTenths < 0 -> stringResource(R.string.subtitle_timing_advanced, subtitleTimingNumber(abs(state.offsetTenths)))
    else -> stringResource(R.string.subtitle_timing_unadjusted)
  }
  var announcement by remember(state.context) { mutableStateOf(description) }
  LaunchedEffect(description, heldDirections.isEmpty(), busy) {
    if (heldDirections.isEmpty() && !busy) announcement = description
  }
  val failure = stringResource(R.string.subtitle_timing_failure)
  val resetLabel = stringResource(R.string.subtitle_timing_reset_accessibility)
  val currentApply by rememberUpdatedState(apply)
  val currentState by rememberUpdatedState(state)
  val canApply by rememberUpdatedState(available && !busy && foreground)
  val request: (Int) -> Boolean = { target ->
    val current = currentState
    val context = current.context
    if (!canApply || context == null || target == current.offsetTenths) false
    else currentApply(context, target.coerceIn(-100, 100)).also {
      if (it) reservedRevision = current.requestRevision
    }
  }
  Column(modifier.fillMaxWidth().padding(top = 8.dp),
    horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
    trackName?.let {
      Text(it, Modifier.fillMaxWidth(), style = MaterialTheme.typography.bodySmall.copy(fontSize = 12.sp, lineHeight = 16.sp),
        textAlign = TextAlign.Center, color = LocalPilotColors.current.metadata)
    }
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp)) {
      Text(value, Modifier.fillMaxWidth().testTag("subtitle-timing-value").semantics { stateDescription = value },
        style = MaterialTheme.typography.headlineMedium.copy(fontSize = 28.sp, lineHeight = 36.sp,
          fontWeight = FontWeight.SemiBold, fontFeatureSettings = "tnum"),
        textAlign = TextAlign.Center)
      Text(description, Modifier.fillMaxWidth().clearAndSetSemantics {
        contentDescription = announcement
        liveRegion = LiveRegionMode.Polite
      }, style = MaterialTheme.typography.bodySmall.copy(fontSize = 12.sp, lineHeight = 20.sp),
        textAlign = TextAlign.Center, color = LocalPilotColors.current.metadata)
    }
    if (!available) Text(subtitleTimingUnavailable(state.availability), Modifier.fillMaxWidth(),
      style = MaterialTheme.typography.bodySmall, textAlign = TextAlign.Center, color = LocalPilotColors.current.metadata)
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
      listOf(-1, 1).forEach { direction ->
        val inBounds = if (direction < 0) state.offsetTenths > -100 else state.offsetTenths < 100
        SubtitleTimingRepeatButton(
          text = stringResource(if (direction < 0) R.string.subtitle_timing_earlier else R.string.subtitle_timing_later),
          label = stringResource(if (direction < 0) R.string.subtitle_timing_earlier_accessibility else R.string.subtitle_timing_later_accessibility),
          value = if (busy) stringResource(R.string.subtitle_timing_pending) else value,
          context = state.context, active = available && foreground && inBounds,
          enabled = available && foreground && inBounds && !busy,
          cancelRevision = state.requestRevision.takeIf { state.failed },
          modifier = Modifier.weight(1f),
          holding = { heldDirections = if (it) heldDirections + direction else heldDirections - direction },
          adjust = { request(currentState.offsetTenths + direction) },
        )
      }
    }
    Text(stringResource(R.string.subtitle_timing_help), Modifier.fillMaxWidth(),
      style = MaterialTheme.typography.bodySmall.copy(fontSize = 12.sp, lineHeight = 16.sp),
      textAlign = TextAlign.Center, color = LocalPilotColors.current.metadata)
    if (state.failed) Text(failure, Modifier.fillMaxWidth().semantics { liveRegion = LiveRegionMode.Polite },
      style = MaterialTheme.typography.bodySmall, textAlign = TextAlign.Center, color = MaterialTheme.colorScheme.error)
    TextButton(onClick = { request(0) }, enabled = available && foreground && !busy && state.offsetTenths != 0,
      colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.onSurface,
        disabledContentColor = LocalPilotColors.current.metadata),
      modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).semantics {
        contentDescription = resetLabel
      }) {
      Text(stringResource(R.string.subtitle_timing_reset))
    }
  }
}

/** A held control survives pending acknowledgments, but never a selection or lifecycle boundary. */
@Composable
private fun SubtitleTimingRepeatButton(
  text: String, label: String, value: String, context: SubtitleTimingContext?, active: Boolean,
  enabled: Boolean, cancelRevision: Long?, modifier: Modifier, holding: (Boolean) -> Unit, adjust: () -> Boolean,
) {
  val scope = rememberCoroutineScope()
  var job by remember { mutableStateOf<Job?>(null) }
  var focused by remember { mutableStateOf(false) }
  val ready by rememberUpdatedState(enabled)
  val currentContext by rememberUpdatedState(context)
  val currentAdjust by rememberUpdatedState(adjust)
  val currentHolding by rememberUpdatedState(holding)
  fun stop() {
    job?.cancel()
    job = null
    currentHolding(false)
  }
  fun start() {
    if (job != null || !ready || !currentAdjust()) return
    val selection = context
    currentHolding(true)
    job = scope.launch {
      delay(400)
      while (true) {
        if (currentContext != selection || (ready && !currentAdjust())) {
          currentHolding(false)
          job = null
          break
        }
        delay(100)
      }
    }
  }
  val currentStart by rememberUpdatedState(::start)
  val currentStop by rememberUpdatedState(::stop)
  DisposableEffect(context, active) { onDispose { stop() } }
  LaunchedEffect(cancelRevision) { if (cancelRevision != null) stop() }
  Box(modifier.clip(RoundedCornerShape(12.dp)).background(PilotPlayerTokens.phonePanelSelected)
    .semantics(mergeDescendants = true) {
      role = Role.Button
      contentDescription = label
      stateDescription = value
      if (!enabled) disabled()
      onClick { if (ready) currentAdjust() else false }
    }
    .onFocusChanged {
      if (focused && !it.isFocused) stop()
      focused = it.isFocused
    }
    .onKeyEvent {
      if (it.key == Key.Enter || it.key == Key.NumPadEnter || it.key == Key.DirectionCenter || it.key == Key.Spacebar) {
        if (it.type == KeyEventType.KeyDown && it.nativeKeyEvent.repeatCount == 0) start()
        else if (it.type == KeyEventType.KeyUp) stop()
        true
      } else false
    }.focusable(enabled = active)
    // Keep pointer ownership until release; context and lifecycle effects only cancel the repeat.
    .pointerInput(Unit) {
      awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false)
        down.consume()
        currentStart()
        try { waitForUpOrCancellation()?.consume() } finally { currentStop() }
      }
    }.heightIn(min = 48.dp).padding(horizontal = 8.dp, vertical = 12.dp), contentAlignment = Alignment.Center) {
    Text(text, style = MaterialTheme.typography.labelLarge.copy(fontSize = 14.sp, lineHeight = 20.sp),
      textAlign = TextAlign.Center,
      color = if (enabled) MaterialTheme.colorScheme.onSurface else LocalPilotColors.current.metadata)
  }
}

@Composable
private fun subtitleTimingForeground(): Boolean {
  val lifecycle = LocalLifecycleOwner.current.lifecycle
  var resumed by remember(lifecycle) { mutableStateOf(lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) }
  DisposableEffect(lifecycle) {
    val observer = LifecycleEventObserver { _, _ -> resumed = lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED) }
    lifecycle.addObserver(observer)
    onDispose { lifecycle.removeObserver(observer) }
  }
  return resumed && LocalWindowInfo.current.isWindowFocused
}

@Composable
private fun subtitleTimingUnavailable(availability: SubtitleTimingAvailability): String = stringResource(when (availability) {
  SubtitleTimingAvailability.NO_TRACKS -> R.string.subtitle_timing_no_tracks
  SubtitleTimingAvailability.SUBTITLES_OFF -> R.string.subtitle_timing_off
  SubtitleTimingAvailability.UNSUPPORTED, SubtitleTimingAvailability.AVAILABLE -> R.string.subtitle_timing_unsupported
})

@Composable
private fun subtitleTimingNumber(offsetTenths: Int): String {
  val locale = LocalConfiguration.current.locales[0]
  return remember(offsetTenths, locale) {
    NumberFormat.getNumberInstance(locale).apply {
      minimumFractionDigits = 1
      maximumFractionDigits = 1
    }.format(offsetTenths / 10.0)
  }
}

@Composable
private fun subtitleTimingValue(offsetTenths: Int): String {
  val number = subtitleTimingNumber(offsetTenths)
  return stringResource(R.string.subtitle_timing_seconds, if (offsetTenths > 0) "+$number" else number)
}
