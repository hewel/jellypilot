package io.github.hewel.jellypilot.player

/**
 * Where the bytes of a media item come from.
 *
 * libmpv cannot open Android `content://` URIs. For SAF-picked media the caller
 * opens a [android.os.ParcelFileDescriptor] itself and hands the raw fd to the
 * player as [BorrowedFd]; mpv reads it through its `fd://` protocol, which does
 * not take ownership and never closes the descriptor.
 */
sealed interface MediaLocator {
  /** `http://` or `https://` stream. Credentials may be embedded; they are redacted from errors and logs. */
  data class Remote(val url: String) : MediaLocator

  /** Absolute filesystem path readable by the app process. */
  data class LocalFile(val path: String) : MediaLocator

  /**
   * Borrowed file descriptor, e.g. from `ContentResolver.openFileDescriptor`.
   * Played through mpv's `fd://<fd>` protocol, which does not take ownership
   * and never closes the descriptor. The caller retains ownership and MUST
   * keep the descriptor open until the load's terminal event —
   * [PlayerEvent.PlaybackStopped] or [PlayerEvent.LoadRejected] for the same
   * generation — arrives; mpv may read lazily at any point while loaded, and
   * a replacement load does not by itself end the outgoing load's use.
   */
  data class BorrowedFd(val fd: Int) : MediaLocator
}

/** An external subtitle source loaded alongside the main media. */
data class ExternalSubtitle(
  val locator: MediaLocator,
  /** Display name used in track listings; falls back to the file name when null. */
  val title: String? = null,
  /** BCP-47 language tag reported to track selection; null when unknown. */
  val language: String? = null,
)

/**
 * One media load request for [PlayerHost.load].
 * The locator must resolve to one FFmpeg media timeline (including HLS/DASH).
 * Native directory and M3U/PLS playlist expansion is refused; queue ownership
 * belongs to the business layer.
 *
 * [mediaId] and [generation] are opaque host-correlation tokens: they are echoed
 * back in [PlayerSnapshot] and [PlayerEvent]s so the business layer can reject
 * late observations from a superseded load. The player never interprets them.
 */
data class MediaLoad(
  val locator: MediaLocator,
  /** Business media identity echoed in snapshots/events; null when not applicable. */
  val mediaId: String? = null,
  /** Monotonic business generation echoed in snapshots/events; 0 when unused. */
  val generation: Long = 0,
  /** Start offset in seconds; null starts at the beginning (or mpv resume state). */
  val startPositionSeconds: Double? = null,
  /** Load paused: the file is prepared and the first frame shown without playing. */
  val startPaused: Boolean = false,
  /**
   * HTTP header lines applied to this load only (`Name` → `value`), sent for the
   * main URL and external subtitle URLs. Cleared when the load is replaced.
   * Values are treated as secret-bearing and redacted from logs and errors.
   */
  val httpHeaders: Map<String, String> = emptyMap(),
  /** External subtitles added with the load, in display order. */
  val externalSubtitles: List<ExternalSubtitle> = emptyList(),
)
