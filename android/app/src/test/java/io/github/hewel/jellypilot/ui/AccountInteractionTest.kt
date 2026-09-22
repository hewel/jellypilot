package io.github.hewel.jellypilot.ui

import android.app.Application
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.height
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.hasScrollAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.R
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** Selection is independent from connection health, and deleting a list requires explicit consent. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class, qualifiers = "w390dp-h844dp")
class AccountInteractionTest {
  @get:Rule val compose = createComposeRule()
  private val context get() = InstrumentationRegistry.getInstrumentation().targetContext
  private fun text(id: Int) = context.getString(id)
  private val parent = ProfileUi("parent", "Ada", "Living room", "Jellyfin", false, "https://media.example.invalid")
  private val child = ProfileUi("child", "Kids", "Living room", "Jellyfin", false, "https://media.example.invalid")

  private fun actions(
    activate: (String) -> Unit = {},
    signOut: (String, Boolean) -> Unit = { _, _ -> },
  ) = AccountActions({}, activate, {}, signOut, {}, {}, {}, {})

  @Test fun disconnectedSelectionKeepsBothAccountsOnOneServerAndAllowsRetryOrSwitch() {
    val state = mutableStateOf(AppUiState(profiles = listOf(child, parent), selectedProfileKey = parent.key))
    val activations = mutableListOf<String>()
    compose.setContent {
      MaterialTheme { AccountOverview(state.value, actions(activate = { activations += it })) }
    }
    val selected = context.getString(R.string.account_connection_identity, parent.name, text(R.string.connection_disconnected))
    compose.onNodeWithText(selected).performScrollTo().assertIsSelected().performClick()
    compose.onNodeWithText(context.getString(R.string.account_connection_identity, child.name, child.provider))
      .performScrollTo().performClick()
    compose.runOnIdle {
      assertEquals(listOf(parent.key, child.key), activations)
      state.value = state.value.copy(activeName = child.name, activeProfileKey = child.key,
        profiles = listOf(parent, child.copy(active = true)))
    }
    compose.onNodeWithText(context.getString(R.string.account_connection_identity, child.name, text(R.string.account_current_connection)))
      .performScrollTo().assertIsSelected()
    compose.onNodeWithText(context.getString(R.string.account_connection_identity, parent.name, parent.provider)).assertExists()
  }

  @Test fun signOutIsConfirmedAndWatchlistDeletionIsOptIn() {
    val active = parent.copy(active = true)
    val requests = mutableListOf<Pair<String, Boolean>>()
    compose.setContent {
      MaterialTheme {
        AccountOverview(AppUiState(activeName = active.name, activeProfileKey = active.key, profiles = listOf(active)),
          actions(signOut = { key, delete -> requests += key to delete }))
      }
    }
    val signOutAccount = context.getString(R.string.account_signout, active.name)
    compose.onNodeWithText(signOutAccount).performScrollTo().performClick()
    compose.runOnIdle { assertEquals(emptyList<Pair<String, Boolean>>(), requests) }
    compose.onNodeWithText(text(R.string.cancel)).performClick()
    compose.onNodeWithText(signOutAccount).performClick()
    compose.onNodeWithText(text(R.string.sign_out)).performClick()
    compose.runOnIdle { assertEquals(listOf(active.key to false), requests) }
    compose.onNodeWithText(signOutAccount).performClick()
    compose.onNodeWithText(text(R.string.forget_watchlist)).performClick()
    compose.onNodeWithText(text(R.string.sign_out)).performClick()
    compose.runOnIdle { assertEquals(listOf(active.key to false, active.key to true), requests) }
  }

  @Test fun shortSettingsWindowWithLargeTextKeepsPlaybackAndSupportDestinationsReachable() {
    val pages = mutableListOf<AccountPage>()
    compose.setContent {
      MaterialTheme {
        CompositionLocalProvider(LocalDensity provides Density(LocalDensity.current.density, 2f)) {
          Box(Modifier.height(300.dp)) {
            AccountSettings(AppUiState(profiles = listOf(parent), selectedProfileKey = parent.key), pages::add)
          }
        }
      }
    }
    listOf(R.string.playback_preferences, R.string.subtitle_tracks, R.string.diagnostics).forEach { title ->
      // Later groups are not composed until the lazy list reaches them.
      compose.onNode(hasScrollAction()).performScrollToNode(hasText(text(title)))
      compose.onNodeWithText(text(title)).performScrollTo().assertIsDisplayed().performClick()
    }
    compose.runOnIdle { assertEquals(listOf(AccountPage.Playback, AccountPage.Subtitles, AccountPage.Diagnostics), pages) }
  }
}
