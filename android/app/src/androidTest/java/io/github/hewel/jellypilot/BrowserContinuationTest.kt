package io.github.hewel.jellypilot

import android.os.Looper
import androidx.lifecycle.ViewModelStore
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
import io.github.hewel.jellypilot.ui.AppUiState
import io.github.hewel.jellypilot.ui.BrowseUiStatus
import io.github.hewel.jellypilot.ui.Destination
import io.github.hewel.jellypilot.ui.LibrarySort
import io.github.hewel.jellypilot.ui.PlayedFilter
import java.io.BufferedInputStream
import java.io.File
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.URI
import java.net.URLDecoder
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
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withContext
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
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
                  session.setDisplayRange(before.viewportRevision, start, start + 6u)
                  phase.set("waiting for the updated snapshot at $start")
                  val updated = pending.await()
                  assertTrue("the setter must publish a new snapshot", updated.revision > before.revision)
                  assertEquals(start, updated.visibleStart)
                  assertEquals((start.toInt() until start.toInt() + 6).map { "Movie $it" }, updated.items.map { it?.name })
                }
                val before = session.snapshot()
                val pending = async(start = CoroutineStart.UNDISPATCHED) {
                  session.nextSnapshot(before.revision)
                }
                assertFalse(pending.isCompleted)
                phase.set("Main.immediate setPreferences has not returned")
                session.setPreferences(before.preferences.copy(favoritesOnly = true))
                val changed = pending.await()
                assertTrue(changed.preferences.favoritesOnly)
                assertNotEquals(before.identity, changed.identity)
                assertTrue(changed.viewportRevision > before.viewportRevision)
                val current = awaitReady(session)
                session.setDisplayRange(before.viewportRevision, 12u, 18u)
                assertEquals("old geometry must not advance the edited query", current.revision, session.snapshot().revision)
                assertEquals(0u, session.snapshot().visibleStart)
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

  @Test fun libraryEditsKeepTheirOwnerAndRejectOldViewportAcrossRetainedNavigation() = runBlocking {
    val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as JellyPilotApplication
    val directory = File(app.cacheDir, "browser-owner-${UUID.randomUUID()}").apply { mkdirs() }
    val credentials = object : SecureCredentialStore {
      private var blob: ByteArray? = null
      @Synchronized override fun read() = blob?.copyOf()
      @Synchronized override fun write(secret: ByteArray) { blob = secret.copyOf() }
      @Synchronized override fun delete() { blob = null }
    }
    val sdk = JellypilotSdk(SdkConfig(directory.path, "Browser owner fixture"), credentials, null)
    val store = ViewModelStore()
    val oldHandoff = app.beforePlaybackHandoff
    val oldIntent = app.player.businessIntent
    val oldEligibility = app.player.admissionEligible
    suspend fun onMain(action: () -> Unit) = withContext(Dispatchers.Main.immediate) { action() }
    suspend fun ready(model: AppViewModel, predicate: (AppUiState) -> Boolean = { true }): AppUiState =
      withTimeout(15_000) { model.state.first { !it.busy && it.browser.status == BrowseUiStatus.Ready && !it.browser.loadingMore && predicate(it) } }
    try {
      BrowserContinuationServer().use { server ->
        val candidate = sdk.passwordLogin(Provider.JELLYFIN, server.baseUrl, "fixture", "")
        try { sdk.activateCandidate(candidate, false) } finally { candidate.destroy() }
        val model = withContext(Dispatchers.Main.immediate) { AppViewModel(app, sdk).also { store.put("browse", it) } }
        withTimeout(15_000) { model.state.first { it.activeName != null && !it.busy && it.libraries.size == 2 } }
        onMain { model.navigate(Destination.Library) }
        val initial = ready(model)
        onMain { model.setBrowserDisplayRange(initial.browser.generation, initial.browser.viewportRevision, 12u, 18u) }
        ready(model) { it.browser.visibleStart == 12u }
        onMain { model.setLibrarySort(LibrarySort.Title); model.setLibraryFilters(PlayedFilter.Unplayed, true) }
        val edited = ready(model) { it.librarySort == LibrarySort.Title && it.libraryPlayed == PlayedFilter.Unplayed && it.libraryFavorites }
        assertEquals("condition edits retain the query owner", initial.browser.generation, edited.browser.generation)
        assertNotEquals(initial.browser.identity, edited.browser.identity)
        assertTrue(edited.browser.viewportRevision > initial.browser.viewportRevision)
        assertEquals(0u, edited.browser.visibleStart)
        onMain { model.setBrowserDisplayRange(initial.browser.generation, initial.browser.viewportRevision, 12u, 18u) }
        assertEquals(edited.browser.revision, model.state.value.browser.revision)

        onMain { model.navigate(Destination.Search); model.search("Movie") }
        ready(model)
        assertTrue("search snapshots must not overwrite ephemeral library conditions", model.state.value.libraryFavorites)
        onMain { model.back() }
        val restored = ready(model) { it.destination == Destination.Library }
        assertEquals(edited.browser.generation, restored.browser.generation)
        assertEquals(edited.browser.viewportRevision, restored.browser.viewportRevision)
        assertEquals(PlayedFilter.Unplayed, restored.libraryPlayed)
        onMain { model.selectLibrary(model.state.value.libraries.last().id) }
        val selected = ready(model) { it.browser.generation != edited.browser.generation }
        assertEquals(LibrarySort.Title, selected.librarySort)
        assertEquals(PlayedFilter.Unplayed, selected.libraryPlayed)
        assertTrue(selected.libraryFavorites)

        onMain { model.disconnect() }
        withTimeout(15_000) { model.state.first { !it.loginBusy && it.activeName == null } }
        onMain { model.setBrowserDisplayRange(selected.browser.generation, selected.browser.viewportRevision, 12u, 18u) }
        assertEquals(BrowseUiStatus.Inactive, model.state.value.browser.status)
        assertTrue("fixture received unexpected endpoints: ${server.unexpectedPaths}", server.unexpectedPaths.isEmpty())
        assertTrue("fixture request failures: ${server.failures}", server.failures.isEmpty())
      }
    } finally {
      onMain { store.clear(); app.beforePlaybackHandoff = oldHandoff; app.player.businessIntent = oldIntent; app.player.setEligible(oldEligibility) }
      try { withTimeout(15_000) { sdk.disconnect() } }
      finally { sdk.shutdown(); sdk.destroy(); directory.deleteRecursively() }
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
    val uri = URI(request[1])
    val path = uri.path
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
      "/UserViews" -> JSONObject().put("Items", JSONArray((0..1).map { index ->
        JSONObject().put("Id", (0x20 + index).toString(16).padStart(32, '0')).put("Name", "Library $index")
          .put("Type", "CollectionFolder").put("CollectionType", "movies")
      })).put("TotalRecordCount", 2).toString()
      "/Items/Latest" -> "[]"
      "/UserItems/Resume", "/Shows/NextUp" -> "{\"Items\":[],\"TotalRecordCount\":0}"
      "/Items" -> libraryPage(uri)
      else -> { unexpectedPaths += path; "{}" }
    }.toByteArray(Charsets.UTF_8)
    socket.getOutputStream().apply {
      write("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: ${body.size}\r\nConnection: close\r\n\r\n".toByteArray(Charsets.US_ASCII))
      write(body)
      flush()
    }
  }

  private fun libraryPage(uri: URI): String {
    val query = uri.rawQuery.orEmpty().split('&').filter { it.isNotEmpty() }.associate {
      URLDecoder.decode(it.substringBefore('='), "UTF-8").lowercase() to URLDecoder.decode(it.substringAfter('='), "UTF-8")
    }
    val filtered = query["isfavorite"] == "true"
    val count = if (filtered) 12 else 24
    val items = JSONArray()
    repeat(count) { index ->
      val id = index.toString(16).padStart(32, '0')
      items.put(JSONObject().put("Id", id).put("Name", "Movie $index").put("Type", "Movie")
        .put("UserData", JSONObject().put("Key", id).put("Played", false).put("IsFavorite", filtered)))
    }
    return JSONObject().put("Items", items).put("TotalRecordCount", count).put("StartIndex", 0).toString()
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
