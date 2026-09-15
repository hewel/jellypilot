package io.github.hewel.jellypilot.player

import androidx.media3.common.SimpleBasePlayer
import androidx.media3.common.util.UnstableApi
import org.junit.Assert.assertEquals
import org.junit.Test

@UnstableApi
class PlayerContractTest {

  @Test
  fun `total buffered duration is buffered end minus position`() {
    val position = SimpleBasePlayer.PositionSupplier.getConstant(1_200_000L)
    val buffered = totalBufferedDuration(position, 1_230_000L)
    assertEquals(30_000L, buffered.get())
  }

  @Test
  fun `total buffered duration never goes negative`() {
    val position = SimpleBasePlayer.PositionSupplier.getConstant(5_000L)
    val buffered = totalBufferedDuration(position, 4_000L)
    assertEquals(0L, buffered.get())
  }

  @Test
  fun `unknown buffering reports zero total buffered duration`() {
    val position = SimpleBasePlayer.PositionSupplier.getConstant(5_000L)
    assertEquals(0L, totalBufferedDuration(position, null).get())
  }

  @Test
  fun `total buffered duration shrinks as the position advances`() {
    var now = 1_200_000L
    val position = SimpleBasePlayer.PositionSupplier { now }
    val buffered = totalBufferedDuration(position, 1_230_000L)
    assertEquals(30_000L, buffered.get())
    now = 1_210_000L
    assertEquals(20_000L, buffered.get())
  }
}
