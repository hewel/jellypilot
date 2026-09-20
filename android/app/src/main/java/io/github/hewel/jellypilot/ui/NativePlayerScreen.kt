@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class, androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package io.github.hewel.jellypilot.ui

import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.hewel.jellypilot.AppViewModel
import io.github.hewel.jellypilot.NativePlayback
import io.github.hewel.jellypilot.R
import io.github.hewel.jellypilot.player.PlayerHost
import io.github.hewel.jellypilot.player.PlayerSnapshot
import io.github.hewel.jellypilot.player.PlayerStatus
import io.github.hewel.jellypilot.player.TrackKind
import java.util.Locale
import kotlinx.coroutines.delay

private enum class PlayerPanel { Audio, Subtitles, Queue, Video }

@Composable
internal fun NativePlayerScreen(model: AppViewModel, player: NativePlayback, back: () -> Unit) {
  val app by model.state.collectAsStateWithLifecycle()
  val snapshot by player.snapshot.collectAsStateWithLifecycle()
  val ready by player.ready.collectAsStateWithLifecycle()
  val error by player.error.collectAsStateWithLifecycle()
  val rewindLabel = stringResource(R.string.rewind_ten)
  val forwardLabel = stringResource(R.string.forward_ten)
  val playback = app.playbackUi
  var controls by rememberSaveable { mutableStateOf(true) }
  var panel by rememberSaveable { mutableStateOf<PlayerPanel?>(null) }
  var dragging by remember { mutableStateOf(false) }
  var focused by remember { mutableStateOf(false) }
  var interaction by remember { mutableIntStateOf(0) }
  val touchExploration = rememberTouchExploration()
  val panelTriggers = remember { PlayerPanel.entries.associateWith { FocusRequester() } }
  fun dismissPanel() {
    val previous = panel
    panel = null
    interaction++
    previous?.let { runCatching { panelTriggers[it]?.requestFocus() } }
  }
  LaunchedEffect(controls, panel, dragging, focused, touchExploration, interaction) {
    if (controls && panel == null && !dragging && !focused && !touchExploration) {
      delay(3_000)
      controls = false
    }
  }
  BackHandler(panel != null) { dismissPanel() }
  PilotTheme(dark = true) {
    BoxWithConstraints(Modifier.fillMaxSize().background(Color.Black)) {
      val landscape = maxWidth > maxHeight
      val panelHeight = maxHeight * 0.8f
      val panelWidth = (maxWidth * 0.45f).coerceIn(280.dp, 400.dp).coerceAtMost(maxWidth)
      Box(Modifier.fillMaxSize().onFocusChanged { focused = it.hasFocus }.focusGroup().pointerInput(Unit) { detectTapGestures { controls = !controls; interaction++ } }) {
        AndroidView(
          modifier = Modifier.fillMaxSize(),
          factory = { context -> SurfaceView(context).apply {
            holder.addCallback(object : SurfaceHolder.Callback {
              override fun surfaceCreated(holder: SurfaceHolder) { player.attach(holder.surface) }
              override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) { player.attach(holder.surface) }
              override fun surfaceDestroyed(holder: SurfaceHolder) { player.detach() }
            })
          } },
          update = { it.keepScreenOn = snapshot.isPlaying && snapshot.admissionEligible },
          onRelease = { player.detach() },
        )
        if (!controls) {
          snapshot.durationSeconds?.takeIf { it > 0 }?.let { duration ->
            LinearProgressIndicator(progress = { (snapshot.positionSeconds / duration).toFloat().coerceIn(0f, 1f) },
              modifier = Modifier.fillMaxWidth().height(3.dp).align(Alignment.BottomCenter).navigationBarsPadding(),
              color = MaterialTheme.colorScheme.primary.copy(alpha = if (snapshot.paused) 0.5f else 1f), trackColor = Color.White.copy(alpha = 0.16f), drawStopIndicator = {})
          }
          if (snapshot.paused && snapshot.status != PlayerStatus.LOADING) Surface(
            modifier = Modifier.align(Alignment.Center), shape = CircleShape, color = LocalPilotColors.current.playerScrim,
            onClick = { controls = true; interaction++ },
          ) { PilotIcon(R.drawable.ic_player_pause, stringResource(R.string.show_controls), Modifier.padding(16.dp), tint = Color.White) }
        }
        if (controls) {
          Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(Color.Black.copy(alpha = 0.6f), Color.Transparent, Color.Black.copy(alpha = 0.8f)))))
          Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
              IconButton(onClick = back) { PilotIcon(R.drawable.ic_chevron_left, stringResource(R.string.close_player), tint = Color.White) }
              Column(Modifier.weight(1f).padding(horizontal = 8.dp)) {
                Text(playback?.title ?: stringResource(R.string.player), style = MaterialTheme.typography.titleMedium, maxLines = 2, overflow = TextOverflow.Ellipsis, color = Color.White)
                playback?.episodeLabel?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = Color.White.copy(alpha = 0.75f)) }
              }
              if (playback?.autoSkipAvailable == true) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                  Text(stringResource(R.string.session_auto_skip), color = Color.White, style = MaterialTheme.typography.labelSmall)
                  Switch(playback.autoSkipEnabled, { model.setSessionAutoSkip(it); interaction++ })
                }
              }
            }
            Spacer(Modifier.weight(1f))
            Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
              IconButton(onClick = { model.previousEpisode(); interaction++ }, enabled = playback?.canPrevious == true) { PilotIcon(R.drawable.ic_player_skip_back, stringResource(R.string.previous_episode), tint = if (playback?.canPrevious == true) Color.White else Color.White.copy(alpha = 0.3f)) }
              TextButton(onClick = { model.seekPlayback((snapshot.positionSeconds - 10).coerceAtLeast(0.0)); interaction++ }, enabled = ready && snapshot.status != PlayerStatus.IDLE) { Text(stringResource(R.string.rewind_short), color = Color.White, modifier = Modifier.clearAndSetSemantics { contentDescription = rewindLabel }) }
              FilledIconButton(
                onClick = { if (snapshot.paused) model.playPlayback() else model.pausePlayback(); interaction++ },
                enabled = ready && snapshot.status != PlayerStatus.IDLE && snapshot.status != PlayerStatus.LOADING,
                modifier = Modifier.size(64.dp), colors = IconButtonDefaults.filledIconButtonColors(containerColor = Color.White.copy(alpha = 0.16f), contentColor = Color.White),
              ) { PilotIcon(if (snapshot.paused) R.drawable.ic_player_play else R.drawable.ic_player_pause, stringResource(if (snapshot.paused) R.string.play else R.string.pause), Modifier.size(32.dp)) }
              TextButton(onClick = { model.seekPlayback(snapshot.positionSeconds + 10); interaction++ }, enabled = ready && snapshot.status != PlayerStatus.IDLE) { Text(stringResource(R.string.forward_short), color = Color.White, modifier = Modifier.clearAndSetSemantics { contentDescription = forwardLabel }) }
              IconButton(onClick = { model.nextEpisode(); interaction++ }, enabled = playback?.canNext == true) { PilotIcon(R.drawable.ic_player_skip_forward, stringResource(R.string.next_episode), tint = if (playback?.canNext == true) Color.White else Color.White.copy(alpha = 0.3f)) }
            }
            Spacer(Modifier.weight(1f))
            Column(Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
              PlayerTimeline(snapshot, ready, { dragging = it; interaction++ }, model::seekPlayback)
              Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                PanelButton(PlayerPanel.Queue, R.drawable.ic_playlist, stringResource(R.string.queue), panelTriggers, playback?.queue?.isNotEmpty() == true) { panel = it }
                PanelButton(PlayerPanel.Audio, R.drawable.ic_headphones, stringResource(R.string.audio_tracks), panelTriggers, snapshot.tracks.any { it.kind == TrackKind.AUDIO }) { panel = it }
                PanelButton(PlayerPanel.Subtitles, R.drawable.ic_subtitles, stringResource(R.string.subtitle_tracks), panelTriggers, snapshot.tracks.any { it.kind == TrackKind.SUBTITLE }) { panel = it }
                PanelButton(PlayerPanel.Video, R.drawable.ic_adjustments, stringResource(R.string.video_options), panelTriggers, true) { panel = it }
                Spacer(Modifier.weight(1f))
                if (landscape) {
                  PilotIcon(when { snapshot.muted || snapshot.volumePercent == 0 -> R.drawable.ic_volume_muted; snapshot.volumePercent <= 33 -> R.drawable.ic_volume; snapshot.volumePercent <= 66 -> R.drawable.ic_volume_medium; else -> R.drawable.ic_volume_loud }, stringResource(R.string.volume), tint = Color.White)
                  Slider(snapshot.volumePercent.toFloat(), { model.setPlaybackVolume(it.toInt()); dragging = true; interaction++ },
                    modifier = Modifier.width(112.dp).heightIn(min = 48.dp), valueRange = 0f..100f,
                    onValueChangeFinished = { dragging = false }, enabled = ready)
                }
              }
            }
          }
        }
        if (snapshot.status == PlayerStatus.LOADING || snapshot.status == PlayerStatus.BUFFERING || !ready) {
          CircularProgressIndicator(Modifier.align(Alignment.Center).size(32.dp))
        }
        val playbackError = error ?: snapshot.error?.message
        if (playbackError != null) Surface(Modifier.align(Alignment.Center).padding(24.dp).widthIn(max = 500.dp), color = MaterialTheme.colorScheme.errorContainer, shape = MaterialTheme.shapes.medium) {
          Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(stringResource(R.string.playback_error), style = MaterialTheme.typography.titleMedium)
            Text(playbackError, style = MaterialTheme.typography.bodyMedium)
            TextButton(onClick = back) { Text(stringResource(R.string.back)) }
          }
        }
        playback?.manualSkipLabel?.let { label ->
          LaunchedEffect(playback.currentItemId, label) { model.acknowledgeSkipPrompt(true) }
          FilledTonalButton(onClick = { model.skipSegment(); interaction++ }, modifier = Modifier.align(Alignment.BottomEnd).windowInsetsPadding(WindowInsets.safeDrawing).padding(end = 24.dp, bottom = if (controls) 148.dp else 32.dp)) { Text(label) }
        }
        Column(Modifier.align(Alignment.TopCenter).statusBarsPadding().padding(top = if (controls) 72.dp else 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
          app.error?.let { message ->
            Snackbar(modifier = Modifier.widthIn(max = 480.dp).padding(horizontal = 16.dp).semantics { liveRegion = LiveRegionMode.Polite },
              dismissAction = { IconButton(onClick = model::dismissError) { PilotIcon(R.drawable.ic_x, stringResource(R.string.close)) } },
            ) { Text(message) }
          }
          if (playback?.skipUndoAvailable == true) SkipUndo(model, playback.skipUndoId, Modifier)
        }
      }
      panel?.let { selected ->
        if (landscape) Dialog(onDismissRequest = ::dismissPanel, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
          Box(Modifier.fillMaxSize()) {
            Box(Modifier.matchParentSize().clickable(onClick = ::dismissPanel))
            Surface(Modifier.align(Alignment.CenterEnd).width(panelWidth).fillMaxHeight().windowInsetsPadding(WindowInsets.safeDrawing), color = MaterialTheme.colorScheme.surface) {
              PlayerPanelContent(selected, snapshot, playback, model, ::dismissPanel)
            }
          }
        } else ModalBottomSheet(onDismissRequest = ::dismissPanel, containerColor = MaterialTheme.colorScheme.surface) {
          Box(Modifier.fillMaxWidth().heightIn(max = panelHeight)) { PlayerPanelContent(selected, snapshot, playback, model, ::dismissPanel) }
        }
      }
    }
  }
}

@Composable
private fun PanelButton(panel: PlayerPanel, icon: Int, title: String, triggers: Map<PlayerPanel, FocusRequester>, enabled: Boolean, open: (PlayerPanel) -> Unit) {
  IconButton(onClick = { open(panel) }, enabled = enabled, modifier = Modifier.focusRequester(triggers.getValue(panel))) { PilotIcon(icon, title, tint = if (enabled) Color.White else Color.White.copy(alpha = 0.3f)) }
}

@Composable
private fun PlayerTimeline(snapshot: PlayerSnapshot, ready: Boolean, dragging: (Boolean) -> Unit, seek: (Double) -> Unit) {
  var position by remember { mutableStateOf<Float?>(null) }
  val duration = snapshot.durationSeconds?.takeIf { it > 0 && it.isFinite() }
  val seekLabel = stringResource(R.string.seek)
  if (duration != null) Slider(
    value = position ?: snapshot.positionSeconds.toFloat().coerceIn(0f, duration.toFloat()),
    onValueChange = { position = it; dragging(true) },
    onValueChangeFinished = { position?.let { seek(it.toDouble()) }; position = null; dragging(false) },
    valueRange = 0f..duration.toFloat(), enabled = ready && snapshot.status != PlayerStatus.LOADING,
    modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).semantics { contentDescription = seekLabel },
  )
  Row(Modifier.fillMaxWidth()) {
    Text(playbackClock(position?.toDouble() ?: snapshot.positionSeconds), color = Color.White, style = MaterialTheme.typography.labelSmall)
    Spacer(Modifier.weight(1f))
    Text(duration?.let(::playbackClock) ?: "—", color = Color.White.copy(alpha = 0.75f), style = MaterialTheme.typography.labelSmall)
  }
}

@Composable
private fun PlayerPanelContent(panel: PlayerPanel, snapshot: PlayerSnapshot, playback: PlaybackUi?, model: AppViewModel, close: () -> Unit) {
  val title = stringResource(when (panel) { PlayerPanel.Audio -> R.string.audio_tracks; PlayerPanel.Subtitles -> R.string.subtitle_tracks; PlayerPanel.Queue -> R.string.queue; PlayerPanel.Video -> R.string.video_options })
  Column(Modifier.fillMaxSize().padding(horizontal = 16.dp)) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
      Text(title, Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
      IconButton(onClick = close) { PilotIcon(R.drawable.ic_x, stringResource(R.string.close)) }
    }
    LazyColumn(Modifier.weight(1f), contentPadding = PaddingValues(bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
      if (panel == PlayerPanel.Audio || panel == PlayerPanel.Subtitles) {
        val kind = if (panel == PlayerPanel.Audio) TrackKind.AUDIO else TrackKind.SUBTITLE
        if (kind == TrackKind.SUBTITLE) item(key = "off") {
          TrackRow(stringResource(R.string.subtitles_off), snapshot.tracks.none { it.kind == TrackKind.SUBTITLE && it.isSelected }) { model.selectPlaybackTrack(kind, PlayerHost.TRACK_ID_NONE) }
        }
        items(snapshot.tracks.filter { it.kind == kind }, key = { it.mpvId }) { track ->
          val label = listOfNotNull(track.title, track.language, track.codec).filter { it.isNotBlank() }.joinToString(" · ").ifBlank { track.mpvId.toString() }
          TrackRow(label, track.isSelected) { model.selectPlaybackTrack(kind, track.mpvId) }
        }
      } else if (panel == PlayerPanel.Queue) {
        items(playback?.queue.orEmpty(), key = { it.id }) { item -> TrackRow(listOfNotNull(item.episodeCode, item.title).joinToString(" · "), playback?.currentItemId == item.id) { model.playItem(item.id, item.played); close() } }
        if (playback?.queueHasMore == true) item { TextButton(onClick = model::loadMorePlaybackEpisodes, enabled = !playback.queueLoading, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.load_more)) } }
      } else {
        item { Text(stringResource(R.string.video_resolution, snapshot.videoWidth, snapshot.videoHeight), Modifier.padding(vertical = 16.dp)) }
        item { Text(stringResource(R.string.video_controls_hint), style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.metadata) }
        item {
          Text(stringResource(R.string.volume), Modifier.padding(top = 24.dp))
          Slider(snapshot.volumePercent.toFloat(), { model.setPlaybackVolume(it.toInt()) }, valueRange = 0f..100f)
        }
      }
    }
  }
}

@Composable
private fun TrackRow(title: String, selected: Boolean, select: () -> Unit) {
  TextButton(onClick = select, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp).semantics { this.selected = selected; role = Role.RadioButton }, shape = MaterialTheme.shapes.small,
    colors = ButtonDefaults.textButtonColors(containerColor = if (selected) MaterialTheme.colorScheme.primaryContainer else Color.Transparent)) {
    Text(title, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium, color = if (selected) MaterialTheme.colorScheme.onPrimaryContainer else MaterialTheme.colorScheme.onSurface)
    if (selected) PilotIcon(R.drawable.ic_check)
  }
}

@Composable
private fun SkipUndo(model: AppViewModel, noticeId: Long, modifier: Modifier) {
  var focused by remember { mutableStateOf(false) }
  val touchExploration = rememberTouchExploration()
  var remaining by remember(noticeId) { mutableLongStateOf(8_000L) }
  LaunchedEffect(noticeId, focused, touchExploration) {
    if (!focused && !touchExploration) {
      while (remaining > 0) { delay(250); remaining -= 250 }
      model.dismissSkipUndo()
    }
  }
  Snackbar(modifier = modifier.widthIn(max = 480.dp).padding(horizontal = 16.dp).onFocusChanged { focused = it.hasFocus }.semantics { liveRegion = LiveRegionMode.Polite },
    action = { TextButton(onClick = model::undoSkip) { Text(stringResource(R.string.undo)) } },
    dismissAction = { IconButton(onClick = model::dismissSkipUndo) { PilotIcon(R.drawable.ic_x, stringResource(R.string.close)) } },
  ) { Text(stringResource(R.string.skip_undone)) }
}

internal fun playbackClock(seconds: Double): String {
  val whole = seconds.takeIf { it.isFinite() && it >= 0 }?.toLong() ?: 0L
  return if (whole < 3_600) String.format(Locale.ROOT, "%d:%02d", whole / 60, whole % 60)
  else String.format(Locale.ROOT, "%d:%02d:%02d", whole / 3_600, whole / 60 % 60, whole % 60)
}
