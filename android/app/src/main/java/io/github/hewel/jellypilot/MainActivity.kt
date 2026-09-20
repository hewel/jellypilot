package io.github.hewel.jellypilot

import android.os.Bundle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.appcompat.app.AppCompatActivity
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.core.view.WindowCompat
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.hewel.jellypilot.ui.Destination
import io.github.hewel.jellypilot.ui.ThemePreference
import io.github.hewel.jellypilot.ui.NativePlayerScreen
import io.github.hewel.jellypilot.ui.PilotApp

class MainActivity : AppCompatActivity() {
  private val model: AppViewModel by lazy {
    ViewModelProvider(application as JellyPilotApplication, ViewModelProvider.AndroidViewModelFactory(application))[AppViewModel::class.java]
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    enableEdgeToEdge()
    setContent {
      val state by model.state.collectAsStateWithLifecycle()
      val dark = when (state.preferences.theme) {
        ThemePreference.System -> isSystemInDarkTheme()
        ThemePreference.Dark -> true
        ThemePreference.Light -> false
      }
      val immersive = state.showPlayer || state.detail != null || (state.destination == Destination.Home && state.activeName != null)
      SideEffect {
        WindowCompat.getInsetsController(window, window.decorView).apply {
          isAppearanceLightStatusBars = !dark && !immersive
          isAppearanceLightNavigationBars = !dark && !state.showPlayer
        }
      }
      PilotApp(model) { NativePlayerScreen(model, model.player, model::back) }
    }
  }

  override fun onStart() {
    super.onStart()
    model.setVisible(true)
  }

  override fun onStop() {
    model.setVisible(false)
    super.onStop()
  }

  override fun onDestroy() {
    if (isFinishing) model.activityFinished()
    super.onDestroy()
  }
}
