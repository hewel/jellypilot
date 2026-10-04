package io.github.hewel.jellypilot.ui

/** Coordinates are picture-local dp, with the same safe edges as gesture recognition. */
internal fun seekFeedbackPosition(
  anchor: GesturePoint, bounds: GestureBounds, width: Float, height: Float,
): GesturePoint {
  // libmpv composites subtitles into the picture; Compose cannot sit behind them.
  val bottom = (bounds.bottom - 64).coerceAtLeast(bounds.top + height)
  val above = anchor.y - 36 - height
  val below = anchor.y + 36
  val fitsAbove = above >= bounds.top && above + height <= bottom
  val fitsBelow = below >= bounds.top && below + height <= bottom
  val y = when {
    fitsAbove -> above
    fitsBelow -> below
    else -> bounds.top
  }
  val centered = !fitsAbove && !fitsBelow
  val x = if (centered) (bounds.left + bounds.right - width) / 2 else anchor.x - width / 2
  return GesturePoint(x.coerceIn(bounds.left, (bounds.right - width).coerceAtLeast(bounds.left)), y)
}
