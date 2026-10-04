package io.github.hewel.jellypilot.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.input.pointer.*
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.hewel.jellypilot.R
import kotlin.math.abs
import kotlin.math.roundToInt

/** Kept below controls as a sibling: a contact that starts on a control never reaches this node. */
@Composable
internal fun PlayerGestureSurface(
  generation: Long,
  enabled: Boolean,
  shortcuts: Boolean,
  label: String,
  bounds: GestureBounds,
  actions: PlayerGestureActions,
  arbitration: PlayerGestureArbitration,
  modifier: Modifier = Modifier,
) {
  val currentActions by rememberUpdatedState(actions)
  val density = LocalDensity.current.density
  Box(modifier.semantics {
    contentDescription = label
    onClick(label) { currentActions.toggleChrome(); true }
  }.pointerInput(generation, enabled, shortcuts, bounds, density) {
    if (!enabled) return@pointerInput
    val machine = PlayerGestureMachine(bounds, shortcuts, object : PlayerGestureActions {
      override fun playback() = currentActions.playback()
      override fun toggleChrome() = currentActions.toggleChrome()
      override fun seeking() = currentActions.seeking()
      override fun finishSeek(token: Long, target: Double?) = currentActions.finishSeek(token, target)
      override fun seek(target: Double) = currentActions.seek(target)
      override fun speed() = currentActions.speed()
      override fun finishSpeed(token: Long) = currentActions.finishSpeed(token)
      override fun brightness(value: Int) = currentActions.brightness(value)
      override fun volume(value: Int) = currentActions.volume(value)
      override fun restoreVolume(value: Int, muted: Boolean) = currentActions.restoreVolume(value, muted)
      override fun feedback(value: GestureFeedback?) = currentActions.feedback(value)
      override fun recognizing(active: Boolean) = currentActions.recognizing(active)
      override fun completed(value: GestureFeedback) = currentActions.completed(value)
    })
    arbitration.attach(machine)
    try {
      awaitPointerEventScope {
        var pointer: PointerId? = null
        var blocked = false
        var clock = 0L
        while (true) {
          val deadline = machine.deadline
          val event = if (deadline == null) awaitPointerEvent() else {
            withTimeoutOrNull((deadline - clock).coerceAtLeast(1)) { awaitPointerEvent() }
          }
          if (event == null) {
            clock = requireNotNull(deadline)
            machine.advance(clock)
            continue
          }
          clock = maxOf(clock, event.changes.maxOf { it.uptimeMillis })
          val held = event.changes.count { it.pressed }
          if (arbitration.blocked || held > 1 || event.changes.any { it.isConsumed }) {
            machine.cancel(); blocked = held > 0; pointer = null
          }
          if (blocked) {
            if (held == 0) blocked = false
            continue
          }
          if (pointer == null) event.changes.firstOrNull { it.changedToDownIgnoreConsumed() }?.let {
            pointer = it.id
            machine.down(GesturePoint(it.position.x / density, it.position.y / density), clock)
          }
          event.changes.firstOrNull { it.id == pointer }?.let { change ->
            val point = GesturePoint(change.position.x / density, change.position.y / density)
            val consumed = machine.consuming
            if (change.changedToUpIgnoreConsumed()) {
              machine.up(point, clock)
              if (consumed) change.consume()
              pointer = null
            } else if (change.pressed && !change.changedToDownIgnoreConsumed()) {
              machine.move(point, clock)
              if (machine.consuming) change.consume()
            }
          }
          // A parent (such as a system-owned gesture) can consume later in this event pass.
          val consumedHere = machine.consuming
          val finalEvent = awaitPointerEvent(PointerEventPass.Final)
          if (!consumedHere && finalEvent.changes.any { it.isConsumed } && pointer != null) {
            machine.cancel(); blocked = held > 0; pointer = null
          }
        }
      }
    } finally { machine.cancel(); arbitration.detach(machine) }
  })
}

/** Cancel in the Initial input pass, before a same-frame up can reach the background's Main pass. */
internal class PlayerGestureArbitration {
  private var machine: PlayerGestureMachine? = null
  var blocked = false
    private set
  fun attach(value: PlayerGestureMachine) { machine = value; if (blocked) value.cancel() }
  fun detach(value: PlayerGestureMachine) { if (machine === value) machine = null }
  fun cancel() { machine?.cancel() }
  fun pointers(count: Int) {
    if (count > 1) { blocked = true; cancel() } else if (count == 0) blocked = false
  }
}

/** Observe every child hit path so a second contact on a button also cancels picture gestures. */
internal fun Modifier.observePlayerMultitouch(arbitration: PlayerGestureArbitration) = pointerInput(arbitration) {
  try {
    awaitPointerEventScope {
      while (true) {
        val pressed = awaitPointerEvent(PointerEventPass.Initial).changes.count { it.pressed }
        arbitration.pointers(pressed)
      }
    }
  } finally { arbitration.cancel(); arbitration.pointers(0) }
}

/** Own the whole overlay's hit region without interfering with its accessible child actions. */
internal fun Modifier.reservePlayerOverlayTouchArea() = pointerInput(Unit) {
  awaitPointerEventScope { while (true) awaitPointerEvent() }
}

@Composable
internal fun playerGestureBounds(width: Float, height: Float, minimumEdge: Float = 24f): GestureBounds {
  val insets = WindowInsets.safeGestures.union(WindowInsets.displayCutout).asPaddingValues()
  val direction = LocalLayoutDirection.current
  return GestureBounds(
    maxOf(minimumEdge, insets.calculateLeftPadding(direction).value),
    maxOf(minimumEdge, insets.calculateTopPadding().value),
    width - maxOf(minimumEdge, insets.calculateRightPadding(direction).value),
    height - maxOf(minimumEdge, insets.calculateBottomPadding().value),
  )
}

@Composable
internal fun PlayerGestureFeedback(
  feedback: GestureFeedback?, bounds: GestureBounds, reducedMotion: Boolean,
  modifier: Modifier = Modifier,
) {
  var last by remember { mutableStateOf<GestureFeedback?>(null) }
  if (feedback != null) last = feedback
  val opacity by animateFloatAsState(if (feedback == null) 0f else 1f,
    tween(if (reducedMotion) 0 else 130), label = "gesture-feedback-opacity")
  val value = last ?: return
  val density = LocalDensity.current
  val scale = density.fontScale.coerceAtLeast(1f)
  val seekStyle = MaterialTheme.typography.titleMedium.copy(fontSize = 20.sp, lineHeight = 24.sp)
  val cancelStyle = MaterialTheme.typography.titleMedium.copy(fontSize = 16.sp, lineHeight = 24.sp)
  val secondaryStyle = MaterialTheme.typography.labelSmall.copy(fontSize = 12.sp, lineHeight = 16.sp)
  val cancelLabel = stringResource(R.string.gesture_release_cancel)
  val brightnessLabel = stringResource(R.string.gesture_brightness)
  val measurer = rememberTextMeasurer()
  val labelWidth = measurer.measure(brightnessLabel,
    style = MaterialTheme.typography.bodyMedium.copy(fontSize = 14.sp, lineHeight = 20.sp)).size.width / density.density
  val seekSize = if (value is GestureFeedback.Seek) remember(value.duration, value.origin, bounds, density,
    seekStyle, cancelStyle, secondaryStyle, cancelLabel, measurer) {
    val constraints = Constraints(maxWidth = ((bounds.width - 24).coerceAtLeast(1f) * density.density).roundToInt())
    // Reserve the whole timecode range and cancellation wording up front so neither hours nor
    // entering cancellation changes the chosen side or moves the bubble away from its anchor.
    val clock = playbackClock(maxOf(value.duration, value.origin))
    val clocks = ('0'..'9').map { digit -> clock.map { if (it.isDigit()) digit else it }.joinToString("") }
    val primary = clocks.map { measurer.measure(it, seekStyle, constraints = constraints).size } +
      measurer.measure(cancelLabel, cancelStyle, constraints = constraints).size
    val secondary = (clocks.flatMap { listOf("+$it", "−$it") } + playbackClock(value.origin))
      .map { measurer.measure(it, secondaryStyle, constraints = constraints).size }
    val width = (primary + secondary).maxOf { it.width } / density.density + 24
    val height = (primary.maxOf { it.height } + secondary.maxOf { it.height }) / density.density + 16
    maxOf(112f, width) to maxOf(56f, height)
  } else null
  val width = when (value) {
    is GestureFeedback.Step -> 72f * scale
    is GestureFeedback.Level -> if (value.brightness) maxOf(40f * scale, labelWidth + 12) else 40f * scale
    is GestureFeedback.Seek -> requireNotNull(seekSize).first
    GestureFeedback.Speed -> 52f * scale
  }.coerceAtMost(bounds.width)
  val height = when (value) {
    is GestureFeedback.Step -> 72f * scale
    is GestureFeedback.Level -> 108f + 28f * scale
    is GestureFeedback.Seek -> requireNotNull(seekSize).second
    GestureFeedback.Speed -> 12f + 20f * scale
  }.coerceAtMost(if (value is GestureFeedback.Seek) (bounds.height - 64).coerceAtLeast(1f) else bounds.height)
  // Native subtitles are composited into the video. Reserve their lower picture lane as well
  // as keeping this passive sibling underneath every actionable overlay.
  val feedbackBottom = (bounds.bottom - 64).coerceAtLeast(bounds.top + height)
  val seekPosition = if (value is GestureFeedback.Seek) seekFeedbackPosition(value.anchor, bounds, width, height) else null
  val x = when (value) {
    is GestureFeedback.Step -> value.anchor.x - width / 2
    is GestureFeedback.Level -> if (value.brightness) bounds.left + 56 - width / 2 else bounds.right - 56 - width / 2
    is GestureFeedback.Seek -> requireNotNull(seekPosition).x
    else -> (bounds.left + bounds.right - width) / 2
  }.coerceIn(bounds.left, (bounds.right - width).coerceAtLeast(bounds.left))
  val y = when (value) {
    is GestureFeedback.Step -> {
      val above = value.anchor.y - 48 - height / 2
      if (above >= bounds.top && above + height <= feedbackBottom) above else value.anchor.y + 48 - height / 2
    }
    is GestureFeedback.Level -> (bounds.top + bounds.bottom - height) / 2
    is GestureFeedback.Seek -> requireNotNull(seekPosition).y
    else -> bounds.top + 24
  }.coerceIn(bounds.top, (feedbackBottom - height).coerceAtLeast(bounds.top))
  Box(modifier.fillMaxSize().clearAndSetSemantics { }) {
    Column(
      Modifier.offset(x.dp, y.dp).size(width.dp, height.dp).alpha(opacity)
        .background(if (value is GestureFeedback.Step) PilotPlayerTokens.gestureStep else PilotPlayerTokens.phoneFeedback,
          if (value is GestureFeedback.Step) CircleShape else RoundedCornerShape(if (value is GestureFeedback.Level) 20.dp else 12.dp)),
      horizontalAlignment = Alignment.CenterHorizontally,
      verticalArrangement = Arrangement.Center,
    ) {
      when (value) {
        is GestureFeedback.Step -> {
          PilotIcon(if (value.backwards) R.drawable.ic_player_rewind_ten else R.drawable.ic_player_forward_ten,
            modifier = Modifier.size(24.dp), tint = PilotPlayerTokens.foreground)
          Spacer(Modifier.height(4.dp))
          Text(stringResource(R.string.gesture_seek_seconds, signedSeconds(value.delta)),
            style = MaterialTheme.typography.labelMedium.copy(fontSize = 14.sp, lineHeight = 20.sp), color = PilotPlayerTokens.foreground)
        }
        is GestureFeedback.Seek -> {
          Text(if (value.cancelled) cancelLabel else playbackClock(value.target),
            modifier = Modifier.padding(horizontal = 12.dp),
            style = if (value.cancelled) cancelStyle else seekStyle, color = PilotPlayerTokens.foreground)
          Text(if (value.cancelled) playbackClock(value.origin) else signedTime(value.target - value.origin),
            modifier = Modifier.padding(horizontal = 12.dp),
            style = secondaryStyle, color = PilotPlayerTokens.secondary)
        }
        is GestureFeedback.Level -> {
          Text(if (!value.brightness && value.value == 0) stringResource(R.string.gesture_muted) else "${value.value}%",
            style = MaterialTheme.typography.labelSmall.copy(fontSize = 12.sp, lineHeight = 16.sp), color = PilotPlayerTokens.foreground)
          Spacer(Modifier.height(6.dp))
          Box(Modifier.size(4.dp, 72.dp).background(PilotPlayerTokens.gestureRail, CircleShape)) {
            Box(Modifier.align(Alignment.BottomCenter).fillMaxWidth().fillMaxHeight(value.value / 100f)
              .background(PilotPlayerTokens.foreground, CircleShape))
          }
          Spacer(Modifier.height(4.dp))
          if (value.brightness) Text(brightnessLabel,
            style = MaterialTheme.typography.bodyMedium.copy(fontSize = 14.sp, lineHeight = 20.sp), color = PilotPlayerTokens.foreground)
          else PilotIcon(when {
            value.value == 0 -> R.drawable.ic_volume_muted
            value.value <= 33 -> R.drawable.ic_volume
            value.value <= 66 -> R.drawable.ic_volume_medium
            else -> R.drawable.ic_volume_loud
          }, modifier = Modifier.size(18.dp), tint = PilotPlayerTokens.foreground)
        }
        GestureFeedback.Speed -> Text("2×", style = MaterialTheme.typography.bodyMedium.copy(fontWeight = FontWeight.SemiBold), color = PilotPlayerTokens.foreground)
      }
    }
  }
}

internal fun signedSeconds(value: Double) = (if (value < 0) "−" else "+") + abs(value).roundToInt()
internal fun signedTime(value: Double) = (if (value < 0) "−" else "+") + playbackClock(abs(value))
