package io.github.hewel.jellypilot.ui

import android.app.Application
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.unit.dp
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.R
import io.github.hewel.jellypilot.player.PlayerSnapshot
import io.github.hewel.jellypilot.player.PlayerStatus
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class, qualifiers = "w844dp-h390dp")
class PlayerTimelineInteractionTest {
  @get:Rule val compose = createComposeRule()
  private val seekLabel get() = InstrumentationRegistry.getInstrumentation().targetContext.getString(R.string.seek)

  @Test fun playbackUpdatesCannotOverrideAnActiveDragAndReleaseSeeksOnce() {
    val snapshot = mutableStateOf(PlayerSnapshot(status = PlayerStatus.READY, durationSeconds = 100.0, positionSeconds = 10.0))
    val seeks = mutableListOf<Double>()
    var dragging = false
    compose.setContent {
      MaterialTheme { Column(Modifier.width(500.dp)) { PlayerTimeline(snapshot.value, true, { dragging = it }, { seeks += it }) } }
    }
    val slider = compose.onNodeWithContentDescription(seekLabel)
    slider.performTouchInput { down(center); moveTo(centerRight - androidx.compose.ui.geometry.Offset(30f, 0f)) }
    val preview = slider.fetchSemanticsNode().config[SemanticsProperties.ProgressBarRangeInfo].current
    compose.runOnIdle {
      assertTrue(dragging)
      assertTrue(seeks.isEmpty())
      snapshot.value = snapshot.value.copy(positionSeconds = 20.0)
    }
    assertEquals(preview, slider.fetchSemanticsNode().config[SemanticsProperties.ProgressBarRangeInfo].current)
    slider.performTouchInput { up() }
    compose.runOnIdle {
      assertFalse(dragging)
      assertEquals(1, seeks.size)
      assertEquals(preview.toDouble(), seeks.single(), 0.01)
    }
  }

  @Test fun accessibilitySeekCommitsAndUnknownDurationDisablesSeeking() {
    val snapshot = mutableStateOf(PlayerSnapshot(status = PlayerStatus.READY, durationSeconds = 100.0, positionSeconds = 10.0))
    val seeks = mutableListOf<Double>()
    compose.setContent {
      MaterialTheme {
        Column(Modifier.width(500.dp)) {
          PlayerTimeline(snapshot.value, true, {}, {
            seeks += it
            snapshot.value = snapshot.value.copy(positionSeconds = it)
          })
        }
      }
    }
    val slider = compose.onNodeWithContentDescription(seekLabel)
    slider.performSemanticsAction(SemanticsActions.SetProgress) { it(75f) }
    compose.onNodeWithText("01:15").assertExists()
    compose.onNodeWithText("−00:25").assertExists()
    compose.runOnIdle {
      assertEquals(listOf(75.0), seeks)
      snapshot.value = snapshot.value.copy(durationSeconds = null)
    }
    slider.assertIsNotEnabled()
    compose.onNodeWithText("—").assertExists()
  }

  @Test fun holdingTheTimelineWithoutMovingKeepsPhoneControlsUntilRelease() {
    val snapshot = PlayerSnapshot(status = PlayerStatus.READY, paused = false, durationSeconds = 100.0, positionSeconds = 50.0)
    compose.setContent {
      val chrome = rememberPlayerChrome(1L, paused = false, phone = true, touchExploration = false)
      LaunchedEffect(Unit) { chrome.reveal() }
      MaterialTheme {
        if (chrome.visible) Column(Modifier.width(500.dp)) {
          PlayerTimeline(snapshot, true, { chrome.dragging = it }, {})
        }
      }
    }
    settleTimeline()
    val slider = compose.onNodeWithContentDescription(seekLabel)
    slider.performTouchInput { down(center) }
    settleTimeline()
    compose.mainClock.advanceTimeBy(3_100)
    settleTimeline()
    slider.assertExists()
    slider.performTouchInput { up() }
    settleTimeline()
    compose.mainClock.advanceTimeBy(3_100)
    settleTimeline()
    slider.assertDoesNotExist()
  }

  private fun settleTimeline() {
    compose.mainClock.autoAdvance = true
    compose.waitForIdle()
    compose.mainClock.autoAdvance = false
  }
}
