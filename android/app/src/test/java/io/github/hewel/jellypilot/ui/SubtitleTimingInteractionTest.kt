package io.github.hewel.jellypilot.ui

import android.app.Application
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.R
import io.github.hewel.jellypilot.player.PlayerSnapshot
import io.github.hewel.jellypilot.player.PlayerStatus
import io.github.hewel.jellypilot.player.PlayerTrack
import io.github.hewel.jellypilot.player.SubtitleTimingAvailability
import io.github.hewel.jellypilot.player.SubtitleTimingContext
import io.github.hewel.jellypilot.player.SubtitleTimingState
import io.github.hewel.jellypilot.player.TrackKind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class, qualifiers = "en-rUS-w667dp-h375dp")
class SubtitleTimingInteractionTest {
  @get:Rule val compose = createComposeRule()
  private val context get() = InstrumentationRegistry.getInstrumentation().targetContext
  private fun text(id: Int) = context.getString(id)
  private val timingContext = SubtitleTimingContext(4, 8, 1)
  private val snapshot = mutableStateOf(PlayerSnapshot(status = PlayerStatus.READY, paused = true,
    tracks = listOf(PlayerTrack(8, TrackKind.SUBTITLE, "English subtitles", "eng", "srt",
      isDefault = true, isForced = false, isExternal = false, isSelected = true)),
    subtitleTiming = SubtitleTimingState(context = timingContext, availability = SubtitleTimingAvailability.AVAILABLE)))
  private val requests = mutableListOf<Pair<SubtitleTimingContext, Int>>()
  private val panel = mutableStateOf<PlayerPanel?>(PlayerPanel.SubtitleTiming)

  private fun actions(apply: (SubtitleTimingContext, Int) -> Boolean = { track, offset -> requests += track to offset; true }) = PlayerPanelActions(
    selectTrack = { _, _ -> }, selectEpisode = {}, previousEpisode = {}, nextEpisode = {}, loadMoreEpisodes = {},
    setVolume = {}, close = { panel.value = null }, setSubtitleTiming = apply,
  )

  private fun mount(acknowledgeImmediately: Boolean = false) {
    compose.setContent {
      MaterialTheme {
        Box(Modifier.size(340.dp, 343.dp)) {
          panel.value?.let { selected ->
            PlayerPanelContent(selected, snapshot.value, null, actions { track, offset ->
              requests += track to offset
              if (acknowledgeImmediately) settleOffset(offset)
              true
            }, phone = true)
          }
        }
      }
    }
    compose.waitForIdle()
  }

  private fun settleOffset(offset: Int, failed: Boolean = false) {
    snapshot.value = snapshot.value.copy(subtitleTiming = snapshot.value.subtitleTiming.copy(
      offsetTenths = offset, pending = false, failed = failed,
      requestRevision = snapshot.value.subtitleTiming.requestRevision + 1,
    ))
  }

  private fun later() = compose.onNodeWithContentDescription(text(R.string.subtitle_timing_later_accessibility))
  private fun earlier() = compose.onNodeWithContentDescription(text(R.string.subtitle_timing_earlier_accessibility))
  private fun reset() = compose.onNodeWithContentDescription(text(R.string.subtitle_timing_reset_accessibility))
  private fun value() = compose.onNodeWithTag("subtitle-timing-value")

  // Compose v2 schedules UI work separately from the test clock. Commit lifecycle/context
  // changes before advancing a repeat deadline, as the existing chrome timer tests do.
  private fun settleComposition() {
    compose.mainClock.autoAdvance = true
    compose.waitForIdle()
    compose.mainClock.autoAdvance = false
  }

  @Test fun onlyConfirmedOffsetsAppearAndConflatedRepeatedFailuresCanRetry() {
    mount()
    value().assertTextEquals("0.0 s")
    reset().assertIsNotEnabled()
    later().performClick()
    later().assertIsNotEnabled()
    earlier().assertIsNotEnabled()
    value().assertTextEquals("0.0 s")
    compose.runOnIdle { settleOffset(0, failed = true) }
    compose.onNodeWithText(text(R.string.subtitle_timing_failure)).performScrollTo().assertIsDisplayed()
    later().performScrollTo().assertIsEnabled().performClick()
    later().assertIsNotEnabled()
    compose.runOnIdle { settleOffset(0, failed = true) }
    later().assertIsEnabled().performClick()
    compose.runOnIdle { settleOffset(1) }
    value().assertTextEquals("+0.1 s")
    compose.onNodeWithText(text(R.string.subtitle_timing_failure)).assertDoesNotExist()
    reset().performScrollTo().assertIsEnabled().performClick()
    value().assertTextEquals("+0.1 s")
    compose.runOnIdle { settleOffset(0) }
    reset().assertIsNotEnabled()
    compose.runOnIdle {
      assertEquals(listOf(1, 1, 1, 0), requests.map { it.second })
      assertTrue(requests.all { it.first == timingContext })
      assertTrue(snapshot.value.paused)
    }
  }

  @Test fun endpointsDisableOnlyTheirDirectionAndNeverIssueOutOfRangeRequests() {
    snapshot.value = snapshot.value.copy(subtitleTiming = snapshot.value.subtitleTiming.copy(offsetTenths = 99))
    mount(acknowledgeImmediately = true)
    later().performClick().assertIsNotEnabled()
    value().assertTextEquals("+10.0 s")
    earlier().assertIsEnabled().performClick()
    value().assertTextEquals("+9.9 s")
    compose.runOnIdle { settleOffset(-99) }
    earlier().performClick().assertIsNotEnabled()
    value().assertTextEquals("-10.0 s")
    later().assertIsEnabled()
    compose.runOnIdle { assertEquals(listOf(100, 99, -100), requests.map { it.second }) }
  }

  @Test fun holdingWaitsForAcknowledgmentThenRepeatsAndCancellationStopsFurtherSteps() {
    mount()
    compose.mainClock.autoAdvance = false
    later().performTouchInput { down(center) }
    compose.mainClock.advanceTimeBy(350)
    compose.runOnIdle { assertEquals(1, requests.size) }
    compose.mainClock.advanceTimeBy(500)
    compose.runOnIdle { assertEquals(1, requests.size); settleOffset(1) }
    compose.mainClock.advanceTimeBy(150)
    compose.runOnIdle { assertEquals(listOf(1, 2), requests.map { it.second }) }
    later().performTouchInput { cancel() }
    compose.runOnIdle { settleOffset(2) }
    compose.mainClock.advanceTimeBy(1_000)
    compose.runOnIdle { assertEquals(2, requests.size) }
  }

  @Test fun successfulHoldUsesInitialDelayAndStopsAtSelectionReplacementAndDismissal() {
    mount(acknowledgeImmediately = true)
    compose.mainClock.autoAdvance = false
    later().performTouchInput { down(center) }
    compose.mainClock.advanceTimeBy(320)
    compose.runOnIdle { assertEquals(1, requests.size) }
    compose.mainClock.advanceTimeBy(130)
    compose.runOnIdle { assertEquals(2, requests.size) }
    compose.mainClock.advanceTimeBy(100)
    compose.runOnIdle { assertEquals("Requests before replacement: $requests", 3, requests.size) }
    compose.runOnIdle {
      snapshot.value = snapshot.value.copy(subtitleTiming = snapshot.value.subtitleTiming.copy(
        context = timingContext.copy(revision = 3), offsetTenths = 0, requestRevision = 0,
      ))
    }
    settleComposition()
    value().assertTextEquals("0.0 s")
    compose.mainClock.advanceTimeBy(1_000)
    compose.runOnIdle { assertEquals("Requests after replacement: $requests", 3, requests.size) }
    later().performTouchInput { up() }
    later().performTouchInput { down(center) }
    compose.mainClock.advanceTimeBy(50)
    compose.runOnIdle { assertEquals(4, requests.size); panel.value = null }
    settleComposition()
    compose.mainClock.advanceTimeBy(1_000)
    compose.runOnIdle { assertEquals(4, requests.size) }
  }

  @Test fun keyboardHoldStopsWhenFocusMovesAway() {
    mount(acknowledgeImmediately = true)
    later().performSemanticsAction(SemanticsActions.RequestFocus) { it() }
    later().assertIsFocused()
    compose.mainClock.autoAdvance = false
    later().performKeyInput { keyDown(Key.Spacebar) }
    compose.mainClock.advanceTimeBy(450)
    compose.runOnIdle { assertEquals(2, requests.size) }
    earlier().performSemanticsAction(SemanticsActions.RequestFocus) { it() }
    earlier().assertIsFocused()
    compose.mainClock.advanceTimeBy(1_000)
    compose.runOnIdle { assertEquals(2, requests.size) }
    earlier().performKeyInput { keyUp(Key.Spacebar) }
  }

  @Test fun unavailableEntryExplainsRealCapabilityAndNeverNavigates() {
    val availability = mutableStateOf(SubtitleTimingAvailability.NO_TRACKS)
    var opened = 0
    compose.setContent {
      MaterialTheme {
        Box(Modifier.size(340.dp, 343.dp)) {
          PlayerPanelContent(PlayerPanel.Subtitles,
            snapshot.value.copy(subtitleTiming = SubtitleTimingState(availability = availability.value)), null,
            actions().copy(open = { opened++ }), phone = true)
        }
      }
    }
    compose.onNodeWithText(text(R.string.subtitle_timing_no_tracks)).assertIsDisplayed()
    compose.onNodeWithTag("subtitle-timing-entry").assertIsNotEnabled()
    compose.runOnIdle { availability.value = SubtitleTimingAvailability.SUBTITLES_OFF }
    compose.onNodeWithText(text(R.string.subtitle_timing_off)).assertIsDisplayed()
    compose.runOnIdle { availability.value = SubtitleTimingAvailability.UNSUPPORTED }
    compose.onNodeWithText(text(R.string.subtitle_timing_unsupported)).assertIsDisplayed()
    compose.onNodeWithTag("subtitle-timing-entry").assertIsNotEnabled()
    compose.runOnIdle { assertEquals(0, opened) }
  }

  @Test fun enlargedTimingScrollsBelowFixedHeaderAndBackRestoresFooterBeforeCloseReturnsToSubtitleTrigger() {
    val chrome = PlayerChromeState(visible = true, paused = true).apply { open(PlayerPanel.Subtitles) }
    compose.setContent {
      MaterialTheme {
        CompositionLocalProvider(LocalDensity provides Density(LocalDensity.current.density, 2f)) {
          Box(Modifier.size(340.dp, 343.dp)) {
            chrome.panel?.let { selected ->
              PlayerPanelContent(selected, snapshot.value, null, actions().copy(
                close = chrome::close, open = chrome::open,
                back = if (selected == PlayerPanel.SubtitleTiming) chrome::back else null,
              ), restoreRow = chrome.restoreRow, phone = true)
            }
          }
        }
      }
    }
    compose.onNodeWithTag("subtitle-timing-entry").assertIsDisplayed().performClick()
    reset().performScrollTo().assertIsDisplayed()
    compose.onNodeWithContentDescription(text(R.string.close)).assertIsDisplayed()
    compose.onNodeWithContentDescription(text(R.string.back)).assertIsDisplayed().performClick()
    compose.onNodeWithTag("subtitle-timing-entry").assertIsFocused().performClick()
    compose.onNodeWithContentDescription(text(R.string.close)).performClick()
    compose.runOnIdle {
      assertEquals(null, chrome.panel)
      assertEquals(PlayerPanel.Subtitles, chrome.restoreTrigger)
      assertTrue(chrome.visible)
      assertTrue(snapshot.value.paused)
    }
  }
}
