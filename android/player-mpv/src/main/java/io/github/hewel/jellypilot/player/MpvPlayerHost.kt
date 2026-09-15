package io.github.hewel.jellypilot.player

import android.content.Context
import android.os.Handler
import android.os.HandlerThread
import android.util.Log
import android.view.Surface
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch

/**
 * [PlayerHost] backed by one libmpv instance.
 *
 * All commands and native event handling run on a single serialized executor
 * thread, so every mutation of [snapshot] happens in one place and in order.
 * Native listener callbacks (mpv event thread) only enqueue work here; they
 * never touch Kotlin state directly, which keeps the observer lifetime
 * bounded by [release] and avoids locks held across reentrant callbacks.
 *
 * [attachSurface]/[detachSurface]/[release] block the caller until the
 * serialized work (including native surface relinquishment for detach) has
 * completed. Calling them from inside a [Listener] callback runs the work
 * inline instead of deadlocking.
 */
class MpvPlayerHost(context: Context, private val config: PlayerHostConfig) : PlayerHost {

  private val thread = HandlerThread("jellypilot-player").apply { start() }
  private val handler = Handler(thread.looper)
  private val listeners = CopyOnWriteArrayList<PlayerHost.Listener>()

  private val nativeListener = object : MpvJni.NativeListener {
    override fun onPropertyFlag(name: String, value: Int) =
      enqueue { applyFlagProperty(name, value != 0) }

    override fun onPropertyLong(name: String, value: Long) =
      enqueue { applyLongProperty(name, value) }

    override fun onPropertyDouble(name: String, value: Double) =
      enqueue { applyDoubleProperty(name, value) }

    override fun onPropertyString(name: String, value: String) =
      enqueue { applyStringProperty(name, value) }

    override fun onPropertyNone(name: String) =
      enqueue { applyNoneProperty(name) }

    override fun onEvent(eventId: Int) = enqueue { applyEvent(eventId) }

    override fun onStartFile(playlistEntryId: Long) =
      enqueue { applyStartFile(playlistEntryId) }

    override fun onEndFile(playlistEntryId: Long, reason: Int, error: Int) =
      enqueue { applyEndFile(playlistEntryId, reason, error) }

    override fun onLog(prefix: String, level: Int, text: String) =
      enqueue { emit(PlayerEvent.LogMessage(prefix, level, redact(text))) }
  }

  @Volatile
  private var handle: Long = 0

  @Volatile
  private var released = false

  /**
   * Latest admission value, published synchronously by [setAdmissionEligible]
   * so commands already queued on the executor observe a revocation that
   * happened after they were posted. The snapshot field lags one executor
   * turn and is only for observers.
   */
  @Volatile
  private var admissionEligible = true

  // A later affirmative intent must not revive an older revoked command.
  private val intentLock = Any()
  private var playRevision = 0L
  private var playIntent = false

  private fun requestPlay(desired: Boolean): Long = synchronized(intentLock) {
    playIntent = desired
    ++playRevision
  }

  private fun revokePlay() = synchronized(intentLock) {
    ++playRevision
    playIntent = false
  }

  private fun shouldPlay(revision: Long): Boolean = synchronized(intentLock) {
    playIntent && revision == playRevision && admissionEligible && !shuttingDown
  }

  @Volatile
  private var shuttingDown = false

  @Volatile
  private var current = PlayerSnapshot()

  // Executor-thread mutable bookkeeping.
  //
  // Load serialization contract: at most one native load is outstanding
  // (`active`) and at most one replacement is queued (`pending`). A pending
  // load is submitted only after the outgoing record's native lifetime has
  // provably ended — its END_FILE was consumed, or teardown proved it never
  // started (see beginTeardown). This keeps every START_FILE/END_FILE pair
  // attributable to exactly one record.
  private var active: LoadRecord? = null
  private var pending: MediaLoad? = null
  private var pendingPlayRevision = 0L

  private var coreIdle = true
  private var pausedForCache = false
  private var eofReached = false
  private var surfaceAttached = false
  private val secrets = mutableSetOf<String>()

  override val snapshot: PlayerSnapshot
    get() = current

  init {
    val latch = CountDownLatch(1)
    var failure: Throwable? = null
    handler.post {
      try {
        handle = MpvJni.nativeCreate(
          context.applicationContext,
          nativeListener,
          config.cacheDir,
          config.tlsCaFile,
        )
        observeProperties()
      } catch (t: Throwable) {
        failure = t
      } finally {
        latch.countDown()
      }
    }
    latch.await()
    failure?.let {
      thread.quitSafely()
      throw it
    }
  }

  // ------------------------------------------------------------------
  // Listener management
  // ------------------------------------------------------------------

  override fun addListener(listener: PlayerHost.Listener) {
    listeners.addIfAbsent(listener)
    listener.onSnapshot(current)
  }

  override fun removeListener(listener: PlayerHost.Listener) {
    listeners.remove(listener)
  }

  // ------------------------------------------------------------------
  // Public commands (any thread)
  // ------------------------------------------------------------------

  override fun setAdmissionEligible(eligible: Boolean) {
    // Publish synchronously: commands queued before this call but executed
    // after it must observe the revocation, not the stale snapshot value.
    admissionEligible = eligible
    if (!eligible) {
      // Losing admission revokes play intent immediately, so a queued
      // play/load or a FILE_LOADED callback cannot resurrect it.
      revokePlay()
    }
    enqueue {
      if (released) return@enqueue
      val wasEligible = current.admissionEligible
      if (!eligible && current.playWhenReady) {
        // Losing admission always drops queued play intent — including intent
        // parked while paused or still loading — so FILE_LOADED never
        // unpauses into an ineligible state.
        mutate { copy(playWhenReady = false) }
      }
      mutate { copy(admissionEligible = eligible) }
      if (wasEligible && !eligible && config.pauseWhenIneligible) {
        // Strict foreground pause: losing eligibility pauses playback. The
        // write is unconditional because the `pause` property observation can
        // lag a play() issued moments earlier.
        setPaused(true)
      }
    }
  }

  private fun ifReleased(command: String): Boolean {
    if (!released && !shuttingDown) return false
    reject(command, RejectionReason.RELEASED)
    return true
  }

  override fun load(request: MediaLoad) {
    validateLoad(request)
    if (released || shuttingDown) {
      emitLoadRejected(request, RejectionReason.RELEASED)
      return
    }
    // A fresh explicit load asserts play intent from its own request, never
    // from the outgoing snapshot's playWhenReady.
    val revision = requestPlay(!request.startPaused)
    enqueue {
      when {
        released || shuttingDown -> emitLoadRejected(request, RejectionReason.RELEASED)
        !admissionEligible -> emitLoadRejected(request, RejectionReason.NOT_ADMITTED)
        else -> acceptLoad(request, revision)
      }
    }
  }

  override fun play() {
    if (ifReleased("play")) return
    val revision = requestPlay(true)
    enqueue {
      when {
        released || shuttingDown -> reject("play", RejectionReason.RELEASED)
        !admissionEligible -> reject("play", RejectionReason.NOT_ADMITTED)
        !shouldPlay(revision) -> Unit
        pending != null -> {
          pendingPlayRevision = revision
          mutate { copy(playWhenReady = true) }
        }
        active == null || active?.endQueued == true -> reject("play", RejectionReason.NOT_READY)
        else -> {
          active?.playRevision = revision
          setPaused(false)
          mutate { copy(playWhenReady = true) }
        }
      }
    }
  }
  override fun pause() {
    if (ifReleased("pause")) return
    revokePlay()
    enqueue {
      if (released) return@enqueue
      setPaused(true)
      mutate { copy(playWhenReady = false) }
    }
  }
  override fun seekTo(positionSeconds: Double) {
    if (ifReleased("seek")) return
    enqueue {
      when {
        released -> reject("seek", RejectionReason.RELEASED)
        active?.fileLoaded != true -> reject("seek", RejectionReason.NOT_READY)
        else -> {
          val target = positionSeconds.coerceAtLeast(0.0)
          runCommand("seek", "seek", formatSeconds(target), "absolute+exact")
        }
      }
    }
  }

  override fun stop() {
    if (ifReleased("stop")) return
    revokePlay()
    enqueue {
      if (released) return@enqueue
      executeStop()
    }
  }

  override fun setVolume(percent: Int) {
    if (ifReleased("volume")) return
    enqueue {
      if (released) return@enqueue
      val clamped = percent.coerceIn(0, 100)
      if (MpvJni.nativeSetPropertyDouble(handle, "volume", clamped.toDouble()) < 0) {
        mutate { copy(error = PlayerError.CommandFailed("volume", "mpv rejected the value")) }
      }
    }
  }

  override fun setMuted(muted: Boolean) {
    if (ifReleased("mute")) return
    enqueue {
      if (released) return@enqueue
      if (MpvJni.nativeSetPropertyFlag(handle, "mute", muted) < 0) {
        mutate { copy(error = PlayerError.CommandFailed("mute", "mpv rejected the value")) }
      }
    }
  }

  override fun setSpeed(speed: Double) {
    if (ifReleased("speed")) return
    enqueue {
      if (released) return@enqueue
      val clamped = speed.coerceIn(0.25, 4.0)
      if (MpvJni.nativeSetPropertyDouble(handle, "speed", clamped) < 0) {
        mutate { copy(error = PlayerError.CommandFailed("speed", "mpv rejected the value")) }
      }
    }
  }

  override fun selectTrack(kind: TrackKind, mpvId: Int) {
    if (ifReleased("selectTrack")) return
    enqueue {
      when {
        released -> reject("selectTrack", RejectionReason.RELEASED)
        active?.fileLoaded != true -> reject("selectTrack", RejectionReason.NOT_READY)
        else -> {
          val property = when (kind) {
            TrackKind.VIDEO -> "vid"
            TrackKind.AUDIO -> "aid"
            TrackKind.SUBTITLE -> "sid"
          }
          runCommand("selectTrack", "set", property, mpvTrackIdValue(mpvId))
        }
      }
    }
  }

  override fun setSurfaceSize(width: Int, height: Int) {
    if (released) return
    enqueue {
      if (released) return@enqueue
      MpvJni.nativeSetPropertyString(handle, "android-surface-size", "${width}x$height")
    }
  }

  // ------------------------------------------------------------------
  // Surface and lifecycle (blocking)
  // ------------------------------------------------------------------

  override fun attachSurface(surface: Surface) = runBlocking {
    if (released) return@runBlocking
    MpvJni.nativeAttachSurface(handle, surface)
    surfaceAttached = true
    emit(PlayerEvent.SurfaceAttached)
  }

  override fun detachSurface() = runBlocking {
    if (released || !surfaceAttached) return@runBlocking
    // Blocks until the mpv core confirms the VO released the ANativeWindow.
    MpvJni.nativeDetachSurface(handle)
    surfaceAttached = false
    emit(PlayerEvent.SurfaceDetached)
  }

  override fun release() {
    // Revoke play intent before queueing so callbacks already in flight
    // cannot resurrect it while teardown runs.
    shuttingDown = true
    revokePlay()
    runBlocking {
      if (released) return@runBlocking
      released = true
      // Native teardown first: only once libmpv provably stopped touching the
      // media may consumers retire its borrowed descriptors.
      MpvJni.nativeDestroy(handle)
      handle = 0
      // The submitted load's terminal event is PlaybackStopped: destroy is the
      // end of its native use. START/END_FILEs queued during teardown are
      // dropped by the released guard in the apply* callbacks.
      active?.let { record ->
        active = null
        emit(PlayerEvent.PlaybackStopped(record.request.mediaId, record.request.generation))
      }
      // A queued replacement never reached mpv.
      pending?.let { emitLoadRejected(it, RejectionReason.RELEASED) }
      pending = null
      secrets.clear()
      mutate {
        copy(
          status = PlayerStatus.IDLE,
          playWhenReady = false,
          isPlaying = false,
          paused = true,
          tracks = emptyList(),
          mediaId = null,
          generation = 0,
        )
      }
      thread.quitSafely()
    }
  }

  // ------------------------------------------------------------------
  // Executor internals
  // ------------------------------------------------------------------

  private fun onExecutorThread(): Boolean = Thread.currentThread() === thread

  private fun enqueue(block: () -> Unit) {
    // Posts are rejected once the looper is gone (post-release); callers
    // already gate on `released` for observable behavior. An uncaught
    // exception would kill the HandlerThread and silently drop every later
    // command, so failures surface as a snapshot error instead.
    handler.post {
      try {
        block()
      } catch (t: Throwable) {
        mutate { copy(error = PlayerError.CommandFailed("internal", t.message ?: t.javaClass.name)) }
      }
    }
  }

  /** Runs [block] on the executor and waits for completion; inline on the executor thread. */
  private fun runBlocking(block: () -> Unit) {
    if (onExecutorThread()) {
      block()
      return
    }
    val latch = CountDownLatch(1)
    var failure: Throwable? = null
    val posted = handler.post {
      try {
        block()
      } catch (t: Throwable) {
        failure = t
      } finally {
        latch.countDown()
      }
    }
    // After release() the looper is gone; a rejected post must not hang the caller.
    if (posted) {
      latch.await()
      failure?.let { throw it }
    }
  }

  private fun mutate(transform: PlayerSnapshot.() -> PlayerSnapshot) {
    current = current.transform()
    val snapshot = current
    for (listener in listeners) {
      try {
        listener.onSnapshot(snapshot)
      } catch (t: Throwable) {
        // A misbehaving listener must not kill the executor thread.
        Log.w(TAG, "listener onSnapshot threw", t)
      }
    }
  }

  private fun emit(event: PlayerEvent) {
    for (listener in listeners) {
      try {
        listener.onEvent(event)
      } catch (t: Throwable) {
        Log.w(TAG, "listener onEvent threw", t)
      }
    }
  }

  private fun reject(command: String, reason: RejectionReason) {
    emit(PlayerEvent.CommandRejected(command, reason))
  }

  /**
   * Terminal event for a load that never reached native playback. Also emits
   * the generic [PlayerEvent.CommandRejected] for real refusals so UI
   * surfaces still see them; ordinary cancellation (superseded or stopped
   * before native start) is not a user-visible error.
   */
  private fun emitLoadRejected(request: MediaLoad, reason: RejectionReason) {
    if (reason != RejectionReason.CANCELLED) {
      reject("load", reason)
    }
    emit(PlayerEvent.LoadRejected(request.mediaId, request.generation, reason))
  }

  private fun runCommand(label: String, vararg args: String) {
    val result = MpvJni.nativeCommand(handle, arrayOf(*args))
    if (result < 0) {
      mutate {
        copy(error = PlayerError.CommandFailed(label, "mpv error $result"))
      }
    }
  }

  private fun setPaused(paused: Boolean) {
    if (MpvJni.nativeSetPropertyFlag(handle, "pause", paused) < 0) {
      mutate { copy(error = PlayerError.CommandFailed("pause", "mpv rejected the value")) }
    }
  }

  // ------------------------------------------------------------------
  // Load / stop
  // ------------------------------------------------------------------

  private fun validateLoad(request: MediaLoad) {
    fun check(locator: MediaLocator) {
      when (locator) {
        is MediaLocator.Remote ->
          require(
            locator.url.startsWith("http://") || locator.url.startsWith("https://"),
          ) { "Remote locator requires http(s): ${redactUrl(locator.url)}" }

        is MediaLocator.LocalFile ->
          require(locator.path.startsWith("/")) { "LocalFile requires an absolute path" }

        is MediaLocator.BorrowedFd ->
          require(locator.fd >= 0) { "BorrowedFd requires a valid descriptor" }
      }
    }
    check(request.locator)
    request.externalSubtitles.forEach { check(it.locator) }
    require(request.startPositionSeconds?.let { it.isFinite() && it >= 0.0 } != false) {
      "startPositionSeconds must be a finite non-negative value"
    }
  }

  private fun locatorUrl(locator: MediaLocator): String = when (locator) {
    is MediaLocator.Remote -> locator.url
    is MediaLocator.LocalFile -> locator.path
    is MediaLocator.BorrowedFd -> "fd://${locator.fd}"
  }

  /**
   * One load accepted by mpv, from `loadfile` success until retirement.
   * [entryId] is the `playlist_entry_id` returned by the command reply.
   */
  private class LoadRecord(
    val request: MediaLoad,
    val entryId: Long,
    /** Secret-bearing strings (URLs, header values) to redact while live. */
    val secrets: Set<String>,
    var playRevision: Long,
  ) {
    /** True once mpv emitted START_FILE for [entryId]. */
    var started: Boolean = false

    /** True once mpv reported FILE_LOADED for this record. */
    var fileLoaded: Boolean = false

    /** True once `stop` was issued for this record's teardown. */
    var endQueued: Boolean = false
  }

  /**
   * Accepts [request]: queues it as the pending replacement when a native
   * load is still outstanding, or submits it immediately when the core is
   * idle. A displaced pending load is cancelled — it never reached mpv.
   */
  private fun acceptLoad(request: MediaLoad, revision: Long) {
    if (active != null) {
      pending?.let { emitLoadRejected(it, RejectionReason.CANCELLED) }
      pending = request
      pendingPlayRevision = revision
      beginTeardown()
    } else {
      submitLoad(request, revision)
    }
  }

  /**
   * Begins ending the active record's native lifetime: issues `stop` (which
   * clears the playlist, so an entry that has not started yet never will),
   * waits on the command barrier, then posts a continuation behind every
   * START_FILE/END_FILE callback the event thread already forwarded.
   *
   * The barrier alone is not teardown: a started file's END_FILE may still be
   * in flight when it returns. The continuation runs strictly after those
   * callbacks, so it observes the record's final started state.
   */
  private fun beginTeardown() {
    val record = active ?: return
    if (record.endQueued) return
    record.endQueued = true
    val stopped = MpvJni.nativeCommand(handle, arrayOf("stop"))
    check(stopped >= 0) { "mpv rejected stop: $stopped" }
    check(MpvJni.nativeBarrier(handle, BARRIER_TIMEOUT_MS)) {
      // Keep this record and its borrowed descriptors alive until END_FILE or
      // destroy. A timeout cannot prove the entry never started.
      "mpv teardown barrier timed out"
    }
    enqueue { finishTeardown(record) }
  }

  /**
   * Runs behind every native event callback queued before the barrier. An
   * accepted entry that still never started can safely retire: `stop` cleared
   * the playlist before it could begin, so no START_FILE/END_FILE will ever
   * arrive for it. A started record waits for its real END_FILE instead.
   */
  private fun finishTeardown(record: LoadRecord) {
    if (released || active !== record) return
    if (!record.started) {
      retireRecord(record, cancelled = true)
      submitPending()
    }
    // Started record: applyEndFile retires it and submits the pending load.
  }

  /** Removes [record] as active and emits its terminal event exactly once. */
  private fun retireRecord(record: LoadRecord, cancelled: Boolean) {
    check(active === record) { "retiring a record that is not active" }
    active = null
    // Clear this load's HTTP credentials; the next submit installs its own.
    MpvJni.nativeSetHttpHeaders(handle, emptyArray())
    rebuildSecrets()
    if (cancelled) {
      emitLoadRejected(record.request, RejectionReason.CANCELLED)
    } else {
      emit(PlayerEvent.PlaybackStopped(record.request.mediaId, record.request.generation))
    }
  }

  /** Submits the queued replacement once no native load is outstanding. */
  private fun submitPending() {
    val request = pending ?: return
    pending = null
    when {
      released || shuttingDown -> emitLoadRejected(request, RejectionReason.RELEASED)
      !admissionEligible -> emitLoadRejected(request, RejectionReason.NOT_ADMITTED)
      else -> submitLoad(request, pendingPlayRevision)
    }
  }

  /**
   * Issues `loadfile` for [request]. Callers guarantee no native load is
   * outstanding, so the file-local `pause` write and the new entry's events
   * cannot interleave with an outgoing file's tail.
   */
  private fun submitLoad(request: MediaLoad, revision: Long) {
    val recordSecrets = collectSecrets(request)
    secrets += recordSecrets

    val headerLines = request.httpHeaders.map { (name, value) -> "$name: $value" }
    if (MpvJni.nativeSetHttpHeaders(handle, headerLines.toTypedArray()) < 0) {
      rebuildSecrets()
      mutate {
        copy(error = PlayerError.LoadFailed(request.mediaId, "mpv rejected HTTP headers"))
      }
      emitLoadRejected(request, RejectionReason.LOAD_FAILED)
      return
    }

    // Desired pause state for the incoming file. `pause` is file-local, so it
    // is set while no file is loaded and applies to the next loadfile. The
    // desired state comes from this request's intent as revoked or reasserted
    // up to this moment — never the outgoing snapshot's playWhenReady.
    val wantPlay = shouldPlay(revision)
    val startPaused = !wantPlay
    MpvJni.nativeSetPropertyFlag(handle, "pause", startPaused)

    eofReached = false
    pausedForCache = false
    coreIdle = true
    mutate {
      copy(
        status = PlayerStatus.LOADING,
        playWhenReady = wantPlay,
        isPlaying = false,
        paused = startPaused,
        positionSeconds = 0.0,
        durationSeconds = null,
        bufferedPositionSeconds = null,
        tracks = emptyList(),
        videoWidth = 0,
        videoHeight = 0,
        mediaId = request.mediaId,
        generation = request.generation,
        error = null,
      )
    }

    val args = mutableListOf("loadfile", locatorUrl(request.locator), "replace", "-1")
    request.startPositionSeconds?.let { args += "start=${formatSeconds(it)}" }
    val entryId = MpvJni.nativeLoadFile(handle, args.toTypedArray())
    if (entryId < 0) {
      rebuildSecrets()
      mutate {
        copy(
          status = PlayerStatus.IDLE,
          mediaId = null,
          generation = 0,
          error = PlayerError.LoadFailed(request.mediaId, "loadfile rejected"),
        )
      }
      emitLoadRejected(request, RejectionReason.LOAD_FAILED)
      return
    }
    // The entry is installed. From here it either starts (START_FILE, then
    // exactly one END_FILE) or is cancelled before start by a later stop.
    active = LoadRecord(request, entryId, recordSecrets, revision)
  }

  private fun executeStop() {
    // A queued replacement is cancelled outright; the active load's terminal
    // event is deferred until its native lifetime provably ends.
    pending?.let { emitLoadRejected(it, RejectionReason.CANCELLED) }
    pending = null
    beginTeardown()
    eofReached = false
    pausedForCache = false
    coreIdle = true
    mutate {
      copy(
        status = PlayerStatus.IDLE,
        playWhenReady = false,
        isPlaying = false,
        paused = true,
        positionSeconds = 0.0,
        durationSeconds = null,
        bufferedPositionSeconds = null,
        tracks = emptyList(),
        videoWidth = 0,
        videoHeight = 0,
        mediaId = null,
        generation = 0,
      )
    }
  }

  /** Rebuilds the redaction set from the load that is still live. */
  private fun rebuildSecrets() {
    secrets.clear()
    active?.let { secrets += it.secrets }
  }

  private fun applyFlagProperty(name: String, value: Boolean) {
    if (released) return
    when (name) {
      "pause" -> {
        // Ungated: pause writes issued while a file is still opening (e.g.
        // admission loss during load) must reach the snapshot; FILE_LOADED
        // reconciles desired-vs-actual unconditionally.
        mutate { copy(paused = value) }
        recomputePlaying()
      }
      "paused-for-cache" -> {
        if (active?.started != true) return
        pausedForCache = value
        if (active?.fileLoaded == true && !eofReached) {
          mutate {
            copy(
              status = if (value) PlayerStatus.BUFFERING else PlayerStatus.READY,
            )
          }
        }
        recomputePlaying()
      }
      "core-idle" -> {
        coreIdle = value
        recomputePlaying()
      }
      "idle-active" -> {
        if (value && active?.fileLoaded == true) {
          // mpv went idle without an END_FILE we acted on; reflect the unload
          // in the snapshot. The record stays active: its real END_FILE (or
          // release) still delivers the terminal retirement.
          mutate {
            copy(
              status = PlayerStatus.IDLE,
              isPlaying = false,
              tracks = emptyList(),
              mediaId = null,
              generation = 0,
            )
          }
        }
      }
      "eof-reached" -> {
        if (value && !eofReached && active?.fileLoaded == true) {
          eofReached = true
          mutate { copy(status = PlayerStatus.ENDED, isPlaying = false) }
          emit(PlayerEvent.NaturalEnd(current.mediaId, current.generation))
        }
      }
      "mute" -> mutate { copy(muted = value) }
      "seeking" -> Unit // position discontinuity is reflected via time-pos
    }
  }

  private fun applyLongProperty(name: String, value: Long) {
    if (released || active?.started != true) return
    when (name) {
      "video-params/w" -> mutate { copy(videoWidth = value.toInt()) }
      "video-params/h" -> mutate { copy(videoHeight = value.toInt()) }
    }
  }

  private fun applyDoubleProperty(name: String, value: Double) {
    if (released) return
    when (name) {
      // File-scoped observations only apply while their file is the live
      // started one; a queued callback from an outgoing file must not
      // decorate the replacement's state.
      "time-pos" -> if (active?.started == true) {
        mutate { copy(positionSeconds = value.coerceAtLeast(0.0)) }
      }
      "duration" -> if (active?.started == true) {
        mutate {
          copy(durationSeconds = if (value.isFinite() && value >= 0.0) value else null)
        }
      }
      "demuxer-cache-duration" -> if (active?.started == true) {
        mutate {
          copy(
            bufferedPositionSeconds =
              if (value.isFinite() && value >= 0.0) positionSeconds + value else null,
          )
        }
      }
      "volume" -> mutate { copy(volumePercent = value.toInt().coerceIn(0, 100)) }
      "speed" -> mutate { copy(speed = value) }
    }
  }

  private fun applyStringProperty(name: String, value: String) = Unit

  private fun applyNoneProperty(name: String) {
    if (released || active?.started != true) return
    when (name) {
      "track-list", "aid", "vid", "sid" -> refreshTracks()
    }
  }

  private fun applyStartFile(playlistEntryId: Long) {
    if (released) return
    // START_FILE carries the entry id returned by loadfile; a callback for an
    // entry that is no longer the active record is stale and ignored.
    if (active?.entryId == playlistEntryId) {
      active?.started = true
    }
  }

  private fun applyEvent(eventId: Int) {
    if (released) return
    when (eventId) {
      MPV_EVENT_FILE_LOADED -> {
        // FILE_LOADED carries no entry id; under the serialized load contract
        // it belongs to the live started record. A queued callback from a
        // file that already ended finds no started record and is dropped, so
        // it can neither decorate new state nor attach new subtitles.
        val record = active?.takeIf { it.started && !it.endQueued } ?: return
        record.fileLoaded = true
        // Reconcile desired-vs-actual pause. Play intent parked while loading
        // only unpause when admission still holds; an explicit pause or an
        // admission loss during loading must not be undone here. The write is
        // unconditional because the `pause` property observation can lag.
        val wantPlay = shouldPlay(record.playRevision)
        setPaused(!wantPlay)
        // External subtitles are added once the file is open so they bind to
        // this file, not the outgoing one.
        record.request.externalSubtitles.forEachIndexed { index, sub ->
          val args = mutableListOf("sub-add", locatorUrl(sub.locator))
          args += if (index == 0) "select" else "auto"
          sub.title?.let { args += it }
          if (sub.title != null && sub.language != null) {
            args += sub.language
          }
          MpvJni.nativeCommand(handle, args.toTypedArray())
        }
        mutate { copy(status = PlayerStatus.READY, playWhenReady = wantPlay) }
        emit(PlayerEvent.FileLoaded(current.mediaId, current.generation))
        refreshTracks()
      }
      // MPV_EVENT_VIDEO_RECONFIG is deliberately not surfaced: it signals an
      // output reconfiguration, not a presented frame, so it cannot back a
      // "first frame rendered" claim.
    }
  }

  private fun applyEndFile(playlistEntryId: Long, reason: Int, error: Int) {
    if (released) return
    // END_FILE carries the entry id of the file that actually played; only
    // the active record can match. A stray or superseded id is ignored.
    val record = active?.takeIf { it.entryId == playlistEntryId } ?: return
    val mediaId = record.request.mediaId
    val generation = record.request.generation
    pausedForCache = false
    coreIdle = true

    // Only touch the visible state when the retired load still owns it; a
    // superseded load's END_FILE must not clobber the replacement's snapshot.
    if (current.status != PlayerStatus.IDLE &&
      current.generation == generation && current.mediaId == mediaId) {
      if (reason == END_FILE_REASON_ERROR) {
        mutate {
          copy(
            status = PlayerStatus.IDLE,
            isPlaying = false,
            paused = true,
            tracks = emptyList(),
            error = if (record.fileLoaded) PlayerError.PlaybackFailed(mediaId, "mpv error $error")
              else PlayerError.LoadFailed(mediaId, "mpv error $error"),
          )
        }
      } else {
        mutate {
          copy(
            status = PlayerStatus.IDLE,
            isPlaying = false,
            paused = true,
            tracks = emptyList(),
          )
        }
      }
    }
    retireRecord(record, cancelled = false)
    // The outgoing native lifetime has ended; a queued replacement may now
    // be submitted.
    submitPending()
  }

  private fun recomputePlaying() {
    val playing = !current.paused && !coreIdle && !pausedForCache &&
      (current.status == PlayerStatus.READY || current.status == PlayerStatus.BUFFERING)
    if (playing != current.isPlaying) {
      mutate { copy(isPlaying = playing) }
    }
  }

  private fun refreshTracks() {
    if (active?.fileLoaded != true) return
    val count = MpvJni.nativeGetPropertyString(handle, "track-list/count")?.toIntOrNull() ?: return
    val tracks = buildList {
      for (i in 0 until count) {
        val type = MpvJni.nativeGetPropertyString(handle, "track-list/$i/type") ?: continue
        val kind = when (type) {
          "video" -> TrackKind.VIDEO
          "audio" -> TrackKind.AUDIO
          "sub" -> TrackKind.SUBTITLE
          else -> continue
        }
        val id = MpvJni.nativeGetPropertyString(handle, "track-list/$i/id")?.toIntOrNull()
          ?: continue
        add(
          PlayerTrack(
            mpvId = id,
            kind = kind,
            title = MpvJni.nativeGetPropertyString(handle, "track-list/$i/title"),
            language = MpvJni.nativeGetPropertyString(handle, "track-list/$i/lang"),
            codec = MpvJni.nativeGetPropertyString(handle, "track-list/$i/codec"),
            isDefault = MpvJni.nativeGetPropertyString(handle, "track-list/$i/default") == "yes",
            isForced = MpvJni.nativeGetPropertyString(handle, "track-list/$i/forced") == "yes",
            isExternal = MpvJni.nativeGetPropertyString(handle, "track-list/$i/external") == "yes",
            isSelected = MpvJni.nativeGetPropertyString(handle, "track-list/$i/selected") == "yes",
          ),
        )
      }
    }
    if (tracks != current.tracks) {
      mutate { copy(tracks = tracks) }
    }
  }

  // ------------------------------------------------------------------
  // Redaction
  // ------------------------------------------------------------------

  /**
   * Removes every collected secret (media URLs, header values) plus URL
   * userinfo and common token query parameters from text leaving the host.
   */
  private fun redact(text: String): String {
    var result = text
    for (secret in secrets) {
      if (secret.isNotEmpty()) {
        result = result.replace(secret, REDACTED)
      }
    }
    result = USERINFO_REGEX.replace(result, "$1$2")
    result = TOKEN_QUERY_REGEX.replace(result, "$1=<redacted>")
    return result
  }

  private fun redactUrl(url: String): String =
    USERINFO_REGEX.replace(url, "$1$2")

  private fun observeProperties() {
    val flag = MPV_FORMAT_FLAG
    val int64 = MPV_FORMAT_INT64
    val double = MPV_FORMAT_DOUBLE
    val none = MPV_FORMAT_NONE
    MpvJni.nativeObserveProperty(handle, "pause", flag)
    MpvJni.nativeObserveProperty(handle, "paused-for-cache", flag)
    MpvJni.nativeObserveProperty(handle, "core-idle", flag)
    MpvJni.nativeObserveProperty(handle, "idle-active", flag)
    MpvJni.nativeObserveProperty(handle, "eof-reached", flag)
    MpvJni.nativeObserveProperty(handle, "mute", flag)
    MpvJni.nativeObserveProperty(handle, "seeking", flag)
    MpvJni.nativeObserveProperty(handle, "time-pos", double)
    MpvJni.nativeObserveProperty(handle, "duration", double)
    MpvJni.nativeObserveProperty(handle, "demuxer-cache-duration", double)
    MpvJni.nativeObserveProperty(handle, "volume", double)
    MpvJni.nativeObserveProperty(handle, "speed", double)
    MpvJni.nativeObserveProperty(handle, "track-list", none)
    MpvJni.nativeObserveProperty(handle, "video-params/w", int64)
    MpvJni.nativeObserveProperty(handle, "video-params/h", int64)
    MpvJni.nativeObserveProperty(handle, "aid", none)
    MpvJni.nativeObserveProperty(handle, "vid", none)
    MpvJni.nativeObserveProperty(handle, "sid", none)
  }

  private fun formatSeconds(value: Double): String =
    if (value == value.toLong().toDouble()) value.toLong().toString() else value.toString()

  private companion object {
    const val TAG = "MpvPlayerHost"
    const val BARRIER_TIMEOUT_MS = 5000
    const val REDACTED = "<redacted>"

    const val MPV_FORMAT_NONE = 0
    const val MPV_FORMAT_FLAG = 3
    const val MPV_FORMAT_INT64 = 4
    const val MPV_FORMAT_DOUBLE = 5

    const val MPV_EVENT_FILE_LOADED = 8

    const val END_FILE_REASON_ERROR = 4

    // scheme://user:pass@host -> scheme://host
    val USERINFO_REGEX = Regex("""([a-zA-Z][a-zA-Z0-9+.-]*://)[^/@\s]+@""")

    // ?api_key=… / &token=… -> ?api_key=<redacted>
    val TOKEN_QUERY_REGEX = Regex(
      """(?i)([?&](?:api_key|apikey|access_token|token|auth|sig|signature|key|credential|password|pwd|secret)=)[^&\s]*""",
    )
  }
}

/** Collects the secret-bearing strings of [request] for log redaction. */
private fun collectSecrets(request: MediaLoad): Set<String> {
  val secrets = mutableSetOf<String>()
  fun add(locator: MediaLocator) {
    if (locator is MediaLocator.Remote) secrets += locator.url
  }
  add(request.locator)
  request.externalSubtitles.forEach { add(it.locator) }
  secrets += request.httpHeaders.values
  return secrets
}
