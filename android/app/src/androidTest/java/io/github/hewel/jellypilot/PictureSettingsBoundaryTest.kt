package io.github.hewel.jellypilot

import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.player.*
import java.io.File
import java.util.UUID
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PictureSettingsBoundaryTest {
  private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

  @Test fun nativeGainClampsReadsBackAndResetsWithoutChangingPausedTracks() {
    val directory = File(instrumentation.targetContext.cacheDir, "picture-settings-${UUID.randomUUID()}")
    val media = TestMedia.stage(instrumentation.context, TestMedia.SAMPLE_ASSET, directory)
    val stagedSubtitles = TestMedia.stage(instrumentation.context, TestMedia.SUBTITLE_ASSET, directory)
    // Keep explicit source identity independent of mpv's automatic sample.* sidecar discovery.
    val subtitles = File(directory, "picture-fixture.srt")
    check(stagedSubtitles.renameTo(subtitles))
    val host = MpvPlayerHost(instrumentation.targetContext, PlayerHostConfig(directory.path))
    val recorder = PlayerHostRecorder().also(host::addListener)
    try {
      ActivityScenario.launch(MainActivity::class.java).use { scenario ->
        scenario.onActivity { activity -> activity.setContentView(MpvSurfaceView(activity).apply { attach(host) }) }
        recorder.awaitEvent { it == PlayerEvent.SurfaceAttached }
        host.load(MediaLoad(MediaLocator.LocalFile(media.path), "picture", 1, startPaused = true,
          externalSubtitles = listOf(ExternalSubtitle(MediaLocator.LocalFile(subtitles.path), "fixture", "en"))))
        val loaded = recorder.awaitSnapshot { it.generation == 1L && it.status == PlayerStatus.READY &&
          it.tracks.any { track -> track.kind == TrackKind.SUBTITLE && track.isExternal } }
        assertTrue(loaded.pictureBrightnessAvailable)
        val subtitle = loaded.tracks.single { it.kind == TrackKind.SUBTITLE && it.isExternal }
        host.selectTrack(TrackKind.SUBTITLE, subtitle.mpvId)
        recorder.awaitSnapshot { it.tracks.any { track -> track.isSelected && track.mpvId == subtitle.mpvId } }
        for ((request, confirmed) in listOf(-5 to 20, 60 to 60, 250 to 100)) {
          host.setPictureBrightness(request)
          val updated = recorder.awaitSnapshot { it.pictureBrightnessPercent == confirmed }
          assertTrue(updated.paused)
          assertFalse(updated.playWhenReady)
          assertTrue(updated.tracks.any { it.kind == TrackKind.SUBTITLE && it.mpvId == subtitle.mpvId && it.isSelected })
          assertEquals(loaded.volumePercent, updated.volumePercent)
          assertNull(updated.error)
        }
        host.setPictureBrightness(40)
        host.setSpeed(1.5)
        recorder.awaitSnapshot { it.pictureBrightnessPercent == 40 && it.speed == 1.5 && it.paused }
        val start = host.snapshot.positionSeconds
        host.play()
        val playing = recorder.awaitSnapshot { it.isPlaying && it.positionSeconds > start + 0.5 }
        assertTrue(playing.pictureBrightnessAvailable)
        assertNull(playing.error)
        host.stop()
        recorder.awaitEvent { it.ends(1) }
        recorder.awaitSnapshot { it.status == PlayerStatus.IDLE && it.pictureBrightnessPercent == 100 && it.speed == 1.0 }
        host.load(MediaLoad(MediaLocator.LocalFile(media.path), "fresh-session", 2, startPaused = true))
        recorder.awaitSnapshot { it.generation == 2L && it.status == PlayerStatus.READY && it.paused }
        assertEquals(100, host.snapshot.pictureBrightnessPercent)
        assertEquals(1.0, host.snapshot.speed, 0.0)
      }
    } finally {
      host.release()
      directory.deleteRecursively()
    }
  }

  @Test fun continuousReplacementPreservesSettingsAndInvalidShaderStorageFailsInitialization() {
    val directory = File(instrumentation.targetContext.cacheDir, "picture-reset-${UUID.randomUUID()}")
    val media = TestMedia.stage(instrumentation.context, TestMedia.SAMPLE_ASSET, directory)
    try {
      val blocker = File(directory, "not-a-directory").apply { writeText("fixture") }
      try {
        MpvPlayerHost(instrumentation.targetContext, PlayerHostConfig(blocker.path)).release()
        fail("Required picture shader staging failure must be reported")
      } catch (_: java.io.IOException) { }
      val host = MpvPlayerHost(instrumentation.targetContext, PlayerHostConfig(directory.path))
      val recorder = PlayerHostRecorder().also(host::addListener)
      try {
        host.load(MediaLoad(MediaLocator.LocalFile(media.path), "first", 10, startPaused = true))
        recorder.awaitSnapshot { it.generation == 10L && it.status == PlayerStatus.READY }
        host.setPictureBrightness(30)
        host.setSpeed(2.0)
        recorder.awaitSnapshot { it.pictureBrightnessPercent == 30 && it.speed == 2.0 }
        host.stop(preserveSessionSettings = true)
        recorder.awaitEvent { it.ends(10) }
        recorder.awaitSnapshot { it.status == PlayerStatus.IDLE }
        host.load(MediaLoad(MediaLocator.LocalFile(media.path), "replacement", 11, startPaused = true))
        recorder.awaitSnapshot { it.generation == 11L && it.status == PlayerStatus.READY && it.paused }
        assertEquals(30, host.snapshot.pictureBrightnessPercent)
        assertEquals(2.0, host.snapshot.speed, 0.0)
        // Direct native replacement belongs to the same open player session too.
        host.load(MediaLoad(MediaLocator.LocalFile(media.path), "replacement", 12, startPaused = true))
        recorder.awaitSnapshot { it.generation == 12L && it.status == PlayerStatus.READY && it.paused }
        assertEquals(30, host.snapshot.pictureBrightnessPercent)
        assertEquals(2.0, host.snapshot.speed, 0.0)
        host.stop()
        // Already queued controls must not reapply session settings behind the stop command.
        host.setPictureBrightness(20)
        host.setSpeed(2.0)
        recorder.awaitEvent { it == PlayerEvent.CommandRejected("speed", RejectionReason.NOT_READY) }
        assertTrue(recorder.events().contains(PlayerEvent.CommandRejected("pictureBrightness", RejectionReason.NOT_READY)))
        if (recorder.terminalEvents(12).isEmpty()) recorder.awaitEvent { it.ends(12) }
        recorder.awaitSnapshot { it.status == PlayerStatus.IDLE && it.pictureBrightnessPercent == 100 && it.speed == 1.0 }
        assertEquals(100, host.snapshot.pictureBrightnessPercent)
        assertEquals(1.0, host.snapshot.speed, 0.0)
      } finally { host.release() }
    } finally { directory.deleteRecursively() }
  }
}
