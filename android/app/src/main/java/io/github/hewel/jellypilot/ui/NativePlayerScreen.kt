@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class, androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package io.github.hewel.jellypilot.ui

import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.*
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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.unit.dp
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.hewel.jellypilot.AppViewModel
import io.github.hewel.jellypilot.NativePlayback
import io.github.hewel.jellypilot.PlayerDialogSystemBars
import io.github.hewel.jellypilot.R
import io.github.hewel.jellypilot.player.PlayerStatus
import io.github.hewel.jellypilot.player.TrackKind
import java.util.Locale
import kotlinx.coroutines.delay

@Composable
internal fun NativePlayerScreen(model: AppViewModel, player: NativePlayback, back: () -> Unit) {
  val app by model.state.collectAsStateWithLifecycle()
  val snapshot by player.snapshot.collectAsStateWithLifecycle()
  val ready by player.ready.collectAsStateWithLifecycle()
  val error by player.error.collectAsStateWithLifecycle()
  val showControls = stringResource(R.string.show_controls)
  val playback = app.playbackUi
  var controls by rememberSaveable { mutableStateOf(false) }
  var bottomHeight by remember { mutableStateOf(180.dp) }
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
      val panelWidth = 340.dp.coerceAtMost((maxWidth - 32.dp).coerceAtLeast(0.dp))
      Box(Modifier.fillMaxSize().onFocusChanged { focused = it.hasFocus }.focusGroup()) {
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
        Box(Modifier.matchParentSize().clickable(interactionSource = null, indication = null,
          onClickLabel = showControls, onClick = { controls = !controls; interaction++ })
          .semantics { contentDescription = showControls })
        if (!controls) {
          Box(Modifier.align(Alignment.BottomCenter).fillMaxWidth().height(72.dp)
            .background(Brush.verticalGradient(listOf(Color.Transparent, PilotPlayerTokens.minimalScrim))))
          snapshot.durationSeconds?.takeIf { it.isFinite() && it > 0 }?.let { duration ->
            Box(Modifier.fillMaxWidth().height(4.dp).align(Alignment.BottomCenter)
              .alpha(if (snapshot.paused) 0.6f else 1f).background(PilotPlayerTokens.minimalRail)) {
              Box(Modifier.fillMaxHeight().fillMaxWidth((snapshot.positionSeconds / duration).toFloat().coerceIn(0f, 1f))
                .background(MaterialTheme.colorScheme.primary))
            }
          }
          if (snapshot.paused && snapshot.status == PlayerStatus.READY) Surface(
            modifier = Modifier.align(Alignment.Center).size(72.dp), shape = CircleShape,
            color = PilotPlayerTokens.chip, border = BorderStroke(1.dp, PilotPlayerTokens.border),
            onClick = { controls = true; interaction++ },
          ) {
            Box(contentAlignment = Alignment.Center) {
              PilotIcon(R.drawable.ic_player_pause, stringResource(R.string.show_controls), Modifier.size(28.dp), PilotPlayerTokens.foreground)
            }
          }
        }
        if (controls) {
          Box(Modifier.fillMaxWidth().height(96.dp).align(Alignment.TopCenter)
            .background(Brush.verticalGradient(listOf(PilotPlayerTokens.topScrim, Color.Transparent))))
          Box(Modifier.fillMaxWidth().height(maxOf(230.dp, bottomHeight + 40.dp)).align(Alignment.BottomCenter)
            .background(Brush.verticalGradient(*PilotPlayerTokens.bottomScrim.toTypedArray())))
          PlayerFullControls(
            snapshot, playback, ready, back,
            playPause = { if (snapshot.paused) model.playPlayback() else model.pausePlayback(); interaction++ },
            seek = { model.seekPlayback(it); interaction++ }, volume = { model.setPlaybackVolume(it); interaction++ },
            autoSkip = { model.setSessionAutoSkip(it); interaction++ },
            dragging = { dragging = it; interaction++ }, bottomHeight = { bottomHeight = it },
          ) {
            PanelButton(PlayerPanel.Queue, R.drawable.ic_playlist, stringResource(R.string.queue), panelTriggers,
              playback?.let { it.queue.isNotEmpty() || it.queueHasMore || it.canPrevious || it.canNext } == true) { panel = it; interaction++ }
            PanelButton(PlayerPanel.Audio, R.drawable.ic_headphones, stringResource(R.string.audio_tracks), panelTriggers, snapshot.tracks.any { it.kind == TrackKind.AUDIO }) { panel = it; interaction++ }
            PanelButton(PlayerPanel.Subtitles, R.drawable.ic_subtitles, stringResource(R.string.subtitle_tracks), panelTriggers, snapshot.tracks.any { it.kind == TrackKind.SUBTITLE }) { panel = it; interaction++ }
            PanelButton(PlayerPanel.Video, R.drawable.ic_adjustments, stringResource(R.string.video_options), panelTriggers, true) { panel = it; interaction++ }
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
          Surface(onClick = { model.skipSegment(); interaction++ }, shape = CircleShape,
            color = PilotPlayerTokens.chip, contentColor = PilotPlayerTokens.foreground,
            border = BorderStroke(1.dp, PilotPlayerTokens.border),
            modifier = Modifier.align(Alignment.BottomEnd).windowInsetsPadding(WindowInsets.displayCutout)
              .padding(end = 24.dp, bottom = if (controls) bottomHeight + 24.dp else 20.dp),
          ) {
            Box(Modifier.heightIn(min = 48.dp).padding(horizontal = 16.dp), contentAlignment = Alignment.Center) {
              Text(label, style = MaterialTheme.typography.labelMedium)
            }
          }
        }
        Column(Modifier.align(Alignment.TopCenter).windowInsetsPadding(WindowInsets.displayCutout.only(WindowInsetsSides.Top)).padding(top = if (controls) 80.dp else 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
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
          PlayerDialogSystemBars()
          Box(Modifier.fillMaxSize()) {
            Box(Modifier.matchParentSize().background(PilotPlayerTokens.backdrop).clickable(onClick = ::dismissPanel))
            Surface(Modifier.align(Alignment.CenterEnd).windowInsetsPadding(WindowInsets.displayCutout).padding(16.dp)
              .width(panelWidth).fillMaxHeight(), shape = MaterialTheme.shapes.large, color = MaterialTheme.colorScheme.surfaceContainer) {
              PlayerPanelContent(selected, snapshot, playback, model, ::dismissPanel)
            }
          }
        } else ModalBottomSheet(onDismissRequest = ::dismissPanel, containerColor = MaterialTheme.colorScheme.surfaceContainer) {
          PlayerDialogSystemBars()
          Box(Modifier.fillMaxWidth().heightIn(max = panelHeight)) { PlayerPanelContent(selected, snapshot, playback, model, ::dismissPanel) }
        }
      }
    }
  }
}

@Composable
private fun PanelButton(panel: PlayerPanel, icon: Int, title: String, triggers: Map<PlayerPanel, FocusRequester>, enabled: Boolean, open: (PlayerPanel) -> Unit) {
  IconButton(onClick = { open(panel) }, enabled = enabled, modifier = Modifier.size(48.dp).focusRequester(triggers.getValue(panel))) { PilotIcon(icon, title, tint = if (enabled) PilotPlayerTokens.secondary else PilotPlayerTokens.disabled) }
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
  Surface(modifier = modifier.padding(horizontal = 16.dp).widthIn(max = 360.dp).onFocusChanged { focused = it.hasFocus }
    .semantics { liveRegion = LiveRegionMode.Polite }, color = PilotPlayerTokens.notice,
    contentColor = PilotPlayerTokens.foreground, shape = MaterialTheme.shapes.medium,
  ) {
    Row(Modifier.padding(horizontal = 16.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
      Text(stringResource(R.string.skip_undone), Modifier.weight(1f), style = MaterialTheme.typography.titleSmall)
      TextButton(onClick = model::undoSkip, colors = ButtonDefaults.textButtonColors(contentColor = PilotPlayerTokens.foreground)) { Text(stringResource(R.string.undo)) }
      IconButton(onClick = model::dismissSkipUndo) { PilotIcon(R.drawable.ic_x, stringResource(R.string.close)) }
    }
  }
}

internal fun playbackClock(seconds: Double): String {
  val whole = seconds.takeIf { it.isFinite() && it >= 0 }?.toLong() ?: 0L
  return if (whole < 3_600) String.format(Locale.ROOT, "%02d:%02d", whole / 60, whole % 60)
  else String.format(Locale.ROOT, "%d:%02d:%02d", whole / 3_600, whole / 60 % 60, whole % 60)
}
