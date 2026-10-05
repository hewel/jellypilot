@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.hewel.jellypilot.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import io.github.hewel.jellypilot.AppViewModel
import io.github.hewel.jellypilot.R

@Composable
internal fun AccountScreen(state: AppUiState, model: AppViewModel) {
  val actions = AccountActions(model::openAccountPage, model::activate,
    model::disconnect, model::signOut, model::addAccount, model::updatePreferences, model::retryCleanup,
    model::retryWatchlistCleanup, model::retryConnectionCheck, model::openRemoteController)
  Column(Modifier.fillMaxSize()) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
      if (state.accountPage != AccountPage.Overview) IconButton(onClick = model::back) { PilotIcon(R.drawable.ic_chevron_left, stringResource(R.string.back)) }
      Text(stringResource(state.accountPage.title), Modifier.padding(horizontal = 8.dp, vertical = 16.dp), style = if (state.accountPage == AccountPage.Overview) MaterialTheme.typography.headlineSmall else MaterialTheme.typography.sectionHeading)
    }
    when (state.accountPage) {
      AccountPage.Overview -> AccountOverview(state, actions)
      AccountPage.Settings -> AccountSettings(state, model::openAccountPage)
      AccountPage.CurrentAccount -> CurrentAccount(state, actions)
      AccountPage.Connections -> AccountOverview(state, actions, connectionsOnly = true)
      AccountPage.Appearance -> AppearanceSettings(state.preferences, model::updatePreferences)
      AccountPage.Playback -> PlaybackSettings(state.preferences, model::updatePreferences)
      AccountPage.Subtitles -> SubtitleSettings(state.preferences, model::updatePreferences)
      AccountPage.Storage -> StorageSettings(model)
      AccountPage.Diagnostics -> Diagnostics(model)
      AccountPage.History -> HistoryScreen(state, model)
    }
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
  var targetName by rememberSaveable(preferences.targetName) { mutableStateOf(preferences.targetName) }
  Column(Modifier.verticalScroll(rememberScrollState()).imePadding().padding(16.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
    AccountGroup(settings = true) {
      SettingToggle(stringResource(R.string.auto_play_next), preferences.autoPlayNext) { change(preferences.copy(autoPlayNext = it)) }
      SettingToggle(stringResource(R.string.original_audio), preferences.preferOriginalAudio) { change(preferences.copy(preferOriginalAudio = it)) }
      SettingToggle(stringResource(R.string.remember_season_volume), preferences.rememberSeasonVolume) { change(preferences.copy(rememberSeasonVolume = it)) }
    }
    ChoiceSetting(stringResource(R.string.intro_mode), IntroPreference.entries, preferences.introMode, { stringResource(it.title) }) { change(preferences.copy(introMode = it)) }
    OutlinedTextField(targetName, { targetName = it }, Modifier.fillMaxWidth(), singleLine = true, label = { Text(stringResource(R.string.target_name)) }, supportingText = { Text(stringResource(R.string.target_name_hint)) })
    Button(onClick = { change(preferences.copy(targetName = targetName.trim())) }, enabled = targetName.isNotBlank(), modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.save)) }
  }
}

@Composable
private fun SubtitleSettings(preferences: PreferencesUi, change: (PreferencesUi) -> Unit) {
  var languages by rememberSaveable(preferences.preferredSubtitleLanguage) { mutableStateOf(preferences.preferredSubtitleLanguage) }
  Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).imePadding().padding(16.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
    OutlinedTextField(languages, { languages = it }, Modifier.fillMaxWidth(), singleLine = true,
      label = { Text(stringResource(R.string.subtitle_language)) }, supportingText = { Text(stringResource(R.string.language_hint)) })
    Button(onClick = { change(preferences.copy(preferredSubtitleLanguage = languages.trim())) }, modifier = Modifier.fillMaxWidth()) {
      Text(stringResource(R.string.save))
    }
  }
}

@Composable
private fun StorageSettings(model: AppViewModel) {
  Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
    Text(stringResource(R.string.image_cache), style = MaterialTheme.typography.titleMedium)
    Text(stringResource(R.string.image_cache_hint), color = LocalPilotColors.current.metadata)
    FilledTonalButton(onClick = model::clearImageCache) { Text(stringResource(R.string.clear_cache)) }
  }
}

@Composable
private fun Diagnostics(model: AppViewModel) {
  Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
    Text(stringResource(R.string.diagnostics_hint), color = LocalPilotColors.current.metadata)
    FilledTonalButton(onClick = model::exportDiagnostics) { Text(stringResource(R.string.export_diagnostics)) }
    Text(stringResource(R.string.about), style = MaterialTheme.typography.titleMedium)
    Text(stringResource(R.string.font_attribution), style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.metadata)
  }
}

@Composable
internal fun SettingToggle(title: String, checked: Boolean, hint: String? = null, change: (Boolean) -> Unit) {
  Row(Modifier.fillMaxWidth().toggleable(checked, role = Role.Switch, onValueChange = change).heightIn(min = 48.dp).padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
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
      Row(Modifier.fillMaxWidth().selectable(selected == choice, role = Role.RadioButton, onClick = { select(choice) }).heightIn(min = 48.dp), verticalAlignment = Alignment.CenterVertically) {
        RadioButton(selected == choice, null)
        Text(label(choice), style = MaterialTheme.typography.bodyMedium)
      }
    }
  }
}
