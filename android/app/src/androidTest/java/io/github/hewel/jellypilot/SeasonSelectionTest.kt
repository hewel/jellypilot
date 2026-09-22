package io.github.hewel.jellypilot

import androidx.lifecycle.ViewModelStore
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.ffi.JellypilotSdk
import io.github.hewel.jellypilot.ffi.Provider
import io.github.hewel.jellypilot.ffi.SdkConfig
import io.github.hewel.jellypilot.ffi.SecureCredentialStore
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
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Real ViewModel → UniFFI → HTTP ordering, without the application's saved account. */
@RunWith(AndroidJUnit4::class)
class SeasonSelectionTest {
  @Test fun switchingSeasonReplacesPendingPageAndLeavingDetailRejectsLateEpisodes() = runBlocking {
    val instrumentation = InstrumentationRegistry.getInstrumentation()
    val app = instrumentation.targetContext.applicationContext as JellyPilotApplication
    val directory = File(app.cacheDir, "season-selection-${UUID.randomUUID()}").apply { mkdirs() }
    val credentials = object : SecureCredentialStore {
      private var blob: ByteArray? = null
      @Synchronized override fun read(): ByteArray? = blob?.copyOf()
      @Synchronized override fun write(secret: ByteArray) { blob = secret.copyOf() }
      @Synchronized override fun delete() { blob = null }
    }
    val sdk = JellypilotSdk(SdkConfig(directory.path, "Season selection fixture"), credentials, null)
    val store = ViewModelStore()
    val oldHandoff = app.beforePlaybackHandoff
    val oldIntent = app.player.businessIntent
    val oldEligibility = app.player.admissionEligible
    try {
      SeasonSelectionServer().use { server ->
        val candidate = withTimeout(15_000) {
          sdk.passwordLogin(Provider.JELLYFIN, server.baseUrl, "fixture-user", "fixture-password")
        }
        try { withTimeout(15_000) { sdk.activateCandidate(candidate, false) } }
        finally { candidate.destroy() }
        val model = withContext(Dispatchers.Main.immediate) {
          AppViewModel(app, sdk).also { store.put("season-selection", it) }
        }
        withTimeout(15_000) { model.state.first { !it.busy && it.items.any { item -> item.id == SERIES } } }
        withContext(Dispatchers.Main.immediate) { model.showDetail(SERIES) }
        val initial = withTimeout(15_000) {
          model.state.first { !it.busy && it.detail?.id == SERIES && it.detailItems.isNotEmpty() }
        }
        assertEquals(SEASON_A, initial.selectedSeasonId)
        assertEquals(listOf("A1"), initial.detailItems.map { it.title })
        assertTrue(initial.episodesHaveMore)

        withContext(Dispatchers.Main.immediate) { model.loadMoreEpisodes() }
        server.olderPage.awaitStarted("the original season's second page")
        withContext(Dispatchers.Main.immediate) {
          assertTrue(model.state.value.busy)
          model.loadMoreEpisodes() // Repeated pagination must not replace its own request.
          model.selectSeason(SEASON_B)
          model.selectSeason(SEASON_B) // Repeated selection must not replace its own first page.
          model.loadMoreEpisodes() // Nor may pagination race the new first page.
        }
        server.newSeason.awaitStarted("the replacement season's first page while the old page is pending")
        server.olderPage.releaseAndAwaitFinished()
        withContext(Dispatchers.Main.immediate) {
          val loading = model.state.value
          assertEquals(SEASON_B, loading.selectedSeasonId)
          assertTrue(loading.detailItems.isEmpty())
          assertTrue(loading.busy)
        }
        server.newSeason.releaseAndAwaitFinished()
        val replacement = withTimeout(15_000) {
          model.state.first { !it.busy && it.selectedSeasonId == SEASON_B && it.detailItems.isNotEmpty() }
        }
        assertEquals(listOf("B1"), replacement.detailItems.map { it.title })
        assertTrue(replacement.episodesHaveMore)
        assertNull(replacement.error)

        withContext(Dispatchers.Main.immediate) { model.loadMoreEpisodes() }
        server.leavingPage.awaitStarted("the new season's second page")
        withContext(Dispatchers.Main.immediate) {
          model.loadMoreEpisodes()
          model.back()
        }
        server.leavingPage.releaseAndAwaitFinished()
        instrumentation.waitForIdleSync()
        val left = model.state.value
        assertNull(left.detail)
        assertTrue(left.detailItems.isEmpty())
        assertFalse(left.busy)
        assertNull(left.error)
        assertEquals(listOf(SEASON_A to 0, SEASON_A to 1, SEASON_B to 0, SEASON_B to 1), server.episodeRequests.toList())
        assertTrue("unexpected fixture endpoints: ${server.unexpectedPaths}", server.unexpectedPaths.isEmpty())
        assertTrue("fixture failures: ${server.failures}", server.failures.isEmpty())
      }
    } finally {
      withContext(Dispatchers.Main.immediate) {
        store.clear()
        app.beforePlaybackHandoff = oldHandoff
        app.player.businessIntent = oldIntent
        app.player.setEligible(oldEligibility)
      }
      try { withTimeout(15_000) { sdk.disconnect() } }
      finally { sdk.shutdown(); sdk.destroy(); directory.deleteRecursively() }
    }
  }
}

private class EpisodeResponseGate {
  private val started = CountDownLatch(1)
  private val release = CountDownLatch(1)
  private val finished = CountDownLatch(1)

  fun hold() {
    started.countDown()
    check(release.await(30, TimeUnit.SECONDS)) { "episode response was never released" }
  }
  fun finish() { finished.countDown() }
  fun unblock() { release.countDown() }
  fun awaitStarted(description: String) {
    assertTrue("Timed out waiting for $description", started.await(15, TimeUnit.SECONDS))
  }
  fun releaseAndAwaitFinished() {
    unblock()
    assertTrue("Released episode response did not finish", finished.await(15, TimeUnit.SECONDS))
  }
}

private class SeasonSelectionServer : AutoCloseable {
  private val listener = ServerSocket(0, 8, InetAddress.getByName("127.0.0.1"))
  private val workers = Executors.newCachedThreadPool()
  private val sockets = ConcurrentLinkedQueue<Socket>()
  val olderPage = EpisodeResponseGate()
  val newSeason = EpisodeResponseGate()
  val leavingPage = EpisodeResponseGate()
  val episodeRequests = ConcurrentLinkedQueue<Pair<String, Int>>()
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
    var remaining = 0
    while (true) {
      val line = readLine(input)
      if (line.isEmpty()) break
      if (line.startsWith("Content-Length:", true)) remaining = line.substringAfter(':').trim().toInt()
    }
    while (remaining > 0) { check(input.read() >= 0); remaining-- }
    val path = uri.path
    if (path == "/Shows/$SERIES/Episodes") {
      val query = uri.rawQuery.orEmpty().split('&').associate {
        URLDecoder.decode(it.substringBefore('='), "UTF-8") to URLDecoder.decode(it.substringAfter('='), "UTF-8")
      }
      val season = requireNotNull(query["seasonId"])
      val start = requireNotNull(query["startIndex"]).toInt()
      episodeRequests += season to start
      val gate = when (season to start) {
        SEASON_A to 0 -> null
        SEASON_A to 1 -> olderPage
        SEASON_B to 0 -> newSeason
        SEASON_B to 1 -> leavingPage
        else -> error("unexpected episode page $season/$start")
      }
      try {
        gate?.hold()
        val prefix = if (season == SEASON_A) "A" else "B"
        val episode = media((if (season == SEASON_A) 0x100 else 0x200) + start, "$prefix${start + 1}", "Episode")
          .put("SeriesId", SERIES).put("SeriesName", "Fixture show").put("SeasonId", season)
          .put("ParentIndexNumber", if (season == SEASON_A) 1 else 2).put("IndexNumber", start + 1)
        try { json(socket, page(JSONArray().put(episode), 2, start).toString()) }
        catch (error: java.io.IOException) {
          // Replacement/back cancels the actual Rust HTTP future and may close this socket.
          if (gate !== olderPage && gate !== leavingPage) throw error
        }
      } finally { gate?.finish() }
      return
    }
    val body = when (path) {
      "/Users/AuthenticateByName" -> JSONObject().put("User", JSONObject().put("Id", USER).put("Name", "Fixture user"))
        .put("AccessToken", "synthetic-token").put("ServerId", "fixture-server").toString()
      "/System/Info/Public" -> "{\"ServerName\":\"Fixture\",\"Version\":\"10.10.0\",\"Id\":\"fixture-server\"}"
      "/UserViews" -> page(JSONArray().put(media(0x20, "Fixture TV", "CollectionFolder").put("CollectionType", "tvshows")), 1).toString()
      "/Items/Latest" -> JSONArray().put(show()).toString()
      "/UserItems/Resume", "/Shows/NextUp", "/Items/$SERIES/Similar" -> page(JSONArray(), 0).toString()
      "/Items/$SERIES" -> show().toString()
      "/Shows/$SERIES/Seasons" -> page(JSONArray()
        .put(media(0xa0, "Season A", "Season").put("IndexNumber", 1))
        .put(media(0xb0, "Season B", "Season").put("IndexNumber", 2)), 2).toString()
      else -> { unexpectedPaths += path; "{}" }
    }
    json(socket, body)
  }

  private fun show() = media(0x10, "Fixture show", "Series").put("OriginalLanguage", "en")
  private fun media(id: Int, name: String, type: String): JSONObject {
    val key = id.toString(16).padStart(32, '0')
    return JSONObject().put("Id", key).put("Name", name).put("Type", type)
      .put("UserData", JSONObject().put("Key", key).put("Played", false).put("IsFavorite", false))
  }
  private fun page(items: JSONArray, total: Int, start: Int = 0) =
    JSONObject().put("Items", items).put("TotalRecordCount", total).put("StartIndex", start)

  private fun json(socket: Socket, body: String) {
    val bytes = body.toByteArray(Charsets.UTF_8)
    socket.getOutputStream().apply {
      write("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: ${bytes.size}\r\nConnection: close\r\n\r\n".toByteArray(Charsets.US_ASCII))
      write(bytes)
      flush()
    }
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

  override fun close() {
    listener.close()
    listOf(olderPage, newSeason, leavingPage).forEach { it.unblock() }
    sockets.forEach { it.close() }
    workers.shutdownNow()
  }
}

private const val USER = "00000000000000000000000000000001"
private const val SERIES = "00000000000000000000000000000010"
private const val SEASON_A = "000000000000000000000000000000a0"
private const val SEASON_B = "000000000000000000000000000000b0"
