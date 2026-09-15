package io.github.hewel.jellypilot.player

import android.view.Surface

/**
 * Configuration for one [PlayerHost] instance.
 *
 * @param cacheDir App-private writable directory for mpv's shader/ICC caches.
 * @param tlsCaFile Path to a PEM CA bundle for HTTPS. Android does not expose
 *   its trust store to ffmpeg's TLS stack; the app must stage a CA file (the
 *   system bundle or a packaged copy) and pass its path here. Required for
 *   HTTPS media; null disables TLS verification setup and HTTPS may fail.
 * @param pauseWhenIneligible When true, playback is paused as soon as
 *   [setAdmissionEligible] reports ineligibility (strict foreground pause).
 *   When false, ineligibility only blocks new play/load commands.
 */
data class PlayerHostConfig(
  val cacheDir: String,
  val tlsCaFile: String? = null,
  val pauseWhenIneligible: Boolean = true,
)

/**
 * The Android libmpv playback host.
 *
 * Owns one libmpv instance, its event thread, and its render thread. All
 * methods are thread-safe; native work is serialized on a single executor, so
 * calls may be made from any thread and are executed in submission order.
 *
 * Lifecycle: construct → use → [release]. The instance survives [detachSurface]
 * and can be re-attached later; only [release] destroys it.
 *
 * Admission: [load] and [play] are re-checked against the last value pushed via
 * [setAdmissionEligible] at execution time — not only when submitted — so a
 * command queued while eligible is still refused if the app became invisible or
 * locked before it ran. Rejection is reported as
 * [PlayerEvent.CommandRejected]; it is never silently deferred. Becoming
 * eligible again never resumes playback by itself.
 */
interface PlayerHost {
  /**
   * Observed state and one-shot events. Callbacks arrive on the host's
   * serialized executor thread, never concurrently, and never while the host
   * holds a lock that native code could re-enter. Listeners must not call back
   * into the host synchronously; post to another executor instead.
   */
  interface Listener {
    /** Called whenever any observed field changes; delivers the full new snapshot. */
    fun onSnapshot(snapshot: PlayerSnapshot)

    /** Called for one-shot occurrences (file loaded, natural end, rejections, logs). */
    fun onEvent(event: PlayerEvent)
  }

  /** Registers [listener]; it immediately receives the current snapshot. */
  fun addListener(listener: Listener)

  fun removeListener(listener: Listener)

  /** Latest snapshot; safe to read from any thread. */
  val snapshot: PlayerSnapshot

  /**
   * Pushes the current foreground admission value (visible + unlocked, per the
   * app's visibility helper). Re-checked when queued play/load commands execute.
   * With [PlayerHostConfig.pauseWhenIneligible], a transition to false pauses
   * playback. A transition to true never resumes playback.
   */
  fun setAdmissionEligible(eligible: Boolean)

  /**
   * Queues a media load. The request is re-checked against admission when it
   * reaches execution; if ineligible it is rejected via
   * [PlayerEvent.CommandRejected] and nothing is loaded.
   *
   * Loads are serialized: at most one native load is outstanding and one
   * replacement is queued behind it. A queued replacement starts only after
   * the outgoing load's native lifetime ends; a newer load displaces a still
   * queued one.
   *
   * Every accepted-or-rejected load produces exactly one terminal event for
   * its [MediaLoad.generation]: [PlayerEvent.LoadRejected] when the load
   * never reached native playback (refusal, or [RejectionReason.CANCELLED]
   * when superseded or stopped before mpv started the file), or
   * [PlayerEvent.PlaybackStopped] once a started file is fully unloaded.
   * Borrowed resources stay valid until that event.
   *
   * @throws IllegalArgumentException synchronously for malformed locators.
   */
  fun load(request: MediaLoad)

  /** Admitted play request; re-checked at execution. */
  fun play()

  /** Pause request; always accepted, even when ineligible. */
  fun pause()

  /**
   * Seeks to [positionSeconds] (absolute, exact). Always accepted; seeking does
   * not start playback and does not require admission.
   */
  fun seekTo(positionSeconds: Double)

  /** Stops and unloads the current media; always accepted. */
  fun stop()

  /** Sets output volume 0–100 percent. */
  fun setVolume(percent: Int)

  fun setMuted(muted: Boolean)

  /** Sets playback speed multiplier (clamped to 0.25–4.0). */
  fun setSpeed(speed: Double)

  /**
   * Selects a track by mpv id (see [PlayerTrack.mpvId]), deselects with
   * [TRACK_ID_NONE], or restores mpv's automatic selection with
   * [TRACK_ID_AUTO]. Only meaningful while media is loaded.
   */
  fun selectTrack(kind: TrackKind, mpvId: Int)

  /**
   * Reports the current surface size (`android-surface-size`) so mpv
   * reconfigures video output on resize without a surface recreation.
   */
  fun setSurfaceSize(width: Int, height: Int)

  /**
   * Attaches a render surface. Any previously attached surface is released
   * first. Safe to call again after [detachSurface]; the mpv instance and
   * playback state are preserved across surface recreation.
   */
  fun attachSurface(surface: Surface)

  /**
   * Detaches the render surface. Blocks until the native render thread has
   * released the underlying ANativeWindow — when this returns, libmpv provably
   * no longer references the surface and it is safe for the SurfaceView to
   * destroy it. Playback (audio) continues while detached.
   */
  fun detachSurface()

  /**
   * Destroys the host: detaches any surface, stops the event and render
   * threads, and terminates libmpv. Blocks until native teardown completes.
   * All later calls are rejected with [RejectionReason.RELEASED].
   */
  fun release()

  companion object {
    /** Track id that deselects a track kind (mpv `no`). */
    const val TRACK_ID_NONE: Int = -1

    /** Track id that restores automatic selection for a kind (mpv `auto`). */
    const val TRACK_ID_AUTO: Int = -2
  }
}

/** Maps a [PlayerHost.selectTrack] id to the mpv `vid`/`aid`/`sid` value. */
internal fun mpvTrackIdValue(mpvId: Int): String = when (mpvId) {
  PlayerHost.TRACK_ID_NONE -> "no"
  PlayerHost.TRACK_ID_AUTO -> "auto"
  else -> mpvId.toString()
}
