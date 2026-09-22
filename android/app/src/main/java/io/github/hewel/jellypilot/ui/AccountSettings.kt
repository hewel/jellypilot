package io.github.hewel.jellypilot.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.hewel.jellypilot.BuildConfig
import io.github.hewel.jellypilot.R

@Composable
internal fun AccountSettings(state: AppUiState, open: (AccountPage) -> Unit) {
  LazyColumn(contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(24.dp)) {
    item {
      SettingsSection(stringResource(R.string.account_settings_group)) {
        AccountIdentity(state, compact = true)
        AccountDivider()
        AccountLink(stringResource(R.string.account_switch_connection)) { open(AccountPage.Connections) }
        AccountDivider()
        AccountLink(stringResource(R.string.account_current)) { open(AccountPage.CurrentAccount) }
      }
    }
    item {
      SettingsSection(stringResource(R.string.account_playback_group)) {
        AccountLink(stringResource(R.string.playback_preferences)) { open(AccountPage.Playback) }
        AccountDivider()
        AccountLink(stringResource(R.string.subtitle_tracks)) { open(AccountPage.Subtitles) }
        AccountDivider()
        AccountLink(stringResource(R.string.watch_history)) { open(AccountPage.History) }
      }
    }
    item {
      SettingsSection(stringResource(R.string.account_general_group)) {
        AccountLink(stringResource(R.string.appearance), subtitle = stringResource(state.preferences.theme.title)) { open(AccountPage.Appearance) }
        AccountDivider()
        AccountLink(stringResource(R.string.storage)) { open(AccountPage.Storage) }
        AccountDivider()
        AccountLink(stringResource(R.string.diagnostics)) { open(AccountPage.Diagnostics) }
      }
    }
    item {
      Text(stringResource(R.string.account_version, BuildConfig.VERSION_NAME), Modifier.fillMaxWidth().padding(bottom = 16.dp),
        style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.metadata, textAlign = TextAlign.Center)
    }
  }
}

@Composable
private fun SettingsSection(title: String, content: @Composable ColumnScope.() -> Unit) {
  Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
    Text(title, Modifier.padding(horizontal = 4.dp).semantics { heading() }, style = MaterialTheme.typography.bodySmall,
      color = LocalPilotColors.current.metadata)
    AccountGroup(settings = true, content = content)
  }
}
