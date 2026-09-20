package io.github.hewel.jellypilot.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.R
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** Exercises UI semantics and state transitions; never captures or judges pixels. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class, qualifiers = "w390dp-h844dp")
class PersonalListsInteractionTest {
  @get:Rule val compose = createComposeRule()
  private val context get() = InstrumentationRegistry.getInstrumentation().targetContext
  private fun text(id: Int) = context.getString(id)
  private fun media(id: String) = MediaUi(id, id, "Movie", "2026", null, "", false, false)

  @Test fun failedRemovalKeepsSelectionAndAllowsRetry() {
    val alpha = media("Alpha")
    val state = mutableStateOf(AppUiState(listItems = listOf(alpha), listCount = 1))
    var submitted = emptyList<String>()
    compose.setContent {
      MaterialTheme {
        PersonalLists(state.value, PersonalListActions({}, {}, {}, { ids ->
          submitted = ids
          state.value = state.value.copy(listBusy = true)
        }, {}, {}, {}))
      }
    }
    compose.onNodeWithText(text(R.string.manage)).performClick()
    compose.onNodeWithText("Alpha").performClick().assertIsSelected()
    compose.onNodeWithText(text(R.string.remove_selected)).performClick()
    compose.runOnIdle {
      assertEquals(listOf("Alpha"), submitted)
      state.value = state.value.copy(listBusy = false, error = "Connection failed")
    }
    compose.onNodeWithText("Alpha").assertIsSelected()
    compose.onNodeWithText(text(R.string.remove_selected)).assertIsEnabled()
    compose.onNodeWithText(text(R.string.done)).assertIsEnabled()
  }

  @Test fun successfulLastRemovalLeavesUndoAvailableAndRestoresBrowsing() {
    val alpha = media("Alpha")
    val state = mutableStateOf(AppUiState(listItems = listOf(alpha), listCount = 1))
    compose.setContent {
      MaterialTheme {
        PersonalLists(state.value, PersonalListActions({}, {}, {}, {
          state.value = state.value.copy(listItems = emptyList(), listCount = 0, listUndo = ListUndoUi(1, 1))
        }, {
          state.value = state.value.copy(listItems = listOf(alpha), listCount = 1, listUndo = null)
        }, { state.value = state.value.copy(listUndo = null) }, {}))
      }
    }
    compose.onNodeWithText(text(R.string.manage)).performClick()
    compose.onNodeWithText("Alpha").performClick()
    compose.onNodeWithText(text(R.string.remove_selected)).performClick()
    compose.onNodeWithText(text(R.string.empty_watchlist)).assertExists()
    compose.onNodeWithText(text(R.string.done)).assertDoesNotExist()
    compose.onNodeWithText(text(R.string.undo)).performClick()
    compose.onNodeWithText("Alpha").assertExists()
    compose.onNodeWithText(text(R.string.manage)).assertIsEnabled()
    compose.onNodeWithText(text(R.string.undo)).assertDoesNotExist()
  }

  @Test fun managementTapsNeverOpenDetailsAndCollectionChangeDiscardsSelection() {
    val alpha = media("Alpha")
    val beta = media("Beta")
    val state = mutableStateOf(AppUiState(listItems = listOf(alpha), listCount = 1, favoriteCount = 1))
    var opened: String? = null
    compose.setContent {
      MaterialTheme {
        PersonalLists(state.value, PersonalListActions({ kind ->
          state.value = state.value.copy(selectedList = kind, listItems = if (kind == PersonalListKind.Favorites) listOf(beta) else listOf(alpha))
        }, { opened = it }, {}, {}, {}, {}, {}))
      }
    }
    compose.onNodeWithText(text(R.string.manage)).performClick()
    compose.onNodeWithText("Alpha").performClick()
    compose.runOnIdle { assertEquals(null, opened) }
    compose.onNodeWithText("${text(R.string.favorites)}  1").performClick()
    compose.onNodeWithText(text(R.string.done)).assertDoesNotExist()
    compose.onNodeWithText("Beta").performClick()
    compose.runOnIdle { assertEquals("Beta", opened) }
  }
}
