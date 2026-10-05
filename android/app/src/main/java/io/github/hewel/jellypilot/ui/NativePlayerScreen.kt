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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.InputMode
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalInputModeManager
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
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
  val gestureSeeking by player.gestureSeeking.collectAsStateWithLifecycle()
  val ready by player.ready.collectAsStateWithLifecycle()
  val error by player.error.collectAsStateWithLifecycle()
  val showControls = stringResource(R.string.show_controls)
  val playback = app.playbackUi
  val phone = LocalConfiguration.current.smallestScreenWidthDp < 600
  val touchExploration = rememberTouchExploration()
  val paused = snapshot.paused && (snapshot.status == PlayerStatus.READY || snapshot.status == PlayerStatus.BUFFERING)
  val chrome = rememberPlayerChrome(snapshot.generation, paused, phone, touchExploration, playing = snapshot.isPlaying)
  var gestureFeedback by remember(snapshot.generation) { mutableStateOf<GestureFeedback?>(null) }
  val gestureArbitration = remember(snapshot.generation) { PlayerGestureArbitration() }
  val controls = chrome.visible && gestureFeedback == null
  val panel = chrome.panel
  val lifecycle = LocalLifecycleOwner.current.lifecycle
  var foreground by remember(lifecycle) { mutableStateOf(lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) }
  val windowFocused = LocalWindowInfo.current.isWindowFocused
  val view = LocalView.current
  DisposableEffect(lifecycle, model) {
    val observer = LifecycleEventObserver { _, _ ->
      foreground = lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)
      if (!foreground) model.cancelPlaybackGestures()
    }
    lifecycle.addObserver(observer)
    onDispose { lifecycle.removeObserver(observer); model.cancelPlaybackGestures() }
  }
  LaunchedEffect(windowFocused, foreground, panel, touchExploration, snapshot.status) {
    if (!windowFocused || !foreground || panel != null || touchExploration ||
      snapshot.status == PlayerStatus.IDLE || snapshot.status == PlayerStatus.ENDED || snapshot.error != null) model.cancelPlaybackGestures()
  }
  val gestureActions = object : PlayerGestureActions {
    override fun playback() = GesturePlayback(snapshot.generation, snapshot.positionSeconds, snapshot.durationSeconds,
      snapshot.seekable && !player.gestureSeekActive, snapshot.isPlaying, snapshot.speed, snapshot.pictureBrightnessPercent,
      snapshot.pictureBrightnessAvailable, snapshot.volumePercent, snapshot.volumeAvailable, snapshot.muted)
    override fun toggleChrome() { if (touchExploration) chrome.reveal() else chrome.toggle() }
    override fun seeking() = model.beginGestureSeek()?.let {
      chrome.gestureSeeking = true
      GestureSeekCapture(it.token, it.positionSeconds)
    }
    override fun finishSeek(token: Long, target: Double?) { model.finishGestureSeek(token, target); chrome.gestureSeeking = false }
    override fun seek(target: Double) { model.seekPlayback(target) }
    override fun speed() = model.beginGestureSpeed()
    override fun finishSpeed(token: Long) { model.endGestureSpeed(token) }
    override fun brightness(value: Int) { player.pictureBrightness(value) }
    override fun volume(value: Int) { model.setPlaybackVolume(value) }
    override fun restoreVolume(value: Int, muted: Boolean) { player.volume(value); player.mute(muted) }
    override fun feedback(value: GestureFeedback?) {
      gestureFeedback = value
      chrome.gestureFeedback = value != null
      if (value == null) chrome.interacted()
    }
    override fun recognizing(active: Boolean) { chrome.gestureRecognizing = active; if (!active) chrome.interacted() }
    @Suppress("DEPRECATION")
    override fun completed(value: GestureFeedback) {
      val message = when (value) {
        is GestureFeedback.Step -> view.context.getString(R.string.gesture_seek_seconds, signedSeconds(value.delta))
        is GestureFeedback.Seek -> view.context.getString(R.string.gesture_seek_to, playbackClock(value.target))
        is GestureFeedback.Level -> if (!value.brightness && value.value == 0) view.context.getString(R.string.gesture_muted)
          else view.context.getString(if (value.brightness) R.string.gesture_brightness_value else R.string.gesture_volume_value, value.value)
        GestureFeedback.Speed -> return
      }
      view.announceForAccessibility(message)
    }
  }
  var bottomHeight by remember { mutableStateOf(if (phone) 136.dp else 180.dp) }
  val inputMode = LocalInputModeManager.current
  var controlsFocused by remember { mutableStateOf(false) }
  LaunchedEffect(controlsFocused, inputMode.inputMode, phone, chrome) {
    chrome.focused = controlsFocused && (!phone || inputMode.inputMode == InputMode.Keyboard)
  }
  val panelTriggers = remember { PlayerPanel.entries.associateWith { FocusRequester() } }
  LaunchedEffect(panel, chrome.restoreTrigger) {
    chrome.restoreTrigger?.let { trigger ->
      // The panel window must leave composition before focus returns to its player trigger.
      withFrameNanos { }
      panelTriggers[trigger]?.requestFocus()
      chrome.restoredFocus()
    }
  }
  BackHandler(panel != null) { chrome.back() }
  PilotTheme(dark = true) {
    BoxWithConstraints(Modifier.fillMaxSize().background(Color.Black)) {
      val landscape = maxWidth > maxHeight
      val panelHeight = maxHeight * 0.8f
      val panelWidth = 340.dp.coerceAtMost((maxWidth - 32.dp).coerceAtLeast(0.dp))
      val gestureBounds = playerGestureBounds(maxWidth.value, maxHeight.value)
      val feedbackBounds = playerGestureBounds(maxWidth.value, maxHeight.value, minimumEdge = 0f)
      Box(Modifier.fillMaxSize().observePlayerMultitouch(gestureArbitration)
        .onFocusChanged { controlsFocused = it.hasFocus }.focusGroup()) {
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
        val controlsLabel = if (controls) stringResource(R.string.player_hide_controls) else showControls
        if (phone && !touchExploration) PlayerGestureSurface(
          generation = snapshot.generation,
          enabled = ready && foreground && windowFocused && panel == null && error == null &&
            snapshot.status in listOf(PlayerStatus.READY, PlayerStatus.BUFFERING) && snapshot.error == null &&
            (gestureFeedback != GestureFeedback.Speed || snapshot.isPlaying),
          shortcuts = app.preferences.playerGestures, label = controlsLabel,
          bounds = gestureBounds, actions = gestureActions, arbitration = gestureArbitration, modifier = Modifier.matchParentSize(),
        ) else Box(Modifier.matchParentSize().clickable(interactionSource = null, indication = null,
          onClickLabel = controlsLabel, onClick = { if (touchExploration) chrome.reveal() else chrome.toggle() })
          .semantics { contentDescription = controlsLabel })
        if (phone) PlayerGestureFeedback(gestureFeedback, feedbackBounds, app.preferences.reducedMotion)
        if (!controls && !phone) {
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
            onClick = chrome::reveal,
          ) {
            Box(contentAlignment = Alignment.Center) {
              PilotIcon(R.drawable.ic_player_pause, stringResource(R.string.show_controls), Modifier.size(28.dp), PilotPlayerTokens.foreground)
            }
          }
        }
        if (controls) {
          if (phone) Box(Modifier.matchParentSize().background(PilotPlayerTokens.phoneVeil))
          Box(Modifier.fillMaxWidth().height(96.dp).align(Alignment.TopCenter)
            .background(Brush.verticalGradient(*(if (phone) PilotPlayerTokens.phoneTopScrim else listOf(0f to PilotPlayerTokens.topScrim, 1f to Color.Transparent)).toTypedArray())))
          Box(Modifier.fillMaxWidth().height(maxOf(if (phone) 152.dp else 230.dp, bottomHeight + if (phone) 16.dp else 40.dp)).align(Alignment.BottomCenter)
            .background(Brush.verticalGradient(*(if (phone) PilotPlayerTokens.phoneBottomScrim else PilotPlayerTokens.bottomScrim).toTypedArray())))
          PlayerFullControls(
            snapshot.copy(seekable = snapshot.seekable && !gestureSeeking), playback, ready, back,
            playPause = { if (snapshot.paused) model.playPlayback() else model.pausePlayback(); chrome.interacted() },
            seek = { model.seekPlayback(it); chrome.interacted() }, volume = { model.setPlaybackVolume(it); chrome.interacted() },
            autoSkip = { model.setSessionAutoSkip(it); chrome.interacted() },
            dragging = { chrome.dragging = it; chrome.interacted() }, bottomHeight = { bottomHeight = it },
            phone = phone,
            more = {
              TextButton(onClick = { chrome.open(PlayerPanel.More) },
                modifier = Modifier.widthIn(min = 48.dp).heightIn(min = 48.dp).focusRequester(panelTriggers.getValue(PlayerPanel.More)),
                contentPadding = PaddingValues(horizontal = 4.dp),
                colors = ButtonDefaults.textButtonColors(contentColor = PilotPlayerTokens.foreground),
              ) { Text(stringResource(R.string.player_settings), style = MaterialTheme.typography.titleSmall) }
            },
          ) {
            PanelButton(PlayerPanel.Queue, R.drawable.ic_playlist,
              stringResource(if (phone) R.string.player_episodes else R.string.queue), panelTriggers,
              playback?.let { it.queue.isNotEmpty() || it.queueHasMore || it.canPrevious || it.canNext } == true,
              labelled = phone, open = chrome::open)
            PanelButton(PlayerPanel.Audio, R.drawable.ic_headphones,
              stringResource(if (phone) R.string.player_audio else R.string.audio_tracks),
              panelTriggers, snapshot.tracks.any { it.kind == TrackKind.AUDIO }, labelled = phone, open = chrome::open)
            PanelButton(PlayerPanel.Subtitles, R.drawable.ic_subtitles,
              stringResource(if (phone) R.string.player_subtitles else R.string.subtitle_tracks), panelTriggers,
              ready && snapshot.status in listOf(PlayerStatus.READY, PlayerStatus.BUFFERING), labelled = phone, open = chrome::open)
            if (!phone) PanelButton(PlayerPanel.Video, R.drawable.ic_adjustments, stringResource(R.string.video_options),
              panelTriggers, true, open = chrome::open)
          }
        }
        if (snapshot.status == PlayerStatus.LOADING || snapshot.status == PlayerStatus.BUFFERING || !ready) {
          CircularProgressIndicator(Modifier.align(Alignment.Center).size(32.dp))
        }
        val playbackError = error ?: snapshot.error?.message
        if (playbackError != null) Surface(Modifier.align(Alignment.Center).padding(24.dp).widthIn(max = 500.dp).reservePlayerOverlayTouchArea(), color = MaterialTheme.colorScheme.errorContainer, shape = MaterialTheme.shapes.medium) {
          Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(stringResource(R.string.playback_error), style = MaterialTheme.typography.titleMedium)
            Text(playbackError, style = MaterialTheme.typography.bodyMedium)
            TextButton(onClick = back) { Text(stringResource(R.string.back)) }
          }
        }
        playback?.manualSkipLabel?.let { label ->
          LaunchedEffect(playback.currentItemId, label) { model.acknowledgeSkipPrompt(true) }
          Surface(onClick = { model.skipSegment(); chrome.interacted() },
            shape = if (phone) RoundedCornerShape(12.dp) else CircleShape,
            color = if (phone) PilotPlayerTokens.phoneFeedback else PilotPlayerTokens.chip, contentColor = PilotPlayerTokens.foreground,
            border = if (phone) null else BorderStroke(1.dp, PilotPlayerTokens.border),
            modifier = Modifier.align(Alignment.BottomEnd).windowInsetsPadding(WindowInsets.displayCutout)
              .padding(end = 24.dp, bottom = if (controls) bottomHeight + 24.dp else 24.dp),
          ) {
            Box(Modifier.heightIn(min = 48.dp).padding(horizontal = 16.dp), contentAlignment = Alignment.Center) {
              Text(label, style = MaterialTheme.typography.labelMedium)
            }
          }
        }
        Column(Modifier.align(Alignment.TopCenter).windowInsetsPadding(WindowInsets.displayCutout.only(WindowInsetsSides.Top)).padding(top = if (controls) 80.dp else 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
          app.error?.let { message ->
            Snackbar(modifier = Modifier.widthIn(max = 480.dp).padding(horizontal = 16.dp).reservePlayerOverlayTouchArea().semantics { liveRegion = LiveRegionMode.Polite },
              dismissAction = { IconButton(onClick = model::dismissError) { PilotIcon(R.drawable.ic_x, stringResource(R.string.close)) } },
            ) { Text(message) }
          }
          if (!phone && playback?.skipUndoAvailable == true) SkipUndo(model, playback.skipUndoId, Modifier)
        }
        if (phone && playback?.skipUndoAvailable == true) {
          SkipUndo(model, playback.skipUndoId,
            Modifier.align(Alignment.BottomEnd).windowInsetsPadding(WindowInsets.displayCutout)
              .padding(end = 24.dp, bottom = if (controls) bottomHeight + 24.dp else 24.dp), phone = true)
        }
      }
      panel?.let { selected ->
        if (landscape) Dialog(onDismissRequest = chrome::back, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
          PlayerDialogSystemBars()
          Box(Modifier.fillMaxSize()) {
            Box(Modifier.matchParentSize().background(PilotPlayerTokens.backdrop).clickable(onClick = chrome::close))
            Surface(Modifier.align(Alignment.CenterEnd).windowInsetsPadding(WindowInsets.displayCutout).padding(16.dp)
              .width(panelWidth).fillMaxHeight(), shape = MaterialTheme.shapes.large, color = MaterialTheme.colorScheme.surfaceContainer) {
              PlayerPanelContent(selected, snapshot, playback, model, chrome::close,
                open = chrome::open,
                back = if (chrome.panelOrigin != null && selected != chrome.panelOrigin) chrome::back else null,
                restoreRow = chrome.restoreRow, phone = phone)
            }
          }
        } else ModalBottomSheet(onDismissRequest = chrome::close,
          properties = ModalBottomSheetProperties(shouldDismissOnBackPress = false),
          containerColor = MaterialTheme.colorScheme.surfaceContainer) {
          BackHandler { chrome.back() }
          PlayerDialogSystemBars()
          Box(Modifier.fillMaxWidth().heightIn(max = panelHeight)) {
            PlayerPanelContent(selected, snapshot, playback, model, chrome::close, open = chrome::open,
              back = if (chrome.panelOrigin != null && selected != chrome.panelOrigin) chrome::back else null,
              restoreRow = chrome.restoreRow, phone = phone)
          }
        }
      }
    }
  }
}

@Composable
private fun PanelButton(panel: PlayerPanel, icon: Int, title: String, triggers: Map<PlayerPanel, FocusRequester>,
  enabled: Boolean, labelled: Boolean = false, open: (PlayerPanel) -> Unit) {
  val tint = if (enabled) PilotPlayerTokens.foreground else PilotPlayerTokens.disabled
  if (labelled) TextButton(onClick = { open(panel) }, enabled = enabled,
    modifier = Modifier.widthIn(min = 72.dp).heightIn(min = 48.dp).focusRequester(triggers.getValue(panel)),
    contentPadding = PaddingValues(horizontal = 4.dp), colors = ButtonDefaults.textButtonColors(contentColor = tint),
  ) {
    PilotIcon(icon, tint = tint, modifier = Modifier.size(20.dp))
    Spacer(Modifier.width(6.dp))
    Text(title, style = MaterialTheme.typography.labelLarge)
  } else IconButton(onClick = { open(panel) }, enabled = enabled,
    modifier = Modifier.size(48.dp).focusRequester(triggers.getValue(panel))) {
    PilotIcon(icon, title, tint = if (enabled) PilotPlayerTokens.secondary else PilotPlayerTokens.disabled)
  }
}

@Composable
private fun SkipUndo(model: AppViewModel, noticeId: Long, modifier: Modifier, phone: Boolean = false) {
  var focused by remember { mutableStateOf(false) }
  val touchExploration = rememberTouchExploration()
  var remaining by remember(noticeId) { mutableLongStateOf(8_000L) }
  LaunchedEffect(noticeId, focused, touchExploration) {
    if (!focused && !touchExploration) {
      while (remaining > 0) { delay(250); remaining -= 250 }
      model.dismissSkipUndo()
    }
  }
  Surface(modifier = modifier.then(if (phone) Modifier.widthIn(max = 264.dp) else Modifier.padding(horizontal = 16.dp).widthIn(max = 360.dp))
    .reservePlayerOverlayTouchArea().onFocusChanged { focused = it.hasFocus }.focusGroup().semantics { liveRegion = LiveRegionMode.Polite },
    color = if (phone) PilotPlayerTokens.phoneFeedback else PilotPlayerTokens.notice,
    contentColor = PilotPlayerTokens.foreground, shape = MaterialTheme.shapes.medium,
  ) {
    Row(Modifier.padding(start = 16.dp, end = if (phone) 8.dp else 16.dp, top = if (phone) 4.dp else 12.dp, bottom = if (phone) 4.dp else 12.dp), verticalAlignment = Alignment.CenterVertically) {
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
