package io.github.hewel.jellypilot

import androidx.annotation.MainThread
import io.github.hewel.jellypilot.ffi.*
import io.github.hewel.jellypilot.ui.RemoteControllerUiState
import io.github.hewel.jellypilot.ui.RemotePlayItem
import io.github.hewel.jellypilot.ui.AppUiState
import io.github.hewel.jellypilot.ui.MediaUi
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch

/** Resolve only a movie or an episode that belongs to this detail page, never a Series id. */
internal fun AppUiState.remotePlayItem(item: MediaUi): RemotePlayItem? {
  val owner = detail ?: return null
  val id = item.playTargetId ?: item.id
  val preview = detailPreview?.takeIf { it.ownerId == owner.id && it.seasonId == selectedSeasonId }?.item
  val target = preview?.takeIf { it.id == id } ?: detailItems.firstOrNull { it.id == id } ?: owner.takeIf { it.id == id }
    ?: return null
  if (!item.playable || !target.playable || target.itemType !in setOf("Movie", "Episode")) return null
  val position = if (target.played || !target.resumeSeconds.isFinite() || target.resumeSeconds <= 0.0) PlaybackStartPosition.Beginning
    else PlaybackStartPosition.At(target.resumeSeconds)
  return RemotePlayItem(id, target.title, position)
}

/** Native ownership seam; discovery, admission and command policy remain in the SDK. */
internal interface RemoteControllerPort {
  fun snapshot(): RemoteControlSnapshot
  suspend fun nextSnapshot(revision: ULong): RemoteControlSnapshot
  fun setActive(active: Boolean)
  fun refresh()
  fun selectTarget(key: RemoteControlTargetKey)
  suspend fun execute(generation: ULong, target: RemoteControlTargetKey, command: RemoteControlCommand): RemoteControlReceipt
  fun close()
}

internal class SdkRemoteController(private val controller: io.github.hewel.jellypilot.ffi.RemoteController) : RemoteControllerPort {
  override fun snapshot() = controller.snapshot()
  override suspend fun nextSnapshot(revision: ULong) = controller.nextSnapshot(revision)
  override fun setActive(active: Boolean) = controller.setActive(active)
  override fun refresh() = controller.refresh()
  override fun selectTarget(key: RemoteControlTargetKey) = controller.selectTarget(key)
  override suspend fun execute(generation: ULong, target: RemoteControlTargetKey, command: RemoteControlCommand) = controller.execute(generation, target, command)
  override fun close() {
    try { controller.shutdown() } finally { controller.destroy() }
  }
}

/** A visible page owns one inactive-by-default SDK session; no Android polling timer exists. */
@MainThread
internal class RemoteControllerCoordinator(
  private val scope: CoroutineScope,
  private val openController: () -> RemoteControllerPort,
  private val publish: (RemoteControllerUiState?) -> Unit,
) {
  private var controller: RemoteControllerPort? = null
  private var ui: RemoteControllerUiState? = null
  private var eligible = false
  private var epoch = 0L
  private var collector: Job? = null
  private var commandJob: Job? = null

  fun open(item: RemotePlayItem?, serverName: String) {
    close()
    ui = RemoteControllerUiState(loading = true, pendingPlay = item, serverName = serverName)
    emit()
    connect()
  }

  private fun connect() {
    try {
      controller = openController()
      controller?.setActive(eligible)
      receive(controller!!.snapshot())
      if (eligible) collect()
    } catch (_: Exception) {
      release()
      ui = ui?.copy(loading = false, failed = true)
      emit()
    }
  }

  fun setEligible(value: Boolean) {
    if (eligible == value) return
    eligible = value
    // Synchronous SDK invalidation precedes coroutine cancellation and any later UI frame.
    val port = controller ?: return
    try {
      port.setActive(value)
      cancelWork()
      ui = ui?.copy(accepted = false, commandFailed = false)
      receive(port.snapshot())
      if (value) collect()
    } catch (_: Exception) { fail() }
  }

  fun refresh() {
    if (ui == null || !eligible) return
    if (controller == null) { connect(); return }
    try { controller!!.refresh(); receive(controller!!.snapshot()) }
    catch (_: Exception) { fail() }
  }

  fun select(key: RemoteControlTargetKey) {
    if (!eligible) return
    val port = controller ?: return
    try {
      port.selectTarget(key)
      cancelWork()
      ui = ui?.copy(accepted = false, commandFailed = false)
      receive(port.snapshot())
      collect()
    } catch (_: Exception) { ui = ui?.copy(commandFailed = true); emit() }
  }

  fun play(generation: ULong, key: RemoteControlTargetKey) {
    val current = ui ?: return
    val item = current.pendingPlay ?: return
    execute(generation, key, RemoteControlCommand.PlayNow(item.itemId, item.position))
  }

  fun execute(generation: ULong, key: RemoteControlTargetKey, command: RemoteControlCommand) {
    val current = ui?.snapshot ?: return
    if (!eligible || commandJob != null || current.generation != generation || current.selected != key ||
      current.status != RemoteControllerStatus.READY || current.error != null) return
    val port = controller ?: return
    val started = epoch
    ui = ui?.copy(accepted = false, commandFailed = false)
    emit()
    commandJob = scope.launch(start = CoroutineStart.LAZY) {
      try {
        val receipt = port.execute(generation, key, command)
        currentCoroutineContext().ensureActive()
        if (epoch == started && ui?.snapshot?.generation == generation && ui?.snapshot?.selected == key) {
          ui = ui?.copy(accepted = receipt.serverAccepted, commandFailed = !receipt.serverAccepted)
          emit()
        }
      } catch (cancelled: CancellationException) { throw cancelled }
      catch (_: Exception) {
        if (epoch == started) { ui = ui?.copy(commandFailed = true); emit() }
      } finally { if (epoch == started) commandJob = null }
    }
    commandJob?.start()
  }

  private fun collect() {
    val port = controller ?: return
    val started = epoch
    collector = scope.launch {
      try {
        val initial = port.snapshot()
        receive(initial)
        if (initial.status == RemoteControllerStatus.CLOSED) { fail(); return@launch }
        var revision = initial.revision
        while (true) {
          val next = port.nextSnapshot(revision)
          currentCoroutineContext().ensureActive()
          if (epoch != started) return@launch
          receive(next)
          revision = next.revision
          if (next.status == RemoteControllerStatus.CLOSED) { fail(); return@launch }
        }
      } catch (cancelled: CancellationException) { throw cancelled }
      catch (_: Exception) { if (epoch == started) fail() }
    }
  }

  private fun receive(snapshot: RemoteControlSnapshot) {
    ui = ui?.copy(snapshot = snapshot, loading = snapshot.status == RemoteControllerStatus.LOADING,
      failed = snapshot.status == RemoteControllerStatus.FAILED || snapshot.status == RemoteControllerStatus.CLOSED)
    emit()
  }

  private fun emit() = publish(ui)
  private fun cancelWork() {
    ++epoch
    collector?.cancel(); collector = null
    commandJob?.cancel(); commandJob = null
  }
  private fun release() {
    val port = controller
    controller = null
    try { port?.setActive(false) } catch (_: Exception) { /* Shutdown still owns cancellation. */ }
    cancelWork()
    try { port?.close() } catch (_: Exception) { /* A closed page has no remaining native work. */ }
  }
  private fun fail() {
    release()
    ui = ui?.copy(loading = false, failed = true, accepted = false)
    emit()
  }
  fun close() {
    release()
    ui = null
    emit()
  }
}
