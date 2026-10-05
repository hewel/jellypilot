package io.github.hewel.jellypilot.ui

import androidx.activity.BackEventCompat
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp

/** A previous page is painted for navigation feedback only; it must not issue new work. */
internal val LocalBackPreview = staticCompositionLocalOf { false }

/** The platform owns gesture admission, progress and cancellation, including three-button Back. */
@Composable
internal fun <T : Any> DetailPredictiveBack(
  current: T,
  pageKey: (T) -> Any,
  route: Any,
  enabled: Boolean,
  reducedMotion: Boolean,
  previous: () -> T?,
  canCommit: () -> Boolean,
  onBack: () -> Unit,
  content: @Composable (T) -> Unit,
) {
  val owner = remember(route) { Any() }
  val currentOwner by rememberUpdatedState(owner)
  val currentEnabled by rememberUpdatedState(enabled)
  var source by remember(owner) { mutableStateOf<T?>(null) }
  var progress by remember(owner) { mutableFloatStateOf(0f) }
  var edge by remember(owner) { mutableIntStateOf(BackEventCompat.EDGE_LEFT) }
  PredictiveBackHandler(enabled) { events ->
    val startedOwner = owner
    val target = previous()
    try {
      events.collect { event ->
        if (currentOwner === startedOwner && currentEnabled && target != null) {
          source = target
          progress = event.progress.coerceIn(0f, 1f)
          edge = event.swipeEdge
        }
      }
      // An account/route change during a gesture must never pop its replacement.
      if (currentOwner === startedOwner && currentEnabled && canCommit()) onBack()
    } finally {
      source = null
      progress = 0f
    }
  }
  Box(Modifier.fillMaxSize()) {
    val previousPage = source?.takeUnless { pageKey(it) == pageKey(current) }
    val pages = listOfNotNull(previousPage, current)
    // Both layers use the same keyed composition path. Separate preview/content slots change
    // rememberSaveable's generated keys, resetting Lists/History/nested-detail scroll on preview.
    pages.forEachIndexed { index, page ->
      key(pageKey(page)) {
        val isPreview = index < pages.lastIndex
        val modifier = if (isPreview) Modifier.clearAndSetSemantics { }
            .focusProperties { onEnter = { cancelFocusChange() } }.focusGroup()
            .pointerInput(Unit) {
              awaitPointerEventScope {
                while (true) awaitPointerEvent(PointerEventPass.Initial).changes.forEach { it.consume() }
              }
            }
          else Modifier.graphicsLayer {
            if (reducedMotion) alpha = 1f - progress
            else {
              scaleX = 1f - 0.06f * progress
              scaleY = scaleX
              translationX = 32.dp.toPx() * progress * if (edge == BackEventCompat.EDGE_LEFT) 1f else -1f
              shape = RoundedCornerShape((28 * progress).dp)
              clip = progress > 0f
            }
          }
        CompositionLocalProvider(LocalBackPreview provides isPreview) {
          Box(Modifier.fillMaxSize().then(modifier).background(MaterialTheme.colorScheme.background)) { content(page) }
        }
      }
    }
  }
}
