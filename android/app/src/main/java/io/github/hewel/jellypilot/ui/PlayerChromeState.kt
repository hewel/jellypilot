package io.github.hewel.jellypilot.ui

import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.saveable.rememberSaveable
import kotlinx.coroutines.delay

/** Presentation only: changing chrome or panel navigation never sends a playback command. */
@Stable
internal class PlayerChromeState(visible: Boolean, paused: Boolean) {
  var visible by mutableStateOf(visible)
    private set
  var panel by mutableStateOf<PlayerPanel?>(null)
    private set
  var panelOrigin by mutableStateOf<PlayerPanel?>(null)
    private set
  var restoreTrigger by mutableStateOf<PlayerPanel?>(null)
    private set
  var restoreRow by mutableStateOf<PlayerPanel?>(null)
    private set
  var interaction by mutableIntStateOf(0)
    private set
  var lastPaused by mutableStateOf(paused)
    private set
  var dragging by mutableStateOf(false)
  var focused by mutableStateOf(false)
  var gestureRecognizing by mutableStateOf(false)
  var gestureFeedback by mutableStateOf(false)
  var gestureSeeking by mutableStateOf(false)

  fun observedPause(paused: Boolean, phone: Boolean) {
    if (phone && paused && !lastPaused && !gestureSeeking) visible = true
    lastPaused = paused
  }
  fun interacted() { interaction++ }
  fun toggle() { visible = !visible; interacted() }
  fun reveal() { visible = true; interacted() }
  fun hide() { visible = false }
  fun open(destination: PlayerPanel) {
    if (panel == null) panelOrigin = destination
    panel = destination
    restoreRow = null
    restoreTrigger = null
    reveal()
  }
  fun back() {
    if (panel == PlayerPanel.GestureHelp) {
      restoreRow = PlayerPanel.GestureHelp
      panel = PlayerPanel.Picture
      interacted()
    } else if (panelOrigin == PlayerPanel.More && panel != PlayerPanel.More) {
      restoreRow = panel
      panel = PlayerPanel.More
      interacted()
    } else close()
  }
  fun close() {
    restoreTrigger = panelOrigin
    panel = null
    panelOrigin = null
    restoreRow = null
    reveal()
  }
  fun restoredFocus() { restoreTrigger = null }

  companion object {
    val Saver = listSaver<PlayerChromeState, Any>(
      save = { listOf(it.visible, it.lastPaused, it.panel?.name.orEmpty(), it.panelOrigin?.name.orEmpty()) },
      restore = {
        PlayerChromeState(it[0] as Boolean, it[1] as Boolean).apply {
          panel = (it[2] as String).takeIf(String::isNotEmpty)?.let(PlayerPanel::valueOf)
          panelOrigin = (it[3] as String).takeIf(String::isNotEmpty)?.let(PlayerPanel::valueOf)
        }
      },
    )
  }
}

@Composable
internal fun rememberPlayerChrome(
  generation: Long,
  paused: Boolean,
  phone: Boolean,
  touchExploration: Boolean,
  playing: Boolean = !paused,
): PlayerChromeState {
  val state = rememberSaveable(generation, saver = PlayerChromeState.Saver) {
    PlayerChromeState(visible = phone && paused, paused = paused)
  }
  LaunchedEffect(state, paused, phone) { state.observedPause(paused, phone) }
  LaunchedEffect(state, touchExploration) { if (touchExploration) state.reveal() }
  LaunchedEffect(state, state.visible, state.panel, state.dragging, state.focused, state.gestureRecognizing,
    state.gestureFeedback, state.interaction, paused, playing, phone, touchExploration) {
    if (state.visible && state.panel == null && !state.dragging && !state.focused &&
      !state.gestureRecognizing && !state.gestureFeedback &&
      !touchExploration && (!phone || (playing && !paused))) {
      delay(3_000)
      state.hide()
    }
  }
  return state
}
