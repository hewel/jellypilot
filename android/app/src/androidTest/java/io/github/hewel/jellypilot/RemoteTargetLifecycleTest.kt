package io.github.hewel.jellypilot

import android.content.ContextWrapper
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.bridge.KeystoreCredentialStore
import io.github.hewel.jellypilot.ffi.JellypilotSdk
import io.github.hewel.jellypilot.ffi.Provider
import io.github.hewel.jellypilot.ffi.SdkConfig
import io.github.hewel.jellypilot.player.PlayerStatus
import java.io.BufferedInputStream
import java.io.File
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.URI
import java.security.MessageDigest
import java.util.Base64
import java.util.UUID
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.json.JSONObject
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Exercises native UniFFI cancellation callbacks through a real remote Playback Target. */
@RunWith(AndroidJUnit4::class)
class RemoteTargetLifecycleTest {
  @Test fun backgroundingRegisteredTargetCanReturnAndRegisterAgain() = withFixture { player, coordinator, server, errors ->
    repeat(3) {
      withContext(Dispatchers.Main) { coordinator.setEligible(true) }
      val connection = server.awaitConnection()
      confirmRemoteCommand(connection, player)
      // Same public boundary invoked by Activity.onStop during rotation/backgrounding.
      withContext(Dispatchers.Main) { coordinator.setEligible(false) }
      connection.awaitClosed()
    }
    assertTrue("normal visibility changes must not report a remote error", errors.value == 0)
  }

  @Test fun failedAndCancelledRegistrationCannotRetireAReplacement() = withFixture { player, coordinator, server, errors ->
    val rejected = server.holdNextRegistration().apply { reply(403) }
    withContext(Dispatchers.Main) { coordinator.setEligible(true) }
    val failedConnection = server.awaitConnection()
    rejected.awaitRequest()
    withTimeout(15_000) { errors.first { it == 1 } }
    failedConnection.awaitClosed()
    withContext(Dispatchers.Main) { coordinator.setEligible(false); coordinator.setEligible(false) }

    val pending = server.holdNextRegistration()
    withContext(Dispatchers.Main) { coordinator.setEligible(true) }
    val cancelledConnection = server.awaitConnection()
    pending.awaitRequest()
    // Replace while the old authenticated capability request is held at the server.
    withContext(Dispatchers.Main) { coordinator.profileChanged() }
    val replacement = server.awaitConnection()
    confirmRemoteCommand(replacement, player)
    pending.reply(200)
    cancelledConnection.awaitClosed()
    confirmRemoteCommand(replacement, player)

    withContext(Dispatchers.Main) { coordinator.close() }
    replacement.awaitClosed()
    assertTrue("obsolete registration completion must not fail the current target", errors.value == 1)
  }

  private suspend fun confirmRemoteCommand(connection: RemoteLifecycleServer.Connection, player: NativePlayback) {
    val before = player.snapshot.value.muted
    connection.toggleMute()
    withTimeout(15_000) { player.snapshot.first { it.muted != before } }
  }

  private fun withFixture(test: suspend (NativePlayback, MediaPlaybackCoordinator, RemoteLifecycleServer, MutableStateFlow<Int>) -> Unit) = runBlocking {
    val context = InstrumentationRegistry.getInstrumentation().targetContext
    val directory = File(context.cacheDir, "remote-lifecycle-${UUID.randomUUID()}").apply { mkdirs() }
    val credentials = File(directory, "credentials").apply { mkdirs() }
    val storage = File(directory, "sdk").apply { mkdirs() }
    val credentialContext = object : ContextWrapper(context) {
      override fun getNoBackupFilesDir(): File = credentials
    }
    val sdk = JellypilotSdk(SdkConfig(storage.path, "Remote lifecycle fixture"), KeystoreCredentialStore(credentialContext), null)
    val player = (context.applicationContext as JellyPilotApplication).player
    val previousIntent = player.businessIntent
    val previousEligibility = player.admissionEligible
    val previousMuted = player.snapshot.value.muted
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    val errors = MutableStateFlow(0)
    val coordinator = withContext(Dispatchers.Main) {
      MediaPlaybackCoordinator(sdk, player, scope, {}, { errors.value += 1 }, {})
    }
    try {
      withTimeout(25_000) { player.ready.first { it } }
      assertTrue("the lifecycle fixture requires no active playback", player.snapshot.value.status == PlayerStatus.IDLE)
      RemoteLifecycleServer().use { server ->
        val candidate = withTimeout(15_000) {
          sdk.passwordLogin(Provider.JELLYFIN, server.baseUrl, "fixture-user", "fixture-password")
        }
        try { withTimeout(15_000) { sdk.activateCandidate(candidate, false) } }
        finally { candidate.destroy() }

        test(player, coordinator, server, errors)
        assertTrue("fixture should cover every SDK endpoint", server.unexpectedPaths.isEmpty())
      }
    } finally {
      withContext(Dispatchers.Main) { coordinator.close(); scope.cancel() }
      assertTrue(withTimeout(25_000) { player.stopAndWait() })
      withContext(Dispatchers.Main) {
        player.businessIntent = previousIntent
        player.setEligible(previousEligibility)
        player.mute(previousMuted)
      }
      withTimeout(10_000) { player.snapshot.first { it.muted == previousMuted } }
      withTimeout(15_000) { sdk.disconnect() }
      sdk.shutdown()
      sdk.destroy()
      directory.deleteRecursively()
    }
  }
}

/** Minimal synthetic Jellyfin HTTP/WebSocket peer; authenticated request headers are discarded. */
private class RemoteLifecycleServer : AutoCloseable {
  class Registration {
    private val request = CountDownLatch(1)
    private val response = CountDownLatch(1)
    private val status = AtomicInteger(200)
    fun reply(code: Int) { status.set(code); response.countDown() }
    fun awaitRequest() = assertTrue("SDK must reach capability registration", request.await(15, TimeUnit.SECONDS))
    fun status(): Int {
      request.countDown()
      check(response.await(15, TimeUnit.SECONDS)) { "fixture registration response was not released" }
      return status.get()
    }
  }

  class Connection(private val socket: Socket) {
    val closed = CountDownLatch(1)
    fun toggleMute() {
      val payload = "{\"MessageType\":\"GeneralCommand\",\"Data\":{\"Name\":\"ToggleMute\"}}".toByteArray(Charsets.UTF_8)
      synchronized(socket) {
        socket.getOutputStream().apply { write(0x81); write(payload.size); write(payload); flush() }
      }
    }
    fun awaitClosed() = assertTrue("revoked remote socket must close", closed.await(15, TimeUnit.SECONDS))
  }

  private val listener = ServerSocket(0, 8, InetAddress.getByName("127.0.0.1"))
  private val workers = Executors.newCachedThreadPool()
  private val sockets = ConcurrentLinkedQueue<Socket>()
  private val connections = LinkedBlockingQueue<Connection>()
  private val registrations = ConcurrentLinkedQueue<Registration>()
  val unexpectedPaths = ConcurrentLinkedQueue<String>()
  val baseUrl = "http://127.0.0.1:${listener.localPort}"

  init {
    workers.execute {
      while (!listener.isClosed) {
        val socket = try { listener.accept() } catch (_: java.io.IOException) { break }
        sockets += socket
        workers.execute { socket.use { runCatching { serve(it) } }; sockets.remove(socket) }
      }
    }
  }

  fun awaitConnection(): Connection = requireNotNull(connections.poll(15, TimeUnit.SECONDS)) { "remote WebSocket did not connect" }
  fun holdNextRegistration(): Registration = Registration().also(registrations::add)

  private fun serve(socket: Socket) {
    socket.soTimeout = 20_000
    val input = BufferedInputStream(socket.getInputStream())
    val request = readLine(input).split(' ')
    if (request.size < 2) return
    val path = URI(request[1]).path
    val headers = mutableMapOf<String, String>()
    while (true) {
      val line = readLine(input)
      if (line.isEmpty()) break
      val colon = line.indexOf(':')
      if (colon > 0) headers[line.substring(0, colon).lowercase()] = line.substring(colon + 1).trim()
    }
    var remaining = headers["content-length"]?.toInt() ?: 0
    while (remaining > 0) { check(input.read() >= 0); remaining-- }
    if (headers["upgrade"]?.equals("websocket", ignoreCase = true) == true) {
      val key = requireNotNull(headers["sec-websocket-key"])
      val digest = MessageDigest.getInstance("SHA-1").digest((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").toByteArray(Charsets.US_ASCII))
      val accept = Base64.getEncoder().encodeToString(digest)
      socket.getOutputStream().apply {
        write("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: $accept\r\n\r\n".toByteArray(Charsets.US_ASCII))
        flush()
      }
      val connection = Connection(socket)
      connections.put(connection)
      try { while (input.read() >= 0) { /* Consume SDK frames until terminal disconnect. */ } }
      finally { connection.closed.countDown() }
      return
    }
    val status = if (path == "/Sessions/Capabilities/Full") registrations.poll()?.status() ?: 200 else 200
    val body = when (path) {
      "/Users/AuthenticateByName" -> JSONObject().put("User", JSONObject().put("Id", USER).put("Name", "Fixture user"))
        .put("AccessToken", "synthetic-token").put("ServerId", "fixture-server").toString()
      "/System/Info/Public" -> "{\"ServerName\":\"Fixture\",\"Version\":\"10.10.0\",\"Id\":\"fixture-server\"}"
      "/Sessions/Capabilities/Full" -> "{}"
      "/Sessions" -> "[]" // Session-list visibility is informational, after capability registration.
      else -> { unexpectedPaths += path; "{}" }
    }.toByteArray(Charsets.UTF_8)
    socket.getOutputStream().apply {
      write("HTTP/1.1 $status ${if (status == 200) "OK" else "Forbidden"}\r\nContent-Type: application/json\r\nContent-Length: ${body.size}\r\nConnection: close\r\n\r\n".toByteArray(Charsets.US_ASCII))
      write(body)
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

  override fun close() { listener.close(); sockets.forEach { it.close() }; workers.shutdownNow() }

  private companion object { const val USER = "00000000000000000000000000000001" }
}
