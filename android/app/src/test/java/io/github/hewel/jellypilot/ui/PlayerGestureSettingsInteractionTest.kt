package io.github.hewel.jellypilot.ui

import android.app.Application
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.R
import io.github.hewel.jellypilot.player.PlayerSnapshot
import io.github.hewel.jellypilot.player.PlayerStatus
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class, qualifiers = "en-rUS-w667dp-h375dp")
class PlayerGestureSettingsInteractionTest {
  @get:Rule val compose = createComposeRule()
  private val context get() = InstrumentationRegistry.getInstrumentation().targetContext
  private fun text(id: Int) = context.getString(id)
  private val snapshot = PlayerSnapshot(status = PlayerStatus.READY, paused = true, speed = 1.5,
    pictureBrightnessAvailable = true, pictureBrightnessPercent = 80)

  private fun actions() = PlayerPanelActions(
    selectTrack = { _, _ -> }, selectEpisode = {}, previousEpisode = {}, nextEpisode = {}, loadMoreEpisodes = {}, setVolume = {}, close = {},
  )

  @Test fun gestureSwitchReflectsConfirmedPreferenceAndNeverChangesPlaybackOrBrightness() {
    val enabled = mutableStateOf(true)
    val changes = mutableListOf<Boolean>()
    var brightnessChanges = 0
    compose.setContent {
      MaterialTheme {
        Box(Modifier.size(340.dp, 340.dp)) {
          PlayerPanelContent(PlayerPanel.Picture, snapshot, null, actions().copy(
            gesturesEnabled = enabled.value, setGesturesEnabled = { changes += it }, setPictureBrightness = { brightnessChanges++ },
          ), phone = true)
        }
      }
    }
    compose.onNodeWithText(text(R.string.player_gestures_title)).assertIsOn().performClick()
    compose.runOnIdle { assertEquals(listOf(false), changes); assertEquals(0, brightnessChanges) }
    compose.onNodeWithText(text(R.string.player_gestures_title)).assertIsOn()
    compose.runOnIdle { enabled.value = false }
    compose.onNodeWithText(text(R.string.player_gestures_title)).assertIsOff()
    compose.onNodeWithText(text(R.string.player_gestures_view_help)).assertIsEnabled()
    compose.runOnIdle { assertEquals(true, snapshot.paused); assertEquals(1.5, snapshot.speed, 0.0) }
  }

  @Test fun largeTextHelpScrollsAndReturnsThroughPictureToOriginalMoreTrigger() {
    val chrome = PlayerChromeState(visible = true, paused = true).apply { open(PlayerPanel.More) }
    compose.setContent {
      MaterialTheme {
        CompositionLocalProvider(LocalDensity provides Density(LocalDensity.current.density, 2f)) {
          Box(Modifier.size(340.dp, 340.dp)) {
            chrome.panel?.let { panel ->
              PlayerPanelContent(panel, snapshot, null, actions().copy(
                close = chrome::close, open = chrome::open, back = if (panel == PlayerPanel.More) null else chrome::back,
              ), restoreRow = chrome.restoreRow, phone = true)
            }
          }
        }
      }
    }
    compose.onNode(hasScrollAction()).performScrollToNode(hasText(text(R.string.player_gestures_picture_title)))
    compose.onNodeWithText(text(R.string.player_gestures_picture_title)).performClick()
    compose.onNode(hasScrollAction()).performScrollToNode(hasText(text(R.string.player_gestures_view_help)))
    compose.onNodeWithText(text(R.string.player_gestures_view_help)).performClick()
    compose.onNode(hasScrollAction()).performScrollToNode(hasText(text(R.string.player_gestures_hold_detail)))
    compose.onNodeWithText(text(R.string.player_gestures_hold_detail)).assertIsDisplayed()
    compose.onNodeWithText(text(R.string.player_gestures_got_it)).assertIsDisplayed().performClick()
    compose.onNodeWithText(text(R.string.player_gestures_view_help)).assertIsFocused()
    compose.onNodeWithContentDescription(text(R.string.back)).performClick()
    compose.onNodeWithText(text(R.string.player_gestures_picture_title)).assertIsFocused().performClick()
    compose.onNode(hasScrollAction()).performScrollToNode(hasText(text(R.string.player_gestures_view_help)))
    compose.onNodeWithText(text(R.string.player_gestures_view_help)).performClick()
    compose.onNodeWithContentDescription(text(R.string.close)).performClick()
    compose.runOnIdle {
      assertEquals(null, chrome.panel)
      assertEquals(PlayerPanel.More, chrome.restoreTrigger)
      assertEquals(true, chrome.visible)
    }
  }
}
