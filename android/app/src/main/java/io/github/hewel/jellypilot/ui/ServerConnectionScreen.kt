package io.github.hewel.jellypilot.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.autofill.ContentType
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.hewel.jellypilot.R

/** A pushed two-step form. The caller retains the source page and removes app navigation. */
@Composable
internal fun ServerConnectionScreen(
  state: AppUiState,
  onBack: () -> Unit,
  onServerChange: (String) -> Unit,
  onUsernameChange: (String) -> Unit,
  onPasswordChange: (String) -> Unit,
  onRememberChange: (Boolean) -> Unit,
  onProviderChange: (Boolean) -> Unit,
  onConnect: () -> Unit,
  onSignIn: () -> Unit,
  onQuickConnect: () -> Unit,
  onManualContinue: () -> Unit,
) {
  val accountStep = state.loginStep == LoginStep.Account && state.loginIdentity != null
  val keyboardOpen = WindowInsets.ime.getBottom(LocalDensity.current) > 0
  val addressFocus = remember { FocusRequester() }
  val passwordFocus = remember { FocusRequester() }
  val focusManager = LocalFocusManager.current
  var previousStep by remember { mutableStateOf(state.loginStep) }
  var showPassword by remember(state.loginIdentity?.address) { mutableStateOf(false) }
  LaunchedEffect(state.loginStep) {
    showPassword = false
    if (previousStep == LoginStep.Account && state.loginStep == LoginStep.Server) addressFocus.requestFocus()
    previousStep = state.loginStep
  }
  LaunchedEffect(state.loginError) { if (state.loginError != null) showPassword = false }
  val submit = {
    if (!state.loginBusy && state.loginUsername.isNotBlank() && state.loginIdentity?.providerSelected == true) {
      focusManager.clearFocus()
      onSignIn()
    }
  }
  Surface(color = MaterialTheme.colorScheme.background) {
    Column(Modifier.fillMaxSize().safeDrawingPadding().imePadding()) {
      Row(Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        IconButton(onClick = onBack, enabled = !state.loginCommitting, modifier = Modifier.size(48.dp)) {
          PilotIcon(R.drawable.ic_chevron_left, stringResource(R.string.back), tint = LocalPilotColors.current.metadata)
        }
        Text(stringResource(R.string.login_add_server), Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
        Text(stringResource(R.string.login_step, if (accountStep) 2 else 1), color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall)
      }
      Column(Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = if (keyboardOpen) 8.dp else 24.dp),
        verticalArrangement = Arrangement.spacedBy(24.dp)) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
          Text(stringResource(if (accountStep) R.string.login_account_title else R.string.connect_server),
            Modifier.semantics { heading() }, style = MaterialTheme.typography.headlineMedium.copy(letterSpacing = (-0.56).sp))
          if (!accountStep) Text(stringResource(R.string.login_server_hint),
            color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodyMedium.copy(lineHeight = 22.sp))
        }
        if (!accountStep) {
          LoginField(state.loginServer, onServerChange, stringResource(R.string.server_url),
            Modifier.focusRequester(addressFocus), enabled = !state.loginBusy, error = state.loginError,
            hint = stringResource(R.string.login_address_hint), keyboardType = KeyboardType.Uri,
            imeAction = ImeAction.Go, onImeAction = { if (!state.loginBusy && state.loginServer.isNotBlank()) onConnect() })
          Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            LoginAction(onConnect, !state.loginBusy && state.loginServer.isNotBlank(), state.loginBusy,
              stringResource(if (state.loginBusy) R.string.login_connecting else if (state.loginError != null) R.string.login_reconnect else R.string.connect_server))
            if (state.loginPublicInfoRestricted) OutlinedButton(onClick = onManualContinue, enabled = !state.loginBusy,
              modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp), shape = MaterialTheme.shapes.medium) {
              Text(stringResource(R.string.login_manual_continue))
            }
            LoginHint(stringResource(R.string.login_address_help))
          }
        } else {
          val identity = requireNotNull(state.loginIdentity)
          Column(verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
              Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                identity.name?.let { Text(it, style = MaterialTheme.typography.titleSmall) }
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                  if (identity.providerKnown) Text(if (identity.jellyfin) "Jellyfin" else "Emby", style = MaterialTheme.typography.bodySmall)
                  val statusColor = when {
                    state.loginConnectionLost -> MaterialTheme.colorScheme.error
                    identity.name == null -> LocalPilotColors.current.metadata
                    else -> MaterialTheme.colorScheme.tertiary
                  }
                  Box(Modifier.size(5.dp).background(statusColor, CircleShape))
                  Text(stringResource(when {
                    state.loginConnectionLost -> R.string.connection_disconnected
                    identity.name == null -> R.string.login_server_unverified
                    else -> R.string.login_connected
                  }), color = statusColor, style = MaterialTheme.typography.bodySmall)
                }
                Text(identity.address, color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall)
              }
              TextButton(onClick = onBack, enabled = !state.loginCommitting, modifier = Modifier.heightIn(min = 48.dp), contentPadding = PaddingValues(horizontal = 8.dp)) {
                Text(stringResource(R.string.login_change_server))
              }
            }
            if (state.loginConnectionLost) state.loginError?.let { message ->
              Text(message, Modifier.semantics { liveRegion = LiveRegionMode.Polite }, color = MaterialTheme.colorScheme.error,
                style = MaterialTheme.typography.bodyMedium.copy(lineHeight = 22.sp))
            }
            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
          }
          if (!identity.providerKnown) Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.login_choose_provider), style = MaterialTheme.typography.bodyMedium)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
              FilterChip(identity.providerSelected && identity.jellyfin, { onProviderChange(true) }, enabled = !state.loginBusy, label = { Text("Jellyfin") })
              FilterChip(identity.providerSelected && !identity.jellyfin, { onProviderChange(false) }, enabled = !state.loginBusy, label = { Text("Emby") })
            }
          }
          Column(verticalArrangement = Arrangement.spacedBy(20.dp)) {
            LoginField(state.loginUsername, onUsernameChange, stringResource(R.string.username), enabled = !state.loginBusy,
              contentType = ContentType.Username, imeAction = ImeAction.Next, onImeAction = { passwordFocus.requestFocus() })
            LoginField(state.loginPassword, onPasswordChange, stringResource(R.string.password), Modifier.focusRequester(passwordFocus),
              enabled = !state.loginBusy, error = state.loginError.takeUnless { state.loginConnectionLost }, contentType = ContentType.Password,
              keyboardType = KeyboardType.Password, imeAction = ImeAction.Go, onImeAction = submit,
              visualTransformation = if (showPassword) VisualTransformation.None else PasswordVisualTransformation(),
              trailing = {
                TextButton(onClick = { showPassword = !showPassword; passwordFocus.requestFocus() }, enabled = !state.loginBusy,
                  modifier = Modifier.heightIn(min = 48.dp).widthIn(min = 48.dp), contentPadding = PaddingValues(horizontal = 8.dp)) {
                  Text(stringResource(if (showPassword) R.string.login_hide_password else R.string.login_show_password), style = MaterialTheme.typography.bodySmall)
                }
              })
          }
          Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).toggleable(state.loginRemember, enabled = !state.loginBusy, role = Role.Checkbox, onValueChange = onRememberChange), verticalAlignment = Alignment.CenterVertically) {
            Checkbox(state.loginRemember, null, enabled = !state.loginBusy)
            Text(stringResource(R.string.remember_account), style = MaterialTheme.typography.bodyMedium)
          }
          Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            LoginAction(submit, !state.loginBusy && state.loginUsername.isNotBlank() && identity.providerSelected, state.loginBusy,
              stringResource(if (state.loginBusy) R.string.login_signing_in else R.string.sign_in))
            if (identity.providerSelected && identity.jellyfin) OutlinedButton(onClick = onQuickConnect, enabled = !state.loginBusy, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp), shape = MaterialTheme.shapes.medium) {
              Text(stringResource(R.string.quick_connect))
            }
            state.quickConnectCode?.let { code ->
              Text(code, Modifier.fillMaxWidth().semantics { liveRegion = LiveRegionMode.Polite }, style = MaterialTheme.typography.displaySmall, textAlign = TextAlign.Center)
              LoginHint(stringResource(R.string.quick_connect_explanation))
            }
            LoginHint(stringResource(R.string.login_account_help))
          }
        }
        if (keyboardOpen) LoginHint(stringResource(R.string.login_preserves_account))
      }
      if (!keyboardOpen) Text(stringResource(R.string.login_preserves_account), Modifier.fillMaxWidth().padding(16.dp, 20.dp),
        color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall, textAlign = TextAlign.Center)
    }
  }
}

@Composable
private fun LoginAction(onClick: () -> Unit, enabled: Boolean, busy: Boolean, label: String) {
  Button(onClick, enabled = enabled, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp), shape = MaterialTheme.shapes.medium,
    colors = ButtonDefaults.buttonColors(containerColor = LocalPilotColors.current.action)) {
    if (busy) { CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 2.dp); Spacer(Modifier.width(8.dp)) }
    Text(label)
  }
}

@Composable
private fun LoginHint(text: String) {
  Text(text, Modifier.fillMaxWidth(), color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall, textAlign = TextAlign.Center)
}

@Composable
private fun LoginField(
  value: String,
  change: (String) -> Unit,
  label: String,
  modifier: Modifier = Modifier,
  enabled: Boolean,
  error: String? = null,
  hint: String? = null,
  contentType: ContentType? = null,
  keyboardType: KeyboardType = KeyboardType.Text,
  imeAction: ImeAction,
  onImeAction: () -> Unit,
  visualTransformation: VisualTransformation = VisualTransformation.None,
  trailing: (@Composable () -> Unit)? = null,
) {
  val interaction = remember { MutableInteractionSource() }
  val focused by interaction.collectIsFocusedAsState()
  val border = if (error != null) MaterialTheme.colorScheme.error else if (focused) MaterialTheme.colorScheme.primary else LocalPilotColors.current.metadata
  Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
    Text(label, style = MaterialTheme.typography.bodyMedium, color = LocalPilotColors.current.body)
    BasicTextField(value, change, modifier.fillMaxWidth().semantics {
      contentDescription = label
      if (error != null) error(error)
      if (contentType != null) this.contentType = contentType
    }, enabled = enabled, singleLine = true, textStyle = MaterialTheme.typography.bodyLarge.copy(color = MaterialTheme.colorScheme.onSurface),
      keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, keyboardType = keyboardType, imeAction = imeAction),
      keyboardActions = KeyboardActions(onGo = { onImeAction() }, onNext = { onImeAction() }),
      visualTransformation = visualTransformation, interactionSource = interaction, cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
      decorationBox = { input ->
        Row(Modifier.fillMaxWidth().heightIn(min = 52.dp).background(MaterialTheme.colorScheme.surfaceContainerLow, MaterialTheme.shapes.medium)
          .border(if (focused) 2.dp else 1.dp, border, MaterialTheme.shapes.medium).padding(start = 14.dp, end = if (trailing == null) 14.dp else 4.dp),
          verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
          Box(Modifier.weight(1f).padding(vertical = 14.dp)) { input() }
          trailing?.invoke()
        }
      })
    if (error != null) Text(error, Modifier.semantics { liveRegion = LiveRegionMode.Polite }, color = MaterialTheme.colorScheme.error,
      style = MaterialTheme.typography.bodyMedium.copy(lineHeight = 22.sp))
    else if (hint != null) Text(hint, color = LocalPilotColors.current.metadata, style = MaterialTheme.typography.bodySmall)
  }
}
