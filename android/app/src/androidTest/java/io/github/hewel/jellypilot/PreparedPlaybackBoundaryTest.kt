package io.github.hewel.jellypilot

import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.player.MediaLoad
import io.github.hewel.jellypilot.player.MediaLocator
import io.github.hewel.jellypilot.player.PlayerStatus
import java.io.File
import java.util.UUID
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Exercises the physical admission boundary without screenshots or timing sleeps. */
@RunWith(AndroidJUnit4::class)
class PreparedPlaybackBoundaryTest {
  @Test fun overlappingResourceOwnersHaveIndependentMediaSessionsAndDisposal() = runBlocking {
    val context = InstrumentationRegistry.getInstrumentation().targetContext
    val first = NativePlayback(context)
    val second = NativePlayback(context)
    try {
      withTimeout(25_000) { first.ready.first { it } }
      withTimeout(25_000) { second.ready.first { it } }
      assertNull(first.error.value)
      assertNull(second.error.value)
      first.close()
      assertTrue(withTimeout(25_000) { first.stopAndWait() })
      assertTrue("disposing another owner must not revoke this session", second.ready.value)
      assertNull(second.error.value)
    } finally {
      first.close()
      second.close()
      withTimeout(25_000) { first.stopAndWait(); second.stopAndWait() }
    }
  }

  @Test fun completedNativeDisposalAllowsFurtherStopAndHandoffRetries() = runBlocking {
    val isolated = NativePlayback(InstrumentationRegistry.getInstrumentation().targetContext)
    try {
      withTimeout(25_000) { isolated.ready.first { it } }
      isolated.close()
      assertTrue("a closing host waits for actual native disposal", withTimeout(25_000) { isolated.stopAndWait() })
      assertTrue("a disposed host remains successfully retired", isolated.stopAndWait())
    } finally { isolated.close() }
  }

  @Test fun preparedLoadReceiptWaitsForFileLoadedAndConfirmsPausedConfiguration() = withPlayer { player, media ->
    HeldMediaServer(media).use { server -> runBlocking {
      val receipt = async(Dispatchers.Default) { player.loadMedia(MediaLoad(MediaLocator.Remote(server.url), "receipt", startPaused = true)) }
      try {
        assertTrue("native HTTP open must be in flight", server.awaitRequest())
        assertFalse("load submission cannot acknowledge business admission", receipt.isCompleted)
        server.release()
        val generation = withTimeout(25_000) { receipt.await() }
        val loaded = withTimeout(25_000) { player.snapshot.first { it.generation == generation && it.tracks.isNotEmpty() } }
        assertEquals(PlayerStatus.READY, loaded.status)
        assertTrue("selection/volume must settle before unpausing", loaded.paused)
        val audio = loaded.tracks.first { it.kind == io.github.hewel.jellypilot.player.TrackKind.AUDIO }.mpvId
        assertTrue(player.configureMedia(27, audio, -1) { true })
        val configured = player.snapshot.value
        assertEquals(27, configured.volumePercent)
        assertTrue(configured.paused)
        assertTrue(configured.tracks.none { it.kind == io.github.hewel.jellypilot.player.TrackKind.SUBTITLE && it.isSelected })
      } finally { server.release(); assertTrue(player.stopAndWait()) }
    } }
  }

  @Test fun revokedIssuingTargetCannotCompleteAPreparedLoadAfterResponseArrives() = withPlayer { player, media ->
    HeldMediaServer(media).use { server -> runBlocking {
      val current = AtomicBoolean(true)
      val receipt = async(Dispatchers.Default) {
        runCatching { player.loadMedia(MediaLoad(MediaLocator.Remote(server.url), "revoked", startPaused = true), current::get) }
      }
      try {
        assertTrue(server.awaitRequest())
        current.set(false)
        server.release()
        assertTrue("a revoked origin must fail instead of acknowledging the load", withTimeout(25_000) { receipt.await() }.isFailure)
        assertEquals(PlayerStatus.IDLE, player.snapshot.value.status)
        assertFalse(player.snapshot.value.playWhenReady)
      } finally { server.release(); assertTrue(player.stopAndWait()) }
    } }
  }

  @Test fun failedExternalSubtitleDoesNotShiftTheRemainingSourceIdentity() = withPlayer { player, media -> runBlocking {
    val stagedSubtitle = TestMedia.stage(InstrumentationRegistry.getInstrumentation().context, TestMedia.SUBTITLE_ASSET, requireNotNull(media.parentFile))
    // A sample.* sidecar is auto-discovered by mpv before explicit sub-add, creating a duplicate.
    val subtitle = File(media.parentFile, "requested-subtitle.srt")
    check(stagedSubtitle.renameTo(subtitle))
    try {
      val generation = player.loadMedia(MediaLoad(MediaLocator.LocalFile(media.path), "subtitles", startPaused = true,
        externalSubtitles = listOf(
          io.github.hewel.jellypilot.player.ExternalSubtitle(MediaLocator.LocalFile(File(media.parentFile, "missing.srt").path)),
          io.github.hewel.jellypilot.player.ExternalSubtitle(MediaLocator.LocalFile(subtitle.path)),
        )))
      val loaded = withTimeout(25_000) { player.snapshot.first { it.generation == generation && it.tracks.any { track -> track.isExternal && track.externalSourceIndex == 1 } } }
      val external = loaded.tracks.single { it.isExternal && it.externalSourceIndex == 1 }
      assertEquals("failed source 0 must not relabel source 1", 1, external.externalSourceIndex)
      assertTrue(player.configureMedia(null, null, external.mpvId) { true })
    } finally { assertTrue(player.stopAndWait()) }
  } }

  @Test fun admissionLossDuringResumeRetainsTheSamePausedNativeSession() = withPlayer { player, media -> runBlocking {
    val revoked = AtomicBoolean(false)
    try {
      val generation = player.loadMedia(MediaLoad(MediaLocator.LocalFile(media.path), "resume-revoked", startPaused = true))
      val resumed = player.resumeMedia {
        // Revoke at the observed unpause boundary, with no sleep or scheduler timing assumption.
        if (player.snapshot.value.paused) true else {
          revoked.set(true)
          player.setEligible(false)
          false
        }
      }
      assertTrue("the test must reach physical resume before revoking it", revoked.get())
      assertFalse(resumed)
      val retained = player.snapshot.value
      assertEquals(generation, retained.generation)
      assertTrue(retained.status == PlayerStatus.READY || retained.status == PlayerStatus.BUFFERING)
      assertTrue(retained.paused)
      assertFalse(retained.playWhenReady)
    } finally { player.setEligible(true); assertTrue(player.stopAndWait()) }
  } }

  private fun withPlayer(action: (NativePlayback, File) -> Unit) {
    val instrumentation = InstrumentationRegistry.getInstrumentation()
    val directory = File(instrumentation.targetContext.cacheDir, "prepared-boundary-${UUID.randomUUID()}")
    val media = TestMedia.stage(instrumentation.context, TestMedia.SAMPLE_ASSET, directory)
    try {
      ActivityScenario.launch(MainActivity::class.java).use { scenario ->
        lateinit var player: NativePlayback
        scenario.onActivity { player = (it.application as JellyPilotApplication).player }
        runBlocking { withTimeout(25_000) { player.ready.first { it } } }
        action(player, media)
      }
    } finally { directory.deleteRecursively() }
  }
}
