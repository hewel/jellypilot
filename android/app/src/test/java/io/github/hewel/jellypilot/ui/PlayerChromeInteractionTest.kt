package io.github.hewel.jellypilot.ui

import android.app.Application
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createComposeRule
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class, qualifiers = "w844dp-h390dp")
class PlayerChromeInteractionTest {
  @get:Rule val compose = createComposeRule()
  private val paused = mutableStateOf(false)
  private val assistive = mutableStateOf(false)
  private val generation = mutableStateOf(1L)
  private lateinit var chrome: PlayerChromeState

  private fun mount(phone: Boolean = true) {
    compose.setContent {
      chrome = rememberPlayerChrome(generation.value, paused.value, phone, assistive.value)
      Column {
        TextButton(onClick = chrome::toggle) { Text("Picture") }
        if (chrome.visible) Text(if (paused.value) "Play control" else "Pause control")
      }
    }
    settle()
  }

  // Drain composition/effects before advancing a deadline; v2 has separate frame and UI scheduling.
  private fun settle() {
    compose.mainClock.autoAdvance = true
    compose.waitForIdle()
    compose.mainClock.autoAdvance = false
  }
  private fun timeout() { compose.mainClock.advanceTimeBy(3_100); settle() }

  @Test fun externalPauseRetainsControlsButExplicitPictureTapCanHideWithoutResuming() {
    mount()
    compose.onNodeWithText("Pause control").assertDoesNotExist()
    compose.runOnIdle { paused.value = true }
    settle()
    compose.onNodeWithText("Play control").assertExists()
    timeout()
    compose.onNodeWithText("Play control").assertExists()
    compose.onNodeWithText("Picture").performClick()
    settle()
    compose.onNodeWithText("Play control").assertDoesNotExist()
    compose.runOnIdle { assertTrue(paused.value) }
    compose.onNodeWithText("Picture").performClick()
    settle()
    compose.runOnIdle { paused.value = false }
    settle()
    compose.onNodeWithText("Pause control").assertExists()
    timeout()
    compose.onNodeWithText("Pause control").assertDoesNotExist()
  }

  @Test fun seekingFocusAndOpenPanelSuspendTheTimerAndReleaseStartsANewDeadline() {
    mount()
    compose.onNodeWithText("Picture").performClick()
    compose.runOnIdle { chrome.dragging = true }
    settle(); timeout()
    compose.onNodeWithText("Pause control").assertExists()
    compose.runOnIdle { chrome.dragging = false; chrome.focused = true }
    settle(); timeout()
    compose.onNodeWithText("Pause control").assertExists()
    compose.runOnIdle { chrome.focused = false; chrome.open(PlayerPanel.More) }
    settle(); timeout()
    compose.onNodeWithText("Pause control").assertExists()
    compose.runOnIdle { chrome.close() }
    settle()
    compose.mainClock.advanceTimeBy(2_500)
    compose.onNodeWithText("Pause control").assertExists()
    compose.mainClock.advanceTimeBy(600)
    settle()
    compose.onNodeWithText("Pause control").assertDoesNotExist()
  }

  @Test fun assistiveExplorationRevealsControlsAndHoldsThemAcrossMediaReplacement() {
    mount()
    compose.runOnIdle { assistive.value = true }
    settle(); timeout()
    compose.onNodeWithText("Pause control").assertExists()
    compose.runOnIdle { generation.value = 2L }
    settle(); timeout()
    compose.onNodeWithText("Pause control").assertExists()
  }

  @Test fun nestedAudioBackReturnsToMoreAndCloseRestoresTheOriginalTrigger() {
    paused.value = true
    mount()
    compose.runOnIdle {
      chrome.open(PlayerPanel.More)
      chrome.open(PlayerPanel.Audio)
      chrome.back()
      assertEquals(PlayerPanel.More, chrome.panel)
      assertEquals(PlayerPanel.Audio, chrome.restoreRow)
      chrome.open(PlayerPanel.Audio)
      chrome.close()
      assertNull(chrome.panel)
      assertEquals(PlayerPanel.More, chrome.restoreTrigger)
      assertTrue(chrome.visible)
      assertTrue(paused.value)
      chrome.open(PlayerPanel.Subtitles)
      chrome.back()
      assertEquals(PlayerPanel.Subtitles, chrome.restoreTrigger)
    }
    timeout()
    compose.onNodeWithText("Play control").assertExists()
  }

  @Test fun tabletRetainsItsIndependentPausedTimeoutContract() {
    paused.value = true
    mount(phone = false)
    compose.onNodeWithText("Play control").assertDoesNotExist()
    compose.onNodeWithText("Picture").performClick()
    settle(); timeout()
    compose.onNodeWithText("Play control").assertDoesNotExist()
  }
}
