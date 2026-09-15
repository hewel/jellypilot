@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.hewel.jellypilot.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import coil3.compose.AsyncImage
import coil3.request.ImageRequest
import io.github.hewel.jellypilot.AppViewModel
import io.github.hewel.jellypilot.R

@Composable
internal fun PilotApp(model: AppViewModel, playerContent: @Composable () -> Unit) {
  val state by model.state.collectAsStateWithLifecycle()
  BackHandler(state.detail != null || state.showPlayer) { model.back() }
  PilotTheme {
    Surface(color = MaterialTheme.colorScheme.background) {
      if (state.showPlayer) {
        playerContent()
      } else {
        BoxWithConstraints(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
          val rail = maxWidth >= 600.dp
          Row(Modifier.fillMaxSize()) {
            if (rail) Navigation(state.destination, true, model::navigate)
            Column(Modifier.weight(1f)) {
              Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                if (state.detail != null) {
                  IconButton(onClick = model::back) { Icon(painterResource(R.drawable.ic_chevron_left), stringResource(R.string.back)) }
                }
                Text(stringResource(state.destination.title), Modifier.weight(1f), style = MaterialTheme.typography.headlineSmall)
                IconButton(onClick = model::openPlayer) { Icon(painterResource(R.drawable.ic_player_play), stringResource(R.string.native_playback)) }
                if (state.activeName != null) {
                  IconButton(onClick = model::refresh, enabled = !state.busy) { Icon(painterResource(R.drawable.ic_refresh), stringResource(R.string.refresh)) }
                }
              }
              state.error?.let { error ->
                Surface(color = MaterialTheme.colorScheme.errorContainer, modifier = Modifier.fillMaxWidth()) {
                  Row(Modifier.padding(start = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(error, Modifier.weight(1f), color = MaterialTheme.colorScheme.onErrorContainer)
                    IconButton(onClick = model::dismissError) { Icon(painterResource(R.drawable.ic_x), stringResource(R.string.close)) }
                  }
                }
              }
              Box(Modifier.weight(1f)) {
                when {
                  state.detail != null -> Detail(state.detail!!, state.detailItems, model)
                  state.destination == Destination.Account -> Accounts(state, model)
                  state.activeName == null -> EmptyConnection(model::addAccount)
                  else -> Browse(state, model)
                }
                if (state.busy) LinearProgressIndicator(Modifier.fillMaxWidth().align(Alignment.TopCenter))
              }
              Text(stringResource(R.string.bringup_notice), Modifier.padding(horizontal = 16.dp, vertical = 4.dp), color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.labelSmall)
              if (!rail) Navigation(state.destination, false, model::navigate)
            }
          }
        }
      }
      if (state.showSignIn) SignInSheet(state, model)
    }
  }
}

@Composable
private fun Navigation(selected: Destination, rail: Boolean, navigate: (Destination) -> Unit) {
  @Composable fun entry(destination: Destination, modifier: Modifier) {
    Column(
      modifier.selectable(selected == destination, role = Role.Tab, onClick = { navigate(destination) }),
      horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center,
    ) {
      val tint = if (selected == destination) MaterialTheme.colorScheme.secondary else LocalPilotColors.current.metadata
      Icon(painterResource(destination.icon), null, Modifier.size(20.dp), tint = tint)
      Text(stringResource(destination.title), color = tint, style = MaterialTheme.typography.labelSmall)
    }
  }
  Surface(color = LocalPilotColors.current.sidebar) {
    if (rail) {
      Column(Modifier.width(80.dp).fillMaxHeight().padding(top = 24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Destination.entries.forEach { entry(it, Modifier.fillMaxWidth().height(56.dp)) }
      }
    } else {
      Row(Modifier.fillMaxWidth().height(56.dp)) {
        Destination.entries.forEach { entry(it, Modifier.weight(1f).fillMaxHeight()) }
      }
    }
  }
}

@Composable
private fun EmptyConnection(connect: () -> Unit) {
  Column(Modifier.fillMaxSize().padding(24.dp), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
    Text(stringResource(R.string.not_connected), color = LocalPilotColors.current.body)
    Spacer(Modifier.height(16.dp))
    Button(onClick = connect) { Text(stringResource(R.string.sign_in)) }
  }
}

@Composable
private fun Browse(state: AppUiState, model: AppViewModel) {
  var query by remember { mutableStateOf("") }
  Column(Modifier.fillMaxSize()) {
    if (state.destination == Destination.Search) {
      OutlinedTextField(
        value = query, onValueChange = { query = it; model.search(it) }, singleLine = true,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
        placeholder = { Text(stringResource(R.string.query_hint)) },
        leadingIcon = { Icon(painterResource(R.drawable.ic_search), null) },
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
        keyboardActions = KeyboardActions(onSearch = { model.search(query) }),
      )
    }
    if (state.destination == Destination.Library) {
      LazyRow(contentPadding = PaddingValues(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        items(state.libraries, key = { it.id }) { library ->
          FilterChip(state.libraryId == library.id, { model.selectLibrary(library.id) }, label = { Text(library.title) })
        }
      }
    }
    if (state.items.isEmpty() && !state.busy) {
      Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Text(stringResource(R.string.no_results), color = LocalPilotColors.current.metadata) }
    } else {
      LazyVerticalGrid(
        columns = GridCells.Adaptive(109.dp), modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(16.dp),
      ) {
        items(state.items, key = { it.id }) { item -> Poster(item) { model.showDetail(item.id) } }
        if (state.hasMore) item { TextButton(onClick = model::loadMore, enabled = !state.busy) { Text(stringResource(R.string.load_more)) } }
      }
    }
  }
}

@Composable
private fun Artwork(artwork: ArtworkUi?, title: String, modifier: Modifier) {
  val context = LocalContext.current
  val request = remember(context, artwork) {
    artwork?.let {
      ImageRequest.Builder(context).data(it).diskCacheKey(it.cacheKey).memoryCacheKey(it.cacheKey).build()
    }
  }
  Box(modifier.clip(MaterialTheme.shapes.medium).background(MaterialTheme.colorScheme.surfaceContainerHigh), contentAlignment = Alignment.Center) {
    if (request == null) Icon(painterResource(R.drawable.ic_movie), title, tint = LocalPilotColors.current.metadata)
    else AsyncImage(request, title, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
  }
}

@Composable
private fun Poster(item: MediaUi, open: () -> Unit) {
  Column(Modifier.clickable(onClick = open), verticalArrangement = Arrangement.spacedBy(6.dp)) {
    Artwork(item.artwork, item.title, Modifier.fillMaxWidth().aspectRatio(2f / 3f))
    Text(item.title, maxLines = 2, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.labelLarge)
    Text(item.metadata, maxLines = 1, overflow = TextOverflow.Ellipsis, color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.labelSmall)
  }
}

@Composable
private fun Detail(item: MediaUi, children: List<MediaUi>, model: AppViewModel) {
  LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
    item {
      Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        Artwork(item.artwork, item.title, Modifier.width(120.dp).aspectRatio(2f / 3f))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
          Text(item.title, style = MaterialTheme.typography.headlineSmall)
          Text(item.metadata, color = LocalPilotColors.current.metadata)
          FilledTonalButton(onClick = { model.setFavorite(item.id, !item.favorite) }) {
            Icon(painterResource(if (item.favorite) R.drawable.ic_heart_filled else R.drawable.ic_heart), null, Modifier.size(20.dp))
            Spacer(Modifier.width(8.dp))
            Text(stringResource(if (item.favorite) R.string.unfavorite else R.string.favorite))
          }
          TextButton(onClick = { model.setPlayed(item.id, !item.played) }) { Text(stringResource(if (item.played) R.string.mark_unplayed else R.string.mark_played)) }
        }
      }
    }
    if (item.overview.isNotBlank()) item {
      Text(stringResource(R.string.overview), style = MaterialTheme.typography.titleMedium)
      Spacer(Modifier.height(8.dp))
      Text(item.overview, color = LocalPilotColors.current.body)
    }
    items(children, key = { it.id }) { child ->
      ListItem(
        headlineContent = { Text(child.title) }, supportingContent = { Text(child.metadata) },
        modifier = Modifier.clickable { model.showDetail(child.id) },
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
      )
    }
  }
}

@Composable
private fun Accounts(state: AppUiState, model: AppViewModel) {
  var remove by remember { mutableStateOf<ProfileUi?>(null) }
  LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
    item {
      state.activeName?.let { Text(stringResource(R.string.connected_as, it), style = MaterialTheme.typography.titleMedium) }
      Button(onClick = model::addAccount) { Text(stringResource(R.string.add_account)) }
    }
    item { Text(stringResource(R.string.saved_accounts), style = MaterialTheme.typography.titleLarge) }
    if (state.profiles.isEmpty()) item { Text(stringResource(R.string.no_saved_accounts), color = LocalPilotColors.current.metadata) }
    items(state.profiles, key = { it.key }) { profile ->
      Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(profile.name, style = MaterialTheme.typography.titleMedium)
        Text("${profile.provider} · ${profile.server}", color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
          FilledTonalButton(onClick = { if (profile.active) model.disconnect() else model.activate(profile.key) }, enabled = !state.loginBusy) {
            Text(stringResource(if (profile.active) R.string.disconnect else R.string.activate))
          }
          TextButton(onClick = { remove = profile }, enabled = !state.loginBusy) { Text(stringResource(R.string.sign_out)) }
        }
      }
    }
    if (state.activeName != null && state.profiles.none { it.active }) item { FilledTonalButton(onClick = model::disconnect) { Text(stringResource(R.string.disconnect)) } }
    item { Text(stringResource(R.string.font_attribution), style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.metadata) }
  }
  remove?.let { profile ->
    AlertDialog(
      onDismissRequest = { remove = null }, title = { Text(stringResource(R.string.sign_out)) },
      text = { Text(stringResource(R.string.sign_out_explanation)) },
      confirmButton = { TextButton(onClick = { model.signOut(profile.key); remove = null }) { Text(stringResource(R.string.sign_out)) } },
      dismissButton = { TextButton(onClick = { remove = null }) { Text(stringResource(R.string.cancel)) } },
    )
  }
}

@Composable
private fun SignInSheet(state: AppUiState, model: AppViewModel) {
  var server by remember { mutableStateOf("") }
  var username by remember { mutableStateOf("") }
  var password by remember { mutableStateOf("") }
  var jellyfin by remember { mutableStateOf(true) }
  var rememberAccount by remember { mutableStateOf(true) }
  ModalBottomSheet(onDismissRequest = model::cancelLogin) {
    Column(Modifier.fillMaxWidth().imePadding().verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
      Text(stringResource(R.string.sign_in), style = MaterialTheme.typography.headlineSmall)
      Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilterChip(jellyfin, { jellyfin = true }, enabled = !state.loginBusy, label = { Text("Jellyfin") })
        FilterChip(!jellyfin, { jellyfin = false }, enabled = !state.loginBusy, label = { Text("Emby") })
      }
      OutlinedTextField(server, { server = it }, Modifier.fillMaxWidth(), enabled = !state.loginBusy, singleLine = true, label = { Text(stringResource(R.string.server_url)) }, placeholder = { Text(stringResource(R.string.server_hint)) }, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri))
      OutlinedTextField(username, { username = it }, Modifier.fillMaxWidth(), enabled = !state.loginBusy, singleLine = true, label = { Text(stringResource(R.string.username)) })
      OutlinedTextField(password, { password = it }, Modifier.fillMaxWidth(), enabled = !state.loginBusy, singleLine = true, label = { Text(stringResource(R.string.password)) }, visualTransformation = PasswordVisualTransformation(), keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password))
      Row(verticalAlignment = Alignment.CenterVertically) {
        Checkbox(rememberAccount, { rememberAccount = it }, enabled = !state.loginBusy)
        Text(stringResource(R.string.remember_account))
      }
      state.quickConnectCode?.let { code ->
        Text(code, style = MaterialTheme.typography.displaySmall)
        Text(stringResource(R.string.quick_connect_explanation))
      }
      state.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
      if (state.loginBusy) LinearProgressIndicator(Modifier.fillMaxWidth())
      Button(
        onClick = { model.signIn(jellyfin, server, username, password, rememberAccount); password = "" },
        enabled = !state.loginBusy && server.isNotBlank() && username.isNotBlank(), modifier = Modifier.fillMaxWidth(),
      ) { Text(stringResource(R.string.sign_in)) }
      if (jellyfin) FilledTonalButton(onClick = { model.quickConnect(server, rememberAccount) }, enabled = !state.loginBusy && server.isNotBlank(), modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.quick_connect)) }
      TextButton(onClick = model::cancelLogin, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.cancel)) }
    }
  }
}
