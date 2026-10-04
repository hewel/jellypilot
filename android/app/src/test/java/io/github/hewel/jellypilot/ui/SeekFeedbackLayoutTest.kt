package io.github.hewel.jellypilot.ui

import org.junit.Assert.*
import org.junit.Test

class SeekFeedbackLayoutTest {
  private val bounds = GestureBounds(24f, 24f, 820f, 424f)

  @Test fun horizontalMotionKeepsLockedHeightAndClampsAtSafeEdges() {
    val first = seekFeedbackPosition(GesturePoint(400f, 200f), bounds, 112f, 56f)
    val moved = seekFeedbackPosition(GesturePoint(900f, 200f), bounds, 112f, 56f)
    assertEquals(first.y, moved.y)
    assertEquals(200f - 36f, moved.y + 56f)
    assertEquals(bounds.right, moved.x + 112f)
    assertEquals(bounds.left, seekFeedbackPosition(GesturePoint(-30f, 200f), bounds, 112f, 56f).x)
  }

  @Test fun topGestureUsesBelowWhileNeitherSideUsesSafeTopCenter() {
    val below = seekFeedbackPosition(GesturePoint(250f, 60f), bounds, 112f, 56f)
    assertEquals(96f, below.y)
    assertEquals(250f, below.x + 56f)
    val compact = GestureBounds(24f, 24f, 820f, 220f)
    val fallback = seekFeedbackPosition(GesturePoint(250f, 100f), compact, 112f, 56f)
    assertEquals(compact.top, fallback.y)
    assertEquals((compact.left + compact.right) / 2, fallback.x + 56f)
  }

  @Test fun largeFeedbackReservesSubtitleLaneAndDoesNotFlipWithHorizontalMotion() {
    val size = 280f to 120f
    val first = seekFeedbackPosition(GesturePoint(300f, 140f), bounds, size.first, size.second)
    val moved = seekFeedbackPosition(GesturePoint(790f, 140f), bounds, size.first, size.second)
    assertEquals(first.y, moved.y)
    assertTrue(first.y >= bounds.top)
    assertTrue(first.y + size.second <= bounds.bottom - 64)
    assertTrue(moved.x >= bounds.left && moved.x + size.first <= bounds.right)
  }
}
