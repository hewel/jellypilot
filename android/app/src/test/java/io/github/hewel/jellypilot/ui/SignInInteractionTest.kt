package io.github.hewel.jellypilot.ui

import android.app.Application
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.unit.Density
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
  private fun text(id: Int) = context.getString(id)
  private var ui by mutableStateOf(AppUiState(showSignIn = true))
  private var connects = 0
  private var backs = 0
  private var quickConnects = 0
  private val passwords = mutableListOf<String>()

  private fun render(largeText: Boolean = false) {
    compose.setContent {
      MaterialTheme {
        val density = LocalDensity.current
        CompositionLocalProvider(LocalDensity provides Density(density.density, if (largeText) 2f else 1f)) {
          ServerConnectionScreen(ui,
            onBack = { backs++ },
            onServerChange = { ui = ui.copy(loginServer = it) },
            onUsernameChange = { ui = ui.copy(loginUsername = it) },
            onPasswordChange = { ui = ui.copy(loginPassword = it) },
            onRememberChange = { ui = ui.copy(loginRemember = it) },
            onProviderChange = { ui = ui.copy(loginIdentity = ui.loginIdentity?.copy(jellyfin = it, providerSelected = true)) },
            onConnect = { connects++; ui = ui.copy(loginBusy = true) },
            onSignIn = { passwords += ui.loginPassword },
            onQuickConnect = { quickConnects++ },
            onManualContinue = { ui = ui.copy(loginStep = LoginStep.Account, loginError = null,
              loginIdentity = LoginServerUi(null, ui.loginServer, ui.loginJellyfin, providerKnown = false, providerSelected = false)) },
          )
        }
      }
    }
  }

  private fun account(jellyfin: Boolean = true, knownProvider: Boolean = true) = AppUiState(
    showSignIn = true, loginStep = LoginStep.Account, loginServer = "https://media.example.test:8096/media",
    loginIdentity = LoginServerUi("Family server", "https://media.example.test:8096/media", jellyfin, knownProvider),
  )
  private fun submit() = compose.onNode(hasText(text(R.string.sign_in)) and hasClickAction())

  @Test fun connectPreservesFullAddressAndBackRemainsAvailableWhileBusy() {
    render()
    compose.onNode(hasText(text(R.string.connect_server)) and hasClickAction()).assertIsNotEnabled()
    compose.onNodeWithContentDescription(text(R.string.server_url)).performTextInput("https://media.example.test:8096/media")
    compose.onNode(hasText(text(R.string.connect_server)) and hasClickAction()).performClick()
    compose.onNodeWithText(text(R.string.login_connecting)).assertIsNotEnabled()
    compose.onNodeWithContentDescription(text(R.string.server_url)).assertIsNotEnabled()
    compose.onNodeWithContentDescription(text(R.string.back)).performClick()
    compose.runOnIdle {
      assertEquals(1, connects)
      assertEquals(1, backs)
      assertEquals("https://media.example.test:8096/media", ui.loginServer)
    }
  }

  @Test fun bothProvidersSubmitAnEmptyPasswordAndOnlyJellyfinOffersQuickConnect() {
    ui = account(knownProvider = false)
    render()
    submit().assertIsNotEnabled()
    compose.onNodeWithContentDescription(text(R.string.username)).performTextInput("viewer")
    submit().performScrollTo().assertIsEnabled().performClick()
    compose.onNodeWithText(text(R.string.quick_connect)).performScrollTo().performClick()
    compose.onNodeWithText("Emby").performScrollTo().performClick()
    compose.onNodeWithText(text(R.string.quick_connect)).assertDoesNotExist()
    submit().performScrollTo().assertIsEnabled().performClick()
    compose.runOnIdle { assertEquals(listOf("", ""), passwords); assertEquals(1, quickConnects) }
  }

  @Test fun invalidCredentialsKeepPasswordAndVisibilityDoesNotSubmitOrClear() {
    ui = account().copy(loginUsername = "viewer")
    render()
    compose.onNodeWithContentDescription(text(R.string.password)).performTextInput("still-secret")
    compose.onNodeWithText(text(R.string.login_show_password)).performClick()
    compose.onNodeWithContentDescription(text(R.string.password)).assertTextEquals("still-secret")
    compose.runOnIdle { assertEquals(emptyList<String>(), passwords) }
    submit().performScrollTo().performClick()
    compose.runOnIdle { ui = ui.copy(loginError = text(R.string.login_credentials_failed)) }
    compose.onNodeWithText(text(R.string.login_credentials_failed)).performScrollTo().assertIsDisplayed()
    compose.onNodeWithContentDescription(text(R.string.password)).assert(SemanticsMatcher.keyIsDefined(SemanticsProperties.Password))
    compose.onNodeWithText(text(R.string.login_show_password)).performClick()
    compose.onNodeWithContentDescription(text(R.string.password)).assertTextEquals("still-secret")
    compose.runOnIdle { assertEquals(listOf("still-secret"), passwords) }
  }

  @Test fun connectionErrorRetainsAddressAndOffersRetry() {
    ui = AppUiState(showSignIn = true, loginServer = "https://media.example.test/path", loginError = text(R.string.login_connection_failed))
    render()
    compose.onNodeWithText(text(R.string.login_connection_failed)).assertIsDisplayed()
    compose.onNodeWithContentDescription(text(R.string.server_url)).assertTextEquals("https://media.example.test/path")
    compose.onNodeWithText(text(R.string.login_reconnect)).performClick()
    compose.runOnIdle { assertEquals(1, connects) }
  }

  @Test fun restrictedPublicInfoRequiresExplicitManualEntryAndProviderSelection() {
    ui = AppUiState(showSignIn = true, loginServer = "https://media.example.test/emby", loginUsername = "viewer",
      loginPublicInfoRestricted = true, loginError = text(R.string.login_public_info_restricted))
    render()
    compose.onNodeWithText(text(R.string.login_manual_continue)).performScrollTo().performClick()
    compose.onNodeWithText(text(R.string.login_server_unverified)).assertIsDisplayed()
    compose.onNodeWithText(text(R.string.login_connected)).assertDoesNotExist()
    compose.onNodeWithText("Jellyfin").assertIsNotSelected()
    compose.onNodeWithText("Emby").assertIsNotSelected()
    submit().performScrollTo().assertIsNotEnabled()
    compose.onNodeWithText("Emby").performScrollTo().performClick()
    submit().performScrollTo().assertIsEnabled().performClick()
    compose.runOnIdle { assertEquals(listOf(""), passwords) }
  }

  @Test @Config(qualifiers = "w320dp-h844dp")
  fun largeTextAllowsScrollingToSubmitAndServerChange() {
    ui = account().copy(loginUsername = "viewer", loginError = text(R.string.login_connection_lost), loginConnectionLost = true)
    render(largeText = true)
    compose.onNodeWithText(text(R.string.login_connection_lost)).performScrollTo().assertIsDisplayed()
    compose.onNodeWithText(text(R.string.login_credentials_failed)).assertDoesNotExist()
    submit().performScrollTo().assertIsDisplayed().performClick()
    compose.onNodeWithText(text(R.string.login_change_server)).performScrollTo().performClick()
    compose.runOnIdle { assertEquals(listOf(""), passwords); assertEquals(1, backs) }
  }
}
