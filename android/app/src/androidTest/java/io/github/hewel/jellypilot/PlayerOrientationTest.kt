package io.github.hewel.jellypilot

import android.content.pm.ActivityInfo
import android.content.res.Configuration
import android.os.Build
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.ComposeTimeoutException
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.hewel.jellypilot.player.PlayerSnapshot
import io.github.hewel.jellypilot.player.PlayerStatus
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.atomic.AtomicReference

@RunWith(AndroidJUnit4::class)
class PlayerOrientationTest {
  @get:Rule val compose = createEmptyComposeRule()

  private fun model(activity: MainActivity): AppViewModel = ViewModelProvider(
    activity.application as JellyPilotApplication,
    ViewModelProvider.AndroidViewModelFactory(activity.application),
  )[AppViewModel::class.java]

  private fun await(player: NativePlayback, predicate: (PlayerSnapshot) -> Boolean): PlayerSnapshot =
    runBlocking { withTimeout(25_000) { player.snapshot.first(predicate) } }

  private fun awaitOrientation(scenario: ActivityScenario<MainActivity>, expected: Int) {
    compose.waitUntil(15_000) {
      var orientation = Configuration.ORIENTATION_UNDEFINED
      scenario.onActivity { orientation = it.resources.configuration.orientation }
      orientation == expected
    }
  }

  private fun awaitSystemBars(scenario: ActivityScenario<MainActivity>, visible: Boolean, view: (MainActivity) -> View? = { it.window.decorView }) {
    var observed = "no window sampled"
    try {
      compose.waitUntil(15_000) {
        var matches = false
        scenario.onActivity { activity ->
          val target = view(activity)
          val insets = target?.let(ViewCompat::getRootWindowInsets)
          val status = insets?.isVisible(WindowInsetsCompat.Type.statusBars())
          val navigation = insets?.isVisible(WindowInsetsCompat.Type.navigationBars())
          observed = "attached=${target?.isAttachedToWindow}, shown=${target?.isShown}, focus=${target?.hasWindowFocus()}, status=$status, navigation=$navigation"
          // Android's first-use ImmersiveModeConfirmation can own focus while both bars remain hidden.
          matches = target?.isAttachedToWindow == true && target.isShown && insets != null &&
            status == visible && navigation == visible
        }
        matches
      }
    } catch (error: ComposeTimeoutException) {
      throw AssertionError("Expected both system bars visible=$visible; $observed", error)
    }
  }

  private fun assertImmersiveDialog(scenario: ActivityScenario<MainActivity>) {
    val showing = mutableStateOf(true)
    val dialogView = AtomicReference<View?>()
    lateinit var overlay: ComposeView
    scenario.onActivity { activity ->
      overlay = ComposeView(activity).apply {
        setContent {
          if (showing.value) Dialog(onDismissRequest = { showing.value = false }, properties = DialogProperties(decorFitsSystemWindows = false)) {
            PlayerDialogSystemBars()
            val view = LocalView.current
            SideEffect { dialogView.set(view) }
            Box(Modifier.size(64.dp))
          }
        }
      }
      activity.addContentView(overlay, ViewGroup.LayoutParams(1, 1))
    }
    try {
      awaitSystemBars(scenario, false) { dialogView.get() }
      scenario.onActivity {
        val attributes = requireNotNull(dialogView.get()).rootView.layoutParams as WindowManager.LayoutParams
        assertEquals("assertions must inspect the real dialog window", WindowManager.LayoutParams.TYPE_APPLICATION, attributes.type)
        assertEquals("Compose content owns the scrim", 0, attributes.flags and WindowManager.LayoutParams.FLAG_DIM_BEHIND)
        showing.value = false
      }
      awaitSystemBars(scenario, false)
    } finally {
      scenario.onActivity {
        showing.value = false
        overlay.disposeComposition()
        (overlay.parent as? ViewGroup)?.removeView(overlay)
      }
    }
  }

  @Test fun enteringPlayerRotatesWithoutInterruptingPlaybackAndExitRestoresOrientation() {
    ActivityScenario.launch(MainActivity::class.java).use { scenario ->
      lateinit var original: MainActivity
      lateinit var viewModel: AppViewModel
      var previousOrientation = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
      var browsingBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_DEFAULT
      var browsingNavigationContrast: Boolean? = null
      scenario.onActivity {
        previousOrientation = it.requestedOrientation
        browsingBarsBehavior = WindowCompat.getInsetsController(it.window, it.window.decorView).systemBarsBehavior
        if (Build.VERSION.SDK_INT >= 29) browsingNavigationContrast = it.window.isNavigationBarContrastEnforced
        it.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_PORTRAIT
      }
      awaitOrientation(scenario, Configuration.ORIENTATION_PORTRAIT)
      awaitSystemBars(scenario, true)
      scenario.onActivity { original = it; viewModel = model(it) }
      val player = viewModel.player
      runBlocking { withTimeout(25_000) { player.ready.first { it } } }
      try {
        scenario.onActivity {
          viewModel.openPlayer()
          player.loadFile(TestMediaProvider.sampleUri, null)
        }
        awaitOrientation(scenario, Configuration.ORIENTATION_LANDSCAPE)
        awaitSystemBars(scenario, false)
        scenario.onActivity {
          assertSame("rotation must not recreate the Activity or revoke playback admission", original, it)
          assertEquals(ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE, it.requestedOrientation)
          assertEquals(WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE,
            WindowCompat.getInsetsController(it.window, it.window.decorView).systemBarsBehavior)
        }
        val playing = await(player) { it.status == PlayerStatus.READY && it.isPlaying && it.positionSeconds > 0.5 }
        await(player) { it.generation == playing.generation && it.isPlaying && it.positionSeconds > playing.positionSeconds + 0.5 }
        assertImmersiveDialog(scenario)

        scenario.moveToState(Lifecycle.State.CREATED)
        await(player) { it.generation == playing.generation && it.paused && !it.playWhenReady }
        scenario.moveToState(Lifecycle.State.RESUMED)
        awaitSystemBars(scenario, false)
        scenario.onActivity {
          assertEquals(ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE, it.requestedOrientation)
          assertTrue("returning from the background must not resume playback", player.snapshot.value.paused)
          assertFalse(player.snapshot.value.playWhenReady)
        }

        // Other configuration changes may still recreate the Activity. Retain the pre-player policy.
        scenario.recreate()
        awaitOrientation(scenario, Configuration.ORIENTATION_LANDSCAPE)
        awaitSystemBars(scenario, false)
        scenario.onActivity {
          assertNotSame(original, it)
          assertSame(viewModel, model(it))
          assertEquals(playing.generation, player.snapshot.value.generation)
          assertTrue(player.snapshot.value.paused)
          viewModel.back()
        }
        awaitOrientation(scenario, Configuration.ORIENTATION_PORTRAIT)
        awaitSystemBars(scenario, true)
        scenario.onActivity {
          assertEquals(ActivityInfo.SCREEN_ORIENTATION_PORTRAIT, it.requestedOrientation)
          assertEquals(browsingBarsBehavior, WindowCompat.getInsetsController(it.window, it.window.decorView).systemBarsBehavior)
          if (Build.VERSION.SDK_INT >= 29) assertEquals(browsingNavigationContrast, it.window.isNavigationBarContrastEnforced)
        }
        await(player) { it.status == PlayerStatus.IDLE }
      } finally {
        scenario.onActivity {
          if (viewModel.state.value.showPlayer) viewModel.back()
          it.requestedOrientation = previousOrientation
        }
        assertTrue(runBlocking { player.stopAndWait() })
      }
    }
  }
}
