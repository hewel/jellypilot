package io.github.hewel.jellypilot.ui

import android.app.Application
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.unit.dp
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class, qualifiers = "w844dp-h390dp")
class PlayerGestureInputTest {
  @get:Rule val compose = createComposeRule()
  private val actions = GestureRecorder()
  private val enabled = mutableStateOf(true)
  private var clicks = 0

  private fun mount() {
    compose.setContent {
      val arbitration = remember { PlayerGestureArbitration() }
      Box(Modifier.size(800.dp, 360.dp).testTag("picture").observePlayerMultitouch(arbitration)) {
        PlayerGestureSurface(1, enabled.value, true, "Picture gestures", GestureBounds(24f, 24f, 776f, 336f), actions, arbitration,
          Modifier.matchParentSize())
        Button(onClick = { clicks++ }, modifier = Modifier.align(Alignment.Center).size(100.dp, 56.dp)) { Text("Control") }
        Box(Modifier.align(Alignment.BottomEnd).size(200.dp, 60.dp).reservePlayerOverlayTouchArea()) { Text("Undo notice") }
      }
    }
  }

  @Test fun controlHitRegionWinsAndCapturedPictureDragCannotActivateControl() {
    mount()
    compose.onNodeWithText("Control").performTouchInput { click() }
    compose.runOnIdle { assertEquals(1, clicks); assertEquals(0, actions.toggles) }
    compose.onNodeWithText("Control").performTouchInput { down(center); moveBy(Offset(180f, 0f)); up() }
    compose.runOnIdle { assertEquals(0, actions.previews); assertEquals(1, clicks) }
    compose.onNodeWithTag("picture").performTouchInput {
      down(Offset(width * 0.2f, height * 0.5f)); moveTo(center); up()
    }
    compose.runOnIdle { assertEquals(1, actions.previews); assertEquals(1, actions.finishes.size); assertEquals(1, clicks) }
  }

  @Test fun secondPointerCancelsPreviewAndNeverTurnsRemainingPointerIntoTap() {
    mount()
    compose.onNodeWithTag("picture").performTouchInput {
      down(0, Offset(width * 0.2f, height * 0.3f))
      moveTo(0, Offset(width * 0.35f, height * 0.3f))
      down(1, Offset(width * 0.7f, height * 0.3f))
      up(1); up(0)
    }
    compose.runOnIdle {
      assertEquals(listOf(1L to null), actions.finishes)
      assertEquals(0, actions.toggles); assertNull(actions.cue)
    }
  }

  @Test fun eligibilityChangeCancelsAnActiveLevelAndRestoresInitialValue() {
    mount()
    compose.onNodeWithTag("picture").performTouchInput {
      down(Offset(width * 0.2f, height * 0.3f)); moveTo(Offset(width * 0.2f, height * 0.7f))
    }
    compose.runOnIdle { assertTrue(actions.brightnessValues.last() < 100); enabled.value = false }
    compose.waitForIdle()
    compose.runOnIdle { assertEquals(100, actions.brightnessValues.last()); assertNull(actions.cue) }
    compose.onNodeWithTag("picture").performTouchInput { up() }
    compose.runOnIdle { assertEquals(0, actions.toggles) }
  }

  @Test fun secondPointerOnControlCancelsPictureRecognitionAndNoticeSurfaceExcludesGestures() {
    mount()
    compose.onNodeWithTag("picture").performTouchInput {
      down(0, Offset(width * 0.2f, height * 0.3f))
      down(1, center)
      moveTo(0, Offset(width * 0.4f, height * 0.3f)); up(0); up(1)
    }
    compose.runOnIdle { assertEquals(0, actions.previews); assertEquals(0, actions.speeds) }
    compose.onNodeWithText("Undo notice").performTouchInput { down(center); moveBy(Offset(0f, -120f)); up() }
    compose.runOnIdle { assertTrue(actions.volumeValues.isEmpty()); assertEquals(0, actions.toggles) }
  }

  @Test fun secondPointerOnControlCancelsAlreadyCapturedSeekBeforeSameFrameRelease() {
    mount()
    compose.onNodeWithTag("picture").performTouchInput {
      down(0, Offset(width * 0.2f, height * 0.3f))
      moveTo(0, Offset(width * 0.35f, height * 0.3f))
      down(1, center); up(0); up(1)
    }
    compose.runOnIdle { assertEquals(listOf(1L to null), actions.finishes); assertEquals(0, actions.toggles) }
  }
}
