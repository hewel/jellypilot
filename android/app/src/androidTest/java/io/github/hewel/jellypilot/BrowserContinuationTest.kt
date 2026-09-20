package io.github.hewel.jellypilot

import android.os.Looper
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.ffi.BrowsePreferences
import io.github.hewel.jellypilot.ffi.BrowseQuery
import io.github.hewel.jellypilot.ffi.BrowseSession
import io.github.hewel.jellypilot.ffi.BrowseSnapshot
import io.github.hewel.jellypilot.ffi.BrowseStatus
import io.github.hewel.jellypilot.ffi.JellypilotSdk
import io.github.hewel.jellypilot.ffi.Provider
import io.github.hewel.jellypilot.ffi.SdkConfig
import io.github.hewel.jellypilot.ffi.SecureCredentialStore
import io.github.hewel.jellypilot.ffi.VideoLibraryPlayedFilter
import io.github.hewel.jellypilot.ffi.VideoLibraryShortcut
import io.github.hewel.jellypilot.ffi.VideoLibrarySort
import io.github.hewel.jellypilot.ffi.VideoLibrarySortDirection
import java.io.BufferedInputStream
import java.io.File
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.URI
import java.util.UUID
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Exercises the grid's synchronous setter with a real Main.immediate UniFFI continuation. */
@RunWith(AndroidJUnit4::class)
class BrowserContinuationTest {
  @Test fun displayRangeReturnsWhileMainImmediateSnapshotWaiterResumes() {
    runBlocking {
      val context = InstrumentationRegistry.getInstrumentation().targetContext
      val directory = File(context.cacheDir, "browser-continuation-${UUID.randomUUID()}").apply { mkdirs() }
      val credentials = object : SecureCredentialStore {
        private var blob: ByteArray? = null
        @Synchronized override fun read(): ByteArray? = blob?.copyOf()
        @Synchronized override fun write(secret: ByteArray) { blob = secret.copyOf() }
        @Synchronized override fun delete() { blob = null }
      }
      val sdk = JellypilotSdk(SdkConfig(directory.path, "Browser continuation fixture"), credentials, null)
      val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
      val mainFinished = CountDownLatch(1)
      val phase = AtomicReference("preparing isolated library")
      val failure = AtomicReference<Throwable?>(null)
      var browser: BrowseSession? = null
      var mainStarted = false
      try {
        BrowserContinuationServer().use { server ->
          val candidate = withTimeout(15_000) {
            sdk.passwordLogin(Provider.JELLYFIN, server.baseUrl, "fixture-user", "fixture-password")
          }
          try { withTimeout(15_000) { sdk.activateCandidate(candidate, false) } }
          finally { candidate.destroy() }
          val session = sdk.openBrowser(BrowseQuery.Library(
            VideoLibraryShortcut("00000000000000000000000000000020", "Fixture library", "movies", 24, null),
            BrowsePreferences(VideoLibrarySort.TITLE, VideoLibrarySortDirection.ASCENDING, VideoLibraryPlayedFilter.ALL, false),
          ))
          browser = session
          val initial = awaitReady(session)
          assertTrue("fixture must exercise virtual library windows", initial.isVirtual)
          assertEquals(24u, initial.totalCount)

          mainStarted = true
          scope.launch {
            try {
              coroutineScope {
                for (start in listOf(0u, 6u, 12u, 0u)) {
                  assertEquals(Looper.getMainLooper(), Looper.myLooper())
                  val before = session.snapshot()
                  // UNDISTPATCHED registers the real Rust waiter before the synchronous setter.
                  val pending = async(start = CoroutineStart.UNDISPATCHED) {
                    session.nextSnapshot(before.revision)
                  }
                  assertFalse("nextSnapshot must be suspended before changing the window", pending.isCompleted)
                  phase.set("Main.immediate setDisplayRange($start, ${start + 6u}) has not returned")
                  session.setDisplayRange(start, start + 6u)
                  phase.set("waiting for the updated snapshot at $start")
                  val updated = pending.await()
                  assertTrue("the setter must publish a new snapshot", updated.revision > before.revision)
                  assertEquals(start, updated.visibleStart)
                  assertEquals((start.toInt() until start.toInt() + 6).map { "Movie $it" }, updated.items.map { it?.name })
                }
              }
            } catch (error: Throwable) {
              failure.set(error)
            } finally {
              mainFinished.countDown()
            }
          }
          // A native mutex can block Main itself, so a Main-dispatched coroutine timeout is insufficient.
          val completed = mainFinished.await(15, TimeUnit.SECONDS)
          assertTrue(
            "Browser continuation timed out: ${phase.get()}. A native Main-thread deadlock requires terminating the instrumentation process.",
            completed,
          )
          failure.get()?.let { throw it }
          assertTrue("fixture received unexpected endpoints: ${server.unexpectedPaths}", server.unexpectedPaths.isEmpty())
          assertTrue("fixture request failures: ${server.failures}", server.failures.isEmpty())
        }
      } finally {
        // Re-entering this session after a native deadlock would also block the instrumentation thread.
        if (!mainStarted || mainFinished.count == 0L) {
          scope.cancel()
          browser?.shutdown()
          browser?.destroy()
          try { withTimeout(15_000) { sdk.disconnect() } }
          finally { sdk.shutdown(); sdk.destroy(); directory.deleteRecursively() }
        }
      }
    }
  }

  private suspend fun awaitReady(session: BrowseSession): BrowseSnapshot = withTimeout(15_000) {
    var snapshot = session.snapshot()
    while (snapshot.status != BrowseStatus.READY || snapshot.loadingMore || snapshot.items.any { it == null }) {
      check(snapshot.status != BrowseStatus.FAILED) { "Synthetic library failed: ${snapshot.error}" }
      snapshot = session.nextSnapshot(snapshot.revision)
    }
    snapshot
  }
}

/** One-page synthetic library; no app account, persisted credential, or external server is used. */
private class BrowserContinuationServer : AutoCloseable {
  private val listener = ServerSocket(0, 8, InetAddress.getByName("127.0.0.1"))
  private val workers = Executors.newCachedThreadPool()
  private val sockets = ConcurrentLinkedQueue<Socket>()
  val unexpectedPaths = ConcurrentLinkedQueue<String>()
  val failures = ConcurrentLinkedQueue<String>()
  val baseUrl = "http://127.0.0.1:${listener.localPort}"

  init {
    workers.execute {
      while (!listener.isClosed) {
        val socket = try { listener.accept() } catch (_: java.io.IOException) { break }
        sockets += socket
        workers.execute {
          try { socket.use { serve(it) } }
          catch (error: Exception) { if (!listener.isClosed) failures += error.javaClass.simpleName }
          finally { sockets.remove(socket) }
        }
      }
    }
  }

  private fun serve(socket: Socket) {
    socket.soTimeout = 15_000
    val input = BufferedInputStream(socket.getInputStream())
    val request = readLine(input).split(' ')
    check(request.size >= 2)
    val path = URI(request[1]).path
    var remaining = 0
    while (true) {
      val line = readLine(input)
      if (line.isEmpty()) break
      if (line.startsWith("Content-Length:", ignoreCase = true)) remaining = line.substringAfter(':').trim().toInt()
    }
    while (remaining > 0) { check(input.read() >= 0); remaining-- }
    val body = when (path) {
      "/Users/AuthenticateByName" -> JSONObject().put("User", JSONObject().put("Id", USER).put("Name", "Fixture user"))
        .put("AccessToken", "synthetic-token").put("ServerId", "fixture-server").toString()
      "/System/Info/Public" -> "{\"ServerName\":\"Fixture\",\"Version\":\"10.10.0\",\"Id\":\"fixture-server\"}"
      "/Items" -> libraryPage()
      else -> { unexpectedPaths += path; "{}" }
    }.toByteArray(Charsets.UTF_8)
    socket.getOutputStream().apply {
      write("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: ${body.size}\r\nConnection: close\r\n\r\n".toByteArray(Charsets.US_ASCII))
      write(body)
      flush()
    }
  }

  private fun libraryPage(): String {
    val items = JSONArray()
    repeat(24) { index ->
      val id = index.toString(16).padStart(32, '0')
      items.put(JSONObject().put("Id", id).put("Name", "Movie $index").put("Type", "Movie")
        .put("UserData", JSONObject().put("Key", id).put("Played", false).put("IsFavorite", false)))
    }
    return JSONObject().put("Items", items).put("TotalRecordCount", 24).put("StartIndex", 0).toString()
  }

  private fun readLine(input: BufferedInputStream): String {
    val line = StringBuilder()
    while (true) {
      val byte = input.read()
      if (byte < 0 || byte == 10) return line.toString().trimEnd('\r')
      check(line.length < 16_384)
      line.append(byte.toChar())
    }
  }

  override fun close() { listener.close(); sockets.forEach { it.close() }; workers.shutdownNow() }

  private companion object { const val USER = "00000000000000000000000000000001" }
}
