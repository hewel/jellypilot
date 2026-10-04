package io.github.hewel.jellypilot.ui

import kotlin.math.abs
import kotlin.math.hypot
import kotlin.math.roundToInt

internal data class GesturePoint(val x: Float, val y: Float)
internal data class GestureBounds(val left: Float, val top: Float, val right: Float, val bottom: Float) {
  val width get() = (right - left).coerceAtLeast(1f)
  val height get() = (bottom - top).coerceAtLeast(1f)
  fun contains(point: GesturePoint) = point.x in left..right && point.y in top..bottom
}

internal data class GesturePlayback(
  val generation: Long,
  val position: Double,
  val duration: Double?,
  val seekable: Boolean,
  val playing: Boolean,
  val speed: Double,
  val brightness: Int,
  val brightnessAvailable: Boolean,
  val volume: Int,
  val volumeAvailable: Boolean,
  val muted: Boolean = false,
) {
  val canSeek get() = seekable && duration?.let { it.isFinite() && it > 0 } == true
}

internal data class GestureSeekCapture(val token: Long, val position: Double)
internal sealed interface GestureFeedback {
  data class Step(val delta: Double, val anchor: GesturePoint, val backwards: Boolean = false) : GestureFeedback
  data class Seek(
    val origin: Double, val target: Double, val cancelled: Boolean,
    val anchor: GesturePoint, val duration: Double,
  ) : GestureFeedback
  data class Level(val brightness: Boolean, val value: Int) : GestureFeedback
  data object Speed : GestureFeedback
}

internal interface PlayerGestureActions {
  fun playback(): GesturePlayback
  fun toggleChrome()
  fun seeking(): GestureSeekCapture?
  fun finishSeek(token: Long, target: Double?)
  fun seek(target: Double)
  fun speed(): Long?
  fun finishSpeed(token: Long)
  fun brightness(value: Int)
  fun volume(value: Int)
  fun restoreVolume(value: Int, muted: Boolean)
  fun feedback(value: GestureFeedback?)
  fun recognizing(active: Boolean)
  fun completed(value: GestureFeedback)
}

/** One arbitration point owns taps, axes and hold timers; time/coordinates are supplied by input. */
internal class PlayerGestureMachine(
  private val bounds: GestureBounds,
  private val shortcuts: Boolean,
  private val actions: PlayerGestureActions,
) {
  private enum class Region { Left, Center, Right }
  private enum class Mode { Pending, Seek, Brightness, Volume, Speed, Ignored }
  private data class Contact(
    val start: GesturePoint, val region: Region, val time: Long,
    val playback: GesturePlayback, var mode: Mode = Mode.Pending,
    var hold: Boolean = true, var moved: Boolean = false,
    var seek: GestureSeekCapture? = null, var lockY: Float = 0f, var anchorX: Float = 0f,
    var target: Double = 0.0, var duration: Double = 0.0, var cancelled: Boolean = false,
    var speed: Long? = null, var value: Int = 0,
  )
  private data class Tap(val region: Region, val time: Long, val point: GesturePoint)
  private data class Series(val region: Region, val origin: Double, val target: Double, val anchor: GesturePoint, val time: Long)
  private var contact: Contact? = null
  private var tap: Tap? = null
  private var series: Series? = null
  private var expires: Long? = null
  private var cue: GestureFeedback? = null
  val consuming get() = contact?.mode in listOf(Mode.Seek, Mode.Brightness, Mode.Volume, Mode.Speed)
  val deadline: Long? get() = listOfNotNull(
    tap?.takeIf { contact == null }?.let { it.time + 280 },
    contact?.takeIf { shortcuts && it.mode == Mode.Pending && it.hold && it.region == Region.Center }?.let { it.time + 400 },
    expires?.takeIf { contact == null },
  ).minOrNull()

  fun down(point: GesturePoint, now: Long) {
    advance(now)
    if (contact != null) { cancel(); return }
    if (!bounds.contains(point)) { cancel(); return }
    val fraction = (point.x - bounds.left) / bounds.width
    val region = if (fraction < 0.4f) Region.Left else if (fraction > 0.6f) Region.Right else Region.Center
    if (series?.region != region) { series = null; show(null) }
    if (tap?.region != region) flushTap()
    contact = Contact(point, region, now, actions.playback())
    busy()
  }

  fun move(point: GesturePoint, now: Long) {
    val active = contact ?: return
    if (actions.playback().generation != active.playback.generation) { cancel(); return }
    val dx = point.x - active.start.x
    val dy = point.y - active.start.y
    val distance = hypot(dx, dy)
    if (distance > 8) active.hold = false
    if (distance >= 12) active.moved = true
    if (active.mode == Mode.Pending && active.moved) {
      tap = null
      series = null
      if (!shortcuts) active.mode = Mode.Ignored
      else if (abs(dx) > abs(dy) * 1.25f) {
        active.hold = false
        val current = actions.playback()
        active.duration = current.duration ?: 0.0
        active.seek = if (current.canSeek) actions.seeking() else null
        active.mode = if (active.seek != null) Mode.Seek else Mode.Ignored
        active.lockY = point.y
      } else if (abs(dy) > abs(dx) * 1.25f) {
        active.hold = false
        active.mode = when {
          active.region == Region.Left && active.playback.brightnessAvailable -> Mode.Brightness
          active.region == Region.Right && active.playback.volumeAvailable -> Mode.Volume
          else -> Mode.Ignored
        }
      }
    }
    when (active.mode) {
      Mode.Seek -> {
        val capture = requireNotNull(active.seek)
        if (!active.cancelled) active.anchorX = point.x
        active.cancelled = active.cancelled || point.y - active.lockY >= 64
        active.target = (capture.position + dx / bounds.width * 120).coerceIn(0.0, active.duration)
        show(GestureFeedback.Seek(capture.position, active.target, active.cancelled,
          GesturePoint(active.anchorX, active.lockY), active.duration))
      }
      Mode.Brightness, Mode.Volume -> {
        val brightness = active.mode == Mode.Brightness
        val initial = if (brightness) active.playback.brightness else if (active.playback.muted) 0 else active.playback.volume
        active.value = (initial - dy / bounds.height * (if (brightness) 80 else 100)).roundToInt().coerceIn(if (brightness) 20 else 0, 100)
        if (brightness) actions.brightness(active.value) else actions.volume(active.value)
        show(GestureFeedback.Level(brightness, active.value))
      }
      else -> Unit
    }
    advance(now)
  }

  fun up(point: GesturePoint, now: Long) {
    move(point, now)
    val active = contact ?: return
    contact = null
    when (active.mode) {
      Mode.Seek -> {
        actions.finishSeek(requireNotNull(active.seek).token, active.target.takeUnless { active.cancelled })
        if (!active.cancelled) cue?.let(actions::completed)
        show(null)
      }
      Mode.Speed -> { actions.finishSpeed(requireNotNull(active.speed)); show(null) }
      Mode.Brightness, Mode.Volume -> { cue?.let(actions::completed); expires = now + 700 }
      Mode.Pending -> if (!active.moved && now - active.time < 400) tapped(active, now) else { tap = null; show(null) }
      Mode.Ignored -> { tap = null; show(null) }
    }
    busy()
  }

  private fun tapped(active: Contact, now: Long) {
    if (!shortcuts) { actions.toggleChrome(); return }
    val previous = tap
    val chain = series?.takeIf { it.region == active.region && now - it.time <= 700 }
    if (chain != null || (previous != null && previous.region == active.region && active.time - previous.time <= 280)) {
      tap = null
      val playback = actions.playback()
      if (active.region == Region.Center || !playback.canSeek) {
        actions.toggleChrome()
        return
      }
      val origin = chain?.origin ?: playback.position
      val from = chain?.target ?: playback.position
      val target = (from + if (active.region == Region.Left) -10 else 10).coerceIn(0.0, requireNotNull(playback.duration))
      val anchor = chain?.anchor ?: previous?.point ?: active.start
      actions.seek(target)
      series = Series(active.region, origin, target, anchor, now)
      val feedback = GestureFeedback.Step(target - origin, anchor, active.region == Region.Left)
      show(feedback)
      actions.completed(feedback)
      expires = now + 700
    } else tap = Tap(active.region, now, active.start)
  }

  fun advance(now: Long) {
    if (contact == null && tap?.let { now >= it.time + 280 } == true) flushTap()
    contact?.takeIf { shortcuts && it.mode == Mode.Pending && it.hold && it.region == Region.Center && now >= it.time + 400 }?.let { active ->
      active.hold = false
      tap = null
      val current = actions.playback()
      active.speed = if (current.generation == active.playback.generation && current.playing && current.speed != 2.0) actions.speed() else null
      active.mode = if (active.speed != null) Mode.Speed else Mode.Ignored
      if (active.speed != null) show(GestureFeedback.Speed)
    }
    if (contact == null && expires?.let { now >= it } == true) { series = null; show(null) }
    busy()
  }

  fun cancel() {
    contact?.let { active ->
      when (active.mode) {
        Mode.Seek -> actions.finishSeek(requireNotNull(active.seek).token, null)
        Mode.Speed -> actions.finishSpeed(requireNotNull(active.speed))
        Mode.Brightness -> if (actions.playback().generation == active.playback.generation) actions.brightness(active.playback.brightness)
        Mode.Volume -> if (actions.playback().generation == active.playback.generation) actions.restoreVolume(active.playback.volume, active.playback.muted)
        else -> Unit
      }
    }
    contact = null; tap = null; series = null
    show(null)
    busy()
  }

  private fun flushTap() { if (tap != null) actions.toggleChrome(); tap = null; busy() }
  private fun show(value: GestureFeedback?) { cue = value; expires = null; actions.feedback(value) }
  private fun busy() = actions.recognizing(contact != null || tap != null)
}
