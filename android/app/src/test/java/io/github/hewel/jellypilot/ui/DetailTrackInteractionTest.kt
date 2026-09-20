package io.github.hewel.jellypilot.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasScrollAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.R
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class, qualifiers = "w390dp-h844dp")
class DetailTrackInteractionTest {
  @get:Rule val compose = createComposeRule()
  private val context get() = InstrumentationRegistry.getInstrumentation().targetContext

  @Test fun selectionUsesProviderIndexAndExplicitOffRemainsDistinctFromPreferences() {
    val state = mutableStateOf(DetailTracksUi("episode", subtitles = listOf(
      DetailTrackUi(41, "English · SRT"), DetailTrackUi(208, "简体中文 · ASS"),
    )))
    val choices = mutableListOf<Int?>()
    compose.setContent {
      MaterialTheme {
        DetailTrackSheet(false, state.value, {}, {}) { selection ->
          choices += selection
          state.value = state.value.copy(selectedSubtitle = selection)
        }
      }
    }
    compose.onNodeWithText("简体中文 · ASS").performClick().assertIsSelected()
    compose.onNodeWithText(context.getString(R.string.subtitles_off)).performClick().assertIsSelected()
    compose.onNodeWithText(context.getString(R.string.playback_preferences_default)).performClick().assertIsSelected()
    compose.runOnIdle { assertEquals(listOf(208, -1, null), choices) }
  }

  @Test fun longTrackListsScrollInsideSheetWithoutLosingCloseAction() {
    val selected = mutableStateOf<Int?>(null)
    val options = (1..40).map { DetailTrackUi(it * 7, "Audio track $it") }
    compose.setContent {
      CompositionLocalProvider(LocalDensity provides Density(LocalDensity.current.density, fontScale = 2f)) {
      MaterialTheme {
        DetailTrackSheet(true, DetailTracksUi("episode", audio = options, selectedAudio = selected.value), {}, {}) { selected.value = it }
      }
      }
    }
    compose.onNode(hasScrollAction()).performScrollToNode(hasText("Audio track 40"))
    compose.onNodeWithText("Audio track 40").performClick().assertIsSelected()
    compose.onNodeWithContentDescription(context.getString(R.string.close)).assertIsDisplayed()
    compose.runOnIdle { assertEquals(280, selected.value) }
  }
}
