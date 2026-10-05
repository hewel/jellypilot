package io.github.hewel.jellypilot

import androidx.lifecycle.ViewModelStore
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.ffi.JellypilotSdk
import io.github.hewel.jellypilot.ffi.Provider
import io.github.hewel.jellypilot.ffi.SdkConfig
import io.github.hewel.jellypilot.ffi.SecureCredentialStore
import io.github.hewel.jellypilot.ui.AppUiState
import io.github.hewel.jellypilot.ui.BrowseUiStatus
import io.github.hewel.jellypilot.ui.ConnectionHealth
import io.github.hewel.jellypilot.ui.Destination
import io.github.hewel.jellypilot.ui.LoginStep
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
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Exercises the real AppViewModel → UniFFI → authenticated HTTP boundary with isolated accounts. */
@RunWith(AndroidJUnit4::class)
class AccountConnectionHealthTest {
  @Test fun failedProbeRetainsSessionAndRetryDeduplicatesWithoutReauthentication() = fixture { app, model, sdk, server ->
    val scopeToken = sdk.newOperationToken()
    val originalScope = try { scopeToken.scopeRef() } finally { scopeToken.cancel(); scopeToken.destroy() }
    val originalIntent = app.player.businessIntent
    val requestsBefore = server.viewRequests.get()
    val failed = server.holdNextViews(503)
    withContext(Dispatchers.Main) { model.navigate(Destination.Account) }
    failed.awaitRequest()
    withContext(Dispatchers.Main) { model.retryConnectionCheck(); model.retryConnectionCheck() }
    assertEquals(ConnectionHealth.Checking, model.state.value.connectionHealth)
    assertEquals(requestsBefore + 1, server.viewRequests.get())
    failed.release()
    settled(model) { it.connectionHealth == ConnectionHealth.Failed }
    assertEquals("viewer", model.state.value.activeName)
    assertEquals(originalScope.profileKey, sdk.activeProfile()?.key)
    assertTrue(sdk.isScopeActive(originalScope))
    assertEquals(1, sdk.savedProfiles().profiles.size)

    val retry = server.holdNextViews(200)
    withContext(Dispatchers.Main) { model.retryConnectionCheck(); model.retryConnectionCheck() }
    retry.awaitRequest()
    assertEquals(requestsBefore + 2, server.viewRequests.get())
    retry.release()
    settled(model) { it.connectionHealth == ConnectionHealth.Connected }
    assertEquals(1, server.authentications.get())
    assertEquals(0, server.unauthenticatedViews.get())
    assertTrue(sdk.isScopeActive(originalScope))
    assertEquals(originalIntent, app.player.businessIntent)

    // A missing media item is a failed request, but a successful independent probe keeps My connected.
    val afterMissingItem = server.holdNextViews(200)
    withContext(Dispatchers.Main) { model.showDetail(MISSING_ITEM) }
    afterMissingItem.awaitRequest()
    afterMissingItem.release()
    settled(model) { !it.busy && it.error != null && it.connectionHealth == ConnectionHealth.Connected }
    assertTrue(sdk.isScopeActive(originalScope))

    // The SDK browse failure is also only a reason to probe, not evidence of a dead server.
    server.itemsStatus.set(404)
    val afterBrowserFailure = server.holdNextViews(200)
    withContext(Dispatchers.Main) { model.navigate(Destination.Library) }
    afterBrowserFailure.awaitRequest()
    afterBrowserFailure.release()
    settled(model) { it.browser.status == BrowseUiStatus.Failed && it.connectionHealth == ConnectionHealth.Connected }
  }

  @Test fun accountSwitchAndDisconnectFenceLateProbeResponses() = fixture { _, model, sdk, server ->
    val previousKey = sdk.activeProfile()?.key
    val oldProbe = server.holdNextViews(503)
    withContext(Dispatchers.Main) { model.navigate(Destination.Account) }
    oldProbe.awaitRequest()
    withContext(Dispatchers.Main) {
      model.addAccount()
      model.changeLoginServer(server.baseUrl)
      model.connectLoginServer()
    }
    settled(model) { !it.loginBusy && it.loginStep == LoginStep.Account }
    withContext(Dispatchers.Main) { model.changeLoginUsername("second"); model.submitLogin() }
    settled(model) { !it.loginBusy && !it.busy && it.activeProfileKey != previousKey && it.activeName == "second" }
    withContext(Dispatchers.Main) { model.navigate(Destination.Account) }
    settled(model) { it.connectionHealth == ConnectionHealth.Connected }
    oldProbe.release()
    oldProbe.awaitFinished()
    withContext(Dispatchers.Main) {
      assertEquals("second", model.state.value.activeName)
      assertEquals(ConnectionHealth.Connected, model.state.value.connectionHealth)
      assertEquals(sdk.activeProfile()?.key, model.state.value.activeProfileKey)
    }

    val disconnecting = server.holdNextViews(200)
    withContext(Dispatchers.Main) { model.retryConnectionCheck() }
    disconnecting.awaitRequest()
    withContext(Dispatchers.Main) { model.disconnect() }
    settled(model) { !it.loginBusy && it.activeProfileKey == null }
    disconnecting.release()
    disconnecting.awaitFinished()
    withContext(Dispatchers.Main) {
      assertNull(sdk.activeProfile())
      assertEquals(ConnectionHealth.Unchecked, model.state.value.connectionHealth)
    }
  }

  private suspend fun settled(model: AppViewModel, predicate: (AppUiState) -> Boolean): AppUiState =
    withTimeout(15_000) { model.state.first(predicate) }

  private fun fixture(test: suspend (JellyPilotApplication, AppViewModel, JellypilotSdk, HealthFixtureServer) -> Unit) = runBlocking {
    val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as JellyPilotApplication
    val directory = File(app.cacheDir, "account-health-${UUID.randomUUID()}").apply { mkdirs() }
    val credentials = object : SecureCredentialStore {
      private var value: ByteArray? = null
      @Synchronized override fun read(): ByteArray? = value?.copyOf()
      @Synchronized override fun write(secret: ByteArray) { value = secret.copyOf() }
      @Synchronized override fun delete() { value = null }
    }
    val sdk = JellypilotSdk(SdkConfig(directory.path, "Account health fixture"), credentials, null)
    val handoff = app.beforePlaybackHandoff
    val intent = app.player.businessIntent
    val eligible = app.player.admissionEligible
    val store = ViewModelStore()
    try {
      HealthFixtureServer().use { server ->
        val candidate = sdk.passwordLogin(Provider.JELLYFIN, server.baseUrl, "viewer", "")
        try { sdk.activateCandidate(candidate, true) } finally { candidate.destroy() }
        val model = withContext(Dispatchers.Main) { AppViewModel(app, sdk).also { store.put("account-health", it) } }
        settled(model) { !it.busy && it.libraries.isNotEmpty() && it.activeName == "viewer" }
        test(app, model, sdk, server)
      }
    } finally {
      withContext(Dispatchers.Main) {
        store.clear()
        app.beforePlaybackHandoff = handoff
        app.player.businessIntent = intent
        app.player.setEligible(eligible)
      }
      withTimeout(15_000) { sdk.disconnect() }
      sdk.shutdown()
      sdk.destroy()
      directory.deleteRecursively()
    }
  }

  private companion object { const val MISSING_ITEM = "000000000000000000000000000000ff" }
}

private class HealthFixtureServer : AutoCloseable {
  class Gate(val status: Int) {
    private val requested = CountDownLatch(1)
    private val response = CountDownLatch(1)
    private val finished = CountDownLatch(1)
    fun awaitRequest() = assertTrue("authenticated probe never reached fixture", requested.await(15, TimeUnit.SECONDS))
    fun release() = response.countDown()
    fun awaitFinished() = assertTrue("fixture did not finish response", finished.await(15, TimeUnit.SECONDS))
    fun waitForRelease() { requested.countDown(); check(response.await(30, TimeUnit.SECONDS)) }
    fun complete() = finished.countDown()
  }
  private val listener = ServerSocket(0, 8, InetAddress.getByName("127.0.0.1"))
  private val workers = Executors.newCachedThreadPool()
  private val sockets = ConcurrentLinkedQueue<Socket>()
  private val gates = ConcurrentLinkedQueue<Gate>()
  private val allGates = ConcurrentLinkedQueue<Gate>()
  val viewRequests = AtomicInteger(0)
  val unauthenticatedViews = AtomicInteger(0)
  val authentications = AtomicInteger(0)
  val itemsStatus = AtomicInteger(200)
  val baseUrl = "http://127.0.0.1:${listener.localPort}/media"

  init {
    workers.execute {
      while (!listener.isClosed) {
        val socket = try { listener.accept() } catch (_: java.io.IOException) { break }
        sockets += socket
        workers.execute { try { socket.use(::serve) } finally { sockets.remove(socket) } }
      }
    }
  }
  fun holdNextViews(status: Int) = Gate(status).also { gates += it; allGates += it }

  private fun serve(socket: Socket) {
    var gate: Gate? = null
    try {
      socket.soTimeout = 15_000
      val input = BufferedInputStream(socket.getInputStream())
      val request = readLine(input).split(' ')
      check(request.size >= 2)
      val path = URI(request[1]).path
      var bytes = 0
      var authenticated = false
      while (true) {
        val line = readLine(input)
        if (line.isEmpty()) break
        if (line.startsWith("Content-Length:", ignoreCase = true)) bytes = line.substringAfter(':').trim().toInt()
        if (line.contains("Authorization:", ignoreCase = true) && line.contains("fixture-token")) authenticated = true
      }
      val requestBody = ByteArray(bytes)
      repeat(bytes) { index -> requestBody[index] = input.read().also { check(it >= 0) }.toByte() }
      val authentication = path.endsWith("/Users/AuthenticateByName")
      val views = path.endsWith("/UserViews") || path.endsWith("/Views")
      if (views) {
        viewRequests.incrementAndGet()
        if (!authenticated) unauthenticatedViews.incrementAndGet()
        gate = gates.poll()
        gate?.waitForRelease()
      }
      val status = when {
        gate != null -> gate.status
        path.endsWith("000000000000000000000000000000ff") -> 404
        path.endsWith("/Items") -> itemsStatus.get()
        else -> 200
      }
      val content = when {
        path.endsWith("/System/Info/Public") -> """{"ServerName":"Health fixture","ProductName":"Jellyfin Server","Version":"10.10.0","Id":"fixture-server"}"""
        authentication -> {
          authentications.incrementAndGet()
          val username = JSONObject(String(requestBody)).optString("Username")
          val id = if (username == "second") "00000000000000000000000000000002" else "00000000000000000000000000000001"
          """{"User":{"Id":"$id","Name":"$username"},"AccessToken":"fixture-token","ServerId":"fixture-server"}"""
        }
        views -> """{"Items":[{"Id":"00000000000000000000000000000010","Name":"Movies","Type":"CollectionFolder","CollectionType":"movies"}],"TotalRecordCount":1}"""
        path.endsWith("/Latest") -> "[]"
        path.endsWith("/Items") || path.endsWith("/Resume") || path.endsWith("/NextUp") -> """{"Items":[],"TotalRecordCount":0}"""
        else -> "{}"
      }.toByteArray()
      socket.getOutputStream().apply {
        write("HTTP/1.1 $status Test\r\nContent-Type: application/json\r\nContent-Length: ${content.size}\r\nConnection: close\r\n\r\n".toByteArray())
        write(content)
        flush()
      }
    } catch (_: java.io.IOException) { /* Cancellation closes old-scope requests before a held response is released. */ }
    finally { gate?.complete() }
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
  override fun close() { listener.close(); allGates.forEach { it.release() }; sockets.forEach { it.close() }; workers.shutdownNow() }
}
