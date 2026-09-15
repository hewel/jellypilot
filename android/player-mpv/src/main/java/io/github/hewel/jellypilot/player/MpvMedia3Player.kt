package io.github.hewel.jellypilot.player

import android.os.Handler
import android.os.Looper
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.DeviceInfo
import androidx.media3.common.Format
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.PlaybackParameters
import androidx.media3.common.Player
import androidx.media3.common.SimpleBasePlayer
import androidx.media3.common.util.Size
import androidx.media3.common.TrackGroup
import androidx.media3.common.TrackSelectionParameters
import androidx.media3.common.Tracks
import androidx.media3.common.VideoSize
import androidx.media3.common.util.UnstableApi
import com.google.common.collect.ImmutableList
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture

/**
 * Media3 [Player] facade over a [PlayerHost].
 *
 * Exposes the host's observed facts (transport state, position, tracks, errors)
 * as a Media3 [SimpleBasePlayer.State] so a `MediaSession` and system media
 * controls see real player state. Every control command is translated into a
 * [PlayerIntent] and delivered to [intentHandler] — the business layer decides
 * what reaches the host, so system controls share the same admission and
 * control path as local and remote commands. The adapter never owns a queue,
 * reporting cadence, or playback policy.
 *
 * Video output plumbing (surface attach/detach) is mechanical and applied to
 * the host directly; [SurfaceView] and [SurfaceHolder] outputs are supported,
 * [android.view.TextureView] is rejected.
 *
 * All [Player] methods must be called on [looper]'s thread, per Media3.
 */
@UnstableApi
class MpvMedia3Player(
  private val host: PlayerHost,
  looper: Looper,
  private val intentHandler: PlayerIntentHandler,
) : SimpleBasePlayer(looper), PlayerHost.Listener {

  private val applicationHandler = Handler(looper)

  @Volatile
  private var snapshot: PlayerSnapshot = host.snapshot

  // Optimistic desired state until the host snapshot catches up.
  private var pendingPlayWhenReady: Boolean? = null
  private var pendingSeekPositionMs: Long? = null
  private var released = false

  // TrackGroup -> TrackKind for resolving selection overrides.
  private var groupKinds: Map<TrackGroup, TrackKind> = emptyMap()
  private var lastSelectionParameters = TrackSelectionParameters.DEFAULT

  private var boundHolder: SurfaceHolder? = null
  private val holderCallback = object : SurfaceHolder.Callback {
    override fun surfaceCreated(holder: SurfaceHolder) {
      host.attachSurface(holder.surface)
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
      host.setSurfaceSize(width, height)
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
      host.detachSurface()
    }
  }

  init {
    host.addListener(this)
  }

  // ------------------------------------------------------------------
  // PlayerHost.Listener (host executor thread -> post to app looper)
  // ------------------------------------------------------------------

  override fun onSnapshot(snapshot: PlayerSnapshot) {
    applicationHandler.post {
      this.snapshot = snapshot
      if (pendingPlayWhenReady == snapshot.playWhenReady) {
        pendingPlayWhenReady = null
      }
      pendingSeekPositionMs?.let { pending ->
        val observedMs = (snapshot.positionSeconds * 1000).toLong()
        if (kotlin.math.abs(observedMs - pending) <= SEEK_TOLERANCE_MS) {
          pendingSeekPositionMs = null
        }
      }
      invalidateState()
    }
  }
  override fun onEvent(event: PlayerEvent) {
    when (event) {
      is PlayerEvent.CommandRejected -> applicationHandler.post {
        // A refused command must not leave optimistic state stuck.
        when (event.command) {
          "play", "pause" -> pendingPlayWhenReady = null
          "seek" -> pendingSeekPositionMs = null
        }
        invalidateState()
      }
      else -> Unit
    }
  }

  // ------------------------------------------------------------------
  // SimpleBasePlayer
  // ------------------------------------------------------------------

  override fun getState(): State {
    val s = snapshot
    val loaded = s.status != PlayerStatus.IDLE
    val playWhenReady = pendingPlayWhenReady ?: s.playWhenReady

    val commands = Player.Commands.Builder()
      .add(Player.COMMAND_PLAY_PAUSE)
      .add(Player.COMMAND_PREPARE)
      .add(Player.COMMAND_STOP)
      .add(Player.COMMAND_SET_VOLUME)
      .add(Player.COMMAND_GET_VOLUME)
      .add(Player.COMMAND_SET_SPEED_AND_PITCH)
      .add(Player.COMMAND_SET_VIDEO_SURFACE)
      .add(Player.COMMAND_GET_TRACKS)
      .add(Player.COMMAND_GET_CURRENT_MEDIA_ITEM)
      .add(Player.COMMAND_GET_METADATA)
      .add(Player.COMMAND_GET_AUDIO_ATTRIBUTES)
      .add(Player.COMMAND_RELEASE)
      .addIf(Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM, loaded)
      .addIf(Player.COMMAND_SEEK_BACK, loaded)
      .addIf(Player.COMMAND_SEEK_FORWARD, loaded)
      .addIf(Player.COMMAND_SEEK_TO_DEFAULT_POSITION, loaded)
      .addIf(Player.COMMAND_SET_TRACK_SELECTION_PARAMETERS, loaded)
      .build()

    val playbackState = when (s.status) {
      PlayerStatus.IDLE -> Player.STATE_IDLE
      PlayerStatus.LOADING, PlayerStatus.BUFFERING -> Player.STATE_BUFFERING
      PlayerStatus.READY -> Player.STATE_READY
      PlayerStatus.ENDED -> Player.STATE_ENDED
    }

    val positionMs = pendingSeekPositionMs ?: (s.positionSeconds * 1000).toLong()
    val positionSupplier = if (s.isPlaying) {
      PositionSupplier.getExtrapolating(positionMs, s.speed.toFloat())
    } else {
      PositionSupplier.getConstant(positionMs)
    }
    val bufferedEndMs = s.bufferedPositionSeconds?.let { (it * 1000).toLong() }
    val bufferedSupplier = bufferedEndMs?.let { PositionSupplier.getConstant(it) }
      ?: PositionSupplier.getConstant(if (loaded) positionMs else C.TIME_UNSET)

    val tracks = buildTracks(s.tracks)
    val durationUs = s.durationSeconds?.let { (it * 1_000_000).toLong() } ?: C.TIME_UNSET

    val builder = State.Builder()
      .setAvailableCommands(commands)
      .setPlayWhenReady(playWhenReady, Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST)
      .setPlaybackState(playbackState)
      .setPlaybackSuppressionReason(Player.PLAYBACK_SUPPRESSION_REASON_NONE)
      .setIsLoading(s.status == PlayerStatus.LOADING || s.status == PlayerStatus.BUFFERING)
      .setPlaybackParameters(PlaybackParameters(s.speed.toFloat()))
      .setAudioAttributes(AudioAttributes.DEFAULT)
      .setVideoSize(
        if (s.videoWidth > 0 && s.videoHeight > 0) {
          VideoSize(s.videoWidth, s.videoHeight)
        } else {
          VideoSize.UNKNOWN
        },
      )
      .setSurfaceSize(Size.UNKNOWN)
      .setContentPositionMs(positionSupplier)
      .setContentBufferedPositionMs(bufferedSupplier)
      .setTotalBufferedDurationMs(totalBufferedDuration(positionSupplier, bufferedEndMs))
      .setPlaylistMetadata(MediaMetadata.EMPTY)
      .setDeviceInfo(DeviceInfo.UNKNOWN)
      .setTrackSelectionParameters(lastSelectionParameters)

    s.error?.let { error ->
      builder.setPlayerError(
        PlaybackException(error.message, null, PlaybackException.ERROR_CODE_UNSPECIFIED),
      )
    }

    if (loaded) {
      val mediaItem = MediaItem.Builder()
        .setMediaId(s.mediaId ?: "mpv-media")
        .build()
      val itemData = MediaItemData.Builder(MEDIA_ITEM_UID)
        .setMediaItem(mediaItem)
        .setTracks(tracks)
        .setDurationUs(durationUs)
        .setIsSeekable(s.durationSeconds != null)
        .setIsDynamic(false)
        .build()
      builder.setPlaylist(ImmutableList.of(itemData))
      builder.setCurrentMediaItemIndex(0)
    }

    return builder.build()
  }

  override fun handleSetPlayWhenReady(playWhenReady: Boolean): ListenableFuture<*> {
    val accepted =
      intentHandler.onPlayerIntent(if (playWhenReady) PlayerIntent.Play else PlayerIntent.Pause)
    // Optimistic desired state only while the business layer accepted the
    // intent; a refusal reconciles immediately to the actual host state.
    pendingPlayWhenReady = if (accepted) playWhenReady else null
    return Futures.immediateVoidFuture()
  }

  override fun handlePrepare(): ListenableFuture<*> = Futures.immediateVoidFuture()
  override fun handleStop(): ListenableFuture<*> {
    val accepted = intentHandler.onPlayerIntent(PlayerIntent.Stop)
    pendingPlayWhenReady = if (accepted) false else null
    return Futures.immediateVoidFuture()
  }

  override fun handleRelease(): ListenableFuture<*> {
    if (!released) {
      released = true
      intentHandler.onPlayerIntent(PlayerIntent.Released)
      host.removeListener(this)
      unbindHolder()
      host.release()
    }
    return Futures.immediateVoidFuture()
  }

  override fun handleSeek(
    mediaItemIndex: Int,
    positionMs: Long,
    seekCommand: Int,
  ): ListenableFuture<*> {
    pendingSeekPositionMs = positionMs
    intentHandler.onPlayerIntent(PlayerIntent.SeekTo(positionMs))
    return Futures.immediateVoidFuture()
  }

  override fun handleSetVolume(volume: Float, volumeOperationType: Int): ListenableFuture<*> {
    intentHandler.onPlayerIntent(PlayerIntent.SetVolume(volume))
    return Futures.immediateVoidFuture()
  }

  override fun handleSetPlaybackParameters(
    playbackParameters: PlaybackParameters,
  ): ListenableFuture<*> {
    intentHandler.onPlayerIntent(PlayerIntent.SetSpeed(playbackParameters.speed))
    return Futures.immediateVoidFuture()
  }

  override fun handleSetTrackSelectionParameters(
    parameters: TrackSelectionParameters,
  ): ListenableFuture<*> {
    // Emit intents for the new selection state; the business layer decides
    // which mpv track ids these map to.
    for ((group, selectionOverride) in parameters.overrides) {
      val kind = groupKinds[group] ?: groupKindFromType(group.type) ?: continue
      val formatId = if (selectionOverride.trackIndices.isEmpty()) {
        PlayerIntent.FORMAT_ID_DISABLED
      } else {
        group.getFormat(selectionOverride.trackIndices[0]).id ?: continue
      }
      intentHandler.onPlayerIntent(PlayerIntent.SelectTrack(kind, formatId))
    }
    for (disabledType in parameters.disabledTrackTypes) {
      trackKindFromTrackType(disabledType)?.let { kind ->
        intentHandler.onPlayerIntent(
          PlayerIntent.SelectTrack(kind, PlayerIntent.FORMAT_ID_DISABLED),
        )
      }
    }
    // A kind that had an override and now has none returns to auto selection.
    for ((group, kind) in groupKinds) {
      if (!parameters.overrides.containsKey(group) &&
        lastSelectionParameters.overrides.containsKey(group) &&
        group.type !in parameters.disabledTrackTypes
      ) {
        intentHandler.onPlayerIntent(
          PlayerIntent.SelectTrack(kind, PlayerIntent.FORMAT_ID_AUTO),
        )
      }
    }
    lastSelectionParameters = parameters
    return Futures.immediateVoidFuture()
  }

  override fun handleSetVideoOutput(videoOutput: Any): ListenableFuture<*> {
    when (videoOutput) {
      is Surface -> host.attachSurface(videoOutput)
      is SurfaceHolder -> bindHolder(videoOutput)
      is SurfaceView -> bindHolder(videoOutput.holder)
      else -> return Futures.immediateFailedFuture<Any>(
        IllegalArgumentException("Unsupported video output: ${videoOutput.javaClass.name}"),
      )
    }
    return Futures.immediateVoidFuture()
  }

  override fun handleClearVideoOutput(videoOutput: Any?): ListenableFuture<*> {
    unbindHolder()
    host.detachSurface()
    return Futures.immediateVoidFuture()
  }

  // ------------------------------------------------------------------
  // Internals
  // ------------------------------------------------------------------

  private fun bindHolder(holder: SurfaceHolder) {
    unbindHolder()
    boundHolder = holder
    holder.addCallback(holderCallback)
    holder.surface?.takeIf { it.isValid }?.let { host.attachSurface(it) }
  }

  private fun unbindHolder() {
    boundHolder?.removeCallback(holderCallback)
    boundHolder = null
  }

  private fun buildTracks(tracks: List<PlayerTrack>): Tracks {
    if (tracks.isEmpty()) {
      groupKinds = emptyMap()
      return Tracks.EMPTY
    }
    val groups = mutableListOf<Tracks.Group>()
    val kinds = mutableMapOf<TrackGroup, TrackKind>()
    for (kind in TrackKind.entries) {
      val ofKind = tracks.filter { it.kind == kind }
      if (ofKind.isEmpty()) continue
      val formats = ofKind.map { it.toFormat() }.toTypedArray()
      val group = TrackGroup(*formats)
      val selected = BooleanArray(ofKind.size) { ofKind[it].isSelected }
      groups += Tracks.Group(group, /* adaptiveSupported= */ false, handledTrackSupport(ofKind.size), selected)
      kinds[group] = kind
    }
    groupKinds = kinds
    return Tracks(groups)
  }

  // Preserve Media3's IntDef across Kotlin's dynamically constructed array boundary.
  @C.FormatSupport
  private fun handledTrackSupport(count: Int): IntArray = IntArray(count) { C.FORMAT_HANDLED }

  private fun PlayerTrack.toFormat(): Format {
    val builder = Format.Builder()
      .setId(mpvId.toString())
      .setLabel(title ?: language ?: "${kind.name.lowercase()} $mpvId")
      .setLanguage(language)
      .setSelectionFlags(
        (if (isDefault) C.SELECTION_FLAG_DEFAULT else 0) or
          (if (isForced) C.SELECTION_FLAG_FORCED else 0),
      )
      .setSampleMimeType(sampleMimeType(kind, codec))
    return builder.build()
  }

  private fun sampleMimeType(kind: TrackKind, codec: String?): String = when (kind) {
    TrackKind.VIDEO -> when (codec) {
      "h264", "avc", "avc1" -> MimeTypes.VIDEO_H264
      "hevc", "h265", "hvc1", "hev1" -> MimeTypes.VIDEO_H265
      "av1", "av01" -> MimeTypes.VIDEO_AV1
      "vp9" -> MimeTypes.VIDEO_VP9
      "vp8" -> MimeTypes.VIDEO_VP8
      "mpeg2video", "mpeg2" -> MimeTypes.VIDEO_MPEG2
      else -> "video/x-unknown"
    }

    TrackKind.AUDIO -> when (codec) {
      "aac" -> MimeTypes.AUDIO_AAC
      "ac3" -> MimeTypes.AUDIO_AC3
      "eac3", "ec-3" -> MimeTypes.AUDIO_E_AC3
      "truehd" -> MimeTypes.AUDIO_TRUEHD
      "dts" -> MimeTypes.AUDIO_DTS
      "flac" -> MimeTypes.AUDIO_FLAC
      "opus" -> MimeTypes.AUDIO_OPUS
      "mp3", "mp3float" -> MimeTypes.AUDIO_MPEG
      "vorbis" -> MimeTypes.AUDIO_VORBIS
      else -> "audio/x-unknown"
    }

    TrackKind.SUBTITLE -> when (codec) {
      "ass", "ssa" -> MimeTypes.TEXT_SSA
      "srt", "subrip" -> MimeTypes.APPLICATION_SUBRIP
      "webvtt", "vtt" -> MimeTypes.TEXT_VTT
      "pgs", "hdmv_pgs_subtitle" -> MimeTypes.APPLICATION_PGS
      "dvd_subtitle", "vobsub" -> MimeTypes.APPLICATION_VOBSUB
      else -> "text/x-unknown"
    }
  }

  private fun groupKindFromType(trackType: Int): TrackKind? = trackKindFromTrackType(trackType)

  private fun trackKindFromTrackType(trackType: Int): TrackKind? = when (trackType) {
    C.TRACK_TYPE_VIDEO -> TrackKind.VIDEO
    C.TRACK_TYPE_AUDIO -> TrackKind.AUDIO
    C.TRACK_TYPE_TEXT -> TrackKind.SUBTITLE
    else -> null
  }

  private companion object {
    const val MEDIA_ITEM_UID = "mpv-media-item"
    const val SEEK_TOLERANCE_MS = 1500L
  }
}

/**
 * Total buffered duration is the buffered range ahead of the current
 * position — never the absolute buffered end position, and never negative or
 * unset. Unknown buffering reports zero.
 */
@UnstableApi
internal fun totalBufferedDuration(
  position: SimpleBasePlayer.PositionSupplier,
  bufferedEndMs: Long?,
): SimpleBasePlayer.PositionSupplier =
  bufferedEndMs?.let { end ->
    SimpleBasePlayer.PositionSupplier { (end - position.get()).coerceAtLeast(0L) }
  } ?: SimpleBasePlayer.PositionSupplier.ZERO
