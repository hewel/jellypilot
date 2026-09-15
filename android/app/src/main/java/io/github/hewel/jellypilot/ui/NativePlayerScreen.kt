@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.hewel.jellypilot.ui

import android.net.Uri
import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.hewel.jellypilot.NativePlayback
import io.github.hewel.jellypilot.R
import io.github.hewel.jellypilot.player.PlayerHost
import io.github.hewel.jellypilot.player.PlayerStatus
import io.github.hewel.jellypilot.player.TrackKind
import java.util.Locale

@Composable
internal fun NativePlayerScreen(player: NativePlayback, back: () -> Unit) {
  val snapshot by player.snapshot.collectAsStateWithLifecycle()
  val ready by player.ready.collectAsStateWithLifecycle()
  val error by player.error.collectAsStateWithLifecycle()
  var url by remember { mutableStateOf("") }
  var mediaFile by remember { mutableStateOf<Uri?>(null) }
  var subtitleFile by remember { mutableStateOf<Uri?>(null) }
  var trackKind by remember { mutableStateOf<TrackKind?>(null) }
  var seekPosition by remember { mutableStateOf<Float?>(null) }
  val mediaPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { result ->
    if (result != null) { mediaFile = result; url = "" }
  }
  val subtitlePicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { result -> subtitleFile = result }
  Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing).verticalScroll(rememberScrollState())) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
      IconButton(onClick = back) { Icon(painterResource(R.drawable.ic_chevron_left), stringResource(R.string.back)) }
      Text(stringResource(R.string.native_playback), Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
    }
    val ratio = if (snapshot.videoWidth > 0 && snapshot.videoHeight > 0) snapshot.videoWidth.toFloat() / snapshot.videoHeight else 16f / 9f
    AndroidView(
      modifier = Modifier.fillMaxWidth().aspectRatio(ratio.coerceIn(0.5f, 3f)),
      factory = { context ->
        SurfaceView(context).apply {
          holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) { player.attach(holder.surface) }
            override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) { player.attach(holder.surface) }
            override fun surfaceDestroyed(holder: SurfaceHolder) { player.detach() }
          })
        }
      },
      update = { it.keepScreenOn = snapshot.isPlaying && snapshot.admissionEligible },
      onRelease = { player.detach() },
    )
    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
      Text(stringResource(R.string.native_playback_explanation), color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall)
      if (!ready && error == null) LinearProgressIndicator(Modifier.fillMaxWidth())
      (error ?: snapshot.error?.message)?.let { Text(it, color = MaterialTheme.colorScheme.error) }
      if (!snapshot.admissionEligible && snapshot.status != PlayerStatus.IDLE) {
        Text(stringResource(R.string.player_paused_background), color = LocalPilotColors.current.metadata)
      }
      Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilledIconButton(
          onClick = { if (snapshot.paused) player.play() else player.pause() },
          enabled = ready && snapshot.status != PlayerStatus.IDLE && snapshot.status != PlayerStatus.LOADING,
        ) {
          Icon(painterResource(if (snapshot.paused) R.drawable.ic_player_play else R.drawable.ic_player_pause), stringResource(if (snapshot.paused) R.string.play else R.string.pause))
        }
        IconButton(onClick = player::stop, enabled = ready && snapshot.status != PlayerStatus.IDLE) { Icon(painterResource(R.drawable.ic_player_stop), stringResource(R.string.stop)) }
        Text(clock(snapshot.positionSeconds), style = MaterialTheme.typography.labelLarge)
        Text("/", color = LocalPilotColors.current.metadata)
        Text(snapshot.durationSeconds?.let { clock(it) } ?: "—", style = MaterialTheme.typography.labelLarge)
      }
      snapshot.durationSeconds?.takeIf { it > 0 && it.isFinite() }?.let { duration ->
        Text(stringResource(R.string.seek), style = MaterialTheme.typography.labelMedium)
        Slider(
          value = seekPosition ?: snapshot.positionSeconds.toFloat().coerceIn(0f, duration.toFloat()),
          onValueChange = { seekPosition = it },
          onValueChangeFinished = { seekPosition?.let { player.seek(it.toDouble()) }; seekPosition = null },
          valueRange = 0f..duration.toFloat(), enabled = ready && snapshot.status != PlayerStatus.LOADING,
        )
      }
      Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilledTonalButton(onClick = { trackKind = TrackKind.AUDIO }, enabled = snapshot.tracks.any { it.kind == TrackKind.AUDIO }) { Text(stringResource(R.string.audio_tracks)) }
        FilledTonalButton(onClick = { trackKind = TrackKind.SUBTITLE }, enabled = snapshot.tracks.any { it.kind == TrackKind.SUBTITLE }) { Text(stringResource(R.string.subtitle_tracks)) }
      }
      Text(stringResource(R.string.volume), style = MaterialTheme.typography.labelMedium)
      Slider(snapshot.volumePercent.toFloat(), { player.volume(it.toInt()) }, valueRange = 0f..100f, enabled = ready)
      HorizontalDivider()
      OutlinedTextField(
        url, { url = it; mediaFile = null }, Modifier.fillMaxWidth(), singleLine = true,
        label = { Text(stringResource(R.string.media_url)) },
      )
      mediaFile?.let { Text(stringResource(R.string.media_file_selected), color = LocalPilotColors.current.metadata) }
      Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilledTonalButton(onClick = { mediaPicker.launch(arrayOf("video/*", "audio/*")) }) { Text(stringResource(R.string.open_media_file)) }
        FilledTonalButton(onClick = { subtitlePicker.launch(arrayOf("text/*", "application/x-subrip", "application/octet-stream")) }) { Text(stringResource(R.string.open_subtitle_file)) }
      }
      if (subtitleFile != null) {
        Row(verticalAlignment = Alignment.CenterVertically) {
          Text(stringResource(R.string.subtitle_for_next_load), Modifier.weight(1f), style = MaterialTheme.typography.bodySmall)
          IconButton(onClick = { subtitleFile = null }) { Icon(painterResource(R.drawable.ic_x), stringResource(R.string.close)) }
        }
      }
      Button(
        onClick = { mediaFile?.let { player.loadFile(it, subtitleFile) } ?: player.loadUrl(url, subtitleFile) },
        enabled = ready && snapshot.admissionEligible && (mediaFile != null || url.isNotBlank()) && snapshot.status != PlayerStatus.LOADING,
        modifier = Modifier.fillMaxWidth(),
      ) { Text(stringResource(R.string.load_media)) }
    }
  }
  trackKind?.let { kind ->
    ModalBottomSheet(onDismissRequest = { trackKind = null }) {
      Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(16.dp)) {
        Text(stringResource(if (kind == TrackKind.AUDIO) R.string.audio_tracks else R.string.subtitle_tracks), style = MaterialTheme.typography.titleLarge)
        if (kind == TrackKind.SUBTITLE) {
          TextButton(onClick = { player.select(kind, PlayerHost.TRACK_ID_NONE); trackKind = null }) { Text(stringResource(R.string.subtitles_off)) }
        }
        snapshot.tracks.filter { it.kind == kind }.forEach { track ->
          TextButton(onClick = { player.select(kind, track.mpvId); trackKind = null }, modifier = Modifier.fillMaxWidth()) {
            RadioButton(track.isSelected, null)
            Text(listOfNotNull(track.title, track.language, track.codec).filter { it.isNotBlank() }.joinToString(" · ").ifBlank { "${track.mpvId}" }, Modifier.weight(1f))
          }
        }
      }
    }
  }
}

@Composable
private fun clock(seconds: Double): String {
  val whole = seconds.takeIf { it.isFinite() && it >= 0 }?.toLong() ?: 0L
  return remember(whole) { String.format(Locale.ROOT, "%d:%02d:%02d", whole / 3600, whole / 60 % 60, whole % 60) }
}
