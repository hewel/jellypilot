package io.github.hewel.jellypilot

import android.os.ParcelFileDescriptor
import android.os.Looper
import android.system.Os
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.player.*
import java.io.File
import java.util.UUID
import kotlin.math.abs
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class NativePlayerBoundaryTest {
  private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
  private lateinit var directory: File
  private lateinit var media: File
  private lateinit var host: MpvPlayerHost
  private lateinit var recorder: PlayerHostRecorder

  @Before fun create() {
    directory = File(instrumentation.targetContext.cacheDir, "native-boundary-${UUID.randomUUID()}")
    media = TestMedia.stage(instrumentation.context, TestMedia.SAMPLE_ASSET, directory)
    host = MpvPlayerHost(instrumentation.targetContext, PlayerHostConfig(directory.path))
    recorder = PlayerHostRecorder()
    host.addListener(recorder)
  }

  @After fun destroy() {
    if (::host.isInitialized) host.release()
    if (::directory.isInitialized) directory.deleteRecursively()
  }

  @Test fun borrowedDescriptorSupportsPausedSeekAndRealTrackSelection() {
    val subtitle = TestMedia.stage(instrumentation.context, TestMedia.SUBTITLE_ASSET, directory)
    ParcelFileDescriptor.open(media, ParcelFileDescriptor.MODE_READ_ONLY).use { descriptor ->
      val identity = Os.fstat(descriptor.fileDescriptor)
      try {
        host.load(MediaLoad(MediaLocator.BorrowedFd(descriptor.fd), "borrowed", 1, startPaused = true,
          externalSubtitles = listOf(ExternalSubtitle(MediaLocator.LocalFile(subtitle.path), "external fixture", "en"))))
        val loaded = recorder.awaitSnapshot {
          it.generation == 1L && it.status == PlayerStatus.READY && it.paused &&
            it.tracks.count { track -> track.kind == TrackKind.AUDIO } == 2 &&
            it.tracks.any { track -> track.kind == TrackKind.SUBTITLE && track.isExternal }
        }
        assertFalse(loaded.playWhenReady)
        assertFalse(loaded.isPlaying)
        val japanese = loaded.tracks.single { it.kind == TrackKind.AUDIO && it.language == "jpn" }
        host.selectTrack(TrackKind.AUDIO, japanese.mpvId)
        recorder.awaitSnapshot { it.tracks.any { track -> track.kind == TrackKind.AUDIO && track.isSelected && track.language == "jpn" } }
        host.selectTrack(TrackKind.AUDIO, PlayerHost.TRACK_ID_AUTO)
        recorder.awaitSnapshot { it.tracks.any { track -> track.kind == TrackKind.AUDIO && track.isSelected && track.language == "eng" } }
        val external = loaded.tracks.single { it.kind == TrackKind.SUBTITLE && it.isExternal }
        host.selectTrack(TrackKind.SUBTITLE, external.mpvId)
        recorder.awaitSnapshot { it.tracks.any { track -> track.kind == TrackKind.SUBTITLE && track.isExternal && track.isSelected } }
        host.selectTrack(TrackKind.SUBTITLE, PlayerHost.TRACK_ID_NONE)
        recorder.awaitSnapshot { it.tracks.none { track -> track.kind == TrackKind.SUBTITLE && track.isSelected } }
        host.seekTo(6.0)
        val sought = recorder.awaitSnapshot { abs(it.positionSeconds - 6.0) < 0.4 && it.paused }
        assertFalse(sought.playWhenReady)
        host.stop()
        recorder.awaitEvent { it.ends(1) }
      } finally {
        host.release()
      }
      assertEquals(1, recorder.terminalEvents(1).size)
      val retained = Os.fstat(descriptor.fileDescriptor)
      assertEquals(identity.st_dev, retained.st_dev)
      assertEquals(identity.st_ino, retained.st_ino)
      val expected = ByteArray(16)
      media.inputStream().use { assertEquals(expected.size, it.read(expected)) }
      val actual = ByteArray(expected.size)
      assertEquals(actual.size, Os.pread(descriptor.fileDescriptor, actual, 0, actual.size, 0))
      assertArrayEquals(expected, actual)
    }
  }

  @Test fun replacementAndImmediateStopRetireEveryGenerationExactlyOnce() {
    HeldMediaServer(media).use { server ->
      host.load(MediaLoad(MediaLocator.Remote(server.url), "opening", 10))
      assertTrue("mpv must enter the held HTTP open", server.awaitRequest())
      host.load(MediaLoad(MediaLocator.LocalFile(media.path), "replacement", 11, startPaused = true))
      recorder.awaitEvent { it.ends(10) }
      server.release()
      val replacement = recorder.awaitSnapshot { it.generation == 11L && it.status == PlayerStatus.READY && it.paused }
      assertFalse(replacement.playWhenReady)
      host.stop()
      recorder.awaitEvent { it.ends(11) }
      host.load(MediaLoad(MediaLocator.LocalFile(media.path), "immediate-stop", 12))
      host.stop()
      recorder.awaitEvent { it.ends(12) }
      host.release()
      for (generation in 10L..12L) assertEquals("generation $generation", 1, recorder.terminalEvents(generation).size)
      assertTrue(recorder.events().none { it is PlayerEvent.FileLoaded && it.generation == 10L })
    }
  }

  @Test fun directoryAndM3uCannotExpandIntoUntrackedNativeChildren() {
    val playlist = File(directory, "queue.m3u").apply { writeText("#EXTM3U\nsample.mkv\n") }
    for ((index, input) in listOf(directory, playlist).withIndex()) {
      val generation = 20L + index
      host.load(MediaLoad(MediaLocator.LocalFile(input.path), "unsupported-queue", generation))
      recorder.awaitEvent { it.ends(generation) }
      assertTrue("queue must fail rather than redirect", host.snapshot.error is PlayerError.PlaybackFailed || host.snapshot.error is PlayerError.LoadFailed)
      assertTrue(recorder.events().none { it is PlayerEvent.FileLoaded && it.generation == generation })
      assertEquals(1, recorder.terminalEvents(generation).size)
    }
    host.load(MediaLoad(MediaLocator.LocalFile(media.path), "after-refusal", 22, startPaused = true))
    recorder.awaitSnapshot { it.generation == 22L && it.status == PlayerStatus.READY && it.paused }
  }

  @Test fun localRelativeHlsAndDashStaySingleSeekableTimelines() {
    for ((index, format) in listOf("hls" to "index.m3u8", "dash" to "index.mpd").withIndex()) {
      val root = TestMedia.tree(instrumentation.context, format.first, directory)
      val generation = 30L + index
      host.load(MediaLoad(MediaLocator.LocalFile(File(root, format.second).path), format.first, generation, startPaused = true))
      recorder.awaitSnapshot { it.generation == generation && it.status == PlayerStatus.READY && it.paused }
      host.seekTo(3.0)
      recorder.awaitSnapshot { it.generation == generation && abs(it.positionSeconds - 3.0) < 0.4 && it.paused }
      host.stop()
      recorder.awaitEvent { it.ends(generation) }
      assertEquals(1, recorder.terminalEvents(generation).size)
    }
  }

  @Test fun surfaceRecreationAcknowledgesNativeDetachAndRebind() {
    ActivityScenario.launch(MainActivity::class.java).use { scenario ->
      fun attach() = scenario.onActivity { activity ->
        activity.setContentView(MpvSurfaceView(activity).apply { attach(host) })
      }
      attach()
      recorder.awaitEvent { it == PlayerEvent.SurfaceAttached }
      host.load(MediaLoad(MediaLocator.LocalFile(media.path), "surface", 40, startPaused = true))
      recorder.awaitSnapshot { it.generation == 40L && it.status == PlayerStatus.READY && it.paused }
      host.seekTo(4.0)
      recorder.awaitSnapshot { abs(it.positionSeconds - 4.0) < 0.4 && it.paused }
      scenario.recreate()
      recorder.awaitEvent { it == PlayerEvent.SurfaceDetached }
      attach()
      recorder.awaitEvent { it == PlayerEvent.SurfaceAttached }
      host.seekTo(7.0)
      recorder.awaitSnapshot { it.generation == 40L && abs(it.positionSeconds - 7.0) < 0.4 && it.paused }
      assertTrue(recorder.terminalEvents(40).isEmpty())
    }
  }

  @androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
  @Test fun refusedPlayDoesNotLeaveMedia3OptimisticallyPlaying() {
    host.load(MediaLoad(MediaLocator.LocalFile(media.path), "denied-intent", 50, startPaused = true))
    recorder.awaitSnapshot { it.generation == 50L && it.status == PlayerStatus.READY && it.paused }
    lateinit var adapter: MpvMedia3Player
    instrumentation.runOnMainSync {
      adapter = MpvMedia3Player(host, Looper.getMainLooper(), object : PlayerIntentHandler {
        override fun onPlayerIntent(intent: PlayerIntent): Boolean = false
      })
    }
    try {
      instrumentation.runOnMainSync { adapter.play() }
      instrumentation.waitForIdleSync()
      instrumentation.runOnMainSync {
        assertFalse(adapter.playWhenReady)
        assertFalse(adapter.isPlaying)
      }
    } finally {
      instrumentation.runOnMainSync { adapter.release() }
    }
  }
}
