package io.github.hewel.jellypilot

import androidx.lifecycle.ViewModelStore
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.ffi.*
import io.github.hewel.jellypilot.ui.AppUiState
import io.github.hewel.jellypilot.ui.Destination
import io.github.hewel.jellypilot.ui.LoginStep
import io.github.hewel.jellypilot.ui.PersonalListKind
import java.io.BufferedInputStream
import java.io.File
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.URI
import java.net.URLDecoder
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Retained collection behavior through the real ViewModel → UniFFI → HTTP boundary. */
@RunWith(AndroidJUnit4::class)
class PersonalListRetentionTest {
  private var diagnosticServer: PersonalListServer? = null
  @Test fun returningFromDetailRefreshesDirtyWatchlistWithoutLosingLoadedDepth() = fixture { model, _ ->
    onMain { model.navigate(Destination.Lists) }
    settled(model, "Await it.listItems.size == 50 && it.favoriteCount == 75") { it.listItems.size == 50 && it.favoriteCount == 75 }
    onMain { model.loadMoreList() }
    settled(model, "Await it.listItems.size == 75") { it.listItems.size == 75 }
    val added = listItemId(0)
    onMain { model.showDetail(added) }
    settled(model, "Await it.detail?.id == added") { it.detail?.id == added }
    onMain { model.setWatchlist(added, true) }
    settled(model, "Await it.detail?.inWatchlist == true && it.detail.updating == false") { it.detail?.inWatchlist == true && it.detail.updating == false }
    onMain { model.back() }
    val returned = settled(model, "Await it.detail == null && it.listItems.any { item -> item.id == added }") { it.detail == null && it.listItems.any { item -> item.id == added } }
    assertEquals(76, returned.listItems.size)
    assertEquals(76, returned.listCount)
    assertFalse(returned.listHasMore)
  }

  @Test fun queuedPaginationRebuildsDirtyFavoritePrefixBeforeContinuing() = fixture { model, _ ->
    onMain { model.navigate(Destination.Lists); model.selectList(PersonalListKind.Favorites) }
    settled(model, "Await it.listItems.size == 50") { it.listItems.size == 50 }
    val added = listItemId(0)
    onMain { model.showDetail(added) }
    settled(model, "Await it.detail?.id == added") { it.detail?.id == added }
    onMain { model.setFavorite(added, true) }
    settled(model, "Await it.detail?.favorite == true && it.detail.updating == false") { it.detail?.favorite == true && it.detail.updating == false }
    // A queued source-list pagination intent may reach the ViewModel while detail is still open.
    onMain { model.loadMoreList() }
    val paged = settled(model, "Await it.listItems.size == 76") { it.listItems.size == 76 }
    assertEquals(added, paged.listItems.first().id)
    assertEquals(76, paged.favoriteCount)
    assertFalse(paged.listHasMore)
    onMain { model.back() }
    assertEquals(paged.listItems.map { it.id }, model.state.value.listItems.map { it.id })
  }

  @Test fun removalAndUndoResumeDirtyCollectionRefreshAfterTheirBusyGate() = fixture { model, server ->
    onMain { model.navigate(Destination.Lists) }
    settled(model, "Await it.listItems.size == 50 && it.favoriteCount == 75") { it.listItems.size == 50 && it.favoriteCount == 75 }
    onMain { model.loadMoreList() }
    settled(model, "Await it.listItems.size == 75") { it.listItems.size == 75 }
    onMain { model.selectList(PersonalListKind.Favorites) }
    settled(model, "Await it.listItems.size == 50") { it.listItems.size == 50 }
    val removed = model.state.value.listItems.first().id
    suspend fun dirtyWatchlist(id: String) {
      onMain { model.showDetail(id) }
      settled(model, "Await it.detail?.id == id") { it.detail?.id == id }
      onMain { model.setWatchlist(id, true) }
      settled(model, "Await it.detail?.inWatchlist == true && it.detail.updating == false") { it.detail?.inWatchlist == true && it.detail.updating == false }
      onMain { model.back() }
    }
    dirtyWatchlist(listItemId(0))
    val removal = server.holdNextFavoriteMutation()
    onMain { model.removeListItems(listOf(removed)) }
    removal.awaitStarted()
    onMain { model.selectList(PersonalListKind.Watchlist) }
    assertTrue(model.state.value.listBusy)
    assertFalse(model.state.value.listItems.any { it.id == listItemId(0) })
    removal.releaseAndAwaitFinished()
    settled(model, "Await !it.listBusy && it.listItems.size == 76") { !it.listBusy && it.listItems.size == 76 }

    onMain { model.selectList(PersonalListKind.Favorites) }
    settled(model, "Await it.listItems.isNotEmpty()") { it.listItems.isNotEmpty() }
    dirtyWatchlist(listItemId(-1))
    val undo = server.holdNextFavoriteMutation()
    onMain { model.undoListRemoval() }
    undo.awaitStarted()
    onMain { model.selectList(PersonalListKind.Watchlist) }
    assertTrue(model.state.value.listUndo?.busy == true)
    assertFalse(model.state.value.listItems.any { it.id == listItemId(-1) })
    undo.releaseAndAwaitFinished()
    val restored = settled(model, "Await it.listUndo == null && it.listItems.size == 77") { it.listUndo == null && it.listItems.size == 77 }
    assertEquals(77, restored.listCount)
    assertTrue(restored.listItems.any { it.id == listItemId(-1) })
  }

  @Test fun bothListsRetainLoadedPagesAndRemovalUndoCannotReviveOldMembership() = fixture { model, server ->
    onMain { model.navigate(Destination.Lists) }
    val first = settled(model, "Await it.listItems.size == 50 && it.favoriteCount == 75") { it.listItems.size == 50 && it.favoriteCount == 75 }
    assertEquals(75, first.listCount)
    onMain { model.loadMoreList() }
    val watchlist = settled(model, "Await it.listItems.size == 75") { it.listItems.size == 75 }
    onMain { model.selectList(PersonalListKind.Favorites) }
    settled(model, "Await it.listItems.size == 50") { it.listItems.size == 50 }
    onMain { model.loadMoreList() }
    val favorites = settled(model, "Await it.listItems.size == 75") { it.listItems.size == 75 }
    val requests = server.favoriteRequests.get()
    onMain { model.selectList(PersonalListKind.Watchlist) }
    assertEquals(watchlist.listItems.map { it.id }, model.state.value.listItems.map { it.id })
    onMain { model.selectList(PersonalListKind.Favorites) }
    assertEquals(favorites.listItems.map { it.id }, model.state.value.listItems.map { it.id })
    assertEquals(requests, server.favoriteRequests.get())

    val removed = favorites.listItems[60].id
    onMain { model.removeListItems(listOf(removed)) }
    settled(model, "Await !it.listBusy && it.listUndo != null") { !it.listBusy && it.listUndo != null }
    onMain { model.selectList(PersonalListKind.Watchlist); model.selectList(PersonalListKind.Favorites) }
    val after = settled(model, "Await it.listItems.size == 74") { it.listItems.size == 74 }
    assertFalse(after.listItems.any { it.id == removed })
    assertEquals(74, after.favoriteCount)
    onMain { model.undoListRemoval() }
    val undone = settled(model, "Await it.listUndo == null && it.listItems.size == 75") { it.listUndo == null && it.listItems.size == 75 }
    assertEquals(favorites.listItems.map { it.id }, undone.listItems.map { it.id })
    onMain { model.selectList(PersonalListKind.Watchlist); model.selectList(PersonalListKind.Favorites) }
    assertEquals(favorites.listItems.map { it.id }, model.state.value.listItems.map { it.id })
  }

  @Test fun cancelledPageCannotReplaceAnotherListAndAccountSwitchClearsRetainedContent() = fixture { model, server ->
    onMain { model.navigate(Destination.Lists) }
    settled(model, "Await it.favoriteCount == 75") { it.favoriteCount == 75 }
    onMain { model.selectList(PersonalListKind.Favorites) }
    settled(model, "Await it.listItems.size == 50") { it.listItems.size == 50 }
    val pending = server.holdNextFavoritePage()
    onMain { model.loadMoreList() }
    pending.awaitStarted()
    onMain { model.selectList(PersonalListKind.Watchlist) }
    pending.releaseAndAwaitFinished()
    onMain { model.selectList(PersonalListKind.Favorites) }
    assertEquals(50, model.state.value.listItems.size)
    onMain { model.loadMoreList() }
    settled(model, "Await it.listItems.size == 75") { it.listItems.size == 75 }

    val oldRefresh = server.holdNextFavoritePage()
    onMain { model.refreshPersonalList() }
    oldRefresh.awaitStarted()
    onMain { model.disconnect() }
    settled(model, "Await !it.loginBusy && it.activeName == null") { !it.loginBusy && it.activeName == null }
    oldRefresh.releaseAndAwaitFinished()
    onMain { model.addAccount(); model.changeLoginServer(server.baseUrl); model.connectLoginServer() }
    settled(model, "Await !it.loginBusy && it.loginStep == LoginStep.Account") { !it.loginBusy && it.loginStep == LoginStep.Account }
    onMain { model.changeLoginUsername("second"); model.submitLogin() }
    settled(model, "Await !it.loginBusy && it.activeName == \"second\"") { !it.loginBusy && it.activeName == "second" }
    onMain { model.navigate(Destination.Lists); model.selectList(PersonalListKind.Favorites) }
    val second = settled(model, "Await it.favoriteCount == 0") { it.favoriteCount == 0 }
    assertTrue(second.listItems.isEmpty())
    assertEquals(0, second.listCount)
  }

  @Test fun failedCountKeepsUsableWatchlistAndRemainsUnknownUntilRetry() = fixture { model, server ->
    server.favoriteStatus.set(503)
    onMain { model.navigate(Destination.Lists) }
    val failed = settled(model, "Await it.error != null") { it.error != null }
    assertEquals(50, failed.listItems.size)
    assertEquals(75, failed.listCount)
    assertNull(failed.favoriteCount)
    server.favoriteStatus.set(200)
    onMain { model.refreshPersonalList() }
    val retried = settled(model, "Await it.favoriteCount == 75") { it.favoriteCount == 75 }
    assertEquals(50, retried.listItems.size)
    assertNull(retried.error)
  }

  private suspend fun settled(model: AppViewModel, stage: String, predicate: (AppUiState) -> Boolean): AppUiState = try {
    withTimeout(15_000) { model.state.first { !it.busy && predicate(it) } }
  } catch (error: TimeoutCancellationException) {
    val state = model.state.value
    throw AssertionError("Timed out: $stage; active=${state.activeName}, destination=${state.destination}, " +
      "busy=${state.busy}, listBusy=${state.listBusy}, loginBusy=${state.loginBusy}, selected=${state.selectedList}, " +
      "items=${state.listItems.size}, watchlist=${state.listCount}, favorites=${state.favoriteCount}, more=${state.listHasMore}, " +
      "detail=${state.detail?.id}, detailUpdating=${state.detail?.updating}, detailWatchlist=${state.detail?.inWatchlist}, " +
      "undo=${state.listUndo}, error=${state.error}, loginError=${state.loginError}; " +
      "requests=${diagnosticServer?.requests}, unexpected=${diagnosticServer?.unexpected}, failures=${diagnosticServer?.failures}", error)
  }

  private suspend fun onMain(action: () -> Unit) = withContext(Dispatchers.Main.immediate) { action() }

  private fun fixture(test: suspend (AppViewModel, PersonalListServer) -> Unit) = runBlocking {
    val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as JellyPilotApplication
    val directory = File(app.cacheDir, "personal-lists-${UUID.randomUUID()}").apply { mkdirs() }
    val credentials = object : SecureCredentialStore {
      private var blob: ByteArray? = null
      @Synchronized override fun read() = blob?.copyOf()
      @Synchronized override fun write(secret: ByteArray) { blob = secret.copyOf() }
      @Synchronized override fun delete() { blob = null }
    }
    val sdk = JellypilotSdk(SdkConfig(directory.path, "Personal lists fixture"), credentials, null)
    val store = ViewModelStore()
    val previousHandoff = app.beforePlaybackHandoff
    val previousIntent = app.player.businessIntent
    val previousEligibility = app.player.admissionEligible
    try {
      PersonalListServer().use { server ->
        diagnosticServer = server
        val candidate = sdk.passwordLogin(Provider.JELLYFIN, server.baseUrl, "first", "")
        try { sdk.activateCandidate(candidate, false) } finally { candidate.destroy() }
        val token = sdk.newOperationToken()
        try {
          val items = sdk.videoItemsByIds(token, (1..75).map(::listItemId))
          assertEquals("Fixture item lookup must return all seeded items; requests=${server.requests}", 75, items.size)
          items.forEach { sdk.watchlistAdd(token, it) }
          assertEquals("Fixture must persist all Watchlist memberships before creating the ViewModel", 75, sdk.watchlistItems(token).size)
        } finally { token.cancel(); token.destroy() }
        val model = withContext(Dispatchers.Main.immediate) { AppViewModel(app, sdk).also { store.put("lists", it) } }
        settled(model, "Fixture initialized with first profile and 75 seeded watchlist records") { it.activeName == "first" && it.listCount == 75 }
        test(model, server)
        assertTrue("unexpected fixture endpoints: ${server.unexpected}", server.unexpected.isEmpty())
        assertTrue("fixture failures: ${server.failures}", server.failures.isEmpty())
      }
    } finally {
      onMain { store.clear(); app.beforePlaybackHandoff = previousHandoff; app.player.businessIntent = previousIntent; app.player.setEligible(previousEligibility) }
      try { withTimeout(15_000) { sdk.disconnect() } }
      finally { sdk.shutdown(); sdk.destroy(); directory.deleteRecursively() }
    }
  }
}

private class PersonalListGate {
  private val started = CountDownLatch(1)
  private val release = CountDownLatch(1)
  private val finished = CountDownLatch(1)
  fun hold() { started.countDown(); check(release.await(30, TimeUnit.SECONDS)) }
  fun finish() { finished.countDown() }
  fun unblock() { release.countDown() }
  fun awaitStarted() { assertTrue("request did not start", started.await(15, TimeUnit.SECONDS)) }
  fun releaseAndAwaitFinished() { unblock(); assertTrue("request did not finish", finished.await(15, TimeUnit.SECONDS)) }
}

private class PersonalListServer : AutoCloseable {
  private val listener = ServerSocket(0, 8, InetAddress.getByName("127.0.0.1"))
  private val workers = Executors.newCachedThreadPool()
  private val sockets = ConcurrentLinkedQueue<Socket>()
  private val favorites = ConcurrentHashMap.newKeySet<String>().apply { addAll((1..75).map(::listItemId)) }
  private val pending = AtomicReference<PersonalListGate?>(null)
  private val pendingMutation = AtomicReference<PersonalListGate?>(null)
  private val gates = ConcurrentLinkedQueue<PersonalListGate>()
  val favoriteRequests = AtomicInteger()
  val favoriteStatus = AtomicInteger(200)
  val unexpected = ConcurrentLinkedQueue<String>()
  val failures = ConcurrentLinkedQueue<String>()
  val requests = ConcurrentLinkedQueue<String>()
  val baseUrl = "http://127.0.0.1:${listener.localPort}"
  init {
    workers.execute {
      while (!listener.isClosed) {
        val socket = try { listener.accept() } catch (_: java.io.IOException) { break }
        sockets += socket
        workers.execute {
          try { socket.use(::serve) }
          catch (error: Exception) { if (!listener.isClosed) failures += error.toString() }
          finally { sockets.remove(socket) }
        }
      }
    }
  }
  fun holdNextFavoritePage() = PersonalListGate().also { gates += it; check(pending.compareAndSet(null, it)) }
  fun holdNextFavoriteMutation() = PersonalListGate().also { gates += it; check(pendingMutation.compareAndSet(null, it)) }
  private fun serve(socket: Socket) {
    socket.soTimeout = 15_000
    val input = BufferedInputStream(socket.getInputStream())
    val request = line(input).split(' ')
    val uri = URI(request[1])
    var length = 0
    var second = false
    while (true) {
      val header = line(input)
      if (header.isEmpty()) break
      if (header.startsWith("Content-Length:", true)) length = header.substringAfter(':').trim().toInt()
      if (header.contains("token-second")) second = true
    }
    val bytes = ByteArray(length)
    for (index in bytes.indices) bytes[index] = input.read().also { check(it >= 0) }.toByte()
    val query = uri.rawQuery.orEmpty().split('&').filter { it.isNotEmpty() }.map {
      URLDecoder.decode(it.substringBefore('='), "UTF-8").lowercase() to URLDecoder.decode(it.substringAfter('='), "UTF-8")
    }.groupBy({ it.first }, { it.second }).mapValues { (_, values) -> values.joinToString(",") }
    val path = uri.path
    requests += "${request[0]} $uri"
    if (path == "/Items" && query.containsKey("isfavorite")) {
      favoriteRequests.incrementAndGet()
      val gate = pending.getAndSet(null)
      try {
        gate?.hold()
        val start = query["startindex"]?.toInt() ?: 0
        val limit = query["limit"]?.toInt() ?: 50
        val ids = if (second) emptyList() else favorites.sorted()
        respond(socket, page(ids.drop(start).take(limit).map(::item), ids.size, start), favoriteStatus.get())
      } catch (error: java.io.IOException) { if (gate == null) throw error }
      finally { gate?.finish() }
      return
    }
    if (path.startsWith("/UserFavoriteItems/")) {
      val gate = pendingMutation.getAndSet(null)
      try {
        gate?.hold()
        val id = path.substringAfterLast('/').replace("-", "")
        if (request[0] == "DELETE") favorites.remove(id) else favorites.add(id)
        respond(socket, JSONObject().put("Key", id).put("Played", false).put("IsFavorite", id in favorites).toString())
      } finally { gate?.finish() }
      return
    }
    if (path.startsWith("/Items/")) {
      respond(socket, if (path.endsWith("/Similar")) page(emptyList(), 0)
        else item(path.substringAfterLast('/').replace("-", "")).toString())
      return
    }
    val body = when (path) {
      "/Users/AuthenticateByName" -> {
        val name = JSONObject(bytes.toString(Charsets.UTF_8)).getString("Username")
        JSONObject().put("User", JSONObject().put("Id", if (name == "second") USER_TWO else USER_ONE).put("Name", name))
          .put("AccessToken", "token-$name").put("ServerId", "fixture-server").toString()
      }
      "/System/Info/Public" -> "{\"ServerName\":\"Fixture\",\"ProductName\":\"Jellyfin Server\",\"Version\":\"10.10.0\",\"Id\":\"fixture-server\"}"
      "/UserViews", "/UserItems/Resume", "/Shows/NextUp" -> page(emptyList(), 0)
      "/Items" -> {
        val ids = query["ids"].orEmpty().split(',').filter { it.isNotEmpty() }.map { it.replace("-", "") }
        page(ids.map(::item), ids.size)
      }
      else -> { unexpected += path; "{}" }
    }
    respond(socket, body)
  }
  private fun item(id: String) = JSONObject().put("Id", id).put("Name", "Movie ${id.takeLast(4).toInt(16)}").put("Type", "Movie")
    .put("UserData", JSONObject().put("Key", id).put("Played", false).put("IsFavorite", id in favorites))
  private fun page(items: List<JSONObject>, total: Int, start: Int = 0) = JSONObject()
    .put("Items", JSONArray(items)).put("TotalRecordCount", total).put("StartIndex", start).toString()
  private fun respond(socket: Socket, body: String, status: Int = 200) {
    val bytes = body.toByteArray(Charsets.UTF_8)
    socket.getOutputStream().apply {
      write("HTTP/1.1 $status Fixture\r\nContent-Type: application/json\r\nContent-Length: ${bytes.size}\r\nConnection: close\r\n\r\n".toByteArray())
      write(bytes); flush()
    }
  }
  private fun line(input: BufferedInputStream): String {
    val result = StringBuilder()
    while (true) {
      val byte = input.read()
      if (byte < 0 || byte == 10) return result.toString().trimEnd('\r')
      check(result.length < 16_384)
      result.append(byte.toChar())
    }
  }
  override fun close() { listener.close(); gates.forEach { it.unblock() }; sockets.forEach { it.close() }; workers.shutdownNow() }
}

private fun listItemId(index: Int) = (0x100 + index).toString(16).padStart(32, '0')
private const val USER_ONE = "00000000000000000000000000000001"
private const val USER_TWO = "00000000000000000000000000000002"
