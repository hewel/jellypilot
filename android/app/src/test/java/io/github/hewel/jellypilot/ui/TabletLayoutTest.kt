package io.github.hewel.jellypilot.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TabletLayoutTest {
  @Test fun shelvesFitAvailableSpaceWithoutShrinkingCardsBelowReadableWidth() {
    for (width in listOf(754f, 804f, 1100f)) {
      val layout = TabletShelfLayout.forWidth(width, 1f)
      assertEquals(3, layout.episodeColumns)
      assertEquals(if (width >= 1000) 7 else 5, layout.posterColumns)
      val available = width - 48
      assertTrue((available - 16 * (layout.posterColumns - 1)) / layout.posterColumns >= 112)
      assertTrue((available - 16 * (layout.episodeColumns - 1)) / layout.episodeColumns >= 200)
    }
    val enlarged = TabletShelfLayout.forWidth(754f, 2f)
    assertTrue(enlarged.posterColumns < 5)
    assertTrue(enlarged.episodeColumns < 3)
  }

  @Test fun detailRequiresActualContentSpaceAndFallsBackBeforeLargeTextCrushesThePanes() {
    assertFalse(tabletDetailHasTwoPanes(754f, 1f))
    assertFalse(tabletDetailHasTwoPanes(1007f, 1f))
    assertTrue(tabletDetailHasTwoPanes(1008f, 1f))
    assertTrue(tabletDetailHasTwoPanes(1100f, 1f))
    assertFalse(tabletDetailHasTwoPanes(1100f, 2f))
  }
}
