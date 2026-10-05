package io.github.hewel.jellypilot

import android.app.Application
import android.app.KeyguardManager
import android.content.Intent
import android.os.PowerManager
import androidx.test.core.app.ApplicationProvider
import io.github.hewel.jellypilot.ffi.*
import io.github.hewel.jellypilot.ui.*
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowLooper

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class RemoteControllerTest {
  private val key = RemoteControlTargetKey("living-room", "tv")
  private val otherKey = RemoteControlTargetKey("bedroom", "bedroom-tv")
  private val target = RemoteControlTarget(key, "Living room", "Jellyfin", null,
    RemoteControlNowPlaying("episode-4", "Episode 4", 80.0, 600.0, true), 40u,
    RemoteControlCapabilities(true, true, true, true, true, true))
  private fun initialSnapshot() = RemoteControlSnapshot(1uL, ProfileScopeRef("profile", 1uL), 1uL,
    RemoteControllerStatus.INACTIVE, listOf(target, target.copy(key = otherKey)), null, false, false, null)

  private inner class Port : RemoteControllerPort {
    var state = initialSnapshot()
    val events = mutableListOf<String>()
    val requests = mutableListOf<Triple<ULong, RemoteControlTargetKey, RemoteControlCommand>>()
    val updates = Channel<RemoteControlSnapshot>(Channel.UNLIMITED)
    val result = CompletableDeferred<RemoteControlReceipt>()
    var ignoreCancellation = false
    var waiters = 0
    override fun snapshot() = state
    override suspend fun nextSnapshot(revision: ULong): RemoteControlSnapshot {
      ++waiters
      return try { updates.receive() } finally { --waiters; events += "cancel-wait" }
    }
    override fun setActive(active: Boolean) {
      events += "active:$active"
      state = state.copy(revision = state.revision + 1uL, generation = state.generation + 1uL,
        status = if (active) RemoteControllerStatus.READY else RemoteControllerStatus.INACTIVE, selected = null)
    }
    override fun refresh() { events += "refresh" }
    override fun selectTarget(key: RemoteControlTargetKey) {
      state = state.copy(revision = state.revision + 1uL, generation = state.generation + 1uL, selected = key)
    }
    override suspend fun execute(generation: ULong, target: RemoteControlTargetKey, command: RemoteControlCommand): RemoteControlReceipt {
      requests += Triple(generation, target, command)
      return if (ignoreCancellation) withContext(NonCancellable) { result.await() } else result.await()
    }
    override fun close() { events += "close" }
  }

  @Test fun foregroundAndLockTransitionsInvalidateSdkBeforeCancellingWaitersAndNeverReplayCommands() {
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
    val port = Port()
    var ui: RemoteControllerUiState? = null
    val controller = RemoteControllerCoordinator(scope, { port }, { ui = it })
    try {
      controller.open(null, "Home server")
      assertEquals(0, port.waiters)
      controller.setEligible(true)
      controller.select(key)
      val generation = ui!!.snapshot!!.generation
      controller.execute(generation, key, RemoteControlCommand.Resume)
      assertEquals(1, port.requests.size)
      port.events.clear()
      controller.setEligible(false)
      assertEquals("active:false", port.events.first())
      assertEquals(0, port.waiters)
      controller.execute(generation, key, RemoteControlCommand.Resume)
      controller.setEligible(true)
      controller.execute(generation, key, RemoteControlCommand.Resume)
      assertEquals(1, port.requests.size)
      assertNull(ui!!.snapshot!!.selected)
      assertFalse(ui!!.accepted)
    } finally { controller.close(); scope.cancel() }
  }

  @Test fun screenOffAndLockedScreenOnStopSdkWorkUntilTheVisiblePhoneIsUnlocked() {
    val app = ApplicationProvider.getApplicationContext<Application>()
    val power = shadowOf(app.getSystemService(PowerManager::class.java))
    val lock = shadowOf(app.getSystemService(KeyguardManager::class.java))
    power.turnScreenOn(true)
    lock.setKeyguardLocked(false)
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
    val port = Port()
    val controller = RemoteControllerCoordinator(scope, { port }, {})
    val visibility = PlaybackVisibility(app, controller::setEligible)
    try {
      controller.open(null, "Server")
      visibility.setVisible(true)
      assertEquals(1, port.waiters)
      power.turnScreenOn(false)
      ShadowLooper.idleMainLooper()
      assertEquals(0, port.waiters)
      lock.setKeyguardLocked(true)
      power.turnScreenOn(true)
      ShadowLooper.idleMainLooper()
      assertEquals(0, port.waiters)
      lock.setKeyguardLocked(false)
      app.sendBroadcast(Intent(Intent.ACTION_USER_PRESENT))
      ShadowLooper.idleMainLooper()
      assertEquals(1, port.waiters)
      visibility.setVisible(false)
      app.sendBroadcast(Intent(Intent.ACTION_USER_PRESENT))
      ShadowLooper.idleMainLooper()
      assertEquals(0, port.waiters)
      assertTrue(port.requests.isEmpty())
    } finally { visibility.close(); controller.close(); scope.cancel() }
  }

  @Test fun closingOrChangingAccountDropsAnUncooperativeOldReceiptAndOldTargetCallbacks() = runBlocking {
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
    val retired = Port().apply { ignoreCancellation = true }
    val replacement = Port()
    var next: RemoteControllerPort = retired
    var ui: RemoteControllerUiState? = null
    val controller = RemoteControllerCoordinator(scope, { next }, { ui = it })
    try {
      controller.setEligible(true)
      controller.open(null, "First server")
      controller.select(key)
      val generation = ui!!.snapshot!!.generation
      controller.execute(generation, key, RemoteControlCommand.Pause)
      controller.close()
      assertNull(ui)
      assertTrue(retired.events.contains("close"))
      next = replacement
      controller.open(null, "Second server")
      retired.result.complete(RemoteControlReceipt(1uL, true))
      controller.execute(generation, key, RemoteControlCommand.Stop)
      assertEquals("Second server", ui!!.serverName)
      assertFalse(ui!!.accepted)
      assertTrue(replacement.requests.isEmpty())
    } finally { controller.close(); scope.cancel() }
  }

  @Test fun explicitPlayConfirmationUsesSelectedGenerationAndActualEpisodeWhileReceiptKeepsObservedPlayback() {
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
    val port = Port()
    var ui: RemoteControllerUiState? = null
    val controller = RemoteControllerCoordinator(scope, { port }, { ui = it })
    val episode = MediaUi("episode-4", "Episode 4", "Episode", "", null, "", false, false,
      resumeSeconds = 80.0, playTargetId = "episode-4")
    val series = episode.copy(id = "series", title = "Series", itemType = "Series", playTargetId = episode.id)
    val detail = AppUiState(detail = series, detailItems = listOf(episode))
    try {
      assertNull(AppUiState(detail = series).remotePlayItem(series))
      controller.setEligible(true)
      controller.open(detail.remotePlayItem(series), "Server")
      controller.select(key)
      assertTrue(port.requests.isEmpty())
      val oldGeneration = ui!!.snapshot!!.generation
      controller.select(otherKey)
      controller.execute(oldGeneration, key, RemoteControlCommand.Stop)
      assertTrue(port.requests.isEmpty())
      controller.play(ui!!.snapshot!!.generation, otherKey)
      val request = port.requests.single()
      assertEquals(ui!!.snapshot!!.generation, request.first)
      assertEquals(otherKey, request.second)
      val command = request.third as RemoteControlCommand.PlayNow
      assertEquals("episode-4", command.itemId)
      assertEquals(80.0, (command.position as PlaybackStartPosition.At).seconds, 0.0)
      port.result.complete(RemoteControlReceipt(1uL, true))
      assertTrue(ui!!.accepted)
      assertEquals(true, ui!!.snapshot!!.targets.first().nowPlaying!!.paused)
      assertEquals(80.0, ui!!.snapshot!!.targets.first().nowPlaying!!.positionSeconds!!, 0.0)
    } finally { controller.close(); scope.cancel() }
  }

  @Test fun observerFailureDisablesTheOldSessionAndRetryOpensAFreshOne() {
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
    val first = Port()
    val retry = Port()
    var next: RemoteControllerPort = first
    var ui: RemoteControllerUiState? = null
    val controller = RemoteControllerCoordinator(scope, { next }, { ui = it })
    try {
      controller.setEligible(true)
      controller.open(null, "Server")
      controller.select(key)
      val generation = ui!!.snapshot!!.generation
      first.updates.close(IllegalStateException("test transport closed"))
      assertTrue(ui!!.failed)
      assertTrue(first.events.contains("active:false"))
      controller.execute(generation, key, RemoteControlCommand.Stop)
      assertTrue(first.requests.isEmpty())
      next = retry
      controller.refresh()
      assertFalse(ui!!.failed)
      assertEquals(1, retry.waiters)
    } finally { controller.close(); scope.cancel() }
  }
}
