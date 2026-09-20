package io.github.hewel.jellypilot

import android.os.Build
import android.os.Bundle
import android.view.View
import android.view.ViewTreeObserver
import android.view.Window
import android.view.WindowManager
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.window.DialogWindowProvider
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

/** Owns the system-bar policy only while this window presents the player. */
internal class PlayerSystemBars(private val window: Window) {
  private data class Previous(val behavior: Int, val navigationContrast: Boolean?)
  private var previous: Previous? = null

  fun saveBrowsingPolicy(): Bundle? = previous?.let { saved ->
    Bundle().apply {
      putInt("behavior", saved.behavior)
      saved.navigationContrast?.let { putBoolean("navigation_contrast", it) }
    }
  }

  fun restoreBrowsingPolicy(state: Bundle?) {
    previous = state?.let {
      Previous(it.getInt("behavior"), if (it.containsKey("navigation_contrast")) it.getBoolean("navigation_contrast") else null)
    }
  }

  fun setPlaying(playing: Boolean) {
    if (!playing) { restore(); return }
    val controller = WindowCompat.getInsetsController(window, window.decorView)
    if (previous == null) {
      previous = Previous(controller.systemBarsBehavior,
        if (Build.VERSION.SDK_INT >= 29) window.isNavigationBarContrastEnforced else null)
    }
    controller.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
    if (Build.VERSION.SDK_INT >= 29) window.isNavigationBarContrastEnforced = false
    controller.hide(WindowInsetsCompat.Type.systemBars())
  }

  fun onFocusAcquired() { if (previous != null) setPlaying(true) }

  private fun restore() {
    val saved = previous ?: return
    previous = null
    WindowCompat.getInsetsController(window, window.decorView).apply {
      systemBarsBehavior = saved.behavior
      // Browsing always shows both bars, including after an Activity recreation during playback.
      show(WindowInsetsCompat.Type.systemBars())
    }
    if (Build.VERSION.SDK_INT >= 29) saved.navigationContrast?.let { window.isNavigationBarContrastEnforced = it }
  }
}

/** Call inside player Dialog or ModalBottomSheet content: both own a separate Android window. */
@Composable
internal fun PlayerDialogSystemBars() {
  val view = LocalView.current
  DisposableEffect(view) {
    val window = requireNotNull(view.dialogWindow()) { "Player dialog requires its own window" }
    // Dialog and bottom-sheet content already draw their own scrim.
    window.clearFlags(WindowManager.LayoutParams.FLAG_DIM_BEHIND)
    val bars = PlayerSystemBars(window)
    val focus = ViewTreeObserver.OnWindowFocusChangeListener { focused -> if (focused) bars.onFocusAcquired() }
    view.viewTreeObserver.addOnWindowFocusChangeListener(focus)
    bars.setPlaying(true)
    onDispose {
      if (view.viewTreeObserver.isAlive) view.viewTreeObserver.removeOnWindowFocusChangeListener(focus)
      // This window is being dismissed. Showing bars here would reveal them over the underlying player;
      // the Activity restores its own prior policy when playback ends.
    }
  }
}

private fun View.dialogWindow(): Window? {
  if (this is DialogWindowProvider) return window
  var ancestor = parent
  while (ancestor != null) {
    if (ancestor is DialogWindowProvider) return ancestor.window
    ancestor = ancestor.parent
  }
  return null
}
