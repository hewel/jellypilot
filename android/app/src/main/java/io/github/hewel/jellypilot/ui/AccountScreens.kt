@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.hewel.jellypilot.ui

import androidx.annotation.DrawableRes
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import io.github.hewel.jellypilot.AppViewModel
import io.github.hewel.jellypilot.R

@Composable
internal fun AccountScreen(state: AppUiState, model: AppViewModel) {
  Column(Modifier.fillMaxSize()) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
      if (state.accountPage != AccountPage.Overview) IconButton(onClick = model::back) { PilotIcon(R.drawable.ic_chevron_left, stringResource(R.string.back)) }
      Text(stringResource(state.accountPage.title), Modifier.padding(horizontal = 8.dp, vertical = 16.dp), style = MaterialTheme.typography.headlineSmall)
    }
    when (state.accountPage) {
      AccountPage.Overview -> AccountOverview(state, model)
      AccountPage.Connections -> Connections(state, model)
      AccountPage.Appearance -> AppearanceSettings(state.preferences, model::updatePreferences)
      AccountPage.Playback -> PlaybackSettings(state.preferences, model::updatePreferences)
      AccountPage.Storage -> StorageSettings(model)
      AccountPage.Diagnostics -> Diagnostics(model)
      AccountPage.History -> HistoryScreen(state, model)
    }
  }
}

@Composable
private fun AccountOverview(state: AppUiState, model: AppViewModel) {
  LazyColumn(contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(24.dp)) {
    item {
      Surface(shape = MaterialTheme.shapes.large, color = MaterialTheme.colorScheme.surfaceContainer) {
        Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
          Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            PilotIcon(R.drawable.ic_user, modifier = Modifier.size(40.dp), tint = MaterialTheme.colorScheme.secondary)
            Column(Modifier.weight(1f)) {
              Text(state.activeName ?: state.profiles.firstOrNull { it.key == state.selectedProfileKey }?.name ?: stringResource(R.string.connection_disconnected), style = MaterialTheme.typography.titleLarge)
              state.profiles.firstOrNull { it.active || it.key == state.selectedProfileKey }?.let { Text(it.server, style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.insetMetadata) }
              if (state.activeName == null && state.selectedProfileKey != null) Text(stringResource(R.string.connection_disconnected), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            }
          }
          if (state.activeName == null && state.selectedProfileKey != null) FilledTonalButton(onClick = { model.activate(state.selectedProfileKey) }, enabled = !state.loginBusy) { Text(stringResource(R.string.retry_connection)) }
          else if (state.activeName == null) Button(onClick = model::addAccount) { Text(stringResource(R.string.sign_in)) }
          else TextButton(onClick = { model.openAccountPage(AccountPage.Connections) }) { Text(stringResource(R.string.connections_hint)) }
        }
      }
    }
    item {
      SettingGroup {
        SettingLink(R.drawable.ic_server, stringResource(R.string.saved_connections), stringResource(R.string.connections_hint)) { model.openAccountPage(AccountPage.Connections) }
        SettingLink(R.drawable.ic_list, stringResource(R.string.watch_history)) { model.openAccountPage(AccountPage.History) }
      }
    }
    item {
      SettingGroup {
        SettingLink(R.drawable.ic_sun, stringResource(R.string.appearance), stringResource(state.preferences.theme.title)) { model.openAccountPage(AccountPage.Appearance) }
        SettingLink(R.drawable.ic_player_play, stringResource(R.string.playback_preferences)) { model.openAccountPage(AccountPage.Playback) }
        SettingLink(R.drawable.ic_database, stringResource(R.string.storage)) { model.openAccountPage(AccountPage.Storage) }
      }
    }
    item {
      SettingGroup { SettingLink(R.drawable.ic_info_circle, stringResource(R.string.diagnostics)) { model.openAccountPage(AccountPage.Diagnostics) } }
      Text(stringResource(R.string.font_attribution), Modifier.padding(top = 16.dp), color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall)
    }
  }
}

@Composable
private fun Connections(state: AppUiState, model: AppViewModel) {
  var remove by remember { mutableStateOf<ProfileUi?>(null) }
  var deleteWatchlist by remember { mutableStateOf(false) }
  LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
    if (state.signOutCleanupPending) item {
      Surface(color = MaterialTheme.colorScheme.errorContainer, shape = MaterialTheme.shapes.medium) {
        Column(Modifier.padding(16.dp)) {
          Text(stringResource(R.string.sdk_sign_out_cleanup_pending), color = MaterialTheme.colorScheme.onErrorContainer)
          TextButton(onClick = model::retryCleanup, enabled = !state.loginBusy) { Text(stringResource(R.string.retry_cleanup)) }
        }
      }
    }
    items(state.profiles.sortedByDescending { it.active || it.key == state.selectedProfileKey }, key = { it.key }) { profile ->
      Surface(shape = MaterialTheme.shapes.medium, color = if (profile.active || profile.key == state.selectedProfileKey) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainerLow) {
        Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
          Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            PilotIcon(R.drawable.ic_server, tint = MaterialTheme.colorScheme.secondary)
            Column(Modifier.weight(1f)) {
              Text(profile.server, style = MaterialTheme.typography.titleMedium)
              Text("${profile.name} · ${profile.provider}", style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.insetMetadata)
              if (!profile.active && profile.key == state.selectedProfileKey) Text(stringResource(R.string.connection_disconnected), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            }
            if (profile.active || profile.key == state.selectedProfileKey) PilotIcon(R.drawable.ic_check, stringResource(R.string.selection_active), tint = MaterialTheme.colorScheme.secondary)
          }
          Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FilledTonalButton(onClick = { if (profile.active) model.disconnect() else model.activate(profile.key) }, enabled = !state.loginBusy) { Text(stringResource(if (profile.active) R.string.disconnect else if (profile.key == state.selectedProfileKey) R.string.retry_connection else R.string.activate)) }
            TextButton(onClick = { remove = profile; deleteWatchlist = false }, enabled = !state.loginBusy) { Text(stringResource(R.string.sign_out)) }
          }
        }
      }
    }
    items(state.watchlistCleanupKeys, key = { "cleanup:$it" }) { key ->
      Surface(shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.errorContainer) {
        Column(Modifier.padding(16.dp)) {
          Text(stringResource(R.string.watchlist_cleanup_pending), color = MaterialTheme.colorScheme.onErrorContainer)
          TextButton(onClick = { model.retryWatchlistCleanup(key) }, enabled = !state.loginBusy) { Text(stringResource(R.string.retry_cleanup)) }
        }
      }
    }
    if (state.profiles.isEmpty()) item { Text(stringResource(R.string.no_saved_accounts), color = LocalPilotColors.current.metadata) }
    item { Button(onClick = model::addAccount, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.connect_server)) } }
    item { SettingToggle(stringResource(R.string.auto_reconnect), state.preferences.startupAutoLogin) { model.updatePreferences(state.preferences.copy(startupAutoLogin = it)) } }
    if (state.activeName != null && state.profiles.none { it.active }) item { FilledTonalButton(onClick = model::disconnect) { Text(stringResource(R.string.disconnect)) } }
  }
  remove?.let { profile ->
    AlertDialog(onDismissRequest = { remove = null }, title = { Text(stringResource(R.string.account_signout, profile.name)) },
      text = {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
          Text(stringResource(R.string.sign_out_explanation))
          Row(Modifier.toggleable(deleteWatchlist, role = Role.Checkbox, onValueChange = { deleteWatchlist = it }), verticalAlignment = Alignment.CenterVertically) {
            Checkbox(deleteWatchlist, null)
            Text(stringResource(R.string.forget_watchlist))
          }
        }
      },
      confirmButton = { TextButton(onClick = { model.signOut(profile.key, deleteWatchlist); remove = null }) { Text(stringResource(R.string.sign_out)) } },
      dismissButton = { TextButton(onClick = { remove = null }) { Text(stringResource(R.string.cancel)) } },
    )
  }
}

@Composable
private fun AppearanceSettings(preferences: PreferencesUi, change: (PreferencesUi) -> Unit) {
  Column(Modifier.verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(24.dp)) {
    ChoiceSetting(stringResource(R.string.theme), ThemePreference.entries, preferences.theme, { stringResource(it.title) }) { change(preferences.copy(theme = it)) }
    ChoiceSetting(stringResource(R.string.language), LanguagePreference.entries, preferences.language, { stringResource(it.title) }) { change(preferences.copy(language = it)) }
    SettingToggle(stringResource(R.string.reduced_motion), preferences.reducedMotion, stringResource(R.string.reduced_motion_hint)) { change(preferences.copy(reducedMotion = it)) }
  }
}

@Composable
private fun PlaybackSettings(preferences: PreferencesUi, change: (PreferencesUi) -> Unit) {
  var languages by rememberSaveable(preferences.preferredSubtitleLanguage) { mutableStateOf(preferences.preferredSubtitleLanguage) }
  var targetName by rememberSaveable(preferences.targetName) { mutableStateOf(preferences.targetName) }
  Column(Modifier.verticalScroll(rememberScrollState()).imePadding().padding(16.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
    SettingGroup {
      SettingToggle(stringResource(R.string.auto_play_next), preferences.autoPlayNext) { change(preferences.copy(autoPlayNext = it)) }
      SettingToggle(stringResource(R.string.original_audio), preferences.preferOriginalAudio) { change(preferences.copy(preferOriginalAudio = it)) }
      SettingToggle(stringResource(R.string.remember_season_volume), preferences.rememberSeasonVolume) { change(preferences.copy(rememberSeasonVolume = it)) }
    }
    ChoiceSetting(stringResource(R.string.intro_mode), IntroPreference.entries, preferences.introMode, { stringResource(it.title) }) { change(preferences.copy(introMode = it)) }
    OutlinedTextField(languages, { languages = it }, Modifier.fillMaxWidth(), singleLine = true, label = { Text(stringResource(R.string.subtitle_language)) }, supportingText = { Text(stringResource(R.string.language_hint)) })
    OutlinedTextField(targetName, { targetName = it }, Modifier.fillMaxWidth(), singleLine = true, label = { Text(stringResource(R.string.target_name)) }, supportingText = { Text(stringResource(R.string.target_name_hint)) })
    Button(onClick = { change(preferences.copy(preferredSubtitleLanguage = languages.trim(), targetName = targetName.trim())) }, enabled = targetName.isNotBlank(), modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.save)) }
  }
}

@Composable
private fun StorageSettings(model: AppViewModel) {
  Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
    Text(stringResource(R.string.image_cache), style = MaterialTheme.typography.titleMedium)
    Text(stringResource(R.string.image_cache_hint), color = LocalPilotColors.current.metadata)
    FilledTonalButton(onClick = model::clearImageCache) { Text(stringResource(R.string.clear_cache)) }
  }
}

@Composable
private fun Diagnostics(model: AppViewModel) {
  Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
    Text(stringResource(R.string.diagnostics_hint), color = LocalPilotColors.current.metadata)
    FilledTonalButton(onClick = model::exportDiagnostics) { Text(stringResource(R.string.export_diagnostics)) }
    Text(stringResource(R.string.about), style = MaterialTheme.typography.titleMedium)
    Text(stringResource(R.string.font_attribution), style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.metadata)
  }
}

@Composable
private fun SettingGroup(content: @Composable ColumnScope.() -> Unit) {
  Surface(shape = MaterialTheme.shapes.large, color = MaterialTheme.colorScheme.surfaceContainerLow) { Column(Modifier.fillMaxWidth(), content = content) }
}

@Composable
private fun SettingLink(@DrawableRes icon: Int, title: String, subtitle: String? = null, open: () -> Unit) {
  Row(Modifier.fillMaxWidth().clickable(onClick = open).heightIn(min = 64.dp).padding(16.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
    PilotIcon(icon, tint = LocalPilotColors.current.metadata)
    Column(Modifier.weight(1f)) {
      Text(title, style = MaterialTheme.typography.bodyMedium)
      subtitle?.let { Text(it, color = LocalPilotColors.current.insetMetadata, style = MaterialTheme.typography.bodySmall) }
    }
    PilotIcon(R.drawable.ic_chevron_right, tint = LocalPilotColors.current.metadata)
  }
}

@Composable
private fun SettingToggle(title: String, checked: Boolean, hint: String? = null, change: (Boolean) -> Unit) {
  Row(Modifier.fillMaxWidth().toggleable(checked, role = Role.Switch, onValueChange = change).padding(16.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
    Column(Modifier.weight(1f)) {
      Text(title, style = MaterialTheme.typography.bodyMedium)
      hint?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.insetMetadata) }
    }
    Switch(checked, null)
  }
}

@Composable
private fun <T> ChoiceSetting(title: String, choices: List<T>, selected: T, label: @Composable (T) -> String, select: (T) -> Unit) {
  Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
    Text(title, style = MaterialTheme.typography.titleMedium)
    choices.forEach { choice ->
      Row(Modifier.fillMaxWidth().clickable { select(choice) }.heightIn(min = 48.dp), verticalAlignment = Alignment.CenterVertically) {
        RadioButton(selected == choice, { select(choice) })
        Text(label(choice), style = MaterialTheme.typography.bodyMedium)
      }
    }
  }
}

@Composable
internal fun SignInSheet(state: AppUiState, model: AppViewModel) {
  var server by rememberSaveable(state.loginServer) { mutableStateOf(state.loginServer) }
  var username by rememberSaveable(state.loginUsername) { mutableStateOf(state.loginUsername) }
  // Passwords stay out of SavedInstanceState and are cleared after submission.
  var password by remember { mutableStateOf("") }
  var jellyfin by rememberSaveable(state.loginJellyfin) { mutableStateOf(state.loginJellyfin) }
  var rememberAccount by rememberSaveable(state.loginRemember) { mutableStateOf(state.loginRemember) }
  ModalBottomSheet(onDismissRequest = model::cancelLogin) {
    Column(Modifier.fillMaxWidth().imePadding().verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
      Row(verticalAlignment = Alignment.CenterVertically) {
        Text(stringResource(R.string.sign_in), Modifier.weight(1f), style = MaterialTheme.typography.headlineSmall)
        IconButton(onClick = model::cancelLogin) { PilotIcon(R.drawable.ic_x, stringResource(R.string.close)) }
      }
      Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilterChip(jellyfin, { jellyfin = true }, enabled = !state.loginBusy, label = { Text("Jellyfin") })
        FilterChip(!jellyfin, { jellyfin = false }, enabled = !state.loginBusy, label = { Text("Emby") })
      }
      OutlinedTextField(server, { server = it }, Modifier.fillMaxWidth(), enabled = !state.loginBusy, singleLine = true, label = { Text(stringResource(R.string.server_url)) }, placeholder = { Text(stringResource(R.string.server_hint)) }, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri))
      OutlinedTextField(username, { username = it }, Modifier.fillMaxWidth(), enabled = !state.loginBusy, singleLine = true, label = { Text(stringResource(R.string.username)) })
      OutlinedTextField(password, { password = it }, Modifier.fillMaxWidth(), enabled = !state.loginBusy, singleLine = true, label = { Text(stringResource(R.string.password)) }, visualTransformation = PasswordVisualTransformation(), keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password))
      Row(Modifier.toggleable(rememberAccount, enabled = !state.loginBusy, role = Role.Checkbox, onValueChange = { rememberAccount = it }), verticalAlignment = Alignment.CenterVertically) {
        Checkbox(rememberAccount, null, enabled = !state.loginBusy)
        Text(stringResource(R.string.remember_account))
      }
      state.quickConnectCode?.let { code -> Text(code, style = MaterialTheme.typography.displaySmall); Text(stringResource(R.string.quick_connect_explanation)) }
      state.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
      if (state.loginBusy) LinearProgressIndicator(Modifier.fillMaxWidth())
      Button(onClick = { model.signIn(jellyfin, server, username, password, rememberAccount); password = "" }, enabled = !state.loginBusy && server.isNotBlank() && username.isNotBlank(), modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.sign_in)) }
      if (jellyfin) FilledTonalButton(onClick = { model.quickConnect(server, rememberAccount) }, enabled = !state.loginBusy && server.isNotBlank(), modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.quick_connect)) }
      TextButton(onClick = model::cancelLogin, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.cancel)) }
    }
  }
}
