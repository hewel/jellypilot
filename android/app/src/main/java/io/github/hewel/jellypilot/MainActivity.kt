package io.github.hewel.jellypilot

import android.content.pm.ActivityInfo
import android.os.Bundle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.appcompat.app.AppCompatActivity
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.platform.LocalConfiguration
import androidx.core.view.WindowCompat
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.hewel.jellypilot.ui.Destination
import io.github.hewel.jellypilot.ui.ThemePreference
import io.github.hewel.jellypilot.ui.NativePlayerScreen
import io.github.hewel.jellypilot.ui.PilotApp

class MainActivity : AppCompatActivity() {
  private var browsingOrientation: Int? = null
  private val playerSystemBars by lazy { PlayerSystemBars(window) }
  private val model: AppViewModel by lazy {
    ViewModelProvider(application as JellyPilotApplication, ViewModelProvider.AndroidViewModelFactory(application))[AppViewModel::class.java]
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    browsingOrientation = savedInstanceState?.takeIf { it.containsKey(BROWSING_ORIENTATION) }?.getInt(BROWSING_ORIENTATION)
    enableEdgeToEdge()
    playerSystemBars.restoreBrowsingPolicy(savedInstanceState?.getBundle(BROWSING_SYSTEM_BARS))
    setContent {
      val state by model.state.collectAsStateWithLifecycle()
      LaunchedEffect(state.showPlayer) {
        updatePlayerOrientation(state.showPlayer)
        playerSystemBars.setPlaying(state.showPlayer)
      }
      val dark = when (state.preferences.theme) {
        ThemePreference.System -> isSystemInDarkTheme()
        ThemePreference.Dark -> true
        ThemePreference.Light -> false
      }
      val compact = LocalConfiguration.current.screenWidthDp < 600
      val immersive = !state.showSignIn && state.remoteController == null && (state.showPlayer ||
        (compact && (state.detail != null || (state.destination == Destination.Home && state.activeName != null))))
      SideEffect {
        WindowCompat.getInsetsController(window, window.decorView).apply {
          isAppearanceLightStatusBars = !dark && !immersive
          isAppearanceLightNavigationBars = !dark && !state.showPlayer
        }
      }
      PilotApp(model) { NativePlayerScreen(model, model.player, model::back) }
    }
  }

  private fun updatePlayerOrientation(showPlayer: Boolean) {
    val orientation = if (showPlayer) {
      if (browsingOrientation == null) browsingOrientation = requestedOrientation
      ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
    } else {
      browsingOrientation?.also { browsingOrientation = null } ?: return
    }
    if (requestedOrientation != orientation) requestedOrientation = orientation
  }

  override fun onSaveInstanceState(outState: Bundle) {
    super.onSaveInstanceState(outState)
    // Language/theme changes can still recreate the Activity while the player is open.
    browsingOrientation?.let { outState.putInt(BROWSING_ORIENTATION, it) }
    playerSystemBars.saveBrowsingPolicy()?.let { outState.putBundle(BROWSING_SYSTEM_BARS, it) }
  }

  override fun onStart() {
    super.onStart()
    model.setVisible(true)
  }

  override fun onWindowFocusChanged(hasFocus: Boolean) {
    super.onWindowFocusChanged(hasFocus)
    if (hasFocus) playerSystemBars.onFocusAcquired()
  }

  override fun onStop() {
    model.setVisible(false)
    super.onStop()
  }

  override fun onDestroy() {
    if (isFinishing) model.activityFinished()
    super.onDestroy()
  }

  private companion object {
    const val BROWSING_ORIENTATION = "browsing_orientation"
    const val BROWSING_SYSTEM_BARS = "browsing_system_bars"
  }
}
