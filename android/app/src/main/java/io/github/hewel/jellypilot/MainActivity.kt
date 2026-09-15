package io.github.hewel.jellypilot

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.lifecycle.ViewModelProvider
import io.github.hewel.jellypilot.ui.NativePlayerScreen
import io.github.hewel.jellypilot.ui.PilotApp

class MainActivity : ComponentActivity() {
  private val model: AppViewModel by lazy {
    ViewModelProvider(application as JellyPilotApplication, ViewModelProvider.AndroidViewModelFactory(application))[AppViewModel::class.java]
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    enableEdgeToEdge()
    setContent { PilotApp(model) { NativePlayerScreen(model.player, model::back) } }
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
