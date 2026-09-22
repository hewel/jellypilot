package io.github.hewel.jellypilot.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import io.github.hewel.jellypilot.R

@Composable
internal fun LibraryControls(
  state: AppUiState,
  selectLibrary: (String) -> Unit,
  sort: (LibrarySort) -> Unit,
  filter: (PlayedFilter, Boolean) -> Unit,
  search: () -> Unit,
) {
  var librariesOpen by remember { mutableStateOf(false) }
  var sortOpen by remember { mutableStateOf(false) }
  val enlarged = LocalDensity.current.fontScale > 1.3f
  val count = state.browser.totalCount.coerceAtMost(Int.MAX_VALUE.toUInt()).toInt()
  val title: @Composable (Modifier) -> Unit = { modifier ->
    Column(modifier) {
      Box {
        TextButton(
          onClick = { librariesOpen = true }, enabled = state.libraries.size > 1,
          modifier = Modifier.heightIn(min = 48.dp), contentPadding = PaddingValues(0.dp),
          colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.onSurface, disabledContentColor = MaterialTheme.colorScheme.onSurface),
        ) {
          Text(state.libraries.firstOrNull { it.id == state.libraryId }?.title ?: stringResource(R.string.library), Modifier.weight(1f, fill = false), style = MaterialTheme.typography.headlineSmall)
          if (state.libraries.size > 1) {
            Spacer(Modifier.width(6.dp))
            PilotIcon(R.drawable.ic_chevron_down)
          }
        }
        DropdownMenu(librariesOpen, { librariesOpen = false }) {
          state.libraries.forEach { library ->
            DropdownMenuItem(
              text = { Text(library.title) }, onClick = { selectLibrary(library.id); librariesOpen = false },
              trailingIcon = { if (library.id == state.libraryId) PilotIcon(R.drawable.ic_check) },
            )
          }
        }
      }
      Text(pluralStringResource(if (state.libraryPlayed != PlayedFilter.All || state.libraryFavorites) R.plurals.filtered_count else R.plurals.items_count, count, count),
        color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall)
    }
  }
  val tools: @Composable RowScope.() -> Unit = {
    IconButton(onClick = search, modifier = Modifier.size(48.dp).clip(MaterialTheme.shapes.medium).background(MaterialTheme.colorScheme.surface)) {
      PilotIcon(R.drawable.ic_search, stringResource(R.string.search))
    }
    Box {
      TextButton(onClick = { sortOpen = true }, contentPadding = PaddingValues(horizontal = 4.dp), modifier = Modifier.heightIn(min = 48.dp),
        colors = ButtonDefaults.textButtonColors(contentColor = LocalPilotColors.current.body)) {
        Text(stringResource(state.librarySort.title), style = MaterialTheme.typography.labelMedium)
        Spacer(Modifier.width(6.dp))
        PilotIcon(R.drawable.ic_sort_descending)
      }
      DropdownMenu(sortOpen, { sortOpen = false }) {
        LibrarySort.entries.forEach { value ->
          DropdownMenuItem(text = { Text(stringResource(value.title)) }, onClick = { sort(value); sortOpen = false },
            trailingIcon = { if (value == state.librarySort) PilotIcon(R.drawable.ic_check) })
        }
      }
    }
  }
  Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp).padding(top = 8.dp, bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
    if (enlarged) Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
      title(Modifier.fillMaxWidth())
      Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp), content = tools)
    } else Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
      title(Modifier.weight(1f))
      tools()
    }
    LibraryFilters(state.libraryPlayed, state.libraryFavorites, filter)
  }
}

@Composable
internal fun LibraryFilters(played: PlayedFilter, favorites: Boolean, change: (PlayedFilter, Boolean) -> Unit) {
  val density = LocalDensity.current
  val labels = PlayedFilter.entries.map { stringResource(it.title) }
  val favoriteLabel = stringResource(R.string.only_favorites)
  val style = MaterialTheme.typography.labelMedium
  val measurer = rememberTextMeasurer()
  val labelWidths = labels.map { with(density) { measurer.measure(it, style).size.width.toDp() } }
  val favoriteWidth = with(density) { measurer.measure(favoriteLabel, style).size.width.toDp() } + 56.dp
  val segmentsWidth = (labelWidths.maxOrNull() ?: 0.dp).plus(8.dp).coerceAtLeast(48.dp) * 3 + 8.dp
  val lineHeight = with(density) { style.lineHeight.toDp() }
  val faceHeight = (lineHeight + 12.dp).coerceAtLeast(28.dp)
  val baseHeight = faceHeight + 8.dp
  val touchHeight = (baseHeight + 12.dp).coerceAtLeast(48.dp)
  val segments: @Composable (Modifier) -> Unit = { modifier ->
    Box(modifier.heightIn(min = touchHeight), contentAlignment = Alignment.Center) {
      Box(Modifier.matchParentSize().padding(vertical = 6.dp).clip(MaterialTheme.shapes.medium).background(LocalPilotColors.current.filterSurface))
      Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min).padding(horizontal = 4.dp).selectableGroup()) {
        PlayedFilter.entries.forEachIndexed { index, value ->
          Box(Modifier.weight(1f).fillMaxHeight().heightIn(min = touchHeight).clip(MaterialTheme.shapes.small)
            .selectable(played == value, role = Role.RadioButton) { change(value, favorites) }, contentAlignment = Alignment.Center) {
            if (played == value) Box(Modifier.matchParentSize().padding(vertical = 10.dp).clip(MaterialTheme.shapes.small).background(MaterialTheme.colorScheme.primaryContainer))
            Text(labels[index], Modifier.padding(horizontal = 4.dp, vertical = 12.dp), style = style,
              color = if (played == value) MaterialTheme.colorScheme.secondary else LocalPilotColors.current.body)
          }
        }
      }
    }
  }
  val favoriteSwitch: @Composable () -> Unit = {
    Row(Modifier.heightIn(min = 48.dp).clip(MaterialTheme.shapes.small)
      .toggleable(favorites, role = Role.Switch) { change(played, it) },
      verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
      PilotIcon(R.drawable.ic_heart, modifier = Modifier.size(16.dp), tint = LocalPilotColors.current.body)
      Text(favoriteLabel, style = style, color = LocalPilotColors.current.body)
      Box(Modifier.width(28.dp).height(16.dp).clip(CircleShape).background(if (favorites) LocalPilotColors.current.action else LocalPilotColors.current.switchOff)) {
        Box(Modifier.align(if (favorites) Alignment.CenterEnd else Alignment.CenterStart).padding(2.dp).size(12.dp).clip(CircleShape).background(MaterialTheme.colorScheme.onPrimary))
      }
    }
  }
  BoxWithConstraints(Modifier.fillMaxWidth()) {
    if (density.fontScale > 1.3f || segmentsWidth + favoriteWidth + 16.dp > maxWidth) {
      Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        segments(Modifier.fillMaxWidth())
        favoriteSwitch()
      }
    } else Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
      segments(Modifier.weight(1f))
      favoriteSwitch()
    }
  }
}
