package io.github.hewel.jellypilot

import java.io.File
import java.net.ServerSocket
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Minimal loopback HTTP server for deterministic "still opening" states.
 *
 * Holds the first response until [release], then serves queued requests
 * serially. Coordination is latch-based; a bounded wait prevents a failed
 * opening assertion from stranding the server thread.
 */
class HeldMediaServer(private val media: File) : AutoCloseable {

  private val socket = ServerSocket(0)
  private val running = AtomicBoolean(true)
  private val firstRequestReceived = CountDownLatch(1)
  private val releaseFirst = CountDownLatch(1)
  private val served = CountDownLatch(1)
  private val held = AtomicBoolean(true)

  val url: String = "http://127.0.0.1:${socket.localPort}/sample.mkv"

  private val thread = Thread({
    while (running.get()) {
      val connection = try {
        socket.accept()
      } catch (_: Exception) {
        break
      }
      try {
        connection.soTimeout = 30_000
        readRequest(connection)
        if (held.compareAndSet(true, false)) {
          firstRequestReceived.countDown()
          releaseFirst.await(30, TimeUnit.SECONDS)
        }
        writeResponse(connection)
      } catch (_: Exception) {
        // A cancelled/closed connection is expected during teardown.
      } finally {
        runCatching { connection.close() }
      }
    }
    served.countDown()
  }, "held-media-server").apply {
    isDaemon = true
    start()
  }

  /** Awaits mpv's first request: the load is provably in flight afterwards. */
  fun awaitRequest(timeoutMs: Long = 15_000): Boolean =
    firstRequestReceived.await(timeoutMs, TimeUnit.MILLISECONDS)

  /** Lets the held response complete. */
  fun release() = releaseFirst.countDown()

  override fun close() {
    running.set(false)
    release()
    runCatching { socket.close() }
    served.await(10, TimeUnit.SECONDS)
  }

  private fun readRequest(connection: java.net.Socket) {
    val input = connection.getInputStream()
    val buffer = ByteArray(16 * 1024)
    var size = 0
    while (size < buffer.size) {
      val read = input.read(buffer, size, buffer.size - size)
      if (read < 0) break
      size += read
      val text = String(buffer, 0, size, Charsets.ISO_8859_1)
      if (text.contains("\r\n\r\n")) break
    }
  }

  private fun writeResponse(connection: java.net.Socket) {
    val bytes = media.readBytes()
    val output = connection.getOutputStream()
    output.write(
      (
        "HTTP/1.1 200 OK\r\n" +
          "Content-Type: video/x-matroska\r\n" +
          "Content-Length: ${bytes.size}\r\n" +
          "Connection: close\r\n" +
          "\r\n"
        ).toByteArray(Charsets.ISO_8859_1),
    )
    output.write(bytes)
    output.flush()
  }
}
