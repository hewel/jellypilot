import { readFileSync } from 'node:fs';

import { expect, test } from '@rstest/core';

test('Saved Session JSON parsing uses an Effect throwing boundary', () => {
  const authSource = readFileSync('src/effects/auth.ts', 'utf8');

  expect(authSource).toContain('yield* Effect.try({');
  expect(authSource).toContain('try: () => JSON.parse(raw)');
  expect(authSource).toContain('new StorageParseError({');
});
test('Login Prefill JSON parsing uses an Effect throwing boundary', () => {
  const sessionSource = readFileSync('src/effects/session.ts', 'utf8');

  expect(sessionSource).toContain('yield* Effect.try({');
  expect(sessionSource).toContain('try: () => JSON.parse(raw)');
  expect(sessionSource).toContain('new StorageParseError({');
});

test('Saved Session restore and route checks use typed command helpers', () => {
  const sessionAccessSource = readFileSync('src/sessionAccess.ts', 'utf8');

  expect(sessionAccessSource).toContain(
    "import { runTauriCommand, runTauriCommandRaw } from './effects/commands';",
  );
  expect(sessionAccessSource).toContain(
    'runTauriCommand(() => commands.jellyfinRestoreSession(savedSession))',
  );
  expect(sessionAccessSource).toContain('runTauriCommandRaw(() => commands.jellyfinIsConnected())');
});
test('Password Login connect command uses typed command helper', () => {
  const loginSource = readFileSync('src/components/LoginPage.tsx', 'utf8');

  expect(loginSource).toContain(
    "import { commandFailureMessage, runTauriCommand } from '../effects/commands';",
  );
  expect(loginSource).toContain('runTauriCommand(() => commands.jellyfinConnect(credentials))');
  expect(loginSource).not.toContain('const result = await commands.jellyfinConnect(credentials);');
});
test('Quick Connect commands use typed command helpers', () => {
  const qcSource = readFileSync('src/effects/quickConnect.ts', 'utf8');

  expect(qcSource).toContain(
    'runTauriCommand(() => commands.jellyfinQuickConnectStart(serverUrl))',
  );
  expect(qcSource).toContain('commands.jellyfinQuickConnectCheck(serverUrl, request.secret)');
  expect(qcSource).toContain(
    'commands.jellyfinQuickConnectAuthenticate(serverUrl, request.secret)',
  );
  expect(qcSource).not.toContain(
    'const result = await commands.jellyfinQuickConnectStart(serverUrl);',
  );
});
test('Operations Console and Shell use refactored connection/config effects', () => {
  const consoleSource = readFileSync('src/components/OperationsConsole.tsx', 'utf8');
  const shellSource = readFileSync('src/components/AuthenticatedShell.tsx', 'utf8');
  const authSource = readFileSync('src/effects/auth.ts', 'utf8');

  // Verify OperationsConsole.tsx imports and uses the new effects
  expect(consoleSource).toContain('fetchConfig');
  expect(consoleSource).toContain('saveConfig');
  expect(consoleSource).toContain('fetchConnectionState');
  expect(consoleSource).toContain('disconnectJellyfin');
  expect(consoleSource).toContain('clearJellyfinSession');

  // Verify OperationsConsole.tsx no longer contains raw command calls or imports commands
  expect(consoleSource).not.toContain('commands.configGet');
  expect(consoleSource).not.toContain('commands.configSet');
  expect(consoleSource).not.toContain('commands.mpvIsConnected');
  expect(consoleSource).not.toContain('commands.jellyfinDisconnect');
  expect(consoleSource).not.toContain('commands.jellyfinClearSession');

  // Verify AuthenticatedShell.tsx uses fetchConnectionState and doesn't contain commands.jellyfinGetState
  expect(shellSource).toContain('fetchConnectionState');
  expect(shellSource).not.toContain('commands.jellyfinGetState');

  // Verify src/effects/auth.ts no longer imports commands
  expect(authSource).not.toContain("import { commands } from '@bindings';");
});
