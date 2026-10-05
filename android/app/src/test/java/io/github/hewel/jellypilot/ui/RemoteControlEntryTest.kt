package io.github.hewel.jellypilot.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.R
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class, qualifiers = "en-rUS-w1280dp-h900dp")
class RemoteControlEntryTest {
  @get:Rule val compose = createComposeRule()
  private val context get() = InstrumentationRegistry.getInstrumentation().targetContext

  @Test fun accountEntryRequiresAnActiveAccountWithoutLoginOrCleanupInProgress() {
    val state = mutableStateOf(AppUiState())
    var opened = 0
    val actions = AccountActions({}, {}, {}, { _, _ -> }, {}, {}, {}, {}, {}, remoteControl = { opened++ })
    compose.setContent { MaterialTheme { AccountOverview(state.value, actions) } }
    val entry = compose.onNodeWithText(context.getString(R.string.remote_control))
    entry.performScrollTo().assertIsNotEnabled()
    compose.runOnIdle { state.value = state.value.copy(activeName = "Ada", activeProfileKey = "profile") }
    entry.assertIsEnabled().performClick()
    compose.runOnIdle { assertEquals(1, opened); state.value = state.value.copy(loginBusy = true) }
    entry.assertIsNotEnabled()
    compose.runOnIdle { state.value = state.value.copy(loginBusy = false, signOutCleanupPending = true) }
    entry.assertIsNotEnabled()
    compose.runOnIdle { assertEquals(1, opened) }
  }

  @Test fun selectedEpisodeHasSeparateRemoteAndLocalPlaybackActions() {
    val episodes = (1..8).map { index ->
      MediaUi("episode-$index", "Episode $index", "Episode", "2024", null, "Synopsis $index", false, false,
        episodeCode = "E0$index", playTargetId = "episode-$index", durationSeconds = 3600.0, runtimeMinutes = 60)
    }
    val series = MediaUi("series", "Series", "Series", "2024", null, "Series synopsis", false, false,
      playTargetId = "episode-3", seasons = listOf(SeasonUi("season", "Season 1")))
    val state = AppUiState(detail = series, detailItems = episodes, selectedSeasonId = "season")
    val local = mutableListOf<Pair<String, Boolean>>()
    val remote = mutableListOf<MediaUi>()
    val actions = TabletDetailActions(
      back = {}, selectSeason = {}, loadMore = {}, preview = { id -> episodes.firstOrNull { it.id == id } },
      play = { id, beginning -> local += id to beginning }, favorite = { _, _ -> }, watchlist = { _, _ -> },
      watched = { _, _ -> }, loadTracks = {}, audio = {}, subtitle = {}, detail = {}, remotePlay = remote::add,
    )
    compose.setContent { MaterialTheme { AdaptiveDetailContent(state, true, actions) { _, _ -> } } }
    compose.onNodeWithText("E04 Episode 4").performClick().assertIsSelected()
    compose.runOnIdle { assertTrue(local.isEmpty()); assertTrue(remote.isEmpty()) }
    val remoteLabel = context.getString(R.string.play_on_another_device)
    compose.onNodeWithTag("tablet-preview-scroll").performScrollToNode(hasText(remoteLabel))
    compose.onNodeWithText(remoteLabel).performClick()
    compose.runOnIdle {
      assertEquals(listOf("episode-4"), remote.map { it.id })
      assertEquals(listOf("episode-4"), remote.map { it.playTargetId })
      assertTrue(local.isEmpty())
    }
    val localLabel = context.getString(R.string.episode_play, "E04")
    compose.onNodeWithTag("tablet-preview-scroll").performScrollToNode(hasText(localLabel))
    compose.onNodeWithText(localLabel).performClick()
    compose.runOnIdle {
      assertEquals(listOf("episode-4" to false), local)
      assertEquals(1, remote.size)
    }
  }
}
