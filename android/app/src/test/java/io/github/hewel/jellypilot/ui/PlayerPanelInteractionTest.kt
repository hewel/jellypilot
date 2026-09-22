package io.github.hewel.jellypilot.ui

import android.app.Application
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.hasScrollAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.R
import io.github.hewel.jellypilot.player.PlayerHost
import io.github.hewel.jellypilot.player.PlayerSnapshot
import io.github.hewel.jellypilot.player.PlayerStatus
import io.github.hewel.jellypilot.player.PlayerTrack
import io.github.hewel.jellypilot.player.TrackKind
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class, qualifiers = "en-rUS-w390dp-h844dp")
class PlayerPanelInteractionTest {
  @get:Rule val compose = createComposeRule()
  private val context get() = InstrumentationRegistry.getInstrumentation().targetContext

  @Test fun audioSelectionUsesMpvIdentityAndKeepsThePanelOpen() {
    val snapshot = mutableStateOf(PlayerSnapshot(tracks = listOf(
      track(41, TrackKind.AUDIO, "Original audio", selected = true).copy(isDefault = true),
      track(208, TrackKind.AUDIO, "Commentary"),
      track(9, TrackKind.SUBTITLE, "English subtitles"),
    )))
    val choices = mutableListOf<Pair<TrackKind, Int>>()
    var closes = 0
    compose.setContent {
      PanelHost {
        PlayerPanelContent(PlayerPanel.Audio, snapshot.value, null, actions(
          selectTrack = { kind, id ->
            choices += kind to id
            snapshot.value = snapshot.value.copy(tracks = snapshot.value.tracks.map {
              if (it.kind == kind) it.copy(isSelected = it.mpvId == id) else it
            })
          },
          close = { closes++ },
        ), phone = true)
      }
    }
    compose.onNodeWithText("English subtitles").assertDoesNotExist()
    compose.onNodeWithText("Original audio").assertIsSelected()
    compose.onNodeWithText("Commentary").performClick().assertIsSelected()
    compose.onNodeWithText("Original audio").assertIsNotSelected()
    compose.onNodeWithText(context.getString(R.string.player_track_default), substring = true).assertIsDisplayed()
    compose.onNodeWithContentDescription(context.getString(R.string.close)).assertIsDisplayed()
    compose.runOnIdle {
      assertEquals(listOf(TrackKind.AUDIO to 208), choices)
      assertEquals(0, closes)
      snapshot.value = snapshot.value.copy(tracks = snapshot.value.tracks.map {
        it.copy(isSelected = it.kind == TrackKind.AUDIO && it.mpvId == 41)
      })
    }
    compose.onNodeWithText("Original audio").assertIsSelected()
    compose.onNodeWithText("Commentary").assertIsNotSelected()
    compose.onNodeWithContentDescription(context.getString(R.string.close)).performClick()
    compose.runOnIdle { assertEquals(1, closes) }
  }

  @Test fun subtitleOffClearsOnlySubtitleSelectionWithoutDismissingThePanel() {
    val snapshot = mutableStateOf(PlayerSnapshot(tracks = listOf(
      track(41, TrackKind.AUDIO, "Original audio", selected = true),
      track(208, TrackKind.SUBTITLE, "English subtitles"),
    )))
    val choices = mutableListOf<Pair<TrackKind, Int>>()
    var closes = 0
    compose.setContent {
      PanelHost {
        PlayerPanelContent(PlayerPanel.Subtitles, snapshot.value, null, actions(
          selectTrack = { kind, id ->
            choices += kind to id
            snapshot.value = snapshot.value.copy(tracks = snapshot.value.tracks.map {
              if (it.kind == kind) it.copy(isSelected = it.mpvId == id) else it
            })
          },
          close = { closes++ },
        ))
      }
    }
    compose.onNodeWithText("Original audio").assertDoesNotExist()
    compose.onNodeWithText(context.getString(R.string.subtitles_off)).assertIsSelected()
    compose.onNodeWithText("English subtitles").performClick().assertIsSelected()
    compose.onNodeWithText(context.getString(R.string.subtitles_off)).performClick().assertIsSelected()
    compose.onNodeWithText("English subtitles").assertIsNotSelected()
    compose.runOnIdle {
      assertEquals(listOf(TrackKind.SUBTITLE to 208, TrackKind.SUBTITLE to PlayerHost.TRACK_ID_NONE), choices)
      assertEquals(true, snapshot.value.tracks.single { it.kind == TrackKind.AUDIO }.isSelected)
      assertEquals(0, closes)
    }
  }

  @Test fun untitledTracksUseLocalizedLanguageAndRemainSelectableByTheirMpvId() {
    val snapshot = mutableStateOf(PlayerSnapshot(tracks = listOf(
      track(41, TrackKind.AUDIO, "").copy(title = null, language = "eng"),
      track(208, TrackKind.AUDIO, " ").copy(language = "zh"),
      track(9, TrackKind.AUDIO, "").copy(title = null, language = null),
    )))
    var selected: Int? = null
    compose.setContent {
      PanelHost {
        PlayerPanelContent(PlayerPanel.Audio, snapshot.value, null, actions(selectTrack = { _, id ->
          selected = id
          snapshot.value = snapshot.value.copy(tracks = snapshot.value.tracks.map {
            it.copy(isSelected = it.mpvId == id)
          })
        }))
      }
    }
    compose.onAllNodesWithText("English", useUnmergedTree = true).assertCountEquals(1)
    compose.onAllNodesWithText("Chinese", useUnmergedTree = true).assertCountEquals(1)
    compose.onNodeWithText(context.getString(R.string.player_track_number, 9)).assertIsDisplayed()
    compose.onNodeWithText("English").performClick().assertIsSelected()
    compose.runOnIdle { assertEquals(41, selected) }
    compose.onNodeWithText("Chinese").performClick().assertIsSelected()
    compose.runOnIdle { assertEquals(208, selected) }
  }

  @Test fun enlargedLongListsScrollWithPersistentHeaderCloseAndContinuationCue() {
    val snapshot = mutableStateOf(PlayerSnapshot(tracks = (1..40).map {
      track(it * 7, TrackKind.AUDIO, "Audio track $it")
    }))
    var selected: Int? = null
    compose.setContent {
      CompositionLocalProvider(LocalDensity provides Density(LocalDensity.current.density, fontScale = 2f)) {
        PanelHost {
          PlayerPanelContent(PlayerPanel.Audio, snapshot.value, null, actions(selectTrack = { _, id ->
            selected = id
            snapshot.value = snapshot.value.copy(tracks = snapshot.value.tracks.map {
              it.copy(isSelected = it.mpvId == id)
            })
          }))
        }
      }
    }
    compose.onNodeWithText("below", substring = true).assertIsDisplayed()
    compose.onNode(hasScrollAction()).performScrollToNode(hasText("Audio track 40"))
    compose.onNodeWithText("Audio track 40").performClick().assertIsSelected()
    compose.onNodeWithText(context.getString(R.string.audio_tracks)).assertIsDisplayed()
    compose.onNodeWithContentDescription(context.getString(R.string.close)).assertIsDisplayed()
    compose.onNodeWithText("below", substring = true).assertDoesNotExist()
    compose.runOnIdle { assertEquals(280, selected) }
  }

  @Test fun episodeNavigationAndPaginationRemainAvailableAndSelectingAnEpisodeCloses() {
    val completed = episode("completed", "Completed episode", played = true)
    val playback = mutableStateOf(PlaybackUi(
      title = "Series", queue = listOf(episode("current", "Current episode"), completed),
      currentItemId = "current", queueHasMore = true, canNext = true,
    ))
    var nextCalls = 0
    var pageCalls = 0
    var closes = 0
    var selected: MediaUi? = null
    compose.setContent {
      PanelHost {
        PlayerPanelContent(PlayerPanel.Queue, PlayerSnapshot(), playback.value, actions(
          selectEpisode = { selected = it },
          nextEpisode = { nextCalls++ },
          loadMoreEpisodes = { pageCalls++; playback.value = playback.value.copy(queueLoading = true) },
          close = { closes++ },
        ))
      }
    }
    compose.onNodeWithText(context.getString(R.string.previous_episode)).assertIsNotEnabled()
    compose.onNodeWithText(context.getString(R.string.next_episode)).performClick()
    compose.onNodeWithText("Current episode").assertIsSelected()
    compose.onNodeWithText(context.getString(R.string.load_more)).performClick().assertIsNotEnabled()
    compose.onNodeWithText("Completed episode").performClick()
    compose.runOnIdle {
      assertEquals(1, nextCalls)
      assertEquals(1, pageCalls)
      assertEquals(completed, selected)
      assertEquals(1, closes)
    }
  }

  @Test fun videoPanelPreservesAccessibleVolumeControl() {
    val snapshot = mutableStateOf(PlayerSnapshot(videoWidth = 1920, videoHeight = 1080, volumePercent = 80, volumeAvailable = true))
    var requestedVolume: Int? = null
    compose.setContent {
      PanelHost {
        PlayerPanelContent(PlayerPanel.Video, snapshot.value, null, actions(setVolume = {
          requestedVolume = it
          snapshot.value = snapshot.value.copy(volumePercent = it)
        }))
      }
    }
    compose.onNodeWithText(context.getString(R.string.video_resolution, 1920, 1080)).assertIsDisplayed()
    compose.onNodeWithContentDescription(context.getString(R.string.volume))
      .performSemanticsAction(SemanticsActions.SetProgress) { it(35f) }
    compose.runOnIdle { assertEquals(35, requestedVolume) }
  }

  @Test fun moreUsesLiveVolumeSkipAndRateWithoutChangingPausedPlayback() {
    val snapshot = mutableStateOf(PlayerSnapshot(status = PlayerStatus.READY, paused = true, volumePercent = 70, volumeAvailable = true, speed = 1.0))
    val playback = mutableStateOf(PlaybackUi(title = "Episode", autoSkipAvailable = true, autoSkipEnabled = true))
    val panel = mutableStateOf(PlayerPanel.More)
    val rates = mutableListOf<Double>()
    compose.setContent {
      PanelHost {
        PlayerPanelContent(panel.value, snapshot.value, playback.value, PlayerPanelActions(
          selectTrack = { _, _ -> }, selectEpisode = {}, previousEpisode = {}, nextEpisode = {}, loadMoreEpisodes = {},
          setVolume = { snapshot.value = snapshot.value.copy(volumePercent = it) }, close = {}, open = { panel.value = it },
          setAutoSkip = { playback.value = playback.value.copy(autoSkipEnabled = it) },
          setSpeed = { rates += it; snapshot.value = snapshot.value.copy(speed = it) },
        ), phone = true)
      }
    }
    compose.onNodeWithContentDescription(context.getString(R.string.volume))
      .performSemanticsAction(SemanticsActions.SetProgress) { it(35f) }
    compose.onNodeWithText(context.getString(R.string.player_volume_percent, 35)).assertExists()
    compose.onNodeWithText(context.getString(R.string.player_auto_skip_full)).performClick()
    compose.onNodeWithText(context.getString(R.string.player_playback_speed)).performClick()
    compose.onNodeWithText(context.getString(R.string.player_speed_value, "1.5")).performClick().assertIsSelected()
    compose.runOnIdle {
      assertEquals(listOf(1.5), rates)
      assertEquals(false, playback.value.autoSkipEnabled)
      assertEquals(true, snapshot.value.paused)
      // A remote/native observation is authoritative after the local request.
      snapshot.value = snapshot.value.copy(speed = 2.0)
    }
    compose.onNodeWithText(context.getString(R.string.player_speed_value, "2")).assertIsSelected()
    compose.onNodeWithText(context.getString(R.string.player_speed_value, "1.5")).assertIsNotSelected()
  }

  @Test fun nestedVideoBackRestoresMoreRowAndCloseExitsTheWholePanelStack() {
    val chrome = PlayerChromeState(visible = true, paused = true).apply { open(PlayerPanel.More) }
    val snapshot = PlayerSnapshot(paused = true, videoWidth = 1920, videoHeight = 1080)
    compose.setContent {
      PanelHost {
        chrome.panel?.let { panel ->
          PlayerPanelContent(panel, snapshot, PlaybackUi(title = "Episode", autoSkipAvailable = true), PlayerPanelActions(
            selectTrack = { _, _ -> }, selectEpisode = {}, previousEpisode = {}, nextEpisode = {}, loadMoreEpisodes = {},
            setVolume = {}, close = chrome::close, open = chrome::open,
            back = if (panel != PlayerPanel.More) chrome::back else null,
          ), restoreRow = chrome.restoreRow, phone = true)
        }
      }
    }
    compose.onNode(hasScrollAction()).performScrollToNode(hasText(context.getString(R.string.player_video_details)))
    compose.onNodeWithText(context.getString(R.string.player_video_details)).performClick()
    compose.onNodeWithText(context.getString(R.string.video_resolution, 1920, 1080)).assertIsDisplayed()
    compose.onNodeWithContentDescription(context.getString(R.string.back)).performClick()
    compose.onNodeWithText(context.getString(R.string.player_video_details)).assertIsFocused().performClick()
    compose.onNodeWithContentDescription(context.getString(R.string.close)).performClick()
    compose.runOnIdle {
      assertEquals(null, chrome.panel)
      assertEquals(PlayerPanel.More, chrome.restoreTrigger)
      assertEquals(true, chrome.visible)
      assertEquals(true, snapshot.paused)
    }
  }

  @Test fun directAudioSelectionCloseAndPlatformBackReturnToAudioWithoutChangingPause() {
    val chrome = PlayerChromeState(visible = true, paused = true).apply { open(PlayerPanel.Audio) }
    val snapshot = PlayerSnapshot(paused = true, tracks = listOf(track(41, TrackKind.AUDIO, "Original audio", true)))
    var selection: Pair<TrackKind, Int>? = null
    compose.setContent {
      PanelHost {
        chrome.panel?.let { panel ->
          PlayerPanelContent(panel, snapshot, null, actions(
            selectTrack = { kind, id -> selection = kind to id }, close = chrome::close,
          ), phone = true)
        }
      }
    }
    compose.onNodeWithContentDescription(context.getString(R.string.back)).assertDoesNotExist()
    compose.onNodeWithText("Original audio").assertIsSelected().performClick()
    compose.runOnIdle {
      assertEquals(TrackKind.AUDIO to 41, selection)
      assertEquals(PlayerPanel.Audio, chrome.panel)
    }
    compose.onNodeWithContentDescription(context.getString(R.string.close)).performClick()
    compose.runOnIdle {
      assertEquals(null, chrome.panel)
      assertEquals(PlayerPanel.Audio, chrome.restoreTrigger)
      chrome.restoredFocus()
      chrome.open(PlayerPanel.Audio)
      chrome.back()
      assertEquals(null, chrome.panel)
      assertEquals(PlayerPanel.Audio, chrome.restoreTrigger)
      assertEquals(true, chrome.visible)
      assertEquals(true, snapshot.paused)
    }
  }

  @Test fun pictureUsesConfirmedNativeValueAndDisablesUnavailableAdjustmentWithoutResuming() {
    val snapshot = mutableStateOf(PlayerSnapshot(status = PlayerStatus.READY, paused = true, speed = 1.5,
      pictureBrightnessAvailable = true, pictureBrightnessPercent = 80))
    val requests = mutableListOf<Int>()
    compose.setContent {
      PanelHost {
        PlayerPanelContent(PlayerPanel.Picture, snapshot.value, null,
          actions().copy(setPictureBrightness = { requests += it }), phone = true)
      }
    }
    val brightness = context.getString(R.string.player_picture_brightness)
    compose.onNodeWithContentDescription(brightness)
      .performSemanticsAction(SemanticsActions.SetProgress) { it(35f) }
    compose.runOnIdle {
      assertEquals(listOf(35), requests)
      assertEquals(true, snapshot.value.paused)
      assertEquals(1.5, snapshot.value.speed, 0.0)
    }
    // The request is not treated as confirmation; only observed native state changes the label.
    compose.onNodeWithText(context.getString(R.string.player_volume_percent, 80)).assertIsDisplayed()
    compose.runOnIdle { snapshot.value = snapshot.value.copy(pictureBrightnessPercent = 35) }
    compose.onNodeWithText(context.getString(R.string.player_volume_percent, 35)).assertIsDisplayed()
    compose.runOnIdle { snapshot.value = snapshot.value.copy(pictureBrightnessAvailable = false) }
    compose.onNodeWithContentDescription(brightness).assertIsNotEnabled()
    compose.onNodeWithText(context.getString(R.string.player_picture_unavailable)).assertIsDisplayed()
  }

  @Test fun pictureAndSpeedWaitForTheLoadedFileBeforeAcceptingCommands() {
    val snapshot = mutableStateOf(PlayerSnapshot(status = PlayerStatus.LOADING, pictureBrightnessAvailable = true))
    val panel = mutableStateOf(PlayerPanel.Picture)
    compose.setContent {
      PanelHost { PlayerPanelContent(panel.value, snapshot.value, null, actions(), phone = true) }
    }
    val brightness = context.getString(R.string.player_picture_brightness)
    val rate = context.getString(R.string.player_speed_value, "1.5")
    compose.onNodeWithContentDescription(brightness).assertIsNotEnabled()
    compose.runOnIdle { panel.value = PlayerPanel.Speed }
    compose.onNodeWithText(rate).assertIsNotEnabled()
    compose.runOnIdle { snapshot.value = snapshot.value.copy(status = PlayerStatus.BUFFERING) }
    compose.onNodeWithText(rate).assertIsEnabled()
    compose.runOnIdle { panel.value = PlayerPanel.Picture }
    compose.onNodeWithContentDescription(brightness).assertIsEnabled()
    compose.runOnIdle { snapshot.value = snapshot.value.copy(status = PlayerStatus.IDLE) }
    compose.onNodeWithContentDescription(brightness).assertIsNotEnabled()
  }

  @Composable private fun PanelHost(content: @Composable () -> Unit) {
    MaterialTheme { Box(Modifier.size(width = 340.dp, height = 440.dp)) { content() } }
  }

  private fun track(id: Int, kind: TrackKind, title: String, selected: Boolean = false) = PlayerTrack(
    mpvId = id, kind = kind, title = title, language = "en", codec = "AAC",
    isDefault = false, isForced = false, isExternal = false, isSelected = selected,
  )

  private fun episode(id: String, title: String, played: Boolean = false) = MediaUi(
    id = id, title = title, itemType = "Episode", metadata = "", artwork = null, overview = "",
    favorite = false, played = played,
  )

  private fun actions(
    selectTrack: (TrackKind, Int) -> Unit = { _, _ -> },
    selectEpisode: (MediaUi) -> Unit = {},
    previousEpisode: () -> Unit = {},
    nextEpisode: () -> Unit = {},
    loadMoreEpisodes: () -> Unit = {},
    setVolume: (Int) -> Unit = {},
    close: () -> Unit = {},
  ) = PlayerPanelActions(selectTrack, selectEpisode, previousEpisode, nextEpisode, loadMoreEpisodes, setVolume, close)
}
