package io.github.hewel.jellypilot

import android.app.Activity
import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.player.PlayerSnapshot
import io.github.hewel.jellypilot.player.PlayerStatus
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.math.abs
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PlaybackLifecycleTest {
  private fun model(activity: MainActivity): AppViewModel = ViewModelProvider(
    activity.application as JellyPilotApplication,
    ViewModelProvider.AndroidViewModelFactory(activity.application),
  )[AppViewModel::class.java]

  private fun await(player: NativePlayback, predicate: (PlayerSnapshot) -> Boolean): PlayerSnapshot =
    runBlocking { withTimeout(25_000) { player.snapshot.first(predicate) } }

  private fun surface(activity: Activity, player: NativePlayback, created: CountDownLatch) {
    val view = SurfaceView(activity)
    view.holder.addCallback(object : SurfaceHolder.Callback {
      override fun surfaceCreated(holder: SurfaceHolder) {
        player.attach(holder.surface)
        created.countDown()
      }
      override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) = Unit
      override fun surfaceDestroyed(holder: SurfaceHolder) { player.detach() }
    })
    activity.setContentView(view)
  }

  @Test fun backgroundReturnAndActivityRecreationRetainPausedSession() {
    ActivityScenario.launch(MainActivity::class.java).use { scenario ->
      lateinit var player: NativePlayback
      val firstSurface = CountDownLatch(1)
      scenario.onActivity { activity ->
        player = model(activity).player
        surface(activity, player, firstSurface)
      }
      runBlocking { withTimeout(25_000) { player.ready.first { it } } }
      assertTrue("initial Surface must be created", firstSurface.await(20, TimeUnit.SECONDS))
      try {
        player.loadFile(TestMediaProvider.sampleUri, TestMediaProvider.subtitleUri)
        val playing = await(player) { it.status == PlayerStatus.READY && it.isPlaying && it.positionSeconds > 0.5 }
        scenario.moveToState(Lifecycle.State.CREATED)
        val paused = await(player) { it.generation == playing.generation && it.paused && !it.playWhenReady && !it.isPlaying }
        scenario.moveToState(Lifecycle.State.RESUMED)
        player.seek(paused.positionSeconds + 1)
        await(player) { it.generation == playing.generation && abs(it.positionSeconds - paused.positionSeconds - 1) < 0.4 && it.paused && !it.playWhenReady }
        assertTrue("explicit foreground play must acquire focus", player.play())
        await(player) { it.isPlaying && it.positionSeconds > paused.positionSeconds + 1.5 }
        scenario.recreate()
        val rebound = CountDownLatch(1)
        scenario.onActivity { activity ->
          player = model(activity).player
          surface(activity, player, rebound)
        }
        assertTrue("replacement Surface must be created", rebound.await(20, TimeUnit.SECONDS))
        player.seek(8.0)
        await(player) { it.generation == playing.generation && abs(it.positionSeconds - 8.0) < 0.4 && it.paused && !it.playWhenReady }
      } finally {
        assertTrue("stop must acknowledge native descriptor retirement", runBlocking { player.stopAndWait() })
      }
    }
  }

  @Test fun backgroundingCancelsRealProviderOpenWithoutDeferredPlayback() {
    val resolver = InstrumentationRegistry.getInstrumentation().targetContext.contentResolver
    fun fixture(method: String): Boolean = resolver.call(TestMediaProvider.blockedUri, method, null, null)?.getBoolean("result") == true
    ActivityScenario.launch(MainActivity::class.java).use { scenario ->
      lateinit var player: NativePlayback
      scenario.onActivity { player = model(it).player }
      runBlocking { withTimeout(25_000) { player.ready.first { it } } }
      assertTrue(fixture("reset"))
      try {
        player.loadFile(TestMediaProvider.blockedUri, null)
        assertTrue("provider must receive the open before revocation", fixture("entered"))
        scenario.moveToState(Lifecycle.State.CREATED)
        assertTrue("CancellationSignal must reach the real provider", fixture("cancelled"))
        scenario.moveToState(Lifecycle.State.RESUMED)
        assertTrue(fixture("release"))
        assertTrue("late descriptor must be retired without a stop command", fixture("closed"))
        assertEquals(PlayerStatus.IDLE, player.snapshot.value.status)
        assertFalse(player.snapshot.value.playWhenReady)
        assertNull("cancelled open is not a user-facing error", player.error.value)
      } finally {
        fixture("release")
        assertTrue(runBlocking { player.stopAndWait() })
      }
    }
  }
}
