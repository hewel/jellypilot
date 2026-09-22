package io.github.hewel.jellypilot.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.hewel.jellypilot.R
import androidx.compose.ui.res.stringResource

internal data class AccountActions(
  val openPage: (AccountPage) -> Unit,
  val openLists: () -> Unit,
  val activate: (String) -> Unit,
  val disconnect: () -> Unit,
  val signOut: (String, Boolean) -> Unit,
  val add: () -> Unit,
  val changePreferences: (PreferencesUi) -> Unit,
  val retryCleanup: () -> Unit,
  val retryWatchlistCleanup: (String) -> Unit,
)

internal fun AppUiState.currentSavedConnection(): ProfileUi? = profiles.firstOrNull { it.active }
  ?: if (activeName == null) profiles.firstOrNull { it.key == selectedProfileKey } else null

@Composable
internal fun AccountOverview(state: AppUiState, actions: AccountActions, connectionsOnly: Boolean = false) {
  var remove by remember { mutableStateOf<ProfileUi?>(null) }
  val current = state.currentSavedConnection()
  LazyColumn(contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
    if (!connectionsOnly) {
      item(key = "identity") {
        AccountGroup {
          AccountIdentity(state, Modifier.clickable(role = Role.Button) { actions.openPage(AccountPage.CurrentAccount) }, showChevron = true)
        }
      }
      item(key = "destinations") {
        AccountGroup {
          AccountLink(stringResource(R.string.personal_lists), R.drawable.ic_bookmark, open = actions.openLists)
          AccountDivider()
          AccountLink(stringResource(R.string.settings), R.drawable.ic_settings) { actions.openPage(AccountPage.Settings) }
        }
      }
    }
    if (state.signOutCleanupPending) item(key = "cleanup") {
      Surface(color = MaterialTheme.colorScheme.errorContainer, shape = MaterialTheme.shapes.medium) {
        Column(Modifier.padding(16.dp)) {
          Text(stringResource(R.string.sdk_sign_out_cleanup_pending), color = MaterialTheme.colorScheme.onErrorContainer)
          TextButton(onClick = actions.retryCleanup, enabled = !state.loginBusy) { Text(stringResource(R.string.retry_cleanup)) }
        }
      }
    }
    item(key = "connections") {
      Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        if (!connectionsOnly) Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
          Text(stringResource(R.string.saved_connections), Modifier.weight(1f).semantics { heading() },
            color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall)
          TextButton(onClick = { actions.openPage(AccountPage.Connections) }) { Text(stringResource(R.string.manage)) }
        }
        if (state.profiles.isEmpty()) {
          Text(stringResource(R.string.no_saved_accounts), Modifier.padding(vertical = 12.dp), color = LocalPilotColors.current.metadata)
        } else AccountGroup {
          state.profiles.sortedByDescending { it.active || (state.activeName == null && it.key == state.selectedProfileKey) }
            .forEachIndexed { index, profile ->
              if (index > 0) AccountDivider()
              SavedConnectionRow(profile, selected = profile.active || (state.activeName == null && profile.key == state.selectedProfileKey),
                enabled = !state.loginBusy, onClick = {
                  if (profile.active) actions.openPage(AccountPage.CurrentAccount) else actions.activate(profile.key)
                })
              if (connectionsOnly) {
                TextButton(onClick = { remove = profile }, enabled = !state.loginBusy, modifier = Modifier.padding(horizontal = 12.dp)) {
                  Text(stringResource(R.string.account_signout, profile.name), color = MaterialTheme.colorScheme.error)
                }
              }
            }
        }
      }
    }
    items(state.watchlistCleanupKeys, key = { "watchlist-cleanup:$it" }) { key ->
      Surface(shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.errorContainer) {
        Column(Modifier.padding(16.dp)) {
          Text(stringResource(R.string.watchlist_cleanup_pending), color = MaterialTheme.colorScheme.onErrorContainer)
          TextButton(onClick = { actions.retryWatchlistCleanup(key) }, enabled = !state.loginBusy) { Text(stringResource(R.string.retry_cleanup)) }
        }
      }
    }
    if (current != null && !current.active) item(key = "retry") {
      TextButton(onClick = { actions.activate(current.key) }, enabled = !state.loginBusy) {
        Text(stringResource(R.string.retry_connection))
      }
    }
    item(key = "actions") {
      AccountGroup {
        SettingToggle(stringResource(R.string.auto_reconnect), state.preferences.startupAutoLogin) {
          actions.changePreferences(state.preferences.copy(startupAutoLogin = it))
        }
        AccountDivider()
        AccountLink(stringResource(R.string.account_add_connection), enabled = !state.loginBusy,
          accent = true, chevron = false, open = actions.add)
        if (current != null) {
          AccountDivider()
          AccountLink(stringResource(R.string.account_signout, current.name), enabled = !state.loginBusy,
            destructive = true, chevron = false) { remove = current }
        } else if (state.activeName != null) {
          AccountDivider()
          AccountLink(stringResource(R.string.disconnect), enabled = !state.loginBusy,
            destructive = true, chevron = false, open = actions.disconnect)
        }
      }
    }
  }
  remove?.let { profile -> AccountSignOutDialog(profile, state.loginBusy, { remove = null }) { deleteWatchlist ->
    actions.signOut(profile.key, deleteWatchlist)
    remove = null
  } }
}

@Composable
internal fun CurrentAccount(state: AppUiState, actions: AccountActions) {
  var removing by remember { mutableStateOf(false) }
  val current = state.currentSavedConnection()
  LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
    item {
      AccountGroup {
        AccountIdentity(state)
        current?.url?.takeIf { it.isNotBlank() }?.let {
          Text(it, Modifier.padding(start = 16.dp, end = 16.dp, bottom = 16.dp), style = MaterialTheme.typography.bodyMedium,
            color = LocalPilotColors.current.insetMetadata)
        }
      }
    }
    item {
      AccountGroup {
        if (state.activeName != null) {
          AccountLink(stringResource(R.string.disconnect), enabled = !state.loginBusy, chevron = false, open = actions.disconnect)
        } else if (current != null) {
          AccountLink(stringResource(R.string.retry_connection), enabled = !state.loginBusy, accent = true, chevron = false) {
            actions.activate(current.key)
          }
        } else AccountLink(stringResource(R.string.account_add_connection), accent = true, open = actions.add)
        if (current != null) {
          AccountDivider()
          AccountLink(stringResource(R.string.account_signout, current.name), enabled = !state.loginBusy,
            destructive = true, chevron = false) { removing = true }
        }
      }
    }
  }
  if (removing && current != null) AccountSignOutDialog(current, state.loginBusy, { removing = false }) { deleteWatchlist ->
    actions.signOut(current.key, deleteWatchlist)
    removing = false
  }
}

@Composable
internal fun SavedConnectionRow(profile: ProfileUi, selected: Boolean, enabled: Boolean, onClick: () -> Unit) {
  val color = if (selected) MaterialTheme.colorScheme.primaryContainer else LocalPilotColors.current.accountSurface
  Row(Modifier.fillMaxWidth().background(color)
    .selectable(selected, enabled = enabled, role = Role.RadioButton, onClick = onClick)
    .heightIn(min = 64.dp).padding(horizontal = 16.dp, vertical = 12.dp),
    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
    AccountAvatar(profile.name, 32.dp, selected)
    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
      Text(profile.server, style = MaterialTheme.typography.titleSmall,
        color = if (selected) MaterialTheme.colorScheme.onPrimaryContainer else LocalPilotColors.current.body)
      val status = when {
        profile.active -> stringResource(R.string.account_current_connection)
        selected -> stringResource(R.string.connection_disconnected)
        else -> profile.provider
      }
      Text(stringResource(R.string.account_connection_identity, profile.name, status), style = MaterialTheme.typography.bodySmall,
        color = if (selected && !profile.active) MaterialTheme.colorScheme.error else LocalPilotColors.current.insetMetadata)
    }
    Box(Modifier.size(20.dp), contentAlignment = Alignment.Center) {
      if (selected) PilotIcon(R.drawable.ic_check, tint = MaterialTheme.colorScheme.secondary)
    }
  }
}

@Composable
internal fun AccountIdentity(state: AppUiState, modifier: Modifier = Modifier, compact: Boolean = false, showChevron: Boolean = false) {
  val current = state.currentSavedConnection()
  val connected = state.activeName != null
  val name = state.activeName ?: current?.name ?: stringResource(R.string.connection_disconnected)
  Row(modifier.fillMaxWidth().padding(if (compact) 12.dp else 16.dp),
    horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
    AccountAvatar(name, if (compact) 36.dp else 52.dp)
    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
      Text(name, style = if (compact) MaterialTheme.typography.titleSmall else MaterialTheme.typography.sectionHeading)
      Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.size(6.dp).background(if (connected) MaterialTheme.colorScheme.tertiary else MaterialTheme.colorScheme.error, CircleShape))
        val status = stringResource(if (connected) R.string.account_connected else R.string.connection_disconnected)
        Text(current?.let { stringResource(R.string.account_server_status, it.server, status) } ?: status,
          style = MaterialTheme.typography.bodySmall,
          color = if (connected) LocalPilotColors.current.insetMetadata else MaterialTheme.colorScheme.error)
      }
    }
    if (showChevron) PilotIcon(R.drawable.ic_chevron_right, tint = LocalPilotColors.current.metadata)
  }
}

@Composable
private fun AccountAvatar(name: String, size: Dp, selected: Boolean = false) {
  Surface(Modifier.size(size), shape = CircleShape,
    color = if (selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surfaceContainerHighest,
    contentColor = if (selected) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface) {
    Box(contentAlignment = Alignment.Center) {
      Text(name.take(1).uppercase(), style = if (size > 36.dp) MaterialTheme.typography.titleLarge else MaterialTheme.typography.titleSmall)
    }
  }
}

@Composable
internal fun AccountGroup(settings: Boolean = false, content: @Composable ColumnScope.() -> Unit) {
  Surface(shape = if (settings) MaterialTheme.shapes.medium else MaterialTheme.shapes.large,
    color = if (settings) LocalPilotColors.current.settingsSurface else LocalPilotColors.current.accountSurface,
    border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)) {
    Column(Modifier.fillMaxWidth(), content = content)
  }
}

@Composable
internal fun AccountDivider() = HorizontalDivider(Modifier.padding(horizontal = 12.dp), color = MaterialTheme.colorScheme.outlineVariant)

@Composable
internal fun AccountLink(title: String, icon: Int? = null, subtitle: String? = null, enabled: Boolean = true,
  accent: Boolean = false, destructive: Boolean = false, chevron: Boolean = true, open: () -> Unit) {
  Row(Modifier.fillMaxWidth().clickable(enabled = enabled, role = Role.Button, onClick = open)
    .heightIn(min = 48.dp).padding(horizontal = 16.dp, vertical = 12.dp),
    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
    icon?.let { PilotIcon(it, modifier = Modifier.size(20.dp), tint = LocalPilotColors.current.metadata) }
    Column(Modifier.weight(1f)) {
      Text(title, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Medium,
        color = when { destructive -> MaterialTheme.colorScheme.error; accent -> MaterialTheme.colorScheme.secondary; else -> LocalPilotColors.current.body })
      subtitle?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = LocalPilotColors.current.insetMetadata) }
    }
    if (chevron) PilotIcon(R.drawable.ic_chevron_right, modifier = Modifier.size(20.dp), tint = LocalPilotColors.current.metadata)
  }
}

@Composable
private fun AccountSignOutDialog(profile: ProfileUi, busy: Boolean, dismiss: () -> Unit, confirm: (Boolean) -> Unit) {
  var deleteWatchlist by remember(profile.key) { mutableStateOf(false) }
  AlertDialog(onDismissRequest = dismiss, title = { Text(stringResource(R.string.account_signout, profile.name)) }, text = {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
      Text(stringResource(R.string.sign_out_explanation))
      Row(Modifier.fillMaxWidth().toggleable(deleteWatchlist, role = Role.Checkbox, onValueChange = { deleteWatchlist = it })
        .heightIn(min = 48.dp), verticalAlignment = Alignment.CenterVertically) {
        Checkbox(deleteWatchlist, null)
        Text(stringResource(R.string.forget_watchlist))
      }
    }
  }, confirmButton = {
    TextButton(onClick = { confirm(deleteWatchlist) }, enabled = !busy) { Text(stringResource(R.string.sign_out)) }
  }, dismissButton = { TextButton(onClick = dismiss) { Text(stringResource(R.string.cancel)) } })
}
