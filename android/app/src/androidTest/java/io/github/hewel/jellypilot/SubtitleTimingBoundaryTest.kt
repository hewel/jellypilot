package io.github.hewel.jellypilot

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.player.*
import java.io.File
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Real libmpv property/lifetime checks; no screenshot or rendered-subtitle timing claim. */
@RunWith(AndroidJUnit4::class)
class SubtitleTimingBoundaryTest {
  private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

  @Test fun nativeReadbackClampingTrackRestorationAndResetPreservePausedPlayback() = fixture { host, recorder, load ->
    host.load(load(1))
    val ready = recorder.awaitSnapshot { it.subtitleTiming.availability == SubtitleTimingAvailability.AVAILABLE &&
      it.tracks.count { track -> track.kind == TrackKind.SUBTITLE && track.isExternal } == 2 }
    val tracks = ready.tracks.filter { it.kind == TrackKind.SUBTITLE && it.isExternal }
    val first = select(host, recorder, tracks[0].mpvId)
    val position = first.positionSeconds
    for ((requested, expected) in listOf(5 to 5, -3 to -3, -101 to -100, 101 to 100, 0 to 0, 7 to 7)) {
      val confirmed = adjust(host, recorder, requested, expected)
      assertTrue(confirmed.paused)
      assertFalse(confirmed.playWhenReady)
      assertEquals(position, confirmed.positionSeconds, 0.05)
      assertEquals(expected / 10.0, nativeDelay(host), 0.00001)
      assertNull(confirmed.error)
    }
    val oldContext = requireNotNull(host.snapshot.subtitleTiming.context)
    select(host, recorder, tracks[1].mpvId)
    assertEquals(0.0, nativeDelay(host), 0.00001)
    adjust(host, recorder, -4, -4)
    host.selectTrack(TrackKind.SUBTITLE, PlayerHost.TRACK_ID_NONE)
    recorder.awaitSnapshot { it.subtitleTiming.availability == SubtitleTimingAvailability.SUBTITLES_OFF }
    assertFalse(host.setSubtitleTiming(oldContext, 20))
    val restored = select(host, recorder, tracks[0].mpvId)
    assertNotEquals(oldContext, restored.subtitleTiming.context)
    assertEquals(7, restored.subtitleTiming.offsetTenths)
    assertEquals(0.7, nativeDelay(host), 0.00001)
    select(host, recorder, tracks[1].mpvId)
    assertEquals(-0.4, nativeDelay(host), 0.00001)
    adjust(host, recorder, 0, 0)
    assertEquals(0.0, nativeDelay(host), 0.00001)
  }

  @Test fun directReplacementAndPreservedPictureSettingsStopBothClearSubtitleOffsets() = fixture { host, recorder, load ->
    host.load(load(10))
    recorder.awaitSnapshot { it.generation == 10L && it.subtitleTiming.availability == SubtitleTimingAvailability.AVAILABLE }
    adjust(host, recorder, 12, 12)
    val old = requireNotNull(host.snapshot.subtitleTiming.context)
    host.load(load(11))
    recorder.awaitSnapshot { it.generation == 11L && it.subtitleTiming.availability == SubtitleTimingAvailability.AVAILABLE }
    assertEquals(0, host.snapshot.subtitleTiming.offsetTenths)
    assertEquals(0.0, nativeDelay(host), 0.00001)
    assertFalse(host.setSubtitleTiming(old, 99))
    adjust(host, recorder, -9, -9)
    host.stop(preserveSessionSettings = true)
    recorder.awaitSnapshot { it.status == PlayerStatus.IDLE && it.subtitleTiming.context == null }
    recorder.awaitEvent { it.ends(11) }
    host.load(load(12))
    recorder.awaitSnapshot { it.generation == 12L && it.subtitleTiming.availability == SubtitleTimingAvailability.AVAILABLE }
    assertEquals(0.0, nativeDelay(host), 0.00001)
    adjust(host, recorder, 8, 8)
    host.stop()
    recorder.awaitEvent { it.ends(12) }
    assertEquals(SubtitleTimingState(), host.snapshot.subtitleTiming)
  }

  @Test fun pendingRequestCannotQueueRepeatsOrSurviveASelectionRoundTripAndAdmissionLoss() = fixture { host, recorder, load ->
    host.load(load(20))
    val ready = recorder.awaitSnapshot { it.subtitleTiming.availability == SubtitleTimingAvailability.AVAILABLE &&
      it.tracks.count { track -> track.kind == TrackKind.SUBTITLE && track.isExternal } == 2 }
    val tracks = ready.tracks.filter { it.kind == TrackKind.SUBTITLE && it.isExternal }
    val original = requireNotNull(select(host, recorder, tracks[0].mpvId).subtitleTiming.context)
    val gate = SnapshotGate { it.subtitleTiming.pending }.also(host::addListener)
    try {
      assertTrue(host.setSubtitleTiming(original, 5))
      gate.awaitReached()
      assertFalse(host.setSubtitleTiming(original, 6))
      host.selectTrack(TrackKind.SUBTITLE, tracks[1].mpvId)
      host.selectTrack(TrackKind.SUBTITLE, tracks[0].mpvId)
      gate.release()
      recorder.awaitSnapshot { it.subtitleTiming.context?.let { context ->
        context.trackId == tracks[0].mpvId && context != original
      } == true && !it.subtitleTiming.pending }
      assertFalse(host.setSubtitleTiming(original, 99))
      assertEquals(0.0, nativeDelay(host), 0.00001)
    } finally { gate.release(); host.removeListener(gate) }

    val context = requireNotNull(host.snapshot.subtitleTiming.context)
    val admissionGate = SnapshotGate { it.subtitleTiming.pending }.also(host::addListener)
    try {
      assertTrue(host.setSubtitleTiming(context, 3))
      admissionGate.awaitReached()
      host.setAdmissionEligible(false)
      admissionGate.release()
      recorder.awaitSnapshot { !it.admissionEligible && !it.subtitleTiming.pending }
      assertEquals(0, host.snapshot.subtitleTiming.offsetTenths)
      assertFalse(host.snapshot.subtitleTiming.failed)
      assertEquals(0.0, nativeDelay(host), 0.00001)
      assertFalse(host.setSubtitleTiming(context, 4))
      host.setAdmissionEligible(true)
      recorder.awaitSnapshot { it.admissionEligible }
      adjust(host, recorder, 1, 1)
      assertTrue(host.snapshot.paused)
    } finally { admissionGate.release(); host.removeListener(admissionGate) }
  }

  @Test fun observingSettledStateAlreadyAllowsTheNextRequest() = fixture { host, recorder, load ->
    host.load(load(30))
    val ready = recorder.awaitSnapshot { it.subtitleTiming.availability == SubtitleTimingAvailability.AVAILABLE }
    val context = requireNotNull(ready.subtitleTiming.context)
    val revision = ready.subtitleTiming.requestRevision
    val gate = SnapshotGate { it.subtitleTiming.context == context &&
      !it.subtitleTiming.pending && it.subtitleTiming.requestRevision == revision + 1
    }.also(host::addListener)
    try {
      assertTrue(host.setSubtitleTiming(context, 1))
      gate.awaitReached()
      // The executor is still notifying completion listeners. Admission from
      // this separate test thread must already be ready, even before finally.
      assertTrue(host.setSubtitleTiming(context, 2))
      gate.release()
      recorder.awaitSnapshot { it.subtitleTiming.requestRevision == revision + 2 && !it.subtitleTiming.pending }
      assertEquals(2, host.snapshot.subtitleTiming.offsetTenths)
      assertEquals(0.2, nativeDelay(host), 0.00001)
    } finally { gate.release(); host.removeListener(gate) }
  }

  private fun select(host: MpvPlayerHost, recorder: PlayerHostRecorder, trackId: Int): PlayerSnapshot {
    host.selectTrack(TrackKind.SUBTITLE, trackId)
    return recorder.awaitSnapshot { it.subtitleTiming.context?.trackId == trackId &&
      it.subtitleTiming.availability == SubtitleTimingAvailability.AVAILABLE }
  }

  private fun adjust(host: MpvPlayerHost, recorder: PlayerHostRecorder, requested: Int, expected: Int): PlayerSnapshot {
    val revision = host.snapshot.subtitleTiming.requestRevision
    assertTrue(host.setSubtitleTiming(requireNotNull(host.snapshot.subtitleTiming.context), requested))
    return recorder.awaitSnapshot { it.subtitleTiming.offsetTenths == expected && !it.subtitleTiming.pending &&
      it.subtitleTiming.requestRevision != revision }
  }

  private class SnapshotGate(private val predicate: (PlayerSnapshot) -> Boolean) : PlayerHost.Listener {
    private val reached = CountDownLatch(1)
    private val released = CountDownLatch(1)
    override fun onSnapshot(snapshot: PlayerSnapshot) {
      if (predicate(snapshot)) {
        reached.countDown()
        check(released.await(10, TimeUnit.SECONDS)) { "Timing test gate was not released" }
      }
    }
    override fun onEvent(event: PlayerEvent) = Unit
    fun awaitReached() = assertTrue(reached.await(10, TimeUnit.SECONDS))
    fun release() = released.countDown()
  }

  private fun fixture(block: (MpvPlayerHost, PlayerHostRecorder, (Long) -> MediaLoad) -> Unit) {
    val directory = File(instrumentation.targetContext.cacheDir, "subtitle-timing-${UUID.randomUUID()}")
    val media = TestMedia.stage(instrumentation.context, TestMedia.SAMPLE_ASSET, directory)
    val staged = TestMedia.stage(instrumentation.context, TestMedia.SUBTITLE_ASSET, directory)
    val first = File(directory, "timing-first.srt")
    check(staged.renameTo(first))
    val second = first.copyTo(File(directory, "timing-second.srt"))
    val host = MpvPlayerHost(instrumentation.targetContext, PlayerHostConfig(directory.path))
    val recorder = PlayerHostRecorder().also(host::addListener)
    try {
      block(host, recorder) { generation -> MediaLoad(
        MediaLocator.LocalFile(media.path), "timing-$generation", generation, startPaused = true,
        externalSubtitles = listOf(first, second).mapIndexed { index, file ->
          ExternalSubtitle(MediaLocator.LocalFile(file.path), "subtitle $index", "en")
        },
      ) }
    } finally {
      host.release()
      directory.deleteRecursively()
    }
  }

  /** Independent readback through the packaged JNI, without adding a public diagnostics API. */
  private fun nativeDelay(host: MpvPlayerHost): Double {
    val handle = MpvPlayerHost::class.java.getDeclaredField("handle").apply { isAccessible = true }.getLong(host)
    val method = Class.forName("io.github.hewel.jellypilot.player.MpvJni").getDeclaredMethod(
      "nativeGetPropertyString", Long::class.javaPrimitiveType, String::class.java,
    )
    return (method.invoke(null, handle, "sub-delay") as String).toDouble()
  }
}
