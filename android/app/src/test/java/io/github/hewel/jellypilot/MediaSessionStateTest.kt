package io.github.hewel.jellypilot

import android.app.Application
import android.os.Looper
import android.view.Surface
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import io.github.hewel.jellypilot.player.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowLooper

/** MediaSession observes the adapter's public Player API, including asynchronous revocation. */
@UnstableApi
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class MediaSessionStateTest {
  @Test fun acceptedPlayIsReconciledWhenAdmissionIsRevoked() = withPlayer { host, player ->
    player.play()
    assertTrue(player.playWhenReady)
    host.publish(host.snapshot.copy(admissionEligible = false))
    assertFalse(player.playWhenReady)
    host.publish(host.snapshot.copy(admissionEligible = true))
    assertFalse("restoring eligibility must not revive the revoked intent", player.playWhenReady)
  }

  @Test fun pendingPlayAndSeekDoNotCarryIntoAnotherEpisode() = withPlayer { host, player ->
    player.play()
    player.seekTo(60_000)
    assertEquals(60_000, player.currentPosition)
    host.publish(host.snapshot.copy(generation = 2, mediaId = "episode-two", positionSeconds = 3.0))
    assertFalse(player.playWhenReady)
    assertEquals(3_000, player.currentPosition)
  }

  @Test fun terminalFailureClearsUnconfirmedTransportState() = withPlayer { host, player ->
    player.play()
    player.seekTo(60_000)
    host.publish(host.snapshot.copy(status = PlayerStatus.IDLE, positionSeconds = 0.0,
      error = PlayerError.PlaybackFailed("episode-one", "fixture failure")))
    assertFalse(player.playWhenReady)
    assertEquals(Player.STATE_IDLE, player.playbackState)
    assertEquals(0, player.currentPosition)
    assertNotNull(player.playerError)
  }

  @Test fun rejectedSeekImmediatelyRetainsObservedPosition() = withPlayer(accepted = false) { _, player ->
    player.seekTo(60_000)
    assertEquals(10_000, player.currentPosition)
  }

  @Test fun unavailableSeekAndVolumeCapabilitiesAreNotAdvertisedToSystemControls() = withPlayer { host, player ->
    assertTrue(player.availableCommands.contains(Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM))
    assertTrue(player.availableCommands.contains(Player.COMMAND_SET_VOLUME))
    host.publish(host.snapshot.copy(seekable = false, volumeAvailable = false))
    assertFalse(player.availableCommands.contains(Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM))
    assertFalse(player.availableCommands.contains(Player.COMMAND_SEEK_BACK))
    assertFalse(player.availableCommands.contains(Player.COMMAND_SEEK_FORWARD))
    assertFalse(player.availableCommands.contains(Player.COMMAND_SET_VOLUME))
    assertFalse(player.isCurrentMediaItemSeekable)
  }

  private fun withPlayer(accepted: Boolean = true, test: (SnapshotHost, MpvMedia3Player) -> Unit) {
    val host = SnapshotHost()
    val player = MpvMedia3Player(host, Looper.getMainLooper(), object : PlayerIntentHandler {
      override fun onPlayerIntent(intent: PlayerIntent) = accepted
    })
    ShadowLooper.idleMainLooper()
    try { test(host, player) } finally { player.release(); ShadowLooper.idleMainLooper() }
  }

  private class SnapshotHost : PlayerHost {
    private val listeners = mutableListOf<PlayerHost.Listener>()
    override var snapshot = PlayerSnapshot(status = PlayerStatus.READY, mediaId = "episode-one",
      generation = 1, positionSeconds = 10.0, durationSeconds = 120.0, seekable = true, volumeAvailable = true)
      private set
    fun publish(value: PlayerSnapshot) {
      snapshot = value
      listeners.toList().forEach { it.onSnapshot(value) }
      ShadowLooper.idleMainLooper()
    }
    override fun addListener(listener: PlayerHost.Listener) { listeners += listener; listener.onSnapshot(snapshot) }
    override fun removeListener(listener: PlayerHost.Listener) { listeners -= listener }
    override fun release() = Unit
    override fun setAdmissionEligible(eligible: Boolean) = error("business layer owns admission")
    override fun load(request: MediaLoad) = error("business layer owns playback")
    override fun play() = error("business layer owns playback")
    override fun pause() = error("business layer owns playback")
    override fun seekTo(positionSeconds: Double) = error("business layer owns playback")
    override fun stop(preserveSessionSettings: Boolean) = error("business layer owns playback")
    override fun setVolume(percent: Int) = error("business layer owns playback")
    override fun setMuted(muted: Boolean) = error("business layer owns playback")
    override fun setSpeed(speed: Double) = error("business layer owns playback")
    override fun setPictureBrightness(percent: Int) = error("business layer owns playback")
    override fun setSubtitleTiming(context: SubtitleTimingContext, offsetTenths: Int): Boolean = false
    override fun selectTrack(kind: TrackKind, mpvId: Int) = error("business layer owns playback")
    override fun setSurfaceSize(width: Int, height: Int) = Unit
    override fun attachSurface(surface: Surface) = Unit
    override fun detachSurface() = Unit
  }
}
