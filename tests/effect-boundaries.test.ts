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
  expect(sessionAccessSource).toContain(
    'runTauriCommandRaw(() => commands.jellyfinIsConnected())',
  );
});
test('Password Login connect command uses typed command helper', () => {
  const loginSource = readFileSync('src/components/LoginPage.tsx', 'utf8');

  expect(loginSource).toContain(
    "import { commandFailureMessage, runTauriCommand } from '../effects/commands';",
  );
  expect(loginSource).toContain(
    'runTauriCommand(() => commands.jellyfinConnect(credentials))',
  );
  expect(loginSource).not.toContain(
    'const result = await commands.jellyfinConnect(credentials);',
  );
});
test('Quick Connect commands use typed command helpers', () => {
  const loginSource = readFileSync('src/components/LoginPage.tsx', 'utf8');

  expect(loginSource).toContain(
    'runTauriCommand(() => commands.jellyfinQuickConnectStart(serverUrlValue))',
  );
  expect(loginSource).toContain(
    'commands.jellyfinQuickConnectCheck(serverUrlValue, secret)',
  );
  expect(loginSource).toContain(
    'commands.jellyfinQuickConnectAuthenticate(serverUrlValue, secret)',
  );
  expect(loginSource).not.toContain(
    'const result = await commands.jellyfinQuickConnectStart(serverUrlValue);',
  );
});
test('Operations Console commands use typed command helpers', () => {
  const consoleSource = readFileSync(
    'src/components/OperationsConsole.tsx',
    'utf8',
  );

  expect(consoleSource).toContain('runTauriCommand,');
  expect(consoleSource).toContain('runTauriCommandRaw,');
  expect(consoleSource).toContain(
    'runTauriCommandRaw(() => commands.configGet())',
  );
  expect(consoleSource).toContain(
    'runTauriCommand(() => commands.configSet(nextSave.config))',
  );
  expect(consoleSource).toContain(
    'runTauriCommand(() => commands.jellyfinDisconnect())',
  );
  expect(consoleSource).toContain(
    'runTauriCommand(() => commands.jellyfinClearSession())',
  );
  expect(consoleSource).not.toContain(
    'const result = await commands.jellyfinDisconnect();',
  );
  expect(consoleSource).not.toContain(
    'const result = await commands.jellyfinClearSession();',
  );
});
