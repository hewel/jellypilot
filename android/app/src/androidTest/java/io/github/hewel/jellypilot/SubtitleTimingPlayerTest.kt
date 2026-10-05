package io.github.hewel.jellypilot

import android.content.pm.ActivityInfo
import android.content.res.Configuration
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.player.PlayerSnapshot
import io.github.hewel.jellypilot.player.PlayerStatus
import io.github.hewel.jellypilot.player.MediaLoad
import io.github.hewel.jellypilot.player.MediaLocator
import io.github.hewel.jellypilot.player.SubtitleTimingAvailability
import io.github.hewel.jellypilot.player.TrackKind
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.TimeoutCancellationException
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Exercises the real player, panel windows and Back dispatcher; appearance remains human-owned. */
@RunWith(AndroidJUnit4::class)
class SubtitleTimingPlayerTest {
  @get:Rule val compose = createEmptyComposeRule()

  private fun await(player: NativePlayback, stage: String, predicate: (PlayerSnapshot) -> Boolean): PlayerSnapshot =
    try { runBlocking { withTimeout(25_000) { player.snapshot.first(predicate) } } }
    catch (failure: TimeoutCancellationException) {
      throw AssertionError("Timed out at $stage: ${player.snapshot.value}; error=${player.error.value}", failure)
    }

  private fun systemWindowFocus(): String = android.os.ParcelFileDescriptor.AutoCloseInputStream(
    InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand("dumpsys window displays")
  ).bufferedReader().use { reader -> reader.lineSequence().filter { it.contains("mCurrentFocus") }.joinToString() }

  @Test fun timingSurvivesRotationAndNestedBackWithoutSeekingOrResuming() {
    ActivityScenario.launch(MainActivity::class.java).use { scenario ->
      lateinit var model: AppViewModel
      lateinit var application: JellyPilotApplication
      scenario.onActivity { activity ->
        application = activity.application as JellyPilotApplication
        model = ViewModelProvider(application, ViewModelProvider.AndroidViewModelFactory(application))[AppViewModel::class.java]
      }
      val player = model.player
      runBlocking { withTimeout(25_000) { player.ready.first { it } } }
      val descriptor = requireNotNull(application.contentResolver.openFileDescriptor(TestMediaProvider.sampleUri, "r"))
      try {
        scenario.onActivity { model.openPlayer() }
        // Prepare paused so delayed time-pos observations from an earlier play intent cannot
        // be mistaken for a seek caused by timing adjustment or window reconfiguration.
        runBlocking { player.loadMedia(MediaLoad(MediaLocator.BorrowedFd(descriptor.fd), startPaused = true)) }
        val loaded = await(player, "media with subtitles loaded") { it.status == PlayerStatus.READY && it.tracks.any { track -> track.kind == TrackKind.SUBTITLE } }
        scenario.onActivity {
          player.select(TrackKind.SUBTITLE, loaded.tracks.first { track -> track.kind == TrackKind.SUBTITLE }.mpvId)
        }
        val paused = await(player, "selected subtitle timing available while paused") { it.paused && it.subtitleTiming.availability == SubtitleTimingAvailability.AVAILABLE }
        val position = paused.positionSeconds
        compose.onNodeWithText(application.localizedString(R.string.player_subtitles)).performClick()
        compose.onNodeWithTag("subtitle-timing-entry").performClick()
        val later = hasContentDescription(application.localizedString(R.string.subtitle_timing_later_accessibility))
        val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
        val originalFlags = automation.serviceInfo.flags
        automation.serviceInfo = automation.serviceInfo.apply {
          flags = flags or android.accessibilityservice.AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS or
            android.accessibilityservice.AccessibilityServiceInfo.FLAG_REPORT_VIEW_IDS
        }
        try {
          compose.waitUntil(15_000) {
            // A fresh Android installation may put its first-use fullscreen hint above the app.
            // Acknowledge that specific hint through accessibility; never change system settings.
            val systemWindow = automation.windows.firstOrNull { it.isFocused && it.root?.packageName == "android" }
            if (systemWindow != null && systemWindowFocus().contains("ImmersiveModeConfirmation")) {
              systemWindow.root?.findAccessibilityNodeInfosByViewId("android:id/ok")?.firstOrNull()
                ?.performAction(android.view.accessibility.AccessibilityNodeInfo.ACTION_CLICK)
            }
            compose.onAllNodes(later and isEnabled()).fetchSemanticsNodes().isNotEmpty()
          }
        } catch (failure: ComposeTimeoutException) {
          var windows = ""
          scenario.onActivity { activity ->
            windows = "lifecycle=${activity.lifecycle.currentState}; activityFocused=${activity.window.decorView.hasWindowFocus()}"
          }
          val focus = systemWindowFocus()
          val accessibility = automation.windows.joinToString { "${it.title}:${it.root?.packageName}" }
          throw AssertionError("Timing button disabled: $windows; $focus; accessibility=$accessibility; ${compose.onNode(later).printToString()}", failure)
        } finally { automation.serviceInfo = automation.serviceInfo.apply { flags = originalFlags } }
        compose.onNode(later).performClick()
        await(player, "timing edit acknowledged") { !it.subtitleTiming.pending && it.subtitleTiming.offsetTenths == 1 }

        // Android can ignore the landscape preference in a resized window; retain the playback origin.
        scenario.onActivity { it.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_PORTRAIT }
        compose.waitUntil(15_000) {
          var portrait = false
          scenario.onActivity { portrait = it.resources.configuration.orientation == Configuration.ORIENTATION_PORTRAIT }
          portrait
        }
        compose.onNodeWithTag("subtitle-timing-value").assertExists()
        assertEquals(paused.generation, player.snapshot.value.generation)
        assertEquals(1, player.snapshot.value.subtitleTiming.offsetTenths)
        assertTrue(player.snapshot.value.paused)
        assertEquals(position, player.snapshot.value.positionSeconds, 0.05)

        scenario.onActivity { it.onBackPressedDispatcher.onBackPressed() }
        compose.onNodeWithTag("subtitle-timing-entry").assertExists()
        compose.onNodeWithTag("subtitle-timing-value").assertDoesNotExist()
        compose.onNodeWithContentDescription(application.localizedString(R.string.close)).performClick()
        compose.onNodeWithTag("subtitle-timing-entry").assertDoesNotExist()
        assertTrue(model.state.value.showPlayer)
        assertTrue(player.snapshot.value.paused)
        assertEquals(1, player.snapshot.value.subtitleTiming.offsetTenths)
      } finally {
        scenario.onActivity { if (model.state.value.showPlayer) model.back() }
        assertTrue(runBlocking { player.stopAndWait() })
        descriptor.close()
      }
    }
  }
}
