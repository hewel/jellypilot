package io.github.hewel.jellypilot.ui

import android.app.Application
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.width
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
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class, qualifiers = "w390dp-h844dp")
class LibraryControlsInteractionTest {
  @get:Rule val compose = createComposeRule()
  private val context get() = InstrumentationRegistry.getInstrumentation().targetContext
  private fun text(id: Int) = context.getString(id)

  @Test fun viewingSegmentsAndFavoriteSwitchRemainIndependent() {
    val played = mutableStateOf(PlayedFilter.Unplayed)
    val favorites = mutableStateOf(true)
    compose.setContent {
      MaterialTheme {
        LibraryFilters(played.value, favorites.value) { value, onlyFavorites -> played.value = value; favorites.value = onlyFavorites }
      }
    }
    compose.onNodeWithText(text(R.string.unwatched)).assertIsSelected()
    compose.onNodeWithText(text(R.string.only_favorites)).assertIsOn()
    compose.onNodeWithText(text(R.string.all_media)).performClick().assertIsSelected()
    compose.onNodeWithText(text(R.string.only_favorites)).assertIsOn().performClick().assertIsOff()
    compose.onNodeWithText(text(R.string.watched)).performClick().assertIsSelected()
    compose.runOnIdle { assertEquals(PlayedFilter.Played, played.value); assertEquals(false, favorites.value) }
  }

  @Test fun librarySwitcherAndSortPreserveCurrentFilterSelection() {
    val state = mutableStateOf(AppUiState(
      libraries = listOf(LibraryUi("shows", "Shows"), LibraryUi("movies", "Movies")), libraryId = "shows",
      libraryPlayed = PlayedFilter.Unplayed, libraryFavorites = true,
    ))
    var searchCalls = 0
    compose.setContent {
      MaterialTheme {
        LibraryControls(state.value,
          { state.value = state.value.copy(libraryId = it) },
          { state.value = state.value.copy(librarySort = it) },
          { played, favorites -> state.value = state.value.copy(libraryPlayed = played, libraryFavorites = favorites) },
          { searchCalls++ })
      }
    }
    compose.onNodeWithText("Shows").performClick()
    compose.onNodeWithText("Movies").performClick()
    compose.onNodeWithText(text(R.string.latest_media)).performClick()
    compose.onNodeWithText(text(R.string.sort_year)).performClick()
    compose.onNodeWithText(text(R.string.unwatched)).assertIsSelected()
    compose.onNodeWithText(text(R.string.only_favorites)).assertIsOn()
    compose.onNodeWithContentDescription(text(R.string.search)).performClick()
    compose.runOnIdle {
      assertEquals("movies", state.value.libraryId)
      assertEquals(LibrarySort.Year, state.value.librarySort)
      assertEquals(1, searchCalls)
    }
  }

  @Test fun enlargedNarrowControlsKeepSeparateReachableTargets() {
    val played = mutableStateOf(PlayedFilter.All)
    val favorites = mutableStateOf(false)
    compose.setContent {
      CompositionLocalProvider(LocalDensity provides Density(LocalDensity.current.density, 2f)) {
        MaterialTheme {
          Box(Modifier.width(288.dp)) {
            LibraryFilters(played.value, favorites.value) { value, onlyFavorites -> played.value = value; favorites.value = onlyFavorites }
          }
        }
      }
    }
    PlayedFilter.entries.forEach { filter ->
      compose.onNodeWithText(text(filter.title)).assertIsDisplayed().assertHeightIsAtLeast(48.dp).assertWidthIsAtLeast(48.dp)
    }
    val segment = compose.onNodeWithText(text(R.string.unwatched)).fetchSemanticsNode().boundsInRoot
    val favorite = compose.onNodeWithText(text(R.string.only_favorites)).assertIsDisplayed().assertHeightIsAtLeast(48.dp).fetchSemanticsNode().boundsInRoot
    assertTrue("The switch has its own row below enlarged viewing controls", favorite.top >= segment.bottom)
    compose.onNodeWithText(text(R.string.unwatched)).performClick()
    compose.onNodeWithText(text(R.string.only_favorites)).performClick()
    compose.runOnIdle { assertEquals(PlayedFilter.Unplayed, played.value); assertEquals(true, favorites.value) }
  }
}
