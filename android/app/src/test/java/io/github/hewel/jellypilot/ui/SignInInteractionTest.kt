package io.github.hewel.jellypilot.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
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
class SignInInteractionTest {
  @get:Rule val compose = createComposeRule()
  private val context get() = InstrumentationRegistry.getInstrumentation().targetContext

  @Test fun bothProvidersSubmitAnEmptyPasswordWithServerAndUsername() {
    val submissions = mutableListOf<Pair<Boolean, String>>()
    compose.setContent {
      MaterialTheme {
        SignInSheet(AppUiState(), {}, { jellyfin, server, username, password, _ ->
          assertEquals("http://media.example.test", server)
          assertEquals("viewer", username)
          submissions += jellyfin to password
        }, { _, _ -> })
      }
    }
    val submit = compose.onNode(hasText(context.getString(R.string.sign_in)) and hasClickAction())
    submit.assertIsNotEnabled()
    compose.onNodeWithText(context.getString(R.string.server_url)).performTextInput("http://media.example.test")
    submit.assertIsNotEnabled()
    compose.onNodeWithText(context.getString(R.string.username)).performTextInput("viewer")
    submit.performScrollTo().assertIsEnabled().performClick()
    compose.onNodeWithText("Emby").performScrollTo().performClick()
    submit.performScrollTo().assertIsEnabled().performClick()
    compose.runOnIdle { assertEquals(listOf(true to "", false to ""), submissions) }
  }
}
