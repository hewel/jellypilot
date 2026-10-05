package io.github.hewel.jellypilot.ui

import android.app.Application
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class, qualifiers = "w1280dp-h900dp")
class TabletDetailInteractionTest {
  @get:Rule val compose = createComposeRule()

  private val episodes = (1..8).map { index ->
    MediaUi("episode-$index", "Episode $index", "Episode", "2024", null, "Synopsis $index", false, false,
      episodeCode = "E0$index", playTargetId = "episode-$index", durationSeconds = 3600.0, runtimeMinutes = 60)
  }
  private val detail = MediaUi("series", "Series", "Series", "2024", null, "Series synopsis", false, false,
    playTargetId = "episode-3", seasons = listOf(SeasonUi("season", "Season 1")))
  private val state = AppUiState(detail = detail, detailItems = episodes, selectedSeasonId = "season")
  private fun actions(play: (String, Boolean) -> Unit = { _, _ -> }, tracks: (String) -> Unit = {}) = TabletDetailActions(
    back = {}, selectSeason = {}, loadMore = {}, preview = { id -> episodes.firstOrNull { it.id == id } },
    play = play, favorite = { _, _ -> }, watchlist = { _, _ -> }, watched = { _, _ -> },
    loadTracks = tracks, audio = {}, subtitle = {}, detail = {},
  )

  @Test fun selectingEpisodeDoesNotPlayAndSelectionSurvivesWidePortraitAndCompactWindows() {
    val width = mutableStateOf(1100.dp)
    val tablet = mutableStateOf(true)
    val played = mutableListOf<String>()
    val actions = actions(play = { id, _ -> played += id })
    compose.setContent {
      MaterialTheme {
        Box(Modifier.width(width.value).fillMaxHeight()) {
          AdaptiveDetailContent(state, tablet.value, actions) { compact, _ -> Text("Compact: ${compact.detail?.id}") }
        }
      }
    }
    compose.onNodeWithTag("tablet-detail-two-panes").assertExists()
    compose.onNodeWithText("E04 Episode 4").performClick().assertIsSelected()
    compose.onNodeWithText("E04 · Episode 4").assertExists()
    compose.runOnIdle { assertTrue(played.isEmpty()); width.value = 754.dp }
    compose.onNodeWithTag("tablet-detail-one-pane").assertExists()
    compose.onNodeWithTag("tablet-detail-scroll").performScrollToNode(hasText("E04 Episode 4"))
    compose.onNodeWithText("E04 Episode 4").assertIsSelected()
    compose.onNodeWithContentDescription("Play E04").performClick()
    compose.runOnIdle { assertEquals(listOf("episode-4"), played); tablet.value = false; width.value = 390.dp }
    compose.onNodeWithText("Compact: episode-4").assertExists()
    compose.runOnIdle { tablet.value = true; width.value = 1100.dp }
    compose.onNodeWithText("E04 Episode 4").assertIsSelected()
    compose.onNodeWithText("E04 · Episode 4").assertExists()
  }

  @Test fun staticPreviewHasNoPlayActionAndSelectedEpisodeOwnsTrackRequest() {
    val tracks = mutableListOf<String>()
    compose.setContent { MaterialTheme { AdaptiveDetailContent(state, true, actions(tracks = { tracks += it })) { _, _ -> } } }
    compose.onNodeWithTag("tablet-static-preview").assert(hasNoClickAction())
    compose.onNodeWithText("E05 Episode 5").performClick()
    compose.onNodeWithTag("tablet-preview-scroll").performScrollToNode(hasText("Audio tracks"))
    compose.onNodeWithText("Audio tracks").performClick()
    compose.runOnIdle { assertEquals(listOf("episode-5"), tracks) }
  }

  @Test fun independentEpisodeScrollReturnsAfterTemporarySinglePaneLayout() {
    val width = mutableStateOf(1100.dp)
    compose.setContent {
      MaterialTheme { Box(Modifier.width(width.value).height(600.dp)) { AdaptiveDetailContent(state, true, actions()) { _, _ -> } } }
    }
    compose.onNodeWithTag("tablet-episodes-scroll").performScrollToNode(hasText("E08 Episode 8"))
    val before = compose.onNodeWithTag("tablet-episodes-scroll").fetchSemanticsNode().config[SemanticsProperties.VerticalScrollAxisRange].value()
    assertTrue(before > 0f)
    compose.runOnIdle { width.value = 754.dp }
    compose.onNodeWithTag("tablet-detail-scroll").performScrollToNode(hasText("E04 Episode 4"))
    compose.runOnIdle { width.value = 1100.dp }
    val after = compose.onNodeWithTag("tablet-episodes-scroll").fetchSemanticsNode().config[SemanticsProperties.VerticalScrollAxisRange].value()
    assertEquals(before, after, 0.01f)
  }

  @Test fun lateEpisodeMetadataCannotReplaceTheNewSelectionAndFailureCanRetry() {
    val old = CompletableDeferred<MediaUi?>()
    var attempts = 0
    val actions = actions().copy(preview = { id ->
      if (id == "episode-3") withContext(NonCancellable) { old.await() }
      else {
        attempts++
        if (attempts == 1) error("Request failed")
        episodes.first { it.id == id }.copy(overview = "Fresh selected episode metadata")
      }
    })
    compose.setContent { MaterialTheme { AdaptiveDetailContent(state, true, actions) { _, _ -> } } }
    compose.onNodeWithText("E04 Episode 4").performClick()
    compose.onNodeWithTag("tablet-preview-scroll").performScrollToNode(hasText("Retry"))
    compose.onNodeWithText("Retry").performClick()
    compose.onNodeWithTag("tablet-preview-scroll").performScrollToNode(hasText("Fresh selected episode metadata"))
    compose.runOnIdle { old.complete(episodes[2].copy(overview = "Late old metadata")) }
    compose.onNodeWithText("Fresh selected episode metadata").assertExists()
    compose.onNodeWithText("Late old metadata").assertDoesNotExist()
    compose.onNodeWithText("E04 Episode 4").assertIsSelected()
    compose.runOnIdle { assertEquals(2, attempts) }
  }

  @Test fun switchingToAnEmptyOrFailedSeasonCannotPlayTheOldSeasonResumeTarget() {
    val current = mutableStateOf(state.copy(detail = detail.copy(resumeSeconds = 300.0)))
    val tablet = mutableStateOf(true)
    val played = mutableListOf<String>()
    compose.setContent {
      MaterialTheme {
        AdaptiveDetailContent(current.value, tablet.value, actions(play = { id, _ -> played += id })) { compact, _ ->
          Text("Compact playable: ${compact.detail?.playable}")
        }
      }
    }
    compose.runOnIdle { current.value = current.value.copy(selectedSeasonId = "new-season", detailItems = emptyList(), busy = true) }
    compose.onAllNodesWithText("Play from beginning").assertCountEquals(0)
    compose.onNodeWithText("Continue watching").assertIsNotEnabled()
    compose.onNodeWithText("Mark watched").assertIsNotEnabled()
    compose.onNodeWithText("Add to watch later").assertIsNotEnabled()
    compose.runOnIdle { current.value = current.value.copy(busy = false, error = "Season failed") }
    compose.onAllNodesWithText("Play from beginning").assertCountEquals(0)
    compose.onNodeWithText("Continue watching").assertIsNotEnabled()
    compose.onNodeWithText("Mark watched").assertIsNotEnabled()
    compose.runOnIdle { tablet.value = false }
    compose.onNodeWithText("Compact playable: false").assertExists()
    compose.runOnIdle { assertTrue(played.isEmpty()) }
  }

  @Test fun restoredOffPageSelectionWaitsForItsOwnMetadataInsteadOfPlayingTheSeriesResumeTarget() {
    val open = mutableStateOf(true)
    val tablet = mutableStateOf(true)
    val current = mutableStateOf(state)
    val restored = CompletableDeferred<MediaUi?>()
    var reopening = false
    val requested = mutableListOf<String>()
    val written = mutableListOf<String>()
    val actions = actions().copy(preview = { id ->
      requested += id
      if (reopening && id == "episode-8") restored.await() else episodes.firstOrNull { it.id == id }
    }, watched = { id, _ -> written += id }, watchlist = { id, _ -> written += id })
    compose.setContent {
      val holder = rememberSaveableStateHolder()
      MaterialTheme {
        if (open.value) holder.SaveableStateProvider("series") {
          AdaptiveDetailContent(current.value, tablet.value, actions) { compact, actionsEnabled ->
            Button(onClick = { written += requireNotNull(compact.detail).id }, enabled = actionsEnabled) { Text("Compact mark watched") }
          }
        }
      }
    }
    compose.onNodeWithTag("tablet-episodes-scroll").performScrollToNode(hasText("E08 Episode 8"))
    compose.onNodeWithText("E08 Episode 8").performClick()
    compose.runOnIdle { open.value = false }
    compose.waitForIdle()
    compose.runOnIdle {
      reopening = true
      current.value = state.copy(detailItems = episodes.take(3))
      open.value = true
    }
    compose.onNodeWithText("Play").assertIsNotEnabled()
    compose.onAllNodesWithText("Play from beginning").assertCountEquals(0)
    compose.onNodeWithText("Mark watched").assertIsNotEnabled()
    compose.onNodeWithText("Add to watch later").assertIsNotEnabled()
    compose.runOnIdle { tablet.value = false }
    compose.onNodeWithText("Compact mark watched").assertIsNotEnabled()
    compose.runOnIdle { assertEquals("episode-8", requested.last()); assertTrue(written.isEmpty()); restored.complete(episodes[7].copy(playable = false)) }
    compose.onNodeWithText("Compact mark watched").assertIsEnabled().performClick()
    compose.runOnIdle { assertEquals(listOf("episode-8"), written); written.clear(); tablet.value = true }
    compose.onNodeWithText("E08 · Episode 8").assertExists()
    compose.onNodeWithText("Play E08").assertIsNotEnabled()
    compose.onNodeWithText("Mark watched").assertIsEnabled().performClick()
    compose.onNodeWithText("Add to watch later").assertIsEnabled().performClick()
    compose.runOnIdle { assertEquals(listOf("episode-8", "episode-8"), written) }
  }
}
