package io.github.hewel.jellypilot.ui

import android.app.Application
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.navigationevent.NavigationEvent
import androidx.navigationevent.NavigationEventDispatcher
import androidx.navigationevent.NavigationEventDispatcherOwner
import androidx.navigationevent.NavigationEventInput
import androidx.navigationevent.compose.LocalNavigationEventDispatcherOwner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class, qualifiers = "w390dp-h844dp")
class DetailPredictiveBackTest {
  @get:Rule val compose = createComposeRule()
  private val input = object : NavigationEventInput() {
    fun start(event: NavigationEvent) = dispatchOnBackStarted(event)
    fun progress(event: NavigationEvent) = dispatchOnBackProgressed(event)
    fun cancel() = dispatchOnBackCancelled()
    fun complete() = dispatchOnBackCompleted()
  }
  private val owner = object : NavigationEventDispatcherOwner, LifecycleOwner {
    override val lifecycle = LifecycleRegistry.createUnsafe(this).apply { currentState = Lifecycle.State.RESUMED }
    override val navigationEventDispatcher = NavigationEventDispatcher().apply { addInput(input) }
  }
  private fun event(progress: Float) = NavigationEvent(swipeEdge = NavigationEvent.EDGE_LEFT, touchX = 12f, touchY = 400f, progress = progress)
  private fun begin() {
    compose.runOnIdle { input.start(event(0f)) }
    compose.runOnIdle { input.progress(event(0.6f)) }
  }

  @Test fun cancelledPreviewPreservesSourceScrollAndCommitPopsOnce() {
    val route = mutableStateOf("library")
    var pops = 0
    var previewIndex = -1
    compose.setContent {
      CompositionLocalProvider(LocalNavigationEventDispatcherOwner provides owner, LocalLifecycleOwner provides owner) {
        MaterialTheme {
          val holder = rememberSaveableStateHolder()
          @Composable fun page(key: String) {
            holder.SaveableStateProvider(key) {
              if (key == "library") {
                val scroll = rememberLazyListState()
                val backPreview = LocalBackPreview.current
                SideEffect { if (backPreview) previewIndex = scroll.firstVisibleItemIndex }
                LazyColumn(state = scroll) {
                  item { Text("Library") }
                  items(100) { Text("source-$it") }
                }
              } else Text("Detail")
            }
          }
          DetailPredictiveBack(route.value, { it }, route.value, route.value == "detail", false, { "library" }, { true },
            { pops++; route.value = "library" }) { page(it) }
        }
      }
    }
    compose.onNode(hasScrollToIndexAction()).performScrollToIndex(50)
    compose.runOnIdle { route.value = "detail" }
    begin()
    compose.onNodeWithText("source-50").assertDoesNotExist()
    compose.runOnIdle { assertEquals(50, previewIndex); input.cancel() }
    compose.onNodeWithText("Detail").assertIsDisplayed()
    compose.runOnIdle { assertEquals(0, pops) }
    begin()
    compose.runOnIdle { input.complete() }
    compose.onNodeWithText("source-49").assertIsDisplayed()
    compose.runOnIdle { assertEquals(1, pops) }
  }

  @Test fun replacementRouteCannotBePoppedByAnOlderGestureAndNestedBackWins() {
    val route = mutableStateOf("profile-a:detail")
    val panel = mutableStateOf(false)
    var pops = 0
    compose.setContent {
      CompositionLocalProvider(LocalNavigationEventDispatcherOwner provides owner, LocalLifecycleOwner provides owner) {
        MaterialTheme {
          DetailPredictiveBack(route.value, { it }, route.value, true, true, { "series" }, { true }, { pops++ }) { page ->
            Text(page)
            BackHandler(panel.value && !LocalBackPreview.current) { panel.value = false }
          }
        }
      }
    }
    begin()
    compose.runOnIdle { route.value = "profile-b:detail" }
    compose.runOnIdle { route.value = "profile-a:detail" }
    compose.runOnIdle { input.complete() }
    compose.runOnIdle { assertEquals(0, pops); panel.value = true }
    compose.runOnIdle { input.complete() }
    compose.runOnIdle { assertEquals(false, panel.value); assertEquals(0, pops) }
    // Three-button/keyboard Back has no progress stream, but commits the same route once.
    compose.runOnIdle { input.complete() }
    compose.runOnIdle { assertEquals(1, pops) }
  }

  @Test fun handoffBeforeRecompositionRejectsTheOldCommit() {
    var player = false
    var signIn = false
    var epoch = 4L
    var pops = 0
    compose.setContent {
      CompositionLocalProvider(LocalNavigationEventDispatcherOwner provides owner, LocalLifecycleOwner provides owner) {
        MaterialTheme {
          DetailPredictiveBack("detail", { it }, "profile-a:detail:4", true, false, { "library" },
            { !player && !signIn && epoch == 4L }, { pops++ }) { Text(it) }
        }
      }
    }
    begin()
    compose.runOnIdle { player = true; input.complete() }
    compose.runOnIdle { assertEquals(0, pops); player = false }
    begin()
    compose.runOnIdle { signIn = true; input.complete() }
    compose.runOnIdle { assertEquals(0, pops); signIn = false }
    begin()
    compose.runOnIdle { epoch++; input.complete() }
    compose.runOnIdle { assertEquals(0, pops) }
  }

  @Test fun previewDoesNotExpireCollectionUndo() {
    compose.mainClock.autoAdvance = false
    val preview = mutableStateOf(true)
    var dismisses = 0
    compose.setContent {
      MaterialTheme {
        CompositionLocalProvider(LocalBackPreview provides preview.value) {
          CollectionUndo(ListUndoUi(7, 1), "Removed", {}, { dismisses++ })
        }
      }
    }
    compose.mainClock.autoAdvance = true
    compose.waitForIdle()
    compose.mainClock.autoAdvance = false
    compose.mainClock.advanceTimeBy(9_000)
    compose.runOnIdle { assertEquals(0, dismisses); preview.value = false }
    compose.mainClock.autoAdvance = true
    compose.waitForIdle()
    compose.mainClock.autoAdvance = false
    compose.mainClock.advanceTimeBy(9_000)
    compose.runOnIdle { assertTrue(dismisses > 0) }
  }
}
