package io.github.hewel.jellypilot.ui

import android.app.Application
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.height
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.R
import io.github.hewel.jellypilot.ffi.*
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class, qualifiers = "en-rUS-w390dp-h844dp")
class RemoteControlScreenTest {
  @get:Rule val compose = createComposeRule()
  private val context get() = InstrumentationRegistry.getInstrumentation().targetContext
  private fun text(id: Int) = context.getString(id)
  private val capabilities = RemoteControlCapabilities(
    canPause = true, canResume = true, canStop = true, canSeek = true, canSetVolume = true, canPlayNow = true,
  )
  private val television = RemoteControlTarget(
    key = RemoteControlTargetKey("session-tv", "device-tv"), deviceName = "Living room TV", clientName = "Jellyfin TV",
    userName = "Ada", nowPlaying = RemoteControlNowPlaying("movie-1", "The Arrival", 10.0, 100.0, false),
    volume = 25u, capabilities = capabilities,
  )
  private val desktop = television.copy(key = RemoteControlTargetKey("session-pc", "device-pc"), deviceName = "Office PC")
  private val snapshot = RemoteControlSnapshot(
    revision = 1uL, scope = ProfileScopeRef("profile-a", 2uL), generation = 3uL,
    status = RemoteControllerStatus.READY, targets = listOf(television, desktop), selected = television.key,
    refreshing = false, commandPending = false, error = null,
  )
  private val state = mutableStateOf(RemoteControllerUiState(snapshot = snapshot, serverName = "Home media"))
  private val commands = mutableListOf<Triple<ULong, RemoteControlTargetKey, RemoteControlCommand>>()

  private fun mount(onSelect: (RemoteControlTargetKey) -> Unit = {}, onRefresh: () -> Unit = {},
    onPlay: (ULong, RemoteControlTargetKey) -> Unit = { _, _ -> }) {
    compose.setContent {
      MaterialTheme {
        RemoteControlScreen(state.value, {}, onRefresh, onSelect, { generation, key, command ->
          commands += Triple(generation, key, command)
        }, onPlay)
      }
    }
  }

  private fun slider(id: Int) = compose.onNodeWithContentDescription(text(id))
  private fun device(target: RemoteControlTarget) = compose.onNode(hasText(target.deviceName) and hasClickAction())

  @Test fun selectingUsesTheSessionAndDeviceIdentityAndPlayRequiresSeparateConfirmation() {
    val pending = RemotePlayItem("episode-7", "Episode Seven", PlaybackStartPosition.At(45.0))
    state.value = state.value.copy(snapshot = snapshot.copy(selected = null), pendingPlay = pending)
    val selections = mutableListOf<RemoteControlTargetKey>()
    var playRequests = 0
    mount(onSelect = { key ->
      selections += key
      state.value = state.value.copy(snapshot = snapshot.copy(selected = key, generation = 4uL))
    }, onPlay = { generation, key ->
      assertEquals(4uL, generation)
      assertEquals(desktop.key, key)
      playRequests++
    })
    compose.onNodeWithText(pending.title).performScrollTo().assertIsDisplayed()
    compose.onNodeWithText(context.getString(R.string.remote_start_at, "00:45")).assertExists()
    device(desktop).performScrollTo().performClick()
    device(desktop).assertIsSelected()
    compose.runOnIdle {
      assertEquals(listOf(desktop.key), selections)
      assertEquals(0, playRequests)
      assertTrue(commands.isEmpty())
      assertEquals(pending, state.value.pendingPlay)
    }
    compose.onNodeWithText(context.getString(R.string.remote_play_on, desktop.deviceName)).performScrollTo().performClick()
    compose.runOnIdle { assertEquals(1, playRequests); assertTrue(commands.isEmpty()) }
  }

  @Test fun acceptedCommandKeepsTheReportedPlaybackStateAndCapturedCommandIdentity() {
    mount()
    compose.onNodeWithText(text(R.string.pause)).performScrollTo().performClick()
    compose.runOnIdle {
      assertEquals(listOf(Triple(snapshot.generation, television.key, RemoteControlCommand.Pause)), commands)
      state.value = state.value.copy(accepted = true)
    }
    compose.onNodeWithText(text(R.string.remote_command_accepted)).performScrollTo().assertIsDisplayed()
    compose.onNodeWithText(text(R.string.remote_playing)).assertExists()
    compose.onNodeWithText(text(R.string.pause)).assertIsEnabled()
    compose.onNodeWithText(text(R.string.remote_resume)).assertDoesNotExist()
    compose.runOnIdle {
      state.value = state.value.copy(snapshot = snapshot.copy(targets = listOf(
        television.copy(nowPlaying = television.nowPlaying!!.copy(paused = true)), desktop,
      )))
    }
    compose.onNodeWithText(text(R.string.remote_paused)).performScrollTo().assertIsDisplayed()
    compose.onNodeWithText(text(R.string.remote_resume)).assertIsEnabled()
    compose.onNodeWithText(text(R.string.pause)).assertDoesNotExist()
  }

  @Test fun changingSelectionUpdatesBothThePlayConfirmationAndItsCommandIdentity() {
    state.value = state.value.copy(pendingPlay = RemotePlayItem("movie-2", "Second Movie", PlaybackStartPosition.Beginning))
    val requests = mutableListOf<Pair<ULong, RemoteControlTargetKey>>()
    mount(onPlay = { generation, key -> requests += generation to key })
    compose.onNodeWithText(context.getString(R.string.remote_play_on, television.deviceName)).performScrollTo().assertIsDisplayed()
    compose.runOnIdle { state.value = state.value.copy(snapshot = snapshot.copy(selected = desktop.key, generation = 4uL)) }
    compose.onNodeWithText(context.getString(R.string.remote_play_on, television.deviceName)).assertDoesNotExist()
    compose.onNodeWithText(context.getString(R.string.remote_play_on, desktop.deviceName)).performScrollTo().performClick()
    compose.runOnIdle {
      assertEquals(listOf(4uL to desktop.key), requests)
    }
  }

  @Test fun unknownValuesDoNotInventPlaybackIntentOrZeroAndStopRemainsIndependent() {
    val unknown = television.copy(nowPlaying = television.nowPlaying!!.copy(
      positionSeconds = null, durationSeconds = null, paused = null,
    ), volume = null)
    state.value = state.value.copy(snapshot = snapshot.copy(targets = listOf(unknown)))
    mount()
    compose.onNodeWithText(text(R.string.remote_state_unknown)).performScrollTo().assertIsDisplayed()
    compose.onNodeWithText(text(R.string.pause)).assertDoesNotExist()
    compose.onNodeWithText(text(R.string.remote_resume)).assertDoesNotExist()
    slider(R.string.seek).assertDoesNotExist()
    slider(R.string.volume).assertDoesNotExist()
    compose.onNodeWithText("— / —").assertExists()
    compose.onNodeWithText("—").assertExists()
    compose.onNodeWithText(text(R.string.stop)).performScrollTo().assertIsEnabled().performClick()
    compose.runOnIdle {
      assertEquals(listOf(Triple(snapshot.generation, television.key, RemoteControlCommand.Stop)), commands)
      state.value = state.value.copy(snapshot = snapshot.copy(targets = listOf(unknown.copy(nowPlaying = null))))
    }
    compose.onNodeWithText(text(R.string.stop)).performScrollTo().assertIsEnabled()
  }

  @Test fun capabilitiesAndPendingCommandsControlAdmissionWithoutOptimisticStateChanges() {
    val limited = television.copy(capabilities = capabilities.copy(
      canPause = false, canResume = false, canStop = false, canSeek = false, canSetVolume = false, canPlayNow = false,
    ))
    state.value = state.value.copy(snapshot = snapshot.copy(targets = listOf(limited)),
      pendingPlay = RemotePlayItem("movie-2", "Second Movie", PlaybackStartPosition.Beginning))
    mount()
    compose.onNodeWithText(text(R.string.pause)).assertDoesNotExist()
    compose.onNodeWithText(text(R.string.remote_resume)).assertDoesNotExist()
    compose.onNodeWithText(text(R.string.stop)).assertDoesNotExist()
    slider(R.string.seek).performScrollTo().assertIsNotEnabled()
    slider(R.string.volume).assertDoesNotExist()
    compose.onNodeWithText(context.getString(R.string.remote_play_on, television.deviceName)).assertIsNotEnabled()
    compose.runOnIdle { state.value = state.value.copy(snapshot = snapshot.copy(commandPending = true)) }
    compose.onNodeWithText(text(R.string.pause)).performScrollTo().assertIsNotEnabled()
    compose.onNodeWithText(text(R.string.stop)).performScrollTo().assertIsNotEnabled()
    slider(R.string.volume).performScrollTo().assertIsNotEnabled()
    device(desktop).assertIsNotEnabled()
    compose.onNodeWithText(text(R.string.remote_playing)).assertExists()
    compose.runOnIdle { assertTrue(commands.isEmpty()) }
  }

  @Test fun incomingPositionsCannotReplaceADragPreviewAndReleaseSendsOneSeek() {
    state.value = state.value.copy(snapshot = snapshot.copy(targets = listOf(television)))
    mount()
    val seek = slider(R.string.seek).performScrollTo()
    seek.performTouchInput { down(center); moveTo(centerRight - Offset(30f, 0f)) }
    val preview = seek.fetchSemanticsNode().config[SemanticsProperties.ProgressBarRangeInfo].current
    compose.runOnIdle {
      assertTrue(commands.isEmpty())
      state.value = state.value.copy(snapshot = snapshot.copy(revision = 2uL, refreshing = true, targets = listOf(
        television.copy(nowPlaying = television.nowPlaying!!.copy(positionSeconds = 20.0)),
      )))
    }
    assertEquals(preview, seek.fetchSemanticsNode().config[SemanticsProperties.ProgressBarRangeInfo].current)
    seek.performTouchInput { up() }
    compose.runOnIdle {
      assertEquals(1, commands.size)
      assertEquals(snapshot.generation, commands.single().first)
      assertEquals(television.key, commands.single().second)
      assertEquals(preview.toDouble(), (commands.single().third as RemoteControlCommand.Seek).seconds, 0.01)
    }
  }

  @Test fun releaseCannotRetargetADragAfterSelectionOrGenerationChanges() {
    mount()
    val seek = slider(R.string.seek).performScrollTo()
    seek.performTouchInput { down(center); moveTo(centerRight - Offset(30f, 0f)) }
    compose.runOnIdle {
      state.value = state.value.copy(snapshot = snapshot.copy(selected = desktop.key, generation = 4uL))
    }
    seek.performTouchInput { up() }
    compose.runOnIdle { assertTrue(commands.isEmpty()) }
    seek.performTouchInput { down(center); moveTo(centerRight - Offset(40f, 0f)) }
    compose.runOnIdle {
      state.value = state.value.copy(snapshot = state.value.snapshot!!.copy(generation = 5uL))
    }
    seek.performTouchInput { up() }
    compose.runOnIdle { assertTrue(commands.isEmpty()) }
  }

  @Test fun accessibleVolumeCommitsOnceAndWaitsForTheReportedValue() {
    mount()
    slider(R.string.volume).performScrollTo().performSemanticsAction(SemanticsActions.SetProgress) { it(70f) }
    compose.runOnIdle {
      assertEquals(listOf(Triple(snapshot.generation, television.key, RemoteControlCommand.SetVolume(70u))), commands)
    }
    compose.onNodeWithText(context.getString(R.string.remote_volume_percent, 25)).assertExists()
  }

  @Test fun loadingEmptyAndFailedRefreshRemainDistinctAndFailuresRetireControls() {
    state.value = RemoteControllerUiState(loading = true)
    var refreshes = 0
    mount(onRefresh = { refreshes++ })
    compose.onNodeWithText(text(R.string.remote_loading)).assertIsDisplayed()
    compose.onNodeWithText(text(R.string.remote_empty)).assertDoesNotExist()
    compose.runOnIdle { state.value = RemoteControllerUiState(snapshot = snapshot.copy(targets = emptyList(), selected = null)) }
    compose.onNodeWithText(text(R.string.remote_empty)).assertIsDisplayed()
    compose.runOnIdle {
      state.value = RemoteControllerUiState(snapshot = snapshot.copy(status = RemoteControllerStatus.READY,
        error = "internal error with private server detail"))
    }
    compose.onNodeWithText(text(R.string.remote_load_failed)).assertIsDisplayed()
    compose.onNodeWithText("internal error with private server detail").assertDoesNotExist()
    compose.onNodeWithText(text(R.string.pause)).assertDoesNotExist()
    slider(R.string.seek).assertDoesNotExist()
    device(television).assertIsNotEnabled()
    compose.onNodeWithText(text(R.string.retry)).performClick()
    compose.runOnIdle { assertEquals(1, refreshes); assertTrue(commands.isEmpty()) }
    compose.runOnIdle { state.value = RemoteControllerUiState(snapshot = snapshot.copy(status = RemoteControllerStatus.INACTIVE,
      targets = emptyList(), selected = null)) }
    compose.onNodeWithText(text(R.string.remote_inactive)).assertIsDisplayed()
    compose.onNodeWithText(text(R.string.remote_empty)).assertDoesNotExist()
  }

  @Test fun shortWindowAndLargeTextKeepSelectedControlsReachable() {
    compose.setContent {
      MaterialTheme {
        CompositionLocalProvider(LocalDensity provides Density(LocalDensity.current.density, 2f)) {
          Box(Modifier.height(300.dp)) {
            RemoteControlScreen(state.value, {}, {}, {}, { generation, key, command ->
              commands += Triple(generation, key, command)
            }, { _, _ -> })
          }
        }
      }
    }
    slider(R.string.volume).performScrollTo().assertIsDisplayed()
    compose.onNodeWithText(text(R.string.stop)).performScrollTo().assertIsDisplayed().performClick()
    compose.runOnIdle { assertEquals(RemoteControlCommand.Stop, commands.single().third) }
  }
}
