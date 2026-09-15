package io.github.hewel.jellypilot.player

/**
 * A user/system playback intent surfaced by the Media3 adapter.
 *
 * Media3 commands (play, pause, seek, track selection, volume, release) are
 * translated into these intents and delivered to the business layer through
 * [PlayerIntentHandler]. The adapter never decides policy itself: the handler
 * decides whether and how to drive [PlayerHost], so system media controls,
 * local controls, and remote commands share one control path.
 */
sealed interface PlayerIntent {
  data object Play : PlayerIntent
  data object Pause : PlayerIntent
  data class SeekTo(val positionMs: Long) : PlayerIntent
  data object Stop : PlayerIntent

  /**
   * A Media3 track-selection request. [formatId] is the [androidx.media3.common.Format.id]
   * the adapter published for the track — for mpv tracks this is the mpv track id
   * as a decimal string; [FORMAT_ID_DISABLED] requests deselection of [kind].
   */
  data class SelectTrack(val kind: TrackKind, val formatId: String) : PlayerIntent

  data class SetVolume(val volume: Float) : PlayerIntent
  data class SetSpeed(val speed: Float) : PlayerIntent

  /** The Media3 player was released; the business layer should end its session. */
  data object Released : PlayerIntent

  companion object {
    /** [SelectTrack.formatId] value meaning "no track of this kind". */
    const val FORMAT_ID_DISABLED: String = "off"

    /** [SelectTrack.formatId] value meaning "let the player/business pick automatically". */
    const val FORMAT_ID_AUTO: String = "auto"
  }
}

/** Receives intents produced by the Media3 adapter. */
fun interface PlayerIntentHandler {
  /**
   * Handles [intent]; returns true when the business layer accepted it.
   * A rejected intent (e.g. play refused by audio focus or admission) lets
   * the adapter drop the optimistic state it would otherwise report.
   */
  fun onPlayerIntent(intent: PlayerIntent): Boolean
}
