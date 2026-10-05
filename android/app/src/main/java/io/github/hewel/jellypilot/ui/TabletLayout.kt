package io.github.hewel.jellypilot.ui

import kotlin.math.floor

/** Available content width excludes the navigation rail and system insets. */
internal data class TabletShelfLayout(val episodeColumns: Int, val posterColumns: Int) {
  companion object {
    fun forWidth(contentWidth: Float, fontScale: Float): TabletShelfLayout {
      val width = (contentWidth - 48f).coerceAtLeast(0f)
      val textScale = fontScale.coerceAtLeast(1f)
      fun fit(minimum: Float, limit: Int) = floor((width + 16f) / (minimum * textScale + 16f)).toInt().coerceIn(1, limit)
      return TabletShelfLayout(fit(200f, 3), fit(112f, if (contentWidth >= 1000f) 7 else 5))
    }
  }
}

internal fun tabletDetailHasTwoPanes(contentWidth: Float, fontScale: Float): Boolean =
  contentWidth - 48f >= 960f * fontScale.coerceAtLeast(1f)
