package io.github.hewel.jellypilot.player

/** mpv track category. */
enum class TrackKind { VIDEO, AUDIO, SUBTITLE }

/** One entry of mpv's `track-list`. [mpvId] is the id used by [PlayerHost.selectTrack]. */
data class PlayerTrack(
  val mpvId: Int,
  val kind: TrackKind,
  val title: String?,
  val language: String?,
  val codec: String?,
  val isDefault: Boolean,
  val isForced: Boolean,
  val isExternal: Boolean,
  val isSelected: Boolean,
)

/**
 * Coarse load/playback phase. Buffering is reported separately from the desired
 * play state: [PlayerSnapshot.playWhenReady] may stay true while [status] is
 * [BUFFERING] or [LOADING].
 */
enum class PlayerStatus {
  /** Nothing loaded. */
  IDLE,

  /** A load was accepted but mpv has not finished opening the file. */
  LOADING,

  /** File is open; the demuxer is waiting for data (`paused-for-cache`). */
  BUFFERING,

  /** File is open and able to render (playing or paused). */
  READY,

  /** Playback reached end of file and is held on the last frame. */
  ENDED,
}

/**
 * A player failure with secrets already removed. [message] never contains media
 * URLs, header values, or credentials; it is safe to show in UI and logs.
 */
sealed class PlayerError {
  /** Redacted, display-safe description. */
  abstract val message: String

  /** mpv could not be created or initialized; the host is unusable. */
  data class NativeInitFailed(val detail: String) : PlayerError() {
    override val message: String get() = "Player initialization failed: $detail"
  }

  /** mpv rejected or failed while opening a load. */
  data class LoadFailed(val mediaId: String?, val detail: String) : PlayerError() {
    override val message: String get() = "Could not load media: $detail"
  }

  /** mpv reported a playback-ending error after a successful load. */
  data class PlaybackFailed(val mediaId: String?, val detail: String) : PlayerError() {
    override val message: String get() = "Playback failed: $detail"
  }

  /** A native command returned an error. */
  data class CommandFailed(val command: String, val detail: String) : PlayerError() {
    override val message: String get() = "Player command '$command' failed: $detail"
  }
}

/** Why an accepted-then-executed command was refused. */
enum class RejectionReason {
  /** Foreground admission was false when the command reached execution. */
  NOT_ADMITTED,

  /** The command needs loaded media and none is present. */
  NOT_READY,

  /** The host has been released. */
  RELEASED,

  /** mpv refused the load before the file was opened. */
  LOAD_FAILED,

  /**
   * The load was accepted but cancelled before mpv started the file: a
   * replacement superseded it or a stop ended it while it was still queued
   * or opening. Ordinary cancellation, not a user-visible error.
   */
  CANCELLED,
}

/** One-shot occurrences; continuous values live in [PlayerSnapshot]. */
sealed interface PlayerEvent {
  /** mpv finished opening the file; tracks and duration are now meaningful. */
  data class FileLoaded(val mediaId: String?, val generation: Long) : PlayerEvent

  /** Playback reached the natural end of the file (not a user stop). */
  data class NaturalEnd(val mediaId: String?, val generation: Long) : PlayerEvent

  /**
   * Terminal acknowledgement for a load generation: the file was fully
   * unloaded (stop, replacement, error, or release) and libmpv no longer
   * references any of its resources. Emitted exactly once per accepted load —
   * for loads still in flight this is deferred until the real END_FILE is
   * consumed, and on [PlayerHost.release] it is emitted only after native
   * teardown completes. Borrowed descriptors for [generation] may be closed
   * once this arrives.
   */
  data class PlaybackStopped(val mediaId: String?, val generation: Long) : PlayerEvent

  /**
   * A load never reached native playback: it was rejected at execution
   * (admission, release, mpv refusal) or cancelled before mpv started the
   * file ([RejectionReason.CANCELLED]). Terminal for [generation] — no
   * [PlaybackStopped] follows. Borrowed descriptors may be closed once this
   * arrives.
   */
  data class LoadRejected(
    val mediaId: String?,
    val generation: Long,
    val reason: RejectionReason,
  ) : PlayerEvent

  /** A command was refused at execution time. */
  data class CommandRejected(val command: String, val reason: RejectionReason) : PlayerEvent

  /** A native surface was attached and the render path is live. */
  data object SurfaceAttached : PlayerEvent

  /** The native surface was released; mpv no longer references it. */
  data object SurfaceDetached : PlayerEvent

  /**
   * An mpv log line, already redacted. [level] uses mpv log levels
   * (10 fatal, 20 error, 30 warn, 40 info, 50 verbose, 60 debug, 70 trace).
   */
  data class LogMessage(val prefix: String, val level: Int, val text: String) : PlayerEvent
}

/**
 * Immutable transport snapshot. All fields are observed facts from libmpv plus
 * the host's desired-state bookkeeping; nothing here is a business decision.
 *
 * [positionSeconds] advances only while [isPlaying]; consumers that need a
 * continuously moving position extrapolate from this value and [speed].
 */
data class PlayerSnapshot(
  val status: PlayerStatus = PlayerStatus.IDLE,
  /** Desired play state: the last admitted play/pause intent. */
  val playWhenReady: Boolean = false,
  /** Actually rendering/audible right now: playing, not paused, not buffering. */
  val isPlaying: Boolean = false,
  /** mpv `pause` property. Distinct from [playWhenReady] during admission denial. */
  val paused: Boolean = true,
  val positionSeconds: Double = 0.0,
  /** Stream duration; null when unknown or not seekable (live). */
  val durationSeconds: Double? = null,
  /** Demuxer cache end position; null when unknown. */
  val bufferedPositionSeconds: Double? = null,
  val volumePercent: Int = 100,
  val muted: Boolean = false,
  val speed: Double = 1.0,
  val tracks: List<PlayerTrack> = emptyList(),
  val videoWidth: Int = 0,
  val videoHeight: Int = 0,
  /** Correlation tokens of the currently loaded [MediaLoad]; null/0 when idle. */
  val mediaId: String? = null,
  val generation: Long = 0,
  /** Last admission value pushed via [PlayerHost.setAdmissionEligible]. */
  val admissionEligible: Boolean = true,
  /** Sticky failure for the current load; cleared by the next accepted load. */
  val error: PlayerError? = null,
)
