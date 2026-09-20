package io.github.hewel.jellypilot

import io.github.hewel.jellypilot.ffi.*
import io.github.hewel.jellypilot.player.ExternalSubtitle as NativeSubtitle
import io.github.hewel.jellypilot.player.*
import io.github.hewel.jellypilot.ui.PlaybackUi
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** One app-owned bridge between SDK business sessions and physically acknowledged native operations. */
internal class MediaPlaybackCoordinator(
  private val sdk: JellypilotSdk,
  private val player: NativePlayback,
  private val scope: CoroutineScope,
  private val onPlaybackUi: (PlaybackUi?) -> Unit,
  private val onError: (String) -> Unit,
  private val onOpenPlayer: () -> Unit,
  private val onRecoveryChanged: () -> Unit = {},
) : AutoCloseable {
  private class Playing(val session: PlaybackSession, val plan: PlaybackPlan, val profile: ProfileScopeRef, val restoring: Boolean) {
    val generation = AtomicLong(-1)
    val reports = Mutex()
    var sequence = 0uL
    var loaded = false
    var finishing = false
    var latest = PlayerSnapshot()
    var undoPosition: Double? = null
    var previousId: String? = null
    var nextId: String? = null
    var seasonNumber: Int? = null
    var queueOffset = 0
    var ui = PlaybackUi(title = plan.title, currentItemId = plan.itemId)
    val pendingTracks = mutableMapOf<TrackKind, Int>()
  }

  private val eligible = AtomicBoolean(false)
  private val intent = AtomicLong()
  private val transitions = Mutex()
  @Volatile private var current: Playing? = null
  @Volatile private var preparing: OperationToken? = null
  private var remoteJob: Job? = null
  @Volatile private var remoteEpoch = 0L
  private var undoSequence = 0L
  @Volatile private var closed = false
  var reportingError = false
    private set

  private val eventsJob = scope.launch {
    player.events.collect { event ->
      val entry = current ?: return@collect
      when (event) {
        is PlayerEvent.NaturalEnd -> if (event.generation == entry.generation.get() && !entry.finishing) {
          // An old EOF never creates a newer intent than a user-requested replacement/stop.
          val command = intent.get()
          launch { transitions.withLock {
            if (current !== entry || entry.finishing || intent.get() != command) return@withLock
            val next = finishCurrent(true)
            if (next != null && canStart(command) && eligible.get()) start(next, PlaybackStartPosition.Beginning, null, command) { true }
          } }
        }
        is PlayerEvent.PlaybackStopped -> if (event.generation == entry.generation.get() && !entry.finishing && entry.loaded) {
          launch { transitions.withLock { if (current === entry && !entry.finishing) finishCurrent(false, interrupted = true) } }
        }
        else -> Unit
      }
    }
  }
  private val observationJob = scope.launch {
    while (isActive) {
      delay(1_000)
      current?.let { observe(it, false) }
    }
  }
  private val snapshotsJob = scope.launch {
    player.snapshot.collect { snapshot ->
      current?.takeIf { it.generation.get() == snapshot.generation && snapshot.status != PlayerStatus.IDLE }
        ?.let { it.latest = snapshot }
    }
  }

  init {
    player.businessIntent = { command ->
      val entry = current
      if (entry == null) false else when (command) {
        PlayerIntent.Play -> if (entry.loaded && !entry.finishing && eligible.get() && entry.session.isActive()) { playPlayback(); true } else false
        PlayerIntent.Pause -> { pausePlayback(); true }
        PlayerIntent.Stop, PlayerIntent.Released -> { stop(); true }
        is PlayerIntent.SeekTo -> { seek(command.positionMs / 1000.0); true }
        is PlayerIntent.SetVolume -> { volume((command.volume * 100).toInt()); true }
        is PlayerIntent.SetSpeed -> { player.speed(command.speed.toDouble()); true }
        is PlayerIntent.SelectTrack -> {
          val index = when (command.formatId) {
            PlayerIntent.FORMAT_ID_AUTO -> PlayerHost.TRACK_ID_AUTO
            PlayerIntent.FORMAT_ID_DISABLED -> PlayerHost.TRACK_ID_NONE
            else -> command.formatId.toIntOrNull()
          }
          if (index == null) false else { selectTrack(command.kind, index); true }
        }
      }
    }
  }

  fun play(itemId: String, fromBeginning: Boolean = false, selection: PlaybackSelection? = null) {
    requestStart(itemId, if (fromBeginning) PlaybackStartPosition.Beginning else PlaybackStartPosition.Resume, selection) { true }
  }

  private fun requestStart(itemId: String, position: PlaybackStartPosition, selection: PlaybackSelection?, originCurrent: () -> Boolean) {
    val command = intent.incrementAndGet()
    preparing?.cancel()
    scope.launch {
      transitions.withLock {
        if (!canStart(command) || !originCurrent()) return@withLock
        start(itemId, position, selection, command, originCurrent = originCurrent)
      }
    }
  }

  private fun canStart(command: Long): Boolean = !closed && eligible.get() && intent.get() == command && !sdk.contentMutationsBlocked()

  private suspend fun start(itemId: String, position: PlaybackStartPosition, selection: PlaybackSelection?, command: Long, restoring: Boolean = false, originCurrent: () -> Boolean) {
    var token: OperationToken? = null
    var created: Playing? = null
    try {
      if (!canStart(command) || !originCurrent()) return
      if (current != null) {
        finishCurrent(false)
        if (current != null) return
      } else if (!player.stopAndWait()) {
        fail(); return
      }
      if (!canStart(command) || !originCurrent()) return
      token = sdk.newOperationToken()
      preparing = token
      val session = sdk.preparePlayback(token, itemId, position, selection ?: PlaybackSelection(null, null, null))
      val plan = session.plan()
      val entry = Playing(session, plan, token.scopeRef(), restoring)
      created = entry
      current = entry
      val autoSkip = sdk.businessPreferences().introMode == IntroSkipMode.AUTOMATIC
      entry.ui = entry.ui.copy(autoSkipAvailable = sdk.activeProfile()?.capabilities?.introSkipper == true && plan.itemType == "Episode", autoSkipEnabled = autoSkip)
      onPlaybackUi(entry.ui)
      onOpenPlayer()
      val accepted = withContext(NonCancellable) { session.runAdmitted(object : PlaybackHostOperation {
        override suspend fun execute(): Boolean {
          if (!canStart(command) || !originCurrent() || current !== entry) return false
          var accepted = false
          return try {
            player.loadMedia(
              MediaLoad(
                locator = MediaLocator.Remote(plan.streamUrl.value()),
                mediaId = plan.itemId,
                startPositionSeconds = plan.startPositionSeconds,
                startPaused = true,
                externalSubtitles = plan.externalSubtitles.map { NativeSubtitle(MediaLocator.Remote(it.url.value()), it.title, it.language) },
              ),
              stillCurrent = { canStart(command) && originCurrent() && current === entry },
              bindGeneration = entry.generation::set,
            )
            val stillCurrent = { canStart(command) && originCurrent() && current === entry }
            val externalIndex = plan.externalSubtitles.indexOfFirst { it.providerIndex == plan.subtitleStreamIndex }
            if (externalIndex >= 0) withTimeout(10_000) {
              player.snapshot.first { it.generation != entry.generation.get() || !stillCurrent() || it.error != null ||
                it.tracks.any { track -> track.kind == TrackKind.SUBTITLE && track.externalSourceIndex == externalIndex } }
            }
            val audio = plan.mpvAudioIndex?.toInt()
            val subtitle = plan.mpvSubtitleIndex?.toInt() ?: plan.subtitleStreamIndex?.let { providerToPlayer(entry, TrackKind.SUBTITLE, it) }
            if (plan.subtitleStreamIndex != null && subtitle == null) return false
            if (!player.configureMedia(plan.initialVolume?.toInt(), audio, subtitle, stillCurrent)) return false
            selection?.audioStreamIndex?.let { session.rememberTrack("Audio", it) }
            selection?.subtitleStreamIndex?.let { session.rememberTrack("Subtitle", it) }
            accepted = player.resumeMedia(stillCurrent)
            accepted
          } catch (_: Exception) { false }
          finally { if (!accepted) player.retireMedia() }
        }
      }) }
      if (!accepted || !canStart(command) || !originCurrent()) {
        finishCurrent(false, interrupted = entry.restoring)
        if (canStart(command)) fail()
        return
      }
      entry.loaded = true
      entry.latest = player.snapshot.value
      scope.launch { observe(entry, true) }
      scope.launch { loadEpisodeContext(entry) }
      onRecoveryChanged()
    } catch (cancelled: CancellationException) {
      withContext(NonCancellable) { if (current === created && created != null) finishCurrent(false, interrupted = created.restoring) }
      throw cancelled
    } catch (_: Exception) {
      if (current === created && created != null) finishCurrent(false, interrupted = created.restoring)
      if (canStart(command)) fail()
    } finally {
      if (preparing === token) preparing = null
      token?.cancel()
      token?.destroy()
    }
  }

  fun restoreRecovery() {
    val command = intent.incrementAndGet()
    scope.launch { transitions.withLock {
      if (!canStart(command)) return@withLock
      val token = try { sdk.newOperationToken() } catch (_: Exception) { fail(); return@withLock }
      try {
        val point = sdk.localPlaybackRecovery(token) ?: return@withLock
        if (canStart(command)) start(point.itemId, PlaybackStartPosition.At(point.positionSeconds), null, command, restoring = true) { true }
      } catch (_: Exception) { fail() }
      finally { token.cancel(); token.destroy() }
    } }
  }

  fun stop() {
    intent.incrementAndGet()
    preparing?.cancel()
    current?.finishing = true
    player.pause()
    scope.launch { transitions.withLock { finishCurrent(false) } }
  }

  private suspend fun finishCurrent(natural: Boolean, interrupted: Boolean = false): String? {
    val entry = current ?: run { player.stopAndWait(); return null }
    entry.finishing = true
    val snapshot = player.snapshot.value
    if (snapshot.generation == entry.generation.get() && snapshot.status != PlayerStatus.IDLE) entry.latest = snapshot
    if (!player.stopAndWait()) { entry.finishing = false; fail(); return null }
    var next: String? = null
    try {
      entry.reports.withLock {
        val finalObservation = observation(entry, entry.latest)
        val finish = if (interrupted) entry.session.interrupt(finalObservation) else entry.session.finish(finalObservation, natural)
        reportFailure(finish.reportError != null)
        next = finish.nextItemId
      }
    } catch (_: Exception) { fail() }
    finally {
      if (current === entry) { current = null; onPlaybackUi(null) }
      entry.plan.destroy()
      entry.session.destroy()
      onRecoveryChanged()
    }
    return next
  }

  fun previous() = adjacent(false)
  fun next() = adjacent(true)
  private fun adjacent(next: Boolean, originCurrent: () -> Boolean = { true }) {
    val entry = current ?: return
    val command = intent.incrementAndGet()
    scope.launch { transitions.withLock {
      if (!canStart(command) || current !== entry || !originCurrent()) return@withLock
      try {
        val item = entry.session.adjacent(next) ?: return@withLock
        if (canStart(command) && current === entry && originCurrent()) start(item, PlaybackStartPosition.Beginning, null, command, originCurrent = originCurrent)
      } catch (_: Exception) { if (canStart(command)) fail() }
    } }
  }

  fun playPlayback() = resume { true }
  private fun resume(originCurrent: () -> Boolean) {
    val entry = current ?: return
    if (!entry.loaded || entry.finishing) return
    val command = intent.get()
    scope.launch { transitions.withLock {
      if (!canStart(command) || current !== entry || entry.finishing || !originCurrent()) return@withLock
      try {
        val accepted = withContext(NonCancellable) { entry.session.runAdmitted(object : PlaybackHostOperation {
          override suspend fun execute(): Boolean = player.resumeMedia { canStart(command) && current === entry && originCurrent() }
        }) }
        if (!accepted && canStart(command) && originCurrent()) fail()
      } catch (_: Exception) { if (canStart(command) && originCurrent()) fail() }
    } }
  }

  fun pausePlayback() {
    intent.incrementAndGet()
    preparing?.cancel()
    player.pause()
    current?.let { entry -> scope.launch { observe(entry, true) } }
  }
  fun seek(seconds: Double) { if (seconds.isFinite() && seconds >= 0 && current?.let { it.loaded && !it.finishing && it.session.isActive() } == true) player.seek(seconds) }
  fun volume(percent: Int) { if (current?.let { it.loaded && !it.finishing && it.session.isActive() } == true) player.volume(percent.coerceIn(0, 100)) }
  fun selectTrack(kind: TrackKind, id: Int) {
    val entry = current ?: return
    if (!entry.loaded || !entry.session.isActive()) return
    if (id >= 0 && player.snapshot.value.tracks.none { it.kind == kind && it.mpvId == id }) return
    if (kind != TrackKind.VIDEO) entry.pendingTracks[kind] = id
    player.select(kind, id)
  }

  fun sessionAutoSkip(enabled: Boolean) {
    val entry = current ?: return
    try {
      entry.session.setIntroMode(if (enabled) IntroSkipMode.AUTOMATIC else IntroSkipMode.MANUAL)
      publish(entry) { copy(autoSkipEnabled = enabled, manualSkipLabel = null) }
    } catch (_: Exception) { fail() }
  }
  fun acknowledgeSkipPrompt(presented: Boolean) {
    current?.let { runCatching { it.session.acknowledgeSkipPrompt(presented) } }
  }
  fun skip() {
    val entry = current ?: return
    try { entry.session.skipIntro()?.let { player.seek(it); publish(entry) { copy(manualSkipLabel = null) } } }
    catch (_: Exception) { fail() }
  }
  fun undoSkip() {
    val entry = current ?: return
    val position = entry.undoPosition ?: return
    try {
      entry.session.setIntroMode(IntroSkipMode.MANUAL)
      player.seek(position)
      entry.undoPosition = null
      publish(entry) { copy(autoSkipEnabled = false, skipUndoAvailable = false) }
    } catch (_: Exception) { fail() }
  }
  fun dismissSkipUndo() {
    val entry = current ?: return
    entry.undoPosition = null
    publish(entry) { copy(skipUndoAvailable = false) }
  }

  private suspend fun observe(entry: Playing, force: Boolean) {
    entry.reports.withLock {
      if (current !== entry || !entry.loaded || entry.finishing) return@withLock
      val snapshot = player.snapshot.value
      if (snapshot.generation != entry.generation.get() || snapshot.status == PlayerStatus.IDLE) return@withLock
      entry.latest = snapshot
      try {
        settleTracks(entry, snapshot)
        val update = entry.session.observe(observation(entry, snapshot), force)
        if (current !== entry || entry.finishing) return@withLock
        if (update.reportError != null) reportFailure(true) else if (update.reported) reportingError = false
        if (eligible.get() && entry.session.isActive()) {
          update.seekTo?.let {
            entry.undoPosition = snapshot.positionSeconds
            player.seek(it)
            publish(entry) { copy(skipUndoAvailable = true, skipUndoId = ++undoSequence) }
          }
          val label = update.skipPrompt?.let { player.message(if (it.kind == IntroSkipKind.CREDITS) R.string.skip_credits else R.string.skip_intro) }
          publish(entry) { copy(manualSkipLabel = label) }
        }
      } catch (_: Exception) { if (current === entry && !entry.finishing && entry.session.isActive()) fail() }
    }
  }

  private fun observation(entry: Playing, snapshot: PlayerSnapshot): PlaybackObservation {
    entry.sequence += 1uL
    return PlaybackObservation(entry.sequence,
      (if (entry.loaded) snapshot.positionSeconds else entry.plan.startPositionSeconds).coerceAtLeast(0.0),
      snapshot.paused, snapshot.muted, snapshot.volumePercent.toDouble(),
      selectedProvider(entry, snapshot, TrackKind.AUDIO), selectedProvider(entry, snapshot, TrackKind.SUBTITLE))
  }

  private fun selectedProvider(entry: Playing, snapshot: PlayerSnapshot, kind: TrackKind): Int? {
    val track = snapshot.tracks.firstOrNull { it.kind == kind && it.isSelected }
    if (track == null) return if (kind == TrackKind.SUBTITLE) -1 else entry.plan.audioStreamIndex
    return playerToProvider(entry, kind, track.mpvId)
  }

  private fun playerToProvider(entry: Playing, kind: TrackKind, id: Int): Int? {
    if (id == PlayerHost.TRACK_ID_NONE) return -1
    val name = if (kind == TrackKind.AUDIO) "Audio" else "Subtitle"
    entry.plan.tracks.firstOrNull { it.kind == name && !it.external && it.playerIndex == id.toLong() }?.let { return it.providerIndex }
    val externalIndex = player.snapshot.value.tracks.firstOrNull { it.kind == kind && it.mpvId == id }?.externalSourceIndex ?: return null
    return entry.plan.externalSubtitles.getOrNull(externalIndex)?.providerIndex
  }

  private fun providerToPlayer(entry: Playing, kind: TrackKind, index: Int): Int? {
    if (index == -1) return PlayerHost.TRACK_ID_NONE
    val name = if (kind == TrackKind.AUDIO) "Audio" else "Subtitle"
    entry.plan.tracks.firstOrNull { it.kind == name && it.providerIndex == index && !it.external }?.let { return it.playerIndex?.toInt() }
    val externalIndex = entry.plan.externalSubtitles.indexOfFirst { it.providerIndex == index }
    return player.snapshot.value.tracks.firstOrNull { it.kind == kind && it.externalSourceIndex == externalIndex }?.mpvId
  }

  private fun settleTracks(entry: Playing, snapshot: PlayerSnapshot) {
    val iterator = entry.pendingTracks.iterator()
    while (iterator.hasNext()) {
      val (kind, id) = iterator.next()
      val selected = snapshot.tracks.firstOrNull { it.kind == kind && it.isSelected }
      val settled = when (id) {
        PlayerHost.TRACK_ID_NONE -> selected == null
        PlayerHost.TRACK_ID_AUTO -> selected != null
        else -> selected?.mpvId == id
      }
      if (settled) {
        val provider = playerToProvider(entry, kind, if (id == PlayerHost.TRACK_ID_AUTO) selected!!.mpvId else id)
        if (provider != null) entry.session.rememberTrack(if (kind == TrackKind.AUDIO) "Audio" else "Subtitle", provider)
        iterator.remove()
      }
    }
  }

  private suspend fun loadEpisodeContext(entry: Playing) {
    if (entry.plan.itemType != "Episode") return
    val token = try { sdk.newOperationToken() } catch (_: Exception) { return }
    try {
      val detail = sdk.itemDetail(token, entry.plan.itemId)
      if (current !== entry || entry.finishing) return
      entry.seasonNumber = detail.seasonNumber
      entry.previousId = entry.session.adjacent(false)
      entry.nextId = entry.session.adjacent(true)
      val episode = CatalogPresentation(entry.profile, emptySet()).item(detail)
      publish(entry) { copy(title = detail.seriesName?.takeIf { it.isNotBlank() } ?: entry.plan.title,
        episodeLabel = listOfNotNull(episode.episodeCode, detail.name).joinToString(" · "),
        canPrevious = entry.previousId != null, canNext = entry.nextId != null, queueHasMore = true) }
      loadQueue(entry)
    } catch (_: Exception) { if (current === entry) fail() }
    finally { token.cancel(); token.destroy() }
  }

  fun loadMoreQueue() { current?.let { entry -> scope.launch { loadQueue(entry) } } }
  private suspend fun loadQueue(entry: Playing) {
    if (current !== entry || entry.ui.queueLoading || !entry.ui.queueHasMore) return
    val series = entry.plan.seriesId ?: return
    val token = try { sdk.newOperationToken() } catch (_: Exception) { return }
    publish(entry) { copy(queueLoading = true) }
    try {
      val page = sdk.seasonEpisodesPage(token, VideoSeasonEpisodesPageRequest(series, null, entry.seasonNumber, entry.queueOffset, 60))
      if (current !== entry || entry.finishing) return
      entry.queueOffset = page.nextStartIndex
      val presentation = CatalogPresentation(entry.profile, emptySet())
      val items = page.episodes.map { presentation.library(it).copy(metadata = "") }
      publish(entry) { copy(queue = (queue + items).distinctBy { it.id }, queueHasMore = page.hasMore, queueLoading = false) }
    } catch (_: Exception) { if (current === entry) fail() }
    finally { token.cancel(); token.destroy(); publish(entry) { copy(queueLoading = false) } }
  }

  private fun publish(entry: Playing, update: PlaybackUi.() -> PlaybackUi) {
    if (current === entry && !entry.finishing) { entry.ui = entry.ui.update(); onPlaybackUi(entry.ui) }
  }
  private fun fail() = onError(player.message(R.string.sdk_request_failed))
  private fun reportFailure(failed: Boolean) {
    if (failed && !reportingError) onError(player.message(R.string.playback_report_failed))
    reportingError = failed
  }

  fun setEligible(value: Boolean) {
    eligible.set(value)
    player.setEligible(value)
    if (!value) {
      intent.incrementAndGet()
      preparing?.cancel()
      closeRemote()
      current?.let { entry -> scope.launch { observe(entry, true) } }
    } else if (remoteJob == null) openRemote()
  }

  fun profileChanged() { closeRemote(); if (eligible.get()) openRemote() }

  /** Revoke issued starts/resumes before an account operation can later fail and reopen admission. */
  fun blockForHandoff() {
    intent.incrementAndGet()
    preparing?.cancel()
    player.setHandoffBlocked(true)
    closeRemote()
  }

  private fun closeRemote() {
    ++remoteEpoch
    val job = remoteJob
    remoteJob = null
    // Cancellation can synchronously run the coroutine's finally through UniFFI callbacks.
    // Only that coroutine owns its SDK handles; detach our job before entering its cleanup.
    job?.cancel()
  }

  private fun openRemote() {
    if (closed || !eligible.get() || sdk.activeProfile()?.capabilities?.remoteControl != true || sdk.contentMutationsBlocked()) return
    val epoch = ++remoteEpoch
    val job = scope.launch(start = CoroutineStart.LAZY) {
      val commandLifetime = Any()
      var commandsValid = true
      var target: RemoteTarget? = null
      var token: OperationToken? = null
      try {
        token = sdk.newOperationToken()
        target = sdk.openRemoteTarget(token)
        if (epoch != remoteEpoch || !eligible.get()) return@launch
        while (isActive && epoch == remoteEpoch) {
          val event = target.nextEvent()
          if (event.state == RemoteTargetState.CLOSED) break
          val command = event.command ?: continue
          val issuingTarget = target
          val isCurrent = { synchronized(commandLifetime) {
            commandsValid && eligible.get() && epoch == remoteEpoch && issuingTarget.isCommandCurrent(event.generation)
          } }
          if (isCurrent()) remoteCommand(command, isCurrent)
        }
      } catch (_: CancellationException) { }
      catch (_: Exception) { if (epoch == remoteEpoch && eligible.get()) fail() }
      finally {
        // Revoke captured command guards before disposing handles. An older connection's
        // completion must not clear a replacement published during cancellation.
        if (epoch == remoteEpoch) { ++remoteEpoch; remoteJob = null }
        // Native workers recheck command admission off the main thread. Drain any check already
        // using the handle, then make subsequent checks return before disposing it below.
        synchronized(commandLifetime) { commandsValid = false }
        target?.stop(); target?.destroy(); token?.cancel(); token?.destroy()
      }
    }
    // Publish before execution so immediate failure/completion cannot leave a completed job here.
    remoteJob = job
    job.start()
  }

  private fun remoteCommand(command: RemoteCommand, stillCurrent: () -> Boolean) {
    if (!stillCurrent()) return
    when (command) {
      is RemoteCommand.Start -> requestStart(command.itemId, command.startPositionSeconds?.let { PlaybackStartPosition.At(it) } ?: PlaybackStartPosition.Resume,
        PlaybackSelection(command.mediaSourceId, command.audioStreamIndex, command.subtitleStreamIndex), stillCurrent)
      RemoteCommand.Resume -> resume(stillCurrent)
      RemoteCommand.Pause -> pausePlayback()
      RemoteCommand.TogglePause -> if (player.snapshot.value.paused) resume(stillCurrent) else pausePlayback()
      RemoteCommand.Stop -> stop()
      RemoteCommand.Next -> adjacent(true, stillCurrent)
      RemoteCommand.Previous -> adjacent(false, stillCurrent)
      is RemoteCommand.Seek -> seek(command.seconds)
      is RemoteCommand.SetVolume -> volume(command.volume.toInt())
      RemoteCommand.ToggleMute -> player.mute(!player.snapshot.value.muted)
      is RemoteCommand.SetAudioTrack -> current?.let { providerToPlayer(it, TrackKind.AUDIO, command.index)?.let { index -> selectTrack(TrackKind.AUDIO, index) } }
      is RemoteCommand.SetSubtitleTrack -> current?.let { providerToPlayer(it, TrackKind.SUBTITLE, command.index)?.let { index -> selectTrack(TrackKind.SUBTITLE, index) } }
    }
  }

  suspend fun beforeHandoff(): Boolean = withContext(Dispatchers.Main.immediate) {
    blockForHandoff()
    transitions.withLock {
      finishCurrent(false)
      current == null && player.stopAndWait()
    }
  }

  override fun close() {
    closed = true
    eligible.set(false)
    intent.incrementAndGet()
    preparing?.cancel()
    closeRemote()
    player.businessIntent = null
    player.setEligible(false)
    eventsJob.cancel()
    observationJob.cancel()
    snapshotsJob.cancel()
    // ViewModel cancellation must not turn explicit application teardown into process-loss recovery.
    CoroutineScope(Dispatchers.Main.immediate).launch { transitions.withLock { finishCurrent(false) } }
  }
}
