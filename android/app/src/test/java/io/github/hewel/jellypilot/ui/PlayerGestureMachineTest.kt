package io.github.hewel.jellypilot.ui

import org.junit.Assert.*
import org.junit.Test

internal class GestureRecorder : PlayerGestureActions {
  var state = GesturePlayback(1, 399.0, 1292.0, true, true, 1.5, 100, true, 70, true)
  var toggles = 0
  var active = false
  var cue: GestureFeedback? = null
  val seeks = mutableListOf<Double>()
  val finishes = mutableListOf<Pair<Long, Double?>>()
  var previews = 0
  var speeds = 0
  val restoredSpeeds = mutableListOf<Long>()
  val brightnessValues = mutableListOf<Int>()
  val volumeValues = mutableListOf<Int>()
  val volumeRestores = mutableListOf<Pair<Int, Boolean>>()
  val announcements = mutableListOf<GestureFeedback>()
  override fun playback() = state
  override fun toggleChrome() { toggles++ }
  override fun seeking(): GestureSeekCapture { previews++; return GestureSeekCapture(previews.toLong(), state.position) }
  override fun finishSeek(token: Long, target: Double?) { finishes += token to target }
  override fun seek(target: Double) { seeks += target }
  override fun speed(): Long { speeds++; return speeds.toLong() }
  override fun finishSpeed(token: Long) { restoredSpeeds += token }
  override fun brightness(value: Int) { brightnessValues += value }
  override fun volume(value: Int) { volumeValues += value }
  override fun restoreVolume(value: Int, muted: Boolean) { volumeRestores += value to muted }
  override fun feedback(value: GestureFeedback?) { cue = value }
  override fun recognizing(active: Boolean) { this.active = active }
  override fun completed(value: GestureFeedback) { announcements += value }
}

class PlayerGestureMachineTest {
  private val actions = GestureRecorder()
  private fun machine(shortcuts: Boolean = true) = PlayerGestureMachine(GestureBounds(24f, 24f, 824f, 424f), shortcuts, actions)
  private fun PlayerGestureMachine.tap(x: Float, time: Long) {
    down(GesturePoint(x, 180f), time); up(GesturePoint(x, 180f), time + 20)
  }

  @Test fun doubleTapCancelsVisibilityAndRepeatedStepsAccumulateWhileNativePositionLags() {
    val machine = machine()
    machine.tap(700f, 0); machine.tap(700f, 150)
    assertEquals(listOf(409.0), actions.seeks)
    assertEquals(0, actions.toggles)
    machine.tap(720f, 500)
    assertEquals(listOf(409.0, 419.0), actions.seeks)
    assertEquals(GestureFeedback.Step(20.0, GesturePoint(700f, 180f)), actions.cue)
    machine.advance(1219); assertNotNull(actions.cue)
    machine.advance(1220); assertNull(actions.cue)
  }

  @Test fun endPointShowsActualDeltaAndOppositeSideNeedsNewPair() {
    actions.state = actions.state.copy(position = 1288.0)
    val machine = machine()
    machine.tap(700f, 0); machine.tap(700f, 150)
    assertEquals(GestureFeedback.Step(4.0, GesturePoint(700f, 180f)), actions.cue)
    machine.tap(100f, 300)
    assertNull(actions.cue)
    assertEquals(1, actions.seeks.size)
    machine.tap(100f, 450)
    assertEquals(1278.0, actions.seeks.last(), 0.0)
  }

  @Test fun centerPairTogglesOnceAndSingleTapWaitsForArbitration() {
    val machine = machine()
    machine.tap(424f, 0); machine.tap(424f, 150); machine.advance(1000)
    assertEquals(1, actions.toggles); assertTrue(actions.seeks.isEmpty())
    machine.tap(700f, 1500)
    assertTrue(actions.active)
    machine.advance(1799); assertEquals(1, actions.toggles)
    machine.advance(1800); assertEquals(2, actions.toggles); assertFalse(actions.active)
  }

  @Test fun seekLocksOnceClampsAndCommitsOnlyOnRelease() {
    val machine = machine()
    machine.down(GesturePoint(424f, 200f), 0)
    actions.state = actions.state.copy(position = 401.0)
    machine.move(GesturePoint(444f, 200f), 40)
    machine.move(GesturePoint(624f, 200f), 100)
    assertEquals(GestureFeedback.Seek(401.0, 431.0, false), actions.cue)
    assertTrue(actions.finishes.isEmpty()); assertEquals(1, actions.previews)
    machine.up(GesturePoint(624f, 200f), 200)
    machine.advance(1000)
    assertEquals(listOf(1L to 431.0), actions.finishes)
    assertEquals(0, actions.toggles); assertEquals(0, actions.speeds)
  }

  @Test fun downwardCancelLatchesWhenFingerReturnsAndPointerCancelDoesNotCommit() {
    val machine = machine()
    machine.down(GesturePoint(424f, 100f), 0)
    machine.move(GesturePoint(450f, 102f), 30)
    machine.move(GesturePoint(600f, 166f), 90)
    machine.move(GesturePoint(624f, 100f), 150)
    assertEquals(GestureFeedback.Seek(399.0, 429.0, true), actions.cue)
    machine.up(GesturePoint(624f, 100f), 200)
    assertEquals(listOf(1L to null), actions.finishes)
    assertTrue(actions.announcements.isEmpty())
    machine.down(GesturePoint(424f, 200f), 400)
    machine.move(GesturePoint(500f, 200f), 450)
    machine.cancel(); machine.cancel()
    assertEquals(listOf(1L to null, 2L to null), actions.finishes)
  }

  @Test fun ambiguousDiagonalAndCentralVerticalSuppressBothHoldAndTap() {
    val machine = machine()
    machine.down(GesturePoint(424f, 200f), 0)
    machine.move(GesturePoint(440f, 215f), 100)
    machine.advance(500)
    assertEquals(0, actions.previews); assertEquals(0, actions.speeds)
    machine.up(GesturePoint(440f, 215f), 600)
    machine.tap(700f, 800); machine.advance(1200)
    assertEquals(1, actions.toggles)
    machine.down(GesturePoint(424f, 200f), 1300)
    machine.move(GesturePoint(424f, 280f), 1500); machine.up(GesturePoint(424f, 280f), 1600)
    assertTrue(actions.volumeValues.isEmpty()); assertTrue(actions.brightnessValues.isEmpty())
    assertEquals(1, actions.toggles)
  }

  @Test fun levelSideIsCapturedAndCancelRestoresInitialValueWithoutTap() {
    val machine = machine()
    machine.down(GesturePoint(100f, 300f), 0)
    machine.move(GesturePoint(100f, 100f), 50)
    machine.move(GesturePoint(700f, 100f), 100)
    assertEquals(GestureFeedback.Level(true, 100), actions.cue)
    machine.move(GesturePoint(700f, 400f), 150)
    assertEquals(80, actions.brightnessValues.last())
    machine.cancel()
    assertEquals(100, actions.brightnessValues.last())
    assertTrue(actions.volumeValues.isEmpty()); assertEquals(0, actions.toggles)
  }

  @Test fun volumeReleaseKeepsValueAndAnnouncesOnlyOnce() {
    val machine = machine()
    machine.down(GesturePoint(700f, 100f), 0)
    machine.move(GesturePoint(700f, 200f), 50)
    machine.move(GesturePoint(700f, 424f), 100)
    assertTrue(actions.announcements.isEmpty())
    machine.up(GesturePoint(700f, 424f), 200)
    assertEquals(0, actions.volumeValues.last())
    assertEquals(listOf(GestureFeedback.Level(false, 0)), actions.announcements)
    machine.advance(900); assertNull(actions.cue)
  }

  @Test fun holdCapturesTokenAndAlwaysEndsWithoutSynthesizingATap() {
    val machine = machine()
    machine.down(GesturePoint(424f, 200f), 0)
    machine.advance(399); assertEquals(0, actions.speeds)
    machine.advance(400); assertEquals(GestureFeedback.Speed, actions.cue)
    machine.move(GesturePoint(700f, 200f), 500)
    machine.up(GesturePoint(700f, 200f), 600)
    assertEquals(listOf(1L), actions.restoredSpeeds)
    assertEquals(0, actions.previews); assertEquals(0, actions.toggles)
    machine.down(GesturePoint(424f, 200f), 1000); machine.advance(1400); machine.cancel()
    assertEquals(listOf(1L, 2L), actions.restoredSpeeds)
  }

  @Test fun pausedAndAlreadyTwoTimesDoNotStartHoldAndEdgeContactsAreIgnored() {
    val machine = machine()
    actions.state = actions.state.copy(playing = false)
    machine.down(GesturePoint(424f, 200f), 0); machine.advance(400); machine.up(GesturePoint(424f, 200f), 500)
    actions.state = actions.state.copy(playing = true, speed = 2.0)
    machine.down(GesturePoint(424f, 200f), 1000); machine.advance(1400); machine.cancel()
    machine.tap(10f, 2000); machine.advance(3000)
    assertEquals(0, actions.speeds); assertEquals(0, actions.toggles)
  }

  @Test fun unavailableCapabilitiesDoNotConsumeVerticalOrStartSeekAndDisableStillAllowsTap() {
    actions.state = actions.state.copy(seekable = false, volumeAvailable = false)
    val machine = machine()
    machine.down(GesturePoint(700f, 200f), 0); machine.move(GesturePoint(700f, 100f), 100)
    assertFalse(machine.consuming); machine.up(GesturePoint(700f, 100f), 200)
    machine.down(GesturePoint(424f, 200f), 400); machine.move(GesturePoint(500f, 200f), 500); machine.cancel()
    assertEquals(0, actions.previews); assertNull(actions.cue)
    val disabled = machine(false)
    disabled.tap(700f, 1000); assertEquals(1, actions.toggles)
    disabled.down(GesturePoint(424f, 200f), 1200); disabled.move(GesturePoint(624f, 200f), 1300); disabled.up(GesturePoint(624f, 200f), 1400)
    assertEquals(0, actions.previews); assertEquals(1, actions.toggles)
  }

  @Test fun mediaReplacementCancelsWithoutWritingOldLevelIntoNewMedia() {
    val machine = machine()
    machine.down(GesturePoint(100f, 200f), 0); machine.move(GesturePoint(100f, 300f), 50)
    assertEquals(listOf(80), actions.brightnessValues)
    actions.state = actions.state.copy(generation = 2)
    machine.move(GesturePoint(100f, 310f), 100)
    assertEquals(listOf(80), actions.brightnessValues); assertNull(actions.cue)
  }

  @Test fun cancelledVolumeRestoresMuteAndTheUnderlyingVolume() {
    actions.state = actions.state.copy(muted = true)
    val machine = machine()
    machine.down(GesturePoint(700f, 200f), 0); machine.move(GesturePoint(700f, 100f), 50)
    assertEquals(25, actions.volumeValues.last())
    machine.cancel()
    assertEquals(listOf(70 to true), actions.volumeRestores)
  }

  @Test fun seekMetadataArrivingAfterDownIsCapturedAtAxisLock() {
    actions.state = actions.state.copy(duration = null, seekable = false)
    val machine = machine()
    machine.down(GesturePoint(424f, 200f), 0)
    actions.state = actions.state.copy(duration = 1292.0, seekable = true)
    machine.move(GesturePoint(624f, 200f), 100)
    machine.up(GesturePoint(624f, 200f), 200)
    assertEquals(listOf(1L to 429.0), actions.finishes)
  }
}
