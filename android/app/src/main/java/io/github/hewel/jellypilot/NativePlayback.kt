package io.github.hewel.jellypilot

import android.content.BroadcastReceiver
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import androidx.core.content.ContextCompat
import android.content.Context
import android.net.Uri
import android.os.CancellationSignal
import android.os.Handler
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.view.Surface
import androidx.media3.session.MediaSession
import io.github.hewel.jellypilot.player.*
import java.io.File
import java.security.KeyStore
import java.util.Base64
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference
import javax.net.ssl.TrustManagerFactory
import javax.net.ssl.X509TrustManager
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withTimeout

/** Gate-1 engine host. It deliberately does not create media-server playback/reporting sessions. */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
internal class NativePlayback(context: Context) : AutoCloseable, PlayerIntentHandler {
  private val application = context.applicationContext
  private val executor = Executors.newSingleThreadExecutor { action -> Thread(action, "jellypilot-media-resources") }
  private val main = Handler(Looper.getMainLooper())
  private val submissionLock = Any()
  private val closed = AtomicBoolean(false)
  private val admitted = AtomicBoolean(false)
  private val handoff = AtomicBoolean(false)
  private val policyLock = Any()
  private val audioManager = application.getSystemService(AudioManager::class.java)
  private var hasAudioFocus = false
  private val audioFocusRequest = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
    .setAudioAttributes(AudioAttributes.Builder()
      .setUsage(AudioAttributes.USAGE_MEDIA)
      .setContentType(AudioAttributes.CONTENT_TYPE_MOVIE)
      .build())
    .setAcceptsDelayedFocusGain(false)
    .setWillPauseWhenDucked(true)
    .setOnAudioFocusChangeListener({ change ->
      // A regained focus grant is not a new user play request.
      if (change != AudioManager.AUDIOFOCUS_GAIN) pause()
    }, main)
    .build()
  private val noisyReceiver = object : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
      if (intent.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY) pause()
    }
  }
  private val commandEpoch = AtomicLong()
  private val opening = AtomicReference<CancellationSignal?>()
  private val surfaceLock = Any()
  private var surfaceEpoch = 0L
  @Volatile private var host: PlayerHost? = null
  private var mediaSession: MediaSession? = null
  private var media3: MpvMedia3Player? = null
  private val descriptors = mutableMapOf<Long, List<ParcelFileDescriptor>>()
  private data class StopWaiter(val generations: MutableSet<Long>, val completion: CompletableDeferred<Boolean>)
  private val stopWaiters = mutableListOf<StopWaiter>()
  private var pendingLoad: Long? = null
  private val mutableSnapshot = MutableStateFlow(PlayerSnapshot(admissionEligible = false))
  val snapshot = mutableSnapshot.asStateFlow()
  private val mutableReady = MutableStateFlow(false)
  val ready = mutableReady.asStateFlow()
  private val mutableError = MutableStateFlow<String?>(null)
  val error = mutableError.asStateFlow()

  private fun execute(action: () -> Unit): Boolean = synchronized(submissionLock) {
    if (closed.get()) false else {
      executor.execute(action)
      true
    }
  }
  private fun updateAdmissionLocked() {
    host?.setAdmissionEligible(admitted.get() && !handoff.get() && !closed.get())
  }

  private fun releaseAudioFocus() = synchronized(policyLock) {
    if (hasAudioFocus) {
      hasAudioFocus = false
      updateAdmissionLocked()
      audioManager.abandonAudioFocusRequest(audioFocusRequest)
    }
  }

  private fun withAudioFocus(expectedEpoch: Long = commandEpoch.get(), action: () -> Unit): Boolean = synchronized(policyLock) {
    if (commandEpoch.get() != expectedEpoch || closed.get() || !admitted.get() || handoff.get()) return@synchronized false
    if (!hasAudioFocus) {
      hasAudioFocus = audioManager.requestAudioFocus(audioFocusRequest) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
    }
    updateAdmissionLocked()
    if (!hasAudioFocus) return@synchronized false
    // Submission and focus loss use the same lock; a stale grant cannot undo a revocation.
    action()
    true
  }

  private fun retire(generation: Long) {
    descriptors.remove(generation)?.forEach { it.close() }
    if (pendingLoad == generation) pendingLoad = null
    stopWaiters.removeAll { waiter ->
      waiter.generations.remove(generation)
      if (waiter.generations.isEmpty()) { waiter.completion.complete(true); true } else false
    }
    if (descriptors.isEmpty()) releaseAudioFocus()
  }

  private val listener = object : PlayerHost.Listener {
    override fun onSnapshot(snapshot: PlayerSnapshot) {
      if (!closed.get()) mutableSnapshot.value = snapshot
      if (snapshot.status == PlayerStatus.ENDED) releaseAudioFocus()
    }
    override fun onEvent(event: PlayerEvent) {
      execute {
        when (event) {
          is PlayerEvent.FileLoaded -> if (pendingLoad == event.generation) pendingLoad = null
          is PlayerEvent.PlaybackStopped -> retire(event.generation)
          is PlayerEvent.LoadRejected -> retire(event.generation)
          is PlayerEvent.CommandRejected -> {
            mutableError.value = "${event.command}: ${event.reason}"
          }
          else -> Unit
        }
      }
    }
  }

  init {
    ContextCompat.registerReceiver(application, noisyReceiver,
      IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY), ContextCompat.RECEIVER_NOT_EXPORTED)
    execute {
      try {
        val directory = File(application.cacheDir, "mpv").apply { mkdirs() }
        val created = MpvPlayerHost(application, PlayerHostConfig(directory.path, exportTrustRoots(directory).path))
        created.setAdmissionEligible(false)
        created.addListener(listener)
        synchronized(surfaceLock) { host = created }
        synchronized(policyLock) { updateAdmissionLocked() }
        if (closed.get()) return@execute
        mutableReady.value = true
        main.post {
          if (!closed.get()) {
            val adapter = MpvMedia3Player(created, Looper.getMainLooper(), this)
            media3 = adapter
            mediaSession = MediaSession.Builder(application, adapter).build()
          }
        }
      } catch (_: Exception) {
        mutableError.value = application.getString(R.string.player_initialization_failed)
      } catch (_: LinkageError) {
        mutableError.value = application.getString(R.string.player_initialization_failed)
      }
    }
  }

  fun setEligible(eligible: Boolean) = synchronized(policyLock) {
    admitted.set(eligible)
    if (!eligible) pause()
    updateAdmissionLocked()
  }

  fun setHandoffBlocked(blocked: Boolean) = synchronized(policyLock) {
    handoff.set(blocked)
    if (blocked) pause()
    updateAdmissionLocked()
  }

  fun loadUrl(url: String, subtitle: Uri?) = load(MediaLocator.Remote(url), null, subtitle)
  fun loadFile(uri: Uri, subtitle: Uri?) = load(null, uri, subtitle)

  private fun load(remote: MediaLocator?, uri: Uri?, subtitle: Uri?) {
    val epoch = commandEpoch.incrementAndGet()
    if (!ready.value || !admitted.get() || handoff.get() || closed.get()) {
      mutableError.value = application.getString(R.string.player_paused_background)
      return
    }
    execute { if (closed.get() || commandEpoch.get() != epoch) return@execute
    if (pendingLoad != null) {
      mutableError.value = application.getString(R.string.player_wait_for_load)
      return@execute
    }
    val owned = mutableListOf<ParcelFileDescriptor>()
    val cancellation = CancellationSignal()
    opening.set(cancellation)
    try {
      fun locator(value: Uri): MediaLocator {
        val descriptor = application.contentResolver.openFileDescriptor(value, "r", cancellation)
          ?: error("Provider returned no descriptor")
        owned += descriptor
        return MediaLocator.BorrowedFd(descriptor.fd)
      }
      val source = remote ?: locator(requireNotNull(uri))
      val subtitles = subtitle?.let { listOf(ExternalSubtitle(locator(it))) } ?: emptyList()
      if (closed.get() || commandEpoch.get() != epoch || !admitted.get() || handoff.get()) {
        owned.forEach { it.close() }
        return@execute
      }
      val current = requireNotNull(host)
      descriptors[epoch] = owned.toList()
      pendingLoad = epoch
      mutableError.value = null
      if (!withAudioFocus(epoch) { current.load(MediaLoad(source, generation = epoch, externalSubtitles = subtitles)) }) {
        retire(epoch)
        if (commandEpoch.get() == epoch) mutableError.value = application.getString(R.string.player_audio_focus_denied)
      }
    } catch (_: Exception) {
      descriptors.remove(epoch)
      owned.forEach { runCatching { it.close() } }
      if (pendingLoad == epoch) pendingLoad = null
      if (commandEpoch.get() == epoch && !closed.get()) mutableError.value = application.getString(R.string.player_open_failed)
      if (descriptors.isEmpty()) releaseAudioFocus()
    } finally {
      opening.compareAndSet(cancellation, null)
    } }
  }

  fun play(): Boolean {
    val current = host ?: return false
    if (closed.get() || !admitted.get() || handoff.get()) {
      mutableError.value = application.getString(R.string.player_paused_background)
      return false
    }
    val accepted = withAudioFocus { current.play() }
    if (!accepted) mutableError.value = application.getString(R.string.player_audio_focus_denied)
    return accepted
  }
  fun pause() {
    synchronized(policyLock) {
      commandEpoch.incrementAndGet()
      opening.get()?.cancel()
      releaseAudioFocus()
      host?.pause()
    }
  }
  fun seek(seconds: Double) { host?.seekTo(seconds) }
  fun volume(percent: Int) { host?.setVolume(percent) }
  fun select(kind: TrackKind, id: Int) { host?.selectTrack(kind, id) }

  fun stop() {
    synchronized(policyLock) {
      commandEpoch.incrementAndGet()
      opening.get()?.cancel()
      releaseAudioFocus()
      host?.stop()
    }
  }

  suspend fun stopAndWait(): Boolean {
    synchronized(policyLock) {
      commandEpoch.incrementAndGet()
      opening.get()?.cancel()
      releaseAudioFocus()
    }
    val receipt = CompletableDeferred<Boolean>()
    val queued = execute {
      val current = host
      val generations = descriptors.keys.toMutableSet()
      pendingLoad?.let(generations::add)
      current?.snapshot?.takeIf { it.status != PlayerStatus.IDLE }?.let { generations.add(it.generation) }
      if (generations.isEmpty()) receipt.complete(true)
      else {
        stopWaiters += StopWaiter(generations, receipt)
        current?.stop()
      }
    }
    if (!queued) return false
    return try { withTimeout(15000) { receipt.await() } }
    catch (_: TimeoutCancellationException) { false }
    finally { execute { stopWaiters.removeAll { it.completion === receipt } } }
  }

  fun attach(surface: Surface) {
    val epoch = synchronized(surfaceLock) { ++surfaceEpoch }
    execute { synchronized(surfaceLock) {
      if (!closed.get() && surfaceEpoch == epoch && surface.isValid) host?.attachSurface(surface)
    } }
  }

  fun detach() {
    synchronized(surfaceLock) {
      ++surfaceEpoch
      // Ordering under the same lock prevents an old queued attach after SurfaceView destruction.
      host?.detachSurface()
    }
  }

  override fun onPlayerIntent(intent: PlayerIntent): Boolean {
    when (intent) {
      PlayerIntent.Play -> return play()
      PlayerIntent.Pause -> pause()
      PlayerIntent.Stop -> stop()
      PlayerIntent.Released -> if (!closed.get()) stop()
      is PlayerIntent.SeekTo -> seek(intent.positionMs / 1000.0)
      is PlayerIntent.SelectTrack -> select(intent.kind, when (intent.formatId) {
        PlayerIntent.FORMAT_ID_DISABLED -> PlayerHost.TRACK_ID_NONE
        PlayerIntent.FORMAT_ID_AUTO -> PlayerHost.TRACK_ID_AUTO
        else -> intent.formatId.toIntOrNull() ?: return false
      })
      is PlayerIntent.SetVolume -> volume((intent.volume * 100).toInt())
      is PlayerIntent.SetSpeed -> host?.setSpeed(intent.speed.toDouble())
    }
    return true
  }

  override fun close() {
    if (!synchronized(submissionLock) { closed.compareAndSet(false, true) }) return
    admitted.set(false)
    releaseAudioFocus()
    application.unregisterReceiver(noisyReceiver)
    mutableReady.value = false
    opening.get()?.cancel()
    host?.setAdmissionEligible(false)
    detach()
    main.post { mediaSession?.release(); media3?.release(); mediaSession = null; media3 = null }
    executor.execute {
      host?.removeListener(listener)
      host?.release()
      host = null
      descriptors.values.forEach { group -> group.forEach { runCatching { it.close() } } }
      descriptors.clear()
      stopWaiters.forEach { it.completion.complete(false) }
      stopWaiters.clear()
      executor.shutdown()
    }
  }
}

private fun exportTrustRoots(directory: File): File {
  val factory = TrustManagerFactory.getInstance(TrustManagerFactory.getDefaultAlgorithm())
  factory.init(null as KeyStore?)
  val certificates = factory.trustManagers.filterIsInstance<X509TrustManager>().flatMap { it.acceptedIssuers.toList() }
  check(certificates.isNotEmpty()) { "No platform trust roots" }
  val output = File(directory, "platform-trust.pem")
  val encoder = Base64.getMimeEncoder(64, byteArrayOf('\n'.code.toByte()))
  output.outputStream().buffered().use { stream ->
    for (certificate in certificates) {
      stream.write("-----BEGIN CERTIFICATE-----\n".toByteArray())
      stream.write(encoder.encode(certificate.encoded))
      stream.write("\n-----END CERTIFICATE-----\n".toByteArray())
    }
  }
  return output
}
