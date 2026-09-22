package io.github.hewel.jellypilot

import android.content.res.Configuration
import androidx.appcompat.app.AppCompatDelegate
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.ui.AccountPage
import io.github.hewel.jellypilot.ui.Destination
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class AppStartupTest {
  @get:Rule val compose = createEmptyComposeRule()

  @Test fun lightStartupAndRecreationKeepTheAppShell() = launchApp(AppCompatDelegate.MODE_NIGHT_NO, Configuration.UI_MODE_NIGHT_NO)

  @Test fun darkStartupAndRecreationKeepTheAppShell() = launchApp(AppCompatDelegate.MODE_NIGHT_YES, Configuration.UI_MODE_NIGHT_YES)

  @Test fun browsingFromAccountHistoryRestoresTheBottomNavigation() {
    ActivityScenario.launch(MainActivity::class.java).use { scenario ->
      lateinit var model: AppViewModel
      lateinit var browse: String
      lateinit var library: String
      scenario.onActivity { activity ->
        val app = activity.application as JellyPilotApplication
        model = ViewModelProvider(app, ViewModelProvider.AndroidViewModelFactory(app))[AppViewModel::class.java]
        browse = activity.getString(R.string.browse_library)
        library = activity.getString(R.string.library)
        model.navigate(Destination.Account)
        model.openAccountPage(AccountPage.Settings)
        model.openAccountPage(AccountPage.History)
      }
      try {
        // The real History action leaves an account subpage retained behind Library.
        compose.onNodeWithText(browse).performClick()
        compose.onNodeWithText(library).assertIsDisplayed().assertIsSelected()
        scenario.recreate()
        compose.onNodeWithText(library).assertIsDisplayed().assertIsSelected()
        scenario.onActivity { model.navigate(Destination.Account); model.back() }
        compose.waitForIdle()
        assertEquals(AccountPage.Settings, model.state.value.accountPage)
      } finally {
        scenario.onActivity {
          model.navigate(Destination.Account)
          repeat(4) { if (model.state.value.accountPage != AccountPage.Overview) model.back() }
          model.navigate(Destination.Home)
          model.dismissError()
        }
      }
    }
  }

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
