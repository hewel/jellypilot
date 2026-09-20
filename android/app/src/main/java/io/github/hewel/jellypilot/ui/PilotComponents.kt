package io.github.hewel.jellypilot.ui

import androidx.annotation.DrawableRes
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import coil3.request.ImageRequest
import io.github.hewel.jellypilot.R
import kotlin.math.ceil

@Composable
internal fun PilotIcon(@DrawableRes icon: Int, description: String? = null, modifier: Modifier = Modifier, tint: Color = LocalContentColor.current) {
  Icon(painterResource(icon), description, modifier.size(22.dp), tint)
}

@Composable
internal fun Artwork(artwork: ArtworkUi?, title: String?, modifier: Modifier, rounded: Boolean = true) {
  val context = LocalContext.current
  val request = remember(context, artwork) {
    artwork?.let { ImageRequest.Builder(context).data(it).diskCacheKey(it.cacheKey).memoryCacheKey(it.cacheKey).build() }
  }
  val shape = if (rounded) Modifier.clip(MaterialTheme.shapes.medium) else Modifier
  Box(modifier.then(shape).background(MaterialTheme.colorScheme.surfaceContainerHigh), contentAlignment = Alignment.Center) {
    PilotIcon(R.drawable.ic_movie, if (request == null) title else null, tint = LocalPilotColors.current.metadata)
    if (request != null) AsyncImage(request, title, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
  }
}

@Composable
internal fun Poster(item: MediaUi, modifier: Modifier = Modifier, selected: Boolean? = null, enabled: Boolean = true, open: () -> Unit) {
  val enlarged = LocalDensity.current.fontScale > 1.3f
  Column(
    modifier.clip(MaterialTheme.shapes.medium).clickable(enabled = enabled, onClick = open).semantics(mergeDescendants = true) {
      if (selected != null) { this.selected = selected; role = Role.Checkbox }
    }, verticalArrangement = Arrangement.spacedBy(6.dp),
  ) {
    Box {
      Artwork(item.artwork, null, Modifier.fillMaxWidth().aspectRatio(2f / 3f))
      if (item.progress > 0f) LinearProgressIndicator(
        progress = { item.progress }, modifier = Modifier.fillMaxWidth().height(3.dp).align(Alignment.BottomCenter),
        color = MaterialTheme.colorScheme.primary, trackColor = MaterialTheme.colorScheme.surfaceContainerHigh,
        drawStopIndicator = {},
      )
      if (selected != null) Box(Modifier.size(48.dp).align(Alignment.TopEnd), contentAlignment = Alignment.Center) {
        Surface(shape = CircleShape, color = if (selected) LocalPilotColors.current.action else MaterialTheme.colorScheme.surface.copy(alpha = 0.9f)) {
          PilotIcon(if (selected) R.drawable.ic_circle_check else R.drawable.ic_circle, modifier = Modifier.padding(4.dp), tint = if (selected) Color.White else MaterialTheme.colorScheme.onSurface)
        }
      } else if (item.played) PilotIcon(R.drawable.ic_circle_check, stringResource(R.string.watched), Modifier.align(Alignment.TopEnd).padding(6.dp), tint = MaterialTheme.colorScheme.tertiary)
    }
    Text(item.title, minLines = 2, maxLines = if (enlarged) Int.MAX_VALUE else 2, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.labelLarge)
    Text(mediaCaption(item), maxLines = if (enlarged) 3 else 1, overflow = TextOverflow.Ellipsis, color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall)
  }
}

@Composable
internal fun PosterPlaceholder(modifier: Modifier = Modifier) {
  val titleHeight = with(LocalDensity.current) { MaterialTheme.typography.labelLarge.lineHeight.toDp() }
  val metadataHeight = with(LocalDensity.current) { MaterialTheme.typography.labelSmall.lineHeight.toDp() }
  Column(modifier, verticalArrangement = Arrangement.spacedBy(6.dp)) {
    Box(Modifier.fillMaxWidth().aspectRatio(2f / 3f).clip(MaterialTheme.shapes.medium).background(MaterialTheme.colorScheme.surfaceContainerHigh))
    Spacer(Modifier.height(titleHeight * 2))
    Spacer(Modifier.height(metadataHeight))
  }
}

@Composable
internal fun mediaCaption(item: MediaUi): String {
  val remaining = item.durationSeconds?.takeIf { it > item.resumeSeconds && item.resumeSeconds > 0 }
    ?.let { stringResource(R.string.remaining_minutes, ceil((it - item.resumeSeconds) / 60.0).toInt()) }
  return listOfNotNull(item.episodeCode, remaining ?: item.metadata.takeIf { it.isNotBlank() }).joinToString(" · ")
}

@Composable
internal fun playbackLabel(item: MediaUi): String {
  val code = item.episodeCode
  return when {
    item.played -> if (code == null) stringResource(R.string.replay) else stringResource(R.string.replay_episode, code)
    item.resumeSeconds > 0 -> if (code == null) stringResource(R.string.resume) else stringResource(R.string.resume_episode, code)
    else -> if (code == null) stringResource(R.string.play) else stringResource(R.string.episode_play, code)
  }
}

@Composable
internal fun PlayAction(item: MediaUi, modifier: Modifier = Modifier, play: () -> Unit) {
  val enabled = item.playable && !item.updating
  Surface(
    onClick = play, enabled = enabled,
    modifier = modifier.heightIn(min = 48.dp), shape = MaterialTheme.shapes.medium,
    color = LocalPilotColors.current.action.copy(alpha = if (enabled) 1f else 0.45f), contentColor = Color.White.copy(alpha = if (enabled) 1f else 0.6f),
  ) {
    Box(contentAlignment = Alignment.Center) {
      if (item.progress > 0f && !item.played) Box(Modifier.matchParentSize()) {
        Box(Modifier.fillMaxHeight().fillMaxWidth(item.progress).background(Color.White.copy(alpha = 0.12f)))
      }
      Row(Modifier.padding(horizontal = 16.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        PilotIcon(R.drawable.ic_player_play)
        Text(playbackLabel(item), style = MaterialTheme.typography.labelLarge)
      }
    }
  }
}

@Composable
internal fun EmptyState(title: String, hint: String? = null, action: String? = null, modifier: Modifier = Modifier, @DrawableRes icon: Int = R.drawable.ic_movie, onAction: () -> Unit = {}) {
  Column(modifier.fillMaxWidth().padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
    PilotIcon(icon, modifier = Modifier.size(40.dp), tint = LocalPilotColors.current.metadata)
    Text(title, style = MaterialTheme.typography.titleMedium)
    hint?.let { Text(it, color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodyMedium) }
    action?.let { FilledTonalButton(onClick = onAction, modifier = Modifier.heightIn(min = 48.dp)) { Text(it) } }
  }
}

@Composable
internal fun SectionHeading(title: String, action: String? = null, onAction: () -> Unit = {}) {
  Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp).heightIn(min = 48.dp), verticalAlignment = Alignment.CenterVertically) {
    Text(title, Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
    if (action != null) TextButton(onClick = onAction, modifier = Modifier.heightIn(min = 48.dp)) { Text(action) }
  }
}

@Composable
internal fun MediaLogo(artwork: ArtworkUi, title: String, modifier: Modifier = Modifier) {
  val context = LocalContext.current
  val request = remember(context, artwork) {
    ImageRequest.Builder(context).data(artwork).diskCacheKey(artwork.cacheKey).memoryCacheKey(artwork.cacheKey).build()
  }
  var loaded by remember(artwork) { mutableStateOf(false) }
  Box(modifier) {
    if (!loaded) Text(title, color = Color.White, style = MaterialTheme.typography.headlineLarge)
    AsyncImage(request, title, Modifier.fillMaxWidth().height(90.dp), contentScale = ContentScale.Fit, alignment = Alignment.CenterStart, onSuccess = { loaded = true })
  }
}
