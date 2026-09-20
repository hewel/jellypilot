package io.github.hewel.jellypilot

import android.content.res.Configuration
import androidx.appcompat.app.AppCompatDelegate
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class AppStartupTest {
  @get:Rule val compose = createEmptyComposeRule()

  @Test fun lightStartupAndRecreationKeepTheAppShell() = launchApp(AppCompatDelegate.MODE_NIGHT_NO, Configuration.UI_MODE_NIGHT_NO)

  @Test fun darkStartupAndRecreationKeepTheAppShell() = launchApp(AppCompatDelegate.MODE_NIGHT_YES, Configuration.UI_MODE_NIGHT_YES)

  private fun launchApp(mode: Int, expectedNightMode: Int) {
    val instrumentation = InstrumentationRegistry.getInstrumentation()
    val previous = AppCompatDelegate.getDefaultNightMode()
    instrumentation.runOnMainSync { AppCompatDelegate.setDefaultNightMode(mode) }
    try {
      // Keep MainActivity's real composition: replacing its content bypasses theme/font startup.
      ActivityScenario.launch(MainActivity::class.java).use { scenario ->
        fun assertAppShell() {
          lateinit var home: String
          scenario.onActivity { activity ->
            assertEquals(expectedNightMode, activity.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK)
            home = activity.getString(R.string.home)
          }
          compose.onNodeWithText(home).assertIsDisplayed()
        }
        assertAppShell()
        scenario.recreate()
        assertAppShell()
      }
    } finally {
      instrumentation.runOnMainSync { AppCompatDelegate.setDefaultNightMode(previous) }
    }
  }
}
