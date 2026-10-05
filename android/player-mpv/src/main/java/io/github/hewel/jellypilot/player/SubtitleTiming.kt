package io.github.hewel.jellypilot.player

/** Identifies one uninterrupted subtitle selection, including an A → B → A round trip. */
data class SubtitleTimingContext(val generation: Long, val trackId: Int, val revision: Long)

enum class SubtitleTimingAvailability { NO_TRACKS, SUBTITLES_OFF, UNSUPPORTED, AVAILABLE }

/** Offset is in tenths of a second; positive values delay the primary subtitle. */
data class SubtitleTimingState(
  val context: SubtitleTimingContext? = null,
  val offsetTenths: Int = 0,
  val availability: SubtitleTimingAvailability = SubtitleTimingAvailability.NO_TRACKS,
  val pending: Boolean = false,
  val failed: Boolean = false,
  /** Settled request sequence, including repeated failures when pending observations coalesce. */
  val requestRevision: Long = 0,
)

/** Executor-owned, media-local memory. Only an acknowledged native write changes an offset. */
internal class SubtitleTimingSession {
  var state = SubtitleTimingState()
    private set
  private var revision = 0L
  private val offsets = mutableMapOf<Int, Int>()

  fun clear() {
    offsets.clear()
    revision++
    state = SubtitleTimingState()
  }

  fun select(generation: Long, tracks: List<PlayerTrack>, apply: (Int) -> Boolean) {
    val subtitles = tracks.filter { it.kind == TrackKind.SUBTITLE }
    val selected = subtitles.firstOrNull { it.isSelected }
    if (selected == null) {
      state = SubtitleTimingState(availability = if (subtitles.isEmpty()) {
        SubtitleTimingAvailability.NO_TRACKS
      } else SubtitleTimingAvailability.SUBTITLES_OFF)
      return
    }
    val old = state.context
    if (old?.generation == generation && old.trackId == selected.mpvId) return
    val context = SubtitleTimingContext(generation, selected.mpvId, ++revision)
    val offset = offsets[selected.mpvId] ?: 0
    val supported = apply(offset)
    state = SubtitleTimingState(
      context = context,
      offsetTenths = if (supported) offset else 0,
      availability = if (supported) SubtitleTimingAvailability.AVAILABLE else SubtitleTimingAvailability.UNSUPPORTED,
    )
  }

  fun begin(context: SubtitleTimingContext): Boolean {
    if (state.context != context || state.pending || state.availability != SubtitleTimingAvailability.AVAILABLE) return false
    state = state.copy(pending = true, failed = false)
    return true
  }

  fun complete(context: SubtitleTimingContext, offsetTenths: Int, success: Boolean) {
    if (state.context != context || !state.pending) return
    if (success) offsets[context.trackId] = offsetTenths.coerceIn(-100, 100)
    state = state.copy(
      offsetTenths = if (success) offsets.getValue(context.trackId) else state.offsetTenths,
      pending = false,
      failed = !success,
      requestRevision = state.requestRevision + 1,
    )
  }

  fun cancel(context: SubtitleTimingContext) {
    if (state.context == context) {
      state = state.copy(pending = false, requestRevision = state.requestRevision + 1)
    }
  }
}
