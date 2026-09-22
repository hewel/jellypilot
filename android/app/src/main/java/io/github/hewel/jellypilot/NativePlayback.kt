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
import java.util.UUID
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference
import javax.net.ssl.TrustManagerFactory
import javax.net.ssl.X509TrustManager
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.withTimeout

internal data class GestureSeek(val token: Long, val positionSeconds: Double)

/** Android resource owner. Media-server policy and reporting belong to the shared SDK. */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
internal class NativePlayback(context: Context) : AutoCloseable, PlayerIntentHandler {
  private val application = context.applicationContext
  private val executor = Executors.newSingleThreadExecutor { action -> Thread(action, "jellypilot-media-resources") }
  private val main = Handler(Looper.getMainLooper())
  private val submissionLock = Any()
  private val closed = AtomicBoolean(false)
  private val released = CompletableDeferred<Unit>()
  private val admitted = AtomicBoolean(false)
  private val handoff = AtomicBoolean(false)
  private val policyLock = Any()
  // User/admission intent is synchronous; delayed mpv snapshots must not undo a newer pause.
  private var requestedPlaying = false
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
  private val gestureSequence = AtomicLong()
  private data class SeekPreview(val token: Long, val generation: Long, val epoch: Long,
    val origin: Double, val resume: Boolean)
  private val seekPreview = MutableStateFlow<SeekPreview?>(null)
  private val mutableGestureSeeking = MutableStateFlow(false)
  val gestureSeeking = mutableGestureSeeking.asStateFlow()
  private fun setSeekPreview(value: SeekPreview?) { seekPreview.value = value; mutableGestureSeeking.value = value != null }
  private var speedGesture: Long? = null
  val gestureSeekActive: Boolean get() = seekPreview.value != null
  private val opening = AtomicReference<CancellationSignal?>()
  private val surfaceLock = Any()
  private var surfaceEpoch = 0L
  @Volatile private var host: MpvPlayerHost? = null
  private var mediaSession: MediaSession? = null
  // A replacement owner can initialize before the previous main-thread session has released.
  private val mediaSessionId = "jellypilot-${UUID.randomUUID()}"
  private var media3: MpvMedia3Player? = null
  private val descriptors = mutableMapOf<Long, List<ParcelFileDescriptor>>()
  private data class StopWaiter(val generations: MutableSet<Long>, val completion: CompletableDeferred<Boolean>)
  private val stopWaiters = mutableListOf<StopWaiter>()
  private val loadWaiters = mutableMapOf<Long, CompletableDeferred<Long>>()
  private var pendingLoad: Long? = null
  private val mutableSnapshot = MutableStateFlow(PlayerSnapshot(admissionEligible = false))
  val snapshot = mutableSnapshot.asStateFlow()
  private val mutableReady = MutableStateFlow(false)
  val ready = mutableReady.asStateFlow()
  private val mutableError = MutableStateFlow<String?>(null)
  val error = mutableError.asStateFlow()
  private val businessEvents = Channel<PlayerEvent>(Channel.UNLIMITED)
  /** Lossless, single-consumer lifecycle events; native log traffic never enters this channel. */
  val events = businessEvents.receiveAsFlow()
  @Volatile var businessIntent: ((PlayerIntent) -> Boolean)? = null
  val admissionEligible: Boolean get() = admitted.get() && !handoff.get() && !closed.get()
  fun message(@androidx.annotation.StringRes id: Int): String = application.localizedString(id)

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
    synchronized(policyLock) {
      if (seekPreview.value?.generation == generation) {
        setSeekPreview(null)
        commandEpoch.incrementAndGet()
      }
    }
    loadWaiters.remove(generation)?.completeExceptionally(IllegalStateException("Native load did not complete"))
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
      if (event !is PlayerEvent.LogMessage && !closed.get()) businessEvents.trySend(event)
      execute {
        when (event) {
          is PlayerEvent.FileLoaded -> {
            if (pendingLoad == event.generation) pendingLoad = null
            loadWaiters.remove(event.generation)?.complete(event.generation)
          }
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
        // Visibility pauses explicitly; account admission must not pause the
        // current session before protected credential deletion succeeds.
        val created = MpvPlayerHost(application, PlayerHostConfig(
          directory.path, exportTrustRoots(directory).path, pauseWhenIneligible = false,
        ))
        created.setAdmissionEligible(false)
        created.addListener(listener)
        synchronized(surfaceLock) { host = created }
        synchronized(policyLock) { updateAdmissionLocked() }
        if (closed.get()) return@execute
        main.post {
          if (!closed.get()) {
            try {
              val adapter = MpvMedia3Player(created, Looper.getMainLooper(), this)
              media3 = adapter
              mediaSession = MediaSession.Builder(application, adapter).setId(mediaSessionId).build()
              synchronized(submissionLock) { if (!closed.get()) mutableReady.value = true }
            } catch (_: Exception) {
              // This callback runs after the native initializer's try/catch has returned.
              mutableError.value = application.localizedString(R.string.player_initialization_failed)
              close()
            }
          }
        }
      } catch (_: Exception) {
        mutableError.value = application.localizedString(R.string.player_initialization_failed)
      } catch (_: LinkageError) {
        mutableError.value = application.localizedString(R.string.player_initialization_failed)
      }
    }
  }

  fun setEligible(eligible: Boolean) = synchronized(policyLock) {
    admitted.set(eligible)
    if (!eligible) pause()
    updateAdmissionLocked()
  }

  fun setHandoffBlocked(blocked: Boolean) = synchronized(policyLock) {
    if (blocked) cancelPlaybackGestures()
    handoff.set(blocked)
    updateAdmissionLocked()
  }

  fun loadUrl(url: String, subtitle: Uri?) = load(MediaLocator.Remote(url), null, subtitle)
  fun loadFile(uri: Uri, subtitle: Uri?) = load(null, uri, subtitle)

  /** Submits a prepared SDK plan without moving Surface or video ownership across FFI. */
  suspend fun loadMedia(
    request: MediaLoad,
    stillCurrent: () -> Boolean = { true },
    bindGeneration: (Long) -> Unit = {},
  ): Long {
    synchronized(policyLock) { requestedPlaying = false; cancelPlaybackGestures() }
    val epoch = commandEpoch.incrementAndGet()
    val submitted = CompletableDeferred<Long>()
    if (!ready.value || !admissionEligible || !stillCurrent()) error("Playback is not currently eligible")
    bindGeneration(epoch)
    val queued = execute {
      if (closed.get() || commandEpoch.get() != epoch || !admissionEligible || !stillCurrent()) {
        submitted.completeExceptionally(IllegalStateException("Playback admission changed"))
        return@execute
      }
      if (pendingLoad != null) {
        submitted.completeExceptionally(IllegalStateException("Previous media load has not settled"))
        return@execute
      }
      val current = host
      if (current == null) {
        submitted.completeExceptionally(IllegalStateException("Player is unavailable"))
        return@execute
      }
      descriptors[epoch] = emptyList()
      pendingLoad = epoch
      loadWaiters[epoch] = submitted
      mutableError.value = null
      try {
        if (withAudioFocus(epoch) {
          check(stillCurrent()) { "Playback admission changed" }
          requestedPlaying = !request.startPaused
          current.load(request.copy(generation = epoch))
        }) {
          // The SDK account admission remains held until FileLoaded, not merely load submission.
        } else {
          retire(epoch)
          mutableError.value = application.localizedString(R.string.player_audio_focus_denied)
          submitted.completeExceptionally(IllegalStateException("Audio focus was denied"))
        }
      } catch (_: Exception) {
        retire(epoch)
        submitted.completeExceptionally(IllegalStateException("Player rejected the media load"))
      }
    }
    if (!queued) error("Player is closed")
    return try {
      val loaded = withTimeout(30_000) { submitted.await() }
      check(stillCurrent() && admissionEligible) { "Playback admission changed" }
      loaded
    }
    catch (failure: Exception) {
      // Cancellation must not release an admitted account operation while its load is still opening.
      retireMedia()
      throw failure
    } finally { execute { loadWaiters.remove(epoch) } }
  }

  private fun load(remote: MediaLocator?, uri: Uri?, subtitle: Uri?) {
    synchronized(policyLock) { requestedPlaying = false; cancelPlaybackGestures() }
    val epoch = commandEpoch.incrementAndGet()
    if (!ready.value || !admitted.get() || handoff.get() || closed.get()) {
      mutableError.value = application.localizedString(R.string.player_paused_background)
      return
    }
    execute { if (closed.get() || commandEpoch.get() != epoch) return@execute
    if (pendingLoad != null) {
      mutableError.value = application.localizedString(R.string.player_wait_for_load)
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
      if (!withAudioFocus(epoch) {
        requestedPlaying = true
        current.load(MediaLoad(source, generation = epoch, externalSubtitles = subtitles))
      }) {
        retire(epoch)
        if (commandEpoch.get() == epoch) mutableError.value = application.localizedString(R.string.player_audio_focus_denied)
      }
    } catch (_: Exception) {
      descriptors.remove(epoch)
      owned.forEach { runCatching { it.close() } }
      if (pendingLoad == epoch) pendingLoad = null
      if (commandEpoch.get() == epoch && !closed.get()) mutableError.value = application.localizedString(R.string.player_open_failed)
      if (descriptors.isEmpty()) releaseAudioFocus()
    } finally {
      opening.compareAndSet(cancellation, null)
    } }
  }

  fun play(stillCurrent: () -> Boolean = { true }, gestureToken: Long? = null): Boolean {
    if (gestureToken == null) cancelPlaybackGestures()
    val current = host ?: return false
    if (closed.get() || !admitted.get() || handoff.get() || !stillCurrent()) {
      mutableError.value = application.localizedString(R.string.player_paused_background)
      return false
    }
    val accepted = withAudioFocus { if (stillCurrent()) { requestedPlaying = true; current.play() } }
    if (!accepted) mutableError.value = application.localizedString(R.string.player_audio_focus_denied)
    return accepted
  }
  suspend fun resumeMedia(gestureToken: Long? = null, stillCurrent: () -> Boolean): Boolean {
    val generation = snapshot.value.generation
    if (!play(stillCurrent, gestureToken)) return false
    return try {
      withTimeout(10_000) {
        val resumed = snapshot.first { !it.paused || it.generation != generation || !stillCurrent() || !admissionEligible || it.error != null }
        val accepted = resumed.generation == generation && !resumed.paused && stillCurrent() && admissionEligible
        if (!accepted) settleRevokedResume(generation)
        accepted
      }
    } catch (_: TimeoutCancellationException) { settleRevokedResume(generation); false }
    catch (cancelled: kotlinx.coroutines.CancellationException) { settleRevokedResume(generation); throw cancelled }
  }
  private suspend fun settleRevokedResume(generation: Long) = withContext(NonCancellable) {
    synchronized(policyLock) {
      if (snapshot.value.generation != generation) return@withContext
      pause()
    }
    val receipt = CompletableDeferred<Boolean>()
    val queued = execute { receipt.complete(runCatching { host?.pauseAndConfirm(generation) == true }.getOrDefault(false)) }
    val acknowledged = queued && kotlinx.coroutines.withTimeoutOrNull(15_000) { receipt.await() } == true
    if (!acknowledged) retireMedia()
  }
  /** Initial preferences settle while paused, before the admitted operation may make audio audible. */
  suspend fun configureMedia(volume: Int?, audio: Int?, subtitle: Int?, stillCurrent: () -> Boolean): Boolean {
    val generation = snapshot.value.generation
    val submitted = synchronized(policyLock) {
      val current = host
      if (current == null || !admissionEligible || !stillCurrent()) false else {
        volume?.let(current::setVolume)
        audio?.let { current.selectTrack(TrackKind.AUDIO, it) }
        subtitle?.let { current.selectTrack(TrackKind.SUBTITLE, it) }
        true
      }
    }
    if (!submitted) return false
    fun selected(value: PlayerSnapshot, kind: TrackKind, id: Int?): Boolean = when (id) {
      null -> true
      PlayerHost.TRACK_ID_NONE -> value.tracks.none { it.kind == kind && it.isSelected }
      PlayerHost.TRACK_ID_AUTO -> value.tracks.any { it.kind == kind && it.isSelected }
      else -> value.tracks.any { it.kind == kind && it.mpvId == id && it.isSelected }
    }
    return try {
      val configured = withTimeout(10_000) { snapshot.first {
        it.generation != generation || !admissionEligible || !stillCurrent() || it.error != null ||
          ((volume == null || it.volumePercent == volume) && selected(it, TrackKind.AUDIO, audio) && selected(it, TrackKind.SUBTITLE, subtitle))
      } }
      configured.generation == generation && configured.error == null && stillCurrent() && admissionEligible
    } catch (_: TimeoutCancellationException) { false }
  }
  fun pause() {
    synchronized(policyLock) {
      requestedPlaying = false
      cancelPlaybackGestures()
      commandEpoch.incrementAndGet()
      opening.get()?.cancel()
      releaseAudioFocus()
      host?.pause()
    }
  }
  fun seek(seconds: Double) = synchronized(policyLock) {
    if (seekPreview.value == null) { cancelPlaybackGestures(); host?.seekTo(seconds) }
  }
  fun volume(percent: Int) { host?.setVolume(percent) }
  fun mute(value: Boolean) { host?.setMuted(value) }
  fun speed(value: Double) { synchronized(policyLock) { speedGesture?.let { host?.endTemporarySpeed(it) }; speedGesture = null; host?.setSpeed(value) } }
  fun pictureBrightness(percent: Int) { host?.setPictureBrightness(percent) }
  fun select(kind: TrackKind, id: Int) { host?.selectTrack(kind, id) }

  fun beginGestureSeek(): GestureSeek? = synchronized(policyLock) {
    if (seekPreview.value != null) return@synchronized null
    val observed = snapshot.value
    if (!ready.value || !admissionEligible || !observed.seekable ||
      observed.status !in listOf(PlayerStatus.READY, PlayerStatus.BUFFERING)) return@synchronized null
    cancelPlaybackGestures()
    val token = gestureSequence.incrementAndGet()
    val epoch = commandEpoch.incrementAndGet()
    setSeekPreview(SeekPreview(token, observed.generation, epoch, observed.positionSeconds, requestedPlaying))
    // Retain focus while previewing; this pause is not a new user pause intent.
    host?.pause()
    GestureSeek(token, observed.positionSeconds)
  }

  /** Called inside the coordinator's admitted operation; cancellation can revoke it at any await. */
  suspend fun finishGestureSeek(token: Long, target: Double?, stillCurrent: () -> Boolean): Boolean {
    val preview = seekPreview.value?.takeIf { it.token == token } ?: return false
    // A caller's admission check can itself revoke playback; inspect our receipt after it returns.
    fun valid(): Boolean = stillCurrent() && seekPreview.value === preview && commandEpoch.get() == preview.epoch &&
      snapshot.value.generation == preview.generation && admissionEligible
    val destination = (target?.takeIf { it.isFinite() } ?: preview.origin).coerceAtLeast(0.0)
      .let { snapshot.value.durationSeconds?.takeIf { end -> end.isFinite() && end > 0 }?.let(it::coerceAtMost) ?: it }
    try {
      val submitted = CompletableDeferred<Boolean>()
      val queued = execute {
        val current = host
        val accepted = if (current == null || !valid() || !current.pauseAndConfirm(preview.generation)) false
        else synchronized(policyLock) {
          // Cancellation restores the origin under this same lock. A revoked preview
          // must never enqueue its old target after that restoration.
          if (!valid()) false else { current.seekTo(destination); true }
        }
        submitted.complete(accepted)
      }
      if (!queued || !submitted.await()) return false
      val landed = withTimeout(10_000) {
        combine(snapshot, seekPreview) { value, active -> value to active }.first { (value, active) ->
          active !== preview || !valid() || value.error != null ||
            (value.paused && kotlin.math.abs(value.positionSeconds - destination) <= 0.3)
        }.first
      }
      if (!valid() || landed.error != null) return false
      return if (preview.resume) resumeMedia(token, ::valid) else true
    } catch (_: TimeoutCancellationException) {
      return false
    } finally {
      synchronized(policyLock) {
        if (seekPreview.value === preview) setSeekPreview(null)
      }
    }
  }

  fun beginGestureSpeed(): Long? = synchronized(policyLock) {
    val observed = snapshot.value
    if (!ready.value || !admissionEligible || !requestedPlaying || !observed.isPlaying || observed.paused ||
      observed.speed == 2.0 || seekPreview.value != null || speedGesture != null) return@synchronized null
    val token = gestureSequence.incrementAndGet()
    speedGesture = token
    host?.beginTemporarySpeed(token, observed.generation)
    token
  }

  fun endGestureSpeed(token: Long) = synchronized(policyLock) {
    if (speedGesture == token) { speedGesture = null; host?.endTemporarySpeed(token) }
  }

  /** System/panel interruption restores the origin and rate but never grants permission to resume. */
  fun cancelPlaybackGestures() = synchronized(policyLock) {
    val preview = seekPreview.value
    setSeekPreview(null)
    if (preview != null) {
      requestedPlaying = false
      commandEpoch.incrementAndGet()
      host?.pause()
      if (snapshot.value.generation == preview.generation && snapshot.value.seekable) host?.seekTo(preview.origin)
      releaseAudioFocus()
    }
    speedGesture?.let { host?.endTemporarySpeed(it) }
    speedGesture = null
  }

  fun stop() {
    synchronized(policyLock) {
      requestedPlaying = false
      cancelPlaybackGestures()
      commandEpoch.incrementAndGet()
      opening.get()?.cancel()
      releaseAudioFocus()
      host?.stop()
    }
  }

  suspend fun stopAndWait(preserveSessionSettings: Boolean = false): Boolean {
    if (closed.get()) { released.await(); return true }
    synchronized(policyLock) {
      requestedPlaying = false
      cancelPlaybackGestures()
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
      if (generations.isEmpty()) {
        // An already unloaded file may still carry the current player-session settings.
        current?.stop(preserveSessionSettings)
        receipt.complete(true)
      }
      else {
        stopWaiters += StopWaiter(generations, receipt)
        current?.stop(preserveSessionSettings)
      }
    }
    if (!queued) { released.await(); return true }
    return try { withTimeout(15000) { receipt.await() } }
    catch (_: TimeoutCancellationException) { false }
    finally { execute { stopWaiters.removeAll { it.completion === receipt } } }
  }

  /** Never releases an SDK admission while native teardown is merely requested or timed out. */
  suspend fun retireMedia() = withContext(NonCancellable) {
    if (!stopAndWait()) {
      // A wedged ordinary stop makes this host unusable. Disposal is the final physical boundary;
      // a stuck native destroy deliberately keeps account admission closed until process restart.
      close()
      released.await()
    }
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
    businessIntent?.let { return it(intent) }
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
    synchronized(policyLock) { requestedPlaying = false; cancelPlaybackGestures() }
    if (!synchronized(submissionLock) { closed.compareAndSet(false, true) }) return
    admitted.set(false)
    businessIntent = null
    businessEvents.close()
    releaseAudioFocus()
    application.unregisterReceiver(noisyReceiver)
    mutableReady.value = false
    opening.get()?.cancel()
    host?.setAdmissionEligible(false)
    host?.pause()
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
      loadWaiters.values.forEach { it.completeExceptionally(IllegalStateException("Player closed")) }
      loadWaiters.clear()
      released.complete(Unit)
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
