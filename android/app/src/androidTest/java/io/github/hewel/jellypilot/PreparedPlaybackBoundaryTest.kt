package io.github.hewel.jellypilot

import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.player.MediaLoad
import io.github.hewel.jellypilot.player.MediaLocator
import io.github.hewel.jellypilot.player.PlayerSnapshot
import io.github.hewel.jellypilot.player.PlayerStatus
import io.github.hewel.jellypilot.player.TrackKind
import java.io.File
import java.util.UUID
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
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

  @Test fun startupWaitsForLoadedAndPausedConfigurationBeforeBusinessCheckpointAndResume() = withPlayer { player, media ->
    HeldMediaServer(media).use { server -> runBlocking {
      val generation = AtomicLong(-1)
      val configured = CompletableDeferred<PlayerSnapshot>()
      val resume = CompletableDeferred<Unit>()
      val receipt = async(Dispatchers.Default) {
        player.startMedia(
          MediaLoad(MediaLocator.Remote(server.url), "receipt"),
          volume = 27, audio = 1, subtitle = StartupSubtitle.Track(-1),
          stillCurrent = { true }, bindGeneration = generation::set,
          beforeResume = {
            configured.complete(player.snapshot.value)
            resume.await()
          },
        )
      }
      try {
        assertTrue("native HTTP open must be in flight", server.awaitRequest())
        assertFalse("load submission cannot acknowledge business admission", receipt.isCompleted)
        assertFalse("business writes cannot precede loaded/configured media", configured.isCompleted)
        server.release()
        val paused = withTimeout(25_000) { configured.await() }
        assertEquals(generation.get(), paused.generation)
        assertTrue("startup forces paused loading even without a paused request", paused.paused)
        assertEquals(27, paused.volumePercent)
        assertTrue(paused.tracks.any { it.kind == TrackKind.AUDIO && it.mpvId == 1 && it.isSelected })
        assertTrue(paused.tracks.none { it.kind == TrackKind.SUBTITLE && it.isSelected })
        assertFalse("the business checkpoint must settle before resume", receipt.isCompleted)
        resume.complete(Unit)
        assertTrue(withTimeout(25_000) { receipt.await() })
        assertEquals(generation.get(), player.snapshot.value.generation)
        assertFalse("success acknowledges physical resume", player.snapshot.value.paused)
      } finally { server.release(); resume.complete(Unit); receipt.cancelAndJoin(); assertTrue(player.stopAndWait()) }
    } }
  }

  @Test fun revokedIssuingTargetCannotCompleteAPreparedLoadAfterResponseArrives() = withPlayer { player, media ->
    HeldMediaServer(media).use { server -> runBlocking {
      val current = AtomicBoolean(true)
      val receipt = async(Dispatchers.Default) {
        player.startMedia(
          MediaLoad(MediaLocator.Remote(server.url), "revoked"),
          stillCurrent = current::get,
          beforeResume = { fail("a revoked load cannot reach business writes") },
        )
      }
      try {
        assertTrue(server.awaitRequest())
        current.set(false)
        server.release()
        assertFalse("a revoked origin must fail instead of acknowledging startup", withTimeout(25_000) { receipt.await() })
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
      assertTrue(player.startMedia(
        MediaLoad(MediaLocator.LocalFile(media.path), "subtitles",
          externalSubtitles = listOf(
            io.github.hewel.jellypilot.player.ExternalSubtitle(MediaLocator.LocalFile(File(media.parentFile, "missing.srt").path)),
            io.github.hewel.jellypilot.player.ExternalSubtitle(MediaLocator.LocalFile(subtitle.path)),
          )),
        subtitle = StartupSubtitle.External(1),
        stillCurrent = { true },
        beforeResume = {
          val configured = player.snapshot.value
          assertTrue(configured.paused)
          val selected = configured.tracks.single { it.kind == TrackKind.SUBTITLE && it.isSelected }
          assertEquals("failed source 0 must not relabel source 1", 1, selected.externalSourceIndex)
        },
      ))
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

  @Test fun failedBusinessCheckpointRetiresStartupWithoutRollingBackCompletedWrites() = withPlayer { player, media -> runBlocking {
    var remembered = false
    val accepted = player.startMedia(
      MediaLoad(MediaLocator.LocalFile(media.path), "checkpoint-failed"),
      stillCurrent = { true },
      beforeResume = { remembered = true; error("later business write failed") },
    )
    assertTrue("startup does not roll back a completed business write", remembered)
    assertFalse(accepted)
    assertEquals("failure cannot release admission before retirement", PlayerStatus.IDLE, player.snapshot.value.status)
    assertFalse(player.snapshot.value.playWhenReady)
  } }

  @Test fun cancellationDuringBusinessCheckpointWaitsForStartupRetirement() = withPlayer { player, media -> runBlocking {
    val entered = CompletableDeferred<Unit>()
    val receipt = async(Dispatchers.Default) {
      player.startMedia(
        MediaLoad(MediaLocator.LocalFile(media.path), "checkpoint-cancelled"),
        stillCurrent = { true },
        beforeResume = { entered.complete(Unit); awaitCancellation() },
      )
    }
    try {
      withTimeout(25_000) { entered.await() }
      receipt.cancelAndJoin()
      assertTrue(receipt.isCancelled)
      assertEquals("cancellation cannot release admission before retirement", PlayerStatus.IDLE, player.snapshot.value.status)
      assertFalse(player.snapshot.value.playWhenReady)
    } finally { receipt.cancelAndJoin(); assertTrue(player.stopAndWait()) }
  } }

  @Test fun admissionLossDuringStartupResumeRetiresInsteadOfRetainingAPausedSession() = withPlayer { player, media -> runBlocking {
    val revoked = AtomicBoolean(false)
    try {
      val accepted = player.startMedia(
        MediaLoad(MediaLocator.LocalFile(media.path), "startup-resume-revoked"),
        stillCurrent = {
          if (player.snapshot.value.paused) true else {
            revoked.set(true)
            player.setEligible(false)
            false
          }
        },
      )
      assertTrue("the test must reach physical resume before revoking it", revoked.get())
      assertFalse(accepted)
      assertEquals(PlayerStatus.IDLE, player.snapshot.value.status)
      assertFalse(player.snapshot.value.playWhenReady)
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
