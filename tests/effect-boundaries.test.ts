import { readFileSync } from 'node:fs';
import { expect, test } from '@rstest/core';

test('Saved Session JSON parsing uses an Effect throwing boundary', () => {
  const authSource = readFileSync('src/effects/auth.ts', 'utf8');

  expect(authSource).toContain('yield* Effect.try({');
  expect(authSource).toContain('try: () => JSON.parse(raw)');
  expect(authSource).toContain('new StorageParseError({');
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
