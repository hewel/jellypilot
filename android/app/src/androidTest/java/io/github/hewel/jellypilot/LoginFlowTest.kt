package io.github.hewel.jellypilot

import androidx.lifecycle.ViewModelStore
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.ffi.JellypilotSdk
import io.github.hewel.jellypilot.ffi.Provider
import io.github.hewel.jellypilot.ffi.SdkConfig
import io.github.hewel.jellypilot.ffi.SecureCredentialStore
import io.github.hewel.jellypilot.ui.AppUiState
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
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.json.JSONObject

/** Real SDK/UniFFI requests with controlled completion; never touches the app's accounts. */
@RunWith(AndroidJUnit4::class)
class LoginFlowTest {
  @Test fun identifiesServerAndKeepsCredentialsForDistinctAuthAndNetworkFailures() = fixture { app, model, sdk, server ->
    withContext(Dispatchers.Main) {
      model.addAccount()
      model.changeLoginServer("media.example.test")
      model.connectLoginServer()
    }
    assertEquals(app.localizedString(R.string.login_invalid_address), settled(model) { it.loginError != null }.loginError)
    assertEquals(0, server.requests.get())
    withContext(Dispatchers.Main) { model.changeLoginServer(server.baseUrl); model.connectLoginServer() }
    val connected = settled(model) { it.loginStep == LoginStep.Account }
    assertEquals("Fixture media room", connected.loginIdentity?.name)
    assertEquals(server.baseUrl, connected.loginIdentity?.address)
    assertEquals(true, connected.loginIdentity?.jellyfin)
    assertTrue(connected.loginIdentity?.providerKnown == true)
    assertNull(sdk.activeProfile())
    assertTrue(sdk.savedProfiles().profiles.isEmpty())

    withContext(Dispatchers.Main) {
      model.changeLoginUsername("viewer")
      model.changeLoginPassword("keep-only-in-memory")
      model.submitLogin()
    }
    val rejected = settled(model) { it.loginError != null }
    assertEquals(app.localizedString(R.string.login_credentials_failed), rejected.loginError)
    assertEquals("keep-only-in-memory", rejected.loginPassword)
    assertFalse(rejected.loginConnectionLost)
    assertNull(sdk.activeProfile())

    server.authStatus.set(503)
    withContext(Dispatchers.Main) { model.submitLogin() }
    val offline = settled(model) { it.loginConnectionLost }
    assertEquals(app.localizedString(R.string.login_connection_lost), offline.loginError)
    assertEquals("keep-only-in-memory", offline.loginPassword)
    withContext(Dispatchers.Main) { model.loginBack() }
    assertEquals(server.baseUrl, model.state.value.loginServer)
    assertEquals("viewer", model.state.value.loginUsername)
    withContext(Dispatchers.Main) { model.changeLoginServer("${server.baseUrl}/changed") }
    assertNull(model.state.value.loginIdentity)
    assertEquals("", model.state.value.loginPassword)
    withContext(Dispatchers.Main) { model.cancelLogin() }
    assertFalse(model.state.value.showSignIn)
    assertTrue(sdk.savedProfiles().profiles.isEmpty())
  }

  @Test fun backCancelsPendingProbeAndLoginAndLateSuccessCannotActivate() = fixture { _, model, sdk, server ->
    val probe = server.holdNext()
    withContext(Dispatchers.Main) { model.addAccount(); model.changeLoginServer(server.baseUrl); model.connectLoginServer(); model.connectLoginServer() }
    probe.awaitRequest()
    assertEquals(1, server.requests.get())
    withContext(Dispatchers.Main) { model.loginBack() }
    assertFalse(settled(model) { !it.showSignIn }.showSignIn)
    probe.release()
    probe.awaitFinished()

    withContext(Dispatchers.Main) { model.addAccount(); model.connectLoginServer() }
    settled(model) { it.loginStep == LoginStep.Account }
    server.authStatus.set(200)
    val login = server.holdNext()
    withContext(Dispatchers.Main) {
      model.changeLoginUsername("viewer")
      model.changeLoginPassword("temporary")
      model.submitLogin()
    }
    login.awaitRequest()
    withContext(Dispatchers.Main) { model.loginBack(); model.cancelLogin() }
    assertFalse(settled(model) { !it.showSignIn }.showSignIn)
    assertEquals("", model.state.value.loginPassword)
    login.release()
    login.awaitFinished()
    // Drive another complete real request after the cancelled response. Its result must own the UI.
    withContext(Dispatchers.Main) { model.addAccount(); model.connectLoginServer() }
    settled(model) { it.loginStep == LoginStep.Account }
    assertNull(sdk.activeProfile())
    assertTrue(sdk.savedProfiles().profiles.isEmpty())
    assertEquals("", model.state.value.loginPassword)
  }

  @Test fun restrictedEmbyAllowsExplicitEmptyPasswordLoginAndCancelledAttemptKeepsCurrentProfile() = fixture { _, model, sdk, server ->
    server.authStatus.set(200)
    val existing = sdk.passwordLogin(Provider.JELLYFIN, server.baseUrl, "previous-account", "")
    try { sdk.activateCandidate(existing, false) } finally { existing.destroy() }
    val previousKey = requireNotNull(sdk.activeProfile()).key
    server.publicInfoStatus.set(403)
    withContext(Dispatchers.Main) { model.addAccount(); model.changeLoginServer(server.baseUrl); model.connectLoginServer() }
    settled(model) { it.loginPublicInfoRestricted }
    assertNull(model.state.value.loginIdentity)
    withContext(Dispatchers.Main) {
      model.continueLoginManually()
      model.changeLoginUsername("viewer")
      model.submitLogin() // A manual flow must not guess the provider.
    }
    assertEquals(previousKey, sdk.activeProfile()?.key)
    assertFalse(model.state.value.loginBusy)
    assertFalse(requireNotNull(model.state.value.loginIdentity).providerSelected)
    assertNull(model.state.value.loginIdentity?.name)
    val cancelled = server.holdNextEmbyAuthentication()
    withContext(Dispatchers.Main) { model.changeLoginProvider(false); model.submitLogin() }
    cancelled.awaitRequest()
    assertEquals(previousKey, sdk.activeProfile()?.key)
    withContext(Dispatchers.Main) { model.loginBack(); model.cancelLogin() }
    settled(model) { !it.showSignIn }
    cancelled.release()
    cancelled.awaitFinished()

    withContext(Dispatchers.Main) { model.addAccount(); model.connectLoginServer() }
    settled(model) { it.loginPublicInfoRestricted }
    assertEquals(previousKey, sdk.activeProfile()?.key)
    withContext(Dispatchers.Main) {
      model.continueLoginManually()
      model.changeLoginProvider(false)
      model.changeLoginRemember(true)
      model.submitLogin()
    }
    settled(model) { !it.showSignIn && it.activeProfileKey != previousKey }
    assertEquals(Provider.EMBY, sdk.activeProfile()?.provider)
    assertEquals("viewer", sdk.activeProfile()?.userName)
    assertEquals("", model.state.value.loginPassword)
    assertTrue("real Emby fallback must receive an explicitly empty password", server.receivedEmptyEmbyPassword.get())
    assertEquals(1, sdk.savedProfiles().profiles.size)
  }

  private suspend fun settled(model: AppViewModel, predicate: (AppUiState) -> Boolean): AppUiState =
    withTimeout(15_000) { model.state.first { !it.loginBusy && predicate(it) } }

  private fun fixture(test: suspend (JellyPilotApplication, AppViewModel, JellypilotSdk, LoginFixtureServer) -> Unit) = runBlocking {
    val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as JellyPilotApplication
    val directory = File(app.cacheDir, "login-flow-${UUID.randomUUID()}").apply { mkdirs() }
    val credentials = object : SecureCredentialStore {
      private var value: ByteArray? = null
      @Synchronized override fun read(): ByteArray? = value?.copyOf()
      @Synchronized override fun write(secret: ByteArray) { value = secret.copyOf() }
      @Synchronized override fun delete() { value = null }
    }
    val sdk = JellypilotSdk(SdkConfig(directory.path, "Login flow fixture"), credentials, null)
    val handoff = app.beforePlaybackHandoff
    val intent = app.player.businessIntent
    val eligible = app.player.admissionEligible
    val store = ViewModelStore()
    val model = withContext(Dispatchers.Main) { AppViewModel(app, sdk).also { store.put("login", it) } }
    try { LoginFixtureServer().use { server -> test(app, model, sdk, server) } }
    finally {
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
}

private class LoginFixtureServer : AutoCloseable {
  class Gate {
    private val requested = CountDownLatch(1)
    private val response = CountDownLatch(1)
    private val finished = CountDownLatch(1)
    fun awaitRequest() = assertTrue("request never reached fixture", requested.await(15, TimeUnit.SECONDS))
    fun release() = response.countDown()
    fun awaitFinished() = assertTrue("fixture did not finish response", finished.await(15, TimeUnit.SECONDS))
    fun waitForRelease() { requested.countDown(); check(response.await(15, TimeUnit.SECONDS)) }
    fun complete() = finished.countDown()
  }
  private val listener = ServerSocket(0, 8, InetAddress.getByName("127.0.0.1"))
  private val workers = Executors.newCachedThreadPool()
  private val sockets = ConcurrentLinkedQueue<Socket>()
  private val gates = ConcurrentLinkedQueue<Gate>()
  private val embyAuthGates = ConcurrentLinkedQueue<Gate>()
  val requests = AtomicInteger(0)
  val authStatus = AtomicInteger(401)
  val publicInfoStatus = AtomicInteger(200)
  val receivedEmptyEmbyPassword = AtomicBoolean(false)
  val baseUrl = "http://127.0.0.1:${listener.localPort}/media"

  init {
    workers.execute {
      while (!listener.isClosed) {
        val socket = try { listener.accept() } catch (_: java.io.IOException) { break }
        sockets += socket
        workers.execute { try { socket.use { serve(it) } } finally { sockets.remove(socket) } }
      }
    }
  }
  fun holdNext() = Gate().also(gates::add)
  fun holdNextEmbyAuthentication() = Gate().also(embyAuthGates::add)

  private fun serve(socket: Socket) {
    var gate = gates.poll()
    try {
      socket.soTimeout = 15_000
      val input = BufferedInputStream(socket.getInputStream())
      val request = readLine(input).split(' ')
      check(request.size >= 2)
      val path = URI(request[1]).path
      var bytes = 0
      while (true) {
        val line = readLine(input)
        if (line.isEmpty()) break
        if (line.startsWith("Content-Length:", ignoreCase = true)) bytes = line.substringAfter(':').trim().toInt()
      }
      val requestBody = ByteArray(bytes)
      repeat(bytes) { index -> requestBody[index] = input.read().also { check(it >= 0) }.toByte() }
      val authentication = path.endsWith("/Users/AuthenticateByName")
      val embyAuthentication = authentication && path.contains("/emby/")
      if (embyAuthentication) {
        gate = gate ?: embyAuthGates.poll()
        receivedEmptyEmbyPassword.set(JSONObject(String(requestBody)).optString("Pw", "missing").isEmpty())
      }
      requests.incrementAndGet()
      gate?.waitForRelease()
      val status = when {
        path.endsWith("/System/Info/Public") -> publicInfoStatus.get()
        authentication && publicInfoStatus.get() == 403 && !embyAuthentication -> 403
        authentication -> authStatus.get()
        else -> 200
      }
      val content = when {
        path.endsWith("/System/Info/Public") -> """{"ServerName":"Fixture media room","ProductName":"Jellyfin Server","Version":"10.10.0","Id":"fixture-server"}"""
        path.endsWith("/System/Info") -> """{"ServerName":"Fixture Emby room","Version":"4.9.3.0","Id":"fixture-server"}"""
        authentication -> """{"User":{"Id":"00000000000000000000000000000001","Name":"${JSONObject(String(requestBody)).optString("Username")}"},"AccessToken":"fixture-token","ServerId":"fixture-server"}"""
        else -> "{}"
      }.toByteArray()
      socket.getOutputStream().apply {
        write("HTTP/1.1 $status Test\r\nContent-Type: application/json\r\nContent-Length: ${content.size}\r\nConnection: close\r\n\r\n".toByteArray())
        write(content)
        flush()
      }
    } catch (_: java.io.IOException) { /* Cancelled requests may close their socket before the released response. */ }
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
  override fun close() { listener.close(); gates.forEach { it.release() }; embyAuthGates.forEach { it.release() }; sockets.forEach { it.close() }; workers.shutdownNow() }
}
