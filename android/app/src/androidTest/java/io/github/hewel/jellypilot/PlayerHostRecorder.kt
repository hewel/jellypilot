package io.github.hewel.jellypilot

import io.github.hewel.jellypilot.player.*
import java.util.Collections
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

internal class PlayerHostRecorder : PlayerHost.Listener {
  private val eventQueue = LinkedBlockingQueue<PlayerEvent>()
  private val changed = LinkedBlockingQueue<Unit>()
  private val history = Collections.synchronizedList(mutableListOf<PlayerEvent>())
  @Volatile private var latest = PlayerSnapshot()

  override fun onSnapshot(snapshot: PlayerSnapshot) {
    latest = snapshot
    changed.offer(Unit)
  }

  override fun onEvent(event: PlayerEvent) {
    history.add(event)
    eventQueue.offer(event)
  }

  fun events(): List<PlayerEvent> = synchronized(history) { history.toList() }

  fun awaitEvent(timeoutMs: Long = 20_000, predicate: (PlayerEvent) -> Boolean): PlayerEvent {
    val deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(timeoutMs)
    while (true) {
      val remaining = deadline - System.nanoTime()
      if (remaining <= 0) throw AssertionError("Player event timeout: ${events()}")
      val event = eventQueue.poll(remaining, TimeUnit.NANOSECONDS)
        ?: throw AssertionError("Player event timeout: ${events()}")
      if (predicate(event)) return event
    }
  }

  fun awaitSnapshot(timeoutMs: Long = 20_000, predicate: (PlayerSnapshot) -> Boolean): PlayerSnapshot {
    val deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(timeoutMs)
    while (true) {
      val current = latest
      if (predicate(current)) return current
      val remaining = deadline - System.nanoTime()
      if (remaining <= 0 || changed.poll(remaining, TimeUnit.NANOSECONDS) == null) {
        throw AssertionError("Player snapshot timeout: $latest; events=${events()}")
      }
    }
  }

  fun terminalEvents(generation: Long): List<PlayerEvent> = events().filter { it.ends(generation) }
}

internal fun PlayerEvent.ends(generation: Long): Boolean = when (this) {
  is PlayerEvent.PlaybackStopped -> this.generation == generation
  is PlayerEvent.LoadRejected -> this.generation == generation
  else -> false
}
