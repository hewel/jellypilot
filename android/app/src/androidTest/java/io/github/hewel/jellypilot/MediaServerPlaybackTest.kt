package io.github.hewel.jellypilot

import android.content.ContextWrapper
import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.hewel.jellypilot.bridge.KeystoreCredentialStore
import io.github.hewel.jellypilot.ffi.JellypilotSdk
import io.github.hewel.jellypilot.ffi.PlaybackSelection
import io.github.hewel.jellypilot.ffi.Provider
import io.github.hewel.jellypilot.ffi.SdkConfig
import io.github.hewel.jellypilot.ui.PlaybackUi
import java.io.BufferedInputStream
import java.io.File
import java.net.ServerSocket
import java.net.Socket
import java.net.URI
import java.util.UUID
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import kotlin.math.abs
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Real HTTP -> Rust/UniFFI -> coordinator -> libmpv -> server report/resume boundary. */
@RunWith(AndroidJUnit4::class)
class MediaServerPlaybackTest {
  @Test fun originalPlaybackReportsConfirmedTracksAndResumesFromServerPosition() = runBlocking {
    val instrumentation = InstrumentationRegistry.getInstrumentation()
    val context = instrumentation.targetContext
    val directory = File(context.cacheDir, "server-playback-${UUID.randomUUID()}").apply { mkdirs() }
    val credentials = File(directory, "credentials").apply { mkdirs() }
    val storage = File(directory, "sdk").apply { mkdirs() }
    val credentialContext = object : ContextWrapper(context) {
      override fun getNoBackupFilesDir(): File = credentials
    }
    val media = TestMedia.stage(instrumentation.context, TestMedia.SAMPLE_ASSET, directory)
    val sdk = JellypilotSdk(SdkConfig(storage.path, "JellyPilot fixture"), KeystoreCredentialStore(credentialContext), null)
    val player = NativePlayback(context)
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    val playback = MutableStateFlow<PlaybackUi?>(null)
    val errors = ConcurrentLinkedQueue<String>()
    val coordinator = withContext(Dispatchers.Main) {
      MediaPlaybackCoordinator(sdk, player, scope, { playback.value = it }, errors::add, {}).also {
        // Admission is enabled before authentication; this fixture does not open a remote target.
        it.setEligible(true)
      }
    }
    val scenario = ActivityScenario.launch(PlaybackFixtureActivity::class.java)
    try {
      val surfaceReady = CountDownLatch(1)
      scenario.onActivity { activity ->
        activity.setContentView(SurfaceView(activity).apply {
          holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
              player.attach(holder.surface)
              surfaceReady.countDown()
            }
            override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) = Unit
            override fun surfaceDestroyed(holder: SurfaceHolder) { player.detach() }
          })
        })
      }
      assertTrue("foreground native surface must be available", surfaceReady.await(15, TimeUnit.SECONDS))
      withTimeout(25_000) { player.ready.first { it } }
      PlaybackFixture(media).use { server ->
        sdk.setPreferOriginalAudio(false)
        val candidate = withTimeout(30_000) {
          sdk.passwordLogin(Provider.JELLYFIN, server.baseUrl, "fixture-user", "fixture-password")
        }
        try { withTimeout(30_000) { sdk.activateCandidate(candidate, true) } }
        finally { candidate.destroy() }
        assertEquals(1, sdk.savedProfiles().profiles.size)
        val token = sdk.newOperationToken()
        try {
          val home = withTimeout(30_000) { sdk.videoHome(token) }
          assertEquals(PlaybackFixture.ITEM, home.continueWatching.single().id)
          val detail = withTimeout(30_000) { sdk.itemDetail(token, PlaybackFixture.ITEM) }
          assertTrue(detail.canPlay)
          assertEquals(PlaybackFixture.ITEM, detail.id)

          withContext(Dispatchers.Main) {
            coordinator.play(PlaybackFixture.ITEM, true, PlaybackSelection(PlaybackFixture.SOURCE, 2, -1))
          }
          val first = withTimeout(30_000) { player.snapshot.first { it.isPlaying && it.positionSeconds > 0.2 } }
          val started = server.awaitReport("/Sessions/Playing")
          assertReportIdentity(started, "fixture-session-1")
          assertEquals(2, started.getInt("AudioStreamIndex"))
          assertEquals(-1, started.getInt("SubtitleStreamIndex"))
          assertFalse(started.getBoolean("IsPaused"))

          withContext(Dispatchers.Main) { coordinator.pausePlayback() }
          withTimeout(10_000) { player.snapshot.first { it.generation == first.generation && it.paused && !it.playWhenReady } }
          withContext(Dispatchers.Main) { coordinator.seek(7.0) }
          withTimeout(10_000) { player.snapshot.first { it.generation == first.generation && abs(it.positionSeconds - 7.0) < 0.4 && it.paused } }
          // An explicit paused observation exercises immediate reporting, independent of SDK cadence.
          withContext(Dispatchers.Main) { coordinator.pausePlayback() }
          val progress = server.awaitReport("/Sessions/Playing/Progress") {
            it.optBoolean("IsPaused") && it.optLong("PositionTicks") >= 65_000_000
          }
          assertReportIdentity(progress, "fixture-session-1")
          assertEquals(2, progress.getInt("AudioStreamIndex"))
          assertEquals(-1, progress.getInt("SubtitleStreamIndex"))
          val recovery = requireNotNull(sdk.localPlaybackRecovery(token))
          assertEquals(PlaybackFixture.ITEM, recovery.itemId)
          assertEquals(7.0, recovery.positionSeconds, 0.4)

          withContext(Dispatchers.Main) { coordinator.stop() }
          val stopped = server.awaitReport("/Sessions/Playing/Stopped")
          assertReportIdentity(stopped, "fixture-session-1")
          assertTrue(stopped.getLong("PositionTicks") >= 65_000_000)
          withTimeout(10_000) { playback.first { it == null } }
          assertNull("explicit stop clears process recovery", sdk.localPlaybackRecovery(token))

          val resumable = sdk.itemDetail(token, PlaybackFixture.ITEM)
          assertTrue("server stop report must become server Resume", resumable.canResume)
          val savedPosition = stopped.getLong("PositionTicks") / 10_000_000.0
          assertEquals(savedPosition, requireNotNull(resumable.resumePositionSeconds), 0.01)
          withContext(Dispatchers.Main) { coordinator.play(PlaybackFixture.ITEM) }
          withTimeout(30_000) {
            player.snapshot.first { it.generation != first.generation && it.isPlaying && it.positionSeconds >= savedPosition - 0.4 }
          }
          val resumedStart = server.awaitReport("/Sessions/Playing", 2)
          assertReportIdentity(resumedStart, "fixture-session-2")
          assertTrue(resumedStart.getLong("PositionTicks") >= stopped.getLong("PositionTicks") - 4_000_000)
          assertEquals("remembered provider audio is applied before resumed playback", 2, resumedStart.getInt("AudioStreamIndex"))
          assertEquals(-1, resumedStart.getInt("SubtitleStreamIndex"))
          val preparations = server.preparations()
          assertEquals(2, preparations.size)
          // Jellyfin's original stream starts through native seek; only Emby sends server start ticks.
          assertTrue(preparations.all { it.isNull("StartTimeTicks") })
          assertTrue(preparations.all { !it.getBoolean("EnableTranscoding") })
          assertTrue("the original selected source must reach native HTTP", server.originalSourceRequests.get() >= 2)

          withContext(Dispatchers.Main) { coordinator.stop() }
          server.awaitReport("/Sessions/Playing/Stopped", 2)
          withTimeout(10_000) { playback.first { it == null } }
          assertTrue("coordinator must not report an error", errors.isEmpty())
          assertTrue("the fixture must cover every requested endpoint", server.unexpectedPaths.isEmpty())
        } finally { token.cancel(); token.destroy() }
      }
    } finally {
      withTimeout(30_000) { coordinator.beforeHandoff() }
      withContext(Dispatchers.Main) { coordinator.close(); scope.cancel() }
      withTimeout(25_000) { player.stopAndWait() }
      scenario.close()
      player.close()
      withTimeout(25_000) { player.stopAndWait() }
      withTimeout(30_000) { sdk.disconnect() }
      sdk.shutdown()
      sdk.destroy()
      directory.deleteRecursively()
    }
  }

  private fun assertReportIdentity(body: JSONObject, session: String) {
    assertEquals(PlaybackFixture.ITEM, body.getString("ItemId"))
    assertEquals(PlaybackFixture.SOURCE, body.getString("MediaSourceId"))
    assertEquals(session, body.getString("PlaySessionId"))
  }
}

/** Deliberately records synthetic payloads only; request headers and authenticated URLs are discarded. */
private class PlaybackFixture(private val media: File) : AutoCloseable {
  private class Report(val path: String, val body: JSONObject)
  private val socket = ServerSocket(0, 8, java.net.InetAddress.getByName("127.0.0.1"))
  private val workers = Executors.newCachedThreadPool()
  private val monitor = Object()
  private val reports = mutableListOf<Report>()
  private val playbackInfo = mutableListOf<JSONObject>()
  private val resumeTicks = AtomicLong()
  private val sessions = AtomicInteger()
  val originalSourceRequests = AtomicInteger()
  val unexpectedPaths = ConcurrentLinkedQueue<String>()
  val baseUrl = "http://127.0.0.1:${socket.localPort}"

  init {
    workers.execute {
      while (!socket.isClosed) {
        val client = try { socket.accept() } catch (_: java.io.IOException) { break }
        workers.execute { client.use { runCatching { serve(it) } } }
      }
    }
  }

  fun preparations(): List<JSONObject> = synchronized(monitor) { playbackInfo.toList() }

  fun awaitReport(path: String, ordinal: Int = 1, predicate: (JSONObject) -> Boolean = { true }): JSONObject {
    val deadline = System.nanoTime() + 30_000_000_000L
    synchronized(monitor) {
      while (true) {
        reports.filter { it.path == path && predicate(it.body) }.getOrNull(ordinal - 1)?.let { return it.body }
        val remaining = deadline - System.nanoTime()
        check(remaining > 0) { "Timed out waiting for synthetic playback report $path" }
        monitor.wait((remaining / 1_000_000).coerceAtLeast(1))
      }
    }
  }

  private fun serve(client: Socket) {
    client.soTimeout = 15_000
    val input = BufferedInputStream(client.getInputStream())
    val request = readLine(input).split(' ')
    if (request.size < 2) return
    val uri = URI(request[1])
    val headers = mutableMapOf<String, String>()
    while (true) {
      val line = readLine(input)
      if (line.isEmpty()) break
      val separator = line.indexOf(':')
      if (separator > 0) headers[line.substring(0, separator).lowercase()] = line.substring(separator + 1).trim()
    }
    val bytes = ByteArray(headers["content-length"]?.toInt() ?: 0)
    var offset = 0
    while (offset < bytes.size) {
      val count = input.read(bytes, offset, bytes.size - offset)
      check(count > 0)
      offset += count
    }
    val path = uri.path
    when {
      path == "/Users/AuthenticateByName" -> json(client, JSONObject()
        .put("User", JSONObject().put("Id", USER).put("Name", "Fixture user"))
        .put("AccessToken", "synthetic-token").put("ServerId", "fixture-server"))
      path == "/System/Info/Public" -> json(client, JSONObject().put("ServerName", "Fixture").put("Version", "10.10.0").put("Id", "fixture-server"))
      path == "/UserItems/Resume" -> json(client, JSONObject().put("Items", JSONArray().put(item())).put("TotalRecordCount", 1))
      path == "/Shows/NextUp" -> json(client, JSONObject().put("Items", JSONArray()).put("TotalRecordCount", 0))
      path == "/Items/$ITEM" -> json(client, item())
      path == "/Items/$ITEM/PlaybackInfo" -> {
        val body = JSONObject(String(bytes, Charsets.UTF_8))
        synchronized(monitor) { playbackInfo += body }
        json(client, JSONObject().put("PlaySessionId", "fixture-session-${sessions.incrementAndGet()}").put("MediaSources", JSONArray().put(source())))
      }
      path == "/Videos/$ITEM/stream.mkv" -> {
        val query = uri.rawQuery.orEmpty().split('&')
        if ("Static=true" in query && "MediaSourceId=$SOURCE" in query) originalSourceRequests.incrementAndGet()
        val start = headers["range"]?.substringAfter("bytes=")?.substringBefore('-')?.toLongOrNull() ?: 0
        val content = media.readBytes()
        check(start in 0 until content.size.toLong())
        val partial = headers.containsKey("range")
        val header = buildString {
          append("HTTP/1.1 ${if (partial) "206 Partial Content" else "200 OK"}\r\n")
          append("Content-Type: video/x-matroska\r\nAccept-Ranges: bytes\r\n")
          append("Content-Length: ${content.size - start}\r\nConnection: close\r\n")
          if (partial) append("Content-Range: bytes $start-${content.lastIndex}/${content.size}\r\n")
          append("\r\n")
        }
        client.getOutputStream().apply {
          write(header.toByteArray(Charsets.US_ASCII))
          if (request[0] != "HEAD") write(content, start.toInt(), content.size - start.toInt())
          flush()
        }
      }
      path in listOf("/Sessions/Playing", "/Sessions/Playing/Progress", "/Sessions/Playing/Stopped") -> {
        val body = JSONObject(String(bytes, Charsets.UTF_8))
        if (body.has("PositionTicks")) resumeTicks.set(body.getLong("PositionTicks"))
        synchronized(monitor) { reports += Report(path, body); monitor.notifyAll() }
        json(client, JSONObject())
      }
      else -> { unexpectedPaths += path; json(client, JSONObject(), "404 Not Found") }
    }
  }

  private fun streams() = JSONArray()
    .put(JSONObject().put("Type", "Video").put("Index", 0).put("Codec", "h264"))
    .put(JSONObject().put("Type", "Audio").put("Index", 1).put("Language", "eng").put("Codec", "aac").put("IsDefault", true))
    .put(JSONObject().put("Type", "Audio").put("Index", 2).put("Language", "jpn").put("Codec", "aac"))
    .put(JSONObject().put("Type", "Subtitle").put("Index", 3).put("Language", "eng").put("Codec", "srt").put("IsExternal", false))

  private fun source() = JSONObject().put("Id", SOURCE).put("Protocol", "Http").put("Container", "mkv")
    .put("SupportsDirectPlay", true).put("SupportsDirectStream", true).put("RunTimeTicks", 200_000_000)
    .put("DefaultAudioStreamIndex", 1).put("MediaStreams", streams())

  private fun item() = JSONObject().put("Id", ITEM).put("Name", "Fixture movie").put("Type", "Movie")
    .put("RunTimeTicks", 200_000_000).put("OriginalLanguage", "en").put("MediaSources", JSONArray().put(source()))
    .put("MediaStreams", streams()).put("UserData", JSONObject().put("Key", "fixture-item")
      .put("PlaybackPositionTicks", resumeTicks.get()).put("Played", false).put("IsFavorite", false))

  private fun json(client: Socket, body: JSONObject, status: String = "200 OK") {
    val bytes = body.toString().toByteArray(Charsets.UTF_8)
    client.getOutputStream().apply {
      write("HTTP/1.1 $status\r\nContent-Type: application/json\r\nContent-Length: ${bytes.size}\r\nConnection: close\r\n\r\n".toByteArray(Charsets.US_ASCII))
      write(bytes)
      flush()
    }
  }

  private fun readLine(input: BufferedInputStream): String {
    val line = StringBuilder()
    while (true) {
      val value = input.read()
      if (value < 0 || value == 10) return line.toString().trimEnd('\r')
      check(line.length < 16_384)
      line.append(value.toChar())
    }
  }

  override fun close() { socket.close(); workers.shutdownNow() }

  companion object {
    const val USER = "00000000000000000000000000000001"
    const val ITEM = "00000000000000000000000000000010"
    const val SOURCE = "fixture-source"
  }
}
