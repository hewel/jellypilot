package io.github.hewel.jellypilot

import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.player.*
import java.io.File
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlin.math.abs
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Real native transaction/lifecycle checks; no visual capture or injected application input. */
@RunWith(AndroidJUnit4::class)
class GesturePlaybackBoundaryTest {
  @Test fun previewCommitsOnceAndCancellationRetainsPausedIntent() = withPlayer { player, media -> runBlocking {
    val generation = player.loadMedia(MediaLoad(MediaLocator.LocalFile(media.path), "seek", startPaused = true))
    withTimeout(15_000) { player.snapshot.first { it.generation == generation && it.seekable } }
    assertTrue(player.snapshot.value.volumeAvailable)
    val first = requireNotNull(player.beginGestureSeek())
    assertTrue(player.gestureSeekActive)
    assertTrue(player.finishGestureSeek(first.token, 5.0) { true })
    assertTrue(player.snapshot.value.paused)
    assertFalse(player.snapshot.value.playWhenReady)
    assertEquals(5.0, player.snapshot.value.positionSeconds, 0.3)
    assertFalse("a completed token cannot commit twice", player.finishGestureSeek(first.token, 9.0) { true })
    val cancelled = requireNotNull(player.beginGestureSeek())
    assertTrue(player.finishGestureSeek(cancelled.token, null) { true })
    assertEquals(cancelled.positionSeconds, player.snapshot.value.positionSeconds, 0.3)
    assertTrue(player.snapshot.value.paused)
  } }

  @Test fun playingPreviewResumesButExplicitPauseAndBackgroundRevokeOldReceipts() = withPlayer { player, media -> runBlocking {
    player.loadMedia(MediaLoad(MediaLocator.LocalFile(media.path), "resume", startPaused = true))
    withTimeout(15_000) { player.snapshot.first { it.seekable } }
    assertTrue(player.resumeMedia { true })
    withTimeout(15_000) { player.snapshot.first { it.isPlaying } }
    val commit = requireNotNull(player.beginGestureSeek())
    withTimeout(10_000) { player.snapshot.first { it.paused } }
    val reached = CountDownLatch(1)
    val release = CountDownLatch(1)
    val finishing = async(Dispatchers.Default) { player.finishGestureSeek(commit.token, 3.0) {
      reached.countDown()
      check(release.await(10, TimeUnit.SECONDS))
      true
    } }
    try {
      assertTrue(reached.await(10, TimeUnit.SECONDS))
      assertTrue(player.gestureSeeking.value)
      assertNull("temporary preview pause must not become a new gesture's paused intent", player.beginGestureSeek())
      player.seek(9.0)
    } finally { release.countDown() }
    assertTrue(finishing.await())
    assertFalse(player.gestureSeeking.value)
    withTimeout(10_000) { player.snapshot.first { it.isPlaying } }
    // Pause intent must win immediately, before mpv publishes its asynchronous paused snapshot.
    player.pause()
    val afterPause = requireNotNull(player.beginGestureSeek())
    assertTrue(player.finishGestureSeek(afterPause.token, 4.0) { true })
    assertTrue(player.snapshot.value.paused)
    assertFalse(player.snapshot.value.playWhenReady)
    assertEquals(4.0, player.snapshot.value.positionSeconds, 0.3)
    assertTrue(player.resumeMedia { true })
    withTimeout(10_000) { player.snapshot.first { it.isPlaying } }
    val revoked = requireNotNull(player.beginGestureSeek())
    player.pause()
    assertFalse(player.finishGestureSeek(revoked.token, 8.0) { true })
    withTimeout(10_000) { player.snapshot.first { it.paused && !it.playWhenReady && abs(it.positionSeconds - revoked.positionSeconds) < 0.3 } }
    assertTrue(player.resumeMedia { true })
    withTimeout(10_000) { player.snapshot.first { it.isPlaying } }
    val background = requireNotNull(player.beginGestureSeek())
    player.setEligible(false)
    player.setEligible(true)
    assertFalse(player.finishGestureSeek(background.token, 9.0) { true })
    withTimeout(10_000) { player.snapshot.first { it.paused && !it.playWhenReady } }
    assertFalse(player.gestureSeekActive)
  } }

  @Test fun revocationInsideFinalAdmissionCheckCannotQueueTheOldTargetAfterRestoration() = withPlayer { player, media -> runBlocking {
    player.loadMedia(MediaLoad(MediaLocator.LocalFile(media.path), "reentrant-revocation", startPaused = true))
    withTimeout(15_000) { player.snapshot.first { it.seekable } }
    val preview = requireNotNull(player.beginGestureSeek())
    val checks = AtomicInteger()
    val accepted = player.finishGestureSeek(preview.token, 8.0) {
      // The last admission decision may synchronously deliver a stronger pause/revocation.
      if (checks.incrementAndGet() == 2) player.pause()
      true
    }
    assertFalse(accepted)
    assertFalse(player.gestureSeeking.value)
    withTimeout(10_000) { player.snapshot.first { it.paused && !it.playWhenReady } }
    // A fresh explicit play observes the settled native location, not the old pre-seek snapshot.
    assertTrue(player.resumeMedia { true })
    val resumed = withTimeout(10_000) { player.snapshot.first { it.isPlaying && it.positionSeconds > preview.positionSeconds + 0.05 } }
    assertTrue("the revoked target must never replace the restored origin", resumed.positionSeconds < preview.positionSeconds + 1.0)
  } }

  @Test fun temporarySpeedUsesTwoAndRestoresBeforePauseAndReplacement() = withPlayer { player, media -> runBlocking {
    player.loadMedia(MediaLoad(MediaLocator.LocalFile(media.path), "speed", startPaused = true))
    player.speed(1.5)
    withTimeout(10_000) { player.snapshot.first { it.speed == 1.5 } }
    assertNull("holding is unavailable while paused", player.beginGestureSpeed())
    assertTrue(player.resumeMedia { true })
    withTimeout(10_000) { player.snapshot.first { it.isPlaying } }
    val hold = requireNotNull(player.beginGestureSpeed())
    withTimeout(10_000) { player.snapshot.first { it.speed == 2.0 } }
    player.pause()
    player.endGestureSpeed(hold)
    withTimeout(10_000) { player.snapshot.first { it.speed == 1.5 && it.paused && !it.playWhenReady } }
    assertTrue(player.resumeMedia { true })
    withTimeout(10_000) { player.snapshot.first { it.isPlaying } }
    val stale = requireNotNull(player.beginGestureSpeed())
    withTimeout(10_000) { player.snapshot.first { it.speed == 2.0 } }
    val replacement = player.loadMedia(MediaLoad(MediaLocator.LocalFile(media.path), "next", startPaused = true))
    withTimeout(15_000) { player.snapshot.first { it.generation == replacement && it.status == PlayerStatus.READY } }
    player.endGestureSpeed(stale)
    assertEquals(1.5, player.snapshot.value.speed, 0.0)
    assertTrue(player.snapshot.value.paused)
    player.speed(2.0)
    assertTrue(player.resumeMedia { true })
    withTimeout(10_000) { player.snapshot.first { it.isPlaying && it.speed == 2.0 } }
    assertNull("base 2x has no temporary override", player.beginGestureSpeed())
  } }

  private fun withPlayer(action: (NativePlayback, File) -> Unit) {
    val instrumentation = InstrumentationRegistry.getInstrumentation()
    val directory = File(instrumentation.targetContext.cacheDir, "gesture-boundary-${UUID.randomUUID()}")
    val media = TestMedia.stage(instrumentation.context, TestMedia.SAMPLE_ASSET, directory)
    val player = NativePlayback(instrumentation.targetContext)
    ActivityScenario.launch(PlaybackFixtureActivity::class.java).use { scenario ->
      try {
        runBlocking { withTimeout(25_000) { player.ready.first { it } } }
        player.setEligible(true)
        scenario.onActivity { activity -> activity.setContentView(SurfaceView(activity).apply {
          holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) { player.attach(holder.surface) }
            override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) = Unit
            override fun surfaceDestroyed(holder: SurfaceHolder) { player.detach() }
          })
        }) }
        action(player, media)
      } finally {
        player.close()
        runBlocking { withTimeout(25_000) { player.stopAndWait() } }
        directory.deleteRecursively()
      }
    }
  }
}
